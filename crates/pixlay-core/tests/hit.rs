//! S6.5: point → slot hit testing, as tests.
//!
//! The promise is small and exact: a normalized canvas point falls in the slot
//! whose outline contains it, and in no slot when it falls in a gutter or off the
//! canvas. The sweep below is that promise over the whole library — every
//! template, every slot, its centroid, one pixel inside every boundary and one
//! pixel outside every boundary — measured against an independent oracle (a
//! winding-number containment, where the implementation is even-odd) rather than
//! against the implementation's own answer.
//!
//! Two things make the "1 px outside" samples discriminating rather than
//! decorative. The library ships one slot that is not a rectangle (an L shape), so
//! a hit test that used bounding boxes would hand the notch to the wrong slot; and
//! a cut template's slots touch the canvas border, so the pixel outside *that*
//! edge is outside every slot and the answer is `None`, not the neighbour.
//!
//! "1 px" is a physical quantity and needs a resolution to mean anything, so the
//! sweep fixes one: the canvas the samples are measured on is the template's own
//! aspect ratio at a 200 mm long edge, at 300 dpi. Normalized coordinates are
//! stretched anisotropically onto that grid, so an offset of one pixel is
//! `1/width` in x and `1/height` in y — the offsets below are computed in pixel
//! space and converted back, which is what makes them one pixel and not "one
//! normalized unit that happens to be a pixel on one axis".

use pixlay_core::templates;
use pixlay_core::{CanvasSpec, PixelSize, Point, Polygon, Slot, Template};

/// Resolution the sweep's one pixel is measured at.
const DPI: u32 = 300;

/// Long edge of the canvas the sweep measures on, in millimetres. Small enough
/// to keep the pixel counts readable, large enough that one pixel is far above
/// float noise (1e-4 of a canvas edge against `EPSILON = 1e-9`).
const CANVAS_MM: f64 = 200.0;

/// Samples taken along each edge of each slot.
const PER_EDGE: usize = 9;

/// The canvas the sweep measures a template's geometry on.
fn canvas_px(aspect: f64) -> (CanvasSpec, PixelSize) {
    let canvas = CanvasSpec::with_ratio(aspect, CANVAS_MM);
    let px = canvas.pixel_size(DPI).expect("a 200 mm canvas at 300 dpi");
    (canvas, px)
}

/// A point in pixel space.
fn to_px(point: Point, px: PixelSize) -> (f64, f64) {
    (
        point.x * f64::from(px.width),
        point.y * f64::from(px.height),
    )
}

/// Back to normalized canvas space.
fn to_norm((x, y): (f64, f64), px: PixelSize) -> Point {
    Point::new(x / f64::from(px.width), y / f64::from(px.height))
}

/// The polygon's area centroid, by the shoelace formula.
///
/// Its own implementation, not a call into the crate under test: the centroid is
/// what the criterion names as the "well inside" sample, so the test has to be
/// able to say whether the centroid really is inside — for an L-shaped slot that
/// is a claim about the shape, not a definition.
fn centroid(polygon: &Polygon) -> Point {
    let (mut area, mut x, mut y) = (0.0, 0.0, 0.0);
    let points = &polygon.points;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let cross = a.x * b.y - b.x * a.y;
        area += cross;
        x += (a.x + b.x) * cross;
        y += (a.y + b.y) * cross;
    }
    area /= 2.0;
    Point::new(x / (6.0 * area), y / (6.0 * area))
}

/// Nonzero-winding containment: a different algorithm from the even-odd crossing
/// count the implementation uses, and the same answer for every simple polygon.
///
/// `winding != 0` rather than `== 1`: a polygon wound the other way is still a
/// polygon, and the contract leaves the winding direction free.
fn winding_contains(polygon: &Polygon, point: Point) -> bool {
    let mut winding = 0;
    let points = &polygon.points;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let left = (b.x - a.x) * (point.y - a.y) - (point.x - a.x) * (b.y - a.y);
        if a.y <= point.y {
            if b.y > point.y && left > 0.0 {
                winding += 1;
            }
        } else if b.y <= point.y && left < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
}

