// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The template generator: arrangements in, frozen geometry out.
//!
//! Since S39 the library is a **matrix**: one [`Arrangement`] — one lattice geometry
//! and the name it is known by — authored at the **sheets** it exists on, and the
//! six sheet shapes are one documented sequence ([`SHEETS`]). Every set arrangement
//! exists at all six sheets, so "the same arrangement at six sheet shapes" is one
//! source row rather than six near-copies — the geometry does not depend on the
//! sheet, because the normalized geometry is stretched onto that sheet's aspect.
//! The matrix is written in the human's order (their list of arrangements, then the
//! members the list does not name, then the irregular cells), and that order **is**
//! the library's order: a strip reads the same way at every sheet, which is the
//! S39 ruling ("所有的比例内的模板，按照一个顺序来；异形的放到最后").
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
//! * **A dyadic lattice.** Every arrangement is written in integer units of
//!   `128` — exact in binary floating point, every area a multiple of `1/16384` —
//!   so "the areas sum to exactly 1.0" is an equality rather than a tolerance. It
//!   also makes the tests' coverage check exact: a lattice-aligned axis-parallel
//!   polygon is a union of lattice cells, so sampling cell centers cannot miss an
//!   overlap or a hole. (The first members were authored on 16 or 32; 128 was the
//!   finest an S38 shape needed, and S39 put every arrangement on it.)
//! * **The frozen data is committed, not generated at build time.** A `build.rs`
//!   would rewrite it silently on every build and nobody would review the diff.
//!   Here the arrangements below are the source and `templates/frozen.rs` is a
//!   committed artifact: `tests/templates.rs` regenerates it into a scratch file
//!   and compares byte for byte, so the two cannot drift.
//!
//! `AGENTS.md` forbids the layout of an existing project from changing, so a member
//! that ships keeps its name, its `version` and its geometry forever; adding a
//! member is a line here plus the regenerated artifact, and the determinism test
//! makes that regeneration the only way to change the shipped bytes. The **name** of
//! a member is `<variant>-<sheet tag>` unless its sheet has an override — a member
//! that predates the tagging rule keeps the name it shipped with (`grid-2-2x1` *is*
//! the 4:3 member of the two-equal-columns arrangement; `mosaic-3-hero` is the 4:3
//! member of left-one-right-two) — and the tags are the ratio row's own vocabulary
//! (`3x4`, `1x1`, `2x3`, `4x3`, `3x2`, `16x9`), which `templates::ratio_label`
//! reads back as `W:H`.

use std::fmt::Write as _;

use crate::geometry::{Point, Polygon};
use crate::{Slot, Template};

/// One slot recipe, in lattice units of [`GRID`].
///
/// Rectangles are written as rectangles so the recipes stay readable; an
/// irregular slot — the only reason templates have more than one shape — is an
/// explicit point list, counter-clockwise or clockwise as it reads.
enum Shape {
    Rect { x0: u32, y0: u32, x1: u32, y1: u32 },
    Poly(&'static [(u32, u32)]),
}

impl Shape {
    /// This shape's outline, at `unit` (one lattice step, normalized).
    ///
    /// The one conversion from lattice units to the document's `[0,1]` geometry, so
    /// the arrangement recipes and the ID photo sheets cannot spell it differently.
    fn outline(&self, unit: f64) -> Polygon {
        match self {
            Self::Rect { x0, y0, x1, y1 } => {
                Polygon::rect(at(*x0, unit), at(*y0, unit), at(*x1, unit), at(*y1, unit))
            }
            Self::Poly(points) => Polygon {
                points: points
                    .iter()
                    .map(|&(x, y)| Point::new(at(x, unit), at(y, unit)))
                    .collect(),
            },
        }
    }
}

/// One template arrangement: a geometry, the sheets it is authored at, and the
/// members whose shipped names predate the tagging rule.
struct Arrangement {
    /// The name without a sheet tag (`strip-2-1x2`, `mosaic-2-hero-top`). A member
    /// not named in [`Arrangement::names`] is this with `-<sheet tag>` appended.
    variant: &'static str,
    /// The geometry, in units of [`GRID`]. One geometry serves every sheet.
    slots: &'static [Shape],
    /// The sheets it is authored at, in [`SHEETS`] order — the order the members
    /// are emitted in, and `build` asserts it.
    sheets: &'static [&'static str],
    /// Sheet tag to shipped name, for members whose names are not `<variant>-<tag>`.
    names: &'static [(&'static str, &'static str)],
}

/// The lattice every arrangement is written on: a power of two, so every
/// coordinate and every area is exact in binary floating point.
const GRID: u32 = 128;

/// All six sheets an arrangement is authored at, in the row's documented order
/// (`templates::ratios_for`: 3:4, 1:1, 2:3, 4:3, 3:2, 16:9). Only the members the
/// list does not name have fewer.
const SET: &[&str] = &["3x4", "1x1", "2x3", "4x3", "3x2", "16x9"];

/// The sheet tags and their aspects (`width / height`), written as divisions of
/// small integers so the value in the data is the ratio a caller can name
/// (`--aspect 4:3`) and not a decimal approximation of it.
const SHEETS: &[(&str, f64)] = &[
    ("3x4", 3.0 / 4.0),
    ("1x1", 1.0),
    ("2x3", 2.0 / 3.0),
    ("4x3", 4.0 / 3.0),
    ("3x2", 3.0 / 2.0),
    ("16x9", 16.0 / 9.0),
];

/// A rectangle slot in lattice units.
const fn rect(x0: u32, y0: u32, x1: u32, y1: u32) -> Shape {
    Shape::Rect { x0, y0, x1, y1 }
}

