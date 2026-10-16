<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S38 · The portrait and square sets

**Progress**: doing (2026-10-17) — **the work is on disk and the entry is green; the gate is the band walk**.
The two rulings of 2026-10-17 are recorded below (the four concave members stay; the seventh count's sixth
layout is the offset twin). The library is **115 templates / 586 slots**, the two sets' 73 members are in,
`ratios_for` leads with 3:4 and 1:1, and nothing was removed. **→ next: the Human walk (the two new rows'
sketches), then the gate's own commit.**

**Goal**: every photo count from two to nine carries the two sheet shapes the human named as the most
used — the **portrait page (3:4)** and the **square (1:1)** — with the arrangements the human listed, and
the ratio row leads with those two shapes.

**Work**

- **73 new recipes** in `pixlay-core/src/templates/generator.rs`, appended after each count's existing
  members so the growth chain and `layout_for`'s tie-breaks cannot move: **38 at 3:4**, **35 at 1:1**. The
  human's list is met whole — 2 cells: the two equal splits and the four unequal ones (`hero-<side>`,
  11:5); 3: the four band splits, both `1+2` and the two strips; 4: the split, `t1b3`/`t3b1`, both strips
  and the brick; 5: the two two-band splits; 6: the two `3+3` and the four `N + M` bands; 7: the two
  `4+3` bands, the three `2/3/2`-family band layouts and the seventh count's **offset twin** (ruling 2
  below); 8: the two `4+4` and the two `3/2/3` band layouts; 9: the nine-grid and its wide-middle twin.
- **The naming rule the sets use**: a member added for one sheet shape carries that shape as a suffix —
  `strip-2-1x2-3x4`, `mosaic-2-hero-top-1x1`, `mosaic-7-t2m3b2-brick-3x4` — because the natural name of a
  layout is already taken by its first sheet when the second one asks for it (`strip-2-1x2` is 2:3,
  `grid-2-2x1` is 4:3). Frozen names never change; the rule scales to the other four ratios unchanged.
