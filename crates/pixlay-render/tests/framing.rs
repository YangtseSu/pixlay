//! S3 at the pixel boundary: the framing a document stores is a *request*, and
//! what `draw` paints is its fit.
//!
//! The document may ask for anything the contract allows — a zoom below the one
//! that covers, a pan that would leave a sliver, a straightening that would need
//! unbounded magnification — and the render still has to cover every slot, spill
//! into no other slot, leave the canvas margin white and keep the canvas its own
//! size. That is what the sweep here measures, on two shipped templates: the one
//! `AGENTS.md`'s verification command uses (eight slots, one of them not a
//! rectangle, no margin at all) and the gutter template (four slots with a gap
//! that reaches the border, so "outside every slot" has somewhere to show).

use pixlay_core::{CanvasSpec, CollageDoc, CropTransform, PixelSize, Point, Rgba8, templates};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

const DPI: u32 = 96;

/// The templates the sweep runs over.
const TEMPLATES: [&str; 2] = [templates::SMOKE_TEMPLATE, "grid-4-2x2g"];

/// One flat color per slot. Flat content is what separates "covered by the right
/// slot" from "blended with a neighbour"; the colors are far apart so a swap or a
/// spill cannot be mistaken for either.
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

/// Distance a sample keeps from every slot boundary, in output pixels. Cairo
/// antialiases the clip edge, so a pixel on a boundary is a blend by construction
/// and says nothing about coverage.
const GUARD_PX: f64 = 3.0;

/// Sample stride, in output pixels: the sweep renders hundreds of images, and
/// every pixel of every render would measure the same sentence.
const STRIDE: i32 = 2;

/// What a sample pixel must show, decided by the template's geometry alone — the
/// same answer for every framing, which is why it is computed once.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    /// Well inside slot `n`: it must show that slot's color. A photo that spilled
    /// out of its own slot shows up here, in the neighbour's color.
    Inside(usize),
    /// Well outside every slot: it must be white, so a photo that overflowed its
    /// slot into the canvas margin shows up here.
    Outside,
}

fn doc(name: &str) -> CollageDoc {
    let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
    // A small canvas at the template's own aspect: the sweep renders hundreds of
    // times, and a canvas of another aspect is a hard error anyway.
    CollageDoc::new(CanvasSpec::with_ratio(template.aspect, 120.0), template)
}

fn canvas_px(doc: &CollageDoc) -> PixelSize {
    doc.canvas.pixel_size(DPI).expect("canvas size")
}

/// The slot's own aspect in the space `draw` places into.
fn slot_aspect(doc: &CollageDoc, index: usize, canvas_aspect: f64) -> f64 {
    let bbox = doc.template.slots[index].outline.bbox();
    bbox.width() * canvas_aspect / bbox.height()
}

/// Classifies a grid of sample pixels by the template's geometry.
fn classify(doc: &CollageDoc, canvas: PixelSize) -> Vec<(i32, i32, Class)> {
    let (width, height) = (f64::from(canvas.width), f64::from(canvas.height));
    let mut samples = Vec::new();
    for y in (1..canvas.height - 1).step_by(STRIDE as usize) {
        for x in (1..canvas.width - 1).step_by(STRIDE as usize) {
            let point = Point::new(f64::from(x) / width, f64::from(y) / height);
            // The guard is measured with the shorter canvas edge, so a sample
            // near a boundary is dropped rather than kept by a rounding choice.
            let depth = doc
                .template
                .slots
                .iter()
                .map(|slot| slot.outline.distance_to_boundary(point) * height)
                .fold(f64::INFINITY, f64::min);
            if depth <= GUARD_PX {
                continue;
            }
            let class = match doc
                .template
                .slots
                .iter()
                .position(|slot| slot.outline.contains(point))
            {
                Some(index) => Class::Inside(index),
                None => Class::Outside,
            };
            samples.push((x, y, class));
        }
    }
    samples
}

/// One bitmap per slot, at the size the slot *displays* it, with the aspect
/// `photo_aspect(index)` asks for.
///
/// The size comes from the **fit**, not from the stored request. A request below
/// the covering zoom gets raised by the clamp, and a bitmap sized for the request
/// would then be magnified by the canvas — one texel's transparent edge smearing
/// several pixels into the slot, which is a resampling artifact and not a coverage
/// question. Sizing from the fit is also what S4's decoder has to do (the fit's
/// zoom is the display size), so this file exercises the real shape. One pixel of
/// slack per axis keeps the bitmap from ever being the smaller one.
fn images(doc: &CollageDoc, canvas: PixelSize, photo_aspect: &dyn Fn(usize) -> f64) -> Images {
    let mut images = Images::new();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        let aspect = photo_aspect(index);
        let fit = doc
            .fitted_crop(index, canvas.aspect(), aspect)
            .expect("the document fits its own cells");
        let bbox = slot.outline.bbox();
        let displayed = fit.transform.zoom * bbox.width() * f64::from(canvas.width);
        let width = displayed.ceil() as i32 + 1;
        let height = (displayed / aspect).ceil() as i32 + 1;
        images.insert(
            index,
            Bitmap::filled(width, height, COLORS[index % COLORS.len()]).expect("bitmap"),
        );
    }
    images
}

/// Every sample shows what the geometry says.
fn check(image: &Rgb8Image, samples: &[(i32, i32, Class)], what: &str) {
    let mut wrong = Vec::new();
    for &(x, y, class) in samples {
        let expected = match class {
            Class::Inside(index) => {
                let color = COLORS[index % COLORS.len()];
                [color.r, color.g, color.b]
            }
            Class::Outside => [255, 255, 255],
        };
        let actual = image.pixel(x, y);
        if actual != expected && wrong.len() < 6 {
            wrong.push((x, y, class, actual, expected));
        }
    }
    assert!(
        wrong.is_empty(),
        "{what}: samples do not match the geometry ({:?})",
        wrong
    );
}

