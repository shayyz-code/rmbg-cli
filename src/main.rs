mod cli;
mod runtime;

use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use cli::Cli;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(AppError::User(err)) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
        Err(AppError::Runtime(err)) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
}

enum AppError {
    User(anyhow::Error),
    Runtime(anyhow::Error),
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
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
        eprintln!("model: briaai/RMBG-2.0");
        eprintln!("requested device: {}", runtime::device_label(cli.device));
    }

    runtime::run_worker(&cli, &output, background)
        .with_context(|| format!("processing {}", cli.input.display()))
        .map_err(AppError::Runtime)?;

    if cli.verbose {
        eprintln!("wrote {}", output.display());
    }
    Ok(())
}
