//! The template library.
//!
//! Templates are document data, so the library lives in `pixlay-core` next to the
//! types it produces (`AGENTS.md`, module boundaries): the GUI needs it to offer
//! a template picker, and the CLI needs it for the photo-free smoke render. S2
//! replaces the hand-written geometry below with the deterministic generator plus
//! the full matrix and its invariant tests (zero overlap, no holes, exact area).
//! S1 needs one valid template so that the CLI's photo-free smoke path — the
//! command `AGENTS.md` lists as the per-round verification — exists from S1 on.
//!
//! The layout below is a *cut* template: the slots tile the canvas exactly, so
//! their areas sum to exactly 1.0 and every coordinate is a multiple of 1/8,
//! which is exact in binary floating point. One slot is concave, so the clip
//! path, the probe's interior sampling and (in S6.5) the hit test all see a
//! non-rectangular outline.

use crate::{CanvasSpec, CollageDoc, Point, Polygon, Slot, Template};

/// The name `AGENTS.md`'s verification command uses. S2 keeps this name and
/// freezes the geometry behind [`TEMPLATE_VERSION`]; changing either afterwards
/// would change the layout of an existing project.
pub const SMOKE_TEMPLATE: &str = "mosaic-8-s14";

/// Geometry version of the smoke template.
pub const TEMPLATE_VERSION: u32 = 1;

/// Long edge of the canvas a template document is created with.
const CANVAS_LONG_EDGE_MM: f64 = 1189.0;

/// One grid unit. Eighths are exact in binary floating point, so the areas and
/// the edge comparisons in tests are exact too.
const E: f64 = 0.125;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Slot {
    let outline = Polygon::rect(x0 * E, y0 * E, x1 * E, y1 * E);
    Slot {
        area: outline.area(),
        outline,
    }
}

fn shape(points: &[(f64, f64)]) -> Slot {
    let outline = Polygon {
        points: points
            .iter()
            .map(|&(x, y)| Point::new(x * E, y * E))
            .collect(),
    };
    Slot {
        area: outline.area(),
        outline,
    }
}

/// 8 slots on an 8x8 grid, cut edge to edge:
///
/// ```text
/// x=0     3    4   6     8
/// y=0 +-----+---+----+-----+
///     |     |   |    |     |
///     |  A  |D1 | D2 | D3  |  y 0..3
///     |     |   |    |     |
/// y=3 +-----+---+----+-----+
///     |     |               |
///     |  B  |       F       |  y 3..5
///     |     |               |
/// y=5 +-----+---------+-----+
///     |     |         |     |
///     |  C  |    G    |  F  |  y 5..8
///     |     |         |     |
/// y=8 +-----+---------+-----+
/// ```
///
/// `F` is the concave slot: it spans x 3..8 in its upper band and x 3..6 below
/// it, so its bounding-box centre — and the centre of the notch beside it — are
/// outside the outline.
fn mosaic_8() -> Template {
    let slots = vec![
        // Left column.
        rect(0.0, 0.0, 3.0, 3.0),
        rect(0.0, 3.0, 3.0, 5.0),
        rect(0.0, 5.0, 3.0, 8.0),
        // Top right: three blocks above the concave slot.
        rect(3.0, 0.0, 4.0, 3.0),
        rect(4.0, 0.0, 6.0, 3.0),
        rect(6.0, 0.0, 8.0, 3.0),
        // The concave slot.
        shape(&[
            (3.0, 3.0),
            (8.0, 3.0),
            (8.0, 5.0),
            (6.0, 5.0),
            (6.0, 8.0),
            (3.0, 8.0),
        ]),
        // The block the concave slot leaves at its lower right.
        rect(6.0, 5.0, 8.0, 8.0),
    ];
    Template {
        name: SMOKE_TEMPLATE.to_string(),
        version: TEMPLATE_VERSION,
        aspect: 4.0 / 3.0,
        slots,
    }
}

/// Looks a template up by name.
pub fn get(name: &str) -> Option<Template> {
    match name {
        SMOKE_TEMPLATE => Some(mosaic_8()),
        _ => None,
    }
}

/// Template names this build knows, for the usage message.
pub fn names() -> &'static [&'static str] {
    &[SMOKE_TEMPLATE]
}

/// A document for `template` with no photos: the photo-free smoke path.
pub fn document(template: &Template) -> CollageDoc {
    let canvas = CanvasSpec::with_ratio(template.aspect, CANVAS_LONG_EDGE_MM);
    CollageDoc::new(canvas, template.clone())
}
