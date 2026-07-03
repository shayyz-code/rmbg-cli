from __future__ import annotations

import argparse
import os
import sys
import traceback
import warnings
from pathlib import Path
from typing import Protocol

import torch
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


def load_model(device: torch.device) -> SegmentationModel:
    model = AutoModelForImageSegmentation.from_pretrained(
        MODEL_ID,
        revision=MODEL_REVISION,
        trust_remote_code=True,
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


def predict_mask(
    model: SegmentationModel,
    image: Image.Image,
    device: torch.device,
) -> Image.Image:
    input_image = preprocess(image, device)
    with torch.inference_mode():
        outputs = model(input_image)
        prediction = outputs[-1].sigmoid().cpu()[0].squeeze()  # type: ignore[index]
    mask = transforms.ToPILImage()(prediction)
    return mask.resize(image.size, Image.Resampling.LANCZOS)


def process_image(
    model: SegmentationModel,
    input_path: Path,
    output_path: Path,
    device: torch.device,
    background: tuple[int, int, int] | None,
) -> None:
    with Image.open(input_path) as opened:
        original = ImageOps.exif_transpose(opened).convert("RGBA")

    predicted_alpha = predict_mask(model, original, device)
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
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--device", choices=("auto", "cuda", "mps", "cpu"), default="auto")
    parser.add_argument("--background", type=parse_background)
    parser.add_argument("--verbose", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        device = select_device(args.device)
        if args.verbose:
            print(f"runtime device: {device}", file=sys.stderr)
            print(f"model revision: {MODEL_REVISION}", file=sys.stderr)
        model = load_model(device)
        process_image(model, args.input, args.output, device, args.background)
        return 0
    except Exception as error:
        print(f"error: {error}", file=sys.stderr)
        if args.verbose:
            traceback.print_exc()
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
