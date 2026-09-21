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

**Current progress: S6.5 — ✅ done (2026-09-21, no unfinished work; it is windowless and gate-free, so it closes with its own Result and tests and nothing waits on a human)**
**Next action: start S7 (the GTK shell and interaction), in a new session. The boundary list's session ended with the S5+S6 pair and this session took S6.5, which adds no gate of its own; S7 opens a session of its own because it is the last window step and ends with the three-minute main path, which only a human can walk.**

| Step | Status | Date | What it delivers |
|---|---|---|---|
| S0 · Cairo limit spike | ✅ done | 2026-09-20 | Cairo renders A0@300dpi inside the budget: 185/551 ms compositing, 941 MB peak `VmHWM`, both formats written. Gate passed: Cairo stays |
| S1 · Minimal contract + feedback loop | ✅ done | 2026-09-20 | `CollageDoc` v1 frozen, the single `draw`, `pixlay-render render` produces images, `probe` answers in numbers. Gate passed 2026-09-21 (review 1), defects from review 2 fixed the same day |
| S2 · Template system (geometry only) | ✅ done | 2026-09-21 | 12 templates covering 2–10 slots, grouped by aspect ratio, generated on a dyadic lattice and frozen under a `templateVersion`; `templates` and `init` added to the CLI |
| S3 · Framing and clamp | ✅ done | 2026-09-21 | `CropTransform::fit`: absolute-zoom framing whose request is fitted to the slot by raising the zoom, clamping the pan and, past `CLAMP_ZOOM_LIMIT`, limiting the rotation; applied by `draw`, exact on all 64 shipped slots |
| S4 · Image pipeline | ✅ done | 2026-09-21 | `pixlay-imaging`: the sandboxed glycin decoder (HEIC included), EXIF orientation applied to the pixels, a 16-bit linear Lanczos3 resample of the region each slot shows, per-slot grading and the global filter, the buffer ladder, and the probe moved down from the CLI |
| S5 · Text layers | ✅ done | 2026-09-21 | Canvas-level text in `pixlay-render` with Pango: free placement and tiled watermark through one mechanism, `{date}`/`{filename}`/`{index}` from the slot's photo, kinsoku by Pango and punctuation squeezing through the font's `halt`; `text` added to the CLI and the probe refuses text documents |
| S6 · Export | ✅ done | 2026-09-21 | Physical size + DPI and long-edge-pixels modes, pixels and metadata written in one pass: `pixlay-imaging::encode` (PNG/JPEG/TIFF) + `icc`, `--long-edge` / `--chroma`, `dpi_for` |
| S6.5 · Command history / project IO / hit testing | ✅ done | 2026-09-21 | `Command` + snapshot undo/redo, atomic `.pixlay` save with relative-path rebasing, `Template::slot_at` hit testing; `hit` and `save` added to the CLI |
| S7 · GTK shell and interaction | ⬜ not started | — | The window: the three-minute main path, keyboard and HIG conformance, i18n wiring |
| S8 · Packaging | ⬜ not started | — | `PKGBUILD`, desktop file, icons, metainfo, translations; installable from AUR |

