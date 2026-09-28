// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S15d: the pending edit and the document survive every boundary that can end a
//! session (PIX-002, PIX-005, PIX-006, PIX-022).
//!
//! **The ruling this test holds the build to** (2026-09-24): *a boundary commits the
//! pending edit and then runs the dirty check.* Two consequences, and both are
//! checked here rather than described:
//!
//! * **Nothing pending is dropped.** A frame or crop change lives in the pending
//!   command for a quiet interval, and every boundary — the `Frame…` dialog's Close,
//!   the end of a crop gesture, Save, New, Open, export — commits it before it does
//!   anything else. The document that reaches the file, and the question that is
//!   asked about it, is the document on screen.
//! * **The question is one question.** Closing the window, `New` and `Open` all end
//!   or replace the document, so all three offer Cancel / Discard / Save, and `Save`
//!   continues the boundary only once the file was actually written — a failed save
//!   leaves the document exactly where it was.
//!
//! Two more claims from the same batch are here because they are facts about the
//! window rather than about `pixlay-core`: a Save As into another directory leaves
//! the *window* holding the document the file holds (PIX-005: the written bytes, the
//! in-memory resolution and a second save are the same document), and "dirty" is a
//! comparison with the file rather than a flag (PIX-022: an edit undone back to the
//! saved state is not unsaved work, and a command that changes nothing is not an undo
//! step).

mod support;

use std::path::{Path, PathBuf};

use gtk4::prelude::*;
use libadwaita::prelude::*;

use pixlay::canvas::Gesture;
use pixlay::i18n::gettext;
use pixlay::window::EditorWindow;
use pixlay_core::{CollageDoc, Command, CropTransform, Frame, templates};

/// The window's unsaved-work question, answered by clicking what a person clicks.
fn answer(window: &EditorWindow, label: &str) {
    let button = support::alert_button(window, label)
        .unwrap_or_else(|| panic!("the window is not showing a `{label}` button"));
    button.emit_clicked();
    support::pump(std::time::Duration::from_millis(50));
    // The question that was answered is off the screen, so the next one this test
    // asks is the next one it answers — not the same buttons again.
    assert!(
        support::alert(window).is_none(),
        "the answered question is still presented"
    );
}

/// This run's own directory: the project, its photos and every copy are here, so a
/// previous run cannot answer for this one.
fn scratch() -> PathBuf {
    let dir = support::out_dir().join("s15d");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test's own directory");
    dir
}

/// A two-photo project beside its own photos, with the sources **relative** — the
/// spelling a Save As into another directory has to rebase.
fn project_in(dir: &Path) -> PathBuf {
    let project_dir = dir.join("proj");
    std::fs::create_dir_all(project_dir.join("photos")).expect("the photos' directory");
    for (from, to) in [("square.png", "a.png"), ("ratio-4-3.png", "b.png")] {
        std::fs::copy(support::photo(from), project_dir.join("photos").join(to))
            .expect("the fixture photo is copied");
    }
    let template = templates::all()
        .into_iter()
        .find(|template| template.slots.len() == 2)
        .expect("the library has a two-slot layout");
    let mut doc = CollageDoc::new(template);
    doc.cells[0].source = Some(PathBuf::from("photos/a.png"));
    doc.cells[1].source = Some(PathBuf::from("photos/b.png"));
    let project = project_dir.join("collage.pixlay");
    doc.save(&project).expect("the test project is written");
    project
}

