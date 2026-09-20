//! The alpha rule, as a test: a bitmap with transparency composites onto the
//! opaque white base.
//!
//! `AGENTS.md` fixes this twice over — "合成到不透明白底", and preview and export
//! must be pixel-identical — so a bitmap that carries alpha has to reach the
//! encoder already composited over white, not as a translucent value that some
//! later step has to guess about. The other render tests use opaque bitmaps, so
//! this is the only place the arithmetic is exercised.

use pixlay_core::{CanvasSpec, CollageDoc, Polygon, Slot, Template};
use pixlay_render::{Bitmap, Images, render_rgb8};

fn single_slot_doc() -> CollageDoc {
    let outline = Polygon::rect(0.0, 0.0, 1.0, 1.0);
    let template = Template {
        name: "one".to_string(),
        version: 1,
        aspect: 1.0,
        slots: vec![Slot { area: 1.0, outline }],
    };
    let mut doc = CollageDoc::new(CanvasSpec::new(100.0, 100.0), template);
    doc.template.slots.truncate(1);
    doc.cells.truncate(1);
    doc
}

/// One bitmap of `[b, g, r, a]` for every pixel, in Cairo's premultiplied
/// `ARgb32` layout.
fn uniform_bitmap(size: i32, color: [u8; 4]) -> Bitmap {
    let mut data = vec![0u8; (size * size * 4) as usize];
    for pixel in data.chunks_mut(4) {
        pixel.copy_from_slice(&color);
    }
    Bitmap::from_argb32(size, size, data).expect("bitmap")
}

#[test]
fn a_translucent_bitmap_reaches_the_output_composited_over_white() {
    let images = |color| {
        let mut images = Images::new();
        images.insert(0, uniform_bitmap(8, color));
        images
    };

    // Black at 50% alpha, stored premultiplied (cairo multiplies the color by
    // alpha, so black stays 0 and only alpha carries the value): over an opaque
    // white base the result is mid grey. cairo's premultiply rounding puts it at
    // 127 rather than 128, which is why this compares against 128 within one
    // level instead of for equality.
    let image =
        render_rgb8(&single_slot_doc(), &images([0, 0, 0, 128]), 72, 1.0, None).expect("renders");
    let got = image.pixel(image.width / 2, image.height / 2);
    for channel in got {
        assert!(
            channel.abs_diff(128) <= 1,
            "black at 50% over white must be ~128, got {got:?}"
        );
    }

    // Fully transparent makes no mark at all: the white base shows through, so
    // the export stays opaque and the cell reads as empty.
    let image =
        render_rgb8(&single_slot_doc(), &images([0, 0, 0, 0]), 72, 1.0, None).expect("renders");
    assert_eq!(
        image.pixel(image.width / 2, image.height / 2),
        [255, 255, 255]
    );

    // Fully opaque is the color itself, unaltered: a premultiplied store of an
    // opaque pixel is the straight color.
    let image = render_rgb8(
        &single_slot_doc(),
        &images([40, 60, 220, 255]),
        72,
        1.0,
        None,
    )
    .expect("renders");
    assert_eq!(
        image.pixel(image.width / 2, image.height / 2),
        [220, 60, 40]
    );
}
