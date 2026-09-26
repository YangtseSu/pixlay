//! S7's machine-checkable GNOME HIG subset, and the interface's language.
//!
//! One test: GTK lives on one thread (see `support`). What is checked here is the
//! part of `docs/HIG-REVIEW.md` section 1 that a machine can answer — the
//! accelerator table against `reference/keyboard`, accessible names
//! (`guidelines/accessibility`), the adaptive minimum (`guidelines/adaptive`), the
//! colour schemes (`guidelines/ui-styling`), the about dialog's metadata — and the
//! step's own criterion that the interface is English when the locale is missing,
//! `C`, or unknown.
//!
//! **Since S22 there is one surface**: the window opens on the editor (ruling 31), so
//! the checks walk that page from the first frame — the header bar's own chrome, the
//! canvas's controls, the layout band and the two dialogs — and `selection-mode`'s
//! row is back to "not applicable" (the app has no multi-select collection view any
//! more, `docs/HIG-REVIEW.md` §1).

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use pixlay::{APP_ID, app, canvas, i18n, window::EditorWindow};

/// The accelerator combinations HIG `reference/keyboard` requires *for the
/// features this product has*: quit, close, open, save, save as, undo, redo, the
/// shortcuts dialog, a new item, and — since S25 gave the app a preferences surface
/// (ruling 36) — `Ctrl+,`, which is that page's own binding for it. Print, send, help
/// and the utility pane's `F9` belong to features v1 does not have — `F9` left with the
/// pane in S13 (ruling 18).
const REQUIRED: [&str; 10] = [
    "<Control>q",
    "<Control>w",
    "<Control>o",
    "<Control>s",
    "<Control><Shift>s",
    "<Control>z",
    "<Control><Shift>z",
    "<Control>question",
    "<Control>n",
    "<Control>comma",
];

#[test]
fn the_interface_meets_the_machine_checkable_hig() {
    support::start();
    // The language checks need a locale of their own, which can only be set before
    // the process starts, so they run in children that re-enter this test with
    // `PIXLAY_LANGUAGE_CHILD` set and do nothing else.
    if std::env::var_os("PIXLAY_LANGUAGE_CHILD").is_some() {
        check_english();
        return;
    }
    let mut failures: Vec<String> = Vec::new();

    let application = support::app();
    let window = support::window(&application);

    // ---- the window, from its first frame ------------------------------------
    // Since S22 the window opens on the editor (ruling 31), so there is no second
    // surface to check first: the header bar, the accessible names and the accelerator
    // table are checked on the window exactly as it opens — one cell, no photo.
    check_shortcuts(&application, &window, &mut failures);
    check_accessible_names(&window, &mut failures);
    check_header_chrome(&window, &mut failures);

    // ---- the document --------------------------------------------------------
    // A project is what fills the canvas and the band, so the checks that need a real
    // document come after this one.
    let project = support::verify_project();
    window
        .open_path(&project)
        .expect("the verification project opens");
    // The canvas's bitmaps and the band's candidates are in hand before anything below
    // is measured: a window that has just been given a document has neither, and every
    // check that follows reads an allocation or a `Placement` (measured 2026-09-23:
    // without this the walk read a 0x0 canvas, a band with no candidates and header
    // controls at position 0 on some runs and was fine on others —
    // `tests/support::canvas_bitmaps` is the harness's answer to exactly that).
    let _ = support::canvas_bitmaps(&window);
    assert!(
        window.wait_for_gallery(support::WAIT),
        "the layout band was built"
    );
    check_accessible_names(&window, &mut failures);
    check_gallery(&window, &mut failures);
    check_compose(&window, &mut failures);
    check_editor_minimum(&window, &mut failures);
    check_colour_schemes(&window, &mut failures);
    check_about(&mut failures);
    check_language(&mut failures);

    if !failures.is_empty() {
        panic!(
            "{} HIG check(s) failed:\n  - {}",
            failures.len(),
            failures.join("\n  - ")
        );
    }
}

/// An accelerator as a comparable value: its modifiers as a sorted set plus the
/// key, so that GTK's token order does not matter.
fn binding(accelerator: &str) -> (Vec<String>, String) {
    let mut modifiers = Vec::new();
    let mut key = String::new();
    for part in accelerator.split('<') {
        if part.is_empty() {
            continue;
        }
        match part.split_once('>') {
            Some((name, rest)) => {
                if !name.is_empty() {
                    modifiers.push(name.to_ascii_lowercase());
                }
                // The text after the `>` is the key, and it is the key whether or not a
                // modifier came first: `<Control>s` is *control + s*, not "control and
                // nothing". (Until S22 this branch dropped `rest` whenever the name was
                // non-empty, so every accelerator's key went missing and the comparison
                // below could only ever have checked the modifier set — the duplicate
                // check S22 added is what made the hole visible.)
                key.push_str(rest);
            }
            None => key.push_str(part),
        }
    }
    modifiers.sort();
    (modifiers, key)
}

