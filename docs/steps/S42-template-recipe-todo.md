<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S42 · The template recipe: a file, a loader and a specification

**Progress**: **todo** — nothing is built. → next: **the probe first** (what the window does with a project
whose template this build's library does not have, measured rather than inferred), then the nine decisions this
file carries, then `docs/CONTRACT.md` §3's specification subsection, then the loader.

**Goal**: a layout becomes **authorable**. A template artifact is a plain file holding a **recipe** — the
arrangement ("eight photos: three over two over three"), not a sheet's shape — and it is loaded, validated and
**materialized onto a sheet's ratio when it enters a document**. The shipped library and a user's file go
through one loader, one validator and one emitter; the **document format does not move at all**.

## What this step inherits

The premises are ruled and recorded (`docs/ROADMAP.md`, "Template editing (custom layouts)", restated by the
human 2026-10-17):

- **the artifact is a recipe and it has no aspect — the document does.** The library's own source is already
  that shape: `Arrangement` in `crates/pixlay-core/src/templates/generator.rs` — 67 rows, **44 emitted at all
  six sheets** and 23 at exactly one — with one lattice geometry per row and `build()` copying it per sheet tag.
  The artifact is that abstraction in a file.
- **`sheets[]` is a whitelist, not a promise**: normalized geometry stretched onto another shape is another
  layout, and the 23 single-sheet rows are deliberate (a five-column strip at 3:4 is five slivers; the gutter
  variants, the `-hero` pairs, `elbow`/`tee`).
- **a proportion is the template's own value.** Every recipe carries its splits as literal lattice numbers
  (77/51, 88/40, 85/43, 40/48/40, 43/42/43); the walk of 2026-10-17 **named** 2:3 for the pairs it left
  unnamed and the human restated that this is not a rule. The format gives a split a field.
- **the two `2:3`s are different things**: `2:3` is already the portrait **sheet** in the band's vocabulary and
  in every census table. A cell's proportion is not an aspect.
- **a loader, not geometry work** (measured 2026-10-17, `pixlay-render`): a hand-written concave 1:1 template
  inside a `.pixlay` renders, hits correctly and round-trips **byte-identically** through `save`; five refusal
  classes exit 2 with their reason; a **non-dyadic** split (1/3 as `0.3333333333333333`) loads as well.
- **the library today**: 293 templates (**287 collage + 6 ID photo sheets**), 1522 + 58 slots
  (`pixlay-render templates`, 2026-10-17, after S40).

## The decisions this step fixes first

Each is a **compatibility commitment** once it is in the specification, so it is decided before code is written.
The recommendation column is what this plan proposes; the step's `Ruling` section records what the human decides.

| # | Question | Recommendation | Lands in |
|---|---|---|---|
| 1 | Where does the sheet's ratio live — the template, the document, or neither? | **The file has none, the document keeps its own** (materialize at import). A `sheet`/ratio field on the document would be additive (no `docVersion` bump) but re-cuts half of S35/S36/S38/S39; it waits for a walk that asks for "change the shape without changing the layout" | the specification's first paragraph; no code change to `CollageDoc` |
| 2 | One artifact type, or `Template` itself with `area` optional? | **Its own type with its own `formatVersion`**, and `Slot.area` stays **required in a document** (derivation happens in the artifact→`Template` conversion). Measured: the cross-check refuses a drift (`template slot 0 declares area 0.75 but its outline covers 0.5`) | `crates/pixlay-core/src/templates/` + `docs/CONTRACT.md` §3 |
| 3 | Identity: shadowing, family, where files live, how the CLI names one | **Refuse a name that shadows a shipped one** (the error names both sources); `family: Option<Family>` becomes an explicit field instead of `Family::of(&name)`; files in `$XDG_DATA_HOME/pixlay/templates/`; the CLI takes an **explicit path** (`--template-file`) so its output stays a function of the command line | `templates/mod.rs`, `selection.rs`, `args.rs` |
| 4 | `(name, version)` semantics when a file is edited in place | A project never moves (it embeds the geometry); **the same `(name, version)` with different geometry is refused inside one run** rather than silently picking one | the specification + the loader |
| 5 | How a split is written, and how a ratio reaches the lattice | Three spellings: `equal`, a ratio, or lattice units; **the rounding rule is the specification's** (2:3 of 128 is 51.2) | the specification |
| 6 | May a user artifact be `onePhoto`? | **No, not in this step**: `is_collage` reads the `frozen::ONE_PHOTO` **name list** rather than the field, so a user sheet would slip past every candidate query | the loader's refusals |
| 7 | Do user templates enter the band / the count rule? | The band **yes** (after the shipped members of the same count and shape); `layout_for` **no** — a file must not silently change what the count control does | S43, recorded here |
| 8 | Extension and MIME | v1 keeps **`.json`** with a `"format": "pixlay-template"` discriminator; a second MIME type is a packaging change and waits until something has to be opened from the file manager | `docs/CONTRACT.md` §3, `data/` (not this step) |
| 9 | Specification before tool, and how it may evolve | The specification and its conformance cases land **in this step**, and afterwards the shape may only **grow additively** | `docs/CONTRACT.md` §3 + `crates/pixlay-core/tests/recipe.rs` |

## Work

1. **The probe, before anything is designed on top of it** — the window's behaviour on a document whose
   template is not in the library is **inferred, not measured**: `candidate_templates` is
   `templates::with_slots_and_aspect(cells.len(), template.aspect)` (`crates/pixlay/src/window.rs:1741`), the
   highlight is looked up by name, and `select_layout` returns silently when `templates::get(name)` is `None`
   (`window.rs:1757`). Write the project with the CLI, open it in the GUI suite's own window, and **record**:
   what the strip holds, whether the document's own layout is marked as current, what a click on a candidate
   does, and whether the way back exists. The result may move decision 7 into this step.
2. **The specification** — `docs/CONTRACT.md` §3 gains a subsection: the abstract data model (a name, a version,
   a sheet whitelist, an ordered list of split segments), the invariants that make a layout machine-checkable
   (simple outlines, no overlap, no interior hole, coverage never above the canvas, `AREA_TOLERANCE = 1e-6`),
   the units and their exactness, the version rules, and the **non-goals** (no style, no colour, no pixels, no
   resolution, no naming scheme). The two `2:3`s are named and kept apart.
3. **The type and the loader** — a recipe type in `crates/pixlay-core/src/templates/`, `Deserialize` with
   `deny_unknown_fields`, `validate()` reusing `Template::validate` + `topology`, and
   `materialize(aspect, …) -> Template` deriving `area` from the outline. Typed errors in `error.rs` for the
   refusals: shadowing, `onePhoto`, an unknown sheet tag, a ratio that does not reach the lattice, an empty or
   over-ceiling slot list.
4. **The CLI** — `templates --export <name> [--out <file>]` (a shipped member written back as a recipe, which is
   also how the round trip is tested) and `--template-file <path>` wherever `--template <name>` is read today
   (`render`, `init`, `edit`, `hit`, `switch`); usage text, the flag-ownership tables in `args.rs`, and the exit
   codes the CLI's contract fixes.
5. **Tests** — `crates/pixlay-core/tests/recipe.rs`: the round trip (a shipped member exported and re-imported
   materializes a `Template` **identical** to the library's), materialization onto each of the six sheets, the
   six refusals (the messages measured 2026-10-17 are the assertion texts), a non-dyadic recipe, and the
   `(name, version)` rule of decision 4.
6. **Docs** — `docs/CONTRACT.md` §3 and §5, `CHANGELOG.md`, and `docs/ROADMAP.md`'s "Template editing" section
   pointing at this file.

## Machine-checkable exit

- `cargo test -p pixlay-core --test recipe` green, including the round trip: for a shipped member,
  `materialize` is equal to `templates::get(name)` field by field.
- The CLI round trip is byte-identical end to end:
  `pixlay-render templates --export mosaic-8-s14 --out /var/tmp/x.json` then
  `pixlay-render init --template-file /var/tmp/x.json --out /var/tmp/a.pixlay` produces a file **equal to**
  `pixlay-render init --template mosaic-8-s14 --out /var/tmp/b.pixlay`.
- The refusal fixtures exit **2** with their own message: two slots overlapping, an interior hole, a
  self-crossing outline, a declared area that drifts from its outline, an unknown key, a name that shadows a
  shipped one, `onePhoto: true`, a `sheets` tag the library does not have.
- `pixlay-render templates` (no flags) is **unchanged byte for byte** — the library is the build's, and the
  user's directory is not an input to it.
- The probe of item 1 is recorded in this file with its numbers, and the GUI suite's own test for it is green
  (or, when the probe shows the band needs more than a plan change, this file's scope note says so).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` and `reuse lint`
  green.

## Human

**Not yet.** The criterion is a **reading**, not a walk: the human reads the specification subsection and rules
on the artifact's shape (it is a compatibility commitment afterwards), and runs three commands —
`templates --export`, `init --template-file`, and an edit of the exported JSON by hand followed by a re-import.
The step stops at that ruling (`AGENTS.md`, "Step discipline", principle 5).

## Ruling

*(to be written: date, verdict, basis — decisions 1 … 9 and the probe's numbers.)*

## Result

*(to be written.)*

## After this file

Two rungs are recorded in `docs/ROADMAP.md` and **are not files yet**: **identity and the band** (decision 7's
band half: user members listed after the shipped ones, the ratio row's semantics on a document whose ratio the
library does not carry) and **authoring** (a sugar layer that desugars into the canonical lattice form, then a
drawing surface if a walk asks — a separate surface over a *template*, never a mode of the collage canvas, and
every gesture needs its CLI mirror).
