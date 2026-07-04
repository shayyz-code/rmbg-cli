mod cli;
mod runtime;
mod ui;

use std::process::ExitCode;

use anyhow::Context;

use cli::{Cli, Invocation};

fn main() -> ExitCode {
    let invocation = cli::parse_invocation();
    let ui = ui::Ui::new(invocation.color());
    match run(invocation, &ui) {
        Ok(()) => ExitCode::SUCCESS,
        Err(AppError::User(err)) => {
            ui.error(&format!("{err:#}"));
            ExitCode::from(1)
        }
        Err(AppError::Runtime(err)) => {
            ui.error(&format!("{err:#}"));
            ExitCode::from(2)
        }
    }
}

enum AppError {
    User(anyhow::Error),
    Runtime(anyhow::Error),
}

fn run(invocation: Invocation, ui: &ui::Ui) -> Result<(), AppError> {
    match invocation {
        Invocation::Remove(cli) => run_remove(cli, ui),
        Invocation::Setup(setup) => {
            runtime::run_setup(setup.device, ui).map_err(|error| match error {
                runtime::SetupError::User(error) => AppError::User(error),
                runtime::SetupError::Runtime(error) => AppError::Runtime(error),
            })
        }
    }
}

fn run_remove(cli: Cli, ui: &ui::Ui) -> Result<(), AppError> {
    if !cli.input.is_file() {
        return Err(AppError::User(anyhow::anyhow!(
            "input file not found: {}",
            cli.input.display()
        )));
    }

    let background = cli.background_rgb().map_err(AppError::User)?;
    let output = cli.output_path();
    if output == cli.input {
        return Err(AppError::User(anyhow::anyhow!(
            "output path must differ from input path"
        )));
    }

    if cli.verbose {
        ui.detail("model", "briaai/RMBG-2.0");
        ui.detail("requested device", runtime::device_label(cli.device));
    }

    let filename = cli
        .input
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let mut processing = ui.processing(filename, cli.verbose);
    let worker_output = runtime::run_worker(&cli, &output, background)
        .with_context(|| format!("processing {}", cli.input.display()))
        .map_err(AppError::Runtime);
    let elapsed = processing.stop();
    let worker_output = worker_output?;

    if cli.verbose {
        for line in worker_output.lines() {
            ui.detail("runtime", line);
        }
    }
    if ui.is_interactive() || cli.verbose {
        ui.success(&format!(
            "Saved {} ({})",
            output.display(),
            ui::format_duration(elapsed)
        ));
    }
    Ok(())
}
