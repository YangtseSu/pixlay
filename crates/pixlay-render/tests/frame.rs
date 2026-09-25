//! S11: the frame, as pixels.
//!
//! `AGENTS.md`: "looks right" is not a criterion. The frame's three fields each
//! change a *countable* thing about the render — the width of the stripe between
//! two cells, the number of backdrop pixels a rounded corner uncovers, the colour
//! every one of those pixels carries — so each of them is measured here rather
//! than looked at:
//!
//! * the gap's stripe is the requested width in pixels;
//! * a rounded corner's backdrop pixels are more than zero and monotonically more
//!   as the radius grows;
//! * a coloured backdrop is *exactly* that colour everywhere the photos do not
//!   reach, and the identity frame leaves the cells untouched (the byte-identity
//!   half is `tests/render.rs`'s committed golden image, which a frame with no gap
//!   and no radius must still match at RMSE 0).
//!
//! Everything here is procedural: flat bitmaps, no decoder, no display.

use pixlay_core::{CollageDoc, Frame, PixelSize, Polygon, Rgba8, Slot, Template};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

/// The pixel grid the probes measure on: a 4:3 grid with a 1417 px long edge,
/// so a 4% gap is 42 px — wide enough that a one-pixel antialiased edge at each
/// side of it is unambiguous.
const LONG_EDGE: u32 = 1417;

const LEFT: [u8; 3] = [200, 30, 40];
const RIGHT: [u8; 3] = [30, 160, 60];

/// Two half-canvas cells side by side, sharing a full-height seam: the simplest
/// shape whose gap is one stripe to measure.
fn doc(frame: Frame) -> CollageDoc {
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
    doc.frame = frame;
    doc
}

/// Flat bitmaps, one per cell, at the size the canvas displays them.
fn images(doc: &CollageDoc, canvas: pixlay_core::PixelSize) -> Images {
    let mut images = Images::new();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        let bbox = slot.outline.bbox();
        let width = (bbox.width() * f64::from(canvas.width)).ceil() as i32;
        let height = (bbox.height() * f64::from(canvas.height)).ceil() as i32;
        let color = if index == 0 { LEFT } else { RIGHT };
        images.insert(
            index,
            Bitmap::filled(width, height, Rgba8::rgb(color[0], color[1], color[2]))
                .expect("a flat bitmap"),
        );
    }
    images
}

fn render(frame: Frame) -> (Rgb8Image, pixlay_core::PixelSize) {
    let doc = doc(frame);
    doc.validate().expect("the frame is a legal one");
    let canvas = PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size");
    let image = render_rgb8(&doc, &images(&doc, canvas), canvas, 1.0, None).expect("renders");
    assert_eq!((image.width, image.height), (canvas.width, canvas.height));
    (image, canvas)
}

fn rgb(color: Rgba8) -> [u8; 3] {
    [color.r, color.g, color.b]
}

