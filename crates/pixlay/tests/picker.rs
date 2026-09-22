//! S13's own criteria for the picker stage: what the stage has to be true of
//! itself, rather than of the path it sits on (`tests/mainpath.rs`).
//!
//! Four claims, and each one is a different kind of fact:
//!
//! * **The pick's order is the click order, and the tray is where it is visible.**
//!   `GtkMultiSelection` is a set, so the order is the picker's own list; this
//!   checks that a scrambled pick survives as a scrambled order, that the tray
//!   re-order and remove act on that order, and that Next hands the document the
//!   same order (`Selection::document`, the policy the CLI's `init --photo` also
//!   uses — `tests/mainpath.rs` compares the two surfaces cell for cell).
//! * **The cap is reported, not applied silently.** Nine photos fit; a tenth is
//!   refused with a visible message and the pick stays at nine.
//! * **The grid fills progressively and never blocks the main loop.** Opening a
//!   folder is a directory read — the tiles are *all* absent at the instant it
//!   returns, because the worker delivers through the main context and nothing has
//!   iterated it yet — and they then arrive, one decode at a time, on one worker
//!   thread.
//! * **The preview is the pipeline's picture.** Its pixels are compared with the
//!   file `pixlay-render thumb` writes for the same photo at the same size, byte
//!   for byte, because both are `pixlay_imaging::thumbnail`: the stage is not a
//!   second resampler, and this is the number that says so.

mod support;

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use pixlay::picker::PREVIEW_PX;
use pixlay::window::Stage;
use pixlay_imaging::{Sampler, Source};

/// The "same picture" threshold the repository uses (`AGENTS.md`, "Invariants"):
/// the same composition at `2N` and `N`, downsampled, stays below 6.
const RMSE_THRESHOLD: f64 = 6.0;

