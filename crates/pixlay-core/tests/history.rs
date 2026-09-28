// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S6.5: the command history, as tests.
//!
//! Two properties, and they are the step's exit criteria in miniature: a sequence
//! of commands can be walked backwards to the exact document it started from and
//! forwards again to every state it passed through, and a command that would leave
//! the document outside the contract is refused *without* leaving a trace. The
//! pixel-identity half of the first one is `pixlay-render/tests/history.rs`; what
//! is measured here is the document itself, which is the stronger statement —
//! identical documents render identically by construction.

use std::path::{Path, PathBuf};

use pixlay_core::{
    Cell, CollageDoc, Command, CoreError, CropTransform, Frame, History, Rgba8, templates,
};

fn document() -> CollageDoc {
    let template = templates::get(templates::SMOKE_TEMPLATE).expect("registered");
    CollageDoc::new(template)
}

/// A document on `template` with a source in every cell — the state S14's count
/// control starts from.
fn occupied(template: &str) -> CollageDoc {
    let template = templates::get(template).unwrap_or_else(|| panic!("template {template}"));
    let mut doc = CollageDoc::new(template);
    for (index, cell) in doc.cells.iter_mut().enumerate() {
        cell.source = Some(PathBuf::from(format!("photos/cell{index}.jpg")));
    }
    doc
}

/// A template the library never had: ten slots, one past `MAX_SLOTS` since S12c
/// removed the ten-slot recipe.
fn ten_slot_template() -> pixlay_core::Template {
    let mut template = templates::get(templates::SMOKE_TEMPLATE).expect("registered");
    template.name = "ten-slot-wish".to_string();
    while template.slots.len() < 10 {
        template.slots.push(pixlay_core::Slot {
            outline: pixlay_core::Polygon::rect(0.05, 0.05, 0.95, 0.95),
            area: 0.81,
        });
    }
    template
}

/// One command of every kind this build has, in an order that stays valid.
fn sequence() -> Vec<Command> {
    vec![
        Command::SetSource {
            slot: 0,
            source: Some(PathBuf::from("photos/a.jpg")),
        },
        Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 1.4,
                offset: (0.2, -0.3),
                rotation_deg: 12.0,
            },
        },
        Command::SetSource {
            slot: 7,
            source: Some(PathBuf::from("photos/b.png")),
        },
        // Last, because it resizes the cells: the commands before it stay valid,
        // and the walk still visits one state per command kind.
        Command::SetTemplate {
            template: templates::get("mosaic-5-hero").expect("registered"),
        },
        // S14b's three, after the resize so they act on a known five-cell document:
        // a cell is taken, two cells are exchanged, one is given back.
        Command::AddCell,
        Command::SwapCells { left: 0, right: 1 },
        Command::RemoveLastCell,
        // S23b's two, after the resize so they act on the known five-cell document:
        // an arrival that points two cells at once, then the move that carries a photo
        // between cells and empties the one it came from.
        Command::PlacePhotos {
            places: vec![
                (2, PathBuf::from("photos/c.jpg")),
                (3, PathBuf::from("photos/d.png")),
            ],
        },
        Command::MovePhoto { from: 2, to: 4 },
        // S15's two: the cell the strip's clear button empties — slot 1, which the swap
        // above moved the framed photo into, so clearing it changes the document — and
        // the frame the dialog edits. Last, so their numbers are read against a known
        // layout.
        Command::ClearCell { slot: 1 },
        Command::SetFrame {
            frame: Frame {
                gap_rel: 0.02,
                radius_rel: 0.01,
                color: Rgba8::rgb(250, 250, 250),
            },
        },
    ]
}

#[test]
fn undo_and_redo_walk_the_exact_states() {
    let initial = document();
    let mut history = History::new(initial.clone()).expect("a valid document");
    assert_eq!(history.undo_depth(), 0);
    assert_eq!(history.redo_depth(), 0);
    assert!(!history.can_undo() && !history.can_redo());
    assert_eq!(history.doc(), &initial);

    // Every state the document passes through, so redo can be checked at every
    // step and not only at the end.
    let mut states = vec![initial.clone()];
    for (index, command) in sequence().iter().enumerate() {
        history.apply(command.clone()).expect("applies");
        assert_eq!(
            history.undo_depth(),
            index + 1,
            "one command is one undo step"
        );
        assert!(!history.can_redo(), "a new command clears the redo path");
        // A command that changed nothing would make the walk below vacuous.
        assert_ne!(
            history.doc(),
            states.last().expect("previous state"),
            "command {index} changed the document not at all"
        );
        states.push(history.doc().clone());
    }
    let mut json = Vec::new();
    for state in &states {
        json.push(state.to_json().expect("serializes"));
    }

    // Backwards, to the exact initial document, one step at a time.
    assert_eq!(history.undo_depth(), sequence().len());
    for expected in states.iter().rev().skip(1) {
        assert!(history.undo(), "there is a step to undo");
        assert_eq!(history.doc(), expected, "undo did not restore its state");
    }
    assert_eq!(
        history.doc(),
        &initial,
        "undoing every command must restore the initial document exactly"
    );
    assert!(!history.can_undo());
    assert_eq!(history.redo_depth(), sequence().len());
    // One more undo past the beginning: refused, and nothing moves.
    assert!(!history.undo());
    assert_eq!(history.doc(), &initial);

    // Forwards again, through every state.
    for (index, expected) in states.iter().enumerate().skip(1) {
        assert!(history.redo(), "step {index} is redoable");
        assert_eq!(
            history.doc(),
            expected,
            "redo did not restore state {index}"
        );
        assert_eq!(
            history.doc().to_json().expect("serializes"),
            json[index],
            "state {index} is not byte-identical to what it was"
        );
    }
    assert!(!history.can_redo());
    assert!(!history.redo(), "there is nothing past the end");
    assert_eq!(history.doc(), states.last().expect("a final state"));
    assert_eq!(history.undo_depth(), sequence().len());
}

