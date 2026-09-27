//! S23b's exit criteria: what the canvas takes from outside — a drop that lands where
//! it is aimed, and the clipboard (`Ctrl+C` / `Ctrl+X` / `Ctrl+V`) on the selected
//! cell's photo.
//!
//! **The rules the criteria are about** (ruling 41): the first file of an arrival takes
//! the cell it was aimed at — filled when it is empty, replaced when it holds a photo —
//! and the files after it fill the empty cells from there on in reading order, wrapping
//! around the sheet. A drop never replaces a cell it did not land on, so on a full
//! collage the rest are ignored and the count that did not fit is reported **once**.
//! One arrival is one `Command::PlacePhotos`, so it is one undo step whatever it
//! touched; a cut-then-paste is one `Command::MovePhoto`, which empties the cell the
//! photo was cut from; and an image copied in another application is written out as a
//! real file first, because a document references paths.
//!
//! **What is driven how.** `GtkDropTarget`'s own `drop` handler cannot be reached
//! without a seat (the limit `support::press` documents for keys: GTK has no way to
//! move a pointer), so a drop is driven through the call that handler makes —
//! `EditorWindow::drop_files(paths, at)` with `at` from the same `slot_at_widget` the
//! handler reads — and a paste through the real `GdkClipboard` (this window's own file
//! list, a file manager's file list, or a texture another application would put there)
//! and the real window actions, which are the closures the accelerators run. What the
//! pointer does with a `GdkDrop` and what the seat does with `Ctrl+V` are the human
//! walk at the gate's items; the document, the history, the toast count and the file a
//! pasted bitmap writes are what a machine can hold this step to, and all of them are
//! here.

mod support;

use std::path::{Path, PathBuf};

use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use pixlay::i18n::{fill, ngettext};
use pixlay::window::EditorWindow;
use pixlay_core::{CollageDoc, CropTransform, Project};

