//! S13's and S13b's criteria for the picker stage: what the stage has to be true
//! of itself, rather than of the path it sits on (`tests/mainpath.rs`).
//!
//! The claims, and each one is a different kind of fact:
//!
//! * **The pick's order is the click order, and the picked list is where it is
//!   visible.** `GtkMultiSelection` is a set, so the order is the picker's own
//!   list; this checks that a scrambled pick survives as a scrambled order, that a
//!   row drag and `Ctrl+Up`/`Ctrl+Down` act on that order, and that Next hands the
//!   document the same order (`Selection::document`, the policy the CLI's
//!   `init --photo` also uses — `tests/mainpath.rs` compares the two surfaces cell
//!   for cell).
//! * **The cap is reported, not applied silently.** Nine photos fit; a tenth is
//!   refused with a visible message and the pick stays at nine.
//! * **A picked cell carries the highlight, and the highlight really draws** (the
//!   2026-09-22 ruling): the cell has the `.picked` class, no `.selection-mode`
//!   check button is left in the stage, and the grid's own pixels change when a
//!   cell is picked.
//! * **The grid asks for the tiles of the cells it is showing**, never for a
//!   folder's worth (the ruling's visible-first policy): opening a folder returns
//!   before *any* decode, a 300-photo folder costs the same bounded number of
//!   requests as a 14-photo one, and the answer arrives on the worker thread.
//! * **The pane is decoded at the size it draws, and it never paints a tile.**
//!   Regression test for the reported blur, and for the second defect the
//!   2026-09-22 ruling named: the pane's photo is the `Contain` fit of the pane's
//!   own device size against the photo's own pixels, rounded up to one
//!   `PREVIEW_PX_STEP` (S13b asked for the pane's long edge whatever the aspect,
//!   1.5× the long edge of a portrait in a landscape pane), the picture it paints
//!   is that photo's own texture, and coming back to a photo — after focusing
//!   another, or after changing folder — does not leave the cell's tile in the pane.
//! * **The stage is the ruled shape.** The media area takes ≥ 80 % of the band above
//!   the status bar, the picked list's height is the pane's, one full-width row of
//!   128 px cells sits below both, and the status bar's four fields — `picked /
//!   total`, the photo's pixels, its size, the zoom — are what the ruling says
//!   (`S13c`).
//! * **The preview is the pipeline's picture.** Its pixels are compared with the
//!   file `pixlay-render thumb` writes for the same photo at the same size, because
//!   both are `pixlay_imaging::thumbnail`: the stage is not a second resampler, and
//!   this is the number that says so.

mod support;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use pixlay::picker::{
    PREVIEW_MAX_PX, PREVIEW_PX_STEP, Picker, STATUS_FIELDS, TILE_REQUEST_MAX, TILE_SIZE,
};
use pixlay::window::Stage;
use pixlay_imaging::{Sampler, Source};

/// The "same picture" threshold the repository uses (`AGENTS.md`, "Invariants"):
/// the same composition at `2N` and `N`, downsampled, stays below 6.
const RMSE_THRESHOLD: f64 = 6.0;