- **The splits, once** (1/3 is not dyadic, so an equal triple does not exist; the library's own
  approximations are reused): a 3-cell band splits 5/6/5, a 2-cell band 8/8, a 4-cell band 4/4/4/4;
  `多/少` splits 11:5 (2.2x, `mosaic-2-hero`'s own); the seven-cell three-band rows 5/6/5; the nine-grid
  6/5/5 (the square's existing `grid-9-3x3` shape) and its wide-middle twin 5/6/5 both ways.
- **The ratio row leads with 3:4 and 1:1** (S38, human: "这是最常用的"): `templates::ratios_for`'s
  documented order becomes `3:4, 1:1`, then the rest ascending (2:3, 4:3, 3:2, 16:9). Every count 2 … 9
  carries both shapes, so the row starts the same way at every count. The new document still opens on
  `grid-1-1x1` (4:3): a single-photo 3:4 or 1:1 sheet is a member count 1 does not have, and the human
  asked for the two shapes beside the album page, not in place of it.
- **The live censuses that move with it**: `crates/pixlay-core/tests/templates.rs` (the ratio row's order),
  `selection.rs`'s "count 2 has no 3:4 layout" contrast (it has six now — the refusal contrast moves to a
  shape the count still lacks), `hit.rs`'s slot-count canary (224 → the new total), the CLI's refusal
  case, and the GUI band's count-2 row length. `docs/CONTRACT.md` §3's census and §8 gain the numbers;
  `docs/ROADMAP.md` records the two rulings and the new per-cell maximum (seven candidates at three
  cells, above the six the roadmap recorded as the threshold to revisit).

**Machine-checkable exit**

- `pixlay-render templates --json` reports **115 templates / 586 slots**, and every count 2 … 9 offers
  both 3:4 and 1:1.
- The library's invariants and censuses run over `templates::all()` and cover the new members: zero
  overlap, no interior hole, a cut template's areas summing to exactly 1.0, simple outlines, the four-ratio
  floor and the poster census.
- Every new member renders: `init --template <name> --photo …` + `render --long-edge 800`, exit 0, for all
  73.
- The band's own pixels equal `render --sketch` at the band's grid (RMSE 0) for the new members.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`,
  `reuse lint` green.

**Rulings (2026-10-17, human)**

1. **The concave members stay.** Asked which of the L/T-cell layouts to remove, the human answered "不想删，
   都别删了。留着吧" — *do not delete any of them*. So `mosaic-3-elbow`, `mosaic-5-tee`, `mosaic-7-elbow`
   and `mosaic-8-s14` all stay, and the first message's third point is **withdrawn**: the L/T cells are
   geometry the product offers, not a defect to cut. The roadmap's "not adopted" list records the ruling
   so a later session does not re-open it uninvited.
2. **The seventh count's sixth layout is the offset twin.** "上2中3下2（上面左：右=2：1,下面相反）" is a
   second layout beside the equal-split one: the top row's two cells split 11/5 (left bigger), the bottom
   row's split mirrored 5/11, the middle row three cells — a running-bond brick, `mosaic-7-t2m3b2-brick`,
   not a repeated line.

**Human**

- The band walk: open the window, add photos (2 … 9), click the **3:4** and the **1:1** buttons in the
  ratio row at each count and read the strip's sketches — does each candidate read as its arrangement
  (the band paints every cell in the theme's foreground and every outline in the dimmed ink, so the
  question is the *shape*), does the two-shape row start read as "these are the usual ones", and is the
  seventh count's offset twin visibly different from its equal-split sibling.
- The walk's material: `crates/pixlay/tests/layout.rs` writes the band's own picture of every member to
  `/var/tmp/pixlay-s7/layout-band-<name>.png` (115 files after S38), so the two rows can also be read
  without opening the window.

**Implemented as** (2026-10-17) — 73 recipes, the artifact regenerated, the row re-ordered, nothing removed:

- **The recipes** (`templates/generator.rs`), appended after each count's existing members so the growth
  chain and `layout_for`'s library-order rung cannot move: 38 at 3:4 and 35 at 1:1. Two cells: the two
  equal splits and the four `hero-<side>` pairs (11/5). Three: the four band splits, both strips and the
  two `1+2`/`2+1` pairs. Four: the split, both `1+3`/`3+1` bands, both strips and the brick. Five: the two
  three-and-two bands and the two three-beside-two splits. Six: the two `3+3` splits and the four
  `2+4`/`4+2` bands. Seven: the two `4+3` bands, the three band layouts and the offset twin. Eight: the two
  `4+4` tilings and the two `3/2/3` band layouts. Nine: the nine-grid and its wide-middle twin. All are
  cut templates and every split reuses the library's own (`5/6/5`, `8/8`, `4/4/4/4`, `11/5`, `6/5/5`).
- **The artifact**: `frozen.rs` regenerated by the committed bin — **873 insertions, 0 deletions**
  (`--diff-algorithm=patience`; git's default diff re-aligns seven lines), and an entry-by-entry check says
  all 42 pre-S38 members are byte-identical with their order a subsequence of the new file.
- **The row**: `templates::ratios_for` ranks 3:4 and 1:1 ahead of the ascending rest, so every count 2 … 9
  leads the same way. The tests that pinned the old order moved with it: `templates.rs`'s documented-order
  assertion, `selection.rs`'s absence contrast (3:4 → 16:9, since count 2 has six 3:4 layouts now) and its
  family-rung case (a 1:1 mosaic exists at four cells now), `hit.rs`'s and `imaging`'s slot canaries
  224 → **586** (the latter 672 → 1758, slots × framings), the CLI's refusal pair, the band's count-2 row
  length (4 → 5), and the band walk itself, which is the **whole library** now.
- **The probes**: 73/73 members render (`init --photo` + `render --long-edge 800`, exit 0; 154–470 KB each);
  the band's pixels are `render --sketch` at the band's grid with RMSE 0 for all **115** members; the
  eight-candidate cell's band rebuild is **0.23–0.24 ms** (`--release`) against a 147–150 ms switch. The
  numbers are `docs/CONTRACT.md` §8, "S38".

**Result** (2026-10-17): the library is **115 templates / 586 slots** over six ratios (3:4 **44**, 1:1
**45**, 4:3 12, 16:9 7, 3:2 4, 2:3 3) and per count 1, **17**, 16, 15, 14, 16, **18**, 11, 7; every count
2 … 9 carries both 3:4 and 1:1, so `ratios_for`'s leading pair sits on every row; the largest count ×
shape cell is **8** (count 7 at 3:4) and `docs/ROADMAP.md` records the six-candidate threshold as crossed.
Nothing was removed (ruling 1) and the growth chain, the poster census and the irregular census are
unmoved. The entry is green — `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test` (the GUI suite included, **697 s** wall) and `reuse lint` (179 files).
