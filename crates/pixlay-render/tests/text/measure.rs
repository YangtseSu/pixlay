//! The text measurements (`docs/STEPS.md` S5).
//!
//! Every test here is `#[ignore]`d and runs only in the pinned child — see
//! `fonts.rs` for why the font has to be a process property. `MEASUREMENTS` is the
//! count the harness checks against, so a test that disappears takes the harness
//! with it instead of quietly measuring less.

use std::f64::consts::PI;

use cairo::{Context, Format, ImageSurface};
use pangocairo::pango;
use pixlay_core::{
    Anchor, CanvasSpec, CollageDoc, PixelSize, Point, Polygon, Rgba8, Slot, Template, TextFallback,
    TextLayer, TextMode, TextValues,
};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8, text};

use super::fonts;

/// How many `#[ignore]`d tests this module has.
pub const MEASUREMENTS: usize = 12;

const DPI: u32 = 96;

/// The character the geometry tests draw: a solid em square, so "where is the ink"
/// has an exact answer that does not depend on glyph outlines.
const BLOCK: &str = "\u{2588}";

/// The date an EXIF `DateTimeOriginal` carries, in its own format (contract §1).
const EXIF_DATE: &str = "2019:07:14 10:32:00";

/// CJK paragraphs the measurements draw, with the punctuation runs that kinsoku
/// and punctuation squeezing are about. Every character here has to be in the
/// committed subset — `measure_the_test_font_covers_every_character_the_
/// measurements_use` fails with the missing codepoint otherwise.
const PARAGRAPHS: [&str; 4] = [
    "他说：“今天天气很好。”然后他走了。",
    "价格是100元，质量很好，值得购买。",
    "（括号里的内容）后面的内容继续写下去看看。",
    "这是一段用来测试断行的中文文字，标点不应该出现在行首。",
];

/// Three slots with a margin around them, so a text layer has white to be measured
/// against and slots to be painted over.
const SLOTS: [(f64, f64, f64, f64); 3] = [
    (0.05, 0.05, 0.5, 0.95),
    (0.5, 0.05, 0.95, 0.5),
    (0.5, 0.5, 0.95, 0.95),
];

const COLORS: [Rgba8; 3] = [
    Rgba8::rgb(200, 30, 40),
    Rgba8::rgb(30, 160, 60),
    Rgba8::rgb(40, 60, 220),
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
    CollageDoc::new(
        CanvasSpec::new(120.0, 90.0),
        Template {
            name: "test-3".to_string(),
            version: 1,
            aspect: 4.0 / 3.0,
            slots,
        },
    )
}

fn canvas_px() -> PixelSize {
    doc().canvas.pixel_size(DPI).expect("canvas size")
}

/// Flat slot colors: a text layer drawn over one of them is measurable against a
/// known colour.
fn flat_images() -> Images {
    let canvas = canvas_px();
    let mut images = Images::new();
    for (index, &(x0, y0, x1, y1)) in SLOTS.iter().enumerate() {
        let width = ((x1 - x0) * f64::from(canvas.width)).round() as i32;
        let height = ((y1 - y0) * f64::from(canvas.height)).round() as i32;
        images.insert(index, Bitmap::filled(width, height, COLORS[index]).unwrap());
    }
    images
}

fn free_layer(
    content: &str,
    position: Point,
    anchor: Anchor,
    size_rel: f64,
    rotation_deg: f64,
) -> TextLayer {
    TextLayer {
        content: content.to_string(),
        mode: TextMode::Free { position, anchor },
        size_rel,
        rotation_deg,
        color: Rgba8::BLACK,
        source_slot: None,
    }
}

/// One layer's text, sized as `draw` sizes it: `size_rel` of the canvas height in
/// canvas pixels.
fn font_px(layer: &TextLayer) -> f64 {
    layer.size_rel * f64::from(canvas_px().height)
}

