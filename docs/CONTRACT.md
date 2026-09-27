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
> The rest of the contract is untouched. The rulings themselves are in `docs/archive/2026-09-22-UX-DIRECTION.md` §6,
> and the steps that carried them out are in `docs/archive/2026-09-22-STEPS.md` (S11 in particular).
>
> **What S20 changes in this file.** The frame's gap stops being an inset and becomes the visible
> distance it is named after (ruled 2026-09-25, ruling 35):
> - **`gapRel` is the distance between two photos, and the same distance stands between the photos and
>   the sheet's edge.** Half of it comes off every side of a cell — so two neighbours are `gapRel`
>   apart — and the sheet's own edge gives up the whole gap, because outside the sheet there is no photo
>   to give up the second half. Before S20 each cell gave up `gapRel/2` and the sheet's border kept
>   `gapRel/2`, so the border measured *half* the number while the seam measured all of it; measured at a
>   4000 px long edge with `--gap 0.04`: the seam 160 px both before and after, the border 80 px before
>   and 160 px after (§8, "S20");
> - **the field's shape, its range and the dialog's rows do not change, and `docVersion` does not
>   bump**: a document written before S20 still loads and still renders, with the gap meaning the new
>   thing. Only `gapRel = 0` renders byte-identically, which is exactly the promise S11's ruling made
>   (§8, "S11"), and a version bump would refuse files that this build reads correctly;
> - a gap of half the canvas height or more now leaves *every* cell with nothing visible, so it is
>   refused per slot naming the slot (`MAX_FRAME_REL` itself stays 1.0 — it governs the radius too,
>   which is clamped at use rather than refused).
>
> The rest of the contract is untouched.

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
> and ruling 17, `docs/archive/2026-09-22-STEPS.md`).
>
> **The shape after S28 (2026-09-26)**: `kept` is new — the cells a layout change took off
> the sheet — declared after `frame` and **skipped while it is empty**, so a project that
> never shrank is byte-identical to what the build before it wrote (measured: `save` of the
> verification project byte-for-byte equal across the step). `docVersion` stays 3, which is
> the policy below: a field that carries a default does not bump it. A document that *does*
> keep a cell is one an earlier build refuses loudly (through `deny_unknown_fields`)
> instead of re-laying out wrongly.

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
- **A photo leaves the collage only when it is deleted** (S28, ruling 43, with the photo
  count ruled 1..=9 by ruling 34). `kept` is the list of cells a layout change could not
  place — `−`, or a template with fewer slots — and they are parked there **whole**: photo,
  framing and order. A later growth (`+`, a template with more slots) places them again
  from the front of the list, so `−` then `+` is the document it was; the CLI's growth of an
  *arrival* (`--add-photo`) appends past them instead, because a photo the user has just
  chosen lands in a cell of its own. An explicit delete — `ClearCell`, a replace (`SetSource`
  with a path), a cut (`MovePhoto`) — writes into `cells` and never into `kept`, which is
  what keeps "kept" and "deleted" two states. `cells.len() + kept.len()` is the document's
  own cell total, on the sheet and off it, and `MAX_SLOTS` bounds it (§2); a cell that was
  already empty when it left the sheet is parked like any other, so the pair is exact for a
  document with holes in its tail too. A kept cell's `source` is resolved when a growth
  places it, not while it waits: it is not drawn, so a file that went away behind it makes
  no white hole — the window's decode reports it the moment the cell is on the sheet again
  (S15h's banner), and `Project::sources` refuses the project from then on like any other.
  The field is `#[serde(default, skip_serializing_if = "Vec::is_empty")]` and `docVersion`
  does not move for it.
- `crop.zoom` is **absolute zoom** (displayed width / slot width), not "a multiple of fill":
  when the photo is swapped the baseline does not move and the framing does not jump focus.
- `rotationDeg` accepts **any finite angle** and is normalized to `(-180, 180]` — the ±45° cap was removed on
  2026-09-22 and the validation is widened by S11 — and every component of `crop.offset` has |offset| ≤ 1 (past that no clamp can get the coverage back).
- **Direction convention**: `crop.rotationDeg` is positive **clockwise on screen** (the sheet's y axis points down,
  cairo's `rotate` in that space is clockwise, and the renderer passes it through as-is).
- **A crop is a request; what gets drawn is its fit** (`CropTransform::fit`, S3). The sheet and the slot never grow, so the
  fit has exactly two levers: `zoom` is raised to the value that covers the visible cell with the photo centred (a larger
  request is kept as it is), and `offset` is pulled back **along each of its own axes** until the photo covers again — a
  pan stops at the frame edge rather than being paid for with magnification, and an axis that still has travel keeps the
  part of the pan the other one's limit leaves it. Since S27 the pull-back is per axis (the horizontal one first, then the
  vertical against it); the rule until then scaled *both* components by one factor, which coupled them — at the covering
  zoom the axis a photo exactly fills has no travel, so any request with a component in it (a pointer drag is never
  axis-aligned) came back at the centre. Measured on `verify.pixlay`'s cell 0, a 2:3 photo in a 4:3 cell (all the travel
  vertical, none horizontal), `edit --slot 0 --offset 0.02,0.2`: **`(0.0000, 0.2000)` since S27, `(0.0000, 0.0000)`
  before** (§8). `rotationDeg` is kept **exactly as asked**:
  since 2026-09-22 the angle is free and the fit never reduces it, so `CLAMP_ZOOM_LIMIT` and
  `CropFit::rotation_limited` are gone (S11) and `CropFit` is the drawn transform alone. The fit is **idempotent**, so
  clamping on an edit and again in `draw` costs nothing. `draw` applies the fit, so no document this build accepts can
  render an uncovered cell; the fit's own boundary is a slot so extreme that covering it needs more than `MAX_ZOOM`, which
  gets the cap (and is what a decoder's memory budget, S4, limits from the other side). Measured, the free angle's worst
  case over the whole library and every photo aspect is **21.7x** — 46x below the cap (§8, "S11").
- **The frame is the canvas decoration** (`CollageDoc::frame`, S11): `gapRel`, `radiusRel` and `color`, all
  with defaults that are what the renderer painted before the field existed (no gap, no radius, white), so
  a project written earlier renders byte-identically. **A new document starts at exactly those defaults**
  (ruled 2026-09-25 in S19): the frame stays opt-in, so the window's `New` and the CLI's `init` write the
  document the field's own defaults describe, and one photo — a legal collage since S19, the one-slot
  `grid-1-1x1` — is framed only when the user asks for a frame
  `docs/completed/2026-09-25-STEPS.md`, `S19 · Result`). Both lengths are **fractions of the canvas height**,
  like every other length the format stores. **The gap is the distance between two photos** (ruled
  2026-09-25, ruling 35; landed in S20): half of it comes off every side of a cell, so two neighbouring
  cells are `gapRel` apart, and the sheet's own edge gives up the whole gap, so the outermost photos
  stand the same distance from the sheet's border — one number, and the same visible stripe between two
  photos and at the border (measured at a 4000 px long edge with `--gap 0.04`: **160 px** in both places,
  §8 "S20"). A template that bakes its own margin (the `*g` gutter layouts, S2) adds that margin to the
  stripe, and that is the template's geometry rather than the frame's. A radius is clamped to half the
  smaller side of the cell's visible rectangle so a large request rounds the corners into a stadium. The
  clamp's coverage reference is the cell's **visible region**: the outline clipped to the visible
  rectangle — the cell's box with half the gap off every side, cut back to the sheet with the whole gap
  off — which is that rectangle exactly for the rectangular slots the library is made of. It does *not*
  subtract the rounded corners — a rounded rectangle's exact support needs circular arcs and the reference
  stays a polygon, so the corner costs a little more zoom than it strictly needs (bounded by the radius,
  and zero at `radiusRel = 0`). The clip is `outline ∩ rounded_rect(visible)`, so a corner shows the
  backdrop rather than a stretched photo.

## 2. Limit constants (all have explicit errors, no panics)