/// The file a slot's `source` resolves to when it is read from `project`, the way a
/// reader resolves it: the project's own directory plus the stored spelling.
fn resolved(project: &Path, source: &Path) -> PathBuf {
    let dir = project.parent().expect("the project has a directory");
    let path = dir.join(source);
    path.canonicalize()
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_pending_edit_and_the_document_survive_every_boundary() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let dir = scratch();
    let project = project_in(&dir);
    window.open_path(&project).expect("the test project opens");
    assert!(
        window.wait_for_idle(support::WAIT),
        "the project's photos decoded"
    );
    assert!(!window.is_dirty(), "an opened project is not unsaved work");

    // ---- the Frame dialog's Close commits the pending frame --------------
    // The rows write live: the canvas draws the value while the dialog is open and
    // the history does not have it yet. Closing the dialog is a boundary, so what it
    // commits is the value the user was looking at — not the last one that happened
    // to be quiet for 250 ms.
    let dialog = window
        .settings_dialog()
        .expect("the window has the settings dialog");
    window.select(None);
    let plain = window.document().frame;
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.frame", None).is_ok(),
        "the win.frame action is installed"
    );
    assert!(dialog.widget().is_visible(), "the dialog is presented");
    dialog.gap_row().set_value(4.0);
    assert_eq!(
        window.display_document().frame.gap_rel,
        0.04,
        "the canvas is drawing the row's value"
    );
    assert_eq!(
        window.document().frame.gap_rel,
        plain.gap_rel,
        "and the history does not have it yet"
    );
    // **Closing the dialog is the boundary** (S15d, PIX-002): `AdwDialog::closed` is
    // what commits the pending frame, so what it commits is the value the user was
    // looking at — not the last one that happened to be quiet for 250 ms.
    dialog.widget().close();
    support::close_dialog(&dialog.widget().upcast::<libadwaita::Dialog>(), &window);
    assert_eq!(
        window.document().frame.gap_rel,
        0.04,
        "closing the Frame dialog dropped the pending frame"
    );
    assert!(window.is_dirty(), "the committed frame is unsaved work");
    assert!(window.can_undo(), "and it is one undo step");
    window.undo();
    assert_eq!(window.document().frame, plain, "one undo walks it back");
    assert!(
        !window.is_dirty(),
        "undoing back to the file's own state is not unsaved work"
    );

    // ---- a crop gesture's end commits it ---------------------------------
    let slot = 0;
    window.select(Some(slot));
    let base = window.fitted_crop(slot).expect("the slot is framed");
    let target = CropTransform {
        zoom: base.zoom * 1.2,
        offset: (base.offset.0.clamp(-0.8, 0.8) + 0.05, base.offset.1),
        rotation_deg: base.rotation_deg + 6.0,
    };
    window.gesture(Gesture::Crop { slot, crop: target });
    assert_ne!(
        window.display_document().cells[slot].crop,
        window.document().cells[slot].crop,
        "the gesture is pending"
    );
    window.gesture(Gesture::End);
    assert_eq!(
        window.document().cells[slot].crop,
        window.display_document().cells[slot].crop,
        "the gesture ended into the document"
    );
    assert_ne!(
        window.document().cells[slot].crop,
        base,
        "and the document has the crop the gesture made"
    );

    // ---- Save commits the pending edit, and the file holds it ------------
    // The pending value is *not* committed first here: `save_to` is the boundary,
    // and the assertion is that the file the window wrote carries what the canvas
    // was showing.
    dialog.present(&window);
    dialog.gap_row().set_value(6.0);
    assert_eq!(window.display_document().frame.gap_rel, 0.06);
    assert_eq!(
        window.document().frame.gap_rel,
        plain.gap_rel,
        "still pending"
    );
    window.save_to(&project).expect("the project saves");
    assert_eq!(
        CollageDoc::load(&project)
            .expect("the written project loads")
            .frame
            .gap_rel,
        0.06,
        "the file does not hold the pending edit"
    );
    assert!(!window.is_dirty(), "saving is the file's own state again");
    support::close_dialog(&dialog.widget().upcast::<libadwaita::Dialog>(), &window);

    // ---- save → edit → undo is not dirty ---------------------------------
    window
        .apply(Command::ClearCell { slot: 1 })
        .expect("clearing a cell is accepted");
    assert!(window.is_dirty(), "the clear is unsaved work");
    window.undo();
    assert!(
        !window.is_dirty(),
        "an undo back to the saved state is not unsaved work"
    );

    // ---- a command that changes nothing is not a step --------------------
    window
        .apply(Command::SetFrame {
            frame: window.document().frame,
        })
        .expect("a command that asks for the document it already has is accepted");
    assert!(
        !window.is_dirty(),
        "a command that changes nothing is not unsaved work"
    );
    window.undo();
    assert_eq!(
        window.document().frame.gap_rel,
        plain.gap_rel,
        "the command that changed nothing was an undo step"
    );

    // ---- New: Cancel, Discard, and Save ----------------------------------
    // The document is the file's state plus one frame step, so every answer below
    // has something to decide about.
    window
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.08,
                ..window.document().frame
            },
        })
        .expect("an edit to lose");
    assert!(window.is_dirty());
    let edited = window.document();

    window.new_document();
    assert!(
        support::alert(&window).is_some_and(|alert| alert.is_visible()),
        "New does not ask about unsaved work"
    );
    assert_eq!(
        support::alert_labels(&window),
        vec![gettext("Cancel"), gettext("Discard"), gettext("Save")],
        "the question's answers are not Cancel / Discard / Save"
    );
    answer(&window, &gettext("Cancel"));
    assert_eq!(window.document(), edited, "Cancel replaced the document");
    assert!(window.is_dirty(), "Cancel lost the unsaved marker");
    assert_eq!(
        window.project_path(),
        Some(project.clone()),
        "Cancel moved the document's own file"
    );

    window.new_document();
    answer(&window, &gettext("Discard"));
    assert_ne!(window.document(), edited, "Discard did not replace it");
    assert!(!window.is_dirty(), "a fresh document is not unsaved work");
    assert_eq!(
        window.project_path(),
        None,
        "the fresh document kept the old file"
    );

    // Save: the file gets the edit, and *then* the document is replaced — the
    // continuation has to survive the save, not stop at it.
    window.open_path(&project).expect("the project reopens");
    window
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.10,
                ..window.document().frame
            },
        })
        .expect("an edit to save");
    window.new_document();
    answer(&window, &gettext("Save"));
    assert_eq!(
        CollageDoc::load(&project)
            .expect("the project loads")
            .frame
            .gap_rel,
        0.10,
        "Save did not write the edit"
    );
    assert_eq!(
        window.project_path(),
        None,
        "Save did not continue into the new document"
    );
    assert!(!window.is_dirty());

    // ---- Open: Cancel, Discard and Save ----------------------------------
    let other = dir.join("other.pixlay");
    let mut other_doc = CollageDoc::load(&project).expect("the project loads");
    other_doc.frame.gap_rel = 0.12;
    // Through `Project::save_as`, so the copy in another directory gets the rebasing
    // every written copy gets — `CollageDoc::save` writes the spellings as they are,
    // which is right for a document being written back to its own file and wrong for
    // one being written somewhere else.
    pixlay_core::Project::new(other_doc, &project)
        .expect("the second document is valid")
        .save_as(&other)
        .expect("the second project is written");

    window
        .open_path(&project)
        .expect("the first project reopens");
    window
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.14,
                ..window.document().frame
            },
        })
        .expect("an edit to lose");
    window.open_asking(&other);
    assert!(
        support::alert(&window).is_some(),
        "Open does not ask about unsaved work"
    );
    answer(&window, &gettext("Cancel"));
    assert_eq!(
        window.project_path(),
        Some(project.clone()),
        "Cancel opened the other document anyway"
    );
    assert!(window.is_dirty());

    window.open_asking(&other);
    answer(&window, &gettext("Discard"));
    assert_eq!(
        window.project_path(),
        Some(other.clone()),
        "Discard did not open the document that was chosen"
    );
    assert_eq!(
        window.document().frame.gap_rel,
        0.12,
        "the opened document is the file's"
    );
    assert!(!window.is_dirty());

    // And the third answer: the edit is written, and *then* the chosen document
    // opens — the continuation has to survive the save, not stop at it.
    window
        .open_path(&project)
        .expect("the first project reopens");
    window
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.14,
                ..window.document().frame
            },
        })
        .expect("an edit to save");
    window.open_asking(&other);
    answer(&window, &gettext("Save"));
    assert_eq!(
        CollageDoc::load(&project)
            .expect("the first project loads")
            .frame
            .gap_rel,
        0.14,
        "Save did not write the edit"
    );
    assert_eq!(
        window.project_path(),
        Some(other.clone()),
        "Save did not open the document that was chosen"
    );
    assert!(!window.is_dirty());

    // ---- Save As: the window adopts what the file holds ------------------
    // A copy in another directory: every relative source has to be rewritten, and
    // the window has to end up with the same spellings — the file's and the
    // window's are one document (PIX-005).
    let deep = dir.join("elsewhere").join("deep");
    std::fs::create_dir_all(&deep).expect("the copy's directory");
    let copy = deep.join("copy.pixlay");
    let before = window.document();
    window.save_to(&copy).expect("Save As writes the copy");
    let written = CollageDoc::load(&copy).expect("the copy loads");
    let adopted = window.document();
    assert_eq!(
        adopted, written,
        "the window is not holding the document the file holds"
    );
    assert_ne!(
        adopted, before,
        "Save As into another directory rebased nothing"
    );
    // Every source resolves to the photo it was pointing at, read the way a reader
    // reads it: the project's own directory plus the stored spelling.
    let photos = [
        dir.join("proj/photos/a.png")
            .canonicalize()
            .expect("a photo"),
        dir.join("proj/photos/b.png")
            .canonicalize()
            .expect("a photo"),
    ];
    for (index, cell) in written.cells.iter().enumerate() {
        let source = cell.source.as_deref().expect("the fixture has photos");
        assert_eq!(
            resolved(&copy, source),
            photos[index],
            "cell {index}: the copy points at a different photo"
        );
    }

    // And saving again writes the same bytes: the document in memory is already
    // expressed against the new directory, so a second save has nothing to rewrite.
    let first = std::fs::read(&copy).expect("read the copy");
    window.save_to(&copy).expect("the second save");
    assert_eq!(
        std::fs::read(&copy).expect("read the copy"),
        first,
        "saving twice wrote two different documents"
    );
    assert!(!window.is_dirty());

    // ---- a save that fails leaves the document where it was --------------
    // The failure is a directory that is not there: the writer's temporary file
    // cannot be created, which is the same shape a full disk has and does not depend
    // on who is running the test.
    let gone = dir.join("gone");
    std::fs::create_dir_all(&gone).expect("the directory to remove");
    let doomed = gone.join("collage.pixlay");
    window.save_to(&doomed).expect("the copy is written");
    window
        .apply(Command::SetFrame {
            frame: Frame {
                gap_rel: 0.16,
                ..window.document().frame
            },
        })
        .expect("an edit that cannot be saved");
    let edited = window.document();
    std::fs::remove_dir_all(&gone).expect("the directory goes away");
    let toasts = window.toasts();
    window.new_document();
    answer(&window, &gettext("Save"));
    assert_eq!(
        window.toasts(),
        toasts + 1,
        "the failed save is reported once"
    );
    assert!(
        window.last_toast().is_some(),
        "a failed save says nothing about it"
    );
    assert_eq!(
        window.document(),
        edited,
        "a failed save let the document be replaced anyway"
    );
    assert!(window.is_dirty(), "and the work is still unsaved");
    assert_eq!(
        window.project_path(),
        Some(doomed.clone()),
        "the document moved to a file that was never written"
    );
}