#[test]
fn every_framing_covers_its_slot_without_spilling_or_growing_the_canvas() {
    let (mut renders, mut covered, mut margin) = (0u64, 0u64, 0u64);
    for name in TEMPLATES {
        let mut doc = doc(name);
        let canvas = canvas_px(&doc);
        let samples = classify(&doc, canvas);
        assert!(
            samples.len() > 4_000,
            "{name}: the classifier found {} samples",
            samples.len()
        );
        covered += samples
            .iter()
            .filter(|(_, _, class)| matches!(class, Class::Inside(_)))
            .count() as u64;
        if name == "grid-4-2x2g" {
            margin += samples
                .iter()
                .filter(|(_, _, class)| *class == Class::Outside)
                .count() as u64;
        }

        for rotation_deg in [-45.0, -20.0, 0.0, 13.0, 45.0] {
            for offset in [(-1.0, 1.0), (-0.5, 0.3), (0.0, 0.0), (0.8, -0.6)] {
                for zoom in [0.5, 1.0, 2.0] {
                    for photo_aspect in [1.0, 1.75, 2.5] {
                        for cell in &mut doc.cells {
                            cell.crop = CropTransform {
                                zoom,
                                offset,
                                rotation_deg,
                            };
                        }
                        let images = images(&doc, canvas, &|_| photo_aspect);
                        let image = render_rgb8(&doc, &images, DPI, 1.0, None).expect("renders");
                        // Crop edges only, never grow the canvas: whatever the
                        // framing, the output is exactly the canvas's pixel size.
                        assert_eq!(
                            (image.width, image.height),
                            (canvas.width, canvas.height),
                            "{name}: zoom {zoom}, offset {offset:?}, rotation {rotation_deg}"
                        );
                        check(
                            &image,
                            &samples,
                            &format!(
                                "{name} zoom {zoom} offset {offset:?} rotation {rotation_deg} photo {photo_aspect}"
                            ),
                        );
                        renders += 1;
                    }
                }
            }
        }
    }
    assert_eq!(renders, 360, "the sweep must actually sweep");
    assert!(covered > 0, "no sample landed inside a slot");
    assert!(
        margin > 0,
        "the gutter template must have canvas margin to check"
    );
}

#[test]
fn a_rotation_the_document_asks_for_is_clamped_into_coverage() {
    // The document asks for zoom 1 and 13 degrees, which does not cover: a photo
    // rotated 13 degrees inside its slot leaves the corners uncovered (a matched
    // photo needs about 1.26x there). The render covers anyway, so the clamp was
    // recomputed for the angle the document now asks for.
    let mut doc = doc(templates::SMOKE_TEMPLATE);
    let canvas = canvas_px(&doc);
    let canvas_aspect = canvas.aspect();
    let samples = classify(&doc, canvas);
    for index in 0..doc.cells.len() {
        let aspect = slot_aspect(&doc, index, canvas_aspect);
        doc.cells[index].crop = CropTransform {
            zoom: 1.0,
            offset: (0.0, 0.0),
            rotation_deg: 13.0,
        };
        let fit = doc
            .fitted_crop(index, canvas_aspect, aspect)
            .expect("the document fits its own cells");
        assert!(
            fit.transform.zoom > 1.0,
            "slot {index}: the request alone already covers, so this proves nothing"
        );
    }
    // Matched photos, so the only thing the fit has to pay for is the rotation.
    let images = images(&doc, canvas, &|index| {
        slot_aspect(&doc, index, canvas_aspect)
    });
    let image = render_rgb8(&doc, &images, DPI, 1.0, None).expect("renders");
    check(&image, &samples, "zoom 1, rotation 13, matched photos");
}

#[test]
fn a_document_that_is_already_fitted_renders_identically() {
    // What gets drawn is the fit, exactly: taking the requests out of the
    // document and putting their fits in changes nothing, pixel for pixel.
    let canvas = canvas_px(&doc(templates::SMOKE_TEMPLATE));
    let canvas_aspect = canvas.aspect();
    let mut requested = doc(templates::SMOKE_TEMPLATE);
    for (index, cell) in requested.cells.iter_mut().enumerate() {
        cell.crop = CropTransform {
            zoom: 0.4 + index as f64 * 0.3,
            offset: (0.9, -0.7),
            rotation_deg: 30.0 - index as f64 * 6.0,
        };
    }
    let images = images(&requested, canvas, &|_| 1.4);

    // The pre-fit has to use the aspect `draw` will use, which is the bitmap's
    // own (its integer dimensions are not exactly the requested 1.4).
    let mut fitted = requested.clone();
    for index in 0..fitted.cells.len() {
        let aspect = images.get(index).expect("bitmap").aspect();
        let crop = requested.cells[index].crop;
        fitted.cells[index].crop = requested
            .fit_crop(index, crop, canvas_aspect, aspect)
            .expect("the document fits its own cells")
            .transform;
    }

    let raw = render_rgb8(&requested, &images, DPI, 1.0, None).expect("renders");
    let pre = render_rgb8(&fitted, &images, DPI, 1.0, None).expect("renders");
    assert_eq!(
        (raw.width, raw.height),
        (pre.width, pre.height),
        "the two renders must be the same size"
    );
    assert_eq!(
        raw.data, pre.data,
        "pre-fitting the document changed the pixels"
    );
}
