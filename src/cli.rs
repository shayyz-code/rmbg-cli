use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use clap::{Args, ColorChoice, Parser, ValueEnum};

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

#[derive(Debug, Clone, Args)]
pub struct OutputArgs {
    /// Suppress progress, success, setup-step, and informational output
    #[arg(long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Emit exactly one machine-readable JSON object on stdout
    #[arg(long, conflicts_with = "verbose")]
    pub json: bool,

    /// Print runtime and diagnostic details
    #[arg(short, long)]
    pub verbose: bool,

    /// Control colored output
    #[arg(
        long,
        value_enum,
        value_name = "WHEN",
        default_value_t = ColorChoice::Auto
    )]
    pub color: ColorChoice,
}

#[derive(Debug, Parser)]
#[command(
    name = "rmbg",
    version,
    about = "Remove image backgrounds locally with BRIA RMBG-2.0",
    long_about = None,
    styles = crate::ui::help_styles(),
    after_help = "Commands:\n  setup   Install dependencies, authenticate, and download RMBG-2.0\n  doctor  Diagnose local runtime readiness without changing it\n\nExamples:\n  rmbg photo.jpg\n  rmbg photo.jpg --background white -o cutout.png\n  rmbg setup --device cpu\n  rmbg doctor --deep\n\nFiles named 'setup' or 'doctor' must be passed as './setup' or './doctor'."
)]
pub struct Cli {
    /// Input image path
    pub input: PathBuf,

    /// Output PNG path (default: <input>-no-bg.png)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Replace an existing output atomically
    #[arg(long)]
    pub force: bool,

    /// Composite the foreground onto a solid color (#RRGGBB, R,G,B, white, black)
    #[arg(long, value_name = "COLOR")]
    pub background: Option<String>,

    /// Inference device; auto prefers CUDA, then MPS, then CPU
    #[arg(long, value_enum, default_value_t = Device::Auto)]
    pub device: Device,

    #[command(flatten)]
    pub output_args: OutputArgs,
}

#[derive(Debug, Parser)]
#[command(
    name = "rmbg setup",
    version,
    about = "Prepare the local RMBG-2.0 runtime",
    long_about = None,
    styles = crate::ui::help_styles(),
    after_help = "Example:\n  rmbg setup --device cpu"
)]
pub struct SetupCli {
    /// Device on which to validate the model; auto prefers CUDA, then MPS, then CPU
    #[arg(long, value_enum, default_value_t = Device::Auto)]
    pub device: Device,

    #[command(flatten)]
    pub output_args: OutputArgs,
}

#[derive(Debug, Parser)]
#[command(
    name = "rmbg doctor",
    version,
    about = "Diagnose local RMBG-2.0 readiness without changing it",
    long_about = None,
    styles = crate::ui::help_styles()
)]
pub struct DoctorCli {
    /// Load and validate the already-cached pinned model without downloading it
    #[arg(long)]
    pub deep: bool,

    #[command(flatten)]
    pub output_args: OutputArgs,
}

#[derive(Debug)]
pub enum Invocation {
    Remove(Cli),
    Setup(SetupCli),
    Doctor(DoctorCli),
}

pub fn parse_invocation() -> Result<Invocation, clap::Error> {
    parse_invocation_from(std::env::args_os())
}

fn parse_invocation_from<I, T>(args: I) -> Result<Invocation, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    crate::ui::configure_color(requested_color(&args));
    let command = args.get(1).and_then(|value| value.to_str());
    let command_args = || std::iter::once(args[0].clone()).chain(args.iter().skip(2).cloned());

    match command {
        Some("setup") => SetupCli::try_parse_from(command_args()).map(Invocation::Setup),
        Some("doctor") => DoctorCli::try_parse_from(command_args()).map(Invocation::Doctor),
        _ => Cli::try_parse_from(args).map(Invocation::Remove),
    }
}

impl Invocation {
    pub fn output_args(&self) -> &OutputArgs {
        match self {
            Self::Remove(cli) => &cli.output_args,
            Self::Setup(cli) => &cli.output_args,
            Self::Doctor(cli) => &cli.output_args,
        }
    }
}

pub fn json_requested(args: &[OsString]) -> bool {
    args.iter().skip(1).any(|arg| arg == OsStr::new("--json"))
}

fn requested_color(args: &[OsString]) -> ColorChoice {
    if json_requested(args) {
        return ColorChoice::Never;
    }
    for (index, argument) in args.iter().enumerate().skip(1) {
        let value = argument.to_string_lossy();
        let color = if let Some(value) = value.strip_prefix("--color=") {
            Some(value.to_owned())
        } else if value == "--color" {
            args.get(index + 1)
                .map(|value| value.to_string_lossy().into_owned())
        } else {
            None
        };

        if let Some(color) = color {
            return match color.as_str() {
                "always" => ColorChoice::Always,
                "never" => ColorChoice::Never,
                _ => ColorChoice::Auto,
            };
        }
    }
    ColorChoice::Auto
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
    fn dispatches_reserved_commands() {
        assert!(matches!(
            parse_invocation_from(["rmbg", "setup", "--device", "cpu"]).unwrap(),
            Invocation::Setup(_)
        ));
        assert!(matches!(
            parse_invocation_from(["rmbg", "doctor", "--deep"]).unwrap(),
            Invocation::Doctor(DoctorCli { deep: true, .. })
        ));
        assert!(matches!(
            parse_invocation_from(["rmbg", "photo.jpg"]).unwrap(),
            Invocation::Remove(_)
        ));
    }

    #[test]
    fn automation_conflicts_are_enforced() {
        assert!(parse_invocation_from(["rmbg", "photo.jpg", "--quiet", "-v"]).is_err());
        assert!(parse_invocation_from(["rmbg", "doctor", "--json", "-v"]).is_err());
    }
}
