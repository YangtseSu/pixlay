//! The probes' own discriminating power.
//!
//! S1 requires that a criterion which cannot pass makes the test go red, and S3
//! moved coverage into `draw`: the clamp now raises any request that could not
//! cover its slot, so no *document* reaches the probe with white inside a slot any
//! more (the CLI-level consequence is pinned in `tests/cli.rs`). The probe's
//! interior criterion still has to be shown to fail against an image that is wrong
//! by construction — a probe that cannot fail its own criterion measures nothing.
//!
//! The images here are built by hand, pixel by pixel, so nothing in this file
//! depends on the renderer being right: flat content in the color the probe
//! expects, on the same two-slot document `tests/cli.rs` uses.

use std::path::PathBuf;

use pixlay_cli::content;
use pixlay_cli::probe::probe;
use pixlay_core::{CanvasSpec, Cell, CollageDoc, Polygon, Slot, Template};
use pixlay_render::Rgb8Image;

const DPI: u32 = 150;

/// A two-slot document with the left cell occupied and the right one empty: the
/// shape every probe question is about.
fn doc() -> CollageDoc {
    let left = Polygon::rect(0.0, 0.0, 0.5, 1.0);
    let right = Polygon::rect(0.5, 0.0, 1.0, 1.0);
    let template = Template {
        name: "test-2".to_string(),
        version: 1,
        aspect: 4.0 / 3.0,
        slots: vec![
            Slot {
                area: left.area(),
                outline: left,
            },
            Slot {
                area: right.area(),
                outline: right,
            },
        ],
    };
    let mut doc = CollageDoc::new(CanvasSpec::new(120.0, 90.0), template);
    doc.cells[0] = Cell {
        source: Some(PathBuf::from("photo.png")),
        crop: Default::default(),
    };
    doc
}

/// A flat render of `doc`: white everywhere, and the left slot filled with the
/// color the probe expects when `paint_left` is set. The fill stops two pixels
/// short of the slot's boundary, so the seam between the two slots is a hard edge
/// with nothing blended into it — the way a correct render leaves it.
fn flat(doc: &CollageDoc, paint_left: bool) -> Rgb8Image {
    let canvas = doc.canvas.pixel_size(DPI).expect("canvas size");
    let color = content::color(0);
    let mut data = vec![255u8; canvas.width as usize * canvas.height as usize * 3];
    if paint_left {
        for y in 2..canvas.height - 2 {
            for x in 2..canvas.width / 2 - 2 {
                let index = (y as usize * canvas.width as usize + x as usize) * 3;
                data[index] = color.r;
                data[index + 1] = color.g;
                data[index + 2] = color.b;
            }
        }
    }
    Rgb8Image {
        width: canvas.width,
        height: canvas.height,
        data,
    }
}

#[test]
fn the_interior_probe_fails_when_a_slot_is_not_covered() {
    let doc = doc();

    let good = probe(&doc, &flat(&doc, true), DPI);
    assert_eq!(good.interiors.len(), 1, "the filled cell must be sampled");
    assert!(
        good.ok(),
        "a correct flat render must pass: {:?}",
        good.failure()
    );

    // The same document, rendered with nothing drawn at all. This is what the
    // probe exists to catch, and S3's clamp is what keeps a real document from
    // producing it.
    let blank = probe(&doc, &flat(&doc, false), DPI);
    assert!(!blank.ok(), "an empty slot must not pass");
    assert!(!blank.interiors[0].matches());
    assert_eq!(blank.interiors[0].actual, [255, 255, 255]);
    assert!(
        blank.background.is_clean(),
        "the failure is the interior sample, not the background"
    );
    assert!(
        blank.seams.iter().all(|seam| seam.is_clean()),
        "the failure is the interior sample, not a seam"
    );
    let reason = blank.failure().expect("a failure must explain itself");
    assert!(reason.contains("0 of 1 slot samples matched"), "{reason}");
}
