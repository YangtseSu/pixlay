#!/usr/bin/env python3
"""Regenerates the fixture photos and the fixture projects.

Committed alongside its output so the fixtures are reproducible: the repo must
stay offline-buildable, and a fixture whose provenance is unknown is a fixture
nobody can review. Run from the repository root:

    python3 crates/pixlay-cli/tests/fixtures/generate.py

The content is deterministic (no randomness, no timestamps) and deliberately not
flat, so a decode or resample bug cannot hide behind uniform pixels. All the
images are generated here, so their licence is the repository's: CC0.

Three of the files are produced by an external tool and only *committed* here —
the build and the tests never call it, so the repository stays offline-buildable:

  * `photo.heic` — `heif-enc`, because Pillow cannot write HEIC. S4's exit
    criterion "HEIC decodes" needs a real HEIC file to decode.
  * `adobe-rgb.jpg` — the 4:3 fixture with the colour profile of
    `/usr/share/color/icc/colord/AdobeRGB1998.icc` attached, which is a file
    whose *numbers* are Adobe RGB. S4's colour decision (the source profile is
    honoured) is tested against `adobe-rgb-srgb.png`, the same file converted to
    sRGB by `magick -profile sRGB.icc`, i.e. by an implementation that is not
    ours.
  * `resample-lanczos-200.png` — `magick -filter Lanczos -resize 200x150`, the
    independent implementation S4's resampling is measured against.
"""

import pathlib
import subprocess

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).parent / "photos"
ADOBE_RGB = pathlib.Path("/usr/share/color/icc/colord/AdobeRGB1998.icc")
SRGB = pathlib.Path("/usr/share/color/icc/colord/sRGB.icc")


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

    # A photo with a date, for the `{date}` text field (S5 reads it through
    # `pixlay_imaging::exif`). EXIF stores it as `YYYY:MM:DD HH:MM:SS` and the
    # contract says it is used verbatim, with no timezone conversion, so the
    # letters here are exactly what a watermark should show.
    dated = bands(960, 540, 5)
    exif = Image.Exif()
    exif[0x9003] = "2019:07:14 10:32:00"
    dated.save(ROOT / "dated.jpg", quality=88, subsampling=0, exif=exif)
    print("dated.jpg: EXIF DateTimeOriginal=2019:07:14 10:32:00")

    # Smooth, multi-frequency content at a size large enough that a 4x reduction
    # is a *large-ratio* resample: low-frequency gradients for the overall shape,
    # a mid-frequency modulation and a small hard-edged square. Nothing here can
    # be reproduced by a nearest-neighbour or a bilinear filter.
    smooth = smooth_source(1600, 1200)
    save(smooth, "resample-source.png")
    lanczos(smooth, (200, 150), "resample-lanczos-200.png")

    # A file whose numbers are Adobe RGB. Attaching the profile does not change a
    # pixel; it changes what the numbers mean, which is exactly what a decoder
    # that honours the source profile has to notice.
    if not ADOBE_RGB.is_file():
        raise SystemExit(f"missing {ADOBE_RGB}; install colord's ICC profiles")
    tagged = bands(800, 600, 2)
    tagged.save(
        ROOT / "adobe-rgb.jpg",
        quality=95,
        subsampling=0,
        icc_profile=ADOBE_RGB.read_bytes(),
    )
    print("adobe-rgb.jpg: 800x600 tagged Adobe RGB (1998)")
    profile("adobe-rgb.jpg", SRGB, "adobe-rgb-srgb.png")

    # A genuinely 16-bit file, which is what pins "the depth is the file's, not a
    # fixed choice": Pillow cannot write one, so ImageMagick does.
    depth16(bands(800, 600, 2), "photo-16bit.png")

    # A grayscale photo: the decoder has to widen it to the pipeline's RGB samples
    # with the same value in all three channels, and a source with only one
    # channel is a case every "does it decode" test would otherwise miss.
    gray(bands(800, 600, 7), "photo-gray.png")

    # HEIC: the format S4's backend decision was made on, and a 12-bit one, so the
    # loader has to hand over more than 8 bits per channel. 800x600 keeps the
    # committed file small; the point is the container and the depth, not size.
    heic(ROOT / "photo-16bit.png", ROOT / "photo.heic")