/// The accelerator table contains the required set, uses nothing the system
/// reserves, and every action it names is really installed.
fn check_shortcuts(
    application: &adw::Application,
    window: &EditorWindow,
    failures: &mut Vec<String>,
) {
    let bound: Vec<String> = app::ACCELERATORS
        .iter()
        .map(|(_, accelerator)| (*accelerator).to_string())
        .collect();
    // Exactly one accelerator per action, and no accelerator bound twice (S22's
    // criterion: the table is a bijection — a second binding for one action would be a
    // key that means two things, which is how `Ctrl+Shift+O` and `Z` were free again
    // once the picker's actions left).
    let mut seen: Vec<((Vec<String>, String), String)> = Vec::new();
    for (action, accelerator) in app::ACCELERATORS {
        let parsed = binding(accelerator);
        if let Some((_, other)) = seen.iter().find(|(key, _)| *key == parsed) {
            failures.push(format!(
                "{action} and {other} are both bound to {accelerator}"
            ));
        }
        seen.push((parsed, action.to_string()));
        let installed = application.accels_for_action(action);
        if installed.len() != 1 {
            failures.push(format!(
                "{action} has {} accelerators ({installed:?}), not exactly one",
                installed.len()
            ));
        }
    }
    for required in REQUIRED {
        if !bound.iter().any(|bound| bound == required) {
            failures.push(format!("HIG requires {required}, which nothing binds"));
        }
    }
    for (action, accelerator) in app::ACCELERATORS {
        if accelerator.is_empty() {
            failures.push(format!("{action} has no accelerator"));
        }
        // Alt+* / Super+* / Ctrl+Alt+* belong to the system, not to an app.
        for reserved in ["<Alt>", "<Super>", "<Meta>", "<Control><Alt>"] {
            if accelerator.contains(reserved) {
                failures.push(format!(
                    "{action} uses the system-reserved {reserved} ({accelerator})"
                ));
            }
        }
        // GTK reorders the modifier tokens (`<Control><Shift>s` comes back as
        // `<Shift><Control>s`), so the comparison is on the parsed binding.
        let installed = application.accels_for_action(action);
        let wanted = binding(accelerator);
        if !installed.iter().any(|value| binding(value) == wanted) {
            failures.push(format!(
                "{action} is not bound to {accelerator} on the application (got {installed:?})"
            ));
        }
    }
    // Every action the shortcuts dialog shows is bound, and has a title: a missing
    // arm in `shortcut_title` would show an empty row.
    for (section, actions) in app::SHORTCUT_SECTIONS {
        if app::shortcut_section_title(section).is_empty() {
            failures.push(format!("the {section} section has no title in the dialog"));
        }
        for action in *actions {
            if !app::ACCELERATORS.iter().any(|(name, _)| name == action) {
                failures.push(format!("{section} lists {action}, which is not bound"));
            }
            if app::shortcut_title(action).is_empty() {
                failures.push(format!("{action} has no title in the shortcuts dialog"));
            }
        }
    }
    // Keyboard path: every action a widget activates is either accelerated above
    // or attached to a control the Tab order reaches.
    let tree = support::descendants(window.upcast_ref::<gtk4::Widget>());
    let accelerated: BTreeSet<&str> = app::ACCELERATORS.iter().map(|(name, _)| *name).collect();
    for widget in &tree {
        let Some(name) = support::action_name(widget) else {
            continue;
        };
        if accelerated.contains(name.as_str()) {
            continue;
        }
        if !widget.is_focusable() && name.starts_with("win.") {
            failures.push(format!(
                "{name} has no accelerator and its widget is not focusable"
            ));
        }
    }
}

/// Every interactive control has an accessible name (`guidelines/accessibility`).
fn check_accessible_names(window: &EditorWindow, failures: &mut Vec<String>) {
    check_accessible_names_in(window.upcast_ref::<gtk4::Widget>(), failures);
}

/// The same walk over any root: the window's tree is one, and a presented dialog's
/// own tree is another (a dialog is not a child of the window it belongs to).
fn check_accessible_names_in(root: &gtk4::Widget, failures: &mut Vec<String>) {
    for widget in support::descendants(root) {
        if !is_interactive(&widget) || is_platform_chrome(&widget) {
            continue;
        }
        // A control that is not on screen is not one a screen reader can reach — and
        // libadwaita keeps hidden controls inside its own rows: `AdwEntryRow` carries an
        // apply button that is hidden unless `show-apply-button` is set, and it has no
        // name because it is never meant to be seen (measured 2026-09-23, the first
        // `AdwEntryRow` in this app). Visibility *where it matters* is asserted by the
        // checks that are about a specific control.
        if !widget.is_visible() {
            continue;
        }
        if !support::has_accessible_name(&widget) {
            failures.push(format!(
                "{} has no accessible name (inside {}); its own text is {:?}",
                widget.type_().name(),
                ancestors(&widget),
                control_text(&widget),
            ));
        }
    }
}

/// The platform's own chrome, which is not this app's control.
///
/// Two cases, both libadwaita's own accessibility decisions rather than this app's:
///
/// * `AdwToastWidget` puts a dismiss button in every toast and gives it a tooltip
///   ("Dismiss") and no label;
/// * `AdwEntryRow` carries an apply button of its own (`adw-entry-apply-symbolic`, a
///   tooltip, no accessible label) and shows it in a dialog whatever `show-apply-button`
///   says — measured 2026-09-23: the row was built with `show-apply-button = false` and
///   the button was still on screen, unnamed, which would otherwise be a failure of a
///   control this app never added.
fn is_platform_chrome(widget: &gtk4::Widget) -> bool {
    if let Some(button) = widget.downcast_ref::<gtk4::Button>()
        && button.icon_name().as_deref() == Some("adw-entry-apply-symbolic")
    {
        return true;
    }
    let mut current = Some(widget.clone());
    while let Some(candidate) = current {
        if candidate.type_().name() == "AdwToastWidget" {
            return true;
        }
        current = candidate.parent();
    }
    false
}

