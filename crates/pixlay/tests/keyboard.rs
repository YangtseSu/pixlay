// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S15h: the canvas's keyboard model — the arrow keys choose a cell, the selection
//! follows, and a screen reader is told which cell it is on (PIX-017's ruling of
//! 2026-09-24).
//!
//! The ruling's shape: "the arrow keys move a focus through the grid and the selection
//! follows it, and the accessible name says which cell the focus is on — not a proxy
//! widget per cell". Before it, a keyboard-only user could focus the canvas and do
//! nothing with it: the arrows panned the photo, and every other key acted on a
//! selection only a pointer could make.
//!
//! What this test holds: a project is opened **without the test selecting anything**,
//! then cells are chosen with the keyboard alone and identified by the canvas's own
//! accessible name; the edge of the sheet does not wrap; the framing nudges the arrows
//! used to be are still reachable under their modifiers; and an edit made on a
//! keyboard-chosen cell is one undo step.
//!
//! `support::press` emits the canvas's own `key-pressed` signal rather than using a
//! seat GTK does not have: it runs the binding the product installed, which is what
//! these criteria are about.

mod support;

use gtk4::gdk;

use pixlay::i18n::{fill, gettext};

#[test]
fn the_canvas_can_be_walked_with_the_keyboard_alone() {
    support::start();
    let app = support::app();
    let window = support::window(&app);
    window
        .open_path(&support::verify_project())
        .expect("the fixture project opens");
    support::canvas_bitmaps(&window);

    let slots = window.document().template.slots.len();
    assert!(slots >= 2, "the fixture has one cell per index to walk");
    // The centre of a cell in canvas coordinates, which is what "to the right of"
    // means geometrically — read off the document rather than off the neighbour
    // function the product uses, so the test's expectation is its own.
    let centre_x = |slot: usize| {
        let bbox = window.document().template.slots[slot].outline.bbox();
        (bbox.x0 + bbox.x1) / 2.0
    };
    let centre_y = |slot: usize| {
        let bbox = window.document().template.slots[slot].outline.bbox();
        (bbox.y0 + bbox.y1) / 2.0
    };

    // ---- nothing is chosen, and the keyboard chooses ----------------------
    assert_eq!(
        window.selection(),
        None,
        "a project opens with nothing selected"
    );
    assert_eq!(
        window.canvas_label(),
        gettext("Collage canvas"),
        "the canvas has no cell to announce yet"
    );

    assert!(
        support::press(&window, gdk::Key::Right, gdk::ModifierType::empty()),
        "the canvas claims the arrow keys"
    );
    assert_eq!(
        window.selection(),
        Some(0),
        "the first arrow press picks the first cell"
    );
    assert_eq!(
        window.canvas_label(),
        fill(gettext("Collage canvas, cell {} of {}"), &[1, slots]),
        "the accessible name says which cell the focus is on"
    );

    // The next press moves to a cell that is genuinely to the right of this one.
    assert!(support::press(
        &window,
        gdk::Key::Right,
        gdk::ModifierType::empty()
    ));
    let second = window.selection().expect("the focus moved to a cell");
    assert_ne!(second, 0, "the focus moved");
    assert!(
        centre_x(second) > centre_x(0),
        "the cell the keyboard calls 'to the right' ({second}) is not to the right of 0"
    );
    assert_eq!(
        window.canvas_label(),
        fill(
            gettext("Collage canvas, cell {} of {}"),
            &[second + 1, slots]
        ),
        "the name follows every step"
    );
    // And back: the geometry is a relation, not a direction that only works one way.
    assert!(support::press(
        &window,
        gdk::Key::Left,
        gdk::ModifierType::empty()
    ));
    assert_eq!(
        window.selection(),
        Some(0),
        "left returns to the first cell"
    );

    // ---- the edge does not wrap -------------------------------------------
    // Pressing right until the sheet has nothing further right lands on a cell in the
    // rightmost column (cells stacked in one column share a centre, and `neighbour`
    // takes a strictly greater x, so the walk stops at the first of them) and stays
    // there: a focus that reappeared on the left would read as a different key having
    // been pressed.
    for _ in 0..slots {
        support::press(&window, gdk::Key::Right, gdk::ModifierType::empty());
    }
    let stopped = window.selection().expect("a cell is chosen");
    let max_x = (0..slots).map(centre_x).fold(f64::MIN, f64::max);
    assert!(
        (centre_x(stopped) - max_x).abs() < 1e-9,
        "walking right ends in the rightmost column, not at cell {stopped}"
    );
    support::press(&window, gdk::Key::Right, gdk::ModifierType::empty());
    assert_eq!(
        window.selection(),
        Some(stopped),
        "the right edge of the sheet does not wrap"
    );
    for _ in 0..slots {
        support::press(&window, gdk::Key::Down, gdk::ModifierType::empty());
    }
    let lowest = window.selection().expect("a cell is chosen");
    let max_y = (0..slots).map(centre_y).fold(f64::MIN, f64::max);
    assert!(
        (centre_y(lowest) - max_y).abs() < 1e-9,
        "walking down ends in the lowest row, not at cell {lowest}"
    );

    // ---- the framing nudges are still there, under their modifiers ---------
    let slot = window.selection().expect("a cell is chosen");
    // A zoom first, so there is slack to pan into: at the covering zoom a photo can
    // sit exactly against the cell's edges, and the clamp would refuse the pan.
    assert!(support::press(
        &window,
        gdk::Key::plus,
        gdk::ModifierType::empty()
    ));
    let before = window
        .fitted_crop(slot)
        .expect("the selected cell has a photo");
    assert!(
        support::press(&window, gdk::Key::Right, gdk::ModifierType::SHIFT_MASK),
        "Shift+arrow is the framing nudge"
    );
    let shifted = window
        .fitted_crop(slot)
        .expect("the selected cell has a photo");
    assert!(
        (shifted.offset.0 - before.offset.0).abs() > 1e-6,
        "Shift+Right moved the photo (offset {} -> {})",
        before.offset.0,
        shifted.offset.0
    );
    assert!(
        support::press(&window, gdk::Key::Right, gdk::ModifierType::CONTROL_MASK),
        "Ctrl+arrow is the coarse nudge"
    );
    let coarse = window
        .fitted_crop(slot)
        .expect("the selected cell has a photo");
    assert!(
        (coarse.offset.0 - shifted.offset.0).abs() > 1e-6,
        "Ctrl+Right moved the photo further"
    );
    // And the swap still answers `Ctrl+Shift+arrow`. Taken on the first cell, whose
    // right neighbour the walk above has already shown to exist — the cell the Down
    // walk ends on is in the template's bottom row, which may be one full-width cell
    // with nothing to its right.
    window.select(Some(0));
    let before_swap = window.document();
    assert!(support::press(
        &window,
        gdk::Key::Right,
        gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
    ));
    let after_swap = window.document();
    assert_ne!(
        after_swap.cells, before_swap.cells,
        "Ctrl+Shift+Right swapped the selected cell with its neighbour"
    );
    assert!(window.can_undo(), "the swap is one undo step");
    window.undo();
    assert_eq!(
        window.document().cells,
        before_swap.cells,
        "undo puts the two cells back"
    );

    // ---- an edit on a keyboard-chosen cell --------------------------------
    // The keys that act on the selection all work without a pointer, which is the
    // half of the ruling that makes the focus worth having.
    let chosen = window.selection().expect("a cell is chosen");
    let zoomed = window
        .fitted_crop(chosen)
        .expect("the selected cell has a photo")
        .zoom;
    assert!(support::press(
        &window,
        gdk::Key::plus,
        gdk::ModifierType::empty()
    ));
    let bigger = window
        .fitted_crop(chosen)
        .expect("the selected cell has a photo")
        .zoom;
    assert!(
        bigger > zoomed,
        "the zoom key acted on the keyboard's own cell ({zoomed} -> {bigger})"
    );
    let before_clear = window.document();
    assert!(support::press(
        &window,
        gdk::Key::Delete,
        gdk::ModifierType::empty()
    ));
    assert!(
        window.document().cells[chosen].source.is_none(),
        "Delete cleared the keyboard's own cell"
    );
    window.undo();
    assert_eq!(
        window.document().cells,
        before_clear.cells,
        "clearing is one undo step"
    );
}
