# Pixlay roadmap

Future directions that are **not** steps.

`docs/2026-09-25-STEPS.md` is the plan: the work that is scheduled, ordered and owned, with a
machine-checkable exit for each step. This file holds what a person has said they want to build later,
together with the research already done, so a later session does not repeat it. **Nothing here is an
instruction and nothing here is a promise**: when a direction is ruled in it becomes a step in the plan
(or a plan of its own) and moves out of this file.

## Template editing (custom layouts)

**Status: not scheduled** (recorded 2026-09-25). The human's stated future direction: a layout should be
authorable, not only shipped.

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
- **Recommended shapes, so the work does not inherit two avoidable couplings** (recommendations, not
  rulings — the step that takes this on decides them):
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
