//! S6.5 at the pixel boundary: what undo and redo do to the finished image.
//!
//! The criterion is pixel-identical, so this file renders. A document is put
//! through a sequence of commands — the ones whose effect *is* `draw`'s: a photo
//! placed in a slot and how it is framed — and every state it passes through is
//! rendered once. Undoing the whole sequence has to reproduce the initial image
//! exactly, redo has to reproduce every intermediate one, and a mixed walk has to
//! follow the document rather than the stack's shape.
//!
//! Each state is also required to *differ* from the one before it: a command that
//! changed no pixel would make the whole comparison vacuous, which is exactly the
//! failure mode this test has to be able to catch.
use pixlay_core::{CollageDoc, Command, CropTransform, History, PixelSize, Rgba8, templates};
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

/// Long edge of the grid the history tests render on, in pixels.
const LONG_EDGE: u32 = 454;

/// One flat colour per slot. The photo a cell points at decides *which* colour it
/// gets, so placing a photo changes the pixels the way it does in the product: the
/// caller hands `draw` the bitmap it decoded for that cell.
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

/// The colour a cell shows.
///
/// This test plays the decoder: it hands `draw` a flat bitmap for every cell. A
/// cell that names a photo gets a different colour from an empty one, so that
/// "which photo is in this slot" — the thing `SetSource` changes — is visible in
/// the pixels instead of being invisible to the comparison.
fn color_of(slot: usize, source: Option<&std::path::Path>) -> Rgba8 {
    let base = COLORS[slot % COLORS.len()];
    match source {
        None => base,
        Some(_) => Rgba8::rgb(255 - base.r, 255 - base.g, 255 - base.b),
    }
}

fn doc() -> CollageDoc {
    let template = templates::get(templates::SMOKE_TEMPLATE).expect("registered");
    CollageDoc::new(template)
}

fn canvas_px(doc: &CollageDoc) -> PixelSize {
    PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size")
}

/// One flat bitmap per occupied cell, at the size the slot displays it (the fit's
/// zoom, which is what S4's decoder sizes from).
fn images(doc: &CollageDoc, canvas: PixelSize) -> Images {
    let mut images = Images::new();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        let color = color_of(index, doc.cells[index].source.as_deref());
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
                color,
            )
            .expect("bitmap"),
        );
    }
    images
}

fn render(history: &History) -> Rgb8Image {
    let doc = history.doc();
    let canvas = canvas_px(doc);
    render_rgb8(doc, &images(doc, canvas), canvas, 1.0, None).expect("renders")
}

/// The number of bytes two renders differ in, for a readable failure.
fn differing_pixels(a: &Rgb8Image, b: &Rgb8Image) -> usize {
    assert_eq!((a.width, a.height), (b.width, b.height), "size changed");
    a.data
        .as_chunks::<3>()
        .0
        .iter()
        .zip(b.data.as_chunks::<3>().0)
        .filter(|(left, right)| left != right)
        .count()
}

/// The commands the walk runs.
///
/// Only the two kinds whose effect *is* `draw`'s: which photo a cell shows (the
/// caller hands in the bitmap it decoded, and this test's decoder answers by
/// colour) and how it is framed. Three of the sequence's five commands are
/// framings, because a framing is the edit that moves pixels without changing the
/// document's shape.
///
/// What is deliberately not here: the layout and count commands (`SetTemplate`,
/// `AddCell`, `RemoveLastCell`, `AddPhotos`), which change the pixel grid the
/// comparison counts on rather than the pixels at a fixed grid, and the other cell
/// edits (`SwapCells`, `ClearCell`), whose effect is the two kinds above composed.
fn sequence() -> Vec<Command> {
    vec![
        Command::SetSource {
            slot: 2,
            source: Some(std::path::PathBuf::from("photos/a.jpg")),
        },
        Command::SetCrop {
            slot: 2,
            crop: CropTransform {
                zoom: 1.6,
                offset: (0.25, -0.4),
                rotation_deg: 12.0,
            },
        },
        Command::SetSource {
            slot: 5,
            source: Some(std::path::PathBuf::from("photos/b.jpg")),
        },
        Command::SetCrop {
            slot: 6,
            crop: CropTransform {
                zoom: 2.2,
                offset: (-0.5, 0.5),
                rotation_deg: -30.0,
            },
        },
        Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 2.0,
                offset: (0.7, -0.6),
                rotation_deg: 0.0,
            },
        },
    ]
}

#[test]
fn undoing_every_command_restores_the_pixels_and_redo_is_isomorphic() {
    let mut history = History::new(doc()).expect("a valid document");

    // Every state the document passes through, rendered once.
    let mut states = vec![render(&history)];
    for (index, command) in sequence().iter().enumerate() {
        history.apply(command.clone()).expect("applies");
        states.push(render(&history));
        assert!(
            differing_pixels(&states[index], &states[index + 1]) > 0,
            "command {index} changed no pixel, so the walk below would prove nothing"
        );
    }

    // Backwards: the exact initial image, and then every intermediate one.
    for expected in states.iter().rev().skip(1) {
        assert!(history.undo(), "there is a step to undo");
        let image = render(&history);
        assert_eq!(
            differing_pixels(&image, expected),
            0,
            "undo did not restore the pixels"
        );
    }
    assert!(!history.can_undo());

    // Forwards: every state again, in order.
    for (index, expected) in states.iter().enumerate().skip(1) {
        assert!(history.redo(), "step {index} is redoable");
        let image = render(&history);
        assert_eq!(
            differing_pixels(&image, expected),
            0,
            "redo did not restore the pixels of state {index}"
        );
    }
    assert!(!history.can_redo());
}

#[test]
fn a_mixed_walk_follows_the_document_not_the_stack() {
    // Undo, undo, redo, undo, redo, redo: the pixels have to match the document the
    // stacks describe at every point, whatever the order the user pressed things.
    let mut history = History::new(doc()).expect("a valid document");
    let initial = render(&history);

    history.apply(sequence()[0].clone()).expect("applies");
    let after_first = render(&history);
    history.apply(sequence()[1].clone()).expect("applies");
    let after_second = render(&history);

    assert!(history.undo());
    assert_eq!(differing_pixels(&render(&history), &after_first), 0);
    assert!(history.undo());
    assert_eq!(differing_pixels(&render(&history), &initial), 0);
    assert!(history.redo());
    assert_eq!(differing_pixels(&render(&history), &after_first), 0);
    assert!(history.undo());
    assert_eq!(differing_pixels(&render(&history), &initial), 0);
    assert!(history.redo());
    assert!(history.redo());
    assert_eq!(differing_pixels(&render(&history), &after_second), 0);
    assert_eq!(history.undo_depth(), 2);
    assert_eq!(history.redo_depth(), 0);
}
