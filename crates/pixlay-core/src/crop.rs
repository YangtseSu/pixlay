//! Per-slot framing state.

use serde::{Deserialize, Serialize};

use crate::MAX_ZOOM;
use crate::canvas::PixelSize;
use crate::error::CoreError;
use crate::geometry::{Point, Polygon, Rect};
use crate::template::Slot;

/// Halvings used by the one-dimensional search of [`CropTransform::fit`].
///
/// 60 halvings take a bracket below `2^-60` of its width, past `f64`'s
/// resolution at these magnitudes; a fixed count keeps the clamp's cost a
/// number rather than a convergence criterion, so the same document always fits
/// to the same bits.
const BISECTION_STEPS: u32 = 60;

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
    /// Displayed photo width / slot width. [`CropTransform::fit`] raises it to
    /// the lower bound that still covers the slot — `max(1, photo_aspect *
    /// slot_height / slot_width)` for a rectangular slot — and never lowers it.
    pub zoom: f64,
    /// Photo centre offset from the slot centre, in slot widths and heights.
    /// `(0, 0)` is centred. [`CropTransform::fit`] pulls it back toward the
    /// centre when the pan would uncover the slot.
    pub offset: (f64, f64),
    /// Rotation in degrees about the photo centre, **clockwise on screen** (the
    /// canvas has y pointing down, and cairo's `rotate` is clockwise in that
    /// space; the renderer passes this value through unchanged).
    ///
    /// Rotation only crops edges — the canvas and the slot never grow, so covering
    /// the slot at an angle costs magnification. **The angle is free**: any finite
    /// value is accepted, the fit never reduces it, and it is magnified to whatever
    /// covering that exact angle needs. A finite value is normalized to
    /// `(-180, 180]` by [`normalized`](Self::normalized), which every load and every
    /// edit applies, so a dial cannot accumulate turns in a project file.
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

    /// True when all four numbers are finite: no NaN, no infinity.
    ///
    /// Narrower than [`validate`](Self::validate) on purpose — the clamp caps a
    /// zoom that is past [`MAX_ZOOM`] instead of refusing it, so only the values
    /// no arithmetic can be done on are excluded here.
    fn is_finite(&self) -> bool {
        self.zoom.is_finite()
            && self.offset.0.is_finite()
            && self.offset.1.is_finite()
            && self.rotation_deg.is_finite()
    }

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
        if !self.rotation_deg.is_finite() {
            // No range: since 2026-09-22 the angle is free, so this is a *domain*
            // check (a number that arithmetic can be done on) rather than a bound,
            // and `NotFinite` is the error that says so without naming a range the
            // value is not being compared against.
            return Err(CoreError::NotFinite {
                what: "crop rotation (degrees)",
                value: self.rotation_deg,
            });
        }
        Ok(())
    }

    /// The same framing with the rotation wrapped into `(-180, 180]`.
    ///
    /// A dial has no reason to accumulate turns: 450 degrees and 90 degrees draw
    /// the same picture, and a project that said 450 would be a file whose numbers
    /// are not the framing. The wrap is idempotent, so applying it at every edit
    /// and again when a document is loaded cannot drift — and it is applied on the
    /// way *in*, so a project written by a build with the old ±45° cap (every value
    /// of which is inside the range) loads unchanged.
    ///
    /// A rotation that is not finite is returned as it is: a document that says
    /// `NaN` is refused by [`validate`](Self::validate), and an error message that
    /// quotes back something other than what the file said is a worse message.
    pub fn normalized(self) -> Self {
        if !self.rotation_deg.is_finite() {
            return self;
        }
        // `rem_euclid` gives `[0, 360)`, so shifting by half a turn gives `(-180,
        // 180]` as soon as the one value it excludes — exactly `-180` — is moved
        // to the `180` it equals. `-0.0` is left alone: it is inside the range and
        // it compares equal to `0.0`.
        let wrapped = (self.rotation_deg + 180.0).rem_euclid(360.0) - 180.0;
        let rotation_deg = if wrapped == -180.0 { 180.0 } else { wrapped };
        Self {
            rotation_deg,
            ..self
        }
    }
}

/// Outcome of clamping a requested transform against a cell's visible region.
///
/// The stored transform is a request; what gets drawn is the fit ([`fit`]).
/// Since 2026-09-22 there is exactly one lever pair left — the zoom is raised to
/// the value that covers, and the pan is pulled back into it — so this is the
/// drawn transform and nothing else.
///
/// [`fit`]: CropTransform::fit
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropFit {
    /// The transform that actually gets drawn.
    pub transform: CropTransform,
}

