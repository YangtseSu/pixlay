<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# S35 · Every count has its canvases

**Progress**: todo — written 2026-10-08 as the plan's first step (`docs/ROADMAP.md`, "More layouts in the
library"); nothing is implemented and no template has been authored yet.

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

**Human**

- The band's legibility walk over the new candidates at the band's own 128x96 (`docs/HIG-REVIEW.md` §2,
  the gallery item): can two arrangements be told apart and a cell's shape read at that size. The step is
  data, but the strip is what that size judges.
