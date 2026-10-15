<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S35 · Every count has its canvases

**Progress**: doing — implemented, measured and committed 2026-10-16 (this step's commit); **the Human walk
is what is left**: the band's legibility over the ten new candidates at its own 128x96
(`docs/HIG-REVIEW.md` §2, the gallery item). Everything else on the exit list is green and its numbers are
`docs/CONTRACT.md` §8, "S35".

**Goal**: every photo count from 2 to 9 offers the sheet shapes a person asks for — 4:3, 1:1, a portrait
member and one more — so that the count rule can keep the sheet's shape while the collage grows, and the
ratio stops being an accident of the count a document is on.

**Why first — the measured defect** (2026-10-08, `pixlay-render edit --add-cell`): the window opens on
`grid-1-1x1` (4:3), **count 2 is the only count with no 4:3 layout**, and so the first added photo takes
the sheet to 3:2 (`strip-2-2x1`) and the second to 16:9 (`strip-3-3x1`), where the rest of the growth
stays. The aspect rank is `layout_for`'s first preference and the 4:3 chain already exists at 1 and 3 … 9
and every growth lands on it (measured), so one 4:3 two-cell member is the whole fix. Data only: no new
machinery, no shell change — a `Recipe` plus a regenerated `frozen.rs`.

**Work**

- Recipes in `crates/pixlay-core/src/templates/generator.rs`. The measured starting point (2026-10-08):
  - 4:3 missing at count 2 only; 16:9 missing at count 2 only; 1:1 missing at 3, 5, 6 and 8; a portrait
    member (2:3 or 3:4) missing at 4, 5, 7, 8 and 9; counts 2, 3, 5 and 7 carry three layouts.
  - The 2-cell 4:3 member is a `grid-2-*`: the family of `grid-1-1x1`, so the first growth keeps the
    ratio *and* the family; the 2→3 step moves family (to `mosaic-3-hero`) and keeps 4:3, which is the
    aspect-first preference doing its job.
  - **The target tranche is 10 new members** (2026-10-08): count 2 **+4:3** (`grid-2-2x1`, the default
    chain's missing link), count 3 **+1:1**, count 4 **+3:4**, count 5 **+1:1** and **+3:4**, count 6
    **+1:1**, count 7 **+3:4**, count 8 **+1:1** and **+3:4**, count 9 **+3:4**. The library lands at
    **37 members / 200 slots**: every count 2 … 9 with four or five layouts over four or five ratios,
    carrying 4:3, 1:1 and a portrait member each, and 3:4 with five members. The step may move a member
    against the census but not the census.
  - **No name carries its ratio** (`<family>-<slots>-<variant>`), so a member of a new ratio needs a
    variant of its own: a *transposed* twin is already expressible (`grid-6-3x2` / `grid-6-2x3`,
    `strip-2-2x1` / `strip-2-1x2`), but a symmetric variant — `2x2`, a hero — has no free twin name, and
    the new member takes a distinct mnemonic instead.
- **3:4 becomes the library's sixth ratio** — Xiaomi's page ratio and the **portrait** sheet: a ratio is
  written width:height, so 3:4 is 0.75, the portrait of the default 4:3 sheet (4:3 is 1.333) — with a
  small number of members (*proposed* here; the step's own ruling confirms or drops it). 9:16 and 4:5
  stay unbuilt (`docs/ROADMAP.md` records why).
- `frozen.rs` regenerated with `cargo run -p pixlay-core --bin pixlay-gen-templates` and reviewed as a
  diff; **no shipped template's name, version, aspect or coordinates move** (the regeneration diff and
  the fingerprint test are the guards).
- `docs/CONTRACT.md`: §3's census (the histogram, the templates/slots totals) and §8's S35 measurement
  row; `docs/ROADMAP.md`'s "what it holds today" numbers follow.

**Machine-checkable exit**

- `crates/pixlay-core/tests/templates.rs` gains the census: every count 2 … 9 carries **at least four
  layouts over at least four aspect ratios**, including **4:3**, **1:1** and at least one portrait member
  (2:3 or 3:4); **3:4 has at least two members**; the histogram is printed as the failure's evidence as
  S10's is. Count 1 keeps its one-member rule (ruling 34).
- The same file walks the **default growth chain**: from `grid-1-1x1`, `layout_for(count, 4:3, previous
  family)` at every count 1 … 9 answers a 4:3 template — which is what `AddCell` does — so "the sheet
  keeps its shape while the collage grows" is a test rather than a hope.
- Every new member passes the existing invariants (zero overlap, no interior hole, areas summing to
  exactly 1.0, simple outlines, the 512² sampling) and the pre-S10 fingerprint test still passes,
  unchanged.
- Every new layout renders: `init --template <name> --photo … --out x.pixlay` then `render --long-edge
  800` exits 0 per member (the S10 exit, repeated over the tranche).
- `pixlay-render templates` reports the new matrix; the counts in `docs/CONTRACT.md` §3 match the CLI.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`,
  `reuse lint` green.

**Implemented as** (2026-10-16) — the ten members, in recipe order, each with the arrangement it carries:

| count | ratio | name | arrangement |
|---|---|---|---|
| 2 | 4:3 | `grid-2-2x1` | two equal columns — the default chain's missing link, in `grid-1-1x1`'s own family |
| 3 | 1:1 | `mosaic-3-t1b2` | a wide top panel over two square ones — `mosaic-3-hero` turned on its side |
| 4 | 3:4 | `mosaic-4-t1b3` | a wide top panel over three — `mosaic-4-hero` on its side, its splits 5/16 · 6/16 · 5/16 |
| 5 | 1:1 | `mosaic-5-t2b3` | two square panes over three — no equal tiling of a square holds five |
| 5 | 3:4 | `mosaic-5-t1b4` | a wide top panel over four — `mosaic-5-hero` on its side |
| 6 | 1:1 | `mosaic-6-t3b3` | two rows of three, splitting 5/6/5 over 6/5/5 — the count's three-by-two read |
| 7 | 3:4 | `mosaic-7-l4r3` | the transpose of `mosaic-7-t4b3`: four panes down the left, three down the right |
| 8 | 1:1 | `mosaic-8-t4b4` | two rows of four, the lower row splitting 5/3/3/5 |
| 8 | 3:4 | `grid-8-2x4` | the transposed twin of `grid-8-4x2`, two columns of four |
| 9 | 3:4 | `mosaic-9-l4r5` | four panes down the left and five down the right, the middle of the five the tallest |

The names follow the step's own rule — no name carries its ratio — as a transposed twin where one is
expressible (`grid-8-2x4`) and a distinct mnemonic where the arrangement is its own transpose or its
variant word is taken (`t<b>b<b>`, `l<l>r<r>`); `docs/CONTRACT.md` §3 now states that rule.

**Result** (2026-10-16): the ten members are recipes in `crates/pixlay-core/src/templates/generator.rs`,
`frozen.rs` is regenerated (**127 insertions, 0 deletions** — no shipped byte moved, and the 14043 px
verification render is `cmp`-identical to the build before the step), and the library is **37 templates /
200 slots** over six ratios, with **4:3, 1:1 and a portrait member at every count 2 … 9**. The default
growth chain is 4:3 at every step. Two tests carry the census and the chain
(`crates/pixlay-core/tests/templates.rs`), a CLI test walks the chain from `grid-1-1x1` to nine cells
(`crates/pixlay-cli/tests/cli.rs`), and one renders every shipped layout with a photo
(13.6 s in the debug profile — the per-slot cost is why it draws one cell, not every cell). Two tests
that had pinned the older library moved with it: the switch measurement now walks **every** candidate the
band offers (count 8 has five since S35), and the history walk's round-trip assertion is gated on **both**
keys the count rule reads — the aspect *and* the family — because a 1:1 grid document's walk leaves the
family at three cells and comes back as the grid the library lists first. Every number is
`docs/CONTRACT.md` §8, "S35"; the whole entry — `cargo fmt --check`, `clippy`, `cargo test`,
`reuse lint` — is green.

**3:4 stands as the library's sixth ratio, with five members** (counts 4, 5, 7, 8, 9): the step wrote it
as *proposed* and left it to this step's ruling, and the exit it wrote — "3:4 has at least two members" —
is satisfiable only through the portrait-page members, since the counts that lack a portrait shape would
otherwise need five more members saying the same thing in 2:3. What the walk judges is the ratio itself;
dropping it would be five recipes and a regeneration, nothing else.

**Human**

- The band's legibility walk over the new candidates at the band's own 128x96 (`docs/HIG-REVIEW.md` §2,
  the gallery item): can two arrangements be told apart and a cell's shape read at that size. The step is
  data, but the strip is what that size judges.
- **Pending** (2026-10-16): the ten candidates' sketches at `candidate_grid` are written out for it
  (`/var/tmp/s35probe/sketches/`); the walk in the window is what closes the step.
