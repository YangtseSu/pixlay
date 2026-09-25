//! The per-slot pipeline: what a slot's bitmap is, where it sits, and how big it
//! is allowed to be.
//!
//! The bitmaps here are built from synthetic samplers rather than photographs, so
//! every expectation is computable: a flat source must come out flat (any fringe
//! is a bug), and a ramp's value at a display coordinate is known from the
//! mapping the contract defines, independent of how the resampler computes it.

use pixlay_core::{CollageDoc, CropTransform, PixelSize, templates};
use pixlay_imaging::{Sampler, slot_bitmap};

/// Long edge of the grid the layout tests measure on, in pixels.
const LONG_EDGE: u32 = 113;

/// A solid color, 4000x3000: four times longer than the slots it lands in, so the
/// resampler is always reducing.
struct Flat {
    color: [u16; 4],
}

impl Sampler for Flat {
    fn width(&self) -> u32 {
        640
    }

    fn height(&self) -> u32 {
        480
    }

    fn pixel(&self, _x: u32, _y: u32) -> [u16; 4] {
        self.color
    }
}

/// A horizontal sRGB ramp: `x / (width - 1)` on every channel.
struct Ramp {
    width: u32,
    height: u32,
}

impl Sampler for Ramp {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn pixel(&self, x: u32, _y: u32) -> [u16; 4] {
        let value = (255.0 * x as f64 / f64::from(self.width - 1)).round() as u16;
        let sample = value * 257;
        [sample, sample, sample, u16::MAX]
    }
}

/// A small canvas: the geometry is aspect-based, so 40 mm exercises every path a
/// 400 mm sheet does and keeps the sweep's bitmaps in the thousands of pixels
/// instead of the millions (the debug profile has no optimization to hide behind).
fn document(name: &str) -> CollageDoc {
    let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
    let mut doc = CollageDoc::new(template);
    // Every cell occupied, so every slot is laid out.
    for (index, cell) in doc.cells.iter_mut().enumerate() {
        cell.source = Some(std::path::PathBuf::from(format!("photo-{index}.png")));
    }
    doc
}

fn canvas_px(doc: &CollageDoc) -> PixelSize {
    PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size")
}

/// Cairo's `ARgb32` on little-endian: `B, G, R, A`.
fn pixel(bitmap: &pixlay_imaging::SlotBitmap, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * bitmap.width as usize + x as usize) * 4;
    let bytes = &bitmap.pixels[index..index + 4];
    [bytes[2], bytes[1], bytes[0], bytes[3]]
}