#[test]
fn a_command_that_changes_nothing_is_not_a_step() {
    // PIX-022 (S15d): the rule the GUI's pending-gesture path had already is now
    // the history's own, so every command from every surface gets it.
    let mut history = History::new(document()).expect("a valid document");
    let source = Command::SetSource {
        slot: 0,
        source: Some(PathBuf::from("photos/a.jpg")),
    };
    assert!(
        history.apply(source.clone()).expect("applies"),
        "the first one changes the document"
    );
    assert_eq!(history.undo_depth(), 1);

    let before = history.doc().to_json().expect("serializes");
    assert!(
        !history.apply(source.clone()).expect("applies"),
        "the same command again changes nothing"
    );
    assert_eq!(history.undo_depth(), 1, "and is not an undo step");
    assert_eq!(history.doc().to_json().expect("serializes"), before);

    // The redo path is not forked by an edit that is not an edit: a no-op after an
    // undo leaves the undone state reachable.
    history
        .apply(Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 1.4,
                offset: (0.2, -0.3),
                rotation_deg: 12.0,
            },
        })
        .expect("applies");
    assert!(history.undo(), "there is a step to undo");
    assert_eq!(history.redo_depth(), 1);
    assert!(
        !history
            .apply(Command::SetSource {
                slot: 0,
                source: Some(PathBuf::from("photos/a.jpg")),
            })
            .expect("applies"),
        "the state the undo returned to already has this source"
    );
    assert_eq!(
        history.redo_depth(),
        1,
        "a command that changes nothing forked the redo path"
    );

    // The other direction too: `ClearCell` on a cell that is already empty is not a
    // step either, which is the "Reset an already-identity crop" case the review
    // named (the direct paths, not the gesture one).
    let mut fresh = History::new(document()).expect("a valid document");
    assert!(
        !fresh
            .apply(Command::ClearCell { slot: 0 })
            .expect("applies"),
        "clearing an empty cell changes nothing"
    );
    assert_eq!(fresh.undo_depth(), 0);
}

#[test]
fn a_rebase_rewrites_every_state_the_history_holds() {
    // PIX-005 (S15d): the document a Save As wrote and the document the window
    // holds are one document, so every state in the stacks has to spell its sources
    // the way the file does — an undo that went back to the old spelling would
    // resolve the photos against the directory they were moved away from.
    let mut history = History::new(document()).expect("a valid document");
    history
        .apply(Command::SetSource {
            slot: 0,
            source: Some(PathBuf::from("photos/a.jpg")),
        })
        .expect("applies");
    history
        .apply(Command::SetSource {
            slot: 1,
            source: Some(PathBuf::from("photos/b.jpg")),
        })
        .expect("applies");
    // Lexical, so the directories need not exist: the project moves from
    // `/proj/one` to `/proj/two`, which is one `..` per source.
    assert!(
        history.rebase(Path::new("/proj/one"), Path::new("/proj/two")),
        "the sources moved"
    );
    assert_eq!(
        history.doc().cells[0].source,
        Some(PathBuf::from("../one/photos/a.jpg"))
    );
    assert!(
        !history.rebase(Path::new("/proj/one"), Path::new("/proj/two")),
        "rebasing twice is not a second move"
    );
    assert!(history.undo(), "there is a step to undo");
    assert_eq!(
        history.doc().cells[0].source,
        Some(PathBuf::from("../one/photos/a.jpg")),
        "the state an undo returns to kept the old spelling"
    );
    assert_eq!(history.doc().cells[1].source, None);
    // An absolute source is nobody's business but the format's, exactly as the
    // written document has it.
    history
        .apply(Command::SetSource {
            slot: 2,
            source: Some(PathBuf::from("/elsewhere/c.jpg")),
        })
        .expect("applies");
    history.rebase(Path::new("/proj/two"), Path::new("/proj/three"));
    assert_eq!(
        history.doc().cells[2].source,
        Some(PathBuf::from("/elsewhere/c.jpg"))
    );
}

#[test]
fn a_command_that_would_break_the_contract_changes_nothing() {
    let initial = document();
    let mut history = History::new(initial.clone()).expect("a valid document");
    history
        .apply(Command::SetSource {
            slot: 0,
            source: Some(PathBuf::from("photos/kept.jpg")),
        })
        .expect("applies");

    let before = history.doc().to_json().expect("serializes");
    let (undo, redo) = (history.undo_depth(), history.redo_depth());

    let refusals: Vec<(&str, Command)> = vec![
        (
            "a zoom past MAX_ZOOM",
            Command::SetCrop {
                slot: 0,
                crop: CropTransform {
                    zoom: 9000.0,
                    ..CropTransform::IDENTITY
                },
            },
        ),
        (
            // The ±45° cap is gone (2026-09-22), so what a rotation can fail on is
            // its domain: a number no arithmetic can be done on.
            "a rotation that is not a number",
            Command::SetCrop {
                slot: 0,
                crop: CropTransform {
                    rotation_deg: f64::NAN,
                    ..CropTransform::IDENTITY
                },
            },
        ),
        (
            "a slot the template does not have",
            Command::SetSource {
                slot: 99,
                source: None,
            },
        ),
        (
            "a template with a slot count outside the limits",
            Command::SetTemplate {
                template: ten_slot_template(),
            },
        ),
    ];

    for (what, command) in refusals {
        let error = history
            .apply(command.clone())
            .expect_err(&format!("{what} must be refused"));
        assert!(
            !error.to_string().is_empty(),
            "{what}: the refusal says nothing"
        );
        assert_eq!(
            history.doc().to_json().expect("serializes"),
            before,
            "{what}: the document changed anyway"
        );
        assert_eq!(
            (history.undo_depth(), history.redo_depth()),
            (undo, redo),
            "{what}: the stacks moved anyway"
        );
        assert_eq!(history.doc(), &{
            let mut expected = document();
            expected.cells[0].source = Some(PathBuf::from("photos/kept.jpg"));
            expected
        });
    }

    // The index errors are the ones the document cannot report on its own, so
    // they are also checked by name.
    assert!(matches!(
        history.apply(Command::SetSource {
            slot: 8,
            source: None
        }),
        Err(CoreError::NoSuchSlot { slot: 8, slots: 8 })
    ));

    // And the history still works: a valid command after all those refusals
    // lands on the state the document really had.
    history
        .apply(Command::SetSource {
            slot: 1,
            source: Some(PathBuf::from("photos/after.jpg")),
        })
        .expect("applies");
    assert_eq!(
        history.doc().cells[1].source,
        Some(PathBuf::from("photos/after.jpg"))
    );
    assert_eq!(history.undo_depth(), undo + 1);
}

