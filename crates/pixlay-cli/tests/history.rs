// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S6.5 through the real pipeline: decode, resample, `draw`.
//!
//! `pixlay-render/tests/history.rs` measures the same criterion at `draw`'s own
//! boundary, where the caller hands in the bitmaps: the canvas blits and clips,
//! and everything that changes the photo or its framing reaches it as a different
//! bitmap. This file closes that gap by running the same walk over the product's
//! pipeline — committed photos are decoded by the sandboxed decoder, cropped to
//! the cell's fitted region and resampled by the imaging crate, and composited by
//! `draw` — so a command's effect is measured where the product produces it.
//!
//! The commands are the two whose effect is a pixel at a fixed grid: `SetSource`,
//! which decides which photo a cell shows, and `SetCrop`, which decides the region
//! it shows. Each state is required to differ from the one before it, because a
//! command that changed no pixel would make the comparison vacuous. The template
//! and count commands are covered in `pixlay-core/tests/history.rs`, at the
//! document rather than the pixel boundary.

use std::path::{Path, PathBuf};

use pixlay_core::{CollageDoc, Command, CropTransform, History, PixelSize, templates};
use pixlay_imaging::SlotBitmap;
use pixlay_render::{Bitmap, Images, Rgb8Image, render_rgb8};

/// A small grid: several states are rendered, each one decoding two photos, and
/// the criterion is about equality of the two renders, not about resolution.
/// Long edge of the grid the undo tests render on, in pixels.
const LONG_EDGE: u32 = 454;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn document(photos: &[PathBuf]) -> CollageDoc {
    let template = templates::get(templates::SMOKE_TEMPLATE).expect("registered");
    let mut doc = CollageDoc::new(template);
    for (slot, photo) in photos.iter().enumerate() {
        doc.cells[slot].source = Some(photo.clone());
    }
    doc
}

/// The sources as `Cell::source` names them: absolute, so no project directory is
/// involved.
fn sources(doc: &CollageDoc) -> Vec<Option<PathBuf>> {
    doc.cells.iter().map(|cell| cell.source.clone()).collect()
}

fn bitmap(bitmap: &SlotBitmap) -> Bitmap {
    Bitmap::from_argb32_region(
        bitmap.width as i32,
        bitmap.height as i32,
        bitmap.origin,
        bitmap.display,
        bitmap.pixels.clone(),
    )
    .expect("bitmap")
}

/// One full render: decode every occupied cell, then draw.
fn render(doc: &CollageDoc) -> Rgb8Image {
    let canvas = PixelSize::for_long_edge(doc.template.aspect, LONG_EDGE).expect("canvas size");
    let images = pixlay_imaging::slot_bitmaps(doc, canvas, &sources(doc)).expect("decodes");
    let mut bitmaps = Images::new();
    for slot in &images {
        bitmaps.insert(slot.slot, bitmap(slot));
    }
    render_rgb8(doc, &bitmaps, canvas, 1.0, None).expect("renders")
}

/// Pixels that differ, for a failure message that says how much moved.
fn differing_pixels(a: &Rgb8Image, b: &Rgb8Image) -> usize {
    assert_eq!(
        (a.width, a.height),
        (b.width, b.height),
        "the two renders are not the same size"
    );
    a.data
        .as_chunks::<3>()
        .0
        .iter()
        .zip(b.data.as_chunks::<3>().0)
        .filter(|(left, right)| left != right)
        .count()
}

/// Whether two renders differ at all. A command that resizes the canvas changes
/// the grid itself, which counts as a change even though the pixels cannot be
/// compared one by one.
fn changed(a: &Rgb8Image, b: &Rgb8Image) -> bool {
    (a.width, a.height) != (b.width, b.height) || a.data != b.data
}

fn photo(slot: usize) -> PathBuf {
    fixture(if slot.is_multiple_of(2) {
        "photos/landscape.jpg"
    } else {
        "photos/portrait.jpg"
    })
}

/// Every command kind, in an order that stays valid.
fn sequence() -> Vec<Command> {
    vec![
        Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 1.5,
                offset: (0.3, -0.2),
                rotation_deg: 15.0,
            },
        },
        // Placing a photo in a slot that was empty, and emptying one that was not.
        Command::SetSource {
            slot: 2,
            source: Some(photo(2)),
        },
        Command::SetSource {
            slot: 1,
            source: None,
        },
        // Slot 2, which the previous command filled: framing an empty cell
        // moves no pixel, so the walk would prove nothing.
        Command::SetCrop {
            slot: 2,
            crop: CropTransform {
                zoom: 1.8,
                offset: (-0.2, 0.4),
                rotation_deg: -30.0,
            },
        },
    ]
}

#[test]
fn every_command_kind_survives_undo_through_the_real_pipeline() {
    let mut history = History::new(document(&[photo(0), photo(1)])).expect("a valid document");

    // The starting point: two photos decoded and drawn.
    let mut states = vec![render(history.doc())];
    let painted = (0..states[0].data.len() / 3)
        .filter(|index| states[0].data[index * 3..index * 3 + 3] != [255, 255, 255])
        .count();
    assert!(
        painted > 10_000,
        "the initial render has only {painted} non-white pixels: the photos did not decode"
    );

    for (index, command) in sequence().iter().enumerate() {
        history.apply(command.clone()).expect("applies");
        states.push(render(history.doc()));
        assert!(
            changed(&states[index], &states[index + 1]),
            "command {index} changed no pixel, so this walk would prove nothing"
        );
    }

    // Backwards to the initial pixels, one command at a time.
    for expected in states.iter().rev().skip(1) {
        assert!(history.undo(), "there is a step to undo");
        let image = render(history.doc());
        assert_eq!(
            differing_pixels(&image, expected),
            0,
            "undo did not reproduce the pixels of the state it went back to"
        );
    }
    assert!(!history.can_undo());

    // Forwards through every state again.
    for (index, expected) in states.iter().enumerate().skip(1) {
        assert!(history.redo(), "step {index} is redoable");
        let image = render(history.doc());
        assert_eq!(
            differing_pixels(&image, expected),
            0,
            "redo did not reproduce the pixels of state {index}"
        );
    }
    assert!(!history.can_redo());
    assert_eq!(history.undo_depth(), sequence().len());
}
