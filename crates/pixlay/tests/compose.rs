//! S15's exit criteria, as one test: the compose stage's own controls and its two
//! dialogs.
//!
//! One `#[test]` because GTK lives on one thread (see `support`). What is checked
//! here, in the order S15's criteria are written:
//!
//! * the selected cell's strip is real, named, keyboard-reachable GTK and sits inside
//!   the selected slot's own rectangle (`Placement` is the reference);
//! * the strip and an empty cell's `+` never cover one cell at once, so "give this
//!   cell a photo" and "here is a photo's controls" are not two paths for one action;
//! * the buttons edit the document: the zoom pair, the rotate step, and the clear the
//!   canvas's own `Delete` performs;
//! * the `Frame…` dialog's three rows round-trip into the document and the canvas
//!   redraws with the frame's own colour;
//! * the settings' two parameters round-trip through one background export, with the
//!   progress bar raised while it runs and the toast carrying the file's name (S25:
//!   the export's own dialog is gone, so this is the path the platform's save dialog's
//!   answer takes);
//! * the header bar's buttons present the dialogs.
//!
//! What only a person can judge — whether a strip lands where the hand expects, and
//! whether the frame reads well at both ends of its radius range — is the human walk
//! at S15's gate, and it is listed in `docs/HIG-REVIEW.md` §2.

mod support;

use std::time::{Duration, Instant};

use gtk4::prelude::*;
use pixlay::canvas;
use pixlay_core::{PixelSize, Point, Rgba8};
use pixlay_imaging::encode::Format;

/// HIG `guidelines/pointer-touch`: "ensure that all interactive elements are at
/// least 24x24 pixels".
const MIN_TARGET: i32 = 24;