/// How many copies the "big folder" case makes.
const BIG_FOLDER: usize = 300;

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

    // ---- the folder, and the grid filling visible-first --------------------
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
    window.pump(Duration::from_millis(300));
    // What a folder costs is the cells on screen, not the files in it (S13b): the
    // bound cells ask for their tiles and nothing else does.
    assert!(
        picker.tile_requests() > 0,
        "the bound cells never asked for their tiles"
    );
    assert!(
        picker.tile_requests() <= TILE_REQUEST_MAX,
        "opening a {} photo folder cost {} tile requests, past the bound of {}",
        picker.len(),
        picker.tile_requests(),
        TILE_REQUEST_MAX
    );

    let filling = Instant::now();
    assert!(
        window.wait_for_tiles(support::WAIT),
        "the grid never finished filling ({} tiles, {} in flight)",
        picker.tiles_built(),
        picker.pending_tiles()
    );
    let filling = filling.elapsed();
    // Every fixture photo decodes; a refusal would be a cell that says so, and the
    // fixtures are all readable (`scan`'s own test lists them as `status = ok`).
    assert!(
        picker.failures().is_empty(),
        "the fixture folder has unreadable photos: {:?}",
        picker.failures()
    );
    // And the cells are showing them: a decoded tile has to reach the widget that
    // is bound to its position, or the grid would be a wall of spinners with a
    // full cache behind it (GTK does not re-bind a row because a texture arrived).
    window.pump(Duration::from_millis(200));
    let painted = painted_cells(&picker);
    assert!(
        painted > 0,
        "no bound cell received its tile ({} painted of {} tiles)",
        painted,
        picker.tiles_built()
    );
    eprintln!(
        "the grid filled in {filling:?} ({} tiles in {} requests at {} px, {} cells painted)",
        picker.tiles_built(),
        picker.tile_requests(),
        picker.tile_px(),
        painted
    );

    // ---- the pick, its order, and the picked list --------------------------
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
    assert_eq!(picker.selected_count(), 3);
    assert_eq!(
        picked_rows(&picker.picked_list()),
        names(&picked),
        "the picked list's rows are the pick's order"
    );

    // The drag is wired: every row can be dragged and the list accepts the drop
    // (this is the pointer path the ruling chose; the check below drives the same
    // move the drop handler does).
    let rows = picked_rows(&picker.picked_list());
    let first_row = picker
        .picked_list()
        .row_at_index(0)
        .expect("the first picked row exists");
    assert!(
        controllers_of::<gtk4::DragSource>(&first_row.upcast()) > 0,
        "a picked row cannot be dragged"
    );
    assert!(
        controllers_of::<gtk4::DropTarget>(&picker.picked_list().upcast()) > 0,
        "the picked list does not accept a dropped row"
    );
    assert_eq!(rows.len(), 3, "three rows before the drag");

    // A drag moves exactly one entry: row 0 dropped on position 2.
    picker.move_row(&window, 0, 2);
    assert_eq!(
        picker.selection().photos(),
        [picked[1].clone(), picked[2].clone(), picked[0].clone()],
        "a drop on position 2 puts the dragged photo there and moves nothing else"
    );
    assert_eq!(
        picked_rows(&picker.picked_list()),
        names(picker.selection().photos()),
        "and the list shows it"
    );

    // The same action from the keyboard, through the list's own shortcut
    // controller: the trigger the user presses and the action it fires, not a
    // private copy of either.
    window.pump(Duration::from_millis(100));
    let list = picker.picked_list();
    assert!(
        list.focus_child().is_some(),
        "a moved row has to keep the focus, or the next Ctrl+Up acts on nothing"
    );
    assert!(
        press(&list, "Up"),
        "Ctrl+Up was not handled by the picked list"
    );
    assert_eq!(
        picker.selection().photos(),
        [picked[1].clone(), picked[0].clone(), picked[2].clone()],
        "Ctrl+Up on the focused row moves it one place earlier"
    );
    window.pump(Duration::from_millis(100));
    assert!(
        press(&picker.picked_list(), "Down"),
        "Ctrl+Down was not handled by the picked list"
    );
    assert_eq!(
        picker.selection().photos(),
        [picked[1].clone(), picked[2].clone(), picked[0].clone()],
        "Ctrl+Down moves the same row back"
    );

    // Removing is the row's own button, and it drops exactly that entry.
    picker.remove_at(&window, 1);
    assert_eq!(
        picker.selection().photos(),
        [picked[1].clone(), picked[0].clone()],
        "removing a photo drops exactly that one"
    );

    // ---- Next is gated by the floor of two, and reports the cap ------------
    let next = picker.next_button();
    let grid = picker.grid();
    picker.clear_selection(&window);
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
        next_label(&picker).contains('2'),
        "Next carries the count: {:?}",
        next_label(&picker)
    );

    // HIG's selection mode: `Ctrl+A` selects the whole collection, and the
    // product's cap has to report what it refuses rather than truncating quietly.
    // The key's own path is what is driven — `GtkListBase`'s `list.select-all`, the
    // only binding for it since S13c (S13 had a second one, so one press fired
    // twice).
    grid.activate_action("list.select-all", None)
        .expect("the grid binds Ctrl+A to list.select-all");
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
        picked_rows(&picker.picked_list()).len() > 1,
        "the picked list holds more than one photo after selecting all"
    );

    // ---- the pick becomes the document, in order --------------------------
    picker.clear_selection(&window);
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
    let sources: Vec<Option<PathBuf>> = doc.cells.iter().map(|cell| cell.source.clone()).collect();
    assert_eq!(
        sources,
        picked.iter().cloned().map(Some).collect::<Vec<_>>(),
        "the document's cells are the picked photos, in the pick's order"
    );
    window.show_picker();
    assert_eq!(window.stage(), Stage::Picker);

    // ---- the highlight ----------------------------------------------------
    // The 2026-09-22 ruling: a picked cell is shown by a highlight, and the
    // platform's check mark is gone from the stage entirely.
    let checks: Vec<gtk4::Widget> =
        support::descendants(picker.grid().upcast_ref::<gtk4::Widget>())
            .into_iter()
            .filter(|widget| {
                widget.is::<gtk4::CheckButton>() && widget.has_css_class("selection-mode")
            })
            .collect();
    assert!(
        checks.is_empty(),
        "the stage still carries {} .selection-mode check button(s)",
        checks.len()
    );
    let cell = picker
        .cell_widget(0)
        .expect("position 0 is on screen and bound");
    assert!(
        cell.has_css_class("picker-cell"),
        "a cell carries the .picker-cell class"
    );
    assert!(
        cell.has_css_class("picked"),
        "a picked cell carries the highlight class"
    );
    // Position 1 is not in the pick, so its cell must not carry it.
    let other = picker
        .cell_widget(1)
        .expect("position 1 is on screen and bound");
    assert!(
        !other.has_css_class("picked"),
        "an unpicked cell must not carry the highlight class"
    );
    // Two frames, not a 200 ms guess: a snapshot reads the widgets' cached render
    // nodes, so a probe taken before the frame that carries the change compares the
    // same pixels twice (measured 2026-09-24: RMSE 0.000 on both sides).
    let painted = support::after_frames(&window, 2, support::WAIT);
    assert!(
        painted >= 2,
        "the picked grid was not drawn ({painted} frames)"
    );
    let before = support::snapshot(&picker.grid());
    picker.clear_selection(&window);
    let painted = support::after_frames(&window, 2, support::WAIT);
    assert!(
        painted >= 2,
        "the cleared grid was not drawn ({painted} frames)"
    );
    // The probe waits for *its* observation — the grid drawn without the highlight —
    // rather than for a span of time: a snapshot reads the widgets' cached render
    // nodes, so a probe taken before the frame that carries the change compares the
    // same pixels twice (measured 2026-09-24: RMSE 0.000 on both sides).
    let difference = support::settle_by(&window, support::PROBE_WAIT, || {
        let after = support::snapshot(&picker.grid());
        let difference = support::rmse(&before, &after);
        (difference, difference > 1.0)
    });
    eprintln!("the highlight is worth RMSE {difference:.3} of the grid's pixels");
    assert!(
        difference > 1.0,
        "picking and unpicking a cell did not change the grid's pixels ({difference:.3}): \
         the highlight is not drawing (the class is {} after clearing, the cell widget is {})",
        if picker
            .cell_widget(0)
            .is_some_and(|cell| cell.has_css_class("picked"))
        {
            "still there"
        } else {
            "gone"
        },
        picker.cell_widget(0).is_some(),
    );
    assert!(
        !picker
            .cell_widget(0)
            .is_some_and(|cell| cell.has_css_class("picked")),
        "clearing the pick drops the highlight"
    );

    // ---- the pane is decoded at the size it draws, and never a tile ---------
    // Two defects in one criterion. The reported blur: what the pane paints is the
    // focused photo's own texture, never a cell's tile. And S13b's over-large
    // decode: the request is the `Contain` fit of the pane's device size against the
    // photo's *own* pixels, rounded up to one step — S13b asked for the pane's long
    // edge whatever the aspect, which is 1.5× the long edge (2.25× the pixels) for a
    // portrait in a landscape pane.
    let pane = picker.preview_widget();
    let pane_edge = pane.width().max(pane.height()).max(0) as u32 * pane.scale_factor() as u32;
    for name in ["portrait.jpg", "landscape.jpg", "square.png"] {
        let position = listed
            .iter()
            .position(|path| path.ends_with(name))
            .unwrap_or_else(|| panic!("{name} is in the fixture folder"));
        picker.focus(&window, position);
        assert!(
            window.wait_for_preview(support::WAIT),
            "the pane never decoded {name}"
        );
        let wanted = decoded_long_edge(&picker, &listed[position]);
        let drawn = fitted_long_edge(&picker, &listed[position]);
        assert_eq!(
            picker.preview_px(),
            wanted,
            "{name}: the pane decoded {} px where it draws {} px",
            picker.preview_px(),
            drawn
        );
        assert!(
            picker.preview_px() <= pane_edge + PREVIEW_PX_STEP,
            "{name}: the pane is {pane_edge} device pixels long and the decode is {}",
            picker.preview_px()
        );
        let (width, height) = pane_is_the_preview(&picker, position);
        assert!(
            width.max(height) as u32 > TILE_SIZE as u32,
            "the pane is showing a {width}x{height} picture, which is a cell's tile"
        );
        eprintln!(
            "{name}: the pane decoded at {} px for a {}x{} pane ({} device pixels), drawing {drawn} px",
            picker.preview_px(),
            pane.width(),
            pane.height(),
            pane_edge
        );
    }

    // Focus another photo and come back: S13 left the *tile* in the pane when the
    // preview of a photo it had already shown was refused as a repeat.
    picker.toggle(&window, 2);
    assert!(
        window.wait_for_preview(support::WAIT),
        "the pane never decoded the picked photo"
    );
    picker.focus(&window, 0);
    assert!(
        window.wait_for_preview(support::WAIT),
        "the pane never decoded the second photo"
    );
    picker.focus(&window, 2);
    pane_is_the_preview(&picker, 2);
    assert!(
        window.wait_for_preview(support::WAIT),
        "coming back to a photo has to show its preview, not its tile"
    );
    pane_is_the_preview(&picker, 2);

    // And a folder change invalidates it all: the file at an index is a different
    // photo afterwards, so nothing about the old listing may be painted.
    picker.open_folder(&window, &folder);
    window.pump(Duration::from_millis(100));
    assert!(
        picker.preview_pixels().is_none(),
        "a folder change has to clear the pane, not leave the old photo in it"
    );
    picker.focus(&window, 1);
    assert!(
        window.wait_for_preview(support::WAIT),
        "the pane never decoded the photo after the folder change"
    );
    pane_is_the_preview(&picker, 1);

    // ---- the pane's pixels are the CLI's own picture -----------------------------
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
        &picker.preview_px().to_string(),
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
        "the preview against pixlay-render thumb at {} px: RMSE {difference:.4} over {width}x{height} pixels",
        picker.preview_px()
    );
    assert!(
        difference <= RMSE_THRESHOLD,
        "the preview and the CLI's thumb diverged: RMSE {difference:.4} > {RMSE_THRESHOLD}"
    );

    // ---- the divider is the session's --------------------------------------
    // The position is what the user chose, so a second window opens where the first
    // one was left (`SPLITS`), not at the default.
    picker.set_split_position(500);
    window.pump(Duration::from_millis(100));
    assert_eq!(
        picker.split_position(),
        500,
        "the divider reports the position it was given"
    );
    let other = support::second_window(&app);
    let other_picker = other.picker().expect("the second window has a picker");
    assert_eq!(
        other_picker.split_position(),
        500,
        "a new window opens on the divider the session left behind"
    );

    // ---- a folder of hundreds of photos costs what a folder of ten does ----
    let many = support::out_dir().join("many");
    let _ = std::fs::remove_dir_all(&many);
    std::fs::create_dir_all(&many).expect("the big folder can be created");
    let original = listed
        .iter()
        .find(|path| path.ends_with("square.png"))
        .expect("the fixture folder has square.png");
    for index in 0..BIG_FOLDER {
        std::fs::copy(original, many.join(format!("photo-{index:04}.png")))
            .expect("a copy can be made");
    }
    let opening = Instant::now();
    picker.open_folder(&window, &many);
    let opening = opening.elapsed();
    assert_eq!(picker.len(), BIG_FOLDER);
    assert_eq!(
        picker.tiles_built(),
        0,
        "opening a big folder decoded something before it returned"
    );
    window.pump(Duration::from_millis(500));
    eprintln!(
        "a {BIG_FOLDER}-photo folder opened in {opening:?} and asked for {} tiles",
        picker.tile_requests()
    );
    assert!(
        picker.tile_requests() > 0,
        "the big folder's visible cells never asked for their tiles"
    );
    assert!(
        picker.tile_requests() <= TILE_REQUEST_MAX,
        "opening a {BIG_FOLDER}-photo folder cost {} tile requests, past the bound of {}",
        picker.tile_requests(),
        TILE_REQUEST_MAX
    );

    // Visible-first, not visible-only: a cell that was off screen when the folder
    // opened gets its tile when it arrives on screen, and only then.
    let before = picker.tile_requests();
    picker
        .grid()
        .scroll_to(200, gtk4::ListScrollFlags::NONE, None);
    assert!(
        window.wait_for_tiles(support::WAIT),
        "the tiles of the scrolled-to cells never arrived"
    );
    let scrolled = picker.tile_requests() - before;
    eprintln!("scrolling to the 200th photo cost {scrolled} tile requests");
    assert!(
        scrolled > 0,
        "scrolling to a far row never asked for its tile"
    );
    assert!(
        scrolled <= TILE_REQUEST_MAX,
        "scrolling to the 200th photo cost {scrolled} tile requests, past the bound of {TILE_REQUEST_MAX}"
    );

    // ---- what the stage looks like -----------------------------------------
    // Back on the fixture folder with a pick of three, so the picture a human looks
    // at — and every geometry the checks below read — is a real state of the stage
    // rather than the last thing a check left behind.
    picker.open_folder(&window, &folder);
    for position in scrambled {
        picker.toggle(&window, position as u32);
    }
    picker.focus(&window, 1);
    let _ = window.wait_for_preview(support::WAIT);
    let _ = window.wait_for_tiles(support::WAIT);
    window.pump(Duration::from_millis(300));

    // ---- the ruled shape, and the status line ------------------------------
    // The 2026-09-22 ruling's arrangement, as geometry and as numbers: the media
    // area is the majority of the band above the status bar, the picked list is the
    // pane's own height, the strip is one full-width row of 128 px cells below both,
    // and the four fields of the status bar are computed, not pasted.
    //
    // "The band above the status bar" is the ruling's own measurement, so it is the
    // **content** band: from the pane's top edge to the status bar's — the header bar
    // is chrome above it, and the reference's own 88 % is `860 / 974`, the media area
    // against the content band *excluding* the header (`docs/2026-09-22-STEPS.md`,
    // `S13c · What the reference actually measures`).
    let root = picker.root().upcast::<gtk4::Widget>();
    let pane = picker.preview_widget().upcast::<gtk4::Widget>();
    let list = picker.picked_list().upcast::<gtk4::Widget>();
    let strip = picker.strip().upcast::<gtk4::Widget>();
    let status_bar = picker.status_bar().upcast::<gtk4::Widget>();
    let corner = |widget: &gtk4::Widget, x: f32, y: f32| {
        widget
            .compute_point(&root, &gtk4::graphene::Point::new(x, y))
            .map(|point| (point.x(), point.y()))
    };
    let band = corner(&status_bar, 0.0, 0.0)
        .expect("the status bar is in the page")
        .1
        - corner(&pane, 0.0, 0.0)
            .expect("the media area is in the page")
            .1;
    let pane_share = f64::from(pane.height()) / f64::from(band);
    let cell = picker.cell_widget(1).expect("a bound cell");
    eprintln!(
        "the content band above the status bar is {band:.0} px: the media area is {:.1}% of it \
         ({} px), the strip is {} px tall (grid {}, cell {}x{} in an item {} px tall), the status \
         bar {} px",
        pane_share * 100.0,
        pane.height(),
        strip.height(),
        picker.grid().height(),
        cell.width(),
        cell.height(),
        cell.parent().map(|item| item.height()).unwrap_or(-1),
        status_bar.height()
    );
    assert!(
        pane_share >= 0.8,
        "the media area takes {:.1}% of the band above the status bar, under the ruled 80%",
        pane_share * 100.0
    );
    assert_eq!(
        (
            list.height(),
            corner(&list, 0.0, 0.0).map(|(_, y)| y.round())
        ),
        (
            pane.height(),
            corner(&pane, 0.0, 0.0).map(|(_, y)| y.round())
        ),
        "the picked list has to be the media area's own height and start with it"
    );
    let top = |widget: &gtk4::Widget| corner(widget, 0.0, 0.0).expect("the band is in the page").1;
    let strip_top = top(&strip);
    let pane_bottom = top(&pane) + pane.height() as f32;
    let list_bottom = top(&list) + list.height() as f32;
    let status_top = top(&status_bar);
    eprintln!(
        "the bands: media area {:.0}..{pane_bottom:.0}, picked list {:.0}..{list_bottom:.0}, \
         strip {strip_top:.0}..{:.0}, status bar {status_top:.0}..",
        top(&pane),
        top(&list),
        strip_top + strip.height() as f32,
    );
    assert!(
        strip_top >= pane_bottom && strip_top >= list_bottom,
        "the strip ({strip_top:.0}) has to be below the media area ({pane_bottom:.0}) and the \
         picked list ({list_bottom:.0})"
    );
    assert_eq!(
        (
            corner(&strip, 0.0, 0.0).map(|(x, _)| x.round()),
            corner(&strip, strip.width() as f32, 0.0).map(|(x, _)| x.round()),
        ),
        (
            corner(&pane, 0.0, 0.0).map(|(x, _)| x.round()),
            Some(root.width() as f32),
        ),
        "the strip spans from the media area's left edge to the window's right edge"
    );
    assert!(
        status_bar.height() >= 24,
        "the status bar is {} px tall, under the ruled 24",
        status_bar.height()
    );
    assert!(
        status_top >= strip_top + strip.height() as f32,
        "the status bar ({status_top:.0}) has to be the last band, below the strip ({:.0})",
        strip_top + strip.height() as f32
    );
    // A cell is 128 logical px plus the CSS border, and the tile is decoded at that
    // size times the screen's scale factor.
    assert!(
        (TILE_SIZE..=TILE_SIZE + 4).contains(&cell.width()),
        "a cell is {} px wide, not {TILE_SIZE} plus its border",
        cell.width()
    );
    assert_eq!(
        picker.tile_px(),
        TILE_SIZE as u32 * cell.scale_factor() as u32,
        "the tile's decode is the cell's own device pixels"
    );

    // The four fields, each one computed from what it is about: the pick and the
    // folder, the focused photo's own pixels, its file's size, and the zoom the pane
    // is drawing at.
    picker.focus(&window, square_position(&listed));
    assert!(
        window.wait_for_preview(support::WAIT),
        "the pane never decoded the photo the status line is checked on"
    );
    let focused = picker
        .file(picker.focused().expect("a photo is focused"))
        .expect("the focused photo is in the listing");
    let source = Source::decode(&focused).expect("the fixture decodes");
    let bytes = std::fs::metadata(&focused)
        .expect("the fixture exists")
        .len();
    let zoom = (100.0 * f64::from(fitted_long_edge(&picker, &focused))
        / f64::from(source.width().max(source.height())))
    .round() as u32;
    let expected: [String; STATUS_FIELDS] = [
        format!("{} / {}", picker.selected_count(), picker.len()),
        format!("{} × {}", source.width(), source.height()),
        glib::format_size(bytes).to_string(),
        format!("{zoom}%"),
    ];
    eprintln!(
        "the status line reads {:?}",
        picker.status_line().join("   ")
    );
    assert_eq!(
        picker.status_line(),
        expected,
        "the status bar's four fields are gthumb's, computed from the stage's own numbers"
    );

    // ---- the picked list switches the pane ---------------------------------
    // Ruling 21: clicking a row previews that photo — S13b's list only selected, and
    // the preview followed the *strip's* click. The row's own `row-selected` is what
    // a click emits, so that is what this drives.
    let row = picker.picked_list().row_at_index(1).expect("a picked row");
    picker.picked_list().select_row(Some(&row));
    window.pump(Duration::from_millis(100));
    let photo = picker.selection().photos()[1].clone();
    let wanted = picker
        .files()
        .iter()
        .position(|file| *file == photo)
        .expect("the picked photo is in the folder");
    assert_eq!(
        picker.focused(),
        Some(wanted),
        "clicking a picked row has to focus that photo"
    );
    assert!(
        window.wait_for_preview(support::WAIT),
        "the pane never decoded the photo the picked row stands for"
    );
    pane_is_the_preview(&picker, wanted);
    let source = Source::decode(&photo).expect("the fixture decodes");
    assert_eq!(
        picker.status_line()[1],
        format!("{} × {}", source.width(), source.height()),
        "and the status line has to follow it"
    );

    let picture = support::artifact("picker.png");
    support::save_png(&picture, &support::snapshot(&window));
    eprintln!("the picker stage is {picture:?}");
}

