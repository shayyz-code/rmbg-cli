use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Device {
    Auto,
    Cuda,
    Mps,
    Cpu,
}

impl Device {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cuda => "cuda",
            Self::Mps => "mps",
            Self::Cpu => "cpu",
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "rmbg",
    version,
    about = "Remove image backgrounds locally with BRIA RMBG-2.0",
    long_about = None,
    after_help = "Commands:\n  setup  Install dependencies, authenticate, and download RMBG-2.0\n\nA file named 'setup' must be passed as './setup'."
)]
pub struct Cli {
    /// Input image path
    pub input: PathBuf,

    /// Output PNG path (default: <input>-no-bg.png)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Composite the foreground onto a solid color (#RRGGBB, R,G,B, white, black)
    #[arg(long, value_name = "COLOR")]
    pub background: Option<String>,

    /// Inference device; auto prefers CUDA, then MPS, then CPU
    #[arg(long, value_enum, default_value_t = Device::Auto)]
    pub device: Device,

    /// Print runtime, device, model, and output information
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Parser)]
#[command(
    name = "rmbg setup",
    version,
    about = "Prepare the local RMBG-2.0 runtime",
    long_about = None
)]
pub struct SetupCli {
    /// Device on which to validate the model; auto prefers CUDA, then MPS, then CPU
    #[arg(long, value_enum, default_value_t = Device::Auto)]
    pub device: Device,
}

#[derive(Debug)]
pub enum Invocation {
    Remove(Cli),
    Setup(SetupCli),
}

pub fn parse_invocation() -> Invocation {
    parse_invocation_from(std::env::args_os()).unwrap_or_else(|error| error.exit())
}

fn parse_invocation_from<I, T>(args: I) -> Result<Invocation, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let is_setup = args.get(1).is_some_and(|value| value == "setup");

    if is_setup {
        let setup_args = std::iter::once(args[0].clone()).chain(args.into_iter().skip(2));
        SetupCli::try_parse_from(setup_args).map(Invocation::Setup)
    } else {
        Cli::try_parse_from(args).map(Invocation::Remove)
    }
}

impl Cli {
    pub fn background_rgb(&self) -> anyhow::Result<Option<[u8; 3]>> {
        self.background.as_deref().map(parse_rgb).transpose()
    }

    pub fn output_path(&self) -> PathBuf {
        self.output
            .clone()
            .unwrap_or_else(|| default_output_path(&self.input))
    }
}

pub fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    input.with_file_name(format!("{stem}-no-bg.png"))
}

fn parse_rgb(value: &str) -> anyhow::Result<[u8; 3]> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok([
            u8::from_str_radix(&hex[0..2], 16)?,
            u8::from_str_radix(&hex[2..4], 16)?,
            u8::from_str_radix(&hex[4..6], 16)?,
        ]);
    }

    if trimmed.eq_ignore_ascii_case("white") {
        return Ok([255, 255, 255]);
    }
    if trimmed.eq_ignore_ascii_case("black") {
        return Ok([0, 0, 0]);
    }

    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.len() == 3 {
        return Ok([parts[0].parse()?, parts[1].parse()?, parts[2].parse()?]);
    }

    anyhow::bail!("invalid color '{value}': use #RRGGBB, R,G,B, white, or black")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_colors() {
        assert_eq!(parse_rgb("#ff00aa").unwrap(), [255, 0, 170]);
        assert_eq!(parse_rgb("10, 20, 30").unwrap(), [10, 20, 30]);
        assert_eq!(parse_rgb("white").unwrap(), [255, 255, 255]);
        assert!(parse_rgb("not-a-color").is_err());
    }

    #[test]
    fn derives_default_output_path() {
        assert_eq!(
            default_output_path(Path::new("images/photo.jpg")),
            PathBuf::from("images/photo-no-bg.png")
        );
    }

    #[test]
    fn dispatches_setup_without_changing_image_syntax() {
        let setup = parse_invocation_from(["rmbg", "setup", "--device", "cpu"]).unwrap();
        assert!(matches!(
            setup,
            Invocation::Setup(SetupCli {
                device: Device::Cpu
            })
        ));

        let remove = parse_invocation_from(["rmbg", "photo.jpg", "--device", "cpu"]).unwrap();
        assert!(matches!(
            remove,
            Invocation::Remove(Cli {
                device: Device::Cpu,
                ..
            })
        ));
    }
}
