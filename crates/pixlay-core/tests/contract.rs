//! The S1 contract, as tests: shapes, limits, versioning and the geometry the
//! probes and the renderer rely on.

use std::path::PathBuf;

use pixlay_core::{
    CanvasSpec, Cell, CollageDoc, CropTransform, DOC_VERSION, Point, Polygon, Project, Rgba8,
    Template,
};

fn temp_dir(name: &str) -> PathBuf {
    // Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-core-tests/{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test directory");
    dir
}

fn two_slot_template() -> Template {
    let left = Polygon::rect(0.05, 0.05, 0.5, 0.95);
    let right = Polygon::rect(0.5, 0.05, 0.95, 0.95);
    let area = 0.45 * 0.9;
    Template {
        name: "test-2".to_string(),
        version: 1,
        aspect: 4.0 / 3.0,
        slots: vec![
            pixlay_core::Slot {
                outline: left,
                area,
            },
            pixlay_core::Slot {
                outline: right,
                area,
            },
        ],
    }
}

fn two_slot_doc() -> CollageDoc {
    CollageDoc::new(
        CanvasSpec::with_ratio(4.0 / 3.0, 120.0),
        two_slot_template(),
    )
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}

#[test]
fn serde_round_trip_is_field_identical() {
    let mut doc = two_slot_doc();
    doc.cells[0] = Cell {
        source: Some(PathBuf::from("photos/a.png")),
        // A free angle: past the old ±45° cap, and inside the `(-180, 180]` the
        // document stores, so the round trip is where "the frame and the free
        // rotation survive a save" is checked rather than assumed.
        crop: CropTransform {
            zoom: 1.25,
            offset: (0.1, -0.2),
            rotation_deg: -172.5,
        },
    };
    doc.frame = pixlay_core::Frame {
        gap_rel: 0.02,
        radius_rel: 0.03,
        color: Rgba8 {
            r: 20,
            g: 40,
            b: 60,
            a: 255,
        },
    };
    doc.validate().expect("document is valid");

    let json = doc.to_json().expect("serializes");
    let back = CollageDoc::from_json(&json).expect("deserializes");
    assert_eq!(doc, back);
    // Fields, not just the round trip: a missing `deny_unknown_fields` or a
    // renamed field would still round-trip through this crate's own types.
    for key in [
        "\"docVersion\"",
        "\"canvas\"",
        "\"widthMm\"",
        "\"template\"",
        "\"version\"",
        "\"slots\"",
        "\"outline\"",
        "\"cells\"",
        "\"crop\"",
        "\"zoom\"",
        "\"rotationDeg\"",
        "\"frame\"",
        "\"gapRel\"",
        "\"radiusRel\"",
    ] {
        assert!(json.contains(key), "{key} missing from {json}");
    }
    // The shape after the S12c purity cut: what the document does *not* carry is as
    // much of the contract as what it does, and a field that quietly came back
    // would fail here rather than at a user's project file.
    for gone in ["\"text\"", "\"textFallback\"", "\"filter\"", "\"grade\""] {
        assert!(!json.contains(gone), "{gone} is back in {json}");
    }
    let outline = serde_json::to_string(&Polygon::rect(0.05, 0.05, 0.5, 0.95)).expect("serializes");
    assert_eq!(
        outline, "[[0.05,0.05],[0.5,0.05],[0.5,0.95],[0.05,0.95]]",
        "outline points are two-element arrays"
    );
}

#[test]
fn point_is_a_two_element_array() {
    let json = serde_json::to_string(&Point::new(0.25, 0.75)).expect("serializes");
    assert_eq!(json, "[0.25,0.75]");
    let back: Point = serde_json::from_str("[1,0]").expect("deserializes");
    assert_eq!(back, Point::new(1.0, 0.0));
}

