# Pixlay implementation steps

Run the steps in order. **One step at a time**; finish a step before starting the next.

The first sentence for a new session:

> Read `AGENTS.md`, then read `docs/STEPS.md`. Do only the step the "Current progress" line under "Status" points at, and do not start later steps ahead of time.
> After finishing a step, run that step's verification command, turn its exit criteria into tests, write the outcome into that step's "Result" subsection, then update its status row and the "Current progress" line.

- Hard constraints and invariants are in `AGENTS.md`; this file only orders the steps and gives the exit criteria, and does not repeat their content.
- Before starting each step, run `cargo test` once to confirm the baseline is green.
- Per-step material lives under that step, in step order. Material shared by several steps is collected at the end: "Measurement rules", "Open decisions", "Measured baseline".
- **S0's spike was deleted along with S1** (`crates/pixlay-cli/src/bin/a0-spike.rs`): its one-off probe duty was taken over by `pixlay-render probe`, and its numbers stay in S0's result and in "Measured baseline".

## Status

Legend: ✅ done · 🚧 in progress · ⏸ blocked (waiting on a human decision or external input) · ⬜ not started

**Current progress: S1 — ✅ done and passed contract review (2026-09-21, human; the contract reading is `docs/CONTRACT.md`, measurements and deviations in "S1 result")**
**Next action: start S2 in a new session.**

| Step | Status | Date | What it delivers |
|---|---|---|---|
| S0 · Cairo limit spike | ✅ done | 2026-09-20 | Cairo renders A0@300dpi inside the budget: 185/551 ms compositing, 941 MB peak `VmHWM`, both formats written. Gate passed: Cairo stays |
| S1 · Minimal contract + feedback loop | ✅ done | 2026-09-20 | `CollageDoc` v1 frozen, the single `draw`, `pixlay-render render` produces images, `probe` answers in numbers. Gate passed 2026-09-21: contract v1 passes |
| S2 · Template system (geometry only) | ⬜ not started | — | Regular and irregular template geometry, deterministic and frozen under a `templateVersion` |
| S3 · Framing and clamp | ⬜ not started | — | Absolute-zoom framing with rotation and a clamp that always covers the slot |
| S4 · Image pipeline | ⬜ not started | — | `pixlay-imaging`: decoding, EXIF rotation, 16-bit linear resampling, per-slot grading and the global filter |
| S5 · Text layers | ⬜ not started | — | Canvas-level text, free placement and tiled watermark through one mechanism, `{date}` from EXIF |
| S6 · Export | ⬜ not started | — | Physical size + DPI and long-edge-pixels modes, pixels and metadata written in one pass |
| S6.5 · Command history / project IO / hit testing | ⬜ not started | — | `Command` + undo stack, `.pixlay` save/load with atomic write, point → slot hit testing |
| S7 · GTK shell and interaction | ⬜ not started | — | The window: the three-minute main path, keyboard and HIG conformance, i18n wiring |
| S8 · Packaging | ⬜ not started | — | `PKGBUILD`, desktop file, icons, metainfo, translations; installable from AUR |

**How a status is marked.** The status row above, the marker on the step's own heading, and the "Current progress" line are three renderings of the same claim and must always agree; changing one is part of closing the step. A step becomes ✅ only when its gate ruling is on disk — the five parts listed in `AGENTS.md` "Session and persistence discipline" (ruling block + status row + the gate entry under "Where humans must step in" + `docs/CONTRACT.md` + commit), with nothing missing. A step that is waiting on a human answer is ⏸, not 🚧.

## Splitting principles

1. Every step must have a **machine-checkable** exit. A "step" with no checkable exit is not a step.
2. First do the one thing that can overturn the whole choice of technology (S0).
3. Freeze the contract first, then scale up (S1).
4. **Put the GUI last (S7)** — S0–S6 all complete in a windowless, screenshot-free fast loop.
5. **Session boundaries line up with "gates", not with step counts.** Finishing a step does not require a new session, but these three cases **must** stop:
   the step ends with a **human criterion or a human decision** (S0's Cairo keep-or-drop, S1's contract review, each step's visual quality, S7's three-minute main path);
   the step produces an **irreversible contract or frozen data** (S1's `CollageDoc` shape, S2's `templateVersion`, S4's decoding backend
   deciding S8's `depends`); the step **may overturn an earlier choice of technology**.
   *Rationale: within one session, the model treats its own unwritten draft as an established premise and keeps building on it; a contract review is only meaningful
   when executed by a session that did not write that draft.*
   *Precondition: a boundary holds only if the **conclusion is already on disk** (the threshold constants in the tests + the measured numbers in this file + the "Current progress" line).
   A conclusion that is not on disk means switching session equals measuring it again.*
   By this rule the natural boundaries are `S0 ┊ S1 ┊ S2+S3 ┊ S4 ┊ S5+S6 ┊ S7 ┊ S8` (six sessions, not nine).

## Where humans must step in

Wherever the original criteria said "visual / readable / three minutes", they were either replaced with computable quantities (the exit criteria below) or listed explicitly as human criteria (this section).

| Gate | Question | Status |
|---|---|---|
| After S0 | Look at the numbers: is Cairo usable? | ✅ passed (2026-09-20, human): **Cairo stays** |
| After S1 | Review the contract. This is the only place a human must confirm — if the contract is wrong, the seven steps after it are all wasted, and the model itself cannot see that "this contract will not be enough later". Before reviewing, first read "Open decisions": every entry in those tables changes the contract's shape | ✅ passed (2026-09-21, human): **contract v1 passes**, defects fixed per the review (see S1's ruling) |

**The gate's closing action** is in `AGENTS.md` "Session and persistence discipline": a ruling block + the status row + this table's entry + `docs/CONTRACT.md` + commit,
and it is not done if one of the five is missing.

The remaining steps complete through the model's automatic loop, except the **visual criteria** (what could be computed has already been turned into computable quantities in the exit criteria):
S0's text readability, S4's downsampling quality, S5's kinsoku and punctuation squeezing feel, S7's "three-minute main path" and whether interface copy has any missed wrapping,
S7's GNOME HIG visual checklist (`docs/HIG-REVIEW.md`: high contrast / large text / keyboard-only / screen reader / touch and OSK).

---

## S0 · Cairo limit spike — ✅ done (2026-09-20)

**Nature: disposable.** This step may be dirty, may hardcode, may use one-off scripts.

