//! The S1 contract, as tests: shapes, limits, versioning and the geometry the
//! probes and the renderer rely on.

use std::path::PathBuf;

use pixlay_core::{
    Anchor, CanvasSpec, Cell, CollageDoc, CropTransform, DOC_VERSION, Point, Polygon, Project,
    Rgba8, Template, TextLayer, TextMode, TextToken, scan_tokens,
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
        crop: CropTransform {
            zoom: 1.25,
            offset: (0.1, -0.2),
            rotation_deg: -12.5,
        },
    };
    doc.text.push(TextLayer {
        content: "trip {date} #{index}".to_string(),
        mode: TextMode::Free {
            position: Point::new(0.5, 0.9),
            anchor: Anchor::BottomCenter,
        },
        size_rel: 0.02,
        rotation_deg: 6.0,
        color: Rgba8::BLACK,
        source_slot: Some(0),
    });
    doc.text.push(TextLayer {
        content: "{filename}".to_string(),
        mode: TextMode::Tiled { step: (0.3, 0.15) },
        size_rel: 0.01,
        rotation_deg: 30.0,
        color: Rgba8 {
            r: 10,
            g: 20,
            b: 30,
            a: 128,
        },
        source_slot: None,
    });
    doc.text_fallback.date = "2026-09-20".to_string();
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
        "\"text\"",
        "\"textFallback\"",
    ] {
        assert!(json.contains(key), "{key} missing from {json}");
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
    for count in [0, 1, 11] {
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
            err.to_string().contains("slots; the limit is 2..=10"),
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
    // A4 at 300 dpi is the reference number in docs/STEPS.md.
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

#[test]
fn crop_transform_limits() {
    CropTransform::IDENTITY.validate().expect("identity");
    for crop in [
        CropTransform {
            rotation_deg: 45.1,
            ..CropTransform::IDENTITY
        },
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
    ] {
        assert!(crop.validate().is_err(), "{crop:?} must be rejected");
    }
    // The limits are inclusive: exactly at them is valid, a hair past is not.
    CropTransform {
        rotation_deg: pixlay_core::MAX_ROTATION_DEG,
        offset: (1.0, -1.0),
        ..CropTransform::IDENTITY
    }
    .validate()
    .expect("the limits themselves are allowed");
    CropTransform {
        rotation_deg: -pixlay_core::MAX_ROTATION_DEG - 0.001,
        ..CropTransform::IDENTITY
    }
    .validate()
    .expect_err("just past the rotation limit is rejected");

    // The clamp degradation threshold is part of the contract (docs/CONTRACT.md),
    // so a change to it must be a deliberate edit here too.
    const { assert!(pixlay_core::CLAMP_ZOOM_LIMIT == 1.5) };
    const { assert!(pixlay_core::MAX_CANVAS_PIXELS == 200_000_000) };
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
fn text_tokens_are_scanned_and_unknown_ones_rejected() {
    let uses = scan_tokens("{date} {filename} {index}").expect("known tokens");
    assert_eq!(
        uses.iter().map(|use_| use_.token).collect::<Vec<_>>(),
        vec![TextToken::Date, TextToken::Filename, TextToken::Index]
    );
    assert_eq!(
        &"{date} {filename} {index}"[uses[0].start..uses[0].end],
        "{date}"
    );
    // Not tokens: braces around non-alphabetic text, and an unclosed brace.
    assert!(scan_tokens("{2 of 3} and {").expect("literal").is_empty());
    assert_eq!(scan_tokens("{Date}").unwrap_err(), "Date");

    let layer = TextLayer {
        content: "{exposure}".to_string(),
        mode: TextMode::Free {
            position: Point::new(0.5, 0.5),
            anchor: Anchor::Center,
        },
        size_rel: 0.05,
        rotation_deg: 0.0,
        color: Rgba8::BLACK,
        source_slot: None,
    };
    let mut doc = two_slot_doc();
    doc.text.push(layer.clone());
    let err = doc.validate().expect_err("unknown token");
    assert!(err.to_string().contains("{exposure}"), "{err}");

    // Text sizes are normalized fractions of the canvas height, and 0 is not a
    // size.
    doc.text[0].content = "{date}".to_string();
    doc.validate().expect("known token");
    doc.text[0].size_rel = 0.0;
    assert!(doc.validate().is_err());
    doc.text[0].size_rel = 0.05;
    doc.text[0].source_slot = Some(7);
    assert!(doc.validate().is_err());
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
