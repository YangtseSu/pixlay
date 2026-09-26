//! The single rendering path: `draw(doc, images, target)` on Cairo.
//!
//! Boundary: no gtk. Preview and export must both go through this crate; a
//! second rendering implementation is forbidden. The canvas only blits and
//! clips — decoding, resampling, rotation interpolation and colour conversion
//! happen in `pixlay-imaging`, so Cairo never sees a bitmap that is not already
//! the right size.
//!
//! * [`draw`] is the one entry point: the same code renders a 1400 px preview
//!   and a 139.5 MP A0 sheet, a band at a time if needed.
//! * [`sketch_rgb8`] draws a template's *geometry* alone — its cells in paper,
//!   every cell's outline and the ground between them in ink — for the layout
//!   band and the CLI's `render --sketch`. It shares [`draw`]'s normalized→pixel
//!   path, so it is a drawing of the same geometry rather than a second model of
//!   it (S21).
//! * [`Bitmap`] is the decoder-agnostic input; [`Images`] maps cell index to
//!   bitmap. A cell with no bitmap stays white.
//! * Output is always composited over opaque white (project hard constraint), so
//!   an export never carries alpha.

mod bitmap;
mod draw;
mod error;
mod sketch;

pub use bitmap::{Bitmap, Images};
pub use draw::{Band, Rgb8Image, Target, draw, output_px, render_rgb8, render_surface, rgb8};
pub use error::RenderError;
pub use sketch::{Sketch, sketch_rgb8};