- **Goal**: prove Cairo can render A0@300dpi.
- **Work**: one hardcoded `[[bin]]` that builds a 9933×14043 surface with `cairo::ImageSurface`, draws an irregular polygon clip, blits a photo with an affine transform, overlays one line of rotated CJK text, and writes PNG and JPEG.
- **Exit criteria**:
  - the output file exists and `identify` reports a size of 9933×14043
  - compositing time and peak memory have concrete numbers
  - no crash, no video-memory / memory exhaustion
  - the photo is inside the slot, there is no bleeding outside the slot, and the text is readable
- **Not doing**: no UI, no template library, no framing math, no project file.
- **Human**: the Cairo keep-or-drop gate. **Passed (2026-09-20, human) — the ruling is in "S0 · ruling" below.**
- **Done (2026-09-20)**: the implementation is `crates/pixlay-cli/src/bin/a0-spike.rs` (disposable, a one-off hardcoded `[[bin]]`),
  and its criteria, numbers and handoff to S1 are in "S0 result" below; `cargo test` has 5 tests that run the same probe set at 1/4 A0 size.
  **That spike was deleted in S1**; this section's numbers are a historical record, and the criteria that were kept are carried by `pixlay-render probe`.

### S0 · review additions (2026-09-20)

The technology-choice risk has largely receded by now (see "Measured baseline"); this step is suggested to become "reproduce the baseline + add 10-slot peak memory".

- peak memory is uniformly `VmHWM` (see "Measurement rules"), and the A0 compositing peak is ≤ 2.5 GB
- "photo inside the slot, no bleeding outside the slot" → probe pixels: a sample point inside a slot equals that slot's photo color; a sample point outside equals the canvas background color;
  the number of blended pixels on a shared edge between adjacent slots is ≤ 2 × the seam length
- "text is readable" → the proportion of ink pixels inside the text bounding box falls in an interval
- **Test content must be non-flat**: flat color blocks underestimate both encoding time and size (measured difference 4.6× / 78×)
- one addition: the canvas and any area not covered by a photo are always **white** (already decided, see AGENTS "Hard constraints"); S0 only has to confirm that rendering leaks no non-white pixel —
  the probe takes a few points each in the uncovered area of a slot and in the canvas area outside slots, and the assertion is pure white

### S0 result (2026-09-20)

`[code `crates/pixlay-cli/src/bin/a0-spike.rs` (disposable, committed only for reproducibility)]`

Every criterion above went into that bin's probe, and there is no longer a "looks right" step. How to run it (release):

    cargo build --release -p pixlay-cli
    ./target/release/a0-spike --content flat   --slots 2  --skip-encode --out /var/tmp/pixlay-s0/flat2
    ./target/release/a0-spike --content flat   --slots 10 --skip-encode --out /var/tmp/pixlay-s0/flat10
    ./target/release/a0-spike --content detail --slots 2  --out /var/tmp/pixlay-s0/detail2  --preview-px 1400
    ./target/release/a0-spike --content detail --slots 10 --out /var/tmp/pixlay-s0/detail10 --preview-px 1400

All four give `verdict = ok`. Assertions and measurements (the sources of the threshold constants are written in the code comments):

| Criterion | Measured (A0) | Threshold |
|---|---|---|
| output size | PNG/JPEG both 9933×14043 | exactly equal |
| compositing time | 2 slots 185 ms · 10 slots 551 ms | none |
| compositing peak memory `VmHWM` | 941 MB (the same for 2 slots and 10 slots) | ≤ 2560 MB |
| whole-run peak `VmHWM` (including encoding) | 1340 MB | none |
| non-white samples outside slots | 0 / 136584 (all white) | = 0 |
| uncovered area inside a slot (a slot corner rotated 8°, a strip shifted down 4%) | all pure white | pure white |
| seam blend | 1.000 px/row, widest 1 px, three-layer convex combination residual 0.20/255, 0 unexplained | ≤ 2 px/row, widest ≤ 2 px |
| text ink ratio | 0.1148 (bbox 3637×610 px, font size 281 px) | 0.02–0.60 |
| content non-flat (per-pixel luminance step after rendering) | 3.61 (detail 2 slots) / 4.60 (detail 10 slots); flat is 0.00 | > 2.0 |

Human criteria (photo inside the slot, no bleeding outside the slot, text readable): the 2-slot and 10-slot 1400 px previews were confirmed visually one by one —
the irregular L-shaped clip is correct, the rotated slot's photo is clipped and does not go out of bounds, uncovered areas show white, CJK glyphs are complete and readable, and the seam is a 1 px hard edge with no bleeding.

### S0 · ruling (2026-09-20, human)

**Cairo stays.** Every criterion is green and no veto condition triggered (compositing 185/551 ms; peak 941 MB compositing,
1340 MB whole run, budget 2.5 GB), and `AGENTS.md`'s "Do not replace Cairo with GPU rendering" stays in force.
S0's page ends here: the spike is **disposable**, its one-off probe is taken over in S1 by the CLI's `probe` subcommand, and this file is deleted when that happens.

### S0 · handoff to S1

Three silent traps, all stepped in during this step:

1. `cairo::SurfacePattern::set_matrix` takes a **user space → pattern space** matrix: the "placement matrix" has to be inverted before handing it in.
   When the direction is written the wrong way cairo reports no error, it just **draws nothing**.
2. `pango::FontDescription::set_absolute_size`'s unit is **pixels × `PANGO_SCALE` (1024)**. Missing that one multiplication
   yields 0.07 px text, equally silent (it drew, but nothing is visible).
3. The seam criterion cannot be written as "lies between the two colors": a seam pixel blended with the **white background** will end up outside the interval of the two colors anyway (measured, a red/blue seam
   is 138,108,183, with the green channel higher than both sides). Only "the three-layer convex combination residual of white + left slot + right slot" is discriminating.

