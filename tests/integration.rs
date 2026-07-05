use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    cargo_bin_cmd!("rmbg")
}

#[cfg(unix)]
fn fake_uv(directory: &std::path::Path, fail: bool) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let uv = directory.join("uv");
    let body = if fail {
        "#!/bin/sh\necho 'error: model exploded' >&2\nexit 7\n".to_owned()
    } else {
        r#"#!/bin/sh
output=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "--output" ]; then output="$arg"; fi
  previous="$arg"
done
echo '::rmbg-progress::{"completed":1,"total":5,"stage":"device_selected","label":"Device selected","device":"cpu"}' >&2
echo '::rmbg-progress::{"completed":2,"total":5,"stage":"model_loaded","label":"Model loaded","device":null}' >&2
echo '::rmbg-progress::{"completed":3,"total":5,"stage":"image_preprocessed","label":"Image decoded and preprocessed","device":null}' >&2
echo '::rmbg-progress::{"completed":4,"total":5,"stage":"inference_completed","label":"Inference completed","device":null}' >&2
echo 'runtime device: cpu' >&2
echo 'model revision: test-revision' >&2
printf 'png' > "$output"
"#
        .to_owned()
    };
    std::fs::write(&uv, body).unwrap();
    let mut permissions = std::fs::metadata(&uv).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&uv, permissions).unwrap();
    uv
}

#[cfg(unix)]
fn doctor_fixture(
    directory: &std::path::Path,
    cached: bool,
    deep: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let uv = directory.join("uv");
    std::fs::write(&uv, "#!/bin/sh\necho 'uv 0.11.26'\n").unwrap();
    let mut mode = std::fs::metadata(&uv).unwrap().permissions();
    mode.set_mode(0o755);
    std::fs::set_permissions(&uv, mode).unwrap();

    let runtime = directory.join("runtime");
    std::fs::create_dir_all(runtime.join(".venv/bin")).unwrap();
    for file in [
        ".python-version",
        "rmbg_runtime.py",
        "pyproject.toml",
        "uv.lock",
    ] {
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("runtime")
                .join(file),
            runtime.join(file),
        )
        .unwrap();
    }
    let python = runtime.join(".venv/bin/python");
    std::fs::write(
        &python,
        format!(
            "#!/bin/sh\necho '{{\"authenticated\":false,\"model_cached\":{cached},\"cache_detail\":\"fixture cache\",\"cuda\":false,\"mps\":false,\"cpu\":true,\"selected_device\":\"cpu\",\"deep_status\":\"{deep}\",\"deep_detail\":\"fixture deep\"}}'\n"
        ),
    )
    .unwrap();
    let mut mode = std::fs::metadata(&python).unwrap().permissions();
    mode.set_mode(0o755);
    std::fs::set_permissions(&python, mode).unwrap();
    (uv, runtime)
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
fn doctor_help_is_reserved_without_an_input() {
    bin()
        .args(["doctor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--deep"));
}

#[test]
fn quiet_and_json_conflict_with_verbose_as_usage_errors() {
    bin().args(["photo.png", "--quiet", "-v"]).assert().code(1);
    let output = bin().args(["doctor", "--json", "-v"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["kind"], "usage");
    assert!(output.stderr.is_empty());
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
    let directory = tempfile::tempdir().unwrap();
    let uv = fake_uv(directory.path(), false);

    let input = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    bin()
        .env("RMBG_UV_BIN", &uv)
        .arg(input.path())
        .args(["--verbose", "--color", "never"])
        .assert()
        .success()
        .stderr(predicate::str::contains("[1/5] Device selected"))
        .stderr(predicate::str::contains("runtime device: cpu"))
        .stderr(predicate::str::contains("model revision: test-revision"))
        .stderr(predicate::str::contains("Saved"))
        .stderr(predicate::str::contains("\x1b[").not());
}

#[cfg(unix)]
#[test]
fn worker_failure_preserves_details_without_duplicate_error_prefixes() {
    let directory = tempfile::tempdir().unwrap();
    let uv = fake_uv(directory.path(), true);

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

#[test]
fn refuses_existing_output_without_force() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.png");
    let output = directory.path().join("output.png");
    std::fs::write(&input, b"input").unwrap();
    std::fs::write(&output, b"original").unwrap();
    bin()
        .arg(&input)
        .args(["--output", output.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--force"));
    assert_eq!(std::fs::read(output).unwrap(), b"original");
}

#[cfg(unix)]
#[test]
fn force_replaces_atomically_and_json_is_clean() {
    let directory = tempfile::tempdir().unwrap();
    let uv = fake_uv(directory.path(), false);
    let input = directory.path().join("input.png");
    let output = directory.path().join("output.png");
    std::fs::write(&input, b"input").unwrap();
    std::fs::write(&output, b"original").unwrap();
    let result = bin()
        .env("RMBG_UV_BIN", uv)
        .arg(&input)
        .args(["--output", output.to_str().unwrap(), "--force", "--json"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert!(!result.stdout.contains(&0x1b));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["device"], "cpu");
    assert_eq!(std::fs::read(&output).unwrap(), b"png");
    assert!(std::fs::read_dir(directory.path())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".rmbg-")));
}

#[cfg(unix)]
#[test]
fn worker_failure_preserves_existing_force_output_and_cleans_temp() {
    let directory = tempfile::tempdir().unwrap();
    let uv = fake_uv(directory.path(), true);
    let input = directory.path().join("input.png");
    let output = directory.path().join("output.png");
    std::fs::write(&input, b"input").unwrap();
    std::fs::write(&output, b"original").unwrap();
    bin()
        .env("RMBG_UV_BIN", uv)
        .arg(&input)
        .args(["--output", output.to_str().unwrap(), "--force"])
        .assert()
        .code(2);
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
    assert!(std::fs::read_dir(directory.path())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".rmbg-")));
}

