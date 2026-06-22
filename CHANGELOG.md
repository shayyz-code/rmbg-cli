# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

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
