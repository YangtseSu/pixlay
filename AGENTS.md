# Pixlay

A Linux-native collage tool. **The editor is the whole application**: open it, add 1–9 photos (a tenth
and beyond are ignored with one report), pick a layout, adjust, export. Regular and irregular
templates; per-slot framing (pan / zoom / rotation by any angle); a canvas frame (gap / corner radius /
colour); export of high-resolution finished images as **PNG or JPEG** (a specified long edge
in pixels — physical size and DPI left with S12d, TIFF and the JPEG chroma request with S12c).
**The product is only a collage** (ruled 2026-09-22, S12c): it places photos and frames them. It has no
colour grading, no text layer, no watermark and no date stamp — S4's per-slot grade and one-click filter
and S5's canvas-level text layers were built and then removed, because none of them is on the main path
and the text layer was the one feature whose pixels depended on the host's installed fonts.
GPL-3.0-or-later · Rust · GTK4 + libadwaita shell · Cairo canvas · target platform Arch/AUR.

**Scope criterion: the shortest main path.** "Open → add photos → pick a layout → adjust → export" must
take under three minutes. Before adding any feature, ask: does it make the main path longer? If so, cut it.
*Ruled 2026-09-25: **the picker stage was removed.** The window opens on the collage and photos enter
from outside it — the platform's file chooser, a drop from the file manager onto the canvas, or
`pixlay a.jpg b.jpg` on the command line, in argument order — because the picker judged nothing the file
manager cannot, and its ordered list is replaced by the canvas's own spatial order plus an arbitrary
two-cell swap (`docs/completed/2026-09-25-STEPS.md`, ruling 31). That supersedes the photos-first route of
2026-09-22 and the review that proposed it (`docs/archive/2026-09-22-UX-DIRECTION.md`).*

**Locked identifiers**

| Item | Value |
|---|---|
| Repository | `https://github.com/YangtseSu/pixlay` |
| Crates | `pixlay` / `pixlay-core` / `pixlay-imaging` / `pixlay-render` / `pixlay-cli` |
| Binaries | `/usr/bin/pixlay`, `/usr/bin/pixlay-render` |
| Config / project | `~/.config/pixlay/` · `.pixlay` |
| Toolchain | edition 2024 · resolver 3 · `rust-version` follows Arch's installed rustc (currently `1.98`); every baseline number is measured `--release` |
| app-id | `org.yangtse.Pixlay` (own domain `yangtse.org`, reversed; not a borrowed `io.github.*` namespace) |
| i18n | gettext, domain `pixlay` (source language English; `.pot`/`po/` at the repository root; extraction via `xgettext` — the shell's Rust strings, the desktop template with `--language=Desktop`, and the metainfo through gettext's AppStream ITS rules, all joined into the one `po/pixlay.pot` by `po/extract-pot`, since S16) |

## Verification entry (must run after every change the product can see)

    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo run --release -p pixlay-cli -- render --project crates/pixlay-cli/tests/fixtures/verify.pixlay --long-edge 14043 --stats --out /var/tmp/a.jpg

**The four are for a change the product can see** — code, shipped data, a template, a constant the render
reads. A change it cannot see (docs, `.gitignore`, the repository layout) runs only the command that reads
what changed, if one exists: "Commit discipline" states the rule, and a path a test resolves is the case
that decides it.

Of the last two: the second one produces a real image, and you must look at it directly.
**If you cannot see the image, do not judge whether the render is correct.**
The project is `crates/pixlay-cli/tests/fixtures/verify.pixlay`: eight photos on `mosaic-8-s14`
(JPEG, PNG, a 16-bit PNG, a HEIC, one carrying EXIF Orientation=6, one carrying a date), so the command
exercises decode, resample, the clamp, `draw` and the encoder in one run. It carries no text layer any
more: S12c removed them, and the document is a `docVersion`-3 file. Until S3 the
command used `--template mosaic-8-s14`, which renders every cell empty and is now a *white sheet*:
the flag is a geometry smoke (it checks that the template loads and the output path works), not an
image to judge. `mosaic-8-s14` has been valid since S1 and, since S2, is emitted by the template
generator (`pixlay-core/src/templates/generator.rs`) under the same name and the same
`templateVersion`; `--stats` makes each round's ruler machine-readable. `pixlay-render templates`
lists what this build ships, and `pixlay-render init --template <name> --out x.pixlay` writes a
project to start from. Since S7 `cargo test` also builds the GUI; its tests need a display and run on one
the harness provides — a private headless `mutter` it starts itself, which is the *test environment* and
not a dependency of the product (no manifest and no `depends` names a compositor). **Mutter when mutter is
available** (ruled 2026-09-26, human): it is the compositor this app is developed against and the one whose
window behaviour these tests measure. It needs no GPU node — measured 2026-09-26 with `/dev/dri` hidden,
which is a container's shape: `Created surfaceless renderer without GPU`, and the suite green behind it —
but it does need a **session bus** (`dbus-run-session`; without one it aborts in `set_gnome_env`) and a
machine-id for `dbus-daemon` to hang that bus on. Where mutter cannot run at all, the harness can be told to
use the display the process already has — `PIXLAY_TEST_CHILD=1`, e.g. `PIXLAY_TEST_CHILD=1 xvfb-run -a cargo
test` — and its own failure message says so; **an Xvfb is not mutter's equal**: with no window manager GTK
frames the window inside its own surface, so every window geometry the suite reads comes out 10 px smaller in
each direction (measured 1090x584 against 1100x594), and a run on it is a fallback run whose numbers are that
display's. So the entry works on a build box and on a machine that is in use. **The GUI suite is the
machine's, not CI's** (ruled 2026-09-27, human): no display a runner could give it worked — the
`archlinux:latest` container's headless mutter never presented a frame, the runner's own mutter dies of its
GL setup, and a headless Weston runs the suite only to fail the HIG walk against the runner's older GTK —
so `ci.yml` runs the entry's commands **minus this suite** (`cargo test --workspace --exclude pixlay`,
plus the shell's `packaging.rs`, which needs no display) and
then the complete build — `meson setup` / `meson compile` (both binaries, the shell included) and
`meson install` into a staging root, which is the definition `makepkg` wraps — and
every error is recorded in `docs/steps/S33-ci-skips-the-gui-suite-done.md`.


