//! Per-slot framing state.

use serde::{Deserialize, Serialize};

use crate::MAX_ROTATION_DEG;
use crate::MAX_ZOOM;
use crate::error::CoreError;

/// How one photo is framed inside one slot.
///
/// The state is *absolute*, not a multiple of "fill the slot": `zoom` is the
/// displayed photo width divided by the slot width, so swapping a photo for one
/// with a different aspect ratio does not move the visible area (a multiple of
/// fill changes meaning with the aspect ratio, so the framing would jump).
///
/// The abstraction is "parent container clips, child transform places": the slot
/// polygon is the clip and this transform is applied inside it. The renderer
/// never recomputes a crop rectangle per frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CropTransform {
    /// Displayed photo width / slot width. The lower bound that still covers the
    /// slot is `max(1, photo_aspect * slot_height / slot_width)`; S3 clamps to it.
    pub zoom: f64,
    /// Photo centre offset from the slot centre, in slot widths and heights.
    /// `(0, 0)` is centred.
    pub offset: (f64, f64),
    /// Rotation in degrees about the photo centre, **clockwise on screen** (the
    /// canvas has y pointing down, and cairo's `rotate` is clockwise in that
    /// space; the renderer passes this value through unchanged).
    ///
    /// Rotation only crops edges — the canvas and the slot never grow, so the
    /// clamp recomputes `zoom` and this angle must stay within the covering
    /// bound. S3 owns that fit.
    pub rotation_deg: f64,
}

impl Default for CropTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: (0.0, 0.0),
            rotation_deg: 0.0,
        }
    }
}

impl CropTransform {
    /// The identity framing: the photo exactly fills the slot when the aspect
    /// ratios match.
    pub const IDENTITY: Self = Self {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg: 0.0,
    };

    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            return Err(CoreError::OutOfRange {
                what: "crop zoom",
                value: self.zoom,
                min: f64::MIN_POSITIVE,
                max: MAX_ZOOM,
            });
        }
        // An upper bound is not cosmetic: the zoom multiplies the slot's pixel
        // size to size the decoded bitmap, so an unbounded value either overflows
        // the dimension arithmetic or asks for an allocation the machine cannot
        // satisfy. 1000x is far past any real framing and still leaves the
        // product of zoom and a 200 MP canvas inside i32.
        if self.zoom > MAX_ZOOM {
            return Err(CoreError::OutOfRange {
                what: "crop zoom",
                value: self.zoom,
                min: f64::MIN_POSITIVE,
                max: MAX_ZOOM,
            });
        }
        for (what, value) in [
            ("crop offset x (slot widths)", self.offset.0),
            ("crop offset y (slot heights)", self.offset.1),
        ] {
            // Beyond half a slot the photo centre leaves the slot entirely, and
            // no clamp could ever cover it again.
            if !value.is_finite() || value.abs() > 1.0 {
                return Err(CoreError::OutOfRange {
                    what,
                    value,
                    min: -1.0,
                    max: 1.0,
                });
            }
        }
        if !self.rotation_deg.is_finite() || self.rotation_deg.abs() > MAX_ROTATION_DEG {
            return Err(CoreError::OutOfRange {
                what: "crop rotation (degrees)",
                value: self.rotation_deg,
                min: -MAX_ROTATION_DEG,
                max: MAX_ROTATION_DEG,
            });
        }
        Ok(())
    }
}

/// Outcome of clamping a requested transform against a slot's geometry.
///
/// The stored transform is a request; what gets drawn is the fit. Below the
/// zoom limit a sliver-shaped slot is covered by magnifying the photo, and past
/// it the requested rotation angle is reduced instead of magnifying further
/// (docs/STEPS.md, "Open decisions → B. Confirmed": elongated-slot clamp degradation). The GUI needs to know which of the two
/// happened so it can say so; the renderer only needs `transform`.
///
/// Produced by the fit function that S3 adds to this module.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropFit {
    /// The transform that actually gets drawn.
    pub transform: CropTransform,
    /// True when the requested rotation angle could not be honoured.
    pub rotation_limited: bool,
}
