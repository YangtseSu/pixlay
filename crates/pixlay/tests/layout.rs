//! S14's exit criteria, as one test: the layout band and the count control.
//!
//! One `#[test]` because GTK lives on one thread (see `support`). What is checked
//! here, in the order the criteria are written in `docs/2026-09-22-STEPS.md`:
//!
//! * the strip lists every layout with the photo count, and only those;
//! * every candidate's thumbnail is `pixlay-render render` of the same document at
//!   the same pixel size (the RMSE threshold) — the gallery is not a second
//!   renderer;
//! * a layout change and a LIFO removal keep every surviving cell's photo and
//!   framing, and `+` adds one empty cell back — a layout edit, not a restore;
//! * the whole band costs one decode per photo, never one per candidate;
//! * the CLI's new flags land on the same document the window's own operations
//!   produce.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use gtk4::prelude::*;
use pixlay::canvas;
use pixlay::window::Stage;
use pixlay_core::{CropTransform, Project, templates};
use pixlay_imaging::Source;

/// The threshold from `AGENTS.md`: the same composition at `2N` and `N`,
/// downsampled, stays below 6.
const RMSE_THRESHOLD: f64 = 6.0;

#[test]
fn the_layout_band_offers_every_layout_with_the_photos_own_count() {
    support::start();
    let app = support::app();
    let window = support::window(&app);

    let project = support::verify_project();
    window
        .open_path(&project)
        .expect("the verification project opens");
    // The band is laid out *before* its candidates arrive — they come from a
    // background build — and it must not resize when they land: the canvas above it
    // would ask for another grid, which is one more decode of every photo. The
    // placeholder is a candidate cell with nothing in it for exactly this reason.
    window.pump(Duration::from_millis(30));
    let placeholder = (
        window.canvas_widget().height(),
        window
            .gallery()
            .map(|band| band.root().height())
            .unwrap_or(-1),
    );
    settle(&window);
    assert_eq!(window.stage(), Stage::Editor, "the band is the editor's");
    let gallery = window.gallery().expect("the editor has a layout band");
    // The band's height is the claim below, and a widget is measured on the frame after
    // the one that laid it out: wait for the allocation rather than assume it
    // (`support::allocated`; measured 2026-09-23: a band of 0x0 in one run of several,
    // which is the same class of failure as the `+` further down).
    assert!(
        support::allocated(&gallery.root().upcast::<gtk4::Widget>(), &window),
        "the band was never allocated"
    );
    let filled = (window.canvas_widget().height(), gallery.root().height());
    assert_eq!(
        placeholder, filled,
        "the canvas moved when the candidates landed: {placeholder:?} → {filled:?}"
    );
    assert!(filled.1 > 0, "the band has a height");

    // ---- the band is a band on the document's page ------------------------
    // Below the canvas, on the same page: not a third page, and not a panel beside
    // the sheet (S14's shape).
    let canvas = window.canvas_widget().upcast::<gtk4::Widget>();
    let band = gallery.root().upcast::<gtk4::Widget>();
    let page = gallery.root().parent().expect("the band is on a page");
    let corner = |widget: &gtk4::Widget, x: f32, y: f32| {
        widget
            .compute_point(&page, &gtk4::graphene::Point::new(x, y))
            .map(|point| (point.x(), point.y()))
    };
    let canvas_bottom = corner(&canvas, 0.0, canvas.height() as f32).map(|(_, y)| y);
    let band_top = corner(&band, 0.0, 0.0).map(|(_, y)| y);
    if let (Some(canvas_bottom), Some(band_top)) = (canvas_bottom, band_top) {
        assert!(
            band_top >= canvas_bottom,
            "the band ({band_top:.0}) is not below the canvas ({canvas_bottom:.0})"
        );
    }

    // ---- the candidate set -------------------------------------------------
    let photos = window.photo_count();
    assert_eq!(photos, 8, "the verification project holds eight photos");
    // What the band is worth in the window's own pixels: a candidate cell, and the
    // strip one cell tall whatever the layout's aspect (all three are fitted into
    // the same box).
    assert_eq!(
        gallery.cell("mosaic-8-s14").map(|cell| cell.height()),
        gallery.cell("strip-8-8x1").map(|cell| cell.height()),
        "a 4:3 candidate and a 16:9 one are the same cell"
    );

    let expected: Vec<String> = window
        .candidate_templates()
        .into_iter()
        .map(|template| template.name)
        .collect();
    assert!(expected.len() >= 3, "{photos} photos have {expected:?}");
    assert_eq!(
        gallery.candidates(),
        expected,
        "the strip lists the library's own query, in library order"
    );
    for name in &expected {
        let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
        assert_eq!(
            template.slots.len(),
            photos,
            "{name} has a slot count the photo count does not"
        );
    }
    assert_eq!(
        gallery.selected().as_deref(),
        Some(window.current_template().as_str()),
        "the document's own layout is the highlighted one"
    );
    // The count control reads the picker's numbers: the label, and the two bounds.
    assert_eq!(gallery.count_label().label(), "8");
    assert!(
        gallery.minus_button().is_sensitive(),
        "8 photos can drop one"
    );
    assert!(
        gallery.plus_button().is_sensitive(),
        "8 photos can take one"
    );

    // ---- every candidate is the CLI's own render ---------------------------
    let mut worst = 0.0f64;
    for name in &expected {
        let (width, height, pixels) = gallery
            .thumbnail(name)
            .unwrap_or_else(|| panic!("{name} has no thumbnail"));
        let cli = cli_render(&window, name);
        assert_eq!(
            (width, height),
            (cli.0, cli.1),
            "{name}: the strip and the CLI must render the same pixel grid"
        );
        let difference = support::rmse(&(width, height, pixels), &cli);
        worst = worst.max(difference);
        eprintln!("{name}: gallery vs CLI RMSE {difference:.4}");
        assert!(
            difference <= RMSE_THRESHOLD,
            "{name} diverged from the CLI: RMSE {difference:.4} > {RMSE_THRESHOLD}"
        );
    }
    eprintln!(
        "worst candidate RMSE {worst:.4} over {} candidates",
        expected.len()
    );

    // ---- the band costs no decode of its own, never one per candidate -------
    // `decoded_sources` counts the decoding *thread*'s work, so what is measured
    // here is the difference each event makes to it — and `gallery_decodes` is the
    // band's own share of that work, which is the number S14's criterion is about:
    // the band names the canvas's preview-grade edge, so its copies are the canvas's
    // copies and each additional candidate only resamples them. The window asks for
    // its own grid while the editor's page is pushed, and once more when the widget
    // settles on open and again on a resize — that is the canvas's own layout, not
    // the band's. S14's finding on the way: the request made before the canvas was
    // allocated at all was a 1x1 grid (`refresh_document`).
    let decoded = window.decoded_sources();
    assert!(decoded > 0, "opening a project decodes its photos");
    eprintln!("decodes on open: {decoded} for {photos} photos (the canvas's own layout)");

    let band = window.gallery_decodes();
    assert_eq!(
        band, 0,
        "the band's own build decoded {band} files: it names the canvas's preview-grade edge"
    );

    // A committed framing change: the thumbnails show the new framing too, and
    // nothing is decoded at all — neither by the canvas (one cell's bitmap is
    // rebuilt from the cached copy, S12b) nor by the band.
    window.select(Some(0));
    window.set_zoom(1.6);
    settle(&window);
    assert_eq!(
        window.decoded_sources(),
        decoded,
        "a committed framing change decoded nothing"
    );

    // A layout change: the sheet's shape changes with the layout, so the canvas
    // re-cuts its own preview-grade copies — one decode per photo, the canvas's own
    // — and the band adds nothing to it.
    window.select_layout("grid-8-4x2");
    settle(&window);
    assert_eq!(
        window.current_template(),
        "grid-8-4x2",
        "the clicked candidate became the document's layout"
    );
    assert_eq!(
        gallery.selected().as_deref(),
        Some("grid-8-4x2"),
        "and the highlight follows the document"
    );
    let after_layout = window.decoded_sources() - decoded;
    assert!(
        after_layout <= 2 * photos as u64,
        "a layout change cost {after_layout} decodes for {photos} photos: the canvas's own, never doubled by the band"
    );
    assert_eq!(
        window.gallery_decodes() - band,
        0,
        "the band decoded nothing of its own through a layout change"
    );

    // A resize moves the canvas's grid, so the preview-grade copies at the new
    // edge are the canvas's own downloads — and the band, whose request goes out in
    // the same batch at that same edge, adds none of its own.
    let area = window.canvas_widget();
    area.set_hexpand(false);
    area.set_vexpand(false);
    area.set_size_request(420, 315);
    window.pump(Duration::from_millis(400));
    settle(&window);
    let after_resize = window.decoded_sources();
    assert!(
        after_resize - decoded <= 2 * photos as u64,
        "a resize cost {} decodes for {photos} photos",
        after_resize - decoded
    );
    assert_eq!(
        window.gallery_decodes() - band,
        0,
        "the band decoded nothing of its own through a resize"
    );
    eprintln!(
        "decodes: {decoded} on open, {} layout + resize (canvas only), {} band, {photos} photos",
        after_resize - decoded,
        window.gallery_decodes() - band,
    );

    // ---- a layout change keeps the surviving cells -------------------------
    let before = window.document();
    assert_eq!(
        before.template.name, "grid-8-4x2",
        "the layout the test clicked last"
    );
    window.select_layout("mosaic-8-s14");
    settle(&window);
    let after = window.document();
    assert_eq!(after.template.name, "mosaic-8-s14");
    assert_eq!(after.cells.len(), 8, "one cell per slot");
    for slot in 0..8 {
        assert_eq!(
            after.cells[slot], before.cells[slot],
            "cell {slot} kept its photo and framing across the layout change"
        );
    }

    // ---- the count control moves the layout one cell at a time -------------
    // S14b (ruled 2026-09-23): `+`/`−` address the *layout*, and neither remembers a
    // photo. Framing on the last cell first, so "the survivors are untouched" means
    // the contents and not only a path.
    window
        .apply(pixlay_core::Command::SetCrop {
            slot: 7,
            crop: CropTransform {
                zoom: 2.2,
                offset: (-0.2, 0.3),
                rotation_deg: -14.0,
            },
        })
        .expect("the last cell takes framing");
    settle(&window);
    let framed = window.document();

    window.remove_photo();
    settle(&window);
    let removed = window.document();
    assert_eq!(
        removed.cells.len(),
        7,
        "the layout gave up one cell of its own"
    );
    assert_eq!(
        removed.template.name, "mosaic-7-t4b3",
        "4:3 stays 4:3 through the count rule: `layout_for`'s first preference"
    );
    assert_eq!(
        removed.cells[..7],
        framed.cells[..7],
        "every surviving cell kept its photo and framing"
    );
    // The strip follows the layout, so the candidates are the seven-cell layouts.
    assert_eq!(
        gallery.candidates(),
        layouts_of(&removed),
        "the strip lists the layouts with the *new* cell count"
    );
    assert_eq!(gallery.count_label().label(), "7");

    // `+` gives the layout one cell back, and the cell is **empty**: it is a layout
    // edit, not a restore of the photo that left.
    window.add_photo();
    settle(&window);
    assert_eq!(window.document().cells.len(), 8, "eight cells again");
    assert_eq!(
        window.document().template.name,
        "mosaic-8-s14",
        "and the same layout the document was on"
    );
    assert!(
        window.document().cells[7].source.is_none(),
        "the new cell has no photo: {:?}",
        window.document().cells[7].source
    );
    assert_eq!(
        window.photo_count(),
        7,
        "so the photo count is one below the cell count"
    );
    // The empty cell is what asks for a photo, and it is a real control over the
    // canvas (`CellControls`), reachable by the Tab order like any other button.
    let empty = window
        .cell_controls()
        .expect("the canvas has a cell-control layer");
    // The buttons follow the canvas's own allocation, which the window writes on
    // every refresh; this call makes the geometry the test measures explicit.
    let (width, height) = support::canvas_size(&window);
    empty.sync_in(&window, width, height);
    let add = empty.button(7).expect("the empty cell has a `+` button");
    assert!(add.is_visible(), "and the `+` is on screen");
    assert!(add.is_focusable(), "and reachable with the keyboard");
    // **The button is really allocated and really over its own cell**, which is the
    // number behind "the empty cell shows a `+`": the widget's own corners, in the
    // canvas's coordinates, against the slot's own rectangle from the placement.
    //
    // A widget it has just been told to show is allocated on the *next* frame, and a
    // display's frame clock is not this test's to schedule (`support::snapshot` waits
    // for the same reason), so the allocation is waited for rather than assumed —
    // measured 2026-09-23: under `xvfb-run` the button reads 0x0 without this wait,
    // where a session display happened to have drawn the frame already.
    assert!(
        support::allocated(&add.clone().upcast::<gtk4::Widget>(), &window),
        "the `+` was never allocated ({}x{})",
        add.width(),
        add.height()
    );
    let canvas_widget = window.canvas_widget().upcast::<gtk4::Widget>();
    let origin = add
        .compute_point(&canvas_widget, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("the `+` is in the canvas's own space");
    let (grid, _) = window.images();
    let placement = canvas::placement(grid, canvas_widget.width(), canvas_widget.height());
    let slot_box = window
        .document()
        .template
        .slots
        .get(7)
        .expect("the layout has eight slots")
        .outline
        .bbox();
    let (x0, y0) = placement.to_widget(pixlay_core::Point::new(slot_box.x0, slot_box.y0));
    let (x1, y1) = placement.to_widget(pixlay_core::Point::new(slot_box.x1, slot_box.y1));
    let centre_x = f64::from(origin.x()) + f64::from(add.width()) / 2.0;
    let centre_y = f64::from(origin.y()) + f64::from(add.height()) / 2.0;
    assert!(
        centre_x > x0 && centre_x < x1 && centre_y > y0 && centre_y < y1,
        "the `+` ({centre_x:.0},{centre_y:.0}) is not inside its own cell \
         ({x0:.0},{y0:.0})-({x1:.0},{y1:.0})"
    );
    eprintln!(
        "the empty cell's `+`: {}x{} at {:.0},{:.0} in the canvas; its cell is \
         {:.0},{:.0}-{:.0},{:.0}",
        add.width(),
        add.height(),
        origin.x(),
        origin.y(),
        x0,
        y0,
        x1,
        y1
    );
    // And a cell that holds a photo has no `+` over it: an occupied cell is dragged
    // and clicked to reframe, and a button on top of it would take that press.
    assert!(
        !empty.button(0).expect("a first button").is_visible(),
        "cell 0 holds a photo and must have no `+`"
    );

    // Clicking it asks for a photo rather than placing one, so what this checks is
    // the wiring: the button exists, is named, and belongs to the empty cell. The
    // chooser itself is a `GtkFileDialog` a test cannot answer.
    // `+` on a full document is the ceiling and reports it once.
    let full = nine_photo_document();
    window.open_document(full);
    settle(&window);
    assert_eq!(window.document().cells.len(), 9);
    assert!(!gallery.plus_button().is_sensitive(), "nine is the ceiling");

    // ---- two photos are swappable in the layout stage ----------------------
    // S14b: a whole cell moves, photo and framing together.
    window
        .open_path(&project)
        .expect("the verification project opens");
    settle(&window);
    window
        .apply(pixlay_core::Command::SetCrop {
            slot: 0,
            crop: CropTransform {
                zoom: 2.5,
                offset: (0.1, -0.2),
                rotation_deg: 21.0,
            },
        })
        .expect("cell 0 takes framing");
    settle(&window);
    let before = window.document();
    window.swap_slots(0, 3);
    settle(&window);
    let after = window.document();
    assert_eq!(after.cells[0], before.cells[3], "the cells exchanged whole");
    assert_eq!(after.cells[3], before.cells[0]);
    assert_eq!(
        after.template.name, before.template.name,
        "and the layout did not move"
    );
    assert_eq!(window.photo_count(), 8, "a swap changes no count");
    // The keyboard's own path: the neighbour is geometric, so the swap lands on
    // whatever cell is to the right of 0 — which the rule decides, not this test.
    let neighbour = after
        .template
        .neighbour(0, (1, 0))
        .expect("cell 0 has a cell to its right");
    let now = window.document();
    window.swap_towards(0, (1, 0));
    settle(&window);
    assert_eq!(
        window.document().cells[0],
        now.cells[neighbour],
        "the arrow key exchanged cell 0 with the cell the rule calls right of it"
    );
    assert_eq!(window.document().cells[neighbour], now.cells[0]);
    window.swap_towards(0, (1, 0));
    settle(&window);
    assert_eq!(
        window.document().cells,
        now.cells,
        "and the same key again puts them back"
    );

    // ---- the `+` path appends in the order its files arrive ----------------
    // Two photos, so the order is a claim and not a coincidence. The layout is
    // dropped to six cells first, which is where the two have room.
    window.remove_photo();
    window.remove_photo();
    settle(&window);
    assert_eq!(window.photo_count(), 6);
    assert_eq!(window.document().cells.len(), 6);
    let first = support::photo("landscape.jpg");
    let second = support::photo("portrait.jpg");
    window.add_photos(vec![first.clone(), second.clone()]);
    settle(&window);
    let doc = window.document();
    assert_eq!(window.photo_count(), 8, "both photos landed");
    assert_eq!(
        doc.cells[6].source.as_deref().map(same_file),
        Some(same_file(&first)),
        "the first argument is the seventh cell"
    );
    assert_eq!(
        doc.cells[7].source.as_deref().map(same_file),
        Some(same_file(&second)),
        "the second argument is the eighth"
    );

    // ---- the count control's two bounds ------------------------------------
    // The floor: two cells is the smallest layout the library has, so `−` is
    // insensitive and the refusal names the number the picker names.
    let two = two_photo_document();
    window.open_document(two.clone());
    settle(&window);
    assert_eq!(gallery.count_label().label(), "2");
    assert!(
        !gallery.minus_button().is_sensitive(),
        "two cells is the floor"
    );
    assert!(gallery.plus_button().is_sensitive(), "and 2 < 9");
    assert_eq!(
        gallery.candidates(),
        layouts_of(&two),
        "the band lists the two-cell layouts"
    );
    for name in gallery.candidates() {
        assert_eq!(
            templates::get(&name)
                .expect("a shipped template")
                .slots
                .len(),
            2,
            "{name} is not a two-cell layout"
        );
    }
    window.remove_photo();
    assert!(
        window
            .last_toast()
            .is_some_and(|message| message.contains('2')),
        "a removal below the floor reports the floor: {:?}",
        window.last_toast()
    );
    assert_eq!(window.photo_count(), 2, "and changes nothing");
    assert_eq!(window.document().cells.len(), 2);

    // The ceiling: nine, which is also the format's slot limit (S12c).
    let nine = nine_photo_document();
    window.open_document(nine);
    settle(&window);
    assert_eq!(gallery.count_label().label(), "9");
    assert!(!gallery.plus_button().is_sensitive(), "nine is the ceiling");
    assert!(gallery.minus_button().is_sensitive());
    let toasts = window.toasts();
    window.add_photo();
    assert!(
        window
            .last_toast()
            .is_some_and(|message| message.contains('9')),
        "an addition past the ceiling reports the ceiling: {:?}",
        window.last_toast()
    );
    assert_eq!(window.toasts(), toasts + 1, "reported once");
    assert_eq!(window.photo_count(), 9, "and changes nothing");

    // ---- the CLI carries the same capabilities -----------------------------
    // The three operations, on a document both sides start from: the file the
    // window opened, with nothing else done to it.
    window
        .open_path(&project)
        .expect("the verification project opens again");
    settle(&window);
    let dir = support::out_dir();
    let from_gui = dir.join("layout-gui.pixlay");
    window.select_layout("grid-8-4x2");
    let after_layout = window.document().template.name.clone();
    window.remove_photo();
    settle(&window);
    let after_removal = window.document().template.name.clone();
    window.add_photos(vec![support::photo("square.png")]);
    settle(&window);
    let after_addition = window.document().template.name.clone();
    window
        .save_to(&from_gui)
        .expect("the window saves its document");
    eprintln!("the window's own operations: {after_layout} → {after_removal} → {after_addition}");

    // The same three operations through `edit`, from the same project.
    let from_cli = dir.join("layout-cli.pixlay");
    let status = pixlay_cli::cli::run(&argv(&[
        "edit",
        "--project",
        path(&project),
        "--template",
        "grid-8-4x2",
        "--remove-cell",
        "--out",
        path(&from_cli),
    ]))
    .expect("the CLI edits the project");
    assert_eq!(status, 0, "the layout change and the removal are one edit");
    let status = pixlay_cli::cli::run(&argv(&[
        "edit",
        "--project",
        path(&from_cli),
        "--add-photo",
        path(&support::photo("square.png")),
        "--out",
        path(&from_cli),
    ]))
    .expect("the CLI appends a photo");
    assert_eq!(status, 0, "the append is one edit");

    let gui = Project::load(&from_gui).expect("the window's project loads");
    let cli = Project::load(&from_cli).expect("the CLI's project loads");
    assert!(
        same_document(&gui, &cli),
        "the window and the CLI produced different documents:\n{gui:?}\n{cli:?}"
    );
    eprintln!(
        "the CLI round trip equals the window's own operations ({} cells, {})",
        cli.doc().cells.len(),
        cli.doc().template.name
    );
}

/// Waits for both background builds: the canvas's bitmaps and the band's
/// candidates are two jobs on one thread.
fn settle(window: &pixlay::EditorWindow) {
    assert!(
        window.wait_for_idle(support::WAIT),
        "the canvas's decode finished"
    );
    assert!(window.wait_for_gallery(support::WAIT), "the band was built");
}

/// `pixlay-render render` of one candidate, at the grid the strip drew it at.
fn cli_render(window: &pixlay::EditorWindow, name: &str) -> support::Image {
    let template = templates::get(name).unwrap_or_else(|| panic!("template {name}"));
    let mut candidate = window.document();
    candidate.template = template;
    candidate
        .cells
        .resize(candidate.template.slots.len(), Default::default());
    let grid = pixlay::layout::thumb_grid(candidate.template.aspect);
    let project = support::artifact(&format!("layout-candidate-{name}.pixlay"));
    // Anchored at the project the window opened, so the candidate's relative photo
    // paths are rebased onto the artifact directory — exactly as `save as…` does.
    let anchor = window
        .project_path()
        .unwrap_or_else(support::verify_project);
    Project::new(candidate, &anchor)
        .expect("the candidate is a valid document")
        .save_as(&project)
        .expect("the candidate project is written");
    let out = support::artifact(&format!("layout-candidate-{name}.png"));
    let status = pixlay_cli::cli::run(&argv(&[
        "render",
        "--project",
        path(&project),
        "--long-edge",
        &grid.width.max(grid.height).to_string(),
        "--out",
        path(&out),
    ]))
    .expect("the CLI renders the candidate");
    assert_eq!(status, 0, "{name}: the CLI reported success");
    read_image(&out)
}

/// A file decoded by the image pipeline, as `(width, height, rgb)`.
fn read_image(path: &Path) -> support::Image {
    let source = Source::decode(path).expect("the render decodes");
    let (width, height) = (source.width() as i32, source.height() as i32);
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height as u32 {
        for x in 0..width as u32 {
            let pixel = source.pixel(x, y);
            data.push((pixel[0] >> 8) as u8);
            data.push((pixel[1] >> 8) as u8);
            data.push((pixel[2] >> 8) as u8);
        }
    }
    (width, height, data)
}

/// The names of the layouts a document's photo count has.
/// The strip's own query for a document: every layout with the document's **cell**
/// count, in library order (S14b — the strip follows the layout, not the photo
/// count, so `+` can leave a cell empty without the strip moving).
fn layouts_of(doc: &pixlay_core::CollageDoc) -> Vec<String> {
    templates::with_slots(doc.cells.len())
        .into_iter()
        .map(|template| template.name)
        .collect()
}

/// A document with a photo in every cell of `template`.
fn document_on(template: &str) -> pixlay_core::CollageDoc {
    let template = templates::get(template).unwrap_or_else(|| panic!("template {template}"));
    let slots = template.slots.len();
    let mut doc = pixlay_core::CollageDoc::new(template);
    for (slot, cell) in doc.cells.iter_mut().enumerate() {
        cell.source = Some(support::photo(if slot % 2 == 0 {
            "landscape.jpg"
        } else {
            "portrait.jpg"
        }));
    }
    assert_eq!(doc.cells.len(), slots);
    doc
}

/// The smallest collage the product makes: two photos, two slots.
fn two_photo_document() -> pixlay_core::CollageDoc {
    document_on("strip-2-2x1")
}

/// The largest: nine photos, nine slots.
fn nine_photo_document() -> pixlay_core::CollageDoc {
    document_on("strip-9-9x1")
}

/// Two projects compared as what they mean.
///
/// The template, the frame, each cell's framing, and each cell's **resolved**
/// photo: a written `source` may be relative or absolute — the window stores the
/// path the file chooser gave it, the CLI rebases against the project — and the
/// claim is about the document, not about the spelling.
fn same_document(left: &Project, right: &Project) -> bool {
    let (a, b) = (left.doc(), right.doc());
    let (Ok(left_sources), Ok(right_sources)) = (left.sources(), right.sources()) else {
        return false;
    };
    a.template == b.template
        && a.frame == b.frame
        && a.cells.len() == b.cells.len()
        && a.cells
            .iter()
            .zip(&b.cells)
            .all(|(left, right)| left.crop == right.crop)
        && left_sources
            .iter()
            .zip(&right_sources)
            .all(|(left, right)| match (left, right) {
                (None, None) => true,
                (Some(left), Some(right)) => same_file(left) == same_file(right),
                _ => false,
            })
}

/// `../../../…` and the path it points at are the same photo.
fn same_file(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn argv(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(std::ffi::OsString::from).collect()
}

fn path(value: &Path) -> &str {
    value.to_str().expect("a UTF-8 path")
}