Measurement rules that go with it:

- Write artifacts to a disk path (`/var/tmp` or `$XDG_CACHE_HOME`), **never `/tmp`**: on this
  machine `/tmp` is tmpfs (7.5 GB free) and an A0 photo-content PNG is 342 MB, so writing tmpfs
  costs another copy in RAM.
- "Looks right" is not a criterion. Pixel-level conclusions (did a slot change, is everything
  outside it clean, how much blends across a seam) become a probe: sample coordinates or count
  blended pixels, and print numbers.
- Peak memory is always `/proc/self/status`'s `VmHWM`; time is `CLOCK_MONOTONIC` wall clock, with
  compositing and encoding reported separately.
- Measuring encoder performance requires **non-flat** content: flat color blocks skew A0 PNG size
  and time by 78× and 4.6× respectively.
- Every threshold constant in the code carries its **source inline** — the measured value and its date —
  or cites `docs/CONTRACT.md`. **`docs/steps/`, `docs/archive/` and
  `docs/completed/` are the process record and are never cited from code**: they are scheduled for
  deletion or archival once their work is done, so a comment that points at them is a comment that stops
  resolving on the day that happens. **S-numbers are the exception and stay citable** — they are how the
  code says which step built a thing ("since S6 the encoder is …"), and the numbering continues across
  plans rather than restarting.

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

- **Commit once per completed step**: finishing one step — its file under `docs/steps/`, renamed to
  `…-done.md` — produces at least one commit. Do not batch several steps into one commit.
- **Pushing requires the user's explicit permission first.** Without it, commit only and never
  push: do not `git push` on your own initiative and do not change remote configuration.
- **End every commit message's first line with `🤖`.** The criterion is whether the change
  **contains AI-generated or AI-modified content** — anything touched by AI carries it, and at this
  stage AI writes all the code, so in practice every commit carries it. It sits at the end of the
  title line (e.g. `S12d: Pixels only 🤖`) rather than on its own line, so `git log --oneline`
  shows it without hiding any message.
- **Messages are English** (see "Language conventions"). First line format:
  `<step>: <what changed>`, e.g. `S2: Freeze template geometry and invariant tests`. Non-step
  changes (docs, CI) use the `docs:` / `chore:` prefix.
- **Run the entry's commands that this change can affect before committing**: a change to code or to
  shipped data runs all four of them; a change the product cannot see — docs, `.gitignore`, the
  repository layout — runs the one command that reads what changed, and a step's own verification
  command is that command when the step names one. If it is red, do not commit.
- Never committed: `target/` (see `.gitignore`). Committed: `Cargo.lock` (AUR discipline).
- **"Done" means the step's `**Progress**` line is rewritten and committed** (see "Session and persistence
  discipline"), not that the code is written and the tests are green.

## Step discipline

How the work splits into steps, what a step's own file records, and the cases in which a step has to end a
session. The steps themselves are one file each under `docs/steps/`.

**One file per step** (ruled 2026-09-27, human): `docs/steps/<S-number>-<slug>-<status>.md` — the slug a
few words, the **status last** and one of `todo`, `doing`, `blocked`, `done`, so a directory listing is
the plan — carrying the step's goal, work, machine-checkable exit, `Human` line, rulings, `Result` and the
one `**Progress**` line that says where it stands. A status change renames the file
(`git mv S32-…-doing.md S32-…-done.md`) in the same commit as that line. The directory is **`steps`** and
not `plans` because a plan is a set of steps and this is the unit that has a file, and not `phases`
because nothing here is one; the S-numbering continues across plans.

**A one-off task is not a step** (ruled 2026-09-27, human): a small change nobody asked to be a step — a CI
fix, a document edit, a dependency bump, a bug fixed on the spot — is done and committed and creates no
file under `docs/steps/`, because the directory holds the plan and a file per errand turns the plan into a
log. The commit message's own prefix (`docs:` / `chore:`, "Commit discipline") is what marks one. A step
file is written when the human names the work a step, or when the work needs the one thing only a step
file carries: a machine-checkable exit of its own, a `Human` gate, or a ruling with a `Result` to record.

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
   *Precondition: a boundary holds only if the **conclusion is already on disk** (the threshold constants in the tests + the measured numbers in `docs/CONTRACT.md` + the step's
   `**Progress**` line).
   A conclusion that is not on disk means switching session equals measuring it again.*
   By this rule the natural boundaries are `S0 ┊ S1 ┊ S2+S3 ┊ S4 ┊ S5+S6 ┊ S7 ┊ S8` (six sessions, not nine).
   *That line belongs to the retired plan of 2026-09-20 and is kept as the example that produced the rule;
   the plan of 2026-09-22 states its own boundaries in its own file.*

