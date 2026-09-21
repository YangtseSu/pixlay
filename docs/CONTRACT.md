# S1 contract v1

Frozen on 2026-09-20. This file is the reading copy for the **contract review**: every shape, every limit and every non-goal that was reviewed is in here.
The implementation is authoritative, and this file is its guide; when the two disagree the tests win (the tests are in
`crates/pixlay-core/tests/`, `crates/pixlay-render/tests/`, `crates/pixlay-cli/tests/`).

Per the splitting principles in `docs/STEPS.md`, the contract must be **frozen at S1**, because all eight steps after it are built on top of it.

---

## 1. `CollageDoc`: the single serialization shape

```jsonc
{
  "docVersion": 1,                 // format version; a higher version is refused outright, never guessed at, never downgraded
  "canvas": { "widthMm": 280.0, "heightMm": 210.0 },   // 4:3, matching the template's aspect
  "template": {                    // geometry is data, not a reference: changing the template in the library does not touch a saved project
    "name": "mosaic-8-s14",
    "version": 1,                  // template geometry version
    "aspect": 1.3333333333333333,  // aspect ratio (width/height); the template matrix is grouped into families by it
    "slots": [                     // a cut template: the slots tile the canvas exactly, areas summing to 1.0
      { "outline": [[0,0],[0.5,0],[0.5,1],[0,1]], "area": 0.5 },
      { "outline": [[0.5,0],[1,0],[1,1],[0.5,1]], "area": 0.5 }
    ]
  },
  "cells": [                        // one cell per slot, the order is the slot index
    { "source": "photos/a.jpg",     // a path relative to the project file
      "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 },
      "grade": { "factor": 1.1, "saturation": 0.9, "delta": -0.1 } },   // per-slot colour (S4)
    { "source": null, "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 } }   // an empty slot renders white
  ],
  "filter": "warm",                 // the canvas-wide one-click filter (S4); "none" is the default
  "text": [                         // canvas-level text layers (S5: drawn by the same `draw`)
    { "content": "{date} #{index}", "mode": {"kind":"free","position":[0.5,0.9],"anchor":"bottomCenter"},
      "sizeRel": 0.02, "rotationDeg": 6, "color": {"r":0,"g":0,"b":0,"a":255}, "sourceSlot": 0 }
  ],
  "textFallback": { "date": "2026-09-20" }
}
```

> This example is a valid document, and since S5 nothing in it is refused by the renderer:
> the two slots give a cut template whose areas sum to exactly 1.0, the canvas is exactly
> 4:3 to match `template.aspect`, and the text layer draws — with `{date}` resolving to what
> the slot's photo says, or to `textFallback` when it says nothing (see "Text layers"
> below). Its photos are the project's own, as any project's are.

**Version policy** (S1 review ruling, 2026-09-20: **breaking changes allowed, but no migrations written**).

- **Adding a field does not bump the version**: every new field carries a `serde` default (or is an `Option`), and old projects load as usual.
  This is discipline: for every field added, ask once whether it can have a default; if it can, do not bump the version.
- **Only changing a meaning / removing a field bumps `DOC_VERSION`**: when that happens old projects are **refused outright**, and the error must say what can be done about it
  (`document version N predates the current format M ... rebuild the project with this version`).
- **No migration code is written**. The cost is known in advance: old projects are scrapped and the user rebuilds. The benefit is that the format will have no deprecated fields,
  and no path of the form "a migration has a bug → user data is lost on that path".
- Reading a **higher** version is also refused (`rebuild` does not apply; the error only says the version is too new).
- `DOC_VERSION_MIN` is kept as a constant rather than deleted so that "which version range this build reads" is readable in one place;
  it is currently always equal to `DOC_VERSION`.

Conventions:

- **Unknown fields are refused outright** (`deny_unknown_fields`). A mistyped key should not silently become a default.
- **Field names are camelCase**; a point is written as a two-element array `[x, y]` (`Point`) — a `.pixlay` is meant to be read by humans.
- All coordinates / sizes / font sizes are **normalized to `[0,1]`**; absolute pixels appear only after `CanvasSpec::pixel_size(dpi)`.
  Normalized font sizes exist so that UI scaling such as "large text" must not change the exported pixels (see `AGENTS.md` "Hard constraints").
- **Empty slot = `source: null`**, and that cell renders white. `source` is a path relative to the project file;
  an absolute path is accepted as it stands. A missing file → an explicit error, not a skip.
- `crop.zoom` is **absolute zoom** (displayed width / slot width), not "a multiple of fill":
  when the photo is swapped the baseline does not move and the framing does not jump focus.
- `rotationDeg` is capped at ±45°, and every component of `crop.offset` has |offset| ≤ 1 (past that no clamp can get the coverage back).
- **Direction convention**: both `rotationDeg` and a text layer's `rotationDeg` are positive **clockwise on screen** (the canvas y axis points down,
  cairo's `rotate` in that space is clockwise, and the renderer passes it through as-is).
- **A crop is a request; what gets drawn is its fit** (`CropTransform::fit`, S3). The canvas and the slot never grow, so the
  fit has exactly three levers: `zoom` is raised to the value that covers the slot with the photo centred (a larger request
  is kept as it is), `offset` is pulled back along the line to the slot centre until the photo covers again — a pan stops at
  the frame edge rather than being paid for with magnification — and `rotationDeg` is kept while the zoom it needs stays
  within `CLAMP_ZOOM_LIMIT` times the upright covering zoom, otherwise the widest angle that fits is used and
  `CropFit::rotation_limited` reports it. The fit is **idempotent**, so clamping on an edit and again in `draw` costs nothing
  and the second pass reports nothing. `draw` applies the fit, so no document this build accepts can render an uncovered
  slot; the fit's own boundary is a slot so extreme that covering it needs more than `MAX_ZOOM`, which gets the cap (and is
  what a decoder's memory budget, S4, limits from the other side).
