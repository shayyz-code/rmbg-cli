mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use image::RgbaImage;
use predicates::prelude::*;
use tempfile::tempdir;

use common::{checkerboard_fixture, ensure_fixtures, foreground_on_grid_fixture, write_fixture};

fn bin() -> Command {
    cargo_bin_cmd!("rmtg")
}

#[test]
fn help_shows_usage() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove transparency checkerboard"));
}

#[test]
fn removes_checkerboard_to_transparent_png() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.png");
    let output = dir.path().join("output.png");

    write_fixture(&input, &checkerboard_fixture(64, 64, 8));

    bin()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("-v")
        .assert()
        .success()
        .stderr(predicate::str::contains("detected tile size: 8 px"));

    let result = image::open(&output).unwrap().to_rgba8();
    assert_eq!(result.dimensions(), (64, 64));
    for pixel in result.pixels() {
        assert_eq!(pixel[3], 0, "expected fully transparent output");
    }
}

#[test]
fn preserves_foreground_on_checkerboard() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.png");
    let output = dir.path().join("output.png");

    write_fixture(&input, &foreground_on_grid_fixture());

    bin()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .assert()
        .success();

    let result = image::open(&output).unwrap().to_rgba8();
    let center = result.get_pixel(30, 30);
    assert_eq!(center[0], 255);
    assert_eq!(center[1], 0);
    assert_eq!(center[2], 0);
    assert_eq!(center[3], 255);
}

#[test]
fn background_flag_replaces_grid_with_solid_color() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.png");
    let output = dir.path().join("output.png");

    write_fixture(&input, &checkerboard_fixture(32, 32, 8));

    bin()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--background")
        .arg("#00ff00")
        .assert()
        .success();

    let result = image::open(&output).unwrap().to_rgba8();
    let pixel = result.get_pixel(0, 0);
    assert_eq!(pixel[0], 0);
    assert_eq!(pixel[1], 255);
    assert_eq!(pixel[2], 0);
    assert_eq!(pixel[3], 255);
}

#[test]
fn missing_input_exits_with_user_error() {
    bin()
        .arg("definitely-missing.png")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("input file not found"));
}

#[test]
fn committed_fixtures_are_processable() {
    let (checkerboard, foreground) = ensure_fixtures();
    let dir = tempdir().unwrap();

    let checker_out = dir.path().join("checker-out.png");
    bin()
        .arg(&checkerboard)
        .arg("-o")
        .arg(&checker_out)
        .assert()
        .success();

    let foreground_out = dir.path().join("foreground-out.png");
    bin()
        .arg(&foreground)
        .arg("-o")
        .arg(&foreground_out)
        .assert()
        .success();

    let checker_result: RgbaImage = image::open(checker_out).unwrap().to_rgba8();
    assert!(checker_result.pixels().all(|p| p[3] == 0));
}
