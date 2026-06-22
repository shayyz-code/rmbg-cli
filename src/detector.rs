use image::RgbaImage;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn distance_sq(self, other: Rgb) -> u32 {
        let dr = i32::from(self.r) - i32::from(other.r);
        let dg = i32::from(self.g) - i32::from(other.g);
        let db = i32::from(self.b) - i32::from(other.b);
        (dr * dr + dg * dg + db * db) as u32
    }

    pub fn matches(self, other: Rgb, tolerance: u8) -> bool {
        self.r.abs_diff(other.r) <= tolerance
            && self.g.abs_diff(other.g) <= tolerance
            && self.b.abs_diff(other.b) <= tolerance
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CheckerboardParams {
    pub color_a: Rgb,
    pub color_b: Rgb,
    pub tile_size: u32,
    pub origin_color: Rgb,
}

#[derive(Debug, Error)]
pub enum DetectError {
    #[error("could not detect checkerboard colors in image corners")]
    ColorsNotFound,
    #[error("could not detect checkerboard tile size")]
    TileSizeNotFound,
}

pub struct DetectOptions {
    pub color_a: Option<Rgb>,
    pub color_b: Option<Rgb>,
    pub tile_size: Option<u32>,
    pub tolerance: u8,
    pub min_checker_value: u8,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self {
            color_a: None,
            color_b: None,
            tile_size: None,
            tolerance: 10,
            min_checker_value: 200,
        }
    }
}

pub fn detect_checkerboard(
    image: &RgbaImage,
    options: &DetectOptions,
) -> Result<CheckerboardParams, DetectError> {
    let (color_a, color_b) = match (options.color_a, options.color_b) {
        (Some(a), Some(b)) => (a, b),
        _ => detect_colors(image, options.min_checker_value)?,
    };

    let tile_size = match options.tile_size {
        Some(size) if size >= 2 => size,
        _ => detect_tile_size(image, color_a, color_b, options.tolerance)?,
    };

    let origin_color = rgb_at(image, 0, 0);

    Ok(CheckerboardParams {
        color_a,
        color_b,
        tile_size,
        origin_color,
    })
}

/// Fixed tolerance used to cluster corner samples into checker-color groups.
/// Kept independent of the user-facing `--tolerance` (which drives mask
/// matching) so that raising mask tolerance can't accidentally merge two
/// faint checker shades back into a single cluster.
const COLOR_CLUSTER_TOLERANCE: u8 = 8;

/// Corner sampling block size in pixels. Must comfortably exceed the
/// largest auto-detected tile size (32px, see `detect_tile_size`) so each
/// corner block spans at least one full tile boundary and observes both
/// checker shades, rather than landing entirely inside a single tile.
const CORNER_SAMPLE_SIZE: u32 = 64;

fn detect_colors(image: &RgbaImage, min_value: u8) -> Result<(Rgb, Rgb), DetectError> {
    let sample_size = CORNER_SAMPLE_SIZE
        .min(image.width() / 2)
        .min(image.height() / 2)
        .max(1);
    let samples = corner_samples(image, sample_size);
    let checker_samples: Vec<Rgb> = samples
        .into_iter()
        .filter(|c| c.r >= min_value && c.g >= min_value && c.b >= min_value)
        .collect();

    if checker_samples.len() < 2 {
        return Err(DetectError::ColorsNotFound);
    }

    let mut clusters: Vec<(Rgb, usize)> = Vec::new();
    for sample in checker_samples {
        if let Some(cluster) = clusters
            .iter_mut()
            .find(|(center, _)| center.matches(sample, COLOR_CLUSTER_TOLERANCE))
        {
            let prev_count = cluster.1 as u64;
            let new_count = prev_count + 1;
            let center = cluster.0;
            cluster.0 = Rgb {
                r: ((u64::from(center.r) * prev_count + u64::from(sample.r)) / new_count) as u8,
                g: ((u64::from(center.g) * prev_count + u64::from(sample.g)) / new_count) as u8,
                b: ((u64::from(center.b) * prev_count + u64::from(sample.b)) / new_count) as u8,
            };
            cluster.1 += 1;
        } else {
            clusters.push((sample, 1));
        }
    }

    clusters.sort_by_key(|b| std::cmp::Reverse(b.1));

    if clusters.len() < 2 {
        return Err(DetectError::ColorsNotFound);
    }

    let color_a = clusters[0].0;
    let color_b = clusters
        .iter()
        .skip(1)
        .map(|(color, _)| *color)
        .max_by_key(|candidate| color_a.distance_sq(*candidate))
        .unwrap_or(clusters[1].0);

    if color_a.distance_sq(color_b) < u32::from(COLOR_CLUSTER_TOLERANCE).pow(2) * 3 {
        return Err(DetectError::ColorsNotFound);
    }

    Ok((color_a, color_b))
}

