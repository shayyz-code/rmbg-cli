use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, BufRead, BufReader, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, SetupCli};
use crate::ui::Ui;

const RUNTIME_DIR_NAME: &str = "runtime";
const WORKER_NAME: &str = "rmbg_runtime.py";
const PROGRESS_PREFIX: &str = "::rmbg-progress::";
const FIVE_GIB: u64 = 5 * 1024 * 1024 * 1024;
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

const EMBEDDED_RUNTIME_FILES: &[(&str, &[u8])] = &[
    (
        ".python-version",
        include_bytes!("../runtime/.python-version"),
    ),
    (
        "pyproject.toml",
        include_bytes!("../runtime/pyproject.toml"),
    ),
    ("uv.lock", include_bytes!("../runtime/uv.lock")),
    (WORKER_NAME, include_bytes!("../runtime/rmbg_runtime.py")),
];

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProgressEvent {
    pub completed: u8,
    pub total: u8,
    pub stage: String,
    pub label: String,
    pub device: Option<String>,
}

pub struct WorkerResult {
    pub diagnostics: String,
    pub device: String,
}

pub enum SetupError {
    User(anyhow::Error),
    Runtime(anyhow::Error),
}

#[derive(Serialize)]
pub struct SetupResult {
    pub status: &'static str,
    pub device: String,
    pub steps: Vec<SetupStep>,
}

#[derive(Serialize)]
pub struct SetupStep {
    pub name: &'static str,
    pub status: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    ActionRequired,
    Error,
    Skipped,
}

#[derive(Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Serialize)]
pub struct DoctorReport {
    pub status: CheckStatus,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn exit_code(&self) -> u8 {
        match self.status {
            CheckStatus::Ok => 0,
            CheckStatus::ActionRequired => 1,
            CheckStatus::Error => 2,
            CheckStatus::Skipped => 2,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PythonDoctor {
    authenticated: bool,
    model_cached: bool,
    cache_detail: String,
    cuda: bool,
    mps: bool,
    cpu: bool,
    selected_device: String,
    deep_status: String,
    deep_detail: String,
}

#[derive(Debug, Deserialize)]
struct SetupWorkerResult {
    device: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvSource {
    Environment,
    Bundled,
    Path,
}

impl UvSource {
    fn label(self) -> &'static str {
        match self {
            Self::Environment => "RMBG_UV_BIN",
            Self::Bundled => "bundled",
            Self::Path => "PATH",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UvBinary {
    pub path: OsString,
    pub source: UvSource,
}

pub fn install_interrupt_handler() -> anyhow::Result<()> {
    ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst))
        .context("installing interruption handler")
}

pub fn reset_interrupted() {
    INTERRUPTED.store(false, Ordering::SeqCst);
}

pub fn run_worker<F>(
    cli: &Cli,
    output: &Path,
    background: Option<[u8; 3]>,
    mut on_progress: F,
) -> anyhow::Result<WorkerResult>
where
    F: FnMut(ProgressEvent),
{
    let runtime_dir = find_runtime_dir()?;
    let worker = runtime_dir.join(WORKER_NAME);
    let uv = resolve_uv();
    let mut command = Command::new(&uv.path);
    command
        .arg("run")
        .arg("--project")
        .arg(&runtime_dir)
        .arg("--frozen")
        .arg("python")
        .arg(&worker)
        .arg("--input")
        .arg(&cli.input)
        .arg("--output")
        .arg(output)
        .arg("--device")
        .arg(cli.device.as_str())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some([r, g, b]) = background {
        command.arg("--background").arg(format!("{r},{g},{b}"));
    }
    if cli.output_args.verbose {
        command.arg("--verbose");
    }

    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to start uv at {}; run `rmbg doctor` for details",
            PathBuf::from(&uv.path).display()
        )
    })?;
    let stdout = child.stdout.take().context("capturing worker stdout")?;
    let stderr = child.stderr.take().context("capturing worker stderr")?;
    let stdout_handle = thread::spawn(move || read_all(stdout));
    let (tx, rx) = mpsc::channel();
    let stderr_handle = thread::spawn(move || {
        let mut collected = String::new();
        for line in BufReader::new(stderr).lines() {
            match line {
                Ok(line) => {
                    collected.push_str(&line);
                    collected.push('\n');
                    let _ = tx.send(line);
                }
                Err(error) => {
                    collected.push_str(&format!("failed to read worker diagnostics: {error}\n"));
                    break;
                }
            }
        }
        collected
    });

    let mut expected = 1u8;
    let mut selected_device = None;
    let status = loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("operation interrupted")
        }
        match rx.recv_timeout(Duration::from_millis(80)) {
            Ok(line) => {
                if let Some(event) = parse_progress_line(&line)? {
                    validate_progress(&event, expected)?;
                    if event.completed == 1 {
                        selected_device = event.device.clone();
                    }
                    expected += 1;
                    on_progress(event);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some(status) = child.try_wait().context("waiting for RMBG-2.0 worker")? {
            break status;
        }
    };

    let stdout = stdout_handle
        .join()
        .map_err(|_| anyhow::anyhow!("worker stdout reader panicked"))??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| anyhow::anyhow!("worker stderr reader panicked"))?;
    while let Ok(line) = rx.try_recv() {
        if let Some(event) = parse_progress_line(&line)? {
            validate_progress(&event, expected)?;
            if event.completed == 1 {
                selected_device = event.device.clone();
            }
            expected += 1;
            on_progress(event);
        }
    }
    let diagnostics = filter_progress_diagnostics(&format!("{stdout}{stderr}"))?;
    if !status.success() {
        let details = clean_error_prefix(diagnostics.trim());
        if details.is_empty() {
            bail!("RMBG-2.0 worker failed with {status}");
        }
        bail!("{details}");
    }
    if expected != 5 {
        bail!(
            "worker progress protocol ended after milestone {} of 4",
            expected - 1
        );
    }
    let device = selected_device.context("worker did not report the selected device")?;
    Ok(WorkerResult {
        diagnostics: diagnostics.trim().to_owned(),
        device,
    })
}

fn read_all(mut reader: impl Read) -> anyhow::Result<String> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    Ok(text)
}

