//! The pixel grid of a render, and the pixel budget.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::{MAX_CANVAS_PIXELS, MAX_LONG_EDGE_PX, MAX_TEMPLATE_ASPECT, MIN_TEMPLATE_ASPECT};

/// A render's size in pixels.
///
/// Since S12d there is no physical canvas behind it: a grid is derived from the
/// template's aspect ratio and one long-edge pixel count
/// ([`PixelSize::for_long_edge`]), and never from millimetres times a
/// resolution — a raster's only intrinsic size is its pixels. Geometry inside
/// the document is normalized; absolute pixels exist only here, at the render
/// and export boundary (`AGENTS.md`, "Hard constraints").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelSize {
    pub width: i32,
    pub height: i32,
}

impl PixelSize {
    /// The grid whose **long edge is exactly `long_edge_px`**, the other edge
    /// keeping `aspect`, rounded half away from zero and never below one pixel —
    /// so a square stays exactly square and every other shape stays within half
    /// a pixel of its ratio.
    ///
    /// The pixel count is the whole request (S12d): there is no resolution behind
    /// it and nothing else to derive. `aspect` is the template's
    /// (`Template::aspect`), which is also why a grid-vs-template aspect check
    /// cannot exist: the grid's shape *is* the template's.
    pub fn for_long_edge(aspect: f64, long_edge_px: u32) -> Result<Self, CoreError> {
        // The aspect first: it is what every branch below multiplies with, and a
        // value the arithmetic cannot use — `NaN`, zero, a negative, an infinity —
        // must be refused before a grid is derived from it, not turned into a
        // one-pixel-by-N shape by the rounding (S15e, PIX-027A). The domain is the
        // template's own (`MIN_TEMPLATE_ASPECT..=MAX_TEMPLATE_ASPECT`), because a
        // grid's shape *is* its template's.
        if !aspect.is_finite() || !(MIN_TEMPLATE_ASPECT..=MAX_TEMPLATE_ASPECT).contains(&aspect) {
            return Err(CoreError::OutOfRange {
                what: "template aspect ratio",
                value: aspect,
                min: MIN_TEMPLATE_ASPECT,
                max: MAX_TEMPLATE_ASPECT,
            });
        }
        if long_edge_px == 0 || long_edge_px > MAX_LONG_EDGE_PX {
            return Err(CoreError::OutOfRange {
                what: "long edge (px)",
                value: f64::from(long_edge_px),
                min: 1.0,
                max: f64::from(MAX_LONG_EDGE_PX),
            });
        }
        let long = f64::from(long_edge_px);
        let short = if aspect >= 1.0 {
            (long / aspect).round().max(1.0)
        } else {
            (long * aspect).round().max(1.0)
        };
        let pixel = if aspect >= 1.0 {
            Self {
                width: long_edge_px as i32,
                height: short as i32,
            }
        } else {
            Self {
                width: short as i32,
                height: long_edge_px as i32,
            }
        };
        pixel.validate()?;
        Ok(pixel)
    }

    /// Refuses a grid past the canvas pixel budget.
    ///
    /// The budget is checked wherever a grid is **asked for**, not only where one
    /// is derived from a long edge (S15e, PIX-003): `render --preview-px` scales
    /// the base grid, and the scaled grid is what gets allocated, so a square
    /// template at `--preview-px 20000` is 400 MP and is refused here rather than
    /// decoded and drawn into a surface the machine cannot serve.
    pub fn validate(&self) -> Result<(), CoreError> {
        let pixels = self.pixels();
        if pixels > MAX_CANVAS_PIXELS {
            return Err(CoreError::CanvasTooLarge {
                pixels,
                max: MAX_CANVAS_PIXELS,
            });
        }
        Ok(())
    }

    pub fn pixels(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }

    pub fn aspect(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }
}