#[test]
fn a_command_after_an_undo_forks_the_redo_path() {
    let mut history = History::new(document()).expect("a valid document");
    let first = Command::SetCrop {
        slot: 0,
        crop: CropTransform {
            zoom: 1.2,
            offset: (0.1, 0.1),
            rotation_deg: 10.0,
        },
    };
    let second = Command::SetCrop {
        slot: 0,
        crop: CropTransform {
            zoom: 1.4,
            offset: (0.2, -0.1),
            rotation_deg: -20.0,
        },
    };
    history.apply(first.clone()).expect("applies");
    let framed = history.doc().clone();
    history.apply(second.clone()).expect("applies");

    assert!(history.undo());
    assert_eq!(history.doc(), &framed);
    assert_eq!(history.redo_depth(), 1);

    // A different command from here: the undone one is no longer reachable.
    history
        .apply(Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 0.9,
                offset: (0.0, 0.0),
                rotation_deg: 0.0,
            },
        })
        .expect("applies");
    assert_eq!(history.redo_depth(), 0, "the redo path must be forked");
    assert!(!history.redo());
    assert!(history.undo());
    assert_eq!(history.doc(), &framed, "undo still walks the real history");
}

#[test]
fn a_swapped_photo_keeps_the_framing_it_had() {
    // The reason `zoom` is absolute rather than "a multiple of fill" (contract
    // §1): swapping the photo must not move the area the user framed, so picking
    // the photo is one command and the framing is another.
    let mut history = History::new(document()).expect("a valid document");
    let crop = CropTransform {
        zoom: 1.4,
        offset: (0.2, -0.3),
        rotation_deg: 12.0,
    };
    history
        .apply(Command::SetCrop { slot: 3, crop })
        .expect("applies");
    history
        .apply(Command::SetSource {
            slot: 3,
            source: Some(PathBuf::from("photos/other.jpg")),
        })
        .expect("applies");
    assert_eq!(history.doc().cells[3].crop, crop);
    assert_eq!(
        history.doc().cells[3].source,
        Some(PathBuf::from("photos/other.jpg"))
    );

    // And emptying the slot keeps it too, so putting the photo back restores
    // exactly the framing the user had.
    history
        .apply(Command::SetSource {
            slot: 3,
            source: None,
        })
        .expect("applies");
    assert_eq!(history.doc().cells[3], Cell { source: None, crop });
}

#[test]
fn a_framing_request_is_stored_as_asked() {
    // What `draw` paints is the *fit* of the request (S3), so the document is free
    // to hold a request that does not cover: the clamp is recomputed at the render
    // boundary and is not this layer's business to pre-apply.
    let mut history = History::new(document()).expect("a valid document");
    let request = CropTransform {
        zoom: 0.4,
        offset: (0.9, -0.9),
        rotation_deg: 40.0,
    };
    history
        .apply(Command::SetCrop {
            slot: 1,
            crop: request,
        })
        .expect("applies");
    assert_eq!(history.doc().cells[1].crop, request);

    // The one thing that *is* applied to a request: a finite angle is wrapped into
    // `(-180, 180]`, because a gesture that spins does not spin the numbers with it
    // (S11). Every angle the old ±45° cap allowed is inside the range, so nothing a
    // document could hold before moves.
    for (given, stored) in [
        (450.0, 90.0),
        (-180.0, 180.0),
        (-400.0, -40.0),
        (12.0, 12.0),
    ] {
        history
            .apply(Command::SetCrop {
                slot: 1,
                crop: CropTransform {
                    rotation_deg: given,
                    ..request
                },
            })
            .expect("applies");
        assert_eq!(history.doc().cells[1].crop.rotation_deg, stored, "{given}");
        assert_eq!(history.doc().cells[1].crop.zoom, request.zoom, "{given}");
    }
}

#[test]
fn a_history_refuses_a_document_that_is_not_valid() {
    let mut broken = document();
    broken.cells.pop();
    assert!(matches!(
        History::new(broken),
        Err(CoreError::CellCount { cells: 7, slots: 8 })
    ));
}

#[test]
fn a_template_change_keeps_the_cells_it_cannot_place() {
    // S7's command, and the reason it exists: a user who has placed
    // photos must be able to try another layout without starting over, so the
    // cells that still exist keep what they hold. Since S28 a smaller template does
    // not drop the tail either (ruling 43: a photo leaves the collage only when it
    // is deleted): those cells are *kept*, whole and in their own order, and a later
    // change with more slots places them again.
    let mut history = History::new(document()).expect("a valid document");
    for (slot, name) in [(0usize, "photos/a.jpg"), (4, "photos/b.png")] {
        history
            .apply(Command::SetSource {
                slot,
                source: Some(PathBuf::from(name)),
            })
            .expect("applies");
    }
    history
        .apply(Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 1.6,
                offset: (0.1, 0.0),
                rotation_deg: -8.0,
            },
        })
        .expect("applies");
    // A layer-free document: the retention rule that matters now is the cells'.
    // `mosaic-5-hero` is 4:3, with five slots.
    let template = templates::get("mosaic-5-hero").expect("registered");
    let before = history.doc().clone();
    history
        .apply(Command::SetTemplate {
            template: template.clone(),
        })
        .expect("applies");

    let doc = history.doc();
    assert_eq!(doc.template, template);
    assert_eq!(doc.cells.len(), 5, "one cell per slot");
    assert_eq!(
        doc.cells[0].source,
        Some(PathBuf::from("photos/a.jpg")),
        "the cells that still exist keep their photos"
    );
    assert_eq!(doc.cells[0].crop.zoom, 1.6, "and their framing");
    assert_eq!(doc.cells[4].source, Some(PathBuf::from("photos/b.png")));
    assert_eq!(
        doc.kept,
        before.cells[5..].to_vec(),
        "the tail is kept, whole and in its own order"
    );
    assert_eq!(
        doc.cells.len() + doc.kept.len(),
        8,
        "the change moved no cell out of the document"
    );
    doc.validate().expect("the result is a valid document");

    // And it is one undo step, like every other command.
    assert_eq!(history.undo_depth(), 4);
    assert!(history.undo());
    assert_eq!(history.doc().cells.len(), 8);
    assert!(history.doc().kept.is_empty(), "the undo took the tail back");

    // A change with more slots places the kept cells again, in their own order,
    // before it appends empty ones.
    assert!(history.redo(), "the change is redoable");
    history
        .apply(Command::SetTemplate {
            template: templates::get("mosaic-8-s14").expect("registered"),
        })
        .expect("applies");
    assert_eq!(history.doc(), &before, "the document is the one it was");
    assert!(history.doc().kept.is_empty());
}