pub fn parse_progress_line(line: &str) -> anyhow::Result<Option<ProgressEvent>> {
    let Some(payload) = line.strip_prefix(PROGRESS_PREFIX) else {
        return Ok(None);
    };
    serde_json::from_str(payload)
        .context("invalid RMBG progress event")
        .map(Some)
}

fn validate_progress(event: &ProgressEvent, expected: u8) -> anyhow::Result<()> {
    const STAGES: [&str; 4] = [
        "device_selected",
        "model_loaded",
        "image_preprocessed",
        "inference_completed",
    ];
    if event.total != 5
        || event.completed != expected
        || event.completed > 4
        || event.stage != STAGES[(event.completed - 1) as usize]
    {
        bail!(
            "invalid RMBG progress sequence at milestone {} ({})",
            event.completed,
            event.stage
        );
    }
    Ok(())
}

pub fn filter_progress_diagnostics(text: &str) -> anyhow::Result<String> {
    let mut result = Vec::new();
    for line in text.lines() {
        if line.starts_with(PROGRESS_PREFIX) {
            parse_progress_line(line)?;
        } else if !line.is_empty() {
            result.push(line);
        }
    }
    Ok(result.join("\n"))
}

pub fn run_setup(cli: &SetupCli, ui: &Ui) -> Result<SetupResult, SetupError> {
    let runtime_dir = find_runtime_dir().map_err(SetupError::Runtime)?;
    let worker = runtime_dir.join(WORKER_NAME);
    let uv = resolve_uv();
    ui.step(1, 4, "Checking uv...");
    let version = Command::new(&uv.path).arg("--version").output();
    match version {
        Ok(output) if output.status.success() => {
            if cli.output_args.verbose {
                ui.detail("uv", output_text(&output).trim());
            }
        }
        Ok(output) => {
            return Err(SetupError::Runtime(anyhow::anyhow!(
                "uv --version failed: {}",
                output_text(&output).trim()
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(SetupError::User(anyhow::anyhow!(
                "uv is required; install it or set RMBG_UV_BIN, then rerun `rmbg setup`"
            )));
        }
        Err(error) => return Err(SetupError::Runtime(error.into())),
    }

    ui.step(2, 4, "Installing locked runtime dependencies...");
    let sync = Command::new(&uv.path)
        .arg("sync")
        .arg("--project")
        .arg(&runtime_dir)
        .arg("--frozen")
        .output()
        .context("failed to start uv sync")
        .map_err(SetupError::Runtime)?;
    if !sync.status.success() {
        return Err(SetupError::Runtime(anyhow::anyhow!(
            "uv dependency sync failed: {}",
            output_text(&sync).trim()
        )));
    }
    if cli.output_args.verbose {
        ui.diagnostics(&output_text(&sync));
    }

    ui.step(3, 4, "Checking Hugging Face authentication...");
    let auth = run_uv_capture(&uv.path, &runtime_dir, &["hf", "auth", "whoami"])
        .map_err(SetupError::Runtime)?;
    if !auth.status.success() {
        let details = output_text(&auth);
        if !needs_login(&details) {
            return Err(SetupError::Runtime(anyhow::anyhow!(
                "Hugging Face authentication check failed: {}",
                details.trim()
            )));
        }
        if cli.output_args.json || !io::stdin().is_terminal() {
            return Err(SetupError::User(anyhow::anyhow!(
                "Hugging Face authentication is required. Set HF_TOKEN or run `rmbg setup` in an interactive terminal."
            )));
        }
        ui.notice("No active Hugging Face login was found; starting interactive login...");
        let login = Command::new(&uv.path)
            .arg("run")
            .arg("--project")
            .arg(&runtime_dir)
            .arg("--frozen")
            .args(["hf", "auth", "login"])
            .status()
            .context("failed to start Hugging Face login")
            .map_err(SetupError::Runtime)?;
        if !login.success() {
            return Err(SetupError::Runtime(anyhow::anyhow!(
                "Hugging Face login failed with {login}"
            )));
        }
    }

    ui.step(
        4,
        4,
        "Downloading and validating RMBG-2.0 (about 844 MB)...",
    );
    let model = Command::new(&uv.path)
        .arg("run")
        .arg("--project")
        .arg(&runtime_dir)
        .arg("--frozen")
        .arg("python")
        .arg(&worker)
        .arg("--setup")
        .arg("--device")
        .arg(cli.device.as_str())
        .output()
        .context("failed to start RMBG-2.0 setup worker")
        .map_err(SetupError::Runtime)?;
    match model.status.code() {
        Some(0) => {
            let result: SetupWorkerResult = serde_json::from_slice(&model.stdout)
                .context("invalid setup worker result")
                .map_err(SetupError::Runtime)?;
            if cli.output_args.verbose {
                ui.diagnostics(&String::from_utf8_lossy(&model.stderr));
            }
            ui.success("Setup complete. `rmbg` is ready to use.");
            Ok(SetupResult {
                status: "ok",
                device: result.device,
                steps: ["uv", "dependencies", "authentication", "model"]
                    .into_iter()
                    .map(|name| SetupStep { name, status: "ok" })
                    .collect(),
            })
        }
        Some(3) => Err(SetupError::User(anyhow::anyhow!(
            "RMBG-2.0 access has not been granted. Accept the non-commercial terms at https://huggingface.co/briaai/RMBG-2.0, then rerun `rmbg setup`."
        ))),
        _ => Err(SetupError::Runtime(anyhow::anyhow!(
            "RMBG-2.0 download or model validation failed: {}",
            String::from_utf8_lossy(&model.stderr).trim()
        ))),
    }
}

pub fn run_doctor(deep: bool) -> DoctorReport {
    let mut checks = Vec::with_capacity(9);
    let uv = resolve_uv();
    let uv_output = Command::new(&uv.path).arg("--version").output();
    checks.push(match uv_output {
        Ok(output) if output.status.success() => DoctorCheck {
            name: "uv",
            status: CheckStatus::Ok,
            detail: format!("{}: {}", uv.source.label(), output_text(&output).trim()),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => DoctorCheck {
            name: "uv",
            status: CheckStatus::ActionRequired,
            detail: "uv was not found; install it or set RMBG_UV_BIN".to_owned(),
        },
        Ok(output) => DoctorCheck {
            name: "uv",
            status: CheckStatus::Error,
            detail: format!("uv --version failed: {}", output_text(&output).trim()),
        },
        Err(error) => DoctorCheck {
            name: "uv",
            status: CheckStatus::Error,
            detail: format!("could not execute uv: {error}"),
        },
    });

    let runtime = inspect_runtime_dir();
    let (runtime_dir, runtime_ok) = match runtime {
        Ok((path, detail)) => {
            checks.push(DoctorCheck {
                name: "runtime_files",
                status: CheckStatus::Ok,
                detail,
            });
            (Some(path), true)
        }
        Err(error) => {
            checks.push(DoctorCheck {
                name: "runtime_files",
                status: CheckStatus::ActionRequired,
                detail: format!("{error:#}"),
            });
            (None, false)
        }
    };

    let python_result = runtime_dir
        .as_ref()
        .and_then(|dir| run_python_doctor(dir, deep).ok());
    checks.push(if python_result.is_some() {
        DoctorCheck {
            name: "python_environment",
            status: CheckStatus::Ok,
            detail: "locked Python environment imports successfully".to_owned(),
        }
    } else {
        DoctorCheck {
            name: "python_environment",
            status: CheckStatus::ActionRequired,
            detail: if runtime_ok {
                "Python environment is missing or incomplete; run `rmbg setup`".to_owned()
            } else {
                "runtime files must be repaired before Python can be checked".to_owned()
            },
        }
    });

    let model_cached = python_result
        .as_ref()
        .is_some_and(|result| result.model_cached);
    checks.push(match &python_result {
        Some(result) if result.authenticated => DoctorCheck {
            name: "huggingface_auth",
            status: CheckStatus::Ok,
            detail: "Hugging Face credentials are configured".to_owned(),
        },
        Some(_) if model_cached => DoctorCheck {
            name: "huggingface_auth",
            status: CheckStatus::Ok,
            detail: "no credentials configured; cached pinned model is usable offline".to_owned(),
        },
        Some(_) => DoctorCheck {
            name: "huggingface_auth",
            status: CheckStatus::ActionRequired,
            detail: "authenticate with Hugging Face before downloading the gated model".to_owned(),
        },
        None => skipped("huggingface_auth", "Python environment is not ready"),
    });
    checks.push(match &python_result {
        Some(result) if result.model_cached => DoctorCheck {
            name: "model_cache",
            status: CheckStatus::Ok,
            detail: result.cache_detail.clone(),
        },
        Some(result) => DoctorCheck {
            name: "model_cache",
            status: CheckStatus::ActionRequired,
            detail: result.cache_detail.clone(),
        },
        None => skipped("model_cache", "Python environment is not ready"),
    });
    checks.push(match &python_result {
        Some(result) => DoctorCheck {
            name: "devices",
            status: CheckStatus::Ok,
            detail: format!(
                "cuda={}, mps={}, cpu={}",
                result.cuda, result.mps, result.cpu
            ),
        },
        None => skipped("devices", "Python environment is not ready"),
    });
    checks.push(match &python_result {
        Some(result) => DoctorCheck {
            name: "selected_device",
            status: CheckStatus::Ok,
            detail: result.selected_device.clone(),
        },
        None => skipped("selected_device", "Python environment is not ready"),
    });
    checks.push(disk_check());
    checks.push(match &python_result {
        Some(result) if deep && result.deep_status == "ok" => DoctorCheck {
            name: "model_load",
            status: CheckStatus::Ok,
            detail: result.deep_detail.clone(),
        },
        Some(result) if deep && result.deep_status == "error" => DoctorCheck {
            name: "model_load",
            status: if model_cached {
                CheckStatus::Error
            } else {
                CheckStatus::ActionRequired
            },
            detail: result.deep_detail.clone(),
        },
        _ if deep => skipped(
            "model_load",
            "model cache or Python environment is not ready",
        ),
        _ => skipped(
            "model_load",
            "run `rmbg doctor --deep` to load the cached model",
        ),
    });

    let status = if checks
        .iter()
        .any(|check| check.status == CheckStatus::Error)
    {
        CheckStatus::Error
    } else if checks
        .iter()
        .any(|check| check.status == CheckStatus::ActionRequired)
    {
        CheckStatus::ActionRequired
    } else {
        CheckStatus::Ok
    };
    DoctorReport { status, checks }
}

fn skipped(name: &'static str, detail: &str) -> DoctorCheck {
    DoctorCheck {
        name,
        status: CheckStatus::Skipped,
        detail: detail.to_owned(),
    }
}

fn run_python_doctor(runtime_dir: &Path, deep: bool) -> anyhow::Result<PythonDoctor> {
    let environment = env::var_os("UV_PROJECT_ENVIRONMENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_dir.join(".venv"));
    let python = if cfg!(windows) {
        environment.join("Scripts").join("python.exe")
    } else {
        environment.join("bin").join("python")
    };
    if !python.is_file() {
        bail!("project Python executable is missing")
    }
    let transient_modules = tempfile::tempdir().context("creating transient doctor cache")?;
    let mut command = Command::new(python);
    command
        .arg(runtime_dir.join(WORKER_NAME))
        .arg("--doctor-json")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .env("HF_MODULES_CACHE", transient_modules.path());
    if deep {
        command.arg("--deep");
    }
    let output = command
        .output()
        .context("running read-only Python diagnostics")?;
    if !output.status.success() {
        bail!("{}", output_text(&output).trim())
    }
    serde_json::from_slice(&output.stdout).context("parsing Python diagnostic result")
}

fn disk_check() -> DoctorCheck {
    let runtime = runtime_cache_root().ok();
    let hf = huggingface_cache_root();
    let paths: Vec<PathBuf> = [runtime, hf].into_iter().flatten().collect();
    let mut values = Vec::new();
    for path in paths {
        let existing = path.ancestors().find(|candidate| candidate.exists());
        let Some(existing) = existing else {
            continue;
        };
        match fs2::available_space(existing) {
            Ok(bytes) => values.push((path, bytes)),
            Err(error) => {
                return DoctorCheck {
                    name: "disk_space",
                    status: CheckStatus::Error,
                    detail: format!("could not measure {}: {error}", existing.display()),
                };
            }
        }
    }
    let minimum = values.iter().map(|(_, bytes)| *bytes).min();
    match minimum {
        Some(bytes) => {
            let detail = values
                .iter()
                .map(|(path, free)| {
                    format!(
                        "{}={:.1} GiB",
                        path.display(),
                        *free as f64 / 1024f64.powi(3)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            DoctorCheck {
                name: "disk_space",
                status: if bytes < FIVE_GIB {
                    CheckStatus::ActionRequired
                } else {
                    CheckStatus::Ok
                },
                detail,
            }
        }
        None => DoctorCheck {
            name: "disk_space",
            status: CheckStatus::Error,
            detail: "could not locate runtime or Hugging Face cache storage".to_owned(),
        },
    }
}

fn huggingface_cache_root() -> Option<PathBuf> {
    if let Some(path) = env::var_os("HUGGINGFACE_HUB_CACHE") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("HF_HOME") {
        return Some(PathBuf::from(path).join("hub"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/huggingface/hub"))
}

fn run_uv_capture(uv: &OsStr, runtime_dir: &Path, args: &[&str]) -> anyhow::Result<Output> {
    Command::new(uv)
        .arg("run")
        .arg("--project")
        .arg(runtime_dir)
        .arg("--frozen")
        .args(args)
        .output()
        .context("failed to start uv command")
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn clean_error_prefix(message: &str) -> &str {
    message.strip_prefix("error: ").unwrap_or(message)
}

fn needs_login(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("not logged in")
        || message.contains("invalid user token")
        || message.contains("authentication token")
}

pub fn resolve_uv() -> UvBinary {
    resolve_uv_from(
        env::var_os("RMBG_UV_BIN"),
        env::current_exe().ok().as_deref(),
        cfg!(windows),
    )
}

fn resolve_uv_from(
    explicit: Option<OsString>,
    executable: Option<&Path>,
    windows: bool,
) -> UvBinary {
    if let Some(path) = explicit {
        return UvBinary {
            path,
            source: UvSource::Environment,
        };
    }
    let name = if windows { "uv.exe" } else { "uv" };
    if let Some(path) = executable
        .and_then(Path::parent)
        .map(|parent| parent.join(name))
        .filter(|path| path.is_file())
    {
        return UvBinary {
            path: path.into_os_string(),
            source: UvSource::Bundled,
        };
    }
    UvBinary {
        path: OsString::from(name),
        source: UvSource::Path,
    }
}

pub fn find_runtime_dir() -> anyhow::Result<PathBuf> {
    if let Some(path) = env::var_os("RMBG_RUNTIME_DIR") {
        return validate_runtime_dir(PathBuf::from(path));
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            let bundled = parent.join(RUNTIME_DIR_NAME);
            if bundled.join(WORKER_NAME).is_file() {
                return validate_runtime_dir(bundled);
            }
        }
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(RUNTIME_DIR_NAME);
    if source.join(WORKER_NAME).is_file() {
        return validate_runtime_dir(source);
    }
    materialize_embedded_runtime(&runtime_cache_root()?)
}

fn inspect_runtime_dir() -> anyhow::Result<(PathBuf, String)> {
    if let Some(path) = env::var_os("RMBG_RUNTIME_DIR") {
        let path = PathBuf::from(path);
        validate_runtime_contents(&path)?;
        return Ok((
            path.clone(),
            format!("explicit runtime files healthy at {}", path.display()),
        ));
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(RUNTIME_DIR_NAME);
    if source.join(WORKER_NAME).is_file() {
        validate_runtime_contents(&source)?;
        return Ok((
            source.clone(),
            format!("embedded/runtime files healthy at {}", source.display()),
        ));
    }
    let cache = runtime_cache_root()?;
    if cache.exists() {
        validate_runtime_contents(&cache)?;
        return Ok((
            cache.clone(),
            format!("materialized runtime files healthy at {}", cache.display()),
        ));
    }
    bail!("embedded runtime is healthy but has not been materialized; run `rmbg setup`")
}

fn validate_runtime_contents(path: &Path) -> anyhow::Result<()> {
    validate_runtime_dir(path.to_path_buf())?;
    for (name, expected) in EMBEDDED_RUNTIME_FILES {
        let actual = fs::read(path.join(name))?;
        if actual.as_slice() != *expected {
            bail!("runtime file {} does not match the embedded version", name)
        }
    }
    Ok(())
}

fn validate_runtime_dir(path: PathBuf) -> anyhow::Result<PathBuf> {
    if path.join(WORKER_NAME).is_file()
        && path.join("pyproject.toml").is_file()
        && path.join("uv.lock").is_file()
    {
        return Ok(path);
    }
    bail!(
        "RMBG runtime is incomplete at {}; reinstall rmbg or set RMBG_RUNTIME_DIR",
        path.display()
    )
}

fn runtime_cache_root() -> anyhow::Result<PathBuf> {
    if let Some(path) = env::var_os("RMBG_CACHE_DIR") {
        return Ok(PathBuf::from(path).join(RUNTIME_DIR_NAME));
    }
    #[cfg(windows)]
    if let Some(path) = env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join("rmbg-cli").join(RUNTIME_DIR_NAME));
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = env::var_os("HOME") {
        return Ok(PathBuf::from(path)
            .join("Library/Caches/rmbg-cli")
            .join(RUNTIME_DIR_NAME));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(path).join("rmbg-cli").join(RUNTIME_DIR_NAME));
        }
        if let Some(path) = env::var_os("HOME") {
            return Ok(PathBuf::from(path)
                .join(".cache/rmbg-cli")
                .join(RUNTIME_DIR_NAME));
        }
    }
    bail!("unable to determine a runtime cache directory; set RMBG_CACHE_DIR")
}

fn materialize_embedded_runtime(path: &Path) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(path)
        .with_context(|| format!("creating runtime cache at {}", path.display()))?;
    for (name, contents) in EMBEDDED_RUNTIME_FILES {
        let destination = path.join(name);
        if fs::read(&destination).is_ok_and(|existing| existing.as_slice() == *contents) {
            continue;
        }
        let temporary = path.join(format!(".{name}.tmp-{}", std::process::id()));
        fs::write(&temporary, contents)?;
        tempfile::TempPath::try_from_path(temporary)?.persist(&destination)?;
    }
    validate_runtime_dir(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_filters_progress() {
        let line = r#"::rmbg-progress::{"completed":1,"total":5,"stage":"device_selected","label":"Device selected","device":"cpu"}"#;
        assert_eq!(parse_progress_line(line).unwrap().unwrap().completed, 1);
        assert_eq!(
            filter_progress_diagnostics(&format!("hello\n{line}\nworld\n")).unwrap(),
            "hello\nworld"
        );
    }

    #[test]
    fn rejects_fabricated_or_out_of_order_progress() {
        let event = ProgressEvent {
            completed: 2,
            total: 5,
            stage: "model_loaded".to_owned(),
            label: "Model loaded".to_owned(),
            device: None,
        };
        assert!(validate_progress(&event, 1).is_err());
    }

    #[test]
    fn uv_precedence_supports_unix_and_windows_names() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("rmbg");
        fs::write(&exe, b"").unwrap();
        fs::write(temp.path().join("uv"), b"").unwrap();
        assert_eq!(
            resolve_uv_from(None, Some(&exe), false).source,
            UvSource::Bundled
        );
        fs::write(temp.path().join("uv.exe"), b"").unwrap();
        assert!(resolve_uv_from(None, Some(&exe), true)
            .path
            .to_string_lossy()
            .ends_with("uv.exe"));
        assert_eq!(
            resolve_uv_from(Some("custom-uv".into()), Some(&exe), false).source,
            UvSource::Environment
        );
    }

    #[test]
    fn materializes_and_refreshes_runtime_without_removing_environment() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        materialize_embedded_runtime(&runtime).unwrap();
        fs::create_dir(runtime.join(".venv")).unwrap();
        fs::write(runtime.join(".venv/marker"), b"keep").unwrap();
        fs::write(runtime.join(WORKER_NAME), b"stale").unwrap();
        materialize_embedded_runtime(&runtime).unwrap();
        assert_eq!(fs::read(runtime.join(".venv/marker")).unwrap(), b"keep");
    }
}
