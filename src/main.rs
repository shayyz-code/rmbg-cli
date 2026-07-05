mod cli;
mod runtime;
mod ui;

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;

use cli::{Cli, Invocation};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let json_requested = cli::json_requested(&args);
    if let Err(error) = runtime::install_interrupt_handler() {
        return emit_early_error(json_requested, "runtime", &format!("{error:#}"), 2);
    }
    runtime::reset_interrupted();
    let invocation = match cli::parse_invocation() {
        Ok(invocation) => invocation,
        Err(error) => {
            use clap::error::ErrorKind;
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            if json_requested {
                return emit_early_error(true, "usage", &error.to_string(), 1);
            }
            let _ = error.print();
            return ExitCode::from(1);
        }
    };
    let json = invocation.output_args().json;
    let ui = ui::Ui::new(invocation.output_args());
    match run(invocation, &ui) {
        Ok(outcome) => {
            if json {
                write_json(&outcome.value);
            }
            ExitCode::from(outcome.exit_code)
        }
        Err(error) => {
            let (kind, message, code) = match error {
                AppError::User(error) => ("user", format!("{error:#}"), 1),
                AppError::Runtime(error) => ("runtime", format!("{error:#}"), 2),
            };
            if json {
                write_json(&ErrorJson {
                    status: "error",
                    kind,
                    message: &message,
                    exit_code: code,
                });
            } else {
                ui.error(&message);
            }
            ExitCode::from(code)
        }
    }
}

struct RunOutcome {
    value: serde_json::Value,
    exit_code: u8,
}

enum AppError {
    User(anyhow::Error),
    Runtime(anyhow::Error),
}

#[derive(Serialize)]
struct RemovalSuccess {
    status: &'static str,
    input: String,
    output: String,
    device: String,
    duration_ms: u128,
}

#[derive(Serialize)]
struct ErrorJson<'a> {
    status: &'static str,
    kind: &'static str,
    message: &'a str,
    exit_code: u8,
}

fn run(invocation: Invocation, ui: &ui::Ui) -> Result<RunOutcome, AppError> {
    match invocation {
        Invocation::Remove(cli) => {
            let success = run_remove(cli, ui)?;
            Ok(RunOutcome {
                value: serde_json::to_value(success).expect("serializing removal result"),
                exit_code: 0,
            })
        }
        Invocation::Setup(setup) => {
            let result = runtime::run_setup(&setup, ui).map_err(|error| match error {
                runtime::SetupError::User(error) => AppError::User(error),
                runtime::SetupError::Runtime(error) => AppError::Runtime(error),
            })?;
            Ok(RunOutcome {
                value: serde_json::to_value(result).expect("serializing setup result"),
                exit_code: 0,
            })
        }
        Invocation::Doctor(doctor) => {
            let report = runtime::run_doctor(doctor.deep);
            ui.doctor(&report);
            let exit_code = report.exit_code();
            Ok(RunOutcome {
                value: serde_json::to_value(report).expect("serializing doctor report"),
                exit_code,
            })
        }
    }
}

fn run_remove(cli: Cli, ui: &ui::Ui) -> Result<RemovalSuccess, AppError> {
    if !cli.input.is_file() {
        return Err(AppError::User(anyhow::anyhow!(
            "input file not found: {}",
            cli.input.display()
        )));
    }
    let background = cli.background_rgb().map_err(AppError::User)?;
    let output = cli.output_path();
    if same_path(&output, &cli.input) {
        return Err(AppError::User(anyhow::anyhow!(
            "output path must differ from input path"
        )));
    }
    if output.exists() && !cli.force {
        return Err(AppError::User(anyhow::anyhow!(
            "output already exists: {}; pass --force to replace it",
            output.display()
        )));
    }

    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(AppError::User(anyhow::anyhow!(
            "output directory does not exist: {}",
            parent.display()
        )));
    }
    let temporary = tempfile::Builder::new()
        .prefix(".rmbg-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .with_context(|| format!("output directory is not writable: {}", parent.display()))
        .map_err(AppError::User)?
        .into_temp_path();

    if cli.output_args.verbose {
        ui.detail("model", "briaai/RMBG-2.0");
        ui.detail("requested device", cli.device.as_str());
    }
    let filename = cli
        .input
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let started = Instant::now();
    let mut progress = ui.progress(filename);
    let worker = match runtime::run_worker(&cli, temporary.as_ref(), background, |event| {
        progress.update(&event)
    }) {
        Ok(worker) => worker,
        Err(error) => {
            progress.fail();
            return Err(AppError::Runtime(
                error.context(format!("processing {}", cli.input.display())),
            ));
        }
    };
    fs::File::open(&*temporary)
        .and_then(|file| file.sync_all())
        .context("synchronizing temporary PNG output")
        .map_err(AppError::Runtime)?;

    let commit = if cli.force {
        temporary.persist(&output)
    } else {
        temporary.persist_noclobber(&output)
    };
    if let Err(error) = commit {
        progress.fail();
        if !cli.force && output.exists() {
            return Err(AppError::User(anyhow::anyhow!(
                "output was created while processing: {}; pass --force to replace it",
                output.display()
            )));
        }
        return Err(AppError::Runtime(anyhow::Error::new(error).context(
            format!("atomically committing output {}", output.display()),
        )));
    }
    let elapsed = progress.complete();
    if cli.output_args.verbose {
        ui.diagnostics(&worker.diagnostics);
    }
    ui.success(&format!(
        "Saved {} ({})",
        output.display(),
        ui::format_duration(elapsed)
    ));
    Ok(RemovalSuccess {
        status: "ok",
        input: cli.input.to_string_lossy().into_owned(),
        output: output.to_string_lossy().into_owned(),
        device: worker.device,
        duration_ms: started.elapsed().as_millis(),
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn emit_early_error(json: bool, kind: &'static str, message: &str, code: u8) -> ExitCode {
    if json {
        write_json(&ErrorJson {
            status: "error",
            kind,
            message,
            exit_code: code,
        });
    } else {
        eprintln!("error: {}", ui::sanitize(message));
    }
    ExitCode::from(code)
}

fn write_json(value: &impl Serialize) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer(&mut lock, value).expect("writing JSON result");
    use std::io::Write;
    writeln!(lock).expect("terminating JSON result");
}