Dependencies (settled together in S1's "Dependency registry"): `cairo-rs 0.22.9` + `pangocairo 0.22.9` (belonging to `pixlay-render`, long-term),
`image 0.25.10` (S0 used only its JPEG encoding and size reading, temporary; whether it stays is decided by S4/S6). When the spike is deleted, `image` should go with it,
unless it has another use by then.

## S1 · Freeze the minimal contract + feedback loop — ✅ done (2026-09-20), gate passed (2026-09-21)

- **Goal**: make the CLI loop hold, and every later step is built on top of it.
- **Work**:
  - `pixlay-core`: `CollageDoc` / `CanvasSpec` / `Template` / `Slot` / `TextLayer` / `CropTransform` v1, with serde and `templateVersion`
  - `pixlay-render`: the single `draw(doc, target)`
  - `pixlay-cli`: `render --project x.pixlay --dpi 300 --out y.jpg`
  - fixtures: 6 test images (EXIF Orientation=6, a PNG with alpha, portrait, landscape, 4:3, square)
  - golden-image pixel tests
- **Exit criteria**:
  - serde round-trip: field-by-field equality after serializing and deserializing
  - `pixlay-cli render` can read a `.pixlay` and produce an image
  - the CLI's machine surface holds: with no TTY and no stdin the command still succeeds; stdout carries machine-readable results only; exit codes match the contract;
    a change of `LANG` does not change the stdout/stderr text
  - `probe` can answer with numbers "did the slot change, is everything outside it clean, how much blends across the seam"
  - when the pixel thresholds are tightened so that passing is impossible, the tests **must go red** (verifying that the tests really check)
  - `cargo test` for `pixlay-core` and `pixlay-render` needs no display
- **Not doing**: no GUI, no multiple templates, no color grading, no text layout.
- **Human**: the contract review. **Passed (2026-09-21, human) — the ruling is in "S1 · ruling" below.**

### S1 · review additions (2026-09-20)

- **The CLI is the AI's only operating surface** (see AGENTS "Module boundaries"), so its contract is written as a machine surface and frozen at the same level as `render::draw`'s contract:
  - every subcommand supports `--json`; stdout emits machine-readable results **only**, and diagnostics/progress/warnings go to stderr; identical input yields identical output
    (no timestamps and no absolute paths in the results)
  - **zero interaction**: it does not read stdin, does not wait for a prompt, and behaves the same with no TTY; `--help` covers every flag and exit code
  - **unaffected by locale**: the stdout/stderr text is **byte-identical** as `LANG` / `LC_ALL` / `LANGUAGE` change (including the error branches).
    The rendered pixels are not under this constraint — glyph fallback for text layers really is locale-sensitive, so the tests still pin `LANG`
  - `--stats` **appends** `{ms, encode_ms, peak_rss_mb, icc}` to the original report, measured per "Measurement rules",
    and every later step reuses the same ruler (`out_w` / `out_h` / `dpi` are in the report anyway and do not appear only because of `--stats`)
  - `probe`: samples several coordinates and outputs numbers (the photo color inside a slot, the white background outside, the number of blended pixels on a shared edge between adjacent slots);
    AGENTS's "write pixel-level conclusions as a probe" is carried by it, and later steps no longer each write their own one-off script
  - `--preview-px <n>`: the scaling target of the same `draw`, producing a preview that can be inspected visually directly (A0 cannot be inspected as a whole)
  - exit codes: 0 success / 1 usage error / 2 decoding or rendering failure; on failure stderr prints the missing file path and stdout stays empty.
    A `probe` criterion not passing **does not** belong to this class: its numbers are the result (the basis of the verdict), so stdout emits all the numbers as usual,
    stderr emits a one-line summary, and the exit code is 2. See `docs/CONTRACT.md` §5
  - **v1 non-goals** (the same discipline as AGENTS "Not doing"): an MCP server, a REPL / watch, natural-language arguments,
    and reading defaults from a config file and thereby changing behavior — none of these are done
- Once `--stats` has landed, replace the second command of AGENTS "Verification entry" with the `--stats` form: every round's ruler is thereby machine-readable
- "when the thresholds are tightened so that passing is impossible the tests must go red" → changed to a **permanent self-check**: add a perturbation larger than the threshold to the golden image in memory,
  and assert that the comparison function returns failure. A one-off manual edit has no regression value
- the decisive environment is pinned inside the tests: `TZ`, `LANG`, `FONTCONFIG_FILE`, `XDG_CACHE_HOME`; the font is pinned (one ttf committed with the repository,
  or the tests use only system fonts) — otherwise the golden tests go red per machine, and S8's chroot blows up first
- fixtures: clean licensing (CC0), small size, committed with the repository; procedurally generated images use a fixed seed
- contract v1 must **list v1's non-goals explicitly** (per-slot independent text, nested groups, blend modes, rotation >±45°, source ICC preservation…),
  otherwise nobody can see that "it will not be enough later"

Conflicts between documents that this step had to settle:

- **The CLI surface of the verification entry was inconsistent with S1.** `AGENTS.md` used `render --template mosaic-8-s14 --dpi 300 --out …`,
  whereas S1 defines `render --project x.pixlay --dpi 300 --out y.jpg`. Both now exist:
  `--template <name>` (no project, no photo, used for smoke) and `--project <file>` (with photos), so `AGENTS.md`'s "must run every round" command runs from S1 on.
- **S1's fixtures mixed in S4's concerns.** "EXIF Orientation=6, a PNG with alpha" needs a decoder and EXIF parsing.
  Ruling: S1 only **commits** those files (the data is cheap and works offline), and the tests that **use** them belong to S4; S1's golden tests use procedurally generated bitmaps.

### S1 result (2026-09-20)

`[the contract reading is `docs/CONTRACT.md`; all criteria are in the tests in the repository]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| serde round-trip field-by-field equality | `pixlay-core/tests/contract.rs` | round-trip equality + assertions on field names and the `[x,y]` shape; unknown fields, a higher `docVersion`, and out-of-range slot counts/DPI/canvas/framing are all rejected |
| `render` can read a `.pixlay` and produce an image | `pixlay-cli/tests/cli.rs` | both PNG and JPEG are produced; size = the rounding of the canvas mm × dpi |
| CLI machine surface | same as above | no TTY and data on stdin still work as usual; stdout/stderr are byte-identical under four `LANG` values (including the error branches); exit codes 0/1/2 each have coverage; on failure stdout is empty (the only exception: a `probe` criterion not passing — the numbers are the result, and are emitted as usual) |
| `probe` answers three questions with numbers | `pixlay-cli/src/probe.rs` | exact in-slot color match, out-of-slot non-white count, shared-edge blended pixel count and the three-color convex combination residual |
| tightened thresholds must go red | `pixlay-render/tests/render.rs::the_comparison_can_actually_fail` | permanent self-check: a perturbation of one column of pixels (equivalent RMSE 12) must exceed the threshold, and identity must be 0 |
| core/render tests need no display | all tests | all green with no `DISPLAY`/`WAYLAND_DISPLAY` |

**S1's deviations from and additions to the contract** (the review looked at these in particular):

1. `draw`'s signature is `draw(doc, images, target)`: bitmaps are passed in through `Images` (slot → `Bitmap`).
   `Bitmap` owns a Cairo surface and **is not `Send`/`Sync`** — what a background decode hands back to the main thread is a bare buffer,
   and the receiving thread builds the `Bitmap`, the same discipline as GTK objects not crossing threads.
2. `Target.scale` and `Target.band` are separate: `scale` is for the preview and `band` is for A0 banding (peak = one band + the sum of bitmaps).
   Measured: band stitching vs the whole image has RMSE 0.033 (so S0's 941 MB peak still has room to come down; left to S4).
3. When the document contains text layers, `draw` **errors** (`TextLayersUnsupported`) instead of silently not drawing them. Once S5 is hooked up this error disappears.
4. `TextLayer` went into the v1 contract (its fields are frozen), but rendering is in S5; `TextFallback.date` is already defined.
   The contract lists the v1 non-goals explicitly (see `docs/CONTRACT.md` §6).
5. S1 has no decoder yet (that is S4's business), so `--content detail|flat` fills slots with **deterministic placeholder content**;
   that flag and `pixlay-cli/src/content.rs` are deleted in S4, and `probe` sinks down to `pixlay-imaging` along with it.
   The golden tests and those 6 fixtures do not depend on it — the fixtures are required only by "committed in the repository"
   (`pixlay-cli/tests/fixtures.rs` checks EXIF Orientation=6, the PNG alpha channel, and the size).
6. `--template mosaic-8-s14`: in S1 it is provided by hand in `pixlay-cli/src/templates.rs` (8 slots, a cut,
   one irregular slot, and coordinates that are all integer multiples of 1/8, hence exact in binary floating point), which lets `AGENTS.md`'s verification command run from S1 on.
   S2 replaces the geometry with a generator, **keeping the name and the `templateVersion` unchanged**.
7. `image` 0.25.10 stayed (S1's PNG/JPEG encoding); `cairo-rs`'s `png` feature was demoted to dev-only
   (only for golden-image read/write). `pangocairo` left with the spike and comes back in S5. Registered in `AGENTS.md`.
8. The report **contains no absolute paths** (the `out = ...` field was removed) — identical input yielding identical
   output matters more than "conveniently printing the output path"; the caller already knows what it passed.

**S1 measured** (`--release`, this machine):

| Item | Value |
|---|---|
| golden-image RMSE | 0.0 (deterministic within one build); the equivalent RMSE of one wrong column of pixels is 12.0 |
| preview vs export (2N downsample) | RMSE 2.32 (threshold 6) |
| band stitching vs the whole image | RMSE 0.033, max pixel difference 2/255, 311/463080 bytes differ |
| 8-slot 300dpi compositing (`probe`) | 623 ms, **`VmHWM` 1611 MB** (14043×10532 output surface + 8 bitmaps at in-slot size) |
| A0 landscape 10 slots 300dpi (whole image) | 592 ms compositing + 2483 ms JPEG encoding, **`VmHWM` 1470 MB**; the same image rendered in 16 bands drops to **597 MB** (output size and the per-pixel sum agree) |
| 8-slot 300dpi JPEG q90 encoding | 3727 ms → 42.5 MB; PNG not measured (no metadata requirement, left to S6) |
| seam blended pixels (8 slots, 11 shared edges, 300dpi) | 0.998–1.995 px/row, widest 2 px, worst residual 0.63/255, `foreign` all 0 |
| visual inspection | 8-slot preview: the irregular slot (orange) wraps the two edges of the lower-right gray slot; 1000 px template smoke preview: 8 color blocks, no white seam |

`VmHWM` 1611 MB is higher than S0's 941 MB, for a different reason: S0's spike drew only one L shape + a grid (fewer photo buffers),
whereas this is 8 bitmaps each generated at in-slot size, and it does not go through `band`. The 2.5 GB budget still has room; S4's buffer ladder will measure it again.

### S1 · ruling (2026-09-21, human)

`[the contract passes; the defects were fixed per the review's comments, and the repository is authoritative for what was fixed]`

- **Verdict: contract v1 passes.** S2 may start; `docs/CONTRACT.md` is the review reading and the tests are the authoritative implementation.
- **The defects the review raised are fixed** (all in `ec5ef27`; reproduce with the release binary first, then change):
  1. `Band` divided by canvas rows and each band rounded on its own → `round` is not additive: at 72dpi/scale 0.3 three bands add up to 759 rows vs 758 for the whole image;
     changed to divide on **output pixels** (`Band::out_rows`), and the stitching test covers scale 0.1/0.3/0.5/1.0. This is exactly S6's export path.
  2. A canvas whose aspect ratio differs from the template's was silently stretched (a 4:3 canvas + a 16:9 template still produced an image, exit code 0);
     `CollageDoc::validate` gained a 1e-6 cross-check → hard error.
  3. `CropTransform::zoom` had only a lower bound: `zoom=1e5` triggers a 30 PB allocation failure and aborts, and `zoom=1e308` wraps the bitmap width to `i32::MIN`;
     added `MAX_ZOOM = 1000` (document layer), and S4's decoder sets its own upper bound by memory budget.
  4. `probe` could pass vacuously: an all-empty-slot project reported `status = ok` / `occupied = 0` / exit code 0; changed to require at least one occupied slot,
     and the failure summary distinguishes "no slot that can be probed" from "criterion not met".
  5. The template library moved from the CLI to `pixlay_core::templates` (the GUI's template picker needs it); `mosaic-8-s14`'s
     areas summing to exactly 1.0 / zero overlap / no hole are already tests, and S2's generator inherits them.
  6. The rotation direction is pinned down as screen-clockwise (y pointing down); tiled text is defined as "the grid anchored at the canvas origin, each tile rotated about its own anchor, no per-tile variation";
     and the alpha rules gained tests (black 50% over white = 127, fully transparent leaves no trace, opaque is not modified).
- **The rulings the review gave and that have already landed in the contract** (`docs/CONTRACT.md` §1 / §2):
  the version policy "breaking changes allowed, no migrations written" (adding a field does not bump `DOC_VERSION`; changing a meaning or deleting a field does, and old projects are then rejected);
  a canvas/template aspect-ratio mismatch is a hard error, and the GUI's template picker groups by aspect ratio; `zoom` keeps its document-layer upper bound.
  With that, `S6.5`'s "version migration" item is cancelled.
- **The gate has been closed**: the status row, the "Where humans must step in" table and this section's "Human" item were rewritten together;
  the later shapes and limits are in `docs/CONTRACT.md`. The next action is a **new session** doing S2.
- **Left over (does not block S2)**: the table in "Open decisions → C. After S1, before S4" drew **no objection** in the review record, so by that table's own rule
  "locked as recommended unless objected to" — if something was orally rejected at review time, it must be recorded here and that table changed; otherwise S4 executes the recommendations when it starts.
  The two paths for the decoding backend are still S4's first measured question, and are not something this ruling can settle.

## S2 · Template system (geometry only) — ⬜ not started

- **Goal**: the geometry of regular and irregular templates is entirely correct and checkable.
- **Work**: irregular slots use SVG paths; regular slots can be parameterized with grid spans; **generation must be deterministic**, and the result is frozen data carrying a `templateVersion`.
- **Exit criteria**:
  - **zero overlap** between slots
  - **no interior hole** in the union
  - the area of the parsed path matches the declared area (within tolerance)
  - repeated generation with the same `templateVersion` yields a bit-identical result
  - covers 2–10 slots, at least one template each
- **Not doing**: introduce no image at all; do no rendering.
- **Human**: none, but this step ends its session: the frozen geometry is irreversible (splitting principle 5).

### S2 · review additions (2026-09-20)

- the suggestion is to **restrict the crop geometry to polygons** (or define the curve discretization tolerance explicitly): only then are area, zero overlap and no hole all analytically checkable;
  curves turn these three into "roughly right within tolerance"
- the template generator is committed with the repository (a bin, not `build.rs`), and tests: regenerate → byte-identical to the frozen data
- do not introduce an external SVG parser (AGENTS: minimal dependencies); a path is just the command list in the template data
- the correspondence between templates and canvas aspect ratios lands in this step: **the template matrix is grouped by aspect ratio** (see "Open decisions → B. Confirmed"), and S7's template picker
  filters by the current canvas's aspect ratio and lists only matches (`CollageDoc::validate` already judges a mismatched project a hard error).
  This means the `templates` subcommand has to support querying by aspect ratio, not just listing all the names.
- the CLI gains two caller-facing subcommands (S1 already froze the machine surface, and these two only add a data source):
  `templates --json` outputs template name / slot count / aspect ratio, and `init --template <name> --out x.pixlay` outputs a loadable default project —
  so callers (including the AI) can pick a template without reading the source and do not have to hand-write `.pixlay` JSON (no `schemars` pulled in)
- **The entry command depends on S2's product.** `mosaic-8-s14` does not exist as generator output until S2. Ruling: the S2 exit criteria name that template
  (8 slots, irregular, a fixed `templateVersion`, a reproducible generator); before S2, treat that command as "valid from S2 on".

## S3 · Framing and clamp — ⬜ not started

- **Goal**: the in-slot framing math is entirely correct, including rotation by any angle.
- **Work**: absolute zoom (displayed width / canvas width), offset, rotation; "parent container clips + child primitive transforms"; recompute the clamp after a rotation or a slot change. Fake image sizes are fine; no image pipeline is needed.
- **Exit criteria**:
  - sweep (rotation × zoom × offset × each slot shape), and after the clamp the photo **always covers the entire slot**
  - a change of rotation angle triggers a clamp recomputation, covered by a test
  - crop edges only, never grow the canvas: the canvas size is unchanged under any framing
- **Not doing**: no GUI gestures; no image decoding.
- **Human**: none. The clamp contract shape was already ruled on in "Open decisions → B. Confirmed", so this step runs to completion without a human answer.

### S3 · review additions (2026-09-20)

- **The degradation policy was ruled on before the criteria were written.** The AGENTS "Open / to be proven" entry (elongated slots need 6.7–7.6× zoom) changes the clamp contract
  (the clamp result must be able to report "the rotation was limited"), and the contract was frozen in S1 → this had to be decided **before the S1 contract review**, not left to S3.
  It was: see "Open decisions → B. Confirmed", elongated-slot clamp degradation.
- the epsilon for "covers the entire slot" is given a number (normalized 1e-6 suggested, or ≤0.5px at 300dpi), written into the test constants
- hit testing (see S6.5) and clamp are both geometry and can be merged into this step

## S4 · Image pipeline — ⬜ not started

- **Goal**: by the time a bitmap enters rendering it is right in decoding, orientation, color and bit depth.
- **Work**: `pixlay-imaging` — glycin decoding (including HEIC/AVIF), automatic EXIF rotation, resampling (`sRGB → linear → process → sRGB`, Lanczos3), a 16-bit intermediate buffer, per-slot color grading + a global uniform filter. The structure for background work returning to the main thread through a channel is settled first.
- **Exit criteria**:
  - **grading identity**: with `factor=1, s=1, Δ=0` the output is pixel-identical to the input
  - HEIC decodes; EXIF Orientation=6 is automatically rotated upright
  - large-ratio downsampling (4000px → 400px) is visually free of aliasing and mush
  - the intermediate buffer is 16-bit, and quantization happens only at the end of the pipeline
- **Not doing**: no grading UI.
- **Human**: none, but this step ends its session: the decoding backend it picks is irreversible and determines S8's `depends` (splitting principle 5).

### S4 · review additions (2026-09-20)

- **Decoding is the first task**: run one real decode down each of the two paths in "Open decisions → the two paths for the decoding backend" (including HEIC, including EXIF Orientation=6) and pick one with numbers.
  This decision determines S8's `depends`.
- make the **buffer ladder** explicit: which stages are 16-bit, and at what resolution. A full-canvas 16-bit RGBA measures 1064 MB, and 10 images cannot be computed that way.
  Suggested: decode → 16-bit linear → **downsample to in-slot display size** → color grading → global filter → sRGB 8-bit → Cairo;
  peak = Σ (in-slot-size buffers) + the output surface = O(output pixels)
- decoding gets a size cap (otherwise a 100MP phone image goes straight into memory); concurrency is capped by the memory budget, not by `nproc`
- color decisions go into the contract: whether the source ICC is respected, whether the output ICC uses sRGB v2 or v4, CMYK JPEG, **how source alpha is handled**
  (S1's "PNG with alpha" fixture currently has no corresponding expected behavior)
- "downsampling visually free of aliasing" → an RMSE threshold against an independent implementation (ImageMagick `-filter Lanczos -resize`),
  plus a zone-plate check of the aliasing energy
- `--content detail|flat` and `pixlay-cli/src/content.rs` are deleted here, and `probe` sinks down to `pixlay-imaging` (see S1's deviations, item 5)

## S5 · Text layers — ⬜ not started

- **Goal**: canvas-level text is correct, and both forms go through the same mechanism.
- **Work**: a text layer is the basic unit, and a tiled watermark is one of its modes; dynamic EXIF fields such as `{date}`; Pango + pangocairo layout.
- **Exit criteria**:
  - free placement and a tiled watermark both produce an image
  - `{date}` is filled correctly from EXIF; **the fallback behavior when EXIF is missing has a test**
  - CJK kinsoku and punctuation squeezing are visually correct
  - the text position is stable after a rotation (verifying the "content layers run last" constraint)
- **Not doing**: no text editing UI.
- **Human**: the CJK kinsoku and punctuation-squeezing feel, judged by eye (see "Where humans must step in").

### S5 · review additions (2026-09-20)

- "CJK kinsoku is correct" is checkable: build text whose line start has forbidden punctuation, and after Pango breaks the lines use `pango_layout_get_line*`
  to assert that no line starts with a forbidden character; punctuation squeezing asserts that the spacing between adjacent punctuation is smaller than the default spacing
- the font size must use **normalized canvas-relative units**, otherwise the preview/export RMSE criterion fails immediately

## S6 · Export — ⬜ not started

- **Goal**: both export modes are correct, and metadata is done in one pass.
- **Work**: physical size + DPI mode; specified long-edge-in-pixels mode; encoding and metadata inside the same pipeline.
- **Exit criteria**:
  - physical-size mode: the file carries the correct DPI and ICC
  - long-edge-pixel mode: the output long-edge pixel count matches the request exactly
  - chroma subsampling matches the request (specifically catching the two-pass metadata trap)
  - A0 can produce PNG / JPEG / TIFF
- **Not doing**: no export UI; no PDF (not in scope).
- **Human**: none. The one visual question this step inherits is S4's downsampling quality.

### S6 · review additions (2026-09-20)

- the implementation meaning of "encoding and metadata in one pass": **cairo supplies pixels only, the encoder writes the metadata itself**. Measured:
  `cairo_surface_write_to_png` on A0 writes only IHDR/bKGD/IDAT — **no pHYs, no iCCP**
  (`identify` reports `Units: Undefined`), and `set_fallback_resolution` has no effect on a bitmap backend. Writing PNG through cairo necessarily loses DPI.
- the per-format field list is written into the criteria: PNG = pHYs + iCCP (+ sRGB chunk); JPEG = JFIF density or EXIF resolution + APP2 ICC;
  TIFF = XResolution/ResolutionUnit + ICCProfile
- the rounding rule of long-edge-pixel mode, and "what DPI is written in that mode", must be defined and tested (AGENTS requires every output to carry DPI)
- the time cap uses a relative value (≤ 3× the baseline); the baseline is in "Measured baseline"
- **the path where artifacts land on disk**: on this machine `/tmp` is tmpfs (7.5 GB), an A0 photo-content PNG is 342 MB and a TIFF 476 MB, so writing tmpfs costs another copy in memory
- banding is the export path's memory lever: `Band::out_rows` partitions output pixels, and S1 measured A0 landscape 10 slots at 300dpi dropping from 1470 MB whole to 597 MB in 16 bands (see S1's ruling, item 1)

## S6.5 · Command history / project IO / hit testing (still windowless) — ⬜ not started

Added by the 2026-09-20 review.

- **Goal**: undo/redo and hit testing are correct and testable before any window exists, so S7 only has to bind them to events.
- **Why it is its own step**: S7 says "undo/redo (command history + AST snapshots)" and "hit testing". Both are pure `pixlay-core` logic, testable without a window,
yet they are the parts of the GUI most likely to go wrong; putting them into S7 breaks two splitting principles at once (S7 is the last window step; machine-checkable things should not wait until then).

- **Work**: `Command` + an undo stack; `.pixlay` save/load (atomic write tmp + rename); point → slot hit testing (including rotation and irregular slots).
  **Write no version migration** (S1 review ruling): adding a field does not bump the version, changing a meaning or deleting a field does, and old projects are then rejected with a prompt to rebuild.
- **Exit criteria**:
  - after any operation sequence is undone continuously back to the initial state, the pixels `draw` produces are **pixel-identical** to the initial state; redo is isomorphic
  - save → load → save again is byte-identical; a missing file / bad version must give a clear error + a non-zero exit code
    (version policy in `docs/CONTRACT.md` §1: "breaking changes allowed, no migrations written")
  - hit-test sweep (each template × each slot's centroid × 1px outside each slot's boundary), with results matching the analytic geometry solution
- **Not doing**: no UI event binding, no gestures.
- **Human**: none.

## S7 · GTK shell and interaction — ⬜ not started

**This is where windows appear for the first time.** The layer underneath has been locked down by S1–S6, so problems from this step can only be in the GUI.

- **Goal**: walk through the main path in under three minutes.
- **Work**: a gtk4 + libadwaita shell (pick containers and styling per `AGENTS.md`'s "GNOME HIG" section); drag photos in; drag inside a slot / wheel / double click; straighten with reference lines; edit text layers; undo/redo (command history + lightweight AST snapshots); hit testing; background decoding back to the main thread; **copy goes through i18n** (English is the source string; this step produces no translation).
- **Exit criteria**:
  - "pick a template → place photos → adjust framing → export" can be walked through end to end
  - with `LANG` unset, `LANG=C`, and `LANG=<unknown language>` the interface is English and starts up
  - the set of files listed in `po/POTFILES` == `crates/pixlay/src/**/*.rs` (machine-comparable), and the `.pot` is committed with the repository;
    whether any copy missed its wrapping is caught by this step's visual criterion (see "Where humans must step in")
  - normal under Wayland, with no blocking UI (decoding/scaling are in the background, the main thread does not stall)
  - what the window renders is pixel-identical to `pixlay-cli render` at the same canvas size (within the RMSE threshold)
  - the machine-checkable subset of GNOME HIG is all green (shortcut table, accessible names, keyboard reachability, narrow-width reflow, starts under both styles;
    details in "S7 · GNOME HIG additions"; the visual part goes item by item through `docs/HIG-REVIEW.md`)
- **Not doing**: no mode switching; no extra panels; no translation (language packs are added later, and this step only guarantees extractability and fallback).
- **Human**: the three-minute main path, walked by hand (see "Where humans must step in").

### S7 · review additions (2026-09-20)

- "walk it through in three minutes" is given as a human criterion (see "Where humans must step in"): a scripted step list + timing
- one addition: the UI does not freeze while a large image exports (export in the background + progress feedback) — S0/S6 operations are on the 6–7 s scale
- **the i18n mechanism (decided)**: `gettext` (crate `gettext-rs`, **no pinned version** — per "track the latest" it is decided by S7's `cargo update`),
  domain `pixlay`, source language English, `.pot` + `po/POTFILES` committed with the repository; the dependency goes only into `pixlay` (AGENTS already forbids
  core/imaging/render/cli from pulling in i18n); that dependency is registered in the "Dependency registry" together with the other new ones.
  *Rationale: GTK and libadwaita's own button copy goes through the system gettext, and the translations of `.desktop` and AppStream metainfo (S8) use the same
  `xgettext` / `msgfmt` toolchain — switching to a fluent-style scheme means wiring up the metainfo half yourself.*
- the extraction command uses **`xgettext --language=Rust`**: gettext-tools 1.0's Rust backend was measured to extract
  `gettext` / `ngettext` / `pgettext`, and to tag `ngettext`'s two msgids with `#, rust-format`;
  **do not take the `--language=C` detour** — it extracts the strings but loses `rust-format`, so `msgfmt --check-format` cannot validate the `{}` placeholders
  (the behavior of extraction and `msgfmt` was measured on 2026-09-20 on this machine's `gettext-tools 1.0`, not recalled)
- `msgfmt --xml` (metainfo) and `msgfmt --desktop` (desktop files) go through the same pipeline as `.po` in S8
- criteria addition: with `LANG` missing / `C` / an unknown language, the GUI shows English and starts (missing-translation fallback is gettext's default behavior,
  and one smoke test suffices); this step **commits no `.po` translation**, it is enough that extraction and fallback hold
- the boundary for hardcoded strings: whether copy missed its wrapping **is not a machine criterion** (the extractor cannot see the strings that were missed), only the
  set comparison between `po/POTFILES` and `crates/pixlay/src/**/*.rs` plus visual inspection

### S7 · GNOME HIG additions

A "GNOME HIG" section has been added to `AGENTS.md`; only the checkable subset goes here, and the rest goes into `docs/HIG-REVIEW.md`.

- **Shortcuts**: the `GAction` accelerator table ⊇ HIG `reference/keyboard`'s required set (`Ctrl+Q` / `Ctrl+W` /
  `Ctrl+O` / `Ctrl+S` / `Ctrl+Z` / `Shift+Ctrl+Z` / `Ctrl+A` / `Ctrl+?` etc., taking the subset matching the features this product actually has),
  and ∩ the system-reserved set = ∅ (`Alt+*`, `Super+*`, `Ctrl+Alt+*`). The two tables are copied into test constants and are not written into `AGENTS.md`.
- **Accessible names**: walk the widget tree and assert that every interactive control's accessible name/role is non-empty — GTK4 ships
  `gtk_test_accessible_*` (gtk4-rs wraps it), with no third-party tool needed. This corresponds to HIG "All interface elements should
  have descriptive, accessible names".
- **Keyboard reachable**: every `GAction` has a keyboard path (an accelerator, or a focusable / mnemonic-bearing control) — HIG "every action
  should also be possible with the keyboard".
- **Adaptive**: window minimum size + reflow at narrow widths (HIG `guidelines/adaptive`); assert that at the minimum size the canvas is not cropped and
  no control is squeezed out of existence.
- **Styling**: it starts and renders the main path under both `ADW_COLOR_SCHEME_FORCE_DARK` and `FORCE_LIGHT`;
  **the same canvas is pixel-identical under both styles** (the white background is content) — this criterion also pins down "large text must not scale canvas text layers".
- **Main menu**: contains the `Ctrl+?` shortcuts dialog and `AdwAboutDialog` (app-id / version taken from `APP_ID` and the package metadata,
  not hardcoded a second time).
- **Visual inspection**: go through `docs/HIG-REVIEW.md` item by item (high contrast, large text, keyboard-only, screen reader, touch and OSK, spacing and hierarchy,
  animation feel); the wording of HIG `guidelines/writing-style` also goes only through that checklist.

## S8 · Packaging — ⬜ not started

- **Goal**: get into AUR.
- **Work**: `PKGBUILD`, a desktop file, icons, a dependency list; the translations under `po/` and the multilingual fields of `.desktop` / metainfo.
- **Exit criteria**:
  - `cargo vendor` passes, `cargo build --frozen --offline` passes
  - `makepkg` succeeds in a clean chroot
  - the desktop file and icons pass validation
  - the installed package starts and can walk through the main path
  - `msgfmt --check` passes; with no language pack the interface is English (a chroot usually has no locale, and this criterion is exactly what that covers)
- **Not doing**: no Flatpak / Snap / other distributions.
- **Human**: none. The install-and-walk-the-main-path criterion is verified on the built package, not judged.

### S8 · review additions (2026-09-20)

- running tests inside `check()` has to be tiered: heavy tests (A0, HEIC, font layout) are marked `#[ignore]` or feature-gated,
  and `check()` runs only the fast tier — a clean chroot has no font cache, no `$HOME`, no display
- add the `.pixlay` MIME registration (a shared-mime-info xml + desktop file + icon); the `depends` list has to be determined by the decoding backend
- add AppStream metainfo: the file name and `<id>` **must equal the app-id** (`org.yangtse.Pixlay.metainfo.xml`),
  `<url type="homepage">` points at the actual page under `yangtse.org`; the PKGBUILD's `url=` and `pkgdesc` likewise use English and point at that page.
  The consequence of missing metainfo is not an error, but a software center with no name, no screenshots and no icon
- i18n lands: the multilingual fields of `.desktop` and AppStream metainfo (S7 already decided gettext) are installed together with `.po`;
  `msgfmt --check` inside `check()` must be runnable offline, and **with no language pack at all the interface is still English**
- `makepkg`'s `check()` must be runnable offline → all fixtures are inside the repository, and `cargo vendor` must not miss test dependencies

---

## Measurement rules

The precondition for numbers to be comparable across steps.

- peak memory = `VmHWM` from `/proc/self/status` (or `getrusage.ru_maxrss`), not RSS sampling
- time = `CLOCK_MONOTONIC` wall clock, with compositing and encoding reported separately
- test content must be non-flat
- artifacts are written to disk (`/var/tmp` or `$XDG_CACHE_HOME`), not to `/tmp`
- every threshold constant is annotated in the code with its **source** (measured value + date) — only then does AGENTS's "Every rule a test can enforce lives only in the tests" hold

## Open decisions

Divided into three tiers by "how much rework a wrong choice causes". The third column is the recommendation; **locked as recommended unless objected to**.

### A. Already landed (2026-09-20)

The scaffolding entered the repository, and `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test` / `cargo build --release` are all green.

- build profile: the baseline is always `--release`; `[profile.release]` is frozen at `lto = "thin"`, `codegen-units = 1`, `debug = 1`
- `[profile.dev.package."*"] opt-level = 2`: it applies only to **external dependencies** (measured: workspace members do not get `-C opt-level` and stay at 0),
  and its purpose is to make S4's image pipeline usable under dev without slowing down iterative compilation of our own crates
- workspace: `edition = "2024"`, `resolver = "3"`, `rust-version = "1.98"` (following Arch's installed rustc,
  see AGENTS's "Version policy: track the latest" rather than the edition floor); the five crates under `crates/*` are all present (see below)
- license: each crate has `license = "GPL-3.0-or-later"` + a root `LICENSE` (the SPDX original) + `publish = false`
- gates: `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` are already in the "Verification entry";
  lints are set uniformly in the root `Cargo.toml`'s `[workspace.lints]` (`unsafe_code = deny`, clippy `all` all deny,
  `dbg_macro` / `todo` / `unimplemented` deny — the last two also mechanically close off the "leave a TODO placeholder" route)
- CI: not done for S0–S6; S8 adds an Arch container job

The scaffolding as it stands (for the next session):

    Cargo.toml            workspace + profile + lints; members = crates/*
    rustfmt.toml          edition/style_edition = 2024
    LICENSE               GPL-3.0-or-later (the SPDX original, 232 lines)
    crates/pixlay-core    doc skeleton, content from S1/S2/S3/S6.5; gtk/cairo forbidden
    crates/pixlay-imaging doc skeleton, S4; exposes synchronous pure functions (threading is the caller's)
    crates/pixlay-render  doc skeleton, S1; the single draw(doc, target)
    crates/pixlay-cli     the bin is named `pixlay-render` (not `pixlay-cli`); from S1 it is a lib + bin:
                          the lib exports `cli::run` (integration tests call it directly) and the bin is just a shell; gtk4 forbidden
    crates/pixlay         lib skeleton, pinning only `APP_ID`; the GUI and the `pixlay` bin target are added in S7
                          (so that S0–S6's `cargo test` does not have to compile gtk4-rs)

Not done: `cargo vendor` currently has empty dependencies, so this criterion cannot be verified; verify it after S1 introduces the first batch of dependencies.

### B. Confirmed (2026-09-20) — must be obeyed before the S1 contract is frozen

| Item | Decision |
|---|---|
| canvas preset set | A4 / A3 / A0 each portrait and landscape + 1:1 + 3:2 + 4:3 + 16:9 + custom; `Template` **declares the aspect ratio it belongs to**, and the template matrix is grouped by aspect ratio |
| `.pixlay` format | JSON (`serde_json`) |
| project version policy | add `docVersion`; reading a higher version **is rejected outright with an error**, no guessing, no downgrading |
| `draw`'s target | `{ cairo ctx, scale, band }` — preview scaling and A0 banded rendering are both just caller arguments and do not change the renderer |
| error type policy | core/render use `thiserror` typed errors; `anyhow` appears only in `pixlay-cli` |
| imaging thread model | exposes **synchronous pure functions**, with no threads/channels; concurrency belongs to the caller (the GUI returns to the main thread through a channel itself) |
| in-slot source alpha | composited onto **opaque white**; an export is never transparent; preview and export pixels are identical |
| text layer v1 | font size **normalized**; tokens only `{date}` / `{filename}` / `{index}`, the rest goes into "v1 non-goals" |
| `{date}` semantics | takes EXIF `DateTimeOriginal` **verbatim, with no timezone conversion**; when missing, falls back to the string stored in the project; tests pin `TZ` |
| undo granularity and snapshots | one gesture = one command (committed when the drag ends); a snapshot stores the whole `CollageDoc`, with no diff |
| config storage | serde files under `~/.config/pixlay/` (the same scheme as `.pixlay`); **no GSettings** |
| limit constants | canvas ≤ **200 MP**, DPI **72–600**, slot count **2–10**; out of range gives a clear error, not a panic (A0@300dpi = 139.5 MP, leaving 43% headroom) |
| Elongated-slot clamp degradation | when the required zoom is > **1.5×**, **limit the rotation angle**; the clamp result carries a "limited" flag for the UI |
| `.pixlay` path resolution | relative to the project file; a missing file = a clear error + a non-zero exit code; atomic write (tmp + rename) |

### C. After S1, before S4 (still recommendations; locked as recommended unless objected to)

| Question | Recommendation |
|---|---|
| source ICC | v1 does not read the source ICC and interprets everything as sRGB, and the documentation states that this is a known limitation (doing it properly needs lcms2 + a rendering-intent definition) |
| output ICC | embed the sRGB IEC61966-2.1 profile bytes; do not pull in lcms2 |
| decoding backend | see "The two paths for the decoding backend" below; measure first, then decide, **and this decision determines S8's `depends`** |
| fonts | production does not bundle fonts (Noto Sans CJK is too large); the golden text tests use a small test font committed in the repository, and checks that render with system fonts are marked `#[ignore]` |
| dependency registry | every new dependency is registered in AGENTS (name / version / why / size); S1 registers the first batch in one go |
| AUR package name and version | package name `pixlay`; the release tag `vX.Y.Z` is pushed to GitHub, and the PKGBUILD's `source=` uses the tag tarball |

**The two paths for the decoding backend** (source: measured on the `glycin-core 4.0.0` / `glycin 4.0.0` source, not recalled):

- `glycin-core 4.0.0`'s `COMPAT_VERSION = 2` → it recognizes `/usr/share/glycin-loaders/2+/conf.d/`,
  which is **compatible** with the `2+` loaders Arch's `glycin` package already installs (there is no "crate 4 needs loader 4+" problem).
- But the `glycin` facade **hard-depends on `glycin-external`** on Linux (non-optional in
  `[target.'cfg(target_os = "linux")'.dependencies]`), i.e. it must go through a **sandboxed loader process**: it needs libseccomp, bwrap, the system loader packages,
  and a D-Bus connection (the loader binary wants `--dbus-fd`). That measurement on this machine where `glycin-thumbnailer` failed for every format
  is a problem of the distribution's binary, and is not the same thing as this crate path.
- **The self-contained option**: depend directly on `glycin-builtin` (= `glycin-core` + `builtin`) + the `builtin-image-rs` feature,
  with the loader in-process, needing no bwrap / D-Bus / distribution loader packages. The cost:
  1. `glycin-image-rs` covers PNG/JPEG/WebP/TIFF/GIF/AVIF and so on, **but not HEIC**;
  2. on Arch, HEIC is a separate `glycin-heif` loader (going through libheif), so the self-contained option has to find another path.
- S4's first step: run one real decode down each of the two paths (including HEIC, including EXIF Orientation=6) and pick one with numbers.

### Decided

- `app-id = org.yangtse.Pixlay` (own domain `yangtse.org`; settled 2026-09-20, which also closed the review's "the `<user>` in `io.github.<user>.Pixlay` is undecided" item — it had to be fixed before S7 wrote code, or S8's app-id and desktop file would be redone)
- repository `YangtseSu/pixlay` (private)
- commit discipline and language conventions (English)

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

## After completion

Once S0–S8 are all ✅, this file can be deleted. By then the constraints that should exist are in `AGENTS.md`, and the specifications that should exist are in the tests.
