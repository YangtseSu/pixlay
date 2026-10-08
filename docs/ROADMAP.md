<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# Pixlay roadmap

Future directions that are **not** steps.

[`docs/steps/`](steps/) holds the plan: one file per step — `<S-number>-<slug>-<status>.md`, the status in
the file's own name — with a machine-checkable exit for each. This file holds what a person has said they
want to build later, together with the research already done, so a later session does not repeat it.
**Nothing here is an instruction and nothing here is a promise**: when a direction is ruled in it becomes a
step in the plan and moves out of this file.

Provenance is marked per item: *asked for* means the human named the direction, *proposed* means it is
the assistant's suggestion, recorded for a decision nobody has made yet.

## More layouts in the library

**Status: planned (2026-10-08)** — *asked for* (2026-09-27, extended 2026-10-08): the shipped library
should offer more layouts, and the references the human named organize theirs by category rather than as a
flat list. The plan's first three steps are **S35, S36 and S37** under [`docs/steps/`](steps/); this
section holds the research they rest on and the directions they leave alone.

- **What it holds today** (`pixlay-render templates`, 2026-10-08): **27 templates, 143 slots** over cell
  counts 1 … 9 — one layout at count 1 (`grid-1-1x1`, which is `DEFAULT_TEMPLATE`), then three, three,
  four, three, four, three, three and three — and over five aspects: **4:3** (8), **16:9** (7), **1:1**
  (5), **3:2** (4), **2:3** (3). A two-to-four-photo collage, which is what the main path mostly is,
  picks from three or four candidates, and a one-photo document has nothing to choose.
- **The measured defect the plan starts from** (2026-10-08, `pixlay-render edit --add-cell`): the window
  opens on `grid-1-1x1` (4:3), **count 2 is the only count with no 4:3 layout**, and so the first added
  photo takes the sheet to 3:2 (`strip-2-2x1`) and the second to 16:9 (`strip-3-3x1`), where the rest of
  the growth stays. The 4:3 chain already exists at 1 and 3 … 9 and every growth lands on it — the aspect
  rank is `layout_for`'s first preference — so one 4:3 two-cell member is what makes the sheet keep its
  shape from photo one to nine. Portrait coverage is counts 2, 3 and 6 only; count 2 carries no 16:9;
  1:1 is missing at 3, 5, 6 and 8.
- **How the two references organize their templates** (*asked for*, 2026-10-08 — the human named both).
  - **Google Photos** is reorganizing a collage's flat template list into at least nine named categories
    — Featured, Grid, Film, Classic, Love, Celebration, Floral, Decoration, Shapes — with border editing
    alongside, the stated reason being that a growing list stops being navigable (an APK teardown of
    unreleased code, reported 2026: development direction, not a shipped feature). Read for this product:
    **Grid and Classic are arrangements, and Film, Love, Celebration, Floral, Decoration and Shapes are
    style** — artwork and frames a pixlay template does not carry, because a template is geometry and no
    style (ruling 32) and the product is only a collage (2026-09-22). What survives is the *navigation*
    insight — and the news that **border editing is in development there and shipped here**: the frame
    (gap, corner radius, colour) has been a document field since S11.
  - **Xiaomi Gallery** splits its 拼图 mode into three: **布局** (plain arrangements, further split by
    canvas ratio — 1:1 and 3:4), **海报** (one photo dominant, the rest small) and **拼接** (photos joined
    whole, edge to edge). This taxonomy is *structural*, and two of its three modes are already pixlay's
    vocabulary: 布局 is the strip / grid / mosaic arrangements with **the canvas ratio as the first
    filter**, and 海报 is the `mosaic-*-hero` family. 拼接 is a **behaviour** rather than a layout — it
    needs fit-not-cover and a canvas derived from the inputs, which the frozen-aspect model does not
    express — and it is a direction of its own below.
  - **Why Xiaomi's model is the one that maps**: Google keeps one flat list mixing ratios because its
    templates differ mostly in interior style, while a pixlay template's ratio *is* the sheet's shape (a
    template field, the document's canvas). So the reference for the layout stage is "ratio first, then
    that ratio's arrangements", and Google's named categories are not adopted: there is nothing in them a
    sketch could show.
- **Adding one is cheap by construction**, which is what makes the plan three data-first steps: a template
  is a `Recipe` in `pixlay-core/src/templates/generator.rs` (an integer lattice),
  `frozen.rs` is regenerated from it and byte-compared by `crates/pixlay-core/tests/templates.rs`, the band
  draws **sketches** rather than artwork (`CANDIDATE_BOX` is 128x96), and the CLI's `templates` and
  `render --template` expose a new one the day it lands. No asset, no translation, no per-template code.
  The template *creator* is the generator's CLI half, which the "nothing only in the GUI" rule wants
  anyway.
- **What an addition must not do, and this is the whole cost**: the geometry is frozen data under a
  `templateVersion` and a document embeds a copy of its own, so an addition is an **addition** — an
  existing name's geometry may never move, because that would move an old project's pixels. Every addition
  also carries the invariants that are already tests (zero overlap, no interior hole, cell areas summing to
  exactly 1.0, simple outlines) and has to stay legible as a sketch at the band's own 128x96 (S21, S29 and
  S30 are the record of how narrow that is: the ink's tone and the gutter's very existence were each
  findings there).
