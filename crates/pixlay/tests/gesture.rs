//! S12's GUI criteria, driven through the window the way a pointer and a wheel
//! drive it.
//!
//! Three claims are checked here, and each of them is a different kind of fact:
//!
//! * **A live gesture is drawn from the coarse grid, and its release is not.**
//!   Every step of a drag asks the decoder for `gesture_grid(resting)`, and the
//!   moment the gesture ends the canvas asks for the resting grid again — which is
//!   what makes the released frame a real render rather than an upscaled gesture
//!   frame. That last part is compared, pixel for pixel, against the *same document
//!   drawn in one edit at rest*: not an RMSE threshold, because the promise is that
//!   the two are the same function at the same grid, and a tolerance would hide
//!   exactly the drift this test exists to catch.
//! * **The window never waits for the decoder.** The steps are sent with the main
//!   context *not* iterated: if anything in the gesture path waited for the worker,
//!   the calls could not return, and the bitmaps would have moved by the time they
//!   did. The grid a step asks for is read from the request rather than from the
//!   reply it eventually gets, so this holds on a loaded machine as well as an idle
//!   one — the reply's own grid is `pixlay-imaging`'s to assert.
//! * **A live gesture decodes nothing.** The window counts what the decoding thread
//!   decoded, and a drag may not add to it. That count is this layer's form of the
//!   claim `pixlay-imaging`'s own tests make about the caches, so a cache that
//!   quietly stopped working fails in two places rather than in none.
//!
//! The document and the canvas size are the test's own: it is about *which grid* a
//! gesture asks for, so a small canvas with small photos keeps the whole test in the
//! millisecond range.

mod support;

use std::time::Duration;

use gtk4::prelude::*;

use pixlay::canvas::{Gesture, MARGIN};
use pixlay_core::{CanvasSpec, Command, CropTransform, templates};
use pixlay_imaging::gesture_grid;

/// The canvas grid this test works at, in pixels: small, so the whole test is a
/// few hundred milliseconds of work in a debug build.
const GRID: i32 = 400;