impl CropTransform {
    /// The framing that actually gets drawn: this request fitted to `covering`.
    ///
    /// A request is what the user asked for, a fit is what covers the cell. The
    /// canvas and the cell never grow, so the levers are the three fields, and each
    /// of them has exactly one:
    ///
    /// * `zoom` is raised to the value that covers the photo centred at **exactly
    ///   the requested angle**, and a larger request is kept as it is;
    /// * `offset` is pulled back along the line to the cell centre until the photo
    ///   covers again — a pan must stop at the frame edge rather than be paid for
    ///   with more magnification, or dragging a photo would zoom it;
    /// * `rotation_deg` is **never touched**: the angle is free (ruled 2026-09-22),
    ///   and the zoom is what pays for it. The ±45° cap and the
    ///   `CLAMP_ZOOM_LIMIT` rule that reduced an over-asking angle are gone, so the
    ///   fit returns the requested angle bit for bit.
    ///
    /// `covering` is the region the photo has to cover — the cell's visible
    /// geometry, which the document's frame narrows (`Frame::covering`); passing
    /// `&slot.outline` is the unframed case. `slot` is still what defines the
    /// *scale*: `zoom` means displayed photo width over the slot's bounding-box
    /// width, whatever the frame cut away.
    ///
    /// `canvas_aspect` is the aspect ratio of the space the slot's normalized
    /// geometry is stretched onto, `photo_aspect` the source bitmap's
    /// `width / height`. Both are needed because normalized coordinates carry no
    /// aspect ratio of their own; `draw` passes the aspect of the surface it
    /// places into, which is the space its own arithmetic uses.
    ///
    /// The fit is **idempotent** (fitting a fit returns it), so clamping on an
    /// edit and again at the render boundary cannot drift.
    ///
    /// Two boundaries the guarantee cannot cross, both reported rather than
    /// hidden: a request that is not four finite numbers, a degenerate `covering`
    /// or a non-finite aspect (no framing to fit against) returns the request
    /// untouched, and a slot so extreme that covering it needs more than
    /// [`MAX_ZOOM`] gets the cap instead of coverage.
    pub fn fit(
        &self,
        slot: &Slot,
        covering: &Polygon,
        canvas_aspect: f64,
        photo_aspect: f64,
    ) -> CropFit {
        let untouched = CropFit { transform: *self };
        if !self.is_finite() {
            return untouched;
        }
        let Some(frame) = Frame::new(slot, covering, canvas_aspect, photo_aspect) else {
            return untouched;
        };
        let zoom = frame
            .required_zoom(self.rotation_deg, frame.centre)
            .max(self.zoom)
            .min(MAX_ZOOM);
        CropFit {
            transform: Self {
                zoom,
                offset: frame.clamp_offset(self.offset, self.rotation_deg, zoom),
                rotation_deg: self.rotation_deg,
            },
        }
    }
}

/// The part of a placed photo that a slot can show, in the *displayed* photo's
/// own pixel grid.
///
/// `pixlay-imaging` sizes and crops a bitmap from this: the displayed photo is
/// `display` pixels wide and tall (the size the bitmap would have if the whole
/// photo were passed to `draw`), and `rect` is the sub-rectangle the slot's
/// outline can reach, plus a guard band for filtering. Handing `draw` only that
/// rectangle is what keeps the memory ladder bounded by the output: a slot in the
/// library's narrowest pane — a 1/16-wide column of `strip-9-9x1` — needs its
/// photo magnified 12x for a 4:3 source, and passing the whole displayed photo
/// would allocate twelve times the memory to show a twelfth of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayRegion {
    /// Full displayed photo size in output pixels: `(width, height)`.
    pub display: (f64, f64),
    /// The rectangle to hold, in the same grid: `(x, y, width, height)`.
    pub rect: (f64, f64, f64, f64),
}

impl DisplayRegion {
    /// The rectangle as integer texel bounds `(x0, y0, x1, y1)` for a bitmap,
    /// floored/ceiled outward so no fractional part of the region is lost.
    pub fn texels(&self) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = self.rect;
        let x0 = x.floor();
        let y0 = y.floor();
        let x1 = (x + w).ceil();
        let y1 = (y + h).ceil();
        (x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32)
    }
}

