//! A bitmap that holds only part of the photo renders exactly like the whole one.
//!
//! S4's buffer ladder hands `draw` the part of the photo a slot can show rather
//! than the whole displayed photo (`pixlay_core::CropTransform::display_region`),
//! which is what keeps a strip template's memory bounded. That is only free if the
//! renderer is indifferent to it: the same displayed photo, cropped or not, must
//! produce the same pixels.
//!
//! The test builds both sides from one synthetic displayed photo, so the only
//! difference between the two renders is the crop and its origin.

use pixlay_core::{CanvasSpec, CollageDoc, CropTransform, PixelSize, Polygon, Slot, Template};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

const DPI: u32 = 96;

/// Three slots sharing seams, one of them not a rectangle, so a framing that
/// pushes the region around has somewhere to show.
const SLOTS: [(f64, f64, f64, f64); 3] = [
    (0.0, 0.0, 0.5, 1.0),
    (0.5, 0.0, 1.0, 0.5),
    (0.5, 0.5, 0.75, 1.0),
];

fn doc() -> CollageDoc {
    let slots = SLOTS
        .iter()
        .map(|&(x0, y0, x1, y1)| {
            let outline = Polygon::rect(x0, y0, x1, y1);
            Slot {
                area: outline.area(),
                outline,
            }
        })
        .collect();
    let template = Template {
        name: "region-3".to_string(),
        version: 1,
        aspect: 104.0 / 78.0,
        slots,
    };
    CollageDoc::new(CanvasSpec::new(104.0, 78.0), template)
}

/// The whole displayed photo, as a function of the texel's own coordinates: a
/// gradient plus a hard-edged square, so a wrong origin moves something visible.
fn displayed_photo(width: f64, height: f64) -> Vec<u8> {
    let (w, h) = (width.round() as i32, height.round() as i32);
    let mut data = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let u = f64::from(x) / f64::from(w);
            let v = f64::from(y) / f64::from(h);
            let red = (255.0 * u) as u8;
            let green = (255.0 * v) as u8;
            let square = u > 0.3 && u < 0.7 && v > 0.3 && v < 0.7;
            let blue = if square {
                250
            } else {
                (255.0 * (1.0 - u)) as u8
            };
            let index = (y as usize * w as usize + x as usize) * 4;
            // ARgb32 on little-endian: B, G, R, A, premultiplied (opaque here).
            data[index] = blue;
            data[index + 1] = green;
            data[index + 2] = red;
            data[index + 3] = 255;
        }
    }
    data
}

