// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S23's exit criteria: two cells exchange whole — photo *and* framing — by drag, by
//! `Shift`+click and from the keyboard, each one undo step, and a plain drag still pans
//! the photo.
//!
//! **The pair is the criterion's own**: cells 0 and 5 of the verification project, and
//! they are different shapes — cell 0 is the sheet's largest (3/8 x 3/8) and cell 5 is a
//! quarter of its width — so a swap that carried the photo and left the framing behind
//! shows up in the document *and* in the pixels: the re-fit a moved photo gets is what a
//! stale crop would not produce.
//!
//! **What is driven how.** The canvas's gestures are driven through their own
//! controllers (`support::press_canvas` / `support::drag_canvas`, S27), which runs the
//! handlers the product installed and the modifier-free branch of each; GTK's own
//! delivery of a pointer event was checked against a real seat once (the helper's own
//! note, which names the Broadway run of 2026-09-26), and the `Shift` forms — which an
//! emitted signal cannot reach, since their branch reads the event's modifier — go
//! through the window calls those branches make: `swap_click` for what a `Shift`+click's
//! release ends in, `swap_drag_begin`/`update`/`end` for the `Shift`+drag. The keyboard
//! path drives the real `EventControllerKey`, and the keyboard half of the marked swap
//! drives the strip's real `GtkToggleButton`. The document, the pixels and the history
//! are what a machine can hold this step to, and all three are here.

mod support;

use std::path::Path;
use std::time::Duration;

use gtk4::gdk;
use gtk4::prelude::*;

use pixlay::canvas;
use pixlay::i18n::{fill, gettext};
use pixlay::window::EditorWindow;
use pixlay::workers::{WorkerPlan, Workers};
use pixlay_core::{CollageDoc, PixelSize, Point, Project};
use pixlay_render::{Images, Target, draw};

/// The criterion's two cells: the sheet's largest, and a quarter of its width.
const LEFT: usize = 0;
const RIGHT: usize = 5;

/// How much of a cell's edge is left out of a pixel comparison, in device pixels: the
/// selection outline and either swap mark are 2 px lines on the cell's own path, and this
/// probe is about what the cell *shows*.
const EDGE: i32 = 4;

/// `AGENTS.md`'s threshold for "the same picture" (measured 2.62/255 for one composition
/// at `2N` and `N`). The comparison below is the same function at the same grid, so it is
/// expected to be exact; the threshold is only what keeps a snapshot's rounding from
/// failing it.
const RMSE_THRESHOLD: f64 = 6.0;

