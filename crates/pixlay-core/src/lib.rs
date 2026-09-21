//! Document model, templates, geometry, framing transforms, command history.
//!
//! Boundary: this crate must not depend on gtk or cairo, and must stay testable
//! without a display. Geometry is normalized — coordinates live in `[0, 1]`;
//! absolute pixels exist only at the render and export boundary.
//!
//! The v1 contract frozen by S1 (review copy: `docs/CONTRACT.md`):
//!
//! * [`CollageDoc`] is the whole document — canvas, frozen template geometry,
//!   one [`Cell`] per slot, and canvas-level [`TextLayer`]s. It is the only shape
//!   ever serialized to `.pixlay`, and it embeds its template geometry so the
//!   layout of a saved project cannot change under it.
//! * Documents carry `docVersion`; a file written by a newer version is rejected
//!   instead of guessed at or downgraded.
//! * Limits (slot count, dpi, canvas pixels, rotation) are enforced by
//!   [`CollageDoc::validate`] and reported as typed errors, never as panics.
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
mod geometry;
mod grade;
mod history;
mod template;
mod text;

/// The template library: name to frozen geometry.
///
/// A module rather than a flat re-export, because its verbs (`get`, `names`,
/// `document`) would collide with nothing useful as bare functions and callers
/// should be able to read where the geometry came from.
pub mod templates;

pub use canvas::{CanvasSpec, MAX_CANVAS_MM, MM_PER_INCH, PixelSize};
pub use crop::{CropFit, CropTransform, DisplayRegion};
pub use doc::{Cell, CollageDoc, Project};
pub use error::CoreError;
pub use geometry::{EPSILON, Point, Polygon, Rect};
pub use grade::{
    FilterPreset, GRADE_DELTA_RANGE, GRADE_FACTOR_RANGE, GRADE_SATURATION_RANGE, Grade,
};
pub use history::{Command, History};
pub use template::{AREA_TOLERANCE, SharedEdge, Slot, Template};
pub use text::{
    Anchor, Rgba8, TextFallback, TextLayer, TextMode, TextToken, TextTokenUse, TextValues,
    scan_tokens, tiled_grid,
};

/// Version of the document format this build reads and writes.
pub const DOC_VERSION: u32 = 1;

/// Oldest `docVersion` this build reads.
///
/// Policy (docs/CONTRACT.md §1): the format is read at exactly one version — this
/// one. A change that only *adds* a field does not bump `DOC_VERSION` (the field
/// carries a `serde` default, so existing projects still load); a change that
/// alters an existing field's meaning or removes one bumps it, and projects from
/// the old version are then refused with an actionable message. There is no
/// migration by decision (S1 review, 2026-09-20).
pub const DOC_VERSION_MIN: u32 = 1;

/// Tolerance when comparing the canvas aspect with the template's declared
/// aspect. Both are authored separately, so an exact comparison would reject a
/// project whose canvas was rounded in millimetres; 1e-6 is ~1e-3 px on A0.
pub const ASPECT_TOLERANCE: f64 = 1e-6;

/// Smallest and largest slot count a template may declare.
pub const MIN_SLOTS: usize = 2;
pub const MAX_SLOTS: usize = 10;

/// DPI range accepted by the render and export boundary.
pub const MIN_DPI: u32 = 72;
pub const MAX_DPI: u32 = 600;

/// Longest edge an export asked for *pixels* may have, in pixels.
///
/// Bounded because the pixel grid, not the DPI, decides the output size in that
/// mode: A0 at the maximum DPI (600) is 28087 px on the long edge, so 30000
/// covers every resolution this product accepts with a little room, and anything
/// larger is a typo rather than a print. The canvas pixel budget
/// ([`MAX_CANVAS_PIXELS`]) is checked as well, so a square 30000 px request is
/// refused for its area even though its edge is inside this range.
pub const MAX_LONG_EDGE_PX: u32 = 30000;

/// Largest canvas the product renders, in pixels. A0 at 300 dpi is 139.5 MP, so
/// this leaves ~43% of headroom.
pub const MAX_CANVAS_PIXELS: u64 = 200_000_000;

/// Largest framing zoom accepted, as displayed photo width over slot width.
///
/// The zoom sizes the decoded bitmap (`slot pixels * zoom`), so it needs an upper
/// bound or the dimension arithmetic overflows: measured before this constant
/// existed, `zoom = 1e5` aborted the process on a failed 30-petabyte allocation
/// and `zoom = 1e308` wrapped the bitmap width to `i32::MIN`. 1000x is far beyond
/// any real framing.
pub const MAX_ZOOM: f64 = 1000.0;

/// Largest framing rotation, in degrees. Rotation only crops edges, so beyond
/// this a slot would need absurd magnification to stay covered.
pub const MAX_ROTATION_DEG: f64 = 45.0;

/// Multiple of the upright covering zoom above which [`CropTransform::fit`]
/// reduces the requested rotation angle instead of magnifying the photo further
/// (`docs/CONTRACT.md` §2, the clamp-degradation row).
///
/// The reference is the *upright floor*: the zoom the slot's shape and the
/// photo's aspect demand with the photo centred and unrotated. Measured that way
/// a slot which is inherently narrow — the ten-column strip needs 6x for a 4:3
/// photo — is not degraded for that reason alone, and rotating such a slot costs
/// *less* than leaving it upright. What is refused is rotation that magnifies
/// without bound: covering a slot of physical aspect `r` with a matching photo
/// needs `r*sin(t) + cos(t)` at angle `t`, so the widest angle this limit keeps is
/// the one where that reaches 1.5 — measured (2026-09-21, matching photo, 45
/// degrees requested) 45 degrees kept for a square slot, 34.0 for 6:5, 27.3 for
/// 4:3, 22.6 for 3:2, 18.0 for 16:9, 11.2 for 8:3, and the same angles mirrored
/// for slots taller than they are wide. Raising this constant is what widens that
/// range; the photo is magnified by exactly as much as the angle it keeps needs.
pub const CLAMP_ZOOM_LIMIT: f64 = 1.5;