- **A cell's colour is its own**: `grade` is three numbers applied in linear light
  (see §4), and `filter` is one preset name applied to every cell after its own
  grade. Both were added by S4 with `serde` defaults, so a project written before
  them loads unchanged — this is the "adding a field does not bump the version"
  rule in practice, and it is why `DOC_VERSION` is still 1.
- **Text layer order**: `text` is drawn in array order, later ones cover earlier ones, and all of them cover the cells.
- **Tiled phase**: the tile grid starts from the canvas origin `(0,0)`, and each tile rotates around its own anchor; there is no per-tile variation.

### Text layers (the shape S5 filled in)

`TextLayer` itself was frozen at S1; S5 fixed what its fields *mean*, and none of it
changed the document shape.

- **What a token resolves to.** `{date}` is EXIF `DateTimeOriginal` **verbatim, no
  timezone conversion**, read from the slot `sourceSlot` names; when that photo has no
  usable tag (or the layer names no slot at all) the document's `textFallback.date` is
  used, which is what makes an export reproducible. `{filename}` is that slot's source
  file name. `{index}` is the slot index `sourceSlot` names, **0-based** — `0` is the
  first slot in template order, the same number `template.<i>` rows and `probe` print.
  **A token with no value renders as nothing**: `{date}` never appears literally on a
  finished collage because a phone stripped the EXIF block. A layer that names no slot
  resolves `{date}` against the fallback and both other tokens to nothing.
- **Where the box goes.** A free layer's text is laid out within the **canvas width**
  (long text wraps there rather than running off the canvas), and the box that results is
  placed by `anchor`: `position` *is* that point of the box, in normalized canvas
  coordinates. `sizeRel` is the font size as a fraction of the **canvas height**, so a
  CJK glyph is exactly `sizeRel * canvas height` wide. The box is the layout's *logical*
  extents, so a line's leading is part of it and the text does not shift when a line is
  added. A tiled layer's tiles are **not** wrapped — a watermark is one mark per tile —
  and each tile's box top-left corner sits on its grid anchor.
- **The tile grid** is anchored at the canvas origin and has one anchor per `step` up to
  and including the far edge: `floor(1/step) + 1` per axis. The far-edge anchors are kept
  on purpose (their tiles are off-canvas unrotated, but a rotated watermark swings ink
  back over the sheet, and dropping them would leave a bare stripe). How many tiles that
  is, and the cap on it, are in the limit table below; `pixlay_core::tiled_grid` is the
  one function both the loader and the renderer ask.
- **Line breaking is Pango's**, which is the product's answer for CJK: it follows the
  Unicode line-breaking rules, so no line starts with `。`, `，`, `”` or `）` and none ends
  with `（` (kinsoku). Measured 2026-09-21: `他他他说。他` at a four-em width breaks as
  `他他他 / 说。他` — the breaker pulls the break back one character rather than starting a
  line with the mark, and `pixlay-render`'s tests pin both that case and the rule over a
  sweep of widths and paragraphs.
- **Punctuation squeezing is ours, through the font's `halt` feature.** Pango does *not*
  compress punctuation by itself (measured: `。，` costs two full ems, exactly like two
  isolated marks), and the rule is about a *run*, not about a character: in a run of
  consecutive CJK marks every mark but the last is drawn at half width (`。”` costs 1.5 em,
  a lone `。` still costs 1). A line break ends a run. The renderer asks for `halt` — the
  OpenType "alternate half widths" *positioning* feature — so how a compressed mark looks
  stays the font's decision, and a font without the feature simply does not compress.

## 2. Limit constants (all have explicit errors, no panics)

