use image::RgbaImage;

use crate::detector::{expected_color_for_cell, CheckerboardParams, Rgb};

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

pub fn remove_checkerboard(
    image: &RgbaImage,
    params: &CheckerboardParams,
    options: &ProcessOptions,
) -> ProcessResult {
    let (width, height) = image.dimensions();
    let mut mask = vec![false; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let rgb = rgb_at(image, x, y);
            let expected = expected_color_for_cell(
                params.color_a,
                params.color_b,
                params.origin_color,
                params.tile_size,
                x,
                y,
            );

            let matches_checker_color = rgb.matches(params.color_a, options.tolerance)
                || rgb.matches(params.color_b, options.tolerance);
            let matches_grid = rgb.matches(expected, options.tolerance);

            mask[idx] = matches_checker_color && matches_grid;
        }
    }

    refine_mask_with_shell_overlap(&mut mask, image, params, options.tolerance, width, height);

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

fn refine_mask_with_shell_overlap(
    mask: &mut [bool],
    image: &RgbaImage,
    params: &CheckerboardParams,
    tolerance: u8,
    width: u32,
    height: u32,
) {
    let color_a_mask = color_mask(image, params.color_a, tolerance, width, height);
    let color_b_mask = color_mask(image, params.color_b, tolerance, width, height);

    let shell_a = shell_from_mask(&color_a_mask, width, height);
    let shell_b = shell_from_mask(&color_b_mask, width, height);

    let overlap = and_masks(&shell_a, &shell_b, width, height);
    let expanded_overlap = dilate_mask(&overlap, width, height, 8);

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if !expanded_overlap[idx] {
                continue;
            }

            let rgb = rgb_at(image, x, y);
            if rgb.matches(params.color_a, tolerance) || rgb.matches(params.color_b, tolerance) {
                mask[idx] = true;
            }
        }
    }
}

fn color_mask(image: &RgbaImage, color: Rgb, tolerance: u8, width: u32, height: u32) -> Vec<bool> {
    let mut mask = vec![false; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            mask[idx] = rgb_at(image, x, y).matches(color, tolerance);
        }
    }
    mask
}

fn shell_from_mask(mask: &[bool], width: u32, height: u32) -> Vec<bool> {
    let dilated = dilate_mask(mask, width, height, 1);
    let mut shell = vec![false; mask.len()];
    for idx in 0..mask.len() {
        shell[idx] = dilated[idx] && !mask[idx];
    }
    dilate_mask(&shell, width, height, 1)
}

fn dilate_mask(mask: &[bool], width: u32, height: u32, radius: u32) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    let r = radius as i32;

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let idx = (y as u32 * width + x as u32) as usize;
            if mask[idx] {
                out[idx] = true;
                continue;
            }

            'neighbor: for dy in -r..=r {
                for dx in -r..=r {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let nidx = (ny as u32 * width + nx as u32) as usize;
                    if mask[nidx] {
                        out[idx] = true;
                        break 'neighbor;
                    }
                }
            }
        }
    }

    out
}

fn and_masks(a: &[bool], b: &[bool], width: u32, height: u32) -> Vec<bool> {
    let len = (width * height) as usize;
    (0..len).map(|i| a[i] && b[i]).collect()
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

    fn checkerboard(width: u32, height: u32, tile: u32) -> RgbaImage {
        let light = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let dark = Rgb {
            r: 204,
            g: 204,
            b: 204,
        };
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
}
