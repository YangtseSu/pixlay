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
    CanvasSpec, Cell, CollageDoc, Command, CoreError, CropTransform, History, templates,
};

fn document() -> CollageDoc {
    let template = templates::get(templates::SMOKE_TEMPLATE).expect("registered");
    CollageDoc::new(CanvasSpec::with_ratio(template.aspect, 297.0), template)
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
        Command::SetCanvas {
            canvas: CanvasSpec::with_ratio(4.0 / 3.0, 420.0),
        },
        Command::SetSource {
            slot: 7,
            source: Some(PathBuf::from("photos/b.png")),
        },
        // Last, because it resizes the cells: the commands before it stay valid,
        // and the walk still visits one state per command kind.
        Command::SetTemplate {
            template: templates::get("mosaic-5-hero").expect("registered"),
            canvas: CanvasSpec::with_ratio(4.0 / 3.0, 297.0),
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
            "a canvas that no longer matches the template",
            Command::SetCanvas {
                canvas: CanvasSpec::new(160.0, 90.0),
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
            "a template the library does not have",
            Command::SetTemplate {
                template: templates::get("mosaic-5-hero").expect("registered"),
                canvas: CanvasSpec::with_ratio(16.0 / 9.0, 297.0),
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
    let first = Command::SetCanvas {
        canvas: CanvasSpec::with_ratio(4.0 / 3.0, 420.0),
    };
    let second = Command::SetCanvas {
        canvas: CanvasSpec::with_ratio(4.0 / 3.0, 594.0),
    };
    history.apply(first.clone()).expect("applies");
    let wide = history.doc().clone();
    history.apply(second.clone()).expect("applies");

    assert!(history.undo());
    assert_eq!(history.doc(), &wide);
    assert_eq!(history.redo_depth(), 1);

    // A different command from here: the undone one is no longer reachable.
    history
        .apply(Command::SetCanvas {
            canvas: CanvasSpec::with_ratio(4.0 / 3.0, 297.0),
        })
        .expect("applies");
    assert_eq!(history.redo_depth(), 0, "the redo path must be forked");
    assert!(!history.redo());
    assert!(history.undo());
    assert_eq!(history.doc(), &wide, "undo still walks the real history");
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
    // `mosaic-5-hero` is 4:3 like the canvas, with five slots.
    let template = templates::get("mosaic-5-hero").expect("registered");
    let canvas = CanvasSpec::with_ratio(template.aspect, 297.0);
    history
        .apply(Command::SetTemplate {
            template: template.clone(),
            canvas,
        })
        .expect("applies");

    let doc = history.doc();
    assert_eq!(doc.template, template);
    assert_eq!(doc.canvas, canvas);
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