| Item | Value | Source |
|---|---|---|
| `docVersion` | exactly `DOC_VERSION` (currently 1); higher refused, lower refused too | see "Version policy" |
| slot count | 2..=10 | `AGENTS.md` |
| DPI | 72..=600 | `AGENTS.md` |
| canvas pixels | ≤ 200 MP | A0@300dpi = 139.5 MP, 43% of headroom left |
| canvas edge length | ≤ 2000 mm | larger than any output device |
| template aspect ratio | 0.1..=10.0 | a template outside this range is not a collage layout; it also bounds what a canvas can be matched to |
| slot outline | ≥ 3 vertices, finite, every vertex inside `[0,1]`, area > 0 | a polygon with no interior is not a slot |
| framing rotation | ±45° (clockwise is positive, canvas y points down) | `AGENTS.md` |
| framing zoom | `0 < zoom ≤ 1000` | the upper bound is necessary: zoom determines the size of the decoded bitmap, and without an upper bound it overflows. S4's decoder sets a limit **separately by memory budget**; the two layers each mind their own. The fit raises the drawn zoom to the covering value and never lowers a larger request |
| crop offset | every component \|offset\| ≤ 1 (slot widths / heights) | beyond half a slot the photo centre leaves the slot, and no clamp can cover it again. The fit reduces it further whenever the requested pan would uncover the slot |
| canvas vs template aspect ratio | difference ≤ 1e-6, otherwise a hard error | the two are each annotated independently, normalized coordinates carry no aspect ratio themselves; the GUI's template selector groups by aspect ratio and lists only the matching ones |
| text font size | 0 < `sizeRel` ≤ 1.0 (fraction of canvas height) | — |
| text position (free mode) | both components inside `[0,1]` | a free layer is placed in normalized canvas coordinates, so anything outside is off the canvas by definition |
| tiled step | both components > 0, finite | step 0 or a negative value makes the tiling loop forever; there is no upper bound |
| tiles per text layer | ≤ 10,000 (`TextLayer::MAX_TILES`) | a step is unbounded from above and therefore unbounded *downward*: `1e-9` is a billion by a billion tiles. A 1/100 step is already a 101x101 grid = 10,201 tiles and is refused when the document loads, so the cap is where a person's watermark stops being a watermark. `pixlay_core::tiled_grid` answers the count; the renderer asks the same function and never hangs on an in-memory document either |
| `--preview-px` | 1..=20000 (long edge, in pixels) | a preview larger than this cannot be reviewed by eye anyway |
| `--at` (`hit`) | both components inside 0..=1 | the canvas *is* `[0,1]`: normalized coordinates are what the document stores and what `probe` prints, so a point outside the canvas is a caller that mis-scaled something, not a hit test with an unusual answer |
| `--long-edge` (export size) | 1..=30000 (long edge, in pixels; `MAX_LONG_EDGE_PX` in `pixlay-core`) | a pixel count, not a resolution: A0 at the maximum DPI (600) is 28087 px on its long edge, so the range covers every resolution this product accepts. The **canvas pixel budget still applies to the grid it derives** (a square canvas at 20000 px is 400 MP and is refused, exit 2), so the flag's range and the budget are two different limits and both are checked |
| JPEG output resolution | ≤ 65535 dpi (`MAX_JPEG_DPI` in `pixlay-imaging`) | JFIF stores the density in 16 bits. A physical-size export is inside this range by construction (≤ 600 dpi); a pixel-count export on a very narrow canvas can derive one past it (`--long-edge 20000` on a 1 mm canvas is 508000 dpi) and is then **refused**, not saturated — a written number that is not the one the grid has is a lie the file cannot take back |
| grade `factor` | 0.2..=5.0 | ±2 stops of exposure around 1.0; beyond that the control only saturates every channel |
| grade `saturation` | 0.0..=4.0 | 0 is greyscale, 1 leaves the pixel alone |
| grade `delta` (`Δ`, warmth) | -1.0..=1.0 | `r *= 1 + delta`, `b *= 1 - delta`; past 1 the mapping is no longer monotone |
| decoded source | ≤ 120 MP and ≤ 20000 px per edge, 20 s | `MAX_DECODE_PIXELS` / `MAX_DECODE_EDGE` / `DECODE_TIMEOUT` in `pixlay-imaging`. A source is RGBA at its own depth, so 120 MP is 480 MB as 8-bit and 960 MB as 16-bit; the area cap is checked between the loader's header and its pixels, so a decompression bomb costs nothing |
| clamp degradation threshold | when the zoom the **requested rotation** needs exceeds `CLAMP_ZOOM_LIMIT` = **1.5 times the upright covering zoom**, the angle is reduced to the widest one that fits (`CLAMP_ZOOM_LIMIT` in `pixlay-core`) | `docs/STEPS.md`, "Open decisions → B. Confirmed". The reference is the upright floor, not an absolute zoom: a ten-column strip needs 6x upright for a 4:3 photo and rotating it needs *less*, so it is never degraded. Measured kept angles, matching photo and 45° asked (2026-09-21): 45° (unlimited) at 1:1, 34.0° at 6:5, 27.3° at 4:3, 22.6° at 3:2, 18.0° at 16:9, 11.2° at 8:3, mirrored for portrait slots |

Every entry above is enforced with a typed error, never a panic, and each is covered by
`crates/pixlay-core/tests/contract.rs` or `crates/pixlay-cli/tests/cli.rs`. An implementation
limit that is not in this table is a contract gap.

## 3. Templates

- `Slot::outline` is a **closed polygon** (the last point connects back to the first), with ≥ 3 vertices, finite, inside `[0,1]`, area > 0.
  Polygons only, no curves: S2's review offered "restrict the crop geometry to polygons, or declare a curve discretization tolerance",
  and polygons are what make area, overlap and holes decidable rather than approximate. A path is the outline's command list — **no SVG parser** is involved.
- `Slot::area` is the declared area and is cross-checked against the outline's actual area (tolerance 1e-6). The two are not allowed to drift.
- S2 owns the complete invariants (pairwise zero overlap, no interior hole in the union, cut-type areas summing to exactly 1.0);
  they live in `crates/pixlay-core/tests/templates.rs`.
- **The library is a generator plus committed data** (`pixlay_core::templates`): `templates/generator.rs` holds one recipe per
  template on an integer lattice, `templates/frozen.rs` is the committed geometry a build ships, and the module serves both.
  Regeneration is `cargo run -p pixlay-core --bin pixlay-gen-templates` (a **committed bin, not `build.rs`** — the frozen geometry is an
  interface, so rewriting it must be a reviewed commit); the determinism test runs that bin into a scratch path and compares the bytes.
- **Coordinates are dyadic**: every vertex is an integer multiple of `1/32` of a canvas edge, so every area is an exact binary value and
  "the areas sum to exactly 1.0" is an equality, not a tolerance. The coverage check in the tests depends on it too: `512` samples per axis
  is a multiple of `32` and the samples sit at cell centers, so no sample lands on an edge and none can miss a feature.