#[test]
fn adding_photos_fills_empty_cells_before_it_grows_the_layout() {
    // S14's count control, in its two halves. A photo goes to the first *empty*
    // cell, and a photo that finds none takes the layout with one slot more —
    // which is what makes `+` one control rather than a menu.
    let mut history = History::new(occupied("mosaic-4-hero")).expect("a valid document");
    history
        .apply(Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 1.8,
                offset: (0.2, -0.1),
                rotation_deg: 9.0,
            },
        })
        .expect("applies");
    let survived = history.doc().cells[..4].to_vec();

    history
        .apply(Command::AddPhotos {
            photos: vec![PathBuf::from("photos/new.jpg")],
        })
        .expect("a fifth photo fits");
    let doc = history.doc();
    assert_eq!(doc.cells.len(), 5, "the layout grew with the count");
    assert_eq!(doc.template.name, "mosaic-5-hero", "4:3 stays 4:3");
    assert_eq!(
        doc.cells[..4],
        survived[..],
        "the cells that survived keep their photo and framing"
    );
    assert_eq!(
        doc.cells[4].source,
        Some(PathBuf::from("photos/new.jpg")),
        "the new photo lands in the appended cell"
    );
    doc.validate().expect("a valid document");

    // An empty cell comes first: the count is what grew, not the layout.
    history
        .apply(Command::SetSource {
            slot: 2,
            source: None,
        })
        .expect("applies");
    history
        .apply(Command::AddPhotos {
            photos: vec![PathBuf::from("photos/hole.jpg")],
        })
        .expect("applies");
    let doc = history.doc();
    assert_eq!(doc.cells.len(), 5, "no sixth cell: there was an empty one");
    assert_eq!(doc.cells[2].source, Some(PathBuf::from("photos/hole.jpg")));
    assert_eq!(
        doc.cells[2].crop,
        CropTransform::IDENTITY,
        "the empty cell had no framing to inherit"
    );

    // Two photos at once, with every cell taken: the layout grows per photo, and
    // it is still one undo step.
    let depth = history.undo_depth();
    history
        .apply(Command::AddPhotos {
            photos: vec![
                PathBuf::from("photos/six.jpg"),
                PathBuf::from("photos/seven.jpg"),
            ],
        })
        .expect("applies");
    let doc = history.doc();
    assert_eq!(doc.cells.len(), 7);
    assert_eq!(doc.template.name, "mosaic-7-t4b3");
    assert_eq!(doc.cells[5].source, Some(PathBuf::from("photos/six.jpg")));
    assert_eq!(doc.cells[6].source, Some(PathBuf::from("photos/seven.jpg")));
    assert_eq!(history.undo_depth(), depth + 1, "one call is one undo step");

    // The ceiling is the format's own: nine is the slot limit, so
    // there is no layout left to grow into.
    let full = History::new(occupied("strip-9-9x1")).expect("a valid document");
    let mut full = full;
    let refused = full
        .apply(Command::AddPhotos {
            photos: vec![PathBuf::from("photos/tenth.jpg")],
        })
        .expect_err("a tenth photo has nowhere to go");
    assert!(
        matches!(refused, CoreError::TooManyPhotos { max: 9 }),
        "{refused}"
    );
    assert!(refused.to_string().contains('9'), "{refused}");
    assert_eq!(full.doc().cells.len(), 9, "the refusal changed nothing");
    assert_eq!(full.undo_depth(), 0);
}