#[test]
fn two_cells_swap_whole_by_drag_click_and_keyboard() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens");
    let controls = window
        .cell_controls()
        .expect("the canvas has a cell-control layer");
    // The strip's swap control, which is the mark's own half of the interaction (S23)
    // and since S27 the pointer ending's too.
    let swap = controls.swap_button();
    let _ = support::canvas_bitmaps(&window);
    let (width, height) = support::canvas_size(&window);

    // The two cells' centres, through the same `Placement` the canvas drew the sheet with:
    // the coordinates the pointer's own hit test already uses, so nothing here is a second
    // opinion about where the cells are.
    let (grid, _) = window.images();
    let placement = canvas::placement(grid, width, height);
    let centre = |slot: usize| {
        let bbox = window.document().template.slots[slot].outline.bbox();
        placement.to_widget(Point::new(
            (bbox.x0 + bbox.x1) / 2.0,
            (bbox.y0 + bbox.y1) / 2.0,
        ))
    };
    let (left_x, left_y) = centre(LEFT);
    let (right_x, right_y) = centre(RIGHT);
    assert_eq!(window.slot_at_widget(left_x, left_y), Some(LEFT));
    assert_eq!(window.slot_at_widget(right_x, right_y), Some(RIGHT));

    // The resting canvas, with nothing selected: the reference every path is measured
    // against.
    window.select(None);
    let before = settled_snapshot(&window);
    assert_eq!(
        (before.0, before.1),
        (width, height),
        "the snapshot is the canvas's own size"
    );
    let before_doc = window.document();

    // ---- a plain pointer drag pans (the gesture that must not regress) ------
    // The drag goes through the canvas's own `GtkGestureDrag`
    // ([`support::drag_canvas`]), and it is **diagonal on purpose**: that is the shape
    // that used to move nothing (S27, finding 2 of the walk of 2026-09-26). Cell 0 is
    // where it shows — a 2:3 photo in a 4:3 cell covers at 1.0x, so all of the travel
    // is vertical and none of it is horizontal, and the clamp that scaled both
    // components by one factor pulled the vertical pan back to nothing as soon as the
    // request carried a horizontal component.
    window.select(Some(LEFT));
    let start = window.document().cells[LEFT].crop;
    let framed = window.fitted_crop(LEFT).expect("cell 0 shows a photo");
    let (step_x, step_y) = window.slot_extent(LEFT).expect("cell 0 has an extent");
    let (sheet_w, sheet_h) = window.sheet_size();
    // The document's own unit: offsets are in slot widths and heights, and the drag is
    // in device pixels, so the conversion is through the slot's size on screen — the
    // same arithmetic the gesture uses.
    let (dx, dy) = (20.0, 30.0);
    support::drag_canvas(&window, (left_x, left_y), (left_x + dx, left_y + dy));
    let panned = window.document().cells[LEFT].crop;
    assert_eq!(window.swap_source(), None, "a plain drag marks no swap");
    assert!(
        (panned.offset.0 - framed.offset.0).abs() <= 1e-9,
        "this cell has no horizontal travel, so the fit must not move that axis: \
         {} instead of {}",
        panned.offset.0,
        framed.offset.0
    );
    assert!(
        (panned.offset.1 - (framed.offset.1 + dy / (step_y * sheet_h))).abs() <= 1e-9,
        "the vertical pan the drag asked for is {} and the document holds {}",
        framed.offset.1 + dy / (step_y * sheet_h),
        panned.offset.1
    );
    assert_ne!(
        panned, start,
        "the pointer drag moved the photo ({sheet_w}x{sheet_h} sheet, {step_x}x{step_y} \
         cell)"
    );
    window.undo();
    assert_eq!(
        window.document().cells[LEFT].crop,
        start,
        "the release is one undo step, like every gesture"
    );
    // A drag the clamp answers with the framing it started from is **not** a step: this
    // cell has no horizontal travel at all, so a sideways drag asks for exactly what is
    // already there (S15d's rule, which is why the user does not have to `Ctrl+Z` through
    // a drag that moved nothing).
    let depth = window.undo_depth();
    support::drag_canvas(&window, (left_x, left_y), (left_x + 40.0, left_y));
    assert_eq!(
        window.document().cells[LEFT].crop,
        start,
        "a drag with no travel in it changes nothing"
    );
    assert_eq!(window.undo_depth(), depth, "and leaves no step behind");

    // ---- by drag: from cell 0 onto cell 5 ----------------------------------
    let depth = window.undo_depth();
    assert!(
        window.swap_drag_begin(left_x, left_y),
        "the drag starts on the cell under the press"
    );
    assert_eq!(
        window.swap_source(),
        Some(LEFT),
        "and marks it as the source"
    );
    window.swap_drag_update(right_x, right_y);
    assert_eq!(
        window.swap_target(),
        Some(RIGHT),
        "the cell under the pointer is the drop's target"
    );
    // The highlight is the promise that this is where the release lands, so it is on
    // screen *before* the release: the target cell is filled in, and nothing else moved.
    let hovering = settled_snapshot(&window);
    for slot in 0..before_doc.cells.len() {
        let was = cell_pixels(&before, &placement, &before_doc, slot).2;
        let now = cell_pixels(&hovering, &placement, &before_doc, slot).2;
        if slot == RIGHT {
            assert_ne!(
                was, now,
                "the drag did not highlight the cell it would land on"
            );
        } else {
            assert_eq!(
                was, now,
                "cell {slot} changed while the pointer was over {RIGHT}"
            );
        }
    }
    assert!(
        window.swap_drag_end(right_x, right_y),
        "the release exchanges the two cells"
    );
    check_swap(
        "by drag",
        &window,
        &placement,
        grid,
        &before_doc,
        &before,
        depth,
    );

    // ---- by `Shift`+click: the selection is the source ---------------------
    window.select(Some(LEFT));
    let depth = window.undo_depth();
    assert!(
        window.swap_click(RIGHT),
        "the click exchanges the selection with the cell it lands on"
    );
    assert_eq!(
        window.selection(),
        Some(RIGHT),
        "and selects the cell it landed on, which is where the photo went"
    );
    check_swap(
        "by Shift+click",
        &window,
        &placement,
        grid,
        &before_doc,
        &before,
        depth,
    );

    // ---- the marked swap answers the pointer too (S27) ---------------------
    // S23's mark was a half-way house: the strip's control marked a cell, and only the
    // arrows plus `Return` or a `Shift`+drag could finish it — which the walk of
    // 2026-09-26 read as "the strip's swap control does nothing under the pointer"
    // (finding 4). Since S27 a plain press on *another* cell means the exchange,
    // through the canvas's own click controller, one undo step and no modifier — and it
    // is done on that press's **release**, exactly as the `Shift`+click below it is, so
    // a press that becomes a drag is still the framing's own drag.
    window.select(Some(LEFT));
    swap.emit_clicked();
    assert_eq!(
        window.swap_source(),
        Some(LEFT),
        "the control marked the cell"
    );
    let depth = window.undo_depth();
    support::press_canvas(&window, right_x, right_y);
    assert_eq!(
        window.swap_source(),
        Some(LEFT),
        "the press alone does not spend the mark: the exchange is the release's"
    );
    support::release_canvas(&window, right_x, right_y);
    assert_eq!(
        window.selection(),
        Some(RIGHT),
        "the release picks the cell it lands on, which is where the photo went"
    );
    check_swap(
        "by a press on another cell",
        &window,
        &placement,
        grid,
        &before_doc,
        &before,
        depth,
    );
    // A press on the marked cell itself is not a swap — two cells are what a swap is —
    // so it takes the mark back and only selects.
    window.select(Some(LEFT));
    swap.emit_clicked();
    assert_eq!(window.swap_source(), Some(LEFT));
    let depth = window.undo_depth();
    support::press_canvas(&window, left_x, left_y);
    support::release_canvas(&window, left_x, left_y);
    assert_eq!(
        window.swap_source(),
        None,
        "a press on the marked cell itself takes the mark back"
    );
    assert_eq!(window.document(), before_doc, "and makes no edit");
    assert_eq!(window.undo_depth(), depth, "so there is no step to undo");
    // A press on the *other* cell whose own drag begins is the drag instead: the release
    // that follows it must not also exchange the two cells (the same deferral the
    // `Shift`+click has, for the same reason).
    window.select(Some(LEFT));
    swap.emit_clicked();
    assert_eq!(window.swap_source(), Some(LEFT));
    let depth = window.undo_depth();
    support::press_canvas(&window, right_x, right_y);
    support::drag_canvas(&window, (right_x, right_y), (right_x, right_y - 24.0));
    support::release_canvas(&window, right_x, right_y - 24.0);
    assert_eq!(
        window.swap_source(),
        Some(LEFT),
        "the drag left the mark alone"
    );
    assert_eq!(
        window.document().cells[LEFT],
        before_doc.cells[LEFT],
        "the release after a drag did not exchange the two cells"
    );
    let steps = window.undo_depth() - depth;
    assert!(steps <= 1, "a drag is one step at most, got {steps}");
    for _ in 0..steps {
        window.undo();
    }
    window.cancel_swap();

    // ---- from the keyboard: mark with the strip's control, choose, `Return` --
    window.select(Some(LEFT));
    assert!(swap.is_visible(), "the strip is over the selected cell");
    swap.emit_clicked();
    assert!(swap.is_active(), "the control's checked state is the mark");
    assert_eq!(window.swap_source(), Some(LEFT));
    window.select(Some(RIGHT));
    assert_eq!(
        window.canvas_label(),
        fill(
            gettext("Collage canvas, cell {} of {}, swapping with cell {}"),
            &[RIGHT + 1, window.document().template.slots.len(), LEFT + 1],
        ),
        "the canvas announces the swap it is in"
    );
    let depth = window.undo_depth();
    assert!(
        support::press(&window, gdk::Key::Return, gdk::ModifierType::empty()),
        "`Return` on the canvas answers a marked swap"
    );
    check_swap(
        "from the keyboard",
        &window,
        &placement,
        grid,
        &before_doc,
        &before,
        depth,
    );

    // ---- `Esc` takes a mark back, and a release outside every cell does nothing
    window.select(Some(LEFT));
    swap.emit_clicked();
    assert_eq!(window.swap_source(), Some(LEFT));
    assert!(
        support::press(&window, gdk::Key::Escape, gdk::ModifierType::empty()),
        "`Esc` is the canvas's while a swap is marked"
    );
    assert_eq!(window.swap_source(), None, "`Esc` took the mark off");
    assert_eq!(window.document(), before_doc, "and touched nothing");

    let depth = window.undo_depth();
    assert!(window.swap_drag_begin(left_x, left_y));
    // Two device pixels outside the sheet's own corner: the canvas keeps `CANVAS_MARGIN`
    // around the sheet, so this is a point a pointer can be at with no cell under it.
    let outside = (placement.origin.0 / 2.0, placement.origin.1 / 2.0);
    assert_eq!(
        window.slot_at_widget(outside.0, outside.1),
        None,
        "the point is off the sheet"
    );
    window.swap_drag_update(outside.0, outside.1);
    assert_eq!(
        window.swap_target(),
        None,
        "off the sheet there is no target"
    );
    assert!(
        !window.swap_drag_end(outside.0, outside.1),
        "a release outside every cell swaps nothing"
    );
    assert_eq!(window.document(), before_doc, "the document is untouched");
    assert_eq!(window.swap_source(), None, "the mark goes with the release");
    assert_eq!(window.undo_depth(), depth, "and no undo step was made");

    // ---- the strip's new control is the strip's own kind of control --------
    // `tests/hig.rs::check_compose` holds every control of the strip to this; here it is
    // the criterion's own half — named, reachable with the keyboard, and past HIG
    // `guidelines/pointer-touch`'s 24 px floor.
    window.select(Some(LEFT));
    let widget = swap.clone().upcast::<gtk4::Widget>();
    assert_eq!(
        swap.tooltip_text().as_deref(),
        Some(gettext("Swap with another cell").as_str()),
        "the control says what it does"
    );
    assert!(swap.is_focusable(), "the control is Tab-reachable");
    assert!(
        swap.width() >= 24 && swap.height() >= 24,
        "the control is {}x{}, below the 24 px target",
        swap.width(),
        swap.height()
    );
    assert!(
        support::has_accessible_name(&widget),
        "the control has no accessible name"
    );
    // **And it is where it looks** (S27): the pick at the control's own centre returns
    // the control (or something inside it), so the half of "the press reaches it" a test
    // without a seat *can* ask is asked here. The other half — GTK delivering a real
    // button event to it — was measured once with a real pointer; the note on
    // `support::press_canvas` names that run and what it saw.
    let centre = swap
        .compute_point(
            &window.clone().upcast::<gtk4::Widget>(),
            &gtk4::graphene::Point::new(swap.width() as f32 / 2.0, swap.height() as f32 / 2.0),
        )
        .expect("the control is inside the window");
    let picked = window.pick(
        f64::from(centre.x()),
        f64::from(centre.y()),
        gtk4::PickFlags::DEFAULT,
    );
    assert!(
        picked
            .as_ref()
            .is_some_and(|picked| picked == &widget || picked.is_ancestor(&widget)),
        "the pick at the control's own centre is {picked:?}, not the control or its icon"
    );

    // ---- the CLI's `edit --swap 0,5` is the same document ------------------
    // The window's own swap, from the project as it opens, against the same edit through
    // the machine surface: two files that load to one document (the sources' spellings may
    // differ — the window rebases when it saves into another directory — which is what
    // `same_document` resolves).
    let dir = support::out_dir();
    let from_gui = dir.join("swap-gui.pixlay");
    let from_cli = dir.join("swap-cli.pixlay");
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens again");
    // The bitmaps are what `slot_at_widget` needs to place the sheet: a freshly opened
    // document has the placeholder grid until the decoder answers, and a coordinate from
    // the real grid would land in the stretched placeholder's cells instead.
    let _ = support::canvas_bitmaps(&window);
    assert!(window.swap_drag_begin(left_x, left_y));
    window.swap_drag_update(right_x, right_y);
    assert!(window.swap_drag_end(right_x, right_y));
    let written = window
        .save_to(&from_gui)
        .expect("the window saves its document");
    let status = pixlay_cli::cli::run(&argv(&[
        "edit",
        "--project",
        path(&support::verify_project()),
        "--swap",
        "0,5",
        "--out",
        path(&from_cli),
    ]))
    .expect("the CLI edits the project");
    assert_eq!(status, 0, "the CLI's swap is one edit");
    let gui = Project::load(&written).expect("the window's project loads");
    let cli = Project::load(&from_cli).expect("the CLI's project loads");
    assert!(
        support::same_document(&gui, &cli),
        "the window's own swap and `edit --swap 0,5` produced different documents:\n\
         {:?}\n{:?}",
        gui.doc(),
        cli.doc()
    );
    eprintln!(
        "the window's drag and `edit --swap 0,5` are one document ({} cells, {}, photo 0 \
         swapped with 5)",
        cli.doc().cells.len(),
        cli.doc().template.name,
    );

    // ---- the drag does not depend on a decoded bitmap (S27) ----------------
    // `fitted_crop` needs the photo's own aspect and nothing before the decoder answers
    // knows it, so the drag used to be a complete no-op — no command, no draw, no report,
    // no undo step — on a cell whose bitmap had not arrived, while the keyboard's own pan
    // went through `fit_for` and still moved the photo. Both start from `gesture_base`
    // now, and `fit_for` passes the request through unfitted while there is no aspect (a
    // crop is a request; `draw` fits it), so the gesture works with the decoder down.
    let blind = support::window_with_workers(
        &app,
        Workers {
            decode: WorkerPlan::Fail,
            ..Workers::default()
        },
    );
    blind
        .open_path(&support::verify_project())
        .expect("a project still opens when the decoder is down");
    blind.pump(Duration::from_millis(300));
    let (blind_grid, blind_images) = blind.images();
    assert!(
        blind_images.is_empty(),
        "no bitmap arrives with the decoder down, so there is no aspect to fit with"
    );
    let (blind_width, blind_height) = support::canvas_size(&blind);
    let blind_placement = canvas::placement(blind_grid, blind_width, blind_height);
    let blind_centre = |slot: usize| {
        let bbox = blind.document().template.slots[slot].outline.bbox();
        blind_placement.to_widget(Point::new(
            (bbox.x0 + bbox.x1) / 2.0,
            (bbox.y0 + bbox.y1) / 2.0,
        ))
    };
    let (blind_x, blind_y) = blind_centre(LEFT);
    assert_eq!(
        blind.slot_at_widget(blind_x, blind_y),
        Some(LEFT),
        "the placeholder grid still names the cell under a point"
    );
    blind.select(Some(LEFT));
    let depth = blind.undo_depth();
    let before = blind.document().cells[LEFT].crop;
    let (blind_step_x, blind_step_y) = blind.slot_extent(LEFT).expect("cell 0 has an extent");
    let (blind_sheet_w, blind_sheet_h) = blind.sheet_size();
    let dy = 30.0;
    support::drag_canvas(&blind, (blind_x, blind_y), (blind_x, blind_y + dy));
    let after = blind.document().cells[LEFT].crop;
    assert_eq!(
        blind.undo_depth(),
        depth + 1,
        "the drag is one step even with nothing decoded"
    );
    assert!(
        (after.offset.1 - (before.offset.1 + dy / (blind_step_y * blind_sheet_h))).abs() <= 1e-9,
        "the drag kept its own number with the decoder down: {} instead of {} \
         ({}x{} sheet, {}x{} cell)",
        after.offset.1,
        before.offset.1 + dy / (blind_step_y * blind_sheet_h),
        blind_sheet_w,
        blind_sheet_h,
        blind_step_x,
        blind_step_y
    );
    // And a framing control moves without one too: the strip's own zoom reads the shared
    // base rather than the fit.
    blind.zoom_by(1.06);
    assert!(
        blind.document().cells[LEFT].crop.zoom > after.zoom,
        "the zoom control works with the decoder down"
    );
    // A step the document cannot take is **reported, not dropped** (S27), and once per
    // gesture rather than once per motion event: with no aspect to fit it back into
    // range, every update of a drag this far (4000 device px against this sheet) asks for
    // an offset past the ±1 the document accepts, and the drag is four updates.
    let toasts = blind.toasts();
    let untouched = blind.document();
    support::drag_canvas(&blind, (blind_x, blind_y), (blind_x, blind_y + 4000.0));
    assert_eq!(
        blind.document(),
        untouched,
        "a refused gesture changes nothing"
    );
    assert_eq!(
        blind.toasts(),
        toasts + 1,
        "the refusal is reported once, not once per motion event"
    );
    assert!(
        blind
            .last_toast()
            .is_some_and(|toast| toast.contains("offset")),
        "the report names the reason, got {:?}",
        blind.last_toast()
    );
}