- **The matrix is grouped by aspect ratio**, because a canvas and a template only fit each other when their ratios agree (see the limit table).
  The families are `strip-<slots>-<cols>x<rows>` (one band), `grid-<slots>-<cols>x<rows>` (a rectangular tiling, `g` = with a gutter) and
  `mosaic-<slots>-<variant>` (mixed splits or a non-rectangular slot) — plus the frozen `mosaic-8-s14`, whose name, `version`, aspect,
  slot order and coordinates are unchanged by S2 (S1's hand-written geometry is now produced by the generator instead of written out).
- **Geometry version and document version are separate**: `template.version` follows the template family, `docVersion` follows the format.
- **The canvas aspect ratio must agree with what the template declares** (tolerance 1e-6): the geometry is normalized, and declaring a canvas with a different ratio silently
  stretches the template, while this error is invisible no matter which layer you look at it from.
- **The library covers every slot count from 2 to 10**, at least one template each; the CLI's `templates` reports the matrix and filters it by aspect ratio.

## 4. Rendering: `draw(doc, images, target)`

The single rendering implementation. Preview and export are the **same function**; the only difference is `scale` (and `band`).

```text
draw(&CollageDoc, &Images, &Target) -> Result<(), RenderError>

Target { ctx, scale, canvas_px, band }
Band  { index, count }            // horizontal bands: an A0 can render just one strip
Bitmap                            // ARgb32 premultiplied, already at display size
Images                            // slot → Bitmap; absent = that cell is left white
```

- **Cairo only blits and clips**: the bitmaps coming in are already decoded, downsampled, rotated and graded (`pixlay-imaging`, S4).
- **The crop is fitted before it is drawn** (`CropTransform::fit`, S3): the slot's outline, the aspect of the space being drawn
  into and the bitmap's aspect go in, and the transform that comes out is what is painted. A document may therefore store any
  contract-legal request and still render covered. S4's decoder sizes its bitmap from the same fit — the fit's zoom *is* the
  display size — because sizing from the stored request instead would leave the canvas resampling, which it must never do
  (measured in S3: a request 11.6x below the fitted zoom smeared one texel's transparent edge about 6 px into the slot).
- Composite onto an **opaque white background**; the output is never transparent.
- **Band rendering**: `Band::out_rows()` partitions on **output pixels** (`first = total * index / count`),
  so at any `scale` the band sizes sum to exactly the whole image. It previously partitioned by canvas rows, rounding each band on its own,
  and at 72dpi/scale=0.3 three bands totaled 759 rows while the whole image was 758 rows — `round` is not additive, and this could only be fixed this way.
  Measured, the whole image vs the three-band stitching has RMSE 0.033 (scale 1.0; see below), and scale 0.1/0.3/0.5 was measured too.
  **Banding is a genuinely usable memory-saving measure**: A0 landscape 10 slots @300dpi is 1470 MB for the whole image → 597 MB for 16 bands (see §8).
- **Text layers are drawn last** (S5): after the cells, in array order, positioned in canvas
  space, so reframing a photo cannot move a caption and a rotated slot cannot rotate one.
  The layout is Pango's and the drawing is cairo's; the font size is `sizeRel * canvas
  height` in *canvas* pixels, so a preview lays the text out identically to the export —
  the glyph raster is smaller, nothing else moves.

  The context's font options are set before any layout exists: `hint_style = none`,
  `hint_metrics = off`, `antialias = gray`. Hinted metrics snap glyph positions to device
  pixels, which lays out the same document differently at two scales, and subpixel
  filtering would put color fringes on an export. The family is the system's
  `sans-serif` — v1 has no font field (§6), so nothing here names a font and nothing fails
  because a particular font is missing; the tests pin the environment instead.

  What a slot's photo contributes (`{date}`, `{filename}`) arrives with the bitmaps:
  `Bitmap` → `Images` also carries [`TextValues`] per slot, filled by whoever decoded the
  file. A document rendered without a decoder therefore still draws its text, with
  `{date}` taking the document's own fallback.

### 4.1 The image pipeline (S4): what arrives at `draw`

`AGENTS.md`: "All resampling belongs upstream; the canvas only blits and clips."
The upstream is `pixlay-imaging`, and the frozen evaluation order is executed
there, in this order:

```text
decode (upright, sRGB, straight, at the file's own depth)
  → crop to the region the slot can show
  → resample in linear light (Lanczos3, kernel widened by the shrink ratio)
  → flatten onto the slot's opaque white base
  → per-slot grade (factor, then delta, then saturation)
  → the canvas-wide filter preset
  → 8-bit sRGB in Cairo's ARgb32 layout
```

**Colour.** A source carrying its own ICC profile is converted to sRGB by the
loader; a source without one is interpreted as sRGB. Output is always sRGB, and
no colour code is ours (no `lcms2`): the conversion is the loader's, measured
against ImageMagick's. Source alpha is preserved through the resample
(premultiplied in linear light, so a transparent neighbourhood cannot bleed into
an opaque pixel) and flattened onto white at the end, which is the same rule as
§4's "composite onto opaque white".

**The output carries the profile** (S6). Every export embeds an sRGB ICC profile,
because an sRGB file whose numbers are not labelled is a file whose colour depends
on who opens it. The bytes are built in `pixlay_imaging::icc` from the IEC
61966-2.1 colorimetry — the primaries and the D65 white point as chromaticities,
the piecewise transfer function, the Bradford adaptation into the D50 profile
connection space — rather than shipped as a blob, because v1 pulls in no colour
library to generate or validate one. The shape is ICC v4 (`mntr` / `RGB ` / `XYZ `,
`para` transfer curves, `chad`), the shape lcms2 writes, and it is deterministic:
the creation date and the profile id are zero, so the same document yields the
same bytes. Measured against the sRGB profile committed in a fixture (lcms2's, via
ImageMagick): the colorants agree to 2.2e-4, the curve parameters to one unit in
the last place, and converting an export from this profile to that one moves the
pixels by 0.0015/255 (§8, "S6").

**Depth.** The decoded buffer keeps the file's own depth (8 or 16 bits per
channel); everything after it is 16-bit — the resampler's output, the flattened
buffer, the graded buffer — and the only quantization is the final 8-bit write.
An 8-bit source is *widened* with `sample * 257`, which is exact, so 8-bit files
do not pay for a 16-bit buffer they cannot fill.

**The buffer ladder.** What exists at once, largest first:

| Buffer | Size | Lifetime |
|---|---|---|
| the decoded source | `src_px × 4` bytes (8-bit) or `× 8` (16-bit), capped at 120 MP | one slot |
| the resampler's row strip | `block_rows × dst_w × 16` bytes, block-bounded | one slot |
| one slot's bitmap | `dst_px × 4` bytes | until the render ends |

`Σ dst_px = O(output pixels)`: a bitmap holds the part of the photo the slot can
show, not the whole displayed photo. That is not an optimization but a
requirement — a slot in the ten-column strip needs its photo magnified 6x, so
handing over the whole displayed photo would allocate 3.33 GB of bitmaps for a
110.9 MP canvas **on top of** the 443 MB output surface, where the region crop
measures 1182 MB peak for the whole render (§8). `decoding is one source at a
time` follows from the same table: `N` concurrent slots need
`N × source + Σ bitmaps + output ≤ budget`.

**One exception, and why it stays: the framing rotation.** `AGENTS.md`'s sentence
also names "rotation interpolation", and the framing rotation (≤ ±45°, from
`CropTransform`) is still applied by `draw` itself, as S3 built it. The reason is
what the bitmap *is*: it already arrives at exactly the size it is displayed at, so
Cairo's affine is a rotation at 1:1, not the downscaling the constraint is about —
and S3 measured that the placement is what makes "a crop is a request; what is drawn
is its fit" true for every caller (28,800 framings, coverage exact to one ulp, and
360 renders with every sample ≥3 px inside a slot showing that slot's colour). Where
the constraint bites — decoding, downsampling, colour, grading, and *not* handing
Cairo a photo to shrink — is exactly where S4 put the work.

**The bitmap's region.** A bitmap may hold a sub-rectangle of the displayed
photo. It then carries where it sits (`Bitmap::origin`, in displayed-photo
pixels) and the whole displayed photo's size (`Bitmap::display_size`), because
the display scale cannot be recovered from a partial bitmap's own dimensions.
The rectangle comes from `CropTransform::display_region`, which inverts `draw`'s
own placement — the slot's outline vertices mapped into the displayed photo's
frame, plus a guard band (`REGION_GUARD_PX = 3` px) for Cairo's filter footprint
and the antialiased clip edge. The property this buys is pinned by a test:
rendering a document with the whole bitmap and with the region gives the same
pixels.
- `render_surface` / `render_rgb8` are just thin shells that allocate a surface + call `draw`; `rgb8` composites ARgb32 premultiplied uniformly
  onto a white background and gives the straight-through RGB the encoder wants. **`render_surface_sized` / `render_rgb8_sized` are the same shells
  with the canvas pixel grid passed in** (S6): a resolution is an export parameter, not a renderer concept, and the two export modes produce
  grids no single DPI reproduces (`CanvasSpec::pixel_size` rounds both edges from a DPI, `pixel_size_for_long_edge` makes one edge exact).
  The DPI-taking shells compute their grid and call the sized ones, so there is one surface allocator and one `draw`.

## 5. CLI: the machine operating surface

```text
pixlay-render render    --project <file.pixlay> --dpi <n> --out <file>
pixlay-render render    --project <file.pixlay> --long-edge <px> --out <file>
pixlay-render render    --project <file.pixlay> --dpi <n> --chroma 444|422|420 --out <file>
pixlay-render render    --template <name> --dpi <n> --out <file>   # no project, no photos
pixlay-render probe     --project <file.pixlay>
pixlay-render image     --photo <file>
pixlay-render text      --project <file.pixlay>
pixlay-render templates [--aspect <ratio>] [--json]
pixlay-render init      --template <name> --out <file.pixlay>
pixlay-render hit       --project <file.pixlay> --at <x>,<y> [--json]
pixlay-render hit       --template <name> --at <x>,<y> [--json]
pixlay-render save      --project <file.pixlay> --out <file.pixlay> [--json]
```

| Item | Contract |
|---|---|
| stdout | **only** machine-readable results (sorted `key = value`, or a single object with `--json`). Diagnostics all go to stderr |
| stability | same input, same output; the results carry no timestamps and no absolute paths. `--stats`'s `ms`/`encode_ms`/`peak_rss_mb` are the **only** exception (they are the measurement) |
| locale | under any value of `LANG` / `LC_ALL` / `LANGUAGE`, stdout and stderr are **byte-identical** (including the error branches) |
| interaction | does not read stdin, does not wait for a prompt, works with no TTY; `--help` covers every flag and every exit code |
| exit codes | 0 success / 1 usage error / 2 project, decode, render or write failure / 2 probe verdict not passed |
| usage error and "failed to produce a result" | stdout stays empty; stderr names the failing path (or the missing flag) |
| probe verdict not passed | **not "failed to produce a result"**: the numbers are the result, so stdout emits all the numbers as usual, with `status = failed` and `passed = false`, stderr emits a one-line summary, and the exit code is 2 |
| probe lower bound | when `occupied = 0` (all empty slots) the verdict is **failed**: every question the probe asks is about some slot, and with no slot there is no conclusion. Previously it "passed vacuously" (status=ok, exit 0) |
| output format | determined by the `--out` extension: `.png` / `.jpg` / `.jpeg` / `.tif` / `.tiff`, anything else is a usage error (exit 1, stdout empty, the message names the formats this build writes) |
| export resolution | `--dpi n` (72..=600) writes **exactly that resolution** into the file and sizes the grid `round(mm / 25.4 * dpi)`; `--long-edge n` (1..=30000) makes the long edge exactly n pixels, sizes the other edge `round(n * short_mm / long_mm)` (at least 1, half away from zero) and writes the resolution the grid works out to, `long_edge_px * 25.4 / long_edge_mm`. The two flags are mutually exclusive (exit 1), because they are two different requests; a resolution is echoed and a pixel count is derived, and neither is guessed from the other |
| per-format metadata (S6) | PNG: `pHYs` = `round(dpi * 1000 / 25.4)` pixels per metre, `iCCP` with the profile (the `sRGB` chunk is **not** written next to it — the specification says the two should not both appear, and the profile is the one carrying the colorimetry). JPEG: JFIF `APP0` density = `round(dpi)` pixels per inch, `APP2` `ICC_PROFILE` segments, and the sampling factors of the request. TIFF: `XResolution`/`YResolution` = `round(dpi * 100) / 100`, unit 2 (inch), tag 34675 (type `UNDEFINED`) for the profile, LZW with the horizontal predictor |
| `--chroma` | JPEG only (444 the default, 422, 420); given with a PNG or TIFF `--out` it is a usage error (exit 1), because those formats store three samples per pixel and accepting it would drop it silently. The request is visible in the file's own `SOF0`, which is what makes it checkable — re-encoding an export to patch metadata is what silently rewrites it (S0 measured 2.71 MB → 1.49 MB when 4:4:4 was re-encoded as 4:2:0) |
| `--preview-px n` | n pixels on the long edge; the same `draw`, only `scale` changes. The **bitmaps are sized for the preview too** (S4): decoding and resampling a full A0 and letting Cairo shrink it would cost the export's time and memory for a thumbnail, and would do the shrinking with Cairo's filter instead of the pipeline's |
| `render`'s report | carries `text` (how many text layers the document has) next to `cells` and `occupied`, so "the layers reached the renderer" is visible without reading pixels. In physical-size mode `dpi` is an **integer** — the resolution that was asked for and written; in pixel mode it is a **decimal**, the one the grid works out to, next to `long_edge`. A JPEG report carries `chroma` as well |
| `--stats` | appends `{ms, encode_ms, peak_rss_mb, icc}`; `render` emits all four, `probe` emits no `encode_ms` (it does not encode). `icc` is the description of the profile the written file carries (`sRGB IEC61966-2.1`); a command that writes no file reports `none`. The measurement rules are below |
| `probe` | samples and outputs numbers (in-slot photo color, out-of-slot white background, shared-edge blended pixels, three-color convex combination residual), exit code 2 when the verdict is not passed |
| `text` | one row per layer: the **resolved** `content` (tokens substituted exactly as `render` substitutes them), `mode`, `size_rel`, `rotation_deg`, the `source_slot` when it names one, and for a free layer `position` / `anchor` or for a tiled one `step` / `tiles` (the grid `tiled_grid` answers). It decodes only the slots a layer names, once each, and writes nothing. Without it, "{date} is filled from EXIF" could only be checked by rendering and reading pixels back |
| `image` | one file's decode facts: `mime`, `width`, `height`, `depth` (8 or 16), `aspect`, `exif_bytes`, `date` (EXIF `DateTimeOriginal`, empty when absent). It is how "HEIC decodes" and "orientation 6 is applied" are visible without rendering a project. `--out`/`--dpi`/etc. are usage errors: it decodes at the file's own size and writes nothing |

**S6.5's two subcommands** turn the interaction layer's questions into the machine surface (`AGENTS.md`: nothing may be possible only in the GUI). `hit` answers about geometry without decoding a byte; `save` is the one command that writes a document that already holds a user's work.

| Item | `hit` | `save` |
|---|---|---|
| shape | `template`, `version`, `slots`, `at`, `hit` and `slot` — `slot = <n>` when the point is in a slot, `slot = none` with `hit = false` when it is in a gutter or off the canvas | `template`, `version`, `aspect`, `cells`, `text`, `bytes` (the file that was written) |
| source | `--project` (the document's **embedded** geometry, which is what makes a saved project's hit region stable) or `--template` (this build's library), exclusively | `--project`, required |
| `--at <x>,<y>` | normalized canvas coordinates, both components in `0..=1` (the limit table). The same space `probe` prints its slot sample points in, so a probe row feeds straight back in | — |
| `--out` | — | required, `.pixlay`, and **replaced** if it exists: that is what saving is, and `init` is the command that refuses to overwrite. The write is atomic (`File::create` a dotfile in the target's directory, `sync_all`, `rename`), so a crash leaves either the old file or the new one |
| relative sources | not resolved at all: a project whose photos have moved still answers | rewritten when `--out` lands in another directory, so the copy still finds the photos of the project it was copied from; an absolute source is left as it stands |
| no slot / no file | exit **0**: "no slot owns this point" is an answer, like `templates --aspect 7:5` reporting `count = 0` | a missing `--project`, a refused version or a write failure is exit **2** with stdout empty, and nothing is written |

**Command history has no subcommand.** `History`/`Command` live in `pixlay-core` and the GUI is their only caller: there is no CLI editing session for a verb to act on, and the observable that matters — the pixels after undoing everything — is a *test* (`pixlay-render/tests/history.rs`, `pixlay-cli/tests/history.rs`), which measures it more directly than a verb could. What the CLI does carry is the write path (`save`) those two share.

**S2's two subcommands report the template library and create a project.** They add a data source, not a new failure mode:

| Item | `templates` | `init` |
|---|---|---|
| shape | `template.<i>.{name,slots,aspect,version}` plus `count` (and `aspect`, when filtering) | `template`, `version`, `aspect`, `canvas`, `cells`, `bytes` |
| `--aspect` | the only flag it takes: accepts `W:H` (`4:3`) or a decimal, matched against the template's declared ratio within `ASPECT_TOLERANCE` (the same comparison `CollageDoc::validate` applies). A ratio nothing was authored for is `count = 0` and exit 0 | — |
| `--template` / `--out` | — | both required; `--out` must end in `.pixlay` |
| refusal | any other flag (`--dpi`, `--project`, …) is a usage error (exit 1) | same; and an existing `--out` path is a **failure** (exit 2) because `init` never overwrites a project |
| unknown template | — | usage error (exit 1), stderr lists the names this build knows |
| content | the whole library in library order (by slot count) | a photo-free default project at the template's aspect on a 1189 mm long edge, every cell empty, written by `CollageDoc::to_json` and loadable by `Project::load` |

Measurement rules (`AGENTS.md`): peak = `/proc/self/status`'s `VmHWM`; time = wall clock, with compositing and encoding reported separately.

`probe`'s threshold constants (the sources are commented in the code):

| Constant | Value | Source |
|---|---|---|
| in-slot sampled color | **exact equality** (no tolerance) | the sample point is the "point farthest from the boundary", far from antialiasing boundaries |
| seam blend cap | ≤ 2 px/row, at most 2 px wide | S0 measured 1.08 px/row, at most 1 px wide |
| three-color convex combination residual cap | 3.0/255 | S0 measured 0.20/255; at 300dpi with eight slots the measured worst was 0.63 |

`probe` uses **flat** content (one color per slot): only when the color blocks are flat can the blended pixels on a seam be distinguished from the content. The in-slot sample point for a geometry is
"the point farthest from the boundary" (a coarse grid search + successive refinement), so the bounding-box center of an L-shaped slot is not misused.

## 6. v1 non-goals (must be listed explicitly, otherwise "it won't be enough later" is invisible to everyone)

- per-slot text layers (text is **canvas-level only**; a tiled watermark is one of its modes, not a second mechanism)
- **a font field, or any bundled font**: a text layer has no family, weight, style or
  alignment of its own, and the layer's box is filled from the left. The renderer asks
  fontconfig for the generic `sans-serif`, which is what makes a collage portable between
  machines and what keeps a translated language pack from needing one font per script.
  *Counted cost: a user who wants a specific typeface cannot have one in v1. The way out
  is a `font` field with a `serde` default, which is an addition and not a version bump.*
- **per-line punctuation trimming**: a `。` at the end of a line keeps its trailing blank
  (JLREQ's 行末の約物, the other half of squeezing), and no mark hangs into a margin.
  v1 compresses a *run* of marks and nothing else — the rule above is what a line's start
  and end are aligned by.
- rich text: no bold/italic runs, no alignment paragraph, no tables or columns, and no
  text box of the user's own size (the canvas width is the box)
- nested groups / layer trees / blend modes
- framing rotation beyond ±45°; **rotation only crops edges, it never grows the canvas**
- CMYK JPEG and per-slot colour spaces. **Not** the source ICC: v1 honours it — the
  decoder converts a profiled file to sRGB (measured within 0.03 levels of
  ImageMagick's own conversion, and 15.4 levels away from ignoring the profile), and
  interprets an unprofiled file as sRGB. No colour code of our own: no `lcms2`, no
  rendering intent to define (§4, "Colour")
- curves / levels / masking / brushes; all of them in `AGENTS.md`'s "Directions not to improve" → "Not doing"
- multi-page / multi-canvas projects
- **version migration** for `.pixlay` (not written; higher refused, lower refused too, see "Version policy")
- MCP server, REPL / watch, natural-language arguments, reading defaults from a config file

## 7. Implemented later but the shape is already frozen

| Item | Lands in | Shape |
|---|---|---|
| clamp math | **S3, landed** | `CropTransform::fit(slot, canvas_aspect, photo_aspect) -> CropFit { transform, rotation_limited }`, applied by `draw`; the fit is idempotent and never exceeds `MAX_ZOOM` |
| template generator | **S2, landed** | `pixlay_core::templates` (`generator` recipes + the committed `frozen` data) and the `templates` / `init` subcommands; see §3 and §5 |
| the image pipeline | **S4, landed** | `pixlay-imaging`: `Source::decode`, `resample`, `LinearRgb16::apply`, `slot_bitmap`/`slot_bitmaps`, `probe`; the buffer ladder and the colour decisions are §4.1 |
| command history / hit testing / project writing | **S6.5, landed** | `pixlay-core`: `Command` (one edit: source, framing, grade, filter, text layers, the `{date}` fallback, the canvas) and `History` (snapshot undo/redo; `apply` is all-or-nothing and the document has no mutable accessor), `Template::slot_at(point)` for hit testing, `CollageDoc::save` / `Project::save` / `Project::save_as` for writing a document. The CLI's `hit` and `save` are the machine surface of the first and the last; the command history is a test surface only, on purpose (§5) |
| text rendering | **S5, landed** | `pixlay_render::text`: one Pango layout per layer, drawn by `draw`; token resolution is `TextLayer::resolve` in `pixlay-core`, the tile grid is `pixlay_core::tiled_grid`; see §1 "Text layers" |
| encoding and metadata | **S6, landed** | `pixlay_imaging::encode`: one pass per format writing pixels, resolution, sampling and the ICC profile (`icc`), for PNG / JPEG / TIFF; the CLI's `--long-edge` / `--chroma` and the per-format rules are §5, the profile is §4.1 |

## 8. Measured (2026-09-20, this machine)

| Item | Value |
|---|---|
| golden image RMSE | **0.0** (deterministic for the same build); the equivalent RMSE of a 1 px geometry error is 12 |
| preview vs export (2N downsample) | RMSE **2.32** (threshold 6, `AGENTS.md`'s A0 measured 2.62) |
| band stitching vs whole image | RMSE **0.033**, max pixel difference 2/255, 311 / 463080 bytes differ (scale 1.0); scale 0.1/0.3/0.5 measured too, the sizes always sum to the whole image |
| banding memory saving (A0 landscape 14043×9933, 10 slots, 300dpi) | whole image **1470 MB** → 4 bands **772 MB** → 16 bands **597 MB** (`VmHWM` measured in a separate process each time) |
| seam blend `probe` | ≤ 2 px/row (the threshold), `foreign = 0` |

### S4 (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| the `AGENTS.md` verification render (`render --project tests/fixtures/verify.pixlay --dpi 300 --stats`, eight real photos: JPEG, PNG, 16-bit PNG, HEIC, EXIF-rotated, dated) | 14043×10532, **ms 6311** (decode + resample + draw) + **encode_ms 1844**, **`peak_rss_mb` 1633**, 9,056,692 bytes |
| the same project as a 1200 px preview | 805 ms, 35 MB peak, 224,649 bytes |
| the photo-free smoke path (`render --template mosaic-8-s14 --dpi 300`, A0, every cell empty) | draw only, 264 ms, 996 MB peak — a white sheet is what "no photos" means |
| decode, per photo (`image`) | 20–52 ms over the eight fixtures, process start included; peak well under 100 MB |
| the strip ladder (`strip-10-10x1`, A0 14043×7899, ten 4000×3000 photos) | compositing 5189 ms + encode 1744 ms, **`peak_rss_mb` 1182** — where handing `draw` the whole displayed photo would have taken 3.33 GB of bitmaps (10 × 333 MB) plus the 443 MB output surface |
| resampling vs ImageMagick Lanczos, 1600 → 200 px | RMSE 1.41 (max channel difference 29 at the fixture's hard edge, where the two implementations' ringing differs) |
| zone plate, 4096 → 512 px, mean error against the exact area average | outer band (past 2x Nyquist) **0.0202** against **0.3183** for one sample per output texel — a 16x separation; inner band 0.0182 against 0.0191 |
| source ICC honoured | RMSE 0.04 against ImageMagick's own Adobe RGB → sRGB conversion, 15.4 against the same numbers unconverted |
| 16-bit intermediate | the sRGB round trip is exact for all 256 code values; an 8-bit *linear* intermediate loses more than 16 of them |
| grading identity | `factor = 1, s = 1, Δ = 0` is byte-identical; `factor = 1.25` moves the mean by more than 5 levels |
| `probe` on the A0 project (flat content) | 532 ms, 2231 MB peak, 8/8 slots on their palette colour, 12/12 seams clean, blend 0.999 px per seam px, worst residual 0.12 (threshold 3.0), widest run 1 px (threshold 2), `foreign = 0` |

### S5 (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| the `AGENTS.md` verification render (`render --project tests/fixtures/verify.pixlay --dpi 300 --stats`, eight photos **and one `{date}` layer** since S5) | 14043x10532, **ms 6164/6359** (two runs) + **encode_ms 2469/2475**, **`peak_rss_mb` 1641**, 9,114,833 bytes. The same project with the layer removed: ms 6360/5660, peak 1631, 9,056,692 bytes — **the one line's cost is below the run-to-run spread of the decode+resample stage**, so no per-layer number is claimed at 139.5 MP |
| per-layer cost at 16.7 MP (400x300 mm at 300 dpi, empty cells) | white sheet alone **24-42 ms** (3 runs); + 2,601 tiles **295-436 ms** → a tile is about **0.13 ms**, so the 10,000-tile cap is ~1.3 s of drawing at that size; + 20 wrapped CJK captions 31-57 ms (below the spread) |
| punctuation squeezing | one em per full-width mark; `。，` = 0.5 + 1.0 em, `。。。` = 0.5 + 0.5 + 1.0, a lone `。` = 1.0, and a mark at a line boundary keeps 1.0 |
| kinsoku | `他他他说。他` at a four-em width breaks as `他他他 / 说。他`; over 4 paragraphs x 6 widths, no line starts with `、。，．：；？！）］｝〕〉》」』】〙〛’”` and none ends with `（［｛〔〈《「『【〘〚‘“` |
| preview vs export with text (2N vs N, downsampled) | RMSE **1.92** (threshold 6; AGENTS.md's photo-only A0 measurement is 2.62); the text's ink rectangle at 2N is the one at N doubled to within 1 px |
| the committed test font | `pixlay-cli/tests/fixtures/fonts/pixlay-test-sans.otf`, **93,100 bytes**, 691 glyphs covering 204 codepoints, GPOS `halt` present; regenerated by `fonts/generate.py` from Arch's `noto-fonts-cjk` (SIL OFL, `OFL.txt` beside it) |
| the text fixture (`render --project tests/fixtures/text.pixlay --dpi 150 --preview-px 2400`) | 2400x1801, **ms 459** + encode 45, peak 77 MB, 3,320,360 bytes with its three layers; the same project with `text: []` is ms 355, 3,276,176 bytes — the three layers (a wrapped 45-character CJK caption, a date line and a 15-tile watermark) cost about **100 ms** at 2400 px |

### S6 (2026-09-21, `--release`, this machine)

The `AGENTS.md` verification render (`render --project tests/fixtures/verify.pixlay --dpi 300`),
eight photos and one `{date}` layer on a 14043x10532 A0 sheet, per format:

| Item | Value |
|---|---|
| JPEG q90 4:4:4 | **9,216,300 bytes**, `encode_ms` **1799**, `peak_rss_mb` 1643. The encoder is `jpeg-encoder` 0.7.1; the S0–S5 baseline was `image`'s (= zune-jpeg): 9,114,833 bytes / 2469 ms — **1.1% larger and 27% faster** |
| JPEG q90 4:2:0 (`--chroma 420`) | **6,110,454 bytes** (−34%), `encode_ms` **1031**, same 300 dpi and the same ICC profile |
| PNG | **33,955,066 bytes**, `encode_ms` **5709**, `peak_rss_mb` 1642. Against the S0 baseline (27,773 ms, 125.6 MB for synthetic grain content, `docs/STEPS.md` "Measured baseline") this is 0.21x the time — the content differs (eight resampled photos here, per-pixel grain there), so it is a ceiling check on the 3x cap, not a like-for-like ratio |
| TIFF (LZW + horizontal predictor) | **42,748,009 bytes**, `encode_ms` **2495**, `peak_rss_mb` 1642 |
| the whole render, all three formats | `ms` 6250–6559, i.e. unchanged from S5's 6164–6359: encoding is the only new cost, and it is inside `encode_ms` |
| `--long-edge 9000` on the same project | 9000x6750 px, `dpi` **192.262405**, `pHYs` 7569 px/m, 16,948,376 bytes, `encode_ms` 2988, `peak_rss_mb` 712 |
| the same project as a 1600 px preview | ms 315 + encode 423, `peak_rss_mb` 54, 1,010,033 bytes |
| the embedded profile | **664 bytes**; `identify` reads it back as `icc:description: sRGB IEC61966-2.1` in all three formats; colorants within 2.2e-4 of the sRGB profile ImageMagick/lcms2 wrote into `photos/adobe-rgb-srgb.png`, all five `para` parameters within 1.5e-5 (one unit in the last place) |
| the profile against lcms2 | `magick export.png -profile /usr/share/color/icc/colord/sRGB.icc`: RMSE **0.378 of 65535** = 0.0015/255 over a 1200 px preview — the two profiles describe the same colour space |
| what the tools report | PNG: `Resolution: 118.11x118.11 PixelsPerCentimeter`; JPEG: `300x300 PixelsPerInch`, `jpeg:sampling-factor: 1x1,1x1,1x1` (and `2x2,1x1,1x1` for `--chroma 420`); TIFF: `300x300 PixelsPerInch` and `ICC Profile: <present>, 664 bytes` (the `914.4, 914.4 pixels/inch` reading is the encoder test's fractional-resolution case; checked with `identify -verbose` and `tiffinfo`) |
| visual inspection | `/var/tmp/pixlay-s6/preview.png`: the same eight slots as S5, the concave slot continuous, the photos' own white blocks where the fixtures have them, the `{date}` caption reading `2019:07:14 10:32:00`, no white inside any slot |

Every threshold constant in the tests annotates this source, so a change in the numbers can be discovered.