/// The index of the square fixture, which is the photo the status line is checked
/// against (its aspect makes the fitted decode differ from the pane's long edge in
/// both directions).
fn square_position(listed: &[PathBuf]) -> usize {
    listed
        .iter()
        .position(|path| path.ends_with("square.png"))
        .expect("square.png is in the fixture folder")
}

/// The pane's size in device pixels.
fn pane_device(picker: &Picker) -> (u32, u32) {
    let pane = picker.preview_widget();
    let scale = pane.scale_factor() as u32;
    (
        pane.width().max(0) as u32 * scale,
        pane.height().max(0) as u32 * scale,
    )
}

/// The long edge `photo` is drawn at when `Contain`-fitted into the pane, in device
/// pixels — the expectation, computed from the widget's allocation and the photo's
/// own pixels rather than pasted.
fn fitted_long_edge(picker: &Picker, photo: &std::path::Path) -> u32 {
    let source = Source::decode(photo).expect("the fixture decodes");
    let (pane_width, pane_height) = pane_device(picker);
    let scale = f64::min(
        f64::from(pane_width) / f64::from(source.width()),
        f64::from(pane_height) / f64::from(source.height()),
    );
    (scale * f64::from(source.width().max(source.height()))).ceil() as u32
}

/// The long edge the pane's decode was asked for: the fitted edge rounded up to one
/// `PREVIEW_PX_STEP`, capped.
fn decoded_long_edge(picker: &Picker, photo: &std::path::Path) -> u32 {
    let wanted = fitted_long_edge(picker, photo).div_ceil(PREVIEW_PX_STEP) * PREVIEW_PX_STEP;
    wanted.clamp(PREVIEW_PX_STEP, PREVIEW_MAX_PX)
}