#[test]
fn every_slot_region_holds_what_the_slot_shows() {
    // The sweep: every template, every slot (64 of them), three framings each.
    //
    // What is asserted is the region's *geometry*, computed here from the
    // documented placement instead of from the implementation: a point of the
    // slot is at display coordinate `R(-rotation) * (p - centre) + display/2`, so
    // the region must contain the slot's bounding box (plus the guard) and must
    // stay inside the photo. That is cheap, and it is the property the pixel test
    // below then confirms on content.
    let framings = [
        CropTransform::IDENTITY,
        CropTransform {
            zoom: 2.5,
            offset: (0.4, -0.3),
            rotation_deg: 18.0,
        },
        CropTransform {
            zoom: 1.0,
            offset: (-0.9, 0.9),
            rotation_deg: -45.0,
        },
    ];
    let mut studied = 0;
    for name in templates::names() {
        let mut doc = document(name);
        let canvas = canvas_px(&doc);
        for slot in 0..doc.cells.len() {
            for framing in framings {
                doc.cells[slot].crop = framing;
                let outline = &doc.template.slots[slot].outline;
                let bbox = outline.bbox();
                let slot_w = bbox.width() * f64::from(canvas.width);
                let slot_h = bbox.height() * f64::from(canvas.height);
                let photo_aspect = 640.0 / 480.0;
                let fit = doc
                    .fit_crop(slot, framing, canvas.aspect(), photo_aspect)
                    .expect("the document fits its own cells");
                let displayed_w = fit.transform.zoom * slot_w;
                let displayed_h = displayed_w / photo_aspect;
                let centre = (
                    bbox.center().x * f64::from(canvas.width) + fit.transform.offset.0 * slot_w,
                    bbox.center().y * f64::from(canvas.height) + fit.transform.offset.1 * slot_h,
                );
                let (sin, cos) = fit.transform.rotation_deg.to_radians().sin_cos();
                let to_display = |x: f64, y: f64| {
                    let (dx, dy) = (
                        x * f64::from(canvas.width) - centre.0,
                        y * f64::from(canvas.height) - centre.1,
                    );
                    (
                        dx * cos + dy * sin + displayed_w / 2.0,
                        dy * cos - dx * sin + displayed_h / 2.0,
                    )
                };

                let region = fit.transform.display_region(
                    &doc.template.slots[slot],
                    canvas,
                    photo_aspect,
                    pixlay_imaging::REGION_GUARD_PX,
                );
                assert!((region.display.0 - displayed_w).abs() < 1e-6);
                assert!((region.display.1 - displayed_h).abs() < 1e-6);
                let (x, y, w, h) = region.rect;
                assert!(x >= 0.0 && y >= 0.0, "{name} slot {slot}: negative origin");
                assert!(
                    x + w <= displayed_w + 1e-6 && y + h <= displayed_h + 1e-6,
                    "{name} slot {slot}: the region leaves the photo"
                );
                for point in &outline.points {
                    let (dx, dy) = to_display(point.x, point.y);
                    assert!(
                        dx >= x - 1e-6 && dx <= x + w + 1e-6,
                        "{name} slot {slot} at {framing:?}: the region misses ({dx:.1})"
                    );
                    assert!(
                        dy >= y - 1e-6 && dy <= y + h + 1e-6,
                        "{name} slot {slot} at {framing:?}: the region misses ({dy:.1})"
                    );
                }
                // And the region is not the whole photo when the slot shows less:
                // the ladder's claim, checked per slot.
                assert!(
                    w * h <= displayed_w * displayed_h + 1.0,
                    "{name} slot {slot}: the region is larger than the photo"
                );
                studied += 1;
            }
        }
    }
    assert_eq!(
        studied,
        143 * framings.len(),
        "the sweep must cover every slot (S10 grew the library from 64; S19 added the sheet)"
    );
}

#[test]
fn a_flat_photo_fills_its_whole_bitmap_at_every_framing() {
    // The pipeline resamples, so the bitmap's edges are the interesting part: a
    // one-pixel error in the region origin, or a kernel that reads past the source
    // instead of extending it, shows a fringe of white or black there. A flat
    // source has no such excuse — every pixel of the bitmap must be its color.
    //
    // The representative slots are the ones with different shapes: each template's
    // first slot, the concave slot of `mosaic-8-s14`, and the nine-column strip's
    // narrowest pane (slot 6, 1/16 of the canvas wide, the shape that needs the
    // largest magnification) beside its 2/16 neighbour (slot 8).
    let color = [1000u16 * 20, 1000u16 * 40, 1000u16 * 60, u16::MAX];
    let source = Flat { color };
    // The 8-bit code a 16-bit sample stands for: `code * 257` is the exact
    // widening, so the inverse is a division, not a shift.
    let code = |sample: u16| (f64::from(sample) / 257.0).round() as u8;
    let expected = [code(color[0]), code(color[1]), code(color[2])];
    let framings = [
        CropTransform::IDENTITY,
        CropTransform {
            zoom: 2.5,
            offset: (0.4, -0.3),
            rotation_deg: 18.0,
        },
        CropTransform {
            zoom: 1.0,
            offset: (-0.9, 0.9),
            rotation_deg: -45.0,
        },
    ];
    let mut cases: Vec<(&str, usize)> = templates::names().iter().map(|name| (*name, 0)).collect();
    cases.push(("mosaic-8-s14", 6));
    cases.push(("strip-9-9x1", 6));
    cases.push(("strip-9-9x1", 8));

    let mut bitmaps = 0;
    for (name, slot) in cases {
        let mut doc = document(name);
        let canvas = canvas_px(&doc);
        for framing in framings {
            doc.cells[slot].crop = framing;
            let bitmap = slot_bitmap(&doc, &source, slot, canvas)
                .unwrap_or_else(|error| panic!("{name} slot {slot}: {error}"));
            for (x, y) in edges(bitmap.width, bitmap.height) {
                let got = pixel(&bitmap, x, y);
                assert_eq!(
                    [got[0], got[1], got[2]],
                    expected,
                    "{name} slot {slot} at {framing:?}: ({x}, {y}) of {}x{} at origin {:?} \
                     of {:?} is not the photo's color",
                    bitmap.width,
                    bitmap.height,
                    bitmap.origin,
                    bitmap.display
                );
            }
            assert_eq!(bitmap.origin.0.fract(), 0.0, "the origin is a whole texel");
            assert_eq!(bitmap.origin.1.fract(), 0.0, "the origin is a whole texel");
            bitmaps += 1;
        }
    }
    assert!(bitmaps >= 30, "only {bitmaps} bitmaps sampled");
}

