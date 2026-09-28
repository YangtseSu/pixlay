// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S6.5 at the pixel boundary: the hit test against what the renderer actually
//! painted.
//!
//! The core sweep (`pixlay-core/tests/hit.rs`) measures the hit test against an
//! independent containment algorithm. This file measures it against something
//! better: the renderer's own output. Every slot gets one flat colour, and a pixel
//! that comes out of `draw` as *exactly* a slot's colour is a pixel the renderer
//! clipped into that slot — so the hit test's answer for that pixel's centre has to
//! be that slot. Blended pixels are dropped by construction rather than by a guard
//! distance: an antialiased pixel on a slot boundary is a mix and therefore equals
//! no slot's colour exactly.
//!
//! Two templates, because they are the two shapes the answer takes: the smoke
//! template, whose eight slots tile the canvas (so every pixel of the sheet, apart
//! from the antialiased edges, is somebody's), and the gutter template, whose four
//! slots leave a white cross. And two framings, because a slot's hit region is its
//! geometry and must not follow the photo: a rotated photo still fills its whole
//! slot, so the answer may not change.
//!
//! **The frame is the same claim seen from the other side** (S15g, the ruling of
//! 2026-09-24): `draw` clips each cell to `outline ∩ rounded_rect(inset)`, so a
//! frame's gap and its rounded corners are pixels the *backdrop* owns — while the
//! hit test keeps answering with the slot's own outline, because the frame is
//! decoration and the hit region is geometry. The sweep below therefore runs with a
//! frame as well as without one, and `the_frames_backdrop_pixels_are_still_the_cells_geometry`
//! pins both halves of that sentence.

use pixlay_core::{CollageDoc, CropTransform, Frame, PixelSize, Point, Rgba8, templates};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

/// Long edge of the grid the hit sweep measures on, in pixels.
const LONG_EDGE: u32 = 454;

/// The templates the sweep runs over: a cut template and the one with a gutter.
const TEMPLATES: [&str; 2] = [templates::SMOKE_TEMPLATE, "grid-4-2x2g"];

/// The frame the framed half of the sweep and the backdrop test render with: a gap
/// wide enough to sample a pixel in, a radius wide enough to cut a corner, and a
/// colour no slot colour is.
const FRAME: Frame = Frame {
    gap_rel: 0.04,
    radius_rel: 0.08,
    color: Rgba8::rgb(20, 200, 120),
};

/// One flat colour per slot, far apart so a swap or a spill cannot be mistaken for
/// a blend.
const COLORS: [Rgba8; 8] = [
    Rgba8::rgb(200, 30, 40),
    Rgba8::rgb(30, 160, 60),
    Rgba8::rgb(40, 60, 220),
    Rgba8::rgb(230, 170, 20),
    Rgba8::rgb(150, 40, 190),
    Rgba8::rgb(20, 190, 190),
    Rgba8::rgb(240, 120, 60),
    Rgba8::rgb(10, 60, 120),
];

fn doc(name: &str, rotation_deg: f64) -> CollageDoc {
    framed_doc(name, rotation_deg, Frame::default())
}

fn framed_doc(name: &str, rotation_deg: f64, frame: Frame) -> CollageDoc {
    let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
    let mut doc = CollageDoc::new(template);
    doc.frame = frame;
    for cell in &mut doc.cells {
        cell.crop = CropTransform {
            rotation_deg,
            ..CropTransform::IDENTITY
        };
    }
    doc
}

fn canvas_px(doc: &CollageDoc) -> PixelSize {
    PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size")
}

/// One flat bitmap per slot, at the size the slot *displays* it: sized from the
/// fit, which is what S4's decoder does (the fit's zoom is the display size), so a
/// rotation gets the magnification it needs instead of leaving the slot's corners
/// to the white background.
fn images(doc: &CollageDoc, canvas: PixelSize) -> Images {
    let mut images = Images::new();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        let fit = doc
            .fitted_crop(index, canvas.aspect(), 1.0)
            .expect("the document fits its own cells");
        let bbox = slot.outline.bbox();
        let displayed = fit.transform.zoom * bbox.width() * f64::from(canvas.width);
        images.insert(
            index,
            Bitmap::filled(
                displayed.ceil() as i32 + 1,
                displayed.ceil() as i32 + 1,
                COLORS[index % COLORS.len()],
            )
            .expect("bitmap"),
        );
    }
    images
}