/// Everything one whole swap must be, whichever path drove it.
///
/// The document: the two cells exchanged, photo *and* framing. The pixels: the two cells
/// changed and every other cell is byte-identical to what it was, and the changed cells
/// are the swapped document's own — the one renderer at the canvas's own grid and
/// placement. The history: exactly one step, and one undo restores the document *and* the
/// pixels, which is what makes this an exchange rather than an edit that overwrote
/// something.
fn check_swap(
    label: &str,
    window: &EditorWindow,
    placement: &canvas::Placement,
    grid: PixelSize,
    before_doc: &CollageDoc,
    before: &support::Image,
    depth: usize,
) {
    let after_doc = window.document();
    assert_eq!(
        after_doc.cells[LEFT], before_doc.cells[RIGHT],
        "{label}: cell {LEFT} is not what cell {RIGHT} was"
    );
    assert_eq!(
        after_doc.cells[RIGHT], before_doc.cells[LEFT],
        "{label}: cell {RIGHT} is not what cell {LEFT} was"
    );
    assert_eq!(
        after_doc.template, before_doc.template,
        "{label}: no relayout"
    );
    assert_eq!(window.undo_depth(), depth + 1, "{label}: one undo step");
    assert_eq!(window.swap_source(), None, "{label}: the mark is spent");

    let after = settled_snapshot(window);
    for slot in 0..after_doc.cells.len() {
        let was = cell_pixels(before, placement, before_doc, slot).2;
        let now = cell_pixels(&after, placement, &after_doc, slot).2;
        if slot == LEFT || slot == RIGHT {
            assert_ne!(was, now, "{label}: cell {slot} did not change");
        } else {
            assert_eq!(
                was, now,
                "{label}: cell {slot} changed although the swap did not touch it"
            );
        }
    }

    let (_, images) = window.images();
    let expected = reference(&after_doc, &images, placement, grid, (after.0, after.1));
    for slot in [LEFT, RIGHT] {
        let difference = support::rmse(
            &cell_pixels(&after, placement, &after_doc, slot),
            &cell_pixels(&expected, placement, &after_doc, slot),
        );
        assert!(
            difference <= RMSE_THRESHOLD,
            "{label}: cell {slot} differs from the document's own render by {difference}"
        );
        eprintln!("{label}: cell {slot} of the swapped document renders at RMSE {difference:.3}");
    }

    window.undo();
    assert_eq!(
        window.document(),
        *before_doc,
        "{label}: one undo is not the whole swap"
    );
    let back = settled_snapshot(window);
    for slot in 0..before_doc.cells.len() {
        assert_eq!(
            cell_pixels(before, placement, before_doc, slot).2,
            cell_pixels(&back, placement, before_doc, slot).2,
            "{label}: cell {slot} did not come back with the undo"
        );
    }
}

