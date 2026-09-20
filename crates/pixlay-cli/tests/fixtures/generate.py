#!/usr/bin/env python3
"""Regenerates the S1 fixture photos.

Committed alongside its output so the fixtures are reproducible: the repo must
stay offline-buildable, and a fixture whose provenance is unknown is a fixture
nobody can review. Run from the repository root:

    python3 crates/pixlay-cli/tests/fixtures/generate.py

The content is deterministic (no randomness, no timestamps) and deliberately not
flat, so a decode or resample bug cannot hide behind uniform pixels. All the
images are generated here, so their licence is the repository's: CC0.
"""

import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).parent / "photos"


def bands(width: int, height: int, phase: int) -> Image.Image:
    """Diagonal bands plus a hard-edged square: low frequency, but nothing for a
    resampler to hide behind."""
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            u = x / width
            v = y / height
            level = int(255 * (0.5 + 0.35 * ((u * 3 + v * 2 + phase) % 2 - 0.5)))
            pixels[x, y] = (level, (level * 2) % 256, 255 - level)
    draw = ImageDraw.Draw(image)
    draw.rectangle(
        [width // 4, height // 3, width // 2, height * 2 // 3],
        fill=(240, 240, 240),
    )
    return image


def save(image: Image.Image, name: str, **kwargs) -> None:
    image.save(ROOT / name, **kwargs)
    print(f"{name}: {image.size[0]}x{image.size[1]}")


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)

    # Vertical and horizontal compositions.
    save(bands(600, 900, 0), "portrait.jpg", quality=88, subsampling=0)
    save(bands(960, 540, 1), "landscape.jpg", quality=88, subsampling=0)
    # 4:3 and square.
    save(bands(800, 600, 2), "ratio-4-3.png")
    save(bands(640, 640, 3), "square.png")

    # A PNG with a real alpha channel: a soft disc over transparency, so the
    # compositing-onto-white rule has something to act on.
    size = (800, 600)
    alpha = Image.new("RGBA", size, (0, 0, 0, 0))
    pixels = alpha.load()
    centre = (size[0] / 2, size[1] / 2)
    radius = min(size) / 2
    for y in range(size[1]):
        for x in range(size[0]):
            distance = ((x - centre[0]) ** 2 + (y - centre[1]) ** 2) ** 0.5
            coverage = max(0.0, min(1.0, (radius - distance) / (radius * 0.25)))
            level = int(255 * (x / size[0]))
            pixels[x, y] = (level, 80, 255 - level, int(255 * coverage))
    alpha.save(ROOT / "alpha.png")
    print(f"alpha.png: {size[0]}x{size[1]} (alpha varies 0..255)")

    # EXIF Orientation=6 ("rotate 90 CW"): the stored pixels are 600x1200, a
    # viewer must show 1200x600. Written with piexif-free raw bytes so the file
    # has exactly one orientation tag and nothing else.
    oriented = bands(600, 1200, 4)
    exif = Image.Exif()
    exif[0x0112] = 6
    oriented.save(ROOT / "oriented-6.jpg", quality=88, subsampling=0, exif=exif)
    print("oriented-6.jpg: 600x1200 stored, EXIF Orientation=6")


if __name__ == "__main__":
    main()