#[test]
fn the_count_control_moves_the_layout_and_keeps_what_it_takes_off() {
    // S14b, ruled 2026-09-23: the control addresses the *layout*. `+` takes the
    // layout with one cell more and `−` takes the layout with one cell fewer.
    // S28 (ruling 43) amends the other half: what `−` takes off the sheet is
    // **kept** — photo, framing and order — so the next growth places it again,
    // and `Ctrl+Z` is no longer the only way a photo comes back. What makes `+`
    // mean one thing rather than two is that it still edits the layout: a cell
    // coming back is what "one more cell" means when one is waiting.
    let mut history = History::new(occupied("mosaic-4-hero")).expect("a valid document");
    history
        .apply(Command::SetCrop {
            slot: 3,
            crop: CropTransform {
                zoom: 1.9,
                offset: (-0.2, 0.15),
                rotation_deg: -13.0,
            },
        })
        .expect("applies");
    let framed = history.doc().clone();

    // `−`: the last cell leaves the sheet whole — photo, framing and all — and the
    // document keeps it (S28). One command, one undo step.
    let depth = history.undo_depth();
    history
        .apply(Command::RemoveLastCell)
        .expect("four cells can drop to three");
    let doc = history.doc();
    assert_eq!(doc.cells.len(), 3);
    assert_eq!(doc.template.name, "mosaic-3-hero", "4:3 stays 4:3");
    assert_eq!(
        doc.cells[..3],
        framed.cells[..3],
        "the survivors are untouched"
    );
    assert_eq!(
        doc.kept,
        vec![framed.cells[3].clone()],
        "the cell left the sheet whole, photo and framing"
    );
    assert_eq!(
        doc.cells.len() + doc.kept.len(),
        4,
        "and nothing left the document"
    );
    assert_eq!(history.undo_depth(), depth + 1, "one call is one undo step");
    doc.validate().expect("a valid document");

    // `+` places it again: same photo, same framing, same index. That is the whole of
    // ruling 43's "three photos → two cells → three cells is the document it was".
    history.apply(Command::AddCell).expect("a fourth cell fits");
    assert_eq!(
        history.doc().cells,
        framed.cells,
        "the document is the one it was"
    );
    assert_eq!(history.doc().template.name, framed.template.name);
    assert!(history.doc().kept.is_empty(), "and nothing is waiting");

    // Undo takes the returned cell off the sheet again, keeping it: the step is the
    // layout change on both sides, not a restore path of its own.
    assert!(history.undo(), "the removal is undoable");
    assert_eq!(history.doc().cells.len(), 3);
    assert_eq!(history.doc().kept, vec![framed.cells[3].clone()]);

    // With nothing waiting, `+` appends an **empty** cell: the S14b rule the
    // retention did not replace.
    assert!(history.redo(), "and redoable");
    history.apply(Command::AddCell).expect("a fifth cell fits");
    assert_eq!(
        history.doc().cells[4],
        Cell::default(),
        "an added cell is empty when nothing waits"
    );

    // An empty cell that leaves the sheet is kept like any other, so the pair
    // round-trips there too — nothing about the rule is about photos.
    history.apply(Command::RemoveLastCell).expect("applies");
    assert_eq!(history.doc().kept, vec![Cell::default()]);
    history.apply(Command::AddCell).expect("applies");
    assert_eq!(history.doc().cells[4], Cell::default());
    assert!(history.doc().kept.is_empty());

    // A delete is the other half of the rule: `ClearCell` empties a cell and puts
    // nothing in the kept list, because *that* is what losing a photo is.
    history
        .apply(Command::ClearCell { slot: 3 })
        .expect("applies");
    assert!(history.doc().cells[3].source.is_none(), "the photo is gone");
    assert!(
        history.doc().kept.is_empty(),
        "and nothing is kept: this was a delete"
    );
    assert_eq!(history.doc().cells.len(), 5, "the cell itself stays");

    // The ceiling is the format's slot limit, and the floor is the selection's own
    // minimum: past either, the refusal changes nothing.
    let mut full = History::new(occupied("strip-9-9x1")).expect("a valid document");
    let refused = full.apply(Command::AddCell).expect_err("no tenth cell");
    assert!(
        matches!(refused, CoreError::TooManyCells { max: 9 }),
        "{refused}"
    );
    assert_eq!(full.doc().cells.len(), 9, "the refusal changed nothing");
    assert_eq!(full.undo_depth(), 0);

    let mut two = History::new(occupied("strip-2-2x1")).expect("a valid document");
    // Since S19 the floor is one cell (ruling 34): a two-cell layout drops to the
    // one-slot sheet, and only *its* removal is refused.
    two.apply(Command::RemoveLastCell)
        .expect("two cells can drop to one");
    assert_eq!(two.doc().cells.len(), 1);
    assert_eq!(two.doc().template.name, "grid-1-1x1");
    let refused = two
        .apply(Command::RemoveLastCell)
        .expect_err("no zero-cell layout");
    assert!(
        matches!(refused, CoreError::TooFewCells { min: 1 }),
        "{refused}"
    );
    assert_eq!(two.doc().cells.len(), 1, "the refusal changed nothing");
    assert_eq!(two.undo_depth(), 1, "only the accepted removal is a step");
}

#[test]
fn no_layout_change_loses_a_cell_whatever_the_sequence_is() {
    // S28's own invariant, swept over the library: whatever the layout changes do,
    // what the document holds — the cells on the sheet plus the cells it keeps —
    // never goes down, and `−` × k then `+` × k gives the document back for every k
    // the floor allows. A delete is the one command that takes a photo out.
    for name in templates::names() {
        let mut history = History::new(occupied(name)).expect("a valid document");
        let start = history.doc().clone();
        let cells = start.cells.len();

        // Every template in the library, each in turn: neither the cell total nor the
        // photo total ever shrinks, and every state is a document this build accepts.
        let mut held = cells;
        let mut photos = cells;
        for template in templates::all() {
            history
                .apply(Command::SetTemplate { template })
                .expect("a layout change always applies");
            let doc = history.doc();
            assert!(
                doc.cells.len() + doc.kept.len() >= held,
                "{name}: the document went from {held} cells to {} placed + {} kept",
                doc.cells.len(),
                doc.kept.len()
            );
            let now = doc
                .cells
                .iter()
                .filter(|cell| cell.source.is_some())
                .count()
                + doc.kept.iter().filter(|cell| cell.source.is_some()).count();
            assert!(
                now >= photos,
                "{name}: the document went from {photos} photos to {now}"
            );
            held = doc.cells.len() + doc.kept.len();
            photos = now;
            doc.validate().expect("a valid document");
        }

        // Back to the layout it started on: the sheet is the document it was. What it
        // gained on the way — the empty cells a bigger layout appended — stays in the
        // document, off the sheet and waiting (S28 keeps cells, and an empty cell is
        // what a growth appends anyway).
        history
            .apply(Command::SetTemplate {
                template: start.template.clone(),
            })
            .expect("applies");
        let doc = history.doc();
        assert_eq!(
            doc.cells, start.cells,
            "{name}: the sheet is the one it was"
        );
        assert_eq!(
            doc.cells.len() + doc.kept.len(),
            held,
            "{name}: and nothing left the document"
        );
        assert!(
            doc.kept.iter().all(|cell| cell.source.is_none()),
            "{name}: what waits is the empty cells the bigger layouts appended"
        );

        // The count control: `−` × k then `+` × k, for every k the floor allows.
        for k in 1..cells {
            let mut history = History::new(start.clone()).expect("a valid document");
            // `layout_for` picks each intermediate layout by the aspect the walk is
            // then carrying, and the library does not have every aspect at every
            // count (there is no 4:3 two-cell layout, so a 4:3 three-cell document
            // comes back on a 16:9 strip). The cells below are what the retention
            // promises; the layout comes back exactly where the aspect survived the
            // walk, which is the count rule's own answer (S14b) and not this step's.
            let mut aspect_kept = true;
            for _ in 0..k {
                history
                    .apply(Command::RemoveLastCell)
                    .expect("there is a cell to take off");
                aspect_kept &= history.doc().template.aspect == start.template.aspect;
            }
            assert_eq!(history.doc().cells.len(), cells - k, "{name}: k = {k}");
            assert_eq!(history.doc().kept.len(), k, "{name}: k = {k}");
            for _ in 0..k {
                history
                    .apply(Command::AddCell)
                    .expect("there is a kept cell to place");
            }
            let doc = history.doc();
            assert_eq!(
                doc.cells, start.cells,
                "{name}: `−` × {k} then `+` × {k} is the document's own cells"
            );
            assert!(doc.kept.is_empty(), "{name}: k = {k}");
            if aspect_kept {
                assert_eq!(
                    doc.template.name, start.template.name,
                    "{name}: k = {k}, with the aspect kept at every step"
                );
            }
        }

        // The exception the rule names: a delete takes the photo out and keeps
        // nothing.
        let mut history = History::new(start.clone()).expect("a valid document");
        history
            .apply(Command::ClearCell { slot: 0 })
            .expect("applies");
        assert!(history.doc().cells[0].source.is_none(), "{name}");
        assert!(
            history.doc().kept.is_empty(),
            "{name}: a delete keeps nothing"
        );
    }
}

