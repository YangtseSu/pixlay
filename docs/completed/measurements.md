## Measured baseline

### A0 one-off probe (2026-09-20, this machine)

One document review plus one one-off probe, not committed to the repository, run in `/var/tmp/pixlay-spike/`. The raw numbers are below.

Environment: cairo 1.18.4 · pixman 0.46.4 · gtk4 4.22.5 · libadwaita 1.9.4 · rustc 1.98.1 · 24 threads ·
15 GB RAM · `/tmp` tmpfs with 7.5 GB free · lcms2 2.19.1 · libheif 1.23.4 · libavif · libjxl ·
libtiff 4.7.2 · libjpeg-turbo 3.2.0 · glycin 2.1.5 (loaders `2+`: heif / image-rs / jxl / svg) · bwrap present.

A0 = 9933×14043 (139.5 MP) `ARGB32` surface, stride 39732:

| Item | Result |
|---|---|
| surface creation | 0.1 ms, 558 MB |
| fill white | 43 ms |
| 2-slot compositing (polygon clip + affine blit + hard fill) | 244 ms |
| 81 blits (photo content) | 699 ms |
| rotated CJK text (pangocairo, Noto Sans CJK) | 16 ms |
| `write_to_png` (flat content) | 1497 ms → 4.4 MB |
| `write_to_png` (photo content) | 6907 ms → 342 MB |
| → JPEG q90 4:4:4 (magick) | 5.5 s → 150 MB |
| → TIFF LZW (magick) | 4.8 s → 476 MB |
| full-canvas 16-bit RGBA intermediate buffer | 1064 MB resident, touched through in 120 ms |
| whole-run peak `VmHWM` | **543 MB** (including the 558 MB surface; the run with a text layer is 569 MB) |

### S0 re-measurement (2026-09-20, after the spike entered the repository, `--release`, the same machine)

The run above was a one-off probe (not committed); this is the formal criteria of the in-repo `a0-spike`. **The content model is different**:
photos are generated at **in-slot display size 1:1** (every pixel carries grain) rather than a small image blitted repeatedly — this is the input shape S4's buffer ladder
(`decode → 16-bit linear → downsample to in-slot display size → color grading → compositing`) will actually receive.

| Configuration | compositing ms | PNG ms / MB | JPEG ms / MB | `VmHWM` compositing / whole run MB |
|---|---|---|---|---|
| flat 2 slots | 189 | — | — | 941 / 941 |
| flat 10 slots | 550 | — | — | 941 / 941 |
| detail 2 slots | 185 | 34002 / 120.0 | 3315 / 38.3 | 941 / 1340 |
| detail 10 slots | 551 | 27773 / 125.6 | 3240 / 40.3 | 941 / 1340 |

Memory composition: output surface **558 MB** + photo buffers **402 MB** (the two slot counts coincidentally the same: at 2 slots it is two large slots,
at 10 slots it is one large slot plus nine small ones) = 960 MB, consistent with the measured 941 MB. **10 slots do not eat extra memory** — the peak is decided by
the output surface plus "the sum of photos at in-slot display size", independent of the slot count, and this is the conclusion S4 should copy.

Differences from the one-off probe above, both of which must be recorded:

1. **PNG is much slower: 34 s / 120 MB vs 6.9 s / 342 MB.** The difference is the content: A0 content with high per-pixel entropy makes zlib
   really do the full work over 558 MB (4 Mpx/s), whereas the probe's "photo content" was far more compressible (the larger output was actually faster).
   `cairo_surface_write_to_png` is **single-threaded zlib**, and S6 already gave it up for metadata reasons; now there is a performance reason as well.
2. **The peak is no longer 543 MB but close to 1 GB**, because this time the photo buffers are counted (see above). The 2.5 GB budget still has 1.6 GB of headroom.

The 1400×1979 previews (two of them) produced by `--preview-px` have been inspected: the irregular L-shaped clip is correct, the rotated slot is clipped,
uncovered areas show white, CJK glyphs are complete, and the seam has no bleeding.

### Seam (2026-09-20)

AGENTS's "Open / to be proven" entry, already measured: blended pixels on a shared edge between adjacent slots / seam length ≈ **1.08**
(the seam length is a geometric length estimate), and it is **exactly the same** at A0 and at 1/5 size → the blend width is 1 physical pixel, independent of the
output resolution, and there is no strong bleeding (the count of strongly blended pixels is 0). **Inference: what makes the seam visible is the preview (low resolution), not the export** — at 300dpi 1px ≈ 0.085 mm.
The criterion is written as "blended pixel count ≤ 2 × the seam length", measured once at each of the two sizes.

### glycin (2026-09-20)

On this machine `glycin-thumbnailer` fails for **all of** PNG / JPEG / HEIC / AVIF
(`Failed to load file/stream: Operation not supported`), the same with and without a session bus; the loader binaries and bwrap are both present.
The cause is not located, but it is enough to show that "glycin usable with zero configuration outside Flatpak" **does not yet hold** → S4's first item must be to prove it first,
and the CLI/tests need a path that does not depend on glycin.

---

