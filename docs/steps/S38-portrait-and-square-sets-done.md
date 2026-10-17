<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S38 · The portrait and square sets

**Progress**: done and passed (2026-10-17, human) — the six findings were re-checked in the walk of S39's
six-sheet row (the human: 「看过了」, and then the push), and the ruling block under `**Result**` carries the
words and the basis. The step was already built when the re-check came, and S39 then grew the library around
it (125 → 287 members) without touching it: nothing here depends on the library's later size. **→ next:
nothing is scheduled** under `docs/steps/`; the library's next directions are `docs/ROADMAP.md`'s.

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
   not a repeated line. (Its split reads 85/43 of the 128 lattice since ruling 4, the closest dyadic to the
   2:1 named here; the two-cell pairs read 77/51.)

**The walk's six findings** (2026-10-17, human — the walk of the band with the sets in it; each is taken and
built):

3. **The obviously symmetric pairs align.** The 6-photo `左3右3` and `上3下3` had staggered seams; a
   symmetric arrangement's seams have to line up. The 3:4 pair became `grid-6-2x3-3x4` and
   `grid-6-3x2-3x4` (both columns / both rows split 5/6/5), the square's left-3-right-3 became
   `grid-6-2x3-1x1` and the square gained the aligned `grid-6-3x2-1x1` beside the frozen staggered
   `mosaic-6-t3b3`.
4. **Every proportion the list leaves unnamed is the 2:3 split.** The count-2 unequal pairs (8 members) and
   the four-cell bricks now read 77:51 of the 128 lattice (0.6016, 1.51:1 — exact 3:5 is not dyadic) rather
   than the 11:5 they were built at; `mosaic-2-hero` (4:3, frozen) keeps its own 11:5.
5. **`mosaic-7-elbow` comes last in its cell** — "把你的 L 形的放最后，别在前面碍眼" — so the recipe moved
   after count 7's block.
6. **Count 9 gains the 5-and-4 bands**: `mosaic-9-l5r4`, `mosaic-9-t5b4` and `mosaic-9-t4b5`, at 3:4 and
   1:1.
7. **The nine-grid gains an even member.** Exact thirds are not dyadic, so "same size" is built as close as
   the lattice allows: 43/42/43 of the **128** lattice in both directions (within 0.8% of a third). It
   replaces the 6/5/5 splits my 3:4 member had (`grid-9-3x3-3x4`) and joins the frozen `grid-9-3x3` at 1:1
   as `grid-9-3x3-1x1`.
8. **The 1→2 growth lands on 3:4, not 4:3.** Count 1 gained the one-slot `grid-1-1x1-3x4` (the new
   `DEFAULT_TEMPLATE`) and `grid-1-1x1-1x1`, so a fresh window opens on the portrait page, its ratio row
   offers all three sheets, and `layout_for` keeps 3:4 to nine: the chain is a test, and it now reads
   `strip-2-1x2-3x4` → `strip-3-1x3-3x4` → `strip-4-1x4-3x4` → `mosaic-5-t1b4` → `mosaic-6-t2b4-3x4` →
   `mosaic-7-l4r3` → `mosaic-8-t3m2b3-3x4` → `mosaic-9-l4r5`.

**Human**

- The band walk: open the window, add photos (2 … 9), click the **3:4** and the **1:1** buttons in the
  ratio row at each count and read the strip's sketches — does each candidate read as its arrangement
  (the band paints every cell in the theme's foreground and every outline in the dimmed ink, so the
  question is the *shape*), does the two-shape row start read as "these are the usual ones", and is the
  seventh count's offset twin visibly different from its equal-split sibling.
- The walk's material: `crates/pixlay/tests/layout.rs` writes the band's own picture of every member to
  `/var/tmp/pixlay-s7/layout-band-<name>.png` (115 files after S38), so the two rows can also be read
  without opening the window.
- **Passed** (2026-10-17): the human looked at the band again while S39's walk ran — the six-sheet row
  re-checks this step's six findings — and ruled both steps passed with 「看过了」, asking for the push the
  same turn. The walk's material is the same `/var/tmp/pixlay-s7/layout-band-<name>.png` set, re-written for
  all 287 members.

**Implemented as** (2026-10-17) — 83 recipes, the artifact regenerated, the row re-ordered, the default moved,
nothing removed:

