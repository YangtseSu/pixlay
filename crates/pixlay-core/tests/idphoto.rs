// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The ID photo sheet's own rules (S40).
//!
//! A sheet holds **one photo in one cell** and its template's slots are the copies the
//! sheet prints, so the model, the validation and the command layer all answer one
//! question differently from a collage: *which cell does this slot draw*, and *what does
//! an edit that names a slot do*. These are the rules the renderer, the window and the
//! CLI then share, so they are checked here where a document can be built in one line.

use std::path::PathBuf;

use pixlay_core::{Cell, CollageDoc, Command, CoreError, CropTransform, History, templates};

/// The sheet most of this file uses: 一寸, twelve copies.
const SHEET: &str = "id-1cun-12-2x3";

/// The one-slot collage a sheet is entered from and left for.
const COLLAGE: &str = "grid-1-1x1-3x4";

fn sheet() -> CollageDoc {
    let template = templates::get(SHEET).expect("the ID photo sheet ships");
    assert!(template.one_photo, "{SHEET} is a sheet");
    CollageDoc::new(template)
}

fn collage(name: &str) -> CollageDoc {
    CollageDoc::new(templates::get(name).expect("the layout ships"))
}

/// The document after one command, validated — the path `History::apply` takes.
fn applied(doc: &CollageDoc, command: Command) -> Result<CollageDoc, CoreError> {
    command.applied_to(doc)
}

/// The document's one cell.
fn cell(doc: &CollageDoc) -> &Cell {
    assert_eq!(doc.cells.len(), 1, "a sheet holds one cell");
    &doc.cells[0]
}

#[test]
fn a_sheet_has_one_cell_and_every_slot_draws_it() {
    let doc = sheet();
    assert_eq!(doc.cells.len(), 1);
    assert_eq!(doc.template.slots.len(), 12);
    doc.validate().expect("a fresh sheet validates");

    // The mapping every slot-indexed reader asks for: slots are the copies, cells the
    // photo.
    for slot in 0..doc.template.slots.len() {
        assert_eq!(doc.cell_index(slot), Some(0), "slot {slot}");
        assert!(doc.cell(slot).is_some(), "slot {slot}");
    }
    assert_eq!(doc.cell_index(doc.template.slots.len()), None);
    assert!(doc.cell(doc.template.slots.len()).is_none());

    // A collage layout keeps the identity mapping, so nothing about the old documents
    // moved.
    let two = collage("grid-2-2x1");
    for slot in 0..two.template.slots.len() {
        assert_eq!(two.cell_index(slot), Some(slot));
    }
}

#[test]
fn a_document_that_is_not_one_cell_is_refused() {
    // Two cells.
    let mut doc = sheet();
    doc.cells.push(Cell::default());
    let error = doc.validate().expect_err("two cells is not a sheet");
    assert!(
        matches!(error, CoreError::OnePhotoDocument { cells: 2, kept: 0 }),
        "{error}"
    );

    // One cell and a kept one: nowhere on a sheet for it to wait.
    let mut doc = sheet();
    doc.kept.push(Cell::default());
    let error = doc.validate().expect_err("a sheet keeps none");
    assert!(
        matches!(error, CoreError::OnePhotoDocument { cells: 1, kept: 1 }),
        "{error}"
    );

    // And a sheet's name is the library's own: `one_photo` is what makes the rules
    // apply, not the name.
    let mut doc = sheet();
    doc.template.one_photo = false;
    let error = doc.validate().expect_err("one cell, twelve slots");
    assert!(
        matches!(
            error,
            CoreError::CellCount {
                cells: 1,
                slots: 12
            }
        ),
        "{error}"
    );
}

#[test]
fn a_per_slot_edit_reaches_the_one_cell() {
    let doc = sheet();
    let photo = PathBuf::from("portrait.png");
    let framing = CropTransform {
        zoom: 2.0,
        offset: (0.1, -0.2),
        rotation_deg: 7.0,
    };

    // Any of the twelve tiles names the sheet's cell, which is what makes "frame this
    // tile" and "frame the sheet" one command.
    let doc = applied(
        &doc,
        Command::SetSource {
            slot: 7,
            source: Some(photo.clone()),
        },
    )
    .expect("slot 7 is a tile of the sheet");
    assert_eq!(cell(&doc).source.as_deref(), Some(photo.as_path()));

    let doc = applied(
        &doc,
        Command::SetCrop {
            slot: 3,
            crop: framing,
        },
    )
    .expect("the framing lands");
    assert_eq!(cell(&doc).crop, framing.normalized());

    let doc = applied(&doc, Command::ClearCell { slot: 11 }).expect("the clear lands");
    assert_eq!(doc.cells[0], Cell::default());

    // A slot the template does not have is still refused, because the caller named a
    // tile and the sheet has twelve.
    let error = applied(
        &doc,
        Command::SetSource {
            slot: 12,
            source: Some(photo),
        },
    )
    .expect_err("there is no tile 12");
    assert!(
        matches!(
            error,
            CoreError::NoSuchSlot {
                slot: 12,
                slots: 12
            }
        ),
        "{error}"
    );
}