/// The controls a screen reader has to announce: everything a user can operate.
fn is_interactive(widget: &gtk4::Widget) -> bool {
    widget.is::<gtk4::Button>()
        || widget.is::<gtk4::ToggleButton>()
        || widget.is::<gtk4::CheckButton>()
        || widget.is::<gtk4::Switch>()
        || widget.is::<gtk4::Entry>()
        || widget.is::<gtk4::SpinButton>()
        || widget.is::<gtk4::DropDown>()
        || widget.is::<gtk4::Scale>()
        || widget.is::<gtk4::DrawingArea>()
        || widget.is::<gtk4::ColorDialogButton>()
        || widget.is::<gtk4::MenuButton>()
        || widget.is::<gtk4::GridView>()
}

/// What a control says about itself, for a failure message that can be acted on: an
/// icon-only button's icon, a labelled one's text, its tooltip.
fn control_text(widget: &gtk4::Widget) -> String {
    if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
        return format!(
            "icon {:?}, label {:?}, tooltip {:?}",
            button.icon_name(),
            button.label(),
            button.tooltip_text(),
        );
    }
    format!("tooltip {:?}", widget.tooltip_text())
}

/// The class names of a widget's ancestors, for a failure message that can be
/// acted on.
fn ancestors(widget: &gtk4::Widget) -> String {
    let mut names = Vec::new();
    let mut current = widget.parent();
    while let Some(parent) = current {
        names.push(parent.type_().name().to_string());
        current = parent.parent();
    }
    names.join(" < ")
}

/// The menu's items, in the order the model lists them, flattened over its sections.
fn menu_actions(model: &gio::MenuModel) -> Vec<String> {
    let mut actions = Vec::new();
    for index in 0..model.n_items() {
        if let Some(section) = model.item_link(index, gio::MENU_LINK_SECTION) {
            actions.extend(menu_actions(&section));
        }
        if let Some(action) = model.item_attribute_value(index, "action", None) {
            actions.push(action.str().unwrap_or_default().to_string());
        }
    }
    actions
}

/// The layout band (S14): the count control and the candidates, against the two HIG
/// chapters a band of controls has to answer — `guidelines/pointer-touch` (a click
/// target has a size) and `guidelines/accessibility` (a control has a name).
///
/// What the band *lists* and what its candidates look like is `tests/layout.rs`'s
/// subject; this is the part that is the same question for every control in the
/// window.
fn check_gallery(window: &EditorWindow, failures: &mut Vec<String>) {
    /// HIG `guidelines/pointer-touch`: "ensure that all interactive elements are
    /// at least 24x24 pixels". The band's cells are 128 logical pixels wide by
    /// construction; the check is here so a later change cannot make them small.
    const MIN_TARGET: i32 = 24;

    let Some(gallery) = window.gallery() else {
        failures.push("the editor has no layout band".into());
        return;
    };
    let photos = window.photo_count();
    if window.candidate_templates().len() < 3 {
        failures.push(format!(
            "{photos} photos have only {} layouts to choose from",
            window.candidate_templates().len()
        ));
    }
    let candidates = gallery.candidates();
    if candidates.len() != window.candidate_templates().len() {
        failures.push(format!(
            "the band lists {} candidates for {} candidate templates",
            candidates.len(),
            window.candidate_templates().len()
        ));
    }
    for name in &candidates {
        let Some(cell) = gallery.cell(name) else {
            failures.push(format!("{name} has no cell in the band"));
            continue;
        };
        let widget = cell.clone().upcast::<gtk4::Widget>();
        // The cells arrive with the band's build, and a widget that has just been added
        // to the tree is allocated on the next frame.
        let _ = support::allocated(&widget, window);
        if cell.width() < MIN_TARGET || cell.height() < MIN_TARGET {
            failures.push(format!(
                "{name}'s cell is {}x{}, below the {MIN_TARGET} px target",
                cell.width(),
                cell.height()
            ));
        }
        if !support::has_accessible_name(&widget) {
            failures.push(format!("{name}'s cell has no accessible name"));
        }
        // The keyboard path (`guidelines/keyboard`): a candidate is reachable and
        // activatable without a pointer, or the main path is not walkable.
        if !cell.is_focusable() {
            failures.push(format!("{name}'s cell cannot be reached with the keyboard"));
        }
    }
    for (what, button) in [
        ("the remove control", gallery.minus_button()),
        ("the add control", gallery.plus_button()),
    ] {
        if button.tooltip_text().is_none() {
            failures.push(format!("{what} has no tooltip"));
        }
        if !button.is_focusable() {
            failures.push(format!("{what} cannot be reached with the keyboard"));
        }
    }
    if gallery.count_label().label().is_empty() {
        failures.push("the band's count has no label".into());
    }
    // The empty cells' own `+` (S14b): the visible control over a cell that has no
    // photo. It is a real button, so the accessible-name and keyboard checks above
    // already cover it — what is checked here is that the document this check runs
    // on has none of them shown and that the control exists for every slot, so a
    // later change cannot quietly drop it.
    let Some(empty) = window.cell_controls() else {
        failures.push("the canvas has no empty-cell layer".into());
        return;
    };
    for slot in 0..window.document().cells.len() {
        let Some(button) = empty.button(slot) else {
            failures.push(format!("slot {slot} has no `+` control"));
            continue;
        };
        if button.tooltip_text().is_none() {
            failures.push(format!("slot {slot}'s `+` has no tooltip"));
        }
        if !button.is_focusable() {
            failures.push(format!(
                "slot {slot}'s `+` cannot be reached with the keyboard"
            ));
        }
        if button.is_visible() {
            failures.push(format!("slot {slot} holds a photo but its `+` is shown"));
        }
    }
}

