//! Accessible names: one helper, so "every control has a name" is one habit
//! rather than three.
//!
//! HIG `guidelines/accessibility`: "All interface elements should have descriptive,
//! accessible names." GTK names many controls by itself — a `GtkButton` carrying a
//! label is announced by its text — but an icon-only button, a drawing area, a
//! spin button or a scale has nothing to derive a name from, so this crate sets one
//! explicitly wherever the widget's own content does not say what it is.
//!
//! `crates/pixlay/tests/hig.rs` walks the widget tree and fails on a control that
//! has neither: an explicit name, or one GTK derives from the control's own text.

use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

/// Gives a widget its accessible name.
pub fn label(widget: &impl IsA<gtk::Accessible>, text: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(text)]);
}

/// Gives a spin row's *spin button* the name: the row is a container, and the
/// spin button inside it is the control a screen reader announces.
pub fn label_spin_row(row: &adw::SpinRow, text: &str) {
    for widget in descendants(row.upcast_ref::<gtk::Widget>()) {
        if let Some(spin) = widget.downcast_ref::<gtk::SpinButton>() {
            label(spin, text);
        }
    }
}

/// Every widget below `root`, root included.
pub fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        found.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(child_widget) = child {
            stack.push(child_widget.clone());
            child = child_widget.next_sibling();
        }
    }
    found
}
