# AGENTS.md

## Project

`rmbg-cli` is a Rust command-line frontend for local background removal with
`briaai/RMBG-2.0`. The executable is named `rmbg`. Rust owns argument
validation, output naming, runtime discovery, and process exit codes; the
uv-managed Python worker owns Transformers inference and image compositing.
Distribution uses the `rmbg2-cli` npm launcher with platform-specific native
packages, plus checksummed GitHub Release installers.

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
npm run check:versions
npm test
uv run --project runtime --frozen python runtime/tests/test_runtime.py
cargo run -- setup --device cpu
cargo run -- photo.jpg -o photo-no-bg.png -v
```

The first real inference requires accepting the model terms on Hugging Face,
authenticating with `hf auth login` or `HF_TOKEN`, and downloading the model.
Unit and CI tests must not depend on network access or cached model weights.

## Architecture

- `src/cli.rs` defines the public `rmbg` interface, validates colors and
  devices, and calculates the default `<stem>-no-bg.png` output path.
- `src/runtime.rs` resolves an explicit, adjacent, development, or embedded
  `runtime` directory and invokes its worker through `uv run --frozen`. It also
  orchestrates dependency sync, authentication, model download, and load
  validation for `rmbg setup`. Native
  release binaries embed the locked runtime sources and materialize them into
  the platform cache when no development or explicitly configured runtime is
  available.
- `npm/bin/rmbg.js` selects one of the four optional native npm packages and
  transparently forwards arguments, signals, and exit status to the Rust CLI.
- `runtime/rmbg_runtime.py` loads RMBG-2.0 with Transformers, preprocesses at
  1024×1024, creates the alpha matte, preserves existing transparency, and
  writes transparent or solid-background PNG output.
- `src/main.rs` maps user errors to exit code 1 and runtime/inference failures
  to exit code 2.

`setup` is a reserved first argument so the existing `rmbg <INPUT>` interface
remains compatible. Keep setup idempotent, never execute a remote uv installer,
and never automate acceptance of BRIA's license terms.

Keep the Rust/Python boundary as command-line arguments. Do not duplicate model
inference in Rust or expose the internal worker as a second user-facing CLI.

## Testing and release conventions

- Keep Rust unit tests inline and CLI integration tests in `tests/`.
- Keep Python worker tests in `runtime/tests/` and inject fake model outputs;
  never download RMBG-2.0 in the ordinary test suite.
- Run formatting, Clippy, Rust tests, the frozen uv sync, and Python tests
  before committing.
- Keep `Cargo.toml`, the Python runtime, the main npm package, all platform
  manifests, and release tags on exactly the same version.
- Publish native npm packages before `rmbg2-cli`. The first release is manually
  bootstrapped with npm 2FA; subsequent releases use GitHub OIDC trusted
  publishing after `NPM_TRUSTED_PUBLISHING` is enabled as a repository variable.
- Supported release targets are Linux glibc x64/ARM64, macOS ARM64, and Windows
  x64. Release archives are functional binary-only artifacts because the locked
  Python runtime sources are embedded.
- Direct installers must verify `SHA256SUMS` and must never run `rmbg setup`
  automatically.
- Update `README.md`, `CHANGELOG.md`, and this file whenever commands,
  architecture, supported platforms, model behavior, or licensing changes.

## First npm release bootstrap

Leave the repository variable `NPM_TRUSTED_PUBLISHING` unset for `v0.4.1`.
After its GitHub Release succeeds, download its five `.tgz` assets and publish
the four platform packages before the launcher:

```bash
npm publish rmbg2-cli-linux-x64-gnu-0.4.1.tgz --access public
npm publish rmbg2-cli-linux-arm64-gnu-0.4.1.tgz --access public
npm publish rmbg2-cli-darwin-arm64-0.4.1.tgz --access public
npm publish rmbg2-cli-windows-x64-0.4.1.tgz --access public
npm publish rmbg2-cli-0.4.1.tgz --access public
```

For each npm package, configure `shayyz-code/rmbg-cli` and
`.github/workflows/release.yml` as its GitHub Actions trusted publisher with
`npm publish` permission. Then set the GitHub repository variable
`NPM_TRUSTED_PUBLISHING` to `true`. Do not add an npm publishing token.