/// The compose stage's own controls (S15): the selected cell's strip, and the two
/// document-level dialogs — against the chapters a control has to answer whichever
/// surface it is on.
///
/// The strip is `guidelines/pointer-touch` (a target has a size) and
/// `guidelines/keyboard` (every action is reachable without a pointer); the dialogs
/// are `guidelines/accessibility` (every control is named) and, with
/// `patterns/feedback/dialogs`, the shape ruling 18 gave them: a header bar with a
/// heading, the cancel button before the affirmative, and the affirmative carrying
/// the verb the action is.
///
/// What the strip *does* to the document, and where it sits inside its cell, is
/// `tests/compose.rs`'s subject; what is checked here is the part that is the same
/// question for every control in the window.
fn check_compose(window: &EditorWindow, failures: &mut Vec<String>) {
    /// HIG `guidelines/pointer-touch`: "ensure that all interactive elements are at
    /// least 24x24 pixels".
    const MIN_TARGET: i32 = 24;

    let Some(controls) = window.cell_controls() else {
        failures.push("the canvas has no cell-control layer".into());
        return;
    };
    // A cell that holds a photo is selected, so the strip is the family on screen.
    let occupied = window
        .document()
        .cells
        .iter()
        .position(|cell| cell.source.is_some());
    if let Some(slot) = occupied {
        window.select(Some(slot));
        // The strip is shown by `refresh`; the allocation it needs is a frame away
        // (`support::allocated`).
        let _ = support::allocated(&controls.strip().upcast::<gtk4::Widget>(), window);
    }
    let strip = controls.strip();
    if !strip.is_visible() {
        failures.push("the selected cell's controls are not shown".into());
    }
    for (index, button) in controls.strip_buttons().into_iter().enumerate() {
        let widget = button.clone().upcast::<gtk4::Widget>();
        if !support::has_accessible_name(&widget) {
            failures.push(format!(
                "the strip's control {index} has no accessible name"
            ));
        }
        if button.tooltip_text().is_none() {
            failures.push(format!("the strip's control {index} has no tooltip"));
        }
        // The keyboard path: these are the pointer's *visible* controls, and a
        // focusable button is what makes them the same controls for the keyboard.
        if !button.is_focusable() {
            failures.push(format!(
                "the strip's control {index} cannot be reached with the keyboard"
            ));
        }
        if button.width() < MIN_TARGET || button.height() < MIN_TARGET {
            failures.push(format!(
                "the strip's control {index} is {}x{}, below the {MIN_TARGET} px target",
                button.width(),
                button.height()
            ));
        }
    }

    // The app's one dialog (S25b), presented so that its own tree can be walked: a
    // dialog that is not on screen has no allocation and no accessible tree. It is the
    // document's frame above the export's settings, and its heading is its own title.
    let dialog = window
        .settings_dialog()
        .map(|dialog| dialog.widget().upcast::<adw::Dialog>());
    let Some(dialog) = dialog else {
        failures.push("the window has no settings dialog".into());
        return;
    };
    dialog.present(Some(window));
    window.pump(Duration::from_millis(50));
    let root = dialog.clone().upcast::<gtk4::Widget>();
    check_accessible_names_in(&root, failures);
    // The heading, and the two groups the dialog carries: the frame's rows are the
    // document's and the export's are the app's (S25b's order — the collage first).
    let controls = support::descendants(&root);
    let labels: Vec<String> = controls
        .iter()
        .filter_map(|widget| widget.downcast_ref::<gtk4::Label>())
        .map(|label| label.label().to_string())
        .collect();
    if !labels.iter().any(|label| label == "Preferences") {
        failures.push("the settings dialog has no heading reading \"Preferences\"".into());
    }
    for group in ["Frame", "Export"] {
        if !labels.iter().any(|label| label == group) {
            failures.push(format!("the settings dialog has no {group} group"));
        }
    }
    dialog.force_close();
    // Leave the window as this check found it: the colour-scheme probe that follows
    // samples the sheet near its edges, and a selection outline is interface drawn on
    // top of the content.
    window.pump(Duration::from_millis(200));
    window.select(None);
}