#[test]
fn an_arrival_lands_in_a_cell_of_its_own_and_the_ceiling_counts_placed_and_kept() {
    // S28: `AddPhotos` never spends a kept cell — the user has just chosen that
    // photo, so it gets a cell of its own — and the ceiling is what the document
    // holds in total, placed and kept together.
    let mut history = History::new(occupied("strip-2-2x1")).expect("a valid document");
    let placed = history.doc().cells.clone();
    history.apply(Command::RemoveLastCell).expect("applies");
    assert_eq!(history.doc().cells.len(), 1);
    assert_eq!(history.doc().kept.len(), 1);

    history
        .apply(Command::AddPhotos {
            photos: vec![PathBuf::from("photos/new.jpg")],
        })
        .expect("the arrival lands");
    let doc = history.doc();
    assert_eq!(doc.cells.len(), 2, "the layout grew for the arrival");
    assert_eq!(
        doc.cells[1].source,
        Some(PathBuf::from("photos/new.jpg")),
        "the arrival is in a cell of its own"
    );
    assert_eq!(
        doc.kept,
        vec![placed[1].clone()],
        "and the kept cell is still waiting"
    );
    doc.validate().expect("a valid document");

    // Nine cells' worth is the ceiling whether they are on the sheet or kept: a
    // document with one cell kept has no room for an arrival.
    let mut full = History::new(occupied("strip-9-9x1")).expect("a valid document");
    full.apply(Command::RemoveLastCell).expect("applies");
    assert_eq!(full.doc().cells.len() + full.doc().kept.len(), 9);
    let refused = full
        .apply(Command::AddPhotos {
            photos: vec![PathBuf::from("photos/tenth.jpg")],
        })
        .expect_err("a tenth photo has nowhere to go");
    assert!(
        matches!(refused, CoreError::TooManyPhotos { max: 9 }),
        "{refused}"
    );
    assert_eq!(full.doc().cells.len(), 8, "the refusal changed nothing");
    assert_eq!(full.doc().kept.len(), 1);
    assert_eq!(full.undo_depth(), 1, "only the removal was a step");
}

#[test]
fn a_swap_exchanges_two_cells_whole() {
    // S14b: "two photos must be swappable". The *cell* moves — photo and framing
    // together — because the framing is what makes a photo look right where it is;
    // swapping the sources and leaving the crops behind would reframe both pictures
    // as a side effect of wanting them in each other's place.
    let mut history = History::new(occupied("grid-4-2x2")).expect("a valid document");
    let framing = |zoom: f64, rotation: f64| CropTransform {
        zoom,
        offset: (0.1, -0.2),
        rotation_deg: rotation,
    };
    history
        .apply(Command::SetCrop {
            slot: 0,
            crop: framing(2.4, 17.0),
        })
        .expect("applies");
    history
        .apply(Command::SetCrop {
            slot: 3,
            crop: framing(1.3, -8.5),
        })
        .expect("applies");
    let before = history.doc().clone();

    let depth = history.undo_depth();
    history
        .apply(Command::SwapCells { left: 0, right: 3 })
        .expect("applies");
    let doc = history.doc();
    assert_eq!(
        doc.cells[0], before.cells[3],
        "cell 0 is what cell 3 was: photo and framing"
    );
    assert_eq!(doc.cells[3], before.cells[0], "and the other way round");
    assert_eq!(doc.cells[1..3], before.cells[1..3], "no other cell moved");
    assert_eq!(history.undo_depth(), depth + 1, "one swap is one undo step");
    doc.validate().expect("a valid document");

    // A swap has to be its own inverse, which is what makes it a swap rather than
    // a rotation of the list.
    history
        .apply(Command::SwapCells { left: 0, right: 3 })
        .expect("applies");
    assert_eq!(history.doc(), &before, "swapping back is the identity");

    // An empty cell is a legal half: the whole point of `+` is that a cell starts
    // without a photo, and moving it somewhere is how a user fills a hole.
    let mut history = History::new(occupied("mosaic-4-hero")).expect("a valid document");
    history
        .apply(Command::SetSource {
            slot: 1,
            source: None,
        })
        .expect("applies");
    history
        .apply(Command::SwapCells { left: 1, right: 3 })
        .expect("applies");
    assert!(history.doc().cells[3].source.is_none(), "the hole moved");
    assert_eq!(
        history.doc().cells[1].source,
        Some(PathBuf::from("photos/cell3.jpg")),
        "and the photo took its place"
    );

    // The two pairs that cannot be a swap are refused, and neither leaves a trace.
    let mut history = History::new(occupied("strip-3-3x1")).expect("a valid document");
    let refused = history
        .apply(Command::SwapCells { left: 1, right: 1 })
        .expect_err("a cell cannot be swapped with itself");
    assert!(
        matches!(refused, CoreError::SameSlot { slot: 1 }),
        "{refused}"
    );
    assert_eq!(history.undo_depth(), 0, "the refusal left no step");

    let refused = history
        .apply(Command::SwapCells { left: 0, right: 7 })
        .expect_err("the layout has three cells");
    assert!(
        matches!(refused, CoreError::NoSuchSlot { slot: 7, slots: 3 }),
        "{refused}"
    );
    assert_eq!(history.undo_depth(), 0);
}

