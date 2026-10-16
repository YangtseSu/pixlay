// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The template generator: lattice recipes in, frozen geometry out.
//!
//! Why a generator at all (`docs/CONTRACT.md` §3): the library's geometry has to be
//! reproducible and reviewable, and its invariants — zero overlap, no interior
//! hole, a cut template's areas summing to exactly 1.0 — have to be *decidable*
//! rather than eyeballed. Three decisions make that possible.
//!
//! * **Polygons only.** The S2 review offered "restrict the crop geometry to
//!   polygons, or declare a curve discretization tolerance". Polygons make area,
//!   overlap and holes exact, so there is no tolerance to declare, and no SVG
//!   parser is involved: a path is the command list [`Polygon`] already is.
//! * **A dyadic lattice.** Every recipe is written in integer units of a
//!   power-of-two grid, so `units / grid` is exact in binary floating point and
//!   every area is an exact multiple of `1 / grid²`. "The areas sum to exactly
//!   1.0" is then an equality rather than a tolerance. It also makes the tests'
//!   coverage check exact: a lattice-aligned axis-parallel polygon is a union of
//!   lattice cells, so sampling cell centers cannot miss an overlap or a hole.
//! * **The frozen data is committed, not generated at build time.** A `build.rs`
//!   would rewrite it silently on every build and nobody would review the diff.
//!   Here the recipes below are the source and `templates/frozen.rs` is a
//!   committed artifact: `tests/templates.rs` regenerates it into a scratch file
//!   and compares byte for byte, so the two cannot drift.
//!
//! `AGENTS.md` forbids the layout of an existing project from changing, so a
//! template that ships keeps its name and its geometry version forever; adding a
//! member is a new line here plus the regenerated artifact, and the determinism
//! test makes that regeneration the only way to change the shipped bytes.

use std::fmt::Write as _;

use crate::geometry::{Point, Polygon};
use crate::{Slot, Template};