/// The ID photo sheets' one physical page (S40), upright: 4R is exactly **4 x 6
/// inch = 101.6 x 152.4 mm**, whose aspect is `2/3` — the `2x3` tag — and the turned
/// page is the same paper's `3/2`, the `3x2` tag. The nominal Chinese "6 inch"
/// (102 x 152 mm) is 0.6% larger; the paper is the inch one.
///
/// **Which way a size is printed is the recipe's own decision** ([`IdSheet::landscape`]),
/// because the product's target is the print and the paper has no upright side: what
/// decides is how many copies fit, which is what the human's ruling of 2026-10-17 asked
/// for (「那几张大的图片，有点浪费纸张」). Measured over the six sizes, turning the page
/// is what fills it for four of them.
///
/// This is the only place in the tree where a sheet's millimetres are a *decision*:
/// the document, the CLI and the export keep the format's rule (ratios and one
/// long-edge pixel count), and the frozen geometry below is the normalized value
/// this constant converts into.
const ID_SHEET_MM: (f64, f64) = (101.6, 152.4);

/// The lattice the ID sheets are authored on: 1/2560 of the page's width, so one
/// step is 0.0397 mm across and 0.0595 mm down. 128 (the collage arrangements'
/// lattice) would leave a cell up to 0.15 mm off its nominal size; 2560 keeps every
/// one within 0.03 mm and within 0.6 px at the 1800-px long edge a 300 dpi print
/// asks for (`docs/CONTRACT.md` §8, S40).
const ID_GRID: u32 = 2560;

/// One ID photo sheet: cells of one size on the 4R page, as many as fit, centred.
struct IdSheet {
    /// The size's own word, which the template's name carries
    /// (`id-<size>-<slots>-<sheet tag>`). Machine vocabulary: the UI's size names are
    /// `gettext` strings keyed by the member's name (`crates/pixlay`'s
    /// `photo_size_name`), not this word.
    size: &'static str,
    /// The cell's nominal size, in millimetres, **upright** — the photo keeps the
    /// orientation it is cut to, so a sheet never turns a copy.
    cell_mm: (f64, f64),
    /// Whether this size is printed on the turned page (`3x2`): the same paper, the
    /// cells still upright, and more copies when the row fits better that way.
    landscape: bool,
}

/// The white between two neighbouring cells, in millimetres, on every ID sheet: a
/// cut line, 23.6 px at 300 dpi.
const ID_GAP_MM: f64 = 2.0;

/// The ID photo sheets, in the human's list order (S40 ruling of 2026-10-17).
///
/// The five the human named, plus **大二寸** — the other "standard" two-inch size the
/// research found (35 x 53 mm, the 413 x 626 px one the sources call the standard
/// 二寸), so the set carries both of the sizes a studio is asked for. The counts are
/// the largest that fit, because the sheet is printed borderless (the same ruling).
/// The six sizes, **smallest photo first** (the human's ruling of 2026-10-17:
/// 「按照照片尺寸大小排序，从小到大」). The order is the library's and therefore the
/// strip's: 小一寸 22x32, 一寸 25x35, 小二寸 35x45, 大一寸 33x48, 二寸 35x49 and
/// 大二寸 35x53 — the height is strictly ascending (32, 35, 45, 48, 49, 53) and so is
/// the area (704, 875, 1575, 1584, 1715, 1855 mm²), so the two readings of "size"
/// agree and neither has to be chosen.
///
/// Each size is authored on **the page orientation that holds more copies** (measured
/// at the 2 mm gap): 一寸 12 upright against 10 turned; 小一寸 18 against 16; 大一寸,
/// 二寸 and 小二寸 8 against 6; and 大二寸 4 either way, where the upright 2x2 is the
/// tidier one. A mixed sheet — a landscape row beside a portrait one — is not
/// expressible: every copy draws the document's one cell, so they share one framing
/// and one orientation, and the leftovers on these pages are too narrow for a turned
/// cell in any case (二寸's widest is 6.4 mm against the 48 mm a turned cell needs).
static ID_SHEETS: &[IdSheet] = &[
    IdSheet {
        size: "1cun-small",
        cell_mm: (22.0, 32.0),
        landscape: true,
    },
    IdSheet {
        size: "1cun",
        cell_mm: (25.0, 35.0),
        landscape: false,
    },
    IdSheet {
        size: "2cun-small",
        cell_mm: (35.0, 45.0),
        landscape: true,
    },
    IdSheet {
        size: "1cun-large",
        cell_mm: (33.0, 48.0),
        landscape: true,
    },
    IdSheet {
        size: "2cun",
        cell_mm: (35.0, 49.0),
        landscape: true,
    },
    IdSheet {
        size: "2cun-large",
        cell_mm: (35.0, 53.0),
        landscape: false,
    },
];

