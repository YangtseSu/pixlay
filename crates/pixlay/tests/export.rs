//! S15c and S25: the export surface, and the two things it refuses to do — write over
//! one of the document's own photos, and write bytes under a name that lies about them.
//!
//! The alias rule itself is `pixlay_imaging::destination`'s and the CLI pins it in all
//! four of its spellings (`pixlay-cli/tests/cli.rs`); what this test adds is the wiring:
//! the window's own verdict, the writer's guard, and — since S25 — the seed the
//! platform's own save dialog opens on (ruling 36) and what the path it answers becomes.
//!
//! **The replace confirmation is the platform's own since ruling 36**, so it is not
//! pressed here: a native dialog is nothing a machine can drive. What *is* asserted is
//! the app's own half of that rule — an existing file is written without a second
//! question of ours, because the platform asked.

mod support;

use std::time::Duration;

use pixlay::export::{self, Request};
use pixlay::settings;
use pixlay_imaging::encode::Format;

#[test]
fn the_export_takes_the_settings_and_refuses_a_source_image_or_a_lying_name() {
    support::start();
    let app = support::app();
    let window = support::window(&app);

    // ---- the seed (S25, ruling 36) ---------------------------------------
    // A fresh account has no settings file: the save dialog opens on the pictures
    // directory — `XDG_PICTURES_DIR` or `~/Pictures`, the same answer the platform's own
    // chooser gives — and suggests a name carrying the settings' format's extension.
    let expected_dir = pixlay::export::default_folder();
    assert!(
        !settings::Settings::path().is_some_and(|path| path.exists()),
        "the harness gives this binary an account with no settings file"
    );
    let seed = window.export_seed();
    assert_eq!(
        seed.folder, expected_dir,
        "the first export opens on the pictures directory"
    );
    assert!(
        seed.name.ends_with(".jpg"),
        "the suggested name carries the default format's extension, got {:?}",
        seed.name
    );

    // A remembered folder is what the next dialog opens on; one that is no longer there
    // falls back to the pictures directory rather than opening somewhere that is not.
    let exports = support::out_dir().join("s25-exports");
    std::fs::create_dir_all(&exports).expect("the export directory can be created");
    let remembered = settings::Settings {
        format: Format::Jpeg,
        long_edge: 800,
        last_export_dir: Some(exports.clone()),
    };
    window.remember_settings(&remembered);
    assert_eq!(window.export_seed().folder, Some(exports.clone()));
    window.remember_settings(&settings::Settings {
        last_export_dir: Some(exports.join("gone")),
        ..remembered.clone()
    });
    assert_eq!(
        window.export_seed().folder,
        expected_dir,
        "a remembered directory that is not there falls back to the pictures directory"
    );
    window.remember_settings(&remembered);

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
    // export itself refusing rather than the window: no pixels are written anywhere.
    let error = window
        .export_to(&Request {
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

    // The window's own verdict, which is what the save dialog's answer meets first: a
    // photo is an error and a free path is not.
    assert!(window.export_destination(&photo).is_err());
    let free = support::artifact("s25-free.png");
    let _ = std::fs::remove_file(&free);
    assert!(
        window.export_destination(&free).is_ok(),
        "a path with nothing at it is writable"
    );

    // ---- the name the settings' format owns (S25) ------------------------
    // The extension is the whole interface between a file and its pixels, and since S25
    // the settings' format is the only format an export has: a name that disagrees with
    // it is refused rather than renamed, because the name came from the platform's own
    // dialog and a file that lands under a name the dialog never showed would be worse
    // than a toast. Both JPEG spellings are the JPEG format, case-insensitively
    // (`Format::from_path`, the CLI's `--out` rule).
    for (name, ok) in [
        ("holiday.jpg", true),
        ("holiday.jpeg", true),
        ("holiday.JPEG", true),
        ("holiday.png", false),
        ("holiday", false),
        ("holiday.2024", false),
    ] {
        assert_eq!(
            export::extension_error(std::path::Path::new(name), Format::Jpeg).is_ok(),
            ok,
            "{name:?} against JPEG"
        );
    }
    assert!(export::extension_error(std::path::Path::new("shot.PNG"), Format::Png).is_ok());
    assert!(export::extension_error(std::path::Path::new("shot.jpg"), Format::Png).is_err());

    // And through the flow the dialog's answer takes: one toast, no export started, and
    // nothing written where the name pointed.
    let mismatched = support::out_dir().join("s25-mismatch.png");
    let _ = std::fs::remove_file(&mismatched);
    let toasts = window.toasts();
    window.export_to_chosen(&mismatched);
    support::pump(Duration::from_millis(50));
    assert_eq!(window.toasts(), toasts + 1, "the refusal is reported once");
    assert!(
        window
            .last_toast()
            .is_some_and(|toast| toast.contains("expected .jpg or .jpeg")),
        "the toast says what the name needs, got {:?}",
        window.last_toast()
    );
    assert!(!window.progress_revealed(), "a refusal starts no export");
    assert!(!mismatched.exists(), "and writes nothing");

    // ---- a file that is already there -------------------------------------
    // Since ruling 36 the *platform's* dialog confirms a replacement, so the app's own
    // half is that it writes over the file without a second question — and remembers
    // the folder the file landed in, which is where the next dialog opens.
    let existing = support::artifact("s25-existing.jpg");
    std::fs::write(&existing, b"an export from yesterday").expect("write");
    let yesterday = std::fs::read(&existing).expect("read");
    window.export_to_chosen(&existing);
    assert!(
        window.progress_revealed(),
        "the export starts on the worker at once"
    );
    assert!(
        support::alert(&window).is_none(),
        "the app asks nothing of its own: the platform's dialog asked"
    );
    assert!(window.wait_for_idle(support::WAIT), "the export finished");
    assert!(!window.progress_revealed(), "and the bar goes away again");
    let written = std::fs::read(&existing).expect("read");
    assert_ne!(written, yesterday, "the export replaced the file");
    assert_eq!(
        &written[..2],
        &[0xff, 0xd8],
        "and it is the JPEG the settings asked for"
    );
    assert!(
        window
            .last_toast()
            .is_some_and(|toast| toast.contains("s25-existing.jpg")),
        "the toast names the file: {:?}",
        window.last_toast()
    );
    // The settings file now remembers the folder, which is the other half of ruling 36.
    let file = settings::Settings::path().expect("this account has a settings file");
    let stored = settings::Settings::read(&file);
    assert_eq!(
        stored.last_export_dir,
        existing.parent().map(std::path::Path::to_path_buf)
    );
    assert_eq!(
        window.export_seed().folder,
        existing.parent().map(std::path::Path::to_path_buf)
    );

    // ---- a source image is refused before anything is spawned -------------
    // The same refusal as the synchronous path above, now through the window's own
    // pre-flight: a toast, no progress bar, and the photo untouched.
    let toasts = window.toasts();
    window.export_to_chosen(&photo);
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
    assert_eq!(
        std::fs::read(&photo).expect("read the photo"),
        before,
        "the photo changed"
    );

    // ---- the writer's own guard (S15h, PIX-010) --------------------------
    // The window resolves the name before it starts anything, and `export::run` asks the
    // same question again: a path that names the other format than the settings do not
    // write a file that lies about itself, whoever calls it.
    let mismatch = support::artifact("s25-writer.png");
    let _ = std::fs::remove_file(&mismatch);
    let error = window
        .export_to(&Request {
            long_edge: 800,
            format: Format::Jpeg,
            path: mismatch,
        })
        .expect_err("a JPEG request writing to a .png name is refused");
    assert!(error.contains("expected .jpg or .jpeg"), "{error}");
}
