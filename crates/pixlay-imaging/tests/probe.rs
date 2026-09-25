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
//! expects, on a two-slot document.
//!
//! S4 moved the probe here from the CLI, with the CLI's `content.rs` deleted: the
//! palette and the flat bitmaps a probe needs are the probe's own business now
//! (`pixlay_imaging::probe::probe_bitmaps`), and real photos cannot be probed at
//! all — a legitimately white photo has a white interior, and a photo with a hard
//! edge beside a seam has no measurable blend.

use std::path::PathBuf;

use pixlay_core::{Cell, CollageDoc, Frame, PixelSize, Polygon, Slot, Template};
use pixlay_imaging::Rgb8View;
use pixlay_imaging::probe::{Side, palette, probe};

/// Long edge of the grid the probe tests run on, in pixels.
const LONG_EDGE: u32 = 709;

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
    let mut doc = CollageDoc::new(template);
    doc.cells[0] = Cell {
        source: Some(PathBuf::from("photo.png")),
        crop: Default::default(),
    };
    doc
}

/// A flat render of `doc`: white everywhere, and the left slot filled with the
/// color the probe expects when `paint_left` is set.
///
/// The fill covers the slot's own rectangle exactly — its first and last pixel
/// included — which is what the renderer paints at gap 0 (the clip has no inset to
/// apply), so the frame's gap measures zero here and the probe's own gap criterion
/// is satisfied rather than dodged.
fn flat(doc: &CollageDoc, paint_left: bool) -> (Vec<u8>, i32, i32) {
    let canvas = PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size");
    let color = palette(0);
    let mut data = vec![255u8; canvas.width as usize * canvas.height as usize * 3];
    if paint_left {
        for y in 0..canvas.height {
            for x in 0..canvas.width / 2 {
                let index = (y as usize * canvas.width as usize + x as usize) * 3;
                data[index] = color.r;
                data[index + 1] = color.g;
                data[index + 2] = color.b;
            }
        }
    }
    (data, canvas.width, canvas.height)
}

fn view(image: &(Vec<u8>, i32, i32)) -> Rgb8View<'_> {
    Rgb8View {
        width: image.1,
        height: image.2,
        data: &image.0,
    }
}