#[test]
fn document_from_a_newer_version_is_rejected() {
    let doc = two_slot_doc();
    let json = doc.to_json().expect("serializes").replace(
        &format!("\"docVersion\": {DOC_VERSION}"),
        &format!("\"docVersion\": {}", DOC_VERSION + 1),
    );
    let err = CollageDoc::from_json(&json).expect_err("newer version must be rejected");
    assert_eq!(
        err.to_string(),
        format!(
            "document version {} is newer than the supported version {DOC_VERSION}",
            DOC_VERSION + 1
        )
    );
}

#[test]
fn older_versions_are_refused_with_an_actionable_message() {
    // Policy (decided at the S1 review, docs/CONTRACT.md §1): adding a field does
    // NOT bump `DOC_VERSION`, so a version mismatch always means a breaking change
    // and the document cannot be interpreted. There is no migration, so the message
    // has to tell the user what to do instead of leaving them stuck.
    assert_eq!(
        pixlay_core::DOC_VERSION_MIN,
        DOC_VERSION,
        "this build reads exactly one version; widening the window is a deliberate policy change"
    );

    let doc = two_slot_doc();
    let current = doc.to_json().expect("serializes");
    CollageDoc::from_json(&current).expect("the current version loads");

    let older = current.replace(
        &format!("\"docVersion\": {DOC_VERSION}"),
        &format!("\"docVersion\": {}", DOC_VERSION - 1),
    );
    let err = CollageDoc::from_json(&older).expect_err("an older version must be refused");
    assert!(
        err.to_string().contains("rebuild the project"),
        "the message must be actionable, got: {err}"
    );
}

#[test]
fn a_canvas_whose_aspect_contradicts_its_template_is_rejected() {
    // The template's geometry is normalized, so it is stretched onto whatever
    // canvas is declared. A mismatch silently distorts the layout and nothing
    // downstream can see it.
    let mut doc = two_slot_doc();
    doc.canvas = CanvasSpec::new(160.0, 90.0); // 16:9 against a 4:3 template
    let err = doc
        .validate()
        .expect_err("aspect mismatch must be rejected");
    assert!(
        err.to_string()
            .contains("does not match the template aspect"),
        "{err}"
    );

    // A rounding-level difference is fine: canvases are authored in millimetres.
    let mut doc = two_slot_doc();
    doc.canvas = CanvasSpec::new(120.000_000_1, 90.0);
    doc.validate()
        .expect("millimetre rounding is not a mismatch");
}

#[test]
fn a_project_from_the_removed_shape_is_refused_by_version_not_by_field() {
    // A version-1 project is a project with `grade`, `filter`, `text` and
    // `textFallback` in it. `deny_unknown_fields` would name `grade` while parsing,
    // which tells a user nothing about what to do; reading the version first is
    // what turns it into the actionable "rebuild the project with this version".
    let doc = two_slot_doc();
    let current = doc.to_json().expect("serializes");
    let older = current
        .replace(
            &format!("\"docVersion\": {DOC_VERSION}"),
            "\"docVersion\": 1",
        )
        .replacen(
            "\"crop\":",
            "\"grade\": { \"factor\": 1.1, \"saturation\": 0.9, \"delta\": -0.1 },\n      \"crop\":",
            1,
        );
    let err = CollageDoc::from_json(&older).expect_err("a version-1 document is refused");
    assert!(
        err.to_string().contains("rebuild the project"),
        "the refusal must be actionable, got: {err}"
    );
    assert!(
        !err.to_string().contains("unknown field"),
        "the version has to be read before the fields, got: {err}"
    );
}

#[test]
fn unknown_json_fields_are_rejected() {
    let doc = two_slot_doc();
    let json = doc
        .to_json()
        .expect("serializes")
        .replace("\"canvas\": {", "\"canvas\": {\n      \"nonsense\": 1,");
    let err = CollageDoc::from_json(&json).expect_err("unknown field must be rejected");
    assert!(err.to_string().contains("nonsense"), "{err}");
}

