// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pixel grid of a render, the pixel budget, and the rules a surface derives a
//! grid with.
//!
//! Two of those rules are the shell's own surfaces and are here rather than in the
//! shell for one reason (S18): the CLI's `switch` ruler measures the layout change the
//! window's click makes, so it has to derive the *same* grid the window draws — the
//! canvas's, from the widget the document is shown in (less [`CANVAS_MARGIN`]), and the
//! band's candidate grid ([`templates::candidate_grid`](crate::templates::candidate_grid)).
//! A second copy of either number would be a second answer to "what grid is the window
//! drawing", which is exactly what the two numbers are compared about.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::{MAX_CANVAS_PIXELS, MAX_LONG_EDGE_PX, MAX_TEMPLATE_ASPECT, MIN_TEMPLATE_ASPECT};

/// Space between a canvas widget's edge and the sheet it shows, in device pixels.
///
/// It is the canvas view's own placement of its content, not styling: no style
/// class or CSS variable describes "how far the paper sits from the pane", and it
/// deliberately does not come from the theme (a sheet of paper has the same
/// margin in dark and light mode).
pub const CANVAS_MARGIN: f64 = 12.0;

/// The grid a canvas widget of `width` x `height` asks for, showing a document of
/// `aspect`: the largest grid of the document's shape inside the widget, less
/// [`CANVAS_MARGIN`] on every side.
///
/// The grid follows the **document**, not the widget: a 4:3 sheet and a 16:9 one in
/// the same window are two differently shaped grids (measured 2026-09-25 at the
/// editor's default 1100x760 window, whose canvas widget is 1100x575: 735x551 and
/// 980x551). That is why a layout change moves the preview-grade edge with it, and
/// why the CLI's ruler takes the widget's size rather than one long edge.
pub fn canvas_grid(aspect: f64, width: i32, height: i32) -> PixelSize {
    PixelSize::fit_in_bounds(
        aspect,
        (
            (f64::from(width) - 2.0 * CANVAS_MARGIN).max(1.0),
            (f64::from(height) - 2.0 * CANVAS_MARGIN).max(1.0),
        ),
    )
}

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

    /// The largest grid with `aspect`'s shape that fits inside a `bounds`-sized
    /// box, each edge rounded half away from zero and never below one pixel.
    ///
    /// One rule, three callers, which is why it is here rather than written out in
    /// each: the canvas's preferred grid (the widget's own size, less its margin),
    /// the layout band's candidate grid (a fixed box,
    /// [`templates::candidate_grid`](crate::templates::candidate_grid)) and the
    /// CLI's `switch --band` (S18), which measures that same band rebuild from the
    /// machine side and must not derive its geometry a second way. `bounds` is the
    /// caller's own units.
    pub fn fit_in_bounds(aspect: f64, bounds: (f64, f64)) -> Self {
        let (width, height) = bounds;
        let (w, h) = if width / height > aspect {
            (height * aspect, height)
        } else {
            (width, width / aspect)
        };
        Self {
            width: (w.round() as i32).max(1),
            height: (h.round() as i32).max(1),
        }
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
