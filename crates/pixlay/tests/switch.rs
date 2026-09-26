//! S18's window-side ruler: one layout switch, timed from the click to the frame that
//! shows the new render.
//!
//! The number this file exists for is the human's finding 1 of 2026-09-25 ("switching a
//! layout in the editor takes too long before the new preview is on screen"), and its
//! criterion is that the CLI measures the same thing the same way:
//! `pixlay-render switch` drives the identical pipeline — `Command::SetTemplate`, the
//! new resting grid, the preview-grade copies, the cell bitmaps — in a windowless
//! process, and the two numbers are compared in S18's Result. The bridge between them
//! is the **canvas box**: the editor's canvas widget is what the window derives a grid
//! from (`canvas::preferred_grid`, the margin included), so this test prints its own
//! size and the CLI takes the same number (`--canvas`; `pixlay-cli`'s default is S18's
//! measurement of this box at the default window, and a session whose toolkit claims
//! part of that window for a frame of its own prints a smaller box — the rows below say
//! which box they were measured at, not which toolkit measured them).
//!
//! **One row per candidate the band offers**, each clicked in a **fresh session** that
//! has just opened the document — which is what a one-shot `pixlay-render switch` models,
//! so those rows are the same experiment the CLI runs rather than two histories. A second
//! session is a second window ([`support::second_window`]): the caches of a layout switch
//! are the *session's* (`Preview` is one editing session, S12b). A third row measures the
//! **same session's second click**, back to the layout it opened on: the CLI cannot model
//! it (its process has one history). Until S21 that row also showed the band's own cost —
//! its candidate renders went through the canvas's `Preview` and evicted its bitmaps
//! (`MAX_GRIDS` is two) — and since S21 the band draws sketches, names no photo and shares
//! no cache with the canvas, so the row is the canvas's alone.
//!
//! **Nothing here asserts a millisecond.** The budget is S18's gate, and a threshold
//! that a busy machine can trip is a false failure; what is asserted is that the switch
//! *happened* — the document moved, the canvas rests at the new document's own grid, the
//! band follows — so the numbers are a fact about a real change rather than about a
//! no-op. The numbers themselves go to stderr, the way the S12 GUI test's do.
//!
//! Timed in three parts, because the change has three and only the last is what the user
//! waits for: the canvas's reply plus the frame that draws it (the picture), the band's
//! reply plus its frame (the strip, built on the same worker *after* the canvas's job),
//! and the click to the band's frame.

mod support;

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use pixlay::EditorWindow;
use pixlay::canvas;

/// One switch, as it was measured.
struct Switch {
    from: String,
    target: String,
    /// The widget box a grid is derived from — the number `pixlay-render switch
    /// --canvas` takes.
    canvas_box: (i32, i32),
    /// The window that canvas sits in, at the same moment: the canvas is the
    /// window's own width by construction, and the two together say what the
    /// session drew *inside* the window the app asked for.
    window_box: (i32, i32),
    from_grid: pixlay_core::PixelSize,
    to_grid: pixlay_core::PixelSize,
    /// The click to the frame that shows the new render.
    canvas_ms: f64,
    /// That frame to the band's own, drawn from its reply.
    band_ms: f64,
    /// The click to the band's frame: the whole change.
    switch_ms: f64,
    /// Files the worker decoded for this switch — the CLI's `decodes` (the sources
    /// phase) is the same number.
    decodes: u64,
}

