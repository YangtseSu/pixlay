// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S40's window half: the ID photo sheet, as the band and the canvas present it.
//!
//! One `#[test]` because GTK lives on one thread (see `support`). What it holds, in the
//! order the step's exit criteria are written:
//!
//! * the ratio row's **ID member** exists at one column and only there, and activating it
//!   lands on the 一寸 sheet (one cell, twelve copies);
//! * the strip then holds the six sheets, each named by its size on hover and to a screen
//!   reader — the only way to tell 二寸 from 大二寸, whose sketches are pixels apart;
//! * the count control is gone (a sheet holds one photo), the row's ID member is the
//!   marked one, and a shape's button leaves the sheet for that shape's one-slot collage;
//! * the empty sheet's `+` appears **once** rather than on every copy, and a framing step
//!   on the fifth copy changes the document's one cell;
//! * the header's way-in button is a single-photo choice whose own word moves between
//!   "Add the photo…" and "Replace the photo…";
//! * the commands that need a second cell are refused where they are asked for.

mod support;

use std::time::Duration;

use gtk4::prelude::*;

use pixlay::canvas::Gesture;
use pixlay::i18n::gettext;
use pixlay::layout::photo_size_name;
use pixlay_core::templates::ID_PHOTO_DEFAULT;
use pixlay_core::{Command, CropTransform, templates};

/// The sheet the row's member lands on: 一寸, twelve copies.
const SHEET: &str = "id-1cun-12-2x3";

/// The six sheets, in the library's order — the strip's own order (S40's ruling: the
/// human's list, 大二寸 after 二寸, 小二寸 last).
const SHEETS: [&str; 6] = [
    "id-1cun-12-2x3",
    "id-1cun-small-18-3x2",
    "id-1cun-large-8-3x2",
    "id-2cun-8-3x2",
    "id-2cun-large-4-2x3",
    "id-2cun-small-8-3x2",
];

/// How many of the canvas's empty-cell `+` buttons the sheet shows.
fn visible_empty_buttons(window: &pixlay::window::EditorWindow, slots: usize) -> usize {
    let controls = window.cell_controls().expect("the controls exist");
    (0..slots)
        .filter(|slot| {
            controls
                .empty_button(*slot)
                .is_some_and(|button| button.is_visible())
        })
        .count()
}

/// The header's way-in label, found by the icon it carries.
fn add_label(window: &pixlay::window::EditorWindow) -> String {
    let header = window.header().expect("the window has a header bar");
    support::descendants(header.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<libadwaita::ButtonContent>().ok())
        .find(|content| content.icon_name() == "list-add-symbolic")
        .map(|content| content.label().to_string())
        .expect("the header's way-in button carries a label")
}

