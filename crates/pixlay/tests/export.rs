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
use libadwaita::prelude::*;

use pixlay::export::Settings;
use pixlay::i18n::gettext;
use pixlay_imaging::encode::Format;

#[test]
fn the_export_refuses_a_source_image_and_confirms_a_replacement() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    // The first export of a window that has never exported goes to the **pictures
    // directory** (the 2026-09-24 ruling; S15h, PIX-010), under a name derived from the
    // template — not to the process's own working directory, which is where a bare
    // suggested name used to land.
    let first = window.export_settings();
    let expected_dir = match pixlay::export::default_folder() {
        Some(pictures) => pictures,
        // An account with no pictures directory keeps the bare name, which is what the
        // product falls back to and what `parent()` reports as the empty path.
        None => std::path::PathBuf::new(),
    };
    assert_eq!(
        first.path.parent().map(std::path::Path::to_path_buf),
        Some(expected_dir),
        "the first export's directory is the pictures directory ({:?})",
        first.path
    );
    assert_eq!(
        first
            .path
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("jpg"),
        "and the name carries the selected format's extension"
    );
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
        support::alert_button(&window, &gettext("Replace")).is_none(),
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
        support::alert_labels(&window),
        vec![gettext("Cancel"), gettext("Replace")],
        "the confirmation's buttons are not Cancel then Replace"
    );

    let cancel = support::alert_button(&window, &gettext("Cancel"))
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
    let replace = support::alert_button(&window, &gettext("Replace"))
        .expect("the confirmation is asked again");
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
    // A later export opens where the last one landed, which is the other half of the
    // 2026-09-24 ruling (S15h, PIX-010): the dialog's rows are the stored path's own
    // directory and name.
    dialog.seed(&window);
    assert_eq!(
        dialog.settings().path,
        existing,
        "the dialog is seeded with the directory and name the last export used"
    );

    // ---- the default widget (S15h, PIX-010) ------------------------------
    // Presented again: the replacement above ran from a click that closed the dialog,
    // and Return's binding is a property of the presented dialog.
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.export", None).is_ok(),
        "the win.export action presents the dialog"
    );
    assert!(
        dialog.widget().is_visible(),
        "the Export dialog is presented again"
    );
    // HIG `patterns/feedback/dialogs`: a dialog with an affirmative action binds Return
    // to it, and the binding *is* the `default-widget` property (libadwaita's own words
    // for it: "The default widget. It's activated when the user presses Enter").
    let default = dialog
        .widget()
        .property::<Option<gtk4::Widget>>("default-widget")
        .expect("the Export dialog has a default widget");
    let export_button = dialog.export_button();
    assert_eq!(
        default,
        export_button.clone().upcast::<gtk4::Widget>(),
        "the dialog's default widget is its affirmative"
    );
    assert_eq!(
        export_button.label().map(|label| label.to_string()),
        Some(gettext("Export")),
        "and the affirmative carries the verb"
    );
    let by_default = support::artifact("s15h-default.png");
    let _ = std::fs::remove_file(&by_default);
    window.set_export_settings(&Settings {
        long_edge: 800,
        format: Format::Png,
        path: by_default.clone(),
    });
    dialog.seed(&window);
    assert_eq!(dialog.settings().path, by_default, "the rows describe it");
    // **The activation itself**, on the widget the property names: Enter reaches this
    // through libadwaita's own binding ("The default widget. It's activated when the
    // user presses Enter"). A `GtkButton` animates press-then-release before it clicks
    // (GTK's documented activation behaviour), so the export starts a few frames after
    // the call rather than inside it.
    assert!(
        default.activate(),
        "the default widget is activatable, which is what Return does"
    );
    let started = support::settle_by(&window, support::WAIT, || {
        (window.progress_revealed(), window.progress_revealed())
    });
    assert!(
        started,
        "activating the default widget started the export (toast {:?}, exporting {})",
        window.last_toast(),
        window.exporting()
    );
    support::close_dialog(&dialog.widget(), &window);
    assert!(
        window.wait_for_idle(support::WAIT),
        "the export the default widget started finished"
    );
    assert_eq!(
        std::fs::read(&by_default)
            .expect("the default widget's export landed")
            .first()
            .copied(),
        Some(0x89),
        "and it is the PNG the rows asked for"
    );

    // ---- the name the format row owns (S15h, PIX-010) --------------------
    // Both JPEG spellings are the JPEG format, case-insensitively — the CLI's own rule
    // for `--out` (`Format::from_path`) — so a name that already means the selected
    // format keeps the user's spelling, and one that means the other format is rewritten
    // in the row the user is looking at.
    dialog.name_row().set_text("holiday.JPEG");
    dialog.format_row().set_selected(0);
    assert_eq!(
        dialog.name_row().text(),
        "holiday.JPEG",
        "a name that already means JPEG is not rewritten"
    );
    dialog.format_row().set_selected(1);
    assert_eq!(
        dialog.name_row().text(),
        "holiday.png",
        "switching to PNG rewrites the name, case-insensitively"
    );
    dialog.format_row().set_selected(0);
    assert_eq!(
        dialog.name_row().text(),
        "holiday.jpg",
        "and back to JPEG with the format's own extension"
    );
    // A name with no extension, one with an extension this build does not write, and no
    // name at all are refused where the CLI refuses them: nothing is written, the dialog
    // stays open, and the toast says what is missing.
    for (refused, expected) in [
        ("holiday", "needs an extension"),
        ("holiday.2024", "this build writes"),
        ("", "Type a name"),
    ] {
        dialog.name_row().set_text(refused);
        let toasts = window.toasts();
        dialog.export_button().emit_clicked();
        support::pump(Duration::from_millis(50));
        assert_eq!(
            window.toasts(),
            toasts + 1,
            "{refused:?}: the refusal is reported once"
        );
        assert!(
            window
                .last_toast()
                .is_some_and(|toast| toast.contains(expected)),
            "{refused:?}: the refusal says what is missing ({expected}), got {:?}",
            window.last_toast()
        );
        assert!(
            !window.progress_revealed(),
            "{refused:?}: a refused name must not start an export"
        );
        assert!(
            dialog.widget().is_visible(),
            "{refused:?}: the dialog stays open for the name to be fixed"
        );
    }

    // ---- the writer's own guard (S15h, PIX-010) --------------------------
    // The dialog resolves the name before it starts anything, and `export::run` asks the
    // same question again: a path that names the other format than the form does not
    // write a file that lies about itself, whoever calls it.
    let mismatch = support::artifact("s15h-mismatch.png");
    let _ = std::fs::remove_file(&mismatch);
    let error = window
        .export_to(&Settings {
            long_edge: 800,
            format: Format::Jpeg,
            path: mismatch,
        })
        .expect_err("a JPEG form writing to a .png name is refused");
    assert!(error.contains("expected .jpg or .jpeg"), "{error}");

    support::close_dialog(&dialog.widget(), &window);
}