/// What one render says about the hit test.
struct Agreement {
    /// Pixels that came out exactly one of the slot colours.
    exact: u64,
    /// Ones whose hit test does not return the slot they were painted with.
    wrong: u64,
    /// A few failures, for the message.
    examples: Vec<String>,
    /// `exact` per slot, so "every slot was sampled" is a number too.
    per_slot: Vec<u64>,
}

/// Compares the hit test with the colours the renderer actually wrote.
fn disagreements(doc: &CollageDoc, image: &Rgb8Image) -> Agreement {
    let slots = doc.template.slots.len();
    let mut result = Agreement {
        exact: 0,
        wrong: 0,
        examples: Vec::new(),
        per_slot: vec![0; slots],
    };
    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image.pixel(x, y);
            let Some(slot) = COLORS[..slots]
                .iter()
                .position(|color| [color.r, color.g, color.b] == pixel)
            else {
                // A blend, the white background or the gutter: it says nothing
                // about ownership.
                continue;
            };
            result.exact += 1;
            result.per_slot[slot] += 1;
            // The pixel centre, which is the point the pixel belongs to.
            let point = Point::new(
                (f64::from(x) + 0.5) / f64::from(image.width),
                (f64::from(y) + 0.5) / f64::from(image.height),
            );
            let hit = doc.template.slot_at(point);
            if hit != Some(slot) {
                result.wrong += 1;
                if result.examples.len() < 6 {
                    result.examples.push(format!(
                        "({x},{y}) painted slot {slot}, hit test says {hit:?}"
                    ));
                }
            }
        }
    }
    result
}

#[test]
fn every_pixel_the_renderer_paints_with_a_slot_colour_hits_that_slot() {
    let mut total_exact = 0u64;
    let mut total_pixels = 0u64;
    for name in TEMPLATES {
        for rotation_deg in [0.0, 30.0] {
            for frame in [Frame::default(), FRAME] {
                let what = format!("{name} at {rotation_deg} degrees, frame {frame:?}");
                let doc = framed_doc(name, rotation_deg, frame);
                let canvas = canvas_px(&doc);
                let image =
                    render_rgb8(&doc, &images(&doc, canvas), canvas, 1.0, None).expect("renders");
                let agreement = disagreements(&doc, &image);
                assert!(
                    agreement.wrong == 0,
                    "{what}: {} painted pixels do not hit their own slot: {:?}",
                    agreement.wrong,
                    agreement.examples
                );
                // Every slot has to be painted and sampled, or the sweep is
                // checking fewer slots than the template has.
                assert!(
                    agreement.per_slot.iter().all(|&count| count > 0),
                    "{what}: a slot was never sampled ({:?})",
                    agreement.per_slot
                );
                let pixels = u64::from(image.width as u32) * u64::from(image.height as u32);
                assert!(
                    agreement.exact * 2 > pixels,
                    "{what}: only {} of {pixels} pixels are a slot colour, so the comparison is \
                     measuring mostly blends",
                    agreement.exact
                );
                total_exact += agreement.exact;
                total_pixels += pixels;
            }
        }
    }
    // The sweep is not allowed to be a handful of pixels: at a 454 px long edge the
    // two templates, the two framings and the two frames are half a million samples.
    assert!(
        total_exact > 100_000,
        "only {total_exact} of {total_pixels} pixels were compared"
    );
}

