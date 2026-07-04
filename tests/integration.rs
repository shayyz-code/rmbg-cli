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
        .stdout(predicate::str::starts_with(format!(
            "rmbg {}",
            env!("CARGO_PKG_VERSION")
        )));
}

#[test]
fn setup_help_does_not_require_uv_or_an_input_image() {
    bin()
        .args(["setup", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Prepare the local RMBG-2.0 runtime",
        ))
        .stdout(predicate::str::contains("--device"));
}

#[test]
fn forced_color_styles_help_when_stdout_is_redirected() {
    bin()
        .args(["--color", "always", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b["));
}

#[test]
fn color_never_keeps_errors_plain() {
    bin()
        .args(["definitely-missing.png", "--color", "never"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("input file not found"))
        .stderr(predicate::str::contains("\x1b[").not());
}

#[test]
fn no_color_disables_automatic_color() {
    bin()
        .env("NO_COLOR", "1")
        .args(["definitely-missing.png", "--color", "auto"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("\x1b[").not());
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

#[cfg(unix)]
#[test]
fn verbose_worker_output_is_presented_after_processing() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let uv = directory.path().join("uv");
    std::fs::write(
        &uv,
        "#!/bin/sh\necho 'runtime device: cpu' >&2\necho 'model revision: test-revision' >&2\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&uv).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&uv, permissions).unwrap();

    let input = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    bin()
        .env("RMBG_UV_BIN", &uv)
        .arg(input.path())
        .args(["--verbose", "--color", "never"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Removing background from"))
        .stderr(predicate::str::contains("runtime device: cpu"))
        .stderr(predicate::str::contains("model revision: test-revision"))
        .stderr(predicate::str::contains("Saved"))
        .stderr(predicate::str::contains("\x1b[").not());
}

#[cfg(unix)]
#[test]
fn worker_failure_preserves_details_without_duplicate_error_prefixes() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let uv = directory.path().join("uv");
    std::fs::write(&uv, "#!/bin/sh\necho 'error: model exploded' >&2\nexit 7\n").unwrap();
    let mut permissions = std::fs::metadata(&uv).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&uv, permissions).unwrap();

    let input = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    bin()
        .env("RMBG_UV_BIN", &uv)
        .arg(input.path())
        .args(["--verbose", "--color", "never"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("model exploded"))
        .stderr(predicate::str::contains("error: model exploded").not());
}
