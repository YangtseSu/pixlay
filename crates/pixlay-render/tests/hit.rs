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

use pixlay_core::{CanvasSpec, CollageDoc, CropTransform, PixelSize, Point, Rgba8, templates};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

const DPI: u32 = 96;

/// The templates the sweep runs over: a cut template and the one with a gutter.
const TEMPLATES: [&str; 2] = [templates::SMOKE_TEMPLATE, "grid-4-2x2g"];

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
    let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
    let mut doc = CollageDoc::new(CanvasSpec::with_ratio(template.aspect, 120.0), template);
    for cell in &mut doc.cells {
        cell.crop = CropTransform {
            rotation_deg,
            ..CropTransform::IDENTITY
        };
    }
    doc
}

fn canvas_px(doc: &CollageDoc) -> PixelSize {
    doc.canvas.pixel_size(DPI).expect("canvas size")
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
            let doc = doc(name, rotation_deg);
            let canvas = canvas_px(&doc);
            let image = render_rgb8(&doc, &images(&doc, canvas), DPI, 1.0, None).expect("renders");
            let agreement = disagreements(&doc, &image);
            assert!(
                agreement.wrong == 0,
                "{name} at {rotation_deg} degrees: {} painted pixels do not hit their own slot: {:?}",
                agreement.wrong,
                agreement.examples
            );
            // Every slot has to be painted and sampled, or the sweep is checking
            // fewer slots than the template has.
            assert!(
                agreement.per_slot.iter().all(|&count| count > 0),
                "{name} at {rotation_deg} degrees: a slot was never sampled ({:?})",
                agreement.per_slot
            );
            let pixels = u64::from(image.width as u32) * u64::from(image.height as u32);
            assert!(
                agreement.exact * 2 > pixels,
                "{name} at {rotation_deg} degrees: only {} of {pixels} pixels are a slot colour, \
                 so the comparison is measuring mostly blends",
                agreement.exact
            );
            total_exact += agreement.exact;
            total_pixels += pixels;
        }
    }
    // The sweep is not allowed to be a handful of pixels: at 120 mm and 96 dpi the
    // two templates and the two framings are a quarter of a million samples.
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
    let image = render_rgb8(&doc, &images(&doc, canvas), DPI, 1.0, None).expect("renders");
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
