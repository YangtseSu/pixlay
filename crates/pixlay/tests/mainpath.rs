//! The main path, walked by machine: pick a template → place photos → adjust
//! framing → export.
//!
//! The step's human criterion is that a person can walk this in under three
//! minutes; what this test adds is that the *whole path* works at all — the same
//! calls the widgets make, in the same order, with the document, the decode, the
//! undo stack, the save and the export all checked at the end. It also checks the
//! two things the criterion "normal under Wayland, with no blocking UI" means
//! mechanically: the export call returns immediately and the work happens on
//! another thread.

mod support;

use std::time::{Duration, Instant};

use pixlay::canvas::Gesture;
use pixlay::export::{Settings, Size};
use pixlay_core::{CollageDoc, CropTransform};
use pixlay_imaging::encode::{Chroma, Format};

#[test]
fn the_main_path_can_be_walked() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let started = Instant::now();

    // ---- pick a template ------------------------------------------------
    window.set_template("grid-4-2x2");
    assert_eq!(window.document().template.name, "grid-4-2x2");
    assert_eq!(window.document().cells.len(), 4, "four slots to fill");

    // ---- place photos ----------------------------------------------------
    window.select(Some(0));
    window.place_photo(0, support::photo("landscape.jpg"));
    // The other three arrive as a drop, which is the gesture the canvas handles:
    // the pointer is over slot 0, and the drop fills the slots after it.
    window.drop_files(
        vec![
            support::photo("portrait.jpg"),
            support::photo("square.png"),
            support::photo("dated.jpg"),
        ],
        Some(0),
    );
    let doc = window.document();
    assert_eq!(
        doc.cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count(),
        4,
        "every slot holds a photo"
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
    let out = support::artifact("mainpath.jpg");
    let settings = Settings {
        size: Size::LongEdge(600),
        format: Format::Jpeg,
        chroma: Chroma::Full,
        path: out.clone(),
    };
    // The background path: the call has to return while the work happens on the
    // export thread, or the window would be frozen for the whole render.
    window.set_export_settings(&settings);
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
    assert_eq!(
        (exported.width(), exported.height()),
        (600, 600),
        "the square canvas of `grid-4-2x2` asked for a 600 px long edge"
    );

    // The synchronous path, which is the same function, at the size the test can
    // check exactly.
    let png = support::artifact("mainpath.png");
    let report = window
        .export_to(&Settings {
            size: Size::Dpi(150),
            format: Format::Png,
            chroma: Chroma::Full,
            path: png.clone(),
        })
        .expect("the export runs");
    assert_eq!(report.path, png);
    assert!(report.bytes > 0 && report.width > 0);
    assert!(
        (report.dpi - 150.0).abs() < 1e-9,
        "a DPI export carries the resolution it was asked for"
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

    // ---- a photo that is not there ---------------------------------------
    // The contract's "a missing photo is visible, not silent": the slot renders
    // white, the window says what is wrong and offers to fix it, the export
    // refuses instead of writing a hole, and the other slots still draw.
    let missing_started = Instant::now();
    let gone = support::out_dir().join("gone.jpg");
    let _ = std::fs::remove_file(&gone);
    window.set_export_settings(&Settings {
        size: Size::LongEdge(400),
        format: Format::Jpeg,
        chroma: Chroma::Full,
        path: support::artifact("missing.jpg"),
    });
    window
        .apply(pixlay_core::Command::SetSource {
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
                size: Size::LongEdge(400),
                format: Format::Jpeg,
                chroma: Chroma::Full,
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