#[test]
fn the_interior_probe_fails_when_a_slot_is_not_covered() {
    let doc = doc();

    let painted = flat(&doc, true);
    let good = probe(&doc, &view(&painted));
    assert_eq!(good.interiors.len(), 1, "the filled cell must be sampled");
    assert!(
        good.ok(),
        "a correct flat render must pass: {:?}",
        good.failure()
    );

    // The same document, rendered with nothing drawn at all. This is what the
    // probe exists to catch, and S3's clamp is what keeps a real document from
    // producing it.
    let nothing = flat(&doc, false);
    let blank = probe(&doc, &view(&nothing));
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

/// A square sheet with two cells side by side, both occupied, under `frame`.
///
/// A square aspect makes the gap's fraction of the canvas height the same number in
/// both axes of the grid, so the rectangles below land on whole pixels and a stripe
/// between two of them measures exactly.
fn framed_doc(gap_rel: f64, radius_rel: f64) -> CollageDoc {
    let left = Polygon::rect(0.0, 0.0, 0.5, 1.0);
    let right = Polygon::rect(0.5, 0.0, 1.0, 1.0);
    let template = Template {
        name: "test-2-square".to_string(),
        version: 1,
        aspect: 1.0,
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
    let mut doc = CollageDoc::new(template);
    doc.frame = Frame {
        gap_rel,
        radius_rel,
        ..Frame::default()
    };
    for (index, cell) in doc.cells.iter_mut().enumerate() {
        cell.source = Some(PathBuf::from(format!("photo{index}.png")));
    }
    doc
}

/// Where a cell's photo goes, in pixels: the frame's visible rectangle, asked of
/// the same function the renderer clips to.
fn visible_px(doc: &CollageDoc, slot: usize, canvas: PixelSize) -> (i32, i32, i32, i32) {
    let (rect, _radius) = doc
        .frame
        .clip(&doc.template.slots[slot], doc.template.aspect);
    (
        (rect.x0 * f64::from(canvas.width)).round() as i32,
        (rect.x1 * f64::from(canvas.width)).round() as i32,
        (rect.y0 * f64::from(canvas.height)).round() as i32,
        (rect.y1 * f64::from(canvas.height)).round() as i32,
    )
}

/// A render of `doc` under the given rectangles: the backdrop everywhere, and each
/// cell's rectangle in its palette colour.
///
/// Hand-painted rather than rendered, because these tests are about the *ruler*: the
/// rectangles can be moved by one pixel to ask exactly what the ruler does when the
/// frame's number and the pixels disagree. Nothing here antialiases, so the stripes
/// measure to the pixel.
fn paint(canvas: PixelSize, rects: &[(i32, i32, i32, i32)]) -> (Vec<u8>, i32, i32) {
    let mut data = vec![255u8; canvas.width as usize * canvas.height as usize * 3];
    for (index, (x0, x1, y0, y1)) in rects.iter().enumerate() {
        let color = palette(index);
        for y in *y0..*y1 {
            for x in *x0..*x1 {
                let at = (y as usize * canvas.width as usize + x as usize) * 3;
                data[at] = color.r;
                data[at + 1] = color.g;
                data[at + 2] = color.b;
            }
        }
    }
    (data, canvas.width, canvas.height)
}

#[test]
fn the_gap_measures_the_documents_number_at_the_seam_and_at_the_border() {
    // S20's criterion as pixels: on a square sheet at a 400 px long edge a 4% gap is
    // 16 px, and the *same* 16 px shows between the two photos and between a photo
    // and the sheet's edge. Before S20 the border measured 8 px and the seam 16 — the
    // number was half a stripe at the border and a whole one at the seam.
    let doc = framed_doc(0.04, 0.0);
    let canvas = PixelSize::for_long_edge(doc.template.aspect, 400).expect("canvas size");
    let rects: Vec<(i32, i32, i32, i32)> =
        (0..2).map(|slot| visible_px(&doc, slot, canvas)).collect();
    // The frame's own rule: half the gap off each cell's sides, and the whole gap off
    // the sheet's — so each cell gives up 8 px of its own box and 16 px at the border.
    assert_eq!(rects[0], (16, 192, 16, 384));
    assert_eq!(rects[1], (208, 384, 16, 384));

    let image = paint(canvas, &rects);
    let report = probe(&doc, &view(&image));
    assert_eq!(report.gap.expected_px, 16.0);
    for seam in &report.seams {
        assert_eq!((seam.gap_min_px, seam.gap_max_px), (16, 16), "{seam:?}");
        // The stripe the geometry leaves here is the frame's own number, so the
        // measured deviation is nothing at all on whole-pixel rectangles.
        assert_eq!(seam.gap_dev_px, 0.0, "{seam:?}");
        assert!(seam.gap_is_ok(), "{seam:?}");
    }
    for border in &report.gap.borders {
        assert_eq!((border.min_px, border.max_px), (16, 16), "{border:?}");
        // One cell reaches each of the sheet's sides, two reach its top and bottom.
        let reaching = if matches!(border.side, Side::Top | Side::Bottom) {
            2
        } else {
            1
        };
        assert_eq!(border.samples, reaching, "{border:?}");
    }
    assert!(report.ok(), "{:?}", report.failure());
}

#[test]
fn a_stripe_that_is_not_the_documents_number_is_reported() {
    // The ruler's discriminating power, in both directions: a render that leaves half
    // the number at the border (pre-S20's rule, and the defect ruling 35 names), and
    // one that leaves twice the number between the photos (the whole gap taken off
    // every side of every cell). Each has to fail, and the failure has to say so.
    let doc = framed_doc(0.04, 0.0);
    let canvas = PixelSize::for_long_edge(doc.template.aspect, 400).expect("canvas size");

    // Half the number at the border, the seam right.
    let image = paint(canvas, &[(8, 192, 8, 392), (208, 392, 8, 392)]);
    let report = probe(&doc, &view(&image));
    assert!(!report.ok(), "a 8 px border against a 16 px number");
    assert_eq!(report.gap.borders[0].min_px, 8);
    assert_eq!(report.gap.borders[0].max_px, 8);
    for seam in &report.seams {
        assert_eq!(seam.gap_min_px, 16, "the seam is the number here");
    }
    let reason = report.failure().expect("a failure must explain itself");
    assert!(reason.contains("gap"), "{reason}");

    // Twice the number between the photos, the border right.
    let image = paint(canvas, &[(16, 184, 16, 384), (216, 384, 16, 384)]);
    let report = probe(&doc, &view(&image));
    assert!(!report.ok(), "a 32 px seam against a 16 px number");
    assert_eq!(report.seams[0].gap_min_px, 32);
    assert_eq!(report.seams[0].gap_max_px, 32);
    assert_eq!(
        report.seams[0].gap_dev_px, 16.0,
        "the seam is judged against the geometry, not against the number"
    );
    for border in &report.gap.borders {
        assert_eq!(border.max_px, 16, "the border is the number here");
    }
}

#[test]
fn a_single_slot_document_has_one_uniform_border() {
    // The one-photo collage (S19's `grid-1-1x1`): one cell covering the sheet, so
    // there is no seam at all and the frame is the whole of what is visible — the same
    // distance on all four sides, which is the border a one-photo document is framed
    // with.
    let outline = Polygon::rect(0.0, 0.0, 1.0, 1.0);
    let template = Template {
        name: "test-1".to_string(),
        version: 1,
        aspect: 4.0 / 3.0,
        slots: vec![Slot {
            area: outline.area(),
            outline,
        }],
    };
    let mut doc = CollageDoc::new(template);
    doc.frame = Frame {
        gap_rel: 0.04,
        ..Frame::default()
    };
    doc.cells[0].source = Some(PathBuf::from("photo.png"));
    // 400x300, so the gap is 12 px of the canvas height.
    let canvas = PixelSize::for_long_edge(doc.template.aspect, 400).expect("canvas size");
    let rect = visible_px(&doc, 0, canvas);
    assert_eq!(rect, (12, 388, 12, 288));

    let image = paint(canvas, &[rect]);
    let report = probe(&doc, &view(&image));
    assert_eq!(report.gap.expected_px, 12.0);
    for border in &report.gap.borders {
        assert_eq!((border.min_px, border.max_px), (12, 12), "{border:?}");
        assert_eq!(border.samples, 1, "{border:?}");
    }
    assert!(report.seams.is_empty(), "a one-cell layout has no seam");
    assert!(report.ok(), "{:?}", report.failure());
}
