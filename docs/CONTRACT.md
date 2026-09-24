# S1 contract v1

Frozen on 2026-09-20. This file is the reading copy for the **contract review**: every shape, every limit and every non-goal that was reviewed is in here.
The implementation is authoritative, and this file is its guide; when the two disagree the tests win (the tests are in
`crates/pixlay-core/tests/`, `crates/pixlay-render/tests/`, `crates/pixlay-cli/tests/`).

The contract is **frozen at S1** and every step after it is built on top of it (`AGENTS.md`, "Step discipline", principle 3).

> **What the ruling of 2026-09-22 changes in this file.** The shapes landed in the plan's S11
> (2026-09-22), so everything below is the implementation this build has.
> - the **±45° cap on a cell's rotation is removed** — the angle is free and the clamp never reduces it, so
>   `CLAMP_ZOOM_LIMIT`, `CropFit::rotation_limited` and §2's "clamp degradation threshold" row are gone;
> - **flip and quarter turns are not product capabilities**, so the geometry stage is `crop → arbitrary rotation`;
> - the canvas backdrop stops being hard-coded white: it becomes `frame.color`, which **defaults to white**, so
>   every project written before the field renders byte-identically (measured: the S1 golden image at RMSE
>   **0.0**, and the S5 `verify.pixlay` render byte-identical — §8, "S11");
> - `CollageDoc` gains `frame: { gapRel, radiusRel, color }`, and the fit's coverage reference becomes the
>   **visible rectangle** rather than the slot polygon.
>
> The rest of the contract is untouched. The rulings themselves are in `docs/2026-09-22-UX-DIRECTION.md` §6,
> and the steps that carried them out are in `docs/2026-09-22-STEPS.md` (S11 in particular).

---

## 1. `CollageDoc`: the single serialization shape

```jsonc
{
  "docVersion": 3,                 // format version; a higher version is refused outright, never guessed at, never downgraded
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
      "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 } },
    { "source": null, "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 } }   // an empty slot renders white
  ],
  "frame": { "gapRel": 0.0, "radiusRel": 0.0, "color": { "r": 255, "g": 255, "b": 255, "a": 255 } }
}
```

> This example is a valid document: the two slots give a cut template whose areas sum to
> exactly 1.0, the template's declared aspect is the sheet's shape, and the frame is the
> S11 default — no gap, square corners, a white canvas. Its photos are the project's own,
> as any project's are.
>
> **The shape after S12d (2026-09-22)**: the `grade`, `filter`, `text`, `textFallback` and
> `canvas` keys are gone, and `docVersion` is **3**. That is what the version policy below
> calls a breaking change: a version-1 or version-2 project is refused with the actionable
> message rather than silently losing the layers and the size it names (the purity ruling
> and ruling 17, `docs/2026-09-22-STEPS.md`).

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
- All coordinates / sizes / font sizes are **normalized to `[0,1]`**; absolute pixels exist only at the
  render and export boundary (`PixelSize::for_long_edge(aspect, n)`). Normalized lengths exist so
  that a preview lays out exactly like an export, whatever the device scale.
- **The document is photos and a layout, nothing else** (S12c): a cell is `source` plus
  `crop`, and the frame is normalized decoration on the sheet. There is no colour stage, no text
  layer, no watermark and no size — the purity ruling removed the first three with the `grade`,
  `filter`, `text` and `textFallback` fields that carried them, and ruling 17 removed the last
  with the `canvas` field.
- **Empty slot = `source: null`**, and that cell renders white. `source` is a path relative to the project file;
  an absolute path is accepted as it stands. A missing file → an explicit error, not a skip.
- `crop.zoom` is **absolute zoom** (displayed width / slot width), not "a multiple of fill":
  when the photo is swapped the baseline does not move and the framing does not jump focus.
- `rotationDeg` accepts **any finite angle** and is normalized to `(-180, 180]` — the ±45° cap was removed on
  2026-09-22 and the validation is widened by S11 — and every component of `crop.offset` has |offset| ≤ 1 (past that no clamp can get the coverage back).