**How a status is marked.** The status row above, the marker on the step's own heading, and the "Current progress" line are three renderings of the same claim and must always agree; changing one is part of closing the step. A step becomes ✅ only when its gate ruling is on disk — the five parts listed in `AGENTS.md` "Session and persistence discipline" (ruling block + status row + the gate entry under "Where humans must step in" + `docs/CONTRACT.md` + commit), with nothing missing. A step that is waiting on a human answer is ⏸, not 🚧.
A step whose "Human" line says **none** has no ruling to write: it closes with its **Result** subsection (criteria → landing point → measured, plus the decisions it made), the status row, the "Current progress" line, whatever shape it changed synchronized into `docs/CONTRACT.md`, and the commit — so S2, S3 and S6.5 close this way and not through the gate list.

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
| After S4 | Look at the downsampling: is a 4000 px photo in a 400 px slot free of aliasing and mush? The numbers are on disk (RMSE 1.41 against ImageMagick's Lanczos, a 16x separation from one-sample-per-texel on the zone plate), the preview is `/var/tmp/verify-preview.png`, and S4's exit criteria call this the one human criterion of the step | ✅ passed (2026-09-21, human): **the downsampling passes** — the resampler stays as built, and the optional permanent threshold was declined (see "S4 · ruling") |
| After S5 | Look at the text: do CJK line breaks and squeezed punctuation read right? The preview is `/var/tmp/pixlay-s5/text-preview.png` (the fixture `crates/pixlay-cli/tests/fixtures/text.pixlay`: a wrapped caption with `：“`, `。”` and a break inside a sentence, a `{date}` line, and a tiled watermark), the numbers are in "S5 · the one visual criterion", and S5's exit criteria call this the one human criterion of the step | ✅ passed (2026-09-21, human): **the text passes** — kinsoku stays Pango's and squeezing stays `halt`; line-end trimming stays a non-goal (see "S5 · ruling") |

**The gate's closing action** is in `AGENTS.md` "Session and persistence discipline": a ruling block + the status row + this table's entry + `docs/CONTRACT.md` + commit,
and it is not done if one of the five is missing.

The remaining steps complete through the model's automatic loop, except the **visual criteria** (what could be computed has already been turned into computable quantities in the exit criteria):
S7's "three-minute main path" and whether interface copy has any missed wrapping, and
S7's GNOME HIG visual checklist (`docs/HIG-REVIEW.md`: high contrast / large text / keyboard-only / screen reader / touch and OSK).
S0's text readability, S4's downsampling quality and S5's kinsoku and punctuation squeezing have all been ruled on (2026-09-20 / 2026-09-21).

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
3. When the document contains text layers, `draw` **errors** (`TextLayersUnsupported`) instead of silently not drawing them. Once S5 is hooked up this error disappears. *(Resolved in S5: the variant is gone and the layers are drawn.)*
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

### S1 · contract review 1 (2026-09-21, human)

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
- **The gate was NOT closed at the time** — this review's verdict was never written to disk, which is
  why it is reproduced as review 1 above and why review 2 had to happen. The `docs/CONTRACT.md` state
  it left behind is sound; only the record was missing.
- **Left over (does not block S2)**: the table in "Open decisions → C. After S1, before S4" drew **no objection** in the review record, so by that table's own rule
  "locked as recommended unless objected to" — if something was orally rejected at review time, it must be recorded here and that table changed; otherwise S4 executes the recommendations when it starts.
  The two paths for the decoding backend are still S4's first measured question, and are not something this ruling can settle.

### S1 · contract review 2 (2026-09-21, human)

`[the contract was re-read against the release binary, not against the code comments; four defects and three optional items were raised and all seven are now fixed]`

This review happened because the first ruling was never written to disk: the session that ran it
committed the defect fixes and left the gate open. It re-checked every normative claim in
`docs/CONTRACT.md` by running the built binary against hand-written `.pixlay` files — 22 scenarios —
and found the contract had drifted from the implementation.

- **D1 — the contract's own example did not load.** §1's `jsonc` block declared a `297.0 × 210.0`
  canvas (aspect 1.4143) against a template whose `aspect` is `4/3` (1.3333), while §2 makes that
  mismatch a hard error. Copying the example gave `exit 2: canvas aspect 1.4142857142857144 does not
  match the template aspect 1.3333333333333333`. The example now uses `280.0 × 210.0` (exactly 4:3)
  and two slots whose areas sum to exactly 1.0, and `crates/pixlay-cli/tests/cli.rs`
  `the_contract_example_is_a_valid_document` extracts the block **out of the document itself** and
  loads it, so the two can no longer drift.
- **D2 — `TextLayer` positions were not bounded.** §1 promises every coordinate is normalized to
  `[0,1]`, but `validate` only checked `is_finite`, so `position: [5.0, 5.0]` loaded and was refused
  later by the unrelated S5 text gate — the real defect was hidden until S5. Bounds are now checked,
  with `text_position_must_be_on_the_canvas` covering the corners (inclusive) and beyond.
- **D3 — the limit table was incomplete.** It claimed to be the complete list, but three enforced
  limits were missing: `template.aspect ∈ 0.1..=10.0`, the slot-outline rule, and
  `--preview-px ∈ 1..=20000`. All three are in §2 now, and the table states that a limit absent from
  it is a contract gap.
- **D4 — the `--stats` field list was wrong in two places.** `probe` emits no `encode_ms` (it does not
  encode), and `--help` did not mention `encode_ms` at all. §5 and the usage text now agree with each
  other and with the binary.
- **Optional items, also fixed**: the third `OutOfRange` misuse — a tiled step of 0 or less reported a
  range up to `1.0` that was never enforced — is now its own `InvalidTiledStep` error, and the
  contract says explicitly that there is no upper bound; `source` accepting an absolute path is
  documented; the clamp row in "Open decisions → B. Confirmed" names `CLAMP_ZOOM_LIMIT` and
  `CropFit::rotation_limited` so S3 can find them.
- **Verified green in the same pass** (no change needed): slot/DPI/canvas-pixel/edge/rotation/zoom
  bounds including their exact edge values, the version policy, `deny_unknown_fields`, empty-slot
  white, relative-path resolution, the slot-area cross-check, unknown text tokens, `draw` refusing
  text layers, stdout purity on usage errors, byte-identical output under four locales, stdin being
  ignored, exit codes, and the `probe` "numbers on stdout, verdict on stderr" rule.
- **Residual, not a defect**: `text` is refused by `draw` until S5, so the §1 example is a valid
  document but is not renderable as written. The example says so. *(S5 removed the residual: the
  example renders as written, and `docs/CONTRACT.md` §1 no longer carries the warning.)*
- **Verdict: the contract still passes after these fixes.** All seven items were ruled *fix now*, not defer — D1 and D3 make
  the contract lie about itself, and D2 is a validation gap that S5 would turn into a visible bug.
- **The gate's five parts are complete** with this commit, so **S2 may start in a new session**. The
  tables under "Open decisions → C. After S1, before S4" drew no objection and stay locked as
  recommended; the decoding backend is still S4's first measured question.

## S2 · Template system (geometry only) — ✅ done (2026-09-21)

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

### S2 result (2026-09-21)

`[all criteria are in the tests in the repository; the numbers below are the release binary's output on this machine]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| zero overlap between slots | `pixlay-core/tests/templates.rs::slots_never_overlap_and_leave_no_hole` | 12 templates, 262144 samples each, **0 overlapping pairs**. Exact, not sampled: the coordinates are dyadic and the samples sit at cell centers (see below) |
| no interior hole in the union | same test, plus `a_non_cut_template_is_a_gutter_not_an_interior_hole` | 11 cut templates: **0 uncovered samples**. The one non-cut template (`grid-4-2x2g`): its uncovered samples are a gutter that a flood fill reaches from the border, so **0 sealed samples** |
| parsed path area matches the declared area | same file, `every_coordinate_lies_on_the_lattice_and_areas_are_exact` (per slot, every template) and `cut_templates_declare_areas_summing_to_exactly_one` | declared area == outline area, **exactly** (not within tolerance), and the cut templates sum to **exactly 1.0** |
| repeated generation with the same `templateVersion` is bit-identical | `the_frozen_data_is_exactly_what_the_generator_produces` | the bin run twice: `sha256 ba3ce76e…` both times and equal to the committed `frozen.rs`; regeneration is byte-identical, and the served data equals a fresh `generate()` |
| covers 2–10 slots, at least one each | `every_slot_count_from_two_to_ten_is_covered` | all nine counts present: 2×2, 3×1, 4×3, 5×1, 6×1, 7×1, 8×1, 9×1, 10×1 |

**The matrix** (library order, which is by slot count; `templates` prints exactly this):

| # | name | slots | aspect | layout |
|---|---|---|---|---|
| 0 | `strip-2-1x2` | 2 | 2:3 | two bands, portrait |
| 1 | `strip-2-2x1` | 2 | 3:2 | two equal columns |
| 2 | `strip-3-3x1` | 3 | 16:9 | three columns, 5/16 · 6/16 · 5/16 |
| 3 | `grid-4-2x2` | 4 | 1:1 | equal 2×2 |
| 4 | `grid-4-2x2g` | 4 | 1:1 | 2×2 with a 1/16 gutter (**not a cut template**) |
| 5 | `strip-4-4x1` | 4 | 16:9 | four equal columns |
| 6 | `mosaic-5-hero` | 5 | 4:3 | left half + four stacked panels |
| 7 | `grid-6-3x2` | 6 | 3:2 | 3×2, unequal columns |
| 8 | `mosaic-7-t4b3` | 7 | 4:3 | a band of four over a band of three |
| 9 | `mosaic-8-s14` | 8 | 4:3 | **S1's frozen geometry**, one L-shaped slot |
| 10 | `grid-9-3x3` | 9 | 1:1 | 3×3, unequal columns and rows |
| 11 | `strip-10-10x1` | 10 | 16:9 | ten columns |

### S2 · decisions this step made

- **Polygons only, and the lattice is what makes the invariants exact.** The S2 review offered "restrict the geometry to polygons, or declare a
  curve discretization tolerance"; restricting to polygons removes the tolerance, which is why the three geometric criteria are equalities here.
  On top of that, every coordinate is an integer multiple of **1/32** of a canvas edge: areas are sums of exactly representable terms, so
  "sums to exactly 1.0" is `==`, and the coverage grid is exact too — `512` samples per axis is a multiple of `32` and the samples sit at cell
  centers (`(i + 0.5)/512`), while every edge lies on an even/1024 line, so no sample is ever ambiguous. The tests assert this lattice rather than
  assuming it (`every_coordinate_lies_on_the_lattice_and_areas_are_exact`), because the whole file's exactness claim rests on it.
- **The generator is a recipe table plus a committed artifact.** `pixlay-core/src/templates.rs` became `templates/{mod,generator,frozen}.rs`:
  `generator.rs` holds one recipe per template (lattice spans for rectangles, an explicit point list for an irregular slot), `frozen.rs` is the
  committed data, `mod.rs` serves it. Regeneration is `cargo run -p pixlay-core --bin pixlay-gen-templates`, and only that: a `build.rs` would
  rewrite an interface silently, which is exactly what must not happen to frozen geometry.
- **`mosaic-8-s14` is unchanged.** Same name, same `version = 1`, same aspect, same slot order, same coordinates, same one concave slot; the
  test pins the area sequence (`9,6,9,3,6,6,19,6` sixty-fourths) so a reordering cannot slip through. S2 changed how the geometry is *produced*,
  not what it is. `AGENTS.md`'s verification command and `docs/CONTRACT.md` §1's example are therefore untouched.
- **No new dependency, no SVG parser, and no rendering added.** A path is the point list `Polygon` already is. `pixlay-render` was used only as S1's
  existing smoke path, to look at the geometry (below) — S2 itself introduces no image and no rendering code.
- **Two caller-facing subcommands, in S1's frozen machine surface**: `templates [--aspect W:H|decimal] [--json]` prints `name/slots/aspect/version`
  per template plus `count`, and `init --template <name> --out x.pixlay` writes a photo-free default project through `CollageDoc::to_json`.
  Both keep every S1 CLI rule, verified: `--json` on both, an empty stdout on usage errors, exit 1 for unknown template and for a non-`.pixlay`
  `--out`, byte-identical output under `LANG=C/zh_CN.UTF-8/de_DE.UTF-8`, and no timestamps. A flag belonging to another subcommand is a usage
  error rather than being ignored, because a silently dropped `--dpi` looks like it worked.
- **How S2 reads the "not doing": no image in, no rendering out.** `init` writes a document whose cells are all `source: null`, and the library
  gained no pixel code. `probe` refuses such a project (`occupied = 0` → verdict `failed`, exit 2), which is S1's vacuous-pass rule working as
  intended; the geometry's visual confirmation therefore goes through `render --template`, the path that exists for it.

### S2 · measured (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| frozen geometry | `src/templates/frozen.rs`, **7967 bytes**, sha256 `ba3ce76e…`; regenerating gives the same bytes twice and equals the committed file |
| libraries' exactness | 12 templates, 12/12 on the 1/32 lattice, declared area == outline area exactly, 11 cut templates summing to exactly 1.0, 0 overlaps, 0 holes |
| `mosaic-8-s14` 300dpi render (the `AGENTS.md` verification command) | 14043×10532, **ms 2345** compositing + **encode_ms 3802** JPEG, **`peak_rss_mb` 1610**, 42.5 MB — the S1 numbers are reproduced, so the regenerated geometry is pixel-for-pixel the geometry S1 froze |
| every template rendered at `--preview-px 480` | 12/12 exit 0, `occupied == cells`, correct pixel size for each aspect (480×320, 480×270, 480×360, 480×480) |
| `templates` (whole library) | 12 entries, `count = 12`, exit 0; `--aspect 4:3` → 3 entries; `--aspect 1:1` → 3; `--aspect 7:5` → 0 with exit 0 |
| `init` + reload | `mosaic-8-s14` → 4234-byte `.pixlay` that `Project::load` accepts, `doc.template == templates::get("mosaic-8-s14")`, all cells empty; a second run on the same path fails with exit 2 and leaves the file untouched |
| visual inspection | the 8-slot render shows 8 color regions with the orange region **L-shaped** and no white inside the image; the 2×2 gutter template shows four equal squares whose white gutter reaches the border, and its corner pixels are slot colors, so the slots do meet the canvas edge |

### S2 · deviations from and additions to the review additions

1. **The Work line said "irregular slots use SVG paths"; the review additions said polygons.** The two contradict each other, and the review
   additions win (they are the later, more specific ruling and they are what makes the criteria checkable). Irregular slots are explicit point
   lists in the recipe. No SVG parser exists in the dependency set.
2. **`Slot::area` is generated, not typed.** The recipe gives geometry and the generator computes the declared area from the outline it just
   built, so the declared value can never drift from the path; the test then re-derives it independently. The `AREA_TOLERANCE = 1e-6` cross-check
   still runs, but on this geometry it is never the thing that makes a template valid.
3. **One member is deliberately not a cut template** (`grid-4-2x2g`). S1's test file assumed every template tiles the canvas; left alone, the
   matrix would have shipped only cut layouts and the margin branch of that test would have stayed dead code. The gutter is real product behavior
   (a printed collage does not have to bleed to the edge), and it is the sharper case for "no hole": a flood fill proves the gutter is reachable
   from the border rather than sealed between slots.
4. **The band names are asymmetric on purpose.** A 4:3 canvas is landscape for 2–10 slots in this matrix except the 2-slot case, where the
   portrait counterpart (`strip-2-1x2`, 2:3) is a separate template rather than a stretched one — a portrait and a landscape layout are different
   compositions even when the slot count matches, and `CollageDoc::validate` refuses to stretch one onto the other.
5. **`count` is part of the `templates` report** (not only the `template.N.*` rows): a caller filtering by aspect needs to know "there are none"
   without counting rows, and it keeps `--json` self-describing.

## S3 · Framing and clamp — ✅ done (2026-09-21)

- **Goal**: the in-slot framing math is entirely correct, including rotation by any angle.
- **Work**: absolute zoom (displayed width / canvas width), offset, rotation; "parent container clips + child primitive transforms"; recompute the clamp after a rotation or a slot change. Fake image sizes are fine; no image pipeline is needed.
- **Exit criteria**:
  - sweep (rotation × zoom × offset × each slot shape), and after the clamp the photo **always covers the entire slot**
  - a change of rotation angle triggers a clamp recomputation, covered by a test
  - crop edges only, never grow the canvas: the canvas size is unchanged under any framing
- **Not doing**: no GUI gestures; no image decoding.
- **Human**: none. The clamp contract shape was already ruled on in "Open decisions → B. Confirmed", so this step runs to completion without a human answer.
- **Done (2026-09-21)**: `CropTransform::fit` in `pixlay-core/src/crop.rs`, applied by `pixlay-render`'s `draw`; criteria, decisions and numbers in "S3 result" below.

### S3 · review additions (2026-09-20)

- **The degradation policy was ruled on before the criteria were written.** The AGENTS "Open / to be proven" entry (elongated slots need 6.7–7.6× zoom) changes the clamp contract
  (the clamp result must be able to report "the rotation was limited"), and the contract was frozen in S1 → this had to be decided **before the S1 contract review**, not left to S3.
  It was: see "Open decisions → B. Confirmed", elongated-slot clamp degradation.
- the epsilon for "covers the entire slot" is given a number (normalized 1e-6 suggested, or ≤0.5px at 300dpi), written into the test constants
- hit testing (see S6.5) and clamp are both geometry and can be merged into this step

### S3 result (2026-09-21)

`[all criteria are in the tests in the repository; the numbers below are the release binary's output on this machine unless a test is named]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| sweep (rotation × zoom × offset × each slot shape), photo always covers after the clamp | `pixlay-core/tests/framing.rs` (`every_framing_covers_its_slot`, `the_floor_is_the_smallest_zoom_that_covers`) and `pixlay-render/tests/framing.rs` (`every_framing_covers_its_slot_without_spilling_or_growing_the_canvas`) | **28,800 fits** — the library's 12 templates, all **64 slots**, 6 rotations (±45°, ±18°, 7.5°, 0°) × 5 offsets (both corners of the allowed box, one-sided, centred) × 5 photo aspects (0.5–2.4) × 3 zooms (0.35/1/3). Coverage tolerance `COVERAGE_EPSILON = 1e-6` of a canvas edge (0.014 px on A0's long edge), and the clamp is *tight*: the worst sample over the whole sweep measures **1.0000000000000002 half extents — one ulp above the photo's edge**, so it magnifies nothing beyond what covering needs. Coverage is measured on the renderer's placement model — vertices, edge midpoints and an interior grid — not by reusing the clamp's own vertex test, so a wrong centre, aspect or rotation direction cannot pass. Both of the clamp's branches are reached (the sweep asserts that: `limited > 0`, `panned > 0`) |
| the same thing at the pixel boundary | `pixlay-render/tests/framing.rs` | **360 renders** (2 templates × 5 rotations × 4 offsets × 3 zooms × 3 photo aspects) at 454×340 and 454×454: every sampled pixel ≥3 px inside a slot shows that slot's color and every sampled pixel ≥3 px outside every slot is white (the gutter template is the one with margin to check). 2.5 s in the debug test profile |
| a change of rotation angle triggers a clamp recomputation | `pixlay-core/tests/framing.rs::a_rotation_change_recomputes_the_clamp`, `pixlay-render/tests/framing.rs::a_rotation_the_document_asks_for_is_clamped_into_coverage` | a 4:3 photo in a 4:3 slot, measured: zoom **1.0 upright, 1.2554 at 12°, 1.3957 at 20°**, and at 45° the limit's own zoom 1.5 (with the angle cut to 27.3°). The render of a document asking zoom 1 with **rotation 13°** — which needs 1.26× and would leave the corners white — covers every sample pixel, while the test asserts the request alone does *not* cover (otherwise it would prove nothing) |
| crop edges only, never grow the canvas | the same render sweep | **360/360 renders are exactly the canvas's pixel size**, and no pixel outside the slots ever shows a photo color (a rotated photo spilling into a neighbour would land in the neighbour's color class) |

**What the clamp is** (`pixlay-core/src/crop.rs`, `CropTransform::fit(slot, canvas_aspect, photo_aspect) -> CropFit`). The stored crop is a request; the fit is what covers. The canvas and the slot never grow, so there are exactly three levers: `zoom` is raised to the covering value with the photo centred (`max(1, photo_aspect * slot_height / slot_width)`, exact for the library's rectangles *and* its concave slot, because a rectangle contains a polygon iff it contains its vertices); `offset` is pulled back along the line to the slot centre; `rotation_deg` is kept while the zoom it needs stays within `CLAMP_ZOOM_LIMIT` times that upright floor, and otherwise reduced to the widest angle that fits, with `CropFit::rotation_limited` reporting it. `draw` applies the fit before placing the bitmap (`crop.rs`: "the stored transform is a request; what gets drawn is the fit").

### S3 · decisions this step made

1. **The degradation limit is relative to the *upright* floor, not an absolute zoom — and that is what makes "very elongated slots" work at all.** The AGENTS entry's own 6.7–7.6× figure is the *upright* covering zoom of a narrow slot (measured here: the ten-column strip needs **6.0×** for a 4:3 photo), so an absolute reading of "> 1.5×" would refuse rotation to every strip template while fixing nothing. Relative, a narrow slot is never degraded for being narrow: measured, that strip at 45° needs **5.19× — less than upright** (a rotated photo fits a sliver better), so the angle survives untouched; `an_elongated_slot_keeps_its_rotation` pins that. The angles the limit does cut, measured with a matching photo asking 45°: **45° kept at 1:1, 34.0° at 6:5, 27.3° at 4:3, 22.6° at 3:2, 18.0° at 16:9, 11.2° at 8:3**, mirrored for slots taller than wide. `CLAMP_ZOOM_LIMIT` is the one constant that widens the range, and the table lives in `docs/CONTRACT.md` §2 so a future review can argue with the numbers instead of the code.
2. **A pan is clamped; it is never paid for with magnification.** The other reading of "the photo must cover" — raise the zoom until the *requested* offset also covers — is legal by the contract and gives a doubled magnification whenever a user drags a photo near the edge: dragging would zoom. So the fit clamps the offset at the zoom the *shape* demands instead. This is also why the S1 golden image is **byte-identical after S3** (its two crops are zoom 1.4/1.2 with offsets that the clamp finds feasible), which is the strongest regression signal this step could ask for: the clamp did not move a single pixel of the framing that was already correct.
3. **The fit is applied inside `draw`, so no contract-legal document can render an uncovered slot.** Consequence that had to be paid for: S1's `a_failing_probe_still_prints_its_numbers` (probe fails because `zoom: 0.5` leaves white inside a slot) is no longer reachable through the CLI — the same document now *passes*, because the clamp raised the zoom. The test was re-pointed at exactly that (`a_crop_below_the_covering_zoom_is_clamped_instead_of_leaving_white`), and the probe's own falsifiability — which S1 required — moved to `crates/pixlay-cli/tests/probe.rs`, which hands the probe an image that is wrong by construction (an unpainted slot) and asserts the interior criterion fails while the background and seam criteria stay clean. The probe keeps its leading question (S1's `probe` reads the renderer's output, which is where a regression shows); what it can no longer be made to fail by is a *document*.
4. **Hit testing was not merged into this step.** The review additions said the two "can be merged"; they are both geometry, but S6.5 already owns hit testing with its own exit criterion (a sweep of centroids and points 1 px outside every boundary against the analytic answer), and merging would move an exit criterion out of the step that has to close it. Nothing here blocks it: `Polygon::contains` and `distance_to_boundary` are the same primitives.
5. **The fit is taken in the space `draw` places into, not in the document's space.** `draw` passes the *output canvas pixels'* aspect, because that is the space its own slot arithmetic uses; the document's millimetre aspect differs from it by the pixel rounding (≤0.1% on the test canvas, ≤3e-5 at A0@300dpi). Every quantity the fit produces is a ratio, so the two agree to that order anyway — taking the renderer's is what makes "covers" exact for the arithmetic that actually paints.
6. **The fit is idempotent, and cheap enough to run unconditionally.** `a_document_that_is_already_fitted_renders_identically` proves it at the pixel level (pre-fitting every crop in the document changes the render by **0 bytes**), and the idempotence holds bit-for-bit because the pan clamp walks the segment by scaling the *offset* — the value the caller stores — rather than by interpolating the photo centre, which would round differently on the second pass. Cost per slot fit, measured `--release` over 50,000 fits: **0.07 µs** for an identity request, **4.98 µs** worst case (rotation search plus a pan clamp), i.e. under 50 µs for a ten-slot document.
7. **The clamp is a *precondition* for S4's decoder, and its zoom is the display size.** Sizing a bitmap from the stored request instead of the fit makes the canvas magnify it, and the half-texel filtering at the bitmap's edge smears transparency into the slot — measured while writing the render tests, with a request 11.6× below the fitted zoom: a **white smear about 6 px wide** inside the slot, which no coverage criterion can be measured through. The render tests size their bitmaps from the fit, which is the shape S4 must copy.
8. **The lower-bound numbers this step produced** (they replace the estimates the AGENTS entry carried): the upright floor is `max(1, photo_aspect * slot_height / slot_width)` — a ten-column strip with a 4:3 photo needs 6.0×, with a 3:2 photo 8.4× at 1/10 width (the shipped strip is 2/16 wide, hence 6.0), and a 5:2 slot with a square photo 2.5×. Two boundaries are *returned untouched* rather than guessed at, because no framing can be computed there: a request holding a NaN or an infinity (only an in-memory document can, since `validate` refuses one on load) and a degenerate slot or aspect. The clamp reaches `MAX_ZOOM` only if a slot's own shape demands it (a 0.05-wide slot on a square canvas with a 100:1 photo), which is the one place the coverage promise cannot be kept; `the_zoom_cap_is_a_hard_ceiling` pins that it returns the cap and a *valid* transform rather than an out-of-range one.

### S3 · measured (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| the `AGENTS.md` verification render (`render --template mosaic-8-s14 --dpi 300 --stats`) | 14043×10532, **ms 1620** compositing + **encode_ms 2148** JPEG, **`peak_rss_mb` 1611**, **42,525,185 bytes** — the same byte count S2 measured, so a document whose crops are already covering is rendered bit-identically after S3 |
| the framing stress project (8 crops: zoom 0.5–3.0, offsets up to ±1, rotations ±45°, every one below or at its floor) | `probe --dpi 150`: `occupied = 8`, `passed = true`, **8/8 slots match** their own color, **12/12 seams clean**, `foreign = 0` on all of them, blend 0.995–0.998 px per seam px |
| visual inspection | the 1200 px preview of that stress project: eight color regions, the orange slot still the L-shaped one wrapping the grey slot's two edges, **no white anywhere inside the frame**, no slot's content crossing into a neighbour, and the four rotated slots show clean grain rather than the smeared edge a wrong clamp would leave |
| every template rendered at `--preview-px 480` | 12/12 exit 0, `occupied == cells`, correct pixel size per aspect — S2's numbers reproduced |
| fit cost | 0.07 µs per slot (identity request), 4.98 µs per slot (rotation + infeasible pan), release, 50,000 fits each |
| coverage tolerance | the review suggested 1e-6 normalized or ≤0.5 px at 300dpi; the tests use **1e-6** (0.014 px on A0's long edge) while the clamp's own arithmetic is exact to ~1e-15 — the budget is spent on nothing, and the measured worst sample is 2.2e-16 past the edge |

### S3 · deviations from and additions to the review additions

1. **The 1.5× threshold got an explicit reference** (the upright covering zoom) instead of the "1.5×" the ruling left open, because the absolute reading is self-defeating for the very case the entry was about — see decision 1. This is the one place where this step had to interpret a ruling rather than execute it, and it is written into `docs/CONTRACT.md` §2 and `CLAMP_ZOOM_LIMIT`'s doc comment.
2. **The pan clamp is part of the fit**, which the "three levers" work line implied but the review additions did not spell out; decision 2 carries the reasoning.
3. **`CropFit` kept exactly two fields** (`transform`, `rotation_limited`): a clamped pan is visible by comparing the request with the fit, so no third flag was added and the S1 contract shape is untouched.
4. **The probe's CLI failure test had to move**, which is a contract-level consequence rather than a test detail: see decision 3.

## S4 · Image pipeline — ✅ done (2026-09-21)

- **Goal**: by the time a bitmap enters rendering it is right in decoding, orientation, color and bit depth.
- **Work**: `pixlay-imaging` — glycin decoding (including HEIC/AVIF), automatic EXIF rotation, resampling (`sRGB → linear → process → sRGB`, Lanczos3), a 16-bit intermediate buffer, per-slot color grading + a global uniform filter. The structure for background work returning to the main thread through a channel is settled first.
- **Exit criteria**:
  - **grading identity**: with `factor=1, s=1, Δ=0` the output is pixel-identical to the input
  - HEIC decodes; EXIF Orientation=6 is automatically rotated upright
  - large-ratio downsampling (4000px → 400px) is visually free of aliasing and mush
  - the intermediate buffer is 16-bit, and quantization happens only at the end of the pipeline
- **Not doing**: no grading UI.
- **Human**: none, but this step ends its session: the decoding backend it picks is irreversible and determines S8's `depends` (splitting principle 5).

### S4 result (2026-09-21)

`[all criteria are in the tests in the repository; the numbers below are the release binary's output on this machine unless a test is named]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| grading identity: `factor=1, s=1, Δ=0` is pixel-identical to the input | `pixlay-imaging/tests/resample.rs` (`the_identity_grade_changes_nothing_at_all`) | byte-identical, and the comparison can fail (`factor=1.25` moves the mean by more than 5 levels) |
| HEIC decodes | `pixlay-imaging/tests/decode.rs` (`heic_decodes_at_its_own_depth`), fixtures `photos/photo.heic` | `image/heif`, 800×600, 16-bit samples from a 12-bit file, not flat |
| EXIF Orientation=6 is rotated upright | `pixlay-imaging/tests/decode.rs` (`exif_orientation_is_applied_to_the_pixels`) | the fixture's bright rectangle lands at (399, 150)-(799, 300) after the rotation — the position rotation predicts and not the stored one, with the stored position asserted *not* bright |
| large-ratio downsampling is free of aliasing and mush | `pixlay-imaging/tests/resample.rs` (`a_large_reduction_matches_imagemagick_lanczos`, `a_large_reduction_does_not_alias`) | 8× (1600→200) RMSE **1.41** against ImageMagick's Lanczos; on a 4096→512 zone plate the mean error against the exact area average is **0.0202** outside the passband against **0.3183** for one sample per output texel (16×), and 0.0182/0.0191 inside it |
| the intermediate buffer is 16-bit, quantization only at the end | `pixlay-imaging/tests/resample.rs` (`a_sixteen_bit_intermediate_is_not_an_eight_bit_one`, `the_two_depths_agree_sample_for_sample`) | the sRGB round trip is exact for all 256 code values; an 8-bit *linear* intermediate loses 16+ of them; a 16-bit and an 8-bit fixture of identical content agree sample for sample |

**The criteria the review additions added**, and where they landed:

| Item | Landing point | Measured |
|---|---|---|
| the decoding backend, measured down both paths, including HEIC and Orientation=6 | "S4 · decisions" 1 | the sandboxed loader (path A) decodes PNG/JPEG/HEIC/AVIF in 11–110 ms per 2400×1600 file, under an *empty* environment; the in-process path (B) has no HEIC/AVIF at all and hangs under a plain executor |
| the buffer ladder, with the size cap and the concurrency rule | `docs/CONTRACT.md` §4.1, `pixlay_imaging::layout` | one source at a time + `Σ` slot bitmaps + the output surface; strip-10 at A0 with ten 12 MP photos: **1182 MB** measured against 3.33 GB + 443 MB if the whole displayed photo were handed over |
| colour decisions in the contract | `docs/CONTRACT.md` §4.1/§6 | the source profile is honoured (RMSE 0.04 against ImageMagick's conversion, 15.4 against ignoring it); output is always sRGB; alpha is flattened onto white; no `lcms2` |
| `--content detail\|flat` and `content.rs` are deleted, and the probe sinks to `pixlay-imaging` | `crates/pixlay-imaging/src/probe.rs`, `crates/pixlay-cli/tests/probe.rs` → `crates/pixlay-imaging/tests/probe.rs` | the flag and the module are gone; the probe paints its own flat content and its falsifiability is pinned by a test that hands it an unpainted slot |

### S4 · decisions this step made

1. **The decoding backend is the sandboxed loader (`glycin` 4.0.0), measured, not assumed.** Both paths were run for real (2026-09-21, this machine):

   | Path | HEIC | AVIF | 2400×1600 decode | Environment |
   |---|---|---|---|---|
   | A: `glycin` 4.0.0 — on Linux the facade *is* the sandboxed loader process | yes (12-bit → 16-bit samples) | yes | PNG 46 ms, JPEG 11–33 ms, HEIC 44–110 ms, AVIF 28 ms | works under `env -i PATH=/usr/bin:/bin HOME=/nonexistent`; no session bus, no XDG runtime dir |
   | B: `glycin-builtin` 4.0.0 (in-process, `builtin-image-rs`) | **no** | **no** | not measurable — see below | needs a glib main context (below) |

   Three measurements decided it. (i) **B cannot decode the formats the criterion names**: `glycin-builtin` covers PNG/JPEG/WebP/TIFF/GIF/BMP/QOI/EXR/JP2 through `builtin-image-rs`, and HEIC/AVIF live only in the distribution's external `glycin-heif` loader. (ii) **B does not even complete under a plain async executor**: driven with `futures_lite::block_on`, every frame request hung until glycin's own 60-second limit fired; the same call completes when the future is driven on a glib `MainContext` (`spawn_local` + `MainLoop::run`, measured), because a glycin frame is delivered through the context. (iii) A is not fragile the way the S0 note feared: the distro's `glycin-thumbnailer` failing for every format (the "Open / to be proven" entry) is that *binary's* problem, not the crate path's. The cost of A is S8's `depends`: `glib2`, `libseccomp`, `bubblewrap`, `fontconfig`, and the loader packages (`glycin`, `glycin-heif`, …) — recorded in `AGENTS.md`.

2. **The decoder runs on one private thread that owns a `MainContext`,** and the pipeline is synchronous from the outside (`pixlay-imaging/src/driver.rs`). Rationale, from the measurement above: a frame request only completes while a main context is iterated, so *some* loop has to exist. A private context on a private thread keeps the GUI's GTK loop out of it entirely (it must not be iterated by, or block, the main thread), and `with_thread_default` makes glycin's own `MainContextSelector::Auto` pick *this* context instead of starting a hidden loop of its own. Jobs serialize: decoding is the memory-heaviest stage, and the ladder's `N × source + Σ bitmaps + output ≤ budget` is the caller's decision, not this thread's.

3. **The bitmap holds the part of the photo the slot can show, and `draw` was taught where it sits.** This is the buffer ladder, and it is load-bearing rather than tidy: a slot in the ten-column strip needs its photo at 6× its own width, so the full displayed photo is 333 MB per slot at A0. `Bitmap` therefore carries `origin` + `display_size` (S3's bitmaps are unaffected: the constructor that does not name them means "this is the whole photo"), and `CropTransform::display_region` inverts `draw`'s own placement to produce the rectangle. The proof that the crop is transparent to the renderer is a test: the same document rendered from the whole bitmap and from the region is byte-identical where the blit is unrotated, and within interpolation round-off where Cairo has to interpolate.

4. **Grading is three numbers in linear light, and the global filter is a named grade.** `factor` (exposure), `delta` (Δ, warmth), `saturation`, applied *in that order* per the frozen order's "per-slot grading → global filter": the cell's grade first, then the preset's. One mechanism, so the identity criterion is one code path, and a preset is data in `pixlay-core` while the arithmetic is in `pixlay-imaging`. The identity path returns without touching a sample, which is what makes "pixel-identical" exact in a pipeline that is otherwise also exact.

5. **Saturation operates on the luminance of the exposed pixel, and the warmth shift is multiplicative.** `r *= 1 + Δ`, `b *= 1 - Δ`, `c = luma + s*(c - luma)` with the Rec. 709 weights. Clamped after exposure and warmth (the visible image) and again after saturation (the control cannot push a channel past white); the alternative — clamping only at the end — lets an over-exposed pixel desaturate into a value it never had.

6. **The probe paints its own content, and the CLI's `--content` flag is gone with `content.rs`.** The two are the same decision seen from both ends. A probe of real photos cannot ask its questions (measured: 12/12 seams "unclean" and 802 "unpainted" samples on the verification project, all false), so flat content is the probe's own business now (`probe::probe_bitmaps`), sized from the same fit as a real bitmap. The user-facing placeholder generator is deleted, and `render --template` therefore renders a white sheet — documented in the contract §5 and used only as a geometry smoke.

7. **`--preview-px` sizes the bitmaps, not just the output surface.** S1's design renders at full size and lets Cairo shrink it; with placeholder content that was free, and with real photos it means decoding and Lanczos-resampling an A0 to show a 1200 px thumbnail (measured before the change: 83 s and 1365 MB for one preview in the debug profile). The bitmaps are therefore sized in the space `draw` writes into (the *output* pixels), which is also what keeps the preview honest: the shrinking is the pipeline's Lanczos, not Cairo's filter.

8. **The CLI gained `image`, and it is the decode stage's machine surface.** "HEIC decodes" and "orientation 6 is applied" have to be visible without rendering a project, and S5 needs the EXIF date to substitute `{date}`; `image` reports the detected MIME, the size *after* rotation, the depth, the EXIF block's size and `DateTimeOriginal`. It writes nothing and takes no `--out`/`--dpi`.

9. **A 16-bit buffer is not a 16-bit buffer unless the offset is right — the step's one real bug.** `Source::pixel` multiplied the sample index by 4 for both depths, so every 16-bit source was read a half-image away: the HEIC and the 16-bit PNG rendered a comb of neighbouring-row content (visible in the first `--preview-px 1600` render of the verification project). It was found by looking at the image, then pinned by a test that cannot be fooled the same way: `photo-16bit.png` and `ratio-4-3.png` are the same content at the two depths, so the decodes must agree sample for sample. The lesson is S0's, again: "looks right" is not a criterion, and neither is a test that only checks a spread.

### S4 · measured (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| the `AGENTS.md` verification render (`render --project crates/pixlay-cli/tests/fixtures/verify.pixlay --dpi 300 --stats`, eight photos: JPEG, PNG, 16-bit PNG, HEIC, EXIF-rotated, dated) | 14043×10532, **ms 6311** + **encode_ms 1844**, **`peak_rss_mb` 1633**, 9,056,692 bytes |
| where the 6.3 s goes | decode ≈150 ms (20–52 ms per photo), resample ≈5.8 s, `draw` 264 ms (measured on the photo-free smoke render, which is the same canvas) |
| the same project as a 1200 px preview | 805 ms, 35 MB peak, 224,649 bytes |
| `probe` at 300 dpi on the A0 project | 532 ms, 2231 MB peak, 8/8 slots on their palette colour, 12/12 seams clean, blend 0.999 px per seam px (threshold 2), worst residual 0.12 (threshold 3.0), `foreign = 0` |
| `probe` at 96 dpi on the same project | 50 ms, 237 MB |
| the strip ladder (`strip-10-10x1`, A0 14043×7899, ten 4000×3000 photos) | compositing 5189 ms + encode 1744 ms, **`peak_rss_mb` 1182**; the whole-displayed-photo alternative would be 3.33 GB of bitmaps + 443 MB of output |
| resample accuracy | 8× reduction RMSE 1.41 against ImageMagick Lanczos; zone plate outer band 0.0202 against the exact average (one-sample baseline 0.3183), inner band 0.0182 |
| source ICC | 0.04 RMSE against ImageMagick's Adobe RGB → sRGB conversion; 15.4 against ignoring the profile |
| the fixtures | 14 files, 1.9 MB total, all CC0 and regenerable (`fixtures/generate.py`); `verify.pixlay` is 325 lines of the canonical `init` output with its cells filled |
| visual inspection | `/var/tmp/verify-preview.png`: eight photos, the concave slot's content upright and continuous with its neighbours, no white inside any slot, the transparent PNG flattened onto white, the rotated fixture upright. The 16-bit comb of decision 9 was found this way |

### S4 · deviations from and additions to the review additions

1. **The review's suggested ladder keeps its shape but gained a crop.** "decode → 16-bit linear → downsample to in-slot display size → grade → filter → sRGB8 → Cairo" assumes the downsample target is the *slot*, and the measured version of that is the region each slot actually shows (decision 3): without it the strip template's ladder is not `O(output pixels)` at all.
2. **The review asked for "a zone-plate check of the aliasing energy"; this is a stronger form of the same idea.** The energy is measured against the *exact area average* — the ideal answer, computed from the analytic plate — rather than against a threshold on "energy", so the test says how far from correct the result is instead of how much high-frequency content it has. The one-sample-per-texel baseline is measured in the same run, which is what shows the metric discriminates.
3. **`pixlay-imaging` does not depend on `pixlay-render`,** so the probe takes a borrowed `Rgb8View` (width, height, `&[u8]`) rather than the renderer's `Rgb8Image`: the renderer owns Cairo, and a probe of an A0 render must not copy 350 MB to ask its question.
4. **The CLI's `probe` renders full size even when `--preview-px` is given** (the flag is refused there): the probe samples pixel coordinates, and a preview moves every one of them.
5. **The framing rotation is still Cairo's, and that is deliberate.** `AGENTS.md`'s
   "rotation interpolation happens in `pixlay-imaging`" is about handing Cairo a photo
   to shrink; S3's placement, where the rotation lives, keeps "what is drawn is the
   fit" in one place for every caller. The bitmap arrives at display size, so the
   rotation is a 1:1 kernel rather than a downsample, and the pixel test
   (`a_decoded_photo_fills_its_slot`) renders a 25° rotation at 2.2x and finds the
   photo's colour at every sample ≥3 px inside the slot. Recorded in `docs/CONTRACT.md` §4.1.
6. **`--preview-px` now changes what the pipeline computes**, not just the target surface (decision 7). This is a behavior change for existing callers: the same document at the same DPI now produces a preview whose bitmaps are preview-sized. Nothing in the contract fixes "the preview resamples with Cairo", and the new behavior is the one the criterion "preview and export are the same `draw`" is about.

### S4 · the one visual criterion

The step's criteria say "Human: none", but "Where humans must step in" carries one
item for S4 — the downsampling quality, "free of aliasing and mush" — and the
allocation rule applies: "the second one produces a real image, and you must look
at it directly".

What is on disk for that look, each in one command:

    cargo run --release -p pixlay-cli -- render \
      --project crates/pixlay-cli/tests/fixtures/verify.pixlay --dpi 300 --stats --out /var/tmp/a.jpg
    cargo run --release -p pixlay-cli -- render \
      --project crates/pixlay-cli/tests/fixtures/verify.pixlay --dpi 300 --preview-px 1600 \
      --out /var/tmp/verify-preview.png
    # the downsampling case specifically: the wave fixture in the eight-slot layout
    python3 - <<'EOF'
    import json, pathlib
    fix = pathlib.Path("crates/pixlay-cli/tests/fixtures").resolve()
    doc = json.loads((fix / "verify.pixlay").read_text())
    for cell in doc["cells"]:
        cell["source"] = str(fix / "photos/resample-source.png")
    pathlib.Path("/var/tmp/downsample.pixlay").write_text(json.dumps(doc, indent=2))
    EOF
    cargo run --release -p pixlay-cli -- render --project /var/tmp/downsample.pixlay \
      --dpi 300 --preview-px 900 --out /var/tmp/s4-downsample-check.png

`/var/tmp/s4-downsample-check.png` is the one that answers the question: the wave
fixture at 4–8x reductions, and the concave slot at more than that. Looked at in
this session (2026-09-21): smooth, no moiré, no mush, the hard square's edges soft
but not ringing. The same content at 8x against ImageMagick's Lanczos is an RMSE of
1.41, and against the exact area average it is 0.0202 outside the passband where
one-sample-per-texel gives 0.3183 — so the eye and the numbers are answering the
same question.

**Status: looked at, numbers on disk, and ruled on 2026-09-21 (human) — the ruling is in
"S4 · ruling" below.** It does not gate S5: nothing in the text layer's contract depends on
the resampler's edge behaviour.

### S4 · ruling (2026-09-21, human)

**The downsampling passes.** Both previews were looked at — `/var/tmp/verify-preview.png`
(eight real photos) and `/var/tmp/s4-downsample-check.png` (the wave fixture at 4-8x
reductions) — and the numbers read: RMSE 1.41 against ImageMagick's Lanczos, and a 16x
separation from one-sample-per-texel on the zone plate (0.0202 against 0.3183). The verdict is
"pass, close the gate": no defect was named, so the resampler stays exactly as S4 built it —
16-bit linear Lanczos3 with the kernel widened by the shrink ratio, over the region the slot
shows, quantized only at the end.

The optional hardening (a *new* permanent threshold) was **declined**, and on inspection it was
mostly already there: S4's own tests pin both numbers — `a_large_reduction_matches_imagemagick_lanczos`
asserts RMSE ≤ 3.0 (measured 1.41) and `a_large_reduction_does_not_alias` asserts the outer-band
error ≤ 0.03 (measured 0.0202) *and* that the one-sample-per-texel baseline is at least 3x ours.
So the verdict leaves nothing unpinned: the criterion is a look at the finished product, the look
passed, and the numbers it was judged against are already assertions rather than measurements.

No shape changed, so `docs/CONTRACT.md` needs no edit from this ruling — the numbers it already
carries (§8 "S4") are the basis it was judged on. The gate row and the "Current progress" line
were rewritten in the same commit, which is this ruling's other half.

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

## S5 · Text layers — ✅ done (2026-09-21)

- **Goal**: canvas-level text is correct, and both forms go through the same mechanism.
- **Work**: a text layer is the basic unit, and a tiled watermark is one of its modes; dynamic EXIF fields such as `{date}`; Pango + pangocairo layout.
- **Exit criteria**:
  - free placement and a tiled watermark both produce an image
  - `{date}` is filled correctly from EXIF; **the fallback behavior when EXIF is missing has a test**
  - CJK kinsoku and punctuation squeezing are visually correct
  - the text position is stable after a rotation (verifying the "content layers run last" constraint)
- **Not doing**: no text editing UI.
- **Human**: the CJK kinsoku and punctuation-squeezing feel, judged by eye (see "Where humans must step in").
- **Done (2026-09-21)**: `pixlay-render/src/text.rs` (one Pango layout per layer, free and
  tiled), token resolution in `pixlay-core`, `Sampler::exif` + `SlotBitmap::date` in
  `pixlay-imaging`, `text` and the probe's refusal in the CLI, and the pinned test font;
  criteria, decisions and numbers in "S5 result" below. The preview for the human gate is
  `/var/tmp/pixlay-s5/text-preview.png`.

### S5 result (2026-09-21)

`[all criteria are in the tests in the repository; the numbers below are the release binary's output on this machine unless a test is named]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| free placement and a tiled watermark both produce an image | `pixlay-render/tests/text/measure.rs` (`measure_free_placement_puts_the_box_on_the_anchor`, `measure_a_tiled_watermark_covers_the_grid_from_the_canvas_origin`), and end to end `pixlay-cli/tests/cli.rs::rendered_text_reaches_the_output` | the free layer's ink rectangle equals the rectangle the layout's **own metrics** predict (anchor × box × ink offset, within 2 px) for four anchors; the tiled layer paints a solid em square at each of the anchors the canvas can see — 15 anchors for `step (0.5, 0.25)`, counted by `tiled_grid`, with the far-edge row and column off-canvas exactly as the contract's "Tiled phase" says. The free layer's ink box is compared against the layout's own metrics, so an off-by-one in the anchor fractions cannot pass. The CLI renders the committed fixture (`text.pixlay`, a caption + a `{date}` + a 15-tile watermark) and the same project with `text: []`: the layers change more than a thousand pixels (the test's floor; the assertion would fail if the layers reached only the report), so the criterion is not a report-only claim |
| `{date}` is filled correctly from EXIF, and the fallback when EXIF is missing has a test | `pixlay-core/tests/text.rs` (resolution), `pixlay-render/tests/text/measure.rs::measure_tokens_render_the_values_the_slot_reports`, `pixlay-cli/tests/cli.rs` (`text_reports_resolved_tokens_from_the_projects_photos`, `text_falls_back_to_the_documents_own_date`) | `pixlay-render text` on the fixture reports `2019:07:14 10:32:00` — the EXIF value of slot 5's `dated.jpg`, not the document's stored `2026-09-21`. A photo with no EXIF block, an empty slot and a layer naming no slot all resolve to the stored string, and with no stored string the token renders as **nothing**, never as `{date}`. Each render case is compared against the same text typed out literally: the two renders are byte-identical, so the assertion is about the *string* and not about ink having appeared |
| CJK kinsoku and punctuation squeezing are visually correct | `pixlay-render/tests/text/measure.rs` (`measure_kinsoku_keeps_punctuation_off_a_line_start`, `measure_consecutive_punctuation_is_compressed_to_half_width`), plus the human gate below | kinsoku is Pango's: `他他他说。他` at a four-em width breaks `他他他 / 说。他` (the breaker pulls back rather than starting a line with `。`), and over 4 paragraphs × 6 widths no line starts with a closing mark and none ends with an opening one. Squeezing is the renderer's, through the font's `halt`: `。，` = 0.5 + 1.0 em, `。。。` = 0.5 + 0.5 + 1.0, a lone `。` = 1.0, and a mark at a line boundary keeps 1.0 (measured with the pinned font, 20 px) |
| the text position is stable after a rotation | `pixlay-render/tests/text/measure.rs` (`measure_a_slot_rotation_does_not_move_the_text`, `measure_a_text_layer_rotates_about_its_anchor`) | rotating a **slot** by 35° changes the slot and moves the text by **0 px** (the ink's bounding box is identical); rotating a **layer** by 45/90/180/−30° moves its ink centre to the rotated position within **2 px**, so the rotation is about the anchor and not about the canvas origin |
| (from the review additions) the font size is normalized | `measure_preview_and_export_agree_with_text`, `docs/CONTRACT.md` §1 "Text layers" | the same document at 2N and N, the 2N one downsampled: RMSE **1.92** (threshold 6), and the ink rectangle at 2N is the rectangle at N **doubled to within 1 px** — the layout is computed in canvas pixels, so only the glyph raster changes with the scale |
| (from the review additions) "no line starts with a forbidden character" is checkable through `pango_layout_get_line*` | the same kinsoku test | it is: each line's text comes from `LayoutLine::start_index` / `length`, and the two sets (`行頭禁則` / `行末禁則`) are the test's own constants |

**What was built, in one line each.**

- `pixlay-render/src/text.rs`: one Pango layout per layer, drawn by `draw` after the
  slots. Free mode wraps within the canvas width and is placed by `anchor`; tiled mode
  puts an unwrapped mark's box's top-left corner on each grid anchor and rotates it about
  that point. Font options (`hint none`, `metrics off`, `antialias gray`) are set before
  any layout exists, which is what keeps the layout scale-independent.
- `pixlay-core/src/text.rs`: `TextLayer::resolve(&values, &fallback)` (the token rule),
  `TextValues`, `TextLayer::MAX_TILES` and `tiled_grid(step)`.
- `pixlay-imaging`: `Sampler::exif()` and `SlotBitmap::date`, so `{date}` rides out with
  the pixels instead of costing a second decode.
- `pixlay-cli`: `text` (the resolved layers, tokens substituted), `render`'s report gained
  `text = n`, and `probe` refuses a document with text layers.
- Fixtures: `text.pixlay` (the S5 fixture) and a `{date}` layer added to `verify.pixlay`,
  so `AGENTS.md`'s verification render exercises text from now on; the pinned test font
  under `fixtures/fonts/`.

### S5 · decisions this step made

1. **Punctuation squeezing is not something Pango does, and the font has to be asked.**
   Measured 2026-09-21: `。，` advances two full ems — exactly like two isolated marks — and
   no layout option changes that. The rule the criteria ask for is also about a *run*
   rather than a character (a lone `。` must keep its blank, or every sentence end in the
   product crowds the word after it), so the renderer marks each mark that another mark
   follows and asks the font for the OpenType `halt` (half-width) positioning feature.
   Consequence, accepted: what a compressed mark *looks* like is the font's design, and a
   font without `halt` simply does not compress. The alternative — moving glyphs ourselves
   — is a layout engine, and the workspace's `unsafe_code = deny` rules out the glyph-level
   FFI it would need.
2. **Pango's line breaking is kept, and `。` at a line *end* is left alone.** Pango
   implements the Unicode line-breaking rules, so kinsoku's 行頭禁則/行末禁則 come for free
   and the tests pin them instead of reimplementing them. What is *not* done is JLREQ's
   other half of squeezing — trimming the trailing blank of a mark that ends a line — which
   needs a per-line layout pass; v1 draws a mark's own advance there. Written into §6 as a
   non-goal rather than left to be discovered.
3. **The free layer's box is the canvas width.** A layer has no size of its own in the
   contract (§6: no text-box field), so wrapping needs *a* width, and the canvas is the one
   width the document already has: long text wraps at the canvas edge instead of running
   off it, and the wrapped box is what `anchor` places. A tiled tile is *not* wrapped — a
   watermark is one mark per tile — which is the one place the two modes differ.
4. **The tile grid keeps its far-edge anchors, and got a cap.** `floor(1/step) + 1` per
   axis: the anchor at the far edge draws nothing unrotated, but a rotated watermark swings
   ink back over the sheet from there, so dropping it would leave a bare stripe. The cap
   (`MAX_TILES = 10,000`, checked when the document loads) exists because a step has no
   upper bound and therefore no lower one either: a 1/100 step is 10,201 tiles, and the
   renderer asks the same `tiled_grid` so an in-memory document cannot hang it either.
5. **`{index}` is the slot index, 0-based.** The contract's own words are "the slot index",
   and every other slot number this product prints (`template.<i>`, `probe`'s rows) is
   0-based; a second numbering would be a bug waiting to be read the wrong way.
6. **A token with no value renders as nothing.** The alternative — leaving `{date}` on the
   finished image because a phone stripped the EXIF block — puts a debugging token in the
   user's product, and the failure is silent until someone exports.
7. **The font is the system's, the tests' font is committed.** Production asks fontconfig
   for `sans-serif` and never names a font (a collage has to open on another machine);
   `tests/fixtures/fonts/pixlay-test-sans.otf` — a 93 KB subset of Noto Sans CJK SC with
   the `halt` feature kept — is what the *tests* measure with, pinned through
   `FONTCONFIG_FILE`. The pin is a process property, so the text measurements run in a
   child process (see 8) rather than through a `set_var` this workspace's lints refuse.
8. **The text measurements are `#[ignore]`d and run by one harness test.** A plain
   `cargo test` runs `the_text_measurements_run_with_the_committed_test_font`, which starts
   the same test binary again with the pinned fontconfig and asserts the child passed *and*
   executed at least the expected number of measurements: a filter that matches nothing (a
   rename, a typo) fails instead of measuring nothing. `fonts.rs` carries the reasoning.
9. **The probe refuses a document with text layers.** Its interior samples assert "this
   slot is its palette colour" and its background samples assert white; a watermark over a
   slot is indistinguishable from a wrong photo, and a probe that answered anyway would be
   lying about the render. Exit 2, empty stdout, and the reason on stderr — the same shape
   as a decode failure, because there is no number to report.
10. **`render` reports `text = n`, and a new `text` subcommand reports the resolved
    strings.** "{date} is filled from EXIF" has no machine-visible surface otherwise: it
    would have to be checked by rendering and reading pixels back. `text` decodes only the
    slots a layer names (once each) and writes nothing, so it stays a query and not a
    second renderer.

### S5 · measured (2026-09-21, `--release`, this machine)

| Item | Value |
|---|---|
| the `AGENTS.md` verification render (`render --project crates/pixlay-cli/tests/fixtures/verify.pixlay --dpi 300 --stats`, eight photos + one `{date}` layer) | 14043x10532, **ms 6164/6359** (two runs) + **encode_ms 2469/2475**, **`peak_rss_mb` 1641**, **9,114,833 bytes**; the same project with `text: []`: ms 6360/5660, peak 1631, 9,056,692 bytes — the layer's cost is **below the run-to-run spread of the decode+resample stage**, so nothing smaller than that is claimed at 139.5 MP |
| text cost at 16.7 MP (400x300 mm at 300 dpi, empty cells) | white sheet **24-42 ms** (3 runs); + **2,601 tiles** 295-436 ms → **≈ 0.13 ms per tile** (the 10,000-tile cap is ~1.3 s of drawing); + 20 wrapped CJK captions 31-57 ms, i.e. below the spread |
| the text fixture as a 2400 px preview (`text.pixlay`, dpi 150, **pinned font**) | ms 459 + encode 45, peak 77 MB, 3,320,360 bytes; with `text: []` ms 355, 3,276,176 bytes — the three layers cost about **100 ms** at 2400 px. The machine's own `sans-serif` gives 443 ms / 3,380,931 bytes: the difference is glyph rasterization, and the line breaks are the same |
| squeezing (pinned font, 20 px) | `。` 20.0; `。，` 10.0 + 20.0; `。。` 10.0 + 20.0; `。”` 10.0 + 20.0; `。。。` 10.0 + 10.0 + 20.0; `（（` 10.0 + 20.0; `。\n。` 20.0 + 20.0 |
| kinsoku | `他他他说。他` at 4 em: `他他他 / 说。他`; 4 paragraphs × 6 widths (3-9 em): no forbidden line start, no forbidden line end, every case wrapped |
| layout vs scale | the ink rectangle at 2N is the one at N doubled to within 1 px; 2N-vs-N RMSE 1.92 (no text: 1.53) |
| the pinned font | `pixlay-test-sans.otf` 93,100 bytes, 691 glyphs, 204 codepoints, GPOS `halt` present; fontconfig with only this font resolves `sans-serif` to it |
| the fixtures | `text.pixlay` (3 layers: a 45-character wrapped caption, a `{date}` line, a 15-tile watermark) and `verify.pixlay` + one `{date}` layer; regenerating the font is `python3 crates/pixlay-cli/tests/fixtures/fonts/generate.py` |
| visual inspection | `/var/tmp/pixlay-s5/text-preview.png`: the caption wraps at the canvas width with no line starting on a mark, `：“` and `。”` are visibly tighter than a full em, the `{date}` line carries the EXIF date, and the tiled watermark sits on a 5x3 grid with its far-edge tiles off the sheet |

### S5 · the one visual criterion

**The human looks at two things, and only at these two:**

1. **Kinsoku** — in `/var/tmp/pixlay-s5/text-preview.png`, the caption is 45 characters and
   wraps inside a sentence: line 1 has to end *short* (the breaker pulls the break back so
   that the `，` after `中文文字` does not start line 2). If a line ever starts with `，` or
   `。`, that is the defect.
2. **Punctuation squeezing** — `他说：“今天天气很好。”` has `：“` and `。”` in it, and both
   runs are drawn at 3/4 of what they would cost uncompressed. Asked simply: do those two
   pairs read as tight typography, or as a collision?

**Status: rendered, numbers on disk (`docs/CONTRACT.md` §8 "S5"), and ruled on 2026-09-21
(human) — the ruling is in "S5 · ruling" below.** It does not gate S6: nothing in the export
step depends on how a squeezed mark looks.

### S5 · ruling (2026-09-21, human)

**The text passes.** `/var/tmp/pixlay-s5/text-preview.png` was looked at, and both halves of
the criterion hold: the 45-character caption wraps inside its sentence with line 2 starting
`字，` — no line starts with a mark and none ends with an opening mark — and `：“` and `。”`
read as tight typography rather than as a collision, with the run's last mark keeping its full
em (which is why `”` carries its own advance before `然后`). Kinsoku stays Pango's and
squeezing stays the font's `halt`, exactly as S5 built them.

Two alternatives were considered and **declined**: turning squeezing off (it would make the
look independent of whether a font has `halt`, at the cost of a loose 2-em `。”` and of the
compression tests), and adding JLREQ's 行末の約物 (trimming the trailing blank of a mark that
ends a line). The latter therefore stays a v1 non-goal, recorded in `docs/CONTRACT.md` §6.

No shape changed, so `docs/CONTRACT.md` needs no edit from this ruling beyond what §8 "S5"
already records. The gate row and the "Current progress" line were rewritten in the same
commit, which is this ruling's other half.

Regenerate the preview with:

    cargo run --release -p pixlay-cli --bin pixlay-render -- render \
      --project crates/pixlay-cli/tests/fixtures/text.pixlay --dpi 150 --preview-px 2400 \
      --stats --out /var/tmp/pixlay-s5/text-preview.png

This renders with the machine's own `sans-serif`, which is what production does; measured
2026-09-21, the committed subset the *tests* pin breaks the same caption at the same
character (the two renders differ by 2.9% of pixels, all of it glyph rasterization), so the
criterion below is about the same layout either way. The preview above is the system-font
one — the product's look.

### S5 · deviations from and additions to the review additions

1. **"Punctuation squeezing" turned out not to exist in Pango**, so the criterion would
   have been unimplementable as "assert Pango squeezes". It is implemented through the
   font's `halt` feature instead (decision 1), and the test asserts the *advances* the
   criterion describes ("the spacing between adjacent punctuation is smaller than the
   default spacing") rather than a Pango behaviour.
2. **The kinsoku test is Pango's, the squeezing test is ours** — the review addition
   treated them as one checkable pair; they are two mechanisms, and the tests say which is
   which, because a future Pango that starts squeezing punctuation by itself must not
   silently double-compress (the test would fail, which is the point).
3. **`text` was added to the CLI.** The review additions did not ask for a subcommand, but
   the criteria did ("`{date}` is filled correctly from EXIF") and a pixel-level assertion
   cannot name the *string* a token produced. `render`'s report also gained `text = n`.
4. **The probe now refuses text documents**, which is new behaviour for «probe» and is in
   the contract's §5. It follows from the probe's design (it probes content it painted
   itself), not from text being unrenderable.
5. **The `fonts` row in "Open decisions → C" is now executed**: production bundles no font,
   the tests pin a committed subset, and no test needs a display or a system font.

### S5 · review additions (2026-09-20)

- "CJK kinsoku is correct" is checkable: build text whose line start has forbidden punctuation, and after Pango breaks the lines use `pango_layout_get_line*`
  to assert that no line starts with a forbidden character; punctuation squeezing asserts that the spacing between adjacent punctuation is smaller than the default spacing
- the font size must use **normalized canvas-relative units**, otherwise the preview/export RMSE criterion fails immediately

## S6 · Export — ✅ done (2026-09-21)

- **Goal**: both export modes are correct, and metadata is done in one pass.
- **Work**: physical size + DPI mode; specified long-edge-in-pixels mode; encoding and metadata inside the same pipeline.
- **Exit criteria**:
  - physical-size mode: the file carries the correct DPI and ICC
  - long-edge-pixel mode: the output long-edge pixel count matches the request exactly
  - chroma subsampling matches the request (specifically catching the two-pass metadata trap)
  - A0 can produce PNG / JPEG / TIFF
- **Not doing**: no export UI; no PDF (not in scope).
- **Human**: none. The one visual question this step inherits is S4's downsampling quality.
- **Done (2026-09-21)**: `pixlay-imaging::encode` (PNG/JPEG/TIFF, one pass) plus
  `pixlay-imaging::icc` (the sRGB profile built from the IEC 61966-2.1 colorimetry),
  `CanvasSpec::pixel_size_for_long_edge` / `dpi_for`, the `render_*_sized` shells, and the
  CLI's `--long-edge` / `--chroma` / `.tif`; criteria, decisions and numbers in "S6 result".

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

### S6 result (2026-09-21)

`[all criteria are in the tests in the repository; the numbers below are the release binary's output on this machine unless a test is named]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| physical-size mode: the file carries the correct DPI and ICC | `pixlay-imaging/tests/encode.rs` (`the_png_carries_its_resolution_and_profile`, `the_jpeg_carries_its_resolution_profile_and_subsampling`, `the_tiff_carries_its_tags_and_round_trips`), `pixlay-cli/tests/cli.rs::every_export_format_is_written_with_its_metadata` | **300 dpi and the profile in all three formats**, read back by tools that are not ours: `identify -verbose` reports `300x300 PixelsPerInch` + `Profile-icc: 664 bytes` + `icc:description: sRGB IEC61966-2.1` for the JPEG and TIFF, `118.11x118.11 PixelsPerCentimeter` (= 11811 px/m = 300.00 dpi, the `pHYs` unit) for the PNG, and `tiffinfo` reports `Resolution: 300, 300 pixels/inch` + `ICC Profile: <present>, 664 bytes`. The PNG test also pins that the `sRGB` chunk is **not** written next to `iCCP` |
| long-edge mode: the long-edge pixel count matches the request exactly | `pixlay-core/tests/contract.rs::a_long_edge_is_exact_and_the_other_edge_keeps_the_ratio`, `pixlay-cli/tests/cli.rs::long_edge_is_exact_and_carries_the_resolution_it_works_out_to` | `--long-edge 9000` on the A0 project renders **9000x6750** (the file and the report agree), and a portrait canvas puts the exact edge on the other axis (`1234` high, `823` wide for a 2:3 canvas). The core test sweeps square / landscape / portrait / A-series and the rounding rule (`round(n * short / long)`, half away from zero), and pins that the flag's range and the canvas budget are two limits that both apply |
| chroma subsampling matches the request (the two-pass trap) | `pixlay-imaging/tests/encode.rs` (all three values), `pixlay-cli/tests/cli.rs::chroma_reaches_the_jpeg_it_was_asked_for` | the assertion is on the file's own `SOF0` sampling factors: `1x1,1x1,1x1` (444, the default), `2x1,1x1,1x1` (422), `2x2,1x1,1x1` (420), and `identify` reads the same back (`jpeg:sampling-factor: 2x2,1x1,1x1`). This is the check the trap fails: re-encoding to patch metadata rewrites the factors whatever the request was. `--chroma` with a PNG/TIFF `--out` is a usage error (exit 1, nothing written) |
| A0 can produce PNG / JPEG / TIFF | "S6 · measured" below | **all three** at 14043x10532: JPEG 9,216,300 bytes / `encode_ms` 1799, PNG 33,955,066 / 5709, TIFF 42,748,009 / 2495, each `peak_rss_mb` **1643/1642/1642** against the 2.5 GB budget, and each decoded back by ImageMagick at the right size, resolution and profile |

**The criteria the review additions added**, and where they landed:

| Item | Landing point | Measured |
|---|---|---|
| "cairo supplies pixels only, the encoder writes the metadata itself" | `pixlay-imaging/src/encode.rs` (module docs: the per-format field list and the rounding rules) | no format is written twice: PNG's `pHYs`/`iCCP` go into the header before the `IDAT` stream, JPEG's density/sampling/`APP2` are set on the encoder before `encode`, TIFF's tags are written before the strips |
| the per-format field list is in the criteria | `docs/CONTRACT.md` §5 ("per-format metadata") | PNG `pHYs` + `iCCP`; JPEG JFIF density + `APP2` + `SOF0`; TIFF `XResolution`/`YResolution`/unit + tag 34675. The PNG `sRGB` chunk is deliberately absent (the specification says it should not accompany `iCCP`) |
| the rounding rule and "what DPI is written" in pixel mode | `docs/CONTRACT.md` §5 + `encode.rs`'s rounding constants | `dpi = long_edge_px * 25.4 / long_edge_mm` (one number for both axes, the long edge's); PNG stores `round(dpi * 1000 / 25.4)` px/m, JPEG `round(dpi)` px/inch in 16 bits (past 65535 it is refused, not saturated), TIFF `round(dpi * 100) / 100` as a rational |
| the time cap (≤ 3x the baseline) | "S6 · measured" | JPEG 1799 ms against S4/S5's 2469 ms (0.73x, the same project and content); PNG 5709 ms against S0's 27773 ms (0.21x, different content, so a ceiling check); TIFF 2495 ms against S0's 4800 ms |
| artifacts land on disk, never on tmpfs | both test files' `out_dir` (`$XDG_CACHE_HOME` or `/var/tmp`), this session's output in `/var/tmp/pixlay-s6/` | the A0 PNG is 34 MB and the TIFF 43 MB; tmpfs would have held another copy |
| banding is the export path's memory lever | not used — see "S6 · decisions" 5 | the measured export peak is 1643 MB of a 2.5 GB budget, and the encoder APIs take a whole buffer per frame, so banding would save the surface without changing what the encoders need |

### S6 · decisions this step made

1. **The encoder lives in `pixlay-imaging`, and it is three writers used directly.**
   `image` could not do the job: its PNG writer exposes neither the pixel dimensions nor the ICC
   profile, and its JPEG writer neither the sampling factor nor the `APP2` profile (it is
   zune-jpeg). So S6 depends on `png`, `jpeg-encoder` and `tiff` directly — all three were
   already in the tree as dependencies of `image`/`glycin-image-rs`, so the download set did not
   grow, and `image` moved to `pixlay-cli`'s dev-dependencies, where it is what the tests read
   renders back with. The encoder sits beside the decoder because it is the same boundary (pixels
   in, pixels out, no cairo and no gtk) and because the GUI will export through it in S7.
2. **The ICC profile is generated, not shipped.** `docs/STEPS.md`'s own decision was "embed the
   sRGB IEC61966-2.1 profile bytes; do not pull in lcms2", and there is no trustworthy copy of
   those bytes in the tree. `pixlay_imaging::icc` builds an ICC v4 display profile from the
   published colorimetry instead: the primaries and the D65 white point as chromaticities, the
   piecewise transfer function as a `para` type-3 curve, and a Bradford D65→D50 adaptation — the
   same shape and the same numbers lcms2 writes. It is ~200 lines of matrix arithmetic with no
   colour code of ours in any *conversion* (the conversion is still the loader's), and it is
   checkable against an implementation that is not ours: the fixture's embedded lcms2 profile.
   Measured: colorants within **2.2e-4**, all five curve parameters within **1.5e-5** (one unit in
   the last place of 15.16 fixed point), and ImageMagick converting an export *from* this profile
   *to* colord's sRGB moves the pixels by **0.0015/255** — lcms2 reads the two as the same space.
   The alternative (committing a third-party `.icc` blob) would have meant shipping bytes whose
   provenance and licence had to be argued about in S8, for no gain.
3. **A pixel count and a resolution are two requests, and the file echoes whichever was made.**
   `--dpi 300` writes 300 dpi even though the rounded grid is 299.96 dpi of actual pixels (A4), and
   `--long-edge n` derives the resolution from the grid it rendered. That asymmetry is deliberate:
   a printer queue is built around the number the user typed, and a pixel request has no number to
   echo, so writing the achieved resolution is the only honest answer. The two flags are mutually
   exclusive, and neither is silently converted into the other. `draw` itself no longer sees a DPI
   at all — `CanvasSpec::pixel_size_for_long_edge` and `pixel_size` both produce a pixel grid, and
   `render_rgb8_sized` takes it (`render_rgb8` is the DPI convenience wrapper).
4. **The long-edge range and the canvas budget are two different limits.** `MAX_LONG_EDGE_PX`
   is 30000, because A0 at the maximum DPI is 28087 px and anything past that is a typo rather than
   a print; the 200 MP budget is checked on the grid the flag derives, so `--long-edge 20000` on a
   square canvas is refused with the pixel count in the message. The flag's own range is a usage
   error (exit 1, the S1 rule for out-of-range values); the budget is a document error (exit 2).
5. **Export renders whole; banding is not used here.** The review pointed at `Band::out_rows` as
   the memory lever, and it is — for *rendering*. For *encoding* the APIs decide the shape: `png`
   wants the whole frame's rows in one call, `jpeg-encoder` wants the whole buffer (`encode(&[u8])`),
   `tiff` could stream strips but the RGB buffer would still exist. So a banded export would save
   the ARgb32 surface while the encoders still need the whole RGB image, and the measured peak
   (1643 MB against the 2.5 GB budget) says the lever is not needed. Recorded rather than silently
   skipped: if a future step needs the last 550 MB, a banded path that renders into the RGB buffer
   directly is the place to look.
6. **JPEG quality stays 90, and 4:4:4 stays the default.** Both are S0/S4's numbers, so every
   measurement in this file stays comparable, and `AGENTS.md` fixes 4:4:4 as the default; `--chroma`
   is the request surface for the two subsampled modes, which the encoder writes into the frame
   header.
7. **TIFF is LZW with the horizontal predictor.** The S0 baseline measured `magick -compress LZW`,
   every reader understands it, and the `tiff` crate's `deflate`/`fax`/`jpeg` features are switched
   off in our manifest — a TIFF decoder we never call is not a dependency worth shipping. The ICC
   tag is written as its spec'd type (`UNDEFINED`, tag 34675): `write_tag` can only express `BYTE`
   for a byte slice, so the payload is written first and the entry built from its offset.
8. **`--stats`'s `icc` field became real.** It was the string `none` from S1 (a placeholder so the
   field's shape would not change); it is now the description of the profile the written file
   carries. It is checkable from both ends: the encoder test reads the profile's own `desc` tag back
   out of the file, and the CLI test asserts the report names the same description the encoder
   writes. A command that writes no file (`probe`) still reports `none`, which is now a statement
   about that command rather than a stub.

### S6 · measured (2026-09-21, `--release`, this machine)

The `AGENTS.md` verification render, eight photos and one `{date}` layer at 300 dpi on A0
(14043x10532), one row per format:

| Item | Value |
|---|---|
| JPEG q90 4:4:4 | **9,216,300 bytes**, `encode_ms` **1799**, whole run `ms` 6559, **`peak_rss_mb` 1643** |
| JPEG q90 4:2:0 (`--chroma 420`) | **6,110,454 bytes** (−34%), `encode_ms` **1031**, `ms` 6393, peak 1643 |
| PNG | **33,955,066 bytes**, `encode_ms` **5709**, `ms` 6316, peak 1642 |
| TIFF (LZW + predictor) | **42,748,009 bytes**, `encode_ms` **2495**, `ms` 6250, peak 1642 |
| the S0–S5 JPEG baseline, same content | 9,114,833 bytes / `encode_ms` 2469: the new writer is **1.1% larger and 27% faster** |
| `--long-edge 9000` (pixel mode) | 9000x6750, `dpi` **192.262405**, `pHYs` 7569 px/m (`identify`: 75.69 px/cm), 16,948,376 bytes, `encode_ms` 2988, `ms` 2497, **`peak_rss_mb` 712** |
| the same project as a 1600 px preview | `ms` 315 + `encode_ms` 423, `peak_rss_mb` 54, 1,010,033 bytes |
| what the tools see | PNG `Resolution: 118.11x118.11 PixelsPerCentimeter`; JPEG `300x300 PixelsPerInch`, `jpeg:sampling-factor: 1x1,1x1,1x1`; TIFF `Resolution: 300, 300 pixels/inch`, `ICC Profile: <present>, 664 bytes`; all three `icc:description: sRGB IEC61966-2.1` |
| the profile against lcms2 | `magick export.png -profile /usr/share/color/icc/colord/sRGB.icc` → RMSE **0.378 of 65535** = 0.0015/255 over a 1200 px preview |
| visual inspection | `/var/tmp/pixlay-s6/preview.png`: eight slots, the concave slot continuous, the `{date}` caption `2019:07:14 10:32:00`, no white inside any slot |

### S6 · deviations from and additions to the review additions

1. **The per-format list gained a subtraction.** The review wrote "PNG = pHYs + iCCP (+ sRGB
   chunk)"; the profile is authoritative and the PNG specification says `sRGB` and `iCCP` should not
   both appear, so `sRGB` is *not* written. A test asserts its absence, so re-adding it is a
   deliberate edit rather than a silent one.
2. **The JPEG profile goes in `APP2` only.** The review offered "JFIF density **or** EXIF resolution
   + APP2 ICC"; JFIF is what `jpeg-encoder` writes and what every reader looks at first, and an
   export's metadata in one place is easier to verify than the same number in two. EXIF is not
   written at all (S6 has no EXIF requirement, and v1 does not preserve source metadata).
3. **The encoder writes no chroma subsampling for PNG/TIFF, and the CLI refuses the flag rather
   than ignoring it** — the S1 rule that a silently dropped flag looks like it worked.
4. **`image` left the production dependency set**, which the review's "for now the `image` crate
   stands in; S6 decides whether it stays" explicitly left open. It stays as a *test* dependency,
   which is where it earns its place (reading renders back and writing fixture photos).
5. **`--quality` was not added.** The criteria ask for chroma subsampling, not for a quality knob,
   and a flag nobody tests is a flag that breaks quietly; quality is 90 at one place in the code.

## S6.5 · Command history / project IO / hit testing (still windowless) — ✅ done (2026-09-21)

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

### S6.5 result (2026-09-21)

`[all criteria are in the tests in the repository; the numbers below are the release binary's output on this machine unless a test is named]`

The exit criteria, item by item:

| Criterion | Landing point | Measured |
|---|---|---|
| undo back to the initial state is pixel-identical; redo is isomorphic | `pixlay-render/tests/history.rs` (bitmaps in, only `draw`), `pixlay-cli/tests/history.rs` (the whole pipeline: decode → resample → grade → filter → `draw`, committed photos), `pixlay-core/tests/history.rs` (the document itself) | a **5-command walk** at `draw`'s boundary (6 renders) and a **6-command walk** through the pipeline (7 renders), each rendering **every state** it passes through: undo reproduces **0 differing pixels** for every state, redo reproduces every state in order, and a mixed undo/undo/redo/undo/redo/redo walk lands on the pixels of the document the stacks describe. Every state is required to *differ* from the one before it, so a command that changed nothing cannot let the walk pass vacuously. The document-level test runs **every one of the nine command kinds** and asserts document equality after undoing to the start and redoing to the end (12 commands, 13 states, compared as JSON bytes as well as by `PartialEq`) |
| save → load → save is byte-identical | `pixlay-core/tests/project.rs::save_load_save_is_byte_identical`, `save_as_beside_the_original_is_a_plain_copy`; `pixlay-cli/tests/cli.rs::save_writes_the_project_that_was_read` | two generations through `Project::save` and through the CLI are the same bytes as the first write; a copy beside the original is byte-identical; saving in place replaces the file with the same bytes; no `*.tmp` survives any of it (the directory is scanned, not assumed) |
| a missing file / a bad version gives a clear error and a non-zero exit code | `pixlay-core/tests/project.rs::a_missing_project_and_a_newer_version_report_clearly`; `pixlay-cli/tests/cli.rs::save_reports_a_missing_project_and_a_newer_version` | exit **2**, stdout empty, stderr naming the missing path (`…/absent.pixlay: No such file or directory`) or the policy (`document version 2 is newer than the supported version 1`), and **nothing written** — the `--out` path still does not exist afterwards |
| hit-test sweep: every template × every slot's centroid × 1 px outside every boundary, against the analytic answer | `pixlay-core/tests/hit.rs` | **12 templates, 64 slots, 258 edges × 9 samples**: 2,322 points one pixel *inside* a boundary (each must be that slot) and 2,322 one pixel *outside* it. Of the outside points **1,143** land in the neighbouring slot, **72** in a gutter and **1,107** past the canvas border — the three answers the criterion is about, each asserted to be non-empty so a sweep that stopped reaching one fails. The oracle is an independent **winding-number** containment (the implementation is even-odd), and every point is also checked against the structural fact that a cut template's slots tile the canvas exactly. **0 disagreements**, and 64/64 slots contain their own area centroid |
| hit testing including rotation and irregular slots | `pixlay-core/tests/hit.rs::a_rotated_slot_is_hit_exactly` | the library ships no rotated slot, so the test builds one: **7 angles** (0°, 15°, 30°, 45°, 90°, −22.5°, 40°), each sweeping a 61x61 grid against the analytic inverse-rotation oracle and keeping the 1,513–1,537 interior and 1,960–2,080 exterior samples that are more than ~5 px from an edge (closer ones are rounding-level ties), plus every corner sampled 1 px inward along the diagonal. **0 disagreements.** The irregular slot is the library's own L (`mosaic-8-s14` slot 6), and a point in its notch answers slot 7 — the case a bounding-box hit test gets wrong |
| the hit test agrees with what the renderer painted | `pixlay-render/tests/hit.rs` | `draw`'s output is compared pixel by pixel: **153,029 of 154,360 pixels (99.13%)** of the smoke template come out *exactly* a slot colour, and every one of them hits its own slot; **179,776 of 206,116 (87.22%)** for the gutter template, the remainder being its white cross and the antialiased edges. Both at 0° and at 30° of framing rotation, with identical partitions — a rotated *photo* does not move a slot. Each slot is sampled (21,590–45,404 pixels) |

**What the step added to the machine surface** (S1's CLI rules all hold: `--json`, stdout purity, exit 1/2, byte-identical under `C` / `zh_CN.UTF-8` / `de_DE.UTF-8`):

| Command | Measured |
|---|---|
| `hit --template mosaic-8-s14 --at 0.2,0.2` | `slot = 0`, exit 0 — geometry only: no photo is decoded and a project whose photos have moved still answers |
| `hit --template grid-4-2x2g --at 0.5,0.5` | `hit = false`, `slot = none`, exit 0 — "no slot owns this point" is an answer, not a failure |
| `save --project a.pixlay --out b.pixlay` | `bytes = 5030`, exit 0; `cmp a.pixlay b.pixlay` → identical. Two directories down the same project stores `../../photos/p.png` and resolves to the same file as the original |
| the `AGENTS.md` verification render (unchanged by this step) | 14043×10532, **ms 6189**, **encode_ms 1175**, **`peak_rss_mb` 1641**, **9,216,300 bytes** — byte-for-byte the file S6 measured, so nothing here touched the rendering path |

### S6.5 · decisions this step made

1. **The command vocabulary is one thing per command, and the two tempting extras are not in it.** There is no `SetTemplate`
   (choosing a template is how a document *starts*, not an edit to one: it changes the slot count and every cell) and `SetSource`
   does **not** reset the crop — `CropTransform::zoom` is absolute precisely so that swapping a photo keeps the area the user framed
   (contract §1), and a command that quietly re-framed would undo that decision. `SetGrade` and `SetFilter` are separate commands
   even though they are the same arithmetic, because they are two different user actions and one gesture is one command.
2. **Snapshots are structural, not a convention.** `History` holds documents and has no mutable accessor, and `apply` writes into a
   copy that becomes current only after `validate` passes — so an edit that would break a limit (a zoom past `MAX_ZOOM`, a canvas
   that no longer matches the template, a text layer naming a slot that does not exist) is refused and leaves **both the document and
   both stacks** untouched. That is what makes "any operation sequence" a real statement: every state in the stacks is a state the
   document actually had. The cost is a `CollageDoc` clone per command (~4–5 KB for a shipped template) and is accepted knowingly.
3. **Hit testing is geometry, and its input is a point, not a document.** `Template::slot_at` never looks at a cell, because nothing a
   crop can do moves a slot — the "including rotation" half of the criterion is therefore about a rotated *slot* (measured on a
   synthetic template, since the library ships none), and the render-level test pins the same fact from the other side (a photo
   rotated 30° still fills exactly its own slot's pixels). Boundary points stay unspecified, exactly as `Polygon::contains` already
   said; the sweeps keep 1 px away and the GUI's press is a pixel.
4. **Two CLI verbs, and a deliberate absence.** `hit` and `save` cover the interaction layer's two questions that are *not* a
   document edit; the command history gets no verb, because there is no CLI editing session for one to act on and the observable that
   matters (the pixels after undoing everything) is measured by tests that are stricter than a verb would be. `AGENTS.md`'s
   "nothing may be possible only in the GUI" is about capabilities, and every capability here — a hit test, a save — has a verb.
5. **`save` overwrites; `init` is the command that refuses.** A save that refused to replace the file it was pointed at would be a
   command nobody could use for its purpose, and the two are visibly different verbs with different promises (contract §5).
6. **A copy rewrites its relative photo paths.** `save_as` rebases every relative `source` onto the new directory, because a copy that
   pointed at nothing is silent data loss and because the GUI's "save as" needs it. The arithmetic is lexical
   (`std::path::absolute` + `Path::components`), so a copy can be written with the photos on an unmounted drive, and `..` is left as
   the filesystem resolves it — the test copies a project whose cell is spelled `../top.png` and checks that both files resolve to the
   same path. A save to the *same* directory rewrites nothing, which is what keeps "save, load, save" byte-identical.
7. **`write_atomic` fsyncs and names the target.** `File::create` a dotfile beside the target, `write_all`, `sync_all`, `rename` — the
   sync is what stops a rename from publishing a file whose bytes never reached the disk. An error names the file the caller asked for
   rather than the temporary one, since nobody asked for the latter; a failed write removes it.
8. **`--at` is refused outside `0..=1`.** The canvas *is* the unit square, so a point outside it is a caller that mis-scaled something,
   not a hit test with an unusual answer — the same reasoning as "a flag that is silently dropped looks honored" (S1).
9. **A defect found on the way and fixed: `--help` printed the `TEXT OPTIONS` block twice.** It was an editing accident in S5 that no
   test could see, because the usage text is only asserted to contain `USAGE:`. The subcommand summary now lists each verb once and
   `hit` / `save` with it.

### S6.5 · deviations from and additions to the review additions

- S6.5 was added by the 2026-09-20 review with no review additions of its own, so there is nothing to deviate from. Two things the
  step's own Work line did not name and this session added, both because the alternative was a silent lie: `save`'s **path rebasing**
  (see decision 6 — without it the command would write copies that point at nothing) and the **`hit` / `save` verbs** (decision 4;
  the step says the logic must be testable without a window, and the CLI is where "testable" is visible to a machine).
- S3's review additions had suggested merging hit testing into S3; S3's decisions kept it here (S3 decision 4), and nothing in this
  session needed that to be revisited.

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
| Elongated-slot clamp degradation | when the required zoom is > **1.5×** (`CLAMP_ZOOM_LIMIT` in `pixlay-core`, beside `MAX_ZOOM` / `MAX_ROTATION_DEG`), **limit the rotation angle**; the clamp result (`CropFit::rotation_limited`) carries a "limited" flag for the UI. **Implemented in S3 (2026-09-21)**: the reference is the *upright covering zoom*, not an absolute zoom — the absolute reading would refuse rotation to every strip template (that is what the 6.7–7.6× in the AGENTS entry actually measured) — and the kept angles plus the reasoning are in `docs/CONTRACT.md` §2 and "S3 · decisions" |
| `.pixlay` path resolution | relative to the project file; a missing file = a clear error + a non-zero exit code; atomic write (tmp + rename) |

### C. After S1, before S4 (still recommendations; locked as recommended unless objected to)

| Question | Recommendation |
|---|---|
| source ICC | v1 does not read the source ICC and interprets everything as sRGB, and the documentation states that this is a known limitation (doing it properly needs lcms2 + a rendering-intent definition) |
| output ICC | embed the sRGB IEC61966-2.1 profile bytes; do not pull in lcms2. **Executed in S6 (2026-09-21)**: the bytes are *generated* in `pixlay_imaging::icc` from the published colorimetry (ICC v4 `mntr`/`RGB `/`XYZ `, `para` TRC, Bradford `chad`) rather than committed as a third-party blob, and validated against the lcms2 profile committed in `photos/adobe-rgb-srgb.png` (colorants within 2.2e-4, and ImageMagick converting through it moves the pixels by 0.0015/255) |
| decoding backend | see "The two paths for the decoding backend" below; measure first, then decide, **and this decision determines S8's `depends`** |
| fonts | production does not bundle fonts (Noto Sans CJK is too large); the golden text tests use a small test font committed in the repository, and checks that render with system fonts are marked `#[ignore]`. **Executed in S5 (2026-09-21)**: production asks fontconfig for `sans-serif` and names no font; `crates/pixlay-cli/tests/fixtures/fonts/pixlay-test-sans.otf` (93 KB, a subset of Noto Sans CJK SC with `halt` kept, regenerated by `generate.py`) is what the text tests measure with, pinned by `FONTCONFIG_FILE` in a child process. **No test needs a system font at all** — so none is `#[ignore]`d for that reason, and S8's `check()` can run the text measurements in a font-free chroot |
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
  **Done, 2026-09-21: path A (the sandboxed loader) is picked; the measurements and the three reasons are in "S4 · decisions" 1.**

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