/// The window's header bar, held to the alignment points HIG
/// `patterns/containers/header-bars` and rulings 24 and 42 fix: the way in is the
/// leftmost control, the document's other controls follow at the start, the heading is
/// in the centre, the menu and the export at the end — and, since S22, no Save button
/// (ruling 37), with the menu carrying the actions a button no longer does.
fn check_header_chrome(window: &EditorWindow, failures: &mut Vec<String>) {
    let Some(header) = window.header() else {
        failures.push("the window has no header bar".into());
        return;
    };
    let root = header.clone().upcast::<gtk4::Widget>();
    let controls = support::descendants(&root);
    // The controls are found by the *action* they carry, which is what the window
    // itself binds: an icon-only lookup would miss the export button (its icon lives in
    // an `AdwButtonContent`, so the button's own `icon-name` is empty) and the menu,
    // which is a `GtkMenuButton`.
    let by_action = |action: &str| {
        controls
            .iter()
            .find(|widget| support::action_name(widget).as_deref() == Some(action))
            .cloned()
    };
    let (Some(frame), Some(export), Some(add)) = (
        by_action("win.frame"),
        by_action("win.export"),
        by_action("win.add-photos"),
    ) else {
        failures
            .push("the header is missing the frame, the export or the Add photos control".into());
        return;
    };
    // Ruling 37: Save is a menu item and an accelerator, not a button beside Export —
    // the two read as the same action.
    if controls
        .iter()
        .any(|widget| support::action_name(widget).as_deref() == Some("win.save"))
    {
        failures.push("the header bar still carries a Save button".into());
    }
    // The controls the header *does* hold are all in the start slot or all in the end
    // one, and the two actions that are one pair (undo, redo) are both at the start.
    for action in ["win.undo", "win.redo"] {
        if by_action(action).is_none() {
            failures.push(format!("the header has no {action} control"));
        }
    }
    let Some(menu) = controls
        .iter()
        .find(|widget| widget.is::<gtk4::MenuButton>())
        .and_then(|widget| widget.clone().downcast::<gtk4::MenuButton>().ok())
    else {
        failures.push("the header has no primary menu".into());
        return;
    };
    let menu_widget = menu.clone().upcast::<gtk4::Widget>();
    let centre = |widget: &gtk4::Widget| {
        widget
            .compute_point(
                &root,
                &gtk4::graphene::Point::new(
                    widget.width() as f32 / 2.0,
                    widget.height() as f32 / 2.0,
                ),
            )
            .map(|point| point.x())
    };
    let title = header
        .title_widget()
        .or_else(|| header.first_child())
        .map(|widget| centre(&widget));
    match (
        centre(&add),
        by_action("win.undo").and_then(|undo| centre(&undo)),
        centre(&frame),
        title.flatten(),
        centre(&menu_widget),
        centre(&export),
    ) {
        (Some(add_x), Some(undo_x), Some(frame_x), Some(title_x), Some(menu_x), Some(export_x)) => {
            // Ruling 42: the way in is the leftmost control, ahead of undo and redo.
            if add_x >= undo_x {
                failures.push(format!(
                    "the Add photos button ({add_x:.0}) is not left of undo ({undo_x:.0})"
                ));
            }
            if add_x >= title_x {
                failures.push(format!(
                    "the Add photos button ({add_x:.0}) is not left of the heading ({title_x:.0})"
                ));
            }
            if frame_x >= title_x {
                failures.push(format!(
                    "the frame button ({frame_x:.0}) is not left of the heading ({title_x:.0})"
                ));
            }
            for (name, x) in [("menu", menu_x), ("export button", export_x)] {
                if x <= title_x {
                    failures.push(format!(
                        "the {name} ({x:.0}) is not right of the heading ({title_x:.0})"
                    ));
                }
            }
            let size = |widget: &gtk4::Widget| format!("{}x{}", widget.width(), widget.height());
            eprintln!(
                "the header bar at the default size (centre px from its start, size): \
                 Add photos {add_x:.0} {}, undo {undo_x:.0}, frame {frame_x:.0} {}, \
                 heading {title_x:.0}, menu {menu_x:.0} {}, export {export_x:.0} {}",
                size(&add),
                size(&frame),
                size(&menu_widget),
                size(&export),
            );
        }
        _ => failures.push("the header's controls are not allocated".into()),
    }
    // Every control *this app* puts in the header carries a tooltip and a name (this
    // page's own "tooltips on primary controls" and `guidelines/pointer-touch`'s
    // 24x24 target for the new control at the default size).
    const MIN_TARGET: i32 = 24;
    for (name, widget) in [
        ("the Add photos button", add.clone()),
        ("the frame button", frame.clone()),
        ("the export button", export.clone()),
        ("the menu", menu_widget.clone()),
    ] {
        if widget.tooltip_text().is_none() {
            failures.push(format!("the header's {name} has no tooltip"));
        }
        if !support::has_accessible_name(&widget) {
            failures.push(format!("the header's {name} has no accessible name"));
        }
        if widget.width() < MIN_TARGET || widget.height() < MIN_TARGET {
            failures.push(format!(
                "the header's {name} is {}x{}, below the {MIN_TARGET} px target",
                widget.width(),
                widget.height()
            ));
        }
    }

    // The heading is the document's name, dirty marker included, and the window's own
    // title is the same string (S22: one page, so one name).
    let heading = header
        .title_widget()
        .and_then(|widget| widget.downcast::<adw::WindowTitle>().ok())
        .map(|widget| widget.title().to_string());
    if heading.as_deref().unwrap_or("").is_empty() {
        failures.push("the header's heading is empty".into());
    }
    if let Some(heading) = heading {
        let window_title = window.title().map(|title| title.to_string());
        if window_title.as_deref() != Some(heading.as_str()) {
            failures.push(format!(
                "the window is called {window_title:?} where its heading reads {heading:?}"
            ));
        }
    }

    // The primary menu holds the actions a button no longer carries — Save among them
    // — and nothing the window cannot do.
    let actions = menu
        .menu_model()
        .map(|model| menu_actions(&model))
        .unwrap_or_default();
    let wanted = [
        "app.new",
        "app.open",
        "win.save",
        "win.save-as",
        "win.export",
        "win.add-photos",
        "win.cut",
        "win.copy",
        "win.paste",
        "win.reset-framing",
        "app.settings",
        "app.shortcuts",
        "app.about",
    ];
    if actions != wanted {
        failures.push(format!(
            "the window's menu is {actions:?}, not the ruled {wanted:?}"
        ));
    }
}