#[test]
fn slot_count_outside_the_limits_is_rejected() {
    // 10 is in the list because it *was* legal: `strip-10-10x1` shipped ten slots
    // until S12c removed it together with the product's above-nine range.
    for count in [0, 1, 10, 11] {
        let mut doc = two_slot_doc();
        doc.template.slots = (0..count)
            .map(|_| pixlay_core::Slot {
                outline: Polygon::rect(0.05, 0.05, 0.95, 0.95),
                area: 0.81,
            })
            .collect();
        doc.cells = vec![Cell::default(); count];
        let err = doc.validate().expect_err("slot count must be limited");
        assert!(
            err.to_string().contains("slots; the limit is 2..=9"),
            "{count}: {err}"
        );
    }
}

#[test]
fn cells_must_match_slots() {
    let mut doc = two_slot_doc();
    doc.cells.pop();
    let err = doc
        .validate()
        .expect_err("cell count must match slot count");
    assert_eq!(
        err.to_string(),
        "document has 1 cells but its template has 2 slots"
    );
}

#[test]
fn wrong_slot_area_is_rejected() {
    let mut doc = two_slot_doc();
    doc.template.slots[0].area = 0.5;
    let err = doc
        .validate()
        .expect_err("declared area must match the outline");
    assert!(err.to_string().contains("declares area 0.5"), "{err}");
}

#[test]
fn canvas_limits_and_rounding() {
    // A4 at 300 dpi is the rounding reference in docs/CONTRACT.md §2.
    let a4 = CanvasSpec::A4_PORTRAIT.pixel_size(300).expect("A4@300dpi");
    assert_eq!((a4.width, a4.height), (2480, 3508));
    assert_eq!(
        CanvasSpec::A0_PORTRAIT.pixel_size(300).unwrap().pixels(),
        139_489_119
    );
    // Half away from zero: 100 mm at 300 dpi is 1181.1 px.
    assert_eq!(
        CanvasSpec::new(100.0, 100.0).pixel_size(300).unwrap().width,
        1181
    );
    assert_eq!(
        CanvasSpec::new(100.0, 100.0).pixel_size(150).unwrap().width,
        591
    );

    for dpi in [71, 601, 0] {
        let err = CanvasSpec::A4_PORTRAIT
            .pixel_size(dpi)
            .expect_err("dpi must be limited");
        assert!(err.to_string().contains("dpi"), "{dpi}: {err}");
    }

    // A0 at 600 dpi would be 558 MP, over the 200 MP budget.
    let err = CanvasSpec::A0_PORTRAIT
        .pixel_size(600)
        .expect_err("canvas pixel budget");
    assert!(err.to_string().contains("557976342 pixels"), "{err}");

    assert!(CanvasSpec::new(0.0, 100.0).validate().is_err());
    assert!(CanvasSpec::new(f64::NAN, 100.0).validate().is_err());
    assert!(CanvasSpec::new(5000.0, 100.0).validate().is_err());
}

