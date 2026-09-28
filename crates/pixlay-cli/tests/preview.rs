// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S12b's drift criterion: the preview-grade source is still the export's renderer.
//!
//! The reduction gave the preview a *second* source of pixels, and a second source
//! is exactly where a second renderer would grow if nobody looked. This test looks,
//! and it is the comparison the criterion names: the same document rendered at the
//! same size by the preview's path (`Preview::build` — the reduction — and
//! `pixlay_render::draw`) and by the export's (`pixlay-render render --preview-px`,
//! whose bitmaps are built from the photo's own pixels), as a measured RMSE against
//! S7's threshold of 6.
//!
//! What makes the measurement meaningful is the photo: `resample-source.png` is
//! 1600x1200 of smooth multi-frequency content with a hard-edged square, four times
//! the grid this test draws at, so the reduction is a genuine 4x area loss rather
//! than a copy. The unrotated cell and the framed one are both in the document,
//! because a rotation is what makes the region the reduction has to satisfy wider
//! than the cell.
//!
//! The comparison runs the built binary rather than `cli::run`, so the reference
//! really is what the command writes — including the encoder and the file.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pixlay_core::{CollageDoc, CropTransform, PixelSize, templates};
use pixlay_imaging::{Preview, SlotBitmap, preview_source_long_edge};
use pixlay_render::{Bitmap, Images, output_px, render_rgb8};

/// `CARGO_BIN_EXE_<name>` is set by Cargo for integration tests.
const BIN: &str = env!("CARGO_BIN_EXE_pixlay-render");

/// The threshold from `AGENTS.md`: the same composition at `2N` and `N`,
/// downsampled, stays below 6.
const RMSE_THRESHOLD: f64 = 6.0;

/// The reference run asks for no grid: `--preview-px` alone renders the default
/// export grid (the 4000 px long edge `render` falls back to) scaled down, which
/// is the CLI's own preview path and therefore the only fair reference.
/// The long edge both renders come out at, in pixels.
const PREVIEW_PX: u32 = 400;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/photos")
        .join(name)
}

/// Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
fn out_dir(name: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-cli-tests/{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test directory");
    dir
}

/// Runs the built binary the way a caller does: a closed stdin, no TTY.
fn run(args: &[&str]) -> std::process::Output {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", "/nonexistent");
    command.output().expect("run pixlay-render")
}

/// One `key = value` field from the default output shape.
fn field(output: &std::process::Output, key: &str) -> String {
    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
    stdout
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            (name == key).then(|| value.to_string())
        })
        .unwrap_or_else(|| panic!("field {key} missing from:\n{stdout}"))
}

/// A written file, decoded by the `image` crate rather than by anything of ours.
fn read_png(path: &Path) -> (i32, i32, Vec<u8>) {
    let image = image::open(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let rgb = image.to_rgb8();
    (rgb.width() as i32, rgb.height() as i32, rgb.into_raw())
}

/// Wraps one decoded slot for the renderer, keeping the region it holds.
fn bitmap(bitmap: &SlotBitmap) -> Bitmap {
    Bitmap::from_argb32_region(
        bitmap.width as i32,
        bitmap.height as i32,
        bitmap.origin,
        bitmap.display,
        bitmap.pixels.clone(),
    )
    .expect("bitmap")
}

fn rmse(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "the two renders must be the same grid");
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(left, right)| {
            let difference = f64::from(*left) - f64::from(*right);
            difference * difference
        })
        .sum();
    (sum / a.len() as f64).sqrt()
}

#[test]
fn the_preview_grade_source_stays_the_exports_own_renderer() {
    let dir = out_dir("preview-drift");
    let template = templates::all()
        .into_iter()
        .find(|template| template.slots.len() == 4)
        .expect("the library has a four-slot layout");
    let mut doc = CollageDoc::new(template);
    for (slot, name) in [
        (0usize, "resample-source.png"),
        (1, "resample-source.png"),
        (2, "resample-source.png"),
        (3, "landscape.jpg"),
    ] {
        doc.cells[slot].source = Some(fixture(name));
    }
    // A framed cell: the region the reduction has to satisfy at an angle.
    doc.cells[1].crop = CropTransform {
        zoom: 1.6,
        offset: (0.12, -0.08),
        rotation_deg: 23.5,
    };
    let project = dir.join("drift.pixlay");
    doc.save(&project).expect("the test project is written");

    // ---- the reference: the export's path, at the preview's size ----------
    let out = dir.join("export.png");
    let reference = run(&[
        "render",
        "--project",
        project.to_str().expect("a UTF-8 path"),
        "--preview-px",
        &PREVIEW_PX.to_string(),
        "--out",
        out.to_str().expect("a UTF-8 path"),
    ]);
    assert_eq!(
        reference.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let (width, height, pixels) = read_png(&out);
    // The preview's own draw is at the size the CLI reported for this run, which is
    // the only way the two are comparable pixel for pixel.
    assert_eq!(field(&reference, "out_w"), width.to_string());
    assert_eq!(field(&reference, "out_h"), height.to_string());
    // `long_edge` is the edge the file was actually written at — not the export
    // base the grid was scaled from — so it is the size read back from the PNG
    // (S15h, PIX-019).
    assert_eq!(
        field(&reference, "long_edge"),
        width.max(height).to_string()
    );

    // ---- the preview's path: the reduction, then the same `draw` ----------
    // The CLI's own arithmetic, repeated so that the *only* difference between the
    // two renders is where the bitmaps came from.
    let canvas_px =
        PixelSize::for_long_edge(doc.template.aspect, 4000).expect("the export grid is valid");
    let scale = f64::from(PREVIEW_PX) / f64::from(canvas_px.width.max(canvas_px.height));
    let grid = PixelSize {
        width: output_px(canvas_px.width, scale),
        height: output_px(canvas_px.height, scale),
    };
    assert_eq!((grid.width, grid.height), (width, height));

    let sources: Vec<Option<PathBuf>> = doc.cells.iter().map(|cell| cell.source.clone()).collect();
    let mut preview = Preview::new();
    let built = preview.build(&doc, &sources, grid);
    assert!(built.failed.is_empty(), "{:?}", built.failed);
    // The copy really is a reduction of the 1600-px photo, not the photo itself:
    // this is the whole point of the step, and a drift measured from a path that
    // never reduced anything would be a measurement of nothing.
    assert_eq!(
        built.source_px.width as u32,
        preview_source_long_edge(grid),
        "the copy's long edge is the grid's own target"
    );
    assert!(built.source_px.width < 1600);

    let mut images = Images::new();
    for slot in &built.bitmaps {
        images.insert(slot.slot, bitmap(slot));
    }
    let preview_image =
        render_rgb8(&doc, &images, canvas_px, scale, None).expect("the preview draws");
    assert_eq!((preview_image.width, preview_image.height), (width, height));

    let difference = rmse(&preview_image.data, &pixels);
    eprintln!(
        "preview-grade source vs export: RMSE {difference:.4} over {} pixels",
        width * height
    );
    assert!(
        difference <= RMSE_THRESHOLD,
        "the reduced source is no longer the export's renderer: RMSE {difference:.4} > {RMSE_THRESHOLD}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
