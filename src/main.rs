mod cli;
mod detector;
mod io;
mod processor;

use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use cli::Cli;
use detector::{detect_checkerboard, DetectError, DetectOptions};
use io::{default_output_path, load_image, save_png};
use processor::{ProcessOptions, ProcessResult};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if let Some(detect_err) = err.downcast_ref::<DetectError>() {
                eprintln!("error: {detect_err}");
                return ExitCode::from(2);
            }

            if err.downcast_ref::<io::IoError>().is_some() {
                eprintln!("error: {err:#}");
                return ExitCode::from(2);
            }

            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let input = &cli.input;

    if !input.exists() {
        anyhow::bail!("input file not found: {}", input.display());
    }

    let image = load_image(input)?;
    let detect_options = DetectOptions {
        color_a: cli.color_a()?,
        color_b: cli.color_b()?,
        tile_size: cli.tile_size,
        tolerance: cli.tolerance,
        ..DetectOptions::default()
    };

    let params = detect_checkerboard(&image, &detect_options)?;
    if cli.verbose {
        eprintln!(
            "detected colors: ({},{},{}) / ({},{},{})",
            params.color_a.r,
            params.color_a.g,
            params.color_a.b,
            params.color_b.r,
            params.color_b.g,
            params.color_b.b
        );
        eprintln!("detected tile size: {} px", params.tile_size);
    }

    let ProcessResult {
        image: output_image,
        masked_pixels,
    } = processor::remove_checkerboard(
        &image,
        &params,
        &ProcessOptions {
            tolerance: cli.tolerance,
            output: cli.output_mode()?,
        },
    );

    if masked_pixels == 0 {
        anyhow::bail!("no checkerboard pixels detected; try adjusting --tolerance or --tile-size");
    }

    if cli.verbose {
        eprintln!("masked pixels: {masked_pixels}");
    }

    let output = cli
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(input));

    save_png(&output, &output_image)
        .with_context(|| format!("writing output to {}", output.display()))?;

    if cli.verbose {
        eprintln!("wrote {}", output.display());
    }

    Ok(())
}
