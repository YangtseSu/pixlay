//! S5: what a text layer's content resolves to, and how big a tiled grid is.
//!
//! Both are document semantics, so they are checked here rather than through a
//! render: no font, no pixels, and a failure names the token or the step.

use pixlay_core::{
    Anchor, CanvasSpec, CollageDoc, Point, Polygon, Rgba8, Slot, Template, TextFallback, TextLayer,
    TextMode, TextValues, tiled_grid,
};

fn doc() -> CollageDoc {
    let outline = Polygon::rect(0.0, 0.0, 0.5, 1.0);
    let other = Polygon::rect(0.5, 0.0, 1.0, 1.0);
    CollageDoc::new(
        CanvasSpec::new(120.0, 90.0),
        Template {
            name: "test-2".to_string(),
            version: 1,
            aspect: 4.0 / 3.0,
            slots: vec![
                Slot {
                    area: outline.area(),
                    outline,
                },
                Slot {
                    area: other.area(),
                    outline: other,
                },
            ],
        },
    )
}

fn layer(content: &str, source_slot: Option<usize>) -> TextLayer {
    TextLayer {
        content: content.to_string(),
        mode: TextMode::Free {
            position: Point::new(0.5, 0.5),
            anchor: Anchor::Center,
        },
        size_rel: 0.05,
        rotation_deg: 0.0,
        color: Rgba8::BLACK,
        source_slot,
    }
}

const EXIF: &str = "2019:07:14 10:32:00";

#[test]
fn tokens_resolve_against_the_slot_the_layer_names() {
    let fallback = TextFallback {
        date: "2026-09-21".to_string(),
    };
    let both = layer("{date} {filename} #{index}", Some(1));
    let values = TextValues {
        date: Some(EXIF.to_string()),
        filename: Some("photos/dated.jpg".to_string()),
    };
    assert_eq!(
        both.resolve(&values, &fallback),
        format!("{EXIF} photos/dated.jpg #1")
    );

    // `{index}` is the slot index as the document numbers slots — `0` is the first
    // slot in template order, the same number `template.<i>` rows and `probe`
    // print — and it is the layer's own `source_slot`, not its position in `text`.
    let first = layer("{index}", Some(0));
    assert_eq!(first.resolve(&values, &fallback), "0");
    let mut doc = doc();
    doc.text.push(layer("{index}", Some(0)));
    doc.text.push(layer("{index}", Some(1)));
    assert_eq!(doc.text[1].resolve(&values, &fallback), "1");
}

#[test]
fn a_missing_date_falls_back_to_the_documents_own_string() {
    let fallback = TextFallback {
        date: "2026-09-21".to_string(),
    };
    // A photo whose file has no `DateTimeOriginal`.
    let silent = TextValues {
        date: None,
        filename: Some("square.png".to_string()),
    };
    assert_eq!(
        layer("{date}", Some(0)).resolve(&silent, &fallback),
        "2026-09-21"
    );
    // A layer that names no slot resolves against the document too, and has no
    // filename or index to substitute.
    let no_slot = layer("{date}{filename}{index}", None);
    assert_eq!(no_slot.resolve(&silent, &fallback), "2026-09-21");
    // An empty stored date leaves nothing behind either: a token never renders as
    // itself.
    assert_eq!(
        layer("{date}", None).resolve(&silent, &TextFallback::default()),
        ""
    );
}

#[test]
fn text_without_tokens_is_returned_as_it_is() {
    let plain = layer("Pixlay 2026", Some(0));
    assert_eq!(
        plain.resolve(&TextValues::default(), &TextFallback::default()),
        "Pixlay 2026"
    );
    // A non-token brace pair is literal text (`scan_tokens`); the literals on both
    // sides of a real token survive the substitution.
    let mixed = layer("a {2 of 3} {index} b", Some(1));
    assert_eq!(
        mixed.resolve(&TextValues::default(), &TextFallback::default()),
        "a {2 of 3} 1 b"
    );
}

#[test]
fn a_tiled_grid_starts_at_the_origin_and_covers_the_canvas() {
    // One anchor at the origin and one per step after it, the far edge included:
    // a rotated watermark reaches back over the canvas from the far edge, so the
    // anchor there is not wasted work.
    assert_eq!(tiled_grid((0.5, 0.5)), Some((3, 3)));
    assert_eq!(tiled_grid((1.0, 1.0)), Some((2, 2)));
    assert_eq!(tiled_grid((0.25, 0.75)), Some((5, 2)));
    // A step larger than the canvas is the origin alone: a sparse watermark is
    // legitimate, and there is deliberately no upper bound on the step.
    assert_eq!(tiled_grid((4.0, 4.0)), Some((1, 1)));

    // An invalid step has no grid at all — the same precondition `validate` reports
    // as `InvalidTiledStep`.
    for step in [
        (0.0, 0.5),
        (0.5, -1.0),
        (f64::NAN, 0.5),
        (f64::INFINITY, 0.5),
    ] {
        assert_eq!(tiled_grid(step), None, "{step:?}");
    }
}

#[test]
fn a_grid_too_dense_to_draw_is_refused_at_load_time() {
    let layer = |step: (f64, f64)| TextLayer {
        content: "wm".to_string(),
        mode: TextMode::Tiled { step },
        size_rel: 0.01,
        rotation_deg: 0.0,
        color: Rgba8::BLACK,
        source_slot: None,
    };

    // A 1/100 step is 101 x 101 = 10,201 tiles: over the cap, and refused when the
    // document loads rather than discovered at export time.
    assert_eq!(tiled_grid((0.01, 0.01)), None);
    let mut doc = doc();
    doc.text.push(layer((0.01, 0.01)));
    let error = doc.validate().expect_err("too many tiles");
    assert!(
        error.to_string().contains("tiles"),
        "the message must name what is wrong: {error}"
    );

    // The largest grid the cap admits: 100 x 100 = 10,000 tiles exactly.
    assert_eq!(tiled_grid((0.0101, 0.0101)), Some((100, 100)));
    doc.text[0] = layer((0.0101, 0.0101));
    doc.validate().expect("10,000 tiles is the cap");

    // A step far below it saturates instead of overflowing: the grid is refused,
    // and neither the product nor the cast wraps.
    doc.text[0] = layer((1e-12, 1e-12));
    assert!(doc.validate().is_err());
    assert_eq!(tiled_grid((1e-300, 1.0)), None);
}
