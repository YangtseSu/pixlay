//! S3: the framing clamp, as tests.
//!
//! The clamp promises one thing — after it, the photo covers the entire slot —
//! and the sweep below is that promise over the whole matrix this build ships:
//! every template, every slot shape, and combinations of rotation, zoom, offset
//! and photo aspect.
//!
//! Coverage is measured the way the renderer places the photo (a canvas point is
//! inside the photo when its coordinates in the photo's own frame are within its
//! half extents), not by reusing the clamp's own vertex test, so a wrong centre,
//! a wrong rotation direction or a wrong photo aspect cannot pass.

use pixlay_core::templates;
use pixlay_core::{CLAMP_ZOOM_LIMIT, CropTransform, MAX_ZOOM, Point, Polygon, Slot};

/// Tolerance for "the photo covers the slot" (docs/STEPS.md, S3 review: "the
/// epsilon for 'covers the entire slot' is given a number, normalized 1e-6
/// suggested, or ≤0.5px at 300dpi").
///
/// 1e-6 of a canvas edge is 0.014 px on A0's long edge at 300 dpi, far under the
/// suggested 0.5 px, while the clamp's own arithmetic is exact to ~1e-15 — so
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

/// Sample points of an outline: its vertices, its edge midpoints and a grid over
/// its bounding box, keeping the points inside.
///
/// The vertices are the sharpest samples there are — a rectangle that covers the
/// polygon must contain them, and at the floor it contains them exactly — while
/// the grid inside catches a wrong centre or a wrong rotation direction.
fn samples(outline: &Polygon, grid: u32) -> Vec<Point> {
    let bbox = outline.bbox();
    let mut points: Vec<Point> = outline.points.clone();
    points.extend(
        outline
            .edges()
            .map(|(a, b)| Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)),
    );
    for iy in 0..grid {
        for ix in 0..grid {
            let point = Point::new(
                bbox.x0 + bbox.width() * (f64::from(ix) + 0.5) / f64::from(grid),
                bbox.y0 + bbox.height() * (f64::from(iy) + 0.5) / f64::from(grid),
            );
            if outline.contains(point) {
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

/// Bit equality of two transforms. `==` calls a NaN request unequal to itself,
/// and "returned untouched" has to hold for those too.
fn same_bits(a: &CropTransform, b: &CropTransform) -> bool {
    a.zoom.to_bits() == b.zoom.to_bits()
        && a.offset.0.to_bits() == b.offset.0.to_bits()
        && a.offset.1.to_bits() == b.offset.1.to_bits()
        && a.rotation_deg.to_bits() == b.rotation_deg.to_bits()
}

#[test]
fn every_framing_covers_its_slot() {
    let rotations = [-45.0, -18.0, -7.5, 0.0, 11.0, 45.0];
    let offsets = [
        (-1.0, 1.0),
        (-0.4, 0.25),
        (0.0, 0.0),
        (0.65, -0.8),
        (1.0, -1.0),
    ];
    let photo_aspects = [0.5, 0.8, 1.0, 1.5, 2.4];
    let zooms = [0.35, 1.0, 3.0];
    let (mut checked, mut limited, mut panned) = (0u64, 0u64, 0u64);

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
                            let fit = request.fit(slot, template.aspect, photo_aspect);
                            let transform = fit.transform;
                            let what = format!(
                                "{} slot {index} for {request:?} at photo aspect {photo_aspect}",
                                template.name
                            );
                            assert!(transform.validate().is_ok(), "{what}: {transform:?}");
                            assert!(transform.zoom <= MAX_ZOOM, "{what}");
                            assert!(
                                transform.rotation_deg.abs() <= rotation_deg.abs() + 1e-12,
                                "{what}: the clamp may only reduce the angle, got {}",
                                transform.rotation_deg
                            );
                            if rotation_deg != 0.0 {
                                assert_eq!(
                                    transform.rotation_deg.signum(),
                                    rotation_deg.signum(),
                                    "{what}: the rotation must keep its direction"
                                );
                            }
                            let outside =
                                worst(&transform, slot, template.aspect, photo_aspect, &points);
                            assert!(
                                outside.is_finite() && outside <= 1.0 + COVERAGE_EPSILON,
                                "{what}: measured {outside} half extents (1.0 = the photo edge)"
                            );
                            checked += 1;
                            limited += u64::from(fit.rotation_limited);
                            panned += u64::from(transform.offset != offset);
                        }
                    }
                }
            }
        }
    }

    // The sweep is only evidence if it actually reaches the branches: 12
    // templates and 63 slots of the shipped library.
    assert!(checked > 10_000, "the sweep must actually sweep: {checked}");
    assert!(limited > 0, "no framing in the sweep hit the degradation");
    assert!(panned > 0, "no framing in the sweep hit the pan clamp");
}