/// The sub-rectangle `(x0, y0, x1, y1)` of a photo of `w x h` texels.
fn crop(data: &[u8], w: i32, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<u8> {
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut out = vec![0u8; cw as usize * ch as usize * 4];
    for y in 0..ch {
        let from = ((y0 + y) as usize * w as usize + x0 as usize) * 4;
        let to = (y as usize * cw as usize) * 4;
        out[to..to + cw as usize * 4].copy_from_slice(&data[from..from + cw as usize * 4]);
    }
    out
}

fn canvas_px(doc: &CollageDoc) -> PixelSize {
    doc.canvas.pixel_size(DPI).expect("canvas size")
}

/// Both renders for one framing, plus whether the crop was a real one.
fn render_pair(
    doc: &CollageDoc,
    framing: CropTransform,
    slot: usize,
) -> (Rgb8Image, Rgb8Image, bool) {
    let canvas = canvas_px(doc);
    let photo_aspect = 4.0 / 3.0;
    let bbox = doc.template.slots[slot].outline.bbox();
    let fit = framing.fit(&doc.template.slots[slot], canvas.aspect(), photo_aspect);
    let region = fit.transform.display_region(
        &doc.template.slots[slot],
        canvas,
        photo_aspect,
        pixlay_imaging_guard(),
    );
    // The whole displayed photo as *texels*: the region's bounds are floored and
    // ceiled outward, so the full bitmap has to cover that same grid.
    // The whole displayed photo as *texels*: the region's bounds are floored and
    // ceiled outward, so the full bitmap has to cover that same grid, while the
    // displayed size it declares stays the fractional one.
    let (w, h) = (
        region.display.0.ceil() as i32,
        region.display.1.ceil() as i32,
    );
    let displayed = displayed_photo(f64::from(w), f64::from(h));
    let (x0, y0, cw, ch) = region.texels();

    // Both sides declare the *same* displayed size — the fractional one the
    // pipeline computes — so the blit is exactly 1:1 in both and the only
    // difference is the region: with this size `draw`'s scale is exactly 1.0, so a
    // sample lands on a texel center and the comparison is exact rather than
    // "within one interpolation level". A bitmap of the ceil'd size would be
    // magnified by 1.0001 and start interpolating.
    let mut full = Images::new();
    full.insert(
        slot,
        Bitmap::from_argb32_region(w, h, (0.0, 0.0), region.display, displayed.clone())
            .expect("bitmap"),
    );
    let mut cropped = Images::new();
    cropped.insert(
        slot,
        Bitmap::from_argb32_region(
            cw,
            ch,
            (f64::from(x0), f64::from(y0)),
            region.display,
            crop(&displayed, w, x0, y0, x0 + cw, y0 + ch),
        )
        .expect("bitmap"),
    );

    let whole = render_rgb8(doc, &full, DPI, 1.0, None).expect("draw");
    let part = render_rgb8(doc, &cropped, DPI, 1.0, None).expect("draw");
    let real_crop = cw < w || ch < h;
    let _ = bbox;
    (whole, part, real_crop)
}

/// The guard band the pipeline uses, so the region here is the one it produces.
///
/// The renderer must not depend on `pixlay-imaging` (the boundary runs the other
/// way: imaging produces bitmaps, the renderer draws them), so the number is
/// restated rather than imported, and this test fails loudly if the two drift.
fn pixlay_imaging_guard() -> f64 {
    // `pixlay_imaging::REGION_GUARD_PX`.
    3.0
}

#[test]
fn a_cropped_bitmap_renders_like_the_whole_one() {
    // Three unrotated framings and three rotated ones. Without rotation the blit
    // is a whole-texel translation and the comparison is *exact* — that is the
    // assertion that pins the origin arithmetic. Rotation makes cairo interpolate,
    // and then the region's integer origin changes the order two floats are added
    // in: a one-level difference on a few thousandths of the pixels is
    // round-off, while a wrong origin moves whole pixels.
    let framings: [(CropTransform, bool); 6] = [
        (CropTransform::IDENTITY, false),
        (
            CropTransform {
                zoom: 2.0,
                offset: (0.35, -0.25),
                rotation_deg: 0.0,
            },
            false,
        ),
        (
            CropTransform {
                zoom: 1.4,
                offset: (-0.6, 0.7),
                rotation_deg: 0.0,
            },
            false,
        ),
        (
            CropTransform {
                zoom: 2.0,
                offset: (0.35, -0.25),
                rotation_deg: 18.0,
            },
            true,
        ),
        (
            CropTransform {
                zoom: 1.0,
                offset: (-0.8, 0.8),
                rotation_deg: -45.0,
            },
            true,
        ),
        (
            CropTransform {
                zoom: 3.0,
                offset: (0.0, 0.5),
                rotation_deg: 33.0,
            },
            true,
        ),
    ];
    let mut cropped_cases = 0;
    for slot in 0..SLOTS.len() {
        for (framing, rotated) in framings {
            let mut doc = doc();
            doc.cells[slot].crop = framing;
            let (whole, part, real_crop) = render_pair(&doc, framing, slot);
            assert_eq!(
                (whole.width, whole.height),
                (part.width, part.height),
                "the two renders differ in size"
            );
            let pixels = (whole.width * whole.height) as f64;
            let differences: Vec<u8> = whole
                .data
                .iter()
                .zip(&part.data)
                .map(|(a, b)| a.abs_diff(*b))
                .collect();
            let differing = differences.iter().filter(|v| **v != 0).count() as f64 / 3.0;
            let worst = differences.iter().copied().max().unwrap_or(0);
            if rotated {
                assert!(
                    worst <= 2 && differing <= 0.001 * pixels,
                    "slot {slot} at {framing:?}: {differing} pixels differ (worst {worst}) — \
                     more than interpolation round-off"
                );
            } else {
                assert_eq!(
                    (differing, worst),
                    (0.0, 0),
                    "slot {slot} at {framing:?}: the crop changed the render"
                );
            }
            if real_crop {
                cropped_cases += 1;
            }
        }
    }
    // If every region had been the whole photo the test would prove nothing.
    assert!(
        cropped_cases >= 6,
        "only {cropped_cases} of the cases actually cropped the photo"
    );
}
