//! S21's rendering surface, as tests: a template drawn as ink on paper.
//!
//! No display and no decoder: these are the sketch alone, so they are the same on
//! any machine. What they pin is the claim the layout band and `render --sketch`
//! rest on — *every* cell's outline is drawn, in the caller's own ink over the
//! caller's own paper — plus determinism, which is what "a candidate equals the
//! CLI's own sketch" is a comparison of.

use pixlay_core::{PixelSize, Rgba8, templates};
use pixlay_render::{RenderError, Sketch, sketch_rgb8};

/// The grid the tests draw on: the band's own candidate box's rule, so what is
/// checked here is checked at the size the band draws at.
fn grid(aspect: f64) -> PixelSize {
    templates::candidate_grid(aspect)
}

/// One pixel of an `Rgb8Image`.
fn pixel(image: &pixlay_render::Rgb8Image, x: i32, y: i32) -> [u8; 3] {
    image.pixel(x, y)
}

#[test]
fn every_cell_outline_is_inked_over_the_sheets_ground() {
    let white = Rgba8::WHITE;
    for name in ["grid-1-1x1", "strip-2-2x1", "mosaic-8-s14"] {
        let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
        let canvas = grid(template.aspect);
        let image = sketch_rgb8(&template, canvas, &Sketch::default()).expect("the sketch draws");
        assert_eq!(
            (image.width, image.height),
            (canvas.width, canvas.height),
            "{name}: the sketch is the grid it was asked for"
        );

        // Every cell's every vertex and edge midpoint is on a stroked outline, so
        // the pixel there is ink or a blend of ink and paper — never the untouched
        // ground. A template drawn with one cell missing fails here at that cell's
        // first vertex. An outline that runs along the sheet's own border is
        // clipped to half a stroke, which is a blend rather than solid ink; that is
        // why this asks "not paper" rather than "the ink colour".
        for (slot, cell) in template.slots.iter().enumerate() {
            let points = &cell.outline.points;
            for (index, point) in points.iter().enumerate() {
                let next = &points[(index + 1) % points.len()];
                for (x, y) in [
                    (point.x, point.y),
                    ((point.x + next.x) / 2.0, (point.y + next.y) / 2.0),
                ] {
                    let x =
                        ((x * f64::from(canvas.width)).round() as i32).clamp(0, canvas.width - 1);
                    let y =
                        ((y * f64::from(canvas.height)).round() as i32).clamp(0, canvas.height - 1);
                    assert_ne!(
                        pixel(&image, x, y),
                        [white.r, white.g, white.b],
                        "{name} slot {slot}: the outline at ({x},{y}) is bare paper"
                    );
                }
            }
        }

        // And the drawing is a drawing: thin ink, mostly ground.
        let total = (canvas.width * canvas.height) as usize;
        let paper = (0..canvas.height)
            .flat_map(|y| (0..canvas.width).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&image, x, y) == [white.r, white.g, white.b])
            .count();
        assert!(
            paper > total / 2,
            "{name}: {paper} of {total} pixels are paper — the outlines are not thin lines"
        );
    }
}

#[test]
fn the_two_colours_and_the_width_are_the_callers() {
    let template = templates::get("grid-1-1x1").expect("the sheet is a template");
    let canvas = grid(template.aspect);
    let sketch = Sketch {
        paper: Rgba8::rgb(255, 0, 0),
        ink: Rgba8::rgb(0, 0, 255),
        stroke_px: 3.0,
    };
    let image = sketch_rgb8(&template, canvas, &sketch).expect("the sketch draws");
    let mut paper = 0;
    let mut ink = 0;
    for y in 0..canvas.height {
        for x in 0..canvas.width {
            match pixel(&image, x, y) {
                [255, 0, 0] => paper += 1,
                [0, 0, 255] => ink += 1,
                // A three-pixel stroke on a rounded vertex is antialiased; what
                // must not exist is a pixel of some *other* colour, which is what
                // a hard-coded palette or a stray white base would show as.
                [r, g, b] => assert!(
                    (r > 0 || g > 0) && b >= r,
                    "({x},{y}) is {r},{g},{b}: neither the caller's paper nor a blend toward the ink"
                ),
            }
        }
    }
    assert!(paper > 0, "the paper is drawn");
    assert!(
        ink > canvas.width,
        "the ink is drawn: {ink} fully inked pixels for a {canvas:?} sheet"
    );
}

#[test]
fn the_same_template_and_style_draw_the_same_bytes() {
    let template = templates::get("mosaic-8-s14").expect("the verification layout");
    let canvas = grid(template.aspect);
    let style = Sketch {
        paper: Rgba8::rgb(20, 20, 24),
        ink: Rgba8::rgb(240, 240, 240),
        stroke_px: 1.0,
    };
    let first = sketch_rgb8(&template, canvas, &style).expect("the sketch draws");
    let second = sketch_rgb8(&template, canvas, &style).expect("the sketch draws");
    assert_eq!(
        first.data, second.data,
        "two sketches of one template differ: the band's parity against the CLI depends on this"
    );
}

#[test]
fn a_width_that_is_not_a_line_is_refused() {
    let template = templates::get("grid-1-1x1").expect("the sheet is a template");
    let canvas = grid(template.aspect);
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let style = Sketch {
            stroke_px: width,
            ..Sketch::default()
        };
        assert!(
            matches!(
                sketch_rgb8(&template, canvas, &style),
                Err(RenderError::InvalidStroke(_))
            ),
            "a stroke of {width} is not a line"
        );
    }
}
