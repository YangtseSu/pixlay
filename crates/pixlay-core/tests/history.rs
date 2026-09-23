//! S6.5: the command history, as tests.
//!
//! Two properties, and they are the step's exit criteria in miniature: a sequence
//! of commands can be walked backwards to the exact document it started from and
//! forwards again to every state it passed through, and a command that would leave
//! the document outside the contract is refused *without* leaving a trace. The
//! pixel-identity half of the first one is `pixlay-render/tests/history.rs`; what
//! is measured here is the document itself, which is the stronger statement —
//! identical documents render identically by construction.

use std::path::PathBuf;

use pixlay_core::{
    Cell, CollageDoc, Command, CoreError, CropTransform, History, remove_last, templates,
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
fn a_template_change_keeps_the_photos_it_can_and_never_leaves_a_dangling_slot() {
    // S7's command. The template picker is why it exists: a user who has placed
    // photos must be able to try another layout without starting over, so the
    // cells that still exist keep what they hold, and a text layer that named a
    // slot the new template does not have keeps its text and loses only the
    // reference (the alternative — dropping the layer — deletes a user's
    // watermark, and keeping the index would make the document invalid).
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
        doc.cells[5..].len(),
        0,
        "the tail is dropped, not carried over"
    );
    doc.validate().expect("the result is a valid document");

    // And it is one undo step, like every other command.
    assert_eq!(history.undo_depth(), 4);
    assert!(history.undo());
    assert_eq!(history.doc().cells.len(), 8);
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

    // The ceiling is the picker's own: nine is also the format's slot limit, so
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
fn the_batch_removal_shrinks_the_layout_and_the_token_brings_the_cell_back() {
    // Ruling 7: one control drops the last photo and brings it back. The layout
    // follows the count down, and the token puts both the cell and its framing
    // back exactly where they were.
    let mut history = History::new(occupied("mosaic-5-hero")).expect("a valid document");
    history
        .apply(Command::SetCrop {
            slot: 4,
            crop: CropTransform {
                zoom: 2.4,
                offset: (-0.3, 0.4),
                rotation_deg: -17.0,
            },
        })
        .expect("applies");
    let before = history.doc().clone();

    // The token is the same `remove_last` the command applies, on the same
    // document: the two cannot disagree about which cell is on its way out.
    let mut probe = history.doc().clone();
    let removed = remove_last(&mut probe).expect("there is a photo to drop");
    history.apply(Command::RemoveLastPhoto).expect("applies");

    let doc = history.doc();
    assert_eq!(doc.cells.len(), 4, "the layout shrank with the count");
    assert_eq!(doc.template.name, "mosaic-4-hero", "4:3 stays 4:3");
    assert_eq!(doc.cells[..4], before.cells[..4]);
    doc.validate().expect("a valid document");

    history
        .apply(Command::RestorePhoto(removed))
        .expect("the cell comes back");
    assert_eq!(
        history.doc(),
        &before,
        "remove then restore is the exact inverse: same layout, same cell, same framing"
    );

    // Two undo steps, and undoing them walks back through both states.
    assert!(history.undo());
    assert_eq!(history.doc().cells.len(), 4);
    assert!(history.undo());
    assert_eq!(history.doc(), &before);

    // The hole case: the last *occupied* cell is what leaves, and the layout
    // shrinks to what the survivors need rather than to one less.
    let mut holed = occupied("strip-4-4x1");
    holed.cells[2] = Cell::default();
    let mut history = History::new(holed).expect("a valid document");
    let mut probe = history.doc().clone();
    let removed = remove_last(&mut probe).expect("cell 3 is the last photo");
    assert_eq!(removed.slot, 3);
    history.apply(Command::RemoveLastPhoto).expect("applies");
    assert_eq!(
        history.doc().cells.len(),
        2,
        "cells 0 and 1 are the survivors, and the cell that was already empty goes with the shrink"
    );
    assert_eq!(history.doc().template.name, "strip-2-2x1");
}

#[test]
fn the_batch_removal_refuses_an_empty_document_and_a_taken_slot() {
    let mut empty = History::new(document()).expect("a valid document");
    let refused = empty
        .apply(Command::RemoveLastPhoto)
        .expect_err("there is no photo to drop");
    assert!(matches!(refused, CoreError::NothingToRemove), "{refused}");
    assert_eq!(empty.doc().cells.len(), 8, "the refusal changed nothing");

    // A restore whose cell was taken while it was out is refused — and the
    // *layout* the restore would have grown is not left behind either, because the
    // command is applied to a copy.
    let mut history = History::new(occupied("strip-3-3x1")).expect("a valid document");
    let mut probe = history.doc().clone();
    let removed = remove_last(&mut probe).expect("a photo");
    history.apply(Command::RemoveLastPhoto).expect("applies");
    assert_eq!(history.doc().cells.len(), 2, "the layout shrank");
    history
        .apply(Command::AddPhotos {
            photos: vec![PathBuf::from("photos/other.jpg")],
        })
        .expect("applies");
    assert_eq!(history.doc().cells.len(), 3, "and grew back");
    let before = history.doc().clone();
    let refused = history
        .apply(Command::RestorePhoto(removed))
        .expect_err("slot 2 holds the photo that took its place");
    assert!(
        matches!(refused, CoreError::SlotOccupied { slot: 2 }),
        "{refused}"
    );
    assert_eq!(history.doc(), &before, "the refusal changed nothing");
}
