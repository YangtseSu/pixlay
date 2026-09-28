// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The selected cell is obvious: the canvas draws it in the theme's accent (S24).
//!
//! Finding 6 of the human's pass of 2026-09-25 was "the selected cell is not
//! distinguishable enough; give it a colour", and the plan's criterion for the step is
//! that a snapshot test **reads the accent colour from the same stylesheet and finds it
//! in the mark's pixels**. Both halves are here:
//!
//! * the colour — `canvas::accent()` (what the canvas draws with) against a probe label
//!   whose `color` is `var(--accent-bg-color)`, the variable `style.css` borders the
//!   layout band's chosen cell with, resolved by the toolkit in the running app;
//! * the pixels — the canvas's own snapshot, taken with the widget's own draw function,
//!   with the mark's pixels read back and compared with that colour.
//!
//! The two style variants are checked because the accent is a *system* value the app does
//! not name: libadwaita resolves it, so dark and light are two different resolutions of
//! the same question.
//!
//! **One test per binary**, for the reason `tests/support`'s module doc gives: GTK is
//! single-threaded and libtest runs a binary's tests on several threads.

mod support;

use std::time::Duration;

use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay::{EditorWindow, canvas};

/// How far a channel of a mark pixel may sit from the theme's accent.
///
/// The stroke is antialiased at its edges, and a probe that demanded the byte exactly
/// would be a probe of `f32 → u8` rounding rather than of the mark; the *sample* pixels
/// below are the ones the stroke covers fully, so the tolerance is slack, not the rule.
const TOLERANCE: i32 = 2;

/// How far apart the outline's own probe points are, in device pixels.
const STEP: f64 = 8.0;

/// One edge of a cell's outline in device coordinates, `(from, to)`.
type Edge = ((f64, f64), (f64, f64));

/// The cells the mark is probed on in the verification project: two different shapes, as
/// the swap test uses them.
const LEFT: usize = 0;
const RIGHT: usize = 5;

#[test]
fn the_selection_mark_is_the_theme_accent() {
    support::start();
    let app = support::app();
    let window = support::window(&app);

    // A probe that resolves the stylesheet's own variable: `style.css`'s
    // `.layout-cell.picked` borders the chosen layout cell with `--accent-bg-color`, and
    // `GtkWidget::color()` is the one public way to read a theme colour back (the style
    // context's `lookup_color` is deprecated since GTK 4.10 — `layout.rs` reads the
    // sketch's two colours the same way).
    let provider = gtk::CssProvider::new();
    provider.load_from_string(".accent-probe { color: var(--accent-bg-color); }");
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("the tests run on a display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_USER,
    );
    let probe = gtk::Label::new(None);
    probe.add_css_class("accent-probe");
    probe.set_visible(false);
    let probe_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    probe_box.append(&probe);
    let probe_window = adw::Window::builder().content(&probe_box).build();
    probe_window.present();

    let manager = adw::StyleManager::default();
    for scheme in [adw::ColorScheme::ForceDark, adw::ColorScheme::ForceLight] {
        manager.set_color_scheme(scheme);
        support::pump(Duration::from_millis(300));

        // ---- the colour: the canvas's own accent is the band's own variable --------
        let accent = rgb8(&canvas::accent());
        let css = rgb8_rgba(&probe.color());
        assert_eq!(
            accent, css,
            "under {scheme:?} the canvas draws the accent {:?} where the stylesheet's \
             `--accent-bg-color` is {:?}",
            accent, css
        );
        eprintln!("{scheme:?}: the canvas's accent and `--accent-bg-color` are {accent:?}");

        // ---- the pixels: the mark is that colour, and only the mark ----------------
        // The default document: one cell, no photo, a white sheet — so every accent pixel
        // the canvas draws while nothing is selected would be a mark that has no cell.
        let _ = support::canvas_bitmaps(&window);
        let (width, height) = support::canvas_size(&window);
        window.select(None);
        assert_eq!(
            window.selection(),
            None,
            "the probe starts with nothing selected"
        );
        let resting = marked_snapshot(&window);
        let stray = accent_pixels(&resting, accent);
        assert!(
            stray.is_empty(),
            "no cell is selected, yet the {width}x{height} canvas draws {} accent pixels \
             (the first at {:?})",
            stray.len(),
            stray.first()
        );

        window.select(Some(0));
        let marked = marked_snapshot(&window);
        let edges = outline_edges(&window, 0);
        let samples = outline_pixels(&edges);
        let missed: Vec<(i32, i32)> = samples
            .iter()
            .copied()
            .filter(|(x, y)| !is_accent(support::pixel(&marked, *x, *y), accent))
            .collect();
        assert!(
            missed.is_empty(),
            "under {scheme:?} the selected cell's outline is not the accent {:?}: {} of \
             {} probe points differ (the first at {:?} is {:?})",
            accent,
            missed.len(),
            samples.len(),
            missed.first(),
            missed.first().map(|(x, y)| support::pixel(&marked, *x, *y))
        );
        // And nothing else on the sheet carries it: the mark is this one cell's outline.
        let strays: Vec<(i32, i32)> = accent_pixels(&marked, accent)
            .into_iter()
            .filter(|(x, y)| distance_to(&edges, *x as f64, *y as f64) > 2.5)
            .collect();
        assert!(
            strays.is_empty(),
            "under {scheme:?} the canvas draws accent pixels away from the selected \
             cell's outline: {:?}",
            &strays[..strays.len().min(8)]
        );
        eprintln!(
            "{scheme:?}: {} probe points on the selected cell's outline are all the \
             accent; {} accent pixels, every one on that outline",
            samples.len(),
            accent_pixels(&marked, accent).len()
        );
    }
    manager.set_color_scheme(adw::ColorScheme::ForceDark);

    // ---- over a photo: the same mark, on the verification project's own cells ------
    window
        .open_path(&support::verify_project())
        .expect("the verification project opens");
    let _ = support::canvas_bitmaps(&window);
    let _ = support::canvas_size(&window);
    let accent = rgb8(&canvas::accent());
    for slot in [LEFT, RIGHT] {
        window.select(Some(slot));
        assert_eq!(window.selection(), Some(slot));
        let marked = marked_snapshot(&window);
        let edges = outline_edges(&window, slot);
        let samples = outline_pixels(&edges);
        let missed: Vec<(i32, i32)> = samples
            .iter()
            .copied()
            .filter(|(x, y)| !is_accent(support::pixel(&marked, *x, *y), accent))
            .collect();
        assert!(
            missed.is_empty(),
            "cell {slot} holds a photo, and its outline is not the accent {accent:?}: {} \
             of {} probe points differ (the first at {:?} is {:?})",
            missed.len(),
            samples.len(),
            missed.first(),
            missed.first().map(|(x, y)| support::pixel(&marked, *x, *y))
        );
        eprintln!(
            "cell {slot} of the verification project: {} probe points, every one the \
             accent",
            samples.len()
        );
    }

    // The walk gate's picture, out of the test that measured it: the mark as the window
    // draws it, over a photo, and — since S24 recoloured the outline S23 dashes — the
    // swap's own marked state next to it.
    let marked = marked_snapshot(&window);
    support::save_png(&support::artifact("selection.png"), &marked);
    window.set_swap_source(Some(RIGHT));
    let swapping = marked_snapshot(&window);
    support::save_png(&support::artifact("selection-swap.png"), &swapping);
    window.cancel_swap();
}

