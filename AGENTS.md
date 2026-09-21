# Pixlay

A Linux-native collage tool. 2–10 images; regular and irregular templates; per-slot framing
(pan / zoom / straightening by ±45° rotation); per-slot color grading plus a one-click global
filter; canvas-level text layers (free placement, with a tiled watermark as one of their modes,
supporting `{date}` and other EXIF-driven fields); export of high-resolution finished images
(physical size + DPI, or a specified long edge in pixels).
GPL-3.0-or-later · Rust · GTK4 + libadwaita shell · Cairo canvas · target platform Arch/AUR.

**Scope criterion: the shortest main path.** "Pick a template → place photos → adjust framing →
export" must take under three minutes. Before adding any feature, ask: does it make the main path
longer? If so, cut it.

**Locked identifiers**

| Item | Value |
|---|---|
| Repository | `https://github.com/YangtseSu/pixlay` (private) |
| Crates | `pixlay` / `pixlay-core` / `pixlay-imaging` / `pixlay-render` / `pixlay-cli` |
| Binaries | `/usr/bin/pixlay`, `/usr/bin/pixlay-render` |
| Config / project | `~/.config/pixlay/` · `.pixlay` |
| Toolchain | edition 2024 · resolver 3 · `rust-version` follows Arch's installed rustc (currently `1.98`); every baseline number is measured `--release` |
| app-id | `org.yangtse.Pixlay` (own domain `yangtse.org`, reversed; not a borrowed `io.github.*` namespace) |
| i18n | gettext, domain `pixlay` (source language English; `.pot`/`po/` at the repository root; extraction only via `xgettext --language=Rust`) |

## Verification entry (must run after every change)

    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo run --release -p pixlay-cli -- render --project crates/pixlay-cli/tests/fixtures/verify.pixlay --dpi 300 --stats --out /var/tmp/a.jpg

