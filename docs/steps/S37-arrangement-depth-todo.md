<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S37 · Arrangement depth: the poster family and irregular cells

**Progress**: todo — written 2026-10-08 as the plan's third step; it assumes S35's ratios and S36's row
(the depth of *kind* after the depth of shapes).

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

**Human**

- The sketch walk: an irregular cell at the band's 128x96 — is its shape readable, is it distinguishable
  from its rectangular neighbour, and does a poster read as a poster.