/// The snapshot the next mark draws into: a `queue_draw` has been asked for, and a
/// snapshot replays the *last* frame's nodes, so two frames are what it takes (the same
/// wait `tests/swap.rs` makes before it reads a mark).
fn marked_snapshot(window: &EditorWindow) -> support::Image {
    support::after_frames(window, 2, support::WAIT);
    support::snapshot(&window.canvas_widget())
}

/// A cell's own outline in device coordinates, as `(from, to)` edges — the path the
/// canvas strokes (`canvas::render`'s `slot_path`).
fn outline_edges(window: &EditorWindow, slot: usize) -> Vec<Edge> {
    let area = window.canvas_widget();
    let (grid, _) = window.images();
    let placement = canvas::placement(grid, area.width(), area.height());
    let doc = window.document();
    let points = &doc.template.slots[slot].outline.points;
    (0..points.len())
        .map(|index| {
            (
                placement.to_widget(points[index]),
                placement.to_widget(points[(index + 1) % points.len()]),
            )
        })
        .collect()
}

/// The pixels those edges pass through, every [`STEP`] device pixels.
///
/// The stroke is two device pixels wide and centred on this path, so the pixel a path
/// point falls in is covered by the stroke's full width in the direction across the edge,
/// and by the whole edge along it — the pixel is the accent itself, not a blend of it and
/// what the cell holds.
fn outline_pixels(edges: &[Edge]) -> Vec<(i32, i32)> {
    let mut pixels = Vec::new();
    for ((ax, ay), (bx, by)) in edges {
        let steps = (((bx - ax).hypot(by - ay)) / STEP).floor().max(1.0) as usize;
        for step in 0..steps {
            let t = step as f64 / steps as f64;
            pixels.push((
                (ax + (bx - ax) * t).floor() as i32,
                (ay + (by - ay) * t).floor() as i32,
            ));
        }
    }
    pixels
}

/// How far a pixel sits from the outline, in device pixels.
fn distance_to(edges: &[Edge], x: f64, y: f64) -> f64 {
    edges
        .iter()
        .map(|((ax, ay), (bx, by))| {
            let (dx, dy) = (bx - ax, by - ay);
            let length = dx * dx + dy * dy;
            let t = if length > 0.0 {
                (((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (x - (ax + dx * t)).hypot(y - (ay + dy * t))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Every pixel of a snapshot that is the accent, within [`TOLERANCE`].
fn accent_pixels(image: &support::Image, accent: (i32, i32, i32)) -> Vec<(i32, i32)> {
    let (width, height, _) = image;
    let mut found = Vec::new();
    for y in 0..*height {
        for x in 0..*width {
            if is_accent(support::pixel(image, x, y), accent) {
                found.push((x, y));
            }
        }
    }
    found
}

/// Whether a pixel is the accent, within [`TOLERANCE`] per channel.
fn is_accent(pixel: [u8; 3], accent: (i32, i32, i32)) -> bool {
    [accent.0, accent.1, accent.2]
        .into_iter()
        .zip(pixel)
        .all(|(wanted, got)| (wanted - i32::from(got)).abs() <= TOLERANCE)
}

/// The canvas's accent (unit floats) as the eight-bit colour a pixel would carry.
fn rgb8(accent: &(f64, f64, f64)) -> (i32, i32, i32) {
    (
        (accent.0 * 255.0).round() as i32,
        (accent.1 * 255.0).round() as i32,
        (accent.2 * 255.0).round() as i32,
    )
}

/// A `GdkRGBA` as the same eight-bit colour.
fn rgb8_rgba(rgba: &gtk::gdk::RGBA) -> (i32, i32, i32) {
    rgb8(&(
        f64::from(rgba.red()),
        f64::from(rgba.green()),
        f64::from(rgba.blue()),
    ))
}
