use std::path::PathBuf;

use clap::Parser;

use crate::detector::Rgb;
use crate::processor::{BackgroundColor, OutputMode};

#[derive(Debug, Parser)]
#[command(
    name = "rmtg",
    version,
    about = "Remove transparency checkerboard grids from images",
    long_about = None
)]
pub struct Cli {
    /// Input image path
    pub input: PathBuf,

    /// Output image path
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Replace removed grid with a solid color (e.g. white, #FFFFFF, 255,255,255)
    #[arg(long, value_name = "COLOR")]
    pub background: Option<String>,

    /// Color match tolerance
    #[arg(long, default_value_t = 10)]
    pub tolerance: u8,

    /// Force checker tile size and skip auto-detection
    #[arg(long, value_name = "N")]
    pub tile_size: Option<u32>,

    /// Override first checker color (e.g. #FFFFFF or 255,255,255)
    #[arg(long, value_name = "COLOR")]
    pub color_a: Option<String>,

    /// Override second checker color (e.g. #CCCCCC or 204,204,204)
    #[arg(long, value_name = "COLOR")]
    pub color_b: Option<String>,

    /// Log detected parameters and masked pixel count
    #[arg(short, long)]
    pub verbose: bool,
}

impl Cli {
    pub fn output_mode(&self) -> anyhow::Result<OutputMode> {
        match &self.background {
            None => Ok(OutputMode::Transparent),
            Some(value) => parse_background_color(value).map(OutputMode::Solid),
        }
    }

    pub fn color_a(&self) -> anyhow::Result<Option<Rgb>> {
        self.color_a.as_deref().map(parse_rgb).transpose()
    }

    pub fn color_b(&self) -> anyhow::Result<Option<Rgb>> {
        self.color_b.as_deref().map(parse_rgb).transpose()
    }
}

fn parse_rgb(value: &str) -> anyhow::Result<Rgb> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;
        return Ok(Rgb { r, g, b });
    }

    if trimmed.eq_ignore_ascii_case("white") {
        return Ok(Rgb {
            r: 255,
            g: 255,
            b: 255,
        });
    }

    if trimmed.eq_ignore_ascii_case("black") {
        return Ok(Rgb { r: 0, g: 0, b: 0 });
    }

    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.len() == 3 {
        let r = parts[0].parse()?;
        let g = parts[1].parse()?;
        let b = parts[2].parse()?;
        return Ok(Rgb { r, g, b });
    }

    anyhow::bail!("invalid color '{value}': use #RRGGBB, R,G,B, white, or black")
}

fn parse_background_color(value: &str) -> anyhow::Result<BackgroundColor> {
    let rgb = parse_rgb(value)?;
    Ok(BackgroundColor {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_named_colors() {
        assert_eq!(
            parse_rgb("#ff00aa").unwrap(),
            Rgb {
                r: 255,
                g: 0,
                b: 170
            }
        );
        assert_eq!(
            parse_rgb("white").unwrap(),
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
    }

    #[test]
    fn parses_comma_separated_rgb() {
        assert_eq!(
            parse_rgb("10, 20, 30").unwrap(),
            Rgb {
                r: 10,
                g: 20,
                b: 30
            }
        );
    }
}