/// The layout `draw` builds for `layer` — the same function, not a copy of it.
fn layout_of(layer: &TextLayer, content: &str, wrap: bool) -> pango::Layout {
    let surface = ImageSurface::create(Format::ARgb32, 8, 8).expect("surface");
    let ctx = Context::new(&surface).expect("context");
    let wrap_px = wrap.then(|| f64::from(canvas_px().width));
    text::layout(&ctx, content, font_px(layer), wrap_px)
}

fn render(doc: &CollageDoc, images: &Images, scale: f64) -> Rgb8Image {
    render_rgb8(doc, images, DPI, scale, None).expect("renders")
}

/// A pixel rectangle, `(x, y, width, height)` in output pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Rect {
    fn center(&self) -> (f64, f64) {
        (
            f64::from(self.x) + f64::from(self.width) / 2.0,
            f64::from(self.y) + f64::from(self.height) / 2.0,
        )
    }
}

/// The bounding box of every pixel `is_ink` calls ink, or `None` for none of them.
fn ink_rect(image: &Rgb8Image, is_ink: impl Fn([u8; 3]) -> bool) -> Option<Rect> {
    let (mut x0, mut y0) = (i32::MAX, i32::MAX);
    let (mut x1, mut y1) = (i32::MIN, i32::MIN);
    for y in 0..image.height {
        for x in 0..image.width {
            if !is_ink(image.pixel(x, y)) {
                continue;
            }
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 <= x1).then(|| Rect {
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0 + 1,
    })
}

/// The ink's bounding box inside `region`, in image coordinates.
fn scan_region(image: &Rgb8Image, region: Rect) -> Option<Rect> {
    let (mut x0, mut y0) = (i32::MAX, i32::MAX);
    let (mut x1, mut y1) = (i32::MIN, i32::MIN);
    for y in region.y.max(0)..(region.y + region.height).min(image.height) {
        for x in region.x.max(0)..(region.x + region.width).min(image.width) {
            if !dark(image.pixel(x, y)) {
                continue;
            }
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 <= x1).then(|| Rect {
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0 + 1,
    })
}

/// Darker than anything the canvas can be: the text layers here are black or
/// nearly so, and the slot colours are not.
fn dark(pixel: [u8; 3]) -> bool {
    pixel[0] < 128 && pixel[1] < 128 && pixel[2] < 128
}

fn ink_pixels(image: &Rgb8Image, is_ink: impl Fn([u8; 3]) -> bool) -> usize {
    let mut count = 0;
    for y in 0..image.height {
        for x in 0..image.width {
            if is_ink(image.pixel(x, y)) {
                count += 1;
            }
        }
    }
    count
}

/// The bounding box of the pixels where two renders differ.
fn difference_rect(a: &Rgb8Image, b: &Rgb8Image) -> Option<Rect> {
    let mut data = vec![255u8; a.data.len()];
    for (index, (left, right)) in a.data.iter().zip(&b.data).enumerate() {
        if left.abs_diff(*right) > 8 {
            data[index] = 0;
        }
    }
    ink_rect(
        &Rgb8Image {
            width: a.width,
            height: a.height,
            data,
        },
        dark,
    )
}

/// Which point of its own box an anchor names, as `(x, y)` fractions.
fn anchor_fractions(anchor: Anchor) -> (f64, f64) {
    let x = match anchor {
        Anchor::TopLeft | Anchor::CenterLeft | Anchor::BottomLeft => 0.0,
        Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => 0.5,
        Anchor::TopRight | Anchor::CenterRight | Anchor::BottomRight => 1.0,
    };
    let y = match anchor {
        Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight => 0.0,
        Anchor::CenterLeft | Anchor::Center | Anchor::CenterRight => 0.5,
        Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => 1.0,
    };
    (x, y)
}

/// The rectangle the layout's *own* metrics say the ink lands in.
///
/// Both rects `pango` reports are relative to the layout's origin, so this is the
/// prediction the renderer has to match: the box (`logical`) placed by the anchor,
/// then the ink's offset inside it. Comparing against it is what makes "the anchor
/// places the text's box" a measurement instead of a restatement of the code.
fn predicted_ink(
    layout: &pango::Layout,
    anchor: Anchor,
    position: Point,
    canvas: PixelSize,
) -> Rect {
    let (ink, logical) = layout.extents();
    let scale = f64::from(pango::SCALE);
    let (fraction_x, fraction_y) = anchor_fractions(anchor);
    let box_x =
        position.x * f64::from(canvas.width) - fraction_x * f64::from(logical.width()) / scale;
    let box_y =
        position.y * f64::from(canvas.height) - fraction_y * f64::from(logical.height()) / scale;
    Rect {
        x: (box_x + f64::from(logical.x() + ink.x()) / scale).round() as i32,
        y: (box_y + f64::from(logical.y() + ink.y()) / scale).round() as i32,
        width: (f64::from(ink.width()) / scale).round() as i32,
        height: (f64::from(ink.height()) / scale).round() as i32,
    }
}

fn close(measured: i32, predicted: i32, what: &str) {
    assert!(
        (measured - predicted).abs() <= 2,
        "{what}: measured {measured}, the layout's own metrics predict {predicted}"
    );
}

/// One CJK layout's lines, as strings.
fn lines_of(layout: &pango::Layout, content: &str) -> Vec<String> {
    layout
        .lines()
        .iter()
        .map(|line| {
            let start = line.start_index() as usize;
            content[start..start + line.length() as usize].to_string()
        })
        .collect()
}

/// The advance of every character of a one-line layout, in pixels.
fn advances(ctx: &Context, content: &str, font: f64) -> Vec<f64> {
    let layout = text::layout(ctx, content, font, None);
    let line = layout.line(0).expect("a one-line layout has a line");
    let mut positions = Vec::new();
    let mut byte = 0i32;
    for character in content.chars() {
        positions.push(line.index_to_x(byte, false) as f64 / f64::from(pango::SCALE));
        byte += character.len_utf8() as i32;
    }
    positions.push(line.index_to_x(byte, false) as f64 / f64::from(pango::SCALE));
    positions.windows(2).map(|pair| pair[1] - pair[0]).collect()
}

// ---------------------------------------------------------------------------
// The measurements
// ---------------------------------------------------------------------------

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_free_placement_puts_the_box_on_the_anchor() {
    let canvas = canvas_px();
    for (anchor, position) in [
        (Anchor::Center, Point::new(0.5, 0.5)),
        (Anchor::TopLeft, Point::new(0.1, 0.12)),
        (Anchor::BottomRight, Point::new(0.9, 0.86)),
        (Anchor::CenterLeft, Point::new(0.05, 0.5)),
    ] {
        let layer = free_layer(BLOCK, position, anchor, 0.08, 0.0);
        let mut doc = doc();
        doc.text.push(layer.clone());
        let image = render(&doc, &flat_images(), 1.0);
        let measured = ink_rect(&image, dark).expect("the block left ink");
        let predicted = predicted_ink(&layout_of(&layer, BLOCK, true), anchor, position, canvas);

        close(measured.x, predicted.x, "ink left edge");
        close(measured.y, predicted.y, "ink top edge");
        close(
            measured.width,
            predicted.width,
            "ink width (the font's own em square)",
        );
        close(measured.height, predicted.height, "ink height");
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_a_tiled_watermark_covers_the_grid_from_the_canvas_origin() {
    let canvas = canvas_px();
    let step = (0.25, 0.5);
    let layer = TextLayer {
        content: BLOCK.to_string(),
        mode: TextMode::Tiled { step },
        size_rel: 0.05,
        rotation_deg: 0.0,
        color: Rgba8::BLACK,
        source_slot: None,
    };
    let mut doc = doc();
    doc.text.push(layer.clone());
    let image = render(&doc, &flat_images(), 1.0);

    // `pixlay_core::tiled_grid` is the grid the renderer draws: an anchor at the
    // canvas origin and one per step after it, the far edge included.
    let (columns, rows) = pixlay_core::tiled_grid(step).expect("inside the tile cap");
    assert_eq!((columns, rows), (5, 3));
    let side = font_px(&layer);
    let layout = layout_of(&layer, BLOCK, false);
    let origin = predicted_ink(&layout, Anchor::TopLeft, Point::new(0.0, 0.0), canvas);

    // Every tile the canvas can see is a solid em square at its own anchor: a grid
    // anchored anywhere else, or a tile that is not rotated about its own anchor,
    // moves one of these boxes.
    let cell = Rect {
        x: 0,
        y: 0,
        width: (step.0 * f64::from(canvas.width)) as i32,
        height: (step.1 * f64::from(canvas.height)) as i32,
    };
    for column in 0..columns - 1 {
        for row in 0..rows - 1 {
            let region = Rect {
                x: column as i32 * cell.width,
                y: row as i32 * cell.height,
                ..cell
            };
            let measured = scan_region(&image, region)
                .unwrap_or_else(|| panic!("tile ({column}, {row}) left no ink in {region:?}"));
            let predicted = Rect {
                x: (f64::from(origin.x) + column as f64 * step.0 * f64::from(canvas.width)).round()
                    as i32,
                y: (f64::from(origin.y) + row as f64 * step.1 * f64::from(canvas.height)).round()
                    as i32,
                width: origin.width,
                height: origin.height,
            };
            close(measured.x, predicted.x, "tile left edge");
            close(measured.y, predicted.y, "tile top edge");
            close(measured.width, predicted.width, "tile width");
            close(measured.height, predicted.height, "tile height");
        }
    }
    // The far-edge column and row of anchors are on the canvas boundary, so their
    // tiles are off the canvas: nothing may be painted in the last cell's strip.
    let expected_ink = ((columns - 1) * (rows - 1)) as f64 * side * side;
    let measured = ink_pixels(&image, dark) as f64;
    assert!(
        (measured - expected_ink).abs() / expected_ink < 0.05,
        "{measured} ink pixels, expected about {expected_ink} for the {} tiles that fit",
        (columns - 1) * (rows - 1)
    );
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_consecutive_punctuation_is_compressed_to_half_width() {
    let surface = ImageSurface::create(Format::ARgb32, 8, 8).expect("surface");
    let ctx = Context::new(&surface).expect("context");
    let font = 20.0;
    let em = font;
    let half = font / 2.0;

    // A lone mark keeps its blank (measured 2026-09-21: a full-width mark advances
    // exactly one em and `halt` halves it). Keeping it is what lets `。` end a
    // sentence, so the rule must not leak into isolated marks.
    assert_eq!(advances(&ctx, "。", font), vec![em]);
    assert_eq!(advances(&ctx, "价", font), vec![em]);
    // Every mark but the last of a run is halved, closing and opening alike.
    assert_eq!(advances(&ctx, "。，", font), vec![half, em]);
    assert_eq!(advances(&ctx, "。。", font), vec![half, em]);
    assert_eq!(advances(&ctx, "。”", font), vec![half, em]);
    assert_eq!(advances(&ctx, "。。。", font), vec![half, half, em]);
    assert_eq!(advances(&ctx, "（（", font), vec![half, em]);
    // A line break ends the run: the marks are on different lines and each keeps
    // the width that keeps them column-aligned.
    let wrapped = text::layout(&ctx, "。\n。", font, None);
    assert_eq!(wrapped.lines().len(), 2);
    for line in wrapped.lines() {
        let (start, end) = (line.start_index(), line.start_index() + line.length());
        let width = f64::from(line.index_to_x(end, false) - line.index_to_x(start, false))
            / f64::from(pango::SCALE);
        assert_eq!(width, em, "a mark at a line boundary lost its width");
    }
    // Latin and digits are untouched by the rule.
    let single = advances(&ctx, "1", font)[0];
    assert_eq!(advances(&ctx, "12", font), vec![single, single]);
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_kinsoku_keeps_punctuation_off_a_line_start() {
    /// Characters a line must not start with (JLREQ 行頭禁則): closing brackets,
    /// commas and periods.
    const FORBIDDEN_START: &str = "、。，．：；？！）］｝〕〉》」』】〙〛’”";
    /// Characters a line must not end with (行末禁則): opening brackets.
    const FORBIDDEN_END: &str = "（［｛〔〈《「『【〘〚‘“";

    let surface = ImageSurface::create(Format::ARgb32, 8, 8).expect("surface");
    let ctx = Context::new(&surface).expect("context");
    let font = 20.0;

    // The decisive case: at 4 em the greedy fill is "他他他说" and the next line
    // would start with "。". Measured 2026-09-21, Pango pulls the break back one
    // character instead. This is the assertion that fails if the breaker ever stops
    // honouring kinsoku — without it, the loop below would pass on a layout that
    // simply never breaks before a mark by accident.
    let pushed = text::layout(&ctx, "他他他说。他", font, Some(font * 4.0));
    assert_eq!(
        lines_of(&pushed, "他他他说。他"),
        vec!["他他他".to_string(), "说。他".to_string()]
    );

    // And the rule itself, over every width of a paragraph with punctuation runs,
    // including punctuation pushed to the end of a line by the fill.
    for content in PARAGRAPHS {
        for width in [3.0, 4.0, 5.0, 6.0, 7.5, 9.0] {
            let layout = text::layout(&ctx, content, font, Some(font * width));
            let lines = lines_of(&layout, content);
            assert!(
                lines.len() > 1,
                "{content:?} at {width} em fitted on one line; the sweep proves nothing"
            );
            for line in &lines {
                let first = line.chars().next().expect("a line has a character");
                let last = line.chars().next_back().expect("a line has a character");
                assert!(
                    !FORBIDDEN_START.contains(first),
                    "{content:?} at {width} em starts a line with {first:?}: {lines:?}"
                );
                assert!(
                    !FORBIDDEN_END.contains(last),
                    "{content:?} at {width} em ends a line with {last:?}: {lines:?}"
                );
            }
        }
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_a_slot_rotation_does_not_move_the_text() {
    // The frozen evaluation order puts text after the slots: reframing a photo must
    // not move a caption, which is what "content layers run last" buys.
    let layer = free_layer("2019", Point::new(0.5, 0.5), Anchor::Center, 0.1, 0.0);
    let mut with_text = doc();
    with_text.text.push(layer);
    let mut plain = doc();
    let images = flat_images();

    let mut at_rotation = |rotation: f64| {
        with_text.cells[0].crop.rotation_deg = rotation;
        plain.cells[0].crop.rotation_deg = rotation;
        difference_rect(
            &render(&with_text, &images, 1.0),
            &render(&plain, &images, 1.0),
        )
    };
    let straight = at_rotation(0.0).expect("the text left ink");
    let rotated = at_rotation(35.0).expect("the text left ink");
    assert_eq!(
        straight, rotated,
        "rotating a slot moved the text: {straight:?} vs {rotated:?}"
    );

    // Not vacuous: the photo really did change, so the renders are not identical.
    with_text.cells[0].crop.rotation_deg = 0.0;
    let before = render(&with_text, &images, 1.0);
    with_text.cells[0].crop.rotation_deg = 35.0;
    assert_ne!(
        before.data,
        render(&with_text, &images, 1.0).data,
        "the slot rotation changed nothing, so this test proves nothing"
    );
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_a_text_layer_rotates_about_its_anchor() {
    // A rotation about the anchor keeps the ink's bounding-box centre on the circle
    // around the anchor, so the measured centre has to be the rotated one. Rotating
    // about the canvas origin, or about the box's centre instead of the anchor,
    // fails immediately.
    let position = Point::new(0.35, 0.6);
    let layer = free_layer("2026", position, Anchor::BottomRight, 0.08, 0.0);
    let mut doc = doc();
    doc.text.push(layer);
    let images = flat_images();
    let baseline = ink_rect(&render(&doc, &images, 1.0), dark)
        .expect("ink")
        .center();
    let anchor_px = (
        position.x * f64::from(canvas_px().width),
        position.y * f64::from(canvas_px().height),
    );
    let offset = (baseline.0 - anchor_px.0, baseline.1 - anchor_px.1);

    for degrees in [45.0, 90.0, 180.0, -30.0] {
        doc.text[0].rotation_deg = degrees;
        let centre = ink_rect(&render(&doc, &images, 1.0), dark)
            .expect("ink")
            .center();
        let (sin, cos) = (degrees * PI / 180.0).sin_cos();
        let expected = (
            anchor_px.0 + offset.0 * cos - offset.1 * sin,
            anchor_px.1 + offset.0 * sin + offset.1 * cos,
        );
        let distance = ((centre.0 - expected.0).powi(2) + (centre.1 - expected.1).powi(2)).sqrt();
        assert!(
            distance <= 2.0,
            "at {degrees} degrees the ink centre is {centre:?}, expected {expected:?}: the \
             layer does not rotate about its anchor {anchor_px:?}"
        );
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_tokens_render_the_values_the_slot_reports() {
    let stored = TextFallback {
        date: "2026-09-21".to_string(),
    };
    let shots = TextValues {
        date: Some(EXIF_DATE.to_string()),
        filename: Some("dated.jpg".to_string()),
    };
    // A slot with a photo that says nothing, and a layer that names no slot at all.
    let silent = TextValues::default();

    // Each case is rendered twice: once with the token and once with the text the
    // token has to produce, typed out. "The renders are identical" is a statement
    // about the string, not about some ink having appeared somewhere.
    let cases: [(&str, &TextValues, &TextFallback, String, bool); 6] = [
        (
            "{date} #{index}",
            &shots,
            &stored,
            format!("{EXIF_DATE} #1"),
            true,
        ),
        // No EXIF date: the document's own string, so an export stays reproducible
        // (contract §1, `textFallback`).
        ("{date}", &silent, &stored, stored.date.clone(), true),
        ("{filename}", &shots, &stored, "dated.jpg".to_string(), true),
        (
            "没有{date}",
            &silent,
            &stored,
            format!("没有{}", stored.date),
            true,
        ),
        // Nothing to substitute and no stored date: empty, never a literal token.
        (
            "<{date}{filename}>",
            &silent,
            &TextFallback::default(),
            "<>".to_string(),
            true,
        ),
        (
            "{index}",
            &silent,
            &stored,
            "1".to_string(),
            // A layer that names no slot has no index either: `{index}` is the slot
            // the layer resolves against, and there is none.
            false,
        ),
    ];
    for (content, values, fallback, expected, has_slot) in cases {
        let mut token_doc = doc();
        let mut layer = free_layer(content, Point::new(0.5, 0.5), Anchor::Center, 0.07, 0.0);
        layer.source_slot = has_slot.then_some(1);
        token_doc.text.push(layer);
        token_doc.text_fallback = fallback.clone();
        if !has_slot {
            // Without a slot the token renders as nothing, and the expectation for
            // this case is the empty version of the pattern.
            let expected = if expected == "1" {
                String::new()
            } else {
                expected
            };
            let mut literal_doc = doc();
            literal_doc.text.push(free_layer(
                &expected,
                Point::new(0.5, 0.5),
                Anchor::Center,
                0.07,
                0.0,
            ));
            let empty_values = flat_images();
            assert_eq!(
                render(&token_doc, &empty_values, 1.0).data,
                render(&literal_doc, &flat_images(), 1.0).data,
                "{content} resolved against no slot should render as {expected:?}"
            );
            continue;
        }

        let mut with_values = flat_images();
        with_values.set_text_values(1, values.clone());
        let mut literal_doc = doc();
        literal_doc.text.push(free_layer(
            &expected,
            Point::new(0.5, 0.5),
            Anchor::Center,
            0.07,
            0.0,
        ));
        literal_doc.text_fallback = fallback.clone();
        assert_eq!(
            render(&token_doc, &with_values, 1.0).data,
            render(&literal_doc, &flat_images(), 1.0).data,
            "{content} did not render as {expected:?}: the two renders differ"
        );
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_text_alpha_composites_over_what_is_beneath() {
    // A half-transparent watermark is unusable if the alpha is ignored, or if the
    // colour is treated as premultiplied.
    let mut layer = free_layer(BLOCK, Point::new(0.5, 0.5), Anchor::Center, 0.6, 0.0);
    layer.color = Rgba8 {
        r: 0,
        g: 0,
        b: 0,
        a: 128,
    };
    let mut doc = doc();
    doc.text.push(layer);
    let image = render(&doc, &flat_images(), 1.0);
    let canvas = canvas_px();
    // The block covers the middle of the canvas, which is inside slot 0.
    let pixel = image.pixel(canvas.width / 3, canvas.height / 2);
    let expected = [
        (u32::from(COLORS[0].r) / 2) as u8,
        (u32::from(COLORS[0].g) / 2) as u8,
        (u32::from(COLORS[0].b) / 2) as u8,
    ];
    for channel in 0..3 {
        assert!(
            pixel[channel].abs_diff(expected[channel]) <= 2,
            "half-transparent black over {:?} gave {pixel:?}, expected about {expected:?}",
            [COLORS[0].r, COLORS[0].g, COLORS[0].b]
        );
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_a_later_layer_covers_an_earlier_one() {
    // `text` is drawn in array order (contract §1), so where two layers overlap the
    // later one wins.
    let mut first = free_layer(BLOCK, Point::new(0.5, 0.5), Anchor::Center, 0.6, 0.0);
    first.color = Rgba8::BLACK;
    let mut second = first.clone();
    second.color = Rgba8::rgb(255, 240, 0);
    let mut doc = doc();
    doc.text.push(first);
    doc.text.push(second);
    let image = render(&doc, &flat_images(), 1.0);
    let pixel = image.pixel(canvas_px().width / 3, canvas_px().height / 2);
    assert_eq!(
        pixel,
        [255, 240, 0],
        "the later layer did not cover the earlier"
    );
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_the_canvas_never_grows_with_text_on_it() {
    // "Rotation crops edges only, it never grows the canvas" holds with text too: a
    // layer half off the canvas is clipped, not accommodated, and the output keeps
    // the canvas's size at every scale.
    let mut layer = free_layer(
        "{filename} {date}",
        Point::new(0.98, 0.98),
        Anchor::Center,
        0.12,
        20.0,
    );
    layer.source_slot = Some(0);
    let mut doc = doc();
    doc.text.push(layer);
    doc.cells[0].crop.rotation_deg = 40.0;
    doc.cells[1].crop.rotation_deg = -40.0;
    let canvas = canvas_px();
    let mut images = flat_images();
    images.set_text_values(
        0,
        TextValues {
            date: Some(EXIF_DATE.to_string()),
            filename: Some("portrait.jpg".to_string()),
        },
    );
    for scale in [1.0, 0.5, 0.25] {
        let image = render(&doc, &images, scale);
        assert_eq!(
            (image.width, image.height),
            (
                (f64::from(canvas.width) * scale).round() as i32,
                (f64::from(canvas.height) * scale).round() as i32
            ),
            "the canvas changed size at scale {scale}"
        );
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_preview_and_export_agree_with_text() {
    // AGENTS.md's invariant, with a text layer in the document: the same document
    // at 2N and N, the 2N one downsampled, stays under the RMSE threshold. This is
    // what fails if the layout is sized in *device* pixels, or if hinting is left
    // on — both lay the same document out differently at two scales. Measured
    // 2026-09-21 with this document: 1.92 at 2N vs N (AGENTS.md's photo-only
    // measurement is 2.62, the threshold 6).
    let mut layer = free_layer(
        "2019:07:14 10:32:00 拍摄于",
        Point::new(0.5, 0.6),
        Anchor::Center,
        0.08,
        8.0,
    );
    layer.source_slot = Some(0);
    let mut doc = doc();
    doc.text.push(layer);
    doc.cells[1].crop.zoom = 1.6;
    doc.cells[1].crop.offset = (0.2, -0.1);
    doc.cells[1].crop.rotation_deg = 15.0;

    let images = flat_images();
    let single = render(&doc, &images, 1.0);
    let double = render(&doc, &images, 2.0);
    let error = rmse(&single, &downsample2(&double));
    assert!(
        error <= 6.0,
        "preview and export differ by RMSE {error:.2} with text on the canvas"
    );

    // And the stronger statement the RMSE only implies: the text is in the same
    // place and the same size, so the ink's box at 2N is the box at N doubled.
    let small = ink_rect(&single, dark).expect("no text in the preview");
    let large = ink_rect(&double, dark).expect("no text in the export");
    for (small, large, what) in [
        (small.x, large.x, "x"),
        (small.y, large.y, "y"),
        (small.width, large.width, "width"),
        (small.height, large.height, "height"),
    ] {
        assert!(
            (large - 2 * small).abs() <= 2,
            "the ink box's {what} is {large} at 2N and {small} at N, not double"
        );
    }
}

#[ignore = "runs in the pinned child: tests/text/fonts.rs"]
#[test]
fn measure_the_test_font_covers_every_character_the_measurements_use() {
    use pangocairo::pango::prelude::FontExt;

    let surface = ImageSurface::create(Format::ARgb32, 8, 8).expect("surface");
    let ctx = Context::new(&surface).expect("context");
    let layout = text::layout(&ctx, "A", 20.0, None);
    let font = layout
        .line(0)
        .expect("a line")
        .runs()
        .into_iter()
        .next()
        .expect("a run")
        .item()
        .analysis()
        .font();
    let coverage = font.coverage(&pango::Language::from_string("zh-cn"));

    // A missing glyph would render as `.notdef` and every measurement above would
    // silently measure the wrong shape — the failure mode a subset font invites.
    for character in BLOCK
        .chars()
        .chain(EXIF_DATE.chars())
        .chain("2019 拍摄 <>#{}".chars())
        .chain("他他他说。他".chars())
        .chain("。，．：；？！）］｝〕〉》」』】〙〛’”（［｛〔〈《「『【〘〚“‘・ー…‥".chars())
        .chain(PARAGRAPHS.iter().flat_map(|paragraph| paragraph.chars()))
    {
        assert_ne!(
            coverage.get(character as i32),
            pango::CoverageLevel::None,
            "{character:?} (U+{:04X}) is not in {}: add it to pixlay-cli/tests/fixtures/fonts/generate.py and \
             regenerate",
            character as u32,
            fonts::FONT_FILE
        );
    }
}

/// Root mean square error over all channels.
fn rmse(a: &Rgb8Image, b: &Rgb8Image) -> f64 {
    assert_eq!((a.width, a.height), (b.width, b.height), "size mismatch");
    let mut sum = 0.0;
    for (left, right) in a.data.iter().zip(&b.data) {
        let difference = f64::from(*left) - f64::from(*right);
        sum += difference * difference;
    }
    (sum / a.data.len() as f64).sqrt()
}

/// Box-filter by 2 in both axes: the 2N render compared against the N one.
fn downsample2(image: &Rgb8Image) -> Rgb8Image {
    let (width, height) = (image.width / 2, image.height / 2);
    let mut data = vec![0u8; width as usize * height as usize * 3];
    for y in 0..height {
        for x in 0..width {
            for channel in 0..3 {
                let mut sum = 0u32;
                for dy in 0..2 {
                    for dx in 0..2 {
                        sum += u32::from(image.pixel(2 * x + dx, 2 * y + dy)[channel as usize]);
                    }
                }
                let target = (y as usize * width as usize + x as usize) * 3 + channel as usize;
                data[target] = (sum / 4) as u8;
            }
        }
    }
    Rgb8Image {
        width,
        height,
        data,
    }
}
