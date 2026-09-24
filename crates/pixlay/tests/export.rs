//! S15c: the export surface, and the two things it refuses to do — write over one
//! of the document's own photos, and replace a file the user already has without
//! asking (PIX-001, PIX-011).
//!
//! The alias rule itself is `pixlay_imaging::destination`'s and the CLI pins it in
//! all four of its spellings (`pixlay-cli/tests/cli.rs`); what this test adds is the
//! wiring: the window's own verdict, the writer's guard, and the confirmation the
//! `Export…` dialog asks — driven by clicking what a person clicks, and read from
//! the files on disk rather than from the widgets.

mod support;

use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;

use pixlay::export::Settings;
use pixlay::i18n::gettext;
use pixlay::window::EditorWindow;
use pixlay_imaging::encode::Format;

/// The alert dialog the window is currently showing, if it is showing one.
///
/// The confirmation is an `AdwAlertDialog` over the export dialog, and finding it in
/// the widget tree is how a test answers it, since the binding exposes no `response()`
/// to call (S15c).
fn alert(window: &EditorWindow) -> Option<adw::AlertDialog> {
    support::descendants(window.upcast_ref::<gtk4::Widget>())
        .into_iter()
        .find_map(|widget| widget.downcast::<adw::AlertDialog>().ok())
}

/// The button carrying `label` in that alert.
fn alert_button(window: &EditorWindow, label: &str) -> Option<gtk4::Button> {
    let alert = alert(window)?;
    let mut found = None;
    walk(alert.upcast_ref::<gtk4::Widget>(), &mut |widget| {
        if found.is_none()
            && let Some(button) = widget.downcast_ref::<gtk4::Button>()
            && button.label().as_deref() == Some(label)
        {
            found = Some(button.clone());
        }
    });
    found
}

/// The alert's own buttons, in the order the alert lays them out.
///
/// A walk over `first_child` / `next_sibling`, in order, rather than
/// [`support::descendants`] — whose depth-first stack visits siblings in reverse, and
/// HIG's criterion here *is* an order (the cancel button comes first), so a check that
/// read the tree backwards would pass for the wrong reason.
fn alert_labels(window: &EditorWindow) -> Vec<String> {
    let Some(alert) = alert(window) else {
        return Vec::new();
    };
    let mut labels = Vec::new();
    walk(alert.upcast_ref::<gtk4::Widget>(), &mut |widget| {
        if let Some(button) = widget.downcast_ref::<gtk4::Button>()
            && let Some(label) = button.label()
        {
            labels.push(label.to_string());
        }
    });
    labels
}

/// Calls `visit` on every widget under `root`, in tree order.
fn walk(root: &gtk4::Widget, visit: &mut impl FnMut(&gtk4::Widget)) {
    visit(root);
    let mut child = root.first_child();
    while let Some(widget) = child {
        walk(&widget, visit);
        child = widget.next_sibling();
    }
}

