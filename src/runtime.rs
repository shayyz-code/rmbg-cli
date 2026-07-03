use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context};

use crate::cli::{Cli, Device};

const RUNTIME_DIR_NAME: &str = "runtime";
const WORKER_NAME: &str = "rmbg_runtime.py";
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

pub enum SetupError {
    User(anyhow::Error),
    Runtime(anyhow::Error),
}

pub fn run_worker(cli: &Cli, output: &Path, background: Option<[u8; 3]>) -> anyhow::Result<()> {
    let runtime_dir = find_runtime_dir()?;
    let worker = runtime_dir.join(WORKER_NAME);
    let uv = env::var_os("RMBG_UV_BIN").unwrap_or_else(|| "uv".into());

    let mut command = Command::new(uv);
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
        .arg(cli.device.as_str());

    if let Some([r, g, b]) = background {
        command.arg("--background").arg(format!("{r},{g},{b}"));
    }
    if cli.verbose {
        command.arg("--verbose");
    }

    let status = command
        .status()
        .context("failed to start uv; install uv and ensure it is available on PATH")?;
    if !status.success() {
        bail!("RMBG-2.0 worker failed with {status}");
    }
    Ok(())
}

pub fn run_setup(device: Device) -> Result<(), SetupError> {
    let runtime_dir = find_runtime_dir().map_err(SetupError::Runtime)?;
    let worker = runtime_dir.join(WORKER_NAME);
    let uv = env::var_os("RMBG_UV_BIN").unwrap_or_else(|| "uv".into());

    eprintln!("[1/4] Checking uv...");
    match Command::new(&uv)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => {
            return Err(SetupError::Runtime(anyhow::anyhow!(
                "uv is installed but `uv --version` failed with {status}"
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(SetupError::User(anyhow::anyhow!(
                "uv is required. Install it with:\n\n{}\n\nThen rerun `rmbg setup`.",
                uv_install_instruction()
            )));
        }
        Err(error) => {
            return Err(SetupError::Runtime(anyhow::Error::new(error).context(
                "failed to check uv; ensure it is installed and available on PATH",
            )));
        }
    }

    eprintln!("[2/4] Installing locked runtime dependencies...");
    let sync_status = Command::new(&uv)
        .arg("sync")
        .arg("--project")
        .arg(&runtime_dir)
        .arg("--frozen")
        .status()
        .context("failed to start uv sync")
        .map_err(SetupError::Runtime)?;
    if !sync_status.success() {
        return Err(SetupError::Runtime(anyhow::anyhow!(
            "uv dependency sync failed with {sync_status}"
        )));
    }

    eprintln!("[3/4] Checking Hugging Face authentication...");
    let auth = run_uv_capture(&uv, &runtime_dir, &["hf", "auth", "whoami"])
        .map_err(SetupError::Runtime)?;
    if !auth.status.success() {
        let details = output_text(&auth);
        if !needs_login(&details) {
            return Err(SetupError::Runtime(anyhow::anyhow!(
                "Hugging Face authentication check failed: {}",
                details.trim()
            )));
        }
        if !io::stdin().is_terminal() {
            return Err(SetupError::User(anyhow::anyhow!(
                "Hugging Face authentication is required. Set HF_TOKEN or run `rmbg setup` in an interactive terminal."
            )));
        }

        eprintln!("No active Hugging Face login was found; starting interactive login...");
        let login_status = Command::new(&uv)
            .arg("run")
            .arg("--project")
            .arg(&runtime_dir)
            .arg("--frozen")
            .args(["hf", "auth", "login"])
            .status()
            .context("failed to start Hugging Face login")
            .map_err(SetupError::Runtime)?;
        if !login_status.success() {
            return Err(SetupError::Runtime(anyhow::anyhow!(
                "Hugging Face login failed with {login_status}"
            )));
        }
    }

    eprintln!("[4/4] Downloading and validating RMBG-2.0 (about 844 MB)...");
    let model_status = Command::new(&uv)
        .arg("run")
        .arg("--project")
        .arg(&runtime_dir)
        .arg("--frozen")
        .arg("python")
        .arg(&worker)
        .arg("--setup")
        .arg("--device")
        .arg(device.as_str())
        .status()
        .context("failed to start RMBG-2.0 setup worker")
        .map_err(SetupError::Runtime)?;

    match model_status.code() {
        Some(0) => {
            eprintln!("Setup complete. `rmbg` is ready to use.");
            Ok(())
        }
        Some(3) => Err(SetupError::User(anyhow::anyhow!(
            "RMBG-2.0 access has not been granted. Accept the non-commercial terms at\nhttps://huggingface.co/briaai/RMBG-2.0\nthen rerun `rmbg setup`."
        ))),
        _ => Err(SetupError::Runtime(anyhow::anyhow!(
            "RMBG-2.0 download or model validation failed with {model_status}"
        ))),
    }
}

fn run_uv_capture(
    uv: &std::ffi::OsStr,
    runtime_dir: &Path,
    args: &[&str],
) -> anyhow::Result<Output> {
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

fn needs_login(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("not logged in")
        || message.contains("invalid user token")
        || message.contains("authentication token")
}

pub fn uv_install_instruction() -> &'static str {
    if cfg!(windows) {
        "powershell -ExecutionPolicy ByPass -c \"irm https://astral.sh/uv/install.ps1 | iex\""
    } else {
        "curl -LsSf https://astral.sh/uv/install.sh | sh"
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

    let source_runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join(RUNTIME_DIR_NAME);
    if source_runtime.join(WORKER_NAME).is_file() {
        return validate_runtime_dir(source_runtime);
    }

    materialize_embedded_runtime(&runtime_cache_root()?)
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
            .join("Library")
            .join("Caches")
            .join("rmbg-cli")
            .join(RUNTIME_DIR_NAME));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(path).join("rmbg-cli").join(RUNTIME_DIR_NAME));
        }
        if let Some(path) = env::var_os("HOME") {
            return Ok(PathBuf::from(path)
                .join(".cache")
                .join("rmbg-cli")
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
        fs::write(&temporary, contents)
            .with_context(|| format!("writing embedded runtime file {}", temporary.display()))?;
        if let Err(error) = fs::rename(&temporary, &destination) {
            if destination.exists() {
                fs::remove_file(&destination)
                    .with_context(|| format!("replacing runtime file {}", destination.display()))?;
                fs::rename(&temporary, &destination).with_context(|| {
                    format!("installing runtime file {}", destination.display())
                })?;
            } else {
                return Err(error)
                    .with_context(|| format!("installing runtime file {}", destination.display()));
            }
        }
    }

    validate_runtime_dir(path.to_path_buf())
}

pub fn device_label(device: Device) -> &'static str {
    device.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_runtime_is_discoverable() {
        let runtime = find_runtime_dir().unwrap();
        assert!(runtime.join(WORKER_NAME).is_file());
        assert!(runtime.join("pyproject.toml").is_file());
    }

    #[test]
    fn missing_uv_instruction_uses_official_installer() {
        assert!(uv_install_instruction().contains("https://astral.sh/uv/install"));
    }

    #[test]
    fn distinguishes_login_failures_from_network_failures() {
        assert!(needs_login("Not logged in"));
        assert!(needs_login("Invalid user token"));
        assert!(!needs_login("failed to resolve huggingface.co"));
    }

    #[test]
    fn materializes_and_refreshes_the_embedded_runtime_without_removing_environment() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");

        materialize_embedded_runtime(&runtime).unwrap();
        assert_eq!(
            fs::read(runtime.join(WORKER_NAME)).unwrap(),
            include_bytes!("../runtime/rmbg_runtime.py")
        );

        fs::create_dir(runtime.join(".venv")).unwrap();
        fs::write(runtime.join(".venv").join("marker"), b"keep").unwrap();
        fs::write(runtime.join(WORKER_NAME), b"stale").unwrap();

        materialize_embedded_runtime(&runtime).unwrap();
        assert_eq!(
            fs::read(runtime.join(".venv").join("marker")).unwrap(),
            b"keep"
        );
        assert_eq!(
            fs::read(runtime.join(WORKER_NAME)).unwrap(),
            include_bytes!("../runtime/rmbg_runtime.py")
        );
    }
}