#[test]
fn the_compose_stage_edits_the_selected_cell_and_the_document() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens");
    let controls = window
        .cell_controls()
        .expect("the canvas has a cell-control layer");
    let canvas_widget = window.canvas_widget().upcast::<gtk4::Widget>();

    // ---- the strip is the selected cell's own controls ---------------------
    // Cell 0 is the verification template's largest cell (3/8 x 3/8), so the strip
    // has room for all six controls inside it.
    //
    // **Selected before the first frame is pumped, on purpose.** The bitmaps are not in
    // hand until the decoder answers, so this is the order in which the window has only
    // its `1x1` placeholder sheet to place a control against. What the checks below ask
    // is that the *arrival* places them (`EditorWindow::on_decoded`) — the test never
    // calls `CellControls::sync_in` for this block (measured 2026-09-24: without the
    // re-sync the strip was placed from the placeholder and stayed there, which is the
    // `0x0` a full-suite run reported).
    window.select(Some(0));
    let strip = controls.strip();
    // The window has only its `1x1` placeholder to place against at this point, and a
    // control placed from it is one GTK answers with a `0x0` allocation: nothing is shown
    // until the bitmaps' own grid is in hand.
    assert!(
        !strip.is_visible(),
        "no control is placed before its bitmaps are in hand"
    );
    support::canvas_bitmaps(&window);
    assert!(strip.is_visible(), "the selected cell's controls are shown");
    assert!(
        laid_out(&window, &strip, true),
        "the strip was never laid out as a row ({}x{}): {}",
        strip.width(),
        strip.height(),
        support::frame_state(&window),
    );
    for (index, button) in controls.strip_buttons().into_iter().enumerate() {
        let widget = button.clone().upcast::<gtk4::Widget>();
        assert!(button.is_visible(), "control {index} is on screen");
        assert!(button.is_focusable(), "control {index} is Tab-reachable");
        assert!(
            button.tooltip_text().is_some(),
            "control {index} has no tooltip"
        );
        assert!(
            button.width() >= MIN_TARGET && button.height() >= MIN_TARGET,
            "control {index} is {}x{}, below the {MIN_TARGET} px target",
            button.width(),
            button.height()
        );
        let _ = widget;
    }

    // **Inside the selected slot's own rectangle**, from the placement: the strip is
    // measured in the canvas's coordinates and compared with the slot's bbox mapped
    // through `Placement::to_widget`, which is the same arithmetic the renderer used
    // to draw the cell underneath it.
    let (grid, _) = window.images();
    let (width, height) = support::canvas_size(&window);
    let placement = canvas::placement(grid, width, height);
    let slot_box = window.document().template.slots[0].outline.bbox();
    let (cell_left, cell_top) = placement.to_widget(Point::new(slot_box.x0, slot_box.y0));
    let (cell_right, cell_bottom) = placement.to_widget(Point::new(slot_box.x1, slot_box.y1));
    let origin = strip
        .compute_point(&canvas_widget, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("the strip is in the canvas's own space");
    let (strip_left, strip_top) = (f64::from(origin.x()), f64::from(origin.y()));
    let (strip_right, strip_bottom) = (
        strip_left + f64::from(strip.width()),
        strip_top + f64::from(strip.height()),
    );
    assert!(
        strip_left >= cell_left && strip_right <= cell_right,
        "the strip ({strip_left:.0}..{strip_right:.0}) is not inside its cell's \
         width ({cell_left:.0}..{cell_right:.0})"
    );
    assert!(
        strip_top >= cell_top && strip_bottom <= cell_bottom,
        "the strip ({strip_top:.0}..{strip_bottom:.0}) is not inside its cell's \
         height ({cell_top:.0}..{cell_bottom:.0})"
    );
    eprintln!(
        "the selected cell's strip: {}x{} at {strip_left:.0},{strip_top:.0}; the cell \
         is {cell_left:.0},{cell_top:.0}-{cell_right:.0},{cell_bottom:.0}",
        strip.width(),
        strip.height()
    );

    // ---- a cell too narrow for the row gets the column ---------------------
    // The library's narrow panes are 61-122 device px wide at the default window, and
    // six 32-px controls cannot fit in one: the strip turns into a column inside the
    // cell instead of covering its neighbour (`strip-9-9x1`, the 16:9 strip, is the
    // layout the test uses; its panes measure 122x551 here).
    window.select_layout("strip-9-9x1");
    settle(&window);
    window.select(Some(0));
    settle(&window);
    let (width, height) = support::canvas_size(&window);
    controls.sync_in(&window, width, height);
    let strip = controls.strip();
    assert_eq!(
        strip.orientation(),
        gtk4::Orientation::Vertical,
        "a pane narrower than the row must get the column"
    );
    assert!(
        laid_out(&window, &strip, false),
        "the strip was never laid out as a column ({}x{}): {}",
        strip.width(),
        strip.height(),
        support::frame_state(&window)
    );
    let (grid, _) = window.images();
    let placement = canvas::placement(grid, width, height);
    let pane = window.document().template.slots[0].outline.bbox();
    let (pane_left, pane_top) = placement.to_widget(Point::new(pane.x0, pane.y0));
    let (pane_right, pane_bottom) = placement.to_widget(Point::new(pane.x1, pane.y1));
    let origin = strip
        .compute_point(&canvas_widget, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("the strip is in the canvas's own space");
    let (left, top) = (f64::from(origin.x()), f64::from(origin.y()));
    let (right, bottom) = (
        left + f64::from(strip.width()),
        top + f64::from(strip.height()),
    );
    assert!(
        left >= pane_left - 0.5
            && right <= pane_right + 0.5
            && top >= pane_top - 0.5
            && bottom <= pane_bottom + 0.5,
        "the column ({left:.0},{top:.0})-({right:.0},{bottom:.0}) is not inside its \
         pane ({pane_left:.0},{pane_top:.0})-({pane_right:.0},{pane_bottom:.0})"
    );
    eprintln!(
        "the narrow pane's strip: {}x{} at {left:.0},{top:.0}; its pane is \
         {pane_left:.0},{pane_top:.0}-{pane_right:.0},{pane_bottom:.0}",
        strip.width(),
        strip.height()
    );
    // Back to the verification project's own layout for the rest of the walk.
    window.select_layout("mosaic-8-s14");
    settle(&window);
    window.select(Some(0));
    settle(&window);

    // ---- the two families never cover one cell at once ---------------------
    // A cell that holds a photo has no `+` over it (S14b): the strip is what a
    // pointer gets there.
    assert!(
        !controls.button(0).expect("a first `+`").is_visible(),
        "an occupied cell must have no `+`"
    );
    // And an empty cell is the `+`, with no strip over it — so "give the cell a
    // photo" is never two controls saying the same thing.
    window.add_photo();
    settle(&window);
    assert!(window.document().cells[8].source.is_none());
    window.select(Some(8));
    settle(&window);
    let (width, height) = support::canvas_size(&window);
    controls.sync_in(&window, width, height);
    assert!(
        !controls.strip().is_visible(),
        "an empty cell must not carry a photo's controls"
    );
    let empty_button = controls.button(8).expect("the ninth `+`");
    assert!(
        empty_button.is_visible(),
        "the empty cell is the control that asks for a photo: {} cells on {}, slot 8 \
         {:?}, canvas {width}x{height}",
        window.document().cells.len(),
        window.current_template(),
        window
            .document()
            .cells
            .get(8)
            .map(|cell| cell.source.clone()),
    );
    window.undo();
    settle(&window);
    assert_eq!(
        window.document().cells.len(),
        8,
        "the walk is back on eight"
    );

    // ---- the buttons edit the cell ----------------------------------------
    window.select(Some(0));
    settle(&window);
    let [zoom_out, zoom_in, rotate, _replace, _swap, clear] = controls.strip_buttons();
    let before = window.document().cells[0].crop;
    zoom_in.emit_clicked();
    settle(&window);
    let zoomed = window.document().cells[0].crop;
    assert!(
        zoomed.zoom > before.zoom,
        "the zoom-in control did not zoom in: {before:?} → {zoomed:?}"
    );
    zoom_out.emit_clicked();
    settle(&window);
    let back = window.document().cells[0].crop.zoom;
    assert!(
        (back - before.zoom).abs() < 1e-9,
        "one step in and one step out must land where the cell started: \
         {before:?} → {back}"
    );

    // Rotate: the angle is free (S11), so the step is the button's own and the fit
    // is recomputed after it — the cell is still covered, which `fitted_crop` is.
    let before = window.document().cells[0].crop.rotation_deg;
    rotate.emit_clicked();
    settle(&window);
    let turned = window.document().cells[0].crop;
    assert!(
        (turned.rotation_deg - before - pixlay::canvas::ROTATE_STEP_DEG).abs() < 1e-9,
        "the rotate control's step: {before}° → {}°",
        turned.rotation_deg
    );
    assert!(
        window
            .fitted_crop(0)
            .is_some_and(|fit| fit.rotation_deg == turned.rotation_deg
                || (fit.rotation_deg - turned.rotation_deg).abs() < 1e-9),
        "a free angle is never reduced, and the fit keeps it"
    );

    // Clear: the same edit the canvas's own `Delete` makes (`win.clear-cell`), one
    // undo step, and the empty cell's own `+` is what appears in its place. It empties
    // the cell — no photo *and* no framing — which is what the CLI's `edit --clear`
    // has written since S7, so the word means one thing on every surface.
    clear.emit_clicked();
    settle(&window);
    assert_eq!(
        window.document().cells[0],
        pixlay_core::Cell::default(),
        "the clear control left something in the cell"
    );
    let (width, height) = support::canvas_size(&window);
    controls.sync_in(&window, width, height);
    assert!(!controls.strip().is_visible(), "the cell has no photo now");
    assert!(controls.button(0).expect("a first `+`").is_visible());
    assert!(window.can_undo(), "clearing a cell is one undo step");
    window.undo();
    settle(&window);
    assert!(window.document().cells[0].source.is_some());

    // ---- the Frame… dialog ------------------------------------------------
    // Ruling 30: three rows in the document's own order — gap, radius, colour — over
    // `frame{gapRel, radiusRel, color}`.
    let frame_dialog = window
        .frame_dialog()
        .expect("the window has a Frame dialog");
    // **The probe's own point**: the four-way junction of the verification template's
    // top-left cells (cell 0 ends at 3/8 of the sheet, and so does cell 3 beside it and
    // cell 1 under it). With no frame it is photo; with one it is the backdrop, and the
    // sample is taken through the same `Placement` the canvas drew the sheet with, so
    // "the canvas redrew" is a coordinate and not an impression.
    let (grid, _) = window.images();
    let (width, height) = support::canvas_size(&window);
    let placement = canvas::placement(grid, width, height);
    let junction = placement.to_widget(Point::new(3.0 / 8.0, 3.0 / 8.0));
    let (probe_x, probe_y) = (junction.0.round() as i32, junction.1.round() as i32);
    // **Nothing is selected while the two snapshots are taken**, because the selection
    // outline is drawn along the very seams the frame fills — it is interface over
    // content, and this probe is about the content (measured 2026-09-23: with cell 0
    // selected, the junction read `[255, 144, 144]`, the outline's own antialiased edge
    // over the red backdrop).
    window.select(None);
    // A snapshot reads the widgets' cached render nodes, so the probe waits for the
    // frame that carries the change rather than for a fixed span of time.
    support::after_frames(&window, 2, support::WAIT);
    let plain = support::snapshot(&window.canvas_widget());
    let plain_pixel = support::pixel(&plain, probe_x, probe_y);
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.frame", None).is_ok(),
        "the win.frame action is installed"
    );
    assert!(
        frame_dialog.widget().is_visible(),
        "the header bar's Frame button presents the dialog"
    );
    frame_dialog.seed(&window);
    assert_eq!(frame_dialog.gap_row().value(), 0.0);
    assert_eq!(frame_dialog.radius_row().value(), 0.0);
    assert_eq!(frame_dialog.color_button().rgba(), gtk4::gdk::RGBA::WHITE);

    // The rows are a share of the collage's height, so 4.0 is `gapRel = 0.04`. The
    // write is **live**: the document the canvas draws carries the row's value while
    // the dialog is open, and the history only sees it once the value is quiet — the
    // same pending-command path a slider gesture takes.
    frame_dialog.gap_row().set_value(4.0);
    assert_eq!(
        window.display_document().frame.gap_rel,
        0.04,
        "the canvas is drawing the frame the row asks for, before the commit"
    );
    assert_eq!(
        window.document().frame.gap_rel,
        0.0,
        "and the history does not have it yet"
    );
    window.commit();
    assert_eq!(
        window.document().frame.gap_rel,
        0.04,
        "the gap row round-trips into the document"
    );
    frame_dialog.radius_row().set_value(2.5);
    window.commit();
    assert_eq!(window.document().frame.radius_rel, 0.025);
    frame_dialog
        .color_button()
        .set_rgba(&gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0));
    window.commit();
    assert_eq!(
        window.document().frame.color,
        Rgba8::rgb(255, 0, 0),
        "the colour row round-trips into the document"
    );
    // The canvas redraws with it: the junction the frame leaves between the four cells
    // is now the backdrop's own colour where it was a photo's. Taken with the dialog
    // dismissed, which is not a convenience: **a widget behind a presented `AdwDialog`
    // snapshots to nothing** (measured 2026-09-23 — the canvas's `WidgetPaintable`
    // produced no node at all for the 180 s the frame dialog was open), so a pixel probe
    // of the sheet is taken with the dialog out of the way. The *live* half is the claim
    // above: the document the canvas draws already carries the row's value.
    frame_dialog.close_button().emit_clicked();
    support::close_dialog(&frame_dialog.widget(), &window);
    settle(&window);
    // The probe waits for *its* observation, not for a span of time: the canvas's own
    // redraw is what carries the frame's colour, and on a background window the frame
    // that redraws it can be seconds away (measured 2026-09-24: a probe that read the
    // junction's pre-frame pixels because two window frames had passed).
    let framed_pixel = support::settle_by(&window, support::PROBE_WAIT, || {
        let image = support::snapshot(&window.canvas_widget());
        let pixel = support::pixel(&image, probe_x, probe_y);
        (pixel, pixel == [255, 0, 0])
    });
    assert_ne!(
        plain_pixel,
        [255, 0, 0],
        "without a frame the junction is the photos', not the backdrop"
    );
    assert_eq!(
        framed_pixel,
        [255, 0, 0],
        "the canvas did not redraw the junction with the frame's own colour"
    );
    eprintln!(
        "the frame at the junction ({probe_x},{probe_y}) of the {width}x{height} canvas: \
         {plain_pixel:?} before, {framed_pixel:?} with a 4% gap and a red backdrop"
    );
    // One gesture per settled change, and undo walks them back.
    window.undo();
    assert_eq!(window.document().frame.color, Rgba8::WHITE);
    window.undo();
    assert_eq!(window.document().frame.radius_rel, 0.0);
    window.undo();
    assert_eq!(window.document().frame, pixlay_core::Frame::default());

    // ---- a frame the document refuses is reported (S15h, PIX-020) ---------
    // The rows offer 0–100 %, and a gap of 100 % leaves every cell of this template with
    // nothing visible: `CollageDoc::validate` refuses it and names the slot. Before S15h
    // the row kept the refused number while the document kept the previous one, and the
    // delayed commit then wrote a frame the row no longer showed.
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.frame", None).is_ok(),
        "the win.frame action is installed"
    );
    frame_dialog.seed(&window);
    assert_eq!(
        frame_dialog.notice(),
        None,
        "a freshly presented dialog has nothing to report"
    );
    let before_refusal = window.document();
    frame_dialog.gap_row().set_value(100.0);
    let notice = frame_dialog.notice();
    assert!(
        notice.is_some(),
        "a refused value is reported in the dialog's own banner (row {}, document gap {}, \
         display gap {}, toast {:?}, dialog visible {})",
        frame_dialog.gap_row().value(),
        window.document().frame.gap_rel,
        window.display_document().frame.gap_rel,
        window.last_toast(),
        frame_dialog.widget().is_visible()
    );
    let notice = notice.expect("checked above");
    assert!(
        notice.contains("slot 0"),
        "the reason names the cell the gap emptied, got {notice:?}"
    );
    assert_eq!(
        frame_dialog.gap_row().value(),
        0.0,
        "the row goes back to what the document holds"
    );
    assert_eq!(
        window.document().frame.gap_rel,
        0.0,
        "and the document never took the value"
    );
    assert_eq!(
        window.display_document().frame.gap_rel,
        0.0,
        "neither did the canvas, which draws the pending command"
    );
    // Nothing was scheduled: past the quiet interval the delayed commit the finding was
    // about would have written the refused frame.
    window.pump(pixlay::window::COMMIT_QUIET + Duration::from_millis(80));
    assert_eq!(
        window.document().frame.gap_rel,
        before_refusal.frame.gap_rel,
        "a refused value is not a commit waiting to happen"
    );
    // The next accepted value clears the report: the row's number is the document's.
    frame_dialog.radius_row().set_value(1.0);
    assert_eq!(
        frame_dialog.notice(),
        None,
        "an accepted value clears the report"
    );
    window.commit();
    assert_eq!(window.document().frame.radius_rel, 0.01);
    frame_dialog.close_button().emit_clicked();
    support::close_dialog(&frame_dialog.widget(), &window);

    // ---- the export (S25) -------------------------------------------------
    // Ruling 36 moved the export's two parameters into the settings and its one dialog
    // into the platform's own save dialog. What a machine can drive is the half after
    // that dialog — the path it answers, through the same call its own callback makes —
    // and the background export that starts there.
    let dir = support::out_dir();
    let _ = support::artifact("compose.png");
    let out = dir.join("compose.png");
    window.remember_settings(&pixlay::settings::Settings {
        format: Format::Png,
        long_edge: 1200,
        last_export_dir: None,
    });
    assert!(
        gtk4::prelude::WidgetExt::activate_action(&window, "win.export", None).is_ok(),
        "the win.export action is installed"
    );
    let call = Instant::now();
    window.export_to_chosen(&out);
    let returned = call.elapsed();
    assert!(
        returned < Duration::from_millis(500),
        "starting the export blocked for {returned:?}"
    );
    assert!(
        window.progress_revealed(),
        "the export's progress bar is raised while the work runs"
    );
    assert!(
        window.wait_for_idle(support::WAIT),
        "the background export finished"
    );
    assert!(!window.progress_revealed(), "and the bar goes away again");
    assert!(out.is_file(), "the export landed at {out:?}");
    assert!(
        window
            .last_toast()
            .is_some_and(|toast| toast.contains("compose.png")),
        "the toast names the file: {:?}",
        window.last_toast()
    );
    let exported = pixlay_imaging::Source::decode(&out).expect("the export decodes");
    let expected = PixelSize::for_long_edge(window.document().template.aspect, 1200)
        .expect("the grid the settings ask for");
    assert_eq!(
        (exported.width(), exported.height()),
        (expected.width as u32, expected.height as u32),
        "the export is the template's shape at the settings' long edge"
    );
    // The export's own answer to "where did it go": the settings remember the folder,
    // which is where the next save dialog opens (ruling 36).
    assert_eq!(
        window.settings().last_export_dir.as_deref(),
        Some(dir.as_path()),
        "the export remembered the folder it landed in"
    );

    // And the whole document is still one the library can render: the frame's rows
    // and the cell edits above went through the same commands the CLI sends.
    window.document().validate().expect("a valid document");
}