## Session and persistence discipline

**A conversation is not storage.** Sessions get truncated, cleared or deleted; a conclusion that
exists only in the conversation never happened. A new session reads files, not someone else's
transcript, and the `**Progress**` line in that step's own file under `docs/steps/` is the **only authority**.

- **A step is complete when its file's `**Progress**` line is rewritten — the file renamed to
  `…-done.md` — and committed.** Green tests and good numbers are necessary, not sufficient.
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
  1. the ruling block written into that step's own `Result` subsection (`docs/steps/…`);
  2. its `**Progress**` line rewritten to "done and passed \_\_\_ → next X";
  3. the step's `Human` line marked as passed (a walk is the matching item of `docs/HIG-REVIEW.md` §2);
  4. any shape the ruling changed synchronized into `docs/CONTRACT.md`;
  5. commit.
- **Writing it down is not a prerequisite of the next step; it is the other half of this ruling.**
  After a ruling, do not continue into the next step in the same session: a gate's next action is a
  new session (`AGENTS.md`, "Step discipline", principle 5).
- **Never reconstruct a record from memory.** If a conclusion rests on a number or ruling that only
  ever appeared in conversation and is not in the files, re-measure it or ask the human;
  reconstructing it from memory is exactly the failure "Step discipline" principle 5 guards against
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
  (currently gtk4 4.24, cairo 1.18.4, libadwaita 1.10), and the bindings take the latest.
  Downgrade a binding only when it demands a newer system version than Arch ships — never downgrade
  the system.
- **CI / packaging**: `runs-on: ubuntu-26.04` — the newest hosted Ubuntu image, named explicitly because
  `ubuntu-latest` still resolves to 24.04 — with **no container** and **no GUI suite** (both ruled
  2026-09-27, human): the suite needs a display, a compositor and the libraries the product targets, and a
  runner could not give it all three, so CI runs the entry's commands minus that suite and every failure is
  recorded in `docs/steps/S33-ci-skips-the-gui-suite-done.md`. CI also makes the **complete build** with the
  project's own definition (`meson setup` / `meson compile` / `meson install` into a staging root, the same
  steps `makepkg` wraps), because a tree that only ever compiles as a side effect of `cargo test` is a tree
  whose meson files first run at a release. So CI answers "does the windowless half run, and does the tree
  build and install" — the GUI suite and the Arch package are the machine's, through `cargo test` and
  `makepkg` (below). The shell's crate contributes the one test a runner can run — `tests/packaging.rs`,
  text files only — so the identity and the version chain are checked here too.
  The one action moves with its major tag (`actions/checkout@v7`) rather than being pinned to a commit SHA:
  under this policy a pin is the thing that has to be justified
- **Keeping the dependency set minimal** does not conflict with tracking the latest: few, but each
  one current.

This policy **does not go into tests**: tests must not depend on the network or on a toolchain
version. It is maintained by three things — the `cargo update` at the start of each step, looking
at `Cargo.lock` diffs during review, and **Dependabot** (`.github/dependabot.yml`, 2026-09-27):
`cargo` and `github-actions` updates checked **daily**, with minor and patch updates grouped into one
pull request and every major one on its own, because a major is the case the code has to move with. It
neither pins nor delays — no `ignore`, no `cooldown`, no lockfile-only strategy — and every one of its
pull requests runs the same `verify` job as a hand-made commit. Its commits are the bot's
(`chore: …`) and carry no `🤖`: nothing in them is written by AI, and a session that has to change
code for one commits that change itself.

## Hard constraints

- **Geometry uses normalized coordinates `[0,1]` only.** Absolute pixels are allowed only at the
  rendering boundary, never inside `CollageDoc` or a template.
  *Rationale: layout units and device pixels were once mixed, and two consecutive rounds drew wrong
  conclusions from it.*
- **Preview and export must call the same `render::draw(doc, images, target)`.** A second renderer is
  forbidden.
  *Rationale: the product is the exported image, and two renderers necessarily diverge — measured:
  two engines measuring the same string identically and still differing by 14.6% of pixels.*
- **All resampling belongs upstream; the canvas only blits and clips.** Decoding, downsampling,
  rotation interpolation and the colour conversion all happen in `pixlay-imaging`; Cairo receives bitmaps that are
  already the right size. *Rationale: this keeps Cairo's weak filtering (bilinear + mipmap only)
  out of the finished product.*
