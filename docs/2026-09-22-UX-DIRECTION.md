# Direction review (2026-09-22): the picker-first UX, and which codebase to build it on

**Status: ruled on 2026-09-22, and now the plan's review of record.** The human ruled on all nine
decisions of §6 and on the seven rule rows of §3 in the same day; the rulings are recorded in §6 below
and carried into the live plan, [`2026-09-22-STEPS.md`](2026-09-22-STEPS.md), which is the authority from
here on. The plan of 2026-09-20 is retired to [`docs/archive/`](archive/) (its open half S7/S8; S7's human
walk is void, S8 is the new plan's last step). The standing `AGENTS.md` clauses this review contradicted
were rewritten clause by clause the same day — §3 below says which, and how.

The human described the target flow in conversation on 2026-09-22:

1. open the app → browse the library; a strip of thumbnails at the bottom, the main view above shows a photo
2. a multi-select mode; select 2–9 photos
3. **Next** → a template stage: candidate layouts previewed **in the main view, rendered with the user's own photos**
4. a layout can add/remove photos — remove drops the last one, add brings it back; past the number originally
   selected, a `+` opens the system chooser
5. several preset layouts for every photo count
6. after a layout is picked: border, corner radius and border colour are adjustable
7. inside a cell: zoom, shrink, rotate, flip, replace — through floating mini buttons
8. references: Google Photos (Android) and the Xiaomi gallery for the flow, Loupe for the interface
9. out of scope for now: Xiaomi's poster mode (presets + text styles)

## 1. Answer to question 1 — extend this codebase, do not fork Loupe

**Verdict: extend `pixlay`.** The two options do not have comparable costs, and the comparison is not
about GTK or decoding at all: Loupe is a *viewer*, and the parts of it that look relevant are the parts
`pixlay` already has, while every part the collage needs is absent from it. Numbers and sources below.

### Loupe 51.0.0 (GNOME, GPL-3.0-or-later) — what a fork would actually buy

| Fact | Evidence |
|---|---|
| ~14.9k lines of Rust in 49 files under `src/**`, one root manifest, **meson driving cargo** (`src/meson.build` has `cargo-build` / `cp-binary` / `cargo-test` targets) | `Cargo.toml`, `meson.build`, `src/meson.build` |
| **No document or project type.** No structured editing state is persisted anywhere; an image is a file path plus decoded frames | `src/lib.rs` module list; no doc/project module exists |
| **No multi-photo composition.** One image at a time; navigation is previous/next over a name-sorted directory in a swipe carousel | `src/file_model.rs`, `src/widgets/sliding_view.rs` |
| **No grid, no filmstrip, no multi-select** — i.e. none of stage 1/2 of the target flow | the `src/widgets/` tree has no collection widget |
| Editing = glycin `Operation` atoms (`Clip` / `Mirror` / `Rotate`) pushed through `glycin::Editor::apply_sparse`, **which writes the user's own file**; the preview is a GSK render node | `src/editing.rs`, `src/editing/preview.rs` |
| **No encoder with DPI/ICC**, and **no headless render path** (its offscreen path needs a realized `gdk::Display`) → it cannot satisfy "`pixlay-cli` is the only machine-operable surface" | `src/widgets/print.rs` is the only other render target |
| Rendering is **GTK/GSK with a tiled texture cache**, not cairo | `src/decoder/tiling.rs` (711 lines) |
| Decoding is **`glycin 4.0.0`** — the backend `pixlay` chose in S4 — and it is asked for *less* than `pixlay` asks (no memory-format selection, no ICC→sRGB, no byte caps) | `src/decoder.rs` vs `crates/pixlay-imaging/src/decode.rs:238-256` |
| The GTK/adw/gettext generation is already the same as this project's: gtk4 0.11 (`v4_14`), libadwaita 0.9 (`v1_8`), gettext-rs 0.8, cairo-rs 0.22, edition 2024 — so the fork buys **nothing** on those axes | `Cargo.toml` |
| A fork drags in meson, `libseccomp`, `libgweather`/geocode-glib and GNOME packaging identity (`org.gnome.Loupe`, `.doap`, GNOME l10n) | `meson.build`, `data/` |
| Its `CONTRIBUTING.md` §"Use of Generative AI" **bans LLM-generated contributions outright** (verified 2026-09-22), while this project's working rule is that AI writes all the code | `CONTRIBUTING.md:25-31` |

What Loupe *is* worth: a **design** reference — the properties/metadata pane, the zoom controls over the
image, the drag-and-drop overlay, the crop tool's handles, the HIG window shell, and its icon set
(`edit-mirror-horizontally-symbolic`, `edit-mirror-vertically-symbolic`). None of that is reusable as code
across the cairo/GSK boundary, and all of it can be read from the mirror.

### `pixlay` today — what extending it inherits for free

The engine half of the target flow already exists and is frozen by tests: the v1 contract (`docs/CONTRACT.md`),
the template generator plus committed geometry (S2), the framing clamp (S3), the glycin pipeline (S4),
canvas text (S5), the one-pass encoder with DPI and ICC (S6), the command history and hit testing (S6.5),
and a GTK shell whose HIG/i18n/render-consistency tests already exist (S7). Rewriting the shell around a
new flow touches `crates/pixlay` (~4.2k lines) and leaves the four engine crates standing.

The decisive asymmetry: **stage 3 of the target flow (layouts previewed with the user's own photos) is
`render::draw` — the function `pixlay` already has and Loupe does not.** A Loupe fork would have to
re-earn S1–S6.5 before it could draw the first collage.

## 2. What the target flow costs, capability by capability

| Need (from the flow above) | Today | Cost |
|---|---|---|
| list the photos of a folder | nothing | new, CLI + GUI |
| thumbnails | `slot_bitmap` resamples **after a full decode**; `glycin::Loader` 4.0.0 exposes no scaled request (`Loader`'s builders: sandbox, cancellable, transformations, memory formats, ICC, base dir, pool, limits, main context) | new helper + an LRU cache; the real cost is decode time, measured in S4 at **11–110 ms per 2400×1600 file** |
| an **ordered** multi-selection (order = cell order) | nothing; the only selection is one slot index (`window.rs:90`) | new; `GtkMultiSelection` does not guarantee order, so the order is owned by the page |
| a stage/page stack | nothing (`Stack` / `NavigationView` / `Overlay` / `Fixed`: **0 matches** in `crates/pixlay/src`) | new; `AdwNavigationView` is in the pinned libadwaita (1.4+) → **no new dependency** |
| showing a bitmap in a widget | nothing (`gtk::Picture` / `Texture` / `set_paintable`: **0 matches**) | new |
| several layouts per photo count | **12** templates: 2→2, 3→1, 4→3, 5→1, 6→1, 7→1, 8→1, 9→1, 10→1 | new recipes; pure data + invariants, no window required |
| per-cell flip | **absent in the whole workspace** (`flip` / `mirror`: prose only) | new `crop` field + the transform |
| per-cell 90° turns | absent (`crop` is `zoom` + `offset` + `rotationDeg` ±45°) | new `crop` field |
| per-cell zoom / pan / replace | exists (`CropTransform`, `Command::SetSource`) | 0 |
| border gap / corner radius / border colour | absent in core, render and GUI; `grid-4-2x2g` proves a **baked** gutter is expressible as template geometry (slots stop 1/16 short) | new doc field + a render-time decoration stage |
| a canvas backdrop colour | hard-coded white in `driver` and `probe` | new field, default white |
| keep photos across a layout change | exists: `SetTemplate` retains the first `min(old, new)` cells (S7 decision 1) | 0 |
| export with DPI + ICC, one pass | exists (S6) | 0 |
| undo/redo, one command per gesture | exists (S6.5 + S7) | 0 |

## 3. Locked rules this direction contradicts — each one needs a ruling

**All seven rows were ruled on 2026-09-22 and the recommendation column is what is now in `AGENTS.md`**
(the seventh row — "nothing may be possible only in the GUI" — was never in dispute and stayed). One row
was resolved differently from its recommendation: row 6's cap is **2–9 in the picker**, with the 10-slot
template kept in the library where only the CLI and project files reach it.

| # | Rule (where) | What the target flow does | Recommendation |
|---|---|---|---|
| 1 | Scope criterion: "the shortest main path … does it make the main path longer? If so, cut it" (`AGENTS.md`) | adds a picker stage and a layout stage before editing | Amend: redefine the path as `open → pick 2–9 photos → pick a layout → adjust → export`. The common case *loses* a step (photos land in selection order, so per-slot placement by hand disappears); the two new stages are the price of not starting from an empty document |
| 2 | "**Do not add mode switching** (edit mode / collage mode)" (`AGENTS.md`) | a two-stage flow is literally two screens | Amend the wording, keep the intent: forbidden is **two parallel modes over one document**; a **sequential creation flow** (`AdwNavigationView` push/pop, Back in the header bar) is HIG's own shape for a multi-step task |
| 3 | "no phone-style layout" (`AGENTS.md` HIG deviations, `docs/HIG-REVIEW.md` §3) | the described affordances (bottom strip, `+`, tap) come from mobile | Amend to "the capability, not the phone's chrome": `GtkGridView` + `GtkMultiSelection` + header-bar Next + `Ctrl+A`, per HIG. The bottom strip keeps a real job — it is the **ordered** tray, and selection order is cell order, so it is also where reordering happens |
| 4 | "Composite onto opaque white … an export is never transparent" (`AGENTS.md`) | a user-chosen border colour shows through the gaps and the rounded corners | Keep opacity absolute; the backdrop colour becomes a **document field defaulting to white**, so every existing project renders byte-identically. Amend to "opaque, never transparent; white is the default backdrop" |
| 5 | HIG `patterns/containers/selection-mode` = **"Not applicable"** (`docs/HIG-REVIEW.md` §1) | the picker *is* a collection view with multi-select | The row flips to a criteria row (selection mode, `Ctrl+A`, batch actions over the selection) — the same page's advice still holds for the canvas |
| 6 | `MIN_SLOTS = 2`, `MAX_SLOTS = 10`; the library covers 2..10 | the human asked for 2–9 | Either cap the picker at 9 and accept that `strip-10-10x1` is then unreachable from the GUI (a capability that exists only in files), or cap at 10. **Recommend: 9 as asked, and note the unused template; if that note is unwelcome, delete the 10-slot recipe** |
| 7 | "Nothing may be possible only in the GUI and not in the CLI" (`AGENTS.md`) | every new capability above | Each one lands as a subcommand **first** (R1, R3) |

## 4. Where the flow contradicts its own references

Primary sources, checked 2026-09-22 (Google's collage help article, one URL per platform; Xiaomi's own
MIUI-13 and Xiaomi-12 user guides; Xiaomi's HyperOS FAQs):

- **The 2–9 range.** Google Photos caps at **6** on Android/iOS ("Select up to 6 photos") and **9** on the
  desktop web. Xiaomi's own MIUI-era manuals say **"Select 1 to 6 photos"** — but **the human ruled on
  2026-09-22 that the current Xiaomi gallery allows nine**, and the human's word is the ruling: those
  manuals are wrong or outdated for it. Either way 2–9 is the product's own decision, taken deliberately
  rather than copied.
- **Live-preview layouts rendered from the user's own photos is real, and Google Photos is the product that
  does it** ("To preview your photos in different grids, at the bottom, swipe through the templates"). This is
  the target flow's single most distinctive stage, and it is exactly what `render::draw` gives cheaply.
- **Flip is documented by neither product.** It is a new requirement, not parity (Loupe's mirror icons are the
  closest precedent).
- **"Floating mini buttons" per cell is not Google's pattern either** — Google uses *select the photo, then a
  bottom sheet with an action list*. The floating-button memory most likely comes from third-party collage apps,
  so its ergonomics are unverified; treat it as a small design decision inside R6, satisfied with real GTK
  widgets over the canvas (see R6).
- **"Remove drops the last photo" is not the reference semantics** (Google's remove is per-slot and the layout
  collapses). The LIFO model is simpler and is what the human asked for: keep it, and make the **photo count a
  first-class value** — the layout list is filtered by it, add/remove changes it, and removing drops the last
  photo. This maps cleanly onto `SetTemplate`'s cell retention.
- **Aspect ratio** (which the human did not list) is Xiaomi's documented control and is *absent* from Google
  Photos' collage editor. In `pixlay` the aspect is a property of the template, and the CLI already filters the
  library by it (`templates --aspect`).

## 5. The roadmap (ruled; now S9–S16 in the live plan)

The R-numbers below became **S9 … S16** of [`2026-09-22-STEPS.md`](2026-09-22-STEPS.md), where the exits
are stated and where progress is tracked; this section stays as the reasoning. Two changes against what
was proposed here: S12 is new (ruling 1's measurement step), and the flip/quarter-turn work in R3 is gone
(ruling 11 also removed the ±45° rotation cap). R0's rulings are in §6 and in `AGENTS.md`.

Ordering follows `AGENTS.md` principle 4 — engine and CLI first, the window last — because the windowless loop
is where the numbers are cheap. **Packaging (S8) moves to the end**, because its human criterion ("the installed
package walks the main path") is defined on the path this direction redefines. As ruled, R1–R6 became S9–S15
(with the new S12) in [`2026-09-22-STEPS.md`](2026-09-22-STEPS.md), S8 became its last step (S16), and S7's
pending walk is void rather than merged.

### R0 · Rulings (human; nothing to code)

- **Goal**: turn section 3's seven rows and section 6's decisions into on-disk rulings.
- **Exit**: a ruling block in this file, the amended `AGENTS.md` sections, `docs/HIG-REVIEW.md`'s
  `selection-mode` row, the rewritten "Current progress" line, one commit. No code.
- **Human**: all of it.

### R1 · The library and the selection, on the CLI

- **Goal**: stages 1–2 as machine surface, so the picker's pixels and its cost are measurable without a window.
- **Work**: three subcommands — `scan --dir <path>` (deterministic listing: path, mime, size after EXIF
  rotation, EXIF date, and a per-file reason when it cannot be read), `thumb --photo <p> --px <n> --out`
  (decode → resample → encode, the thumbnail's pixels as a file), and an ordered selection → project entry
  point (`init --template <name> --photo <p>…`, argument order = cell order; today `init` writes a photo-free
  project). The selection **policy** (the 2–9 clamp, order, and which layouts survive an add/remove) lands in
  `pixlay-core` as pure functions, so both the CLI and the GUI call the same rules and the tests need no display.
- **Exit criteria**: JSON field set fixed and asserted; ordering stable across runs on the same directory;
  a fixture directory with PNG/JPEG/HEIC/16-bit PNG/EXIF-6/non-image/unreadable members produces the expected
  rows and exit codes; `init --photo` N times gives N cells in argument order; N outside 2..=9 is refused naming
  both bounds; a slot-count/template mismatch is refused naming both numbers; `thumb` output's size and its
  `VmHWM`/time are recorded as the picker's budget number.
- **Not doing**: no recursion into other filesystems, no thumbnailing cache on disk, no EXIF editing.
- **Human**: none.

### R2 · The layout library: several presets per photo count

- **Goal**: stage 5 — for every count 2..9, **at least three** layouts, in at least two of the existing aspect
  families, so the gallery has something to show.
- **Work**: new lattice recipes in `templates/generator.rs` + the regenerated `templates/frozen.rs`
  (`cargo run -p pixlay-core --bin pixlay-gen-templates`). **Existing names keep their geometry and their
  `templateVersion`** — a new layout is a new name, never an edit of a shipped one. Reuse the family naming
  (`strip-<slots>-<cols>x<rows>`, `grid-<slots>-<cols>x<rows>` with `g` for a gutter, `mosaic-<slots>-<variant>`).
- **Exit criteria**: the S2 invariants hold for every new template (pairwise zero overlap, no interior hole,
  cut-type areas summing to exactly 1.0, declared area == outline area within 1e-6, dyadic coordinates); the
  determinism test still regenerates the frozen artifact byte for byte; a new assertion gives the histogram
  "≥3 layouts for every count in 2..=9"; `pixlay-render templates --aspect 4:3` shows them.
- **Not doing**: no free-form/SVG geometry, no curves, no per-layout custom aspect beyond the existing ratios.
- **Human**: none. The *look* of the new layouts is judged later, in R5, on the gallery.

### R3 · Contract: frame, flip, quarter turns

- **Goal**: stage 6's decoration and stage 7's flip/rotate, in the document and in the renderer.
- **Work**:
  - `CollageDoc.frame: { gapRel, radiusRel, color }`, all with serde defaults (`0`, `0`, white) → **no `docVersion`
    bump** (`docs/CONTRACT.md` §1's version policy).
  - `CropTransform.flipH` / `flipV` (default false) and `quarterTurns` (default 0). Flip needs no new `Command`
    (it rides on `SetCrop`); the 90° turns must be applied in the frozen order's geometry stage, before the
    arbitrary rotation.
  - A **decoration stage in `render::draw`, after slot compositing and before the text layers**: each cell is
    clipped to its outline inset by `gapRel/2` and rounded by `radiusRel`; the gaps and corners show
    `frame.color`. The inset is strictly inside the slot, so the S3 clamp's guarantee ("the clamped photo covers
    the entire slot") still implies coverage of what is visible — no clamp change.
  - The CLI surface for both: `render --gap … --radius … --border-color …` and an `edit` subcommand
    (`--slot <i> --flip-h --flip-v --quarter-turns <n> --out`) so slot transforms are settable, and therefore
    checkable, without a window.
- **Exit criteria**: a project with no `frame`/`flip`/`quarterTurns` renders **byte-identical** to the current
  build (the probe's own pixels, not an eyeball); rounded corners are clipped — count background-coloured pixels
  in the slot's corners and require > 0 at `radiusRel > 0`, with the count monotone in the radius; the clamp's
  coverage probe still passes with a non-zero gap and radius; grading identity still pixel-identical; flip is
  applied before the arbitrary rotation (a probe with an asymmetric fixture); `.pixlay` round-trip keeps all
  three fields.
- **Not doing**: no per-cell border colour (frame colour is canvas-level), no drop shadows, no movable dividers
  (Google Photos lacks them too — noted as a possible later differentiator, not now).
- **Human**: none for the code; the *look* of the frame is judged at R6.

### R4 · The picker stage (GUI)

- **Goal**: stages 1–2 — browse, select 2–9 in order, Next.
- **Work**: an `AdwNavigationView` in the window with the picker as the root page and the editor pushed on Next.
  The picker is a folder chooser (`GtkFileDialog`, default `XDG_PICTURES_DIR`, last folder kept for the session
  — no config file, which `docs/CONTRACT.md` §6 excludes), a `GtkGridView` over a `gio::ListStore` of photo
  objects with `GtkMultiSelection`, a header bar whose Next button carries the count and is insensitive below 2,
  an ordered tray along the bottom (the selection as a list, plus reordering), and a **fit-to-window preview** of
  the focused photo as a `gtk::Picture`.
- **Two deliberate bounds** (each a ruling candidate):
  - the preview is a *browser*, not a second product: no zoom/pan gestures in this step. If the human wants
    Loupe-style zooming and panning inside the picker, say so in R0 — that is the one requirement that would
    reopen question 1, and it is a viewer's job, not a collage's.
  - thumbnails are full decodes (glycin exposes no scaled request) on a bounded pool with an LRU cache and
    progressive fill of the visible range; a 10k-photo folder is out of scope for the first cut.
- **Exit criteria**: the machine-checkable subset — the grid's cells all resolve to a decoded bitmap
  (progressive fill completes on the fixture folder); the tray's order equals the order the CLI's `init --photo`
  would produce for the same selection (one shared policy function); Next is insensitive at 0 and 1 selected and
  at 10; `Ctrl+A` selects all and the clamp reports the overflow instead of silently truncating; the preview's
  pixels equal `pixlay-render thumb` of the same photo at the same size (RMSE threshold); no display-free test
  breaks (the picker's policy is in core, so those tests need no window).
- **What breaks, knowingly**: `crates/pixlay/tests/mainpath.rs` (the window no longer opens on a default
  document), `tests/hig.rs` (new actions need accelerators + accessible names), `tests/i18n.rs`
  (`po/POTFILES` must equal `crates/pixlay/src/**/*.rs` — every new GUI file is added in the same step).
- **Human**: the picker's feel — grid density, how selection reads, whether the tray earns its space.

### R5 · The layout stage (GUI)

- **Goal**: stages 3–5 — candidates previewed in the main view *with the user's own photos*, add/remove photo,
  `+` past the original count.
- **Work**: after Next, the main view shows the selected layout rendered by `draw`, the candidate layouts as
  thumbnails below it, each composited from **one preview-sized decode per selected photo** (cached) — so the
  cost is N decodes plus C cheap composites, never N×C decodes. The photo count is the filter: add/remove moves
  to the nearest layout with that count, preferring the same family; remove drops the last photo; add past the
  original selection opens `GtkFileDialog::open_multiple` (currently unused) and either fills the new slots or
  reaches for a layout that does.
- **Exit criteria**: for each candidate, the thumbnail equals `pixlay-render render` of the same document at the
  same pixel size (RMSE threshold) — i.e. **the gallery is not a second renderer**; the count filter never shows
  a layout with the wrong slot count; add/remove preserves every surviving cell's photo, crop and grade; the
  `+` path's order matches the tray's; the whole stage is walkable from the keyboard.
- **Human**: whether the gallery reads as "my photos in that layout" at a glance, and whether the strip's
  placement (main view above, candidates below) is the right one.

### R6 · The compose stage, then the walk and packaging

- **Goal**: stage 6–7 in the editor, then the human walk of the whole path.
- **Work**: the canvas becomes a `GtkOverlay` carrying a `GtkFixed` of small buttons anchored to the selected
  slot's bounding box (`Placement::to_widget` already gives the rectangle): zoom in/out, rotate 90°, straighten,
  flip horizontally/vertically, replace, clear. Real GTK buttons, not cairo shapes: HIG's accessible-name and
  keyboard-reachability tests must see them, and the canvas-vs-CLI pixel test must stay green because they are
  interface, not content. Then the sidebar's Frame group (gap, radius, colour — `gtk::ColorDialogButton` already
  has a precedent in the Text group), the main-path walk, the HIG re-read (`docs/HIG-REVIEW.md`: selection-mode
  becomes a criteria row, the phone-style deviation is rewritten), and S7's gate closes here.
- **Exit criteria**: every floating button is reachable and named (the existing HIG checks); the selected slot's
  buttons sit inside its rectangle (asserted from `Placement`); the canvas still matches `pixlay-cli render`
  (RMSE threshold unchanged); the new main path is walked end to end (machine walk in `tests/mainpath.rs`, human
  walk in the gate); `docs/HIG-REVIEW.md` re-read for a second time (it is re-read at the start of every UI step).
- **Human**: the three-minute walk of the *new* path, and the copy.
- **After it**: S8 packaging, with `check()` on the new flow.

## 6. The rulings (2026-09-22, human)

Every decision this review raised, and what the human ruled. The plan that follows from them is
[`2026-09-22-STEPS.md`](2026-09-22-STEPS.md) (steps S9–S16); the rule clauses were rewritten in
`AGENTS.md` the same day.

| # | Decision | Ruling |
|---|---|---|
| D1 | Direction: extend `pixlay` or fork Loupe | **Extend `pixlay`** — as recommended in §1. Loupe is a design reference only |
| D2 | What "browse photos" means | **Grid + a large preview that can zoom and pan** — one step further than recommended (§2 ruled that zoom/pan browsing *is* wanted; the product stays a collage, so no fullscreen or flip-through) |
| D3 | The seven rule rows of §3 | **All amended**, clause by clause, in `AGENTS.md` and `docs/HIG-REVIEW.md` — the new design wins wherever the two conflicted |
| D4 | The selection cap | **2–9.** `strip-10-10x1` keeps its place in the library and never appears in the picker |
| D5 | Where the library comes from | **`XDG_PICTURES_DIR` + a folder chooser**, no configuration file |
| D6 | Photo-count semantics on add/remove | **LIFO, plus per-cell remove/replace** — the batch control is first-in-first-out's opposite (the last photo goes, and comes back), and a single cell can also be cleared or replaced |
| D7 | S7's pending walk | **Void**, as recommended: the path it walks is not the product's path any more. The walk happens once, on the new path, in S15 |
| D8 | S8 packaging order | **Last**, as recommended — it packs the product that exists |
| D9 | The floating buttons' ergonomics | **Real GTK buttons in a `GtkOverlay` + `GtkFixed`** over the canvas, so the HIG checks cover them |

Two further rulings that the questions did not cover and that change the contract:

- **No mirroring and no quarter turns.** Verbatim: *"不要翻转了。只要任意角度旋转和移动和缩放。旋转上限取消。"*
  So the per-cell capabilities are **zoom, move, rotate by any angle** — and the ±45° cap, together with
  the `CLAMP_ZOOM_LIMIT`/`rotation_limited` machinery that existed to reduce an over-asking angle, is
  **removed** (S11). Flip is added to `AGENTS.md`'s "not doing" list.
- **The Xiaomi gallery allows nine photos.** The human's word overrides the guides this review's web
  check found ("Select 1 to 6 photos" in Xiaomi's own MIUI-era manuals, §4): those guides are wrong or
  outdated for the current gallery. The 2–9 range stands as the product's own decision either way, and
  the parts of §4 that remain load-bearing are Google Photos' *live-preview templates* pattern and the
  observation that flip and floating per-cell buttons come from neither reference product.