/// Waits until the strip has been laid out **in the shape it now has**.
///
/// `support::allocated` answers "it has an allocation", and after a re-orientation the
/// *old* one is still there: the column was measured as the row's own 224x34 in one run
/// (measured 2026-09-23), which is a measurement of the previous frame's layout rather
/// than of this one. A control is 32 px across, so a row is 34 tall and a column 34 wide,
/// and waiting for the shape is waiting for the frame that made it.
fn laid_out(window: &pixlay::EditorWindow, strip: &gtk4::Box, horizontal: bool) -> bool {
    let deadline = Instant::now() + support::WAIT;
    while Instant::now() < deadline {
        let shaped = if horizontal {
            strip.height() > 0 && strip.height() < 40
        } else {
            strip.width() > 0 && strip.width() < 40
        };
        if shaped {
            return true;
        }
        window.pump(Duration::from_millis(20));
    }
    false
}

/// Waits until neither a decode, nor a band build, nor an export is in flight.
fn settle(window: &pixlay::EditorWindow) {
    assert!(
        window.wait_for_idle(support::WAIT),
        "the background pipeline never finished: {}",
        support::frame_state(window),
    );
    assert!(
        window.wait_for_gallery(support::WAIT),
        "the layout band was never built: {}",
        support::frame_state(window),
    );
}