impl CropTransform {
    /// The region of the displayed photo that `slot` can show.
    ///
    /// `canvas_px` is the canvas in output pixels and `photo_aspect` the decoded
    /// photo's `width / height` — the same two inputs [`fit`](Self::fit) takes, so
    /// this is the placement arithmetic of `draw` inverted, not a second
    /// definition of where the photo goes: the slot's outline vertices are mapped
    /// into the displayed photo's frame and their bounding box is taken. A polygon
    /// lies inside its bounding box, so holding that box holds everything the
    /// clip can show.
    ///
    /// `guard_px` is added on every side: cairo's filter reads a texel or so
    /// outside the boundary it writes, and the clip edge itself is antialiased,
    /// so a bitmap cut exactly at the boundary would show transparent slivers
    /// inside the slot. The caller passes a constant with its source.
    ///
    /// The document's frame is deliberately **not** subtracted from this box (S11):
    /// a frame can only ever *narrow* what a cell shows, so the outline's own
    /// bounding box is a superset of the visible region — holding it always holds
    /// enough, and the memory the ladder promises is still bounded by the slot's own
    /// bbox (`AGENTS.md`). Tightening it to the inset would save a little bit of
    /// bitmap and cost every caller a second geometry path.
    pub fn display_region(
        &self,
        slot: &Slot,
        canvas_px: PixelSize,
        photo_aspect: f64,
        guard_px: f64,
    ) -> DisplayRegion {
        let bbox = slot.outline.bbox();
        let slot_w = bbox.width() * f64::from(canvas_px.width);
        let slot_h = bbox.height() * f64::from(canvas_px.height);
        let displayed_w = (self.zoom * slot_w).max(1.0);
        let displayed_h = (displayed_w / photo_aspect).max(1.0);
        let centre = Point::new(
            bbox.center().x * f64::from(canvas_px.width) + self.offset.0 * slot_w,
            bbox.center().y * f64::from(canvas_px.height) + self.offset.1 * slot_h,
        );

        // The inverse of `draw`'s placement: a canvas point is expressed in the
        // displayed photo's frame by undoing the rotation and the centring.
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        let to_display = |point: Point| {
            let dx = point.x * f64::from(canvas_px.width) - centre.x;
            let dy = point.y * f64::from(canvas_px.height) - centre.y;
            Point::new(
                dx * cos + dy * sin + displayed_w / 2.0,
                dy * cos - dx * sin + displayed_h / 2.0,
            )
        };
        let corners: Vec<Point> = slot
            .outline
            .points
            .iter()
            .copied()
            .map(to_display)
            .collect();
        let Some(box_) = Rect::from_points(&corners) else {
            return DisplayRegion {
                display: (displayed_w, displayed_h),
                rect: (0.0, 0.0, displayed_w, displayed_h),
            };
        };
        // Clamped into the photo: the fit guarantees the outline is inside the
        // photo rectangle, and the guard band has nothing to extend into past its
        // edge.
        let x0 = (box_.x0 - guard_px).clamp(0.0, displayed_w);
        let y0 = (box_.y0 - guard_px).clamp(0.0, displayed_h);
        let x1 = (box_.x1 + guard_px).clamp(0.0, displayed_w);
        let y1 = (box_.y1 + guard_px).clamp(0.0, displayed_h);
        DisplayRegion {
            display: (displayed_w, displayed_h),
            rect: (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)),
        }
    }
}

/// A slot and the region a photo must cover, as the clamp sees them: in
/// canvas-height units, i.e. a space where the canvas is `canvas_aspect` wide and
/// `1.0` high.
///
/// Normalized coordinates carry no aspect ratio, so a slot occupying `0.5 x 1.0`
/// of a 4:3 canvas is not the shape the same numbers describe on a square one.
/// This is the smallest space in which the framing is aspect-correct, and it is
/// the renderer's space divided by the canvas height: every quantity the clamp
/// produces is a ratio, so the two agree.
///
/// The *scale* comes from the slot's bounding box and the *coverage* from
/// `covering`, which may be smaller (`docs/CONTRACT.md` §1: `zoom` is displayed
/// width over slot width, so a frame that crops the cell must not silently change
/// what a stored zoom means).
struct Frame<'a> {
    covering: &'a Polygon,
    canvas_aspect: f64,
    photo_aspect: f64,
    /// Slot width in canvas-height units.
    width: f64,
    height: f64,
    /// The slot's bounding box centre, which is the photo's centre at a zero
    /// offset and the origin the rotation is measured from.
    centre: Point,
}