/// The count the Next button carries, read off the `AdwButtonContent` its child is.
///
/// The label has to live there: `GtkButton::set_label` *replaces* the button's child,
/// so S13b's `update_next` destroyed the icon the first time it ran (fixed in S13c,
/// and this is the check that keeps it fixed).
fn next_label(picker: &Picker) -> String {
    picker
        .next_button()
        .child()
        .and_downcast::<libadwaita::ButtonContent>()
        .expect("Next carries an AdwButtonContent, not a bare label")
        .label()
        .to_string()
}

/// Asserts the pane is showing `index`'s own preview, and returns its size.
///
/// The claim is made of three things at once, which is what makes it the
/// regression test for the reported blur: the pane's photo is the focused one, the
/// texture it paints is that photo's preview (so a 128 px tile cannot pass), and
/// the pane considers that preview current for the pane's own size.
fn pane_is_the_preview(picker: &Picker, index: usize) -> (i32, i32) {
    assert!(
        picker.preview_current(),
        "the pane is not showing photo {index}'s preview at its own size"
    );
    let (path, width, height, _) = picker
        .preview_pixels()
        .expect("the pane has pixels once it is current");
    assert_eq!(
        Some(path),
        picker.file(index),
        "the pane's pixels belong to the focused photo"
    );
    let texture = picker
        .preview_picture()
        .paintable()
        .and_then(|paintable| paintable.downcast::<gtk4::gdk::Texture>().ok())
        .expect("the pane paints a texture");
    assert_eq!(
        (texture.width(), texture.height()),
        (width, height),
        "the pane paints the preview's own texture, not a tile's"
    );
    // Holding a texture is not the same as drawing it: the pane's own snapshot has
    // to show something where the photo is. Measured 2026-09-23: the strip and the
    // picked list drew while the pane was still on the empty state, because the
    // `GtkStack`'s crossfade had not run — so this is the check that a preview on
    // screen is a *pixel* and not a property.
    let painted = support::snapshot(&picker.preview_widget());
    let backdrop = support::pixel(&painted, 1, 1);
    let covered = [(0.5, 0.5), (0.4, 0.5), (0.6, 0.5), (0.5, 0.4), (0.5, 0.6)]
        .iter()
        .any(|(x, y)| {
            support::pixel(
                &painted,
                (f64::from(painted.0) * x) as i32,
                (f64::from(painted.1) * y) as i32,
            ) != backdrop
        });
    assert!(
        covered,
        "the pane is holding photo {index}'s preview but drew nothing of it"
    );
    (width, height)
}

