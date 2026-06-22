# rmtg

[![CI](https://github.com/shayyz-code/rmtg/actions/workflows/ci.yml/badge.svg)](https://github.com/shayyz-code/rmtg/actions/workflows/ci.yml)
[![Release](https://github.com/shayyz-code/rmtg/actions/workflows/release.yml/badge.svg)](https://github.com/shayyz-code/rmtg/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

**rmtg** removes the gray-and-white transparency checkerboard baked into exported images (common in Photoshop, Illustrator, and screenshot tools) and produces a clean PNG with real transparency or a solid background.

## Install

Download a prebuilt binary for your platform from the [GitHub Releases](https://github.com/shayyz-code/rmtg/releases) page, or build from source:

```bash
git clone https://github.com/shayyz-code/rmtg.git
cd rmtg
cargo install --path .
```

## Usage

Remove the checkerboard and write a transparent PNG (default output: `<input>-no-grid.png`):

```bash
rmtg photo.png
rmtg photo.png -o clean.png -v
```

Replace the grid with a solid background color:

```bash
rmtg photo.png --background white -o on-white.png
rmtg photo.png --background "#336699" -o on-blue.png
rmtg photo.png --background 255,128,0 -o on-orange.png
```

Override detection when auto-detect struggles:

```bash
rmtg photo.png --tile-size 16 --tolerance 12
rmtg photo.png --color-a "#FFFFFF" --color-b "#CCCCCC"
```

### Options

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Output path (default: `<input>-no-grid.png`) |
| `--background <COLOR>` | Replace grid with solid color (`#RRGGBB`, `R,G,B`, `white`, `black`) |
| `--tolerance <N>` | Color match tolerance (default: `10`) |
| `--tile-size <N>` | Force checker tile size in pixels |
| `--color-a`, `--color-b` | Override detected checker colors |
| `-v, --verbose` | Print detected parameters and masked pixel count |
| `-h, --help` | Show help |

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | User error (missing file, invalid arguments) |
| `2` | Processing error (no checkerboard detected, unsupported format) |

## How it works

1. **Color detection** — samples image corners to find the two dominant light checker colors.
2. **Tile size detection** — scores candidate square sizes (4–32 px) against grid periodicity.
3. **Grid-aware masking** — marks pixels that match checker colors *and* fall on the detected grid; refines edges with a shell-overlap pass for anti-aliased boundaries.
4. **Output** — sets masked pixels to transparent (default) or a user-chosen solid color.

## Limitations

- Works best when the checkerboard is the background and foreground content does not contain large uniform regions in the same gray/white tones.
- JPEG input is supported, but output is always PNG (transparency requires it).
- Unusual checker colors may need manual `--color-a` / `--color-b` overrides.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

MIT — see [LICENSE](LICENSE).