impl<'a> Frame<'a> {
    /// `None` when there is no shape to fit against: an outline to cover with
    /// fewer than three vertices, an outline with **no interior** (three collinear
    /// vertices, or a repeated one — an outline whose own area is zero), a
    /// degenerate slot, or an aspect that is not a positive finite number.
    ///
    /// The zero-area case is not the 3-vertex check: `fit`'s documented boundary is
    /// "a degenerate `covering` returns the request untouched", and a valid
    /// three-point *collinear* polygon is a degenerate covering that used to reach
    /// the covering arithmetic and come back magnified (PIX-027B, S15g).
    fn new(
        slot: &Slot,
        covering: &'a Polygon,
        canvas_aspect: f64,
        photo_aspect: f64,
    ) -> Option<Self> {
        if covering.points.len() < Polygon::MIN_VERTICES {
            return None;
        }
        // The same test `Polygon::validate` applies to an outline: an area that is
        // not a positive finite number is a region with no interior to cover, so
        // there is nothing to fit (`NaN` included, which is why finiteness comes
        // first).
        let area = covering.area();
        if !area.is_finite() || area <= 0.0 {
            return None;
        }
        if !canvas_aspect.is_finite() || canvas_aspect <= 0.0 {
            return None;
        }
        if !photo_aspect.is_finite() || photo_aspect <= 0.0 {
            return None;
        }
        let bbox = slot.outline.bbox();
        let (width, height) = (bbox.width() * canvas_aspect, bbox.height());
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        Some(Self {
            covering,
            canvas_aspect,
            photo_aspect,
            width,
            height,
            centre: Point::new(bbox.center().x * canvas_aspect, bbox.center().y),
        })
    }

    /// The photo centre for an offset, in canvas-height units.
    fn centre_for(&self, offset: (f64, f64)) -> Point {
        Point::new(
            self.centre.x + offset.0 * self.width,
            self.centre.y + offset.1 * self.height,
        )
    }

    /// The smallest zoom at which the photo, rotated by `rotation_deg` about
    /// `centre`, covers the whole region.
    ///
    /// A rectangle that contains every vertex contains the convex hull and hence
    /// the polygon, and a polygon inside a rectangle must have all its vertices
    /// inside it — so testing the vertices is exact, for the concave slots this
    /// library ships as much as for rectangles. The photo's half width is
    /// `zoom * slot_width / 2` and its half height is that divided by the photo
    /// aspect, which gives the two bounds below.
    ///
    /// The zoom is what pays for
    /// the angle, and its cost is bounded for every slot shape (`docs/CONTRACT.md`
    /// §8 measures it).
    fn required_zoom(&self, rotation_deg: f64, centre: Point) -> f64 {
        let (sin, cos) = rotation_deg.to_radians().sin_cos();
        let (mut half_width, mut half_height) = (0.0f64, 0.0f64);
        for point in &self.covering.points {
            // The photo is rotated clockwise on screen, so a canvas point is
            // rotated the other way to be expressed in the photo's own frame.
            let dx = point.x * self.canvas_aspect - centre.x;
            let dy = point.y - centre.y;
            half_width = half_width.max((dx * cos + dy * sin).abs());
            half_height = half_height.max((dy * cos - dx * sin).abs());
        }
        (2.0 / self.width) * half_width.max(self.photo_aspect * half_height)
    }

    fn covers(&self, rotation_deg: f64, centre: Point, zoom: f64) -> bool {
        self.required_zoom(rotation_deg, centre) <= zoom
    }

    /// The largest part of `offset` that still covers at `rotation_deg` and
    /// `zoom`.
    ///
    /// Every vertex contributes a strip of feasible photo centres (the vertex
    /// must fall inside the rotated rectangle), so the feasible set is convex and
    /// contains the slot centre — the zoom is at least the upright floor, which
    /// is exactly the zoom that covers with a zero offset. The feasible part of
    /// the segment from the centre to the requested centre is therefore an
    /// interval `[0, t]`, which is what the bisection finds.
    ///
    /// The segment is walked by scaling the offset, not by interpolating the
    /// centre: the result is stored as an offset, so scaling it in the offset's
    /// own coordinates is what makes fitting a fit return the same bits.
    fn clamp_offset(&self, offset: (f64, f64), rotation_deg: f64, zoom: f64) -> (f64, f64) {
        let scaled = |t: f64| (offset.0 * t, offset.1 * t);
        let fits = |t: f64| self.covers(rotation_deg, self.centre_for(scaled(t)), zoom);
        if offset == (0.0, 0.0) || fits(1.0) {
            return offset;
        }
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..BISECTION_STEPS {
            let mid = 0.5 * (low + high);
            if fits(mid) {
                low = mid;
            } else {
                high = mid;
            }
        }
        scaled(low)
    }
}