- **The recipes** (`templates/generator.rs`): 73 in the first round, appended after each count's existing
  members so the growth chain and `layout_for`'s library-order rung cannot move — 38 at 3:4 and 35 at 1:1;
  two cells: the two equal splits and the four unequal pairs (11/5 then, per ruling 4, 77/51). Three: the
  four band splits, both strips and the two `1+2`/`2+1` pairs. Four: the split, both `1+3`/`3+1` bands, both
  strips and the brick. Five: the two three-and-two bands and the two three-beside-two splits. Six: the two
  `3+3` splits and the four `2+4`/`4+2` bands. Seven: the two `4+3` bands, the three band layouts and the
  offset twin. Eight: the two `4+4` tilings and the two `3/2/3` band layouts. Nine: the nine-grid, its
  wide-middle twin and (the walk's round) the 5-and-4 bands. Ten more came with the walk: the aligned
  `grid-6-3x2-1x1`, the even `grid-9-3x3-1x1` and the one-slot 3:4 and 1:1 sheets — 83 in all. Every split
  is the library's own (`5/6/5`, `8/8`, `4/4/4/4`) or the walk's (2:3 at 77/51, 2:1 at 85/43), and the
  obviously symmetric pairs align (ruling 3). `mosaic-7-elbow` is the one shipped member that moved within
  its count (ruling 5).
- **The artifact**: `frozen.rs` regenerated by the committed bin, and an entry-by-entry audit separates what
  the round did: 13 entries **rewritten in place** (the 8 unequal pairs, the 4 bricks and the 3:4 nine-grid
  — the walk's proportions and the even grid), 3 **renamed** (`mosaic-6-l3r3-*`, `mosaic-6-t3b3-3x4` →
  their `grid-6-*` names), 13 **added**, one **moved** (the elbow) and every other entry byte-identical.
- **The row and the default**: `templates::ratios_for` ranks 3:4 and 1:1 ahead of the ascending rest, and
  `DEFAULT_TEMPLATE` is the new one-slot `grid-1-1x1-3x4` (ruling 8), so the window opens on the portrait
  page and the 1→2 growth keeps it. The tests that pinned the old order and shape moved with it:
  `templates.rs`'s documented-order assertion, its count-1 census and its growth-chain test (renamed to
  `the_default_growth_chain_keeps_the_portrait_shape`, the chain written down), `selection.rs`'s absence
  contrast (3:4 → 16:9), its family-rung case and its 1.2 grid case, the slot canaries 224 → **657**
  (imaging: 1758 → 1971), the CLI's refusal pair and its chain walk, the band's count-2 row length, and the
  band walk itself, which is the **whole library** now.
- **The probes**: every new member renders (`init --photo` + `render --long-edge 800`, exit 0); the band's
  pixels are `render --sketch` at the band's grid with RMSE 0 for every member of the library; the
  eight-candidate cell's band rebuild is **0.23–0.24 ms** (`--release`) against a 147–150 ms switch. The
  numbers are `docs/CONTRACT.md` §8, "S38".

**Result** (2026-10-17): the library is **125 templates / 657 slots** over six ratios (3:4 **48**, 1:1
**51**, 4:3 12, 16:9 7, 3:2 4, 2:3 3) and per count 3, **17**, 16, 15, 14, 17, **18**, 11, **14**; every
count 1 … 9 leads its row with 3:4 and 1:1, and the largest count × shape cell is **8** (count 7 at 3:4) —
`docs/ROADMAP.md` records the six-candidate threshold as crossed. Nothing was removed (ruling 1); the 4:3
chain, the poster census and the irregular census still hold, and the 3:4 chain is the one the window walks
(ruling 8). The entry is green — `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `cargo test` (the GUI suite included) and `reuse lint` (179 files).

**Ruling (2026-10-17, human) — the gate**: **passed**. The human looked at the band with the sets in it and
said 「看过了」, and the same turn asked for the push. The six findings this step built (rulings 3 … 8) are
what that look re-checked, and it closes the gate with no shape left open: the ratios' order, the aligned
pairs, the 2:3 splits, the even nine-grid, the 5-and-4 bands and the 3:4 default are `docs/CONTRACT.md` §3
with their numbers in §8, "S38". **What this ruling does not say**: S37's arrangement-depth item stays
**unjudged** — it is a waiver, and this walk does not reach it (the 128x96 band's legibility across a
count's twelve candidates is still nobody's judgement); everything else on this step's exit list is
machine-checkable and green.
