//! The S1 contract, as tests: shapes, limits, versioning and the geometry the
//! probes and the renderer rely on.

use std::path::PathBuf;

use pixlay_core::{
    Cell, CollageDoc, CropTransform, DOC_VERSION, MAX_LONG_EDGE_PX, MAX_SLOTS, PixelSize, Point,
    Polygon, Project, Rgba8, Template, templates,
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
    CollageDoc::new(two_slot_template())
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
    // S28: the cells a layout change kept. They are stored like the placed ones —
    // photo and framing — and an angle outside `(-180, 180]` is wrapped on the way in
    // for either list, so `normalize` covers both.
    doc.kept = vec![
        Cell {
            source: Some(PathBuf::from("photos/kept.png")),
            crop: CropTransform {
                zoom: 1.1,
                offset: (0.0, 0.25),
                rotation_deg: -190.0,
            },
        },
        Cell::default(),
    ];
    doc.normalize();
    assert_eq!(
        doc.kept[0].crop.rotation_deg, 170.0,
        "a kept cell's rotation is wrapped like a placed one's"
    );
    doc.validate().expect("document is valid");

    let json = doc.to_json().expect("serializes");
    let back = CollageDoc::from_json(&json).expect("deserializes");
    assert_eq!(doc, back);
    assert_eq!(
        back.kept, doc.kept,
        "the kept cells round-trip whole, photo and framing"
    );
    // Fields, not just the round trip: a missing `deny_unknown_fields` or a
    // renamed field would still round-trip through this crate's own types.
    for key in [
        "\"docVersion\"",
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
        "\"kept\"",
    ] {
        assert!(json.contains(key), "{key} missing from {json}");
    }
    // And the key is **skipped while the list is empty** (S28): a project that never
    // shrank is byte-identical to one written before the field existed, which is what
    // lets a document that keeps nothing load in a build that has the field and a
    // document that keeps a cell fail loudly in one that does not.
    let plain = two_slot_doc().to_json().expect("serializes");
    assert!(!plain.contains("\"kept\""), "{plain}");
    // The shape after the S12c purity cut and S12d's pixels-only cut: what the
    // document does *not* carry is as much of the contract as what it does, and a
    // field that quietly came back would fail here rather than at a user's project
    // file.
    for gone in [
        "\"text\"",
        "\"textFallback\"",
        "\"filter\"",
        "\"grade\"",
        "\"canvas\"",
        "\"widthMm\"",
    ] {
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
fn a_project_from_the_removed_shape_is_refused_by_version_not_by_field() {
    // A version-2 project is the shape S12c wrote: `grade`, `filter`, `text` and
    // `textFallback` are gone from it, but it still carries the `canvas` field
    // S12d removed. `deny_unknown_fields` would name `canvas` while parsing,
    // which tells a user nothing about what to do; reading the version first is
    // what turns it into the actionable "rebuild the project with this version".
    let doc = two_slot_doc();
    let current = doc.to_json().expect("serializes");
    let older = current
        .replace(
            &format!("\"docVersion\": {DOC_VERSION}"),
            "\"docVersion\": 2",
        )
        .replacen(
            "\"template\":",
            "\"canvas\": { \"widthMm\": 120.0, \"heightMm\": 90.0 },\n  \"template\":",
            1,
        );
    let err = CollageDoc::from_json(&older).expect_err("a version-2 document is refused");
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
        .replace("\"template\":", "\"nonsense\": 1,\n  \"template\":");
    let err = CollageDoc::from_json(&json).expect_err("unknown field must be rejected");
    assert!(err.to_string().contains("nonsense"), "{err}");
}

#[test]
fn slot_count_outside_the_limits_is_rejected() {
    // 10 is in the list because it *was* legal: `strip-10-10x1` shipped ten slots
    // until S12c removed it together with the product's above-nine range. 1 left
    // the list in S19, when it became the floor (ruling 34).
    for count in [0, 10, 11] {
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
            err.to_string().contains("slots; the limit is 1..=9"),
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
fn a_document_may_not_hold_more_cells_than_the_ceiling() {
    // S28's one new bound: `cells + kept` is the document's own cell total — on the
    // sheet and off it — and `MAX_SLOTS` bounds it. Nine cells' worth is legal
    // (nothing kept is the ordinary case, and the ceiling is the format's own slot
    // limit); a tenth cell anywhere is not, however it got there.
    let mut doc = two_slot_doc();
    doc.kept = vec![Cell::default(); MAX_SLOTS - doc.cells.len()];
    doc.validate()
        .expect("nine cells' worth is inside the ceiling");
    doc.kept.push(Cell::default());
    let err = doc.validate().expect_err("a tenth cell is past it");
    assert_eq!(
        err.to_string(),
        "document has 2 cells and keeps 8; a collage takes at most 9 cells"
    );
    // The bound is a *document* limit like the cell/slot match: nine placed cells and
    // nothing kept is the boundary itself, and one kept cell on top of it is past it.
    let mut nine = CollageDoc::new(templates::get("strip-9-9x1").expect("registered"));
    nine.validate().expect("nine placed cells are legal");
    nine.kept.push(Cell::default());
    assert!(nine.validate().is_err(), "a tenth cell is not");
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

/// The global topology is the **loader's** rule, not only the library test's
/// (S15g, PIX-007): a `.pixlay` embeds its own geometry, so a file may carry slots
/// this build never ships. The three refusals are the ones the ruling names, each
/// with the reason and a witness a caller can act on.
#[test]
fn a_templates_own_topology_is_refused_at_load() {
    let band = |x0: f64, y0: f64, x1: f64, y1: f64| {
        let outline = Polygon::rect(x0, y0, x1, y1);
        pixlay_core::Slot {
            area: outline.area(),
            outline,
        }
    };

    // Overlap: the second slot moved onto the first. The two areas still sum to
    // 0.81 of the canvas, so it is the overlap that refuses it.
    let mut overlap = two_slot_doc();
    overlap.template.slots[1] = overlap.template.slots[0].clone();

    // A sealed region: four bands around a centre nothing covers.
    let mut hole = two_slot_doc();
    hole.template.slots = vec![
        band(0.0, 0.0, 1.0, 0.4),
        band(0.0, 0.6, 1.0, 1.0),
        band(0.0, 0.4, 0.4, 0.6),
        band(0.6, 0.4, 1.0, 0.6),
    ];
    hole.cells = vec![Cell::default(); 4];

    // Over the canvas between them: 0.6 + 0.6 is more than there is.
    let mut oversize = two_slot_doc();
    oversize.template.slots = vec![band(0.0, 0.0, 0.6, 1.0), band(0.4, 0.0, 1.0, 1.0)];

    // A slot that crosses itself: a bowtie, whose two halves both answer
    // `contains`, and whose area is positive — so it reaches the simplicity check
    // rather than the zero-area one.
    let bowtie_outline = Polygon {
        points: vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.5),
        ],
    };
    let mut bowtie = two_slot_doc();
    bowtie.template.slots[0] = pixlay_core::Slot {
        area: bowtie_outline.area(),
        outline: bowtie_outline,
    };

    for (what, doc, expected) in [
        ("overlap", &overlap, "template slots 0 and 1 overlap at ("),
        ("hole", &hole, "template slots leave an interior hole at ("),
        (
            "over the canvas",
            &oversize,
            "template slots declare 1.2 of the canvas",
        ),
        ("self-intersection", &bowtie, "outline crosses itself"),
    ] {
        let err = doc
            .validate()
            .err()
            .unwrap_or_else(|| panic!("{what}: must be refused"));
        assert!(err.to_string().contains(expected), "{what}: {err}");
    }
}

/// The one size parameter there is (S12d): the long edge is exact, the other
/// edge keeps the template's aspect, and the pixel budget is the binding limit.
#[test]
fn grid_for_long_edge() {
    // The rounding reference in docs/CONTRACT.md §2.
    let grid = PixelSize::for_long_edge(4.0 / 3.0, 4000).expect("a 4:3 grid");
    assert_eq!((grid.width, grid.height), (4000, 3000));
    assert_eq!(
        PixelSize::for_long_edge(3.0 / 4.0, 4000).unwrap().pixels(),
        12_000_000
    );
    // Half away from zero: 3 px over an aspect of 2 gives a short edge of 1.5.
    assert_eq!(
        PixelSize::for_long_edge(2.0, 3).unwrap(),
        PixelSize {
            width: 3,
            height: 2
        }
    );
    // A square stays exactly square.
    let square = PixelSize::for_long_edge(1.0, 1000).expect("a square grid");
    assert_eq!((square.width, square.height), (1000, 1000));
    assert_eq!(square.pixels(), 1_000_000);

    for edge in [0, MAX_LONG_EDGE_PX + 1] {
        let err = PixelSize::for_long_edge(1.0, edge).expect_err("the edge must be limited");
        assert!(err.to_string().contains("long edge"), "{edge}: {err}");
    }

    // The budget is what binds a square request: 30000² = 900 MP, past the 200 MP
    // budget even though the edge itself is inside `MAX_LONG_EDGE_PX`.
    let err = PixelSize::for_long_edge(1.0, 30000).expect_err("canvas pixel budget");
    assert!(err.to_string().contains("900000000 pixels"), "{err}");
}

/// The aspect is checked before anything is derived from it (S15e, PIX-027A):
/// `NaN`, zero, a negative and an infinity used to reach the rounding and come
/// back as a grid unrelated to the template — a one-pixel-by-N shape is what
/// `NaN.max(1.0)` and a negative's `round` produce.
#[test]
fn grid_aspect_boundaries() {
    use pixlay_core::{MAX_TEMPLATE_ASPECT, MIN_TEMPLATE_ASPECT};

    for aspect in [f64::NAN, 0.0, -1.0, -0.0, f64::INFINITY, f64::NEG_INFINITY] {
        let err = PixelSize::for_long_edge(aspect, 4000)
            .expect_err("an aspect no arithmetic can use must be refused");
        assert!(
            err.to_string().contains("template aspect ratio"),
            "{aspect}: {err}"
        );
    }
    // The domain is the template's own: the edges are inside it, and one step
    // outside is refused with the same bound `Template::validate` names.
    PixelSize::for_long_edge(MIN_TEMPLATE_ASPECT, 4000).expect("0.1 is a legal layout");
    PixelSize::for_long_edge(MAX_TEMPLATE_ASPECT, 4000).expect("10.0 is a legal layout");
    for aspect in [
        MIN_TEMPLATE_ASPECT - 1e-9,
        MAX_TEMPLATE_ASPECT + 1e-9,
        0.001,
        100.0,
    ] {
        let err = PixelSize::for_long_edge(aspect, 4000).expect_err("outside 0.1..=10.0");
        assert!(
            err.to_string().contains("must be in 0.1..=10"),
            "{aspect}: {err}"
        );
    }

    // A grid is refused by the budget wherever it is asked for, not only where it
    // is derived: this is the check `render --preview-px` needs, and it is the
    // same `CanvasTooLarge` the long-edge path raises.
    let scaled = PixelSize {
        width: 20000,
        height: 20000,
    };
    let err = scaled.validate().expect_err("400 MP");
    assert!(err.to_string().contains("400000000 pixels"), "{err}");
    PixelSize {
        width: 20000,
        height: 10000,
    }
    .validate()
    .expect("exactly the budget is inside it");
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

/// The live text and the code answer the same question the same way (S15i,
/// PIX-027).
///
/// The 2026-09-24 review found the opposite: a live paragraph two `docVersion`s
/// behind, a template count from before the purity cut, a `draw` signature with an
/// argument missing. Prose drifts silently — nothing compiles a sentence — so the
/// claims that have a constant behind them are checked here. **Only the live text
/// is checked**: a section whose heading names a date, or a step's own "landed"
/// record, is a statement about what was true then and stays as it is
/// (`AGENTS.md`, "Session and persistence discipline"). What this test can catch is
/// a constant that moved without its sentence; what it cannot catch is a sentence
/// whose subject was deleted, which is why the review's list was worked through by
/// hand.
#[test]
fn the_live_documents_carry_the_codes_numbers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |name: &str| {
        std::fs::read_to_string(root.join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
    };
    let contract = read("docs/CONTRACT.md");
    let agents = read("AGENTS.md");

    let templates = pixlay_core::templates::all();
    let slots: usize = templates.iter().map(|template| template.slots.len()).sum();
    let claims = [
        ("docs/CONTRACT.md", format!("\"docVersion\": {DOC_VERSION}")),
        ("docs/CONTRACT.md", format!("(currently **{DOC_VERSION}**)")),
        ("AGENTS.md", format!("`docVersion`-{DOC_VERSION} file")),
        (
            "docs/CONTRACT.md",
            format!("{} templates and {slots} slots", templates.len()),
        ),
        (
            "docs/CONTRACT.md",
            format!("slot count | 1..={}", pixlay_core::MAX_PHOTOS),
        ),
        ("docs/CONTRACT.md", format!("1..={MAX_LONG_EDGE_PX} px")),
        (
            "docs/CONTRACT.md",
            format!("`0 < zoom ≤ {}`", pixlay_core::MAX_ZOOM),
        ),
        (
            "docs/CONTRACT.md",
            // `{:.1}`: the document writes the bound as a fraction with a decimal
            // point (`1.0`), which `Display` alone would shorten to `1`.
            format!("`0 ≤ value ≤ {:.1}`", pixlay_core::MAX_FRAME_REL),
        ),
    ];
    let documents = [("docs/CONTRACT.md", &contract), ("AGENTS.md", &agents)];
    for (file, claim) in &claims {
        let text = documents
            .iter()
            .find(|(name, _)| name == file)
            .map(|(_, text)| *text)
            .expect("the file is in the table above");
        assert!(
            text.contains(claim.as_str()),
            "{file} no longer says {claim:?}: the constant moved and the sentence did not"
        );
    }
}