#[test]
fn what_the_canvas_takes_from_outside() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens");
    let _ = support::canvas_bitmaps(&window);
    assert_eq!(
        window.document().cells.len(),
        8,
        "the verification document is the eight-cell one"
    );

    let files: Vec<PathBuf> = ["alpha.png", "photo-gray.png", "square.png"]
        .iter()
        .map(|name| support::photo(name))
        .collect();

    // ---- a drop lands where it was aimed, and nowhere else -----------------
    // The document is full: the first file replaces the cell it was dropped on, and
    // the two after it have no empty cell to go to — they are ignored and reported
    // once, because a drop never quietly replaces a cell it did not land on.
    let before = window.document();
    let toasts = window.toasts();
    let depth = window.undo_depth();
    window.drop_files(files.clone(), Some(4));
    let after = window.document();
    assert_eq!(
        source(&after, 4),
        Some(resolved(&files[0])),
        "the cell the drop was aimed at took the first file"
    );
    for slot in 0..8 {
        if slot != 4 {
            assert_eq!(
                after.cells[slot], before.cells[slot],
                "cell {slot} changed although the drop never landed on it"
            );
        }
    }
    assert_eq!(
        window.toasts(),
        toasts + 1,
        "one report, not one per file that did not fit"
    );
    let report = window.last_toast().expect("the drop reported itself");
    // The report is the product's own message with the count filled in — read through
    // `i18n` rather than written out here, because a catalog may translate it.
    let expected = fill(
        ngettext(
            "{} photo did not fit in the collage",
            "{} photos did not fit in the collage",
            2,
        ),
        &[2],
    );
    assert_eq!(
        report, expected,
        "the report says how many did not fit, and says it once"
    );
    assert_eq!(
        window.undo_depth(),
        depth + 1,
        "the whole arrival is one undo step"
    );
    assert_eq!(
        window.selection(),
        Some(4),
        "and the selection is the cell it landed on"
    );
    window.undo();
    assert_eq!(window.document(), before, "one undo puts the document back");

    // ---- the aimed cell fills, and the rest fill in reading order ----------
    // The criterion's own shape: cell 4 holds a photo, cells 5 and 6 are empty, and
    // three files dropped on cell 4 land in 4, 5 and 6 — the cell the pointer was over
    // and the empty cells after it.
    window.clear_cell(5);
    window.clear_cell(6);
    // A framing of the user's own on the cell the drop is aimed at, so "the replaced
    // cell keeps the area the user framed" is a claim about a value that could be lost.
    let framing = CropTransform {
        zoom: 1.6,
        offset: (0.08, -0.05),
        rotation_deg: 9.0,
    };
    window
        .apply(pixlay_core::Command::SetCrop {
            slot: 4,
            crop: framing,
        })
        .expect("cell 4 takes a framing");
    assert_eq!(window.document().cells[4].crop, framing);
    let before = window.document();
    let toasts = window.toasts();
    let depth = window.undo_depth();
    window.drop_files(files.clone(), Some(4));
    let after = window.document();
    for (offset, file) in files.iter().enumerate() {
        assert_eq!(
            source(&after, 4 + offset),
            Some(resolved(file)),
            "the {offset}th file of the drop is in cell {}",
            4 + offset
        );
    }
    assert_eq!(
        after.cells[4].crop, framing,
        "the replaced cell kept the area the user framed (the Replace rule)"
    );
    for slot in [0, 1, 2, 3, 7] {
        assert_eq!(
            after.cells[slot], before.cells[slot],
            "cell {slot} is untouched"
        );
    }
    assert_eq!(
        window.toasts(),
        toasts,
        "an arrival that fits reports nothing"
    );
    assert_eq!(window.undo_depth(), depth + 1, "one arrival, one step");
    assert!(
        after.cells[5].source.is_some() && after.cells[6].source.is_some(),
        "the two empty cells were filled"
    );

    // ---- the reading order wraps around the sheet --------------------------
    // One empty cell, and the aim is the last cell: the second file has nowhere to go
    // but the cell the wrap reaches, which is the empty one.
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens again");
    let _ = support::canvas_bitmaps(&window);
    window.clear_cell(0);
    let toasts = window.toasts();
    let depth = window.undo_depth();
    window.drop_files(files[..2].to_vec(), Some(7));
    let after = window.document();
    assert_eq!(source(&after, 7), Some(resolved(&files[0])));
    assert_eq!(
        source(&after, 0),
        Some(resolved(&files[1])),
        "the second file wrapped around the sheet to the empty cell"
    );
    assert_eq!(window.undo_depth(), depth + 1);
    assert_eq!(
        window.toasts(),
        toasts,
        "both files landed, so nothing to report"
    );

    // ---- one file on an empty cell fills it and touches nothing else -------
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens again");
    let _ = support::canvas_bitmaps(&window);
    window.clear_cell(3);
    let before = window.document();
    window.drop_files(vec![files[0].clone()], Some(3));
    let after = window.document();
    assert_eq!(source(&after, 3), Some(resolved(&files[0])));
    for slot in 0..8 {
        if slot != 3 {
            assert_eq!(
                after.cells[slot], before.cells[slot],
                "cell {slot} is untouched"
            );
        }
    }

    // ---- `Ctrl+C` + `Ctrl+V`: copy, into a cell that holds a photo and into one
    // that does not ---------------------------------------------------------
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens again");
    let _ = support::canvas_bitmaps(&window);
    let copied = window.document().cells[0]
        .source
        .clone()
        .expect("cell 0 holds a photo");
    window.select(Some(0));
    assert_eq!(
        window.action_enabled("copy"),
        Some(true),
        "there is a photo to copy"
    );
    activate(&window, "copy");
    assert_eq!(
        window.document().cells[0].source,
        Some(copied.clone()),
        "a copy does not touch the document"
    );

    // Replace: the paste goes into a cell that holds a photo, and that cell keeps its
    // own framing while the source keeps its photo.
    let target_framing = window.document().cells[5].crop;
    window.select(Some(5));
    assert_eq!(
        window.action_enabled("paste"),
        Some(true),
        "the clipboard holds a file"
    );
    activate(&window, "paste");
    let after = until(&window, |doc| source(doc, 5) == Some(resolved(&copied)));
    assert_eq!(
        source(&after, 5),
        Some(resolved(&copied)),
        "the paste replaced cell 5's photo"
    );
    assert_eq!(
        source(&after, 0),
        Some(resolved(&copied)),
        "a copy leaves the photo it copied where it was"
    );
    assert_eq!(
        after.cells[5].crop, target_framing,
        "the pasted cell keeps the framing it had (the Replace rule)"
    );

    // Fill: an empty cell takes it the same way.
    window.clear_cell(6);
    window.select(Some(6));
    assert_eq!(window.action_enabled("paste"), Some(true));
    activate(&window, "paste");
    let after = until(&window, |doc| source(doc, 6) == Some(resolved(&copied)));
    assert_eq!(source(&after, 6), Some(resolved(&copied)));

    // ---- `Ctrl+X` + `Ctrl+V` is one step and leaves the source empty --------
    // A framing of the user's own on the cell that is cut, so "the cell it was cut from
    // is empty, framing and all" is a claim about a value that could be left behind.
    let moved_framing = CropTransform {
        zoom: 1.4,
        offset: (-0.06, 0.03),
        rotation_deg: -12.0,
    };
    window
        .apply(pixlay_core::Command::SetCrop {
            slot: 2,
            crop: moved_framing,
        })
        .expect("cell 2 takes a framing");
    let moved = window.document().cells[2]
        .source
        .clone()
        .expect("cell 2 holds a photo");
    window.select(Some(2));
    let depth = window.undo_depth();
    activate(&window, "cut");
    assert_eq!(window.cut_source(), Some(2), "the cut remembers its cell");
    assert_eq!(
        window.document().cells[2].source,
        Some(moved.clone()),
        "a cut changes nothing yet — the edit happens where the paste lands"
    );
    window.select(Some(6));
    activate(&window, "paste");
    let after = until(&window, |doc| doc.cells[6].source.as_ref() == Some(&moved));
    assert_eq!(
        source(&after, 6),
        Some(resolved(&moved)),
        "the photo arrived in the cell the paste was aimed at"
    );
    assert_eq!(
        after.cells[2],
        pixlay_core::Cell::default(),
        "and the cell it was cut from is empty, framing and all"
    );
    assert_eq!(window.cut_source(), None, "the paste spent the cut");
    assert_eq!(
        window.undo_depth(),
        depth + 1,
        "a cut-then-paste is one undo step, not two"
    );
    window.undo();
    let back = window.document();
    assert_eq!(
        source(&back, 2),
        Some(resolved(&moved)),
        "one undo brings it back"
    );
    assert_eq!(back.cells[2].crop, moved_framing, "with the framing it had");

    // A cut is spent by the paste that used it: pasting again is a copy, so the cell
    // the photo came from stays empty.
    window.redo();
    window.select(Some(6));
    assert_eq!(window.cut_source(), None);
    activate(&window, "copy");
    window.select(Some(3));
    activate(&window, "paste");
    let after = until(&window, |doc| source(doc, 3) == Some(resolved(&moved)));
    assert!(
        source(&after, 6).is_some(),
        "the second paste copied rather than moved"
    );

    // ---- a file manager's clipboard behaves exactly like a drop ------------
    // Two files, put on the clipboard the way another application puts them (a
    // `text/uri-list`), pasted into the aimed cell: the first replaces it and the
    // second takes the empty cell after it — and the whole paste is one step.
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens again");
    let _ = support::canvas_bitmaps(&window);
    window.clear_cell(5);
    let clipboard = window.clipboard();
    let list = gdk::FileList::from_array(&[
        gio::File::for_path(&files[0]),
        gio::File::for_path(&files[1]),
    ]);
    clipboard
        .set_content(Some(&gdk::ContentProvider::for_value(&list.to_value())))
        .expect("the clipboard takes a file list the way a file manager puts one there");
    assert!(
        wait_for(&window, |window| window.clipboard_usable()),
        "the window noticed the file list on the clipboard"
    );
    window.select(Some(4));
    let depth = window.undo_depth();
    activate(&window, "paste");
    let after = until(&window, |doc| source(doc, 5) == Some(resolved(&files[1])));
    assert_eq!(
        source(&after, 4),
        Some(resolved(&files[0])),
        "the first file replaced cell 4"
    );
    assert_eq!(
        source(&after, 5),
        Some(resolved(&files[1])),
        "the second filled cell 5"
    );
    assert_eq!(window.undo_depth(), depth + 1, "one paste, one undo step");

    // ---- an image with no file behind it is written out as a real file -----
    let texture = gdk::Texture::from_file(&gio::File::for_path(&files[1]))
        .expect("the fixture decodes through GDK");
    clipboard.set_texture(&texture);
    assert!(
        wait_for(&window, |window| window.clipboard_usable()),
        "the window noticed the texture on the clipboard"
    );
    window.clear_cell(1);
    window.select(Some(1));
    activate(&window, "paste");
    let after = until(&window, |doc| doc.cells[1].source.is_some());
    let pasted = after.cells[1].source.clone().expect("the paste landed");
    assert!(
        pasted.is_absolute(),
        "a pasted bitmap is written somewhere real: {pasted:?}"
    );
    assert_eq!(
        pasted.extension().and_then(|extension| extension.to_str()),
        Some("png"),
        "and written as a PNG: {pasted:?}"
    );
    // The location `docs/CONTRACT.md` §9 pins: the app's own cache, which is one rule
    // for a document that has a project directory and one that has none.
    assert!(
        pasted.starts_with(gtk4::glib::user_cache_dir().join("pixlay").join("pasted")),
        "the pasted file is not in the app's cache: {pasted:?}"
    );
    // Pasting the same image twice is one file, not a second copy of it.
    window.clear_cell(2);
    window.select(Some(2));
    activate(&window, "paste");
    let again = until(&window, |doc| doc.cells[2].source.is_some());
    assert_eq!(
        again.cells[2].source, after.cells[1].source,
        "the same image pasted again is the same file"
    );

    // The criterion's other half: the file survives a save and a reopen, because a
    // document holds paths and a path that is not there is a cell that shows nothing.
    let written = window
        .save_to(&support::out_dir().join("pasted.pixlay"))
        .expect("the window saves its document");
    let project = Project::load(&written).expect("the saved project loads");
    let sources = project
        .sources()
        .expect("every source in the saved project resolves");
    assert!(
        sources[1].is_some(),
        "the pasted photo is still a file after the save"
    );
    window
        .open_path(&written)
        .expect("the saved project reopens in the window");
    assert_eq!(
        source(&window.document(), 1),
        Some(resolved(&pasted)),
        "and it reopens pointing at the same file"
    );
    assert!(
        window.missing_photos().is_empty(),
        "nothing is missing after the reopen"
    );

    // ---- the sensitivity of the three items --------------------------------
    window.select(None);
    assert_eq!(
        window.action_enabled("copy"),
        Some(false),
        "no cell is selected"
    );
    assert_eq!(window.action_enabled("cut"), Some(false));
    assert_eq!(window.action_enabled("paste"), Some(false));

    window.clipboard().set_text("a note, not a photo");
    assert!(
        wait_for(&window, |window| !window.clipboard_usable()),
        "plain text is not something a cell can take"
    );
    window.select(Some(0));
    assert_eq!(
        window.action_enabled("paste"),
        Some(false),
        "a clipboard of plain text is nothing to paste"
    );
    assert_eq!(
        window.action_enabled("copy"),
        Some(true),
        "the selected cell holds a photo"
    );
    assert_eq!(window.action_enabled("cut"), Some(true));

    window.clear_cell(0);
    window.select(Some(0));
    assert_eq!(
        window.action_enabled("copy"),
        Some(false),
        "an empty cell has no photo to copy"
    );
    assert_eq!(window.action_enabled("cut"), Some(false));

    // ---- the CLI writes the same document ---------------------------------
    // The machine surface has no single command for "these cells take these files" —
    // that is the history's shape, not the document's — and it does not need one: the
    // same document is `edit --slot i --photo p` once per cell. The two files below are
    // the window's own drop from the fill case, written through the CLI.
    let dir = support::out_dir();
    let from_gui = dir.join("drop-gui.pixlay");
    let from_cli = dir.join("drop-cli.pixlay");
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens again");
    let _ = support::canvas_bitmaps(&window);
    window.clear_cell(5);
    window.clear_cell(6);
    window.drop_files(files.clone(), Some(4));
    let written = window
        .save_to(&from_gui)
        .expect("the window saves its document");

    let edit = |args: &[&str]| {
        let status = pixlay_cli::cli::run(&argv(args)).expect("the CLI runs");
        assert_eq!(status, 0, "the CLI edit succeeds: {args:?}");
    };
    edit(&[
        "edit",
        "--project",
        path(&support::verify_project()),
        "--slot",
        "5",
        "--clear",
        "--out",
        path(&from_cli),
    ]);
    edit(&[
        "edit",
        "--project",
        path(&from_cli),
        "--slot",
        "6",
        "--clear",
        "--out",
        path(&from_cli),
    ]);
    for (slot, file) in [(4, &files[0]), (5, &files[1]), (6, &files[2])] {
        edit(&[
            "edit",
            "--project",
            path(&from_cli),
            "--slot",
            &slot.to_string(),
            "--photo",
            path(file),
            "--out",
            path(&from_cli),
        ]);
    }
    let gui = Project::load(&written).expect("the window's project loads");
    let cli = Project::load(&from_cli).expect("the CLI's project loads");
    assert!(
        support::same_document(&gui, &cli),
        "the window's own drop and the CLI's per-cell edits produced different documents:\n\
         {:?}\n{:?}",
        gui.doc(),
        cli.doc()
    );
    eprintln!(
        "a three-file drop on cell 4 and `edit --slot i --photo p` per cell are one \
         document ({} cells, {})",
        cli.doc().cells.len(),
        cli.doc().template.name
    );
}

