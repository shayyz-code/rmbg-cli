# AGENTS.md

## Project

`rmbg-cli` is a Rust command-line frontend for local background removal with
`briaai/RMBG-2.0`. The executable is named `rmbg`. Rust owns argument
validation, output naming, runtime discovery, and process exit codes; the
uv-managed Python worker owns Transformers inference and image compositing.

The model weights are gated and licensed separately for non-commercial use
under CC BY-NC 4.0. Do not describe the model weights as MIT-licensed or
silently change the model identifier, revision policy, or `trust_remote_code`
behavior.

## Commands

```bash
uv sync --project runtime --frozen
cargo build
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
uv run --project runtime --frozen python runtime/tests/test_runtime.py
cargo run -- photo.jpg -o photo-no-bg.png -v
```

The first real inference requires accepting the model terms on Hugging Face,
authenticating with `hf auth login` or `HF_TOKEN`, and downloading the model.
Unit and CI tests must not depend on network access or cached model weights.

## Architecture

- `src/cli.rs` defines the public `rmbg` interface, validates colors and
  devices, and calculates the default `<stem>-no-bg.png` output path.
- `src/runtime.rs` finds the bundled `runtime` directory and invokes its worker
  through `uv run --frozen`. Release archives place this directory beside the
  Rust executable.
- `runtime/rmbg_runtime.py` loads RMBG-2.0 with Transformers, preprocesses at
  1024×1024, creates the alpha matte, preserves existing transparency, and
  writes transparent or solid-background PNG output.
- `src/main.rs` maps user errors to exit code 1 and runtime/inference failures
  to exit code 2.

Keep the Rust/Python boundary as command-line arguments. Do not duplicate model
inference in Rust or expose the internal worker as a second user-facing CLI.

## Testing and release conventions

- Keep Rust unit tests inline and CLI integration tests in `tests/`.
- Keep Python worker tests in `runtime/tests/` and inject fake model outputs;
  never download RMBG-2.0 in the ordinary test suite.
- Run formatting, Clippy, Rust tests, the frozen uv sync, and Python tests
  before committing.
- Release archives must contain both `rmbg` (or `rmbg.exe`) and the complete
  locked `runtime/` directory. A binary-only archive is not functional.
- Update `README.md`, `CHANGELOG.md`, and this file whenever commands,
  architecture, supported platforms, model behavior, or licensing changes.

