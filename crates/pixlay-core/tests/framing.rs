//! S3 and S11: the framing clamp, as tests.
//!
//! The clamp promises one thing — after it, the photo covers the whole visible
//! cell — and the sweeps below are that promise over the whole matrix this build
//! ships: every template, every slot shape, and combinations of rotation, zoom,
//! offset, photo aspect and frame.
//!
//! Two of them are the step's own criterion:
//!
//! * [`every_framing_covers_its_cell`] is S3's sweep with **no rotation cap**: the
//!   angles now run the whole circle, and the clamp may not touch the angle at all;
//! * [`the_frame_narrows_the_reference_and_the_photo_still_covers`] runs the same
//!   check against the region a frame leaves visible, which is the reference S11
//!   introduced.
//!
//! Coverage is measured the way the renderer places the photo (a canvas point is
//! inside the photo when its coordinates in the photo's own frame are within its
//! half extents), not by reusing the clamp's own vertex test, so a wrong centre,
//! a wrong rotation direction or a wrong photo aspect cannot pass.

use pixlay_core::templates;
use pixlay_core::{CropTransform, Frame, MAX_ZOOM, Point, Polygon, Slot};

/// Tolerance for "the photo covers the cell": 1e-6 normalized.
///
/// 1e-6 of a sheet edge is 0.014 px on the 14043-px reference grid, far under
/// half a pixel, while the clamp's own arithmetic is exact to ~1e-15 — so
/// nothing that is actually uncovered can pass this.
const COVERAGE_EPSILON: f64 = 1e-6;

/// The photo as the renderer places it, in canvas-height units: the same
/// arithmetic as `pixlay_render::draw_slot`, written out independently of the
/// clamp that has to satisfy it.
struct Placed {
    centre: Point,
    half_width: f64,
    half_height: f64,
    rotation_deg: f64,
}

impl Placed {
    fn new(crop: &CropTransform, slot: &Slot, canvas_aspect: f64, photo_aspect: f64) -> Self {
        let bbox = slot.outline.bbox();
        let width = bbox.width() * canvas_aspect;
        let height = bbox.height();
        let displayed = crop.zoom * width;
        Self {
            centre: Point::new(
                bbox.center().x * canvas_aspect + crop.offset.0 * width,
                bbox.center().y + crop.offset.1 * height,
            ),
            half_width: displayed / 2.0,
            half_height: displayed / photo_aspect / 2.0,
            rotation_deg: crop.rotation_deg,
        }
    }

    /// How far outside the photo a canvas point falls, in half extents: `<= 1` is
    /// inside, `> 1` is uncovered. The boundary is exactly `1`, which is what
    /// makes the tightness checks below meaningful.
    fn outside(&self, point: Point, canvas_aspect: f64) -> f64 {
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        let dx = point.x * canvas_aspect - self.centre.x;
        let dy = point.y - self.centre.y;
        let u = (dx * cos + dy * sin) / self.half_width;
        let v = (dy * cos - dx * sin) / self.half_height;
        u.abs().max(v.abs())
    }
}

/// The worst `outside` over `points`: covered means
/// `worst <= 1.0 + COVERAGE_EPSILON`.
fn worst(
    crop: &CropTransform,
    slot: &Slot,
    canvas_aspect: f64,
    photo_aspect: f64,
    points: &[Point],
) -> f64 {
    let placed = Placed::new(crop, slot, canvas_aspect, photo_aspect);
    points
        .iter()
        .map(|point| placed.outside(*point, canvas_aspect))
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Sample points of a region: its vertices, its edge midpoints and a grid over
/// its bounding box, keeping the points inside.
///
/// The vertices are the sharpest samples there are — a rectangle that covers the
/// polygon must contain them, and at the floor it contains them exactly — while
/// the grid inside catches a wrong centre or a wrong rotation direction.
fn samples(region: &Polygon, grid: u32) -> Vec<Point> {
    let bbox = region.bbox();
    let mut points: Vec<Point> = region.points.clone();
    points.extend(
        region
            .edges()
            .map(|(a, b)| Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)),
    );
    for iy in 0..grid {
        for ix in 0..grid {
            let point = Point::new(
                bbox.x0 + bbox.width() * (f64::from(ix) + 0.5) / f64::from(grid),
                bbox.y0 + bbox.height() * (f64::from(iy) + 0.5) / f64::from(grid),
            );
            if region.contains(point) {
                points.push(point);
            }
        }
    }
    points
}

