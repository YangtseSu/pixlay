//! S15h: a background worker that cannot start, or that is gone, is a report and a
//! cleared pending state — not a panic and not a wait that never ends (PIX-014).
//!
//! The window's two workers are the decoding thread and an export's own thread. Each
//! one used to end in a state nobody could leave: a `spawn` that `expect`ed and took
//! the window down with it, or a `send` whose failure was discarded while the request
//! stayed marked pending, so the canvas or the progress bar waited for a reply that
//! could not come. (The picker's tile thread was the third until S22 deleted the
//! stage and its worker with it.)
//!
//! `EditorWindow::with_workers` is the way in: the same window, with a plan per worker
//! (`WorkerPlan::Fail` — the thread cannot be started; `WorkerPlan::Vanish` — it starts
//! and is already gone, so the first send fails). Every check below reads the state a
//! wait would hang on and the sentence the user is shown, and opens the same window the
//! product opens when the plan is `Run`.

mod support;

use std::time::Duration;

use pixlay::workers::{WorkerPlan, Workers};

/// The state a wait on the canvas would hang on: a grid marked in flight.
///
/// Read through the public handle the tests already use — a request that was queued
/// leaves it `Some`, one that could not be is `None`.
fn canvas_pending(window: &pixlay::EditorWindow) -> bool {
    window.requested_grid().is_some()
}

#[test]
fn a_worker_that_is_down_reports_and_clears_its_pending_state() {
    support::start();
    let app = support::app();

    // ---- the decoding thread cannot be started ---------------------------
    let window = support::window_with_workers(
        &app,
        Workers {
            decode: WorkerPlan::Fail,
            ..Workers::default()
        },
    );
    window
        .open_path(&support::verify_project())
        .expect("a project still opens when the decoder is down");
    window.pump(Duration::from_millis(300));
    assert!(
        !canvas_pending(&window),
        "a decode that was never queued must not be marked in flight ({:?})",
        window.requested_grid()
    );
    assert!(
        window
            .last_toast()
            .is_some_and(|toast| toast.contains("photo decoder could not be started")),
        "the window says the decoder could not be started, got {:?}",
        window.last_toast()
    );
    // And it says it once: the canvas asks again on every resize and every edit.
    let toasts = window.toasts();
    window.pump(Duration::from_millis(50));
    window.refresh();
    window.pump(Duration::from_millis(50));
    assert_eq!(
        window.toasts(),
        toasts,
        "the same failure is news once, not once per request"
    );
    // The document itself is untouched and still walkable: the user can keep
    // working, they just cannot see the collage's pixels.
    assert_eq!(window.document().cells.len(), 8, "the project is open");
    assert!(window.selection().is_none());
    // The band's own build goes to the same worker, so it is refused the same way:
    // no build lands, and nothing is left marked in flight for a wait to hang on.
    assert_eq!(
        window.gallery_builds(),
        0,
        "the band's build counts what landed, and nothing can land while the worker is down"
    );
    assert!(
        window.wait_for_idle(support::WAIT),
        "the idle wait returns: nothing is pending for a worker that never ran (grid {:?})",
        window.requested_grid()
    );

    // ---- the decoding thread is gone after starting ----------------------
    let vanished = support::window_with_workers(
        &app,
        Workers {
            decode: WorkerPlan::Vanish,
            ..Workers::default()
        },
    );
    vanished
        .open_path(&support::verify_project())
        .expect("the project opens");
    vanished.pump(Duration::from_millis(300));
    assert!(
        !canvas_pending(&vanished),
        "a send that failed must not leave a grid in flight"
    );
    assert!(
        vanished
            .last_toast()
            .is_some_and(|toast| toast.contains("photo decoder stopped")),
        "the window says the decoder stopped, got {:?}",
        vanished.last_toast()
    );

    // ---- the export thread cannot be started -----------------------------
    let export_window = support::window_with_workers(
        &app,
        Workers {
            export: WorkerPlan::Fail,
            ..Workers::default()
        },
    );
    export_window
        .open_path(&support::verify_project())
        .expect("the project opens");
    export_window.pump(Duration::from_millis(200));
    let out = support::artifact("s15h-down.png");
    let _ = std::fs::remove_file(&out);
    export_window.set_export_settings(&pixlay::export::Settings {
        long_edge: 800,
        format: pixlay_imaging::encode::Format::Png,
        path: out.clone(),
    });
    export_window.start_export(out.clone());
    export_window.pump(Duration::from_millis(100));
    assert!(
        !export_window.progress_revealed(),
        "a failed start must not leave a progress bar up"
    );
    assert!(
        export_window
            .last_toast()
            .is_some_and(|toast| toast.contains("export worker could not be started")),
        "the window says the export worker could not be started, got {:?}",
        export_window.last_toast()
    );
    assert!(!out.exists(), "nothing was written");
    assert!(
        !export_window.exporting(),
        "a failed start leaves no export running"
    );
    assert!(
        export_window.wait_for_idle(support::WAIT),
        "the idle wait returns: nothing is pending for a worker that never ran (grid {:?}, \
         {} decodes so far)",
        export_window.requested_grid(),
        export_window.decoded_sources()
    );

    // ---- the product's own window ----------------------------------------
    // The same calls with every plan `Run`, which is what the checks above are
    // measured against: the canvas does decode, and its bitmaps land.
    let healthy = support::window(&app);
    healthy
        .open_path(&support::verify_project())
        .expect("the project opens");
    support::canvas_bitmaps(&healthy);
    assert!(!healthy.images().1.is_empty(), "the healthy window decodes");
}