#[test]
fn the_floor_is_the_smallest_zoom_that_covers() {
    // With the photo centred and upright the required zoom is exactly
    // `max(1, photo_aspect * slot_height / slot_width)`, and the fit lands on it:
    // a clamp that magnified more than necessary would silently crop every photo
    // it touched, one that magnified less would leave a sliver white.
    for template in templates::all() {
        for (index, slot) in template.slots.iter().enumerate() {
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
                let fit = request.fit(slot, template.aspect, photo_aspect);
                let what = format!(
                    "{} slot {index} at photo aspect {photo_aspect}",
                    template.name
                );
                assert!(
                    (fit.transform.zoom - expected).abs() <= 1e-12,
                    "{what}: floor {} instead of {expected}",
                    fit.transform.zoom
                );
                assert_eq!(fit.transform.offset, (0.0, 0.0), "{what}");
                assert_eq!(fit.transform.rotation_deg, 0.0, "{what}");
                assert!(!fit.rotation_limited, "{what}");
                let outside = worst(&fit.transform, slot, template.aspect, photo_aspect, &points);
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
    let points = samples(&slot.outline, 24);
    let at = |rotation_deg| CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg,
    };

    let upright = at(0.0).fit(&slot, canvas_aspect, photo_aspect);
    assert!((upright.transform.zoom - 1.0).abs() <= 1e-12);
    assert!(!upright.rotation_limited);

    for angle in [12.0, 20.0, -20.0] {
        let rotated = at(angle).fit(&slot, canvas_aspect, photo_aspect);
        assert!(
            rotated.transform.zoom > upright.transform.zoom,
            "{angle} degrees: zoom {} is not above the upright {}",
            rotated.transform.zoom,
            upright.transform.zoom
        );
        assert_eq!(rotated.transform.rotation_deg, angle);
        assert!(!rotated.rotation_limited);
        for (crop, what) in [
            (&upright.transform, "upright"),
            (&rotated.transform, "rotated"),
        ] {
            let outside = worst(crop, &slot, canvas_aspect, photo_aspect, &points);
            assert!(
                outside <= 1.0 + COVERAGE_EPSILON,
                "{what} at {angle} degrees: measured {outside}"
            );
        }
    }

    // A request above the floor is not pulled back to it: the user's own zoom is
    // the one they asked for, and the angle has to be affordable at it.
    let zoomed = CropTransform {
        zoom: 30.0,
        offset: (0.0, 0.0),
        rotation_deg: 20.0,
    }
    .fit(&slot, canvas_aspect, photo_aspect);
    assert_eq!(zoomed.transform.zoom, 30.0);
    assert_eq!(zoomed.transform.rotation_deg, 20.0);
}

#[test]
fn the_fit_follows_the_slot_shape() {
    // Same request, same photo, two slot shapes: the fit is a property of the
    // slot as much as of the request. A square photo fills a square slot at 1x
    // and a slot four times taller than it is wide at 2.5x.
    let square = rect_slot(0.0, 0.0, 1.0, 1.0);
    let tall = rect_slot(0.3, 0.0, 0.7, 1.0);
    let request = CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg: 0.0,
    };
    let square_fit = request.fit(&square, 1.0, 1.0);
    let tall_fit = request.fit(&tall, 1.0, 1.0);
    assert!((square_fit.transform.zoom - 1.0).abs() <= 1e-12);
    assert!((tall_fit.transform.zoom - 2.5).abs() <= 1e-12);
    for (slot, fit) in [(&square, &square_fit), (&tall, &tall_fit)] {
        let points = samples(&slot.outline, 24);
        let outside = worst(&fit.transform, slot, 1.0, 1.0, &points);
        assert!(outside <= 1.0 + COVERAGE_EPSILON, "measured {outside}");
    }
}

#[test]
fn a_wide_slot_limits_the_rotation_instead_of_magnifying_without_bound() {
    // Covering a slot of physical aspect `r` with a matching photo needs
    // `r*sin(t) + cos(t)` at angle `t`, so straightening costs magnification that
    // grows with the slot's width. Past `CLAMP_ZOOM_LIMIT` times the upright floor
    // the clamp keeps the widest angle that fits — measured (2026-09-21) a 4:3
    // slot keeps 27.3 degrees of the 45 asked for, and the zoom it needs there is
    // 1.5x.
    let canvas_aspect = 4.0 / 3.0;
    let photo_aspect = 4.0 / 3.0;
    let slot = rect_slot(0.0, 0.0, 1.0, 1.0);
    let points = samples(&slot.outline, 24);
    let request = CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg: 45.0,
    };
    let fit = request.fit(&slot, canvas_aspect, photo_aspect);

    assert!(
        fit.rotation_limited,
        "45 degrees on a 4:3 slot is past the limit"
    );
    let kept = fit.transform.rotation_deg;
    assert!(
        (15.0..35.0).contains(&kept),
        "the kept angle must be the measured 27.3 degrees, got {kept}"
    );
    // The kept angle is the *widest* that fits: at the limit's zoom it covers,
    // and one degree wider does not.
    let limit = CLAMP_ZOOM_LIMIT * 1.0;
    let at_limit = |rotation_deg| CropTransform {
        zoom: limit,
        offset: (0.0, 0.0),
        rotation_deg,
    };
    let covered = worst(&at_limit(kept), &slot, canvas_aspect, photo_aspect, &points);
    assert!(
        covered <= 1.0 + COVERAGE_EPSILON,
        "the kept angle must fit within the limit, measured {covered}"
    );
    let wider = worst(
        &at_limit(kept + 1.0),
        &slot,
        canvas_aspect,
        photo_aspect,
        &points,
    );
    assert!(
        wider > 1.0 + COVERAGE_EPSILON,
        "a degree wider must not fit, measured {wider}"
    );
    // And what the clamp does keep still covers the slot.
    let outside = worst(&fit.transform, &slot, canvas_aspect, photo_aspect, &points);
    assert!(outside <= 1.0 + COVERAGE_EPSILON, "measured {outside}");
}