#[test]
fn one_photo_is_what_the_sheet_accepts() {
    let doc = sheet();
    let one = PathBuf::from("portrait.png");
    let other = PathBuf::from("other.png");

    // One photo: it is the sheet's, whatever cell it was aimed at.
    let placed = applied(
        &doc,
        Command::PlacePhotos {
            places: vec![(9, one.clone())],
        },
    )
    .expect("one photo lands");
    assert_eq!(cell(&placed).source.as_deref(), Some(one.as_path()));

    // The same photo named twice is still one photo.
    let same = applied(
        &doc,
        Command::PlacePhotos {
            places: vec![(0, one.clone()), (11, one.clone())],
        },
    )
    .expect("two names, one photo");
    assert_eq!(cell(&same).source.as_deref(), Some(one.as_path()));

    // Two photos are refused: the sheet has nowhere to put the second.
    let error = applied(
        &doc,
        Command::PlacePhotos {
            places: vec![(0, one.clone()), (1, other.clone())],
        },
    )
    .expect_err("two photos do not fit");
    assert!(
        matches!(error, CoreError::OnePhotoPhotos { found: 2 }),
        "{error}"
    );

    let error = applied(
        &doc,
        Command::AddPhotos {
            photos: vec![one.clone(), other.clone()],
        },
    )
    .expect_err("two photos do not fit");
    assert!(
        matches!(error, CoreError::OnePhotoPhotos { found: 2 }),
        "{error}"
    );

    // One photo *replaces* (S40's own decision: a sheet has no empty cell to append
    // to, and the window's single-photo choice is exactly this edit).
    let replaced = applied(
        &placed,
        Command::AddPhotos {
            photos: vec![other.clone()],
        },
    )
    .expect("one photo replaces");
    assert_eq!(cell(&replaced).source.as_deref(), Some(other.as_path()));
}

#[test]
fn the_commands_that_need_a_second_cell_are_refused() {
    let doc = sheet();
    // Neither count-control edit is here: `+` leaves the sheet for a collage and `−`
    // is answered by the floor — `the_count_controls_plus_leaves_the_sheet` walks both.
    // What is left are the two edits that would index a one-cell list by slot.
    let cases: [(&str, Command); 2] = [
        (
            "move its photo between cells",
            Command::MovePhoto { from: 0, to: 5 },
        ),
        ("swap two cells", Command::SwapCells { left: 0, right: 5 }),
    ];
    for (what, command) in cases {
        let error = applied(&doc, command).expect_err("a sheet has one cell");
        match error {
            CoreError::OnePhotoCommand { what: got } => assert_eq!(got, what),
            other => panic!("{what}: {other}"),
        }
    }
    // A collage still takes them, so the refusal is the sheet's and not a door that
    // closed for the product.
    let two = collage("grid-2-2x1");
    History::new(two.clone())
        .expect("the collage is valid")
        .apply(Command::SwapCells { left: 0, right: 1 })
        .expect("a collage swaps");
}

#[test]
fn the_count_controls_plus_leaves_the_sheet() {
    // 「加1,就自动退出，到别的模式」 (the human's ruling of 2026-10-17): `+` on a sheet
    // is the count rule's own growth — two cells, at the sheet's shape — and the photo
    // comes with it, in cell 0. The minus side is the floor and stays refused.
    let photo = PathBuf::from("portrait.png");
    let sheet_doc = applied(
        &sheet(),
        Command::SetSource {
            slot: 0,
            source: Some(photo.clone()),
        },
    )
    .expect("the sheet takes a photo");
    assert_eq!(sheet_doc.cells.len(), 1);

    let two = applied(&sheet_doc, Command::AddCell).expect("+ leaves the sheet");
    assert_eq!(two.cells.len(), 2, "two cells");
    assert!(!two.template.one_photo, "a collage, not a sheet");
    assert_eq!(
        two.template.aspect, sheet_doc.template.aspect,
        "the count rule keeps the page's shape",
    );
    assert_eq!(
        two.cells[0].source.as_deref(),
        Some(photo.as_path()),
        "the sheet's photo is cell 0"
    );
    assert_eq!(two.cells[1], Cell::default(), "and the new cell is empty");
    two.validate().expect("the collage is a valid document");

    let error = applied(&sheet_doc, Command::RemoveLastCell).expect_err("one cell is the floor");
    assert!(
        matches!(error, CoreError::TooFewCells { min: 1 }),
        "{error}"
    );
}