#[test]
fn the_gutter_belongs_to_no_slot_in_the_rendered_image() {
    // The converse direction, on the template where it can be seen: the gutter is
    // painted white, and no white pixel in it is claimed by a slot. The vertical
    // strip through the middle is the gutter, so its hit test answer is `None` for
    // every y.
    let doc = doc("grid-4-2x2g", 0.0);
    let canvas = canvas_px(&doc);
    let image = render_rgb8(&doc, &images(&doc, canvas), canvas, 1.0, None).expect("renders");
    let mut samples = 0u64;
    for y in 0..image.height {
        let point = Point::new(0.5, (f64::from(y) + 0.5) / f64::from(image.height));
        assert_eq!(
            doc.template.slot_at(point),
            None,
            "the vertical gutter is claimed at y={y}"
        );
        let x = image.width / 2;
        assert_eq!(
            image.pixel(x, y),
            [255, 255, 255],
            "the vertical gutter is not white at ({x},{y})"
        );
        samples += 1;
    }
    assert!(samples > 100, "only {samples} gutter samples");
}

/// The pixel and the hit test for the pixel's centre, in one place.
fn sample(doc: &CollageDoc, image: &Rgb8Image, x: i32, y: i32) -> ([u8; 3], Option<usize>) {
    let point = Point::new(
        (f64::from(x) + 0.5) / f64::from(image.width),
        (f64::from(y) + 0.5) / f64::from(image.height),
    );
    (image.pixel(x, y), doc.template.slot_at(point))
}

#[test]
fn the_frames_backdrop_pixels_are_still_the_cells_geometry() {
    // S15g's pixel-backed half of PIX-008, and the ruling of 2026-09-24: the frame
    // is *decoration*, so the pixels it takes back are the backdrop's while the hit
    // test keeps answering with the slot's own outline. Both halves are pinned
    // here, because either one silently following the other would be a contract
    // change — the hit region growing a gap it does not have, or the renderer
    // painting a cell into its own gap.
    //
    // `grid-4-2x2` is a 2x2 tiling of a square canvas, so slot 0's box is
    // `[0, 0.5] x [0, 0.5]` with the canvas's own corner at its top-left: the gap
    // band beside that corner and the rounded corner inside it are both easy to
    // point at.
    let doc = framed_doc("grid-4-2x2", 0.0, FRAME);
    let canvas = canvas_px(&doc);
    let image = render_rgb8(&doc, &images(&doc, canvas), canvas, 1.0, None).expect("renders");
    let [r, g, b] = [FRAME.color.r, FRAME.color.g, FRAME.color.b];
    let backdrop = [r, g, b];
    let slot_0 = [COLORS[0].r, COLORS[0].g, COLORS[0].b];
    let height = f64::from(canvas.height);

    // The gap: the frame leaves the sheet's own edge inset by the whole gap (S20),
    // so the band between the canvas's border and the cell's visible rectangle is
    // `gapRel` of the canvas height wide. Sample its middle.
    let (rect, radius_rel) = doc.frame.clip(&doc.template.slots[0], doc.template.aspect);
    let band = FRAME.gap_rel * height;
    let x = (band / 2.0) as i32;
    let y = (height / 4.0) as i32;
    let (pixel, hit) = sample(&doc, &image, x, y);
    assert_eq!(pixel, backdrop, "the gap at ({x},{y}) is not the backdrop");
    assert_eq!(
        hit,
        Some(0),
        "the gap at ({x},{y}) is not the geometry of the cell behind it"
    );

    // The rounded corner: the arc is centred `radius` in from the visible
    // rectangle's own corner, and the point 0.15 of the radius along the diagonal
    // from that corner is well outside the disc — cut away, and far enough from the
    // arc to be a clean pixel rather than a blend.
    let radius = radius_rel * height;
    let corner = ((rect.x0 * f64::from(canvas.width)) + 0.15 * radius) as i32;
    let (pixel, hit) = sample(&doc, &image, corner, corner);
    assert_eq!(
        pixel, backdrop,
        "the rounded corner at ({corner},{corner}) is not the backdrop"
    );
    assert_eq!(
        hit,
        Some(0),
        "the rounded corner at ({corner},{corner}) is not the geometry of the cell it cuts"
    );

    // And the agreement the two halves are the exception to: inside the cell, the
    // pixel is the slot's own colour and the hit test returns that slot.
    let (pixel, hit) = sample(&doc, &image, 100, 100);
    assert_eq!(pixel, slot_0, "the cell's own middle is not its colour");
    assert_eq!(hit, Some(0), "the cell's own middle does not hit the cell");
}