/// The pixel-count export mode (S6): the long edge is exact, the other edge keeps
/// the ratio, and the resolution the file then carries is derived from the grid.
#[test]
fn a_long_edge_is_exact_and_the_other_edge_keeps_the_ratio() {
    let square = CanvasSpec::SQUARE;
    let pixel = square
        .pixel_size_for_long_edge(1000)
        .expect("a square grid");
    assert_eq!((pixel.width, pixel.height), (1000, 1000));

    // A4 landscape: 297 x 210 mm. 1000 px on the long edge, and
    // round(210 / 297 * 1000) = 707 on the short one.
    let pixel = CanvasSpec::A4_LANDSCAPE
        .pixel_size_for_long_edge(1000)
        .expect("landscape");
    assert_eq!((pixel.width, pixel.height), (1000, 707));

    // The same canvas portrait puts the exact edge on the other axis.
    let pixel = CanvasSpec::A4_PORTRAIT
        .pixel_size_for_long_edge(1000)
        .expect("portrait");
    assert_eq!((pixel.width, pixel.height), (707, 1000));

    // The rounding rule is half away from zero, the same as `pixel_size`'s:
    // 210 / 595 * 1000 = 352.94, and 1000 is exact either way.
    let pixel = CanvasSpec::new(595.0, 210.0)
        .pixel_size_for_long_edge(1000)
        .expect("an A-series long edge");
    assert_eq!((pixel.width, pixel.height), (1000, 353));

    // A0 landscape at 16000 px: 16000 x 11317 (round(841 / 1189 * 16000)), and the
    // resolution the grid works out to is 341.8 dpi.
    let a0 = CanvasSpec::A0_LANDSCAPE
        .pixel_size_for_long_edge(16_000)
        .expect("well inside the budget");
    assert_eq!((a0.width, a0.height), (16000, 11317));
    assert!((CanvasSpec::A0_LANDSCAPE.dpi_for(a0) - 16000.0 * 25.4 / 1189.0).abs() < 1e-9);

    // The flag's range and the canvas budget are two different limits and both
    // apply: 30000 is inside the range, and a 4:3 canvas at that edge is 636.6 MP,
    // over the 200 MP budget. The budget is what refuses it, and the message says
    // how much it was.
    let err = CanvasSpec::A0_LANDSCAPE
        .pixel_size_for_long_edge(pixlay_core::MAX_LONG_EDGE_PX)
        .expect_err("over the canvas budget");
    assert!(err.to_string().contains("636600000 pixels"), "{err}");

    // A resolution in physical mode is echoed, not derived: an A4 at 300 dpi is
    // 2480 x 3508 px, which is 300.01 dpi on the long edge and 299.96 on the short
    // one, and the file still says 300 — that is the number the user asked for.
    let a4 = CanvasSpec::A4_PORTRAIT.pixel_size(300).expect("A4@300dpi");
    assert!(CanvasSpec::A4_PORTRAIT.dpi_for(a4) > 300.0);

    // The range, and the canvas budget the grid still has to respect: a square
    // canvas at the maximum edge is 900 MP.
    for pixels in [0, pixlay_core::MAX_LONG_EDGE_PX + 1] {
        let err = square
            .pixel_size_for_long_edge(pixels)
            .expect_err("outside the range");
        assert!(err.to_string().contains("long edge"), "{pixels}: {err}");
    }
    let err = square
        .pixel_size_for_long_edge(20000)
        .expect_err("over the canvas budget");
    assert!(err.to_string().contains("400000000 pixels"), "{err}");
    // Exactly at the budget is inside it (14142^2 = 199,996,164).
    let pixel = square.pixel_size_for_long_edge(14142).expect("inside");
    assert_eq!(pixel.pixels(), 199_996_164);
}

#[test]
fn crop_transform_limits() {
    CropTransform::IDENTITY.validate().expect("identity");
    for crop in [
        CropTransform {
            zoom: 0.0,
            ..CropTransform::IDENTITY
        },
        CropTransform {
            zoom: f64::NAN,
            ..CropTransform::IDENTITY
        },
        CropTransform {
            offset: (1.5, 0.0),
            ..CropTransform::IDENTITY
        },
        // The zoom sizes the decoded bitmap, so it needs an upper bound: 1e5 used
        // to abort on a failed 30-petabyte allocation and 1e308 wrapped the width
        // to i32::MIN.
        CropTransform {
            zoom: 1e308,
            ..CropTransform::IDENTITY
        },
        CropTransform {
            zoom: pixlay_core::MAX_ZOOM + 1.0,
            ..CropTransform::IDENTITY
        },
    ] {
        assert!(crop.validate().is_err(), "{crop:?} must be rejected");
    }
    // The zoom and offset limits are inclusive: exactly at them is valid, a hair
    // past is not.
    CropTransform {
        offset: (1.0, -1.0),
        ..CropTransform::IDENTITY
    }
    .validate()
    .expect("the limits themselves are allowed");

    // The rotation has **no** range (2026-09-22's ruling: the angle is free), so
    // every finite value validates — including the ones the old ±45° cap refused,
    // which is what makes a widened range a change no project can notice.
    for rotation_deg in [0.0, -45.0, 45.1, 180.0, -180.0, 450.0, 1e9, -1e9] {
        CropTransform {
            rotation_deg,
            ..CropTransform::IDENTITY
        }
        .validate()
        .unwrap_or_else(|error| panic!("{rotation_deg} degrees must validate: {error}"));
    }
    // What is left is the domain: a number arithmetic can be done on. The error is
    // its own variant rather than a range, because there is no range to be outside
    // of and a message naming one would be a lie.
    for rotation_deg in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = CropTransform {
            rotation_deg,
            ..CropTransform::IDENTITY
        }
        .validate()
        .expect_err("a non-finite angle has no framing");
        assert!(
            err.to_string().contains("must be a finite number"),
            "{rotation_deg}: {err}"
        );
        assert!(
            !err.to_string().contains("must be in"),
            "{rotation_deg}: {err}"
        );
    }
}