#[test]
fn a_layout_switch_onto_a_sheet_needs_a_one_cell_document() {
    let sheet_template = templates::get(SHEET).expect("the sheet ships");
    let photo = PathBuf::from("portrait.png");

    // From a one-photo collage: the photo comes with it, into the sheet's one cell.
    let one = applied(
        &collage(COLLAGE),
        Command::SetSource {
            slot: 0,
            source: Some(photo.clone()),
        },
    )
    .expect("the photo lands");
    let on_sheet = applied(
        &one,
        Command::SetTemplate {
            template: sheet_template.clone(),
        },
    )
    .expect("one photo fits a sheet");
    assert_eq!(on_sheet.cells.len(), 1);
    assert_eq!(cell(&on_sheet).source.as_deref(), Some(photo.as_path()));
    on_sheet.validate().expect("the switched document is valid");

    // From two photos: refused, and the message says what to do instead.
    let two = applied(&one, Command::AddCell).expect("the collage grows");
    assert_eq!(two.cells.len(), 2);
    let error = applied(
        &two,
        Command::SetTemplate {
            template: sheet_template.clone(),
        },
    )
    .expect_err("two cells do not fit a sheet");
    assert!(
        matches!(error, CoreError::OnePhotoLayout { cells: 2, kept: 0 }),
        "{error}"
    );
    assert!(error.to_string().contains("one photo"), "{error}");

    // From a document that keeps a cell, even at one cell on the sheet.
    let mut keeps = one.clone();
    keeps.kept.push(Cell::default());
    let error = applied(
        &keeps,
        Command::SetTemplate {
            template: sheet_template,
        },
    )
    .expect_err("a kept cell has nowhere to wait");
    assert!(
        matches!(error, CoreError::OnePhotoLayout { cells: 1, kept: 1 }),
        "{error}"
    );

    // And back off the sheet: the ordinary retention rule, the photo in cell 0.
    let back = applied(
        &on_sheet,
        Command::SetTemplate {
            template: templates::get("grid-2-2x1").expect("the layout ships"),
        },
    )
    .expect("a collage takes the photo back");
    assert_eq!(back.cells.len(), 2);
    assert_eq!(back.cells[0].source.as_deref(), Some(photo.as_path()));
    assert_eq!(back.cells[1], Cell::default());
    back.validate().expect("the collage is valid");
}

#[test]
fn the_fit_of_a_copy_is_its_own_slot_with_the_cells_crop() {
    // `fitted_crop` is the one place the frame and the clamp meet, and a copy's fit is
    // computed against **that copy's** geometry with the sheet's one crop: that is what
    // makes the same photo cover twelve congruent cells.
    let mut doc = sheet();
    let framing = CropTransform {
        zoom: 1.0,
        offset: (0.2, -0.1),
        rotation_deg: 12.0,
    };
    doc.cells[0] = Cell {
        source: Some(PathBuf::from("portrait.png")),
        crop: framing,
    };
    doc.validate().expect("the sheet is valid");
    let aspect = doc.template.aspect;
    for slot in [0, 5, 11] {
        let fitted = doc.fitted_crop(slot, aspect, 0.75).expect("the copy fits");
        let expected = doc
            .fit_crop(slot, framing, aspect, 0.75)
            .expect("the same reference");
        assert_eq!(fitted.transform, expected.transform, "slot {slot}");
    }
}

#[test]
fn a_sheet_round_trips_through_its_own_json() {
    let doc = sheet();
    let json = doc.to_json().expect("serializes");
    // The field is written **only** when it is true, which is what keeps a pre-S40
    // project byte-identical.
    assert!(json.contains("\"onePhoto\": true"), "{json}");
    let back = CollageDoc::from_json(&json).expect("reads back");
    assert_eq!(back, doc);

    let collage_json = collage(COLLAGE).to_json().expect("serializes");
    assert!(!collage_json.contains("onePhoto"), "{collage_json}");
}