- **Encoding and metadata happen in one pass.** Colour sampling and ICC must live in the
  same pipeline; "encode first, patch the metadata afterwards" is forbidden.
  *Rationale: the second pass re-encodes with default parameters and silently drops 4:4:4 to 4:2:0
  (measured 2.71 MB → 1.49 MB).*
  Implementation meaning: **Cairo supplies pixels only; the encoder writes the metadata itself.**
  Measured: `cairo_surface_write_to_png` on A0 emits only IHDR/bKGD/IDAT and no iCCP, so an export
  written through it loses its colour space. Writing PNG through Cairo is forbidden for the same
  reason as a second pass.
  Since S6 the encoder is `pixlay_imaging::encode`: PNG `iCCP`, JPEG `APP2` ICC + the `SOF0`
  sampling factors, which are 4:4:4 since S12c — each written while the pixels go out, never by
  a second pass over the finished file. The third format, TIFF, the `--chroma` request and every
  resolution (PNG `pHYs`, the JFIF density, the `--dpi` flag, the `canvas` field) left with the
  same two rulings. The ICC rule and the per-format field list are in `docs/CONTRACT.md` §5.
- **The evaluation order is frozen** and must not be reordered:
  `decode + color normalization → geometry (crop / arbitrary rotation) → per-slot
  compositing → canvas decoration → output transform`
  *Rationale: operations that change the coordinate system must run first, and content layers
  positioned relative to the canvas must run last. Counterexample: draw the frame's gaps and then
  composite the photos — the photos would paint straight over the gaps.*
  *Ruled 2026-09-22: flip and quarter turns left the product, so the geometry stage is
  `crop → arbitrary rotation`; and "canvas decoration" is what the frame (gap / corner radius /
  colour) is drawn in — after the slots, and it is the last stage (`docs/CONTRACT.md` §4).*
- **Composite onto an opaque backdrop, white by default.** Source alpha is always flattened, so an
  export is never transparent; whatever a photo does not cover — the frame's gaps, a rounded corner,
  an empty slot — shows the document's own `frame.color`, which defaults to white.
  *Rationale: the product is a photo collage, preview and export must be pixel-identical, and a
  transparent export would open a second encoding branch. Ruled 2026-09-22: the backdrop colour became
  a document field so a frame's border could be coloured; "white by default" is what keeps every
  project written before that field byte-identical, and opacity stays absolute.*
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
- **Rotation crops edges only; it never grows the canvas.** After a rotation angle, a frame or a slot
  geometry change the clamp **must be recomputed** so the visible cell stays filled. **The angle is
  free**: it has no cap, and the clamp never reduces it — the zoom is raised to whatever covering that
  exact angle needs (ruled 2026-09-22, which retired S3's ±45° cap and with it the
  `CLAMP_ZOOM_LIMIT` angle-reduction rule and `CropFit::rotation_limited`; S11 removed all three and
  measured what replaced them: over the whole library, every whole degree and six photo aspects the
  worst covering zoom is **21.7×**, 46× below the `MAX_ZOOM` bound).
  *Rationale: collage slot sizes are fixed by the template, and this cuts the most troublesome
  branch of a general-purpose editor: growing the canvas. The cap existed to keep the zoom modest,
  not because coverage was impossible: the required covering zoom is bounded for every slot shape
  (its worst case is a diagonal), and a slot's bitmap is sized by the slot rather than by the zoom —
  measured (S11): every cell at the worst angle in an A0 ten-slot strip costs +22% of peak RSS
  (1189 → 1451 MB), and that is the *rotation's* own cost — a rotated cell's bitmap is the
  axis-aligned box of the rotated cell — not the zoom's, which resamples a smaller source region
  into the same output.*
- **The canvas frame is a document field, and the backdrop is painted, not blended.** `frame{gapRel,
  radiusRel, color}` defaults to no gap, square corners and white, so a project written before S11 renders
  byte-identically (measured: the S1 golden image at RMSE 0.0 and the S5 verification render byte-identical —
  `docs/CONTRACT.md` §8). **The gap is the distance between two photos** (ruled 2026-09-25, ruling 35):
  the visible area of a cell is the cell's bounding box eroded by `gapRel/2`, intersected with the sheet
  eroded by `gapRel` — two neighbours each give up half, so the seam between them measures `gapRel`, and
  the sheet's own edge gives up the whole gap because outside the sheet there is no photo to give the
  second half, so the outermost photos stand `gapRel` from the border too (`docs/CONTRACT.md` §1, and §8
  "S20" for the measurement). A *concave* slot's interior edge is the template's own geometry and keeps
  its place, so the stripe beside it is the neighbour's half alone. The clamp's reference is the outline
  clipped to that eroded region — a rectangle, so for the rectangular slots the library is made of the
  gap crops the photo rather than magnifying it — and a rounded corner is *not*
  subtracted from it — the reference stays a polygon rather than approximating arcs, which costs a little
  magnification bounded by the radius and exactly nothing at `radiusRel = 0`.
- **GTK types do not implement `Send`/`Sync`.** Background decoding and scaling must return to the
  main thread through a channel; a GTK object must never be held across threads.
- **The shell (`pixlay`) must not touch pixels directly.**

## GNOME HIG (constrains the `pixlay` shell only)

Spec: https://developer.gnome.org/hig/ — the platform definition is GTK4 + libadwaita, consistent
with "Module boundaries". **It applies to the GUI layer only**: `pixlay-core` / `-imaging` /
`-render` / `-cli` have no interface, are unaffected, and must not pull in GTK for it.
HIG has no version number and is **not frozen**; cite URLs and section names, and at the start of
every step that touches UI re-read them and update `docs/HIG-REVIEW.md`.

