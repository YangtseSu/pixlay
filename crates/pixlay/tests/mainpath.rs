//! The main path, walked by machine: pick photos → Next → pick a layout → adjust
//! framing → export.
//!
//! The step's human criterion is that a person can walk this in under three
//! minutes; what this test adds is that the *whole path* works at all — the same
//! calls the widgets make, in the same order, with the document, the decode, the
//! undo stack, the save and the export all checked at the end. It also checks the
//! two things the criterion "normal under Wayland, with no blocking UI" means
//! mechanically: the export call returns immediately and the work happens on
//! another thread.
//!
//! **Re-routed 2026-09-22** (ruling 12): the path starts by picking photos in the
//! picker stage, and the document only exists once Next has been pressed. The
//! retired walk — "pick a template, then place photos into an empty sheet" — is in
//! `docs/archive/2026-09-20-STEPS.md`; S13 rewrote this test for the path that
//! replaced it.

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pixlay::canvas::Gesture;
use pixlay::export::Settings;
use pixlay::window::Stage;

use pixlay_core::{CollageDoc, Command, CropTransform, Project};
use pixlay_imaging::encode::Format;

#[test]
fn the_main_path_can_be_walked() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let started = Instant::now();

    // ---- pick photos ------------------------------------------------------
    // Stages 1–2: the window opens on the picker, and the fixture folder stands in
    // for the user's own (the folder chooser's dialog is the only part this walk
    // cannot drive).
    assert_eq!(
        window.stage(),
        Stage::Picker,
        "the window opens on the picker's stage"
    );
    let picker = window.picker().expect("the window has a picker stage");
    picker.open_folder(&window, &support::fixtures().join("photos"));
    assert!(
        picker.len() >= 4,
        "the fixture folder has the photos this walk needs ({} found)",
        picker.len()
    );
    // The four photos this walk uses, in the order they are picked — which is the
    // order of the cells, and is asserted against the CLI's own `init --photo`
    // below.
    let wanted = ["landscape.jpg", "portrait.jpg", "square.png", "dated.jpg"];
    let mut positions = Vec::new();
    for name in wanted {
        let index = picker
            .files()
            .iter()
            .position(|path| path.ends_with(name))
            .unwrap_or_else(|| panic!("{name} is not in the fixture folder"));
        positions.push(index);
    }
    for position in &positions {
        picker.toggle(&window, *position as u32);
    }
    let picked = picker.selection().photos().to_vec();
    assert_eq!(picked.len(), 4, "four photos were picked");
    assert!(
        picked
            .iter()
            .zip(wanted)
            .all(|(path, name)| path.ends_with(name)),
        "the pick keeps the order the photos were clicked in: {picked:?}"
    );
    // Nothing is a document yet: the picker's stage is the user's, and Next is what
    // turns a pick into one.
    assert_eq!(
        window
            .document()
            .cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count(),
        0,
        "picking photos does not touch the document"
    );

    // ---- Next: the pick becomes a document --------------------------------
    assert!(picker.can_continue(), "four photos are enough to continue");
    picker.next(&window);
    assert_eq!(
        window.stage(),
        Stage::Editor,
        "Next moves to the editor's stage"
    );
    let doc = window.document();
    assert_eq!(doc.cells.len(), 4, "the layout has a cell per photo");
    assert!(
        doc.cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count()
            == 4,
        "every cell holds a photo"
    );
    // The tray's order and the CLI's argument order are one policy: the same four
    // paths through `pixlay_core::Selection`, which is what `init --photo` uses.
    let written = support::artifact("mainpath-init.pixlay");
    let argv: Vec<std::ffi::OsString> = {
        let mut argv: Vec<std::ffi::OsString> = ["init", "--template", &doc.template.name, "--out"]
            .iter()
            .map(std::ffi::OsString::from)
            .collect();
        argv.push(written.clone().into());
        for path in &picked {
            argv.push("--photo".into());
            argv.push(path.clone().into());
        }
        argv
    };
    let status = pixlay_cli::cli::run(&argv).expect("the CLI writes the project");
    assert_eq!(status, 0, "init --photo succeeds on the same selection");
    // The CLI rebases each `source` relative to the project file (that is what
    // makes a project movable) and a rebased path is joined lexically, so the two
    // sides are compared as files rather than as spellings: the order is the claim.
    let from_cli = Project::load(&written).expect("the CLI's project loads");
    let resolved: Vec<PathBuf> = from_cli
        .sources()
        .expect("its photos resolve")
        .into_iter()
        .map(|source| same_file(&source.expect("every cell holds its photo")))
        .collect();
    let expected: Vec<PathBuf> = picked.iter().map(|path| same_file(path)).collect();
    assert_eq!(
        resolved, expected,
        "the tray's order is the CLI's argument order, cell for cell"
    );
    assert_eq!(
        from_cli.doc().cells.len(),
        doc.cells.len(),
        "and the same number of cells"
    );

    assert!(
        window.wait_for_idle(support::WAIT),
        "the background decode finished"
    );
    assert_eq!(
        window.images().1.len(),
        4,
        "four bitmaps reached the canvas"
    );

    // ---- adjust the framing ---------------------------------------------
    // What the photos left behind, so the undo walk below has an exact target.
    let after_photos = window.document();
    window.select(Some(1));
    window.set_zoom(1.6);
    window.straighten(-7.5);
    window.commit();
    window.gesture(Gesture::Crop {
        slot: 1,
        crop: CropTransform {
            zoom: 1.6,
            offset: (0.1, 0.05),
            rotation_deg: -7.5,
        },
    });
    window.gesture(Gesture::End);
    let framed = window.document().cells[1].crop;
    assert!(
        framed.zoom >= 1.6 - 1e-9,
        "the zoom the user asked for survives the fit: {framed:?}"
    );
    assert!(
        (framed.rotation_deg - (-7.5)).abs() < 1e-9,
        "a square slot can afford a small angle: {framed:?}"
    );
    assert_ne!(framed.offset, (0.0, 0.0), "the pan was applied");

    // Three gestures, three undo steps (S6.5's "one gesture = one command"), and
    // undoing them returns the document to exactly what the photos made.
    for step in 0..3 {
        assert!(window.can_undo(), "gesture {step} is on the undo stack");
        window.undo();
    }
    assert_eq!(
        &window.document(),
        &after_photos,
        "undoing the framing returns the document to what the photos made"
    );
    for _ in 0..3 {
        assert!(window.can_redo());
        window.redo();
    }
    assert_eq!(window.document().cells[1].crop, framed, "redo comes back");

    // ---- export ----------------------------------------------------------
    // The export form's state, which ruling 18 moved out of the pane and S15's
    // `Export…` dialog will show as its rows.
    let out = support::artifact("mainpath.jpg");
    let settings = Settings {
        long_edge: 1500,
        format: Format::Jpeg,
        path: out.clone(),
    };
    window.set_export_settings(&settings);
    let echoed = window.export_settings();
    assert_eq!(
        echoed.long_edge, 1500,
        "the form's quality option is read back"
    );
    assert_eq!(echoed.format, Format::Jpeg, "and its format");
    // The background path: the call has to return while the work happens on the
    // export thread, or the window would be frozen for the whole render.
    let call = Instant::now();
    window.start_export(out.clone());
    let returned = call.elapsed();
    assert!(
        returned < Duration::from_millis(500),
        "starting an export blocked for {returned:?}"
    );
    assert!(window.wait_for_idle(support::WAIT), "the export finished");
    assert!(out.is_file(), "the export landed on disk");

    let exported = pixlay_imaging::Source::decode(&out).expect("the export decodes");
    // The one quality option is the long edge in pixels (S12d): the export is
    // the template's own aspect at that edge.
    let expected = pixlay_core::PixelSize::for_long_edge(window.document().template.aspect, 1500)
        .expect("the grid the export asked for");
    assert_eq!(
        (exported.width(), exported.height()),
        (expected.width as u32, expected.height as u32),
        "the export is the template's shape at the requested edge"
    );

    // The synchronous path, which is the same function, at the size the test can
    // check exactly.
    let png = support::artifact("mainpath.png");
    let report = window
        .export_to(&Settings {
            long_edge: 1500,
            format: Format::Png,
            path: png.clone(),
        })
        .expect("the export runs");
    assert_eq!(report.path, png);
    assert!(report.bytes > 0 && report.width > 0);
    assert_eq!(
        report.long_edge, 1500,
        "the report echoes the edge it was asked for"
    );

    // ---- save and reopen -------------------------------------------------
    let project = support::artifact("mainpath.pixlay");
    window.save_to(&project).expect("the project is written");
    assert!(!window.is_dirty(), "saving clears the unsaved marker");
    assert!(project.is_file());
    let reopened = CollageDoc::load(&project).expect("the saved project loads");
    assert_eq!(reopened, window.document());

    let other = support::second_window(&app);
    other.open_path(&project).expect("the project reopens");
    assert_eq!(other.document(), window.document());
    assert!(!other.is_dirty(), "a freshly opened project is not dirty");
    assert_eq!(
        other.stage(),
        Stage::Editor,
        "opening a project lands on the editor's stage, not the picker"
    );

    // ---- a photo that is not there ---------------------------------------
    // The contract's "a missing photo is visible, not silent": the slot renders
    // white, the window says what is wrong and offers to fix it, the export
    // refuses instead of writing a hole, and the other slots still draw.
    let missing_started = Instant::now();
    let gone = support::out_dir().join("gone.jpg");
    let _ = std::fs::remove_file(&gone);
    window.set_export_settings(&Settings {
        long_edge: 1000,
        format: Format::Jpeg,
        path: support::artifact("missing.jpg"),
    });
    window
        .apply(Command::SetSource {
            slot: 2,
            source: Some(gone.clone()),
        })
        .expect("a path that does not exist is still a legal document");
    assert!(window.wait_for_idle(support::WAIT), "the decode finished");
    assert_eq!(window.missing_photos(), vec![2], "the slot is reported");
    assert!(
        window.notice().is_some_and(|notice| notice.contains('1')),
        "the window has to say that a photo is missing, got {:?}",
        window.notice()
    );
    assert_eq!(
        window.images().1.len(),
        3,
        "the other three slots still have bitmaps"
    );
    assert!(
        window
            .export_to(&Settings {
                long_edge: 1000,
                format: Format::Jpeg,
                path: support::artifact("missing.jpg"),
            })
            .is_err(),
        "an export with a missing photo is refused rather than written"
    );
    eprintln!(
        "the missing-photo case took {:?}",
        missing_started.elapsed()
    );

    // What the window looks like at the end of the walk, for a human to look at:
    // everything above is numbers, and `AGENTS.md` asks for the picture as well.
    let picture = support::artifact("window.png");
    support::save_png(&picture, &support::snapshot(&window));
    eprintln!(
        "the whole walk, machine-driven, took {:?}; the window is {picture:?}",
        started.elapsed()
    );
}

/// The file a path names, resolved: `../../../…` and the path it points at are
/// the same photo, and the claim under test is the *order*, not the spelling.
///
/// A path that cannot be resolved (it does not exist) is returned as it stands,
/// so the failure still names what was looked for.
fn same_file(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