/// The oracle's answer for a whole template: the first slot that contains the
/// point, plus how many slots contain it. "Exactly one" is itself a structural
/// claim about the library, so the count is returned alongside the answer.
fn oracle(template: &Template, point: Point) -> (Option<usize>, usize) {
    let mut owners = template
        .slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| winding_contains(&slot.outline, point));
    let first = owners.next().map(|(index, _)| index);
    (first, usize::from(first.is_some()) + owners.count())
}

/// True when the template's slots tile the canvas exactly (S2's "cut" templates),
/// which the sum of the declared areas decides without a list of names to keep in
/// step. The areas are exact binary values on the 1/32 lattice, so this is an
/// equality and not a tolerance.
fn is_cut(template: &Template) -> bool {
    template.slots.iter().map(|slot| slot.area).sum::<f64>() == 1.0
}

/// The point one pixel outside the edge and the point one pixel inside it, both
/// measured from the edge's midpoint in pixel space.
///
/// Which side is outward is settled by asking the oracle: the winding direction of
/// an outline is free, so the sign of the normal says nothing on its own.
fn across_edge(outline: &Polygon, a: Point, b: Point, t: f64, px: PixelSize) -> (Point, Point) {
    let (ax, ay) = to_px(a, px);
    let (bx, by) = to_px(b, px);
    let (dx, dy) = (bx - ax, by - ay);
    let len = dx.hypot(dy);
    let mid = (ax + t * dx, ay + t * dy);
    let normal = (-dy / len, dx / len);
    let offset = |normal: (f64, f64), factor: f64| {
        to_norm((mid.0 + factor * normal.0, mid.1 + factor * normal.1), px)
    };
    let (outward, inward) = if winding_contains(outline, offset(normal, 1.0)) {
        ((-normal.0, -normal.1), (normal.0, normal.1))
    } else {
        (normal, (-normal.0, -normal.1))
    };
    (offset(outward, 1.0), offset(inward, 1.0))
}

/// True for a point inside the canvas square.
fn on_canvas(point: Point) -> bool {
    (0.0..=1.0).contains(&point.x) && (0.0..=1.0).contains(&point.y)
}

/// Rotate `point` about `centre` by the angle whose sine and cosine are given.
fn rotate_about(point: Point, centre: Point, sin: f64, cos: f64) -> Point {
    let (dx, dy) = (point.x - centre.x, point.y - centre.y);
    Point::new(
        centre.x + dx * cos - dy * sin,
        centre.y + dx * sin + dy * cos,
    )
}

#[test]
fn every_slot_contains_its_own_centroid() {
    let (mut slots, mut inside) = (0u64, 0u64);
    for template in templates::all() {
        for (index, slot) in template.slots.iter().enumerate() {
            slots += 1;
            let centre = centroid(&slot.outline);
            // The criterion names the centroid as the interior sample, so its
            // premise is checked before it is used: a concave slot's centroid can
            // lie outside it, and then this test would be measuring the wrong
            // thing instead of failing.
            assert!(
                winding_contains(&slot.outline, centre),
                "{} slot {index}: the centroid {centre:?} is not inside its own outline",
                template.name
            );
            assert_eq!(
                template.slot_at(centre),
                Some(index),
                "{} slot {index}: hit test disagrees at the centroid {centre:?}",
                template.name
            );
            inside += 1;
        }
    }
    // A canary, not a derived number: if a template left the library the sweep
    // would cover less and still pass. S10 grew it from 64 slots to 152.
    assert_eq!(slots, 152, "the library's slot count changed");
    assert_eq!(inside, slots);
}

