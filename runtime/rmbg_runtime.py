from __future__ import annotations

import argparse
import json
import os
import sys
import traceback
import warnings
from pathlib import Path
from typing import Callable, Protocol

import torch
from huggingface_hub import get_token
from huggingface_hub.constants import HF_HUB_CACHE
from huggingface_hub.errors import GatedRepoError
from PIL import Image, ImageChops, ImageOps
from torchvision import transforms
from transformers import AutoModelForImageSegmentation

MODEL_ID = "briaai/RMBG-2.0"
# Pin the trusted remote model code and weights to the authenticated revision
# verified by this project. The environment override is for deliberate updates.
MODEL_REVISION = os.environ.get(
    "RMBG_MODEL_REVISION", "5df4c9c76d8170882c34f6986e848ee07fd0ba43"
)
IMAGE_SIZE = (1024, 1024)

# RMBG-2.0 currently imports compatibility aliases from timm. Keep ordinary
# CLI output clean while still allowing other warnings through.
warnings.filterwarnings(
    "ignore",
    message="Importing from timm.models.* is deprecated.*",
    category=FutureWarning,
)


class SegmentationModel(Protocol):
    def __call__(self, image: torch.Tensor) -> object: ...


def select_device(requested: str) -> torch.device:
    if requested == "auto":
        if torch.cuda.is_available():
            return torch.device("cuda")
        if torch.backends.mps.is_available():
            return torch.device("mps")
        return torch.device("cpu")

    if requested == "cuda" and not torch.cuda.is_available():
        raise RuntimeError("CUDA was requested but is not available")
    if requested == "mps" and not torch.backends.mps.is_available():
        raise RuntimeError("MPS was requested but is not available")
    return torch.device(requested)


def load_model(
    device: torch.device, *, local_files_only: bool = False
) -> SegmentationModel:
    model = AutoModelForImageSegmentation.from_pretrained(
        MODEL_ID,
        revision=MODEL_REVISION,
        trust_remote_code=True,
        local_files_only=local_files_only,
    )
    return model.eval().to(device)


def preprocess(image: Image.Image, device: torch.device) -> torch.Tensor:
    transform = transforms.Compose(
        [
            transforms.Resize(IMAGE_SIZE),
            transforms.ToTensor(),
            transforms.Normalize(
                [0.485, 0.456, 0.406],
                [0.229, 0.224, 0.225],
            ),
        ]
    )
    return transform(image.convert("RGB")).unsqueeze(0).to(device)


def predict_mask(model: SegmentationModel, input_image: torch.Tensor) -> torch.Tensor:
    with torch.inference_mode():
        outputs = model(input_image)
        return outputs[-1].sigmoid().cpu()[0].squeeze()  # type: ignore[index]


def process_image(
    model: SegmentationModel,
    input_path: Path,
    output_path: Path,
    device: torch.device,
    background: tuple[int, int, int] | None,
    progress: Callable[[int, str, str], None] | None = None,
) -> None:
    with Image.open(input_path) as opened:
        original = ImageOps.exif_transpose(opened).convert("RGBA")

    input_image = preprocess(original, device)
    if progress is not None:
        progress(3, "image_preprocessed", "Image decoded and preprocessed")
    prediction = predict_mask(model, input_image)
    if progress is not None:
        progress(4, "inference_completed", "Inference completed")
    predicted_alpha = transforms.ToPILImage()(prediction).resize(
        original.size, Image.Resampling.LANCZOS
    )
    combined_alpha = ImageChops.multiply(original.getchannel("A"), predicted_alpha)
    foreground = original.copy()
    foreground.putalpha(combined_alpha)

    if background is None:
        result = foreground
    else:
        canvas = Image.new("RGBA", foreground.size, (*background, 255))
        result = Image.alpha_composite(canvas, foreground)

    result.save(output_path, format="PNG")