#[test]
fn clearing_a_cell_empties_it_in_one_undo_step() {
    // S15: "clear this cell" is one intent — photo *and* framing — so it is one
    // command and one undo step, the same document `edit --clear` writes. `SetSource
    // { source: None }` is the other thing: it keeps the framing, which is what makes
    // replacing a photo keep the area the user framed.
    let mut history = History::new(occupied("grid-4-2x2")).expect("a valid document");
    let framing = CropTransform {
        zoom: 2.4,
        offset: (0.1, -0.2),
        rotation_deg: 17.0,
    };
    history
        .apply(Command::SetCrop {
            slot: 2,
            crop: framing,
        })
        .expect("applies");
    let before = history.doc().cells[2].source.clone();
    assert!(before.is_some(), "the cell holds a photo to clear");

    let depth = history.undo_depth();
    history
        .apply(Command::ClearCell { slot: 2 })
        .expect("applies");
    assert_eq!(
        history.doc().cells[2],
        Cell::default(),
        "the cell is empty and its framing is the default"
    );
    assert_eq!(
        history.undo_depth(),
        depth + 1,
        "one command, one undo step"
    );
    assert!(history.undo());
    assert_eq!(history.doc().cells[2].source, before);
    assert_eq!(
        history.doc().cells[2].crop,
        framing,
        "and the framing came back"
    );

    // The photo-only half is still its own command: it leaves the framing alone.
    history
        .apply(Command::SetSource {
            slot: 2,
            source: None,
        })
        .expect("applies");
    assert!(history.doc().cells[2].source.is_none());
    assert_eq!(
        history.doc().cells[2].crop,
        framing,
        "clearing the source is not clearing the cell"
    );

    // A cell the layout does not have is refused, and nothing moves.
    let depth = history.undo_depth();
    let refused = history
        .apply(Command::ClearCell { slot: 7 })
        .expect_err("the layout has four cells");
    assert!(
        matches!(refused, CoreError::NoSuchSlot { slot: 7, slots: 4 }),
        "{refused}"
    );
    assert_eq!(history.undo_depth(), depth, "the refusal left no step");
}

#[test]
fn a_frame_change_is_one_undo_step_and_its_refusals_leave_no_trace() {
    // S15: the frame is a document field the user edits, so setting it is a command
    // like any other — one undo step, and a frame the contract cannot render is
    // refused before it becomes the document. The CLI's `--gap/--radius/
    // --border-color` and the `Frame…` dialog both send this, which is what makes
    // `pixlay-cli edit` and the window one writer for the frame.
    let mut history = History::new(occupied("mosaic-4-hero")).expect("a valid document");
    let before = history.doc().clone();
    assert_eq!(before.frame, Frame::default(), "the default is no frame");

    let depth = history.undo_depth();
    history
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.03,
                radius_rel: 0.02,
                color: Rgba8::rgb(12, 200, 240),
            },
        })
        .expect("applies");
    assert_eq!(
        history.undo_depth(),
        depth + 1,
        "one command, one undo step"
    );
    let framed = history.doc().frame;
    assert_eq!(framed.gap_rel, 0.03);
    assert_eq!(framed.radius_rel, 0.02);
    assert_eq!(framed.color, Rgba8::rgb(12, 200, 240));
    assert_eq!(
        history.doc().cells,
        before.cells,
        "a frame is not a relayout: no cell moves"
    );
    history.doc().validate().expect("a valid document");

    assert!(history.undo(), "the frame change is undoable");
    assert_eq!(
        history.doc(),
        &before,
        "undo restores the frame the document had"
    );
    assert!(history.redo());
    assert_eq!(history.doc().frame, framed);

    // A translucent backdrop is refused: the export's surface starts transparent, so
    // a translucent document would make preview and export two different pictures
    // (`Frame::validate`).
    let refused = history
        .apply(Command::SetFrame {
            frame: Frame {
                color: Rgba8 {
                    r: 10,
                    g: 10,
                    b: 10,
                    a: 128,
                },
                ..Frame::default()
            },
        })
        .expect_err("a translucent backdrop is not a document");
    assert!(matches!(refused, CoreError::OutOfRange { .. }), "{refused}");

    // A length past the whole canvas is a typo, not a frame.
    let refused = history
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 2.0,
                ..Frame::default()
            },
        })
        .expect_err("a gap past the canvas is refused");
    assert!(matches!(refused, CoreError::OutOfRange { .. }), "{refused}");

    // And a gap *inside* the range can still empty the smallest cell of a layout;
    // that is refused by the per-slot check and the error names the slot.
    let mut history = History::new(occupied("grid-9-3x3")).expect("a valid document");
    let refused = history
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.5,
                ..Frame::default()
            },
        })
        .expect_err("a gap that empties a cell is refused");
    match refused {
        CoreError::InvalidSlot { slot, .. } => assert!(slot < 9, "slot {slot} is in the layout"),
        other => panic!("{other}"),
    }
    assert_eq!(history.undo_depth(), 0, "the refusals left no step");
    assert_eq!(history.doc().frame, Frame::default(), "and no frame");
}

