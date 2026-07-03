from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import torch
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import rmbg_runtime


class FakeModel:
    def __init__(self, logits: torch.Tensor) -> None:
        self.logits = logits

    def __call__(self, _image: torch.Tensor) -> list[torch.Tensor]:
        return [self.logits]


class RuntimeTests(unittest.TestCase):
    def test_auto_device_falls_back_to_cpu(self) -> None:
        with (
            mock.patch.object(torch.cuda, "is_available", return_value=False),
            mock.patch.object(torch.backends.mps, "is_available", return_value=False),
        ):
            self.assertEqual(rmbg_runtime.select_device("auto").type, "cpu")

    def test_unavailable_explicit_device_fails(self) -> None:
        with mock.patch.object(torch.cuda, "is_available", return_value=False):
            with self.assertRaisesRegex(RuntimeError, "CUDA"):
                rmbg_runtime.select_device("cuda")

    def test_transparent_output_combines_existing_alpha(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.png"
            output = Path(directory) / "output.png"
            Image.new("RGBA", (2, 2), (200, 100, 50, 128)).save(source)
            logits = torch.zeros((1, 1, 2, 2))

            rmbg_runtime.process_image(
                FakeModel(logits), source, output, torch.device("cpu"), None
            )

            with Image.open(output) as result:
                self.assertEqual(result.mode, "RGBA")
                # Pillow quantizes a sigmoid value of 0.5 to 127 before the
                # 8-bit alpha multiplication: floor(128 * 127 / 255) == 63.
                self.assertEqual(result.getpixel((0, 0))[3], 63)

    def test_solid_background_is_opaque(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.png"
            output = Path(directory) / "output.png"
            Image.new("RGB", (2, 2), (255, 0, 0)).save(source)
            logits = torch.full((1, 1, 2, 2), -20.0)

            rmbg_runtime.process_image(
                FakeModel(logits),
                source,
                output,
                torch.device("cpu"),
                (0, 255, 0),
            )

            with Image.open(output) as result:
                self.assertEqual(result.getpixel((0, 0)), (0, 255, 0, 255))

    def test_background_parser_validates_range(self) -> None:
        self.assertEqual(rmbg_runtime.parse_background("1,2,3"), (1, 2, 3))
        with self.assertRaises(Exception):
            rmbg_runtime.parse_background("256,0,0")

    def test_setup_loads_model_without_running_inference(self) -> None:
        with (
            mock.patch.object(rmbg_runtime, "select_device", return_value=torch.device("cpu")),
            mock.patch.object(rmbg_runtime, "load_model") as load_model,
            mock.patch.object(rmbg_runtime, "process_image") as process_image,
        ):
            self.assertEqual(rmbg_runtime.main(["--setup", "--device", "cpu"]), 0)
            load_model.assert_called_once_with(torch.device("cpu"))
            process_image.assert_not_called()

    def test_setup_returns_distinct_code_for_gated_model(self) -> None:
        gated = rmbg_runtime.GatedRepoError("accept the model terms")
        with mock.patch.object(rmbg_runtime, "load_model", side_effect=gated):
            self.assertEqual(rmbg_runtime.main(["--setup", "--device", "cpu"]), 3)


if __name__ == "__main__":
    unittest.main()