- **Direction convention**: `crop.rotationDeg` is positive **clockwise on screen** (the sheet's y axis points down,
  cairo's `rotate` in that space is clockwise, and the renderer passes it through as-is).
- **A crop is a request; what gets drawn is its fit** (`CropTransform::fit`, S3). The sheet and the slot never grow, so the
  fit has exactly two levers: `zoom` is raised to the value that covers the visible cell with the photo centred (a larger
  request is kept as it is), and `offset` is pulled back along the line to the slot centre until the photo covers again — a
  pan stops at the frame edge rather than being paid for with magnification. `rotationDeg` is kept **exactly as asked**:
  since 2026-09-22 the angle is free and the fit never reduces it, so `CLAMP_ZOOM_LIMIT` and
  `CropFit::rotation_limited` are gone (S11) and `CropFit` is the drawn transform alone. The fit is **idempotent**, so
  clamping on an edit and again in `draw` costs nothing. `draw` applies the fit, so no document this build accepts can
  render an uncovered cell; the fit's own boundary is a slot so extreme that covering it needs more than `MAX_ZOOM`, which
  gets the cap (and is what a decoder's memory budget, S4, limits from the other side). Measured, the free angle's worst
  case over the whole library and every photo aspect is **21.7x** — 46x below the cap (§8, "S11").
- **The frame is the canvas decoration** (`CollageDoc::frame`, S11): `gapRel`, `radiusRel` and `color`, all
  with defaults that are what the renderer painted before the field existed (no gap, no radius, white), so
  a project written earlier renders byte-identically. Both lengths are **fractions of the canvas height**,
  like every other length the format stores: a gap takes half of itself off every side of every cell (two neighbours are
  then `gapRel` apart, and the clip shows the backdrop in between), and a radius is clamped to half the
  smaller side of the cell's inset rectangle so a large request rounds the corners into a stadium. The
  clamp's coverage reference is the cell's **visible region**: the outline clipped to the inset rectangle,
  which is that rectangle exactly for the rectangular slots the library is made of. It does *not* subtract
  the rounded corners — a rounded rectangle's exact support needs circular arcs and the reference stays a
  polygon, so the corner costs a little more zoom than it strictly needs (bounded by the radius, and zero at
  `radiusRel = 0`). The clip is `outline ∩ rounded_rect(inset)`, so a corner shows the backdrop rather than a
  stretched photo.

## 2. Limit constants (all have explicit errors, no panics)

| Item | Value | Source |
|---|---|---|
| `docVersion` | exactly `DOC_VERSION` (currently **3**); higher refused, lower refused too | see "Version policy" |
| slot count | 2..=9 | `AGENTS.md`; nine since S12c removed the ten-slot recipe |
| long edge | 1..=30000 px (`MAX_LONG_EDGE_PX`) | a pixel count, the one size parameter: what a render renders and what an export writes |
| canvas pixels | ≤ 200 MP | the largest grid the product has rendered measured 139.5 MP (§8, "S0"); 43% of headroom left. Checked wherever a grid is **asked for** (S15e): the grid a long edge derives, and the scaled grid `render --preview-px` and `gesture --grid` derive from it |
| one slot's bitmap | ≤ 200 MP texels (`MAX_BITMAP_PIXELS`, the same budget at the bitmap boundary — S15e) | a bitmap is the part of the photo the slot can show: the slot's own extent in output pixels plus the axis-aligned box a rotation needs, so it is bounded by the canvas rather than by the zoom. Refused per slot with the slot named and the conversion's peak bytes reported (§8, "S15e") |
| template aspect ratio | 0.1..=10.0 (`MIN_TEMPLATE_ASPECT` / `MAX_TEMPLATE_ASPECT`) | a template outside this range is not a collage layout. Checked where a template is validated **and** where a pixel grid is derived from one (S15e, PIX-027A): a `NaN`, zero, negative or infinite aspect used to reach the rounding and come back as a one-pixel-by-N grid |
| the render grid | long edge exact, the other edge `round` (half away from zero, at least 1 px) — asserted as 4:3 at 4000 → 4000x3000 | the whole grid request, frozen so an export's size does not drift between builds |
| slot outline | ≥ 3 vertices, finite, every vertex inside `[0,1]`, area > 0 | a polygon with no interior is not a slot |
| framing rotation | ~~±45°~~ **any finite angle, normalized to `(-180, 180]`** (the cap was removed on 2026-09-22; S11 widens the validation). Clockwise is positive, sheet y points down | the 2026-09-22 ruling, `AGENTS.md` |
| framing zoom | `0 < zoom ≤ 1000` | the upper bound is necessary: zoom determines the size of the decoded bitmap, and without an upper bound it overflows. S4's decoder sets a limit **separately by memory budget**; the two layers each mind their own. The fit raises the drawn zoom to the covering value and never lowers a larger request |
| crop offset | every component \|offset\| ≤ 1 (slot widths / heights) | beyond half a slot the photo centre leaves the slot, and no clamp can cover it again. The fit reduces it further whenever the requested pan would uncover the slot |
| template aspect query | `templates::of_aspect` matches within ≤ 1e-6 (`ASPECT_TOLERANCE`) | the picker's grouping: layouts whose declared ratio agrees with the named one |
| frame gap / radius | both finite, `0 ≤ value ≤ 1.0` (`MAX_FRAME_REL`, fraction of canvas height) | the bound is a typo bound, not a design one: a length past the whole canvas height is not a frame around anything. A gap *inside* the range can still empty a small cell, and that is refused per slot by `CollageDoc::validate`, naming the slot |
| frame colour alpha | exactly `255` | the backdrop is painted, not blended: a translucent one would make the exported pixel depend on the surface behind it, which is exactly what "preview and export are the same picture" and "an export is never transparent" forbid |
| `--preview-px` | 1..=20000 (long edge, in pixels) | a preview larger than this cannot be reviewed by eye anyway. The flag bounds the *request*; the grid it scales the base canvas to still has to fit the canvas pixel budget, so `--preview-px 20000` on a square template is 400 MP and is refused with `CanvasTooLarge` (exit 2) before a byte is decoded (S15e, PIX-003) |
| `--at` (`hit`) | both components inside 0..=1 | the canvas *is* `[0,1]`: normalized coordinates are what the document stores and what `probe` prints, so a point outside the canvas is a caller that mis-scaled something, not a hit test with an unusual answer |
| `--long-edge` (export size) | 1..=30000 (long edge, in pixels; `MAX_LONG_EDGE_PX` in `pixlay-core`) | the whole size request: the edge is exact, the other edge follows the template's aspect rounded half away from zero (at least 1 px). The **canvas pixel budget still applies to the grid it derives** (a square canvas at 20000 px is 400 MP and is refused, exit 2), so the flag's range and the budget are two different limits and both are checked |
| `--grid` (`gesture`) | 1..=20000 (long edge, in pixels) | the resting canvas grid a gesture is measured at, derived by the same `PixelSize::for_long_edge` the window's and `render`'s grids are (S15e) — so the rounding and the canvas pixel budget are one rule rather than three, and a 4:3 canvas at 20000 (300 MP) is refused, exit 2 |
| decoded source | ≤ 120 MP and ≤ 20000 px per edge, 20 s | `MAX_DECODE_PIXELS` / `MAX_DECODE_EDGE` / `DECODE_TIMEOUT` in `pixlay-imaging`. A source is RGBA at its own depth, so 120 MP is 480 MB as 8-bit and 960 MB as 16-bit; the area cap is checked between the loader's header and its pixels, so a decompression bomb costs nothing |
| clamp degradation threshold | ~~when the zoom the **requested rotation** needs exceeds `CLAMP_ZOOM_LIMIT` = **1.5 times the upright covering zoom**, the angle is reduced to the widest one that fits~~ — **removed by S11 (2026-09-22): the angle is free and is never reduced, so the rule and the constant are gone; the zoom pays for the angle, and its worst case over the whole library is 21.7x against a cap of 1000x (§8)** | the S3 row as it was decided (`docs/completed/2026-09-20-STEPS-done.md`): its reference was the upright floor, not an absolute zoom, and a ten-column strip needs 6x upright for a 4:3 photo, so a narrow slot was never degraded. Measured kept angles, matching photo and 45° asked (2026-09-21): 45° (unlimited) at 1:1, 34.0° at 6:5, 27.3° at 4:3, 22.6° at 3:2, 18.0° at 16:9, 11.2° at 8:3, mirrored for portrait slots. Kept as the record of what the cap did |

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
- **The matrix is grouped by aspect ratio**, because a sheet's shape *is* its template's (see the limit table).
  The families are `strip-<slots>-<cols>x<rows>` (one band), `grid-<slots>-<cols>x<rows>` (a rectangular tiling) and
  `mosaic-<slots>-<variant>` (mixed splits or a non-rectangular slot) — plus the frozen `mosaic-8-s14`, whose name, `version`, aspect,
  slot order and coordinates are unchanged by S2 (S1's hand-written geometry is now produced by the generator instead of written out).
  A `g` suffix marks a **gutter** in either family (`grid-4-2x2g`, `strip-2-2x1g`): the panes stop short of each other, so the template
  does not tile its canvas and its areas sum to less than 1.0.
- **Geometry version and document version are separate**: `template.version` follows the template family, `docVersion` follows the format.
- **The library covers every slot count from 2 to 9**, at least three layouts each in at least two aspect families (S10); the CLI's `templates` reports the matrix and filters it by aspect ratio. `strip-10-10x1` was the only member above nine and left with S12c.
- **The picker's range is deeper than one layout** (S10, ruling 10): every count from 2 to 9 carries **at least three
  layouts, in at least two aspect families** — 26 templates and 142 slots in all since S12c removed the ten-slot
  recipe, which `pixlay-render templates` reports
  and `crates/pixlay-core/tests/templates.rs` asserts as a histogram over `MIN_PHOTOS..=MAX_PHOTOS`.
- **A shipped name keeps its geometry and its `templateVersion` forever**: S10 added 15 layouts and moved none, and that is a
  test as well as a regeneration diff — `crates/pixlay-core/tests/templates.rs` pins a fingerprint of the geometry every
  template that had shipped before S10 still has, because the determinism test alone cannot see a recipe edit that was
  regenerated with it.

## 4. Rendering: `draw(doc, images, target)`

The single rendering implementation. Preview and export are the **same function**; the only difference is `scale` (and `band`).

```text
draw(&CollageDoc, &Images, &Target) -> Result<(), RenderError>

Target { ctx, scale, canvas_px, band }
Band  { index, count }            // horizontal bands: an A0 can render just one strip
Bitmap                            // ARgb32 premultiplied, already at display size
Images                            // slot → Bitmap; absent = that cell is left white
```

- **Cairo only blits and clips**: the bitmaps coming in are already decoded, downsampled and rotated (`pixlay-imaging`, S4); there is no colour stage left (`S12c`).
- **The crop is fitted before it is drawn** (`CropTransform::fit`, S3): the cell's visible geometry, the aspect of the
  space being drawn into and the bitmap's aspect go in, and the transform that comes out is what is painted. A document
  may therefore store any contract-legal request and still render covered. S4's decoder sizes its bitmap from the same fit
  — the fit's zoom *is* the display size — because sizing from the stored request instead would leave the canvas
  resampling, which it must never do (measured in S3: a request 11.6x below the fitted zoom smeared one texel's
  transparent edge about 6 px into the slot).
- **The canvas decoration stage is the frame** (S11), and it is two halves of the same field:
  - **the backdrop** is the first thing `draw` paints — `frame.color`, white by default — so everything a photo does
    not reach (the gaps, a rounded corner, an empty cell, the canvas border) is that colour. It is painted with
    `Operator::Source`, not blended, so the output is opaque whatever the surface held before.
  - **the clip** is `outline ∩ rounded_rect(inset)` per cell: the outline first, then the inset rectangle with its
    corners rounded, which cairo intersects with the current clip. A corner therefore shows the backdrop instead of a
    stretched photo. An identity frame (`gapRel == 0`, `radiusRel == 0`) adds **no** second clip — clipping to a
    superset of the outline would be clipping to something let through — which is what keeps a project written before
    S11 pixel-identical: measured, the S1 golden image is **RMSE 0.0** against the committed PNG, and the S5
    `verify.pixlay` render is byte-identical (§8, "S11").
- Composite onto an **opaque backdrop, white by default** (`frame.color`): the output is never transparent.
- **Band rendering**: `Band::out_rows()` partitions on **output pixels** (`first = total * index / count`),
  so at any `scale` the band sizes sum to exactly the whole image. It previously partitioned by canvas rows, rounding each band on its own,
  and at 72dpi/scale=0.3 three bands totaled 759 rows while the whole image was 758 rows — `round` is not additive, and this could only be fixed this way.
  Measured, the whole image vs the three-band stitching has RMSE 0.033 (scale 1.0; see below), and scale 0.1/0.3/0.5 was measured too.
  **Banding is a genuinely usable memory-saving measure**: A0 landscape 10 slots @300dpi is 1470 MB for the whole image → 597 MB for 16 bands (see §8).
### 4.1 The image pipeline (S4): what arrives at `draw`

`AGENTS.md`: "All resampling belongs upstream; the canvas only blits and clips."
The upstream is `pixlay-imaging`, and the frozen evaluation order is executed
there, in this order:

```text
decode (upright, sRGB, straight, at the file's own depth)
  → crop to the region the slot can show
  → resample in linear light (Lanczos3, kernel widened by the shrink ratio)
  → flatten onto the slot's opaque white base
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
buffer — and the only quantization is the final 8-bit write.
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

**One bitmap is budgeted; the sum is measured.** Since S15e each bitmap is checked
against the canvas pixel budget (`MAX_BITMAP_PIXELS`) *before* it is allocated, and
the refusal names the slot and what the conversion would have held: the three
destination buffers are 18 bytes per texel (the 16-bit RGBA the resampler writes,
the 16-bit RGB it flattens into, and the ARgb32 the canvas takes), and the row
strip is the one buffer that does not follow the texel count — `16 × dst_w × rows`
with `rows` one block's kernel reach, under 84 MB for any source inside the
decoder's caps. The **sum** over slots is not enforced: it is the memory budget's
question (`Σ dst_px` above), measured per document in §8 rather than checked, and
the 200 MP bound is per bitmap.

**One exception, and why it stays: the framing rotation.** `AGENTS.md`'s sentence
also names "rotation interpolation", and the framing rotation (any angle since
2026-09-22, from `CropTransform`) is still applied by `draw` itself, as S3 built it. The reason is
what the bitmap *is*: it already arrives at exactly the size it is displayed at, so
Cairo's affine is a rotation at 1:1, not the downscaling the constraint is about —
and S3 measured that the placement is what makes "a crop is a request; what is drawn
is its fit" true for every caller (28,800 framings, coverage exact to one ulp, and
360 renders with every sample ≥3 px inside a slot showing that slot's colour). Where
the constraint bites — decoding, downsampling, colour, and *not* handing
Cairo a photo to shrink — is exactly where S4 put the work.

**The preview-grade source (S12b).** The editor's preview does not resample the photo; it resamples a
**reduction** of it. `pixlay_imaging::reduce::PreviewSource` is a box average in linear light to a
requested long edge (`PREVIEW_SOURCE_SCALE` x the grid, or the photo itself when that is smaller), and
`pixlay_imaging::Preview` caches it beside the photo's identity (path + `mtime` + target size). The
resampler then runs unchanged from those pixels, so this is not a second renderer and not a new stage in
the frozen order: it is *decode + colour normalization* handed fewer samples, and the export path
(`slot_bitmaps`) never sees it. The reason it exists is §8 "S12": one cell's cost follows the source's
resolution, and 24 MP per step is 12x a 60 Hz frame. What it costs in fidelity is measured in §8 "S12b"
— a fraction of a level on photo content — and **a picture whose fidelity is compared against the
export** (the window's canvas test, a gallery candidate) must stay inside the RMSE it names.

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
  onto a white background and gives the straight-through RGB the encoder wants. The grid is the caller's
  (`PixelSize::for_long_edge`, S12d): it arrives sized, because there is one surface allocator and one `draw`.

## 5. CLI: the machine operating surface

```text
pixlay-render render    --project <file.pixlay> --long-edge <px> --out <file>
pixlay-render render    --project <file.pixlay> --gap <rel> --radius <rel> --border-color <r,g,b> --out <file>
pixlay-render render    --template <name> --long-edge <px> --out <file>   # no project, no photos
pixlay-render probe     --project <file.pixlay> --long-edge <px>
pixlay-render image     --photo <file>
pixlay-render scan      --dir <path> [--recursive] [--json]
pixlay-render thumb     --photo <file> --px <n> --out <file>
pixlay-render templates [--aspect <ratio>] [--slots <n>] [--json]
pixlay-render init      --template <name> --out <file.pixlay> [--photo <p>...]
pixlay-render edit      --project <file.pixlay> --out <file.pixlay> --slot <i> --rotate <deg> --zoom <z> --offset <x>,<y> --clear
pixlay-render edit      --project <file.pixlay> --out <file.pixlay> --slot <i> --photo <file>
pixlay-render edit      --project <file.pixlay> --out <file.pixlay> --template <name> --add-cell --remove-cell --add-photo <file> --swap <i>,<j>
pixlay-render edit      --project <file.pixlay> --out <file.pixlay> --gap <rel> --radius <rel> --border-color <r,g,b>
pixlay-render hit       --project <file.pixlay> --at <x>,<y> [--json]
pixlay-render hit       --template <name> --at <x>,<y> [--json]
pixlay-render save      --project <file.pixlay> --out <file.pixlay> [--json]
```

| Item | Contract |
|---|---|
| stdout | **only** machine-readable results (sorted `key = value`, or a single object with `--json`). Diagnostics all go to stderr |
| stability | same input, same output; the results carry no timestamps and no absolute paths. `--stats`'s `ms`/`encode_ms`/`peak_rss_mb` are the **only** exception (they are the measurement), and `scan` is the other one **by subject**: a directory listing *is* a set of paths and modification times (S9), so reporting them is the result rather than contamination — two runs over an unchanged directory are still byte-identical, which is what the rule protects |
| locale | under any value of `LANG` / `LC_ALL` / `LANGUAGE`, stdout and stderr are **byte-identical** (including the error branches) |
| interaction | does not read stdin, does not wait for a prompt, works with no TTY; `--help` covers every flag and every exit code |
| exit codes | 0 success / 1 usage error / 2 project, decode, render or write failure / 2 probe verdict not passed. An `--out` that names one of the document's own photos is a **usage error** (1): the command as written is one this build never runs, and refusing it is cheaper than deciding it after a decode |
| usage error and "failed to produce a result" | stdout stays empty; stderr names the failing path (or the missing flag) |
| probe verdict not passed | **not "failed to produce a result"**: the numbers are the result, so stdout emits all the numbers as usual, with `status = failed` and `passed = false`, stderr emits a one-line summary, and the exit code is 2 |
| probe lower bound | when `occupied = 0` (all empty slots) the verdict is **failed**: every question the probe asks is about some slot, and with no slot there is no conclusion. Previously it "passed vacuously" (status=ok, exit 0) |
| output format | determined by the `--out` extension: `.png` / `.jpg` / `.jpeg`, anything else is a usage error (exit 1, stdout empty, the message names the formats this build writes). **Two formats since S12c** — TIFF left with the purity ruling, so `.tif` is refused like any other unknown extension rather than falling back to PNG |
| the destination may not be a source image (S15c) | `render` and `thumb` refuse an `--out` that names one of the document's own photos (`render`) or the photo being read (`thumb`), before a byte is decoded: `refusing to write <out>: it is the source image <photo>`, exit 1, nothing written. Four spellings are the same file and all four are refused — the literal path, a `..` form, a symbolic link and a hard link — by comparing the **normalized spelling** (lexical, no filesystem access) and the **file identity** (device and inode, which is what only the filesystem knows). `AGENTS.md`'s source images are read-only is the constraint; this is the surface that would have broken it. A *document* write (`init` / `edit` / `save`) is outside the rule: those write `.pixlay` only |
| export size | `--long-edge n` (1..=30000) makes the long edge exactly n pixels and sizes the other edge from the template's aspect rounded half away from zero (at least 1 px). Absent the flag, `render` and `probe` use 4000 (`DEFAULT_LONG_EDGE_PX`) — a square grid of it is 16 MP, an eighth of the 200 MP budget, so the default never touches the limit |
| per-format metadata (S6, resolutions removed by S12d) | PNG: **no `pHYs`**, `iCCP` with the profile (the `sRGB` chunk is **not** written next to it — the specification says the two should not both appear, and the profile is the one carrying the colorimetry). JPEG: JFIF `APP0` with the density unit **0** (square pixels, no resolution — the encoder's default), `APP2` `ICC_PROFILE` segments, and the frame's own sampling factors, which are **4:4:4** since S12c removed the request. There is no third format |
| JPEG quality | **90, fixed** (not a flag): it is the S0–S6 baseline, so every measurement in §8 stays
comparable, and `--quality` was deliberately not added — a knob nobody tests breaks quietly |
| `--preview-px n` | n pixels on the long edge; the same `draw`, only `scale` changes. The **bitmaps are sized for the preview too** (S4): decoding and resampling a full A0 and letting Cairo shrink it would cost the export's time and memory for a thumbnail, and would do the shrinking with Cairo's filter instead of the pipeline's. The scaled grid is checked against the canvas pixel budget before the first decode (S15e, PIX-003) |
| `render`'s report | carries `long_edge` (the integer the output was rendered at), `cells` and `occupied` next to the written file's facts |
| `probe`'s report | carries `long_edge` (the integer grid it sampled) instead of a resolution for the same reason |
| `--stats` | appends `{ms, encode_ms, peak_rss_mb, icc}`; `render` emits all four, `probe` emits no `encode_ms` (it does not encode). `icc` is the description of the profile the written file carries (`sRGB IEC61966-2.1`); a command that writes no file reports `none`. The measurement rules are below |
| `probe` | samples and outputs numbers (in-slot photo color, out-of-slot backdrop, shared-edge blended pixels, three-color convex combination residual), exit code 2 when the verdict is not passed. The background field is `bg_off_backdrop` — "off the document's backdrop colour", which is `frame.color` and white unless the document says otherwise (S11; it was `bg_non_white` while the backdrop was hard-coded) |
| `image` | one file's decode facts: `mime`, `width`, `height`, `depth` (8 or 16), `aspect`, `exif_bytes`, `date` (EXIF `DateTimeOriginal`, empty when absent). It is how "HEIC decodes" and "orientation 6 is applied" are visible without rendering a project. `--out`/etc. are usage errors: it decodes at the file's own size and writes nothing |

**S6.5's two subcommands** turn the interaction layer's questions into the machine surface (`AGENTS.md`: nothing may be possible only in the GUI). `hit` answers about geometry without decoding a byte; `save` is the one command that writes a document that already holds a user's work.

| Item | `hit` | `save` |
|---|---|---|
| shape | `template`, `version`, `slots`, `at`, `hit` and `slot` — `slot = <n>` when the point is in a slot, `slot = none` with `hit = false` when it is in a gutter or off the canvas | `template`, `version`, `aspect`, `cells`, `bytes` (the file that was written) |
| source | `--project` (the document's **embedded** geometry, which is what makes a saved project's hit region stable) or `--template` (this build's library), exclusively | `--project`, required |
| `--at <x>,<y>` | normalized canvas coordinates, both components in `0..=1` (the limit table). The same space `probe` prints its slot sample points in, so a probe row feeds straight back in | — |
| `--out` | — | required, `.pixlay`, and **replaced** if it exists: that is what saving is, and `init` is the command that refuses to overwrite. The write is atomic (a temporary file in the target's own directory, `sync_all`, `rename`; `pixlay_core::atomic`), so a crash leaves either the old file or the new one — and a file that is already there **keeps its mode**, so a project saved while it is readable only by its owner does not come back world-readable (S15c, PIX-016). A file that is not there yet gets the process's umask default. The path itself is what is replaced: a symbolic link at it is replaced by the regular file rather than followed |
| relative sources | not resolved at all: a project whose photos have moved still answers | rewritten when `--out` lands in another directory, so the copy still finds the photos of the project it was copied from; an absolute source is left as it stands |
| no slot / no file | exit **0**: "no slot owns this point" is an answer, like `templates --aspect 7:5` reporting `count = 0` | a missing `--project`, a refused version or a write failure is exit **2** with stdout empty, and nothing is written |

**Command history has no subcommand.** `History`/`Command` live in `pixlay-core` and the GUI is their only caller: there is no CLI *session* — no undo stack, no interactive editing — and the observable that matters — the pixels after undoing everything — is a *test* (`pixlay-render/tests/history.rs`, `pixlay-cli/tests/history.rs`), which measures it more directly than a verb could. What the CLI carries is the write path (`save`, and `edit` for the framing) those two share: one document in, one document out, no state between runs.

**S2's two subcommands report the template library and create a project.** They add a data source, not a new failure mode:

| Item | `templates` | `init` |
|---|---|---|
| shape | `template.<i>.{name,slots,aspect,version}` plus `count` (and `aspect`, when filtering) | `template`, `version`, `aspect`, `cells`, `bytes` |
| `--aspect` / `--slots` | the only flags it takes (S14 added the second): `--aspect` accepts `W:H` (`4:3`) or a decimal, matched against the template's declared ratio within `ASPECT_TOLERANCE` (the picker's own `templates::of_aspect` query), and `--slots` filters by slot count (`Selection::layouts`, the gallery's query). A ratio or a count nothing was authored for is `count = 0` and exit 0 | — |
| `--template` / `--out` | — | both required; `--out` must end in `.pixlay` |
| refusal | any other flag (`--long-edge`, `--project`, …) is a usage error (exit 1) | same; and an existing `--out` path is a **failure** (exit 2) because `init` never overwrites a project. The refusal is the creation itself (`create_new`), not a check followed by a write: two `init`s that race leave exactly one winner, and a symbolic link at the path — dangling or not — is a file that is already there rather than a name to write through (S15c, PIX-015) |
| unknown template | — | usage error (exit 1), stderr lists the names this build knows |
| content | the whole library in library order (by slot count) | a photo-free project at the template's aspect, written by `CollageDoc::to_json` and loadable by `Project::load`; **with `--photo` the arguments fill the cells in order** (below) |

**S9's two subcommands are the library's machine surface** — stages 1–2 of the main path, "browse a folder" and "show me this photo" — plus the extension of `init` that turns a selection into a document. The picker's grid and its fit-and-zoom preview call the same two pieces of code, so what the GUI shows has a number behind it.

| Item | `scan` | `thumb` |
|---|---|---|
| shape | `dir`, `recursive`, `count`, `failed`, and one `file.<i>` row per photo: `path`, `status`, and either `mime` / `width` / `height` / `date` / `mtime`, or `reason` | `format`, `mime`, `src_w`, `src_h`, `px`, `out_w`, `out_h`, `bytes` |
| what it is for | what a picker needs from a folder, and the key S12's decode cache invalidates on: `mtime`, whole seconds since the Unix epoch | the picker's expensive half — decode plus resample to a tile's size — as a CLI number; `--stats` is the budget number S12's decisions are measured against |
| size | `height`/`width` are the size **after EXIF rotation** (`ImageDetails`' early dimensions are a hint and are *not* post-rotation, which is why a full decode happens), so `image` and `scan` cannot disagree about a file | `--px n` is the exact long edge, 1..=**8192**; the other edge keeps the photo's ratio (`round`, at least 1 px). The bound is the product's largest preview with room: a full-window 4K photo preview is 3840 px and a HiDPI one 7680, so past 8192 the caller wants `render --preview-px` |
| candidates | files whose extension is in `PHOTO_EXTENSIONS` (`.jpg .jpeg .png .heic .heif .avif .jxl .webp .tif .tiff` — TIFF is still read even though it is no longer written), case-insensitively; **no recursion unless `--recursive`**, and only real directories are descended into (a symlink to a parent would never terminate). A non-photo extension is neither a row nor an error — the alternative is a folder's README becoming an error row | `--out`'s extension, the same four formats `render` writes |
| refusal | a file with a photo extension that does not decode **is** a row (`status = failed`) with the decoder's own reason, and the command still exits **0**: the listing is the result. A `--dir` that is not a directory is exit **2** with the path named | a photo that does not decode, or an `--out` this build cannot write, is exit **2**; `--px` outside the range is exit **1**, and an `--out` that *is* `--photo` is exit **1** (the destination row above, asked before the decode) |
| pixels | — | the whole photo, resampled once at the preview's own grid — the same `resample` (Lanczos3, linear light, kernel widened by the downscale ratio) and the same `over_white` + quantize as a slot, so a preview is not a second picture of the same file |

**`init --photo` is where a selection becomes a document** (S9), and it goes through `pixlay_core::Selection` — the same policy the picker uses (S13), so "the third photo the user picked is the third cell" has one implementation:

| Item | Rule |
|---|---|
| order | **argument order is cell order**; the source of cell *i* is the *i*-th `--photo` |
| count | 2..=9 inclusive (ruling 3). Outside it: usage error (exit 1) naming both bounds (`a collage needs 2..=9 photos, got 10`). Omitting `--photo` entirely is still the photo-free project S2 shipped |
| template | the slot count must equal the number of photos; a mismatch is a usage error (exit 1) naming the template, its slots and the photo count |
| paths | a **photo that is not there** is a failure (exit 2, the path named) — the same rule a project that points at a deleted file follows. Each stored `source` is relative to the project file when the two share a root (`pixlay_core::relative_to`, the function `Project::save_as` rebases with) and absolute otherwise, so a project whose photos sit beside it can be moved. Both sides of that comparison are lexically normalized first (`pixlay_core::normalize_lexical`, S15d), so a `..` in the project path or the copy path cannot produce a relative source that resolves somewhere else |
| the policy itself | `pixlay_core::selection`: `Selection` (ordered photos, the 2..=9 clamp, `layouts()` = the templates with that many slots), `layout_for` (the count rule, S14: same aspect → same recipe family → nearest aspect → library order), `remove_last` / `Removed::restore` (the LIFO batch rule: the last **occupied** cell, because a per-cell clear leaves holes, and the cell comes back in its own slot with its framing — **and the token carries the document's template**, because ruling 7's "brings it back" is exact only if the layout comes back too: the count moves the layout with it, and no three-slot *grid* exists for a four-photo `grid-4-2x2` to grow back into). Pure functions, no filesystem |

**S11 added one subcommand (`edit`) and three shared flags**, because the free rotation and the frame are things a *person*
does and a machine has to be able to do too (`AGENTS.md`: nothing may be possible only in the GUI). The flags are the same
three on both commands, and the difference between them is scope:

| Item | Rule |
|---|---|
| `--gap <rel>` / `--radius <rel>` | fractions of the sheet height, `0..=1` (exit 1 outside). On `render` they override the document **for that render only** — the file is not touched — and on `edit` they are written into the document through `Command::SetFrame` (S15), so the CLI's edit is one undo step of the same command the window's `Frame…` dialog sends, and a gap that empties a cell is refused where it is asked for rather than when the file is validated |
| `--border-color <r,g,b>` | three channels `0..=255`, stored opaque (the frame's alpha rule is §2). The report prints it back the same way |
| what `render` reports | `gap`, `radius` and `border` always, so "which frame did that render use" is answerable without counting pixels — the document's own values, unless a flag overrode one |
| `edit --slot <i>` | the cell the framing flags apply to; `--rotate`/`--zoom`/`--offset`/`--clear` without it are exit 1 **naming the flag**, because taking them as "the frame, then" would drop them silently. `--slot` past the last cell is exit 1 naming the count |
| `--rotate <deg>` | any finite angle, clockwise on screen, stored wrapped into `(-180, 180]` (so a dial cannot accumulate turns in a file). Exit 1 for a non-finite one |
| `--zoom <z>` | `0 < z ≤ 1000` (exit 1 outside) |
| `--offset <x>,<y>` | cell widths and heights from the cell's centre, each within `-1..=1` (exit 1 outside). A **comma pair**, the surface's own convention for a pair (`--at`), where the plan's sketch wrote two arguments |
| `--clear` | empties the cell: no photo, and the framing back to its default. Exclusive with the framing flags (exit 1) |
| what `edit` stores | **the fit** of what was asked for, not the request: a crop is a request and what is drawn is what covers it, so the written file says what it draws. A cell with no photo has no photo aspect to fit against and keeps the numbers as given |
| idempotence | fitting a fit returns it bit for bit, so `edit` applied twice to the same project writes the same bytes — asserted on a rotation that has to be paid for *and* a pan that has to be clamped. A frame is likewise idempotent |
| writing | through `Project::save_as`, the same call `save` makes: atomic, and relative photo paths are rebased when the copy lands in another directory. Since S15d it **returns the project it wrote** — the rebased copy — so a caller that keeps the document in memory (the window) adopts the file's own spellings rather than keeping the old ones. `--out` may be `--project` (edit in place) |
| what `edit` reports | `template`, `version`, `cells`, `photos`, the frame's three fields, `bytes`, and — when `--slot` was given — `slot`, `occupied`, `zoom`, `offset`, `rotation_deg` |
| no `--long-edge` | an edit changes a cell's framing, the document's frame, its layout and the photos it holds; how big an export is another command's question. `--photo` joined the framing flags in S14 (`edit --slot <i> --photo <file>`), with the rules S14 added further down |

**S12 added one subcommand and no flags to the others.** `gesture` is the ruler for what one step of a live
gesture costs, which is the number ruling 1 (2026-09-22) hands the preview's fate to. It drives the same
[`pixlay_imaging::Preview`] the window's decoding thread drives and prints what that build did, split the way
the problem splits:

| Item | Rule |
|---|---|
| shape | `command`, `template`, `version`, `slots`, `occupied`, `slot`, `steps`, `step_deg`, `grid_w`, `grid_h`, `gesture_w`, `gesture_h`, `open_decodes`, `cold_decodes`, `warm_decodes`, `refine_decodes`, `src_w`, `src_h`, `budget_ms`, `verdict` |
| `--project` | required, and **every occupied cell must decode**: a step that cannot be timed is exit 2 naming the cells, because a sequence with a hole in it describes nothing |
| `--grid <px>` | required: the long edge of the **resting** canvas grid, 1..=20000. The report's `grid_*` is that grid and `gesture_*` is the one a live gesture draws at (`gesture_grid`, half of it), so the two the editor uses are both visible. Derived by `PixelSize::for_long_edge`, so the canvas pixel budget applies to the grid it makes: a 4:3 canvas at 20000 is 300 MP and is refused, exit 2, before a decode (S15e) |
| `--slot <i>` | the cell the gesture frames; default the first occupied one. A slot with no photo is exit 1 naming the occupied ones (`--slot` past the cell count is exit 1 as well, as on `edit`) |
| `--steps <n>` | 2..=3600, default 60. **Step 1 is the cold one** (the gesture grid built from scratch) and `warm_ms` is the **median** of the rest: a mean over 60 steps on a busy machine is a number about the machine |
| the four phases | `open` — the document as a window opens on it, at the resting grid (every occupied cell decoded and built); `cold` — the first step of a live gesture, at the gesture grid (the sources for *that* grid's copy are built here, S12b, and the grid's bitmaps are cold); `warm` — every step after it (one cell rebuilt, no decode); `refine` — the release, the resting grid again. Their decode counts are reported separately for exactly that reason |
| `src_w`, `src_h` | the size of the source the **warm** step resampled — since S12b a *preview-grade reduction* (`pixlay_imaging::PreviewSource`), not the file. Before it the field was the photo's own size; against a 6000-px photo the difference (6000 → the copy's own long edge) is what says "the big decode left the step" |
| the step | a *straightening* one, 1 degree further per step: a rotation grows the region the cell shows and since S11 the clamp pays for the angle with zoom, so it is the most per-step work the editor can be asked for |
| `verdict` | `pipeline_holds` when the warm median is ≤ `budget_ms` = **16.666667** (one frame at 60 Hz), `gpu_preview` otherwise. **The exit code is 0 either way**: the measurement is the result, and an exit code that moved with the host's speed would make the same input's answer depend on the machine |
| stability | the *counts* are stable and locale-independent; the *times* are measurements and only appear with `--stats` (`ms`, `open_ms`, `cold_ms`, `warm_ms`, `warm_max_ms`, `refine_ms`, `peak_rss_mb`, `icc = none`), which is the same exception §5 already makes for `--stats` |
| what it does not measure | the cairo blit of the finished bitmaps and the widget's own paint. Those are the window's, and a windowless command cannot reach them; what it measures is the half that used to re-decode |

`step_deg` is `pixlay_imaging::preview::GESTURE_STEP_DEG`, the same constant the canvas's Ctrl+scroll
straightening uses, so the thing measured and the thing used cannot drift apart.

**S14 taught the CLI the layout stage's own vocabulary**, because the count control and the layout gallery
are things a person does and a machine has to be able to do too. Two of the flags are new questions rather
than new commands — `templates --slots` is the gallery's candidate set, and `edit` gained the three
operations the band performs:

| Item | Rule |
|---|---|
| `templates --slots <n>` | only the templates with exactly `n` slots, `2..=9` (exit 1 outside, and the same bound the format's slot limit gives). This is `Selection::layouts` — the gallery's own query — seen from the outside, so a caller can list a photo count's candidates; the two filters combine with `--aspect`. The report echoes `slots` beside `aspect` |
| `edit --template <name>` | switches the document to another layout, keeping the surviving cells' photos and framing (`Command::SetTemplate`'s retention: a layout with fewer slots drops the tail, one with more appends empty cells). An unknown name is exit 1 with the library listed |
| `edit --add-cell` | takes the layout with one cell more, leaving it empty (`Command::AddCell`, the window's `+`). An edit *about the layout*, so it moves the count without placing a photo: the cell the user wants filled is the one that shows a `+`, and clicking that is what asks for the file (S14b) |
| `edit --remove-cell` | takes the layout with one cell fewer, dropping the last cell whatever it holds (`Command::RemoveLastCell`, the window's `−`). Exit 2 at two cells (`a collage's layout has at least 2 cells`) — the floor is the layout's, not the photo count's. The mirror image of `--add-cell`, and refused together with it (exit 1): they are opposites, and one edit is one intent |
| `edit --swap <i>,<j>` | exchanges two cells **whole** — photo and framing both (`Command::SwapCells`), because the framing is what makes a photo look right in *that* cell. Exit 2 for the same cell twice (`slot i cannot be swapped with itself`) and for a cell the layout does not have; a malformed pair is exit 1. The window's own path is `Ctrl+Shift+Arrow`, which names the neighbour geometrically (`Template::neighbour`, S14b) |
| `edit --add-photo <file>` | appends a photo: the first empty cell, else the layout with one slot more (`Command::AddPhotos`). Repeated once per photo in argument order; a photo that is not there is exit 2 with the path named, and a tenth is exit 2 (`a collage takes at most 9 photos`) |
| `edit --slot <i> --photo <file>` | the photo that cell shows instead. Needs `--slot` (exit 1 otherwise, like the framing flags), and the stored path follows `init --photo`'s rule (relative to the project when the two share a root, absolute otherwise) |
| the order of one `edit` | `--template`, `--add-cell`/`--remove-cell`, `--swap`, `--add-photo`, `--slot`/`--photo`, then the framing — so the framing is fitted against the document the earlier flags produced, and `--swap 0,3 --slot 0 --rotate 10` frames the cell that ends up at index 0. `--clear` is exclusive with `--photo` as well as with the framing flags |
| one implementation | every one of these goes through the same `pixlay_core::Command` the window sends (`crates/pixlay-cli/src/cli.rs::edit_project` applies them to a `History`), so "the CLI and the window produce the same document" is a property of the code rather than of two editors kept in step by hand — asserted in `crates/pixlay/tests/layout.rs` |

Measurement rules (`AGENTS.md`): peak = `/proc/self/status`'s `VmHWM`; time = wall clock, with compositing and encoding reported separately.

`probe`'s threshold constants (the sources are commented in the code):

| Constant | Value | Source |
|---|---|---|
| in-slot sampled color | **exact equality** (no tolerance) | the sample point is the "point farthest from the boundary", far from antialiasing boundaries |
| seam blend cap | ≤ 2 px/row, at most 2 px wide | S0 measured 1.08 px/row, at most 1 px wide |
| three-color convex combination residual cap | 3.0/255 | S0 measured 0.20/255; at 300dpi with eight slots the measured worst was 0.63 |

`probe` uses **flat** content (one color per slot): only when the color blocks are flat can the blended pixels on a seam be distinguished from the content. The three layers its residual model fits are the **document's own backdrop** (`frame.color`) and the two slot colors, so a colored frame does not turn a legitimate blend into a "foreign" pixel (S11). The in-slot sample point for a geometry is
"the point farthest from the boundary" (a coarse grid search + successive refinement), so the bounding-box center of an L-shaped slot is not misused.

## 6. v1 non-goals (must be listed explicitly, otherwise "it won't be enough later" is invisible to everyone)

- **text of any kind**: no caption, no watermark, no date stamp, no font field or bundled
  font. S5 implemented canvas-level text layers and S12c removed them — they are not on the
  main path, and they were the one feature whose pixels depended on the host's installed
  fonts (against "identical input yields identical output", `AGENTS.md`). The way back in is
  a `text` field with a `serde` default plus its own `DOC_VERSION` bump, and it is a product
  decision rather than a bug fix
- **per-slot or canvas-wide colour grading**: no exposure / saturation / warmth and no
  one-click filter preset. S4 implemented both and S12c removed them; the pipeline has one
  colour rule left, which is "the decoder's ICC → sRGB" (§4.1)
- nested groups / layer trees / blend modes
- ~~framing rotation beyond ±45°~~ — **removed 2026-09-22: the angle is free**, so the cap and the
  angle-degradation rule that went with it are gone (S11); **rotation only crops edges, it never grows the canvas**
- **flipping or mirroring a cell, in any form**: 2026-09-22's ruling — the per-cell capabilities are
  zoom, move and rotation by any angle. (Loupe has mirror icons and glycin has a `Mirror` operation;
  neither is a reason to add one.)
- CMYK JPEG and per-slot colour spaces. **Not** the source ICC: v1 honours it — the
  decoder converts a profiled file to sRGB (measured within 0.03 levels of
  ImageMagick's own conversion, and 15.4 levels away from ignoring the profile), and
  interprets an unprofiled file as sRGB. No colour code of our own: no `lcms2`, no
  rendering intent to define (§4, "Colour")
- curves / levels / masking / brushes; all of them in `AGENTS.md`'s "Directions not to improve" → "Not doing"
- **physical sizes and resolutions in any form**: no millimetres, no DPI, no `--dpi`,
  no `canvas` field, no resolution in a file the product writes (S12d, ruling 17: a
  raster's only intrinsic size is its pixels, and the product has no concept of
  paper). Sizes are one long-edge pixel count
- multi-page / multi-canvas projects
- **version migration** for `.pixlay` (not written; higher refused, lower refused too, see "Version policy")
- MCP server, REPL / watch, natural-language arguments, reading defaults from a config file

## 7. Implemented later but the shape is already frozen

| Item | Lands in | Shape |
|---|---|---|
| clamp math | **S3, landed**; the angle-reduction half retired and the visible-region reference landed in **S11 (2026-09-22)** | `CropTransform::fit(slot, covering, canvas_aspect, photo_aspect) -> CropFit { transform }`: the angle is never reduced and the coverage reference is the cell's visible region (`Frame::covering`), applied by `draw`; the fit is idempotent and never exceeds `MAX_ZOOM`. `CollageDoc::fitted_crop` / `fit_crop` are the two entry points that pair the frame with the clamp |
| canvas decoration (the frame) | **S11, landed**; its editor is a command since **S15** | `CollageDoc::frame`: `Frame { gapRel, radiusRel, color }`, plus `Frame::covering` / `Frame::clip` and the backdrop + clip stage in `draw`; the CLI's `render --gap/--radius/--border-color` (render-time) and `edit` (§5), and since S15 `Command::SetFrame { frame }` is the one writer both `edit` and the window's `Frame…` dialog send (one undo step, validated per slot). Measured cost at A0: none — the frame is a clip path and a fill (§8, "S11") |
| template generator | **S2, landed** | `pixlay_core::templates` (`generator` recipes + the committed `frozen` data) and the `templates` / `init` subcommands; see §3 and §5 |
| the image pipeline | **S4, landed**; the preview-grade reduction landed in **S12b**; the grading stage removed by **S12c** | `pixlay-imaging`: `Source::decode`, `resample`, `slot_bitmap`/`slot_bitmaps`, `probe`, and the preview's `Preview` caches + `reduce::PreviewSource`; the buffer ladder and the colour decisions are §4.1 |
| command history / hit testing / project writing | **S6.5, landed**; `SetTemplate` added by **S7**; the grade/filter/text commands removed by **S12c** | `pixlay-core`: `Command` (one edit: source, framing, or the template) and `History` (snapshot undo/redo; `apply` is all-or-nothing, answers whether the command was a **step** — one that changes nothing is not (S15d) — and the document has no mutable accessor; the GUI commits **one command per gesture**, §9), `Template::slot_at(point)` for hit testing, `CollageDoc::save` / `Project::save` / `Project::save_as` for writing a document. The CLI's `hit` and `save` are the machine surface of the first and the last; the command history is a test surface only, on purpose (§5) |
| encoding and metadata | **S6, landed**; TIFF and the chroma request removed by **S12c**, resolutions by **S12d** | `pixlay_imaging::encode`: one pass per format writing pixels, sampling and the ICC profile (`icc`), for PNG / JPEG; the CLI's `--long-edge` and the per-format rules are §5, the profile is §4.1 |
| the library and the selection | **S9, landed** | `pixlay_core::selection`: `Selection` (the ordered photo list, the 2..=9 clamp, `layouts()`), `last_photo` / `remove_last` / `Removed::restore` (the LIFO batch rule) — pure, no filesystem. `pixlay_imaging::thumb`: `thumbnail(source, long_edge)`, the same `resample` at a preview grid. The CLI's `scan` / `thumb` / `init --photo` are the machine surface (the rules are §5) |

## 8. Measured (2026-09-20, this machine)

**A note on the records below (S12c + S12d, 2026-09-22).** The sections from S5 onwards include rows
for text layers, and the rows for S4 and later include grading and the canvas-wide filter.
Those features were removed by the purity ruling, so a row that mentions `text`, `grade`,
`filter` or `chroma` is a record of the build that measured it, not a description of this
one. The rows from S0 to S12 also name millimetres, DPI and resolutions (`--dpi`, `pHYs`,
JFIF densities, "A0 at 300 dpi = 139.5 MP" grids): S12d removed the whole concept, so those
numbers are records of the grids the builds rendered — including what a "14043 px long
edge" *means*, which is why the verification entry still renders that many pixels — and
not claims about a document field or file chunk this build has. The numbers stay as they
were taken: they are the process record, and the S12c/S12d results in
`docs/2026-09-22-STEPS.md` are where the removals themselves are accounted for.

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

### S5 (2026-09-21, `--release`, this machine) — text layers, removed by S12c

| Item | Value |
|---|---|
| the `AGENTS.md` verification render (`render --project tests/fixtures/verify.pixlay --dpi 300 --stats`, eight photos **and one `{date}` layer** since S5) | 14043x10532, **ms 6164/6359** (two runs) + **encode_ms 2469/2475**, **`peak_rss_mb` 1641**, 9,114,833 bytes. The same project with the layer removed: ms 6360/5660, peak 1631, 9,056,692 bytes — **the one line's cost is below the run-to-run spread of the decode+resample stage**, so no per-layer number is claimed at 139.5 MP |
| per-layer cost at 16.7 MP (400x300 mm at 300 dpi, empty cells) | white sheet alone **24-42 ms** (3 runs); + 2,601 tiles **295-436 ms** → a tile is about **0.13 ms**, so the 10,000-tile cap is ~1.3 s of drawing at that size; + 20 wrapped CJK captions 31-57 ms (below the spread) |
| punctuation squeezing | one em per full-width mark; a full-width full stop followed by a full-width comma = 0.5 + 1.0 em, three consecutive full-width full stops = 0.5 + 0.5 + 1.0, a lone full-width full stop = 1.0, and a mark at a line boundary keeps 1.0 |
| kinsoku | a six-character CJK sample (three identical Han characters, then one more Han character, a full-width full stop and a final Han character) at a four-em width breaks after the third character; over 4 paragraphs x 6 widths, no line starts with any of a 21-mark closing set (eight punctuation marks — ideographic comma, ideographic full stop, full-width comma, full-width full stop, full-width colon, full-width semicolon, full-width question mark, full-width exclamation mark — the three full-width closing brackets, the eight CJK closing brackets and the two closing quotation marks) and none ends with any of a 13-mark opening set (the three full-width opening brackets, the eight CJK opening brackets and the two opening quotation marks) |
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
| PNG | **33,955,066 bytes**, `encode_ms` **5709**, `peak_rss_mb` 1642. Against the S0 baseline (27,773 ms, 125.6 MB for synthetic grain content, `docs/completed/measurements.md`) this is 0.21x the time — the content differs (eight resampled photos here, per-pixel grain there), so it is a ceiling check on the 3x cap, not a like-for-like ratio |
| TIFF (LZW + horizontal predictor) | **42,748,009 bytes**, `encode_ms` **2495**, `peak_rss_mb` 1642 |
| the whole render, all three formats | `ms` 6250–6559, i.e. unchanged from S5's 6164–6359: encoding is the only new cost, and it is inside `encode_ms` |
| `--long-edge 9000` on the same project | 9000x6750 px, `dpi` **192.262405**, `pHYs` 7569 px/m, 16,948,376 bytes, `encode_ms` 2988, `peak_rss_mb` 712 |
| the same project as a 1600 px preview | ms 315 + encode 423, `peak_rss_mb` 54, 1,010,033 bytes |
| the embedded profile | **664 bytes**; `identify` reads it back as `icc:description: sRGB IEC61966-2.1` in all three formats; colorants within 2.2e-4 of the sRGB profile ImageMagick/lcms2 wrote into `photos/adobe-rgb-srgb.png`, all five `para` parameters within 1.5e-5 (one unit in the last place) |
| the profile against lcms2 | `magick export.png -profile /usr/share/color/icc/colord/sRGB.icc`: RMSE **0.378 of 65535** = 0.0015/255 over a 1200 px preview — the two profiles describe the same colour space |
| what the tools report | PNG: `Resolution: 118.11x118.11 PixelsPerCentimeter`; JPEG: `300x300 PixelsPerInch`, `jpeg:sampling-factor: 1x1,1x1,1x1` (and `2x2,1x1,1x1` for `--chroma 420`); TIFF: `300x300 PixelsPerInch` and `ICC Profile: <present>, 664 bytes` (the `914.4, 914.4 pixels/inch` reading is the encoder test's fractional-resolution case; checked with `identify -verbose` and `tiffinfo`) |
| visual inspection | `/var/tmp/pixlay-s6/preview.png`: the same eight slots as S5, the concave slot continuous, the photos' own white blocks where the fixtures have them, the `{date}` caption reading `2019:07:14 10:32:00`, no white inside any slot |

### S7 (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| the window against the CLI (`tests/canvas.rs`) | five photos, one rotated slot and a `{date}` caption at a 640x480 grid: **RMSE 0.0077** over 307,200 pixels (threshold 6). The same comparison at 900x675 measured 0.0036 |
| the canvas under both colour schemes (`tests/hig.rs`) | **byte-identical** (RMSE 0.0), so the sheet is content and not styling |
| the machine's walk of the main path (`tests/mainpath.rs`) | template → four photos (one chosen, three dropped) → zoom/straighten/pan → JPEG and PNG export → save and reopen → the missing-photo case: **12.6 s** including two windows, four decodes at two sizes, both exports and the 2.2 s missing-photo half; the JPEG at `--long-edge 600` is 600x600 for that square template |
| a missing photo | the slot is reported, the window notices it, its bitmap drops out and the export is refused (asserted) — the contract's "visible, not silent" is a claim with a test |
| the real app | `target/debug/pixlay` runs under the session's Wayland for as long as it is left alone, with nothing on stderr; the window the tests draw is `/var/tmp/pixlay-s7/window.png` |
| the strings | `po/POTFILES` = the crate's **14** source files; `xgettext --language=Rust` finds **74** parseable msgids after S14 added the layout band's (63 before it). Counted the way `tests/i18n.rs::msgids` counts them — `msgid `/`msgid_plural ` lines, the header's own `msgid ""` excluded — because that is the comparison the test makes against the committed `po/pixlay.pot` |
| the layout | no utility pane since ruling 18 (S13): at the minimum window size (480x360) the sheet is still drawn in full (asserted) |
| a display, or none | the four GUI test binaries are one test each and run in a private headless `mutter` the harness starts itself (`tests/support/mod.rs`; the compositor is the test environment, not a product dependency, and `PIXLAY_TEST_CHILD=1` runs them on a display of your own instead), so the whole suite is green on a build box and on a machine in use. What a test can say about that display is that it mapped the window (asserted); the frame count is the harness's own tick callback's, which keeps GTK's clock running whether or not the window is on screen, and a display that never drives the clock costs a wait that times out and says what it saw |

### S9 (2026-09-22, `--release`, this machine)

`scan` and `thumb` on the committed fixtures (the numbers are `--stats`'s own fields, three runs each;
`ms` is decode + resample, `encode_ms` the write):

| Item | Value |
|---|---|
| `scan --dir tests/fixtures/photos` (14 files: JPEG, PNG, 16-bit PNG, HEIC, EXIF-Orientation-6) | **ms 190–194**, **`peak_rss_mb` 32.6–32.8** → ~13.6 ms per file: opening a folder *is* the decodes, and nothing is cached between them |
| `thumb --px 256` of a 960x540 JPEG and a 600x900 JPEG | **ms 49–50** + **`encode_ms` 8.8–9.1**, **`peak_rss_mb` 19.5–19.9**, PNG |
| `thumb --px 1024` of the same photos plus the 1600x1200 PNG and the HEIC | ms 70–155, `encode_ms` 33–179, `peak_rss_mb` 26.6–35.5 |
| the whole preview path against ImageMagick (`magick compare -metric RMSE`, `thumb --px 200` of `resample-source.png` vs the committed `resample-lanczos-200.png`) | **1.51/255** — the same reduction the resample test measures at 1.41/255 through the decoder, so the CLI's decode → resample → flatten → quantize → write path is one implementation of the reference rather than a second one |

That is the picker's budget number S12 is decided against: a 256 px preview of a 1 MP photo costs
~50 ms and 20 MB in this build, which is the cost a gesture step must not pay per frame.

### S10 (2026-09-22, `--release`, this machine)

The gallery's raw material: 15 new layouts, and not one shipped layout moved. The counts are what
`pixlay-render templates` reports; the histogram is (photos → layouts, aspect families).

| Item | Value |
|---|---|
| the library | **27 templates, 152 slots** (S9: 12 and 64), `count = 27`, exit 0 |
| the histogram, 2..=9 | 2 → **3**/3 · 3 → **3**/3 · 4 → **4**/3 · 5 → **3**/3 · 6 → **4**/4 · 7 → **3**/3 · 8 → **3**/3 · 9 → **3**/3 — no count below three layouts, none in a single aspect family |
| ten | `strip-10-10x1` alone, unchanged and never offered: the picker's ceiling is 9 (ruling 3) |
| the S2 invariants over the grown library | 262,144 samples per template: **0 overlapping pairs**; **0 uncovered samples** in the 25 cut templates; the two guttered ones (`grid-4-2x2g`, `strip-2-2x1g`) leave an uncovered region a flood fill from the border reaches (**0 sealed samples**); every cut template's declared areas sum to **exactly 1.0** |
| the shipped geometry | `frozen.rs` **16,795 bytes**; the regeneration diff is **193 insertions, 0 deletions**, so every byte a project built before S10 embeds is still there |
| the same fact as a test | `templates_that_shipped_before_s10_keep_their_geometry`: FNV-1a against the emitted source of all 12 pre-S10 templates; verified to fail by moving `strip-2-1x2`'s split by one lattice cell and regenerating |
| the new layouts render | 15/15 `init --photo` then `render --long-edge 800` exit 0, **135,986–282,539 bytes** each on the fixture photos, and the contact sheet of all 15 is `/var/tmp/pixlay-s10/s10-layouts.png` |
| the `AGENTS.md` verification render, unchanged by this step | 14043x10532, **ms 7232** + **`encode_ms` 2132**, **`peak_rss_mb` 1638**, 9,221,906 bytes, `text = 1`, `occupied = 8` — the same document S9 rendered, since `verify.pixlay` carries its own geometry |
| the test suite | `cargo test` green; the sweeps that walk the whole library got 88 slots longer (hit test 152 slots, layout region sweep 152 × framings, framing sweep 27 templates) |

**Ruling (2026-09-22, human): keep all 15 layouts.** The visual gate after S10 was answered against the
contact sheet rendered with the fixture photos (`/var/tmp/pixlay-s10/s10-layouts.png`), so the library of
§3 stands as measured, nothing was dropped and nothing reworked; the numbers above are its basis, and the
plan's next step is S11.

**Decisions this step made** (all of them additions to a frozen artifact, so each one is a name that can
never be edited again):

- **The histogram is asserted over `MIN_PHOTOS..=MAX_PHOTOS`, not over a literal 2..=9.** The range is the
  selection's own pair of constants, so a change to the picker's cap moves the assertion with it instead of
  leaving a gap behind. Ten keeps the one template it had: it is the library's `MAX_SLOTS`, not a count the
  product offers, and its coverage is S2's criterion, which still holds it.
- **The shipped geometry is pinned by a fingerprint, not by the regeneration diff alone.** The determinism
  test cannot catch "edit an old recipe, regenerate, commit": both sides move together. The fingerprint table
  (`SHIPPED_BEFORE_S10`, FNV-1a over the emitted source, written out rather than `DefaultHasher` because a
  hash algorithm that changes with the toolchain would fail for the wrong reason) is what makes the rule
  *a shipped name keeps its geometry forever* machine-checked, and the failure message prints the whole
  emitted geometry so a reviewer sees which vertex moved.
- **The gutter idiom extended to a strip** (`strip-2-2x1g`, 1/16 of the canvas, top border to bottom). It
  gives the second gutter *shape* a hit test has to answer for — `grid-4-2x2g`'s cross reaches the border in
  four directions, this one in two — and it is the layout two photos with a visible frame between them is.
  The `g` suffix is therefore documented for both families, and the hit test now checks both.
- **A second portrait layout, and four new aspect groups.** `strip-3-1x3` (2:3) and `grid-6-2x3` (2:3) are
  the portrait counterparts of layouts that only existed landscape: a portrait canvas with three or six
  photos had one candidate and now has three. Every new layout was placed in a family the count did not have
  yet where one was missing, which is why every count 2..=9 ends up with three or more aspects rather than
  the required two.
- **The new layouts are equal-sized where a dyadic lattice allows it and deliberately uneven otherwise**
  (`strip-5-5x1` is 3/16 × four plus 4/16, `strip-9-9x1` is six 2/16 panes then 1/16, 1/16, 2/16): an odd
  number of equal columns is not representable on a power-of-two lattice, and `strip-10-10x1` had already set
  the pattern. The alternative — a /64 lattice for the strips — would have multiplied the ladder's smallest
  cells for no visible gain.
- **No new dependency, no `Cargo.lock` change** (`cargo update --workspace` locked 0 packages), and no change
  to any document, limit or CLI shape: S10 is data, plus the assertion that keeps the data's growth honest.

### S11 (2026-09-22, `--release`, this machine)

Two changes were measured against the build before them, and neither costs anything:

| Item | Value |
|---|---|
| byte-identity, the S1 golden image (rectangle slots, angles 12°/0°) | **RMSE 0.0** against the committed `tests/golden/draw-v1.png` — the same 0.0 S1 measured, so the backdrop fill and the (skipped) frame clip moved no pixel |
| byte-identity, the S5 verification document (`verify.pixlay`: eight photos **including the concave slot**, one `{date}` layer) | the rendered JPEG is **byte-identical**: md5 `b28abd09648280185c95002b5f49f4f4` before and after — same for the *regenerated* fixture, which now carries an explicit `frame` block, so "the default frame renders as the absent field did" is a measured claim and not an argument |
| the `AGENTS.md` verification render, new build | 14043x10532, **ms 6704/6854** + `encode_ms` 2009/2128, **`peak_rss_mb` 1638–1640**, 9,221,906 bytes, `gap = 0.000000`, `radius = 0.000000`, `border = 255,255,255`, `text = 1`, `occupied = 8` — inside the S10 spread of 7232/1638 for the same document |
| the covering zoom, as a function of the angle | over **all 152 shipped slots, every whole degree 0..=180 and six photo aspects**, the worst covering zoom is **21.73** (a 2.4:1 photo in `strip-9-9x1`'s 1/16-wide pane at 6 degrees, whose *upright* floor is already 21.6). Per photo aspect: 0.5 → 9.06, 0.8 → 9.06, 1 → 9.06, 4:3 → **12.07**, 1.5 → 13.58, 2.4 → 21.73. `MAX_ZOOM` is 1000, so the cap is 46x above the worst angle — the free angle never reaches the bound that exists for the bitmap arithmetic (`pixlay-core/tests/framing.rs` prints this table) |
| the free angle's cost, exactly (a 4:3 slot with a matching photo) | 1.0x upright, **1.396x at 20°**, **1.886x at 45°**, 1.333x at 90° — the closed form `r·sin t + cos t`, asserted to 1e-9 |
| A0 at the worst angle (`strip-10-10x1`, ten 1600x1200 photos, 300 dpi, 14043x7899) | upright: **ms 5067** + encode 1574, **`peak_rss_mb` 1189**, 8,481,685 bytes. Every cell at **6 degrees** (the worst angle, zoom 6.11x on the 2/16 panes and 12.07x on the 1/16 ones): **ms 8583** + encode 1642, **`peak_rss_mb` 1451**, 8,463,260 bytes. The extra 262 MB is the rotation's own cost, not the zoom's: a rotated cell's bitmap is the *axis-aligned* box of the rotated cell, about 22% larger at 6°, while the zoom itself resamples a smaller source region into the same output. Against S4's 1182 MB for the same shape upright, and `AGENTS.md`'s 2.5 GB budget, the free angle fits |
| the frame at A0 (the same document at 6 degrees, `--gap 0.02 --radius 0.03 --border-color 240,240,235`) | **ms 8418** + encode 1574, **`peak_rss_mb` 1450**, 8,187,135 bytes: a fill plus one more clip path per cell, inside the run-to-run spread of the unframed render |
| the frame's pixels (300 dpi, two half-canvas cells) | the gap's stripe measures the requested width to ±2 px over `gapRel` 0.01/0.02/0.04/0.08 (42 px at 0.04); the rounded corner's exactly-backdrop pixels are **0 at radius 0** and rise monotonically to **4,831** at radius 0.08 (a 85 px radius, whose four corners are 6,202 px of which the rest is arc antialiasing); a coloured backdrop is the requested colour **to the byte** in every one of them, with `white = 0` over the whole sheet |
| `edit` | stores the fit: a 25° request on a portrait cell with a landscape photo writes `rotationDeg = 25` with the zoom the angle needs, and the same edit twice writes **byte-identical** files (also with a frame, and with a pan that has to be clamped); a request above the floor keeps the user's zoom exactly (`3.5` stays `3.5`) |
| the test suite | `cargo test`: the core framing sweep is 125,400 framings (11 angles across the whole circle, 5 offsets, 5 photo aspects, 3 zooms, all 27 templates) plus a 36,480-framing framed sweep; the render crate adds `tests/frame.rs` (five pixel probes) |

**Decisions this step made** (recorded here because each one is a shape later steps build on):

- **The coverage reference is the outline *clipped* to the inset rectangle, not the inset rectangle itself.**
  For every rectangular slot — all of them but `mosaic-8-s14`'s L — the two are the same polygon, so the
  measured behaviour of the gap is exactly what the plan described. They differ for the L: taking the
  bounding box literally would have magnified its photo by up to 4.5% at near-diagonal angles, and the plan's
  own exit criterion says a document with no frame renders byte-identically. `Polygon::clipped_to` is
  Sutherland–Hodgman against the rectangle (exact, since the clip region is convex) and it returns a polygon
  that is already inside unchanged *bit for bit*, which is what makes the unframed fit S3's own arithmetic.
- **A rounded corner is not subtracted from that reference.** The visible region is a rounded rectangle whose
  exact support needs circular arcs; the reference stays a polygon, so the corner asks for slightly more zoom
  than it strictly needs — bounded by the radius, exactly zero at `radiusRel = 0`. The alternative was
  polygon-approximating the arcs, which would make the fit's numbers depend on a segment count.
- **The rotation is normalized on the way in and on every edit, not inside `validate`.** `validate` is
  `&self`, so it cannot wrap; `from_json` and `Command::SetCrop` / `edit` do, and `validate` then only checks
  that the angle is finite. The new `CoreError::NotFinite` exists for exactly that check: a domain
  (`NaN` is not a number to compute with) is not a range, and an `OutOfRange` message would have named a
  bound the value was never compared against.
- **The frame's colour must be opaque.** `Rgba8` carries an alpha channel because the format writes a
  four-channel colour, but a translucent backdrop would make the exported pixel depend on the surface
  behind it — which is the one thing "preview and export are the same picture" cannot survive. It is refused at load rather than silently
  forced to 255.
- **`render`'s frame flags are a render-time override and `edit` is the writer.** Both spellings are the same
  three flags; the scope is the difference, and the report always prints the frame that was used, so "which
  frame did that render use" needs no pixel counting. `edit` is also what the round-trip criterion needed: a
  document's frame must be writable without a window.
- **`edit` stores the fit, and only where a photo exists to fit against.** Storage that described the picture
  was worth more than storage that repeated the request — and it is what makes `edit` idempotent, which is the
  property the plan asked to be re-asserted through the new entry point. An empty cell has no aspect to cover
  and keeps its numbers; `draw` fits it when it gets a photo.
- **`--offset` is a comma pair** (`--offset 0.2,-0.3`), not the two arguments the plan's sketch wrote: `--at`
  already established the convention for a pair on this surface, and a second spelling of "a point" would be
  one more thing to remember.
- **No new dependency, no `Cargo.lock` change** (`cargo update --workspace` locked 0 packages). The frame's
  rounded corners are cairo arcs and the new geometry is 40 lines of clipping in `pixlay-core`.

Every threshold constant in the tests annotates this source, so a change in the numbers can be discovered.

### S12 (2026-09-22, `--release`, this machine)

The step's number, and the reason it is not one number. `pixlay-render gesture` on the verification project
(`verify.pixlay`, seven photos of 0.35–0.72 MP each) and on the same layout with **24 MP** photos
(6000x4000, one file in every cell), two runs each:

| Item | verify project | 24 MP photos |
|---|---|---|
| grid 780 (780x585; gesture 390x293) — the editor's canvas at its default window, measured 2026-09-22, is **768x576** for this document, so this grid is that one within 2% | `open` 160/167 ms (7 decodes), `cold` 42/74, **`warm` 5.48/6.13 ms** (max 6.1/10.6), `refine` 8.95/9.00, peak 42.4 MB, `pipeline_holds` | `open` 2151/2604 ms (1 decode — eight cells, one file), `cold` 1567/2381, **`warm` 199.9/306.8 ms** (max 242/366), `refine` 182/274, peak 175.2 MB, `gpu_preview` |
| grid 1600 (1600x1200; gesture 800x600) | `open` 212/192, `cold` 79.4/79.7, **`warm` 11.98/11.92** (max 13.6/13.8), `refine` 27.4/28.3, peak 72.5 MB, `pipeline_holds` | `open` 2726/2686, `cold` 2393/2363, **`warm` 321.1/319.5** (max 374/370), `refine` 307/201, peak 175.2 MB, `gpu_preview` |
| grid 3840 (3840x2880; gesture 1920x1440) | `open` 577/572, `cold` 158, **`warm` 34.38/33.90** (max 37.0/37.6), `refine` 122.3/123.0, peak 258.7 MB, `gpu_preview` | — |

Read together, those columns say one thing: **the cost of one cell's bitmap follows the source's resolution,
not the output grid.** `resample` widens its kernel with the downscale ratio (S4), so a cell that shows a
quarter of a 24 MP photo reads ~24 MP of source taps whatever size the preview grid is — which is why the
coarse grid buys almost nothing there (one cell: **283 ms at the gesture grid against 289 ms at the resting
one**) while it halved the same cell for the small sources (**9.5 ms → 7.5 ms** at a 1600-px grid). The
`cold` column is the decode leaving the gesture path: it is the *only* phase of the 24 MP sequence that pays
for a file, and it pays 1.6–2.4 s for it.

- **The caches do what they claim.** `open_decodes = 7` for the seven distinct files of the verify project
  (its eighth cell re-uses one), `cold_decodes = 0`, `warm_decodes = 0`, `refine_decodes = 0`: after the
  document is open, **no step of any gesture touches the disk**, at every grid and for either photo size.
  The count is taken over *every* reply, superseded ones included, because a superseded build decoded the
  file all the same.
- **The coarse grid is what the canvas asks for.** `gesture_w/h` is exactly half of `grid_w/h` at every size
  (390x293 of 780x585, 1920x1440 of 3840x2880). The GUI test reads the *request* rather than the reply for
  that claim — every step of a live drag asks for the coarse grid and the release asks for the resting one —
  because a reply is built for the grid it was asked for (asserted byte-for-byte in the imaging tests) and
  *when* it lands is a fact about the machine, not about the canvas.
- **The refinement leaves no trace.** `crates/pixlay/tests/gesture.rs` drives a drag at a pinned 400-px
  canvas (twelve steps, each asserting the grid it asks for), then the release, and compares the canvas
  against **the same document drawn in one edit at rest**: **RMSE 0.0000 over 335,808 pixels**. Zero is the claim (the same function at the same grid), not a
  threshold — and the same test asserts the burst of drag steps does not move the bitmaps without the main
  context being iterated, which is "the UI never waits for the decoder" in the only form a test can hold it.
- **The caches' cost, for the record**: the whole probe at grid 780 peaks at **42.4 MB** with the verify
  project and **175.2 MB** with 24 MP photos in a 2:3 layout (`Preview::MAX_SOURCE_BYTES` is 512 MiB; nine
  12 MP photos are 439 MB, and the verify project's seven are 15.7 MB).
- **Two runs of the same command differ by up to 50%** (34.4 vs 33.9 ms at grid 3840 is 1%; 199.9 vs
  306.8 ms at 24 MP is 53%; 321.1 vs 319.5 ms for the 24 MP/grid-1600 pair is 0.5%). The median over 59 steps removes the mean's sensitivity to
  one slow frame, not the machine's own variance, so every number in this block is quoted as the runs it came
  from rather than as a single figure.

- **The export path is untouched, and proven so.** The same verification document rendered by the **S11
  build** (a detached worktree at `dd94a3d`) and by this one is **byte-identical**: md5
  `b1c8bbb88243eac43fe70d9c9b75455e`, 9,216,300 bytes, from both. The S11 record's own md5 (`b28abd…`,
  measured in that session) does **not** reproduce here even with the S11 build itself, so what moved those
  bytes is outside the repository — the machine took a package update between the two sessions — and the two
  builds agreeing with each other is the claim this step needs. A render with text in it is not comparable
  byte for byte across a library update; a render against a *pinned* font (`FONTCONFIG_FILE`, as
  `pixlay-render`'s text tests do) is.
- **A harness wait was raised, and why.** `crates/pixlay/tests/support/mod.rs::WAIT` is 180 s instead of 60:
  it covers the decoding thread's reply *and* the frame [`snapshot`] waits for before GSK will hand a widget's
  pixels over, and a machine busy with another heavy job can miss 60 s of frames (observed 2026-09-22:
  `mainpath.rs` — a test S12 does not touch — timed out in `snapshot` twice while a release build of the A0
  render tests ran alongside it, and the same test takes 10-12 s on an idle machine; `cargo test --workspace`
  on its own was green in every run). A timeout that fires still means "hung".
- **The `AGENTS.md` verification entry**, unchanged code as above: 14043x10532, **ms 6791** + `encode_ms`
  1809, **`peak_rss_mb` 1641**, 9,216,300 bytes, `text = 1`, `occupied = 8`, `gap = 0.000000`, `radius =
  0.000000`, `border = 255,255,255` — inside the S10/S11 spread for the same document. The image was looked
  at (a downscale of `/var/tmp/a.jpg`): eight cells filled, the `{date}` layer reading `2019:07:14 10:32:00`
  at the bottom, no seam or corner artefact.

**Decisions this step made** (recorded here because each one is a shape later steps build on):

- **The caches live in `pixlay-imaging::preview`, not in the window.** The window's decoding thread and the
  CLI's `gesture` probe are then the *same* build, which is the only way the number can be about the window
  (`AGENTS.md`: nothing may be possible only in the GUI) — and it lets the caches be tested without a display.
- **The bitmap identity includes the source file's modification time**, which S7's rule did not: without it a
  photo edited in another program kept its old bitmap until the user touched that cell, and "a stale cache is a
  wrong picture" is the one thing this cache may never be. The price is one `stat` per occupied cell per build,
  measured 2026-09-22 at **2 µs for eight files** against a 30–110 ms decode.
- **The source cache keys on path **and** modification time, with the file's own identity read by the caller
  once per build.** A file that changed is decoded again; a file replaced while reproducing the same
  modification time is not detected, and the module says so rather than implying otherwise.
- **The cache keeps at least its newest entry**, even when that entry alone is over budget: dropping it would
  re-decode on every motion, which is the cost the cache exists to remove. A source larger than the whole
  budget is decoded and *not* kept (there is nowhere to put it), which is the only case where a lookup
  re-decodes.
- **The gesture grid is half the resting one, and a discrete step never coarsens.** A key press or the zoom
  spin row is one frame the user is meant to look at, so it is committed and drawn at the resting grid
  (`Gesture::Step`); a drag, a wheel and a slider are streams, and those are drawn coarse and refined when
  they end.
- **The verdict is computed from the warm median and does not move the exit code.** The counts and the grids
  are stable output; the times appear only with `--stats`.
- **No new dependency, no `Cargo.lock` change** (`cargo update --workspace` locked 0 packages).

- **What a preview-grade source would buy, measured.** The same 24 MP photo, pre-reduced to the same layout at
  several sizes, stepped at grid 780: **1024 px → `warm` 7.44 ms** (max 8.28, `pipeline_holds`), **1560 px →
  15.65 ms** (max 17.96, holds by 1 ms), **2048 px → 39.08 ms** (max 46.25, `gpu_preview`), and the original
  6000 px → 199.9 ms. The reduction's own size is what decides it, not the original's: a source at or below
  ~1.5x the preview grid is inside the frame budget, and one at 2.5x is not. This is the measurement the fork's
  ruling reads next to the 200 ms above; it is not a decision S12 made.

### S12b (2026-09-22, `--release`, this machine)

The step's two builds, in one session, on the same files: the **S12 build** (a detached worktree at
`c7e0d50`) and this one. `pixlay-render gesture`, 61 steps, two runs per cell (the machine's own variance
at these sizes is up to 50% between runs, which is why every cell carries both); `src_*` is the new field
— the size of the preview-grade source the **warm** step resampled.

| Document, grid | the S12 build | this build |
|---|---|---|
| `mosaic-8-s14` with one 24 MP photo (6000x4000) in all eight cells, grid 780 (780x585) | `open` 1961 ms (1 decode), `cold` 1535, **`warm` 199.9 ms**, `refine` 182, peak 175.2 MB, `gpu_preview` | `open` 369/397, `cold` 237/336 (1 decode: the coarse grid's own copies), **`warm` 2.20/2.22 ms** (max 2.33/3.21), `refine` 7.8/7.9, peak 185.0/185.1 MB, **`src 488x325`**, warm decodes 0, `pipeline_holds` |
| the same document, grid 1600 (1600x1200) | `open` 1944, `cold` 1569, **`warm` 206.5 ms**, `refine` 307, peak 175.2 MB, `gpu_preview` | `open` 492/847, `cold` 278/425, **`warm` 9.12/13.95 ms** (max 9.55/15.58), `refine` 33/51, peak 206.4/206.3 MB, **`src 1000x667`**, `pipeline_holds` |
| the same document, grid 3840 | — (over budget for the verify project at that grid since S12) | `warm` **52.9 ms**, `src 2400x1600`, `gpu_preview`, peak 335.7 MB |
| `mosaic-9-hero` with **nine distinct** 24 MP photos, grid 780 | `open` 3769 (9 decodes), `cold` 3647, `warm` 177.2, `refine` 233, peak **649.4 MB** | `open` 2911, `cold` 2521, `warm` 3.61, `refine` 15.6, peak **217.2 MB** |
| the same nine-photo document, grid 1600 | `open` 4115, `cold` 3639, `warm` 196.9, `refine` 281, peak **665.8 MB** | `open` 3449, `cold` 2316, `warm` 15.17, `refine` 65.6, peak **316.5 MB** |
| `verify.pixlay` (seven photos of 0.35–0.72 MP, so *below* the target at both grids), grid 780 | `open` 239/173, `warm` 9.10/8.40, peak 42.6 MB | `open` 176/174, `warm` **3.21/3.21**, `src 325x488` (the photo itself), peak 51.5 MB |
| the same project, grid 1600 | `open` 216/217, `warm` 12.10/12.14, peak 72.6 MB | `open` 218/220, `warm` 11.50/7.55, `src 600x900` (the photo itself), peak 92.5 MB |

- **The criterion's number.** The 24 MP document steps in **2.20–2.22 ms** at the editor's own grid and
  **9.12–13.95 ms** at 1600, against the 16.666667 ms budget (`warm_ms` is the median of 60 steps; the
  *worst* step measured 15.58 ms, which is the tail rather than the verdict). S12 measured the same two
  numbers as 199.9–200.6 and 206.5–321.1 ms: **the step is 91x and 23x cheaper**, and the phases around it
  moved too — `open` 1961 → 369 ms, `cold` 1535 → 237 ms — because a build's resamples now read a 0.16-1 MP
  copy instead of 24 MP.
- **The scale, and why 1.25.** `PREVIEW_SOURCE_SCALE` is the copy's long edge over the grid's, and it is
  the largest value that holds the budget, measured as a ladder on the same document's 1600-px row:
  **1.5 → 18.40/18.50 ms** (misses), **1.25 → 9.06/9.21**, **1.0 → 7.94/11.03**. Higher is sharper at the
  same cost only until it is not, and this is the row where it stops. The drift barely moves across the
  ladder on photo content (below), so the quality argument is not what decides it.
- **A copy per grid, which is what the two decodes are.** The resting grid and the half-size one a gesture
  draws at have different targets, so the first frame of a gesture builds the coarse copies: `cold_decodes`
  is 1 for the one-file document and 9 for the nine-photo one, where S12's was 0 — and `warm_decodes` is 0
  everywhere, which is what S12's whole claim rests on. The work is the same one-off opening pays, at half
  the target, and the release is served by the resting copies that were already there.
- **The export path is untouched, byte for byte.** The verification document rendered by the S12 build and
  by this one in the same session: md5 **`b1c8bbb88243eac43fe70d9c9b75455e`** (9,216,300 bytes) from
  both, 14043x10532, `ms` 5032 + `encode_ms` 1180, `peak_rss_mb` 1643, `text = 1`, `occupied = 8`. This is
  also S12's own md5, so the machine moved neither library nor font between the two sessions this time. The
  image was looked at (`/var/tmp/pixlay-s12b/export/look.png`): eight cells filled, the `{date}` layer at
  the bottom, the concave slot continuous, no artefact.
- **The drift, as a number with a threshold.** Three measurements, one content each:

  | Comparison | RMSE | source |
  |---|---|---|
  | the preview's path vs `render --preview-px` at 400 px, four cells of a 1600x1200 photo (one rotated) | **1.0051** over 160,000 px | `pixlay-cli/tests/preview.rs`, threshold 6 |
  | the window's canvas vs `pixlay-render render` at 640x480, S7's own document (the fixtures' synthetic hard-edged bands) | **2.1423** over 307,200 px (was 0.0077 before this step) | `pixlay/tests/canvas.rs`, S7's threshold 6 |
  | `slot_bitmap` from the photo vs from the copy, at a 640x480 grid — the reduction *factor* ladder | factor 1.2 **3.38** · 1.5 **4.94** · 2 **6.19** · 3 **7.83** · 4.8 **10.25** on those same bands; on a 24 MP plasma photo at grid 780 (1600): 1.5 **0.12** (0.18) · 2 **0.23** (0.25) · 3 **0.18** (0.29) · 6 **0.30** (0.54) | a throwaway probe, per `AGENTS.md`'s measurement rules |

  So the reduction's cost in fidelity is a **fraction of a level on photo content** and is only visible at
  all on synthetic hard edges — which is why the two committed tests, one on smooth content and one on the
  bands at a 1.2x factor, both stay two to three times inside S7's threshold of 6. **The ladder is also a
  hazard the later stages have to respect**: a picture whose fidelity is compared against the export (a
  picker tile, a gallery candidate — S13, S14) must come from the photo, as `thumb` does, not from a
  reduction at a large factor.
- **The reduction is a pure function**, and it is pinned as one: the same file reduced twice is byte-identical;
  a file that changed is reduced again (the `mtime` rule, asserted through the cache); the fit of a given
  crop is the *same transform* from the copy as from the photo, bit for bit, because the copy carries the
  decoded photo's `aspect()` rather than its own buffer's ratio (`pixlay-imaging/tests/reduce.rs`; the
  criterion allowed 1e-9).
- **Memory.** On the document the criterion names — **nine distinct 24 MP photos** — the peak is **217.2 MB**
  at grid 780 and **316.5 MB** at 1600, against the S12 build's **649.4 MB** and **665.8 MB** for the same
  runs: the cache now holds copies (33-138 MB for nine photos) where it used to hold decoded photos (96 MB
  each, and nine of them do not fit in `MAX_SOURCE_BYTES`). On documents whose photos are *small* the peak
  grows slightly instead (verify: 42.6 → 51.5 MB at grid 780, 72.6 → 92.5 MB at 1600; the one-file 24 MP
  document: 175.2 → 185.0 / 206.4 MB), because a photo below its target is stored as-is and each of the two
  targets keeps its own entry. Both are far inside the 2.5 GB budget of `AGENTS.md`.

**Decisions this step made** (recorded here because each one is a shape later steps build on):

- **The reduction is a box average in linear light, not `resample`.** One reading pass, no ringing, no
  three-lobe kernel; the colour path is the pipeline's own, because averaging sRGB code values is not
  averaging light (a 2x2 black-and-white checkerboard would come back at 0.22 of its linear value). It is
  exact for an integer factor, and it *never enlarges* a photo: at or below the target the samples are the
  decoder's own, which is what makes "a photo the preview can already show" cost nothing but the copy the
  cache has to own.
- **The copy carries the photo's `aspect()`, and that is the whole geometry story.** The fit and the region
  are functions of that number, so a preview's framing is *identical* to an export's — only the sampling
  grid differs. Without it a 1600x1200 photo reduced to 350x263 would frame the cell differently from the
  photo, and every later comparison against `render` would carry a geometry error on top of the filtering
  one.
- **The cache keys on path + `mtime` + target size.** The target is a function of the grid, the resting
  grid moves with the window, and a gesture's grid is half of it; so a file legitimately has up to two
  copies, and a window resize simply makes a new key (the LRU budget evicts the old ones). A copy is a
  function of (file, target) alone — never of what was built before — so the preview's pixels cannot
  depend on the order in which the user happened to resize or drag.
- **The target is a function of the grid, not of the document's own cells.** Sizing the copy by the largest
  cell's displayed extent would buy another 2-3x on small-cell layouts (a 3x3 grid's cells cover a third of
  the canvas), at the price of a copy that depends on the layout rather than on the window, and of a cache
  key the document can invalidate. Not worth it: the step is 23x inside the budget at the editor's grid
  already.
- **No new dependency, no `Cargo.lock` change** (`cargo update --workspace` locked 0 packages). The
  reduction is arithmetic over a buffer that was already there, and the only new code outside it is a
  16-bit inverse-transfer table (`linear_to_srgb16`, 128 KB, built once) so a 16-bit source is reduced at
  its own depth.

### What S12's number says about the preview's future
Read on the plan's own subject — the verification project, at the editor's own grid — the pipeline **holds**:
5.5 ms against a 16.7 ms frame. Read on a realistic photo — 12 to 24 MP, which is what the product's users
pick — it does **not**: ~200 ms per step, and the coarse grid and the caches between them only bought a factor
of ~2.4 (the same cell cost `decode` 171 ms + `resample` 289 ms before them). That is the fork ruling 1
(2026-09-22) reserved, and it was ruled the same day:
**the preview's future is a preview-grade source, not a GPU renderer** — a cached, preview-sized reduction per
photo that the preview's bitmaps are resampled from, inside the one renderer (`docs/2026-09-22-STEPS.md`,
"S12 · Result" and the step "S12b"). So `draw`, `resample` and the export's quality path stay as this
document describes them, and the preview's pixels stay `draw`'s; the GPU preview path is **not** written, and
"do not replace Cairo with GPU rendering" needs no amendment.

**It landed in S12b** (this document, "S12b" above): the step is 91x cheaper at the editor's grid and 23x at
1600, the export is byte-identical, and the price is the drift that block measures — a fraction of a level on
photo content, 2.14 RMSE against S7's window-vs-CLI comparison on the fixtures' synthetic hard edges, both
inside S7's threshold of 6.

### S14 (2026-09-23, this machine)

The layout stage's numbers. Two of the three were taken by the CLI's own pipeline (the source-choice probe
below, `--release`); the rest are the committed test's (`crates/pixlay/tests/layout.rs`, a debug test build —
the geometry and the decode counts are the same code a release build runs, so these are the *shape* of the
cost rather than its floor).

| what | number |
|---|---|
| the band, at 1100x760 | **139** logical px tall of the window's 760; a candidate cell **128x115**, its thumbnail the largest grid inside **128x96** (a 4:3 candidate 128x96, a 16:9 one 128x72, a 2:3 one 64x96) |
| the canvas, before the candidates land and after | **575** px both times (`tests/layout.rs` asserts the two are equal) — the placeholder is a candidate cell, so the band cannot resize the canvas under it |
| decodes for the eight-photo verification project, on open | **7** — one per distinct file. Before S14's two fixes it was **21**: a request at a 1x1 grid, made before the canvas was allocated at all, plus the canvas being laid out twice because the band grew when the candidates arrived. Both are gone (`EditorWindow::refresh_document`, `layout::placeholder_cell`) |
| the band's **own** decodes | **0** through a layout change, a committed framing change and a resize (`EditorWindow::gallery_decodes`) |
| what a layout change and a resize cost the canvas | **7** decodes across both events for the same eight photos: a layout change moves the sheet's aspect and a resize moves the grid, so the copy at the new edge is cut once — the canvas's own work, and neither event is doubled by the band, whose request goes out beside it at that same edge |
| a candidate's pixels vs `pixlay-render render` of the same document at the same grid | **0.1094** / **0.2269** / **0.1432** across the three candidates of the eight-photo document (worst **0.2269**, threshold 6) |
| the band's rebuild | **74.6 ms** for three candidates at a 128x96 grid (`--release`) |
| **the source choice**, measured as a probe: the canvas's own preview-grade copy (975 px for a 780-px canvas) against the gallery's own thumbnail-sized one | **(a) shared: 0 gallery decodes, 74.6 ms, worst drift 0.083** · **(b) its own: 7 further decodes, 4.0 ms, worst drift 3.42** — **(a) is what shipped** |

- **The source choice is (a), and it is the same question S12b's fidelity ladder names.** A copy *larger* than
  the candidate is a downsampling source, which is what the resampler wants; a copy *at* the candidate's size
  is read at 1:1, where the reduction's own sampling is what the picture shows — which is exactly the 3.42.
  So the hazard "a gallery candidate must not come from a reduction at a large factor" (S12b, above) is
  resolved by naming the canvas's *edge* rather than by reducing a second copy: the factor from that copy to
  a candidate is 7.6, and the drift is **0.083** on the CLI's own pipeline, **0.11-0.23** through the
  window's. `pixlay_imaging::Preview::build_at_source_edge` carries this measurement in its own docs.
- **The window's own criterion is the decode count, and it is now exactly "one per photo".** `decoded_sources`
  is a claim about the decoding *thread*, so the band's share is counted separately
  (`gallery_decodes`): the band never decodes anything once the canvas has built at the same edge, and the
  canvas itself is one decode per distinct file per grid it is asked for.
- **The band's geometry is a design constant, not a measurement of the reference** — there is no reference
  for it — and it is chosen so the canvas keeps the majority of the page: 139 of 760 leaves the sheet 575 px
  tall, and the thumbnail box (128x96) is the largest that does.

### S15e (2026-09-24, `--release`, this machine)

The bitmap boundary's numbers: what the shipped library can ask for, and what the check refuses. The sweep
is `display_region`'s own arithmetic over every template at its largest legal grid, every whole degree and
six photo aspects (0.5, 0.8, 1, 4:3, 1.5, 2.4) — geometry, not pixels; the refusals are the CLI's own runs,
timed.

| what | number |
|---|---|
| the worst single slot bitmap at A0 (14043 px) | **212.8 MP** — `strip-2-2x1g` slot 0 at 45 degrees (a half-canvas slot), whose conversion alone would hold **3.89 GB**; the bound refuses it. The CLI's own run of the same template and angle measures 212,722,225 texels / 3,890,140,370 bytes: the stored zoom differs from the sweep's request and the region is clamped into the displayed photo, so the two numbers are the same case measured twice |
| the worst single slot bitmap at the largest legal grid | **215.8 MP** — the same template at 45 degrees on a 14142x14142 (199 MP) canvas |
| the sum over slots at 45 degrees | **403 MP** of bitmaps for `strip-9-9x1` at A0 — 1.6 GB held at once, about 2.9 GB at the peak with the output surface and one slot's conversion. **Not refused**: the per-bitmap bound is the pixel budget's, the sum is the memory budget's (measured here rather than enforced) |
| the row strip's own bound | `16 * dst_w * min(src_h, 262 * step)` bytes; under **84 MB** for any source inside `MAX_DECODE_EDGE`, so it needs no budget of its own |
| `render --preview-px 20000`, square template | `canvas would be 400000000 pixels; the limit is 200000000`, exit 2, nothing written, **0.17 s** — no decode, no allocation |
| `gesture --grid 20000`, 4:3 project | `canvas would be 300000000 pixels; the limit is 200000000`, exit 2, **0.16 s** |
| `render --long-edge 14043` on `strip-2-2x1g` with one cell at 45 degrees | `slot 0: bitmap needs 212722225 pixels (3890140370 bytes at the conversion peak); the limit is 200000000 pixels`, exit 2, **0.20 s** — and the same document at 2000 px renders |
| the aspect boundary | `NaN`, `0`, `-0`, `-1`, `±inf` and anything outside `0.1..=10.0` are refused as `template aspect ratio is NaN but must be in 0.1..=10.0`, before any multiplication; `0.1` and `10.0` themselves are legal |
| the verification render (`verify.pixlay`, `--long-edge 14043`) | 14043x10532, **ms 6999.5** + **encode_ms 2110.6**, **`peak_rss_mb` 1632.3**, 9,157,639 bytes — inside the S10 spread of 7232/1638 for the same document; `probe` on it reports `passed = true` with 8/8 slot colours exact and 0 foreign pixels on all 12 seams |

## 9. The window (S7), and the stages added after it

The GUI is the fifth consumer of the same document, and what it adds is interaction. Its
contract is what a caller can rely on without looking at a widget:

**Since the 2026-09-22 ruling the window is a sequence of stages** (`docs/2026-09-22-STEPS.md`,
S13–S15), and **S13 landed the first of them**: the picker is the `AdwNavigationView`'s root page and
the editor of S7 is pushed on top of it, so a new window opens on photos rather than on an empty sheet.
**S14 landed the layout stage, and it is a band on the document's page rather than a third page** — a
second `AdwNavigationPage` would have to own a second canvas, and S15's compose controls attach to the
canvas the band sits under. The stage is a moment in the *flow*, not a place in the navigation stack; the
sentences above that said it "will sit between them" meant the flow and are rewritten here. The invariants
above are unchanged by the sequence: still one document, one renderer, one gesture per command. The
library and the gallery are **not** renderers of the document — a candidate thumbnail is `render_rgb8` of
the same drawn document at a smaller size, which S14's own criteria hold it to (§8, "S14").

What the layout stage is, as of S14b (`crates/pixlay/src/layout.rs`, `canvas.rs`), and what a caller may
rely on:

- **The candidates are the layouts with the document's cell count, and only those** (ruling 25, 2026-09-23;
  S14b moved the count from the *photo* count to the *cell* count): `templates::with_slots()`, the one
  function the CLI's `templates --slots` and `Selection::layouts()` are expressed in. Each candidate is a
  **real document**: the editor's own document with `Command::SetTemplate` applied, so a candidate of
  another aspect is drawn at its own shape and the sheet's shape changes with the click. The strip follows
  the layout rather than the photo count because `+` can leave a cell empty: a three-cell document with two
  photos in it is still a three-cell document, and a strip filtered to the photos would offer the layouts of
  a *different* one. A count with no layout shows an empty cell-shaped placeholder instead of a strip.
- **The band is a band on the document's page**: the editor's content is
  `banner · canvas · gallery`, so the canvas keeps the majority of the page and the band is one candidate
  cell tall (measured: 139 logical px of a 760-px window, a 128x96 thumbnail in a 115-px cell). The
  placeholder is **the same widgets as a candidate** — a `GtkToggleButton` with the cell's class and an
  empty thumbnail — so the canvas does not resize when the candidates land from their background build:
  measured 2026-09-23, a shorter placeholder cost the document's eight photos **21** decodes on open and
  the same document costs **7** with it (one per distinct file; the count that remains is the canvas's
  own, and the request made before the canvas was allocated at all — a 1x1 grid — is gone with it,
  `EditorWindow::refresh_document`).
- **The count is a control of the layout, and it reads the number it edits** (ruled 2026-09-23): the label
  is the **cell count alone** — no noun beside it, because the control sits between two buttons and above a
  strip of the very layouts it counts, and what the number counts is the accessible name
  (`Photos in the collage`), which is where HIG `guidelines/accessibility` asks for it. `+` takes the
  layout with one cell more and leaves the new cell **empty**; `−` takes the layout with one cell fewer,
  dropping the last cell whatever it holds. Neither remembers a photo: `Ctrl+Z` is the way a dropped photo
  comes back, which is what makes `+` mean one thing rather than two. Both are insensitive at their bound
  (`MIN_PHOTOS` / `MAX_PHOTOS`, the picker's own floor and ceiling), with the picker's own message if a
  caller asks anyway, and `selection::layout_for` (same aspect → same recipe family → nearest aspect →
  library order) is the one rule that decides *which* layout either one moves to.
- **An empty cell is a control of its own.** The canvas is wrapped in a `GtkOverlay` and each empty cell
  carries a real `GtkButton` with `list-add-symbolic` at the cell's centre (32x32, `osd` + `circular`
  classes, explicit accessible name): clicking it asks for the photo of *that* cell
  (`EditorWindow::choose_photo`), which is the pointer's half of "an empty cell asks for a picture" — the
  keyboard's half is `Return` on the selected cell. The buttons are built once, at construction — one per
  slot, nine is the format's own ceiling — and shown or hidden by `EditorWindow::refresh`, never from a
  draw: showing a widget inside GTK's own traversal leaves it snapshotted before it is allocated (measured
  2026-09-23, "Trying to snapshot GtkButton … without a current allocation"). An occupied cell has no
  button over it, so a drag or a click on a photo is still the framing gesture.
- **Two cells can be exchanged whole** (ruled 2026-09-23): `Command::SwapCells { left, right }` moves the
  [`Cell`], so the photo keeps the framing that made it look right where it was; the keyboard's path is
  `Ctrl+Shift+Left/Right/Up/Down`, which names the neighbour geometrically — `Template::neighbour` is in
  `pixlay-core`, so the canvas and the CLI cannot disagree about which cell is "to the right" — and the
  edge of the sheet answers `None` rather than clamping. The same cell twice and a cell the layout does not
  have are refused by the command itself (`CoreError::SameSlot` / `NoSuchSlot`), so the window and the CLI
  report them the same way.
- **A candidate cell is a `GtkToggleButton`** with an explicit accessible name, so HIG
  `guidelines/accessibility` and `guidelines/pointer-touch` cover it for free (focusable, named, `Space`
  activates it), and the layout the document is on is shown by the app's own highlight — the accent border
  of `style.css`'s `.layout-cell.picked`, beside the platform's checked state.
- **The band's own decodes are zero**: it renders on the canvas's decode worker and names the canvas's own
  preview-grade edge (`Preview::build_at_source_edge`), so its first build is answered by the copies the
  canvas already has. Measured 2026-09-23 (`--release`, this machine, the verification project's three
  candidates at a 128x96 thumbnail grid): the gallery's own builds decode **0** files, the band takes
  **74.6 ms** to rebuild, and its pixels differ from `pixlay-render render` of the same candidate at the
  same grid by **0.11–0.23 RMSE** (threshold 6). Reducing its own thumbnail-sized copies instead would
  decode every photo a second time and differ by up to 3.42 — that measurement is on
  `pixlay_imaging::Preview::build_at_source_edge`.

**S15 landed the compose stage's own controls and the two document-level dialogs**
(`crates/pixlay/src/canvas.rs`, `dialogs.rs`), which is what ruling 18 left of the utility pane: each of
its groups already had a home, and the two that did not — the frame's three settings and the export's three
questions — became dialogs of one shape behind header-bar buttons rather than permanent rows. What a caller
may rely on:

- **The selected cell carries five real GTK controls** — zoom out, zoom in, rotate, replace, clear — in one
  `GtkBox` over the canvas, the same `GtkOverlay` the empty cells' `+` lives in (`canvas::CellControls`;
  ruling 9, so the accessible-name and keyboard checks see them). One family per cell, by construction: a
  cell that holds a photo **and** is selected shows the strip, an empty cell shows its `+`, and a cell that
  is neither shows nothing. The strip is one widget moved to the selection, not nine copies of it.
- **The strip is placed from the cell's own rectangle** through `Placement::to_widget`, the same arithmetic
  that drew the cell, and **it is a row when the cell can hold one and a column when it cannot**: five 32-px
  controls are 176 px long, and the library's narrow panes are 61–122 device px wide at the default window
  (a 1/16 column of the 16:9 sheet measures 61, `strip-9-9x1`'s panes 122 — measured 2026-09-23), so a row
  there would start at the cell's left edge and cover the neighbouring photo, taking its clicks. The
  same five controls stacked need 32 px across and 176 down, which those panes have, so the strip turns.
  A row sits `CONTROL_INSET` (6 px) above the cell's bottom edge and is centred in the cell; a column sits
  at its right edge, 6 px inside. A control is 32x32, past HIG `guidelines/pointer-touch`'s 24x24 floor;
  measured 2026-09-23 in the canvas's own coordinates: the row is **186x34** inside a 3/8 x 3/8 cell
  (182,12–458,219) and the column is **34x186** inside one of `strip-9-9x1`'s panes (60,12–182,563) — both
  inside on every side, which is what the criterion asks of the buttons.
- **Each control is one finished step** (`Gesture::Step`, so it is committed and drawn at the resting grid):
  the zoom pair multiplies the *fitted* zoom — what the user is looking at — by `ZOOM_STEP` = 1.06, the same
  notch the wheel and the `+`/`-` keys use; rotate adds `ROTATE_STEP_DEG` = 15° to the free angle (S11: never
  capped, never reduced) and refits; replace opens the same `GtkFileDialog` the double click opens; clear is
  the window's own `win.clear-cell` — the cell empties, photo *and* framing, which is what `Delete` on the
  canvas does and what `edit --clear` writes (one command, `Command::ClearCell`, since S15: one press is
  one undo step). Every one of those edits has a keyboard path on the same cell (`+`/`-`, `Ctrl`+scroll,
  `Delete`, `Return`), and each is its own undo step.
- **`Frame…` is three rows in the document's own order** (ruling 30, 2026-09-23): gap, radius, colour, over
  `frame{gapRel, radiusRel, color}`, with both lengths typed as per cent of the collage's height (the
  document keeps fractions; `edit --gap` takes them). The rows write **live** — the canvas redraws behind the
  dialog and `Ctrl+Z` is the way back — through `EditorWindow::set_frame`, which is the gesture path a slider
  uses: the command is kept pending while the value moves and committed once it is quiet
  (`COMMIT_QUIET`, 250 ms), so one settled frame is one undo step. Its only button is *Close*: there is
  nothing left to confirm, and a Cancel would be a second undo stack.
- **`Export…` is the export's three questions as rows** — the format (JPEG/PNG), the long edge in pixels
  (`MIN_EXPORT_PX`..`=MAX_EXPORT_PX`), and the file name with the platform's own `GtkFileDialog` as its
  chooser — and its affirmative button starts the same background export the menu's action does
  (`EditorWindow::start_export`), with the same progress bar in the bottom bar and the same toast. The
  format row owns the file's extension, and the chooser's filter follows it.
- **Two questions are answered before an export starts (S15c), and the dialog stays open for both.** A
  path that names one of the document's own photos is refused on the spot — the same rule and the same
  message `render` and `thumb` use (`pixlay_imaging::destination`, asked through
  `EditorWindow::export_destination`), reported as a toast, because HIG `patterns/feedback/dialogs` says
  error dialogs are disruptive and a toast is the right shape for a non-critical error. A file that is
  already there is **confirmed** (`AdwAlertDialog`, Cancel and Replace, Cancel first and the default and
  the close response, the destructive one marked as such — HIG's "Confirmation Dialogs": a destructive
  action is confirmed, and Return is not bound to a destructive affirmative). `export::run` asks the alias
  rule again, because it is the function that reaches the file: no caller can bypass it.
- **The frame is a command since S15**: `Command::SetFrame { frame }`. One edit, one undo step, validated
  like every other command — a length outside `0..=MAX_FRAME_REL`, a translucent backdrop, or a gap that
  empties a cell (the error names the slot) changes nothing — and it is the one writer both `edit`'s three
  flags and the dialog use, so "the CLI and the window produce the same document" holds for the frame too.

What the picker stage is, as of S13c (`crates/pixlay/src/picker.rs`), and what a caller may rely on
without looking at a widget:

- **It lists the session's folder and nothing else.** `XDG_PICTURES_DIR` (or `~/Pictures`) on the first
  map, then whatever the folder chooser last picked — kept for the session, never written to a
  configuration file (ruling 8). The listing is `pixlay_imaging::list_folder`, the same function the
  CLI's `scan` walks with, so the grid and a listing of the same folder cannot disagree about which
  files are photos or in what order.
- **Its shape is the 2026-09-22 ruling's, and S13c built it** (`docs/2026-09-22-STEPS.md`, "the picker, as
  gthumb has it"): three bands, measured off the reference's own window and off this build.
  **(1) The media area takes the vast majority** — the preview pane with the picked list down its right
  edge, both inside one horizontal `GtkPaned`, so the list's height *is* the pane's. **(2) One row of
  thumbnails spans the page's width** below it: a `GtkGridView` that reflows **horizontally** (`GtkListBase`'s
  orientation decides which axis the items flow along) with `min_columns = max_columns = 1`, which makes
  the row single at any height (GTK takes the items per vertical slice to be `height / cell` clamped to
  `[min_columns, max_columns]`) and keeps GTK's own live-cell bound small (`30 x max_columns`). **(3) A
  status bar closes the window** with gthumb's four fields, in its order: `picked / total`, the focused
  photo's own pixels, its file's size (`GLib.format_size`) and the zoom
  (`round(100 x drawn / photo long edge)`, where drawn is the `Contain` fit — 100 % is one image pixel per
  device pixel, as in both references). Measured (this build, 1100x760, 2x screen): the content band above
  the status bar is **686 logical px**, the media area is **552** of them — **80.5 %**, against the
  reference's own 88 % of its band (`860 / 974`) — the strip is **130**, the status bar **24**, and the
  picked list's height and top edge are the pane's to the pixel.
- **A cell is 128 logical px**, the reference's own size (gthumb's `thumbnail-size` default of 256 is
  *device* px — measured off the reference: 250 device = 125 logical on a 2× display), and it is shown as
  picked by a **highlight**, not by the platform's check box: `.picker-cell` / `.picked` in
  `crates/pixlay/src/style.css`, the app's only stylesheet, installed on the display at startup and using
  the theme's `--accent-bg-color` and nothing literal. That is a deliberate deviation from HIG
  `patterns/containers/selection-mode`, recorded in `docs/HIG-REVIEW.md` §3. The cell's own outline is
  `--border-color`, and the theme's per-item padding (`gridview > child { padding: 3px }`, GTK 4.24's base
  stylesheet) is zeroed for this grid alone (`.thumbnail-grid > child`), because it is the difference
  between the strip being 136 px (79.6 % of the band) and 130 (80.5 %) — styling the grid's children is
  what the reference's own stylesheet does too (`gthumb/data/css/style.css:26-42`). **A scrolled list
  inside a `GtkPaned` needs `shrink-end-child`** — with the default the paned's minimum becomes the list's
  *content* width, and the pane beside it loses the space the ruling gives it.