#[test]
fn a_point_one_pixel_from_a_boundary_matches_the_analytic_answer() {
    // The four buckets the sweep has to reach, so a sweep that stopped measuring
    // one of the interesting cases fails here instead of passing quietly.
    let (mut inner, mut neighbour, mut gutter, mut border) = (0u64, 0u64, 0u64, 0u64);
    let mut slots = 0u64;

    for template in templates::all() {
        let (_, px) = canvas_px(template.aspect);
        for (index, slot) in template.slots.iter().enumerate() {
            slots += 1;
            for (a, b) in slot.outline.edges() {
                for step in 0..PER_EDGE {
                    let t = (step as f64 + 0.5) / PER_EDGE as f64;
                    let (outside, inside) = across_edge(&slot.outline, a, b, t, px);

                    // One pixel inside the boundary is this slot's, and the
                    // oracle has to agree about all of it.
                    assert!(
                        winding_contains(&slot.outline, inside),
                        "{} slot {index}: the sample 1 px inside edge {a:?}-{b:?} is outside the \
                         oracle's polygon",
                        template.name
                    );
                    assert_eq!(
                        template.slot_at(inside),
                        Some(index),
                        "{} slot {index}: 1 px inside edge {a:?}-{b:?} at t={t} is not this slot",
                        template.name
                    );
                    inner += 1;

                    // One pixel outside is *not* this slot: it is the neighbour
                    // across the edge, or nothing at all when the edge faces the
                    // canvas border or a gutter. Nothing here may assume which.
                    assert!(
                        !winding_contains(&slot.outline, outside),
                        "{} slot {index}: the sample 1 px outside edge {a:?}-{b:?} is still inside \
                         its own polygon",
                        template.name
                    );
                    let (expected, owners) = oracle(&template, outside);
                    assert!(
                        owners <= 1,
                        "{}: {owners} slots claim {outside:?}",
                        template.name
                    );
                    assert_eq!(
                        template.slot_at(outside),
                        expected,
                        "{} slot {index}: 1 px outside edge {a:?}-{b:?} at t={t} is {outside:?}, the \
                         oracle says {expected:?}",
                        template.name
                    );

                    if owners == 1 {
                        neighbour += 1;
                    } else if on_canvas(outside) {
                        gutter += 1;
                    } else {
                        border += 1;
                    }

                    // A cut template tiles the canvas, so a point strictly inside
                    // it is owned exactly once — including every one of these
                    // samples that is not past the border.
                    if on_canvas(outside) {
                        assert_eq!(
                            owners,
                            usize::from(is_cut(&template)),
                            "{}: the cut template leaves {outside:?} uncovered, or a guttered one \
                             covers its gutter",
                            template.name
                        );
                    }
                }
            }
        }
    }

    assert_eq!(
        slots, 152,
        "the library's slot count changed (S10 grew it from 64)"
    );
    assert!(
        inner > 2_000,
        "the sweep measured only {inner} interior samples"
    );
    assert!(
        neighbour > 500,
        "only {neighbour} outside samples landed in a neighbouring slot, so the sweep is not \
         testing the case the criterion is about"
    );
    assert!(
        gutter > 0,
        "no outside sample landed in a gutter; the gutter template stopped being measured"
    );
    assert!(
        border > 0,
        "no outside sample left the canvas, so a slot that faces the border is never checked"
    );
}

#[test]
fn a_gutter_belongs_to_no_slot() {
    // The two shipped templates that do not tile their canvas, and their gutters
    // are the two shapes one can have: a cross that reaches the border in all four
    // directions, and a band that reaches it top and bottom.
    let template = templates::get("grid-4-2x2g").expect("registered");
    assert_eq!(
        template.slot_at(Point::new(0.5, 0.5)),
        None,
        "the gutter owns its centre"
    );
    for y in [0.05, 0.2, 0.5, 0.8, 0.95] {
        assert_eq!(
            template.slot_at(Point::new(0.5, y)),
            None,
            "the vertical gutter is not a slot at y={y}"
        );
    }
    for x in [0.05, 0.5, 0.95] {
        assert_eq!(
            template.slot_at(Point::new(x, 0.5)),
            None,
            "the horizontal gutter is not a slot at x={x}"
        );
    }
    // And the four cells are theirs.
    for (index, at) in [
        (0, (0.2, 0.2)),
        (1, (0.8, 0.2)),
        (2, (0.2, 0.8)),
        (3, (0.8, 0.8)),
    ] {
        assert_eq!(
            template.slot_at(Point::new(at.0, at.1)),
            Some(index),
            "cell {index} is not hit at {at:?}"
        );
    }
    // Outside the canvas is outside every slot, gutter or not.
    assert_eq!(template.slot_at(Point::new(1.01, 0.5)), None);
    assert_eq!(template.slot_at(Point::new(-0.01, 0.5)), None);

    // The pair's gutter (S10) is the other shape: a full-height band between the
    // two panes, so `slot_at` has to answer `None` along a whole line.
    let pair = templates::get("strip-2-2x1g").expect("registered");
    for y in [0.05, 0.2, 0.5, 0.8, 0.95] {
        assert_eq!(
            pair.slot_at(Point::new(0.5, y)),
            None,
            "the pair's gutter is not a slot at y={y}"
        );
    }
    assert_eq!(pair.slot_at(Point::new(0.3, 0.5)), Some(0));
    assert_eq!(pair.slot_at(Point::new(0.7, 0.5)), Some(1));
    assert_eq!(pair.slot_at(Point::new(1.01, 0.5)), None);
    assert_eq!(pair.slot_at(Point::new(-0.01, 0.5)), None);
}

