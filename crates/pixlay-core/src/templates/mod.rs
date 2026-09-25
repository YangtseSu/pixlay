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
//! The matrix is grouped by aspect ratio: a template's aspect *is* the sheet's
//! shape (S12d removed the canvas it used to have to agree with; the normalized
//! geometry is stretched onto the render grid, whose long edge is the one
//! parameter a caller gives), and the picker offers the matching group
//! ([`of_aspect`]). The families, by how the geometry is laid out:
//!
//! * `strip-<slots>-<cols>x<rows>` — one band: a single row or a single column.
//! * `grid-<slots>-<cols>x<rows>` — a rectangular tiling that repeats the same
//!   splits in both directions.
//! * `mosaic-<slots>-<variant>` — mixed splits or a slot that is not a rectangle.
//!
//! A `g` suffix marks a **gutter** — a strip or a grid whose panes stop short of
//! each other, so the slots do not tile the canvas and their areas sum to less
//! than 1.0 (`grid-4-2x2g`, `strip-2-2x1g`). Since S10 every photo count from 2
//! to 9 carries at least three layouts, in at least two aspect families; nine is
//! the ceiling, because S12c removed the ten-slot recipe that used to sit above
//! it. Count 1 has exactly one member since S19 — `grid-1-1x1`, the whole sheet —
//! because one photo is a legal collage (ruling 34) and a second one-slot layout
//! would be the same geometry under another name.
//!
//! `mosaic-8-s14` predates the scheme: it is the template S1 froze and the name
//! `AGENTS.md`'s verification command uses, so its name, version, aspect, slot
//! order and coordinates are unchanged by S2.

mod frozen;

/// The recipes and the code that turns them into geometry.
pub mod generator;

use crate::geometry::{Point, Polygon};
use crate::{ASPECT_TOLERANCE, CollageDoc, PixelSize, Slot, Template};

/// The name `AGENTS.md`'s verification command uses. S2 keeps this name and
/// freezes the geometry behind [`TEMPLATE_VERSION`]; changing either afterwards
/// would change the layout of an existing project.
pub const SMOKE_TEMPLATE: &str = "mosaic-8-s14";

/// Geometry version of the smoke template.
pub const TEMPLATE_VERSION: u32 = 1;

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

/// Templates with exactly `slots` slots, in library order (S14).
///
/// The layout stage's own query, and since S14b the *only* place that filter is
/// written: [`Selection::layouts`](crate::Selection::layouts) is this call on the
/// selection's length, and the CLI's `templates --slots` is this call on its own
/// argument. The three used to be one rule with three expressions of it, which is
/// exactly the drift the count filter cannot afford — the strip and the CLI have to
/// agree about which layouts a count offers.
pub fn with_slots(slots: usize) -> Vec<Template> {
    frozen::TEMPLATES
        .iter()
        .filter(|entry| entry.slots.len() == slots)
        .map(thaw)
        .collect()
}

/// The box one layout candidate is drawn in, in logical pixels.
///
/// The layout band's own surface — and S18's `switch --band` measures that same
/// rebuild from the CLI, so the box lives here beside [`candidate_grid`] rather
/// than in either caller: two copies of it would be two answers to "how big is a
/// candidate". 128x96 is the largest box that leaves the canvas the majority of the
/// page at the default window (S14).
pub const CANDIDATE_BOX: (i32, i32) = (128, 96);

/// The grid one candidate is rendered at: the largest grid with `aspect`'s shape
/// that fits inside [`CANDIDATE_BOX`].
///
/// A candidate is a real render at a smaller size (`docs/CONTRACT.md` §5), so its
/// pixels are comparable with `pixlay-render render` of the same document at this
/// grid.
pub fn candidate_grid(aspect: f64) -> PixelSize {
    PixelSize::fit_in_bounds(
        aspect,
        (f64::from(CANDIDATE_BOX.0), f64::from(CANDIDATE_BOX.1)),
    )
}

/// Looks a template up by name.
pub fn get(name: &str) -> Option<Template> {
    frozen::TEMPLATES
        .iter()
        .find(|entry| entry.name == name)
        .map(thaw)
}

/// Templates whose declared aspect ratio matches `aspect` within
/// [`ASPECT_TOLERANCE`], the tolerance the picker's query uses.
///
/// This is the picker's query (`docs/CONTRACT.md` §3): the sheet's shape is the
/// template's, so the aspect a caller names *is* the layout family it wants.
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
    CollageDoc::new(template.clone())
}
