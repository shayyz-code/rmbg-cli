use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::{ImageFormat, RgbaImage};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("unsupported image format: {0}")]
    UnsupportedFormat(String),
}

pub fn load_image(path: &Path) -> Result<RgbaImage> {
    let format = ImageFormat::from_path(path).ok();
    let img =
        image::open(path).with_context(|| format!("failed to open image: {}", path.display()))?;

    if let Some(fmt) = format {
        if !is_supported_format(fmt) {
            return Err(IoError::UnsupportedFormat(format!("{fmt:?}")).into());
        }
    }

    Ok(img.to_rgba8())
}

pub fn save_png(path: &Path, image: &RgbaImage) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory: {}", parent.display())
            })?;
        }
    }

    image
        .save_with_format(path, ImageFormat::Png)
        .with_context(|| format!("failed to write PNG: {}", path.display()))?;

    Ok(())
}

pub fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}-no-grid.png"))
}

fn is_supported_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::WebP
            | ImageFormat::Bmp
            | ImageFormat::Gif
            | ImageFormat::Tiff
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn default_output_path_appends_suffix() {
        let input = Path::new("photos/icon.png");
        assert_eq!(
            default_output_path(input),
            Path::new("photos/icon-no-grid.png")
        );
    }

    #[test]
    fn round_trip_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");

        let mut img = RgbaImage::new(4, 4);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgba([x as u8, y as u8, 128, 255]);
        }

        save_png(&path, &img).unwrap();
        let loaded = load_image(&path).unwrap();
        assert_eq!(loaded.dimensions(), (4, 4));
        assert_eq!(loaded.get_pixel(2, 1)[0], 2);
    }
}
