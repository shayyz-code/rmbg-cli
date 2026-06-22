# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`rmtg` is a Rust CLI that removes the gray/white transparency checkerboard baked into exported images (Photoshop, Illustrator, screenshots) and produces a clean PNG — either with real transparency or a solid background fill.

## Commands

```bash
cargo build                              # debug build
cargo test                               # unit + integration tests
cargo test --test integration            # integration tests only
cargo test <test_name>                   # run a single test by name (substring match)
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo run -- photo.png -o clean.png -v   # run the CLI locally
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy, and tests on Linux/macOS/Windows. The release workflow (`.github/workflows/release.yml`) builds binaries for all platforms on tag push.

## Architecture

The pipeline is a strict linear flow through four modules, each owning one stage:

1. **`cli.rs`** — argument parsing (`clap`). Also parses color strings (`#RRGGBB`, `R,G,B`, `white`, `black`) into `detector::Rgb` and resolves `--background` into a `processor::OutputMode`.
2. **`io.rs`** — loads the input image to `RgbaImage`, validates format support, computes the default output path (`<input>-no-grid.png`), and saves PNG output.
3. **`detector.rs`** — given the loaded image, determines `CheckerboardParams` (just the two checker colors): samples the four image corners (in 64px blocks, large enough to span several tiles even on big checker patterns), filters for bright pixels above `min_checker_value`, and clusters them with a fixed tolerance (`COLOR_CLUSTER_TOLERANCE`, independent of the user-facing `--tolerance`) to find the two most distinct color clusters. Colors can be overridden via CLI flags, skipping detection.
4. **`processor.rs`** — given `CheckerboardParams`, removes the background via **border-seeded flood fill**: every border pixel matching either checker color (within `--tolerance`) seeds a 4-connected BFS that grows into checker-colored neighbors. The visited set becomes the background mask, applied as alpha=0 (transparent) or a solid RGB fill. There is deliberately no tile-size or grid-phase modeling — connectivity to the border is what discriminates background from foreground, which makes this robust to non-integer/drifting checker periods (real exported checkerboards are rarely an exact integer pixel grid). The tradeoff: checkerboard not connected to the border won't be removed, and foreground that is itself checker-colored *and* touches the border can get swept up (see README Limitations).

`main.rs` wires the two stages together and maps domain errors to exit codes: `1` for user errors (bad args, missing file), `2` for processing errors (`DetectError`, `IoError`, or zero masked pixels).

## Testing conventions

- Unit tests live inline in each module (`#[cfg(test)] mod tests`), generally building a synthetic checkerboard via a small parity-based test helper and asserting on detection/masking output.
- Integration tests (`tests/integration.rs`) drive the compiled binary via `assert_cmd`, using fixtures from `tests/common/mod.rs` and `tests/fixtures/`.