- **The plan, in order** (each a step under [`docs/steps/`](steps/), written 2026-10-08, status `todo`):
  1. **Ratio coverage at every count** (S35) — data only. Every count 2 … 9 carries at least four layouts
     over at least four aspect ratios, including **4:3**, **1:1** and a portrait member; **3:4 joins as
     the sixth ratio** (Xiaomi's page ratio, the portrait of the default 4:3 sheet) with a small number
     of members; and the default growth chain keeps 4:3. This is the measured defect above, and it goes
     first because it is the cheapest and because everything else is worth more once the ratios exist.
  2. **The ratio becomes a control** (S36) — the band gains a **ratio row** ahead of the strip, the
     document's own ratio marked; choosing one re-lays the document through the same preference the count
     rule uses (cells kept, one undo step) and the strip narrows to that ratio's candidates;
     `edit --aspect` is the CLI's half, and a ratio the count has no layout for is a refusal rather than
     a fallback. It comes second because a count's candidates have grown past HIG's "small sets only"
     for a radio strip, and because the row is the reference's own model (布局's ratio split) while a
     pixlay template's ratio *is* the sheet's shape.
  3. **Arrangement depth** (S37) — the poster family at the counts that lack one and the first irregular
     cells beyond `mosaic-8-s14`'s single L: the in-model cousin of Google's "Shapes", and the depth of
     kind after the depth of ratios.
- **Not adopted, so a later session does not re-litigate**: Google's themed categories (style, above);
  category tabs in general — a count × ratio cell holds one to three candidates, and the threshold for
  revisiting is a cell above **six** candidates or the day template packs and user templates land; a
  "Featured" ordering (library order is deterministic and a person scanning sketches does not need
  curation at these set sizes); the 9:16 and 4:5 ratios (the same cheap recipe work the day someone asks).