/// Runs the window action the accelerator runs: `win.cut` is one closure whether the
/// user pressed `Ctrl+X` or a test activated it.
fn activate(window: &EditorWindow, action: &str) {
    gtk4::prelude::WidgetExt::activate_action(window, &format!("win.{action}"), None)
        .unwrap_or_else(|_| panic!("the window has a `win.{action}` action"));
}

/// The document once `done` holds for it.
///
/// A paste reads the clipboard on the main loop, so its edit lands a frame or two after
/// the keystroke, and a wait that reads the *observation* is a condition rather than a
/// guess (`support::settle_by`).
fn until(window: &EditorWindow, mut done: impl FnMut(&CollageDoc) -> bool) -> CollageDoc {
    support::settle_by(window, support::PROBE_WAIT, || {
        let doc = window.document();
        let done = done(&doc);
        (doc, done)
    })
}

/// Pumps until `condition` holds, and says whether it ever did.
fn wait_for(window: &EditorWindow, mut condition: impl FnMut(&EditorWindow) -> bool) -> bool {
    support::settle_by(window, support::PROBE_WAIT, || {
        let held = condition(window);
        (held, held)
    })
}

/// A cell's photo, as the real file behind it.
///
/// A source may be spelled absolutely or relative to the project's directory (a drop
/// and a paste store what the file manager or the clipboard gave them, which is
/// absolute; a project saved beside its photos spells them relative), so both sides of
/// every comparison below resolve to the file itself.
fn source(doc: &CollageDoc, slot: usize) -> Option<PathBuf> {
    doc.cells[slot].source.as_deref().map(resolved)
}

fn resolved(path: &Path) -> PathBuf {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        support::fixtures().join(path)
    };
    full.canonicalize().unwrap_or(full)
}

fn argv(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(std::ffi::OsString::from).collect()
}

fn path(value: &Path) -> &str {
    value.to_str().expect("a UTF-8 path")
}
