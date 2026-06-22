use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};

pub fn checkerboard_fixture(width: u32, height: u32, tile: u32) -> RgbaImage {
    let light = Rgba([255, 255, 255, 255]);
    let dark = Rgba([204, 204, 204, 255]);
    let mut img = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let parity = (x / tile + y / tile) % 2;
            img.put_pixel(x, y, if parity == 0 { light } else { dark });
        }
    }

    img
}

pub fn foreground_on_grid_fixture() -> RgbaImage {
    let mut img = checkerboard_fixture(64, 64, 8);
    for y in 20..44 {
        for x in 20..44 {
            img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }
    img
}

pub fn write_fixture(path: &Path, image: &RgbaImage) {
    image.save(path).expect("write fixture png");
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn ensure_fixtures() -> (PathBuf, PathBuf) {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("create fixtures dir");

    let checkerboard = dir.join("checkerboard_8px.png");
    let foreground = dir.join("foreground_on_grid.png");

    if !checkerboard.exists() {
        write_fixture(&checkerboard, &checkerboard_fixture(64, 64, 8));
    }
    if !foreground.exists() {
        write_fixture(&foreground, &foreground_on_grid_fixture());
    }

    (checkerboard, foreground)
}
