# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.5.0] - 2026-07-04

### Added

- Added purple and pink terminal styling, a Claude Code-inspired animated
  processing shimmer, and semantic setup, success, and error messages.
- Added `--color auto|always|never` with terminal and `NO_COLOR` detection.

### Changed

- Interactive removals now report their output path and elapsed time, while
  redirected runs remain quiet unless verbose output is requested.

## [0.4.1] - 2026-07-04

### Fixed

- Renamed the Windows native npm package to `rmbg2-cli-windows-x64` after npm
  incorrectly rejected the original platform package name as spam.
- Updated `rmbg2-cli` to install the renamed Windows package, restoring Windows
  npm installation.

## [0.4.0] - 2026-07-03

### Added

- Added `rmbg setup` to install the locked Python runtime, start interactive
  Hugging Face authentication when needed, download the pinned RMBG-2.0 model,
  and validate that it loads on the selected device.
- Added actionable setup guidance for missing uv, non-interactive
  authentication, and unaccepted BRIA model terms.
- Added npm distribution through `rmbg2-cli` and four platform-specific native
  packages for Linux glibc x64/ARM64, macOS ARM64, and Windows x64.
- Added checksummed curl and PowerShell installers backed by GitHub Releases.
- Added tag-driven native builds, package verification, build provenance, and
  tokenless npm trusted-publishing support.

### Changed

- Native executables now embed the locked Python runtime sources and
  materialize them into the user cache, so release archives no longer require
  a sibling `runtime/` directory.
- Cargo registry publishing is disabled; npm is the supported package registry.

## [0.3.0] - 2026-07-03

### Changed

- Renamed the project to `rmbg-cli` and the executable to `rmbg`.
- Replaced checkerboard color detection with local background-removal inference
  using `briaai/RMBG-2.0` through a locked uv/Transformers runtime.
- Changed the default output suffix from `-no-grid.png` to `-no-bg.png`.
- Release archives now include the Python runtime required by the Rust wrapper.

### Added

- Automatic CUDA, Apple MPS, or CPU device selection with `--device` override.
- Explicit non-commercial model-weight licensing and first-run authentication
  documentation.

### Removed

- Checkerboard-specific `--tolerance`, `--color-a`, and `--color-b` options.

## [0.2.0] - 2026-06-22

### Changed

- Replaced grid-parity masking with border-seeded flood fill. The previous
  approach detected an integer tile size and masked pixels by predicted grid
  parity; real exported checkerboards often have a non-integer, slightly
  drifting tile period, which desynced that prediction over large images and
  left bands of un-removed checker behind. Flood fill from the image border
  needs no tile size or grid phase, fixing this class of failure.
- Color detection now samples larger (64px) corner blocks so it reliably
  observes both checker shades even on faint, large-tile checkerboards,
  instead of a 5px window that could land entirely inside one tile.
- Default `--tolerance` raised from `10` to `12`.

### Removed

- `--tile-size` flag and all tile-size/grid-phase detection.

## [0.1.0] - 2026-06-22

### Added

- Initial release: CLI to remove transparency checkerboard grids from
  images, with auto-detected or manually-specified checker colors,
  transparent or solid-color output, and PNG/JPEG/WebP/BMP/GIF/TIFF input
  support.