/// At the minimum size the sheet is still drawn in full inside the canvas
/// (HIG `guidelines/adaptive`) and the header bar still holds its controls
/// (S26: the way in keeps its 24x24 target when its label has given way, and the
/// heading keeps an allocation of its own).
fn check_editor_minimum(window: &EditorWindow, failures: &mut Vec<String>) {
    let minimum = window.size_request();
    window.set_default_size(minimum.0, minimum.1);
    window.pump(Duration::from_millis(500));

    // The header bar at this size, where `AdwButtonContent:can-shrink` may have
    // dropped the Add photos label: the control is still a control, and the heading
    // is not squeezed to nothing by the controls beside it.
    const MIN_TARGET: i32 = 24;
    let header = window
        .header()
        .map(|header| header.upcast::<gtk4::Widget>());
    let add = header.as_ref().and_then(|header| {
        support::descendants(header)
            .into_iter()
            .find(|widget| support::action_name(widget).as_deref() == Some("win.add-photos"))
    });
    let add_size = match &add {
        Some(add) => {
            if add.width() < MIN_TARGET || add.height() < MIN_TARGET {
                failures.push(format!(
                    "at the minimum size {}x{} the Add photos button is {}x{}, below the \
                     {MIN_TARGET} px target",
                    minimum.0,
                    minimum.1,
                    add.width(),
                    add.height()
                ));
            }
            format!("{}x{}", add.width(), add.height())
        }
        None => {
            failures.push("at the minimum size the header has no Add photos control".into());
            "none".into()
        }
    };
    let heading = window
        .header()
        .and_then(|header| header.title_widget().or_else(|| header.first_child()))
        .map(|widget| widget.width())
        .unwrap_or(0);
    if heading <= 0 {
        failures.push(format!(
            "at the minimum size {}x{} the heading has no allocation of its own",
            minimum.0, minimum.1
        ));
    }
    eprintln!(
        "the header bar at {}x{}: Add photos {add_size}, heading {heading} px wide",
        minimum.0, minimum.1
    );

    // The band shares the page with the canvas, so "the canvas is drawn in full"
    // and "the band exists" are one criterion at this size.
    if let Some(gallery) = window.gallery() {
        let band = gallery.root();
        if band.height() <= 0 || gallery.strip().width() <= 0 {
            failures.push(format!(
                "at the minimum size {}x{} the layout band is {}x{} tall",
                minimum.0,
                minimum.1,
                band.width(),
                band.height()
            ));
        }
    } else {
        failures.push("the editor has no layout band".into());
    }
    let area = window.canvas_widget();
    let (width, height) = (area.width(), area.height());
    if width <= 0 || height <= 0 {
        failures.push(format!(
            "at the minimum size {}x{} the canvas is not allocated ({width}x{height})",
            minimum.0, minimum.1
        ));
        return;
    }
    let (grid, _) = window.images();
    let placement = canvas::placement(grid, width, height);
    if placement.origin.0 < 0.0
        || placement.origin.1 < 0.0
        || placement.origin.0 + placement.width() > f64::from(width) + 0.5
        || placement.origin.1 + placement.height() > f64::from(height) + 0.5
    {
        failures.push(format!(
            "the sheet ({:.0}x{:.0} at {:.0},{:.0}) does not fit the {width}x{height} canvas",
            placement.width(),
            placement.height(),
            placement.origin.0,
            placement.origin.1,
        ));
    }

    // The picture the look at this size is judged from (S26): the walk
    // `docs/HIG-REVIEW.md` §2 item 11 asks whether the header bar still reads as three
    // groups here, where the Add photos label has given way — and a question about a
    // *look* needs the window, so it is written out like the band's and the selection's
    // own pictures.
    let picture = support::artifact("minimum.png");
    support::save_png(&picture, &support::snapshot(window));
    eprintln!(
        "the window at its minimum size {}x{}: {picture:?}",
        minimum.0, minimum.1
    );
}

