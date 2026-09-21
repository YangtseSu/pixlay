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

/// The whole library, in slot-count order.
///
/// The matrix has to cover every slot count from `MIN_SLOTS` to `MAX_SLOTS`
/// (`docs/CONTRACT.md` §3) and be grouped by aspect ratio, so the
/// recipes read as: which counts, on which canvas shapes. Every aspect ratio in
/// the list is one a printed canvas uses (1:1, 4:3, 3:2, 16:9) or its portrait
/// counterpart (2:3).
static RECIPES: &[Recipe] = &[
    // Two rows on a portrait canvas. The simplest layout there is, and the
    // portrait counterpart of `strip-2-2x1`: the picker offers one or the other
    // depending on the canvas shape.
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
    // Three columns, unequal: 5/16, 6/16, 5/16. Unequal spans are what makes a
    // strip a layout rather than a repeated pane.
    Recipe {
        name: "strip-3-3x1",
        version: 1,
        aspect: R_16_9,
        grid: 16,
        slots: &[rect(0, 0, 5, 16), rect(5, 0, 11, 16), rect(11, 0, 16, 16)],
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
    // 3x3, unequal columns and rows (6/16, 5/16, 5/16 each way).
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
    // Ten columns. A ten-way vertical split cannot be equal on a dyadic
    // lattice, so the panes are 2/16 wide with four 1/16 panes in the middle —
    // which is also what a real ten-photo strip looks like.
    Recipe {
        name: "strip-10-10x1",
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
            rect(14, 0, 15, 16),
            rect(15, 0, 16, 16),
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
/// prose and its data are regenerated by the same command.
const HEADER: &str = "\
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