/// The whole library, in the canonical order (S39): per slot count, the human's
/// arrangements in the human's list order, then the members the list does not
/// name, then the irregular members last; within one arrangement, the sheets in
/// [`SHEETS`] order. The order is the library's own — `templates::all` and every
/// query over it read in it, and a strip shows its members in it.
static ARRANGEMENTS: &[Arrangement] = &[
    // ---- count 1: the sheet itself -----------------------------------------
    // The whole sheet as one cell (S19, ruling 34: a single photo is a legal
    // collage), authored at every sheet since S39 — the sheet's shape *is* the
    // product's canvas, so these are six layouts and not one geometry six times.
    // 4:3 keeps its own name; `DEFAULT_TEMPLATE` is the 3:4 member.
    Arrangement {
        variant: "grid-1-1x1",
        slots: &[rect(0, 0, 128, 128)],
        sheets: SET,
        names: &[("4x3", "grid-1-1x1")],
    },
    // ---- count 2: the pairs ------------------------------------------------
    // Two equal rows; `strip-2-1x2` is the 2:3 member's own name.
    Arrangement {
        variant: "strip-2-1x2",
        slots: &[rect(0, 0, 128, 64), rect(0, 64, 128, 128)],
        sheets: SET,
        names: &[("2x3", "strip-2-1x2")],
    },
    // Two equal columns; the 3:2 member is `strip-2-2x1` and the 4:3 one
    // `grid-2-2x1`, both older than the tagging rule.
    Arrangement {
        variant: "strip-2-2x1",
        slots: &[rect(0, 0, 64, 128), rect(64, 0, 128, 128)],
        sheets: SET,
        names: &[("3x2", "strip-2-2x1"), ("4x3", "grid-2-2x1")],
    },
    // The unequal pair with the big cell on top: 77/128 against 51/128. The walk of
    // 2026-10-17 named this split — the small cell two thirds the big one, 1.51:1 — for
    // the pairs its list left unnamed, and **that is this recipe's own value, not a
    // rule** (restated by the human on 2026-10-17: a template's proportions are the
    // template's own, and the template format makes one an authorable item). The other
    // pairs the list does not name keep their own: 88/40 for `mosaic-2-hero`, 85/43 for
    // the seven-cell bricks. Exact thirds and exact 2:3 are not dyadic; 128 is the grid
    // S38 landed on.
    Arrangement {
        variant: "mosaic-2-hero-top",
        slots: &[rect(0, 0, 128, 77), rect(0, 77, 128, 128)],
        sheets: SET,
        names: &[],
    },
    // ... inverted.
    Arrangement {
        variant: "mosaic-2-hero-bottom",
        slots: &[rect(0, 0, 128, 51), rect(0, 51, 128, 128)],
        sheets: SET,
        names: &[],
    },
    // ... the big cell on the left.
    Arrangement {
        variant: "mosaic-2-hero-left",
        slots: &[rect(0, 0, 77, 128), rect(77, 0, 128, 128)],
        sheets: SET,
        names: &[],
    },
    // ... and on the right.
    Arrangement {
        variant: "mosaic-2-hero-right",
        slots: &[rect(0, 0, 51, 128), rect(51, 0, 128, 128)],
        sheets: SET,
        names: &[],
    },
    // The members the list does not name keep their order here, after the list's
    // arrangements. The square's guttered columns (the plain pair's own sibling)
    // and the 4:3 hero — `mosaic-2-hero`'s own 11:5 split, the member S37 added
    // and the seeding change kept.
    Arrangement {
        variant: "strip-2-2x1g",
        slots: &[rect(0, 0, 60, 128), rect(68, 0, 128, 128)],
        sheets: &["1x1"],
        names: &[("1x1", "strip-2-2x1g")],
    },
    Arrangement {
        variant: "mosaic-2-hero",
        slots: &[rect(0, 0, 88, 128), rect(88, 0, 128, 128)],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-2-hero")],
    },
    // ---- count 3: one over two, the strips, one beside two -----------------
    // One band over two squares; the 1:1 member keeps its own name.
    Arrangement {
        variant: "mosaic-3-t1b2",
        slots: &[
            rect(0, 0, 128, 64),
            rect(0, 64, 64, 128),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("1x1", "mosaic-3-t1b2")],
    },
    // Two squares over one band.
    Arrangement {
        variant: "mosaic-3-b1t2",
        slots: &[
            rect(0, 0, 64, 64),
            rect(64, 0, 128, 64),
            rect(0, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Three rows, 5/6/5; the 2:3 member is `strip-3-1x3`.
    Arrangement {
        variant: "strip-3-1x3",
        slots: &[
            rect(0, 0, 128, 40),
            rect(0, 40, 128, 88),
            rect(0, 88, 128, 128),
        ],
        sheets: SET,
        names: &[("2x3", "strip-3-1x3")],
    },
    // Three columns, 5/6/5; the 16:9 member is `strip-3-3x1`.
    Arrangement {
        variant: "strip-3-3x1",
        slots: &[
            rect(0, 0, 40, 128),
            rect(40, 0, 88, 128),
            rect(88, 0, 128, 128),
        ],
        sheets: SET,
        names: &[("16x9", "strip-3-3x1")],
    },
    // One pane down the left, two stacked beside it; the 4:3 member is
    // `mosaic-3-hero`, named before the scheme had a tag for it.
    Arrangement {
        variant: "mosaic-3-l1r2",
        slots: &[
            rect(0, 0, 64, 128),
            rect(64, 0, 128, 64),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("4x3", "mosaic-3-hero")],
    },
    // ... its mirror.
    Arrangement {
        variant: "mosaic-3-r1l2",
        slots: &[
            rect(0, 0, 64, 64),
            rect(0, 64, 64, 128),
            rect(64, 0, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // The irregular members are last in their count, and each count has at most
    // one. The album page's elbow (S37): an L down the left, a small pane in its
    // bite, one pane carrying the whole right half.
    Arrangement {
        variant: "mosaic-3-elbow",
        slots: &[
            Shape::Poly(&[(0, 0), (64, 0), (64, 32), (32, 32), (32, 128), (0, 128)]),
            rect(32, 32, 64, 128),
            rect(64, 0, 128, 128),
        ],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-3-elbow")],
    },
    // ---- count 4: the split, the one-over-three bands, the strips, the brick --
    // The plain 2x2; the 1:1 member keeps its own name.
    Arrangement {
        variant: "grid-4-2x2",
        slots: &[
            rect(0, 0, 64, 64),
            rect(64, 0, 128, 64),
            rect(0, 64, 64, 128),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("1x1", "grid-4-2x2")],
    },
    // A band over three columns; the 3:4 member is `mosaic-4-t1b3`.
    Arrangement {
        variant: "mosaic-4-t1b3",
        slots: &[
            rect(0, 0, 128, 64),
            rect(0, 64, 40, 128),
            rect(40, 64, 88, 128),
            rect(88, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("3x4", "mosaic-4-t1b3")],
    },
    // ... inverted: three over one.
    Arrangement {
        variant: "mosaic-4-t3b1",
        slots: &[
            rect(0, 0, 40, 64),
            rect(40, 0, 88, 64),
            rect(88, 0, 128, 64),
            rect(0, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Four equal rows.
    Arrangement {
        variant: "strip-4-1x4",
        slots: &[
            rect(0, 0, 128, 32),
            rect(0, 32, 128, 64),
            rect(0, 64, 128, 96),
            rect(0, 96, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Four equal columns; the 16:9 member is `strip-4-4x1`.
    Arrangement {
        variant: "strip-4-4x1",
        slots: &[
            rect(0, 0, 32, 128),
            rect(32, 0, 64, 128),
            rect(64, 0, 96, 128),
            rect(96, 0, 128, 128),
        ],
        sheets: SET,
        names: &[("16x9", "strip-4-4x1")],
    },
    // The running-bond brick: two equal rows, the top split 77/51 and the bottom
    // mirrored 51/77, so no seam lines up.
    Arrangement {
        variant: "mosaic-4-brick",
        slots: &[
            rect(0, 0, 77, 64),
            rect(77, 0, 128, 64),
            rect(0, 64, 51, 128),
            rect(51, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // The members the list does not name: the square's guttered 2x2, and the album
    // page's one-pane-and-four-columns hero.
    Arrangement {
        variant: "grid-4-2x2g",
        slots: &[
            rect(0, 0, 60, 60),
            rect(68, 0, 128, 60),
            rect(0, 68, 60, 128),
            rect(68, 68, 128, 128),
        ],
        sheets: &["1x1"],
        names: &[("1x1", "grid-4-2x2g")],
    },
    Arrangement {
        variant: "mosaic-4-hero",
        slots: &[
            rect(0, 0, 64, 128),
            rect(64, 0, 128, 40),
            rect(64, 40, 128, 88),
            rect(64, 88, 128, 128),
        ],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-4-hero")],
    },
    // ---- count 5: the three-and-two bands ----------------------------------
    // A column of three panes beside a column of two.
    Arrangement {
        variant: "mosaic-5-l3r2",
        slots: &[
            rect(0, 0, 64, 40),
            rect(0, 40, 64, 88),
            rect(0, 88, 64, 128),
            rect(64, 0, 128, 64),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... three on the right.
    Arrangement {
        variant: "mosaic-5-r3l2",
        slots: &[
            rect(0, 0, 64, 64),
            rect(0, 64, 64, 128),
            rect(64, 0, 128, 40),
            rect(64, 40, 128, 88),
            rect(64, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // A band of three over a band of two; the 3:2 member is `mosaic-5-t3b2`.
    Arrangement {
        variant: "mosaic-5-t3b2",
        slots: &[
            rect(0, 0, 40, 64),
            rect(40, 0, 88, 64),
            rect(88, 0, 128, 64),
            rect(0, 64, 64, 128),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("3x2", "mosaic-5-t3b2")],
    },
    // ... inverted: two over three — the 1:1 member is `mosaic-5-t2b3`, and the
    // 3:4 member it had when it was first built under the other reading of the
    // same arrangement (`mosaic-5-b3t2-3x4`) is now its computed name.
    Arrangement {
        variant: "mosaic-5-t2b3",
        slots: &[
            rect(0, 0, 64, 64),
            rect(64, 0, 128, 64),
            rect(0, 64, 40, 128),
            rect(40, 64, 88, 128),
            rect(88, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("1x1", "mosaic-5-t2b3")],
    },
    // The members the list does not name: the 4:3 hero (one pane and four),
    // the 3:4 transcription of it, and the square's tee.
    Arrangement {
        variant: "mosaic-5-hero",
        slots: &[
            rect(0, 0, 64, 128),
            rect(64, 0, 128, 32),
            rect(64, 32, 128, 64),
            rect(64, 64, 128, 96),
            rect(64, 96, 128, 128),
        ],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-5-hero")],
    },
    // The wide sheet's own member: five columns (24/24/24/24/32 of the width),
    // the count's filmstrip.
    Arrangement {
        variant: "strip-5-5x1",
        slots: &[
            rect(0, 0, 24, 128),
            rect(24, 0, 48, 128),
            rect(48, 0, 72, 128),
            rect(72, 0, 96, 128),
            rect(96, 0, 128, 128),
        ],
        sheets: &["16x9"],
        names: &[("16x9", "strip-5-5x1")],
    },
    Arrangement {
        variant: "mosaic-5-t1b4",
        slots: &[
            rect(0, 0, 128, 64),
            rect(0, 64, 32, 128),
            rect(32, 64, 64, 128),
            rect(64, 64, 96, 128),
            rect(96, 64, 128, 128),
        ],
        sheets: &["3x4"],
        names: &[("3x4", "mosaic-5-t1b4")],
    },
    // The square's irregular cell (S37): a tee with four panes in the two corners
    // under its arms — last in its count, like every irregular member.
    Arrangement {
        variant: "mosaic-5-tee",
        slots: &[
            Shape::Poly(&[
                (0, 0),
                (128, 0),
                (128, 40),
                (88, 40),
                (88, 128),
                (40, 128),
                (40, 40),
                (0, 40),
            ]),
            rect(0, 40, 40, 88),
            rect(0, 88, 40, 128),
            rect(88, 40, 128, 88),
            rect(88, 88, 128, 128),
        ],
        sheets: &["1x1"],
        names: &[("1x1", "mosaic-5-tee")],
    },
    // ---- count 6: three and three, the bands of two and four ----------------
    // Three panes beside three, the same rows in both columns (the walk of
    // 2026-10-17: an obviously symmetric arrangement's seams line up); the 2:3
    // member is `grid-6-2x3`, whose rows are 6/5/5 — the offset that shipped
    // first, and the arrangement's one geometry.
    Arrangement {
        variant: "grid-6-2x3",
        slots: &[
            rect(0, 0, 64, 48),
            rect(64, 0, 128, 48),
            rect(0, 48, 64, 88),
            rect(64, 48, 128, 88),
            rect(0, 88, 64, 128),
            rect(64, 88, 128, 128),
        ],
        sheets: SET,
        names: &[("2x3", "grid-6-2x3")],
    },
    // ... and three over three, aligned the same way; the 3:2 member is
    // `grid-6-3x2`.
    Arrangement {
        variant: "grid-6-3x2",
        slots: &[
            rect(0, 0, 40, 64),
            rect(40, 0, 88, 64),
            rect(88, 0, 128, 64),
            rect(0, 64, 40, 128),
            rect(40, 64, 88, 128),
            rect(88, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("3x2", "grid-6-3x2")],
    },
    // A band of two over a band of four.
    Arrangement {
        variant: "mosaic-6-t2b4",
        slots: &[
            rect(0, 0, 64, 64),
            rect(64, 0, 128, 64),
            rect(0, 64, 32, 128),
            rect(32, 64, 64, 128),
            rect(64, 64, 96, 128),
            rect(96, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... two beside four; the 4:3 member is `mosaic-6-hero`.
    Arrangement {
        variant: "mosaic-6-l2r4",
        slots: &[
            rect(0, 0, 64, 64),
            rect(0, 64, 64, 128),
            rect(64, 0, 128, 32),
            rect(64, 32, 128, 64),
            rect(64, 64, 128, 96),
            rect(64, 96, 128, 128),
        ],
        sheets: SET,
        names: &[("4x3", "mosaic-6-hero")],
    },
    // ... four beside two.
    Arrangement {
        variant: "mosaic-6-l4r2",
        slots: &[
            rect(0, 0, 64, 32),
            rect(0, 32, 64, 64),
            rect(0, 64, 64, 96),
            rect(0, 96, 64, 128),
            rect(64, 0, 128, 64),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... and four over two.
    Arrangement {
        variant: "mosaic-6-t4b2",
        slots: &[
            rect(0, 0, 32, 64),
            rect(32, 0, 64, 64),
            rect(64, 0, 96, 64),
            rect(96, 0, 128, 64),
            rect(0, 64, 64, 128),
            rect(64, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // The members the list does not name: the square's staggered three-over-three
    // (frozen, so it stays) and the wide sheet's filmstrip.
    Arrangement {
        variant: "strip-6-6x1",
        slots: &[
            rect(0, 0, 16, 128),
            rect(16, 0, 40, 128),
            rect(40, 0, 64, 128),
            rect(64, 0, 88, 128),
            rect(88, 0, 112, 128),
            rect(112, 0, 128, 128),
        ],
        sheets: &["16x9"],
        names: &[("16x9", "strip-6-6x1")],
    },
    Arrangement {
        variant: "mosaic-6-t3b3",
        slots: &[
            rect(0, 0, 40, 64),
            rect(40, 0, 88, 64),
            rect(88, 0, 128, 64),
            rect(0, 64, 48, 128),
            rect(48, 64, 88, 128),
            rect(88, 64, 128, 128),
        ],
        sheets: &["1x1"],
        names: &[("1x1", "mosaic-6-t3b3")],
    },
    // ---- count 7: the four-over-three bands and the three-band layouts ------
    // A band of four over a band of three; the 4:3 member is `mosaic-7-t4b3`.
    Arrangement {
        variant: "mosaic-7-t4b3",
        slots: &[
            rect(0, 0, 32, 64),
            rect(32, 0, 64, 64),
            rect(64, 0, 96, 64),
            rect(96, 0, 128, 64),
            rect(0, 64, 40, 128),
            rect(40, 64, 88, 128),
            rect(88, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("4x3", "mosaic-7-t4b3")],
    },
    // ... and three over four.
    Arrangement {
        variant: "mosaic-7-t3b4",
        slots: &[
            rect(0, 0, 40, 64),
            rect(40, 0, 88, 64),
            rect(88, 0, 128, 64),
            rect(0, 64, 32, 128),
            rect(32, 64, 64, 128),
            rect(64, 64, 96, 128),
            rect(96, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Three cells, then two, then two.
    Arrangement {
        variant: "mosaic-7-t3m2b2",
        slots: &[
            rect(0, 0, 40, 40),
            rect(40, 0, 88, 40),
            rect(88, 0, 128, 40),
            rect(0, 40, 64, 88),
            rect(64, 40, 128, 88),
            rect(0, 88, 64, 128),
            rect(64, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Three cells over one wide pane over three.
    Arrangement {
        variant: "mosaic-7-t3m1b3",
        slots: &[
            rect(0, 0, 40, 40),
            rect(40, 0, 88, 40),
            rect(88, 0, 128, 40),
            rect(0, 40, 128, 88),
            rect(0, 88, 40, 128),
            rect(40, 88, 88, 128),
            rect(88, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Two over three over two.
    Arrangement {
        variant: "mosaic-7-t2m3b2",
        slots: &[
            rect(0, 0, 64, 40),
            rect(64, 0, 128, 40),
            rect(0, 40, 40, 88),
            rect(40, 40, 88, 88),
            rect(88, 40, 128, 88),
            rect(0, 88, 64, 128),
            rect(64, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... the offset twin: the outer rows split for the 2:1 the human's list
    // names — 85:43 on this lattice (1.977:1), the closest dyadic to exactly two —
    // and mirrored, so the seams run the other way.
    Arrangement {
        variant: "mosaic-7-t2m3b2-brick",
        slots: &[
            rect(0, 0, 85, 40),
            rect(85, 0, 128, 40),
            rect(0, 40, 40, 88),
            rect(40, 40, 88, 88),
            rect(88, 40, 128, 88),
            rect(0, 88, 43, 128),
            rect(43, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // The members the list does not name: the square's three-column mosaic, the
    // wide sheet's filmstrip, the portrait page's four-beside-three and the album
    // page's left slab over a 2x3 block.
    Arrangement {
        variant: "strip-7-7x1",
        slots: &[
            rect(0, 0, 16, 128),
            rect(16, 0, 40, 128),
            rect(40, 0, 56, 128),
            rect(56, 0, 80, 128),
            rect(80, 0, 96, 128),
            rect(96, 0, 112, 128),
            rect(112, 0, 128, 128),
        ],
        sheets: &["16x9"],
        names: &[("16x9", "strip-7-7x1")],
    },
    Arrangement {
        variant: "mosaic-7-hero",
        slots: &[
            rect(0, 0, 40, 48),
            rect(0, 48, 40, 88),
            rect(0, 88, 40, 128),
            rect(40, 0, 88, 64),
            rect(40, 64, 88, 128),
            rect(88, 0, 128, 64),
            rect(88, 64, 128, 128),
        ],
        sheets: &["1x1"],
        names: &[("1x1", "mosaic-7-hero")],
    },
    Arrangement {
        variant: "mosaic-7-l4r3",
        slots: &[
            rect(0, 0, 64, 32),
            rect(0, 32, 64, 64),
            rect(0, 64, 64, 96),
            rect(0, 96, 64, 128),
            rect(64, 0, 128, 40),
            rect(64, 40, 128, 88),
            rect(64, 88, 128, 128),
        ],
        sheets: &["3x4"],
        names: &[("3x4", "mosaic-7-l4r3")],
    },
    Arrangement {
        variant: "mosaic-7-l1r6",
        slots: &[
            rect(0, 0, 64, 128),
            rect(64, 0, 96, 40),
            rect(96, 0, 128, 40),
            rect(64, 40, 96, 88),
            rect(96, 40, 128, 88),
            rect(64, 88, 96, 128),
            rect(96, 88, 128, 128),
        ],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-7-l1r6")],
    },
    // The portrait page's irregular cell (S37), last in its count since the walk
    // of 2026-10-17: an L a person finds when they want it.
    Arrangement {
        variant: "mosaic-7-elbow",
        slots: &[
            Shape::Poly(&[(0, 0), (64, 0), (64, 32), (32, 32), (32, 128), (0, 128)]),
            rect(32, 32, 64, 64),
            rect(32, 64, 64, 96),
            rect(32, 96, 64, 128),
            rect(64, 0, 128, 40),
            rect(64, 40, 128, 88),
            rect(64, 88, 128, 128),
        ],
        sheets: &["3x4"],
        names: &[("3x4", "mosaic-7-elbow")],
    },
    // ---- count 8: the four-and-four tilings and the three-two-three bands ----
    // Four beside four: two columns of four, the slots in the row-major order
    // `grid-8-2x4` shipped with (the matrix keeps a shipped slot order).
    Arrangement {
        variant: "grid-8-2x4",
        slots: &[
            rect(0, 0, 64, 32),
            rect(64, 0, 128, 32),
            rect(0, 32, 64, 64),
            rect(64, 32, 128, 64),
            rect(0, 64, 64, 96),
            rect(64, 64, 128, 96),
            rect(0, 96, 64, 128),
            rect(64, 96, 128, 128),
        ],
        sheets: SET,
        names: &[("3x4", "grid-8-2x4")],
    },
    // ... four over four; the 3:2 member is `grid-8-4x2`.
    Arrangement {
        variant: "grid-8-4x2",
        slots: &[
            rect(0, 0, 32, 64),
            rect(32, 0, 64, 64),
            rect(64, 0, 96, 64),
            rect(96, 0, 128, 64),
            rect(0, 64, 32, 128),
            rect(32, 64, 64, 128),
            rect(64, 64, 96, 128),
            rect(96, 64, 128, 128),
        ],
        sheets: SET,
        names: &[("3x2", "grid-8-4x2")],
    },
    // Three over two over three: the middle band's two panes wider and taller.
    Arrangement {
        variant: "mosaic-8-t3m2b3",
        slots: &[
            rect(0, 0, 40, 40),
            rect(40, 0, 88, 40),
            rect(88, 0, 128, 40),
            rect(0, 40, 64, 88),
            rect(64, 40, 128, 88),
            rect(0, 88, 40, 128),
            rect(40, 88, 88, 128),
            rect(88, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... three beside two beside three.
    Arrangement {
        variant: "mosaic-8-l3m2r3",
        slots: &[
            rect(0, 0, 40, 40),
            rect(0, 40, 40, 88),
            rect(0, 88, 40, 128),
            rect(40, 0, 88, 64),
            rect(40, 64, 88, 128),
            rect(88, 0, 128, 40),
            rect(88, 40, 128, 88),
            rect(88, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // The members the list does not name: the wide sheet's filmstrip, the square's
    // uneven four-over-four (frozen), and — last — the smoke template itself.
    Arrangement {
        variant: "strip-8-8x1",
        slots: &[
            rect(0, 0, 16, 128),
            rect(16, 0, 32, 128),
            rect(32, 0, 48, 128),
            rect(48, 0, 64, 128),
            rect(64, 0, 80, 128),
            rect(80, 0, 96, 128),
            rect(96, 0, 112, 128),
            rect(112, 0, 128, 128),
        ],
        sheets: &["16x9"],
        names: &[("16x9", "strip-8-8x1")],
    },
    Arrangement {
        variant: "mosaic-8-t4b4",
        slots: &[
            rect(0, 0, 32, 64),
            rect(32, 0, 64, 64),
            rect(64, 0, 96, 64),
            rect(96, 0, 128, 64),
            rect(0, 64, 40, 128),
            rect(40, 64, 64, 128),
            rect(64, 64, 88, 128),
            rect(88, 64, 128, 128),
        ],
        sheets: &["1x1"],
        names: &[("1x1", "mosaic-8-t4b4")],
    },
    // The template S1 froze: `AGENTS.md`'s per-round verification command names it
    // and `docs/CONTRACT.md` §1 uses it as the example, so its name, its `version`,
    // its aspect, its slot order and its coordinates are all frozen — S2 rewrote
    // how the geometry is *produced*, not what it is. One slot is concave (an L),
    // so it is last in its count like every irregular member.
    Arrangement {
        variant: "mosaic-8-s14",
        slots: &[
            rect(0, 0, 48, 48),
            rect(0, 48, 48, 80),
            rect(0, 80, 48, 128),
            rect(48, 0, 64, 48),
            rect(64, 0, 96, 48),
            rect(96, 0, 128, 48),
            Shape::Poly(&[
                (48, 48),
                (128, 48),
                (128, 80),
                (96, 80),
                (96, 128),
                (48, 128),
            ]),
            rect(96, 80, 128, 128),
        ],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-8-s14")],
    },
    // ---- count 9: the nine-grids and the five-and-four bands ----------------
    // The even nine-grid: exact thirds are not dyadic, so this is as close as the
    // lattice gets (43/42/43, cell edges within 0.8% of a third). The square's
    // frozen `grid-9-3x3` (6/5/5) is a member of its own, below.
    Arrangement {
        variant: "grid-9-3x3",
        slots: &[
            rect(0, 0, 43, 43),
            rect(43, 0, 85, 43),
            rect(85, 0, 128, 43),
            rect(0, 43, 43, 85),
            rect(43, 43, 85, 85),
            rect(85, 43, 128, 85),
            rect(0, 85, 43, 128),
            rect(43, 85, 85, 128),
            rect(85, 85, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... and its wide-middle twin (5/6/5 both ways).
    Arrangement {
        variant: "grid-9-3x3-wide-middle",
        slots: &[
            rect(0, 0, 40, 40),
            rect(40, 0, 88, 40),
            rect(88, 0, 128, 40),
            rect(0, 40, 40, 88),
            rect(40, 40, 88, 88),
            rect(88, 40, 128, 88),
            rect(0, 88, 40, 128),
            rect(40, 88, 88, 128),
            rect(88, 88, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // Five panes beside four, the five-column side's tallest pane in the middle
    // (3/3/4/3/3), as `mosaic-9-l4r5`'s five-row column has.
    Arrangement {
        variant: "mosaic-9-l5r4",
        slots: &[
            rect(0, 0, 64, 24),
            rect(0, 24, 64, 48),
            rect(0, 48, 64, 80),
            rect(0, 80, 64, 104),
            rect(0, 104, 64, 128),
            rect(64, 0, 128, 32),
            rect(64, 32, 128, 64),
            rect(64, 64, 128, 96),
            rect(64, 96, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... its transpose, five over four.
    Arrangement {
        variant: "mosaic-9-t5b4",
        slots: &[
            rect(0, 0, 24, 64),
            rect(24, 0, 48, 64),
            rect(48, 0, 80, 64),
            rect(80, 0, 104, 64),
            rect(104, 0, 128, 64),
            rect(0, 64, 32, 128),
            rect(32, 64, 64, 128),
            rect(64, 64, 96, 128),
            rect(96, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // ... and four over five.
    Arrangement {
        variant: "mosaic-9-t4b5",
        slots: &[
            rect(0, 0, 32, 64),
            rect(32, 0, 64, 64),
            rect(64, 0, 96, 64),
            rect(96, 0, 128, 64),
            rect(0, 64, 24, 128),
            rect(24, 64, 48, 128),
            rect(48, 64, 80, 128),
            rect(80, 64, 104, 128),
            rect(104, 64, 128, 128),
        ],
        sheets: SET,
        names: &[],
    },
    // The members the list does not name: the wide sheet's filmstrip, the album
    // page's one-and-eight hero, its four-beside-five reading, and the square's
    // frozen nine-grid (6/5/5, which the even one above does not replace).
    Arrangement {
        variant: "strip-9-9x1",
        slots: &[
            rect(0, 0, 16, 128),
            rect(16, 0, 32, 128),
            rect(32, 0, 48, 128),
            rect(48, 0, 64, 128),
            rect(64, 0, 80, 128),
            rect(80, 0, 96, 128),
            rect(96, 0, 104, 128),
            rect(104, 0, 112, 128),
            rect(112, 0, 128, 128),
        ],
        sheets: &["16x9"],
        names: &[("16x9", "strip-9-9x1")],
    },
    Arrangement {
        variant: "mosaic-9-hero",
        slots: &[
            rect(0, 0, 64, 128),
            rect(64, 0, 96, 32),
            rect(96, 0, 128, 32),
            rect(64, 32, 96, 64),
            rect(96, 32, 128, 64),
            rect(64, 64, 96, 96),
            rect(96, 64, 128, 96),
            rect(64, 96, 96, 128),
            rect(96, 96, 128, 128),
        ],
        sheets: &["4x3"],
        names: &[("4x3", "mosaic-9-hero")],
    },
    Arrangement {
        variant: "mosaic-9-l4r5",
        slots: &[
            rect(0, 0, 64, 32),
            rect(0, 32, 64, 64),
            rect(0, 64, 64, 96),
            rect(0, 96, 64, 128),
            rect(64, 0, 128, 24),
            rect(64, 24, 128, 48),
            rect(64, 48, 128, 80),
            rect(64, 80, 128, 104),
            rect(64, 104, 128, 128),
        ],
        sheets: &["3x4"],
        names: &[("3x4", "mosaic-9-l4r5")],
    },
    // ... and the square's *frozen* nine-grid (6/5/5), kept as its own member: the
    // set's nine-grid is the even one above, and this entry exists only to ship the
    // name the library has carried since S1. Its variant base is never computed
    // from (the one member names itself).
    Arrangement {
        variant: "grid-9-3x3-frozen",
        slots: &[
            rect(0, 0, 48, 48),
            rect(48, 0, 88, 48),
            rect(88, 0, 128, 48),
            rect(0, 48, 48, 88),
            rect(48, 48, 88, 88),
            rect(88, 48, 128, 88),
            rect(0, 88, 48, 128),
            rect(48, 88, 88, 128),
            rect(88, 88, 128, 128),
        ],
        sheets: &["1x1"],
        names: &[("1x1", "grid-9-3x3")],
    },
];

/// Builds every template the arrangements describe, in the canonical order.
///
/// Pure: the result is a function of the constants above alone, with no clock,
/// no filesystem and no iteration order to depend on, which is what "generation
/// must be deterministic" means here.
pub fn generate() -> Vec<Template> {
    let mut templates: Vec<Template> = ARRANGEMENTS.iter().flat_map(build).collect();
    // The ID photo sheets come after every collage arrangement: they are a
    // different kind of member (one photo, many copies), and no candidate query
    // lists them, so their place only decides where a caller that walks the whole
    // library meets them.
    templates.extend(build_id_sheets());
    templates
}

/// Builds the ID photo sheets (S40), in [`ID_SHEETS`] order.
///
/// The recipe is physical (a cell in millimetres on the 4R page) and the geometry
/// is normalized, so the cell and gap are converted to [`ID_GRID`] units, the count
/// is the largest that fits per axis, and the leftover is split between the two
/// margins. Integer arithmetic on the lattice: the result is a function of the
/// constants above, like every other member.
fn build_id_sheets() -> Vec<Template> {
    let grid = ID_GRID;
    ID_SHEETS
        .iter()
        .map(|sheet| {
            // The page this size is printed on: the same paper, turned or not (S40's
            // ruling — the target is the print, and the paper has no upright side).
            let (page_w, page_h, tag) = if sheet.landscape {
                (ID_SHEET_MM.1, ID_SHEET_MM.0, "3x2")
            } else {
                (ID_SHEET_MM.0, ID_SHEET_MM.1, "2x3")
            };
            let unit_x = page_w / f64::from(grid);
            let unit_y = page_h / f64::from(grid);
            let cell_x = (sheet.cell_mm.0 / unit_x).round() as u32;
            let cell_y = (sheet.cell_mm.1 / unit_y).round() as u32;
            let gap_x = (ID_GAP_MM / unit_x).round() as u32;
            let gap_y = (ID_GAP_MM / unit_y).round() as u32;
            // c cells and c-1 gaps fit iff c * (cell + gap) <= grid + gap.
            let count = |cell: u32, gap: u32| (grid + gap) / (cell + gap);
            let cols = count(cell_x, gap_x);
            let rows = count(cell_y, gap_y);
            let used_x = cols * cell_x + (cols - 1) * gap_x;
            let used_y = rows * cell_y + (rows - 1) * gap_y;
            // Centred; the odd unit, when there is one, goes to the far margin.
            let first_x = (grid - used_x) / 2;
            let first_y = (grid - used_y) / 2;
            let mut slots = Vec::with_capacity((cols * rows) as usize);
            for row in 0..rows {
                for col in 0..cols {
                    let x0 = first_x + col * (cell_x + gap_x);
                    let y0 = first_y + row * (cell_y + gap_y);
                    let shape = Shape::Rect {
                        x0,
                        y0,
                        x1: x0 + cell_x,
                        y1: y0 + cell_y,
                    };
                    let outline = shape.outline(1.0 / f64::from(grid));
                    slots.push(Slot {
                        // The area is the outline's own, exactly as `build` takes it.
                        area: outline.area(),
                        outline,
                    });
                }
            }
            Template {
                name: format!("id-{}-{}-{tag}", sheet.size, slots.len()),
                // The one-slot sheets' rule: a new member starts at version 1.
                version: 1,
                // The page's shape is the tag's own division of small integers.
                aspect: aspect_of(tag),
                one_photo: true,
                slots,
            }
        })
        .collect()
}

fn build(arrangement: &Arrangement) -> Vec<Template> {
    debug_assert!(
        arrangement
            .sheets
            .windows(2)
            .all(|pair| order_of(pair[0]) < order_of(pair[1])),
        "{}: sheets are not in SHEETS order",
        arrangement.variant
    );
    let unit = 1.0 / f64::from(GRID);
    let slots: Vec<Slot> = arrangement
        .slots
        .iter()
        .map(|shape| {
            let outline = shape.outline(unit);
            Slot {
                // The declared area is the outline's own area, computed from the
                // same geometry: the two cannot drift apart, and the frozen file
                // records the value the tests then verify independently.
                area: outline.area(),
                outline,
            }
        })
        .collect();
    arrangement
        .sheets
        .iter()
        .map(|tag| {
            let aspect = aspect_of(tag);
            let name = arrangement
                .names
                .iter()
                .find(|(named, _)| named == tag)
                .map_or_else(
                    || format!("{}-{tag}", arrangement.variant),
                    |(_, name)| (*name).to_string(),
                );
            Template {
                name,
                // Every member is at version 1; a bump is a per-member event, and
                // the field comes back the day one is needed.
                version: 1,
                aspect,
                one_photo: false,
                slots: slots.clone(),
            }
        })
        .collect()
}

/// The position of a sheet tag in [`SHEETS`].
fn order_of(tag: &str) -> usize {
    SHEETS
        .iter()
        .position(|(named, _)| *named == tag)
        .unwrap_or_else(|| panic!("{tag}: no such sheet"))
}

/// The aspect of a sheet tag.
fn aspect_of(tag: &str) -> f64 {
    SHEETS[order_of(tag)].1
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
    let mut out = String::with_capacity(64 * 1024);
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
    out.push_str("];\n\n");
    out.push_str(
        "/// The members whose sheet holds **one photo in every slot** (S40).\n\
         ///\n\
         /// A name list rather than a field on every entry: the ID photo sheets are the\n\
         /// only ones, and every other member's frozen bytes stay exactly what they were\n\
         /// — the audit \"every member that shipped keeps its geometry byte for byte\"\n\
         /// still reads as one diff. `templates::thaw` sets `Template::one_photo` from\n\
         /// it, and the candidate queries skip these names (`templates::is_collage`).\n\
         pub const ONE_PHOTO: &[&str] = &[\n",
    );
    for template in templates.iter().filter(|template| template.one_photo) {
        let _ = writeln!(out, "    {:?},", template.name);
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
//! The arrangements live in `templates/generator.rs`, and the determinism test in
//! `crates/pixlay-core/tests/templates.rs` regenerates this file into a scratch
//! path and compares it byte for byte, so the arrangement and the geometry a build
//! ships can never drift apart. Coordinates are normalized to `[0, 1]`
//! (`AGENTS.md`) and are written on a dyadic lattice: the collage arrangements on
//! integer units of `1/128`, and the ID photo sheets (S40) on `1/2560`, which is
//! what lands a 4R cell within 0.03 mm of its millimetre size. Every area is exact
//! in binary floating point, which is why a cut template's sum is an equality.
//!
//! [`ONE_PHOTO`] names the members whose sheet holds one photo in every slot; the
//! rest of the table is the collage library.

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

/// Every template this build ships, in the canonical order.
#[rustfmt::skip]
pub const TEMPLATES: &[Frozen] = &[
";