#[test]
fn a_finite_rotation_is_normalized_into_the_half_open_turn() {
    // A dial cannot accumulate turns: 450 degrees and 90 draw the same picture, and
    // a project file has no business saying 450. The range is `(-180, 180]`, so
    // `-180` — the one value the range excludes — lands on the `180` it equals.
    let wrap = |rotation_deg| {
        CropTransform {
            rotation_deg,
            ..CropTransform::IDENTITY
        }
        .normalized()
        .rotation_deg
    };
    for (given, expected) in [
        (0.0, 0.0),
        (-0.0, 0.0),
        (45.0, 45.0),
        (-45.25, -45.25),
        (180.0, 180.0),
        (-180.0, 180.0),
        (181.0, -179.0),
        (-181.0, 179.0),
        (360.0, 0.0),
        (450.0, 90.0),
        (-450.0, -90.0),
        (720.5, 0.5),
        // 10^9 mod 360 is 280, so the wrapped angle is 280 - 360 = -80.
        (1e9, -80.0),
    ] {
        assert_eq!(wrap(given), expected, "{given} degrees");
    }
    // Idempotent, so normalizing on load and again on an edit cannot drift.
    for rotation_deg in [-720.5, -180.0, 0.0, 179.9, 1e9] {
        assert_eq!(
            wrap(wrap(rotation_deg)),
            wrap(rotation_deg),
            "{rotation_deg}"
        );
    }
    // A non-finite angle is left exactly as it is: the document is about to be
    // refused, and the error has to quote what the file said.
    for rotation_deg in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            wrap(rotation_deg).to_bits(),
            rotation_deg.to_bits(),
            "{rotation_deg}"
        );
    }
}

#[test]
fn polygon_clipping_is_the_intersection() {
    use pixlay_core::Rect;

    let square = Polygon::rect(0.2, 0.2, 0.8, 0.8);

    // A clip that already contains the polygon is the identity **bit for bit**,
    // which is what keeps the unframed fit the arithmetic it was: the frame's
    // covering region is the outline clipped to its own bounding box.
    let unchanged = square.clipped_to(square.bbox());
    assert_eq!(unchanged, square);
    for (a, b) in unchanged.points.iter().zip(&square.points) {
        assert_eq!(
            (a.x.to_bits(), a.y.to_bits()),
            (b.x.to_bits(), b.y.to_bits())
        );
    }

    // A corner cut: the result is the rectangle they share.
    let cut = square.clipped_to(Rect {
        x0: 0.5,
        y0: 0.5,
        x1: 1.0,
        y1: 1.0,
    });
    close(cut.area(), 0.09);
    assert_eq!(cut.bbox().x0, 0.5);
    assert_eq!(cut.bbox().y1, 0.8);

    // A polygon the clip misses entirely has no interior left.
    let missed = square.clipped_to(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 0.1,
        y1: 0.1,
    });
    assert!(missed.points.is_empty(), "{missed:?}");
    assert_eq!(missed.area(), 0.0);
    assert!(missed.validate().is_err(), "an empty polygon is not a slot");

    // The concave slot's own case: the notch is what must not survive a clip that
    // cuts it away, and the L's three shared corners must.
    let l_shape = Polygon {
        points: vec![
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.0),
            Point::new(0.5, 0.5),
            Point::new(1.0, 0.5),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ],
    };
    let clipped = l_shape.clipped_to(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 1.0,
        y1: 0.75,
    });
    close(clipped.area(), 0.5);
    assert_eq!(clipped.bbox().y1, 0.75, "the clip cut the top");
    assert!(clipped.contains(Point::new(0.25, 0.25)));
    assert!(clipped.contains(Point::new(0.75, 0.6)));
    assert!(
        !clipped.contains(Point::new(0.75, 0.25)),
        "the notch stays out"
    );
    assert!(!clipped.contains(Point::new(0.75, 0.8)), "above the clip");
}

