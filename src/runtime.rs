use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

use crate::cli::{Cli, Device};

const RUNTIME_DIR_NAME: &str = "runtime";
const WORKER_NAME: &str = "rmbg_runtime.py";

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

pub fn find_runtime_dir() -> anyhow::Result<PathBuf> {
    if let Some(path) = env::var_os("RMBG_RUNTIME_DIR") {
        return validate_runtime_dir(PathBuf::from(path));
    }

    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            let bundled = parent.join(RUNTIME_DIR_NAME);
            if bundled.join(WORKER_NAME).is_file() {
                return Ok(bundled);
            }
        }
    }

    validate_runtime_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(RUNTIME_DIR_NAME))
}

fn validate_runtime_dir(path: PathBuf) -> anyhow::Result<PathBuf> {
    if path.join(WORKER_NAME).is_file() && path.join("pyproject.toml").is_file() {
        return Ok(path);
    }
    bail!(
        "RMBG runtime not found at {}; reinstall the complete release archive or set RMBG_RUNTIME_DIR",
        path.display()
    )
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
}