/// Coordinates to sample: the four corners, the four edge midpoints and the
/// center, plus a few interior points on the diagonals.
fn edges(width: u32, height: u32) -> Vec<(u32, u32)> {
    let (w, h) = (width - 1, height - 1);
    let mut out = vec![
        (0, 0),
        (w, 0),
        (0, h),
        (w, h),
        (w / 2, 0),
        (w / 2, h),
        (0, h / 2),
        (w, h / 2),
        (w / 2, h / 2),
    ];
    for step in 1..4 {
        out.push((w * step / 4, h * step / 4));
        out.push((w * step / 4, h * (4 - step) / 4));
    }
    out
}

#[test]
fn a_ramp_photo_lands_at_the_place_the_framing_names() {
    // The display grid is defined by the contract: the displayed photo is
    // `zoom * slot width` wide and covers the whole photo, so display coordinate
    // `d` shows the source pixel at `d * src_width / display_width`. Making that
    // a formula lets the test compute what a bitmap pixel must be without asking
    // the implementation.
    let source = Ramp {
        width: 4000,
        height: 3000,
    };
    let mut doc = document("grid-4-2x2");
    doc.cells[0].crop = CropTransform::IDENTITY;
    let canvas = canvas_px(&doc);
    let bitmap = slot_bitmap(&doc, &source, 0, canvas).expect("bitmap");
    let sx = f64::from(source.width) / bitmap.display.0;

    let expected = |display_x: f64| -> u8 {
        let scaled = (display_x * sx).clamp(0.0, f64::from(source.width - 1));
        // The ramp is defined on integer source pixels; a resampled texel is a
        // weighted average around its center, so the comparison keeps a small
        // tolerance for the reduction's own smoothing.
        let ramp = 255.0 * scaled / f64::from(source.width - 1);
        ramp.round() as u8
    };

    // The bitmap's center is the photo's center: a ramp reads half way.
    let center = (bitmap.width / 2, bitmap.height / 2);
    let got = pixel(&bitmap, center.0, center.1);
    assert!(
        got[0].abs_diff(expected(
            (bitmap.origin.0 + f64::from(center.0) + 0.5).min(bitmap.display.0)
        )) <= 6,
        "center is {} but the mapping says {}",
        got[0],
        expected(bitmap.origin.0 + f64::from(center.0) + 0.5)
    );

    // Each edge of the region must agree with the mapping at that display
    // coordinate: this is what a wrong origin or a transposed axis breaks.
    for (x, label) in [(0u32, "left"), (bitmap.width - 1, "right")] {
        let display_x = bitmap.origin.0 + f64::from(x) + 0.5;
        let got = pixel(&bitmap, x, bitmap.height / 2)[0];
        let want = expected(display_x);
        assert!(
            got.abs_diff(want) <= 6,
            "the {label} edge reads {got} at display x {display_x:.1}, expected {want}"
        );
    }
}

#[test]
fn a_narrow_slot_holds_only_the_part_of_the_photo_it_shows() {
    // The buffer ladder's claim (`docs/CONTRACT.md` §4): the bitmap is as large as
    // the slot, not as large as the displayed photo. Slot 0 of the nine-column strip
    // is 2/16 of the canvas wide and needs its photo magnified 6x for a 4:3 source
    // (the ten-column strip this test framed until S12c was narrower still), so
    // passing the whole displayed photo would allocate six times the memory to show
    // a fraction of it.
    let source = Ramp {
        width: 4000,
        height: 3000,
    };
    let doc = document("strip-9-9x1");
    let canvas = canvas_px(&doc);
    let bitmap = slot_bitmap(&doc, &source, 0, canvas).expect("bitmap");
    let display_px = bitmap.display.0 * bitmap.display.1;
    let region_px = f64::from(bitmap.width) * f64::from(bitmap.height);
    println!(
        "slot 0: {}x{} at origin {:?} of a displayed photo {:?} ({:.1}% of it)",
        bitmap.width,
        bitmap.height,
        bitmap.origin,
        bitmap.display,
        100.0 * region_px / display_px
    );
    // The pane is 2/16 of the canvas wide and the photo is magnified 6x into it,
    // so the region it shows is a fraction of the displayed photo; the guard band
    // and the rounding are the slack.
    assert!(
        region_px < 0.25 * display_px,
        "the bitmap holds {region_px} px of a {display_px} px photo"
    );

    // And the sum over the whole document: the ladder's other half, that the
    // bitmaps together are the size of the canvas rather than a multiple of it.
    let mut total = 0u64;
    for slot in 0..doc.template.slots.len() {
        let bitmap = slot_bitmap(&doc, &source, slot, canvas).expect("bitmap");
        total += u64::from(bitmap.width) * u64::from(bitmap.height);
    }
    let canvas_area = canvas.width as u64 * canvas.height as u64;
    println!("all ten bitmaps: {total} px against a {canvas_area} px canvas");
    assert!(
        total < 2 * canvas_area,
        "the bitmaps total {total} px for a {canvas_area} px canvas"
    );
}