fn detect_tile_size(
    image: &RgbaImage,
    color_a: Rgb,
    color_b: Rgb,
    tolerance: u8,
) -> Result<u32, DetectError> {
    let width = image.width();
    let height = image.height();
    let max_scan = width.min(128);

    let mut transition: Option<u32> = None;
    let origin = rgb_at(image, 0, 0);
    let mut last = origin;

    for x in 1..max_scan {
        let current = rgb_at(image, x, 0);
        if !current.matches(last, tolerance)
            && (current.matches(color_a, tolerance) || current.matches(color_b, tolerance))
        {
            transition = Some(x);
            break;
        }
        last = current;
    }

    let mut candidates: Vec<u32> = (4..=32).collect();
    if let Some(t) = transition {
        if !candidates.contains(&t) {
            candidates.push(t);
        }
    }

    let mut best_size = None;
    let mut best_score = 0.0_f64;

    for size in candidates {
        let score = score_tile_size(image, color_a, color_b, tolerance, size, width, height);
        if score > best_score {
            best_score = score;
            best_size = Some(size);
        }
    }

    if best_score < 0.55 {
        return Err(DetectError::TileSizeNotFound);
    }

    best_size.ok_or(DetectError::TileSizeNotFound)
}

fn score_tile_size(
    image: &RgbaImage,
    color_a: Rgb,
    color_b: Rgb,
    tolerance: u8,
    tile_size: u32,
    width: u32,
    height: u32,
) -> f64 {
    let sample_w = width.min(64);
    let sample_h = height.min(64);
    let mut matches = 0_u32;
    let mut total = 0_u32;

    for y in 0..sample_h {
        for x in 0..sample_w {
            let expected =
                expected_color_for_cell(color_a, color_b, rgb_at(image, 0, 0), tile_size, x, y);
            let actual = rgb_at(image, x, y);
            total += 1;
            if actual.matches(expected, tolerance) {
                matches += 1;
            }
        }
    }

    if total == 0 {
        return 0.0;
    }

    f64::from(matches) / f64::from(total)
}

pub fn expected_color_for_cell(
    color_a: Rgb,
    color_b: Rgb,
    origin_color: Rgb,
    tile_size: u32,
    x: u32,
    y: u32,
) -> Rgb {
    let parity = (x / tile_size + y / tile_size) % 2;
    let origin_is_a = origin_color.matches(color_a, 15)
        || origin_color.distance_sq(color_a) <= origin_color.distance_sq(color_b);
    match (parity, origin_is_a) {
        (0, true) | (1, false) => color_a,
        _ => color_b,
    }
}

fn corner_samples(image: &RgbaImage, sample_size: u32) -> Vec<Rgb> {
    let width = image.width();
    let height = image.height();
    let corners = [
        (0_u32, 0_u32),
        (width.saturating_sub(sample_size), 0),
        (0, height.saturating_sub(sample_size)),
        (
            width.saturating_sub(sample_size),
            height.saturating_sub(sample_size),
        ),
    ];

    let mut samples = Vec::new();
    for (cx, cy) in corners {
        for dy in 0..sample_size {
            for dx in 0..sample_size {
                let x = (cx + dx).min(width.saturating_sub(1));
                let y = (cy + dy).min(height.saturating_sub(1));
                samples.push(rgb_at(image, x, y));
            }
        }
    }
    samples
}

fn rgb_at(image: &RgbaImage, x: u32, y: u32) -> Rgb {
    let pixel = image.get_pixel(x, y);
    Rgb {
        r: pixel[0],
        g: pixel[1],
        b: pixel[2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn make_checkerboard(width: u32, height: u32, tile: u32, light: Rgb, dark: Rgb) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let color = expected_color_for_cell(light, dark, light, tile, x, y);
                img.put_pixel(x, y, Rgba([color.r, color.g, color.b, 255]));
            }
        }
        img
    }

    #[test]
    fn detects_standard_checkerboard() {
        let img = make_checkerboard(
            64,
            64,
            8,
            Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            Rgb {
                r: 204,
                g: 204,
                b: 204,
            },
        );

        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        assert_eq!(params.tile_size, 8);
        assert!(params.color_a.matches(
            Rgb {
                r: 255,
                g: 255,
                b: 255
            },
            15
        ));
        assert!(params.color_b.matches(
            Rgb {
                r: 204,
                g: 204,
                b: 204
            },
            15
        ));
    }

    #[test]
    fn detects_faint_large_tile_checkerboard() {
        // Regression test: a 24px-tile board with only ~13/channel contrast
        // (253,253,253) vs (240,240,240) used to fail color detection because
        // the old 5x5 corner sampling window was smaller than the tile size
        // and could land entirely inside a single tile.
        let img = make_checkerboard(
            128,
            128,
            24,
            Rgb {
                r: 253,
                g: 253,
                b: 253,
            },
            Rgb {
                r: 240,
                g: 240,
                b: 240,
            },
        );

        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        assert_eq!(params.tile_size, 24);
        assert!(params.color_a.matches(
            Rgb {
                r: 253,
                g: 253,
                b: 253
            },
            5
        ));
        assert!(params.color_b.matches(
            Rgb {
                r: 240,
                g: 240,
                b: 240
            },
            5
        ));
    }

    #[test]
    fn detects_sixteen_pixel_tiles() {
        let img = make_checkerboard(
            96,
            96,
            16,
            Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            Rgb {
                r: 230,
                g: 230,
                b: 230,
            },
        );

        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        assert_eq!(params.tile_size, 16);
    }

    #[test]
    fn expected_color_alternates_by_grid() {
        let a = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let b = Rgb {
            r: 204,
            g: 204,
            b: 204,
        };
        assert!(expected_color_for_cell(a, b, a, 8, 0, 0).matches(a, 0));
        assert!(expected_color_for_cell(a, b, a, 8, 8, 0).matches(b, 0));
        assert!(expected_color_for_cell(a, b, a, 8, 0, 8).matches(b, 0));
    }
}