- **The pick is ordered, and the order is the click order.** A `GtkMultiSelection` is a set, so the
  ordered list is the picker's own (`pixlay_core::Selection`, the policy `init --photo` shares): the
  picked list is where that order is visible, re-orderable and truncatable — **and clicking a row switches
  the pane to that photo** (ruling 21) — and `Picker::document` is the one place the pick becomes a
  document. The cell's click *toggles* — the picker claims the gesture, because GTK's own row handling
  replaces a multi-selection on a plain click (`gtklistfactorywidget.c`: `modify = Ctrl held`) — and a pick
  past the cap is refused with a visible report rather than truncated (ruling 3). `Enter` goes through
  GTK's own `list.activate-item` (the grid's `activate` signal) and `Space` through the list item's
  `listitem.select`, both ending in the same toggle; `Ctrl+A` is GTK's `list.select-all` **only** — S13
  bound it a second time, so one press fired twice. Order changes by dragging a row onto another position
  (`GtkDragSource` on the row, `GtkDropTarget` on the list) and by `Ctrl+Up`/`Ctrl+Down` on the focused
  row — a `GtkShortcutController` on the list rather than an application accelerator, because the action
  belongs to the focused row and a global binding would fire it in the editor too. The row's only button is
  the remove at its right end. A rebuild removes the rows one at a time and never calls `remove_all`: the
  list's placeholder is a child of the box, and `remove_all` takes it and forgets it (`gtklistbox.c`), so
  the empty hint would never come back.