def parse_background(value: str) -> tuple[int, int, int]:
    try:
        values = tuple(int(part) for part in value.split(","))
    except ValueError as error:
        raise argparse.ArgumentTypeError("background must be R,G,B") from error
    if len(values) != 3 or any(component < 0 or component > 255 for component in values):
        raise argparse.ArgumentTypeError("background must contain three values from 0 to 255")
    return values  # type: ignore[return-value]


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--setup", action="store_true")
    parser.add_argument("--doctor-json", action="store_true")
    parser.add_argument("--deep", action="store_true")
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--device", choices=("auto", "cuda", "mps", "cpu"), default="auto")
    parser.add_argument("--background", type=parse_background)
    parser.add_argument("--verbose", action="store_true")
    return parser.parse_args(argv)


def emit_progress(
    completed: int,
    stage: str,
    label: str,
    device: torch.device | None = None,
) -> None:
    event: dict[str, object] = {
        "completed": completed,
        "total": 5,
        "stage": stage,
        "label": label,
    }
    if device is not None:
        event["device"] = device.type
    print(f"::rmbg-progress::{json.dumps(event, separators=(',', ':'))}", file=sys.stderr, flush=True)


def cached_model_snapshot() -> Path | None:
    repository = "models--" + MODEL_ID.replace("/", "--")
    snapshot = Path(HF_HUB_CACHE) / repository / "snapshots" / MODEL_REVISION
    if not (snapshot / "config.json").is_file():
        return None
    weight_names = ("model.safetensors", "pytorch_model.bin")
    if not any((snapshot / name).is_file() for name in weight_names):
        return None
    return snapshot


def doctor_result(deep: bool) -> dict[str, object]:
    cached = cached_model_snapshot()
    cuda = torch.cuda.is_available()
    mps = torch.backends.mps.is_available()
    selected = "cuda" if cuda else "mps" if mps else "cpu"
    deep_status = "skipped"
    deep_detail = "deep model validation was not requested"
    if deep:
        if cached is None:
            deep_status = "error"
            deep_detail = "the exact pinned model revision is not fully cached"
        else:
            try:
                load_model(torch.device(selected), local_files_only=True)
                deep_status = "ok"
                deep_detail = f"cached model loaded successfully on {selected}"
            except Exception as error:
                deep_status = "error"
                deep_detail = f"cached model load failed: {error}"
    return {
        "authenticated": get_token() is not None,
        "model_cached": cached is not None,
        "cache_detail": (
            f"pinned revision cached at {cached}"
            if cached is not None
            else "the exact pinned model revision is not fully cached"
        ),
        "cuda": cuda,
        "mps": mps,
        "cpu": True,
        "selected_device": selected,
        "deep_status": deep_status,
        "deep_detail": deep_detail,
    }


def is_gated_error(error: BaseException) -> bool:
    current: BaseException | None = error
    seen: set[int] = set()
    while current is not None and id(current) not in seen:
        if isinstance(current, GatedRepoError):
            return True
        message = str(current).lower()
        if "gated repo" in message or "restricted model" in message:
            return True
        seen.add(id(current))
        current = current.__cause__ or current.__context__
    return False


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.doctor_json:
        try:
            print(json.dumps(doctor_result(args.deep), separators=(",", ":")))
            return 0
        except Exception as error:
            print(f"error: {error}", file=sys.stderr)
            return 2
    try:
        device = select_device(args.device)
        if args.verbose:
            print(f"runtime device: {device}", file=sys.stderr)
            print(f"model revision: {MODEL_REVISION}", file=sys.stderr)
        if not args.setup:
            emit_progress(1, "device_selected", "Device selected", device)
        model = load_model(device)
        if args.setup:
            print(json.dumps({"device": device.type}, separators=(",", ":")))
            return 0
        emit_progress(2, "model_loaded", "Model loaded")
        if args.input is None or args.output is None:
            raise ValueError("--input and --output are required for image processing")
        process_image(
            model,
            args.input,
            args.output,
            device,
            args.background,
            emit_progress,
        )
        return 0
    except Exception as error:
        print(f"error: {error}", file=sys.stderr)
        if args.verbose:
            traceback.print_exc()
        if args.setup and is_gated_error(error):
            return 3
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
