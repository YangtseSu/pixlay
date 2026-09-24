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
fn a_template_change_keeps_the_photos_it_can_and_never_leaves_a_dangling_slot() {
    // S7's command. The template picker is why it exists: a user who has placed
    // photos must be able to try another layout without starting over, so the
    // cells that still exist keep what they hold and a smaller template drops the
    // tail — one cell per slot, so the result stays valid.
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
fn the_count_control_takes_and_drops_a_cell_without_remembering_it() {
    // S14b, ruled 2026-09-23: the control addresses the *layout*. `+` takes the
    // layout with one cell more and leaves it empty; `−` takes the layout with one
    // cell fewer, whatever the last cell holds. Neither remembers a photo, and
    // `Ctrl+Z` is what brings one back — which is the property that makes `+` mean
    // "one more cell" rather than "undo the last removal".
    let mut history = History::new(occupied("mosaic-4-hero")).expect("a valid document");
    history
        .apply(Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 1.9,
                offset: (-0.2, 0.15),
                rotation_deg: -13.0,
            },
        })
        .expect("applies");
    let framed = history.doc().clone();

    history.apply(Command::AddCell).expect("a fifth cell fits");
    let doc = history.doc();
    assert_eq!(doc.cells.len(), 5, "the layout took one cell more");
    assert_eq!(doc.template.name, "mosaic-5-hero", "4:3 stays 4:3");
    assert_eq!(
        doc.cells[..4],
        framed.cells[..],
        "every existing cell kept its photo and framing"
    );
    assert!(
        doc.cells[4].source.is_none(),
        "the new cell is empty: it is a layout edit, not a photo"
    );
    assert_eq!(
        doc.cells[4].crop,
        CropTransform::IDENTITY,
        "and its framing is the default one"
    );
    doc.validate().expect("a valid document");

    // `−` takes the cell away again — the empty one, here, because it is last.
    history.apply(Command::RemoveLastCell).expect("applies");
    assert_eq!(history.doc().cells.len(), 4);
    assert_eq!(
        history.doc().cells[..4],
        framed.cells[..],
        "the survivors are untouched"
    );

    // A removal with a photo in the cell drops the photo with the cell: that is
    // what "one cell fewer" means, and the undo stack is the way back.
    let depth = history.undo_depth();
    history
        .apply(Command::RemoveLastCell)
        .expect("there is a cell to drop");
    assert_eq!(history.doc().cells.len(), 3);
    assert_eq!(
        history.doc().template.name,
        "mosaic-3-hero",
        "three photos of 4:3 take the three-cell 4:3 layout"
    );
    assert_eq!(history.undo_depth(), depth + 1, "one call is one undo step");
    assert!(history.undo());
    assert_eq!(
        history.doc().cells.len(),
        4,
        "undo is the only way a dropped photo comes back"
    );
    assert_eq!(
        history.doc().cells[3].source,
        framed.cells[3].source,
        "and it comes back whole, photo and framing"
    );

    // The ceiling is the format's slot limit, and the floor is the picker's own
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
    let refused = two
        .apply(Command::RemoveLastCell)
        .expect_err("no one-cell layout");
    assert!(
        matches!(refused, CoreError::TooFewCells { min: 2 }),
        "{refused}"
    );
    assert_eq!(two.doc().cells.len(), 2, "the refusal changed nothing");
    assert_eq!(two.undo_depth(), 0);
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
