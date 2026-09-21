//! The canvas frame: the gap between cells, their corner radius, and the backdrop.
//!
//! The frame is the canvas **decoration** stage of the frozen evaluation order
//! (`AGENTS.md`): it sits after the slots are composited and before the text
//! layers, and it is the one stage that paints *around* the photos instead of
//! inside them. Three fields, three jobs:
//!
//! * `gap_rel` takes half of itself off every side of a cell, which is what makes
//!   two neighbouring cells show a stripe of the canvas between them;
//! * `radius_rel` rounds the corners of what is left;
//! * `color` is what the canvas is painted with where no photo covers it — the
//!   gaps, the corners, an empty cell, and everything outside the slots.
//!
//! **Both lengths are fractions of the canvas height**, like a text layer's
//! `sizeRel`, so a frame is resolution-independent and a preview shows exactly
//! what an export does.
//!
//! # What the frame changes about the framing clamp
//!
//! A cell's *visible* area is `slot.outline ∩ rounded_rect(slot_bbox inset by
//! gapRel/2, radiusRel)`, and the clamp's job is that the photo covers it. The
//! reference the clamp measures against ([`Frame::covering`]) is the outline
//! clipped to the inset rectangle:
//!
//! * it contains everything visible, so covering it covers the cell, for a
//!   concave slot as much as for a rectangle, and it is a polygon — so the clamp
//!   stays S3's vertex test;
//! * it is *exactly* the inset rectangle for a rectangular slot, which is every
//!   slot in the library but one, so the gap does not magnify the photo: it
//!   crops it at the frame, the way a mount crops a print;
//! * with no gap the clip is the identity, so the reference is the outline itself
//!   and a project written before the frame existed fits to the same bits.
//!
//! The corner rounding is **not** subtracted from that reference: the visible
//! region is a rounded rectangle, whose exact support needs circular arcs, and
//! the clamp's reference stays a polygon. Covering the full rectangle therefore
//! asks for a little more zoom than the rounded corners strictly need — bounded
//! by the radius, and never so much that the frame is visible as a jump: at
//! radius 0 (the default) there is no difference at all, and the case is
//! measured in `docs/CONTRACT.md` §8.
//!
//! # Why the frame is not a template
//!
//! A *baked* gutter is template geometry: it changes which pixels belong to which
//! cell and therefore the template's name and `templateVersion`
//! (`grid-4-2x2g` is one). The frame is a render-time parameter of *any* layout,
//! so a project can gain or lose a frame without its layout changing.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geometry::{Polygon, Rect};
use crate::template::Slot;
use crate::text::Rgba8;

/// Largest `gapRel` / `radiusRel` the format accepts, as a fraction of the canvas
/// height.
///
/// Not a design bound but a typo bound: a length past the whole canvas height is
/// not a frame around anything. A gap *inside* this range can still empty a small
/// cell, and that is refused per slot by [`CollageDoc::validate`], which names
/// the slot; this one is what makes the error message name a range.
///
/// [`CollageDoc::validate`]: crate::CollageDoc::validate
pub const MAX_FRAME_REL: f64 = 1.0;

/// The canvas frame: gap, corner radius, and the colour of the canvas itself.
///
/// Every field defaults to what the renderer painted before the field existed —
/// no gap, square corners, white — so a project written before S11 renders
/// byte-identically, and adding the shape did not bump `DOC_VERSION`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Frame {
    /// Gap between neighbouring cells, as a fraction of the canvas height.
    ///
    /// Half of it is taken off every side of every cell, so two cells that share
    /// an edge are `gapRel` apart and a cell on the canvas border is `gapRel/2`
    /// from it.
    #[serde(default)]
    pub gap_rel: f64,
    /// Corner radius of a cell, as a fraction of the canvas height.
    ///
    /// Clamped at use to half the smaller side of the cell's inset rectangle, so
    /// a large request rounds the corners into a stadium rather than folding the
    /// cell inside out.
    #[serde(default)]
    pub radius_rel: f64,
    /// The canvas backdrop: what shows in the gaps, outside a rounded corner,
    /// outside every slot, and in an empty cell.
    ///
    /// Opaque by construction (`validate` refuses a translucent one): the backdrop
    /// is *painted*, not blended, so that preview and export stay the same picture
    /// and an export is never transparent. White is the default, which is what
    /// keeps every project written before this field byte-identical.
    #[serde(default = "white")]
    pub color: Rgba8,
}