#[test]
fn a_live_gesture_refines_into_the_resting_grids_own_pixels() {
    support::start();
    let app = support::app();
    let window = support::window(&app);

    let template = templates::all()
        .into_iter()
        .find(|template| template.slots.len() == 2)
        .expect("the library has a two-slot layout");
    let mut doc =
        pixlay_core::CollageDoc::new(CanvasSpec::with_ratio(template.aspect, 297.0), template);
    doc.cells[0].source = Some(support::photo("square.png"));
    doc.cells[1].source = Some(support::photo("ratio-4-3.png"));
    let project = support::artifact("gesture.pixlay");
    doc.save(&project).expect("the test project is written");

    window
        .open_path(&project)
        .expect("the window opens the test project");
    // Pin the canvas widget, so the grid is this test's number rather than
    // whatever the session's window manager hands out.
    let area = window.canvas_widget();
    area.set_hexpand(false);
    area.set_vexpand(false);
    area.set_size_request(GRID + 2 * MARGIN as i32, GRID + 2 * MARGIN as i32);
    window.pump(Duration::from_millis(300));
    assert!(
        window.wait_for_idle(support::WAIT),
        "the opening decode finished"
    );

    let resting = window.images().0;
    let decoded = window.decoded_sources();
    assert!(decoded > 0, "opening the document decodes its photos");

    // The framing gesture, and the same edit as the one command the direct path
    // applies: the fit is idempotent, so both end as the same drawn transform.
    let slot = 0;
    window.select(Some(slot));
    let base = window.fitted_crop(slot).expect("the slot is framed");
    let target = CropTransform {
        zoom: base.zoom * 1.25,
        offset: (base.offset.0.clamp(-0.8, 0.8) + 0.05, base.offset.1),
        rotation_deg: base.rotation_deg + 7.0,
    };

    // ---- the reference: one edit, at rest ---------------------------------
    window
        .apply(Command::SetCrop { slot, crop: target })
        .expect("the direct edit is accepted");
    assert!(window.wait_for_idle(support::WAIT));
    assert_eq!(
        window.images().0,
        resting,
        "an edit at rest is drawn at the resting grid"
    );
    let direct = support::snapshot(&area);
    let direct_framing = window.fitted_crop(slot).expect("the slot is framed");

    // ---- the same edit, made by a live gesture ----------------------------
    window.undo();
    assert!(window.wait_for_idle(support::WAIT));
    assert_eq!(window.images().0, resting);

    // One live step, and the canvas asks for the *coarse* grid: this is the
    // coarsening itself, and it is a question about the request rather than about
    // when its reply lands (a reply is built for the grid it was asked for, which
    // the imaging crate's tests hold to byte-identical bitmaps).
    let coarse = gesture_grid(resting);
    assert_ne!(coarse, resting, "the gesture grid is a smaller grid");
    window.gesture(Gesture::Crop {
        slot,
        crop: turn(base, 2.0),
    });
    assert_eq!(
        window.requested_grid(),
        Some(coarse),
        "a live gesture asks for the gesture grid"
    );
    // …and nothing has been serviced yet: the bitmaps are still the resting grid's,
    // because the reply crosses to the main context and that has not been iterated.
    // This is "the window never waits for the decoder", in the form a test can hold
    // it: the call returned, and the work is somewhere else.
    assert_eq!(
        window.images().0,
        resting,
        "a gesture step must not be serviced synchronously"
    );

    // The rest of the drag, still with the main context **not** iterated between the
    // steps: this is what the loop sees while a pointer moves, and none of it may
    // wait for the decoding thread.
    for step in 3..=12 {
        window.gesture(Gesture::Crop {
            slot,
            crop: turn(base, f64::from(step)),
        });
        assert_eq!(
            window.requested_grid(),
            Some(coarse),
            "every step of a live gesture asks for the gesture grid"
        );
    }
    assert_eq!(
        window.images().0,
        resting,
        "the burst did not wait for the decoder"
    );
    assert_eq!(
        window.decoded_sources(),
        decoded,
        "a live gesture decodes nothing at all"
    );

    // The release: the gesture ends, the canvas returns to the resting grid, and the
    // framing the reference applied is applied as one finished step.
    window.gesture(Gesture::End);
    assert!(window.wait_for_idle(support::WAIT));
    assert_eq!(
        window.images().0,
        resting,
        "the release is drawn at the resting grid"
    );
    window.gesture(Gesture::Step { slot, crop: target });
    assert!(window.wait_for_idle(support::WAIT));
    assert_eq!(window.images().0, resting);
    assert_eq!(
        window.decoded_sources(),
        decoded,
        "a live gesture must not touch the disk"
    );

    let refined = support::snapshot(&area);
    let difference = support::rmse(&refined, &direct);
    eprintln!(
        "refined vs direct: RMSE {difference:.4} over {} pixels",
        refined.0 * refined.1
    );
    // Exact, not "close": the refined frame is built by the same function at the
    // same grid as the direct one, and anything above zero would mean the coarse
    // grid left a trace in the pixels.
    assert!(
        difference == 0.0,
        "the coarse frame survived the release: RMSE {difference:.4}"
    );

    // The two paths left the document framing the slot the same way, which is the
    // other half of "the gesture changed nothing but the framing it was asked to
    // change": each path stored its own request, and the *fit* — the thing the user
    // is looking at — is the same one.
    assert_eq!(
        window.fitted_crop(slot).expect("the slot is framed"),
        direct_framing,
        "the gesture and the direct edit must fit the cell identically"
    );
}

/// One step of a straightening-and-zooming drag: the base framing with one more
/// degree of rotation.
fn turn(base: CropTransform, degrees: f64) -> CropTransform {
    CropTransform {
        rotation_deg: base.rotation_deg + degrees,
        ..base
    }
}