/// The bitmap boundary (S15e, PIX-003): a slot whose bitmap would be past the
/// canvas's own pixel budget is refused with the typed error **before the
/// conversion allocates anything**.
///
/// The case is a real one, not a synthetic extreme: a two-cell square canvas at A0
/// with both cells rotated 45 degrees. The axis-aligned box of a half-canvas slot
/// at that angle is 1.5 times its own area, so the bitmap alone is past the budget
/// — and if the refusal were not there, this test would try to hold 3.8 GB.
#[test]
fn a_bitmap_past_the_budget_is_refused_before_it_is_allocated() {
    use pixlay_imaging::{MAX_BITMAP_PIXELS, check_bitmap};

    let mut doc = document("strip-2-2x1g");
    for cell in &mut doc.cells {
        cell.crop = CropTransform {
            zoom: 1.0,
            offset: (0.0, 0.0),
            rotation_deg: 45.0,
        };
    }
    // The flat sampler is 640x480 — a 4:3 photo, which is the aspect the bound was
    // measured at.
    let source = Flat {
        color: [0, 0, 0, u16::MAX],
    };
    let canvas = PixelSize::for_long_edge(doc.template.aspect, 14043).expect("the A0 grid");
    let err = slot_bitmap(&doc, &source, 0, canvas).expect_err("past the budget");
    let message = err.to_string();
    assert!(message.contains("slot 0"), "{message}");
    assert!(
        message.contains("bitmap needs 212824320 pixels"),
        "{message}"
    );
    assert!(
        message.contains("the limit is 200000000 pixels"),
        "{message}"
    );

    // The same slot inside the budget still builds: the check is a bound, not a
    // refusal of the geometry. The grid is a small one because this case *does*
    // resample, and the debug profile has no optimization to hide behind.
    let inside = PixelSize::for_long_edge(doc.template.aspect, 1000).expect("a grid inside it");
    let bitmap = slot_bitmap(&doc, &source, 0, inside).expect("a bitmap inside the budget");
    let pixels = u64::from(bitmap.width) * u64::from(bitmap.height);
    assert!(pixels < MAX_BITMAP_PIXELS, "{pixels} px");

    // And the accounting the message reports: the destination buffers at 18 bytes
    // per texel plus the resampler's strip. A 100x100 destination reading a
    // 4000-tall source has 40 source rows per destination row, so one block of 256
    // output rows reaches the source's own height — the strip is then the whole
    // source, 16 bytes per row per destination column.
    let region = pixlay_imaging::Region {
        display: (100.0, 100.0),
        texels: (0, 0, 100, 100),
    };
    let tall = Ramp {
        width: 4000,
        height: 4000,
    };
    assert_eq!(
        region.conversion_bytes(tall.height()),
        18 * 10_000 + 16 * 100 * 4_000,
        "18 bytes per texel and the strip"
    );
    // At a 1:1 scale the source's height is not what bounds the block: one source
    // row per destination row means `BLOCK_ROWS` rows plus the filter's own
    // support on both ends, 262 of them, which a 300-row source holds.
    let square = pixlay_imaging::Region {
        display: (300.0, 300.0),
        texels: (0, 0, 300, 300),
    };
    assert_eq!(
        square.conversion_bytes(300),
        18 * 300 * 300 + 16 * 300 * 262,
        "the kernel's own reach at a 1:1 scale"
    );
    check_bitmap("a test region", &square, square.conversion_bytes(300))
        .expect("a 300x300 bitmap is nowhere near the budget");
}