/// Waits for the canvas to be the document again, and snapshots it.
///
/// A committed edit decodes new bitmaps on the worker and a snapshot reads what the last
/// frame drew, so both waits are the condition rather than a span of time.
fn settled_snapshot(window: &EditorWindow) -> support::Image {
    assert!(
        window.wait_for_idle(support::WAIT),
        "the decode finished: {}",
        support::frame_state(window)
    );
    support::after_frames(window, 2, support::WAIT);
    support::snapshot(&window.canvas_widget())
}

/// One cell's pixels out of a canvas-sized image, through the `Placement` the canvas drew
/// the sheet with, inset by [`EDGE`] so that the cell's own edges are not compared.
fn cell_pixels(
    image: &support::Image,
    placement: &canvas::Placement,
    doc: &CollageDoc,
    slot: usize,
) -> support::Image {
    let bbox = doc.template.slots[slot].outline.bbox();
    let (x0, y0) = placement.to_widget(Point::new(bbox.x0, bbox.y0));
    let (x1, y1) = placement.to_widget(Point::new(bbox.x1, bbox.y1));
    let (width, height, data) = image;
    let left = (x0.round() as i32 + EDGE).clamp(0, *width);
    let top = (y0.round() as i32 + EDGE).clamp(0, *height);
    let right = (x1.round() as i32 - EDGE).clamp(left, *width);
    let bottom = (y1.round() as i32 - EDGE).clamp(top, *height);
    let mut pixels = Vec::new();
    for y in top..bottom {
        for x in left..right {
            let index = (y as usize * *width as usize + x as usize) * 3;
            pixels.extend_from_slice(&data[index..index + 3]);
        }
    }
    (right - left, bottom - top, pixels)
}

/// The document's own pixels, through the one renderer, at the canvas's grid and
/// placement: what the window must be showing after a swap.
fn reference(
    doc: &CollageDoc,
    images: &Images,
    placement: &canvas::Placement,
    grid: PixelSize,
    size: (i32, i32),
) -> support::Image {
    let (width, height) = size;
    let mut surface = gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::Rgb24, width, height)
        .expect("a surface the size of the canvas");
    {
        let ctx = gtk4::cairo::Context::new(&surface).expect("a cairo context");
        ctx.translate(placement.origin.0, placement.origin.1);
        draw(
            doc,
            images,
            &Target {
                ctx: &ctx,
                scale: placement.scale,
                canvas_px: grid,
                band: None,
            },
        )
        .expect("the reference render");
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().expect("the cairo surface is mapped");
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = y * stride + x * 4;
            // `Format::Rgb24` in memory is BGRX on a little-endian machine.
            rgb.extend_from_slice(&[data[index + 2], data[index + 1], data[index]]);
        }
    }
    (width, height, rgb)
}

fn argv(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(std::ffi::OsString::from).collect()
}

fn path(value: &Path) -> &str {
    value.to_str().expect("a UTF-8 path")
}
