use std::collections::VecDeque;

use image::RgbaImage;

use crate::detector::{CheckerboardParams, Rgb};

#[derive(Debug, Clone, Copy)]
pub struct BackgroundColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputMode {
    Transparent,
    Solid(BackgroundColor),
}

pub struct ProcessOptions {
    pub tolerance: u8,
    pub output: OutputMode,
}

pub struct ProcessResult {
    pub image: RgbaImage,
    pub masked_pixels: u64,
}

/// Removes the checkerboard background by flood-filling from the image
/// border across pixels that match either checker color, rather than
/// predicting a grid pattern. This sidesteps non-integer / drifting tile
/// periods entirely: the background is treated as whatever checker-colored
/// region is reachable from the edge, and foreground content (which the
/// checker colors don't reach, by construction of a real checkerboard) is
/// left untouched even where it happens to be a similar light shade, as long
/// as it's enclosed rather than border-connected.
pub fn remove_checkerboard(
    image: &RgbaImage,
    params: &CheckerboardParams,
    options: &ProcessOptions,
) -> ProcessResult {
    let (width, height) = image.dimensions();
    let mask = flood_fill_from_border(image, params, options.tolerance, width, height);

    let mut output = image.clone();
    let mut masked_pixels = 0_u64;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if !mask[idx] {
                continue;
            }

            masked_pixels += 1;
            let pixel = output.get_pixel_mut(x, y);
            match options.output {
                OutputMode::Transparent => pixel[3] = 0,
                OutputMode::Solid(bg) => {
                    pixel[0] = bg.r;
                    pixel[1] = bg.g;
                    pixel[2] = bg.b;
                    pixel[3] = 255;
                }
            }
        }
    }

    ProcessResult {
        image: output,
        masked_pixels,
    }
}

fn flood_fill_from_border(
    image: &RgbaImage,
    params: &CheckerboardParams,
    tolerance: u8,
    width: u32,
    height: u32,
) -> Vec<bool> {
    let mut visited = vec![false; (width * height) as usize];
    let is_checker = |x: u32, y: u32| {
        let rgb = rgb_at(image, x, y);
        rgb.matches(params.color_a, tolerance) || rgb.matches(params.color_b, tolerance)
    };

    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    let enqueue = |x: u32, y: u32, visited: &mut Vec<bool>, queue: &mut VecDeque<(u32, u32)>| {
        let idx = (y * width + x) as usize;
        if !visited[idx] && is_checker(x, y) {
            visited[idx] = true;
            queue.push_back((x, y));
        }
    };

    if width == 0 || height == 0 {
        return visited;
    }

    for x in 0..width {
        enqueue(x, 0, &mut visited, &mut queue);
        if height > 1 {
            enqueue(x, height - 1, &mut visited, &mut queue);
        }
    }
    for y in 0..height {
        enqueue(0, y, &mut visited, &mut queue);
        if width > 1 {
            enqueue(width - 1, y, &mut visited, &mut queue);
        }
    }

    while let Some((x, y)) = queue.pop_front() {
        if x + 1 < width {
            enqueue(x + 1, y, &mut visited, &mut queue);
        }
        if x > 0 {
            enqueue(x - 1, y, &mut visited, &mut queue);
        }
        if y + 1 < height {
            enqueue(x, y + 1, &mut visited, &mut queue);
        }
        if y > 0 {
            enqueue(x, y - 1, &mut visited, &mut queue);
        }
    }

    visited
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
    use crate::detector::{detect_checkerboard, DetectOptions};
    use image::{Rgba, RgbaImage};

    const LIGHT: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    const DARK: Rgb = Rgb {
        r: 204,
        g: 204,
        b: 204,
    };

    fn checkerboard(width: u32, height: u32, tile: u32) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let parity = (x / tile + y / tile) % 2;
                let color = if parity == 0 { LIGHT } else { DARK };
                img.put_pixel(x, y, Rgba([color.r, color.g, color.b, 255]));
            }
        }
        img
    }

    #[test]
    fn removes_entire_checkerboard_with_transparency() {
        let img = checkerboard(64, 64, 8);
        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        let result = remove_checkerboard(
            &img,
            &params,
            &ProcessOptions {
                tolerance: 10,
                output: OutputMode::Transparent,
            },
        );

        assert_eq!(result.masked_pixels, 64 * 64);
        for pixel in result.image.pixels() {
            assert_eq!(pixel[3], 0);
        }
    }

    #[test]
    fn preserves_foreground_shape() {
        let mut img = checkerboard(64, 64, 8);
        for y in 20..44 {
            for x in 20..44 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }

        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        let result = remove_checkerboard(
            &img,
            &params,
            &ProcessOptions {
                tolerance: 10,
                output: OutputMode::Transparent,
            },
        );

        assert_eq!(result.image.get_pixel(30, 30)[0], 255);
        assert_eq!(result.image.get_pixel(30, 30)[1], 0);
        assert_eq!(result.image.get_pixel(30, 30)[3], 255);
        assert_eq!(result.image.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn applies_solid_background() {
        let img = checkerboard(32, 32, 8);
        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        let result = remove_checkerboard(
            &img,
            &params,
            &ProcessOptions {
                tolerance: 10,
                output: OutputMode::Solid(BackgroundColor {
                    r: 0,
                    g: 128,
                    b: 255,
                }),
            },
        );

        assert_eq!(result.image.get_pixel(0, 0)[0], 0);
        assert_eq!(result.image.get_pixel(0, 0)[1], 128);
        assert_eq!(result.image.get_pixel(0, 0)[2], 255);
        assert_eq!(result.image.get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn enclosed_light_region_survives_border_fill() {
        // A non-matching ring (dark navy) encloses a light/white square in
        // the middle of the checkerboard. Border flood-fill must clear the
        // checkerboard but must NOT reach the enclosed light square, proving
        // connectivity (not color alone) discriminates background from
        // foreground.
        let mut img = checkerboard(40, 40, 8);
        let navy = Rgba([20, 20, 60, 255]);
        let white = Rgba([255, 255, 255, 255]);

        // Ring from (10,10) to (29,29).
        for y in 10..30 {
            for x in 10..30 {
                let on_ring = x == 10 || x == 29 || y == 10 || y == 29;
                img.put_pixel(x, y, if on_ring { navy } else { white });
            }
        }

        let params = detect_checkerboard(&img, &DetectOptions::default()).unwrap();
        let result = remove_checkerboard(
            &img,
            &params,
            &ProcessOptions {
                tolerance: 10,
                output: OutputMode::Transparent,
            },
        );

        // Outer checkerboard cleared.
        assert_eq!(result.image.get_pixel(0, 0)[3], 0);
        assert_eq!(result.image.get_pixel(39, 39)[3], 0);

        // Enclosed light square (interior of the ring) untouched.
        for y in 15..25 {
            for x in 15..25 {
                let pixel = result.image.get_pixel(x, y);
                assert_eq!(pixel[3], 255, "interior pixel ({x},{y}) was masked");
                assert_eq!([pixel[0], pixel[1], pixel[2]], [255, 255, 255]);
            }
        }

        // The ring itself never matched the checker colors, so it's
        // untouched too.
        assert_eq!(result.image.get_pixel(10, 10)[3], 255);
    }
}
