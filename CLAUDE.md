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
3. **`detector.rs`** — given the loaded image, determines `CheckerboardParams` (the two checker colors, tile size in px, and the color at the image origin):
   - Color detection samples the four image corners and clusters pixel colors above a brightness threshold, picking the two most distinct clusters.
   - Tile size detection scans for a color transition along the top edge, then scores candidate tile sizes (4–32px, plus the detected transition) by how well they predict pixel colors against the expected alternating-checker pattern (`expected_color_for_cell`). Best score must clear a 0.55 threshold or detection fails.
   - Either color or tile size can be overridden via CLI flags, in which case detection is skipped for that piece.
4. **`processor.rs`** — given `CheckerboardParams`, builds a per-pixel mask of checker cells (color match AND grid-position match), then refines it with a **shell-overlap pass**: it dilates the color-A and color-B masks by 1px to get their boundary "shells," ANDs the shells together (this finds the checkerboard's interior seams where anti-aliasing blurs the two colors), dilates that overlap by 8px, and re-tests pixels in the expanded region against the checker colors. This catches anti-aliased grid pixels that the strict grid-position check would otherwise miss without bleeding into foreground content. Finally applies the mask as either alpha=0 (transparent) or a solid RGB fill.

`main.rs` wires the four stages together and maps domain errors to exit codes: `1` for user errors (bad args, missing file), `2` for processing errors (`DetectError`, `IoError`, or zero masked pixels).

### Key invariant

Detection and removal must agree on the checker pattern's phase. `origin_color` (the pixel at (0,0)) anchors `expected_color_for_cell`'s parity calculation — it's computed once in `detect_checkerboard` and threaded through to `processor::remove_checkerboard` so both stages mask the same logical grid cells.

## Testing conventions

- Unit tests live inline in each module (`#[cfg(test)] mod tests`), generally building a synthetic checkerboard via `expected_color_for_cell` and asserting on detection/masking output.
- Integration tests (`tests/integration.rs`) drive the compiled binary via `assert_cmd`, using fixtures from `tests/common/mod.rs` and `tests/fixtures/`.