/// One slot recipe, in lattice units.
///
/// Rectangles are written as rectangles so the recipes stay readable; an
/// irregular slot — the only reason templates have more than one shape — is an
/// explicit point list, counter-clockwise or clockwise as it reads.
enum Shape {
    Rect { x0: u32, y0: u32, x1: u32, y1: u32 },
    Poly(&'static [(u32, u32)]),
}

/// One template recipe.
struct Recipe {
    /// Name, as it appears in `templates`, `init --template` and every error
    /// message: `<family>-<slots>-<variant>`.
    ///
    /// * `strip` — one band: the slots are a single row or a single column.
    /// * `grid` — a rectangular tiling: the same row and column splits
    ///   everywhere, so every slot is a rectangle of the same lattice size.
    /// * `mosaic` — anything else: mixed splits, or a slot that is not a
    ///   rectangle at all.
    ///
    /// The variant is a short mnemonic of the split, and `g` marks a gutter.
    /// `mosaic-8-s14` is the exception to the pattern: it is the template S1
    /// froze and `AGENTS.md`'s verification command names, so its variant tag is
    /// whatever it was then.
    name: &'static str,
    /// Geometry version of this template alone. Bumping one member never
    /// invalidates a project built on another.
    version: u32,
    /// Aspect ratio (`width / height`) the geometry was authored for; the
    /// library is grouped by it and `templates --aspect` queries it.
    aspect: f64,
    /// Lattice resolution. A power of two keeps every coordinate exact.
    grid: u32,
    slots: &'static [Shape],
}

/// A rectangle slot in lattice units.
const fn rect(x0: u32, y0: u32, x1: u32, y1: u32) -> Shape {
    Shape::Rect { x0, y0, x1, y1 }
}

/// Aspect ratios the library uses. Written as divisions of small integers so the
/// value in the data is the ratio a caller can name (`--aspect 4:3`) and not a
/// decimal approximation of it.
const R_1_1: f64 = 1.0;
const R_4_3: f64 = 4.0 / 3.0;
const R_3_2: f64 = 3.0 / 2.0;
const R_16_9: f64 = 16.0 / 9.0;
const R_2_3: f64 = 2.0 / 3.0;
const R_3_4: f64 = 3.0 / 4.0;

/// The whole library, in slot-count order.
///
/// The matrix has to cover every slot count from `MIN_SLOTS` to `MAX_SLOTS`
/// (`docs/CONTRACT.md` §3) and be grouped by aspect ratio, so the
/// recipes read as: which counts, on which canvas shapes. Every aspect ratio in
/// the list is one a printed canvas uses (1:1, 4:3, 3:2, 16:9) or its portrait
/// counterpart (2:3, 3:4).
///
/// Since S10 the counts 2..=9 carry **at least three layouts per count, in at
/// least two aspect families**, because a gallery that offers one candidate is not
/// a choice (asserted by `crates/pixlay-core/tests/templates.rs`). Count 1 has
/// exactly one member since S19: `grid-1-1x1`, the whole sheet, because three
/// one-photo layouts would be three names for one geometry (ruling 34 — a single
/// photo is a legal collage). Since S12c the library stops at nine slots:
/// `strip-10-10x1` was the only member above the format's cap, and it left with
/// the purity ruling, so `MAX_SLOTS` and `MAX_PHOTOS` are one number.
///
/// Since S35 the counts 2..=9 carry **at least four layouts over at least four
/// aspect ratios**, including the album page (4:3), the square (1:1) and a
/// portrait member (2:3 or 3:4): the sheet a document was opened on keeps its shape
/// while the collage grows, because the count rule's first preference is the
/// document's own aspect and every count now has one of its own. `grid-2-2x1` is
/// the member that made the default chain hold — before it, count 2 had no 4:3
/// layout and the first added photo turned the album page into 3:2 and then 16:9.
///
/// Since S37 the library carries two kinds of arrangement the counts did not have
/// before, both **measured rather than named** (`crates/pixlay-core/tests/templates.rs`
/// asserts them over the whole library):
///
/// * a **poster** — the largest cell is at least **twice the median cell**, i.e. one
///   photo dominates the sheet with a mass of half-size cells around it, which is
///   Xiaomi's 海报 reading and what the `-hero` variant names — at **every count
///   2..=9**. Measured when the step started: the hero arrangement and its
///   transcribed twins (`t1b2` / `t1b3` / `t1b4`) satisfy it, as does
///   `mosaic-8-s14` (3.2x), and `mosaic-7-hero` does **not** (its two largest cells
///   tie at 0.1875, 1.2x the median) — its name is frozen and stays, so count 7 got
///   a poster of its own (`mosaic-7-l1r6`) and count 2 got `mosaic-2-hero`.
/// * an **irregular cell** — a slot whose outline is not a rectangle — at counts
///   3, 5, 7 and 8: three are new (`mosaic-3-elbow`, `mosaic-5-tee`,
///   `mosaic-7-elbow`) beside the smoke template's own L, which was the library's
///   only one from S1 to S36.
///
/// Since S38 the library carries **the portrait and square sets**: the human's list of 2026-10-17 asked
/// for one set per sheet shape at every count 2..=9, and the two shapes a phone produces — the portrait
/// page (3:4) and the square (1:1) — are the ones it named as the most used. 38 members joined at 3:4
/// and 35 at 1:1. A member added for one sheet shape carries that shape as a suffix (`-3x4`, `-1x1`),
/// because a layout's natural name is already taken by the shape that first asked for it
/// (`strip-2-1x2` is 2:3, `grid-2-2x1` is 4:3). Every one is appended after its count's existing
/// members, so the growth chain and `layout_for`'s library-order rung are unmoved. The ratio row does
/// move: `ratios_for` leads with 3:4 and 1:1 before the rest ascending, because those are the shapes a
/// person reaches for first (`templates/mod.rs`).
static RECIPES: &[Recipe] = &[
    // The whole sheet, as one cell. The library's first member since S19, when
    // the floor became one photo (ruling 34): a single photo is a legal collage,
    // and its template is the sheet itself — the frame is what gives it a border,
    // which is why this layout's whole canvas is one slot. 4:3, the album page
    // `mosaic-5-hero` was authored for, so a new document opens on the shape the
    // editor used to start at. `grid-` rather than `strip-` or `mosaic-`: a
    // 1x1 tiling is the plainest rectangular one.
    Recipe {
        name: "grid-1-1x1",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[rect(0, 0, 16, 16)],
    },
    // Two rows on a portrait canvas. The simplest layout there is, and the
    // portrait counterpart of `strip-2-2x1`: the aspect query returns one or the
    // other depending on the canvas shape.
    Recipe {
        name: "strip-2-1x2",
        version: 1,
        aspect: R_2_3,
        grid: 16,
        slots: &[rect(0, 0, 16, 8), rect(0, 8, 16, 16)],
    },
    // Two columns on a landscape canvas, split down the middle.
    Recipe {
        name: "strip-2-2x1",
        version: 1,
        aspect: R_3_2,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 16)],
    },
    // The same pair on a square canvas, with a binder's gutter between the panes:
    // they stop a 1/16 canvas width short of each other, so this is the one
    // *strip* that does not tile its canvas and its areas sum to less than 1.0.
    // The gutter runs from the top border to the bottom one, so it is a gutter and
    // not an interior hole — the argument `grid-4-2x2g`'s cross makes, rotated.
    Recipe {
        name: "strip-2-2x1g",
        version: 1,
        aspect: R_1_1,
        grid: 32,
        slots: &[rect(0, 0, 15, 32), rect(17, 0, 32, 32)],
    },
    // Two equal columns on the album page. S35 added it, and it is the member the
    // shape-survives-growth claim hangs on: it is `grid-1-1x1`'s own family and the
    // album page's own aspect, so the first photo a person adds to the window's
    // opening document keeps both. Until then count 2 had no 4:3 member at all, and
    // the growth chain turned the sheet into 3:2 and then 16:9.
    Recipe {
        name: "grid-2-2x1",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 16)],
    },
    // Two panes, one dominant: the poster reading of the smallest collage, and the
    // member count 2 lacked until S37 (the step's measurement: count 3 onward had a
    // `-hero`, and two photos had only equal or near-equal pairs). The split is at
    // 11/16 rather than in the middle — an equal pair *is* `grid-2-2x1` — and 11/5 is
    // the ratio a 16-cell lattice allows that clears the poster property S37 measured
    // and asserts (the largest cell at least twice the *median* one: 0.6875 against
    // 0.3125 is 2.2x, and no other two-cell layout comes near). 4:3, the album page
    // the editor opens on, so the count's poster sits on the sheet a new document
    // already has, and the big pane is nearly square there (0.6875 x 4/3 by 1.0).
    Recipe {
        name: "mosaic-2-hero",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[rect(0, 0, 11, 16), rect(11, 0, 16, 16)],
    },
    // S38: the portrait (3:4) and square (1:1) sets.
    //
    // The human's list of 2026-10-17: one set per sheet shape at every count
    // 2..=9, on the two shapes a phone produces — the portrait page and the
    // square. A member added for one sheet carries it as a suffix (`-3x4`,
    // `-1x1`), because a layout's natural name is already taken by the shape
    // that first needed it (`strip-2-1x2` is 2:3). The splits, once for the
    // whole tranche: a two-cell band is halves; `多/少` splits 11/16 against
    // 5/16, `mosaic-2-hero`'s own 2.2x; a three-cell band 5/6/5 (an equal
    // triple is not dyadic); a four-cell band 4/4/4/4; the seven- and
    // eight-cell three-band rows 5/6/5; the nine-grid 6/5/5, `grid-9-3x3`'s
    // own shape, with a wide-middle twin at 5/6/5 both ways. Every member is
    // appended after its count's existing ones, so the growth chain
    // (`docs/CONTRACT.md` §8, "S35") and the count rule's library-order rung
    // answer what they answered before.
    //
    // Two cells: the two equal splits and the four unequal ones.
    // Two equal rows on the portrait page: `strip-2-1x2`'s own split at the
    // other portrait sheet.
    Recipe {
        name: "strip-2-1x2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 16, 8), rect(0, 8, 16, 16)],
    },
    // Two equal columns on the portrait page.
    Recipe {
        name: "strip-2-2x1-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 16)],
    },
    // The poster pair with the big cell on top: 11/16 over 5/16, the hero's own
    // split with its side turned (`mosaic-2-hero` is the same pair, wide).
    Recipe {
        name: "mosaic-2-hero-top-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 16, 11), rect(0, 11, 16, 16)],
    },
    // The same pair inverted: the bottom cell is the big one.
    Recipe {
        name: "mosaic-2-hero-bottom-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 16, 5), rect(0, 5, 16, 16)],
    },
    // The hero on the left of the portrait page: the pair `mosaic-2-hero`
    // carries on the album page, at 3:4.
    Recipe {
        name: "mosaic-2-hero-left-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 11, 16), rect(11, 0, 16, 16)],
    },
    // And on the right.
    Recipe {
        name: "mosaic-2-hero-right-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 5, 16), rect(5, 0, 16, 16)],
    },
    // The square's two equal rows.
    Recipe {
        name: "strip-2-1x2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 16, 8), rect(0, 8, 16, 16)],
    },
    // The square's two equal columns: the plain twin of `strip-2-2x1g`, whose
    // panes are the same size with a binder's gutter stopping them short.
    Recipe {
        name: "strip-2-2x1-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 16)],
    },
    // The square's poster pair, big cell on top.
    Recipe {
        name: "mosaic-2-hero-top-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 16, 11), rect(0, 11, 16, 16)],
    },
    // ... on the bottom.
    Recipe {
        name: "mosaic-2-hero-bottom-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 16, 5), rect(0, 5, 16, 16)],
    },
    // ... on the left.
    Recipe {
        name: "mosaic-2-hero-left-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 11, 16), rect(11, 0, 16, 16)],
    },
    // ... on the right.
    Recipe {
        name: "mosaic-2-hero-right-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 5, 16), rect(5, 0, 16, 16)],
    },
    // Three columns, unequal: 5/16, 6/16, 5/16. Unequal spans are what makes a
    // strip a layout rather than a repeated pane.
    Recipe {
        name: "strip-3-3x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[rect(0, 0, 5, 16), rect(5, 0, 11, 16), rect(11, 0, 16, 16)],
    },
    // The portrait counterpart of `strip-3-3x1`: three rows, the same unequal
    // 5/16, 6/16, 5/16 spans. Before it a portrait canvas offered no three-photo
    // layout at all.
    Recipe {
        name: "strip-3-1x3",
        version: 1,
        aspect: R_2_3,
        grid: 16,
        slots: &[rect(0, 0, 16, 5), rect(0, 5, 16, 11), rect(0, 11, 16, 16)],
    },
    // A hero panel down the left half with two stacked panels beside it: three
    // photos of two sizes, which is what the `hero` family means.
    Recipe {
        name: "mosaic-3-hero",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 8), rect(8, 8, 16, 16)],
    },
    // One wide panel over two square ones, on the square sheet: `mosaic-3-hero`
    // turned on its side, so the square carries the same one-dominant-panel reading
    // the album page has. A `-hero` at another aspect has no free twin name (the
    // variant word is the arrangement, not its orientation), which is why this one
    // is `t1b2` — top band one, bottom band two.
    Recipe {
        name: "mosaic-3-t1b2",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 16, 8), rect(0, 8, 8, 16), rect(8, 8, 16, 16)],
    },
    // The album page's first irregular cell: an elbow (an L) down the left with a
    // small pane filling its bite, and one pane carrying the whole right half.
    // Count 3's other four members are all arrangements of rectangles, so this is
    // where the library's second cell that is *not* a rectangle arrives (S37; the
    // smoke template's L was the only one before it) — the depth of kind beside
    // S35's depth of shapes. All three slots are lattice-aligned and tile the sheet
    // exactly (80 + 48 + 128 of 256), so the layout is a cut template like every
    // mosaic. The elbow is written as a point list because that is the only shape
    // the recipe vocabulary needs beyond a rectangle, and the pane in the bite is
    // what makes the outline read as an L rather than as a wide left column at the
    // band's 128x96. Measured, this is the one irregular member the poster property
    // does *not* hold for (the elbow is 0.5 of the sheet at most and its median cell
    // is 0.3125, 1.6x): a kind of *cell* and an arrangement are two questions, and
    // this member answers only the first.
    Recipe {
        name: "mosaic-3-elbow",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            Shape::Poly(&[(0, 0), (8, 0), (8, 4), (4, 4), (4, 16), (0, 16)]),
            rect(4, 4, 8, 16),
            rect(8, 0, 16, 16),
        ],
    },
    // Three cells: the two one-over-two splits, the two strips and the two
    // one-beside-two splits (`mosaic-3-t1b2` already ships the square's
    // one-over-two).
    // `mosaic-3-t1b2`'s arrangement at the portrait sheet.
    Recipe {
        name: "mosaic-3-t1b2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 16, 8), rect(0, 8, 8, 16), rect(8, 8, 16, 16)],
    },
    // The same pair inverted: two squares over one band.
    Recipe {
        name: "mosaic-3-b1t2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 8, 8), rect(8, 0, 16, 8), rect(0, 8, 16, 16)],
    },
    // Three rows, 5/6/5: `strip-3-1x3`'s own split at the other portrait sheet.
    Recipe {
        name: "strip-3-1x3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 16, 5), rect(0, 5, 16, 11), rect(0, 11, 16, 16)],
    },
    // Three columns, 5/6/5, on the portrait page.
    Recipe {
        name: "strip-3-3x1-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 5, 16), rect(5, 0, 11, 16), rect(11, 0, 16, 16)],
    },
    // One pane down the left, two stacked beside it: `mosaic-3-hero`'s own
    // arrangement at the portrait page.
    Recipe {
        name: "mosaic-3-l1r2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 8), rect(8, 8, 16, 16)],
    },
    // The same one-beside-two pair mirrored: two stacked on the left, one pane
    // down the right.
    Recipe {
        name: "mosaic-3-r1l2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[rect(0, 0, 8, 8), rect(0, 8, 8, 16), rect(8, 0, 16, 16)],
    },
    // The square's two-squares-over-one-band split: `mosaic-3-t1b2`'s bands
    // inverted.
    Recipe {
        name: "mosaic-3-b1t2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 8, 8), rect(8, 0, 16, 8), rect(0, 8, 16, 16)],
    },
    // The square's three rows, 5/6/5.
    Recipe {
        name: "strip-3-1x3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 16, 5), rect(0, 5, 16, 11), rect(0, 11, 16, 16)],
    },
    // The square's three columns, 5/6/5.
    Recipe {
        name: "strip-3-3x1-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 5, 16), rect(5, 0, 11, 16), rect(11, 0, 16, 16)],
    },
    // The square's one-left-two-right split.
    Recipe {
        name: "mosaic-3-l1r2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 8, 16), rect(8, 0, 16, 8), rect(8, 8, 16, 16)],
    },
    // ... and its mirror.
    Recipe {
        name: "mosaic-3-r1l2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[rect(0, 0, 8, 8), rect(0, 8, 8, 16), rect(8, 0, 16, 16)],
    },
    // The plain 2x2 grid: four equal rectangles.
    Recipe {
        name: "grid-4-2x2",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // 2x2 with a gutter: the slots stop a 1/16 canvas wide gutter short of each
    // split, so the layout is *not* a cut template and its areas sum to less than
    // 1.0 — this is the one member that exercises the non-cut branch of the
    // library's invariants. The gap is a cross that reaches the canvas border in
    // all four directions, which makes it a gutter rather than an interior hole,
    // and the /32 lattice is what puts a slot boundary at 15/32.
    Recipe {
        name: "grid-4-2x2g",
        version: 1,
        aspect: R_1_1,
        grid: 32,
        slots: &[
            rect(0, 0, 15, 15),
            rect(17, 0, 32, 15),
            rect(0, 17, 15, 32),
            rect(17, 17, 32, 32),
        ],
    },
    // Four equal columns on a wide canvas.
    Recipe {
        name: "strip-4-4x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 16),
            rect(4, 0, 8, 16),
            rect(8, 0, 12, 16),
            rect(12, 0, 16, 16),
        ],
    },
    // The hero idea one size up: the left half is one panel, the right half three
    // stacked ones. Count 4 had a plain grid, a guttered grid and a strip — all of
    // them equal panes; this is its first layout with panels of two sizes.
    Recipe {
        name: "mosaic-4-hero",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 16),
            rect(8, 0, 16, 5),
            rect(8, 5, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // `mosaic-4-hero` turned on its side for the portrait page: one wide panel over
    // three, the splits of the bottom band exactly the hero's right column (5/16,
    // 6/16, 5/16). The transposed twin of a rectangle name is free (`grid-6-3x2` /
    // `grid-6-2x3`); a hero's is not, so this one is named by its bands.
    Recipe {
        name: "mosaic-4-t1b3",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // Four cells: the split, the two one-over-three bands, the two strips and
    // the brick (`mosaic-4-t1b3` already ships the portrait page's
    // one-over-three).
    // The plain 2x2 at the portrait page.
    Recipe {
        name: "grid-4-2x2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // A band of three over one wide pane: `mosaic-4-t1b3`'s bands inverted, on
    // the portrait page.
    Recipe {
        name: "mosaic-4-t3b1-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 16, 16),
        ],
    },
    // Four equal rows on the portrait page: `strip-4-4x1`'s split, rotated.
    Recipe {
        name: "strip-4-1x4-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 16, 4),
            rect(0, 4, 16, 8),
            rect(0, 8, 16, 12),
            rect(0, 12, 16, 16),
        ],
    },
    // Four equal columns on the portrait page.
    Recipe {
        name: "strip-4-4x1-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 16),
            rect(4, 0, 8, 16),
            rect(8, 0, 12, 16),
            rect(12, 0, 16, 16),
        ],
    },
    // The running-bond brick: two equal rows, the top split 11/5 and the
    // bottom mirrored 5/11, so no seam lines up.
    Recipe {
        name: "mosaic-4-brick-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 16, 16),
        ],
    },
    // A band over three columns on the square: `mosaic-4-t1b3`'s own
    // arrangement at 1:1.
    Recipe {
        name: "mosaic-4-t1b3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // The square's band of three over one wide pane.
    Recipe {
        name: "mosaic-4-t3b1-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 16, 16),
        ],
    },
    // Four equal rows on the square.
    Recipe {
        name: "strip-4-1x4-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 16, 4),
            rect(0, 4, 16, 8),
            rect(0, 8, 16, 12),
            rect(0, 12, 16, 16),
        ],
    },
    // Four equal columns on the square.
    Recipe {
        name: "strip-4-4x1-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 16),
            rect(4, 0, 8, 16),
            rect(8, 0, 12, 16),
            rect(12, 0, 16, 16),
        ],
    },
    // The brick at the square.
    Recipe {
        name: "mosaic-4-brick-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 16, 16),
        ],
    },
    // A hero panel down the left half with four stacked panels beside it. Every
    // slot is a rectangle, but the splits of the two halves differ, so this is
    // a mixed layout rather than a grid.
    Recipe {
        name: "mosaic-5-hero",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 16),
            rect(8, 0, 16, 4),
            rect(8, 4, 16, 8),
            rect(8, 8, 16, 12),
            rect(8, 12, 16, 16),
        ],
    },
    // Five columns on a wide canvas. A five-way equal split is not a dyadic
    // rational, so a strip of five always has one pane of another size; here the
    // last pane is 4/16 and the other four are 3/16.
    Recipe {
        name: "strip-5-5x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[
            rect(0, 0, 3, 16),
            rect(3, 0, 6, 16),
            rect(6, 0, 9, 16),
            rect(9, 0, 12, 16),
            rect(12, 0, 16, 16),
        ],
    },
    // A top band of three over a bottom band of two, splitting at different
    // points: five photos as a feature row above a pair.
    Recipe {
        name: "mosaic-5-t3b2",
        version: 1,
        aspect: R_3_2,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // Two square panes over three of the strip's unequal ones: no equal tiling of a
    // square holds five cells, so the square's five-photo reading is two bands whose
    // counts differ (2 over 3) — the mirror of `mosaic-5-t3b2`'s idea, on the
    // square. The top band splits down the middle; the bottom one 5/6/5, the unequal
    // spans `strip-3-3x1` and the hero columns use.
    Recipe {
        name: "mosaic-5-t2b3",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // `mosaic-5-hero` turned on its side: one wide panel over four, the portrait
    // page's reading of the same dominant-panel arrangement. The two bands split
    // differently — the top one not at all — which is the mixing the hero's own two
    // halves have; its transpose has no free twin name, so this one is named by its
    // bands.
    Recipe {
        name: "mosaic-5-t1b4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // The square's irregular cell: a tee (a T) with four panes in the two corners
    // under its arms. The T is also the count's second poster — its bar and stem
    // are one cell, 0.5703 of the sheet against a median of 0.1172, 4.9x — so a
    // square five-photo collage can have one big photo that is not a rectangle at
    // all, which is the reading Google's "Shapes" category exists for and the one
    // the in-model vocabulary can actually express. The stem is 6/16 wide and the bar
    // 5/16 tall, both wide enough to survive the band's 96x96 sketch; the four
    // corner panes split their 5/16-wide columns at the same 5/6 line, so the
    // layout reads as one shape over a pair of columns.
    Recipe {
        name: "mosaic-5-tee",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            Shape::Poly(&[
                (0, 0),
                (16, 0),
                (16, 5),
                (11, 5),
                (11, 16),
                (5, 16),
                (5, 5),
                (0, 5),
            ]),
            rect(0, 5, 5, 11),
            rect(0, 11, 5, 16),
            rect(11, 5, 16, 11),
            rect(11, 11, 16, 16),
        ],
    },
    // Five cells: the two three-over-two bands and the two three-beside-two
    // splits.
    // A column of three panes beside a column of two, three on the left.
    Recipe {
        name: "mosaic-5-l3r2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 5),
            rect(0, 5, 8, 11),
            rect(0, 11, 8, 16),
            rect(8, 0, 16, 8),
            rect(8, 8, 16, 16),
        ],
    },
    // ... three on the right.
    Recipe {
        name: "mosaic-5-r3l2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(0, 8, 8, 16),
            rect(8, 0, 16, 5),
            rect(8, 5, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // A band of three over a band of two: `mosaic-5-t3b2`'s own bands at the
    // portrait page.
    Recipe {
        name: "mosaic-5-t3b2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // The same pair inverted: the band of three below.
    Recipe {
        name: "mosaic-5-b3t2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // The square's three-left-two-right split.
    Recipe {
        name: "mosaic-5-l3r2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 5),
            rect(0, 5, 8, 11),
            rect(0, 11, 8, 16),
            rect(8, 0, 16, 8),
            rect(8, 8, 16, 16),
        ],
    },
    // The square's pair with three on the right.
    Recipe {
        name: "mosaic-5-r3l2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(0, 8, 8, 16),
            rect(8, 0, 16, 5),
            rect(8, 5, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // The square's band of three over two.
    Recipe {
        name: "mosaic-5-t3b2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // ... and two over three.
    Recipe {
        name: "mosaic-5-b3t2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // 3x2, unequal columns (5/16, 6/16, 5/16) and equal rows.
    Recipe {
        name: "grid-6-3x2",
        version: 1,
        aspect: R_3_2,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // Six as a hero pair: the left half is two stacked panels, the right half four.
    Recipe {
        name: "mosaic-6-hero",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(0, 8, 8, 16),
            rect(8, 0, 16, 4),
            rect(8, 4, 16, 8),
            rect(8, 8, 16, 12),
            rect(8, 12, 16, 16),
        ],
    },
    // Six columns on a wide canvas: the two outer panes 2/16, the four middle
    // ones 3/16.
    Recipe {
        name: "strip-6-6x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[
            rect(0, 0, 2, 16),
            rect(2, 0, 5, 16),
            rect(5, 0, 8, 16),
            rect(8, 0, 11, 16),
            rect(11, 0, 14, 16),
            rect(14, 0, 16, 16),
        ],
    },
    // The `grid-6-3x2` tiling turned on its side for a portrait canvas: two
    // columns of equal width, three rows spanning 6/16, 5/16, 5/16.
    Recipe {
        name: "grid-6-2x3",
        version: 1,
        aspect: R_2_3,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 6),
            rect(8, 0, 16, 6),
            rect(0, 6, 8, 11),
            rect(8, 6, 16, 11),
            rect(0, 11, 8, 16),
            rect(8, 11, 16, 16),
        ],
    },
    // Two rows of three on the square, the rows splitting a little differently
    // (5/6/5 over 6/5/5): the count's own three-by-two reading — `grid-6-3x2` is the
    // equal tiling of it, on 3:2 — whose two rows give the square sheet the shape
    // its six photos want. The differing splits are what make it a mosaic and not a
    // grid.
    Recipe {
        name: "mosaic-6-t3b3",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 6, 16),
            rect(6, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // Six cells: the two three-and-three splits and the four bands whose counts
    // differ (`mosaic-6-t3b3` already ships the square's three-over-three).
    // Three panes beside three, the columns' rows splitting 5/6/5 and 6/5/5:
    // the transposed reading of `mosaic-6-t3b3`, whose bands do the same.
    Recipe {
        name: "mosaic-6-l3r3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 5),
            rect(0, 5, 8, 11),
            rect(0, 11, 8, 16),
            rect(8, 0, 16, 6),
            rect(8, 6, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // Three over three on the portrait page, the rows split the way
    // `mosaic-6-t3b3`'s bands are.
    Recipe {
        name: "mosaic-6-t3b3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 6, 16),
            rect(6, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // A band of two over a band of four.
    Recipe {
        name: "mosaic-6-t2b4-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // Two panes beside four.
    Recipe {
        name: "mosaic-6-l2r4-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(0, 8, 8, 16),
            rect(8, 0, 16, 4),
            rect(8, 4, 16, 8),
            rect(8, 8, 16, 12),
            rect(8, 12, 16, 16),
        ],
    },
    // Four panes beside two.
    Recipe {
        name: "mosaic-6-l4r2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 4),
            rect(0, 4, 8, 8),
            rect(0, 8, 8, 12),
            rect(0, 12, 8, 16),
            rect(8, 0, 16, 8),
            rect(8, 8, 16, 16),
        ],
    },
    // A band of four over a band of two.
    Recipe {
        name: "mosaic-6-t4b2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // The square's three beside three: its three-over-three already ships as
    // `mosaic-6-t3b3`, and this is the same reading turned on its side.
    Recipe {
        name: "mosaic-6-l3r3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 5),
            rect(0, 5, 8, 11),
            rect(0, 11, 8, 16),
            rect(8, 0, 16, 6),
            rect(8, 6, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // The square's band of two over a band of four.
    Recipe {
        name: "mosaic-6-t2b4-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(8, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // The square's two beside four.
    Recipe {
        name: "mosaic-6-l2r4-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 8),
            rect(0, 8, 8, 16),
            rect(8, 0, 16, 4),
            rect(8, 4, 16, 8),
            rect(8, 8, 16, 12),
            rect(8, 12, 16, 16),
        ],
    },
    // The square's four beside two.
    Recipe {
        name: "mosaic-6-l4r2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 4),
            rect(0, 4, 8, 8),
            rect(0, 8, 8, 12),
            rect(0, 12, 8, 16),
            rect(8, 0, 16, 8),
            rect(8, 8, 16, 16),
        ],
    },
    // The square's band of four over a band of two.
    Recipe {
        name: "mosaic-6-t4b2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 8, 16),
            rect(8, 8, 16, 16),
        ],
    },
    // A top band of four over a bottom band of three: the two bands split at
    // different points, which is the second thing `mosaic` covers.
    Recipe {
        name: "mosaic-7-t4b3",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // Seven columns on a wide canvas: five 2/16 panes and two 3/16 ones, placed so
    // that no two wide panes are adjacent.
    Recipe {
        name: "strip-7-7x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[
            rect(0, 0, 2, 16),
            rect(2, 0, 5, 16),
            rect(5, 0, 7, 16),
            rect(7, 0, 10, 16),
            rect(10, 0, 12, 16),
            rect(12, 0, 14, 16),
            rect(14, 0, 16, 16),
        ],
    },
    // Seven as a three-column mosaic: the 5/16 left column is three panels, the
    // 6/16 middle and the 5/16 right are two each, so the two rows of the middle
    // and right columns read as one band across the layout.
    Recipe {
        name: "mosaic-7-hero",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 6),
            rect(0, 6, 5, 11),
            rect(0, 11, 5, 16),
            rect(5, 0, 11, 8),
            rect(5, 8, 11, 16),
            rect(11, 0, 16, 8),
            rect(11, 8, 16, 16),
        ],
    },
    // The transpose of `mosaic-7-t4b3`: four panes down the left and three down the
    // right, the right column's rows spanning 5/16, 6/16, 5/16 as the original's
    // bottom band's columns do. The portrait page's reading of the same
    // four-over-three arrangement.
    Recipe {
        name: "mosaic-7-l4r3",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 4),
            rect(0, 4, 8, 8),
            rect(0, 8, 8, 12),
            rect(0, 12, 8, 16),
            rect(8, 0, 16, 5),
            rect(8, 5, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // Seven as a left slab over a 2x3 block of small panes: `mosaic-9-hero`'s own
    // arrangement one size down (the nine-cell hero is this with a 2x4 block), and
    // the poster count 7 needed once the property was measured. `mosaic-7-hero`
    // carries the name, but its two largest cells tie at 0.1875 and its median cell
    // is 0.15625 — 1.2x, a three-column mosaic, which is not a poster by the
    // definition S37 measured and asserts (the largest cell at least twice the
    // median one). Its name is frozen data and stays; this member is the count's
    // poster, named by its bands the way a transcribed hero is (`mosaic-9-l4r5`):
    // one pane down the left, six on the right. The block's rows split 5/16, 6/16,
    // 5/16, so each small pane is nearly square on the album page, and the layout
    // measures 0.5 against a median of 0.078125 — 6.4x, a poster by a wide margin.
    Recipe {
        name: "mosaic-7-l1r6",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 16),
            rect(8, 0, 12, 5),
            rect(12, 0, 16, 5),
            rect(8, 5, 12, 11),
            rect(12, 5, 16, 11),
            rect(8, 11, 12, 16),
            rect(12, 11, 16, 16),
        ],
    },
    // The portrait page's irregular cell, and the second elbow: the same L as
    // `mosaic-3-elbow` down the left, with the three small panes stacked in its
    // bite and three more filling the right column. It came with the same step
    // because the sketch is what decides whether an L reads at all — at 3:4 the
    // candidate is drawn 72x96, the band's narrowest box, and the bite is 4/16 of
    // the width there. The elbow is 0.3125 of the sheet against a median of
    // 0.15625, exactly 2.0x, so this member satisfies the poster property *and*
    // carries an irregular cell: the two questions are independent, which is what
    // measuring rather than naming shows, and `mosaic-3-elbow` is the irregular one
    // that is not a poster.
    Recipe {
        name: "mosaic-7-elbow",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            Shape::Poly(&[(0, 0), (8, 0), (8, 4), (4, 4), (4, 16), (0, 16)]),
            rect(4, 4, 8, 8),
            rect(4, 8, 8, 12),
            rect(4, 12, 8, 16),
            rect(8, 0, 16, 5),
            rect(8, 5, 16, 11),
            rect(8, 11, 16, 16),
        ],
    },
    // Seven cells: the two four-over-three bands, the three band layouts whose
    // middle band differs, and the offset twin the human's list asked for
    // beside the plain one (ruling 2026-10-17: two layouts, the second with its
    // top row split 11/5 and its bottom mirrored).
    // `mosaic-7-t4b3`'s own bands at the portrait page.
    Recipe {
        name: "mosaic-7-t4b3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // The same pair inverted: three over four.
    Recipe {
        name: "mosaic-7-t3b4-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // Three cells, then two, then two: the lower bands splitting down their
    // middles.
    Recipe {
        name: "mosaic-7-t3m2b2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 8, 11),
            rect(8, 5, 16, 11),
            rect(0, 11, 8, 16),
            rect(8, 11, 16, 16),
        ],
    },
    // Three cells over one wide pane over three.
    Recipe {
        name: "mosaic-7-t3m1b3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // Two over three over two.
    Recipe {
        name: "mosaic-7-t2m3b2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 5),
            rect(8, 0, 16, 5),
            rect(0, 5, 5, 11),
            rect(5, 5, 11, 11),
            rect(11, 5, 16, 11),
            rect(0, 11, 8, 16),
            rect(8, 11, 16, 16),
        ],
    },
    // The same bands with their outer rows split 11/5 and 5/11: the brick
    // reading of the two-three-two, its seams running the other way.
    Recipe {
        name: "mosaic-7-t2m3b2-brick-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 5, 11),
            rect(5, 5, 11, 11),
            rect(11, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 16, 16),
        ],
    },
    // The square's four over three.
    Recipe {
        name: "mosaic-7-t4b3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // ... and three over four.
    Recipe {
        name: "mosaic-7-t3b4-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 8),
            rect(5, 0, 11, 8),
            rect(11, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // The square's three over two over two.
    Recipe {
        name: "mosaic-7-t3m2b2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 8, 11),
            rect(8, 5, 16, 11),
            rect(0, 11, 8, 16),
            rect(8, 11, 16, 16),
        ],
    },
    // The square's three over one over three.
    Recipe {
        name: "mosaic-7-t3m1b3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // The square's two over three over two.
    Recipe {
        name: "mosaic-7-t2m3b2-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 5),
            rect(8, 0, 16, 5),
            rect(0, 5, 5, 11),
            rect(5, 5, 11, 11),
            rect(11, 5, 16, 11),
            rect(0, 11, 8, 16),
            rect(8, 11, 16, 16),
        ],
    },
    // The square's two-three-two offset twin.
    Recipe {
        name: "mosaic-7-t2m3b2-brick-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 5, 11),
            rect(5, 5, 11, 11),
            rect(11, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 16, 16),
        ],
    },
    // The template S1 froze: `AGENTS.md`'s per-round verification command names
    // it and `docs/CONTRACT.md` §1 uses it as the example, so its name, its
    // version, its aspect, its slot order and its coordinates are all frozen —
    // S2 rewrote how the geometry is *produced*, not what it is. One slot is
    // concave (an L), which is what keeps the clip path, the probes' deepest
    // point search and the area sums non-trivial.
    Recipe {
        name: "mosaic-8-s14",
        version: 1,
        aspect: R_4_3,
        grid: 8,
        slots: &[
            rect(0, 0, 3, 3),
            rect(0, 3, 3, 5),
            rect(0, 5, 3, 8),
            rect(3, 0, 4, 3),
            rect(4, 0, 6, 3),
            rect(6, 0, 8, 3),
            Shape::Poly(&[(3, 3), (8, 3), (8, 5), (6, 5), (6, 8), (3, 8)]),
            rect(6, 5, 8, 8),
        ],
    },
    // Eight equal columns on a wide canvas: 2/16 each, the only way eight panes fit
    // a 16-cell lattice exactly. The panorama counterpart of `grid-8-4x2`.
    Recipe {
        name: "strip-8-8x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[
            rect(0, 0, 2, 16),
            rect(2, 0, 4, 16),
            rect(4, 0, 6, 16),
            rect(6, 0, 8, 16),
            rect(8, 0, 10, 16),
            rect(10, 0, 12, 16),
            rect(12, 0, 14, 16),
            rect(14, 0, 16, 16),
        ],
    },
    // 4x2, all eight panes equal: the plain eight-photo grid, and the reference
    // the concave smoke template is not.
    Recipe {
        name: "grid-8-4x2",
        version: 1,
        aspect: R_3_2,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // Two rows of four on the square, the lower row splitting 5/3/3/5 — the grid's
    // own eight-photo reading, which only `grid-8-4x2` carried before and only on
    // 3:2. The differing splits make it a mosaic; equal ones would be a grid.
    Recipe {
        name: "mosaic-8-t4b4",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 5, 16),
            rect(5, 8, 8, 16),
            rect(8, 8, 11, 16),
            rect(11, 8, 16, 16),
        ],
    },
    // The transposed twin of `grid-8-4x2` — the relationship `grid-6-3x2` /
    // `grid-6-2x3` and `strip-2-2x1` / `strip-2-1x2` already have — two columns of
    // four, for the portrait page, where a 2x4 grid's panes come out landscape.
    Recipe {
        name: "grid-8-2x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 4),
            rect(8, 0, 16, 4),
            rect(0, 4, 8, 8),
            rect(8, 4, 16, 8),
            rect(0, 8, 8, 12),
            rect(8, 8, 16, 12),
            rect(0, 12, 8, 16),
            rect(8, 12, 16, 16),
        ],
    },
    // Eight cells: the two four-and-four tilings and the two three-two-three
    // band layouts (`grid-8-2x4` already ships the portrait page's
    // four-and-four, `mosaic-8-t4b4` the square's).
    // Four over four on the portrait page: the transposed twin of `grid-8-2x4`.
    Recipe {
        name: "grid-8-4x2-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 4, 8),
            rect(4, 0, 8, 8),
            rect(8, 0, 12, 8),
            rect(12, 0, 16, 8),
            rect(0, 8, 4, 16),
            rect(4, 8, 8, 16),
            rect(8, 8, 12, 16),
            rect(12, 8, 16, 16),
        ],
    },
    // Three over two over three: the middle band's two panes wider and taller.
    Recipe {
        name: "mosaic-8-t3m2b3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 8, 11),
            rect(8, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // Three beside two beside three.
    Recipe {
        name: "mosaic-8-l3m2r3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(0, 5, 5, 11),
            rect(0, 11, 5, 16),
            rect(5, 0, 11, 8),
            rect(5, 8, 11, 16),
            rect(11, 0, 16, 5),
            rect(11, 5, 16, 11),
            rect(11, 11, 16, 16),
        ],
    },
    // Four beside four on the square.
    Recipe {
        name: "grid-8-2x4-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 4),
            rect(0, 4, 8, 8),
            rect(0, 8, 8, 12),
            rect(0, 12, 8, 16),
            rect(8, 0, 16, 4),
            rect(8, 4, 16, 8),
            rect(8, 8, 16, 12),
            rect(8, 12, 16, 16),
        ],
    },
    // The square's three over two over three.
    Recipe {
        name: "mosaic-8-t3m2b3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 8, 11),
            rect(8, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // The square's three beside two beside three.
    Recipe {
        name: "mosaic-8-l3m2r3-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(0, 5, 5, 11),
            rect(0, 11, 5, 16),
            rect(5, 0, 11, 8),
            rect(5, 8, 11, 16),
            rect(11, 0, 16, 5),
            rect(11, 5, 16, 11),
            rect(11, 11, 16, 16),
        ],
    },
    // 3x3, unequal columns and rows (6/16, 5/16, 5/16) each way.
    Recipe {
        name: "grid-9-3x3",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 6, 6),
            rect(6, 0, 11, 6),
            rect(11, 0, 16, 6),
            rect(0, 6, 6, 11),
            rect(6, 6, 11, 11),
            rect(11, 6, 16, 11),
            rect(0, 11, 6, 16),
            rect(6, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // Nine columns on a wide canvas: six 2/16 panes, then 1/16, 1/16, 2/16, which
    // is how an odd number of panes splits on a dyadic lattice.
    Recipe {
        name: "strip-9-9x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[
            rect(0, 0, 2, 16),
            rect(2, 0, 4, 16),
            rect(4, 0, 6, 16),
            rect(6, 0, 8, 16),
            rect(8, 0, 10, 16),
            rect(10, 0, 12, 16),
            rect(12, 0, 13, 16),
            rect(13, 0, 14, 16),
            rect(14, 0, 16, 16),
        ],
    },
    // Nine as one hero panel with a 2x4 block of small ones beside it: the two
    // halves split differently, which is what makes it a mosaic rather than a
    // grid.
    Recipe {
        name: "mosaic-9-hero",
        version: 1,
        aspect: R_4_3,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 16),
            rect(8, 0, 12, 4),
            rect(12, 0, 16, 4),
            rect(8, 4, 12, 8),
            rect(12, 4, 16, 8),
            rect(8, 8, 12, 12),
            rect(12, 8, 16, 12),
            rect(8, 12, 12, 16),
            rect(12, 12, 16, 16),
        ],
    },
    // Four panes down the left and five down the right, on the portrait page: the
    // two-column reading of nine, since a 3x3 tiling's name is taken by the square
    // one and nine rows in a single column would be a filmstrip. The five-row column
    // puts its tallest pane in the middle (3/16, 3/16, 4/16, 3/16, 3/16), the way
    // `mosaic-7-t4b3`'s bottom band does.
    Recipe {
        name: "mosaic-9-l4r5",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 8, 4),
            rect(0, 4, 8, 8),
            rect(0, 8, 8, 12),
            rect(0, 12, 8, 16),
            rect(8, 0, 16, 3),
            rect(8, 3, 16, 6),
            rect(8, 6, 16, 10),
            rect(8, 10, 16, 13),
            rect(8, 13, 16, 16),
        ],
    },
    // Nine cells: the nine-grid and its wide-middle twin (`grid-9-3x3` already
    // ships the square's own nine-grid).
    // `grid-9-3x3`'s splits (6/5/5 each way) at the portrait page.
    Recipe {
        name: "grid-9-3x3-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 6, 6),
            rect(6, 0, 11, 6),
            rect(11, 0, 16, 6),
            rect(0, 6, 6, 11),
            rect(6, 6, 11, 11),
            rect(11, 6, 16, 11),
            rect(0, 11, 6, 16),
            rect(6, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // The nine-grid with its middle row and column a little wider (5/6/5).
    Recipe {
        name: "grid-9-3x3-wide-middle-3x4",
        version: 1,
        aspect: R_3_4,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 5, 11),
            rect(5, 5, 11, 11),
            rect(11, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
    // The square's wide-middle nine-grid.
    Recipe {
        name: "grid-9-3x3-wide-middle-1x1",
        version: 1,
        aspect: R_1_1,
        grid: 16,
        slots: &[
            rect(0, 0, 5, 5),
            rect(5, 0, 11, 5),
            rect(11, 0, 16, 5),
            rect(0, 5, 5, 11),
            rect(5, 5, 11, 11),
            rect(11, 5, 16, 11),
            rect(0, 11, 5, 16),
            rect(5, 11, 11, 16),
            rect(11, 11, 16, 16),
        ],
    },
];

/// Builds every template the recipes describe, in recipe order.
///
/// Pure: the result is a function of the constants above alone, with no clock,
/// no filesystem and no iteration order to depend on, which is what "generation
/// must be deterministic" means here.
pub fn generate() -> Vec<Template> {
    RECIPES.iter().map(build).collect()
}

fn build(recipe: &Recipe) -> Template {
    debug_assert!(
        recipe.grid.is_power_of_two(),
        "{}: a grid that is not a power of two loses exactness",
        recipe.name
    );
    let unit = 1.0 / f64::from(recipe.grid);
    let slots = recipe
        .slots
        .iter()
        .map(|shape| {
            let outline = match shape {
                Shape::Rect { x0, y0, x1, y1 } => {
                    Polygon::rect(at(*x0, unit), at(*y0, unit), at(*x1, unit), at(*y1, unit))
                }
                Shape::Poly(points) => Polygon {
                    points: points
                        .iter()
                        .map(|&(x, y)| Point::new(at(x, unit), at(y, unit)))
                        .collect(),
                },
            };
            Slot {
                // The declared area is the outline's own area, computed from the
                // same geometry: the two cannot drift apart, and the frozen file
                // records the value the tests then verify independently.
                area: outline.area(),
                outline,
            }
        })
        .collect();
    Template {
        name: recipe.name.to_string(),
        version: recipe.version,
        aspect: recipe.aspect,
        slots,
    }
}

/// One lattice coordinate, normalized.
fn at(units: u32, unit: f64) -> f64 {
    f64::from(units) * unit
}

/// Renders `templates` as the source of `templates/frozen.rs`.
///
/// The output is valid Rust and byte-identical for equal input, so writing it to
/// the committed path and comparing the file with a fresh rendering are the same
/// statement. Floats use `{:?}`, the shortest representation that reads back
/// exactly, so a coordinate cannot drift through decimal rounding.
pub fn emit_source(templates: &[Template]) -> String {
    let mut out = String::with_capacity(16 * 1024);
    out.push_str(HEADER);
    for template in templates {
        let _ = writeln!(out, "    Frozen {{");
        let _ = writeln!(out, "        name: {:?},", template.name);
        let _ = writeln!(out, "        version: {},", template.version);
        let _ = writeln!(out, "        aspect: {:?},", template.aspect);
        let _ = writeln!(out, "        slots: &[");
        for slot in &template.slots {
            let points: Vec<String> = slot
                .outline
                .points
                .iter()
                .map(|point| format!("({:?}, {:?})", point.x, point.y))
                .collect();
            let _ = writeln!(
                out,
                "            (&[{}], {:?}),",
                points.join(", "),
                slot.area
            );
        }
        let _ = writeln!(out, "        ],");
        let _ = writeln!(out, "    }},");
    }
    out.push_str("];\n");
    out
}

/// Everything above the template table. Kept next to the emitter so the file's
/// prose, its data and its own license tags are regenerated by the same command —
/// the file is what `reuse lint` reads (`AGENTS.md`, "Distribution").
const HEADER: &str = "\
// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Frozen template geometry: the data a build ships.
//!
//! GENERATED FILE — do not edit by hand. Regenerate with:
//!
//! ```text
//! cargo run -p pixlay-core --bin pixlay-gen-templates
//! ```
//!
//! The recipes live in `templates/generator.rs`, and the determinism test in
//! `crates/pixlay-core/tests/templates.rs` regenerates this file into a scratch
//! path and compares it byte for byte, so the recipe and the geometry a build
//! ships can never drift apart. Coordinates are normalized to `[0, 1]`
//! (`AGENTS.md`) and are exact multiples of `1 / grid` for the grid the recipe
//! declared, which is why the areas are exact binary values too.

/// One frozen template: its name, geometry version, aspect ratio and slots.
pub struct Frozen {
    pub name: &'static str,
    /// Geometry version of this template. A document embeds it, so it must not
    /// change while any project built on it exists.
    pub version: u32,
    /// Aspect ratio (`width / height`) the geometry was authored for.
    pub aspect: f64,
    /// Slots in template order: outline points and the declared area.
    pub slots: &'static [(&'static [(f64, f64)], f64)],
}

/// Every template this build ships, in recipe order.
#[rustfmt::skip]
pub const TEMPLATES: &[Frozen] = &[
";
