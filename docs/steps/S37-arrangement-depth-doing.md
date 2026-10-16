<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S37 · Arrangement depth: the poster family and irregular cells

**Progress**: doing — implemented, measured and committed 2026-10-16 (this step's commit); **the Human walk
is what is left**: the sketch walk (`docs/HIG-REVIEW.md` §2's arrangement-depth item — does an L or a T read
at the band's own 128x96, is it distinguishable from its rectangular neighbour, and does a poster read as a
poster). Everything else on the exit list is green and its numbers are `docs/CONTRACT.md` §8, "S37".

**Goal**: the library grows in kinds, not only in counts — the poster arrangement (one photo dominant,
Xiaomi's 海报) wherever a count lacks one, and the first irregular cells beyond `mosaic-8-s14`'s single
L, which is the in-model cousin of Google's "Shapes": geometry that a sketch can show.

**Work**

- Recipes in the generator. Measured starting point (2026-10-08, from `frozen.rs`): poster-shaped members
  (`mosaic-*-hero`) exist at counts 3, 4, 5, 6, 7 and 9; count 2 has none; count 8 carries
  `mosaic-8-s14`, whose largest cell is 0.297 of the sheet and which is also the library's irregular
  member — the step decides whether that counts as a poster or whether count 8 gets another member.
- The step **measures what earns the name**: the obvious properties do not separate the shipped heroes
  from their neighbours on their own — the largest cell is 0.50 of the sheet at counts 3, 4, 5 and 9 but
  0.25 at 6 and 0.19 at 7, and the largest/smallest ratio is 1.9 for `mosaic-7-hero` where
  `strip-9-9x1`, no hero, is 2.0. So the step measures a property (or a pair of them) that holds for
  every `-hero` member and no non-hero, asserts it if it finds one, and records the numbers either way;
  the census below holds by name in any case.
- The target tranche: a `-hero` member at count 2, and at count 8 if `mosaic-8-s14` is not accepted as
  its poster, plus three irregular members (counts 3, 5 and 7) — four to five new members, the library at
  **41 or 42** against S35's 37.
- **2–3 irregular members** (`Shape::Poly`): an L or T at counts 3, 5 and 7, against machinery that is
  already general (the clamp tests the outline's own vertices, the topology checks a concave slot, the
  sketch strokes any simple polygon). The risk is the *look* at the band's 128x96 (S21, S29 and S30 are
  the record of how narrow that size is), which is why this is a step of its own.
- `docs/CONTRACT.md` §3's census and §8's numbers; `docs/ROADMAP.md`'s numbers follow.

**Machine-checkable exit**

- The census extends S35's in `crates/pixlay-core/tests/templates.rs`: a poster member at **every count
  2 … 9** (asserted through the definition above), and **at least three irregular members across at least
  three counts**; the invariants and the framing sweeps run over `templates::all()` and cover the new
  members.
- Every new layout renders (`init --photo` + `render --long-edge 800`, exit 0), and for each the band's
  own pixels equal `render --sketch` at the band's grid (`tests/layout.rs`'s existing RMSE-0 check).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`,
  `reuse lint` green.

**Implemented as** (2026-10-16) — five members, and the definition the census is asserted through:

- **What earns the name, measured rather than assumed.** The step's own note was right that the obvious
  properties do not separate the `-hero` members from their neighbours, and the measurement resolved it:
  a poster is **the largest cell at least twice the *median* cell** of the same template (the median being
  the cell at index `count / 2` in descending order). Over S36's library that is `mosaic-3-hero` and
  `-t1b2` at 2.0x, `-4-hero`/`-t1b3` at 3.2x, `-5-hero`/`-t1b4` at 4.0x, `-6-hero` at 2.0x, `-9-hero` at
  8.0x and `mosaic-8-s14` at 3.17x — and `mosaic-7-hero` at **1.2x**, so the count's only member named for
  the arrangement is a three-column mosaic, not a poster. Nothing else reaches 2.0x (the loosest
  non-poster is `strip-7-7x1`'s 1.5x). Two consequences the step could not have assumed:
  **count 8's poster is `mosaic-8-s14`**, so count 8 needed no second member (the step's own question,
  answered by the number); and **no property separates a `-hero` from its transcribed twin**
  (`t1b2`/`t1b3`/`t1b4`), because a twin *is* the same arrangement on a rotated sheet and carries the same
  areas by construction — the pair is told apart by its name, which is why the library names one of each
  `hero` and the other by its bands.
- **Five new recipes** (`templates/generator.rs`), each appended after its count's existing members so the
  growth chain cannot move: `mosaic-2-hero` (2 slots, 4:3: 11/16 + 5/16, 2.2x) — the poster count 2 lacked;
  `mosaic-7-l1r6` (7, 4:3: a left half plus a 2x3 block, 6.4x) — count 7's poster, named by its bands the
  way a transcribed hero is, because `mosaic-7-hero` holds that name and is not one; and the three
  irregular members, `mosaic-3-elbow` (3, 4:3), `mosaic-5-tee` (5, 1:1) and `mosaic-7-elbow` (7, 3:4) —
  an L, a T and an L on the portrait page. All five tile their sheet exactly (their areas sum to 1.0, each
  a sum of exact dyadic values) and are cut templates like every mosaic. `mosaic-5-tee` and
  `mosaic-7-elbow` satisfy the poster property as well (4.87x and 2.0x): a kind of cell and an arrangement
  are two independent questions, which is what measuring rather than naming shows.
- **The vocabulary, told once.** `-elbow` (an L-shaped slot) and `-tee` (a T-shaped one) joined the
  variant words, beside the frozen `mosaic-8-s14` whose L was the library's only irregular cell from S1 to
  S36 — `docs/CONTRACT.md` §3 states it, and §3's new "two kinds of depth are measured, not named"
  carries both definitions, the measured numbers and the one `-hero` that fails the property.
- **Two censuses, asserted over the whole library** (`crates/pixlay-core/tests/templates.rs`):
  `every_count_from_two_up_carries_a_poster` (a poster at every count 2..=9, the count-2 and count-7
  members named in the assertion, and the `-hero` members that fail the property pinned to exactly
  `["mosaic-7-hero"]`, so another cannot join unnoticed) and
  `the_library_carries_irregular_cells_across_several_counts` (at least three irregular members over three
  counts, the census `[3, 5, 7, 8]`, and the three new names present). Both are asserted through the
  *property*, not through a name, which is the step's own answer to "asserts it if it finds one, and
  records the numbers either way".
- **The band's pixels, per new member** (`crates/pixlay/tests/layout.rs`): a document of each new member's
  own count and shape is opened, and the band's own sketch of it is compared with
  `pixlay-render render --sketch` at `candidate_grid(aspect)` with the band's own colours and width —
  RMSE **0** for all five — with the sketch's grid asserted to be the shape's (128x96, 96x96, 72x96) and
  each band written out for the Human walk (`/var/tmp/pixlay-s7/layout-band-<name>.png`).
- **Two canaries the library's growth moved, and one assertion whose premise S37 changed**:
  `crates/pixlay-core/tests/hit.rs` pins the library's slot count twice, 200 → **224** (its purpose is to
  fail when a sweep covers less than the library); and `crates/pixlay-core/tests/history.rs`'s
  count-control walk asserted that `−` × k then `+` × k returns the layout it started on *whenever the walk
  kept the aspect and the family*. S37 puts two members in one shape-and-family group at four counts (2/4:3,
  3/4:3, 5/1:1, 7/4:3, 7/3:4, and count 4/1:1 already was one), and `layout_for` cannot tell two members of
  one group apart — it answers with the one the library lists first. So that assertion now reads "the
  walk's return is the **rule's own answer** for the keys it kept", which is the same claim where the cell
  holds one member and a true one where it holds two; the invariant the test is named for (no layout change
  loses a cell) is untouched.

**Result** (2026-10-16): the library is **42 templates / 224 slots** over six ratios (4:3 12, 1:1 10, 16:9
7, 3:4 6, 3:2 4, 2:3 3) and per count 1, 5, 5, 5, 6, 5, 6, 5, 4 — **13 posters** satisfying the measured
property, over every count 2 … 9, and **4 irregular members** at counts 3, 5, 7 and 8. The regeneration
diff is 59 insertions, **0 deletions**, so no shipped name's geometry moved; the growth chain is
`grid-2-2x1` → `mosaic-3-hero` → `mosaic-4-hero` → `mosaic-5-hero` → `mosaic-6-hero` → `mosaic-7-t4b3` →
`mosaic-8-s14` → `mosaic-9-hero`, unchanged and 4:3 at every step; the five new layouts render at 800 px
with a photo in every cell (265–409 KB, `ms` 75.0–147.5) and their sketches at the band's own grid are
`/var/tmp/s37probe/s37-sketches.png`. The numbers are `docs/CONTRACT.md` §8, "S37", the definitions §3, the
walk `docs/HIG-REVIEW.md` §2, and the library's own census `docs/ROADMAP.md`. The entry is green —
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` (the GUI suite
included) and `reuse lint` (178 files) — and **the Human walk is the only thing left on this step**.

**Human**

- The sketch walk: an irregular cell at the band's 128x96 — is its shape readable, is it distinguishable
  from its rectangular neighbour, and does a poster read as a poster.