- **A fourth family** stays as recorded: the three families are read out of the *name* (`Family::of`,
  `layout_for`'s second preference), so a family that is not `strip` / `grid` / `mosaic` waits for the
  explicit field the template-format item below proposes — S35 … S37 do not need it, the poster family's
  own members being `mosaic-*-hero` names already.

## Joining photos whole (stitching)

**Status: not scheduled.** *Asked for* (2026-10-08): Xiaomi's third mode, 拼接 — photos joined edge to
edge with nothing cropped, which is what screenshots want.

- **Why it is not a layout.** A template is frozen geometry with a declared aspect, and every cell crops
  its photo to *cover* it, so a stitch is its inverse: every photo **whole**, and the canvas **derived
  from the inputs** — two 1080x2400 screenshots side by side are 2160x2400, an aspect no frozen template
  guesses. A template approximates a stitch only where every photo already matches its cell's shape.
- **What it would take** (the step that takes it on decides): a framing mode — **fit** rather than cover
  — as a document field, and a canvas that follows the inputs. Both touch rules the contract freezes (the
  clamp in §1, a template's aspect), so this is a product decision before it is work: it changes what an
  export's size means.
- **Until then**: a strip layout with the frame's gap at zero is a seamless join whenever the photos
  match their cells' shapes, and the gap is already a document field.

## Template editing (custom layouts)

**Status: not scheduled.** *Asked for* (2026-09-25). A layout should be authorable, not only shipped.

- **What a template is today.** `Template { name, version, aspect, slots }` with `Slot { outline, area }`
  — a closed polygon in normalized `[0, 1]` coordinates plus its declared area — serde camelCase with
  `deny_unknown_fields`, so it is already a serializable shape. The library has three parts:
  `templates/generator.rs` (recipes on an integer lattice), `templates/frozen.rs` (a generated Rust table
  of `(name, version, aspect, slots)`, byte-compared against the generator by
  `crates/pixlay-core/tests/templates.rs`), and `mod.rs` (the queries `get` / `all` / `names` /
  `of_aspect` / `with_slots`). **There is no template file format**: a shipped template is code, and a
  document embeds a copy of the geometry, which is why the library can change without moving an existing
  project.
- **A custom template is a loader, not geometry work.** `Template::validate` already checks name, version,
  aspect bounds (`0.1..=10.0`), slot count bounds, each outline's simplicity, each declared area against
  its outline (`AREA_TOLERANCE = 1e-6`) and the slots against each other (`topology`: no overlap, no
  interior hole, no more canvas than there is) — and it runs on **every** load, so a hand-written geometry
  in a `.pixlay` is already checked today. What is missing is (1) a file artifact and a loader, (2) an
  identity rule, (3) an authoring surface, (4) the bounds decisions.
- **The four work items**: (1) a loader reading the same shape, with `area` **derived** from the outline
  when it is absent (the declared value exists to cross-check generated data, not to burden a hand-written
  file); (2) identity — a namespace so a user template cannot shadow a shipped name, whether the band's
  count query lists user templates, and how `layout_for` treats one (below); (3) authoring, in rising
  cost: a file edited outside (cheap), a recipe DSL on an integer lattice like the generator's (its CLI
  half is free, which the "nothing only in the GUI" rule wants), a drawing surface in the app (a real
  feature — a new canvas interaction, its CLI mirror and its HIG rows); (4) the bounds: whether
  `MAX_SLOTS` moves, whether non-rectangular slots are allowed (the format already allows them —
  `mosaic-8-s14` has a concave one), and where user templates live (`$XDG_DATA_HOME/pixlay/templates/` is
  the XDG place; `~/.config/pixlay/` holds `settings.json`).
- **Recommended shapes, so the work does not inherit two avoidable couplings** (*proposed*, not ruled —
  the step that takes this on decides them):
  - **the shipped table stays inside the binary.** Moving it to runtime data files
    (`$XDG_DATA_DIRS/pixlay/templates/*.json`) would make `templates`' output a function of the
    installation and of the environment — which the CLI's rules forbid — and would give a built-in
    template a way to fail that it cannot have today. The cheaper unification is an `include_str!`-ed
    **JSON artifact of the same shape** a user loader reads: one loader, one validator, no runtime
    lookup, no new dependency (`serde_json` is already in `pixlay-core`). The generator stays the
    authoring tool and the determinism test keeps byte-comparing.
  - **the family stops being parsed out of the name.** `layout_for`'s second preference reads
    `Family::of(&name)`, which is what makes a free-form name awkward; it becomes an explicit
    `family: Option<Family>` field (shipped templates keep their three families, a user template may have
    none). Names are then free-form identifiers, and no UI shows them (ruling 40 of the plan, 2026-09-25).

### The artifact is a general format, with a specification of its own

**Status: not scheduled.** *Asked for* (2026-09-25): if a layout can be authored, the artifact should be a
**general-purpose format** — JSON, XML, YAML or TOML — and there should be **a specification document**,
so that the same layouts can be used by other software of the same kind.

- **What the specification pins.** The abstract data model first (a canvas aspect; an ordered list of
  slots, each a simple polygon in normalized `[0, 1]` coordinates, with the declared area optional and
  derived when absent); then the **invariants that make a layout machine-checkable** — simple outlines,
  no overlap between slots, no region sealed off from the canvas border, coverage never above the canvas
  (a gutter below it being legal), and the tolerance the area check uses (`AREA_TOLERANCE = 1e-6`); then
  the units and their exactness; then the version rules; and finally **the non-goals**, which matter as
  much as the goals: no style, no colour, no pixels, no resolution, and **no naming scheme** — a name is
  identity and not semantics (ruling 40), so a consumer must never infer the arrangement from it.
- **The binding.** JSON as the canonical one: the `Template` shape is already JSON inside every `.pixlay`,
  and it costs no new dependency, which is what the dependency registry asks of every crate. TOML is the
  friendliest for a hand, but it adds a parser to `pixlay-core`; YAML carries a young serde story and
  significant whitespace; XML is verbose. The step decides, and the specification keeps the *model*
  separate from the *binding* so a second binding stays cheap — a file declares the version it was
  written to.
- **What makes it land with another program.** The model is small enough to reimplement in an afternoon,
  and the value is the library plus the invariants, so the two things that make it usable are a
  **conformance suite** (a handful of valid and invalid geometries with their expected verdicts — the
  rules are already exact enough to freeze as tests), and the reverse direction in the CLI
  (`templates --export <name>` writes an artifact, an import command reads one), which is also what
  makes the format testable from outside the GUI.
- **Metadata a shared file wants, none of it load-bearing for rendering**: a human title, an author, a
  licence (SPDX — the project's own is `GPL-3.0-or-later`) and a version of its own.
- **The risk, stated up front**: a published specification is a compatibility commitment. Afterwards the
  shape may only grow additively, every change needs a version rule, and a document that embeds geometry
  has to keep meaning the same thing forever — which is why this document has to be written **before** a
  drawing tool exists, not after one has shipped.

### Template packs

**Status: not scheduled.** *Proposed.* A directory of artifacts — the format above — scanned by both the
CLI and the GUI, listed beside the shipped library, and delimited by a namespace so a pack can never
shadow a shipped name. Packs are the vehicle the format travels in: a package of layouts, a friend's
handful of grids, a generated family. They are also what makes a band with a *count* filter insufficient,
which is the point at which categories or a search would have to be considered.

## Sharing a whole collage

**Status: not scheduled.** *Proposed.*

- **A project that travels with its photos.** A `.pixlay` references photos by path, so a project is
  portable only while the files stay where they were; a self-contained variant (the photos copied beside
  the project, or embedded in it) would let a collage be sent to another machine and opened anywhere.
  Costs, in the order they bite: the file grows by the size of the photos; the copy has to respect
  "source images are read-only" (copies, never writes back); and a relative-path policy has to survive
  being moved between systems. It also needs the project format's own specification, which is the same
  work item as the template specification above. **This is also what would remove the one soft spot in
  pasting**: an image copied from another application has no file behind it, so S23b writes one out
  (a PNG) and the document references that path — a self-contained project would carry it instead.

## The editor's own gaps

**Status: not scheduled.** Each item says where it comes from: the plan's own "not doing" notes, a ruling
that parked the question, or the assistant's suggestion.

- **Inspect a photo at 1:1.** *Proposed.* The picker's preview carried a fit ↔ 1:1 toggle (S15j) and
  ruling 31 handed that job to the file manager's own viewer, so the editor has no way to look at one
  photo's pixels. If a walk asks for it, the canvas is where it would live — a *view* transform on the
  whole sheet, which is a different thing from a cell's framing and must not be confusable with it.
- **The frame's backdrop as a hit region.** *Parked by a ruling* (2026-09-24, PIX-008): the hit stays
  geometry-only, and the backdrop — thin by construction — does not select the cell behind it. The same
  ruling says a person who expects it to *is* the trigger for a new step, so the question is parked here
  rather than closed.

## Robustness

**Status: not scheduled.** *Proposed.*

- **Autosave and a recovery file.** The atomic writer (S15c) and the dirty check (S15d) exist, and the
  boundaries ask before discarding work — but nothing is written until the user saves, so a crash or a
  killed session loses everything since the last save. The shape that fits what is already there: while
  the document is dirty, write it periodically to `$XDG_CACHE_HOME/pixlay/`, and at startup offer to
  recover a document found there. It must never touch the user's own project file, and it must be
  removable without leaving anything behind.

## More languages

**Status: not scheduled.** *Asked for* (2026-09-27): more languages should be possible — and adding one is
cheap — but it is not needed while the project has no audience.

- **What holds today.** The pipeline is S16's and exactly one language pack ships: `po/zh_CN.po`
  (Simplified Chinese, 2026-09-27) with `zh_CN` in `po/LINGUAS`. English is the source language and the
  fallback, and the CLI is never translated (`AGENTS.md`, "Language conventions"). A language is a `.po`
  file plus a line in `LINGUAS` and **nothing else changes** (`docs/CONTRACT.md` §10): `po/meson.build`
  runs `msgfmt --check` over every listed language and installs the compiled catalog, `msgfmt --desktop` /
  `msgfmt --xml` merge the same catalogs into the desktop entry and the metainfo, and
  `crates/pixlay/tests/i18n.rs` holds a listed catalog to the template — the template's whole message set,
  nothing `fuzzy`, the `{}` placeholders of every entry kept, `msgfmt --check` clean, and the compiled
  catalog really answering `gettext` in a process whose locale selects it.
- **Measured on the one pack that exists** (2026-09-27): 110 messages, all translated; the GUI suite is
  green with that catalog bound (`PIXLAY_LOCALEDIR` + `LANG=zh_CN.UTF-8`); the desktop entry carries
  `Name[zh_CN]`, the metainfo `<summary xml:lang="zh-Hans-CN">`; the window and its settings dialog read
  Chinese in a snapshot. That is the whole cost of a language: one file, one line, and a speaker to read
  the copy.
- **Why it is not scheduled.** *Asked for* (2026-09-27): **nobody but the author uses the product, and
  nobody else is watching the repository.** A third language would be translated for nobody — and the one
  thing no test can check is whether the copy *reads well* (`docs/HIG-REVIEW.md`'s walk item), which needs
  a speaker of that language rather than a translation. So this is a decision about an audience that does
  not exist yet, and the honest state is "the mechanism is ready, the reason is missing".
- **What would trigger it**: the first person who asks for a language, a distribution's translation
  community offering a catalog, or the author's own decision. The mechanism is proven, so a step is only
  needed for the parts that are decisions rather than translation: which languages the repository ships
  (curating `po/LINGUAS`), and whether a pack travels with the tree at all or ships separately — a
  translation-only package, the shape a distribution's own language packaging takes.
- **What must not be added**: a per-language branch in the code, a second copy of the copy, or a language
  that needs more than a `.po` and a `LINGUAS` line. Extraction, the template and the English fallback are
  one pipeline, and the CLI's output is English by rule — a language that wants either changed is a design
  change, not a language pack.