#[test]
fn frame_limits() {
    use pixlay_core::Frame;

    Frame::default().validate().expect("the default frame");
    // The frame does not move with the document: no gap, no radius, white.
    let default = Frame::default();
    assert_eq!(default.gap_rel, 0.0);
    assert_eq!(default.radius_rel, 0.0);
    assert_eq!(default.color, Rgba8::WHITE);
    assert!(default.is_identity());

    for (what, frame) in [
        (
            "negative gap",
            Frame {
                gap_rel: -0.01,
                ..Frame::default()
            },
        ),
        (
            "gap past the canvas",
            Frame {
                gap_rel: 1.5,
                ..Frame::default()
            },
        ),
        (
            "negative radius",
            Frame {
                radius_rel: -1.0,
                ..Frame::default()
            },
        ),
        (
            "NaN gap",
            Frame {
                gap_rel: f64::NAN,
                ..Frame::default()
            },
        ),
        (
            "infinite radius",
            Frame {
                radius_rel: f64::INFINITY,
                ..Frame::default()
            },
        ),
    ] {
        assert!(frame.validate().is_err(), "{what} must be rejected");
    }
    // A translucent backdrop is refused: the canvas is painted, not blended, and
    // "preview and export are the same picture" depends on it.
    let translucent = Frame {
        color: Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 254,
        },
        ..Frame::default()
    };
    let err = translucent.validate().expect_err("translucent");
    assert!(err.to_string().contains("alpha"), "{err}");

    // The frame's length bound is part of the contract, like the other limits.
    const { assert!(pixlay_core::MAX_FRAME_REL == 1.0) };
    const { assert!(pixlay_core::MAX_CANVAS_PIXELS == 200_000_000) };
}

#[test]
fn a_gap_that_empties_a_cell_is_refused_naming_it() {
    // The frame is checked against the geometry it decorates: a gap is taken off
    // every side of every cell, and a cell with nothing left is not a document this
    // build can render. The error names the slot, because "the frame is too big" is
    // not actionable for a layout with sixteen cells of different sizes.
    let mut doc = two_slot_doc();
    doc.frame.gap_rel = 0.95;
    let err = doc
        .validate()
        .expect_err("a 0.95 gap empties a 0.9-tall slot");
    assert!(err.to_string().contains("slot 0"), "{err}");
    assert!(err.to_string().contains("visible area"), "{err}");

    // Just inside the limit is accepted, and it is the *cell* that decides: a flat
    // but tall slot survives a gap that would empty a short one.
    doc.frame.gap_rel = 0.05;
    doc.validate().expect("a 5% gap fits a 90%-tall slot");

    // A zero-height cell has nothing left whatever the gap is: the frame's own
    // slots are what make that a refusal rather than a division by zero.
    let mut doc = two_slot_doc();
    let mut flat = two_slot_template();
    flat.slots[0].outline = Polygon::rect(0.05, 0.5, 0.5, 0.5);
    doc.template = flat;
    doc.frame.gap_rel = 0.001;
    assert!(doc.validate().is_err());
}

