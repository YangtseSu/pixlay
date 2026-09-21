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
    { "source": "photos/a.jpg", "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 } },
    { "source": null, "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 } }   // an empty slot renders white
  ],
  "text": [                         // canvas-level text layers (rendered in S5)
    { "content": "{date} #{index}", "mode": {"kind":"free","position":[0.5,0.9],"anchor":"bottomCenter"},
      "sizeRel": 0.02, "rotationDeg": 6, "color": {"r":0,"g":0,"b":0,"a":255}, "sourceSlot": 0 }
  ],
  "textFallback": { "date": "2026-09-20" }
}
```

> This example is a valid document: the two slots give a cut template whose areas sum to
> exactly 1.0, and the canvas is exactly 4:3 to match `template.aspect`. It is not
> renderable as written, because `draw` refuses text layers until S5 (see §4); drop the
> `text` array to render it today.

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
- **Text layer order**: `text` is drawn in array order, later ones cover earlier ones, and all of them cover the cells.
- **Tiled phase**: the tile grid starts from the canvas origin `(0,0)`, and each tile rotates around its own anchor; there is no per-tile variation.

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
| `--preview-px` | 1..=20000 (long edge, in pixels) | a preview larger than this cannot be reviewed by eye anyway |
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
- Text layers **are refused by `draw`** in v1 (`TextLayersUnsupported`) — better to error than to export something with text missing.
  S5 wires it up and the contract does not change.
- `render_surface` / `render_rgb8` are just thin shells that allocate a surface + call `draw`; `rgb8` composites ARgb32 premultiplied uniformly
  onto a white background and gives the straight-through RGB the encoder wants.

## 5. CLI: the machine operating surface

```text
pixlay-render render    --project <file.pixlay> --dpi <n> --out <file>
pixlay-render render    --template <name> --dpi <n> --out <file>   # no project, no photos
pixlay-render probe     --project <file.pixlay>
pixlay-render templates [--aspect <ratio>] [--json]
pixlay-render init      --template <name> --out <file.pixlay>
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
| output format | determined by the `--out` extension: `.png` / `.jpg` / `.jpeg`, anything else is a usage error |
| `--preview-px n` | n pixels on the long edge; the same `draw`, only `scale` changes |
| `--stats` | appends `{ms, encode_ms, peak_rss_mb, icc}`; `render` emits all four, `probe` emits no `encode_ms` (it does not encode). The measurement rules are below |
| `probe` | samples and outputs numbers (in-slot photo color, out-of-slot white background, shared-edge blended pixels, three-color convex combination residual), exit code 2 when the verdict is not passed |

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
- nested groups / layer trees / blend modes
- framing rotation beyond ±45°; **rotation only crops edges, it never grows the canvas**
- preserving the source ICC (v1 always interprets as sRGB), CMYK JPEG, per-slot color spaces
- curves / levels / masking / brushes; all of them in `AGENTS.md`'s "Directions not to improve" → "Not doing"
- multi-page / multi-canvas projects
- **version migration** for `.pixlay` (not written; higher refused, lower refused too, see "Version policy")
- **writing an edited `.pixlay`** (S6.5 owns saving a document the user changed, atomic tmp+rename included). `init` writes a *new* file
  and refuses to overwrite an existing one, which is a creation, not a save: it never touches a document that already holds the user's work
- MCP server, REPL / watch, natural-language arguments, reading defaults from a config file

## 7. Implemented later but the shape is already frozen

| Item | Lands in | Shape |
|---|---|---|
| clamp math | **S3, landed** | `CropTransform::fit(slot, canvas_aspect, photo_aspect) -> CropFit { transform, rotation_limited }`, applied by `draw`; the fit is idempotent and never exceeds `MAX_ZOOM` |
| template generator | **S2, landed** | `pixlay_core::templates` (`generator` recipes + the committed `frozen` data) and the `templates` / `init` subcommands; see §3 and §5 |
| command history / hit testing / project writing | S6.5 | not in the S1 contract; `CollageDoc` is their state carrier |
| text rendering | S5 | `TextLayer` is already in the contract; `draw` refuses it for now |
| encoding and metadata | S6 | for now the `image` crate stands in; S6 replaces it with a single pass writing pixels + chroma sampling + ICC + DPI |

## 8. Measured (2026-09-20, this machine)

| Item | Value |
|---|---|
| golden image RMSE | **0.0** (deterministic for the same build); the equivalent RMSE of a 1 px geometry error is 12 |
| preview vs export (2N downsample) | RMSE **2.32** (threshold 6, `AGENTS.md`'s A0 measured 2.62) |
| band stitching vs whole image | RMSE **0.033**, max pixel difference 2/255, 311 / 463080 bytes differ (scale 1.0); scale 0.1/0.3/0.5 measured too, the sizes always sum to the whole image |
| banding memory saving (A0 landscape 14043×9933, 10 slots, 300dpi) | whole image **1470 MB** → 4 bands **772 MB** → 16 bands **597 MB** (`VmHWM` measured in a separate process each time) |
| seam blend `probe` | ≤ 2 px/row (the threshold), `foreign = 0` |

Every threshold constant in the tests annotates this source, so a change in the numbers can be discovered.