fn rect_slot(x0: f64, y0: f64, x1: f64, y1: f64) -> Slot {
    let outline = Polygon::rect(x0, y0, x1, y1);
    Slot {
        area: outline.area(),
        outline,
    }
}

/// The region a photo must cover: what the document's frame leaves visible, which
/// is the outline itself for an identity frame.
///
/// The clamp takes its reference from `Frame::covering`, so the tests do too —
/// asking the same function rather than re-deriving the inset means a frame that
/// stopped insetting would fail the coverage check instead of passing both.
fn covering(frame: &Frame, slot: &Slot, canvas_aspect: f64) -> Polygon {
    frame
        .covering(slot, canvas_aspect)
        .unwrap_or_else(|| panic!("the frame leaves no visible cell: {frame:?}"))
}

/// A request fitted to `slot` under `frame`.
fn fit(
    request: CropTransform,
    slot: &Slot,
    frame: &Frame,
    canvas_aspect: f64,
    photo_aspect: f64,
) -> CropTransform {
    request
        .fit(
            slot,
            &covering(frame, slot, canvas_aspect),
            canvas_aspect,
            photo_aspect,
        )
        .transform
}

/// Bit equality of two transforms. `==` calls a NaN request unequal to itself,
/// and "returned untouched" has to hold for those too.
fn same_bits(a: &CropTransform, b: &CropTransform) -> bool {
    a.zoom.to_bits() == b.zoom.to_bits()
        && a.offset.0.to_bits() == b.offset.0.to_bits()
        && a.offset.1.to_bits() == b.offset.1.to_bits()
        && a.rotation_deg.to_bits() == b.rotation_deg.to_bits()
}

#[test]
fn every_framing_covers_its_cell() {
    // The S3 sweep with the cap gone: the angles are the whole circle, including
    // both halves of the seam at ±180 and both diagonals.
    let rotations = [
        -179.5, -135.0, -90.0, -45.0, -18.0, -7.5, 0.0, 11.0, 45.0, 90.0, 179.5,
    ];
    let offsets = [
        (-1.0, 1.0),
        (-0.4, 0.25),
        (0.0, 0.0),
        (0.65, -0.8),
        (1.0, -1.0),
    ];
    let photo_aspects = [0.5, 0.8, 1.0, 1.5, 2.4];
    let zooms = [0.35, 1.0, 3.0];
    let frame = Frame::default();
    let (mut checked, mut panned) = (0u64, 0u64);

    for template in templates::all() {
        for (index, slot) in template.slots.iter().enumerate() {
            let points = samples(&slot.outline, 24);
            for &rotation_deg in &rotations {
                for &offset in &offsets {
                    for &photo_aspect in &photo_aspects {
                        for &zoom in &zooms {
                            let request = CropTransform {
                                zoom,
                                offset,
                                rotation_deg,
                            };
                            let transform =
                                fit(request, slot, &frame, template.aspect, photo_aspect);
                            let what = format!(
                                "{} slot {index} for {request:?} at photo aspect {photo_aspect}",
                                template.name
                            );
                            assert!(transform.validate().is_ok(), "{what}: {transform:?}");
                            assert!(transform.zoom <= MAX_ZOOM, "{what}");
                            // The angle is free and the clamp never reduces it: the
                            // fit hands back exactly what was asked for, bit for
                            // bit, at every angle up to and including the seam.
                            assert!(
                                transform.rotation_deg.to_bits() == rotation_deg.to_bits(),
                                "{what}: the angle must be kept exactly, got {}",
                                transform.rotation_deg
                            );
                            let outside =
                                worst(&transform, slot, template.aspect, photo_aspect, &points);
                            assert!(
                                outside.is_finite() && outside <= 1.0 + COVERAGE_EPSILON,
                                "{what}: measured {outside} half extents (1.0 = the photo edge)"
                            );
                            checked += 1;
                            panned += u64::from(transform.offset != offset);
                        }
                    }
                }
            }
        }
    }

    // The sweep is only evidence if it actually reaches the branches: 27
    // templates and 143 slots of the shipped library.
    eprintln!("unframed sweep: {checked} framings, {panned} pan-clamped");
    assert!(
        checked > 100_000,
        "the sweep must actually sweep: {checked}"
    );
    assert!(panned > 0, "no framing in the sweep hit the pan clamp");
}