#[test]
fn polygon_geometry() {
    let square = Polygon::rect(0.25, 0.25, 0.75, 0.75);
    close(square.area(), 0.25);
    assert_eq!(square.bbox().width(), 0.5);
    assert_eq!(square.bbox().center(), Point::new(0.5, 0.5));
    assert!(square.contains(Point::new(0.5, 0.5)));
    assert!(!square.contains(Point::new(0.24, 0.5)));
    assert!(!square.contains(Point::new(0.5, 0.76)));
    close(square.distance_to_boundary(Point::new(0.5, 0.5)), 0.25);
    close(square.distance_to_boundary(Point::new(0.1, 0.5)), 0.15);

    // Concave outline: the notch must not be contained.
    let l_shape = Polygon {
        points: vec![
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.0),
            Point::new(0.5, 0.5),
            Point::new(1.0, 0.5),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ],
    };
    close(l_shape.area(), 0.75);
    assert!(l_shape.contains(Point::new(0.25, 0.25)));
    assert!(!l_shape.contains(Point::new(0.75, 0.25)));

    assert!(Polygon { points: vec![] }.validate().is_err());
    assert!(
        Polygon {
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(2.0, 0.5)
            ]
        }
        .validate()
        .is_err()
    );
}

#[test]
fn shared_edges_are_found_and_bounded() {
    fn close_point(got: Point, want: Point) {
        close(got.x, want.x);
        close(got.y, want.y);
    }

    let template = two_slot_template();
    let seams = template.shared_edges();
    assert_eq!(seams.len(), 1);
    assert_eq!(seams[0].a, 0);
    assert_eq!(seams[0].b, 1);
    close_point(seams[0].from, Point::new(0.5, 0.05));
    close_point(seams[0].to, Point::new(0.5, 0.95));
    close(seams[0].length(), 0.9);

    // Slots that only touch at a corner share no edge.
    let mut apart = two_slot_template();
    apart.slots[1].outline = Polygon::rect(0.5, 0.96, 0.95, 1.0);
    apart.slots[1].area = 0.45 * 0.04;
    assert!(apart.shared_edges().is_empty());

    // Partially overlapping edges report only the shared stretch.
    let mut partial = two_slot_template();
    partial.slots[1].outline = Polygon::rect(0.5, 0.5, 0.95, 0.95);
    partial.slots[1].area = 0.45 * 0.45;
    let seams = partial.shared_edges();
    assert_eq!(seams.len(), 1);
    close_point(seams[0].from, Point::new(0.5, 0.5));
    close_point(seams[0].to, Point::new(0.5, 0.95));
}

#[test]
fn project_resolves_sources_against_its_own_directory() {
    let dir = temp_dir("sources");
    let project_path = dir.join("album.pixlay");
    let photo = dir.join("photos/one.png");
    std::fs::create_dir_all(photo.parent().unwrap()).expect("create photos dir");
    std::fs::write(&photo, b"not decoded in S1").expect("write photo placeholder");

    let mut doc = two_slot_doc();
    doc.cells[0].source = Some(PathBuf::from("photos/one.png"));
    std::fs::write(&project_path, doc.to_json().expect("serializes")).expect("write project");

    let project = Project::load(&project_path).expect("loads");
    assert_eq!(project.dir(), dir);
    assert_eq!(project.doc(), &doc);
    let sources = project.sources().expect("resolves");
    assert_eq!(sources[0].as_deref(), Some(photo.as_path()));
    assert_eq!(sources[1], None);

    doc.cells[0].source = Some(PathBuf::from("photos/gone.png"));
    std::fs::write(&project_path, doc.to_json().expect("serializes")).expect("write project");
    let project = Project::load(&project_path).expect("loads");
    let err = project.sources().expect_err("missing source must fail");
    assert_eq!(
        err.to_string(),
        format!(
            "source image does not exist: {}",
            dir.join("photos/gone.png").display()
        )
    );

    let err = Project::load(&dir.join("absent.pixlay")).expect_err("missing project file");
    assert!(err.to_string().contains("absent.pixlay"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn template_limits() {
    let mut doc = two_slot_doc();
    doc.template.name = "  ".to_string();
    assert!(doc.validate().is_err());

    let mut doc = two_slot_doc();
    doc.template.version = 0;
    assert!(doc.validate().is_err());

    let mut doc = two_slot_doc();
    doc.template.aspect = 0.05;
    assert!(doc.validate().is_err());
}