#[cfg(unix)]
#[test]
fn redirected_progress_has_no_ansi_or_cursor_controls() {
    let directory = tempfile::tempdir().unwrap();
    let uv = fake_uv(directory.path(), false);
    let input = directory.path().join("input.png");
    std::fs::write(&input, b"input").unwrap();
    let output = bin().env("RMBG_UV_BIN", uv).arg(input).output().unwrap();
    assert!(output.status.success());
    assert!(!output.stderr.contains(&0x1b));
    assert!(!output.stderr.contains(&b'\r'));
    let text = String::from_utf8(output.stderr).unwrap();
    assert_eq!(text.matches("/5]").count(), 5);
}

#[test]
fn json_user_and_runtime_errors_are_single_objects() {
    let user = bin().args(["missing.png", "--json"]).output().unwrap();
    let user_json: serde_json::Value = serde_json::from_slice(&user.stdout).unwrap();
    assert_eq!(user_json["kind"], "user");
    assert_eq!(user_json["exit_code"], 1);
    assert!(user.stderr.is_empty());

    #[cfg(unix)]
    {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.png");
        std::fs::write(&input, b"input").unwrap();
        let runtime = bin()
            .env("RMBG_UV_BIN", fake_uv(directory.path(), true))
            .arg(input)
            .arg("--json")
            .output()
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&runtime.stdout).unwrap();
        assert_eq!(value["kind"], "runtime");
        assert_eq!(value["exit_code"], 2);
        assert!(runtime.stderr.is_empty());
    }
}

#[cfg(unix)]
#[test]
fn doctor_json_covers_ready_actionable_and_deep_modes() {
    let ready_dir = tempfile::tempdir().unwrap();
    let (uv, runtime) = doctor_fixture(ready_dir.path(), true, "ok");
    let ready = bin()
        .env("RMBG_UV_BIN", &uv)
        .env("RMBG_RUNTIME_DIR", &runtime)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(ready.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&ready.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["checks"].as_array().unwrap().len(), 9);
    assert_eq!(value["checks"][8]["status"], "skipped");
    assert!(ready.stderr.is_empty());

    let deep = bin()
        .env("RMBG_UV_BIN", &uv)
        .env("RMBG_RUNTIME_DIR", &runtime)
        .args(["doctor", "--deep", "--json"])
        .output()
        .unwrap();
    let deep_value: serde_json::Value = serde_json::from_slice(&deep.stdout).unwrap();
    assert_eq!(deep_value["checks"][8]["status"], "ok");

    std::fs::write(&uv, "#!/bin/sh\necho 'broken uv' >&2\nexit 9\n").unwrap();
    let failure = bin()
        .env("RMBG_UV_BIN", &uv)
        .env("RMBG_RUNTIME_DIR", &runtime)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(failure.status.code(), Some(2));
    let failure_value: serde_json::Value = serde_json::from_slice(&failure.stdout).unwrap();
    assert_eq!(failure_value["status"], "error");

    let action_dir = tempfile::tempdir().unwrap();
    let (uv, runtime) = doctor_fixture(action_dir.path(), false, "skipped");
    let action = bin()
        .env("RMBG_UV_BIN", uv)
        .env("RMBG_RUNTIME_DIR", runtime)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(action.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&action.stdout).unwrap();
    assert_eq!(value["status"], "action_required");
}

#[cfg(unix)]
#[test]
fn setup_json_is_single_structured_result_and_never_prompts() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let uv = directory.path().join("uv");
    std::fs::write(
        &uv,
        r#"#!/bin/sh
case "$*" in
  "--version") echo 'uv 0.11.26' ;;
  *"hf auth whoami"*) echo 'fixture-user' ;;
  *"--setup"*) echo '{"device":"cpu"}' ;;
  *) : ;;
esac
"#,
    )
    .unwrap();
    let mut mode = std::fs::metadata(&uv).unwrap().permissions();
    mode.set_mode(0o755);
    std::fs::set_permissions(&uv, mode).unwrap();
    let output = bin()
        .env("RMBG_UV_BIN", uv)
        .args(["setup", "--device", "cpu", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["device"], "cpu");
    assert_eq!(value["steps"].as_array().unwrap().len(), 4);
}

#[cfg(unix)]
#[test]
fn setup_json_returns_user_action_instead_of_interactive_login() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let uv = directory.path().join("uv");
    let marker = directory.path().join("login-started");
    std::fs::write(
        &uv,
        format!(
            "#!/bin/sh\ncase \"$*\" in\n  \"--version\") echo 'uv 0.11.26' ;;\n  *\"hf auth whoami\"*) echo 'Not logged in' >&2; exit 1 ;;\n  *\"hf auth login\"*) touch '{}'; exit 9 ;;\n  *) : ;;\nesac\n",
            marker.display()
        ),
    )
    .unwrap();
    let mut mode = std::fs::metadata(&uv).unwrap().permissions();
    mode.set_mode(0o755);
    std::fs::set_permissions(&uv, mode).unwrap();
    let output = bin()
        .env("RMBG_UV_BIN", uv)
        .args(["setup", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["kind"], "user");
    assert!(!marker.exists());
}
