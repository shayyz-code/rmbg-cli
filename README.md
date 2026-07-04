# rmbg-cli

[![CI](https://github.com/shayyz-code/rmbg-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/shayyz-code/rmbg-cli/actions/workflows/ci.yml)
[![Release](https://github.com/shayyz-code/rmbg-cli/actions/workflows/release.yml/badge.svg)](https://github.com/shayyz-code/rmbg-cli/actions/workflows/release.yml)
[![npm](https://img.shields.io/npm/v/rmbg2-cli.svg)](https://www.npmjs.com/package/rmbg2-cli)
[![Code License: MIT](https://img.shields.io/badge/code-MIT-blue.svg)](LICENSE)
[![Model License: CC BY--NC 4.0](https://img.shields.io/badge/model-CC_BY--NC_4.0-orange.svg)](https://huggingface.co/briaai/RMBG-2.0)

`rmbg` removes image backgrounds locally with
[`briaai/RMBG-2.0`](https://huggingface.co/briaai/RMBG-2.0). A small Rust CLI
validates arguments and starts a locked, uv-managed Transformers runtime. No
hosted inference API is used.

> [!IMPORTANT]
> The RMBG-2.0 model weights are available for **non-commercial use only**
> under CC BY-NC 4.0. Commercial use requires a separate agreement with BRIA.
> The repository code remains MIT-licensed; that license does not extend to
> the model weights.

## Example

The example below was processed locally with:

```bash
rmbg marin.png -o marin-no-bg.png
```

| Before | After — transparent PNG |
|:------:|:-----------------------:|
| <img width="427" height="640" alt="marin" src="https://github.com/user-attachments/assets/a810e266-607f-43d5-b8e6-845bed9f1d67" /> | <img width="427" height="640" alt="marin-no-bg" src="https://github.com/user-attachments/assets/99711dfc-6ee7-4df0-bdb1-09e7b64b8d44" /> |

## System requirements

BRIA publishes RMBG-2.0 as a 0.2B-parameter model with FP32 weights and a
1024×1024 inference size, but does not publish minimum RAM or VRAM figures. The
values below are conservative project guidance based on that architecture, the
844 MB pinned weights, and the local PyTorch runtime. See the
[official model card](https://huggingface.co/briaai/RMBG-2.0) and
[BRIA repository](https://github.com/Bria-AI/RMBG-2.0).

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| Platform | Linux glibc x64/ARM64, Apple Silicon macOS, or Windows x64 | A current release of one of the supported operating systems |
| CPU | 2 cores; CPU-only inference is supported | 4 or more modern CPU cores |
| Memory | 8 GB RAM | 16 GB RAM |
| Free storage | 5 GB for dependencies, weights, and caches | 10 GB, especially for Linux CUDA packages |
| Acceleration | None; a GPU is optional | NVIDIA GPU with 6 GB VRAM, or Apple Silicon with 16 GB unified memory |
| Network | Required during initial setup | Broadband connection for the model and dependency download |
| Account | Hugging Face account with the RMBG-2.0 terms accepted | `HF_TOKEN` configured for non-interactive setup |
| Software | [uv](https://docs.astral.sh/uv/) and Python 3.10–3.12, managed by uv | Node.js 18+ for npm installation; Rust 1.75+ only when building from source |

The official implementation depends on PyTorch, Torchvision, Pillow, Kornia,
and Transformers. CUDA, Apple MPS, and CPU execution are selected automatically
by this CLI.

## Installation

Install the native CLI from npm:

```bash
npm install --global rmbg2-cli
rmbg setup
```

Node.js is only used by the npm launcher. The image processing runtime remains
local Python, Transformers, and PyTorch managed by uv.

To install without Node.js on Linux or macOS:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/shayyz-code/rmbg-cli/releases/latest/download/rmbg-installer.sh | sh
```

On Windows PowerShell:

```powershell
irm https://github.com/shayyz-code/rmbg-cli/releases/latest/download/rmbg-installer.ps1 | iex
```

Both direct installers verify the downloaded archive against the release
SHA-256 manifest, install into a user-local bin directory, and print PATH
guidance when needed. Set `RMBG_INSTALL_DIR` to choose another directory or
`RMBG_VERSION` to install a specific release.

Then prepare the local model runtime once:

```bash
rmbg setup
```

To build from source instead:

```bash
git clone https://github.com/shayyz-code/rmbg-cli.git
cd rmbg-cli
cargo build --release
./target/release/rmbg setup
```

Setup checks for [uv](https://docs.astral.sh/uv/getting-started/installation/),
prints the official installation command if it is missing, installs the locked
Python dependencies, starts Hugging Face login when needed, downloads the pinned
844 MB model, and validates that it loads on the selected device. If BRIA's
non-commercial terms have not been accepted, setup prints the model page and can
be rerun after access is granted. You can use `HF_TOKEN` instead of interactive
login.

Setup is idempotent and reuses installed dependencies and cached weights:

```bash
rmbg setup --device cpu
```

## Usage

Remove a background and write `<input>-no-bg.png`:

```bash
rmbg photo.jpg
rmbg photo.jpg -o cutout.png -v
```

Composite the foreground onto a solid color:

```bash
rmbg photo.jpg --background white -o on-white.png
rmbg photo.jpg --background "#336699" -o on-blue.png
rmbg photo.jpg --background 255,128,0 -o on-orange.png
```

Device selection defaults to CUDA, then Apple MPS, then CPU. Override it when
needed:

```bash
rmbg photo.jpg --device cpu
```

Interactive terminals show a purple-to-pink animated processing indicator and
the output path when processing completes. Redirected runs stay quiet unless
`--verbose` is used, so scripts do not receive cursor-control output.

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Output PNG path (default: `<input>-no-bg.png`) |
| `--background <COLOR>` | Solid background (`#RRGGBB`, `R,G,B`, `white`, `black`) |
| `--device <DEVICE>` | `auto`, `cuda`, `mps`, or `cpu` (default: `auto`) |
| `-v, --verbose` | Print model, device, revision, and output details |
| `--color <WHEN>` | `auto`, `always`, or `never` (default: `auto`) |
| `-h, --help` | Show help |

Color is enabled automatically for supported terminals and can also be
controlled with `NO_COLOR`, `CLICOLOR`, and `CLICOLOR_FORCE`. Animation is
restricted to interactive stderr even when color is forced.

`rmbg setup [--device auto|cuda|mps|cpu] [--color auto|always|never]` prepares
and validates all local runtime prerequisites. Because `setup` is reserved as a
command, process a file with that exact name as `rmbg ./setup`.

Exit code `1` indicates invalid input or a setup action the user must complete,
such as installing uv, authenticating non-interactively, or accepting model
terms. Exit code `2` indicates dependency, network, runtime, model-load,
inference, or output failure.

## How it works

The worker follows the RMBG-2.0 model card: it normalizes a 1024×1024 RGB copy,
runs local Transformers/PyTorch inference, resizes the predicted grayscale
matte to the original dimensions, and applies it as alpha. Existing alpha is
multiplied with the prediction so transparent source pixels are never restored.

The model requires `trust_remote_code=True`. The tested model revision is
[`5df4c9c76d8170882c34f6986e848ee07fd0ba43`](https://huggingface.co/briaai/RMBG-2.0/tree/5df4c9c76d8170882c34f6986e848ee07fd0ba43),
reported by `rmbg -v`. It can be overridden for deliberate maintenance with
`RMBG_MODEL_REVISION`.

## Development

```bash
uv sync --project runtime --frozen
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
npm run check:versions
npm test
uv run --project runtime --frozen python runtime/tests/test_runtime.py
```

Ordinary tests use a fake segmentation model and do not download weights.

## License

Repository code is MIT-licensed. RMBG-2.0 model weights are separately licensed
for non-commercial use under CC BY-NC 4.0. See [LICENSE](LICENSE) and the
[official model card](https://huggingface.co/briaai/RMBG-2.0).