/// `Frame::color`'s default, as a function because serde wants one.
fn white() -> Rgba8 {
    Rgba8::WHITE
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            gap_rel: 0.0,
            radius_rel: 0.0,
            color: Rgba8::WHITE,
        }
    }
}

impl Frame {
    /// True when the frame changes no geometry: no gap, no radius.
    ///
    /// The renderer's shortcut, and the honest one: with no gap the cell's inset
    /// rectangle is its bounding box, so clipping to it again is clipping to a
    /// superset and the drawn pixels are the outline's own — which is what makes
    /// an unframed project byte-identical to a build with no frame at all.
    ///
    /// The colour is not geometry, so it does not appear here: a red backdrop with
    /// no gap and no radius is still the identity *clip*, and only the base fill
    /// changes.
    pub fn is_identity(&self) -> bool {
        self.gap_rel == 0.0 && self.radius_rel == 0.0
    }

    /// A cell's inset rectangle: its bounding box, `gapRel/2` off every side.
    ///
    /// Normalized coordinates, so the horizontal inset is divided by the canvas
    /// aspect — the gap is a length, and a length means different fractions of the
    /// two axes.
    fn inset(&self, slot: &Slot, canvas_aspect: f64) -> Rect {
        let bbox = slot.outline.bbox();
        let x = self.gap_rel / 2.0 / canvas_aspect;
        let y = self.gap_rel / 2.0;
        Rect {
            x0: bbox.x0 + x,
            y0: bbox.y0 + y,
            x1: bbox.x1 - x,
            y1: bbox.y1 - y,
        }
    }

    /// The region of `slot` a photo has to cover: the outline clipped to the inset
    /// rectangle.
    ///
    /// `None` when the gap leaves the cell with nothing visible — no inset
    /// rectangle at all, or a clipping of the outline that has no interior. That is
    /// an error a document reports (`CollageDoc::validate` names the slot), not a
    /// framing a renderer can invent.
    ///
    /// The radius is not part of this: see the module's comment.
    pub fn covering(&self, slot: &Slot, canvas_aspect: f64) -> Option<Polygon> {
        if !canvas_aspect.is_finite() || canvas_aspect <= 0.0 {
            return None;
        }
        let inset = self.inset(slot, canvas_aspect);
        if inset.width() <= 0.0 || inset.height() <= 0.0 {
            return None;
        }
        let clipped = slot.outline.clipped_to(inset);
        (clipped.points.len() >= Polygon::MIN_VERTICES && clipped.area() > 0.0).then_some(clipped)
    }

    /// Where the canvas clips a cell to: the inset rectangle in normalized canvas
    /// coordinates, and the corner radius as a fraction of the canvas height.
    ///
    /// The radius is clamped here, to half the smaller side of the inset rectangle
    /// — measured in canvas-height units on both axes, since the two axes are not
    /// the same length. A call for a cell the frame emptied is a caller bug: the
    /// rectangle is then degenerate and clips everything away, which is what a
    /// renderer that already refused the document in `covering` never reaches.
    pub fn clip(&self, slot: &Slot, canvas_aspect: f64) -> (Rect, f64) {
        let inset = self.inset(slot, canvas_aspect);
        let width = (inset.width() * canvas_aspect).max(0.0);
        let height = inset.height().max(0.0);
        let radius = self.radius_rel.min(width.min(height) / 2.0).max(0.0);
        (inset, radius)
    }

    /// Checks the frame's own numbers, without looking at a slot.
    pub fn validate(&self) -> Result<(), CoreError> {
        for (what, value) in [
            ("frame gap (fraction of canvas height)", self.gap_rel),
            (
                "frame corner radius (fraction of canvas height)",
                self.radius_rel,
            ),
        ] {
            if !value.is_finite() {
                return Err(CoreError::NotFinite { what, value });
            }
            if !(0.0..=MAX_FRAME_REL).contains(&value) {
                return Err(CoreError::OutOfRange {
                    what,
                    value,
                    min: 0.0,
                    max: MAX_FRAME_REL,
                });
            }
        }
        if self.color.a != 255 {
            // Not a taste decision: a translucent backdrop is composited over
            // whatever the surface already held, and the export's surface starts
            // transparent. "Preview and export are the same picture" and "an export
            // is never transparent" both stop being true, so it is refused where the
            // user can still change it.
            return Err(CoreError::OutOfRange {
                what: "frame colour alpha",
                value: f64::from(self.color.a),
                min: 255.0,
                max: 255.0,
            });
        }
        Ok(())
    }
}
