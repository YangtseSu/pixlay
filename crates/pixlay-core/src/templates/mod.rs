//! The template library: name to frozen geometry.
//!
//! Templates are document data, so the library lives in `pixlay-core` next to the
//! types it produces (`AGENTS.md`, module boundaries): the GUI needs it to offer a
//! template picker, the CLI needs it for the photo-free smoke render and for
//! `templates` / `init`.
//!
//! Three parts, in the order a change to the library flows through them:
//!
//! * [`generator`] holds the recipes — one entry per template, written on an
//!   integer lattice — and turns them into `Template` values.
//! * [`frozen`] is the committed artifact those recipes produce: the geometry a
//!   build actually ships. `AGENTS.md` forbids an existing project's layout from
//!   changing, so the bytes in this file are the interface. Regenerating it is a
//!   reviewed commit, not a build step: `cargo run -p pixlay-core --bin
//!   pixlay-gen-templates`, and `crates/pixlay-core/tests/templates.rs` fails if
//!   the file and the recipes disagree.
//! * this module serves the data: [`get`], [`all`], [`names`], [`of_aspect`] and
//!   [`document`].
//!
//! The matrix is grouped by aspect ratio, because a canvas and a template only fit
//! each other when their ratios agree (`CollageDoc::validate` makes a mismatch a
//! hard error) and the picker offers the matching group. The families, by how the
//! geometry is laid out:
//!
//! * `strip-<slots>-<cols>x<rows>` — one band: a single row or a single column.
//! * `grid-<slots>-<cols>x<rows>` — a rectangular tiling that repeats the same
//!   splits in both directions. A `g` suffix is the same grid with a gutter, so
//!   the slots do not tile the canvas and their areas sum to less than 1.0.
//! * `mosaic-<slots>-<variant>` — mixed splits or a slot that is not a rectangle.
//!
//! `mosaic-8-s14` predates the scheme: it is the template S1 froze and the name
//! `AGENTS.md`'s verification command uses, so its name, version, aspect, slot
//! order and coordinates are unchanged by S2.

mod frozen;

/// The recipes and the code that turns them into geometry.
pub mod generator;

use crate::geometry::{Point, Polygon};
use crate::{ASPECT_TOLERANCE, CanvasSpec, CollageDoc, Slot, Template};

/// The name `AGENTS.md`'s verification command uses. S2 keeps this name and
/// freezes the geometry behind [`TEMPLATE_VERSION`]; changing either afterwards
/// would change the layout of an existing project.
pub const SMOKE_TEMPLATE: &str = "mosaic-8-s14";

/// Geometry version of the smoke template.
pub const TEMPLATE_VERSION: u32 = 1;

/// Long edge of the canvas a template document is created with (A0's long edge).
/// A template declares a ratio, not a size, so this is the size `init` and the
/// smoke render give it; a real project sets its own canvas.
const CANVAS_LONG_EDGE_MM: f64 = 1189.0;

/// Rebuilds one template from its frozen data.
fn thaw(entry: &frozen::Frozen) -> Template {
    let slots = entry
        .slots
        .iter()
        .map(|(points, area)| {
            let outline = Polygon {
                points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            };
            Slot {
                outline,
                area: *area,
            }
        })
        .collect();
    Template {
        name: entry.name.to_string(),
        version: entry.version,
        aspect: entry.aspect,
        slots,
    }
}

/// Every template this build ships, in the library's canonical order (by slot
/// count, then by recipe order).
pub fn all() -> Vec<Template> {
    frozen::TEMPLATES.iter().map(thaw).collect()
}

/// Template names this build knows, in the same order as [`all`].
///
/// Names only, so a caller that just needs valid values — a usage message, a
/// completion list — does not build the geometry.
pub fn names() -> Vec<&'static str> {
    frozen::TEMPLATES.iter().map(|entry| entry.name).collect()
}

/// Looks a template up by name.
pub fn get(name: &str) -> Option<Template> {
    frozen::TEMPLATES
        .iter()
        .find(|entry| entry.name == name)
        .map(thaw)
}

/// Templates whose declared aspect ratio matches `aspect` within
/// [`ASPECT_TOLERANCE`], the same tolerance `CollageDoc::validate` applies.
///
/// This is the picker's query (`docs/STEPS.md`, S2): a canvas and a template only
/// fit each other when their ratios agree, and the canvas is what the user picks
/// first.
pub fn of_aspect(aspect: f64) -> Vec<Template> {
    frozen::TEMPLATES
        .iter()
        .filter(|entry| (entry.aspect - aspect).abs() <= ASPECT_TOLERANCE)
        .map(thaw)
        .collect()
}

/// A document for `template` with no photos: the photo-free smoke path, and what
/// `init --template` writes.
pub fn document(template: &Template) -> CollageDoc {
    let canvas = CanvasSpec::with_ratio(template.aspect, CANVAS_LONG_EDGE_MM);
    CollageDoc::new(canvas, template.clone())
}
