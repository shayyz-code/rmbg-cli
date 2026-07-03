use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    cargo_bin_cmd!("rmbg")
}

#[test]
fn help_describes_model_cli() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("BRIA RMBG-2.0"))
        .stdout(predicate::str::contains("--device"))
        .stdout(predicate::str::contains("--background"));
}

#[test]
fn version_uses_renamed_executable() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("rmbg 0.3.0"));
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
fn invalid_background_exits_with_user_error_before_starting_runtime() {
    let input = tempfile::NamedTempFile::new().unwrap();
    bin()
        .arg(input.path())
        .arg("--background")
        .arg("invalid")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("invalid color"));
}

#[test]
fn refusing_to_overwrite_input_is_a_user_error() {
    let input = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    bin()
        .arg(input.path())
        .arg("--output")
        .arg(input.path())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "output path must differ from input path",
        ));
}
