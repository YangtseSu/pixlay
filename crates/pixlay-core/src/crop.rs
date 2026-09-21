//! Per-slot framing state.

use serde::{Deserialize, Serialize};

use crate::canvas::PixelSize;
use crate::error::CoreError;
use crate::geometry::{Point, Polygon, Rect};
use crate::template::Slot;
use crate::{CLAMP_ZOOM_LIMIT, MAX_ROTATION_DEG, MAX_ZOOM};

/// Halvings used by the two one-dimensional searches of [`CropTransform::fit`].
///
/// 60 halvings take a bracket below `2^-60` of its width, past `f64`'s
/// resolution at these magnitudes; a fixed count keeps the clamp's cost a
/// number rather than a convergence criterion, so the same document always fits
/// to the same bits.
const BISECTION_STEPS: u32 = 60;

/// Angles scanned between "upright" and the requested angle before the bracket
/// is bisected.
///
/// The required zoom is **not** monotone in the angle: a slot much narrower than
/// it is tall (the ten-column strip needs 7.5x upright with a 4:3 photo) needs a
/// *smaller* photo at 45 degrees than at 0, so "a wider angle always needs more
/// zoom" is false and a plain bisection would give up early. The scan finds the
/// widest angle that fits; the bisection then refines inside that bracket.
const ANGLE_SCAN_STEPS: u32 = 256;

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
    /// Rotation only crops edges — the canvas and the slot never grow, so
    /// covering the slot at an angle costs magnification. [`CropTransform::fit`]
    /// honours this angle while that magnification stays within
    /// [`CLAMP_ZOOM_LIMIT`] times the upright floor, and otherwise keeps the
    /// widest angle that does ([`CropFit::rotation_limited`]).
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
/// The stored transform is a request; what gets drawn is the fit ([`fit`]).
/// Below the zoom limit a sliver-shaped slot is covered by magnifying the photo,
/// and past it the requested rotation angle is reduced instead of magnifying
/// further (docs/STEPS.md, "Open decisions → B. Confirmed": elongated-slot clamp degradation). The GUI needs to know which of the two
/// happened so it can say so; the renderer only needs `transform`.
///
/// [`fit`]: CropTransform::fit
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropFit {
    /// The transform that actually gets drawn.
    pub transform: CropTransform,
    /// True when *this* call had to reduce the requested rotation angle.
    ///
    /// It describes the call, not the transform: fitting an already-fitted
    /// transform returns the same transform and reports `false`, so a caller
    /// that clamps on every edit does not report a limitation it did not apply.
    pub rotation_limited: bool,
}

