//! The main path, walked by machine: open → Add photos… → pick a layout → adjust
//! framing → export → save → reopen, plus the same path's second entry, the command
//! line.
//!
//! The step's human criterion is that a person can walk this in under three
//! minutes; what this test adds is that the *whole path* works at all — the same
//! calls the widgets make, in the same order, with the document, the decode, the
//! undo stack, the save and the export all checked at the end. It also checks the
//! two things the criterion "normal under Wayland, with no blocking UI" means
//! mechanically: the export call returns immediately and the work happens on
//! another thread.
//!
//! **Re-routed 2026-09-25** (ruling 31, landed by S22): the window opens on the
//! collage itself and the photos enter through `Add photos…` — the multi-file chooser
//! whose callback is `EditorWindow::add_photos`, the call this walk makes. The walk it
//! replaced began in the picker stage and pressed Next.

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use pixlay::canvas::Gesture;
use pixlay::export::Settings;

use pixlay_core::{CollageDoc, Command, CropTransform, Project, Rgba8};
use pixlay_imaging::encode::Format;

#[test]
fn the_main_path_can_be_walked() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let started = Instant::now();

    // ---- the window opens on a collage ------------------------------------
    // No picker stage, no Next (S22, ruling 31): the window is an editable collage
    // from the first frame, and what it opens on is the one-cell default layout with
    // an empty cell — whose own `+` is the pointer's way in.
    let opening = window.document();
    assert_eq!(opening.cells.len(), 1, "a new collage is one cell");
    assert!(
        opening.cells[0].source.is_none(),
        "and that cell is empty, waiting for its photo"
    );
    assert_eq!(window.selection(), None, "nothing is selected yet");
    assert_eq!(
        window.title().map(|title| title.to_string()).as_deref(),
        Some("Untitled collage"),
        "the window is named after the document"
    );
    // Every entry point of the new document exists as an action or a control: the
    // menu's `Add photos…` (`Ctrl+I`), the empty cell's `+` and the strip's `Replace`
    // (both `tests/compose.rs`'), and a drop (the canvas's own target).
    assert_eq!(
        window.action_enabled("add-photos"),
        Some(true),
        "the window has an Add photos action"
    );
    assert_eq!(
        window.action_enabled("save"),
        Some(true),
        "and the Save action, whose header button was removed"
    );
    // Ruling 37: the header bar holds **no Save button** — it sat beside Export and
    // read as the same action. The function lives in the menu and on `Ctrl+S`.
    let header = window.header().expect("the window has a header bar");
    let saved = support::descendants(header.upcast_ref::<gtk4::Widget>())
        .into_iter()
        .any(|widget| support::action_name(&widget).as_deref() == Some("win.save"));
    assert!(!saved, "the header bar still carries a Save control");

    // ---- Add photos…: the chooser's own callback --------------------------
    // The fixture folder stands in for the user's own; `choose_photos` presents a
    // native dialog whose callback ends in exactly this call, so the walk drives the
    // half a test can drive.
    let wanted = ["landscape.jpg", "portrait.jpg", "square.png", "dated.jpg"];
    let photos: Vec<PathBuf> = wanted.iter().map(|name| support::photo(name)).collect();
    window.add_photos(photos.clone());
    let doc = window.document();
    assert_eq!(doc.cells.len(), 4, "the layout grew to a cell per photo");
    assert!(
        doc.cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count()
            == 4,
        "every cell holds a photo"
    );
    assert!(
        doc.cells
            .iter()
            .zip(&photos)
            .all(|(cell, path)| cell.source.as_ref() == Some(path)),
        "the order the chooser gave is the order of the cells: {:?}",
        doc.cells
            .iter()
            .map(|cell| &cell.source)
            .collect::<Vec<_>>()
    );
    // The list's order and the CLI's argument order are one policy: the same four
    // paths through `pixlay_core::Selection`, which is what `init --photo` uses.
    let written = support::artifact("mainpath-init.pixlay");
    let argv: Vec<std::ffi::OsString> = {
        let mut argv: Vec<std::ffi::OsString> = ["init", "--template", &doc.template.name, "--out"]
            .iter()
            .map(std::ffi::OsString::from)
            .collect();
        argv.push(written.clone().into());
        for path in &photos {
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
    let expected: Vec<PathBuf> = photos.iter().map(|path| same_file(path)).collect();
    assert_eq!(
        resolved, expected,
        "the chooser's order is the CLI's argument order, cell for cell"
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

    // ---- pick a layout ----------------------------------------------------
    // The band under the canvas lists every layout with four slots and choosing one
    // is a document edit that keeps the cells that survive (S14, ruled 25).
    let gallery = window.gallery().expect("the editor has a layout band");
    assert!(window.wait_for_gallery(support::WAIT), "the band was built");
    assert_eq!(gallery.count_label().label(), "4");
    let candidates = gallery.candidates();
    assert!(
        candidates.len() >= 3,
        "every count has at least three layouts: {candidates:?}"
    );
    assert!(
        candidates.contains(&doc.template.name),
        "the document's own layout is one of the candidates: {candidates:?}"
    );
    assert_eq!(
        gallery.selected().as_deref(),
        Some(doc.template.name.as_str()),
        "and the band highlights it"
    );
    let chosen = candidates
        .iter()
        .find(|name| name.as_str() != doc.template.name)
        .expect("a second candidate");
    window.select_layout(chosen);
    assert!(
        window.wait_for_idle(support::WAIT) && window.wait_for_gallery(support::WAIT),
        "the new layout rendered"
    );
    let relaid = window.document();
    assert_eq!(relaid.template.name, *chosen);
    assert_eq!(relaid.cells.len(), 4, "one cell per slot");
    for slot in 0..4 {
        assert_eq!(
            relaid.cells[slot].source, doc.cells[slot].source,
            "cell {slot} kept its photo across the layout change"
        );
    }
    assert_eq!(
        gallery.selected().as_deref(),
        Some(chosen.as_str()),
        "the band highlights the layout the document is on"
    );

    // The count control, on the same band (S14b): `−` takes the layout with one
    // cell fewer and `+` gives it back, empty — the control moves the layout, so a
    // wrong count is fixed without re-adding photos and without `+` meaning two
    // different things depending on history.
    assert!(gallery.minus_button().is_sensitive());
    window.remove_photo();
    assert!(
        window.wait_for_idle(support::WAIT) && window.wait_for_gallery(support::WAIT),
        "the shorter layout rendered"
    );
    assert_eq!(
        window.document().cells.len(),
        3,
        "the layout gave up a cell"
    );
    assert_eq!(window.photo_count(), 3, "and the photo went with it");
    window.add_photo();
    assert!(
        window.wait_for_idle(support::WAIT) && window.wait_for_gallery(support::WAIT),
        "the grown layout rendered"
    );
    assert_eq!(window.document().cells.len(), 4, "four cells again");
    assert!(
        window.document().cells[3].source.is_none(),
        "and the new cell is empty — it is the layout that grew"
    );
    assert_eq!(
        window.photo_count(),
        3,
        "so the photo count is one below the cell count, and the next thing the \
         window offers is the empty cell's own `+`"
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

    // The same edits from the selected cell's own controls (S15): a pointer has a
    // visible path to the zoom, the rotation and the clear, and each button is one
    // finished step — the same undo step the wheel's notch and the `+` key make.
    let controls = window
        .cell_controls()
        .expect("the canvas has a cell-control layer");
    let [zoom_out, zoom_in, rotate, _replace, _swap, clear] = controls.strip_buttons();
    let before = window.document().cells[1].crop;
    zoom_in.emit_clicked();
    assert!(
        window.document().cells[1].crop.zoom > before.zoom,
        "the strip's zoom-in control zooms in"
    );
    rotate.emit_clicked();
    assert!(
        window.document().cells[1].crop.rotation_deg > before.rotation_deg,
        "the strip's rotate control turns the photo"
    );
    window.undo();
    window.undo();
    assert_eq!(
        window.document().cells[1].crop,
        before,
        "the strip's edits are undo steps like any other"
    );
    let _ = (zoom_out, clear);

    // ---- the frame --------------------------------------------------------
    // The document-level question, where ruling 18 left it: three rows behind the
    // header bar's button, over `frame{gapRel, radiusRel, color}` (ruling 30).
    // The rows write live — the canvas redraws behind the dialog — and the settled
    // value is one undo step, which is what `commit` is here.
    let frame_dialog = window
        .frame_dialog()
        .expect("the window has a Frame dialog");
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.frame", None).is_ok(),
        "the win.frame action is installed"
    );
    assert!(frame_dialog.widget().is_visible());
    frame_dialog.gap_row().set_value(3.0);
    frame_dialog.radius_row().set_value(1.5);
    frame_dialog
        .color_button()
        .set_rgba(&gtk4::gdk::RGBA::new(0.25, 0.25, 0.25, 1.0));
    window.commit();
    let framed_doc = window.document();
    assert_eq!(
        framed_doc.frame.gap_rel, 0.03,
        "the gap row reached the file"
    );
    assert_eq!(framed_doc.frame.radius_rel, 0.015, "and the radius row");
    assert_eq!(framed_doc.frame.color, Rgba8::rgb(64, 64, 64));
    assert!(
        window.wait_for_idle(support::WAIT),
        "the framed re-decode finished"
    );
    support::close_dialog(&frame_dialog.widget(), &window);
    // The frame is a document field the CLI writes with its own flags, so the same
    // edit has to be expressible there: the three numbers in `edit`'s vocabulary.
    let stored_frame = support::artifact("mainpath-framed.pixlay");
    window
        .save_to(&stored_frame)
        .expect("the framed document saves");
    let framed_edit = support::artifact("mainpath-edged.pixlay");
    let status = pixlay_cli::cli::run(&[
        "edit".into(),
        "--project".into(),
        stored_frame.clone().into(),
        "--gap".into(),
        "0.05".into(),
        "--out".into(),
        framed_edit.clone().into(),
    ])
    .expect("the CLI edits the frame");
    assert_eq!(status, 0, "edit --gap succeeds on the window's own project");
    let reloaded = Project::load(&framed_edit).expect("the edited project loads");
    assert_eq!(reloaded.doc().frame.gap_rel, 0.05);
    assert_eq!(
        reloaded.doc().frame.radius_rel,
        0.015,
        "an unnamed flag keeps the document's own value"
    );
    assert_eq!(
        reloaded.doc().frame.color,
        Rgba8::rgb(64, 64, 64),
        "and so does the colour"
    );

    // ---- export ----------------------------------------------------------
    // The `Export…` dialog (S15) asks the three questions — the format, the one size
    // parameter, and the file — and starts the same background export the menu's
    // action does, with the same progress bar and the same toast.
    let out = support::artifact("mainpath.jpg");
    let export_dialog = window
        .export_dialog()
        .expect("the window has an Export dialog");
    window.set_export_settings(&Settings {
        long_edge: 1500,
        format: Format::Jpeg,
        path: out.clone(),
    });
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.export", None).is_ok(),
        "the win.export action is installed"
    );
    assert!(export_dialog.widget().is_visible());
    window.pump(Duration::from_millis(50));
    assert_eq!(
        export_dialog.quality_row().value(),
        1500.0,
        "the dialog opens on the form's own size"
    );
    // The background path: the call has to return while the work happens on the
    // export thread, or the window would be frozen for the whole render.
    // The dialog's own affirmative is clicked, which is what a person does: it reads
    // the rows, stores them, closes and starts the export.
    let call = Instant::now();
    export_dialog.export_button().emit_clicked();
    let returned = call.elapsed();
    assert!(
        returned < Duration::from_millis(500),
        "starting an export blocked for {returned:?}"
    );
    assert!(
        window.progress_revealed(),
        "the progress bar is raised while the export runs"
    );
    // The click started libadwaita's own close transition; the exporter runs in the
    // background either way, and the harness forces the dismissal so the final
    // snapshot of the window is of the window without a dialog over it.
    support::close_dialog(&export_dialog.widget(), &window);
    let echoed = window.export_settings();
    assert_eq!(
        echoed.long_edge, 1500,
        "the form's quality option is read back"
    );
    assert_eq!(echoed.format, Format::Jpeg, "and its format");
    assert!(window.wait_for_idle(support::WAIT), "the export finished");
    assert!(!window.progress_revealed(), "and the bar goes away again");
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
    assert!(
        other.gallery().is_some(),
        "the reopened window is the editor, band and all"
    );

    // ---- `Ctrl+S` still saves ---------------------------------------------
    // Ruling 37 removed the header bar's Save *button*, not saving: the command is
    // still the accelerator's action (`win.save`, bound to `<Control>s` in
    // `app::ACCELERATORS`), and on a document that has a path it writes without
    // asking anything.
    window.select(Some(0));
    window.set_zoom(2.2);
    window.commit();
    assert!(window.is_dirty(), "the edit is unsaved work");
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.save", None).is_ok(),
        "the win.save action is installed"
    );
    assert!(!window.is_dirty(), "Ctrl+S's action wrote the document");
    let written_again = CollageDoc::load(&project).expect("the saved project loads");
    assert_eq!(
        written_again,
        window.document(),
        "and wrote what is on screen"
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
    // How many bitmaps the canvas should hold: one per cell that names a photo,
    // minus the one that cannot be read. Derived from the document rather than
    // written down, because `+` can leave a cell empty (S14b) and the count is
    // therefore a fact about this walk rather than a constant of the product.
    let expected_bitmaps = window
        .document()
        .cells
        .iter()
        .filter(|cell| cell.source.is_some())
        .count()
        - 1;
    assert_eq!(
        window.images().1.len(),
        expected_bitmaps,
        "the slots that still have a photo and a readable file still have bitmaps"
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

    // ---- the other entry: `pixlay a.jpg b.jpg …` --------------------------
    // The command line is a *second* way into the same path, and the walk drives it the
    // way a process does: no window at all, `open` with the files, and the handler
    // builds the window and puts the photos in in argument order (S22, ruling 31). Both
    // of the walk's own windows are closed first, because "no window yet" is the state a
    // command line starts in — `HANDLES_OPEN` is what routes a process's arguments to
    // that handler, and `app::open_files` is the handler itself, the same function
    // `connect_open` calls.
    other.destroy();
    window.destroy();
    support::pump(Duration::from_millis(300));
    assert!(
        app.flags()
            .contains(gtk4::gio::ApplicationFlags::HANDLES_OPEN),
        "the application handles open, or the arguments would go nowhere"
    );
    let argv_photos: Vec<PathBuf> = ["landscape.jpg", "square.png", "portrait.jpg"]
        .iter()
        .map(|name| support::photo(name))
        .collect();
    let files: Vec<gtk4::gio::File> = argv_photos.iter().map(gtk4::gio::File::for_path).collect();
    pixlay::app::open_files(&app, &files);
    let argv_window = app
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<pixlay::EditorWindow>().ok())
        .expect("the open handler built the window");
    support::watch(&argv_window);
    support::present(&argv_window);
    let argv_doc = argv_window.document();
    assert_eq!(
        argv_doc.cells.len(),
        3,
        "the photos opened a document with a cell each"
    );
    assert!(
        argv_doc
            .cells
            .iter()
            .zip(&argv_photos)
            .all(|(cell, path)| cell.source.as_ref() == Some(path)),
        "and the order is the argument order: {:?}",
        argv_doc
            .cells
            .iter()
            .map(|cell| &cell.source)
            .collect::<Vec<_>>()
    );
    assert!(
        argv_window.wait_for_idle(support::WAIT),
        "the command line's own photos decoded"
    );
    assert_eq!(argv_window.images().1.len(), 3, "and reached the canvas");

    // And that window exports its own document: the command line entered the one
    // document model and the one renderer, not a second path to the same screen.
    let rendered = support::artifact("mainpath-argv.png");
    let report = argv_window
        .export_to(&Settings {
            long_edge: 900,
            format: Format::Png,
            path: rendered.clone(),
        })
        .expect("the command line's own document exports");
    assert_eq!(report.path, rendered);
    assert!(rendered.is_file(), "and the export landed on disk");

    eprintln!(
        "the command line's own window opened on {} photos, {} cells, after {:?}",
        argv_doc
            .cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count(),
        argv_doc.cells.len(),
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
