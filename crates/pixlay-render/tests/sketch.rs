// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S21's rendering surface, as tests: a template drawn as ink on paper.
//!
//! No display and no decoder: these are the sketch alone, so they are the same on
//! any machine. What they pin is the claim the layout band and `render --sketch`
//! rest on — *every* cell's outline is drawn, in the caller's own ink over the
//! caller's own paper, and the ink is exactly a cell's outline or the sheet's
//! ground no cell covers (S29) — plus determinism, which is what "a candidate
//! equals the CLI's own sketch" is a comparison of.

use pixlay_core::{PixelSize, Point, Rgba8, templates};
use pixlay_render::{RenderError, Rgb8Image, Sketch, sketch_rgb8};

/// The grid the tests draw on: the band's own candidate box's rule, so what is
/// checked here is checked at the size the band draws at.
fn grid(aspect: f64) -> PixelSize {
    templates::candidate_grid(aspect)
}

/// One pixel of an `Rgb8Image`.
fn pixel(image: &Rgb8Image, x: i32, y: i32) -> [u8; 3] {
    image.pixel(x, y)
}

/// One pixel of an `Rgb8Image`, addressed in normalized coordinates: the pixel the
/// point falls in, which is the pixel a probe written in the template's own units
/// means.
fn at(image: &Rgb8Image, canvas: PixelSize, nx: f64, ny: f64) -> [u8; 3] {
    let x = ((nx * f64::from(canvas.width)).floor() as i32).clamp(0, canvas.width - 1);
    let y = ((ny * f64::from(canvas.height)).floor() as i32).clamp(0, canvas.height - 1);
    pixel(image, x, y)
}

/// The sketch style the rules below are checked in: two colours no blend of which
/// can be mistaken for the other, and the band's own one-pixel line.
fn two_colours() -> Sketch {
    Sketch {
        paper: Rgba8::rgb(255, 0, 0),
        ink: Rgba8::rgb(0, 0, 255),
        stroke_px: 1.0,
    }
}

/// One of a style's two colours as a pixel of the surface.
fn rgb(color: Rgba8) -> [u8; 3] {
    [color.r, color.g, color.b]
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

/// The human's finding of 2026-09-26, as numbers: the library's two guttered
/// layouts draw their gutter as a gap, and their cells are still paper.
///
/// `grid-4-2x2g`'s four cells stop a 1/16 canvas short of each other on both
/// axes, `strip-2-2x1g`'s two stop short on one; until S29 a sketch painted that
/// ground in the paper's own colour, so at the band's 128x96 a gutter read as one
/// more cell.
#[test]
fn a_gutter_is_drawn_as_a_gap() {
    let style = two_colours();
    let (paper, ink) = (rgb(style.paper), rgb(style.ink));

    for (name, gutter) in [
        // The crossing, the four arms' middles and the sheet's own edges, all
        // inside the 1/16-wide cross.
        (
            "grid-4-2x2g",
            [(0.5, 0.5), (0.5, 0.1), (0.9, 0.5), (0.5, 0.9), (0.1, 0.5)].as_slice(),
        ),
        (
            "strip-2-2x1g",
            [(0.5, 0.5), (0.5, 0.1), (0.5, 0.9)].as_slice(),
        ),
    ] {
        let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
        let canvas = grid(template.aspect);
        let image = sketch_rgb8(&template, canvas, &style).expect("the sketch draws");

        for &(nx, ny) in gutter {
            assert_eq!(
                at(&image, canvas, nx, ny),
                ink,
                "{name}: the gutter at ({nx}, {ny}) is not the ink — a gap has to read as one"
            );
        }
        for (slot, cell) in template.slots.iter().enumerate() {
            let centre = cell.outline.bbox().center();
            assert_eq!(
                at(&image, canvas, centre.x, centre.y),
                paper,
                "{name} slot {slot}: the middle of a cell is not the paper"
            );
        }
    }
}

/// The rule the two colours are drawn by, over the whole library: a pixel far
/// enough from every outline to be no antialiasing is **paper where a cell covers
/// it and ink where none does**.
///
/// "Far enough" is two pixels: an outline is one pixel wide and shifted half a
/// pixel onto a pixel centre, so everything within two pixels of the geometry is
/// the stroke's own business. The probe is the geometry's own predicate
/// (`Polygon::contains`, the same even-odd reading the drawing's fill rule makes),
/// so this is a comparison rather than a restatement of the code that drew it.
#[test]
fn the_ink_is_a_cells_outline_or_the_ground_no_cell_covers() {
    let style = two_colours();
    let (paper, ink) = (rgb(style.paper), rgb(style.ink));

    for name in templates::names() {
        let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
        let canvas = grid(template.aspect);
        let image = sketch_rgb8(&template, canvas, &style).expect("the sketch draws");
        let (w, h) = (f64::from(canvas.width), f64::from(canvas.height));
        let guard = 2.0 / w.min(h);

        for y in 0..canvas.height {
            for x in 0..canvas.width {
                let point = Point::new((f64::from(x) + 0.5) / w, (f64::from(y) + 0.5) / h);
                if template
                    .slots
                    .iter()
                    .any(|slot| slot.outline.distance_to_boundary(point) < guard)
                {
                    continue;
                }
                let covered = template
                    .slots
                    .iter()
                    .any(|slot| slot.outline.contains(point));
                let want = if covered { paper } else { ink };
                assert_eq!(
                    pixel(&image, x, y),
                    want,
                    "{name}: ({x},{y}) is not what the rule says — \
                     {} expected, cell coverage {covered}",
                    if covered { "paper" } else { "ink" }
                );
            }
        }
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
