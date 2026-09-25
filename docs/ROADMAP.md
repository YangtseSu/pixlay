# Pixlay roadmap

Future directions that are **not** steps.

`docs/2026-09-25-STEPS.md` is the plan: the work that is scheduled, ordered and owned, with a
machine-checkable exit for each step. This file holds what a person has said they want to build later,
together with the research already done, so a later session does not repeat it. **Nothing here is an
instruction and nothing here is a promise**: when a direction is ruled in it becomes a step in the plan
(or a plan of its own) and moves out of this file.

Provenance is marked per item: *asked for* means the human named the direction, *proposed* means it is
the assistant's suggestion, recorded for a decision nobody has made yet.

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
shadow a shipped name. Packs are the vehicle the format travels in: an AUR package of layouts, a friend's
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
  work item as the template specification above.

## The editor's own gaps

**Status: not scheduled.** Each item says where it comes from: the plan's own "not doing" notes, a ruling
that parked the question, or the assistant's suggestion.

- **Inspect a photo at 1:1.** *Proposed.* The picker's preview carried a fit ↔ 1:1 toggle (S15j) and
  ruling 31 handed that job to the file manager's own viewer, so the editor has no way to look at one
  photo's pixels. If a walk asks for it, the canvas is where it would live — a *view* transform on the
  whole sheet, which is a different thing from a cell's framing and must not be confusable with it.
- **Drag a cell onto another.** *From S23's "not doing".* S23 gives every pair of cells a swap
  (`Shift`+click and the keyboard); a pointer drag from one cell to another, with a drop on an occupied
  cell exchanging the two, is the follow-on it leaves out.
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