#[test]
fn an_elongated_slot_keeps_its_rotation() {
    // The degradation limit is relative to the upright floor, not an absolute
    // zoom, and an elongated slot is where that matters: the ten-column strip is
    // 0.125 x 1.0 of a 16:9 canvas, so a 4:3 photo covers it upright at
    // `4/3 * 16/9 / (16/9 * 0.125) = 6x` — and at 45 degrees it needs *less*
    // (5.19x), because a narrow slot fits a rotated photo better than an upright
    // one. An absolute reading of the limit would refuse to rotate every strip
    // template there is; the relative one has no reason to touch the angle.
    let template = templates::get("strip-10-10x1").expect("the strip template");
    let photo_aspect = 4.0 / 3.0;
    let slot = &template.slots[0];
    let points = samples(&slot.outline, 24);
    let request = CropTransform {
        zoom: 1.0,
        offset: (0.0, 0.0),
        rotation_deg: 45.0,
    };
    let fit = request.fit(slot, template.aspect, photo_aspect);
    assert!(!fit.rotation_limited, "the angle must survive here");
    assert_eq!(fit.transform.rotation_deg, 45.0);

    let upright = CropTransform {
        rotation_deg: 0.0,
        ..request
    }
    .fit(slot, template.aspect, photo_aspect);
    assert!(
        (upright.transform.zoom - 6.0).abs() <= 1e-12,
        "the upright floor for this slot is 6x, got {}",
        upright.transform.zoom
    );
    assert!(
        fit.transform.zoom < upright.transform.zoom,
        "rotating this slot needs less zoom, not more: {} against {}",
        fit.transform.zoom,
        upright.transform.zoom
    );
    for crop in [&fit.transform, &upright.transform] {
        let outside = worst(crop, slot, template.aspect, photo_aspect, &points);
        assert!(outside <= 1.0 + COVERAGE_EPSILON, "measured {outside}");
    }
}

#[test]
fn fitting_a_fit_returns_it() {
    // Idempotence is what lets the clamp run on every edit *and* at the render
    // boundary: the second pass changes nothing and reports nothing.
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
        ),
    ];
    for (slot, canvas_aspect, photo_aspect, request) in cases {
        let once = request.fit(&slot, canvas_aspect, photo_aspect);
        let twice = once.transform.fit(&slot, canvas_aspect, photo_aspect);
        assert_eq!(twice.transform, once.transform, "for {request:?}");
        assert!(
            !twice.rotation_limited,
            "a fit is never itself in need of the degradation: {request:?}"
        );
    }
}

#[test]
fn a_shape_that_cannot_be_fitted_is_returned_untouched() {
    // A degenerate outline and an aspect that is not a positive finite number
    // have no coverage promise to keep, so the clamp hands the request back
    // instead of inventing a framing for a shape that is not there.
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
    for (what, slot, canvas_aspect, photo_aspect) in [
        ("degenerate outline", &degenerate, 4.0 / 3.0, 1.5),
        ("NaN canvas aspect", &slot, f64::NAN, 1.5),
        ("zero canvas aspect", &slot, 0.0, 1.5),
        ("infinite photo aspect", &slot, 4.0 / 3.0, f64::INFINITY),
        ("zero photo aspect", &slot, 4.0 / 3.0, 0.0),
    ] {
        let fit = request.fit(slot, canvas_aspect, photo_aspect);
        assert_eq!(fit.transform, request, "{what}");
        assert!(!fit.rotation_limited, "{what}");
    }

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
        let fit = crop.fit(&slot, 4.0 / 3.0, 1.5);
        assert!(same_bits(&fit.transform, &crop), "{what}");
        assert!(!fit.rotation_limited, "{what}");
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
    let request = CropTransform {
        zoom: MAX_ZOOM,
        offset: (0.0, 0.0),
        rotation_deg: 40.0,
    };
    let fit = request.fit(&slot, 1.0, 100.0);
    assert_eq!(fit.transform.zoom, MAX_ZOOM);
    assert!(fit.transform.validate().is_ok());
}
