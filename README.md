# rmbg-cli

[![CI](https://github.com/shayyz-code/rmbg-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/shayyz-code/rmbg-cli/actions/workflows/ci.yml)
[![Release](https://github.com/shayyz-code/rmbg-cli/actions/workflows/release.yml/badge.svg)](https://github.com/shayyz-code/rmbg-cli/actions/workflows/release.yml)
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

## Requirements and installation

- [uv](https://docs.astral.sh/uv/getting-started/installation/)
- A Hugging Face account that has accepted the RMBG-2.0 access conditions
- Rust 1.75+ when building from source

Download a release archive and keep its `rmbg` executable and `runtime/`
directory together. Alternatively, build from source:

```bash
git clone https://github.com/shayyz-code/rmbg-cli.git
cd rmbg-cli
uv sync --project runtime --frozen
cargo build --release
```

Authenticate before the first inference:

```bash
uv run --project runtime hf auth login
```

You can instead provide `HF_TOKEN` in the environment. The first run downloads
the gated model and its trusted remote code into the Hugging Face cache;
subsequent runs use the local cache.

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

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Output PNG path (default: `<input>-no-bg.png`) |
| `--background <COLOR>` | Solid background (`#RRGGBB`, `R,G,B`, `white`, `black`) |
| `--device <DEVICE>` | `auto`, `cuda`, `mps`, or `cpu` (default: `auto`) |
| `-v, --verbose` | Print model, device, revision, and output details |
| `-h, --help` | Show help |

Exit code `1` indicates invalid user input. Exit code `2` indicates a missing
runtime, uv failure, model-access problem, inference failure, or output error.

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
uv run --project runtime --frozen python runtime/tests/test_runtime.py
```

Ordinary tests use a fake segmentation model and do not download weights.

## License

Repository code is MIT-licensed. RMBG-2.0 model weights are separately licensed
for non-commercial use under CC BY-NC 4.0. See [LICENSE](LICENSE) and the
[official model card](https://huggingface.co/briaai/RMBG-2.0).
