//! S28's exit criterion through the window's own controls: a layout change keeps
//! every photo it takes off the sheet.
//!
//! Finding 3 of the human's walk of 2026-09-26 ("three photos, `−` to a two-cell
//! layout, `+` back to three cells: the last photo is gone. It must stay — in the
//! queue") and ruling 43: a photo leaves the collage only when it is **deleted**, so
//! `−` and a smaller template park the cell they cannot place, and the next growth
//! places it again — photo, framing and order. What a delete is, by contrast, is the
//! last section: `Delete` (the strip's *Clear the cell*) loses the photo and keeps
//! nothing.
//!
//! **One `#[test]` because GTK lives on one thread** (see `support`): the harness
//! initialises GTK once, in the single worker thread this binary's one test runs on.

mod support;

use std::path::Path;

use gtk4::prelude::*;
use pixlay_core::{CollageDoc, CropTransform, templates};

/// The window's own report of a document it shows: the same calls the canvas makes.
fn settle(window: &pixlay::EditorWindow) {
    assert!(
        window.wait_for_idle(support::WAIT),
        "the canvas's decode finished"
    );
    assert!(window.wait_for_gallery(support::WAIT), "the band was built");
}

/// A document with a photo in every cell of `template` — the state a user is in
/// after filling a layout.
fn document_on(template: &str) -> CollageDoc {
    let template = templates::get(template).unwrap_or_else(|| panic!("template {template}"));
    let mut doc = CollageDoc::new(template);
    for (slot, cell) in doc.cells.iter_mut().enumerate() {
        cell.source = Some(support::photo(if slot % 2 == 0 {
            "landscape.jpg"
        } else {
            "portrait.jpg"
        }));
    }
    doc
}

/// The file name a cell's `source` names, as the `+`'s hint spells it.
fn photo_name(cell: &pixlay_core::Cell) -> String {
    cell.source
        .as_deref()
        .and_then(Path::file_name)
        .expect("the cell holds a photo")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn a_layout_change_keeps_the_photos_it_takes_off_the_sheet() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let gallery = window.gallery().expect("the editor has a layout band");

    // The walk's own document: three photos, one per cell, the last one framed so
    // "comes back whole" means the photograph *and* the numbers it was framed with.
    let mut three = document_on("mosaic-3-hero");
    three.cells[2].crop = CropTransform {
        zoom: 1.7,
        offset: (-0.1, 0.2),
        rotation_deg: -6.0,
    };
    let before = three.clone();
    window.open_document(three);
    settle(&window);
    assert_eq!(gallery.count_label().label(), "3");

    // `−`: the cell leaves the sheet whole, and the window says so once.
    let toasts = window.toasts();
    window.remove_photo();
    settle(&window);
    let shrunk = window.document();
    assert_eq!(shrunk.cells.len(), 2, "the layout gave up one cell");
    assert_eq!(
        shrunk.kept,
        vec![before.cells[2].clone()],
        "and the cell that held the third photo is kept whole"
    );
    assert_eq!(window.toasts(), toasts + 1, "reported once");
    let report = window.last_toast().expect("the shrink reported itself");
    assert!(
        report.contains("kept"),
        "the report reads as kept, not lost: {report:?}"
    );
    assert_eq!(window.photo_count(), 2, "the photo is off the sheet");
    assert_eq!(window.kept_count(), 1, "and the document still holds it");
    let waiting = photo_name(&shrunk.kept[0]);
    let hint = gallery
        .plus_button()
        .tooltip_text()
        .expect("the `+` carries a tooltip");
    assert!(
        hint.contains(&waiting),
        "the `+`'s hint names {waiting:?} while it waits: {hint:?}"
    );

    // `+`: the photo is back in its own cell, with its framing. The *layout* follows
    // the count rule (`layout_for`, S14b) — there is no 4:3 two-cell layout in the
    // library, so this three-cell 4:3 document comes back on a 16:9 three-cell strip
    // — and that is the count rule's own answer, not the retention's. What the
    // retention promises is the cells, and the section below is the exact inverse.
    window.add_photo();
    settle(&window);
    assert_eq!(
        window.document().cells,
        before.cells,
        "cell for cell, framing for framing"
    );
    assert!(
        window.document().kept.is_empty(),
        "and nothing is waiting any more"
    );
    assert_eq!(window.photo_count(), 3, "every photo is back on the sheet");
    let after = gallery
        .plus_button()
        .tooltip_text()
        .expect("the `+` keeps its tooltip");
    assert!(
        !after.contains(&waiting),
        "the hint no longer names the photo: {after:?}"
    );

    // The two steps are one undo each — the layout change on both sides — so the
    // history is still what restores the document *exactly*, template included.
    window.undo();
    assert_eq!(
        window.document().cells.len(),
        2,
        "undo takes the cell off again"
    );
    assert_eq!(
        window.document().kept.len(),
        1,
        "keeping it, exactly as the shrink did"
    );
    window.undo();
    assert_eq!(
        window.document(),
        before,
        "and the second undo is the shrink's own inverse"
    );

    // Where the counts share an aspect, `+` restores the layout too: `mosaic-4-hero`
    // and `mosaic-3-hero` are both 4:3, so this pair is the one whose round trip is
    // the document itself.
    let mut four = document_on("mosaic-4-hero");
    four.cells[3].crop = CropTransform {
        zoom: 1.3,
        offset: (0.15, -0.05),
        rotation_deg: 11.0,
    };
    let four_before = four.clone();
    window.open_document(four);
    settle(&window);
    window.remove_photo();
    settle(&window);
    assert_eq!(window.photo_count(), 3);
    assert_eq!(window.kept_count(), 1);
    window.add_photo();
    settle(&window);
    assert_eq!(
        window.document(),
        four_before,
        "the shrink and the growth are inverses, layout included"
    );

    // A delete is the other half of the rule, and a different control: `Delete` on a
    // cell loses its photo, and no later growth brings it back.
    window.clear_cell(3);
    settle(&window);
    assert!(window.document().cells[3].source.is_none());
    assert!(window.document().kept.is_empty(), "a delete keeps nothing");
    assert_eq!(window.photo_count(), 3);
    window.remove_photo();
    settle(&window);
    window.add_photo();
    settle(&window);
    assert_eq!(
        window.photo_count(),
        3,
        "the deleted photo did not come back with the layout"
    );
    assert_eq!(window.document().cells.len(), 4, "four cells again");
    assert!(window.document().cells[3].source.is_none());
    assert!(
        window.document().kept.is_empty(),
        "and nothing was kept: the deleted cell was empty"
    );
}