/// The colour scheme is applied to the window, and the **sheet's own pixels** do not
/// depend on it: the collage is content, and the space around it is the theme's.
///
/// The check used to compare the whole canvas widget between the two schemes and
/// fail on any difference. That passed while the canvas flooded its entire widget
/// with the document's backdrop — in other words it *asserted* the defect the human
/// reported on 2026-09-23 ("the dark theme still has not been applied"): the window
/// was dark around a canvas that had been painted white edge to edge. A widget's own
/// snapshot cannot see the theme either (its surrounding pixels come back with a
/// zero alpha, which this harness reads as black), so what is measured here is the
/// **window**: the thing the human was looking at.
fn check_colour_schemes(window: &EditorWindow, failures: &mut Vec<String>) {
    let manager = adw::StyleManager::default();
    // The app is dark by default (ruling 23): `app.rs` forces it at startup, and this
    // is the state the window was built in before either probe below moves it.
    if manager.color_scheme() != adw::ColorScheme::ForceDark {
        failures.push(format!(
            "the app is not dark by default ({:?})",
            manager.color_scheme()
        ));
    }
    let area = window.canvas_widget();
    let canvas_widget = area.clone().upcast::<gtk4::Widget>();
    // **Nothing selected, so nothing is drawn over the sheet.** The strip and the
    // selection outline are interface over content — themed by design — and a probe that
    // landed on one would measure the theme twice over (the mark is the accent, the
    // strip's buttons are libadwaita's) instead of measuring the collage. The state is
    // the window's own: `open_document` clears the selection the same way.
    window.select(None);
    let mut painted: Vec<(String, support::Image)> = Vec::new();
    let mut geometry: Vec<(i32, i32, pixlay_core::PixelSize)> = Vec::new();
    for scheme in [adw::ColorScheme::ForceDark, adw::ColorScheme::ForceLight] {
        manager.set_color_scheme(scheme);
        // The repaint the scheme change causes is what this measures, and a snapshot
        // replays the last nodes: ask for the frame (`check_picker_theme`'s reason).
        window.queue_draw();
        window.pump(Duration::from_millis(300));
        // **A settled canvas, not merely a painted one** (S25). A colour-scheme change
        // moves the window's layout, and a layout asks the canvas for the grid its own
        // allocation needs (`EditorWindow::request_grid_for`): a snapshot taken while
        // that decode is in flight reads the *previous* window's bitmaps — measured
        // 2026-09-26: the first read had 760x570 bitmaps and the second the canvas's own
        // 307x230, which differ by ±1 per channel inside the sheet because the canvas
        // scales one and blits the other. That is a resampling difference between two
        // reads of two different decode states, and it says nothing about the theme; the
        // wait is what makes both reads the same measurement.
        //
        // **And the geometry each read was drawn at is part of the measurement.** The
        // canvas draws through the *widget's* size — `canvas::placement(grid, width,
        // height)` — while a snapshot replays the widgets' cached render nodes, which can
        // be a frame older than the allocation: measured in a chroot, the frame's canvas
        // was 236 px tall where the widget read 244, so the sheet came out 283x212 against
        // the 293x220 the placement promises. The reads' own geometry is therefore
        // recorded per scheme and compared below, and the probes are placed in the pair
        // both agreed on.
        let (width, height) = support::canvas_bitmaps(window);
        geometry.push((width, height, window.images().0));
        painted.push((format!("{scheme:?}"), support::snapshot(window)));
    }
    // The app's own scheme, not `Default`: dark is what `app.rs` sets at startup
    // (ruling 23), so the window is left where the application put it.
    manager.set_color_scheme(adw::ColorScheme::ForceDark);
    if geometry[0] != geometry[1] {
        failures.push(format!(
            "the canvas moved between the two colour schemes ({}x{} at {:?}, then {}x{} at \
             {:?}); the two reads are not one measurement",
            geometry[0].0,
            geometry[0].1,
            geometry[0].2,
            geometry[1].0,
            geometry[1].1,
            geometry[1].2,
        ));
    }
    let (dark, light) = (&painted[0].1, &painted[1].1);

    // The canvas the probes below are placed in: the settled pair above — the two reads
    // agree, or the failure just pushed says they are not one measurement.
    let (canvas_w, canvas_h, grid) = geometry[1];

    // The sheet's rectangle in the *window's* coordinates, which is what the two
    // snapshots are in: the sheet is where the canvas's own placement puts it, on the
    // canvas both reads agreed on.
    let placement = canvas::placement(grid, canvas_w, canvas_h);
    let origin = canvas_widget
        .compute_point(
            window.upcast_ref::<gtk4::Widget>(),
            &gtk4::graphene::Point::new(placement.origin.0 as f32, placement.origin.1 as f32),
        )
        .expect("the canvas is in the window");
    let (sheet_x, sheet_y) = (origin.x().round() as i32, origin.y().round() as i32);
    let (sheet_w, sheet_h) = (
        placement.width().round() as i32,
        placement.height().round() as i32,
    );

    // 1. **The frame around the sheet is the theme's.** It has to differ between the
    //    two schemes, or the canvas is painting over the theme's own background —
    //    exactly what the human saw as "the dark theme is not applied". Sampled in
    //    the middle of the margin the canvas leaves, which is outside the sheet for
    //    every layout and above the selection outline.
    let margin = ((placement.origin.0 / 2.0).round() as i32).max(1);
    let sample_y = (sheet_y + sheet_h / 2).max(margin);
    let (dark_edge, light_edge) = (
        support::pixel(dark, margin, sample_y),
        support::pixel(light, margin, sample_y),
    );
    if dark_edge == light_edge {
        failures.push(format!(
            "the canvas paints over the theme: the frame beside the sheet reads \
             {dark_edge:?} in both colour schemes"
        ));
    }

    // 2. **The scheme really is dark.** The app forces it at startup (ruling 23), and
    //    the frame the theme paints has to be dark, not merely different.
    if manager.is_dark() && luminance(dark_edge) > 128.0 {
        failures.push(format!(
            "the app is dark but the canvas frame is {dark_edge:?}, which is light"
        ));
    }

    // 3. **The sheet itself is content.** Sampled inside the sheet and away from its
    //    own edge, so the 1-px frame the canvas strokes around the sheet — drawn in the
    //    theme's own foreground colour *by design*, so a white collage on a dark pane
    //    still reads as a page — and the selection outline (the theme's accent, S24)
    //    are not what this compares. The collage's pixels are the export's pixels and
    //    must not depend on the desktop's appearance.
    // **The inset is a fraction of the sheet, not four pixels.** Four pixels was the
    // old value, and it assumed the snapped frame's sheet is exactly the placement's
    // rectangle: a frame drawn a moment before the window's last resize is a few pixels
    // narrower (measured in a chroot: 283x212 against 293x220, because the frame was
    // drawn at the previous allocation), which put the probe on the sheet's antialiased
    // edge — where the theme shows through *by design*, since the sheet is clipped to
    // its own rectangle. An eighth of the sheet is far from every edge and still content.
    let (inset_x, inset_y) = (sheet_w / 8, sheet_h / 8);
    let probes = [
        (sheet_x + inset_x, sheet_y + inset_y),
        (sheet_x + sheet_w / 2, sheet_y + sheet_h / 2),
        (sheet_x + sheet_w - inset_x, sheet_y + sheet_h - inset_y),
    ];
    for (x, y) in probes {
        let (dark_pixel, light_pixel) = (support::pixel(dark, x, y), support::pixel(light, x, y));
        if dark_pixel != light_pixel {
            failures.push(format!(
                "the sheet changed with the colour scheme at {x},{y}: {dark_pixel:?} vs \
                 {light_pixel:?} — the collage is content, not styling"
            ));
        }
    }
    eprintln!(
        "the canvas frame: {dark_edge:?} in dark, {light_edge:?} in light; the sheet \
         {sheet_w}x{sheet_h} at {sheet_x},{sheet_y} on a {canvas_w}x{canvas_h} canvas, \
         unchanged at all three probes (bitmaps {grid:?} in both schemes)",
    );
}