def smooth_source(width: int, height: int) -> Image.Image:
    """Low-frequency gradient plus mid-frequency waves: the content a resampler
    with too narrow a kernel aliases, and one with a wide enough kernel
    reproduces."""
    import math

    image = Image.new("RGB", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            u = x / width
            v = y / height
            wave = (
                math.sin(2 * math.pi * (6 * u + 2 * v))
                + math.sin(2 * math.pi * (13 * u - 5 * v))
                + math.sin(2 * math.pi * (29 * u + 11 * v))
            ) / 3
            ramp = 0.5 + 0.45 * (u - v)
            level = int(255 * min(1.0, max(0.0, ramp * (0.75 + 0.25 * wave))))
            pixels[x, y] = (level, int(255 * (0.5 + 0.4 * wave)) % 256, 255 - level)
    draw = ImageDraw.Draw(image)
    draw.rectangle(
        [width // 8, height // 2, width // 4, height * 3 // 4], fill=(250, 250, 250)
    )
    return image


def lanczos(image: Image.Image, size: tuple[int, int], name: str) -> None:
    """Reduces `image` with ImageMagick's Lanczos filter, the reference S4's
    resampler is compared against. `-filter Lanczos` widens the kernel with the
    shrink ratio, which is the property the comparison is about."""
    image.save(ROOT / "_resample-input.png")
    subprocess.run(
        [
            "magick",
            str(ROOT / "_resample-input.png"),
            "-filter",
            "Lanczos",
            "-resize",
            f"{size[0]}x{size[1]}!",
            str(ROOT / name),
        ],
        check=True,
    )
    (ROOT / "_resample-input.png").unlink()
    print(f"{name}: {size[0]}x{size[1]} by ImageMagick Lanczos")


def profile(source: str, to: pathlib.Path, name: str) -> None:
    """Converts `source` (which carries its own profile) to `to` with ImageMagick:
    the independent implementation of the colour conversion S4 relies on."""
    subprocess.run(
        ["magick", str(ROOT / source), "-profile", str(to), str(ROOT / name)],
        check=True,
    )
    print(f"{name}: converted to sRGB by ImageMagick")


def depth16(image: Image.Image, name: str) -> None:
    """Writes `image` as a 16-bit PNG with ImageMagick: Pillow's `I;16` mode
    produced a file whose green and blue channels were empty, which both
    ImageMagick and the decoder read as a red-only image."""
    image.save(ROOT / "_depth16-input.png")
    subprocess.run(
        [
            "magick",
            str(ROOT / "_depth16-input.png"),
            # Without both defines ImageMagick optimizes the output back to 8 bits
            # (measured: `-depth 16` alone still wrote an 8-bit PNG).
            "-define",
            "png:bit-depth=16",
            "-define",
            "png:color-type=2",
            "-depth",
            "16",
            str(ROOT / name),
        ],
        check=True,
    )
    (ROOT / "_depth16-input.png").unlink()
    print(f"{name}: {image.size[0]}x{image.size[1]} at 16 bits by ImageMagick")


def gray(image: Image.Image, name: str) -> None:
    """Writes `image` as an 8-bit grayscale PNG with ImageMagick."""
    image.save(ROOT / "_gray-input.png")
    subprocess.run(
        [
            "magick",
            str(ROOT / "_gray-input.png"),
            "-colorspace",
            "gray",
            str(ROOT / name),
        ],
        check=True,
    )
    (ROOT / "_gray-input.png").unlink()
    print(f"{name}: {image.size[0]}x{image.size[1]} grayscale")


def heic(source: pathlib.Path, target: pathlib.Path) -> None:
    """Encodes `source` as a 12-bit HEIC with `heif-enc` (Pillow cannot write
    HEIC, and its default depth is 8)."""
    subprocess.run(
        ["heif-enc", "-b", "12", "-q", "70", "-o", str(target), str(source)],
        check=True,
    )
    print(f"{target.name}: {Image.open(source).size} 12-bit HEIC by heif-enc")


if __name__ == "__main__":
    main()