/// The runs of pixels of exactly `color` in row `y`, as `(x0, x1)` inclusive.
fn runs(image: &Rgb8Image, y: i32, color: [u8; 3]) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut start = None;
    for x in 0..image.width {
        let matches = image.pixel(x, y) == color;
        match (matches, start) {
            (true, None) => start = Some(x),
            (false, Some(from)) => {
                runs.push((from, x - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        runs.push((from, image.width - 1));
    }
    runs
}

/// The widest run of `color` in row `y`.
fn widest_run(image: &Rgb8Image, y: i32, color: [u8; 3]) -> i32 {
    runs(image, y, color)
        .into_iter()
        .map(|(from, to)| to - from + 1)
        .max()
        .unwrap_or(0)
}

/// The runs of pixels of exactly `color` in column `x`, as `(y0, y1)` inclusive.
fn column_runs(image: &Rgb8Image, x: i32, color: [u8; 3]) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut start = None;
    for y in 0..image.height {
        let matches = image.pixel(x, y) == color;
        match (matches, start) {
            (true, None) => start = Some(y),
            (false, Some(from)) => {
                runs.push((from, y - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        runs.push((from, image.height - 1));
    }
    runs
}

/// How many pixels of exactly `color` fall inside `(x0, y0)..(x1, y1)` (exclusive),
/// skipping a 2 px band at the border so the clip's own antialiasing is not counted.
fn count_inside(image: &Rgb8Image, rect: (i32, i32, i32, i32), color: [u8; 3], band: i32) -> u64 {
    let (x0, y0, x1, y1) = rect;
    let mut count = 0;
    for y in (y0 + band)..(y1 - band) {
        for x in (x0 + band)..(x1 - band) {
            if image.pixel(x, y) == color {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn the_identity_frame_leaves_the_cells_untouched() {
    // No gap, no radius: every pixel inside a cell is that cell's colour, and the
    // only backdrop is what no cell covers. This is the same claim the committed
    // golden image makes against the pre-S11 build; here it is the *frame* that is
    // being pinned, on a layout whose seam the frame would move.
    let (image, canvas) = render(Frame::default());
    let (w, h) = (canvas.width, canvas.height);
    let middle = h / 2;

    assert_eq!(
        runs(&image, middle, [255, 255, 255]),
        vec![],
        "no backdrop at all"
    );
    // The seam itself: one antialiased pixel column at most, so the left cell's
    // colour runs to w/2 - 1 and the right cell's starts at w/2.
    let left = widest_run(&image, middle, LEFT);
    let right = widest_run(&image, middle, RIGHT);
    assert!(
        (left - w / 2).abs() <= 1,
        "the left cell is {left} px of {w}"
    );
    assert!(
        (right - (w - w / 2)).abs() <= 1,
        "the right cell is {right} px of {w}"
    );
}

#[test]
fn the_gap_is_the_requested_width_in_pixels() {
    // The measured width is the assertion, and this document has five stripes to
    // measure: the two cells share a full-height seam, and the sheet's own edge is a
    // stripe as well. Half the gap comes off each side of a cell and the whole gap
    // off the sheet's, so **all five measure `gapRel × canvas height`** — that one
    // number for the seam and for the border is what ruling 35 of 2026-09-25 asked
    // for, and it holds at any resolution because `gapRel` is a fraction of the
    // canvas height. The last row of the table has a radius: the corners are cut
    // away, and the stripes at the middles must not move because of it.
    for (gap_rel, radius_rel) in [
        (0.01, 0.0),
        (0.02, 0.0),
        (0.04, 0.0),
        (0.08, 0.0),
        (0.04, 0.03),
    ] {
        let frame = Frame {
            gap_rel,
            radius_rel,
            ..Frame::default()
        };
        let (image, canvas) = render(frame);
        let expected = (gap_rel * f64::from(canvas.height)).round() as i32;
        let what = format!("gap {gap_rel} radius {radius_rel}");

        // Across the sheet, in one row: the left border, the seam, the right border.
        let row: Vec<i32> = runs(&image, canvas.height / 2, [255, 255, 255])
            .into_iter()
            .map(|(from, to)| to - from + 1)
            .collect();
        assert_eq!(row.len(), 3, "{what}: the row's stripes are {row:?}");
        for stripe in &row {
            assert!(
                (stripe - expected).abs() <= 2,
                "{what}: a {stripe} px stripe instead of {expected} px in the middle row"
            );
        }

        // And down the sheet, through the left cell: the top and bottom borders.
        let column: Vec<i32> = column_runs(&image, canvas.width / 4, [255, 255, 255])
            .into_iter()
            .map(|(from, to)| to - from + 1)
            .collect();
        assert_eq!(
            column.len(),
            2,
            "{what}: the column's stripes are {column:?}"
        );
        for stripe in &column {
            assert!(
                (stripe - expected).abs() <= 2,
                "{what}: a {stripe} px stripe instead of {expected} px in a column"
            );
        }
    }
}

#[test]
fn a_single_slot_document_shows_one_uniform_border() {
    // The one-photo collage (S19's `grid-1-1x1`): one cell covering the sheet, so
    // there is no seam at all and the frame is the whole of what is visible — the
    // same distance on all four sides, which is the border a one-photo document is
    // framed with.
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
    let gap_rel = 0.06;
    let mut doc = CollageDoc::new(template);
    doc.frame = Frame {
        gap_rel,
        ..Frame::default()
    };
    doc.validate().expect("a legal frame");
    doc.cells[0].source = Some("photo.png".into());
    let canvas = PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size");
    let image = render_rgb8(&doc, &images(&doc, canvas), canvas, 1.0, None).expect("renders");

    let expected = (gap_rel * f64::from(canvas.height)).round() as i32;
    let row: Vec<i32> = runs(&image, canvas.height / 2, [255, 255, 255])
        .into_iter()
        .map(|(from, to)| to - from + 1)
        .collect();
    let column: Vec<i32> = column_runs(&image, canvas.width / 2, [255, 255, 255])
        .into_iter()
        .map(|(from, to)| to - from + 1)
        .collect();
    assert_eq!(row.len(), 2, "the row's borders are {row:?}");
    assert_eq!(column.len(), 2, "the column's borders are {column:?}");
    for stripe in row.iter().chain(column.iter()) {
        assert!(
            (stripe - expected).abs() <= 2,
            "a {stripe} px border instead of {expected} px"
        );
    }
    // The photo is still the middle of the sheet, and it is the only thing there.
    assert_eq!(image.pixel(canvas.width / 2, canvas.height / 2), LEFT);
}

#[test]
fn a_rounded_corner_uncovers_the_backdrop_monotonically() {
    // The corner is where the radius shows: at radius 0 a cell's inset rectangle is
    // all photo, and every step of the radius leaves more backdrop inside it. The
    // count is of *exact* backdrop pixels, so the clip's antialiased edge is not
    // what is being measured.
    let gap_rel = 0.02;
    let mut previous = 0;
    for radius_rel in [0.0, 0.01, 0.02, 0.04, 0.08] {
        let frame = Frame {
            gap_rel,
            radius_rel,
            ..Frame::default()
        };
        // A coloured backdrop makes "the corner is backdrop" and "the corner is the
        // canvas' own colour" the same measurement.
        let frame = Frame {
            color: Rgba8::rgb(20, 40, 60),
            ..frame
        };
        let (image, canvas) = render(frame);
        let (w, h) = (f64::from(canvas.width), f64::from(canvas.height));
        // Cell 0's visible rectangle in pixels, asked of the renderer's own
        // definition rather than re-derived: half the gap off the cell's sides, cut
        // back to the sheet with the whole gap off the sheet's (S20). Everything
        // inside it that is not the photo is the radius's doing.
        let doc = doc(frame);
        let (rect, _radius) = doc.frame.clip(&doc.template.slots[0], doc.template.aspect);
        let inset = (
            (rect.x0 * w) as i32,
            (rect.y0 * h) as i32,
            (rect.x1 * w) as i32,
            (rect.y1 * h) as i32,
        );
        let corners = count_inside(&image, inset, rgb(frame.color), 2);
        if radius_rel == 0.0 {
            assert_eq!(
                corners, 0,
                "a square corner leaves no backdrop inside the cell"
            );
        } else {
            assert!(
                corners > previous,
                "radius {radius_rel}: {corners} px, not more than {previous}"
            );
        }
        previous = corners;
    }
    // The last radius is large enough to be visible: a 0.08 radius on a 1063 px
    // canvas is 85 px, and its four corners are 4*r^2*(1 - pi/4) = 6,202 px of
    // which 4,831 measure as *exactly* the backdrop (the rest is the arc's own
    // antialiasing, which is not what a corner count is asking about). Measured
    // 2026-09-22 (S11); the bound is an order of magnitude below, so a corner that
    // stopped being drawn fails here.
    assert!(
        previous > 4_000,
        "the largest radius uncovered {previous} px"
    );
}

#[test]
fn a_coloured_backdrop_carries_exactly_that_colour() {
    // The backdrop is painted, not blended: every pixel the photos do not reach is
    // the requested colour to the byte, and the cells still carry their own.
    let border = Rgba8 {
        r: 12,
        g: 200,
        b: 240,
        a: 255,
    };
    let frame = Frame {
        gap_rel: 0.04,
        radius_rel: 0.05,
        color: border,
    };
    let (image, canvas) = render(frame);
    let (w, h) = (canvas.width, canvas.height);
    let wanted = rgb(border);

    // The canvas border, the stripe between the cells and the four rounded
    // corners: all of it is the one colour, and never white.
    for (x, y) in [
        (2, h / 2),
        (w - 3, h / 2),
        (w / 2, 2),
        (w / 2, h - 3),
        (w / 2, h / 2),
    ] {
        assert_eq!(image.pixel(x, y), wanted, "({x}, {y})");
    }
    let mut backdrop = 0u64;
    let mut white = 0u64;
    for y in 0..h {
        for x in 0..w {
            match image.pixel(x, y) {
                pixel if pixel == wanted => backdrop += 1,
                [255, 255, 255] => white += 1,
                _ => {}
            }
        }
    }
    assert!(backdrop > 10_000, "only {backdrop} backdrop pixels");
    assert_eq!(
        white, 0,
        "{white} pixels are white although the frame is not"
    );

    // The cells are still their own colours in the middle, and the rounded corner
    // is inside the cell's own rectangle — so the corner pixels are backdrop, not a
    // stretched photo.
    assert_eq!(image.pixel(w / 4, h / 2), LEFT);
    assert_eq!(image.pixel(3 * w / 4, h / 2), RIGHT);
}

#[test]
fn a_radius_larger_than_the_cell_is_clamped_to_a_stadium() {
    // `radiusRel` is clamped to half the smaller side of the cell, so a request
    // past that rounds the ends instead of folding the cell inside out. The cell is
    // still fully covered along its own centre line, which is what "stadium" means.
    let frame = Frame {
        gap_rel: 0.02,
        radius_rel: 1.0,
        ..Frame::default()
    };
    let (image, canvas) = render(frame);
    let middle = canvas.height / 2;
    // The visible rectangle is what the radius is clamped to and what the stadium's
    // body spans at its centre line: its own width, minus the antialiased edges.
    let doc = doc(frame);
    let (rect, radius) = doc.frame.clip(&doc.template.slots[0], doc.template.aspect);
    let expected = (rect.width() * f64::from(canvas.width)) as i32;
    let radius_px = radius * f64::from(canvas.height);
    assert!(
        (radius_px - f64::from(expected) / 2.0).abs() <= 1.0,
        "a 1.0 radius is clamped to half the visible rectangle's smaller side, \
         measured {radius_px} px against a {} px cell",
        expected / 2
    );
    let left = widest_run(&image, middle, LEFT);
    assert!(
        (left - expected).abs() <= 2,
        "a 1.0 radius left {left} px of a {expected} px cell"
    );
    // Away from the centre the stadium narrows: at 5% from the top the cell is
    // backdrop on the left, and still photo where the narrowest part has passed.
    let near_top = (f64::from(canvas.height) * 0.05) as i32;
    assert_eq!(
        image.pixel(2, near_top),
        [255, 255, 255],
        "the corner is photo"
    );
    assert_eq!(
        image.pixel(canvas.width / 4, near_top),
        LEFT,
        "the stadium's own body is not photo"
    );
}
