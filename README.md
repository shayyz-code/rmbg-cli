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
rmtg photo.png --tolerance 20
rmtg photo.png --color-a "#FFFFFF" --color-b "#CCCCCC"
```

### Options

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Output path (default: `<input>-no-grid.png`) |
| `--background <COLOR>` | Replace grid with solid color (`#RRGGBB`, `R,G,B`, `white`, `black`) |
| `--tolerance <N>` | Color match tolerance for the flood fill (default: `12`) |
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

1. **Color detection** — samples image corners to find the two dominant checker colors.
2. **Border flood fill** — starting from every edge pixel that matches either checker color, grows into 4-connected neighbors that also match. This needs no assumption about tile size or grid alignment, which makes it robust to the non-integer, slightly drifting tile periods real exported checkerboards often have.
3. **Output** — sets the filled (background) pixels to transparent (default) or a user-chosen solid color. Foreground content is preserved because it isn't reachable from the border unless it's both checker-colored *and* connected all the way to an edge.

## Limitations

- Works best when the checkerboard is the background and touches the image border. Checkerboard that is fully enclosed by foreground (e.g. visible only through a hole in the artwork) is not connected to the border and won't be removed.
- Foreground content that is itself checker-colored *and* touches the image border can get removed along with the background, since connectivity (not just color) is what's used to tell them apart.
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