#[test]
fn several_photos_land_in_one_step() {
    // S23b: an arrival that fills or replaces several cells is **one** command, so it
    // is one undo step rather than one per file — which is what a drop from the file
    // manager and a paste are. The framing is `SetSource`'s rule: a cell that receives
    // a photo keeps the area the user framed for it.
    let mut history = History::new(occupied("grid-4-2x2")).expect("a valid document");
    let framing = CropTransform {
        zoom: 2.0,
        offset: (0.1, -0.2),
        rotation_deg: 17.0,
    };
    history
        .apply(Command::SetCrop {
            slot: 2,
            crop: framing,
        })
        .expect("applies");
    history
        .apply(Command::ClearCell { slot: 3 })
        .expect("applies");
    let before = history.doc().clone();

    let depth = history.undo_depth();
    history
        .apply(Command::PlacePhotos {
            places: vec![
                (2, PathBuf::from("photos/drop-a.jpg")),
                (3, PathBuf::from("photos/drop-b.png")),
            ],
        })
        .expect("applies");
    let doc = history.doc();
    assert_eq!(
        doc.cells[2].source,
        Some(PathBuf::from("photos/drop-a.jpg")),
        "the first pair replaced what cell 2 held"
    );
    assert_eq!(
        doc.cells[2].crop, framing,
        "and the framing it was shown with is untouched"
    );
    assert_eq!(
        doc.cells[3].source,
        Some(PathBuf::from("photos/drop-b.png")),
        "the second pair filled the empty cell"
    );
    assert_eq!(doc.cells[0], before.cells[0], "no other cell moved");
    assert_eq!(doc.cells[1], before.cells[1]);
    assert_eq!(
        history.undo_depth(),
        depth + 1,
        "two cells, one arrival, one undo step"
    );
    doc.validate().expect("a valid document");

    // The order is the command's own: a pair naming the same slot twice ends on the
    // last one, because the command says "in the order given".
    history
        .apply(Command::PlacePhotos {
            places: vec![
                (1, PathBuf::from("photos/first.jpg")),
                (1, PathBuf::from("photos/last.jpg")),
            ],
        })
        .expect("applies");
    assert_eq!(
        history.doc().cells[1].source,
        Some(PathBuf::from("photos/last.jpg")),
        "the last pair wins"
    );

    // An empty list is not a step, and neither is a list that asks for what the
    // document already has (the history's own rule, PIX-022).
    let depth = history.undo_depth();
    assert!(
        !history
            .apply(Command::PlacePhotos { places: Vec::new() })
            .expect("an empty arrival is legal"),
        "nothing to place is not a step"
    );
    assert!(
        !history
            .apply(Command::PlacePhotos {
                places: vec![(1, PathBuf::from("photos/last.jpg"))],
            })
            .expect("the same photo in the same cell is legal"),
        "the same document is not a step"
    );
    assert_eq!(history.undo_depth(), depth);

    // A slot the layout does not have is refused, and nothing moves.
    let refused = history
        .apply(Command::PlacePhotos {
            places: vec![
                (0, PathBuf::from("photos/ok.jpg")),
                (9, PathBuf::from("photos/past-the-end.jpg")),
            ],
        })
        .expect_err("the layout has four cells");
    assert!(
        matches!(refused, CoreError::NoSuchSlot { slot: 9, slots: 4 }),
        "{refused}"
    );
    assert_eq!(history.undo_depth(), depth, "the refusal left no step");
    assert_eq!(
        history.doc().cells[0].source,
        before.cells[0].source,
        "and the pairs before the bad one did not land"
    );
}

#[test]
fn a_moved_photo_leaves_its_cell_empty() {
    // S23b: cut-then-paste is one intent, so it is one command and one undo step. The
    // target keeps its own framing — the same rule as Replace, which `draw` re-fits —
    // and the source comes out whole: no photo and the default framing, which is the
    // document `edit --slot i --clear` writes.
    let mut history = History::new(occupied("grid-4-2x2")).expect("a valid document");
    let carried = CropTransform {
        zoom: 2.4,
        offset: (0.1, -0.2),
        rotation_deg: 17.0,
    };
    let target_framing = CropTransform {
        zoom: 1.3,
        offset: (-0.4, 0.05),
        rotation_deg: -8.5,
    };
    history
        .apply(Command::SetCrop {
            slot: 1,
            crop: carried,
        })
        .expect("applies");
    history
        .apply(Command::SetCrop {
            slot: 3,
            crop: target_framing,
        })
        .expect("applies");
    let before = history.doc().clone();
    assert!(before.cells[1].source.is_some());

    let depth = history.undo_depth();
    history
        .apply(Command::MovePhoto { from: 1, to: 3 })
        .expect("applies");
    let doc = history.doc();
    assert_eq!(
        doc.cells[1],
        Cell::default(),
        "the source is an empty cell: no photo and no framing"
    );
    assert_eq!(
        doc.cells[3].source, before.cells[1].source,
        "the photo arrived"
    );
    assert_eq!(
        doc.cells[3].crop, target_framing,
        "and the target kept the framing that was its own"
    );
    assert_eq!(doc.cells[0], before.cells[0], "no other cell moved");
    assert_eq!(doc.cells[2], before.cells[2]);
    assert_eq!(history.undo_depth(), depth + 1, "one move, one step");
    doc.validate().expect("a valid document");

    // One undo puts the photo back *with* the framing the source had, because the
    // whole document is the state.
    assert!(history.undo());
    assert_eq!(history.doc(), &before, "undo is the state before the move");

    // Two moves that are not edits: into the cell the photo came from, and out of a
    // cell that holds nothing. Neither is a step, and neither touches the target.
    let depth = history.undo_depth();
    assert!(
        !history
            .apply(Command::MovePhoto { from: 1, to: 1 })
            .expect("a move onto itself is legal"),
        "a paste into the cell the photo was cut from is not an edit"
    );
    history
        .apply(Command::ClearCell { slot: 2 })
        .expect("applies");
    let occupied_target = history.doc().cells[3].clone();
    assert!(
        !history
            .apply(Command::MovePhoto { from: 2, to: 3 })
            .expect("a move out of an empty cell is legal"),
        "an empty source moves nothing"
    );
    assert_eq!(
        history.doc().cells[3],
        occupied_target,
        "and it cannot empty the target"
    );
    assert_eq!(history.undo_depth(), depth + 1, "only the clear was a step");

    // A slot the layout does not have is refused, and nothing moves.
    let depth = history.undo_depth();
    let before = history.doc().clone();
    let refused = history
        .apply(Command::MovePhoto { from: 3, to: 7 })
        .expect_err("the layout has four cells");
    assert!(
        matches!(refused, CoreError::NoSuchSlot { slot: 7, slots: 4 }),
        "{refused}"
    );
    let refused = history
        .apply(Command::MovePhoto { from: 7, to: 3 })
        .expect_err("the layout has four cells");
    assert!(
        matches!(refused, CoreError::NoSuchSlot { slot: 7, slots: 4 }),
        "{refused}"
    );
    assert_eq!(history.undo_depth(), depth, "the refusals left no step");
    assert_eq!(history.doc(), &before, "and the document is untouched");
}