#[test]
fn the_frame_narrows_the_reference_and_the_photo_still_covers() {
    // The framed sweep: the photo must cover the region the frame leaves visible,
    // which is smaller than the outline — and the clamp must reach it with the
    // angle untouched, as it does unframed.
    let frames = [
        // A gap alone: for a rectangular slot the reference is exactly the inset
        // rectangle, so this is the case that would show a reference that had grown.
        Frame {
            gap_rel: 0.02,
            ..Frame::default()
        },
        // A radius alone: geometrically the identity reference (the frame module
        // explains why the corners are not subtracted), so this is the case that
        // would show a reference that had shrunk.
        Frame {
            radius_rel: 0.05,
            ..Frame::default()
        },
        Frame {
            gap_rel: 0.04,
            radius_rel: 0.1,
            ..Frame::default()
        },
        // The largest gap the narrowest slot in the library survives: the strips
        // have 1/16-wide panes, and this leaves them 1.75% of the canvas.
        Frame {
            gap_rel: 0.08,
            radius_rel: 0.03,
            ..Frame::default()
        },
    ];
    let rotations = [-135.0, -30.0, 0.0, 30.0, 135.0];
    let offsets = [(-0.5, 0.4), (0.0, 0.0), (0.7, -0.6)];
    let photo_aspects = [0.6, 1.0, 1.6, 2.4];
    let mut checked = 0u64;

    for frame in &frames {
        for template in templates::all() {
            for (index, slot) in template.slots.iter().enumerate() {
                let visible = covering(frame, slot, template.aspect);
                assert!(visible.area() > 0.0);
                // The visible region is inside the outline, always: the frame can
                // only ever cut a cell down.
                for point in &visible.points {
                    assert!(
                        slot.outline.contains(*point)
                            || slot.outline.distance_to_boundary(*point) <= 1e-12,
                        "{} slot {index}: the frame grew the cell",
                        template.name
                    );
                }
                let points = samples(&visible, 12);
                for &rotation_deg in &rotations {
                    for &offset in &offsets {
                        for &photo_aspect in &photo_aspects {
                            let request = CropTransform {
                                zoom: 1.0,
                                offset,
                                rotation_deg,
                            };
                            let transform =
                                fit(request, slot, frame, template.aspect, photo_aspect);
                            let what = format!(
                                "{} slot {index} {frame:?} for {request:?} at photo aspect {photo_aspect}",
                                template.name
                            );
                            assert!(
                                transform.rotation_deg.to_bits() == rotation_deg.to_bits(),
                                "{what}: the angle moved"
                            );
                            let outside =
                                worst(&transform, slot, template.aspect, photo_aspect, &points);
                            assert!(
                                outside.is_finite() && outside <= 1.0 + COVERAGE_EPSILON,
                                "{what}: measured {outside} half extents"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("framed sweep: {checked} framings");
    assert!(checked > 30_000, "the framed sweep must sweep: {checked}");
}

#[test]
fn the_covering_zoom_is_bounded_for_every_slot_shape() {
    // S11's own number: with the angle free, the question is whether covering a
    // cell at *any* angle stays inside `MAX_ZOOM`. It does, with more than an order
    // of magnitude to spare, and this is where that is measured rather than assumed.
    //
    // The worst case is a smooth function of the angle (the required zoom is
    // `(2/W) * max(|u·p|, aspect·|v·p|)` over the cell's vertices), so a one-degree
    // step cannot miss a peak by more than the peak's own curvature — and the
    // extreme it finds is printed, so a change shows up as a number.
    let photo_aspects = [0.5, 0.8, 1.0, 1.3333333333333333, 1.5, 2.4];
    let (mut overall, mut overall_at) = (0.0f64, String::new());
    for photo_aspect in photo_aspects {
        let (mut worst, mut worst_at) = (0.0f64, String::new());
        for template in templates::all() {
            for (index, slot) in template.slots.iter().enumerate() {
                let covering = covering(&Frame::default(), slot, template.aspect);
                for step in 0..=180u32 {
                    let angle = f64::from(step);
                    let request = CropTransform {
                        zoom: 1.0,
                        offset: (0.0, 0.0),
                        rotation_deg: angle,
                    };
                    let zoom = request
                        .fit(slot, &covering, template.aspect, photo_aspect)
                        .transform
                        .zoom;
                    if zoom > worst {
                        worst = zoom;
                        worst_at = format!("{} slot {index} at {angle} degrees", template.name);
                    }
                }
            }
        }
        eprintln!("photo aspect {photo_aspect}: worst covering zoom {worst} ({worst_at})");
        if worst > overall {
            overall = worst;
            overall_at = format!("photo aspect {photo_aspect}, {worst_at}");
        }
    }
    eprintln!("worst over every slot, angle and photo aspect: {overall} ({overall_at})");
    // Measured 2026-09-22 (S11), worst covering zoom per photo aspect, over all 152
    // (142 since S12c dropped the ten-slot recipe; the worst pane is unchanged)
    // shipped slots and every whole degree: 0.5 → 9.06, 0.8 → 9.06, 1 → 9.06,
    // 4:3 → 12.07, 1.5 → 13.58, 2.4 → **21.73**. Every one of them is
    // `strip-9-9x1`'s 1/16-wide pane, whose *upright* floor is already 21.6 with a
    // 2.4:1 photo — a panorama in the narrowest cell this library ships. `MAX_ZOOM`
    // is 1000, so the cap is not a live limit for any angle this product can be
    // asked for.
    assert!(
        overall <= 25.0,
        "the free angle costs {overall} ({overall_at}), past the bound this build measured"
    );
    assert!(
        overall > 20.0,
        "the sweep found {overall}, so it is not reaching the narrow panes"
    );
}

#[test]
fn a_rotation_of_any_angle_is_kept_exactly() {
    // The cap is gone, so the one thing the fit must never do is touch the angle —
    // including at the seam (±180), on the diagonals, and for a request far past a
    // full turn (which only a document mutated in memory can hold; `normalized`
    // wraps what a file says).
    let canvas_aspect = 4.0 / 3.0;
    let photo_aspect = 4.0 / 3.0;
    let slot = rect_slot(0.0, 0.0, 1.0, 1.0);
    let frame = Frame::default();
    let points = samples(&slot.outline, 24);

    for angle in [
        0.0, -0.5, 11.0, -45.0, 45.0, 90.0, -90.0, 179.9, 180.0, -179.9, -180.0, 450.0, -720.5,
    ] {
        let request = CropTransform {
            zoom: 1.0,
            offset: (0.0, 0.0),
            rotation_deg: angle,
        };
        let transform = fit(request, &slot, &frame, canvas_aspect, photo_aspect);
        assert_eq!(
            transform.rotation_deg.to_bits(),
            angle.to_bits(),
            "{angle} degrees was not kept"
        );
        let outside = worst(&transform, &slot, canvas_aspect, photo_aspect, &points);
        assert!(
            outside <= 1.0 + COVERAGE_EPSILON,
            "{angle} degrees: measured {outside}"
        );
    }
}

#[test]
fn a_wide_slot_keeps_the_angle_and_pays_with_zoom() {
    // What the free angle costs, exactly. A 4:3 slot with a matching photo covers
    // upright at 1.0x, and at angle `t` it needs `r*sin t + cos t` with `r` the
    // slot's physical aspect (4/3) — 1.396 at 20 degrees, 1.886 at 45, 1.333 at 90.
    // This is the test the retired
    // `a_wide_slot_limits_the_rotation_instead_of_magnifying_without_bound` became:
    // the angle is what survives now, not the zoom.
    let canvas_aspect = 4.0 / 3.0;
    let photo_aspect = 4.0 / 3.0;
    let slot = rect_slot(0.0, 0.0, 1.0, 1.0);
    let frame = Frame::default();
    let points = samples(&slot.outline, 24);
    // The slot's physical aspect: a unit square in normalized coordinates is the
    // canvas's own aspect in canvas-height units.
    let r = canvas_aspect;

    let upright = fit(
        CropTransform {
            zoom: 1.0,
            offset: (0.0, 0.0),
            rotation_deg: 0.0,
        },
        &slot,
        &frame,
        canvas_aspect,
        photo_aspect,
    );
    assert!((upright.zoom - 1.0).abs() <= 1e-12, "{}", upright.zoom);

    for angle in [15.0, 20.0, 30.0, 45.0, 60.0, 90.0, 135.0] {
        let transform = fit(
            CropTransform {
                zoom: 1.0,
                offset: (0.0, 0.0),
                rotation_deg: angle,
            },
            &slot,
            &frame,
            canvas_aspect,
            photo_aspect,
        );
        assert_eq!(transform.rotation_deg, angle, "the angle is kept");
        let (sin, cos) = angle.to_radians().sin_cos();
        let expected = r * sin.abs() + cos.abs();
        assert!(
            (transform.zoom - expected).abs() <= 1e-9,
            "{angle} degrees: zoom {} instead of {expected}",
            transform.zoom
        );
        let outside = worst(&transform, &slot, canvas_aspect, photo_aspect, &points);
        assert!(
            (outside - 1.0).abs() <= COVERAGE_EPSILON,
            "{angle} degrees: measured {outside}, which is not tight"
        );
    }
}

#[test]
fn the_floor_is_the_smallest_zoom_that_covers() {
    // With the photo centred and upright the required zoom is exactly
    // `max(1, photo_aspect * slot_height / slot_width)`, and the fit lands on it:
    // a clamp that magnified more than necessary would silently crop every photo
    // it touched, one that magnified less would leave a sliver of backdrop.
    for template in templates::all() {
        for (index, slot) in template.slots.iter().enumerate() {
            let frame = Frame::default();
            let points = samples(&slot.outline, 24);
            let bbox = slot.outline.bbox();
            let width = bbox.width() * template.aspect;
            let height = bbox.height();
            for photo_aspect in [0.6, 1.0, 1.5, 2.8] {
                let expected = (photo_aspect * height / width).max(1.0);
                let request = CropTransform {
                    zoom: 0.2,
                    offset: (0.0, 0.0),
                    rotation_deg: 0.0,
                };
                let transform = fit(request, slot, &frame, template.aspect, photo_aspect);
                let what = format!(
                    "{} slot {index} at photo aspect {photo_aspect}",
                    template.name
                );
                assert!(
                    (transform.zoom - expected).abs() <= 1e-12,
                    "{what}: floor {} instead of {expected}",
                    transform.zoom
                );
                assert_eq!(transform.offset, (0.0, 0.0), "{what}");
                assert_eq!(transform.rotation_deg, 0.0, "{what}");
                let outside = worst(&transform, slot, template.aspect, photo_aspect, &points);
                assert!(
                    (outside - 1.0).abs() <= COVERAGE_EPSILON,
                    "{what}: the floor is not tight, measured {outside}"
                );
            }

            // With a gap, the floor is the *visible* rectangle's, and it is still
            // tight: the gap crops the photo at the frame instead of magnifying it,
            // which is what makes "add a gap" and "zoom the photo" different
            // operations.
            //
            // The extents are measured from the slot's own centre, because that is
            // where the photo is centred at a zero offset — and since S20 that is
            // not the same as half the visible rectangle: at the sheet's edge the
            // frame's own band takes a whole gap off the outer side and half of one
            // off the inner sides, so the rectangle sits off-centre in its cell and
            // the covering zoom has that much further to reach. The bbox of the
            // visible polygon is realized by its own vertices, so measuring against
            // the bbox is exact rather than an over-estimate, for the concave slot
            // too.
            let gap = 0.02;
            let frame = Frame {
                gap_rel: gap,
                ..Frame::default()
            };
            let visible = covering(&frame, slot, template.aspect);
            let vbox = visible.bbox();
            let visible_points = samples(&visible, 12);
            let centre = slot.outline.bbox().center();
            let (cx, cy) = (centre.x * template.aspect, centre.y);
            let half_width = (vbox.x1 * template.aspect - cx).max(cx - vbox.x0 * template.aspect);
            let half_height = (vbox.y1 - cy).max(cy - vbox.y0);
            for photo_aspect in [0.6, 1.0, 1.5, 2.8] {
                let expected = (2.0 / width) * half_width.max(photo_aspect * half_height);
                let transform = fit(
                    CropTransform {
                        zoom: 0.2,
                        offset: (0.0, 0.0),
                        rotation_deg: 0.0,
                    },
                    slot,
                    &frame,
                    template.aspect,
                    photo_aspect,
                );
                let what = format!(
                    "{} slot {index} with a gap, at photo aspect {photo_aspect}",
                    template.name
                );
                assert!(
                    (transform.zoom - expected).abs() <= 1e-12,
                    "{what}: floor {} instead of {expected}",
                    transform.zoom
                );
                let outside = worst(
                    &transform,
                    slot,
                    template.aspect,
                    photo_aspect,
                    &visible_points,
                );
                assert!(
                    (outside - 1.0).abs() <= COVERAGE_EPSILON,
                    "{what}: the floor is not tight, measured {outside}"
                );
            }
        }
    }
}

#[test]
fn a_rotation_change_recomputes_the_clamp() {
    // The fit is a function of the framing it is asked about, never a value kept
    // from an earlier angle or slot: the same photo in the same slot fits
    // differently the moment the angle changes. A 4:3 photo in a 4:3 slot needs
    // 1.0x upright and 1.4x at 20 degrees, so a switch to 20 degrees has to move
    // the drawn zoom even though the request's own zoom did not move.
    let canvas_aspect = 4.0 / 3.0;
    let photo_aspect = 4.0 / 3.0;
    let slot = rect_slot(0.0, 0.0, 1.0, 1.0);
    let frame = Frame::default();
    let points = samples(&slot.outline, 24);
    let at = |rotation_deg| CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg,
    };

    let upright = fit(at(0.0), &slot, &frame, canvas_aspect, photo_aspect);
    assert!((upright.zoom - 1.0).abs() <= 1e-12);

    for angle in [12.0, 20.0, -20.0] {
        let rotated = fit(at(angle), &slot, &frame, canvas_aspect, photo_aspect);
        assert!(
            rotated.zoom > upright.zoom,
            "{angle} degrees: zoom {} is not above the upright {}",
            rotated.zoom,
            upright.zoom
        );
        assert_eq!(rotated.rotation_deg, angle);
        for (crop, what) in [(&upright, "upright"), (&rotated, "rotated")] {
            let outside = worst(crop, &slot, canvas_aspect, photo_aspect, &points);
            assert!(
                outside <= 1.0 + COVERAGE_EPSILON,
                "{what} at {angle} degrees: measured {outside}"
            );
        }
    }

    // A request above the floor is not pulled back to it: the user's own zoom is
    // the one they asked for, and the angle has to be affordable at it.
    let zoomed = fit(
        CropTransform {
            zoom: 30.0,
            offset: (0.0, 0.0),
            rotation_deg: 20.0,
        },
        &slot,
        &frame,
        canvas_aspect,
        photo_aspect,
    );
    assert_eq!(zoomed.zoom, 30.0);
    assert_eq!(zoomed.rotation_deg, 20.0);
}

#[test]
fn the_fit_follows_the_slot_shape() {
    // Same request, same photo, two slot shapes: the fit is a property of the
    // slot as much as of the request. A square photo fills a square slot at 1x
    // and a slot four times taller than it is wide at 2.5x.
    let square = rect_slot(0.0, 0.0, 1.0, 1.0);
    let tall = rect_slot(0.3, 0.0, 0.7, 1.0);
    let frame = Frame::default();
    let request = CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg: 0.0,
    };
    let square_fit = fit(request, &square, &frame, 1.0, 1.0);
    let tall_fit = fit(request, &tall, &frame, 1.0, 1.0);
    assert!((square_fit.zoom - 1.0).abs() <= 1e-12);
    assert!((tall_fit.zoom - 2.5).abs() <= 1e-12);
    for (slot, crop) in [(&square, &square_fit), (&tall, &tall_fit)] {
        let points = samples(&slot.outline, 24);
        let outside = worst(crop, slot, 1.0, 1.0, &points);
        assert!(outside <= 1.0 + COVERAGE_EPSILON, "measured {outside}");
    }
}

#[test]
fn an_elongated_slot_keeps_its_rotation() {
    // An elongated slot is where the retired degradation limit mattered: the
    // nine-column strip's first pane is 0.125 x 1.0 of a 16:9 canvas, so a 4:3
    // photo covers it upright at `4/3 * 16/9 / (16/9 * 0.125) = 6x` — and at 45
    // degrees it needs *less* (5.19x), because a narrow slot fits a rotated photo
    // better than an upright one. With the cap gone there is nothing to reduce in
    // any case, and this pins both halves: the angle is kept and the zoom is what
    // the shape asks. (`strip-10-10x1` used to be the fixture here; S12c removed
    // that recipe and this pane has the same shape.)
    let template = templates::get("strip-9-9x1").expect("the strip template");
    let photo_aspect = 4.0 / 3.0;
    let slot = &template.slots[0];
    let frame = Frame::default();
    let points = samples(&slot.outline, 24);
    let request = CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg: 45.0,
    };
    let rotated = fit(request, slot, &frame, template.aspect, photo_aspect);
    assert_eq!(rotated.rotation_deg, 45.0);

    let upright = fit(
        CropTransform {
            rotation_deg: 0.0,
            ..request
        },
        slot,
        &frame,
        template.aspect,
        photo_aspect,
    );
    assert!(
        (upright.zoom - 6.0).abs() <= 1e-12,
        "the upright floor for this slot is 6x, got {}",
        upright.zoom
    );
    assert!(
        rotated.zoom < upright.zoom,
        "rotating this slot needs less zoom, not more: {} against {}",
        rotated.zoom,
        upright.zoom
    );
    for crop in [&rotated, &upright] {
        let outside = worst(crop, slot, template.aspect, photo_aspect, &points);
        assert!(outside <= 1.0 + COVERAGE_EPSILON, "measured {outside}");
    }
}

#[test]
fn fitting_a_fit_returns_it() {
    // Idempotence is what lets the clamp run on every edit *and* at the render
    // boundary: the second pass changes nothing. `edit`'s own idempotence (the CLI
    // writing the same bytes twice) rests on this exact property.
    let frame = Frame::default();
    let framed = Frame {
        gap_rel: 0.03,
        radius_rel: 0.06,
        ..Frame::default()
    };
    let cases = [
        (
            rect_slot(0.0, 0.0, 1.0, 1.0),
            4.0 / 3.0,
            4.0 / 3.0,
            CropTransform {
                zoom: 1.0,
                offset: (0.0, 0.0),
                rotation_deg: 45.0,
            },
            &frame,
        ),
        (
            rect_slot(0.1, 0.1, 0.4, 0.9),
            4.0 / 3.0,
            1.5,
            CropTransform {
                zoom: 0.5,
                offset: (-1.0, 1.0),
                rotation_deg: -30.0,
            },
            &frame,
        ),
        (
            rect_slot(0.0, 0.4, 1.0, 0.6),
            16.0 / 9.0,
            1.0,
            CropTransform {
                zoom: 4.0,
                offset: (0.7, -0.2),
                rotation_deg: 12.0,
            },
            &frame,
        ),
        // The same, under a frame: the reference changes, the property does not.
        (
            rect_slot(0.0, 0.1, 1.0, 0.9),
            3.0 / 2.0,
            1.2,
            CropTransform {
                zoom: 1.1,
                offset: (0.6, -0.3),
                rotation_deg: 123.0,
            },
            &framed,
        ),
    ];
    for (slot, canvas_aspect, photo_aspect, request, frame) in cases {
        let once = fit(request, &slot, frame, canvas_aspect, photo_aspect);
        let twice = fit(once, &slot, frame, canvas_aspect, photo_aspect);
        assert!(
            same_bits(&twice, &once),
            "for {request:?}: {twice:?} != {once:?}"
        );
        assert_eq!(twice, once, "for {request:?}");
    }
}

#[test]
fn a_shape_that_cannot_be_fitted_is_returned_untouched() {
    // A degenerate region and an aspect that is not a positive finite number have
    // no coverage promise to keep, so the clamp hands the request back instead of
    // inventing a framing for a shape that is not there.
    let request = CropTransform {
        zoom: 2.0,
        offset: (0.5, -0.5),
        rotation_deg: 30.0,
    };
    let degenerate = Slot {
        area: 1.0,
        outline: Polygon {
            points: vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)],
        },
    };
    let slot = rect_slot(0.0, 0.0, 1.0, 1.0);
    let covering = Polygon::rect(0.0, 0.0, 1.0, 1.0);
    let empty = Polygon { points: vec![] };
    // A region with three vertices and no interior: a vertical segment down the
    // middle of the slot as three collinear points, and the same segment as two
    // points with one repeated. `fit`'s boundary is that a degenerate *covering*
    // returns the request untouched (PIX-027B, S15g); before S15g only a region with
    // fewer than three vertices took it, so a three-point collinear region reached
    // the covering arithmetic — and at this request came back panned.
    let mut segment = vec![Point::new(0.75, 0.0), Point::new(0.75, 0.5)];
    let collinear = Polygon {
        points: {
            let mut points = segment.clone();
            points.push(Point::new(0.75, 1.0));
            points
        },
    };
    let repeated = Polygon {
        points: {
            segment.push(Point::new(0.75, 0.5));
            segment
        },
    };
    for (what, slot, region, canvas_aspect, photo_aspect) in [
        ("degenerate outline", &degenerate, &covering, 4.0 / 3.0, 1.5),
        ("empty region", &slot, &empty, 4.0 / 3.0, 1.5),
        ("collinear region", &slot, &collinear, 4.0 / 3.0, 1.5),
        ("repeated-point region", &slot, &repeated, 4.0 / 3.0, 1.5),
        ("NaN canvas aspect", &slot, &covering, f64::NAN, 1.5),
        ("zero canvas aspect", &slot, &covering, 0.0, 1.5),
        (
            "infinite photo aspect",
            &slot,
            &covering,
            4.0 / 3.0,
            f64::INFINITY,
        ),
        ("zero photo aspect", &slot, &covering, 4.0 / 3.0, 0.0),
    ] {
        let fit = request.fit(slot, region, canvas_aspect, photo_aspect);
        assert_eq!(fit.transform, request, "{what}");
    }

    // The control: the same request against a region that *has* an interior is
    // fitted, so the rows above are the degeneracy branch and not a request the
    // clamp would leave alone anyway.
    assert_ne!(
        request.fit(&slot, &covering, 4.0 / 3.0, 1.5).transform,
        request,
        "the control region did not move the request"
    );

    // A request with a NaN in it has no framing to compute, and the clamp hands
    // it back rather than turning it into a different kind of nonsense. Only an
    // in-memory document can hold one: `validate` refuses it on load.
    for (what, crop) in [
        (
            "NaN zoom",
            CropTransform {
                zoom: f64::NAN,
                ..request
            },
        ),
        (
            "infinite offset",
            CropTransform {
                offset: (f64::INFINITY, 0.0),
                ..request
            },
        ),
        (
            "NaN rotation",
            CropTransform {
                rotation_deg: f64::NAN,
                ..request
            },
        ),
    ] {
        assert!(crop.validate().is_err(), "{what} must not validate either");
        let fit = crop.fit(&slot, &covering, 4.0 / 3.0, 1.5);
        assert!(same_bits(&fit.transform, &crop), "{what}");
    }
}

#[test]
fn the_zoom_cap_is_a_hard_ceiling() {
    // MAX_ZOOM exists because the zoom sizes the decoded bitmap, so the clamp
    // cannot go past it even to keep its promise: a slot that narrow gets the cap
    // and an uncovered sliver rather than an allocation the machine cannot serve.
    // The fit still returns a valid transform, never an out-of-range one that the
    // document layer would reject.
    let slot = rect_slot(0.0, 0.0, 0.05, 1.0);
    let covering = Polygon::rect(0.0, 0.0, 0.05, 1.0);
    let request = CropTransform {
        zoom: MAX_ZOOM,
        offset: (0.0, 0.0),
        rotation_deg: 40.0,
    };
    let fit = request.fit(&slot, &covering, 1.0, 100.0);
    assert_eq!(fit.transform.zoom, MAX_ZOOM);
    assert!(fit.transform.validate().is_ok());
}
