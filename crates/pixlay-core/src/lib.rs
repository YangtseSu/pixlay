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
//!   slot width), so swapping a photo does not move the visible area.
//!
//! Template generation (S2), the framing clamp (S3) and command history, project
//! writing and hit testing (S6.5) build on these types.

mod canvas;
mod crop;
mod doc;
mod error;
mod geometry;
mod template;
mod text;

pub use canvas::{CanvasSpec, MAX_CANVAS_MM, MM_PER_INCH, PixelSize};
pub use crop::{CropFit, CropTransform};
pub use doc::{Cell, CollageDoc, Project};
pub use error::CoreError;
pub use geometry::{EPSILON, Point, Polygon, Rect};
pub use template::{AREA_TOLERANCE, SharedEdge, Slot, Template};
pub use text::{
    Anchor, Rgba8, TextFallback, TextLayer, TextMode, TextToken, TextTokenUse, scan_tokens,
};

/// Version of the document format this build reads and writes.
pub const DOC_VERSION: u32 = 1;

/// Smallest and largest slot count a template may declare.
pub const MIN_SLOTS: usize = 2;
pub const MAX_SLOTS: usize = 10;

/// DPI range accepted by the render and export boundary.
pub const MIN_DPI: u32 = 72;
pub const MAX_DPI: u32 = 600;

/// Largest canvas the product renders, in pixels. A0 at 300 dpi is 139.5 MP, so
/// this leaves ~43% of headroom.
pub const MAX_CANVAS_PIXELS: u64 = 200_000_000;

/// Largest framing rotation, in degrees. Rotation only crops edges, so beyond
/// this a slot would need absurd magnification to stay covered.
pub const MAX_ROTATION_DEG: f64 = 45.0;

/// Zoom above which the clamp reduces the rotation angle instead of magnifying
/// further (docs/STEPS.md, "细长格 clamp 退化").
pub const CLAMP_ZOOM_LIMIT: f64 = 1.5;