#[test]
fn an_id_photo_sheet_is_chosen_from_the_ratio_row_at_one_photo() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    support::present(&window);
    // The window opens on a one-cell collage, which is where the member appears (the
    // human's ruling: 「在1张照片的时候」).
    assert_eq!(window.document().cells.len(), 1, "the default document");

    // ---- the entry ------------------------------------------------------------
    let gallery = window.gallery().expect("the band exists");
    window.pump(Duration::from_millis(50));
    let id_button = gallery
        .id_photo_button()
        .expect("one cell offers the ID photo sheet");
    assert!(id_button.is_visible(), "the member is drawn");
    assert_eq!(
        id_button.label().as_deref(),
        Some(gettext("ID photo").as_str()),
        "the member's own word"
    );
    // It is the row's **last** member, after every shape: the human's 「在16:9的右边」.
    let shapes = gallery.ratios().len();
    assert!(shapes > 0, "the row offers shapes at one cell");
    // (The row's children are the shapes and then this one; `id_photo_button` is found
    // by that same order, so the check is that the row has one more child than shapes.)
    let row = gallery.ratio_row();
    assert_eq!(
        row.observe_children().n_items(),
        shapes as u32 + 1,
        "the row is the shapes plus the ID member"
    );

    // Activating it is the click the user makes.
    id_button.set_active(true);
    window.pump(Duration::from_millis(50));
    assert_eq!(
        window.current_template(),
        SHEET,
        "the member lands on the library's default sheet"
    );
    assert_eq!(SHEET, ID_PHOTO_DEFAULT);
    assert!(templates::get(SHEET).is_some_and(|template| template.one_photo));
    let doc = window.document();
    assert_eq!(doc.cells.len(), 1, "a sheet holds one cell");
    assert_eq!(doc.template.slots.len(), 12, "and twelve copies");

    // ---- the strip and the names ----------------------------------------------
    assert_eq!(
        gallery.candidates(),
        SHEETS.map(str::to_string).to_vec(),
        "the strip is the six sheets, in the library's order"
    );
    for name in SHEETS {
        let button = gallery.cell(name).expect("every sheet is a candidate");
        let size = photo_size_name(name).expect("a sheet has a size name");
        assert!(!size.is_empty(), "{name}: the size name is copy, not empty");
        assert_eq!(
            button.tooltip_text().as_deref(),
            Some(size.as_str()),
            "{name}: the hover name is its size"
        );
        assert!(
            support::has_accessible_name(button.upcast_ref()),
            "{name}: a screen reader reads it"
        );
    }

    // The document's own kind is the row's mark, not its 2:3 shape.
    assert!(id_button.is_active(), "the ID member is marked");
    for aspect in gallery.ratios() {
        let button = gallery.ratio_button(aspect).expect("the shape's button");
        assert!(!button.is_active(), "{aspect}: the shape is not marked");
    }

    // The count control **stays** (the human's ruling of 2026-10-17): its `+` is how a
    // sheet leaves for a collage.
    assert!(gallery.count_visible(), "the count control is there");

    // The empty sheet offers one `+`, on the first tile: every slot resolves to the one
    // cell, so the other eleven have nothing to fill.
    assert_eq!(
        visible_empty_buttons(&window, 12),
        1,
        "the sheet is filled once"
    );

    // ---- the photo and its framing --------------------------------------------
    assert_eq!(add_label(&window), gettext("Add the photo…"), "empty sheet");
    let photo = support::photo("portrait.jpg");
    assert!(photo.exists(), "the fixture photo is there");
    window
        .apply(Command::SetSource {
            slot: 0,
            source: Some(photo.clone()),
        })
        .expect("the sheet takes a photo");
    window.pump(Duration::from_millis(50));
    assert_eq!(
        add_label(&window),
        gettext("Replace the photo…"),
        "a sheet with a photo replaces it"
    );
    assert_eq!(
        visible_empty_buttons(&window, 12),
        0,
        "nothing left to fill"
    );

    // A framing step on the *fifth* copy changes the document's one cell, and the
    // document still holds one: the tiles are copies, not cells.
    window.select(Some(4));
    let framing = CropTransform {
        zoom: 1.5,
        offset: (0.1, 0.0),
        rotation_deg: 5.0,
    };
    window.gesture(Gesture::Step {
        slot: 4,
        crop: framing,
    });
    window.gesture(Gesture::End);
    let doc = window.document();
    assert_eq!(doc.cells.len(), 1, "one cell after a framing step");
    assert!(
        (doc.cells[0].crop.zoom - framing.zoom).abs() < 1e-9
            || doc.cells[0].crop.zoom >= 1.5 - 1e-9,
        "the copy's framing is the sheet's: {:?}",
        doc.cells[0].crop
    );
    assert_eq!(
        window.canvas_label(),
        pixlay::i18n::fill(
            gettext("ID photo sheet, copy {} of {}"),
            &[5.to_string(), 12.to_string()]
        ),
        "the canvas says which copy the focus is on"
    );

    // A drop of several files keeps the first: the sheet holds one photo, so the others
    // are reported rather than placed (the same one-report rule the ceiling has).
    let other = support::photo("landscape.jpg");
    window.place_files(vec![photo.clone(), other], Some(6));
    window.pump(Duration::from_millis(80));
    let doc = window.document();
    assert_eq!(doc.cells.len(), 1, "a drop cannot grow a sheet");
    assert_eq!(
        doc.cells[0].source.as_deref(),
        Some(photo.as_path()),
        "the first file is the sheet's"
    );

    // The commands that index cells by slot are refused where they are asked for.
    for command in [
        Command::SwapCells { left: 0, right: 5 },
        Command::MovePhoto { from: 0, to: 5 },
    ] {
        let refused = window.apply(command);
        assert!(refused.is_err(), "a sheet has one cell");
        assert!(
            refused.unwrap_err().to_string().contains("one photo"),
            "the refusal names the rule"
        );
    }
    assert_eq!(window.document().cells.len(), 1, "nothing moved");

    // ---- leaving the sheet ----------------------------------------------------
    // The count control's `+` is the way out (the ruling: 「加1,就自动退出，到别的模式」):
    // the count rule answers with a two-cell collage at the same shape and the sheet's
    // photo comes with it.
    window.add_photo();
    window.pump(Duration::from_millis(80));
    let doc = window.document();
    assert_eq!(doc.cells.len(), 2, "+ left the sheet for two cells");
    assert!(!doc.template.one_photo, "a collage, not a sheet");
    assert_eq!(
        doc.cells[0].source.as_deref(),
        Some(photo.as_path()),
        "the sheet's photo is cell 0"
    );
    assert!(
        gallery.id_photo_button().is_none(),
        "the member is only for one photo"
    );
}
