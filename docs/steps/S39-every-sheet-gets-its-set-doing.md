<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S39 · Every sheet gets its set

**Progress**: doing (2026-10-17) — **the work is on disk and the entry is green; the band walk is the gate.**
The ruling below is built: all six sheets carry the sets, one arrangement order across a count's six cells
with the irregular ones last, and the library is a matrix. **287 templates / 1522 slots**. **→ next: the
Human walk (the six-sheet row, the ordering, and S38's six fixes), then the gate's own commit.**

**Goal**: the human's ruling of 2026-10-17 — "全都配齐。调整每个里面的顺序，所有的比例内的模板，按照一个顺序来。异形的
放到最后" — made the four thin sheets first-class: **2:3, 3:2, 4:3 and 16:9 get the same arrangements 3:4 and 1:1
already have**; **every count's six ratio cells list their arrangements in one order** — the human's own list,
then the members the list does not name, then the irregular (concave-cell) members; and the library stays *one
arrangement × six sheets* rather than six near-copies of the same geometry.

**Work**

- **The generator's data model**: `Recipe` (one member) becomes **`Arrangement` × sheets** — one geometry, the
  sheets it is authored at, and the shipped-name overrides where a native predates the tagging rule
  (`grid-2-2x1` *is* the 4:3 member of the 左右平分 arrangement; `strip-2-2x1` is the 3:2 one). The frozen
  artifact is the flattening, so "this arrangement at six sheets" is one source row rather than six
  near-identical recipes — which is what makes the human's "one order" structural instead of a convention.
- **The canonical order**: per count, the human's arrangements in the human's list order, then the members the
  list does not name, then the irregular members last; within one arrangement, the sheets in the row's
  documented order (`ratios_for`: 3:4, 1:1, 2:3, 4:3, 3:2, 16:9). A strip therefore reads the same way at every
  sheet, and the count-1 row and every other count's row hold the same six shapes.
- **The members**: the four thin sheets' sets (the same list of arrangements, from the two-cell pairs through
  count 9's five) and the three missing one-slot sheets at count 1 — about 160 new members — every one a copy
  of geometry that already exists, because an arrangement's lattice does not depend on the sheet it is
  stretched onto.
- **Tests**: the order becomes an assertion (per count: one arrangement order across its six cells, the
  irregular members last, one arrangement's sheets ascending); the band walk becomes a **per-cell** walk (one
  document per count × sheet, every candidate of it in that document); the growth chains, the slot canaries,
  the censuses and the CLI/GUI counts move with the library.
- **Documents**: `docs/CONTRACT.md` §3 (the matrix, the order, the census) and §8 (S39's numbers),
  `docs/ROADMAP.md`, `CHANGELOG.md`, `docs/HIG-REVIEW.md` (the walk's item covers the six-sheet row now).

**Machine-checkable exit**

- `pixlay-render templates --json` reports every count 1 … 9 with **all six sheets**, and for each count the six
  cells list the same arrangements in the same order with the irregular ones last (the test below asserts it).
- The regeneration audit: **every member that shipped before S39 keeps its name, version and geometry byte for
  byte**; only the order and the new members move.
- The library's invariants and censuses are green over the grown library; every new member renders; the band's
  pixels are `render --sketch` at the band's grid (RMSE 0) for the whole library; `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` and `reuse lint` are green.

**Ruling (2026-10-17, human)**

- **"全都配齐"** — all six ratios get sets; the four thin ones are not kept as near-empty rooms.
- **"调整每个里面的顺序，所有的比例内的模板，按照一个顺序来"** — one arrangement order across every ratio's cell
  of a count.
- **"异形的放到最后"** — the concave-cell members go last.

**Human**

- The band walk over the six sheets: open the window, add photos, click each of the row's six buttons — does
  every sheet read as a set of arrangements, does the strip hold the same sequence whichever sheet is clicked,
  and are the irregular ones last.
- The re-check of S38's six fixes, which this round folds into the same walk.

**Implemented as** (2026-10-17) — the matrix, every sheet's set, one order, the irregulars last:

- **The generator is a matrix**: `Arrangement { variant, slots, sheets, names }` — one geometry, the sheets it
  is authored at, and the name overrides where a member predates the tagging rule (`grid-2-2x1` is the 4:3
  member of the two-equal-columns arrangement, `mosaic-3-hero` of left-one-right-two, `mosaic-7-t4b3` of
  top-four-bottom-three). `SHEETS` and `SET` carry the six tags in the row's order; `generate()` flattens in
  the canonical order and the frozen artifact is that flattening. The matrix surfaced three pre-S39 slips and
  fixed them: `mosaic-5-b3t2-1x1` was a **duplicate** of the frozen `mosaic-5-t2b3` (dropped; its 3:4 sibling
  renamed to `mosaic-5-t2b3-3x4`), `grid-6-2x3-3x4`/`-1x1` disagreed with the frozen `grid-6-2x3`'s rows
  (both now 6/5/5), and `grid-8-2x4-1x1`'s slot order differed from the frozen `grid-8-2x4`'s (now the same).
- **Every sheet's set**: +164 members (the four thin sheets' arrangements and the missing one-slot sheets) —
  the library is **287 templates / 1522 slots**; per count 6, 38, 37, 38, 28, 38, 41, 27, 34; per sheet
  3:4 48, 1:1 51, 2:3 44, 4:3 51, 3:2 44, 16:9 49; the largest cell is 8 candidates (count 7 at 3:4).
- **The order**: per count the human's list, then the members the list does not name (strip-5..9, the
  gutters, the heroes, `mosaic-6-t3b3`, the elbows/tee/s14), then the irregulars last; within an arrangement
  the sheets in `SHEETS` order. `every_sheet_of_a_count_holds_the_arrangements_in_one_order` asserts it over
  the whole library, and the audit (below) says every other member is byte-identical.
- **The tests**: the walk is per (count, sheet) cell now — one document per cell, every candidate compared —
  and the landings that pinned a name moved with the order (`selection.rs`'s rungs, `history.rs`'s walks and
  floor, `kept.rs`, the CLI's chain and step tests, `mainpath.rs`'s default). The slot canaries are
  1522 (imaging: 4566 = slots × framings).
- **The probes**: 287/287 members `init --photo` + `render --long-edge 800` exit 0 (108,263–486,844 bytes;
  0.113–0.322 s each, release); the band's pixels are `render --sketch` at the band's grid for **every
  (count, sheet) cell** (RMSE 0).

**Result** (2026-10-17): the library is **287 templates / 1522 slots** — one arrangement × six sheets, every
count 1 … 9 carrying all six, the strip reading one order with the irregular cells last — and the
regeneration audit says no member that shipped before S39 moved (three reshaped and two renamed among S38's
own unreleased members are the only exceptions, and they are recorded above). The entry is green —
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` (the GUI suite
included, **774 s** wall — the band's walk 162.7 s of it, and every member's band picture written to
`/var/tmp/pixlay-s7/`) and `reuse lint` (**180 files**).