- **Widgets**: use libadwaita containers and widgets by default (`AdwApplicationWindow` /
  `AdwToolbarView` / `AdwHeaderBar` / `AdwToast` / `AdwStatusPage` / `AdwAboutDialog` and so on — and, since
  S25, `AdwPreferencesDialog` for the app's own dialog, which S25b made one surface: the document's frame
  above the export's settings, rulings 36 and the human's of 2026-09-26). S7
  landed the shell as `AdwApplicationWindow` + `AdwToolbarView` + `AdwHeaderBar` + `AdwToastOverlay` +
  `AdwBanner`, with one custom-drawn widget — the canvas, whose stated reason is that it draws the
  document itself. A custom-drawn widget is the exception and needs a stated reason. **S13 replaced the
  utility pane** (`AdwOverlaySplitView` + `AdwPreferencesPage` + `F9`, ruling 18) with a sequence of
  stages; **ruling 31 of 2026-09-25 removed the first of them and S22 landed it**: the window opens on the
  editor — one page, nothing to push, and `AdwNavigationView` went with the stage it held — because the
  picker judged nothing the file manager cannot and cost a stage to do it. Its ordered list, its preview
  and its zoom, the folder scan and the tile cache went with it; a photo's order is the order it was
  added, and re-ordering is the swap of S23 (`docs/completed/2026-09-25-STEPS.md`). **The layout band draws sketches**: each candidate is the template's
  geometry drawn in ink — its cells in paper, every cell's outline and the sheet's ground no cell covers in
  ink, so a layout whose cells leave a gutter shows it as a gap (S29) — rather than a render of the user's
  photos (ruling 32), because a template carries geometry and no style, so a sketch is a complete account of
  it — and the band decodes nothing.
  **The window's one header bar follows HIG
  `patterns/containers/header-bars` and the two references**: primary actions at the *start* — **the way in
  first**, `Add photos…` as the leftmost control ahead of undo and redo (S26, ruling 42 of 2026-09-26: the
  window opens on an empty cell, so the control that fills it is what has to read as "start here") — the
  heading (the document's name) in the centre, a primary menu at the *end* — and no Save button, which
  ruling 37 of 2026-09-25 removed because it sat beside Export. The editor's
  per-cell buttons arrived in S15 as children of the canvas's own `GtkOverlay`, placed by their own
  margins — a `GtkFixed` was rejected because it measures only its children, so a document whose empty
  cells come and go would leave the container 0x0 (`crates/pixlay/src/canvas.rs`). None of those is
  custom-drawn, so the shell keeps exactly one.
- **Styling**: use only libadwaita style classes and CSS variables; hard-coded colors and spacing
  are forbidden (they break dark mode and high contrast). **The app is dark by default** — ruled
  2026-09-22, superseding "never force light or dark": HIG `guidelines/ui-styling` recommends the dark style
  by default for "apps which display rich visual content like images or video", and both reference apps do
  exactly that (gthumb `Adw.ColorScheme.FORCE_DARK`, `Application.vala:676`; loupe `PreferDark`,
  `application.rs:76-79`). v1 ships **no per-app style switch** (neither reference app has one), and the
  canvas and the export stay style-independent because they are document content, not styling. The canvas and the export are **always opaque and independent of the UI theme** — the
  backdrop is document content (white unless the document's own frame says otherwise), not styling
  (see "Hard constraints").
- **Icons and the app's own files** (S16): the app icon and its symbolic variant are drawn on the HIG's
  own grids (`guidelines/app-icons`, `guidelines/ui-icons`, `reference/palette` — the app-id is their file
  name, and the desktop file's `Icon=` is the same string), and the desktop entry, the AppStream metainfo
  and the `.pixlay` MIME registration live as templates under `data/`, generated with `msgfmt` at build
  time. `crates/pixlay/tests/packaging.rs` holds the names, the version and the license to
  `pixlay::APP_ID` and the manifests; what only the tools can judge is the install's own `meson test`
  (`desktop-file-validate`, `appstreamcli validate`).
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
  - no GNOME Shell search provider, no notification workflow — the same discipline as the "not doing"
    list;
  - no per-app style preference (light / dark / system) — amended 2026-09-22: the app is **dark by
    default**, as HIG recommends for one that displays rich visual content, and neither reference app offers
    the switch. The settings file ruling 39 of 2026-09-25 allows carries the export's own settings and the
    last export folder, not this one;

  - **the phone's chrome, not its capability** (ruled 2026-09-22, superseded 2026-09-25): the
    picker-first flow came from mobile galleries and was removed with them (ruling 31). What the mobile
    references still give the product is the layout strip's own form: **a sketch of the geometry rather
    than a sample image**, the way Xiaomi's layout strip draws it — because a pixlay template carries
    geometry and no style. Google's strip shows sample photos for the opposite reason: its templates
    carry style, which a sketch could not show (ruling 32; the research is in
    `docs/completed/2026-09-25-STEPS.md`).
- HIG `patterns/containers/selection-mode` **is not applicable again** (ruled 2026-09-25, ruling 31): the
  picker was this app's only multi-select collection view and it is gone. The layout band is a
  single-choice set (HIG `patterns/controls/radio-buttons`), and the canvas keeps no mode of its own —
  the page's own advice, "when editing is the primary interaction there should be no separate edit
  mode", is what it has always followed.
- Whatever can be machine-checked lives only in the GUI's tests (see the step's own file under `docs/steps/`); the visual
  part goes item by item through `docs/HIG-REVIEW.md`.

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
- **Do not add parallel modes over one document** (edit mode / collage mode). The user's mental model
  is always "I am making a collage"; editing is a property of the currently selected slot.
  *Ruled 2026-09-22, replacing the flat "do not add mode switching": what is forbidden is two
  **parallel** modes over one document. The **sequential** creation flow the product now has (pick
  photos → pick a layout → compose) is a multi-step task, which HIG itself shapes as a navigable
  sequence (`AdwNavigationView` push/pop with Back) rather than as two modes to know about.*
- **Not doing**: beauty retouching, levels / curves, online geocoding, RAW, brush marking, a
  single-image retouch mode, **TIFF output and a JPEG chroma request** (S12c: two formats, one
  sampling), **physical sizes and resolutions in any form** (S12d: an export is one long-edge
  pixel count, and the file carries no resolution), **and flipping or mirroring a cell in any
  form** — ruled 2026-09-22: the
  per-cell capabilities are zoom, move and rotation by any angle.
  *Rationale: levels / curves is a professional control that needs a full ICC pipeline and is opaque
  to the target user; geocoding carries API quotas, identity requirements and privacy costs, and a
  hand-typed place name replaces it; the rest is unrelated to the core value of a collage.*
- **Linux is the only platform** — ruled 2026-09-27 (human): macOS, or any second desktop platform, is
  not a target, and a release publishes Linux binaries only. The basis, stated the same day: the
  decoding backend's sandboxed loader is `cfg(target_os = "linux")` and the in-process one covers no
  HEIC and no AVIF (S4's measurement); `meson.build` requires `libseccomp`, which is the Linux one; and
  a macOS application is an `.app` bundle with a signature this product has no story for, while the
  shell follows the target's own GTK and libadwaita.

## Module boundaries

    pixlay-core     CollageDoc, templates, geometry, framing transforms, command history, the selection policy. Must not depend on gtk / cairo
    pixlay-imaging  decoding (glycin), resampling, EXIF, color spaces, preview thumbnails, encoding (PNG/JPEG). Must not depend on gtk or cairo
    pixlay-render   the single draw(doc, images, target), on Cairo. Must not depend on gtk
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
(subcommands, fields, exit codes) live in the tests and the step's own file under `docs/steps/` and are
not repeated here.*

## Invariants that must hold

- templates: zero overlap between slots; no interior hole in their union; a cut template's areas sum
  to exactly 1.0
- framing: for any (rotation, zoom, offset) combination, the clamped photo covers the entire visible
  cell — the slot, narrowed by the document's frame (`docs/CONTRACT.md` §1)
- render consistency: the same composition at `2N` and `N`, downsampled, stays below the RMSE
  threshold (measured 2.62/255; threshold 6)
- encoding: every export carries its ICC profile in the same pass as its pixels, and a JPEG is 4:4:4 in its own `SOF0`

**Every rule a test can enforce lives only in the tests; this file does not restate it.**

## Open / to be proven

- A0@300dpi on Cairo (9933×14043, 139.5 MP) has been **re-measured and passes** (S0, 2026-09-20; the
  formal criterion is the in-repo probe, not a one-off script): 185 ms for 2 slots and 551 ms for
  10, peak `VmHWM` 941 MB compositing and 1340 MB including encoding (budget 2.5 GB), PNG and JPEG
  both emit 9933×14043, and the white-base and seam criteria are all green. Numbers in
  `docs/completed/2026-09-20-STEPS-done.md` under "S0 result".
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
  `docs/completed/2026-09-20-STEPS-done.md` "S4 · decisions" 1 and `docs/CONTRACT.md` §4.1.
- The conservative clamp for irregular slots — computed from a circumscribed axis-aligned rectangle,
  allowing slight white slivers — is **not needed and not used**: S3's clamp tests the outline's own
  vertices, which is exact for a concave slot too (a rectangle contains a polygon iff it contains its
  vertices), so no sliver is allowed and none is measured (28,800 framings against a 1e-6 tolerance,
  the worst sample 2.2e-16 past the photo's edge).
- **Framing clamp (S3, 2026-09-21): superseded and replaced on 2026-09-22 (S11).** S3 ruled that past
  `CLAMP_ZOOM_LIMIT = 1.5` times the *upright covering zoom*, the clamp reduces the requested rotation
  angle to the widest one that fits instead of magnifying further (measured then: a ten-column strip
  needs 6.0× with a 4:3 photo, and a matched 4:3 slot kept 27.3° of the 45° asked for).
  **Ruled 2026-09-22: the angle is free and is never reduced**, and S11 removed `CLAMP_ZOOM_LIMIT`,
  `CropFit::rotation_limited` and `widest_rotation`, re-ran the coverage sweep with no cap, and measured the
  covering zoom as a function of the angle for every slot shape: the worst is **21.7×** (a 2.4:1 photo in a
  1/16-wide pane at 6°; 12.07× for the same pane with a 4:3 photo), against `MAX_ZOOM` = 1000. The pan-clamp
  decision stands. The per-aspect angle table is gone with the cap, because there are no kept angles left to
  table; the numbers above stay as the S3 record.
- Seam behavior re-measured (2026-09-20): blended pixels on shared edges / seam length ≈ 1.08,
  **identical** at A0 and at 1/5 size → the blend width is one physical pixel and independent of
  output resolution, with no strong bleeding. **Ruled 2026-09-22 (human): the seam is accepted** — the
  product is the exported image, and at 300 dpi one pixel is 0.085 mm and invisible; a preview an eighth
  of that size shows the same pixel proportionally larger, and it is the same renderer's
  (`docs/archive/2026-09-22-STEPS.md`, `S13 · Ruling`).

## AUR discipline

- Commit `Cargo.lock`; keep dependencies minimal; `cargo vendor` must pass
- **The build and the install are the project's own** (S31): `meson.build` runs cargo over the workspace
  (`--locked`, offline against the vendored registry) and installs everything the desktop reads — both
  binaries, the desktop entry and the metainfo generated from their templates with `msgfmt`, the two icons,
  the `.pixlay` MIME registration and the catalogs `po/LINGUAS` lists (`docs/CONTRACT.md` §10). GNOME's
  applications are built this way and this is one, so a distribution other than Arch installs it with
  `meson setup build && meson compile -C build && meson install -C build` and nothing else
- The PKGBUILD (`packaging/arch/PKGBUILD`) wraps that install and adds what is Arch's: `source=` is the
  release tag's tarball built from `pkgver` (a release pushes `vX.Y.Z`, fills `sha256sums` with
  `updpkgsums` and writes `.SRCINFO`), the registry is vendored (`CARGO_NET_OFFLINE=true` for the cargo
  meson starts), `depends=('gtk4' 'libadwaita' 'glycin')` (the decoding backend of "Open / to be proven"
  is a linked library, so it is a runtime dependency), `makedepends` names `cargo`, `rust`, `meson` and
  `gettext`, and the license goes to `/usr/share/licenses/$pkgname/` — Arch's path, not the prefix's
- **A release is a tag plus four steps** (ruled 2026-09-27, human). **The release's notes are
  `CHANGELOG.md`'s section for that version, written before the tag is pushed** —
  `crates/pixlay/tests/packaging.rs` holds that section's version to `Cargo.toml`'s, so a bump without
  one fails the suite. Push `vX.Y.Z` — the tag has to equal
  `meson.build`'s `project(version:)`, which the workflow checks — and `release.yml` builds the tree with
  the project's own build and attaches the **Linux binaries** (`pixlay-<version>-linux-amd64.tar.gz` and
  `pixlay-<version>-linux-arm64.tar.gz`, each with its `.sha256`) to that tag's GitHub Release. Then, on
  the machine: **update `pkgver` and `sha256sums`**
  (`updpkgsums`, so the sums stop being `SKIP` — and when the tag was re-pointed, delete the cached
  `packaging/arch/*.tar.gz` first: `updpkgsums` reads the file already in `SRCDEST` and would print the
  old tag's sum, measured 2026-09-27); **build the package** (`makepkg` in `packaging/arch`, which
  leaves `x86_64.pkg.tar.zst` beside the PKGBUILD); **upload it to the same release** (`gh release upload
  <tag> --clobber …`), so the release carries the binaries and the package together; and **delete the
  previous version's files** — `packaging/arch/{src,pkg}`, its `.pkg.tar.zst`, the `*.tar.gz` `makepkg`
  downloaded — so that directory holds the current version only. `arch=('x86_64' 'aarch64')` is the
  statement that the tree builds under Arch Linux ARM too, and that half of the **package** is built
  there, because no GitHub runner has an aarch64 Arch userland; the arm64 **binaries** come from the arm64
  runner (`ubuntu-26.04-arm`), beside the amd64 ones. The AUR upload stays a human step: the PKGBUILD,
  `.SRCINFO` and nothing else
- **No tests run in a package build** (ruled 2026-09-26, human): `makepkg`'s standard is that it builds and
  packages, the suite is the verification entry's (and CI's), and the two artifact validators
  (`desktop-file-validate`, `appstreamcli validate --no-net`) run in `meson test` wherever they are
  installed (`data/meson.build`, `required: false`)
- **CI runs the verification entry and does not build the package** (same ruling): `makepkg`, the PKGBUILD
  and the install are verified on a machine, where a package can be built *and installed*; the runner's
  job is to answer whether the program runs. `release.yml` builds the **binaries** for a tag and attaches
  them to the release — its build is the same `meson setup` / `meson compile` the package wraps, and it runs
  no `makepkg` either
- SPDX is `GPL-3.0-or-later` throughout (not `-only`)

## Dependency registry

Register every new dependency here: **name / version / why / size and impact**. The criteria are
"how much code would deleting it cost" and whether it weakens a hard constraint. Policy in "Version
policy: track the latest": latest stable only, no upper pin.

| Dependency | Used by | Why | Notes |
|---|---|---|---|
| `serde` + `serde_derive` 1.0.229 | `pixlay-core`, `pixlay` | `.pixlay` is JSON and every `CollageDoc` field has to round-trip; hand-written serialization means reimplementing format validation. `pixlay` derives the same way for the app's own settings file (S25, ruling 39: three fields) — one dependency for both files rather than a second, hand-written reader | Small, no system dependencies |
| `serde_json` 1.0.151 | `pixlay-core`, `pixlay`, `pixlay-cli` (dev) | JSON read/write; `deny_unknown_fields` turns "misspelled field" into a load-time error. The settings file goes through the same crate (S25) and deliberately does **not** deny unknown fields: a settings file is not a document, and refusing one over a key a later version added would silently reset the settings that are known | Same |
| `thiserror` 2.0.20 | `pixlay-core`, `pixlay-render` | core/render errors are typed errors (part of the contract); `anyhow` is allowed only in `pixlay-cli` | Pure macro, zero runtime |
| `cairo-rs` 0.22.9 | `pixlay-render` | The only rendering backend; GTK4 already depends on cairo, so packaging is free | System cairo 1.18.4; the `png` feature is dev-only (golden image read/write) |
| `png` 0.18.1 | `pixlay-imaging` | The PNG writer of the one-pass encoder (S6). `image`'s PNG writer cannot embed an ICC profile in the same pass as the pixels, and Cairo's emits no `iCCP` at all — and an sRGB file whose numbers are not labelled is a file whose colour depends on who opens it | Pure Rust; it was already in the tree through `image`, so the download set did not grow |
| `jpeg-encoder` 0.7.1 | `pixlay-imaging` | The JPEG writer of the one-pass encoder (S6): `set_sampling_factor` (4:4:4 / 4:2:2 / 4:2:0) and `add_icc_profile` (`APP2`), which is exactly the "pixels + sampling + ICC in one pass" the constraint names (the JFIF density stays at the encoder's resolution-free default since S12d) | Pure Rust; already in the tree through `glycin-image-rs`. Measured against the previous writer (`image` = zune-jpeg): +1.1% bytes, −27% time on the S6 grid (14043 px, q90, 4:4:4) |
| `image` 0.25.10 | `pixlay-cli` (**dev only** since S6) | It was S1's encoder stand-in and S6 replaced it (`pixlay_imaging::encode` writes the ICC profile and the JPEG sampling factors that this crate's writers leave at their defaults; the resolutions left with S12d). What it is still for: the CLI's **tests** read renders back with `image::open` (PNG/JPEG) and write flat photos to render against, and `pixlay-cli/tests/fixtures/generate.py` produced the fixtures | Not a production dependency any more, so the shipped binary no longer links it |
| `glycin` 4.0.0 | `pixlay-imaging` | The decoding backend, measured against the in-process alternative (S4): the sandboxed loader is the only one of the two that decodes HEIC and AVIF, and it works with an empty environment | Pulls `glib`/`gio` and, through `cfg(target_os = "linux")`, `libseccomp` / `bubblewrap` / `fontconfig` / the distro's loader packages — this is what S8's `depends` must name |
| `glib` 0.22 / `gio` 0.22 | `pixlay-imaging` | The decode is driven on a private `MainContext`: a glycin frame request only completes while one is iterated (measured: every frame hung under a plain executor until glycin's own 60 s limit). `glib`'s `futures` feature provides `MainContext::block_on`; `gio::File` is glycin's own input type | Already in the tree with `glycin`; named here because the API is used directly |
|`gtk4` 0.11.5 + `libadwaita` 0.9.2|`pixlay`|The shell: the window, the header bar and its menu, the canvas's controls and the dialogs. `v4_12` is the level the window needs: `GtkCssProvider::load_from_string` (the app's one stylesheet, whose remaining rules are the layout band's) and `GdkSurface::layout` — GTK4's only "the window was resized" signal, which is what the canvas's own decode grid follows, so below it the build would compile and never resize the preview. Below that, `v4_10` carries `GtkFileDialog` and `GtkColorDialogButton` (4.10 dropped the deprecated chooser dialogs) and libadwaita's `v1_8` carries `AdwDialog` / `AdwToastOverlay` / `AdwShortcutsDialog`|System gtk4 4.24 / libadwaita 1.10 through pkg-config; GTK already depends on cairo, pango and gdk-pixbuf, so the download set grows by the bindings alone. Linked by `pixlay` only — the other four crates must not name it|
|`gettext-rs` 0.8.0 (`gettext-system`)|`pixlay`|i18n, as the plan of 2026-09-20 decided before S7 (`docs/archive/2026-09-20-STEPS.md`): the same gettext toolchain GTK and libadwaita use for their own copy, so `.po`, the `.desktop` file and AppStream metainfo (S16) all go through one pipeline. `po/POTFILES` and `po/pixlay.pot` are committed|Tiny; `gettext-sys` links the system `libintl` rather than building a private copy. Only `pixlay` depends on it, which is what the language conventions require|

`pangocairo` was a temporary S0 spike dependency, came back in S5 for the canvas text layers, and left
again with them in S12c (along with the pinned test font and the `FONTCONFIG_FILE` machinery the tests
used); the spike's own use of `cairo-rs/png` went with the spike.
`gtk4` + `libadwaita` + `gettext-rs` were added by S7, the first step that has a window at all.