/// Perceived brightness of a colour, `0..=255`: the ITU-R BT.601 luma, which is
/// what "does this read as dark" means for a background.
fn luminance(rgb: [u8; 3]) -> f64 {
    0.299 * f64::from(rgb[0]) + 0.587 * f64::from(rgb[1]) + 0.114 * f64::from(rgb[2])
}

/// The about dialog takes its identity from the application, not from a second
/// copy of the string.
fn check_about(failures: &mut Vec<String>) {
    let about = app::about_dialog();
    if about.application_icon() != APP_ID {
        failures.push(format!(
            "the about dialog's icon is {:?}, not the app id",
            about.application_icon()
        ));
    }
    if about.version() != env!("CARGO_PKG_VERSION") {
        failures.push(format!(
            "the about dialog's version is {:?}, not the crate's",
            about.version()
        ));
    }
}

/// Untranslated strings are English, whether or not a language pack exists.
fn check_language(failures: &mut Vec<String>) {
    let cases: [(&str, Option<&str>); 3] = [
        ("no LANG at all", None),
        ("LANG=C", Some("C")),
        ("an unknown language", Some("xx_YY.UTF-8")),
    ];
    for (name, locale) in cases {
        let mut command = std::process::Command::new(
            std::env::current_exe().expect("the test binary's own path"),
        );
        command
            .arg("--exact")
            .arg("the_interface_meets_the_machine_checkable_hig")
            .arg("--nocapture")
            .env("PIXLAY_LANGUAGE_CHILD", "1");
        match locale {
            Some(locale) => {
                command.env("LANG", locale).env("LC_ALL", locale);
            }
            None => {
                command.env_remove("LANG").env_remove("LC_ALL");
            }
        }
        let output = command.output().expect("the child test runs");
        if !output.status.success() {
            failures.push(format!(
                "the interface is not usable with {name}:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
}

/// The child half of [`check_language`]: with the locale missing or unknown, the
/// window still builds and its strings are the English source strings.
fn check_english() {
    let application = support::app();
    let window = support::window(&application);
    assert_eq!(
        i18n::gettext("Add photos…"),
        "Add photos…",
        "an untranslated msgid has to come back as itself"
    );
    // The window is the editor (S22), and its title is the document's own source
    // string.
    assert_eq!(
        window.title().map(|title| title.to_string()).as_deref(),
        Some("Untitled collage"),
        "the window title must be the document's English source string"
    );
    // The copy on screen is the English source string: every visible label of the
    // window, button labels included.
    let mut visible: Vec<String> = Vec::new();
    for widget in support::descendants(window.upcast_ref::<gtk4::Widget>()) {
        if let Some(button) = widget.downcast_ref::<gtk4::Button>()
            && let Some(label) = button.label()
        {
            visible.push(label.to_string());
        }
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            visible.push(label.label().to_string());
        }
    }
    // The window's own copy. `Sheet size` and `Resolution` were the export form's rows
    // before S12c/S12d collapsed them into one quality option; `Quality` and `Format`
    // left the window in S13, when ruling 18 removed the pane that held them — S15's
    // `Export…` dialog is where they come back; `Pick photos`, `Nothing picked yet` and
    // `Next (0)` left with the picker in S22.
    for expected in ["Untitled collage", "Export", "1"] {
        assert!(
            visible.iter().any(|label| label == expected),
            "the interface should read English; {expected:?} is missing from {visible:?}"
        );
    }
    // And no msgid-shaped placeholder was left in a string that is supposed to be
    // finished copy (`{date}` in a label is the token syntax being taught to the
    // user, which is why this looks for the `{}` form only).
    assert!(
        !visible.iter().any(|label| label.contains("{}")),
        "an unfilled placeholder leaked into the copy: {visible:?}"
    );
}