#[test]
fn a_layout_switch_is_measured_from_the_click() {
    support::start();
    let app = support::app();
    let first = support::window(&app);
    first
        .open_path(&support::verify_project())
        .expect("the verification project opens");
    // The grid the canvas rests at, and nothing in flight: the state a click happens
    // in. (`canvas_bitmaps` also waits for the background pipeline, so the session's
    // caches hold the document's own layout by the time anything is measured.)
    let _ = support::canvas_bitmaps(&first);

    // Library order, so the candidates of the verification document are
    // `mosaic-8-s14` (the one it is on), `strip-8-8x1` and `grid-8-4x2`: the two the
    // band offers, and the two a click can produce.
    let targets: Vec<String> = first
        .candidate_templates()
        .into_iter()
        .map(|template| template.name)
        .filter(|name| *name != first.document().template.name)
        .collect();
    assert_eq!(
        targets.len(),
        2,
        "the eight-slot library offers two layouts beside the document's own"
    );

    let mut rows = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        // One session per row: the first click happens in the window the application
        // opened, and the second in a window of its own that has just opened the same
        // project — the state `pixlay-render switch` starts its process in.
        let window = if index == 0 {
            first.clone()
        } else {
            let window = support::second_window(&app);
            window
                .open_path(&support::verify_project())
                .expect("the verification project opens in a second window");
            let _ = support::canvas_bitmaps(&window);
            window
        };
        rows.push(("fresh session".to_string(), measure(&window, target)));
    }
    // And the same session's **second** click, back to the layout it opened on: the
    // row the CLI cannot model (its process is one session with one history). Since S21
    // the band shares nothing with the canvas — a candidate is a sketch, not a render —
    // so what this row costs is the canvas's own work on a warm session.
    rows.push((
        "second click, same session".to_string(),
        measure(&first, "mosaic-8-s14"),
    ));

    // The two fresh rows start on the document's own layout (that is what makes them
    // the CLI's experiment), and the third starts on the layout the first one moved to.
    let expected = [
        ("mosaic-8-s14", "strip-8-8x1"),
        ("mosaic-8-s14", "grid-8-4x2"),
        ("strip-8-8x1", "mosaic-8-s14"),
    ];
    for ((label, switch), (from, target)) in rows.iter().zip(&expected) {
        assert_eq!(&switch.from, from, "{label}: the row's starting layout");
        assert_eq!(&switch.target, target, "{label}: the row's target");
    }
    // **One box for the three rows, and that box is the window's own.** The CLI is
    // handed this number (`--canvas`), so rows measured at different boxes would not be
    // one experiment; and the box is the canvas's by construction — the drawing area
    // expands into the window (`canvas.rs::build`), so a canvas narrower than its window
    // is a stale or refused allocation, which is the failure this checks (a full-suite
    // run has reported a control placed against a `0x0` canvas).
    //
    // The box's own *value* is the session's, and that is why no literal stands here.
    // What a display adds inside the window the app asked for is not this app's
    // behaviour: a window manager gives the window a frame of its own, while a display
    // with **no** window manager leaves GTK drawing the frame *inside* the surface
    // (`window.solid-csd`, whose libadwaita rule is `padding: 5px`), so the content box
    // is 10 px smaller in each direction than the window: **1090x750** and a canvas of
    // **1090x584**, against **1100x760** and **1100x594** in a session. Measured
    // 2026-09-26 with the same GTK (4.24.0) both ways: the harness's own mutter gives
    // 1100, and `PIXLAY_TEST_CHILD=1` on a bare Xvfb gives 1090x584 — to the pixel of
    // the number CI printed when it failed. These rows therefore say which box they were
    // measured at, not which session measured them, and that is what lets the Xvfb
    // fallback (`AGENTS.md`, the entry: mutter wherever mutter runs) run this suite at all.
    let boxes: Vec<(i32, i32)> = rows.iter().map(|(_, row)| row.canvas_box).collect();
    assert!(
        boxes.windows(2).all(|pair| pair[0] == pair[1]),
        "the rows were not measured at one canvas box: {boxes:?}"
    );
    for (label, row) in &rows {
        assert_eq!(
            row.canvas_box.0, row.window_box.0,
            "{label}: the canvas is the window's own width"
        );
    }
    for (label, switch) in &rows {
        eprintln!(
            "switch, {label}: {} → {} · canvas {}x{} (of a {}x{} window) · grid {}x{} → {}x{} · \
             canvas {:.1} ms ({} decodes) · band {:.1} ms (0 decodes, a sketch) · total {:.1} ms · \
             budget {:.0} ms · {}",
            switch.from,
            switch.target,
            switch.canvas_box.0,
            switch.canvas_box.1,
            switch.window_box.0,
            switch.window_box.1,
            switch.from_grid.width,
            switch.from_grid.height,
            switch.to_grid.width,
            switch.to_grid.height,
            switch.canvas_ms,
            switch.decodes,
            switch.band_ms,
            switch.switch_ms,
            pixlay_cli::cli::SWITCH_BUDGET_MS,
            if switch.canvas_ms <= pixlay_cli::cli::SWITCH_BUDGET_MS {
                "within_budget"
            } else {
                "over_budget"
            },
        );
    }
    eprintln!(
        "the CLI measures the same click in a fresh process: pixlay-render switch \
         --project <verify.pixlay> --template <target> --canvas {}x{} --stats",
        rows[0].1.canvas_box.0, rows[0].1.canvas_box.1,
    );
}