Of the last two: the second one produces a real image, and you must look at it directly.
**If you cannot see the image, do not judge whether the render is correct.**
The project is `crates/pixlay-cli/tests/fixtures/verify.pixlay`: eight photos on `mosaic-8-s14`
(JPEG, PNG, a 16-bit PNG, a HEIC, one carrying EXIF Orientation=6, one carrying a date) and, since
S5, one `{date}` text layer reading that photo's EXIF date, so the command exercises decode,
resample, the clamp, `draw`, the text layout and the encoder in one run. Until S3 the
command used `--template mosaic-8-s14`, which renders every cell empty and is now a *white sheet*:
the flag is a geometry smoke (it checks that the template loads and the output path works), not an
image to judge. `mosaic-8-s14` has been valid since S1 and, since S2, is emitted by the template
generator (`pixlay-core/src/templates/generator.rs`) under the same name and the same
`templateVersion`; `--stats` makes each round's ruler machine-readable. `pixlay-render templates`
lists what this build ships, and `pixlay-render init --template <name> --out x.pixlay` writes a
project to start from. Since S7 `cargo test` also builds the GUI; its tests need a display and re-run
themselves under `xvfb-run` where there is none (pinning `GTK_IM_MODULE=gtk-im-context-simple`, because
GTK's ibus module recurses without a session bus), so the entry still works on a build box.

Measurement rules that go with it:

- Write artifacts to a disk path (`/var/tmp` or `$XDG_CACHE_HOME`), **never `/tmp`**: on this
  machine `/tmp` is tmpfs (7.5 GB free), an A0 photo-content PNG is 342 MB and a TIFF 476 MB, so
  writing tmpfs costs another copy in RAM.
- "Looks right" is not a criterion. Pixel-level conclusions (did a slot change, is everything
  outside it clean, how much blends across a seam) become a probe: sample coordinates or count
  blended pixels, and print numbers.
- Peak memory is always `/proc/self/status`'s `VmHWM`; time is `CLOCK_MONOTONIC` wall clock, with
  compositing and encoding reported separately.
- Measuring encoder performance requires **non-flat** content: flat color blocks skew A0 PNG size
  and time by 78× and 4.6× respectively.

## Language conventions

- **Everything machine- or upstream-facing is English**: commit messages, code comments (inline and
  doc), identifiers, test names, logs and error messages, configuration keys, the PKGBUILD's
  `pkgdesc` — every text aimed at machines or upstream maintainers.
  *Rationale: comments and error messages are text that talks to upstream crates, issues and
  patches; mixing languages means translating them again every time.*
- **All repository documents are English**: `AGENTS.md`, everything under `docs/`, and any file
  added later. This includes the step tracker's prose, its measured numbers and its rulings.
  *Rationale: these files are what every new session and every future contributor reads first, and
  a document that switches language halfway is one that has to be re-read twice. `AGENTS.md` is
  cited by name from code comments and tests, so a cited heading must be citable verbatim.*
- **Reverting this is a decision, not a default.** If any document is to be Chinese again, the
  human says so explicitly; do not carry a second language "just for the planning files".
- **UI copy goes through i18n only, and the source language is English**: GUI strings are English
  source strings; when `LANG` is missing, unknown, or has no translation for that language,
  everything falls back to English. Translations are a language pack added later, not part of S7.
  The translation layer lives **only in `pixlay`** — `pixlay-core` / `-imaging` / `-render` / `-cli`
  must not depend on any i18n library; their error messages are English identifying text, which the
  GUI attaches as-is.
  *Rationale: core's errors have to talk to upstream crates, and translating them only makes them
  ungreppable; keeping the copy in one layer is also what makes extraction possible.*
- **The CLI is not multilingual**: its output is always English and it does not read `LANG` /
  `LC_ALL` / `LANGUAGE` or format numbers and dates per locale (dates are always ISO 8601); the
  values in `--json` are stable English enums, never localized.
  *Rationale: the CLI is a machine interface, and output that varies with the environment means
  guessing again on every parse.*

## Commit discipline

- **Commit once per completed step** (finishing one step in `docs/STEPS.md` produces at least one
  commit). Do not batch several steps into one commit.
- **Pushing requires the user's explicit permission first.** Without it, commit only and never
  push: do not `git push` on your own initiative and do not change remote configuration.
- **End every commit message with `🤖`** (on its own line). The criterion is whether the change
  **contains AI-generated or AI-modified content** — anything touched by AI carries it, and at this
  stage AI writes all the code, so in practice every commit carries it.
- **Messages are English** (see "Language conventions"). First line format:
  `<step>: <what changed>`, e.g. `S2: Freeze template geometry and invariant tests`. Non-step
  changes (docs, CI) use the `docs:` / `chore:` prefix.
- **Run the two commands of the "Verification entry" before committing** (when they do not apply to
  this step, run the step's own verification command); if it is red, do not commit.
- Never committed: `target/` (see `.gitignore`). Committed: `Cargo.lock` (AUR discipline).
- **"Done" means the progress line is rewritten and committed** (see "Session and persistence
  discipline"), not that the code is written and the tests are green.

## Session and persistence discipline

**A conversation is not storage.** Sessions get truncated, cleared or deleted; a conclusion that
exists only in the conversation never happened. A new session reads files, not someone else's
transcript, and the "Current progress" line in `docs/STEPS.md` is the **only authority**.

- **A step is complete when the "Current progress" line is rewritten and committed.** Green tests
  and good numbers are necessary, not sufficient.
  *Rationale: the S1 contract review had actually passed; the AI only committed the defect fixes
  and the measurements and never touched the progress line, so deleting the original session erased
  the gate — the next session would have read "next: review the contract" as if the review had
  never happened.*
- **A human ruling is written to disk in the same turn.** Once the human rules (keep/drop, pass/
  reject, fix a threshold, decide a deviation), write it into the files within the same turn: a
  ruling block (date + verdict + basis) plus the rewritten progress line, then commit. Do not
  "remember it and write it later", and do not restate a ruling only in the reply while leaving the
  files untouched.
  *Reference shape: S0's `Ruling (2026-09-20, human): Cairo stays` plus the progress line
  `S0 — done and ruled on`.*
- **Closing a gate takes five parts; missing one means it is not done**:
  1. the ruling block written into that step's "Result" subsection in `docs/STEPS.md`;
  2. the "Current progress" line rewritten to "done and passed \_\_\_ → next X";
  3. the matching entry under "Where humans must step in" marked as passed or removed;
  4. any shape the ruling changed synchronized into `docs/CONTRACT.md`;
  5. commit.
- **Writing it down is not a prerequisite of the next step; it is the other half of this ruling.**
  After a ruling, do not continue into the next step in the same session: a gate's next action is a
  new session (see `docs/STEPS.md`, splitting principle 5).
- **Never reconstruct a record from memory.** If a conclusion rests on a number or ruling that only
  ever appeared in conversation and is not in the files, re-measure it or ask the human;
  reconstructing it from memory is exactly the failure splitting principle 5 guards against
  "treating an unwritten draft as an established premise".
- **Self-check before the session ends**: are this round's conclusions (rulings, thresholds,
  measured numbers) all in the files, and is `git status` clean?
  *Rationale: how a session ends is not up to the AI; the only thing it can guarantee is the state
  on disk.*

## Version policy: track the latest

The target platform is Arch (rolling), so **there is no reason to stay compatible with old
versions**. Everything follows the latest stable release.

- **Toolchain**: follow Arch's installed rustc. `rust-version` is Arch's version verbatim (currently
  `1.98`) with no backward compatibility, and no giving up a feature "to support older toolchains".
- **Dependencies**: latest stable only. No `=` / `<=` caps and no pinning to older versions; at the
  start of every step run `cargo update --workspace` and then the "Verification entry". Breakage
  from upgrades is **fixed, not shimmed**: no compatibility shims and no legacy paths left behind
  (the same rule as clean cutover).
- **edition / style edition / resolver**: the highest the current stable supports (currently
  edition 2024, resolver 3).
- **System libraries and bindings**: GTK / cairo / pango / libadwaita follow Arch's system versions
  (currently gtk4 4.22.5, cairo 1.18.4, libadwaita 1.9.4), and the bindings take the latest.
  Downgrade a binding only when it demands a newer system version than Arch ships — never downgrade
  the system.
- **CI / packaging**: an `archlinux:latest` container, no pinned image tag.
- **Keeping the dependency set minimal** does not conflict with tracking the latest: few, but each
  one current.

This policy **does not go into tests**: tests must not depend on the network or on a toolchain
version. It is maintained by two things — the `cargo update` at the start of each step, and looking
at `Cargo.lock` diffs during review.

## Hard constraints

- **Geometry uses normalized coordinates `[0,1]` only.** Absolute pixels are allowed only at the
  rendering boundary, never inside `CollageDoc` or a template.
  *Rationale: layout units and device pixels were once mixed, and two consecutive rounds drew wrong
  conclusions from it.*
- **Preview and export must call the same `render::draw(doc, target)`.** A second renderer is
  forbidden.
  *Rationale: the product is the exported image, and two renderers necessarily diverge — measured:
  the same CJK text measured identically in two engines and still differed by 14.6% of pixels.*
- **All resampling belongs upstream; the canvas only blits and clips.** Decoding, downsampling,
  rotation interpolation and grading all happen in `pixlay-imaging`; Cairo receives bitmaps that are
  already the right size. *Rationale: this keeps Cairo's weak filtering (bilinear + mipmap only)
  out of the finished product.*
- **Encoding and metadata happen in one pass.** Chroma subsampling, ICC and DPI must live in the
  same pipeline; "encode first, patch the metadata afterwards" is forbidden.
  *Rationale: the second pass re-encodes with default parameters and silently drops 4:4:4 to 4:2:0
  (measured 2.71 MB → 1.49 MB).*
  Implementation meaning: **Cairo supplies pixels only; the encoder writes the metadata itself.**
  Measured: `cairo_surface_write_to_png` on A0 emits only IHDR/bKGD/IDAT — no pHYs and no iCCP
  (`identify` reports `Units: Undefined`) — and `cairo_surface_set_fallback_resolution` has no
  effect on a bitmap backend. Writing PNG through Cairo necessarily loses DPI.
  Since S6 the encoder is `pixlay_imaging::encode`: PNG `pHYs` + `iCCP`, JPEG JFIF density + `APP2`
  ICC + the `SOF0` sampling factors of the request, TIFF `XResolution`/`YResolution` + tag 34675 —
  each written while the pixels go out, never by a second pass over the finished file. The DPI/ICC
  rounding rules and the per-format field list are in `docs/CONTRACT.md` §5.
- **The evaluation order is frozen** and must not be reordered:
  `decode + color normalization → geometry (crop / flip / 90° / arbitrary rotation) → per-slot
  grading → global filter → slot compositing → canvas decoration → text layers → output transform`
  *Rationale: operations that change the coordinate system must run first, and content layers
  positioned relative to the canvas must run last. Counterexample: add a watermark and then rotate —
  the watermark rotates too and gets blurred by interpolation.*
- **Composite onto opaque white.** Source alpha and any canvas or in-slot area not covered by a
  photo are always flattened to white; an export is never transparent.
  *Rationale: the product is a photo collage, preview and export must be pixel-identical, and a
  transparent export would open a second encoding branch.*
- **Source images are read-only.** Every edit is a parameter, not a pixel overwrite. Under no
  circumstances may a user's source file be written back.
- **Resampling must happen in the correct color space**: `sRGB → linear → process → sRGB`.
  Intermediate buffers are **16-bit**, and quantization happens at the end of the pipeline.
  *Rationale: 8-bit intermediate buffers amplify banding, producing visible steps in skies, gray
  walls and shadows.*
- **Template geometry is frozen data**, carries a `templateVersion`, and must be generated
  deterministically. The layout of an existing project must never change.
- **The framing abstraction is "parent container clips + child primitive transforms"**, not
  "compute a clip rectangle every frame and drawImage".
  Framing state uses **absolute zoom** (displayed width / canvas width), not "a multiple of fill".
  *Rationale: the latter makes the framing jump when the user swaps a photo, because the "fill"
  baseline moves with the image's aspect ratio.*
- **Rotation crops edges only; it never grows the canvas.** After a rotation angle or slot geometry
  change the clamp **must be recomputed** so the slot stays filled.
  *Rationale: collage slot sizes are fixed by the template, and this cuts the most troublesome
  branch of a general-purpose editor: growing the canvas.*
- **GTK types do not implement `Send`/`Sync`.** Background decoding and scaling must return to the
  main thread through a channel; a GTK object must never be held across threads.
- **`ui` must not touch pixels directly.**

## GNOME HIG (constrains the `pixlay` shell only)

Spec: https://developer.gnome.org/hig/ — the platform definition is GTK4 + libadwaita, consistent
with "Module boundaries". **It applies to the GUI layer only**: `pixlay-core` / `-imaging` /
`-render` / `-cli` have no interface, are unaffected, and must not pull in GTK for it.
HIG has no version number and is **not frozen**; cite URLs and section names, and at the start of
every step that touches UI re-read them and update `docs/HIG-REVIEW.md`.

- **Widgets**: use libadwaita containers and widgets by default (`AdwApplicationWindow` /
  `AdwToolbarView` / `AdwHeaderBar` / `AdwToast` / `AdwStatusPage` / `AdwAboutDialog` and so on). S7
  landed the shell as `AdwApplicationWindow` + `AdwToolbarView` + `AdwHeaderBar` + `AdwToastOverlay` +
  `AdwBanner` + `AdwOverlaySplitView` + `AdwPreferencesPage`, with one custom-drawn widget — the
  canvas, whose stated reason is that it draws the document itself. A custom-drawn widget is the
  exception and needs a stated reason.
- **Styling**: use only libadwaita style classes and CSS variables; hard-coded colors and spacing
  are forbidden (they break dark mode and high contrast). App styling **follows the system**
  (`AdwStyleManager` stays at its default; never force light or dark), and v1 ships no per-app style
  switch. The canvas and the export are **always opaque white, independent of the UI theme** — white
  is content, not styling (see "Hard constraints").
- **Keyboard**: standard shortcuts per HIG `reference/keyboard`; `Alt+*`, `Super+*` and
  system-reserved combinations are forbidden; the main path must be walkable with the keyboard
  alone, and every action needs a keyboard path. Since S7 the table is data —
  `crates/pixlay/src/app.rs::ACCELERATORS`, which the bindings, the shortcuts dialog and the HIG test
  all read — and `po/` holds the extractable strings.
- **Accessibility**: every interactive control needs an accessible name (HIG
  `guidelines/accessibility`).
- **Copy**: follow HIG `guidelines/writing-style` (sentence case, no jargon, no honorifics); the
  mechanism is still the i18n of "Language conventions" and is not repeated here.
- **Deliberate deviations (do not fix, not bugs)**:
  - no GNOME Shell search provider, no notification workflow, no phone-style layout — the same
    discipline as the "not doing" list;
  - no per-app style preference (light / dark / system): it would lengthen the main path, and
    "follow the system" already covers how users express "I want dark";
  - large-text mode **must not** scale canvas text layers: that is document content, and preview and
    export must stay pixel-identical;
  - HIG `patterns/containers/selection-mode` **does not apply** (no collection views, no
    multi-select batch operations); that page itself states that "when editing is the primary
    interaction there should be no separate edit mode", which points the same way as "no mode
    switching" — **not** a deviation.
- Whatever can be machine-checked lives only in S7's tests (see `docs/STEPS.md`); the visual part
  goes item by item through `docs/HIG-REVIEW.md`.

## Directions not to "improve"

- **Do not replace Cairo with GPU rendering** (wgpu / vello / skia and the like). Cairo is a
  deliberate choice: the CPU has no texture size limit (exports reach 139.5 MP), and preview and
  export are identical by construction; GTK4 already depends on Cairo, so packaging costs nothing.
  This app handles ≤10 images and measures 40–250 ms for full-resolution compositing, so a GPU has
  no payoff.
- **Do not introduce a scene-graph framework to "simplify" interaction.** Hit testing, z-order and
  the undo stack are written by hand; **this is a known cost, not an oversight.**
- **Do not default to mozjpeg.** The default is libjpeg-turbo 4:4:4; mozjpeg is optional only.
  *Measured on A1/69.7 MP: 4590 ms / 6.79 MB vs 475 ms / 8.50 MB — 9.7× the time for 20% less size.*
- **Do not add mode switching** (edit mode / collage mode). The user's mental model is always "I am
  making a collage"; editing is a property of the currently selected slot.
  *(HIG's `selection-mode` does not apply, and its own advice points the same way — see "GNOME HIG".)*
- **Not doing**: beauty retouching, levels / curves, online geocoding, RAW, brush marking, a
  single-image retouch mode.
  *Rationale: levels / curves is a professional control that needs a full ICC pipeline and is opaque
  to the target user; geocoding carries API quotas, identity requirements and privacy costs, and a
  hand-typed place name replaces it; the rest is unrelated to the core value of a collage.*

## Module boundaries

    pixlay-core     CollageDoc, templates, geometry, framing transforms, command history. Must not depend on gtk / cairo
    pixlay-imaging  decoding (glycin), resampling, grading, EXIF, color spaces, encoding (PNG/JPEG/TIFF). Must not depend on gtk or cairo
    pixlay-render   the single draw(doc, target), Cairo + pangocairo. Must not depend on gtk
    pixlay-cli      windowless render entry point, automation and verification tooling, and the AI's operating surface. Must not depend on gtk4
    pixlay          gtk4 + libadwaita shell and interaction

`pixlay-core` and `pixlay-imaging` must be able to `cargo test` without a display, staying in the
millisecond-to-second range.
`pixlay-cli` must not depend on gtk4 — it is the fast loop, and compile time is the iteration cost.

**`pixlay-cli` is the only machine-operable surface.** Every capability must first exist as a
subcommand: zero interaction (no stdin reads, no prompts), stdout carries machine-readable results
only, diagnostics go to stderr, exit codes are fixed, identical input yields identical output, and
**output is unaffected by locale**.
**Nothing may be possible only in the GUI and not in the CLI.**
*Rationale: the model cannot see windows and can only read the CLI's stdout, and "looks right" is
not a criterion — a visual conclusion must become a number (a probe) in the CLI. Contract details
(subcommands, fields, exit codes) live in the tests and `docs/STEPS.md` and are not repeated here.*

## Invariants that must hold

- templates: zero overlap between slots; no interior hole in their union; a cut template's areas sum
  to exactly 1.0
- framing: for any (rotation, zoom, offset) combination, the clamped photo covers the entire slot
- grading identity: with `factor=1`, `s=1`, `Δ=0` the output is **pixel-identical** to the input
- render consistency: the same composition at `2N` and `N`, downsampled, stays below the RMSE
  threshold (measured 2.62/255; threshold 6)
- encoding: physical-size mode must carry DPI + ICC, and chroma subsampling must match the request

**Every rule a test can enforce lives only in the tests; this file does not restate it.**

## Open / to be proven

- A0@300dpi on Cairo (9933×14043, 139.5 MP) has been **re-measured and passes** (S0, 2026-09-20; the
  formal criterion is the in-repo probe, not a one-off script): 185 ms for 2 slots and 551 ms for
  10, peak `VmHWM` 941 MB compositing and 1340 MB including encoding (budget 2.5 GB), PNG and JPEG
  both emit 9933×14043, and the white-base / seam / text criteria are all green. Numbers in
  `docs/STEPS.md` under "S0 results".
  **Ruling (2026-09-20): Cairo stays** — "do not replace Cairo with GPU rendering" remains in force.
- glycin in a non-Flatpak environment: **settled by measurement (S4, 2026-09-21) — the sandboxed
  path is what gets used.** `glycin` 4.0.0 decodes PNG/JPEG/HEIC/AVIF in 11–110 ms per 2400x1600
  file, including a 12-bit HEIC and an EXIF-Orientation-6 JPEG, and it does so under an *empty*
  environment (`env -i PATH=/usr/bin:/bin HOME=/nonexistent`, no session bus, no `XDG_RUNTIME_DIR`).
  The distribution's `glycin-thumbnailer` failing for every format was that *binary's* problem, not
  this crate path's. `glycin-builtin` + `builtin-image-rs` is not a candidate: it covers no HEIC and
  no AVIF (those live in the external `glycin-heif` loader), and its in-process frames hang under a
  plain async executor — they complete only while a glib `MainContext` is being iterated, which
  `pixlay-imaging::driver` therefore provides on one private thread. Numbers and reasoning in
  `docs/STEPS.md` "S4 · decisions" 1 and `docs/CONTRACT.md` §4.1.
- Pango's CJK line-breaking and punctuation squeezing: **settled by measurement (S5, 2026-09-21)**.
  Kinsoku is Pango's own and is correct — over four CJK paragraphs at six widths, no line starts
  with `、。，．：；？！）”` and none ends with `（“`, and `他他他说。他` at a four-em width breaks
  `他他他 / 说。他`, i.e. the breaker pulls the break back one character rather than starting a line
  with the mark (`pixlay-render/tests/text/measure.rs`).
  **Squeezing it does not do**: `。，` advances two full ems, exactly like two isolated marks, and no
  layout option changes that. `pixlay-render` therefore implements the rule itself — in a run of
  consecutive CJK punctuation every mark but the last is asked for the font's OpenType `halt`
  (half-width) feature, so `。”` costs 1.5 em while a lone `。` keeps its blank — and what a
  compressed mark *looks* like stays the font's decision (a font without `halt` does not compress).
  The one part of JLREQ left out is trimming a mark that ends a line, which is in
  `docs/CONTRACT.md` §6 as a non-goal.
- The conservative clamp for irregular slots — computed from a circumscribed axis-aligned rectangle,
  allowing slight white slivers — is **not needed and not used**: S3's clamp tests the outline's own
  vertices, which is exact for a concave slot too (a rectangle contains a polygon iff it contains its
  vertices), so no sliver is allowed and none is measured (28,800 framings against a 1e-6 tolerance,
  the worst sample 2.2e-16 past the photo's edge).
- **Framing clamp (S3, 2026-09-21): decided and implemented.** The framing zoom floor for very
  elongated slots is not made unbounded by rotation: `CLAMP_ZOOM_LIMIT = 1.5` is a multiple of the
  *upright covering zoom* (measured: a ten-column strip needs 6.0× with a 4:3 photo), and past it the
  clamp reduces the requested rotation angle to the widest that fits instead of magnifying further.
  A narrow slot is therefore never degraded for being narrow — rotating one costs *less* than leaving
  it upright (5.19× against 6.0×, measured) — while a matched 4:3 slot keeps 27.3° of the 45° asked
  for. The rule, the per-aspect angle table and the pan-clamp decision are in `docs/CONTRACT.md` §1/§2.
- Seam behavior re-measured (2026-09-20): blended pixels on shared edges / seam length ≈ 1.08,
  **identical** at A0 and at 1/5 size → the blend width is one physical pixel and independent of
  output resolution, with no strong bleeding. **Remaining question: this 1 px seam is visible in a
  low-resolution preview, while at a 300 dpi export 1 px ≈ 0.085 mm and is invisible.** Preview and
  export share one path; whether to accept this seam still needs a decision.

## AUR discipline

- Commit `Cargo.lock`; keep dependencies minimal; `cargo vendor` must pass
- The PKGBUILD uses `--frozen --offline`, `depends=('gtk4' 'libadwaita')`
- SPDX is `GPL-3.0-or-later` throughout (not `-only`)

## Dependency registry

Register every new dependency here: **name / version / why / size and impact**. The criteria are
"how much code would deleting it cost" and whether it weakens a hard constraint. Policy in "Version
policy: track the latest": latest stable only, no upper pin.

| Dependency | Used by | Why | Notes |
|---|---|---|---|
| `serde` + `serde_derive` 1.0.229 | `pixlay-core` | `.pixlay` is JSON and every `CollageDoc` field has to round-trip; hand-written serialization means reimplementing format validation | Small, no system dependencies |
| `serde_json` 1.0.151 | `pixlay-core`, `pixlay-cli` (dev) | JSON read/write; `deny_unknown_fields` turns "misspelled field" into a load-time error | Same |
| `thiserror` 2.0.20 | `pixlay-core`, `pixlay-render` | core/render errors are typed errors (part of the contract); `anyhow` is allowed only in `pixlay-cli` | Pure macro, zero runtime |
| `cairo-rs` 0.22.9 | `pixlay-render` | The only rendering backend; GTK4 already depends on cairo, so packaging is free | System cairo 1.18.4; the `png` feature is dev-only (golden image read/write) |
| `png` 0.18.1 | `pixlay-imaging` | The PNG writer of the one-pass encoder (S6). `image`'s PNG writer exposes neither `pHYs` nor `iCCP` (their values stay at the defaults) and Cairo's emits no `pHYs` at all, so neither can carry an export's DPI | Pure Rust; it was already in the tree through `image`, so the download set did not grow |
| `jpeg-encoder` 0.7.1 | `pixlay-imaging` | The JPEG writer of the one-pass encoder (S6): `set_density` (JFIF), `set_sampling_factor` (4:4:4 / 4:2:2 / 4:2:0) and `add_icc_profile` (`APP2`), which is exactly the "pixels + sampling + ICC + DPI in one pass" the constraint names | Pure Rust; already in the tree through `glycin-image-rs`. Measured against the previous writer (`image` = zune-jpeg): +1.1% bytes, −27% time at A0/300dpi/q90/4:4:4 |
| `tiff` 0.11.3 (`lzw`) | `pixlay-imaging` | The TIFF writer of the one-pass encoder (S6): `XResolution`/`YResolution`/`ResolutionUnit` and tag 34675 for the profile. Only the `lzw` feature is enabled — it is the compression the S0 baseline measured, and every reader understands it | Pure Rust (`weezl`); already in the tree through `image` |
| `image` 0.25.10 | `pixlay-cli` (**dev only** since S6) | It was S1's encoder stand-in and S6 replaced it (`pixlay_imaging::encode` writes the DPI and the ICC profile that this crate's writers leave at their defaults). What it is still for: the CLI's **tests** read renders back with `image::open` (PNG/JPEG/TIFF) and write flat photos to render against, and `pixlay-cli/tests/fixtures/generate.py` produced the fixtures | Not a production dependency any more, so the shipped binary no longer links it |
| `glycin` 4.0.0 | `pixlay-imaging` | The decoding backend, measured against the in-process alternative (S4): the sandboxed loader is the only one of the two that decodes HEIC and AVIF, and it works with an empty environment | Pulls `glib`/`gio` and, through `cfg(target_os = "linux")`, `libseccomp` / `bubblewrap` / `fontconfig` / the distro's loader packages — this is what S8's `depends` must name |
| `glib` 0.22 / `gio` 0.22 | `pixlay-imaging` | The decode is driven on a private `MainContext`: a glycin frame request only completes while one is iterated (measured: every frame hung under a plain executor until glycin's own 60 s limit). `glib`'s `futures` feature provides `MainContext::block_on`; `gio::File` is glycin's own input type | Already in the tree with `glycin`; named here because the API is used directly |
| `pangocairo` 0.22.9 | `pixlay-render` | Canvas-level text: a `pango::Layout` drawn through `pangocairo` is the only way shaped text reaches a cairo context. The family is the system's `sans-serif`; the tests pin the committed subset under `crates/pixlay-cli/tests/fixtures/fonts/` with `FONTCONFIG_FILE` | Pulls `pango` + `pango-sys` alongside the `cairo`/`glib` S4 already had, and Arch's `pango` 1.58.2 is in the GTK stack S7 links anyway |

|`gtk4` 0.11.5 + `libadwaita` 0.9.2|`pixlay`|The shell: the window, the rows, the utility pane and the dialogs. The `gtk_v4_10` / `v1_8` feature levels are the lowest that carry `GtkFileDialog` and `GtkColorDialogButton` (4.10 dropped the deprecated chooser dialogs) and `AdwDialog` / `AdwToastOverlay` / `AdwShortcutsDialog`|System gtk4 4.24 / libadwaita 1.10 through pkg-config; GTK already depends on cairo, pango and gdk-pixbuf, so the download set grows by the bindings alone. Linked by `pixlay` only — the other four crates must not name it|
|`gettext-rs` 0.8.0 (`gettext-system`)|`pixlay`|i18n, as `docs/STEPS.md` decided before S7: the same gettext toolchain GTK and libadwaita use for their own copy, so `.po`, the `.desktop` file and AppStream metainfo (S8) all go through one pipeline. `po/POTFILES` and `po/pixlay.pot` are committed|Tiny; `gettext-sys` links the system `libintl` rather than building a private copy. Only `pixlay` depends on it, which is what the language conventions require|

`pangocairo` was a temporary S0 spike dependency, left with the spike (and with the spike's use of
`cairo-rs/png`), and came back in S5 — registered in the table above, where it says what it is for now.
`gtk4` + `libadwaita` + `gettext-rs` were added by S7, the first step that has a window at all.
