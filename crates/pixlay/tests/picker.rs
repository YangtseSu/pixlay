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
//! * **The pane is decoded at its own size, and it never paints a tile.**
//!   Regression test for the reported blur: the pane's photo is at least
//!   min(pane device long edge, `PREVIEW_MAX_PX`) long, the picture it paints is
//!   that photo's own texture, and coming back to a photo — after focusing another,
//!   or after changing folder — does not leave the cell's 256 px tile in the pane.
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

use pixlay::picker::{PREVIEW_MAX_PX, Picker, TILE_REQUEST_MAX, TILE_SIZE};
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
        picked_rows(&picker.picked_list()).len() > 1,
        "the picked list holds more than one photo after selecting all"
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
    window.pump(Duration::from_millis(200));
    let before = support::snapshot(&picker.grid());
    picker.clear_selection();
    window.pump(Duration::from_millis(200));
    let after = support::snapshot(&picker.grid());
    let difference = support::rmse(&before, &after);
    eprintln!("the highlight is worth RMSE {difference:.3} of the grid's pixels");
    assert!(
        difference > 1.0,
        "picking and unpicking a cell did not change the grid's pixels ({difference:.3}): \
         the highlight is not drawing"
    );
    assert!(
        !picker
            .cell_widget(0)
            .is_some_and(|cell| cell.has_css_class("picked")),
        "clearing the pick drops the highlight"
    );

    // ---- the pane is the pane's size, and never a tile ---------------------
    // The reported blur, as a number: the pane is decoded at its own device long
    // edge (rounded up, capped), and what it paints is that photo's own texture.
    let pane = picker.preview_widget();
    let pane_edge = pane.width().max(pane.height()).max(0) as u32 * pane.scale_factor() as u32;
    let wanted = pane_edge.min(PREVIEW_MAX_PX);
    picker.toggle(&window, 2);
    assert!(
        window.wait_for_preview(support::WAIT),
        "the pane never decoded the focused photo"
    );
    let (width, height) = pane_is_the_preview(&picker, 2);
    assert!(
        picker.preview_px() >= wanted,
        "the pane is {}x{} device pixels and the preview was decoded at {}",
        pane.width(),
        pane.height(),
        picker.preview_px()
    );
    assert!(
        width.max(height) as u32 > TILE_SIZE as u32,
        "the pane is showing a {width}x{height} picture, which is a cell's tile"
    );
    eprintln!(
        "the pane decoded at {} px for a {}x{} pane ({} device pixels)",
        picker.preview_px(),
        pane.width(),
        pane.height(),
        pane_edge
    );

    // Focus another photo and come back: S13 left the *tile* in the pane when the
    // preview of a photo it had already shown was refused as a repeat.
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

    // ---- the dividers are the session's ------------------------------------
    // Both positions are what the user chose, so a second window opens where the
    // first one was left (`SPLITS`), not at the defaults.
    picker.set_split_position(500, 300);
    window.pump(Duration::from_millis(100));
    assert_eq!(
        picker.split_position(),
        (500, 300),
        "the dividers report the position they were given"
    );
    let other = support::second_window(&app);
    let other_picker = other.picker().expect("the second window has a picker");
    assert_eq!(
        other_picker.split_position(),
        (500, 300),
        "a new window opens on the dividers the session left behind"
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
    // Back on the fixture folder with a pick of three, so the picture a human
    // looks at is a real state of the stage rather than the last thing a check
    // left behind.
    picker.open_folder(&window, &folder);
    for position in scrambled {
        picker.toggle(&window, position as u32);
    }
    picker.focus(&window, 1);
    let _ = window.wait_for_preview(support::WAIT);
    let _ = window.wait_for_tiles(support::WAIT);
    window.pump(Duration::from_millis(300));
    let picture = support::artifact("picker.png");
    support::save_png(&picture, &support::snapshot(&window));
    eprintln!("the picker stage is {picture:?}");
}

/// Asserts the pane is showing `index`'s own preview, and returns its size.
///
/// The claim is made of three things at once, which is what makes it the
/// regression test for the reported blur: the pane's photo is the focused one, the
/// texture it paints is that photo's preview (so a 256 px tile cannot pass), and
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
