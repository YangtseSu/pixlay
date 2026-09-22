//! Template geometry: the frozen layout a document carries.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geometry::{EPSILON, Point, Polygon};
use crate::{MAX_SLOTS, MIN_SLOTS};

/// Tolerance between a slot's declared area and the area of its outline.
/// 1e-6 of a sheet edge is 0.014 px on the 14043-px reference grid.
pub const AREA_TOLERANCE: f64 = 1e-6;

/// One template cell: where it is and how much of the canvas it covers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slot {
    /// Closed outline in normalized canvas coordinates.
    pub outline: Polygon,
    /// Declared area as a fraction of the canvas. "Cut" templates must declare
    /// areas summing to exactly 1.0, and the declared value is cross-checked
    /// against the outline so the two can never drift apart.
    pub area: f64,
}

/// A template's geometry.
///
/// The geometry is *data*, not a procedure: a document embeds a copy of it, so
/// changing the template library can never change the layout of an existing
/// project. `version` is the frozen geometry version of the template family
/// member — regenerating a template must not touch an existing version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Template {
    /// Stable name, e.g. `mosaic-8-s14`.
    pub name: String,
    /// Geometry version of this template.
    pub version: u32,
    /// Aspect ratio (`width / height`) the geometry was authored for. The
    /// template matrix is grouped by aspect ratio (`docs/CONTRACT.md` §3).
    pub aspect: f64,
    /// Slots in template order. Slot identity is the index: cells, photos and
    /// hit tests all speak in terms of it.
    pub slots: Vec<Slot>,
}

impl Template {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::EmptyTemplateName);
        }
        if self.version == 0 {
            return Err(CoreError::OutOfRange {
                what: "template version",
                value: 0.0,
                min: 1.0,
                max: f64::from(u32::MAX),
            });
        }
        if !self.aspect.is_finite() || !(0.1..=10.0).contains(&self.aspect) {
            return Err(CoreError::OutOfRange {
                what: "template aspect ratio",
                value: self.aspect,
                min: 0.1,
                max: 10.0,
            });
        }
        if !(MIN_SLOTS..=MAX_SLOTS).contains(&self.slots.len()) {
            return Err(CoreError::SlotCount {
                found: self.slots.len(),
                min: MIN_SLOTS,
                max: MAX_SLOTS,
            });
        }
        for (index, slot) in self.slots.iter().enumerate() {
            slot.outline
                .validate()
                .map_err(|reason| CoreError::InvalidSlot {
                    slot: index,
                    reason,
                })?;
            if !slot.area.is_finite() || slot.area <= 0.0 {
                return Err(CoreError::InvalidSlot {
                    slot: index,
                    reason: "declared area must be finite and positive",
                });
            }
            let outline = slot.outline.area();
            if (slot.area - outline).abs() > AREA_TOLERANCE {
                return Err(CoreError::SlotAreaMismatch {
                    slot: index,
                    declared: slot.area,
                    outline,
                });
            }
        }
        Ok(())
    }

    /// The slot a normalized canvas point falls in, or `None` when it falls in
    /// none of them (S6.5: point → slot hit testing).
    ///
    /// The question is about the *slot's geometry*, so it takes a point and not a
    /// document: a photo's framing — its zoom, its pan, its rotation — moves the
    /// picture inside the slot and can never move the slot, which is why nothing
    /// here looks at a [`Cell`](crate::Cell). The GUI asks this on every press and
    /// every drag; the CLI exposes it as `hit`.
    ///
    /// The test is the outline's own even-odd containment, which is exact for the
    /// irregular slot the library ships (an L shape) and for any simple polygon,
    /// not a bounding box: an L-shaped slot's notch belongs to its neighbour, and
    /// the answer for a point one pixel outside a shared edge is that neighbour.
    /// The matrix's cut templates tile the canvas, so exactly one slot answers
    /// there; the gutter template leaves its gutter to `None`, as does any point
    /// outside the canvas.
    ///
    /// A point exactly on a boundary is unspecified, the same way it is for
    /// [`Polygon::contains`]: the answer is deterministic for a given point, but
    /// which of the two slots it lands in is that function's business and not
    /// something a caller should rely on. Nothing in the product needs it —
    /// a press is a pixel, and the hit tests keep a band away from boundaries.
    pub fn slot_at(&self, point: Point) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.outline.contains(point))
    }

    /// Pairs of slots whose outlines share a stretch of boundary, with the
    /// shared segment itself.
    ///
    /// This is the seam a collage exposes: the place where two photos meet, and
    /// the only place where the renderer can blend one slot into another. The
    /// seam probes measure exactly these segments.
    pub fn shared_edges(&self) -> Vec<SharedEdge> {
        let mut found = Vec::new();
        for a in 0..self.slots.len() {
            for b in (a + 1)..self.slots.len() {
                for (a0, a1) in self.slots[a].outline.edges() {
                    for (b0, b1) in self.slots[b].outline.edges() {
                        if let Some(edge) = shared_segment(a, b, (a0, a1), (b0, b1)) {
                            found.push(edge);
                        }
                    }
                }
            }
        }
        found
    }
}

/// A stretch of boundary two slots share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharedEdge {
    pub a: usize,
    pub b: usize,
    pub from: Point,
    pub to: Point,
}

impl SharedEdge {
    pub fn length(&self) -> f64 {
        (self.to.x - self.from.x).hypot(self.to.y - self.from.y)
    }
}

/// The overlapping, collinear part of two edges, or `None` when they are not on
/// the same line or do not overlap.
fn shared_segment(
    a: usize,
    b: usize,
    (a0, a1): (Point, Point),
    (b0, b1): (Point, Point),
) -> Option<SharedEdge> {
    let (dx, dy) = (a1.x - a0.x, a1.y - a0.y);
    let len = dx.hypot(dy);
    let (ex, ey) = (b1.x - b0.x, b1.y - b0.y);
    let elen = ex.hypot(ey);
    if len <= 0.0 || elen <= 0.0 {
        return None;
    }
    // Sine of the angle between the edges.
    if ((dx * ey - dy * ex) / (len * elen)).abs() > EPSILON {
        return None;
    }
    // Distance of the second edge's endpoints from the first edge's line.
    let off_line = |p: Point| ((p.x - a0.x) * dy - (p.y - a0.y) * dx).abs() / len;
    if off_line(b0) > EPSILON || off_line(b1) > EPSILON {
        return None;
    }
    // Overlap interval along the first edge, in its parameter space.
    let along = |p: Point| ((p.x - a0.x) * dx + (p.y - a0.y) * dy) / (len * len);
    let (t0, t1) = (along(b0), along(b1));
    let lo = t0.min(t1).max(0.0);
    let hi = t0.max(t1).min(1.0);
    if hi - lo <= 0.0 {
        return None;
    }
    let at = |t: f64| Point::new(a0.x + t * dx, a0.y + t * dy);
    Some(SharedEdge {
        a,
        b,
        from: at(lo),
        to: at(hi),
    })
}