| Item | Value | Source |
|---|---|---|
| `docVersion` | exactly `DOC_VERSION` (currently **3**); higher refused, lower refused too | see "Version policy" |
| slot count | 1..=9 | `AGENTS.md`; nine since S12c removed the ten-slot recipe, one since S19 (ruling 34: a single photo is a legal collage, and `grid-1-1x1` is its layout) |
| cells the document holds | `cells.len() + kept.len()` ≤ 9 (`MAX_SLOTS`) | S28 (ruling 43): a layout change keeps what it cannot place, so the ceiling counts both lists — the cells on the sheet and the ones waiting. Refused by `CollageDoc::validate` with `CellTotalOverLimit`, naming both counts |
| long edge | 1..=30000 px (`MAX_LONG_EDGE_PX`) | a pixel count, the one size parameter: what a render renders and what an export writes |
| canvas pixels | ≤ 200 MP | the largest grid the product has rendered measured 139.5 MP (§8, "S0"); 43% of headroom left. Checked wherever a grid is **asked for** (S15e): the grid a long edge derives, and the scaled grid `render --preview-px` and `gesture --grid` derive from it |
| one slot's bitmap | ≤ 200 MP texels (`MAX_BITMAP_PIXELS`, the same budget at the bitmap boundary — S15e) | a bitmap is the part of the photo the slot can show: the slot's own extent in output pixels plus the axis-aligned box a rotation needs, so it is bounded by the canvas rather than by the zoom. Refused per slot with the slot named and the conversion's peak bytes reported (§8, "S15e") |
| template aspect ratio | 0.1..=10.0 (`MIN_TEMPLATE_ASPECT` / `MAX_TEMPLATE_ASPECT`) | a template outside this range is not a collage layout. Checked where a template is validated **and** where a pixel grid is derived from one (S15e, PIX-027A): a `NaN`, zero, negative or infinite aspect used to reach the rounding and come back as a one-pixel-by-N grid |
| the render grid | long edge exact, the other edge `round` (half away from zero, at least 1 px) — asserted as 4:3 at 4000 → 4000x3000 | the whole grid request, frozen so an export's size does not drift between builds |
| slot outline | ≥ 3 vertices, finite, every vertex inside `[0,1]`, area > 0, and **simple** (no edge meets another except at the vertex consecutive edges share — S15g, PIX-007) | a polygon with no interior is not a slot, and neither is one that crosses itself: the even-odd rule every consumer uses would give it two regions |
| framing rotation | ~~±45°~~ **any finite angle, normalized to `(-180, 180]`** (the cap was removed on 2026-09-22; S11 widens the validation). Clockwise is positive, sheet y points down | the 2026-09-22 ruling, `AGENTS.md` |
| framing zoom | `0 < zoom ≤ 1000` | the upper bound is necessary: zoom determines the size of the decoded bitmap, and without an upper bound it overflows. S4's decoder sets a limit **separately by memory budget**; the two layers each mind their own. The fit raises the drawn zoom to the covering value and never lowers a larger request |
| crop offset | every component \|offset\| ≤ 1 (slot widths / heights) | beyond half a slot the photo centre leaves the slot, and no clamp can cover it again. The fit reduces it further whenever the requested pan would uncover the slot |
| template aspect query | `templates::of_aspect` matches within ≤ 1e-6 (`ASPECT_TOLERANCE`) | the library's own grouping: layouts whose declared ratio agrees with the named one |
| frame gap / radius | both finite, `0 ≤ value ≤ 1.0` (`MAX_FRAME_REL`, fraction of canvas height) | the bound is a typo bound, not a design one: a length past the whole canvas height is not a frame around anything. A gap *inside* the range can still leave a cell with nothing visible, and that is refused per slot by `CollageDoc::validate`, naming the slot. Since S20 a gap of **0.5 or more** leaves *every* cell invisible (the sheet's own band has no interior left), and the bound stays 1.0 because it governs the radius as well — a radius past the cell's half-side is a stadium, not a typo |
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

- `Slot::outline` is a **closed polygon** (the last point connects back to the first), with ≥ 3 vertices, finite, inside `[0,1]`, area > 0, and **simple** (no edge meets another except at the vertex two consecutive ones share — S15g, PIX-007: a bowtie's two halves both answer `contains`, so a slot that crossed itself would claim two regions).
  Polygons only, no curves: S2's review offered "restrict the crop geometry to polygons, or declare a curve discretization tolerance",
  and polygons are what make area, overlap and holes decidable rather than approximate. A path is the outline's command list — **no SVG parser** is involved.
- `Slot::area` is the declared area and is cross-checked against the outline's actual area (tolerance 1e-6). The two are not allowed to drift.
- S2 owns the complete invariants (pairwise zero overlap, no interior hole in the union, cut-type areas summing to exactly 1.0), and
  **since S15g the loader enforces them** (PIX-007, ruled 2026-09-24): a `.pixlay` embeds its own geometry, so a hand-authored or
  script-generated file keeps the right to its own slots and has them checked like the library's. Three refusals, each with the reason:
  two slots that **overlap** (`template slots 0 and 1 overlap at (x, y)`), a region **sealed off from the canvas border**
  (`template slots leave an interior hole at (x, y)` — a gutter is uncovered too and is *not* a hole, because it reaches the border), and
  areas that **cannot fit the canvas** (`template slots declare 1.2 of the canvas; at most 1.0 can be covered`; a sum *below* 1.0 is what a
  gutter layout is). The third is the sum clause of the ruling; the second's complementary identity — that a template covering the canvas
  tiles it with areas summing to exactly 1.0 — needs no separate rule, because no overlap plus full coverage forces it.
  The algorithm (`pixlay_core::topology`) cuts the canvas into vertical slabs at every vertex x and every crossing between two slots'
  edges, and samples one x per slab: inside a slab no vertex and no crossing exists, so the y-intervals a vertical line sees through each
  slot keep their shape and one sample is the whole truth about it. The shipped library's invariants are asserted independently, by a
  `512`-sample raster with a flood fill, in `crates/pixlay-core/tests/templates.rs`; the hand-authored cases there are checked by both.
- **A degenerate covering is not a shape to fit** (S15g, PIX-027B): `CropTransform::fit` returns the request untouched when the region to
  cover has fewer than three vertices **or no interior** (three collinear points, or a repeated one — `Polygon::area()` is 0). Before S15g
  only the vertex count was checked, so a three-point collinear region reached the covering arithmetic and came back magnified or panned.
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
  does not tile its canvas and its areas sum to less than 1.0. S19's one-slot member is `grid-1-1x1`: a 1x1 tiling, the whole
  sheet as one cell, 4:3.
- **Geometry version and document version are separate**: `template.version` follows the template family, `docVersion` follows the format.
- **The library covers every slot count from 1 to 9**: counts 2..=9 carry at least three layouts each in at least two aspect families (S10), and count 1 carries exactly one — `grid-1-1x1`, the whole sheet (S19, ruling 34), because one photo is a legal collage and a second one-slot layout would be the same geometry under another name. The CLI's `templates` reports the matrix and filters it by aspect ratio. `strip-10-10x1` was the only member above nine and left with S12c.
- **The library's range is deeper than one layout** (S10, ruling 10): every count from 2 to 9 carries **at least three
  layouts, in at least two aspect families** — 27 templates and 143 slots in all since S19 added the one-slot
  sheet, which `pixlay-render templates` reports
  and `crates/pixlay-core/tests/templates.rs` asserts as a histogram over `2..=MAX_PHOTOS` plus the
  one-layout rule for count 1.
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
  - **the clip** is `outline ∩ rounded_rect(visible)` per cell: the outline first, then the rectangle the frame leaves visible — the cell's box with half the gap off every side, cut back to the sheet with the whole gap off (S20) — with its corners rounded, which cairo intersects with the current clip. A corner therefore shows the backdrop instead of a
    stretched photo. An identity frame (`gapRel == 0`, `radiusRel == 0`) adds **no** second clip — clipping to a
    superset of the outline would be clipping to something let through — which is what keeps a project written before
    S11 pixel-identical: measured, the S1 golden image is **RMSE 0.0** against the committed PNG, and the S5
    `verify.pixlay` render is byte-identical (§8, "S11" — and S20 re-measured both after the gap's meaning
    changed, since only `gapRel = 0` carries that promise).
- Composite onto an **opaque backdrop, white by default** (`frame.color`): the output is never transparent.
- **Band rendering**: `Band::out_rows()` partitions on **output pixels** (`first = total * index / count`),
  so at any `scale` the band sizes sum to exactly the whole image. It previously partitioned by canvas rows, rounding each band on its own,
  and at 72dpi/scale=0.3 three bands totaled 759 rows while the whole image was 758 rows — `round` is not additive, and this could only be fixed this way.
  Measured, the whole image vs the three-band stitching has RMSE 0.033 (scale 1.0; see below), and scale 0.1/0.3/0.5 was measured too.
  **Banding is a genuinely usable memory-saving measure**: A0 landscape at 300dpi is 1470 MB for the whole image → 597 MB for 16 bands (see §8; measured on the ten-slot strip that shipped at the time, the library's ceiling having been nine since S12c).
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
against ImageMagick's. **That is the whole rule, and it is narrower than a file
can be** (measured, S15i): a source that declares BT.2020 primaries with the PQ
transfer function through CICP alone, and carries no profile, decodes to its own
code values — no PQ decode, no gamut matrix — and a profile whose bytes are not a
profile is ignored the same way rather than refusing the file. Measured
2026-09-25 through this decoder against an independent ffmpeg/zimg BT.2020+PQ →
sRGB conversion: the neutral patches read their code values (`0x80` → 128) where
that conversion reads 24, and the whole render is **RMSE 0.239919** of full scale
away from it. Both are this boundary's documented limitation, and
`crates/pixlay-imaging/tests/decode.rs` is its canary. Source alpha is preserved through the resample
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
the profile id is zero and the creation date is the fixed constant `CREATED`, so
the same document yields the same bytes. The date is a real one rather than a
zeroed field (S15i, PIX-025): ICC 1:2010 §7.2.8 requires the header to record the
profile's creation time and §4.2 defines month as 1..=12 and day as 1..=31, so
zero is a date a strict validator rejects — the sentinel role belongs to the
profile id alone. Measured against the sRGB profile committed in a fixture (lcms2's, via
ImageMagick): the colorants agree to 2.2e-4, the curve parameters to one unit in
the last place, and converting an export from this profile to that one moves the
pixels by 0.0015/255 (§8, "S6").

**Depth.** The decoded buffer keeps the file's own depth (8 or 16 bits per
channel); everything after it is 16-bit — the resampler's output, the flattened
buffer, and the preview-grade reduction as well (S15f, PIX-013) — and the only
quantization is the final 8-bit write.
An 8-bit source is *widened* with `sample * 257`, which is exact, so 8-bit files
do not pay for a 16-bit buffer they cannot fill. The one buffer that *gains* a
depth is the reduction: it is neither the decode nor the final write but an
intermediate between two resampling stages, so a reduced 8-bit photo is stored at
16 bits and the source cache budgets `pixels × 8` bytes for it (`× 4` only for the
copy that is the decoded photo itself, a photo at or below the target, which is no
reduction at all).

**The buffer ladder.** What exists at once, largest first:

| Buffer | Size | Lifetime |
|---|---|---|
| the decoded source | `src_px × 4` bytes (8-bit) or `× 8` (16-bit), capped at 120 MP | one slot |
| the resampler's row strip | `block_rows × dst_w × 16` bytes, block-bounded | one slot |
| one slot's bitmap | `dst_px × 4` bytes | until the render ends |

`Σ dst_px = O(output pixels)`: a bitmap holds the part of the photo the slot can
show, not the whole displayed photo. That is not an optimization but a
requirement — a narrow pane magnifies its photo several times over (6x in the
ten-column strip this was measured on, retired by S12c), so handing over the whole
displayed photo would allocate 3.33 GB of bitmaps for a 110.9 MP canvas **on top
of** the 443 MB output surface, where the region crop measures 1182 MB peak for
the whole render (§8). `decoding is one source at a
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
export** (the window's canvas test; a gallery candidate was the second until S21, when the candidate became
a sketch and left the comparison) must stay inside the RMSE it names.

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
pixlay-render render    --template <name> --sketch --out <file> [--paper r,g,b] [--ink r,g,b] [--stroke <px>]
pixlay-render probe     --project <file.pixlay> --long-edge <px>
pixlay-render image     --photo <file>
pixlay-render scan      --dir <path> [--recursive] [--json]
pixlay-render thumb     --photo <file> --px <n> --out <file> [--region <x>,<y>,<w>,<h>]
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
| escaping (S15h, PIX-018) | every value a line carries is escaped, so a value can never add a field line: `\` → `\\`, newline / CR / TAB → `\n` / `\r` / `\t`, any other byte below 0x20 or the byte 0x7F → `\xNN` (lowercase hex), and any byte that is not part of valid UTF-8 → `\xNN` byte by byte. Bytes of valid UTF-8 pass through unchanged, so an ordinary value's line is byte-identical to what it was. `--json` carries the same escaped string (JSON-escaped on top of it, since `\xNN` is not a JSON escape), so a control byte reaches neither shape. stderr is diagnostics rather than a machine surface and keeps plain text |
| stability | same input, same output; the results carry no timestamps and no absolute paths. `--stats`'s `ms`/`encode_ms`/`peak_rss_mb` are the **only** exception (they are the measurement), and `scan` is the other one **by subject**: a directory listing *is* a set of paths and modification times (S9), so reporting them is the result rather than contamination — two runs over an unchanged directory are still byte-identical, which is what the rule protects |
| locale | under any value of `LANG` / `LC_ALL` / `LANGUAGE`, stdout and stderr are **byte-identical** (including the error branches) |
| the app's settings are not an input (S25) | the GUI remembers its export's format and long edge at `~/.config/pixlay/settings.json` (ruling 39, §9), and the CLI reads **no configuration file at all**: `--long-edge`, `--out` and the rest are a function of the command line alone, whatever that file says. Asserted with the file present and asking for another format and size (`crates/pixlay/tests/settings.rs`) |
| interaction | does not read stdin, does not wait for a prompt, works with no TTY; `--help` covers every flag and every exit code |
| exit codes | 0 success / 1 usage error / 2 project, decode, render or write failure / 2 probe verdict not passed. An `--out` that names one of the document's own photos is a **usage error** (1): the command as written is one this build never runs, and refusing it is cheaper than deciding it after a decode |
| usage error and "failed to produce a result" | stdout stays empty; stderr names the failing path (or the missing flag) |
| a project that does not parse or validate (S15h, PIX-021) | the message names the file it was read from and keeps the reason: `<path>: project JSON: <serde's message>`, `<path>: document version <n> …`, `<path>: <validation reason>`. Only parse and validation failures are wrapped — they are the ones that do not know the file; an I/O failure already carries its path and is not wrapped, so nothing prints `path: path:` |
| probe verdict not passed | **not "failed to produce a result"**: the numbers are the result, so stdout emits all the numbers as usual, with `status = failed` and `passed = false`, stderr emits a one-line summary, and the exit code is 2 |
| probe lower bound | when `occupied = 0` (all empty slots) the verdict is **failed**: every question the probe asks is about some slot, and with no slot there is no conclusion. Previously it "passed vacuously" (status=ok, exit 0) |
| output format | determined by the `--out` extension: `.png` / `.jpg` / `.jpeg`, anything else is a usage error (exit 1, stdout empty, the message names the formats this build writes). **Two formats since S12c** — TIFF left with the purity ruling, so `.tif` is refused like any other unknown extension rather than falling back to PNG |
| the destination may not be a source image (S15c) | `render` and `thumb` refuse an `--out` that names one of the document's own photos (`render`) or the photo being read (`thumb`), before a byte is decoded: `refusing to write <out>: it is the source image <photo>`, exit 1, nothing written. Four spellings are the same file and all four are refused — the literal path, a `..` form, a symbolic link and a hard link — by comparing the **normalized spelling** (lexical, no filesystem access) and the **file identity** (device and inode, which is what only the filesystem knows). `AGENTS.md`'s source images are read-only is the constraint; this is the surface that would have broken it. A *document* write (`init` / `edit` / `save`) is outside the rule: those write `.pixlay` only |
| export size | `--long-edge n` (1..=30000) makes the long edge exactly n pixels and sizes the other edge from the template's aspect rounded half away from zero (at least 1 px). Absent the flag, `render` and `probe` use 4000 (`DEFAULT_LONG_EDGE_PX`) — a square grid of it is 16 MP, an eighth of the 200 MP budget, so the default never touches the limit |
| per-format metadata (S6, resolutions removed by S12d) | PNG: **no `pHYs`**, `iCCP` with the profile (the `sRGB` chunk is **not** written next to it — the specification says the two should not both appear, and the profile is the one carrying the colorimetry). JPEG: JFIF `APP0` with the density unit **0** (square pixels, no resolution — the encoder's default), `APP2` `ICC_PROFILE` segments, and the frame's own sampling factors, which are **4:4:4** since S12c removed the request. There is no third format |
| JPEG quality | **90, fixed** (not a flag): it is the S0–S6 baseline, so every measurement in §8 stays
comparable, and `--quality` was deliberately not added — a knob nobody tests breaks quietly |
| `--preview-px n` | n pixels on the long edge; the same `draw`, only `scale` changes. The **bitmaps are sized for the preview too** (S4): decoding and resampling a full A0 and letting Cairo shrink it would cost the export's time and memory for a thumbnail, and would do the shrinking with Cairo's filter instead of the pipeline's. The scaled grid is checked against the canvas pixel budget before the first decode (S15e, PIX-003). The report's `long_edge` is the edge the file was written at, not the export base the preview's grid was scaled from (S15h, PIX-019) |
| `--sketch` (S21, S29) | draws the **template's geometry** instead of a document: its cells in paper, and every cell's outline plus the sheet's ground no cell covers in ink (S29: a layout whose cells leave a gutter between them draws the gutter as the gap it is, where paper read as one more cell) — `pixlay_render::sketch_rgb8`, the same normalized→pixel path `draw` places photos with. This is what the window's layout band shows for every candidate, so `render --template <n> --sketch` at the band's grid and with the band's own three parameters reproduces a candidate's pixels exactly — `crates/pixlay/tests/layout.rs` holds the two to **RMSE 0**. `--template` is required (`--project`, `--preview-px`, `--gap`/`--radius`/`--border-color` are refused, exit 1: a sketch has no document, no preview and no frame), `--long-edge` sizes it as it sizes a render (the sheet's aspect is the template's, and a sketch's grid has its shape), and `--paper`/`--ink` are `r,g,b` 0..=255 with defaults 255,255,255 / 0,0,0 while `--stroke` is a positive finite width in pixels defaulting to 1. A JPEG export is legal and shows the same two colours: the paper fills the surface, so a sketch is opaque everywhere. Its report is a *sketch* shape: `sketch = true`, `paper`, `ink`, `stroke`, `slots`, `long_edge`, `out_w`, `out_h`, `bytes` — and no `cells` / `occupied` / `gap` / `radius` / `border`, which are a document's |
| `render`'s report | carries `long_edge` — the integer the output was **actually rendered at**, `max(out_w, out_h)` of the written file (S15h, PIX-019), so a preview render reports the preview's edge while `preview_px` stays the request — `cells` and `occupied`, next to the written file's facts |
| `probe`'s report | carries `long_edge` (the integer grid it sampled) instead of a resolution for the same reason |
| `--stats` | appends `{ms, encode_ms, peak_rss_mb, icc}`; `render` emits all four, `probe` emits no `encode_ms` (it does not encode). `icc` is the description of the profile the written file carries (`sRGB IEC61966-2.1`); a command that writes no file reports `none`. The measurement rules are below |
| `probe` | samples and outputs numbers (in-slot photo color, out-of-slot backdrop, shared-edge blended pixels, three-color convex combination residual, and since S20 the frame's own gap: `gap_px` is the width the document's `gapRel` claims at this grid, `seam.N.gap_min_px`/`gap_max_px`/`gap_rows`/`gap_skipped`/`gap_dev_px`/`gap_ok` the stripe across each shared edge (judged against the stripe the two cells' *geometry* leaves on that row, because the frame insets the bounding box and a concave slot's notch keeps its own place), and `border.N.side`/`gap_min_px`/`gap_max_px`/`samples`/`skipped`/`gap_ok` the run from each side of the sheet to the outermost photo that reaches it), exit code 2 when the verdict is not passed. The background field is `bg_off_backdrop` — "off the document's backdrop colour", which is `frame.color` and white unless the document says otherwise (S11; it was `bg_non_white` while the backdrop was hard-coded) |
| `image` | one file's decode facts: `mime`, `width`, `height`, `depth` (8 or 16), `aspect`, `exif_bytes`, `date` (EXIF `DateTimeOriginal`, empty when absent). It is how "HEIC decodes" and "orientation 6 is applied" are visible without rendering a project. `--out`/etc. are usage errors: it decodes at the file's own size and writes nothing |

**S6.5's two subcommands** turn the interaction layer's questions into the machine surface (`AGENTS.md`: nothing may be possible only in the GUI). `hit` answers about geometry without decoding a byte; `save` is the one command that writes a document that already holds a user's work.

| Item | `hit` | `save` |
|---|---|---|
| shape | `template`, `version`, `slots`, `at`, `hit` and `slot` — `slot = <n>` when the point is in a slot, `slot = none` with `hit = false` when it is in a gutter or off the canvas | `template`, `version`, `aspect`, `cells`, `bytes` (the file that was written) |
| source | `--project` (the document's **embedded** geometry, which is what makes a saved project's hit region stable) or `--template` (this build's library), exclusively | `--project`, required |
| `--at <x>,<y>` | normalized canvas coordinates, both components in `0..=1` (the limit table). The same space `probe` prints its slot sample points in, so a probe row feeds straight back in | — |
| what owns a point (S15g) | the slot's **geometry**, and nothing else. The frame's gap and its rounded corners are decoration: the renderer paints the backdrop there, and the hit test still answers with the slot. Both halves are measured in `pixlay-render/tests/hit.rs` — `grid-4-2x2` at `gapRel` 0.04 / `radiusRel` 0.08, a pixel in the gap band and one in a rounded corner are the backdrop's and both hit slot 0. **Ruled 2026-09-24 (PIX-008): the geometry-only hit stays**; a walk that finds a person expects the frame's backdrop to select the cell behind it is a new step after the walk, not a change here | — |
| `--out` | — | required, `.pixlay`, and **replaced** if it exists: that is what saving is, and `init` is the command that refuses to overwrite. The write is atomic (a temporary file in the target's own directory, `sync_all`, `rename`; `pixlay_core::atomic`), so a crash leaves either the old file or the new one — and a file that is already there **keeps its mode**, so a project saved while it is readable only by its owner does not come back world-readable (S15c, PIX-016). A file that is not there yet gets the process's umask default. The path itself is what is replaced: a symbolic link at it is replaced by the regular file rather than followed |
| relative sources | not resolved at all: a project whose photos have moved still answers | rewritten when `--out` lands in another directory, so the copy still finds the photos of the project it was copied from; an absolute source is left as it stands |
| no slot / no file | exit **0**: "no slot owns this point" is an answer, like `templates --aspect 7:5` reporting `count = 0` | a missing `--project`, a refused version or a write failure is exit **2** with stdout empty, and nothing is written |

**Command history has no subcommand.** `History`/`Command` live in `pixlay-core` and the GUI is their only caller: there is no CLI *session* — no undo stack, no interactive editing — and the observable that matters — the pixels after undoing everything — is a *test* (`pixlay-render/tests/history.rs`, `pixlay-cli/tests/history.rs`), which measures it more directly than a verb could. What the CLI carries is the write path (`save`, and `edit` for the framing) those two share: one document in, one document out, no state between runs.

**S2's two subcommands report the template library and create a project.** They add a data source, not a new failure mode:

| Item | `templates` | `init` |
|---|---|---|
| shape | `template.<i>.{name,slots,aspect,version}` plus `count` (and `aspect`, when filtering) | `template`, `version`, `aspect`, `cells`, `bytes` |
| `--aspect` / `--slots` | the only flags it takes (S14 added the second): `--aspect` accepts `W:H` (`4:3`) or a decimal, matched against the template's declared ratio within `ASPECT_TOLERANCE` (the same `templates::of_aspect` query the window's band uses), and `--slots` filters by slot count (`Selection::layouts`, the gallery's query). A ratio or a count nothing was authored for is `count = 0` and exit 0 | — |
| `--template` / `--out` | — | both required; `--out` must end in `.pixlay` |
| refusal | any other flag (`--long-edge`, `--project`, …) is a usage error (exit 1) | same; and an existing `--out` path is a **failure** (exit 2) because `init` never overwrites a project. The refusal is the creation itself (`create_new`), not a check followed by a write: two `init`s that race leave exactly one winner, and a symbolic link at the path — dangling or not — is a file that is already there rather than a name to write through (S15c, PIX-015) |
| unknown template | — | usage error (exit 1), stderr lists the names this build knows |
| content | the whole library in library order (by slot count) | a photo-free project at the template's aspect, written by `CollageDoc::to_json` and loadable by `Project::load`; **with `--photo` the arguments fill the cells in order** (below) |

**S9's two subcommands are the library's machine surface** — "browse a folder" and "show me this photo" — plus the extension of `init` that turns a selection into a document. They were built for the picker stage, which S22 deleted (ruling 31); what keeps them is that a folder listing and a photo preview are questions a *user* asks too (`Add photos…` lists the same folder, and the canvas draws the same resampled photo), so both have a number behind them on the machine surface.

| Item | `scan` | `thumb` |
|---|---|---|
| shape | `dir`, `recursive`, `count`, `failed`, and one `file.<i>` row per photo: `path`, `status`, and either `mime` / `width` / `height` / `date` / `mtime`, or `reason`. `dir` and every `file.<i>.path` are **byte paths** — the path's own OS bytes, escaped by the rule above — so a filename with a newline cannot forge a line and a name that is not UTF-8 survives instead of becoming U+FFFD (S15h, PIX-018) | `format`, `mime`, `src_w`, `src_h`, `region`, `px`, `out_w`, `out_h`, `bytes` |
| what it is for | what a caller needs from a folder, and the key S12's decode cache invalidates on: `mtime`, whole seconds since the Unix epoch | decode plus resample to a preview's size as a CLI number; `--stats` is the budget number S12's decisions were measured against |
| size | `height`/`width` are the size **after EXIF rotation** (`ImageDetails`' early dimensions are a hint and are *not* post-rotation, which is why a full decode happens), so `image` and `scan` cannot disagree about a file | `--px n` is the exact long edge, 1..=**8192**; the other edge keeps the source's ratio (`round`, at least 1 px) — the photo's, or the `--region` rectangle's when one is given. The bound is the product's largest preview with room: a full-window 4K photo preview is 3840 px and a HiDPI one 7680, so past 8192 the caller wants `render --preview-px` |
| candidates | files whose extension is in `PHOTO_EXTENSIONS` (`.jpg .jpeg .png .heic .heif .avif .jxl .webp .tif .tiff` — TIFF is still read even though it is no longer written), case-insensitively; **no recursion unless `--recursive`**, and only real directories are descended into (a symlink to a parent would never terminate). A non-photo extension is neither a row nor an error — the alternative is a folder's README becoming an error row | `--out`'s extension, the same two formats `render` writes (`.png` / `.jpg` / `.jpeg` — S12c removed TIFF) |
| refusal | a file with a photo extension that does not decode **is** a row (`status = failed`) with the decoder's own reason, and the command still exits **0**: the listing is the result. A `--dir` that is not a directory is exit **2** with the path named | a photo that does not decode, or an `--out` this build cannot write, is exit **2**; `--px` outside the range is exit **1**, and an `--out` that *is* `--photo` is exit **1** (the destination row above, asked before the decode). A `--region` is refused in two halves, and which one is which is the point: a malformed one (fewer or more than four numbers, a negative, a fractional, a width or height of 0) is a **usage** error (exit **1**) because the command as written is one this build never runs, while a well-formed rectangle the **photo does not contain** is exit **2** and names the file's own size (`the region 900,700 800x600 is not inside the 1600x1200 photo`) — only the decode knows that, so it cannot be a usage error |
| pixels | — | the whole photo — or, with `--region x,y,w,h`, that rectangle of it, in the photo's own pixels (`pixlay_imaging::Rect`) — resampled once at the preview's own grid: the same `resample` (Lanczos3, linear light, kernel widened by the downscale ratio) and the same `over_white` + quantize as a slot, so a preview is not a second picture of the same file. **A region whose long edge is `--px` is a 1:1 resample** (S15j): the taps degenerate to the identity (`lanczos(0)` is 1, every other tap 0), so the output is that rectangle of the photo pixel for pixel — measured: `thumb --px 1600 --region 200,100,800,600` of the 1600x1200 fixture is byte-identical to the same crop of `thumb --px 1600`. The window's preview pane at 1:1 is this call, which is what makes the pane's pixels a number the CLI can reproduce |

**`init --photo` is where a list of photos becomes a document** (S9), and it goes through `pixlay_core::Selection` — the same policy the window's own entries use (`Add photos…`, an empty cell's `+`, a drop, `pixlay a.jpg b.jpg …`), so "the third photo given is the third cell" has one implementation:

| Item | Rule |
|---|---|
| order | **argument order is cell order**; the source of cell *i* is the *i*-th `--photo` |
| count | 1..=9 inclusive (ruling 3's ceiling, ruling 34's floor: a single photo is a legal collage, and `grid-1-1x1` is its one-slot layout). Outside it: usage error (exit 1) naming both bounds (`a collage needs 1..=9 photos, got 10`). **The CLI never trims**: a list past nine is refused rather than cut to nine, because a machine caller may not have input dropped on its behalf — the window is the surface that trims, with one report of how many photos were not used. Omitting `--photo` entirely is still the photo-free project S2 shipped |
| template | the slot count must equal the number of photos; a mismatch is a usage error (exit 1) naming the template, its slots and the photo count |
| paths | a **photo that is not there** is a failure (exit 2, the path named) — the same rule a project that points at a deleted file follows. Each stored `source` is relative to the project file when the two share a root (`pixlay_core::relative_to`, the function `Project::save_as` rebases with) and absolute otherwise, so a project whose photos sit beside it can be moved. Both sides of that comparison are lexically normalized first (`pixlay_core::normalize_lexical`, S15d), so a `..` in the project path or the copy path cannot produce a relative source that resolves somewhere else |
| the policy itself | `pixlay_core::selection`: `Selection` (ordered photos, the 1..=9 clamp, `layouts()` = the templates with that many slots), `layout_for` (the count rule, S14: same aspect → same recipe family → nearest aspect → library order), `last_photo` / `remove_last` (the batch rule: the last **occupied** cell, because a per-cell clear leaves holes — and the removal is now only a removal, since S14b retired ruling 7's add-back: `+` gives the layout a cell back and leaves it empty). Pure functions, no filesystem |

**S11 added one subcommand (`edit`) and three shared flags**, because the free rotation and the frame are things a *person*
does and a machine has to be able to do too (`AGENTS.md`: nothing may be possible only in the GUI). The flags are the same
three on both commands, and the difference between them is scope:

| Item | Rule |
|---|---|
| `--gap <rel>` / `--radius <rel>` | fractions of the sheet height, `0..=1` (exit 1 outside). On `render` they override the document **for that render only** — the file is not touched — and on `edit` they are written into the document through `Command::SetFrame` (S15), so the CLI's edit is one undo step of the same command the window's `Frame…` dialog sends, and a gap that empties a cell is refused where it is asked for rather than when the file is validated. **On `edit` the frame is applied before the framing flags** (S15f, PIX-009): a crop is stored as its *fit*, and the fit reads the frame's `covering` (the cell's outline clipped to its visible rectangle, S20), so `--gap 0.04 --zoom 1.4 --rotate 20` in one command is fitted against the frame the file will carry. Fitting first and applying the frame afterwards wrote a crop the renderer then refits — the file did not hold the fit it claimed, and the same edit run twice moved the bytes |
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
| what `edit` reports | `template`, `version`, `cells`, `photos`, `kept` (the cells a layout change took off the sheet, S28: `cells + kept` is the number §2's ceiling bounds), the frame's three fields, `bytes`, and — when `--slot` was given — `slot`, `occupied`, `zoom`, `offset`, `rotation_deg` |
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
| `src_w`, `src_h` | the size of the source the **warm** step resampled — since S12b a *preview-grade reduction* (`pixlay_imaging::PreviewSource`), not the file. Before it the field was the photo's own size; against a 6000-px photo the difference (6000 → the copy's own long edge) is what says "the big decode left the step". **The largest of the cells that step rebuilt** (S15f, PIX-027C): a cell carried over from the bitmap cache was not resampled, and with sources of different aspects the pair is one copy's own two edges (compared by area), never a width from one copy beside a height from another |
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
| `templates --slots <n>` | only the templates with exactly `n` slots, `1..=9` (exit 1 outside, and the same bound the format's slot limit gives). This is `Selection::layouts` — the gallery's own query — seen from the outside, so a caller can list a photo count's candidates; the two filters combine with `--aspect`. The report echoes `slots` beside `aspect`. Count 1 answers with `grid-1-1x1` alone (S19) |
| `edit --template <name>` | switches the document to another layout, keeping the surviving cells' photos and framing (`Command::SetTemplate`'s retention). Since S28 the count need not match at all: a layout with fewer slots **keeps** the tail it cannot place and one with more **places those cells again** (photo and framing), then appends empty ones — so a shrink and a growth are inverses on the document, not only on the layout. An unknown name is exit 1 with the library listed |
| `edit --add-cell` | takes the layout with one cell more (`Command::AddCell`, the window's `+`). A cell a layout change kept is placed again first — that is how a kept photo comes back (S28) — otherwise the new cell is empty: an edit *about the layout*, so it moves the count without placing a photo. The cell the user wants filled is the one that shows a `+`, and clicking that is what asks for the file (S14b) |
| `edit --remove-cell` | takes the layout with one cell fewer (`Command::RemoveLastCell`, the window's `−`). The last cell leaves the sheet **whole and kept**, not deleted (S28, ruling 43): it waits in `kept` and a later growth places it again, so a photo leaves the collage only through `--slot <i> --clear`, a replace or a cut. Exit 2 at one cell (`a collage's layout has at least 1 cell`) — the floor is the layout's, not the photo count's, and it is one cell since S19 (ruling 34). The mirror image of `--add-cell`, and refused together with it (exit 1): they are opposites, and one edit is one intent |
| `edit --swap <i>,<j>` | exchanges two cells **whole** — photo and framing both (`Command::SwapCells`), because the framing is what makes a photo look right in *that* cell. Exit 2 for the same cell twice (`slot i cannot be swapped with itself`) and for a cell the layout does not have; a malformed pair is exit 1. The window's own paths are the four ruling 33 gave it (S23, plus S27's marked press): a `Shift`+drag from one cell onto another, a `Shift`+click on another cell, the strip's swap control plus `Return` — or plus a press on the target cell (S27) — and `Ctrl+Shift+Arrow` (S14b), which names the neighbour geometrically (`Template::neighbour`) and stays |
| `edit --add-photo <file>` | appends a photo: the first empty cell, else the layout with one slot more (`Command::AddPhotos`). **An arrival lands in a cell of its own** (S28): the growth appends past the cells a layout change kept rather than placing them, because the user has just chosen that photo. Repeated once per photo in argument order; a photo that is not there is exit 2 with the path named, and a document already holding nine cells' worth — placed plus kept — is exit 2 (`a collage takes at most 9 photos`), with nothing written |
| `edit --slot <i> --photo <file>` | the photo that cell shows instead. Needs `--slot` (exit 1 otherwise, like the framing flags), and the stored path follows `init --photo`'s rule (relative to the project when the two share a root, absolute otherwise). **An arrival that points several cells at once — a drop from the file manager, a paste (S23b, `Command::PlacePhotos`) — is the same document as one of these per cell**; the difference is the history's, not the file's: the window keeps the whole arrival as *one* undo step and reports the files that did not fit once, while a machine caller gets no history and no silent drop at all |
| the order of one `edit` | `--template`, `--add-cell`/`--remove-cell`, `--swap`, `--add-photo`, `--slot`/`--photo`, then the framing — so the framing is fitted against the document the earlier flags produced, and `--swap 0,3 --slot 0 --rotate 10` frames the cell that ends up at index 0. `--clear` is exclusive with `--photo` as well as with the framing flags |
| one implementation | every one of these goes through the same `pixlay_core::Command` the window sends (`crates/pixlay-cli/src/cli.rs::edit_project` applies them to a `History`), so "the CLI and the window produce the same document" is a property of the code rather than of two editors kept in step by hand — asserted in `crates/pixlay/tests/layout.rs` |

Measurement rules (`AGENTS.md`): peak = `/proc/self/status`'s `VmHWM`; time = wall clock, with compositing and encoding reported separately.

`probe`'s threshold constants (the sources are commented in the code):

| Constant | Value | Source |
|---|---|---|
| in-slot sampled color | **exact equality** (no tolerance) | the sample point is the "point farthest from the boundary", far from antialiasing boundaries |
| seam blend cap | ≤ 2 px/row, at most 2 px wide | S0 measured 1.08 px/row, at most 1 px wide |
| three-color convex combination residual cap | 3.0/255 | S0 measured 0.20/255; at 300dpi with eight slots the measured worst was 0.63 |
| gap stripe tolerance | ± 2 px against the stripe the geometry leaves | the stripe is measured as a *pixel span* — the pixels between the last that is exactly one photo's colour and the first that is exactly the other's — so it is the geometric width rounded outward, up to one pixel per end; measured in S20 over the library's six template families at `gapRel` 0.01–0.08, radius 0 and 0.03, grids 709–1417 (42 runs), worst **1.96 px** |

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
| clamp math | **S3, landed**; the angle-reduction half retired and the visible-region reference landed in **S11 (2026-09-22)**; the pull-back made per axis by **S27** | `CropTransform::fit(slot, covering, canvas_aspect, photo_aspect) -> CropFit { transform }`: the angle is never reduced and the coverage reference is the cell's visible region (`Frame::covering`), applied by `draw`; the fit is idempotent and never exceeds `MAX_ZOOM`. `CollageDoc::fitted_crop` / `fit_crop` are the two entry points that pair the frame with the clamp |
| canvas decoration (the frame) | **S11, landed**; its editor is a command since **S15** | `CollageDoc::frame`: `Frame { gapRel, radiusRel, color }`, plus `Frame::covering` / `Frame::clip` and the backdrop + clip stage in `draw`; the CLI's `render --gap/--radius/--border-color` (render-time) and `edit` (§5), and since S15 `Command::SetFrame { frame }` is the one writer both `edit` and the window's `Frame…` dialog send (one undo step, validated per slot). Measured cost at A0: none — the frame is a clip path and a fill (§8, "S11") |
| template generator | **S2, landed** | `pixlay_core::templates` (`generator` recipes + the committed `frozen` data) and the `templates` / `init` subcommands; see §3 and §5 |
| the image pipeline | **S4, landed**; the preview-grade reduction landed in **S12b**; the grading stage removed by **S12c** | `pixlay-imaging`: `Source::decode`, `resample`, `slot_bitmap`/`slot_bitmaps`, `probe`, and the preview's `Preview` caches + `reduce::PreviewSource`; the buffer ladder and the colour decisions are §4.1 |
| command history / hit testing / project writing | **S6.5, landed**; `SetTemplate` added by **S7**; the grade/filter/text commands removed by **S12c**; the multi-cell arrival and the move added by **S23b** | `pixlay-core`: `Command` (one edit: source, framing, or the template; `PlacePhotos` for an arrival that points several cells at once and `MovePhoto` for a cut-then-paste — each one undo step, §9) and `History` (snapshot undo/redo; `apply` is all-or-nothing, answers whether the command was a **step** — one that changes nothing is not (S15d) — and the document has no mutable accessor; the GUI commits **one command per gesture**, §9), `Template::slot_at(point)` for hit testing, `CollageDoc::save` / `Project::save` / `Project::save_as` for writing a document. The CLI's `hit` and `save` are the machine surface of the first and the last; the command history is a test surface only, on purpose (§5) |
| encoding and metadata | **S6, landed**; TIFF and the chroma request removed by **S12c**, resolutions by **S12d** | `pixlay_imaging::encode`: one pass per format writing pixels, sampling and the ICC profile (`icc`), for PNG / JPEG; the CLI's `--long-edge` and the per-format rules are §5, the profile is §4.1 |
| the library and the selection | **S9, landed** | `pixlay_core::selection`: `Selection` (the ordered photo list, the clamp, `layouts()`), `last_photo` / `remove_last` — pure, no filesystem (the batch add-back left with S14b; the clamp's floor is 1 since S19). `pixlay_imaging::thumb`: `thumbnail(source, long_edge)`, the same `resample` at a preview grid. The CLI's `scan` / `thumb` / `init --photo` are the machine surface (the rules are §5) |

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
`docs/archive/2026-09-22-STEPS.md` are where the removals themselves are accounted for.

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

That is the budget number S12 was decided against: a 256 px preview of a 1 MP photo costs
~50 ms and 20 MB in this build, which is the cost a gesture step must not pay per frame.

### S10 (2026-09-22, `--release`, this machine)

The gallery's raw material: 15 new layouts, and not one shipped layout moved. The counts are what
`pixlay-render templates` reports; the histogram is (photos → layouts, aspect families).

| Item | Value |
|---|---|
| the library | **27 templates, 152 slots** (S9: 12 and 64), `count = 27`, exit 0 |
| the histogram, 2..=9 | 2 → **3**/3 · 3 → **3**/3 · 4 → **4**/3 · 5 → **3**/3 · 6 → **4**/4 · 7 → **3**/3 · 8 → **3**/3 · 9 → **3**/3 — no count below three layouts, none in a single aspect family |
| ten | `strip-10-10x1` alone, unchanged and never offered: the library's ceiling is 9 (ruling 3) |
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
  selection's own pair of constants, so a change to the ceiling moves the assertion with it instead of
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
| a pan beside a clamped axis (**S27**) | `verify.pixlay`'s cell 0 is the case by construction — a 2:3 photo in a 4:3 cell, so the covering zoom is 1.0 and **all** of the travel is vertical: `edit --slot 0 --offset <x,y>` stores `(0, 0.02)` → `(0.0000, 0.0200)` and `(0.02, 0)` → `(0.0000, 0.0000)` both before and after (those are the two single-axis requests), while **`(0.02, 0.02)` → `(0.0000, 0.0000)` before, `(0.0000, 0.0200)` after** and **`(0.02, 0.2)` → `(0.0000, 0.0000)` before, `(0.0000, 0.2000)` after**: the horizontal component lands on its limit (this cell has none) and the vertical one is no longer paid for it. The same shape through a **real pointer** (`gtk4-broadwayd` + a browser, 2026-09-26): with the old rule a diagonal drag of 60x40 device px asked for an offset of `(0.1402, 0.0702)` and the document held `(0.0012, 0.0006)`; with the per-axis rule the photo's vertical pan is what moves (`/var/tmp/pixlay-s7/s27-drag-before.png` and `…-after.png` are the same drag, looked at) |
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
  gallery candidate — S14) must come from the photo, as `thumb` does, not from a
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
  targets keeps its own entry. Both are far inside the 2.5 GB budget of `AGENTS.md`. **S15f re-measured the
  same shape after the reductions became 16-bit (PIX-013): 241.3 MB at grid 780 and 419.8 MB at 1600, with
  the copies' own byte counts in §8, "S15f"** — the copies doubled and the peak rose by a fraction of that,
  still six times inside the budget.

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
  16-bit inverse-transfer table (`linear_to_srgb16`, 128 KB, built once). S12b reduced *at the source's own
  depth*; **S15f made every reduction 16-bit** (PIX-013: a reduction is an intermediate between two
  resampling stages, and the only quantization belongs at the final write), which is what the copy sizes in
  §8, "S15f" are about.

### What S12's number says about the preview's future
Read on the plan's own subject — the verification project, at the editor's own grid — the pipeline **holds**:
5.5 ms against a 16.7 ms frame. Read on a realistic photo — 12 to 24 MP, which is what the product's users
pick — it does **not**: ~200 ms per step, and the coarse grid and the caches between them only bought a factor
of ~2.4 (the same cell cost `decode` 171 ms + `resample` 289 ms before them). That is the fork ruling 1
(2026-09-22) reserved, and it was ruled the same day:
**the preview's future is a preview-grade source, not a GPU renderer** — a cached, preview-sized reduction per
photo that the preview's bitmaps are resampled from, inside the one renderer (`docs/archive/2026-09-22-STEPS.md`,
"S12 · Result" and the step "S12b"). So `draw`, `resample` and the export's quality path stay as this
document describes them, and the preview's pixels stay `draw`'s; the GPU preview path is **not** written, and
"do not replace Cairo with GPU rendering" needs no amendment.

**It landed in S12b** (this document, "S12b" above): the step is 91x cheaper at the editor's grid and 23x at
1600, the export is byte-identical, and the price is the drift that block measures — a fraction of a level on
photo content, 2.14 RMSE against S7's window-vs-CLI comparison on the fixtures' synthetic hard edges, both
inside S7's threshold of 6.

### S21 (2026-09-26, this machine)

The layout band's sketch, and the switch re-measured against S18's ruler. The rows marked *test* are the
committed tests' own (`crates/pixlay/tests/layout.rs`, a debug test build: the drawing and the decode counts
are the same code a release build runs, so they are the *shape* of the cost); the rest are `--release` runs
on a quiet machine, three consecutive each where a row says so. **S14's section stood here until this step**
— candidate renders, the shared preview-grade copies and the source choice — and its seven live
measurements moved to the archive with a note saying what replaced them
(`docs/archive/2026-09-22-STEPS.md`, "S14 · Result").

| what | number |
|---|---|
| a candidate vs `pixlay-render render --template <n> --sketch` at the same grid and the same three parameters (*test*) | **RMSE 0** across the eight-photo document's three candidates — not "below a threshold": both sides call `sketch_rgb8`, and the PNG the CLI writes round-trips through the decoder bit-exactly |
| the band's own decodes (*test*) | **0**, measured where the canvas cannot contribute (`grid-4-2x2` → `grid-4-2x2g`, two 1:1 four-cell layouts, so the click keeps the canvas's grid and the whole delta is the band's) — and structural: `decode::GalleryJob` carries templates and a style, no path at all |
| the band, at 1100x760 (*test*) | **120** logical px tall of the window's 760 (was 139); a candidate cell **128x96** (was 128x115 — the caption left with ruling 40), and the canvas is **594** px where it was 575 |
| the two colours the band draws with (*test*) | paper **255,255,255** · ink **29,29,32** — the dark style's `@view_fg_color` / `@view_bg_color`, read back from `style.css` through `GtkWidget::color()`; **S30 made the ink a dimmed tone** (108,108,110 in the dark style) and its row below carries today's values |
| a sketch's own cost | **0.12–0.14 ms** per candidate at the band's grid (`render --sketch --stats`, a 128-px sheet) |
| the band's rebuild, `--band` | **0.14–0.18 ms** for the three candidates, three runs (S18's same row: 169.6–172.5 ms) |
| the click `mosaic-8-s14` → `strip-8-8x1`, CLI | **125.5 / 126.3 / 127.9 ms** — `template_ms` 0.014, `sources_ms` 82.9–85.7, `composite_ms` 42.2–42.6, **7 decodes** (S18: 123.5–124.8) |
| the click `mosaic-8-s14` → `grid-8-4x2`, CLI | **176.7 / 177.5 / 181.9 ms** — `sources_ms` 96.3–102.4, `composite_ms` 79.4–80.6, **7 decodes** (S18: 176.9–179.0) |
| the same two clicks in the window, release | **147.5 ms** and **201.4 ms** to the canvas's frame (S18: 149–151 and 204–207), each followed by **20.6 ms** to the band's frame — one display frame, because the build itself is 0.15 ms on the worker. One run each (the window ruler opens a window per row; the CLI rows above are three) |
| the whole turn, fresh session | **168.1 ms** and **222.0 ms** (S18: 294–374 ms) |
| the **same session's second click** | the canvas **29.9 ms with 0 decodes** (S18: 115.3–120.9): the band no longer goes through the canvas's `Preview`, so no candidate build evicts its bitmaps — S18's expectation, confirmed |
| peak `VmHWM` | the CLI **64.7–65.8 MB** for the eight-photo document (S18: 63.8–65.6) |

- **The canvas half is unchanged, and that is the point of re-measuring**: `switch_ms` 125.5–127.9 / 176.7–181.9
  against S18's 123.5–124.8 / 176.9–179.0 is the same number within the run-to-run spread, so the sketch did
  not touch the click's own work — it removed everything the band used to add to it (169.6–172.5 ms of
  rebuild, and the eviction that made a repeat click rebuild every bitmap). A click's whole turn went from
  294–374 ms to 168–222 ms, and the second click of a session from 115–121 ms to 30 ms.
- **What is left of the band's row in the window ruler is one frame, not a build.** The build is 0.15 ms on
  the worker, so the wait for its reply returns immediately and the number is the display frame that draws
  it — the same 20.6 ms in all three rows, which is what "the band costs nothing" looks like when measured
  with a frame clock. The CLI's `--band` (0.14–0.18 ms) is the build alone.
- **The band's geometry is still a design constant, not a measurement of the reference** — there is no
  reference for it — and it is chosen so the canvas keeps the majority of the page: 120 of 760 leaves the
  sheet 594 px tall, and the thumbnail box (128x96) is the largest that does. The caption's 19 px went back
  to the canvas.
- **The band's own decodes are a structural claim now, not a counter.** S14 measured them with
  `EditorWindow::gallery_decodes`; S21 deleted the counter with the machinery it counted — the band's job
  cannot name a file — and what `tests/layout.rs` asserts is that `decoded_sources`, the one counter of
  decoded files, does not move across a band rebuild.

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

### S15f (2026-09-24, `--release`, this machine)

The preview-identity numbers: what a cached bitmap is keyed on now, what a 16-bit reduction costs the source
cache, what the picker's own pane did when a listed file was another file, and the defect the step's own test
found on the way. The identity rows are the committed tests' own assertions; the memory rows are
`pixlay-render gesture --project nine.pixlay --grid 780|1600 --steps 6 --stats` on nine **6000x4000** photos
(`strip-9-9x1`, the same shape S12b's criterion names), and the last two rows measured the picker's pane — the
stage S22 deleted; the rule they measured (a file's own stamp is its identity) is `pixlay-imaging`'s and
survives in `thumb` and the CLI.

| what | number |
|---|---|
| a frame **gap** change (0 → 0.04, `strip-2-2x1g`, grid 800) | every cell is **rebuilt** — `source_px` non-zero and the pixels differ — and **0 files are decoded**: the copies are the file's, not the frame's. A **colour**-only change and a **radius**-only change are carried over (`source_px` 0x0, pixels bit-identical): the gap is the geometry the fit reads, the radius is the renderer's clip and the colour is painted after the slots |
| one grid asked at two **source edges** (grid 800, edges 400 and 1200) | each edge costs **1 decode** (two cells, one file) and reports its own copy (400x300 / 1200x900); asking the first edge again is answered from its own set with **0 decodes**, and the fine build's pixels equal a cold fine build's |
| a preview-grade copy at **16 bits** | the S12b dimensions doubled in bytes: **5.07 MB** for a 24 MP photo at the editor's grid (975x650) and **21.3 MB** at 1600 (2000x1333), against 2.5 / 10.7 MB as 8-bit. Nine of them are 45.6 MB and 192 MB — both inside `MAX_SOURCE_BYTES` (512 MB), so the two targets stay cached together |
| the source cache's peak, nine 24 MP photos, two grids | **`peak_rss_mb` 241.3** at grid 780 and **419.8** at 1600 — the same quantity S12b measured on its own probe (217.2 / 316.5). It is not a controlled before/after: this document is the step's own nine 6000x4000 JPEGs on `strip-9-9x1`, and what moved is the copies' second byte |
| the warm step on that document | **1.92 ms** median at grid 780 and **7.35 ms** at 1600 (`--release`, 6 steps, `warm_max_ms` 2.33 / 9.41), both inside the 16.667 ms budget; `refine_decodes` 0 |
| a file **replaced in place**, in the picker's pane (square.png → landscape.jpg, same name) | the pane re-decodes at the new photo's fitted size — **512x512 → 512x288** — and matches `pixlay-render thumb` of the new file at **RMSE 0.0000**; the cell's tile snapshot moves by **RMSE 111.43**; a second `refresh_pane` with the file untouched costs **0 further requests** |
| the **stale unbind** the step's test found (before the fix, in the picker's strip) | after a folder change: `bind_tile(0)` then the *previous* binding's `unbind_tile(0)` left **1 request, 0 built, 0 in flight, no failure and no bound cell**, and **2831 frames** of pumping changed nothing. The guard — only the row that is the position's current binding may take its entry away — is 3 lines |
| the verification render (`verify.pixlay`, `--long-edge 14043`) | 14043x10532, **ms 8150.6** + **encode_ms 2343.9**, **`peak_rss_mb` 1629.6**, 9,157,639 bytes; `probe` on the same document reports `passed = true` with 8/8 slot colours exact and 0 foreign pixels on all 12 seams |

### S18 (2026-09-25, `--release`, this machine)

The layout switch's own numbers: the human's finding 1 of 2026-09-25 ("switching a layout in the editor takes
too long before the new preview is on screen"), turned into a measurement **before** any code changed, so
that a ruling can be made about it. Two rulers, one experiment: `pixlay-render switch --project <p>
--template <t> --canvas <w>x<h> [--band]` drives the click in a windowless process (`SetTemplate` through a
`History`, both grids from the canvas box — `pixlay_core::canvas_grid` — the preview-grade copies, the cell
bitmaps), and `crates/pixlay/tests/switch.rs` drives the same click in the window and times it to the frame
that shows the new render. Both at the editor's default window, whose canvas widget is **1100x575**, and both
in a **fresh session** — the CLI starts a process, the test opens a window — because the caches are the
session's; each row is three consecutive runs on a quiet machine.

| what | number |
|---|---|
| the click `mosaic-8-s14` → `strip-8-8x1` (grid 735x551 → 980x551) | CLI **123.5 / 124.6 / 124.8 ms** — `template_ms` 0.013–0.014, `sources_ms` 80.1–82.6, `composite_ms` 42.2–43.4 — and the window **149.0 / 151.0 / 151.1 ms** |
| the click `mosaic-8-s14` → `grid-8-4x2` (735x551 → 827x551) | CLI **176.9 / 176.9 / 179.0 ms** — `sources_ms` 97.6–100.2, `composite_ms` 78.9–79.2 — and the window **204.1 / 206.7 / 204.0 ms** |
| the two rulers against each other | the window is the CLI **+26 / +27 ms**, the two rows within 1 ms of each other: one 16.6 ms display frame plus the window's own blit and paint, which a windowless command cannot reach |
| decodes per click | **7** — every distinct file, both rows and both rulers: a layout change moves the grid's shape and with it the preview-grade edge (1.25 x 735 = 919 → 1225 / 1034), and the copies are keyed by the edge (S15f, PIX-004) |
| a switch that *keeps* the edge | **0 decodes** (`crates/pixlay-cli/tests/cli.rs`: a two-slot project at a tall canvas box, 276x207 → 276x184, both at edge 345) — the reason `sources_ms` is a phase of its own |
| the same session's **second** click (strip → mosaic after the row above) | the window **115.3 / 120.0 / 120.9 ms** with **0 decodes**: the copies are still in hand, and every bitmap is rebuilt — the band's candidate builds share the canvas's `Preview`, whose bitmap cache is `MAX_GRIDS = 2`, so a candidate's grid evicts the canvas's. Recorded, not fixed: S18 does not optimise |
| the band's rebuild after the same click | CLI **169.6–172.5 ms** for the three candidates (`--band`); window **165.8–178.2 ms** — about as much as the preview it follows, on the same worker. The band's **own** decodes are **0** in every row: S14's claim holds through a switch |
| peak `VmHWM` | the CLI **63.8–65.6 MB** for the eight-photo document at this grid, the whole process including the seven decodes and both caches |
| the **budget** | `SWITCH_BUDGET_MS = 210` — the worse row's worst run (206.7 ms) rounded up to the next 10 ms. It governs the canvas half (`switch_ms`), which is what the finding is about; the band's share is reported beside it. It is a regression line, not a tolerance: S21 re-measures the switch against it, and **100 ms** — the usual instant-response threshold — is a number this baseline does not meet |

- **The cost is not the layout change; it is the copies.** `template_ms` is 0.01–0.02 ms — a `SetTemplate` and
  a grid derivation — while `sources_ms` is 80–100 ms of it: seven decodes plus their reductions, paid because
  the new grid asks for a **new edge**. `composite_ms` (42–79 ms) is the resample and quantization of the eight
  cells at the new grid, and it follows the *target's* cell shapes: the same canvas from the strip to the mosaic
  costs 95–113 ms where the mosaic to the strip costs 42–43 ms.
- **The band costs about as much as the preview it follows**, and it is serialized behind it on the same worker:
  a click's whole turn is 294–374 ms. That is finding 1 in one number, and S21's sketch is the step it points at.
- **The switch is one-shot per session in the CLI by construction.** A window that clicks back and forth pays the
  composite every time (the eviction row above) but not the decodes; a fresh process pays the decodes because its
  source cache is empty. Both are real, and the two rows of the table are the fresh one.
- **Ruling (2026-09-25, human): the measured switch stands as the baseline, and no optimisation step is added.**
  `SWITCH_BUDGET_MS = 210` is therefore a regression line and not a provisional number: the switch is re-measured
  against it after S21's sketch band, which is expected to remove both the band's own rebuild and the eviction above
  — an expectation to re-measure, not a measurement. The two cheaper optimisations the ruling was offered and did not
  take (a second preview-grade edge held in the source cache; a `Preview` of the band's own) are recorded in
  `docs/completed/2026-09-25-STEPS.md`, `S18 · Result`. **S21 re-measured it (2026-09-26): the numbers are the "S21"
  section below**, and what it found about the eviction is the reason the second optimisation is moot.

### S20 (2026-09-26, `--release`, this machine)

The gap's own meaning, measured as pixels. The ruler is `pixlay-render probe`'s new gap rows (the stripes across
every shared edge and the run from each side of the sheet to the outermost photo that reaches it); "after" is this
build, and "before" is the same document against the geometry S19 committed (read with this step's ruler, which is
new — the *blend* row below was measured against the S19 binary itself, which could report it).

| what | number |
|---|---|
| the criterion's document (`grid-4-2x2`: a square library sheet, so 4000 px tall at `--long-edge 4000`), `--gap 0.04` | `gap_px = 160.000000`; **before**: the seam **160 px**, every border **80 px** (`passed = false`, exit 2); **after**: the seam **160 px** and all four borders **160 px** (`passed = true`, exit 0) |
| the plan's own sketch of the old meaning ("320 px inside and 160 px at the border") | 2x the measured values, and recorded here so nobody re-derives it: the code already took *half* the gap off every side, so the old seam was the number and only the border was half of it. The step's code was written against the measurement, not the sketch |
| the library's six template families, 42 runs (`gapRel` 0.01/0.02/0.04/0.08, radius 0 and 0.03, grids 709/1000/1417, `grid-1-1x1` … `grid-9-3x3`) | every stripe within **1.96 px** of what the geometry leaves; a border stripe never below its number and at most **0.96 px** above it — the ±2 px tolerance's source, and the reason it is 2 rather than 1: the stripe is measured as a pixel span, so a fractional boundary adds up to a pixel at each end |
| the concave slot (`mosaic-8-s14`'s L, any gap) | the notch's two seams measure the *neighbour's half* alone (43 px against a frame number of 85 at `--gap 0.08`) — the frame insets a cell's bounding box, so an interior edge of the outline keeps its own place. The probe judges each row against the geometry there, so this is reported and passes; it is the template's geometry, not a defect, and S20 did not change it |
| the same L, `--gap 0.01`–`0.08` at 709–1417, framed | `probe` still reports **2 of 12 seams unclean**, exactly as it did before S20: the *blend* criterion (S0) sees the notch's two blend bands inside its ±3 px window. Pre-existing (the criterion predates the frame) and unchanged by this step; the numbers it is quoted from (S0: 1.08 px of blend per seam px, §5's `probe` threshold table) are untouched |
| the interior sample at a gap (S20's one ruler regression, fixed here) | the sample was the point farthest from the *outline's* boundary, so at a large gap a cell on the sheet's edge could be sampled inside the frame's band and read the backdrop (measured: `slot.3.match = false` on `mosaic-8-s14` at `--gap 0.08`). It is now the point farthest from the *visible* region's boundary, and the same document is 8 of 8 |
| byte-identity at `gapRel = 0` | the S1 golden image is still **RMSE 0.0** (`pixlay-render/tests/render.rs`), and the `AGENTS.md` verification render is the same **9,157,639 bytes** as S19's: the sheet's own band is zero-width at gap 0, so nothing about the identity frame moved |
| the fit's floor with a frame | unchanged property, moved reference: the framed sweep (36,480 framings) is green, and the floor is now measured about the *slot's* centre against the visible rectangle, which since S20 can sit off-centre in its cell (the sheet's band takes a whole gap off the outer side and half off the inner ones) |

### S28 (2026-09-26, `--release`, this machine)

| what | number |
|---|---|
| a project that never shrank (`pixlay-render save` of the verification project) | **byte-identical** to the file the build before the step wrote (`cmp`): `kept` is `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, so the key is absent and nothing else moved |
| the verification render (`render --project … --long-edge 14043 --stats`, JPEG) | **byte-identical** (`cmp`) — a kept cell has no slot and is not drawn, so the step moves no pixel |
| the retention through the CLI (a four-photo `mosaic-4-hero` → `--template mosaic-3-hero` → `--add-cell`) | `cells` 4 → 3 → 4, `photos` 4 → 3 → 4, `kept` 0 → 1 → 0, and the grown document equals the original cell for cell (`crates/pixlay-cli/tests/cli.rs`) |
| a switch that places a kept cell again (`switch --project <three cells + 1 kept> --template mosaic-4-hero`) | `slots` 4, `occupied` **4** — the ruler's own source list follows the new document (before this step it would have built the returned cell empty) |
| the document's cell total | `cells + kept` ≤ 9, refused by `validate` with both counts named; the sweep over the whole library (`crates/pixlay-core/tests/history.rs`) never sees it shrink under any layout change |

### S29 (2026-09-26, `--release`, this machine)

The sketch's ground: what the rule "the ink is everything a cell is not" moved, measured against the build
before the step (both binaries at HEAD of their own commit, `cmp` over every template at four long edges).

| what | number |
|---|---|
| every template's sketch, before vs after | **25 of 27 byte-identical** at 96, 128, 512 and 1000 px — every layout whose cells tile the sheet; the two that changed are its two guttered ones, `grid-4-2x2g` and `strip-2-2x1g`, at all four grids |
| `grid-4-2x2g` at the band's 128-px grid, probe `(64,64)` the gutter's crossing / `(30,30)` a cell's middle | before **255,255,255 · 255,255,255** (the gutter read as a cell: the human's finding); after **0,0,0 · 255,255,255**. The sheet's own edge stays ink in both |
| row 20 of `grid-4-2x2g` vs its tiling twin `grid-4-2x2`, the ink outside the sheet's own border (*test*) | **9** px — the 1/16-canvas gutter's ground plus the cell's own 1-px line — against **1**: the line alone |
| a sketch's own cost (`render --sketch --stats`, a 128-px sheet, five runs) | **0.115–0.159 ms** (S21: 0.12–0.14) — the extra fill is one more pass over the same path; `peak_rss_mb` **10.0–10.4** |
| the band as the window draws it (`crates/pixlay/tests/layout.rs`) | `/var/tmp/pixlay-s7/layout-band-gutter.png` — the four 4-cell candidates, `grid-4-2x2g`'s gutter visibly wider than `grid-4-2x2`'s line |

### S30 (2026-09-26, `--release`, this machine)

The band's ink, and the surface it is drawn on: what the tone is, against the tiles a candidate lives in.
The colour rows are `crates/pixlay/tests/layout.rs`'s own (it reads the band's two colours and the theme's
two through its probes, under `ForceDark` then `ForceLight`); the tile rows are the window's pixels, sampled
from a snapshot with the guttered document open.

| what | number |
|---|---|
| the band's two colours, dark style (*test*) | paper **255,255,255** (the theme's foreground) · ink **108,108,110** — `color-mix(in srgb, @view_fg_color 35%, @view_bg_color)`, where it was **29,29,32** (`@view_bg_color`, finding 6's defect) |
| the band's two colours, light style (*test*) | paper **0,0,6** (the foreground in a light theme) · ink **178,178,180**, strictly between it and the ground **255,255,255** |
| the band's tiles, which the ink has to sit with | dark style **56,56,60** unselected and **100,100,103** checked; light style **230,230,231** and **190,190,192** — so a checked candidate's ink and its tile are 10 levels apart in the dark style, and the gap reads as the tile showing through, which is what the reference draws |
| the candidates measured before 35% | `@view_bg_color` 29,29,32 (the defect) · `@headerbar_bg_color` 46,46,50 · `@dialog_bg_color` 54,54,58 — surfaces of other parts of the window, all still darker than the band's own tile — and a 50% mix 142,142,144, lighter than both tile states and too light for the outlines |
| `color-mix()` in this toolkit | resolved by GTK 4.24 in both schemes (the theme's own `--border-color` is one), so the ink is derived from two theme variables and no literal is named |

### S16 (2026-09-26, this machine)

The package, measured where it can be measured without root (the step's own record is
`docs/completed/2026-09-25-STEPS.md`, `S16 · Result`):

| | |
|---|---|
| `makepkg` on this machine (CachyOS, Arch-compatible) | **1 m 30 s** wall (14 m 40 s CPU) from a `git archive` tarball in `SRCDEST`; the release build inside it is the profile every other number on this page is measured in |
| the package | `pixlay-0.1.0-1-x86_64.pkg.tar.zst` **3,727,778 bytes**, 26 entries, and makepkg's packaging check is silent — the `references to $srcdir` warning the `debug = 1` release profile earns is remapped away in `prepare()` |
| the payload, run from an extracted copy (`pacman -U` is root) | the installed `pixlay-render render` on the verification project at a 2000 px long edge: **478,359 bytes**, `peak_rss_mb` **62.9**; the installed `pixlay`, given a photo on its command line, stays up for ten seconds on a private headless `mutter` |
| the `.pixlay` type | `xdg-mime query filetype x.pixlay` answers **`application/x-pixlay`**; GIO's `standard::icon` for it is **`org.yangtse.Pixlay`** plus the `-symbolic` variants, so the registration's `<icon name>` is honoured |
| the translation template | `po/extract-pot` writes **111** messages: the shell's Rust strings plus the desktop entry's and the metainfo's, in one `po/pixlay.pot` |
| the entry's render (unchanged by this step) | 14043x10532, **ms 5133.1 + encode_ms 1217.5**, `peak_rss_mb` **1633.4**, **9,157,670 bytes**, `cmp`-identical to the S30 render — packaging moves no pixel of a document |

**Not measured here**: the clean-chroot `makepkg`, and two things recorded beside it as skipped have since
been done on this machine from the same PKGBUILD (2026-09-26): the package built — **4,130,536 bytes** against
the table's 3,727,778, a build artifact and not a source change, and not chased further — and, until S31
removed it, a `check()` that ran the whole suite. The install is the project's own now (§10), and a package
build runs no tests.

## 9. The window (S7), and the shell ruling 31 re-cut (S22)

The GUI is the fifth consumer of the same document, and what it adds is interaction. Its
contract is what a caller can rely on without looking at a widget:

**Since S22 the window is one page** (ruling 31 of 2026-09-25, re-cutting the main path): the editor is the
application, there is no picker stage and no `AdwNavigationView`, and the shell is
`AdwToastOverlay → AdwToolbarView` — the header bar and the progress bar as the view's bars, and
`banner · canvas · layout band` as its content (the band is a box under the canvas, S14's design). The
window opens on the default document — `grid-1-1x1`, one empty cell (S19, ruling 34) — and **photos enter
from outside it**:

- `Add photos…` (`Ctrl+I`, `win.add-photos`) opens the platform's multi-file chooser and sends one
  `Command::AddPhotos`, so a longer list is trimmed once with one report (below) and the whole arrival is
  one undo step;
- an empty cell's own `+`, and `Return` on the selected empty cell, ask for that cell's photo
  (`GtkFileDialog::open`); the selected cell's `Replace` does the same for an occupied one;
- a **drop** from the file manager (`GtkDropTarget`, the canvas's own) hands the paths to
  `EditorWindow::drop_files`, which places them by the rule below — the cell under the pointer
  first, then the empty cells in reading order — as **one** `Command::PlacePhotos`, so the whole
  arrival is one undo step;
- the **clipboard** (`Ctrl+X` / `Ctrl+C` / `Ctrl+V`, `win.cut` / `win.copy` / `win.paste`, the menu's
  Edit group) works on the selected cell's photo; the rule is below;
- `Open…` (`Ctrl+O`) reads a `.pixlay` project;
- **`pixlay a.jpg b.jpg …`** — the application carries `HANDLES_OPEN`, so a command line's arguments arrive
  as the `open` signal and `pixlay::app::open_files` adds them in argument order through the same
  `add_photos` the chooser's callback calls. (A `.pixlay` on the command line is not special: the argument
  list is photos, which is what `AGENTS.md`'s entry sentence says.)

The invariants are unchanged by the re-cut: one document, one renderer, one gesture per command. The
library and the band are **not** renderers of the document — **since S21 a candidate is a sketch of its
template's geometry**, drawn by `pixlay_render::sketch_rgb8`, which the CLI's `render --sketch` is the
machine surface of and which §8's "S21" measures.

What the layout band is, as of S14b (`crates/pixlay/src/layout.rs`, `canvas.rs`), and what a caller may
rely on:

- **The candidates are the layouts with the document's cell count, and only those** (ruling 25, 2026-09-23;
  S14b moved the count from the *photo* count to the *cell* count): `templates::with_slots()`, the one
  function the CLI's `templates --slots` and `Selection::layouts()` are expressed in. Each candidate is a
  **sketch of that template's geometry** (S21, ruling 32): its cells in paper, every cell's outline and the
  sheet's ground no cell covers in ink (S29, so a layout whose cells leave a gutter between them draws it as
  the gap it is), drawn by `pixlay_render::sketch_rgb8` at the largest grid inside the band's own `CANDIDATE_BOX`
  — so a candidate of another aspect is drawn at its own shape and the sheet's shape changes with the
  click, and a candidate is a complete account of a template, which carries geometry and no style. The
  strip follows the layout rather than the photo count because `+` can leave a cell empty: a three-cell
  document with two photos in it is still a three-cell document, and a strip filtered to the photos would
  offer the layouts of a *different* one. A count with no layout shows an empty cell-shaped placeholder
  instead of a strip.
- **A candidate is drawn in the theme's own two colours, and says its position and nothing else** (S21):
  the paper and the ink come from `style.css`'s `.sketch-paper` / `.sketch-ink` classes, read back through
  `GtkWidget::color()` on two invisible probes — a candidate is interface, not content, so neither colour
  is a constant and both follow the theme and its high-contrast variant. **Since S30 the ink is a dimmed
  tone** (`color-mix(in srgb, @view_fg_color 35%, @view_bg_color)`, measured in §8's "S30"), because it is
  the ground a candidate is drawn on as well as its outlines: the band's own tiles are raised surfaces, and
  the ink has to sit with them the way the reference's tile shows through a gap. The cell carries **no caption**:
  a template's name is machine identity (`edit --template`, `templates`, the document's own embedded copy)
  and never text a user reads (ruling 40), so what a screen reader announces is the position
  (`Layout 3 of 5`) and the widget's own name — the template's — is for callers, not for people.
- **The band is a band on the document's page**: the editor's content is
  `banner · canvas · gallery`, so the canvas keeps the majority of the page and the band is one candidate
  cell tall (a 128x96 sketch in a cell of its own; the measured height is §8's "S21"). The
  placeholder is **the same widgets as a candidate** — a `GtkToggleButton` with the cell's class and no
  picture — so the canvas does not resize when the candidates land from their background build:
  measured 2026-09-23, a shorter placeholder cost the document's eight photos **21** decodes on open and
  the same document costs **7** with it (one per distinct file; the count that remains is the canvas's
  own, and the request made before the canvas was allocated at all — a 1x1 grid — is gone with it,
  `EditorWindow::refresh_document`). Since S21 a candidate has no caption, so the placeholder has none
  either: the two are the same height because both are the box.
- **The count is a control of the layout, and it reads the number it edits** (ruled 2026-09-23): the label
  is the **cell count alone** — no noun beside it, because the control sits between two buttons and above a
  strip of the very layouts it counts, and what the number counts is the accessible name
  (`Photos in the collage`), which is where HIG `guidelines/accessibility` asks for it. `+` takes the
  layout with one cell more; `−` takes the layout with one cell fewer. **Since S28 (ruling 43) a photo
  leaves the collage only when it is deleted**: the cell `−` takes off the sheet is *kept* — photo,
  framing and order — and the next growth places it again, so `−` then `+` is the document it was. `+`
  therefore means one thing still: it edits the layout, and a kept cell coming back is what "one more
  cell" means while one is waiting; with nothing waiting, the new cell is empty. A kept photo has no
  cell, so it cannot be selected, replaced or deleted — **it returns** — and the `+`'s own tooltip names
  it (and is its accessible name) while it waits, which is what makes it findable. A growth whose
  candidate is a kept cell never reaches the ceiling: it places a cell the sheet gave up rather than
  appending one. Both controls are insensitive at their bound (`MIN_PHOTOS` / `MAX_PHOTOS`, the format's
  own floor and ceiling, S19), with the refusal's own message if a caller asks anyway, and
  `selection::layout_for` (same aspect → same recipe family → nearest aspect → library order) is the one
  rule that decides *which* layout either one moves to — so a `−`/`+` pair restores the layout too
  wherever the counts in between share an aspect, and the *cells* (photo and framing) in every case.
  One report says what a shrink kept, once per change and not once per cell.
- **A list of photos longer than the ceiling is trimmed once, with one report** (S19, ruling 34): the
  window is the surface that trims, and each of its list paths trims to what *it* can use —
  `Add photos…` to the room the ceiling leaves (`MAX_PHOTOS` less the photos on the sheet **and the kept
  cells**, S28: a kept cell is one of the nine before it is a cell again), because `Command::AddPhotos`
  is all-or-nothing and grows the layout one cell at a time, and a **drop or a paste** to the room the
  document has, one report naming everything that did not land (below). Core never truncates:
  `Command::AddPhotos` and `Selection::new` still refuse past the cap, which is what the CLI's
  `init --photo` and `edit --add-photo` report (exit 1 and exit 2).
- **A drop, and a paste, land where they are aimed** (S23b, ruling 41 of 2026-09-25). The first file takes
  the cell the user aimed at — filled when it is empty, **replaced** when it holds a photo — and the files
  after it fill the empty cells from there on, in reading order and wrapping around the sheet. A file that
  finds no empty cell is ignored, and the count of everything that did not land (past `MAX_PHOTOS` *or*
  past the room the sheet has) is **one toast**: a drop never quietly replaces a cell it did not land on,
  which is the whole point of the rule. The aim is the cell under the pointer for a drop
  (`GtkDropTarget`'s hit test, `Template::slot_at` — geometry, so the frame's gap and rounded corners are
  not part of the answer, §5) and the selected cell for a paste; a drop with no cell under it (the
  canvas's own margin) falls back to the selection, the first empty cell and finally the first cell, so it
  still lands somewhere visible. The whole arrival is one `Command::PlacePhotos { places }` — the same
  document as one `edit --slot <i> --photo <file>` per cell — so it is **one undo step**, and the framing
  of a replaced cell is kept (`SetSource`'s rule: what a photo arrives in re-fits at draw).
- **The clipboard works on the selected cell's photo** (S23b, ruling 41). `Ctrl+C` puts the photo's path
  on the clipboard as a **file list** (`text/uri-list`, GTK's own `GdkFileList`), so the photo can be
  pasted into another application as easily as into another cell, and a file manager's own
  `text/uri-list` arrives in the same shape. `Ctrl+V` reads it back:
  - a **file list** is placed exactly as a drop aimed at the selected cell would be (the rule above, one
    `Command::PlacePhotos`, one report);
  - an **image with no file behind it** — a texture another application copied — is written out as a real
    PNG first, because a document references *paths* and a cell whose source is not a file is a cell that
    cannot be reopened. It lands in the app's own cache, `$XDG_CACHE_HOME/pixlay/pasted/`
    (`~/.cache/pixlay/pasted/`): one rule for a document with a project directory and one without, and it
    leaves the user's own tree alone. The name is the digest of the bytes (FNV-1a, 64 bit), so pasting the
    same image twice is one file, and the bytes are compared rather than the hash trusted;
  - **`Ctrl+X` remembers the cell** it cut from, and the paste that follows *that photo* is one
    `Command::MovePhoto { from, to }`: the photo arrives in the target (which keeps its own framing) and
    the cell it came from comes out whole — no photo and the default framing, which is the document
    `edit --slot <i> --clear` writes — in **one** undo step. The pair is checked when the paste happens
    (the cell still holds that source, the clipboard still holds that file), so an edit between the two, a
    replaced clip or a layout that took the cell away makes the paste a placement rather than a move. A
    cut on its own changes nothing: the edit happens where the paste lands;
  - **sensitivity** is the state the window can really act in: copy and cut need a selected cell whose
    photo is a file that is there, and paste needs a selected cell and a clipboard holding something a
    cell can take (a file list, or an image). The paste items are insensitive with nothing selected and
    with a clipboard that holds neither.
- **An empty cell is a control of its own.** The canvas is wrapped in a `GtkOverlay` and each empty cell
  carries a real `GtkButton` with `list-add-symbolic` at the cell's centre (32x32, `osd` + `circular`
  classes, explicit accessible name): clicking it asks for the photo of *that* cell
  (`EditorWindow::choose_photo`), which is the pointer's half of "an empty cell asks for a picture" — the
  keyboard's half is `Return` on the selected cell. The buttons are built once, at construction — one per
  slot, nine is the format's own ceiling — and shown or hidden by `EditorWindow::refresh`, never from a
  draw: showing a widget inside GTK's own traversal leaves it snapshotted before it is allocated (measured
  2026-09-23, "Trying to snapshot GtkButton … without a current allocation"). An occupied cell has no
  button over it, so a drag or a click on a photo is still the framing gesture.
- **Two cells can be exchanged whole** (ruled 2026-09-23; the pointer and keyboard paths landed in S23,
  ruling 33 of 2026-09-25): `Command::SwapCells { left, right }` moves the [`Cell`], so the photo keeps the
  framing that made it look right where it was — and a cell the photo arrives in for the first time re-fits
  *at draw*, because `draw` asks `doc.fitted_crop` per slot; the stored crop stays the fit it was. One
  command, so one undo step from every path. **The paths are**:

  - **`Shift`+drag from one cell onto another** — the cell under the press is the swap's source, the cell
    under the release is its target, and the target is **filled** while the pointer is over it: a highlight
    under the pointer is the promise that this is where the release lands. A release outside every cell, or
    on the source itself, changes nothing. `Esc` cancels a drag in flight. The plain drag inside a cell is
    still the framing's pan (S7), which is why the swap drag takes the modifier: one gesture with two
    branches, so "a plain drag still pans" is a property of the code rather than of GTK's arbitration.
  - **`Shift`+click on another cell** — the one-press form. The source is the marked cell, or the selection
    with none marked; the click's own cell is what gets selected, because that is where the photo the user
    moved now is. A press that becomes a drag is the drag instead: the swap happens on the release, never
    on the press.
  - **The strip's swap control plus `Return`** — the keyboard's path. The control is a **toggle**: checking
    it marks the selected cell (drawn as a *dashed* outline around it in the same accent — the selection's
    own mark, dashed, S24 — and drawn *instead of* the solid one when the marked cell is the selected one,
    so the dashes are visible), the arrows move the selection to the other cell, and `Return` exchanges
    them; `Esc` takes the mark off and
    touches nothing. The canvas's accessible name carries the state while the mark is up
    (`Collage canvas, cell 3 of 8, swapping with cell 1`), so the sequence is audible.
  - **The marked swap + a press** — the control's own pointer ending (S27). With a swap marked, a plain
    press on *another* cell **means the exchange**, and that press's **release** does it: one undo step, no
    modifier — which is what a user who pressed the control and then clicked a cell expects (finding 4 of
    the walk of 2026-09-26: the mark had no pointer ending at all, so the control read as doing nothing).
    It is deferred to the release exactly as `Shift`+click is, so a press that becomes a drag is still the
    framing's own drag. A press on the marked cell itself is not a swap — two cells are what a swap is — so
    it takes the mark back and selects; the release's own cell is what gets selected, because that is where
    the photo now is.
  - **`Ctrl+Shift+Arrow`** (S14b) names the neighbour geometrically — `Template::neighbour` is in
    `pixlay-core`, so the canvas and the CLI cannot disagree about which cell is "to the right" — and the
    edge of the sheet answers `None` rather than clamping.

  The same cell twice and a cell the layout does not have are refused by the command itself
  (`CoreError::SameSlot` / `NoSuchSlot`), so the window and the CLI report them the same way, and the mark
  is spent by every swap the window applies (a refusal included), so no stale source survives a layout
  change. `tests/swap.rs` is the machine walk: every path, the document, the pixels (the swapped
  document's own render, RMSE 0) and one undo step each — and since S27 it drives the canvas's own
  `GtkGestureClick`/`GtkGestureDrag` for the pointer's two halves instead of the window methods behind
  them.
- **The selected cell is marked in the theme's accent** (S24; finding 6 of the human's pass of 2026-09-25,
  "the selected cell is not distinguishable enough"): the canvas strokes the selected slot's *own outline*
  (`slot_path`), 2 device px wide, in `canvas::accent()` — `Adw.StyleManager:accent-color-rgba`, the
  *system* accent, read from the toolkit rather than named in Rust, and the very colour `style.css`'s
  `.layout-cell.picked` gives the band's chosen cell, so the app says "this is the one that is chosen" in
  one colour. Measured 2026-09-26: the accent and the stylesheet's `--accent-bg-color` resolve to
  `#3584e4` in the dark and the light style, and `tests/selection.rs` reads both halves — the colour, and
  the mark's pixels over the verification project's own photos. The mark is drawn **over** the document:
  it is not part of `draw`, so the export and the CLI have no mark, and a canvas with nothing selected has
  no accent pixel at all.
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

**S15 landed the compose stage's own controls and one of its two document-level dialogs**
(`crates/pixlay/src/canvas.rs`, `dialogs.rs`), which is what ruling 18 left of the utility pane: each of
its groups already had a home, and the two that did not — the frame's three settings and the export's three
questions — became dialogs of one shape behind header-bar buttons rather than permanent rows. **S25 removed
the export's** (ruling 36): its two parameters moved into the app's own settings surface and its one dialog
became the platform's. What a caller may rely on:

- **The selected cell carries six real GTK controls** — zoom out, zoom in, rotate, replace, swap, clear — in one
  `GtkBox` over the canvas, the same `GtkOverlay` the empty cells' `+` lives in (`canvas::CellControls`;
  ruling 9, so the accessible-name and keyboard checks see them). One family per cell, by construction: a
  cell that holds a photo **and** is selected shows the strip, an empty cell shows its `+`, and a cell that
  is neither shows nothing. The strip is one widget moved to the selection, not nine copies of it. The swap
  control is the strip's only `GtkToggleButton` (S23): its checked state is the swap's mark, written from
  the window rather than held by the button, so `Esc`, a `Shift`+click and a swap that happened cannot leave
  it checked with nothing marked.
- **The strip is placed from the cell's own rectangle** through `Placement::to_widget`, the same arithmetic
  that drew the cell, and **it is a row when the cell can hold one and a column when it cannot**: six 32-px
  controls are 212 px long, and the library's narrow panes are 61–122 device px wide at the default window
  (a 1/16 column of the 16:9 sheet measures 61, `strip-9-9x1`'s panes 122 — measured 2026-09-23), so a row
  there would start at the cell's left edge and cover the neighbouring photo, taking its clicks. The
  same six controls stacked need 32 px across and 212 down, which those panes have, so the strip turns.
  A row sits `CONTROL_INSET` (6 px) above the cell's bottom edge and is centred in the cell; a column sits
  at its right edge, 6 px inside. A control is 32x32, past HIG `guidelines/pointer-touch`'s 24x24 floor;
  measured 2026-09-26 with the swap control in place (S23, six controls instead of five) in the canvas's own
  coordinates: the row is **224x34** inside a 3/8 x 3/8 cell (170,12–455,226) and the column is **34x224**
  inside one of `strip-9-9x1`'s panes (44,12–170,582) — both inside on every side, which is what the
  criterion asks of the buttons. (S15's five-control measurements were 186x34 inside 182,12–458,219 and
  34x186 inside 60,12–182,563: the strip is one control longer now, and this run's sheet is a little larger
  than that one's.)
- **Each control is one finished step** (`Gesture::Step`, so it is committed and drawn at the resting grid):
  the zoom pair multiplies the *fitted* zoom — what the user is looking at — by `ZOOM_STEP` = 1.06, the same
  notch the wheel and the `+`/`-` keys use; rotate adds `ROTATE_STEP_DEG` = 15° to the free angle (S11: never
  capped, never reduced) and refits; replace opens the same `GtkFileDialog` the double click opens; clear is
  the window's own `win.clear-cell` — the cell empties, photo *and* framing, which is what `Delete` on the
  canvas does and what `edit --clear` writes (one command, `Command::ClearCell`, since S15: one press is
  one undo step). Every one of those edits has a keyboard path on the same cell (`+`/`-`, `Ctrl`+scroll,
  `Delete`, `Return`), and each is its own undo step. **The swap control is not one of those** (S23): it
  edits nothing — it marks the cell, and the edit is the `Command::SwapCells` the `Return` on the target
  applies. That is why the interaction has a state at all, and why the mark is a toggle rather than a
  fourth one-press button.
- **The arrow keys choose a cell, and the selection follows** (S15h, PIX-017's ruling of 2026-09-24). The
  focus *is* the selection — the one cell every other control acts on — and it is visible (the canvas
  outlines it) and announced: the canvas's accessible name is `Collage canvas, cell <n> of <cells>`, and a
  screen reader hears it move. `EditorWindow::focus_step` is the model: the arrow keys step geometrically
  (`Template::neighbour`, the function `Ctrl+Shift+Arrow` and `edit --swap` already name a neighbour with),
  and the edge of the sheet does nothing rather than wrapping. With nothing focused the first arrow picks
  the first cell — which is what makes the main path walkable with the keyboard alone, since before it a
  cell could only be chosen with a pointer. The framing nudges the arrows used to be kept their jobs under
  a modifier: `Shift`+arrow pans, `Ctrl`+arrow pans coarsely, `Ctrl+Shift`+arrow swaps two cells.
- **`Frame…` is three rows in the document's own order** (ruling 30, 2026-09-23): gap, radius, colour, over
  `frame{gapRel, radiusRel, color}`, with both lengths typed as per cent of the collage's height (the
  document keeps fractions; `edit --gap` takes them). The rows write **live** — the canvas redraws behind the
  dialog and `Ctrl+Z` is the way back — through `EditorWindow::set_frame`, which is the gesture path a slider
  uses: the command is kept pending while the value moves and committed once it is quiet
  (`COMMIT_QUIET`, 250 ms), so one settled frame is one undo step. Its only button is *Close*: there is
  nothing left to confirm, and a Cancel would be a second undo stack.
- **A value the document refuses is reported, and the rows go back** (S15h, PIX-020). The rows offer
  0–100 %, and a gap of 100 % leaves every cell of the library's layouts with nothing visible — as does
  every gap from 50 % up, since the sheet's own band leaves no interior then (S20):
  `EditorWindow::set_frame` returns the `CoreError` (which names the slot) instead of swallowing it, the
  dialog shows it in its own `AdwBanner` — a toast would be behind the modal — and re-seeds the three rows
  from the document, so a row can never display a number the document does not hold. Nothing is left
  pending and no commit is scheduled, so the delayed commit that used to write a frame the row no longer
  showed cannot happen.
- **The export is one dialog, and it is the platform's own** (S25, ruling 36): pressing `Export…` (or
  `Ctrl+E`, or the header bar's button — all three are `win.export`) calls `GtkFileDialog::save` with the
  settings' format's filter and the seed `pixlay::export::seed` builds — the folder the last export used
  (`lastExportDir`), the pictures directory (`XDG_PICTURES_DIR` or `~/Pictures`,
  `pixlay::export::default_folder`) when there is none, and the name this document suggests — and the path
  it answers goes through `EditorWindow::export_to_chosen`: the extension rule, the source-image rule, the
  folder remembered in the settings, and then the same background export the menu's action has always
  started (`EditorWindow::start_export`), with the same progress bar in the bottom bar and the same toast.
  **The replace confirmation is the platform's own** — the dialog asks before it returns a path that names a
  file that is there, and the app adds no second question, which is why S15c's `AdwAlertDialog` is gone.
- **The settings are the export's two parameters, and they are remembered** (S25, rulings 36 and 39): a file
  at `~/.config/pixlay/settings.json` under `XDG_CONFIG_HOME` (`crates/pixlay/src/settings.rs`) carrying
  `format` (the encoder's own name: `png` / `jpeg`), `longEdge` (`MIN_EXPORT_PX`..`=MAX_EXPORT_PX`, the
  bounds the surface's row offers) and `lastExportDir` (left out until the first export of the account). It
  is read **once**, when a window is built; written through `pixlay_core::atomic::write_atomic` (S15c)
  whenever a row moves or an export remembers its folder; and **never read by the CLI** — `--long-edge` and
  `--out` stay a function of the CLI's own command line (§5). A missing file, an unreadable one and one this
  build cannot parse are all the defaults and never an error the user sees; a field this build does not know
  is ignored rather than refused (a settings file is not a document), and a `longEdge` outside the range is
  clamped. The surface is `AdwPreferencesDialog` — `EditorWindow::show_settings`, the menu's *Preferences*
  item, and `app.settings` on `Ctrl+,` (HIG `reference/keyboard`) — with one page, one group and two rows:
  the format, and the long edge in pixels with the unit in its accessible name. A row writes as it moves;
  there is nothing to confirm and no *Save* button.
- **The name's extension decides the format** (S25c, the human's ruling of 2026-09-26; the CLI's `--out`
  rule, applied to the GUI). The settings' format is the *default*: it is what the save dialog's filter
  offers and what the suggested name carries. A name the dialog returns with any other extension this build
  writes is written in **that** format instead — `export::format_for` is the one function
  (`Format::from_path`'s rule: both JPEG spellings, any case) — so with JPEG in the settings a `.png` name
  is a PNG at the settings' long edge, and the export does not rewrite the settings' row. A name with no
  extension, or with one this build does not write, is refused with a toast carrying the CLI's own message
  (`<path>: expected .png, .jpg or .jpeg`), and `export::run` asks the same question again because it is the
  function that reaches the file. `export::Request` therefore has no `format` field: a request cannot name a
  format its file would lie about.
- **One question is answered before an export starts (S15c)**: a path that names one of the document's own
  photos is refused on the spot — the same rule and the same message `render` and `thumb` use
  (`pixlay_imaging::destination`, asked through `EditorWindow::export_destination`), reported as a toast,
  because HIG `patterns/feedback/dialogs` says error dialogs are disruptive and a toast is the right shape
  for a non-critical error. `export::run` asks that rule again, because it is the function that reaches the
  file: no caller can bypass it.
- **The frame is a command since S15**: `Command::SetFrame { frame }`. One edit, one undo step, validated
  like every other command — a length outside `0..=MAX_FRAME_REL`, a translucent backdrop, or a gap that
  empties a cell (the error names the slot) changes nothing — and it is the one writer both `edit`'s three
  flags and the dialog use, so "the CLI and the window produce the same document" holds for the frame too.

- **The app is dark by default** (ruling 23): `app.rs` sets `Adw.ColorScheme.FORCE_DARK` at startup, as HIG
  `guidelines/ui-styling` recommends for an app that displays rich visual content and as both reference apps
  do. There is no per-app switch (ruling 8 forbids the settings file it would need; ruling 39 of 2026-09-25
  allows one for the export's settings and not for this), and the canvas and the export are unaffected —
  they are document content, not styling. The sheet's frame is the theme's and the sheet's own pixels are
  not: `tests/hig.rs::check_colour_schemes` asserts both under a forced light *and* a forced dark scheme.
- **The chrome follows HIG `patterns/containers/header-bars`** (ruling 24, re-cut by S22 and S26): the
  window's one header bar holds the way into the application at the **start** — `Add photos…`, the
  leftmost control before undo and redo (S26, ruling 42: the window opens on an empty cell, and the
  control that fills it is the one entry besides the menu item and `Ctrl+I`) — with a spacer and the
  frame's settings after them, the heading in the **centre** (`AdwWindowTitle`, the document's name with
  its dirty marker — the same string the window's own title carries) and a **primary menu** with the
  export button at the **end**. The menu is
  `[New collage, Open…, Save, Save as…, Export…] · [Add photos…, Reset the framing] ·
  [Preferences, Keyboard shortcuts, About Pixlay]` — the last group is HIG `patterns/controls/menus`'
  "Standard Primary Menu Items", and *Preferences* is S25's addition to it. **There is no Save button**
  (ruling 37: it sat beside Export and read as the same action), which `tests/hig.rs::check_header_chrome`
  asserts together with the three slots and the menu's items; the export and Add photos buttons are
  `AdwButtonContent`s (icon plus label — one `suggested-action` on the export; `can-shrink` on the Add
  photos button, the guard for a bar too narrow for the label: measured 2026-09-26, the property
  changes nothing at 560x420 — `Add photos` is 122x24 and the heading keeps 99 px with it either way, so
  it is what a longer translation would need rather than something that fires today), and every control
  carries a tooltip and an accessible name.
- **The export's bounds are the settings' bounds**: `MIN_EXPORT_PX` / `MAX_EXPORT_PX` (`export.rs`) are what
  the settings surface's row offers and what a value read from the file is clamped to (S25); the CLI's own
  range is wider (`MAX_LONG_EDGE_PX`, §5), because it is a machine surface rather than a row.

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
- **Every framing gesture starts from the same base, bitmap or not** (S27):
  `EditorWindow::gesture_base` is the crop the cell is *shown* with when the canvas holds a decoded
  bitmap for it, and the cell's own stored crop when it does not — the fit needs the photo's own aspect,
  and nothing before the decoder answers knows it. With no bitmap, `fit_for` passes the request through
  **unfitted** and `draw` fits it at the boundary (§1's "a crop is a request"), so the drag, the keyboard's
  pan, the wheel and the strip's zoom/rotate all work on a cell whose bitmap has not arrived. Before S27 the
  drag asked `fitted_crop` alone and gave up on `None` — no command, no draw, no report, no undo step —
  while the keyboard's own pan went through `fit_for` and still moved the photo, which is the "the keyboard
  works, the drag does not" of the walk of 2026-09-26; and a step that is refused anyway is **reported once
  per gesture** (S15h's idiom: one toast, not one per motion event), never dropped in silence.
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
  recently used evicted, every reduced sample 16 bits: §4.1) and **one bitmap set per grid and source
  edge** (the resting grid and the coarse one, each at its own edge — S15f, PIX-004), carrying over
  every cell whose cell, source, source identity, template **and frame gap** are unchanged. The gap is
  in that list and the frame's radius and colour are not: the gap moves the region the fit covers, while
  the radius is the renderer's clip and the colour is painted after the slots, so both leave every
  bitmap bit for bit identical (S15f, PIX-004). A set built at one source edge is never an answer for
  another: a coarse set handed to a finer request would answer it with lower-quality pixels and store
  them as the new edge's own. A step of a gesture therefore rebuilds one cell and touches no disk at
  all; the window
  counts the decodes the worker reports (`EditorWindow::decoded_sources`) and the GUI test holds a whole
  drag to zero of them once both grids' copies exist. The copy is a **box average in linear light** to
  `PREVIEW_SOURCE_SCALE` (1.25) times the grid's long edge — the largest value that keeps the measured
  step inside the frame budget — or the photo itself when that is smaller; §4.1 has the shape and §8
  ("S12b") the numbers, including what it costs in fidelity.
- **Background work, one thread each.** Decoding (`decode.rs`) and exporting (`export.rs`) run on their own
  threads and hand plain data back through
  `MainContext::invoke`, because a GTK object may not leave the main thread. Decode requests are coalesced (latest wins) so a drag
  costs one build at a time, and the re-use of what a gesture does not change is
  `pixlay_imaging::Preview`'s (S12, above). An export reports progress, which
  the window shows in a progress bar rather than a modal.
- **A worker that cannot start, or that is gone, is a report and not a wait** (S15h, PIX-014). Starting a
  worker answers `Result` (`pixlay::workers::WorkerPlan`, whose product value is `Run`), every request
  answers whether it was queued, and the failing request clears the state it would have marked pending
  before it says anything: the canvas's grid, the band's build and the export progress bar all go back to
  resting, because a request nobody will answer must not be waited on. The canvas reports once per window
  (`decode_reported`) — an edit asks again and is not a second failure — and an export that could not be
  started says so as a toast. A thread that has started and died takes the same branch: its request channel
  answers `Err`. `EditorWindow::with_workers` is the tests' way in (`Workers { decode, export }`), and
  `EditorWindow::new` is the product's.
- **The accelerator table is data** (`crates/pixlay/src/app.rs::ACCELERATORS`): the bindings, the
  shortcuts dialog and the HIG test all read it, so they cannot drift. No binding uses
  `Alt+*`, `Super+*` or `Ctrl+Alt+*`; `F9` left the table with the utility pane (S13) and the picker's two
  keys left with the stage (S22) — `Ctrl+I` is now `Add photos…` (`win.add-photos`), and `Ctrl+Shift+O`
  (the folder chooser) and `Z` (the preview zoom) exist nowhere. Every action in the table has exactly one
  binding, and every other action is reachable from a control the Tab order reaches.
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
translation shipped **in the repository** — the pipeline that installs one is §10's (S16); what
ships today is the English source strings and the machinery that would carry a language pack.

## 10. The package (S16)

The AUR package is not a second product: it installs the two binaries this repository builds, the data
files the identity is spelled in, and the pipeline that would carry a language pack. What a caller can
rely on without reading the PKGBUILD:

- **One identity, spelled once per surface.** `org.yangtse.Pixlay` is the app-id at the same time in
  `pixlay::APP_ID` (`lib.rs`), the desktop file's own name and its `Icon=`, the icon files
  (`hicolor/scalable/apps/org.yangtse.Pixlay.svg` and `hicolor/symbolic/apps/org.yangtse.Pixlay-symbolic.svg`),
  the metainfo's file name and its `<id>`, its `<launchable>` (`org.yangtse.Pixlay.desktop`) and the MIME
  registration's `<icon>`. `crates/pixlay/tests/packaging.rs` is what fails when one of them moves
  without the others.
- **Paths.** `/usr/bin/pixlay` and `/usr/bin/pixlay-render` — the window and the machine surface
  (`AGENTS.md`, "the CLI is the only machine-operable surface"; a caller with no display uses the
  second); `/usr/share/applications/org.yangtse.Pixlay.desktop` with `Exec=pixlay %F`, so a double click
  is the app's own `open` handler (§9) and a `.pixlay` really opens a project;
  `/usr/share/metainfo/org.yangtse.Pixlay.metainfo.xml`; the two icons;
  `/usr/share/mime/packages/org.yangtse.Pixlay.xml`, the `application/x-pixlay` type — a `*.pixlay`
  glob with `sub-class-of application/json` (a project is JSON, §1) and the app's own icon;
  `/usr/share/locale/<language>/LC_MESSAGES/pixlay.mo`, one per language `po/LINGUAS` lists, in the
  domain the shell binds; and `/usr/share/licenses/pixlay/LICENSE`.
- **Two of those files are generated and never edited.** The desktop entry and the metainfo come from
  `data/org.yangtse.Pixlay.desktop.in` and `data/org.yangtse.Pixlay.metainfo.xml.in` plus the catalogs,
  through `msgfmt --desktop` and `msgfmt --xml` with `-d po` — one call each, because `po/LINGUAS` is
  what tells gettext which languages to merge in (`Name[de]`, `<summary xml:lang="de">`). The two
  templates' strings are extracted into the one `po/pixlay.pot` together with the shell's Rust strings:
  three `xgettext` passes — Rust, Desktop, and AppStream through gettext's own ITS rules — joined into
  one template by `po/extract-pot`, which `tests/i18n.rs` runs and compares with the committed file.
  `po/POTFILES` (this crate's sources) and `po/POTFILES.data` (the two templates) are held to the tree
  by the same test.
- **The language fallback is gettext's** and §9 states it: no catalog for the locale, and every string
  is the English source string. `po/LINGUAS` is empty in this repository, so the package installs no
  catalog at all today and the two generated files are their templates verbatim; a language pack is a
  `.po` file plus a line in `LINGUAS`, and nothing else changes.
- **The build and the install are the project's own** (S31). `meson.build` declares the system libraries
  (`gtk4 >= 4.12`, `libadwaita-1 >= 1.8`, `glycin-2`, `libseccomp`, `glib-2.0`, `gio-2.0`), so `meson setup`
  fails naming the one that is missing rather than failing inside a cargo build, and `crates/meson.build`
  runs `cargo build --profile release --locked` over the workspace with its target directory inside the build
  directory. `meson install` puts both binaries in `bindir`, merges the catalogs into the desktop entry and
  the metainfo (`msgfmt --desktop` / `--xml`), installs both icons, the `.pixlay` MIME registration (under
  the app-id) and `<localedir>/<language>/LC_MESSAGES/pixlay.mo` per language `po/LINGUAS` lists, and passes
  the prefix's localedir into the binary as `PIXLAY_LOCALEDIR`, which `crates/pixlay/src/i18n.rs` reads at
  compile time. A distribution installs the application with `meson setup build && meson compile -C build &&
  meson install -C build` and nothing else.
- **The PKGBUILD wraps that install** (at `packaging/arch/PKGBUILD`, where a `makepkg` run's own work tree
  and packages land beside it and `.gitignore` covers them): `source=` is the release tag's tarball, built
  from `pkgver`; a release pushes `vX.Y.Z`, fills `sha256sums` (`updpkgsums`) and writes `.SRCINFO`. The
  registry is vendored once (`cargo vendor`, with `CARGO_NET_OFFLINE=true` for the cargo call meson
  makes), `depends` is `gtk4`, `libadwaita` and `glycin` — the decoding backend S4 measured is a linked
  library, so it is a runtime dependency — with `libheif` an optdepend for HEIC and AVIF (as it is for
  `glycin` itself), and the license goes to `/usr/share/licenses/pixlay/`, which is Arch's path and not
  the prefix's.
- **Two architectures, and a release's assets** (ruled 2026-09-27, human; extended the same day: the arm64
  runner exists): `arch=('x86_64' 'aarch64')` — the tree is expected to build under Arch Linux ARM too, and
  that half of the **package** is built there by whoever runs it, because no GitHub runner has an aarch64
  Arch userland. A tag's GitHub Release therefore carries the **Linux binaries**
  (`pixlay-<version>-linux-amd64.tar.gz` and `pixlay-<version>-linux-arm64.tar.gz`, each with its
  `.sha256`, built from the tag by `release.yml` with the project's own build on `ubuntu-26.04` and
  `ubuntu-26.04-arm`) and the **`x86_64` package**
  (`pixlay-<version>-1-x86_64.pkg.tar.zst`, built on the machine and uploaded by hand). `url=` is the
  repository, `https://github.com/YangtseSu/pixlay`
- **A package build runs no tests** (ruled 2026-09-26, human): `makepkg`'s standard is that it builds and
  packages, the suite is the verification entry's (`AGENTS.md`) and CI's, and the two artifact validators
  (`desktop-file-validate`, `appstreamcli validate --no-net`) are `meson test`'s wherever the tools are
  installed (`data/meson.build`, `required: false`). Nothing in the build reaches the network, and every
  fixture is in the repository.
- **Not doing** (the plan's own list): Flatpak, Snap, any other distribution.