#[test]
fn a_rotated_slot_is_hit_exactly() {
    // No shipped template has a slot that is not axis-aligned, so the rotation
    // half of the criterion builds one: a square rotated by `angle` about the
    // canvas centre, plus a strip along the top edge so the template still has the
    // two slots a template needs.
    //
    // The oracle is the analytic one — rotate the point back by the same angle
    // about the same centre and compare the half extents — which is a different
    // statement from "run a containment test", and the one a reader can check by
    // eye.
    const HALF: f64 = 0.2;
    const CENTRE: Point = Point::new(0.5, 0.5);
    // Two percent of the half extent, about 5 px at the sweep's resolution: a
    // sample this close to an edge is a rounding-level tie, not a hit-test
    // question, and the criterion's own 1 px case is measured in the sweep above.
    const NEAR: f64 = 0.004;

    for angle_deg in [0.0f64, 15.0, 30.0, 45.0, -22.5, 40.0, 90.0] {
        let (sin, cos) = angle_deg.to_radians().sin_cos();
        let outline = Polygon {
            points: [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(sx, sy)| {
                    rotate_about(
                        Point::new(CENTRE.x + sx * HALF, CENTRE.y + sy * HALF),
                        CENTRE,
                        sin,
                        cos,
                    )
                })
                .to_vec(),
        };
        let strip = Polygon::rect(0.0, 0.0, 1.0, 0.04);
        let template = Template {
            name: "rotated-test".to_string(),
            version: 1,
            aspect: 1.0,
            slots: vec![
                Slot {
                    area: outline.area(),
                    outline: outline.clone(),
                },
                Slot {
                    area: strip.area(),
                    outline: strip,
                },
            ],
        };
        template
            .validate()
            .expect("the synthetic template is valid");

        let (mut inside, mut outside) = (0u64, 0u64);
        for iy in 0..=60 {
            for ix in 0..=60 {
                let point = Point::new(
                    CENTRE.x - 0.3 + f64::from(ix) * 0.01,
                    CENTRE.y - 0.3 + f64::from(iy) * 0.01,
                );
                // The point in the square's own frame: undo the rotation.
                let back = rotate_about(point, CENTRE, -sin, cos);
                let (u, v) = (back.x - CENTRE.x, back.y - CENTRE.y);
                if (u.abs() - HALF).abs().min((v.abs() - HALF).abs()) < NEAR {
                    continue;
                }
                let hit = template.slot_at(point);
                if u.abs() < HALF && v.abs() < HALF {
                    assert_eq!(
                        hit,
                        Some(0),
                        "angle {angle_deg}: {point:?} is inside the rotated square but hit {hit:?}"
                    );
                    inside += 1;
                } else {
                    assert_ne!(
                        hit,
                        Some(0),
                        "angle {angle_deg}: {point:?} is outside the rotated square but hit slot 0"
                    );
                    outside += 1;
                }
            }
        }
        assert!(
            inside > 300,
            "angle {angle_deg}: only {inside} interior samples"
        );
        assert!(
            outside > 300,
            "angle {angle_deg}: only {outside} exterior samples"
        );

        // The corners are the sharpest case there is, so they get their own
        // 1 px sample: a point one pixel along the diagonal toward the centre is
        // inside, and the corner itself lies on the boundary and is therefore
        // unspecified.
        let (_, px) = canvas_px(1.0);
        let centre_px = to_px(CENTRE, px);
        for corner in &outline.points {
            let (cx, cy) = to_px(*corner, px);
            let (dx, dy) = (centre_px.0 - cx, centre_px.1 - cy);
            let len = dx.hypot(dy);
            let inward = to_norm((cx + dx / len, cy + dy / len), px);
            assert_eq!(
                template.slot_at(inward),
                Some(0),
                "angle {angle_deg}: 1 px inside the corner {corner:?} is not slot 0"
            );
        }
    }
}