#[test]
fn the_export_refuses_a_source_image_and_confirms_a_replacement() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let project = support::verify_project();
    window
        .open_path(&project)
        .expect("the fixture project opens");
    assert!(
        window.wait_for_idle(support::WAIT),
        "the project's photos decoded"
    );

    // One of the document's own photos, as the window resolves it: the fixture stores
    // `photos/portrait.jpg` relative to the project, and this is that file.
    let photo = support::photo("portrait.jpg");
    let source = window
        .document()
        .cells
        .iter()
        .find_map(|cell| cell.source.clone())
        .expect("the fixture has photos");
    assert_eq!(
        project
            .parent()
            .expect("the fixture has a directory")
            .join(&source),
        photo,
        "the test's photo is not the document's first source"
    );
    let before = std::fs::read(&photo).expect("read the photo");

    // ---- the writer's own guard ------------------------------------------
    // The synchronous path is the same function the worker calls, so this is the
    // export itself refusing rather than the form: no pixels are written anywhere.
    let error = window
        .export_to(&Settings {
            long_edge: 800,
            format: Format::Jpeg,
            path: photo.clone(),
        })
        .expect_err("an export over one of the document's photos is refused");
    assert!(error.contains("refusing to write"), "{error}");
    assert!(
        error.contains("portrait.jpg"),
        "the message names the photo: {error}"
    );
    assert_eq!(
        std::fs::read(&photo).expect("read the photo"),
        before,
        "the refused export changed the photo"
    );

    // The verdict the form asks before it closes: a photo is an error, a free path is
    // nothing to ask about, and a file that is there is a question.
    assert!(window.export_destination(&photo).is_err());
    let free = support::artifact("s15c-free.png");
    let _ = std::fs::remove_file(&free);
    assert!(
        !window
            .export_destination(&free)
            .expect("a free path is writable"),
        "a path with nothing at it needs no confirmation"
    );
    let existing = support::artifact("s15c-existing.jpg");
    std::fs::write(&existing, b"an export from yesterday").expect("write");
    let yesterday = std::fs::read(&existing).expect("read");
    assert!(
        window
            .export_destination(&existing)
            .expect("an occupied path is writable"),
        "a file that is already there is the user's question"
    );

    // ---- the dialog: a refusal, then a replacement to confirm -------------
    let dialog = window
        .export_dialog()
        .expect("the window has an Export dialog");
    window.export();
    assert!(
        dialog.widget().is_visible(),
        "the Export dialog is presented"
    );
    let toasts = window.toasts();

    // A name that is one of the document's photos: refused, reported, and the dialog
    // stays open so the name can be fixed.
    window.set_export_settings(&Settings {
        long_edge: 800,
        format: Format::Jpeg,
        path: photo.clone(),
    });
    dialog.seed(&window);
    dialog.export_button().emit_clicked();
    support::pump(Duration::from_millis(50));
    assert_eq!(window.toasts(), toasts + 1, "the refusal is reported once");
    assert!(
        window
            .last_toast()
            .is_some_and(|toast| toast.contains("refusing to write")),
        "the toast says why, got {:?}",
        window.last_toast()
    );
    assert!(
        !window.progress_revealed(),
        "a refused export must not start"
    );
    assert!(
        dialog.widget().is_visible(),
        "the dialog stays open for the name to be fixed"
    );
    assert!(
        alert_button(&window, &gettext("Replace")).is_none(),
        "a refusal is not a replacement to confirm"
    );
    assert_eq!(
        std::fs::read(&photo).expect("read the photo"),
        before,
        "the photo changed"
    );

    // A file that is already there: asked about, and cancelling changes nothing.
    window.set_export_settings(&Settings {
        long_edge: 800,
        format: Format::Jpeg,
        path: existing.clone(),
    });
    dialog.seed(&window);
    dialog.export_button().emit_clicked();
    support::pump(Duration::from_millis(50));
    // HIG `patterns/feedback/dialogs`, "Confirmation Dialogs": the cancel button comes
    // first, before the affirmative, and the two are the whole alert.
    assert_eq!(
        alert_labels(&window),
        vec![gettext("Cancel"), gettext("Replace")],
        "the confirmation's buttons are not Cancel then Replace"
    );

    let cancel = alert_button(&window, &gettext("Cancel"))
        .expect("replacing an existing file is confirmed before it happens");
    cancel.emit_clicked();
    support::pump(Duration::from_millis(50));
    assert!(
        !window.progress_revealed(),
        "a cancelled export must not run"
    );
    assert_eq!(
        std::fs::read(&existing).expect("read"),
        yesterday,
        "cancelling changed the file"
    );
    assert_eq!(window.toasts(), toasts + 1, "cancelling is not a failure");

    // And the same click again, answered: the export runs and the file is replaced.
    dialog.export_button().emit_clicked();
    support::pump(Duration::from_millis(50));
    let replace =
        alert_button(&window, &gettext("Replace")).expect("the confirmation is asked again");
    replace.emit_clicked();
    assert!(
        window.progress_revealed(),
        "the confirmed export starts on the worker"
    );
    assert!(window.wait_for_idle(support::WAIT), "the export finished");
    let written = std::fs::read(&existing).expect("read");
    assert_ne!(written, yesterday, "the export did not replace the file");
    assert_eq!(
        &written[..2],
        &[0xff, 0xd8],
        "the replacement is the JPEG that was asked for"
    );
    assert_eq!(
        window.export_settings().path,
        existing,
        "the form remembers where the export went"
    );

    support::close_dialog(&dialog.widget(), &window);
}
