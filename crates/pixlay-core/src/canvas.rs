//! Canvas size and the pixel budget.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::{MAX_CANVAS_PIXELS, MAX_DPI, MIN_DPI};

/// Millimetres per inch. The canvas is declared in millimetres and converted
/// here, at the render boundary; normalized geometry never sees pixels.
pub const MM_PER_INCH: f64 = 25.4;

/// Largest canvas edge the product accepts, in millimetres (2 m). A0 is 1189 mm.
pub const MAX_CANVAS_MM: f64 = 2000.0;

/// Physical canvas size in millimetres.
///
/// The export DPI is a render parameter, not part of the document: the same
/// project is rendered at preview scale and at 300 dpi by the same `draw`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasSpec {
    pub width_mm: f64,
    pub height_mm: f64,
}

impl CanvasSpec {
    pub const A4_PORTRAIT: Self = Self::new(210.0, 297.0);
    pub const A4_LANDSCAPE: Self = Self::new(297.0, 210.0);
    pub const A3_PORTRAIT: Self = Self::new(297.0, 420.0);
    pub const A3_LANDSCAPE: Self = Self::new(420.0, 297.0);
    pub const A0_PORTRAIT: Self = Self::new(841.0, 1189.0);
    pub const A0_LANDSCAPE: Self = Self::new(1189.0, 841.0);
    pub const SQUARE: Self = Self::new(200.0, 200.0);

    pub const fn new(width_mm: f64, height_mm: f64) -> Self {
        Self {
            width_mm,
            height_mm,
        }
    }

    /// Canvas built from an aspect ratio (`width / height`) and a long edge.
    /// Covers the 1:1, 3:2, 4:3 and 16:9 presets, and custom sizes.
    pub fn with_ratio(aspect: f64, long_edge_mm: f64) -> Self {
        if aspect >= 1.0 {
            Self::new(long_edge_mm, long_edge_mm / aspect)
        } else {
            Self::new(long_edge_mm * aspect, long_edge_mm)
        }
    }

    pub fn aspect(&self) -> f64 {
        self.width_mm / self.height_mm
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        for (what, value) in [
            ("canvas width (mm)", self.width_mm),
            ("canvas height (mm)", self.height_mm),
        ] {
            if !value.is_finite() || value <= 0.0 || value > MAX_CANVAS_MM {
                return Err(CoreError::OutOfRange {
                    what,
                    value,
                    min: 0.0,
                    max: MAX_CANVAS_MM,
                });
            }
        }
        Ok(())
    }

    /// Pixel size at `dpi`.
    ///
    /// Rounding rule: `round(mm / 25.4 * dpi)`, half away from zero — frozen so
    /// that a project's export size does not drift between builds. A4 at 300
    /// dpi lands exactly on 2480x3508; A0 at 300 dpi on 9933x14043.
    pub fn pixel_size(&self, dpi: u32) -> Result<PixelSize, CoreError> {
        if !(MIN_DPI..=MAX_DPI).contains(&dpi) {
            return Err(CoreError::DpiOutOfRange {
                dpi,
                min: MIN_DPI,
                max: MAX_DPI,
            });
        }
        let dpi = f64::from(dpi);
        let width = (self.width_mm / MM_PER_INCH * dpi).round().max(1.0);
        let height = (self.height_mm / MM_PER_INCH * dpi).round().max(1.0);
        let pixels = (width as u64).saturating_mul(height as u64);
        if pixels > MAX_CANVAS_PIXELS {
            return Err(CoreError::CanvasTooLarge {
                pixels,
                max: MAX_CANVAS_PIXELS,
            });
        }
        Ok(PixelSize {
            width: width as i32,
            height: height as i32,
        })
    }
}

/// Canvas size in pixels at one DPI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelSize {
    pub width: i32,
    pub height: i32,
}

impl PixelSize {
    pub fn pixels(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }

    pub fn aspect(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }
}
