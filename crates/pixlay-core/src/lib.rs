//! Document model, templates, geometry, framing transforms, command history.
//!
//! Boundary: this crate must not depend on gtk or cairo, and must stay testable
//! without a display. Geometry is normalized — coordinates live in `[0, 1]`;
//! absolute pixels exist only at the render and export boundary.
//!
//! The v1 contract frozen by S1 (review copy: `docs/CONTRACT.md`):
//!
//! * [`CollageDoc`] is the whole document — frozen template geometry, one
//!   [`Cell`] per slot, and the frame around them. It is the only shape
//!   ever serialized to `.pixlay`, and it embeds its template geometry so the
//!   layout of a saved project cannot change under it. It carries no size:
//!   the template's aspect is the sheet's shape, and a render's pixel grid is
//!   a parameter of the render (`PixelSize::for_long_edge`, S12d).
//! * Documents carry `docVersion`; a file written by a newer version is rejected
//!   instead of guessed at or downgraded.
//! * Limits (slot count, long edge, canvas pixels, zoom, the frame's two lengths)
//!   are enforced by [`CollageDoc::validate`] and reported as typed errors, never
//!   as panics. Rotation is *not* one of them: since 2026-09-22 any finite angle
//!   is legal and every finite angle is normalized into `(-180, 180]`.
//! * Framing state is absolute ([`CropTransform::zoom`] is displayed width over
//!   slot width), so swapping a photo does not move the visible area, and what is
//!   drawn is the *fit* of the stored request ([`CropTransform::fit`]), so a
//!   photo always covers its slot.
//!
//! Template generation (S2), the framing clamp (S3) and command history, project
//! writing and hit testing (S6.5) build on these types.

mod canvas;
mod crop;
mod doc;
mod error;
mod frame;
mod geometry;
mod history;
mod selection;
mod template;

/// The template library: name to frozen geometry.
///
/// A module rather than a flat re-export, because its verbs (`get`, `names`,
/// `document`) would collide with nothing useful as bare functions and callers
/// should be able to read where the geometry came from.
pub mod templates;

pub use canvas::PixelSize;
pub use crop::{CropFit, CropTransform, DisplayRegion};
pub use doc::{Cell, CollageDoc, Project, relative_to};
pub use error::CoreError;
pub use frame::{Frame, MAX_FRAME_REL, Rgba8};
pub use geometry::{EPSILON, Point, Polygon, Rect};
pub use history::{Command, History};
pub use selection::{
    MAX_PHOTOS, MIN_PHOTOS, Removed, Selection, SelectionError, last_photo, layout_for, remove_last,
};
pub use template::{AREA_TOLERANCE, Family, SharedEdge, Slot, Template};

/// Version of the document format this build reads and writes.
///
/// **3 since S12d**: pixels-only removed the `canvas` field, which is the one
/// change the policy below says bumps this number. A version-1 or version-2
/// project is refused with `VersionUnsupported` instead of a `serde`
/// unknown-field error, and there is no migration: a file from either shape is a
/// file this build cannot express (the rulings of 2026-09-22,
/// `docs/2026-09-22-STEPS.md`).
pub const DOC_VERSION: u32 = 3;

/// Oldest `docVersion` this build reads.
///
/// Policy (docs/CONTRACT.md §1): the format is read at exactly one version — this
/// one. A change that only *adds* a field does not bump `DOC_VERSION` (the field
/// carries a `serde` default, so existing projects still load); a change that
/// alters an existing field's meaning or removes one bumps it, and projects from
/// the old version are then refused with an actionable message. There is no
/// migration by decision (S1 review, 2026-09-20).
pub const DOC_VERSION_MIN: u32 = 3;

/// Largest canvas the product renders, in pixels.
///
/// Measured (S0, 2026-09-20): the largest grid this product has ever rendered is
/// 139.5 MP, and the budget leaves ~43% of headroom over it; `peak VmHWM` at that
/// grid is 1340 MB including encoding, inside the 2.5 GB budget
/// (`docs/CONTRACT.md` §8).
pub const MAX_CANVAS_PIXELS: u64 = 200_000_000;

/// Tolerance for the template-aspect picker query (`templates::of_aspect`): a
/// caller names an aspect by rounding (`16:9`, `1.5`), so an exact comparison
/// would miss a template whose aspect is computed from slot geometry.
pub const ASPECT_TOLERANCE: f64 = 1e-6;

/// Smallest and largest slot count a template may declare.
///
/// The ceiling was 10 while `strip-10-10x1` shipped; S12c removed that recipe, so
/// the format's cap and the picker's ([`MAX_PHOTOS`]) are the same number again.
pub const MIN_SLOTS: usize = 2;
pub const MAX_SLOTS: usize = 9;

/// Longest edge a render or export may be asked for, in pixels.
///
/// Bounded because the long edge is the one parameter that decides the output
/// size (S12d): past this, a request is a typo rather than an image. The canvas
/// pixel budget ([`MAX_CANVAS_PIXELS`]) is checked as well, so a square request
/// inside this range is still refused for its area.
pub const MAX_LONG_EDGE_PX: u32 = 30000;

/// Largest framing zoom accepted, as displayed photo width over slot width.
///
/// The zoom sizes the decoded bitmap (`slot pixels * zoom`), so it needs an upper
/// bound or the dimension arithmetic overflows: measured before this constant
/// existed, `zoom = 1e5` aborted the process on a failed 30-petabyte allocation
/// and `zoom = 1e308` wrapped the bitmap width to `i32::MIN`. 1000x is far beyond
/// any real framing — and beyond what the free rotation asks for: the worst
/// covering zoom over every shipped slot, angle and photo aspect measures **21.73**
/// (`docs/CONTRACT.md` §8, S11: a 2.4:1 photo in `strip-9-9x1`'s 1/16-wide pane),
/// 46 times below the cap.
pub const MAX_ZOOM: f64 = 1000.0;