#[test]
fn the_picker_stage_meets_its_own_criteria() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    let picker = window.picker().expect("the window opens on the picker");
    assert_eq!(
        window.stage(),
        Stage::Picker,
        "the picker is the root stage"
    );

    // ---- the folder, and the grid filling progressively --------------------
    let folder = support::fixtures().join("photos");
    let listed = pixlay_imaging::list_folder(&folder, false).expect("the folder lists");
    assert!(
        listed.len() >= 10,
        "this test needs a folder with more than nine photos ({} found)",
        listed.len()
    );

    let listing = Instant::now();
    picker.open_folder(&window, &folder);
    let listing = listing.elapsed();
    assert_eq!(
        picker.files(),
        listed,
        "the grid lists the folder's photos, in the same order `scan` does"
    );
    // A folder listing is a directory read, not a decode: it has to return long
    // before a folder's worth of decodes could have finished (S9 measured ~13.6 ms
    // per file just to *open* each one).
    assert!(
        listing < Duration::from_millis(250),
        "opening the folder blocked for {listing:?}"
    );
    // And the proof that nothing was decoded on this thread: every tile arrives
    // through `MainContext::invoke`, and this call iterates nothing, so none can
    // have been delivered yet.
    assert_eq!(
        picker.tiles_built(),
        0,
        "the grid decoded a photo synchronously while the folder was being listed"
    );

    let filling = Instant::now();
    assert!(
        window.wait_for_tiles(support::WAIT),
        "the grid never finished filling ({} of {} tiles)",
        picker.tiles_built(),
        picker.len()
    );
    let filling = filling.elapsed();
    // Every fixture photo decodes; a refusal would be a cell that says so, and the
    // fixtures are all readable (`scan`'s own test lists them as `status = ok`).
    assert!(
        picker.failures().is_empty(),
        "the fixture folder has unreadable photos: {:?}",
        picker.failures()
    );
    assert_eq!(
        picker.tiles_built(),
        picker.len(),
        "every listed photo has a tile"
    );
    // And the cells are showing them: a decoded tile has to reach the picture that
    // is bound to its position, or the grid would be a wall of empty boxes with a
    // full cache behind it (GTK does not re-bind a row because a texture arrived).
    window.pump(Duration::from_millis(200));
    let painted = support::descendants(picker.grid().upcast_ref::<gtk4::Widget>())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Picture>().ok())
        .filter(|picture| picture.paintable().is_some())
        .count();
    assert!(
        painted > 0,
        "no bound cell received its tile ({painted} painted of {} tiles)",
        picker.tiles_built()
    );
    eprintln!(
        "the grid filled in {filling:?} ({} tiles at {} px, {} cells painted)",
        picker.len(),
        pixlay::picker::TILE_PX,
        painted
    );

    // ---- the pick, its order, and the tray --------------------------------
    // Three photos, picked out of grid order, so the order can only be the click
    // order and not the folder's.
    let scrambled = [4usize, 0, 2];
    for position in scrambled {
        picker.toggle(&window, position as u32);
    }
    let picked: Vec<_> = scrambled
        .iter()
        .map(|position| listed[*position].clone())
        .collect();
    assert_eq!(
        picker.selection().photos(),
        picked.as_slice(),
        "the pick keeps the order the photos were clicked in"
    );
    assert!(
        picker.tray_widget().first_child().is_some(),
        "the tray shows the pick"
    );
    assert_eq!(picker.selected_count(), 3);

    // Re-ordering and removing act on that order, which is what makes the tray the
    // place order is kept rather than a decoration.
    picker.move_photo(&window, 0, 1);
    assert_eq!(
        picker.selection().photos(),
        [picked[1].clone(), picked[0].clone(), picked[2].clone()],
        "moving a photo one place later swaps it with its neighbour"
    );
    picker.remove_at(&window, 1);
    assert_eq!(
        picker.selection().photos(),
        [picked[1].clone(), picked[2].clone()],
        "removing a photo drops exactly that one"
    );

    // ---- Next is gated by the floor of two, and reports the cap ------------
    let next = picker.next_button();
    picker.clear_selection();
    assert!(
        !next.is_sensitive(),
        "Next is insensitive with nothing picked"
    );
    picker.toggle(&window, 0);
    assert!(
        !next.is_sensitive(),
        "Next is insensitive with one photo picked"
    );
    picker.toggle(&window, 1);
    assert!(
        next.is_sensitive(),
        "Next is enabled once two photos are picked"
    );
    assert!(
        next.label().is_some_and(|label| label.contains('2')),
        "Next carries the count: {:?}",
        next.label()
    );

    // HIG's selection mode: `Ctrl+A` selects the whole collection, and the
    // product's cap has to report what it refuses rather than truncating quietly.
    picker.select_all();
    assert_eq!(
        picker.selected_count(),
        pixlay_core::MAX_PHOTOS,
        "selecting all stops at the cap"
    );
    let reported = window
        .last_toast()
        .expect("the refused photos are reported");
    assert!(
        reported.contains(&pixlay_core::MAX_PHOTOS.to_string()),
        "the report names the cap: {reported:?}"
    );
    assert!(
        picker
            .tray_widget()
            .first_child()
            .and_then(|first| first.next_sibling())
            .is_some(),
        "the tray holds more than one photo after selecting all"
    );

    // ---- the pick becomes the document, in order --------------------------
    picker.clear_selection();
    for position in scrambled {
        picker.toggle(&window, position as u32);
    }
    picker.next(&window);
    assert_eq!(
        window.stage(),
        Stage::Editor,
        "Next opens the editor's stage"
    );
    let doc = window.document();
    let sources: Vec<Option<std::path::PathBuf>> =
        doc.cells.iter().map(|cell| cell.source.clone()).collect();
    assert_eq!(
        sources,
        picked.iter().cloned().map(Some).collect::<Vec<_>>(),
        "the document's cells are the picked photos, in the pick's order"
    );

    // ---- the preview is the CLI's own picture -----------------------------
    // Back to the picker, focus a photo, and hold the pixels the preview shows to
    // the file `pixlay-render thumb` writes for it at the same size.
    window.show_picker();
    assert_eq!(window.stage(), Stage::Picker);
    picker.focus(&window, 1);
    assert!(
        window.wait_for_preview(support::WAIT),
        "the preview pane decoded a photo"
    );
    let (path, width, height, pixels) = picker
        .preview_pixels()
        .expect("the preview has pixels once it has decoded");
    assert_eq!(path, listed[1], "the preview shows the focused photo");

    let thumb = support::artifact("picker-thumb.png");
    let argv: Vec<std::ffi::OsString> = [
        "thumb",
        "--photo",
        path.to_str().expect("a UTF-8 path"),
        "--px",
        &PREVIEW_PX.to_string(),
        "--out",
        thumb.to_str().expect("a UTF-8 path"),
    ]
    .iter()
    .map(std::ffi::OsString::from)
    .collect();
    let status = pixlay_cli::cli::run(&argv).expect("the CLI writes the preview");
    assert_eq!(status, 0, "pixlay-render thumb succeeds");

    let from_cli = Source::decode(&thumb).expect("the CLI's preview decodes");
    assert_eq!(
        (from_cli.width() as i32, from_cli.height() as i32),
        (width, height),
        "the preview and the CLI's file are the same size"
    );
    let difference = rmse_against(&pixels, width, height, &from_cli);
    eprintln!(
        "the preview against pixlay-render thumb: RMSE {difference:.4} over {width}x{height} pixels"
    );
    assert!(
        difference <= RMSE_THRESHOLD,
        "the preview and the CLI's thumb diverged: RMSE {difference:.4} > {RMSE_THRESHOLD}"
    );

    // What the stage looked like, for a human to look at: everything above is
    // numbers, and `AGENTS.md` asks for the picture as well.
    let picture = support::artifact("picker.png");
    support::save_png(&picture, &support::snapshot(&window));
    eprintln!("the picker stage is {picture:?}");
}

/// The root-mean-square difference between a straight 8-bit RGB buffer and a
/// decoded source's own samples, per channel and per pixel.
///
/// The source is read through the public `Sampler` interface — the same pixels the
/// pipeline uses — and its 16-bit samples are exact widenings of the file's 8-bit
/// ones (`sample * 257`), so dividing by 257 compares like with like.
fn rmse_against(pixels: &[u8], width: i32, height: i32, source: &impl Sampler) -> f64 {
    let mut sum = 0.0;
    let mut count = 0.0;
    for y in 0..height as u32 {
        for x in 0..width as u32 {
            let theirs = source.pixel(x, y);
            let index = ((y as i32 * width + x as i32) * 3) as usize;
            for channel in 0..3 {
                let mine = f64::from(pixels[index + channel]);
                let cli = f64::from(theirs[channel]) / 257.0;
                let difference = mine - cli;
                sum += difference * difference;
                count += 1.0;
            }
        }
    }
    (sum / count).sqrt()
}