/// The file names of the picked list's rows, in the order the list shows them.
///
/// A row is an `AdwActionRow`, which *is* the `GtkListBoxRow` the list holds, so
/// the list's own children are the rows.
fn picked_rows(list: &gtk4::ListBox) -> Vec<String> {
    let mut titles = Vec::new();
    let mut row = list.first_child();
    while let Some(widget) = row {
        if let Some(action_row) = widget.downcast_ref::<libadwaita::ActionRow>() {
            titles.push(action_row.title().to_string());
        }
        row = widget.next_sibling();
    }
    titles
}

/// The file names of a list of paths, in the same order.
fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect()
}

/// How many controllers of a kind a widget carries.
fn controllers_of<T: glib::types::StaticType>(widget: &gtk4::Widget) -> usize {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .filter(|index| controllers.item(*index).is_some_and(|item| item.is::<T>()))
        .count()
}

/// How many cells are showing a picture (rather than a spinner).
fn painted_cells(picker: &Picker) -> usize {
    let mut painted = 0;
    for position in 0..picker.len() {
        let Some(cell) = picker.cell_widget(position) else {
            continue;
        };
        let stack = cell.downcast::<gtk4::Stack>().expect("a cell is a stack");
        if stack.visible_child_name().as_deref() == Some("photo") {
            painted += 1;
        }
    }
    painted
}

/// Presses one of the picked list's own re-order shortcuts.
///
/// The controller the list owns is read the way GTK reads it — its model of
/// `GtkShortcut`s — and the action each shortcut carries is activated, so what is
/// exercised is the trigger the user presses *and* the action it fires.
fn press(list: &gtk4::ListBox, key: &str) -> bool {
    let mut triggers = Vec::new();
    let controllers = list.observe_controllers();
    for index in 0..controllers.n_items() {
        let Some(controller) = controllers.item(index) else {
            continue;
        };
        let Ok(controller) = controller.downcast::<gtk4::ShortcutController>() else {
            continue;
        };
        for shortcut in 0..controller.n_items() {
            let Some(item) = controller.item(shortcut) else {
                continue;
            };
            let Ok(shortcut) = item.downcast::<gtk4::Shortcut>() else {
                continue;
            };
            let trigger = shortcut
                .trigger()
                .map(|trigger| trigger.to_str().to_string())
                .unwrap_or_default();
            if trigger.contains(key)
                && let Some(action) = shortcut.action()
            {
                return action.activate(gtk4::ShortcutActionFlags::EXCLUSIVE, list, None);
            }
            triggers.push(trigger);
        }
    }
    panic!("the picked list does not bind {key}: {triggers:?}");
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