impl CropTransform {
    /// The framing that actually gets drawn: this request fitted to a slot.
    ///
    /// A request is what the user asked for, a fit is what covers the slot. The
    /// canvas and the slot never grow, so the only levers are the three fields,
    /// and each of them has exactly one:
    ///
    /// * `zoom` is raised to the value that covers the slot with the photo
    ///   centred, and a larger request is kept as it is;
    /// * `offset` is pulled back along the line to the slot centre until the
    ///   photo covers again — a pan must stop at the frame edge rather than be
    ///   paid for with more magnification, or dragging a photo would zoom it;
    /// * `rotation_deg` is honoured while the zoom it needs stays within
    ///   [`CLAMP_ZOOM_LIMIT`] times that upright floor, and is otherwise reduced
    ///   to the widest angle that does (`CropFit::rotation_limited`).
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
    /// hidden: a request that is not four finite numbers, a degenerate slot or a
    /// non-finite aspect (no framing to fit against) returns the request
    /// untouched, and a slot so extreme that covering it needs more than
    /// [`MAX_ZOOM`] gets the cap instead of coverage.
    pub fn fit(&self, slot: &Slot, canvas_aspect: f64, photo_aspect: f64) -> CropFit {
        let untouched = CropFit {
            transform: *self,
            rotation_limited: false,
        };
        if !self.is_finite() {
            return untouched;
        }
        let Some(frame) = Frame::new(slot, canvas_aspect, photo_aspect) else {
            return untouched;
        };
        // The zoom the slot's shape and the photo's aspect demand with the photo
        // centred and upright. It is the "1x" the degradation limit is relative
        // to, and it never depends on the offset or the angle, so a slot that is
        // inherently narrow is not degraded for that reason alone.
        let floor = frame.required_zoom(0.0, frame.centre);
        let affordable =
            |angle| frame.required_zoom(angle, frame.centre) <= CLAMP_ZOOM_LIMIT * floor;
        let mut rotation_limited = false;
        let rotation_deg = if affordable(self.rotation_deg) {
            self.rotation_deg
        } else {
            rotation_limited = true;
            frame.widest_rotation(self.rotation_deg, CLAMP_ZOOM_LIMIT * floor)
        };
        let zoom = frame
            .required_zoom(rotation_deg, frame.centre)
            .max(self.zoom)
            .min(MAX_ZOOM);
        CropFit {
            transform: Self {
                zoom,
                offset: frame.clamp_offset(self.offset, rotation_deg, zoom),
                rotation_deg,
            },
            rotation_limited,
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
/// rectangle is what keeps the memory ladder bounded by the output: a slot in a
/// ten-column strip needs a photo 6x its own width, and passing the whole
/// displayed photo would allocate six times the memory to display one tenth of
/// it.
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

/// A slot and a photo as the clamp sees them: in canvas-height units, i.e. a
/// space where the canvas is `canvas_aspect` wide and `1.0` high.
///
/// Normalized coordinates carry no aspect ratio, so a slot occupying `0.5 x 1.0`
/// of a 4:3 canvas is not the shape the same numbers describe on a square one.
/// This is the smallest space in which the framing is aspect-correct, and it is
/// the renderer's space divided by the canvas height: every quantity the clamp
/// produces is a ratio, so the two agree.
struct Frame<'a> {
    outline: &'a Polygon,
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
    /// `None` when there is no shape to fit against: a degenerate outline, or an
    /// aspect that is not a positive finite number.
    fn new(slot: &'a Slot, canvas_aspect: f64, photo_aspect: f64) -> Option<Self> {
        if slot.outline.points.len() < Polygon::MIN_VERTICES {
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
            outline: &slot.outline,
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
    /// `centre`, covers the whole outline.
    ///
    /// A rectangle that contains every vertex contains the convex hull and hence
    /// the polygon, and a polygon inside a rectangle must have all its vertices
    /// inside it — so testing the vertices is exact, for the concave slots this
    /// library ships as much as for rectangles. The photo's half width is
    /// `zoom * slot_width / 2` and its half height is that divided by the photo
    /// aspect, which gives the two bounds below.
    fn required_zoom(&self, rotation_deg: f64, centre: Point) -> f64 {
        let (sin, cos) = rotation_deg.to_radians().sin_cos();
        let (mut half_width, mut half_height) = (0.0f64, 0.0f64);
        for point in &self.outline.points {
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

    /// The widest angle between upright and `rotation_deg` (same sign) whose
    /// required zoom stays within `limit`.
    ///
    /// Upright, and any angle on the way to it, is within the limit: the limit is
    /// at least the upright floor by construction, and the caller only asks when
    /// the requested angle is not.
    fn widest_rotation(&self, rotation_deg: f64, limit: f64) -> f64 {
        let (sign, magnitude) = (rotation_deg.signum(), rotation_deg.abs());
        let sample = |step: u32| sign * magnitude * f64::from(step) / f64::from(ANGLE_SCAN_STEPS);
        let mut step = ANGLE_SCAN_STEPS;
        while step > 0 && !self.covers(sample(step), self.centre, limit) {
            step -= 1;
        }
        // The scan stopped on the first angle that fits; the sample one step out
        // is the angle it rejected, and the crossing lies between them.
        let mut low = sample(step);
        let mut high = sample(step + 1);
        for _ in 0..BISECTION_STEPS {
            let mid = 0.5 * (low + high);
            if self.covers(mid, self.centre, limit) {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
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