/// Pumps until `ready`, at the finest granularity a wait can have.
///
/// **Not the harness's [`support::settle_by`]**: that one pumps in 50 ms slices, which
/// is the right granularity for a pixel probe and 35% of the number this test is here
/// to read. The observation is a flag the window sets when a reply lands, so the wait
/// can be as tight as a main-context iteration, and what is left in the number is the
/// pipeline's own work plus one display frame.
fn wait_for(window: &EditorWindow, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + support::WAIT;
    loop {
        if ready() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        window.pump(Duration::from_millis(1));
    }
}

/// The click on `target`'s candidate, timed.
fn measure(window: &EditorWindow, target: &str) -> Switch {
    let from = window.document().template.name.clone();
    let from_grid = window.images().0;
    let area = window.canvas_widget();
    let canvas_box = (area.width(), area.height());
    let window_box = (window.width(), window.height());

    // The click, through the band's own cell: `set_active` is what a click leaves
    // behind on a `GtkToggleButton`, and the handler it runs is the window's own
    // `select_layout`.
    let gallery = window.gallery().expect("the editor has a layout band");
    let cell = gallery
        .cell(target)
        .unwrap_or_else(|| panic!("the band lists {target}"));
    // The worker's counts *before* the click: the band's rebuild goes out with the same
    // click, and on a fast machine it can have landed before the canvas's reply is even
    // read, so the conditions have to be counts taken here.
    let builds_before = window.gallery_builds();
    let decodes_before = window.decoded_sources();
    let started = Instant::now();
    cell.set_active(true);
    // The request is synchronous (`refresh_document` → `request_decode`), and a grid in
    // flight is what says the new bitmaps are not in hand yet.
    assert!(
        window.requested_grid().is_some(),
        "the click did not ask the decoder for the new grid"
    );

    // The canvas's half: its reply, and the frame that draws the bitmaps it carried.
    let landed = wait_for(window, || window.requested_grid().is_none());
    assert!(
        landed,
        "the canvas's reply never landed: {}",
        support::frame_state(window)
    );
    support::after_frames(window, 1, support::WAIT);
    let canvas_ms = started.elapsed().as_secs_f64() * 1000.0;

    // The band's: the same worker, its job sent after the canvas's (S14).
    let rebuilt = wait_for(window, || window.gallery_builds() > builds_before);
    assert!(
        rebuilt,
        "the band's rebuild never landed: {}",
        support::frame_state(window)
    );
    support::after_frames(window, 1, support::WAIT);
    let switch_ms = started.elapsed().as_secs_f64() * 1000.0;

    // What the click produced: the document on the new layout, the canvas's bitmaps
    // for the grid that layout asks for, and the band following. Asserted on every
    // row, so a number that a stuck switch produced cannot pass as a measurement.
    assert_eq!(
        window.document().template.name,
        target,
        "the document moved"
    );
    assert_eq!(
        window.gallery().and_then(|band| band.selected()).as_deref(),
        Some(target),
        "the band highlights the layout the document is on"
    );
    let to_grid = window.images().0;
    let expected = canvas::preferred_grid(
        window.document().template.aspect,
        canvas_box.0,
        canvas_box.1,
    );
    assert_eq!(
        to_grid, expected,
        "the canvas rests at the new document's own grid"
    );
    assert!(
        to_grid.width > 0 && to_grid.height > 0,
        "the new grid is empty"
    );
    Switch {
        from,
        target: target.to_string(),
        canvas_box,
        window_box,
        from_grid,
        to_grid,
        canvas_ms,
        band_ms: switch_ms - canvas_ms,
        switch_ms,
        decodes: window.decoded_sources() - decodes_before,
    }
}