- **A tile and the preview are `pixlay_imaging::thumbnail` pixels** — the same function the CLI's `thumb`
  writes to a file. Both are built at the size the widget *is*: a tile at `TILE_SIZE` times the screen's
  scale factor (256 device px on this 2× machine, so a HiDPI screen is sharp without a hard-coded 2x), and
  the pane's photo at **the size it draws** — the `Contain` fit of the pane's device size against the
  photo's own pixels, rounded up to 128 px and capped at `PREVIEW_MAX_PX` = 2048 (2048 because a
  pane-sized decode costs 229 ms at 1024, 593 ms at 2048 and 1112 ms at 3840 on the 3840x2160 display this
  machine has, measured 2026-09-22, `S13 · Ruling`). The photo's own size comes from the reply itself:
  `Thumbnail` carries the decoded `Source`'s width and height, so a photo whose tile is on screen — which
  is every photo the strip can show — is decoded at its fitted size on the first request, and a photo whose
  size is not known yet is decoded at the pane's long edge and re-asked for once the answer arrives (one
  extra decode, once per photo). Measured (S13c, debug profile, 840x552 pane = 1680x1104 device px):
  portrait **1152** px decoded for **1104** drawn, landscape **1792** for **1680**, square **1152** for
  **1104** — against S13b's 1.5× the long edge (2.25× the pixels) whatever the aspect — and the pane's
  pixels against `pixlay-render thumb` at the same size differ by **RMSE 0.0235** over 1536x1152 pixels
  (threshold 6), the 8-bit PNG round trip rather than a second resampler.
