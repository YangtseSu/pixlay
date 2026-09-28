// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the window draws is what the CLI writes.
//!
//! The step's criterion: "what the window renders is pixel-identical to
//! `pixlay-cli render` at the same canvas size (within the RMSE threshold)". It is
//! the strongest statement about this layer — preview and export are one renderer,
//! so anything that diverges here is a second renderer that grew where nobody was
//! looking.
//!
//! Two independent paths meet in this test: the window's own widget, snapshotted
//! through a real render node, and the CLI's file, written by `pixlay-render` and
//! read back by the decoder. Neither is the other's own code, and the document
//! they are given exercises decoded photos, EXIF rotation, a transparent PNG
//! flattened onto white and a framing rotation.

mod support;

use std::time::Duration;

use gtk4::prelude::*;
use pixlay::canvas;
use pixlay_core::{CANVAS_MARGIN, CropTransform, PixelSize, templates};

/// The long edge the test renders at, in pixels. Small enough to stay fast, large
/// enough that a one-pixel geometry error is visible.
const LONG_EDGE: u32 = 640;

/// The threshold from `AGENTS.md`: the same composition at `2N` and `N`,
/// downsampled, stays below 6.
const RMSE_THRESHOLD: f64 = 6.0;

#[test]
fn the_canvas_draws_what_the_cli_writes() {
    support::start();
    let app = support::app();
    let window = support::window(&app);

    // A document of this test's own, on a canvas whose aspect is exactly 4:3, so
    // that the widget's fitted grid and the CLI's grid are the same numbers and
    // the comparison is between two renders rather than between two roundings.
    let project = support::artifact("canvas.pixlay");
    let template = templates::get("mosaic-5-hero").expect("the template is in the library");
    let mut doc = pixlay_core::CollageDoc::new(template);
    for (slot, name) in [
        (0usize, "landscape.jpg"),
        (1, "portrait.jpg"),
        (2, "square.png"),
        (3, "dated.jpg"),
        (4, "alpha.png"),
    ] {
        doc.cells[slot].source = Some(support::photo(name));
    }
    // A rotated slot: content the two paths have to agree about, and code the
    // plain case does not reach.
    doc.cells[2].crop = CropTransform {
        zoom: 1.4,
        offset: (0.15, -0.1),
        rotation_deg: 12.0,
    };
    doc.save(&project).expect("the test project is written");

    window
        .open_path(&project)
        .expect("the window opens the test project");
    let grid = PixelSize::for_long_edge(window.document().template.aspect, LONG_EDGE)
        .expect("a grid inside the budget");

    // Pin the canvas widget to exactly the grid's size plus its margin: the widget
    // is what asks for bitmaps, and asking for this grid is what makes the two
    // renders comparable.
    let area = window.canvas_widget();
    area.set_hexpand(false);
    area.set_vexpand(false);
    area.set_size_request(
        grid.width + 2 * CANVAS_MARGIN as i32,
        grid.height + 2 * CANVAS_MARGIN as i32,
    );
    window.pump(Duration::from_millis(300));
    assert!(
        window.wait_for_idle(support::WAIT),
        "the background decode finished"
    );
    assert_eq!(
        window.images().0,
        grid,
        "the canvas decoded the grid the CLI will render"
    );

    let painted = support::snapshot(&area);
    let placement = canvas::placement(grid, area.width(), area.height());
    assert!(
        (placement.scale - 1.0).abs() < f64::EPSILON,
        "the widget draws the sheet 1:1 (scale {})",
        placement.scale
    );
    let sheet = crop_sheet(&painted, &placement);

    // The CLI's render of the same document, at the same size.
    let out = support::artifact("cli-render.png");
    let argv: Vec<std::ffi::OsString> = [
        "render",
        "--project",
        project.to_str().expect("a UTF-8 path"),
        "--long-edge",
        &LONG_EDGE.to_string(),
        "--out",
        out.to_str().expect("a UTF-8 path"),
    ]
    .iter()
    .map(std::ffi::OsString::from)
    .collect();
    let status = pixlay_cli::cli::run(&argv).expect("the CLI renders the project");
    assert_eq!(status, 0, "the CLI reported success");

    let cli = read_png(&out);
    assert_eq!(
        (sheet.0, sheet.1),
        (cli.0, cli.1),
        "the window and the CLI must render the same pixel grid"
    );
    let difference = support::rmse(&sheet, &cli);
    eprintln!(
        "window vs CLI: RMSE {difference:.4} over {} pixels",
        sheet.0 * sheet.1
    );
    assert!(
        difference <= RMSE_THRESHOLD,
        "the window and the CLI diverged: RMSE {difference:.4} > {RMSE_THRESHOLD}"
    );

    // The document survived the round trip through the file, which is also what
    // says the window was rendering the document the test wrote.
    let reloaded = pixlay_core::Project::load(&project).expect("the written project loads");
    assert_eq!(reloaded.doc(), &window.document());
}

/// The sheet's own pixels out of a snapshot of the whole widget.
fn crop_sheet(painted: &support::Image, placement: &canvas::Placement) -> support::Image {
    let (width, height, data) = painted;
    let x0 = placement.origin.0.round() as i32;
    let y0 = placement.origin.1.round() as i32;
    let w = placement.width().round() as i32;
    let h = placement.height().round() as i32;
    let mut cropped = Vec::with_capacity((w * h * 3) as usize);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            assert!(
                x >= 0 && y >= 0 && x < *width && y < *height,
                "the sheet has to lie inside the widget"
            );
            let index = (y as usize * *width as usize + x as usize) * 3;
            cropped.extend_from_slice(&data[index..index + 3]);
        }
    }
    (w, h, cropped)
}

/// A written file, decoded by the image pipeline rather than by anything of ours.
fn read_png(path: &std::path::Path) -> support::Image {
    let source = pixlay_imaging::Source::decode(path).expect("the export decodes");
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