- **The pane never paints a tile.** It shows the focused photo's preview at the size it draws, or a
  spinner while that decodes; S13 painted the cell tile and could stay on it, because a repeated
  `(index, preview)` request was dropped as already seen. The request identity is
  `(folder generation, kind, index, device pixels)`, so a resize asks for the size the pane now is, a
  re-focus is served from a bounded per-photo cache, and a folder change invalidates all of it. The stack
  that holds the pane's three states swaps them **without a transition**: measured 2026-09-23, a
  `Crossfade` in flight paints both children at a partial opacity, and a window snapshot showed the strip
  and the list drawn while the pane — holding the right texture — was empty.
- **Tiles are built on one worker thread** (`thumbs.rs`) and cross back as plain bytes through
  `MainContext::invoke`; a folder listing therefore returns before any decode happens. **What is asked for
  is what is on screen**: `bind_tile` and the strip's own horizontal adjustment both end in
  `refresh_visible`, `unbind_tile` drops the request again, and the test for "on screen" is the cell's own
  allocation against the scroller's — because GTK's item manager keeps many more items alive than it shows
  (measured: 257 for a 1000-photo model in a 536x396 viewport; the strip's own bound is `30 x
  max_columns` = 30). Measured (S13c, debug profile): the 14-photo fixture folder asks for **9** tiles and
  a 300-photo folder opens with **9** requests — 9 ms, no decode — with a scroll to its 200th photo costing
  **17** more; the bound the test holds this to is `TILE_REQUEST_MAX` = 64. Decoded tiles and previews are
  cached in memory (64 MB each, LRU, keyed by size as well as by file), which is what makes a
  scrolled-back row instant.
- **The stage has no zoom of its own**: the preview is `Contain`-fitted (ruling 2's "fit and zoom" is the
  photo filling the pane), and magnification is the editor's business. The status bar's zoom percentage is
  a readout of that fit, not a control.
- **The app is dark by default** (ruling 23): `app.rs` sets `Adw.ColorScheme.FORCE_DARK` at startup, as HIG
  `guidelines/ui-styling` recommends for an app that displays rich visual content and as both reference apps
  do. There is no per-app switch (ruling 8 forbids the settings file it would need), and the canvas and the
  export are unaffected — they are document content, not styling. The media area sits on the theme's own
  background: the check is that its backdrop equals a plain widget's (the status bar's) under a forced light
  *and* a forced dark scheme, and that the two differ — the idiom both references copy, not their literal
  `#111`.
- **The chrome follows HIG `patterns/containers/header-bars`** (ruling 24): the folder button in the header's
  **start** slot, the heading (`AdwWindowTitle`, with the folder as its subtitle) in the centre, and a
  **primary menu** plus Next at the end — `[New collage, Open…] · [Choose folder…] · [Keyboard shortcuts,
  About Pixlay]`, the shape both reference apps use. Next is an `AdwButtonContent` (icon plus count, no
  `suggested-action`: this page asks header bars to avoid it), and its label is set on the *content* — S13b
  called `GtkButton::set_label`, which replaces the button's child and destroyed the icon on the first
  update. The editor's header moves Undo/Redo to its start slot for the same reason. `win.choose-folder`
  (the menu's view option) is the picker's action and is enabled only while the picker's stage is on screen.
- **The export form's state lives in the window** (`EditorWindow::set_export_settings` /
  `export_settings`), because ruling 18 removed the pane that used to hold it; S15's `Export…` dialog is
  the rows over that state, and `MIN_EXPORT_PX` / `MAX_EXPORT_PX` (`export.rs`) are its bounds.

- **One document at a time**, edited only through `Command` (`History` in `pixlay-core`): the
  window has no second edit path, and **one gesture is one command**, committed when the
  gesture ends. A gesture is *pending* while it happens (`Editor::begin`), so the canvas shows
  the drag without the undo stack recording forty states; a slider, which has no end signal,
  commits when its value has been quiet for 250 ms.
- **A boundary commits the pending edit, and only then asks** (S15d, PIX-002's ruling of
  2026-09-24). Save, close, `New`, `Open` and export all mean "the document as it is on screen":
  each commits the pending command first — `Editor::save` for the write, `EditorWindow::commit`
  for the window's own boundaries, and the `Frame…` dialog's Close — so nothing that is visible
  can be lost inside the quiet interval. Closing the window, `New` and `Open` then ask the *same*
  Cancel / Discard / Save question (`EditorWindow::ask_to_save`), and `Save` continues the
  boundary only once the file was written; a failed save leaves the document exactly where it
  was. "Dirty" is a comparison rather than a flag (`Editor::is_dirty`: the document against the
  one the file holds), so an edit undone back to the saved state is not unsaved work, and a
  command that changes nothing is not an undo step at all (`History::apply` answers whether it
  was one — the same rule the pending path had, now for every command from every surface).
- **A Save As adopts what it wrote** (S15d, PIX-005/PIX-006). `Project::save_as` returns the
  project it wrote, rebased copy included, and the window adopts the file's own spellings,
  rebasing every state its history holds the same way (`History::rebase`): the memory document
  and the file are one document, so an undo that went back to the old spelling cannot resolve
  the photos against the directory they were moved away from. The rebase is lexical
  (`pixlay_core::normalize_lexical`: `.` dropped, `..` resolved against the component before it,
  no filesystem access, no symlink resolution), so a `..` in the project path, the copy path or
  the source itself still produces a copy that points at the same files.
- **The rotation control is free-angle** (S11): the straightening slider spans `-180..=180` and
  the wheel/keyboard step wraps the angle into that range, because the document accepts any
  finite angle and never reduces it. A gesture fits its own candidate numbers against the same
  visible region the canvas is drawing (`CollageDoc::fit_crop`), so what the user sees while
  dragging is what the renderer will paint.
- **One renderer.** The canvas paints the document with `pixlay_render::draw` into the widget's
  own cairo context — the same call the CLI and the export make. Measured: what the window
  draws and what `pixlay-render render` writes differ by an RMSE of **0.0077** over 307,200
  pixels of a five-photo document (threshold 6), which is glyph antialiasing and the 1 px
  framing rect, not a divergence.
- **The preview grid is the widget's.** The bitmaps are decoded and resampled for the largest
  canvas-aspect grid that fits the canvas pane (`canvas::preferred_grid`), and while a
  just-resized widget waits for its new bitmaps the previous grid is drawn with a uniform
  preview scale — a scale of the whole canvas, in the sense `Target::scale` already has it,
  replaced as soon as the decode lands.
- **A live gesture draws coarse, and the release refines it** (S12). While a gesture is pending the
  canvas asks for `pixlay_imaging::gesture_grid(resting)` — half the resting grid on each edge — and
  a control that produces one *finished* step (`Gesture::Step`: a key press, the zoom spin row) is
  drawn at the resting grid instead, because one frame the user is meant to look at is worth the
  pixels. What the release draws is the resting grid's own render, not an upscaled gesture frame:
  measured, the refined canvas equals the same document drawn in one edit at rest at **RMSE 0**
  (335,808 pixels).
- **The preview keeps what it can reuse** (S12), **and resamples a preview-grade copy** (S12b).
  `pixlay_imaging::Preview` — the same type the CLI's `gesture` probe drives — holds preview-grade
  sources keyed by path, modification time **and target size** (budgeted by `MAX_SOURCE_BYTES`, least
  recently used evicted) and **one bitmap set per grid** (at most two: the resting one and the coarse
  one), carrying over every cell whose cell, source, source identity and template are
  unchanged. A step of a gesture therefore rebuilds one cell and touches no disk at all; the window
  counts the decodes the worker reports (`EditorWindow::decoded_sources`) and the GUI test holds a whole
  drag to zero of them once both grids' copies exist. The copy is a **box average in linear light** to
  `PREVIEW_SOURCE_SCALE` (1.25) times the grid's long edge — the largest value that keeps the measured
  step inside the frame budget — or the photo itself when that is smaller; §4.1 has the shape and §8
  ("S12b") the numbers, including what it costs in fidelity.
- **Background work, one thread each.** Decoding (`decode.rs`), the picker's tiles (`thumbs.rs`) and
  exporting (`export.rs`) run on their own threads and hand plain data back through
  `MainContext::invoke`, because a GTK object may not leave the main thread. Decode requests are coalesced (latest wins) so a drag
  costs one build at a time, and the re-use of what a gesture does not change is
  `pixlay_imaging::Preview`'s (S12, above). An export reports progress, which
  the window shows in a progress bar rather than a modal.
- **The accelerator table is data** (`crates/pixlay/src/app.rs::ACCELERATORS`): the bindings, the
  shortcuts dialog and the HIG test all read it, so they cannot drift. No binding uses
  `Alt+*`, `Super+*` or `Ctrl+Alt+*`, and `F9` left the table with the utility pane (S13);
  `Ctrl+Shift+O` for "Choose a folder of photos" joined it in S13c, when the picker gained its primary menu.
- **Everything user-visible goes through gettext**, domain `pixlay`, source language English
  (`i18n.rs`); `po/POTFILES` lists this crate's sources, `po/pixlay.pot` is committed, and with
  no catalog — a missing, `C` or unknown locale — the msgs come back as the English source
  strings. The locale itself is set by `gtk::init()` (measured: `gettext` returns the msgid
  before it and the translated string after), so no `unsafe` `setlocale` call exists.
- **A missing photo is visible, not silent**: the cell renders white, an `AdwBanner` says how
  many photos are missing and its button selects the first of them, and an export refuses
  (as the CLI does) instead of writing a hole.

What the window does *not* do, by decision: no second renderer, no second document model, no
**parallel** modes over one document (a sequential creation flow is not a mode — 2026-09-22's ruling),
no utility pane (ruling 18: the shell has one custom-drawn widget, the canvas, and every other control
is a stock or libadwaita widget), no per-window state that a saved project does not carry, and no
translation files (S16 adds the language packs).
