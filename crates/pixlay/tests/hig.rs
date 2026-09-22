//! S7's machine-checkable GNOME HIG subset, and the interface's language.
//!
//! One test: GTK lives on one thread (see `support`). What is checked here is the
//! part of `docs/HIG-REVIEW.md` section 1 that a machine can answer — the
//! accelerator table against `reference/keyboard`, accessible names
//! (`guidelines/accessibility`), the picker's selection mode
//! (`patterns/containers/selection-mode`, which applies from S13 on), the adaptive
//! minimum (`guidelines/adaptive`), the two colour schemes
//! (`guidelines/ui-styling`), the about dialog's metadata — and the step's own
//! criterion that the interface is English when the locale is missing, `C`, or
//! unknown.
//!
//! **Both stages are checked, in the order a user meets them**: the window opens
//! on the picker (S13), so the picker's own criteria run first and the editor's
//! (canvas, colour schemes) run after a project is opened, which is what puts the
//! editor page on screen.

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;

use pixlay::window::Stage;
use pixlay::{APP_ID, app, canvas, i18n, window::EditorWindow};

/// The accelerator combinations HIG `reference/keyboard` requires *for the
/// features this product has*: quit, close, open, save, save as, undo, redo, the
/// shortcuts dialog, and a new item. Print, send, preferences, help and the
/// utility pane's `F9` belong to features v1 does not have — `F9` left with the
/// pane in S13 (ruling 18).
const REQUIRED: [&str; 9] = [
    "<Control>q",
    "<Control>w",
    "<Control>o",
    "<Control>s",
    "<Control><Shift>s",
    "<Control>z",
    "<Control><Shift>z",
    "<Control>question",
    "<Control>n",
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

    // ---- stage 1: the picker ------------------------------------------------
    check_shortcuts(&application, &window, &mut failures);
    check_accessible_names(&window, &mut failures);
    check_picker(&window, &mut failures);
    check_picker_minimum(&window, &mut failures);

    // ---- stage 2: the editor ------------------------------------------------
    // A project is what puts the editor's page on screen (and what the canvas
    // needs to be allocated at all), so the canvas checks come after this.
    let project = support::verify_project();
    window
        .open_path(&project)
        .expect("the verification project opens");
    assert_eq!(
        window.stage(),
        Stage::Editor,
        "opening a collage shows the editor's stage"
    );
    assert!(
        window.wait_for_idle(support::WAIT),
        "the open decode finished"
    );
    check_accessible_names(&window, &mut failures);
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
                if name.is_empty() {
                    key.push_str(rest);
                } else {
                    modifiers.push(name.to_ascii_lowercase());
                }
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
    let root = window.clone().upcast::<gtk4::Widget>();
    for widget in support::descendants(&root) {
        if !is_interactive(&widget) {
            continue;
        }
        if !has_accessible_name(&widget) {
            failures.push(format!(
                "{} has no accessible name (inside {})",
                widget.type_().name(),
                ancestors(&widget),
            ));
        }
    }
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

/// An accessible name: either set explicitly — on the control or on the compound
/// widget that owns it, since a `GtkSpinButton`'s inner entry is announced by the
/// spin button — or derived by GTK from the control's own text, which is how a
/// `GtkButton` carrying a `GtkLabel` gets its name.
///
/// Both halves matter. The first is what this app is responsible for and what most
/// controls here satisfy; the second is the platform behaviour HIG `guidelines/
/// accessibility` leans on ("GTK provides default accessible descriptions for many
/// UI elements"), without which the check would fail on GTK's own internals, such
/// as the button `AdwBanner` creates from its label.
fn has_accessible_name(widget: &gtk4::Widget) -> bool {
    let mut current = Some(widget.clone());
    for _ in 0..3 {
        let Some(candidate) = current else {
            break;
        };
        if gtk4::test_accessible_has_property(&candidate, gtk4::AccessibleProperty::Label) {
            return true;
        }
        current = candidate.parent();
    }
    support::descendants(widget)
        .iter()
        .filter_map(|child| child.downcast_ref::<gtk4::Label>())
        .any(|label| !label.label().is_empty())
}

/// The picker is a collection view in selection mode (S13), arranged as the
/// 2026-09-22 ruling fixed it (S13b).
///
/// HIG `patterns/containers/selection-mode`, which the plan's review turned from
/// "not applicable" into a criteria row: a grid whose model is a real
/// multi-selection, a picked cell shown by a highlight — the one part of the page
/// this product deviates on, recorded in `docs/HIG-REVIEW.md` §3 — and the batch
/// action in the header, the Next button, carrying the count and insensitive below
/// the floor of two.
///
/// The arrangement is `guidelines/adaptive`'s half: the preview above, the
/// thumbnails below it, the picked list down the right edge, which is checked as
/// geometry rather than as a widget tree.
fn check_picker(window: &EditorWindow, failures: &mut Vec<String>) {
    let picker = window.picker().expect("the window has a picker stage");
    // The fixture folder, so the check does not depend on what `~/Pictures` holds
    // on the machine running the tests.
    picker.open_folder(window, &support::fixtures().join("photos"));
    window.pump(Duration::from_millis(300));

    let model = picker.grid().model();
    let multi = model
        .as_ref()
        .and_then(|model| model.downcast_ref::<gtk4::MultiSelection>());
    if multi.is_none() {
        failures.push(format!(
            "the picker's grid is not backed by a GtkMultiSelection ({:?})",
            model.as_ref().map(|model| model.type_().name().to_string())
        ));
    }

    // The check mark S13 used is gone, and the highlight is what replaced it
    // (the 2026-09-22 ruling).
    let checks: Vec<gtk4::Widget> =
        support::descendants(picker.grid().upcast_ref::<gtk4::Widget>())
            .into_iter()
            .filter(|widget| {
                widget.is::<gtk4::CheckButton>() && widget.has_css_class("selection-mode")
            })
            .collect();
    if !checks.is_empty() {
        failures.push(format!(
            "{} cell(s) still carry a .selection-mode check button, which the ruling replaced \
             with the highlight",
            checks.len()
        ));
    }
    let cells = support::descendants(picker.grid().upcast_ref::<gtk4::Widget>())
        .into_iter()
        .filter(|widget| widget.has_css_class("picker-cell"))
        .count();
    if cells == 0 {
        failures.push("no grid cell carries the .picker-cell class".to_string());
    }

    // The count is the button's own text, and the floor of the product's 2–9 rule
    // turns it off rather than letting Next open an empty collage.
    let next = picker.next_button();
    let label = next
        .label()
        .map(|label| label.to_string())
        .unwrap_or_default();
    if !label.contains('0') {
        failures.push(format!(
            "Next does not carry the count of picked photos (label is {label:?})"
        ));
    }
    if next.is_sensitive() {
        failures.push("Next is sensitive with nothing picked".to_string());
    }
    if picker.len() < 2 {
        failures.push(format!(
            "the fixture folder has {} photos, too few to check the picker with",
            picker.len()
        ));
        return;
    }
    picker.toggle(window, 0);
    if next.is_sensitive() {
        failures.push("Next is sensitive with one photo picked".to_string());
    }
    let picked_cell = picker
        .cell_widget(0)
        .map(|cell| cell.has_css_class("picked"));
    if picked_cell != Some(true) {
        failures.push(format!(
            "a picked cell does not carry the highlight class ({picked_cell:?})"
        ));
    }
    picker.toggle(window, 1);
    if !next.is_sensitive() {
        failures.push("Next is insensitive with two photos picked".to_string());
    }
    let label = next
        .label()
        .map(|label| label.to_string())
        .unwrap_or_default();
    if !label.contains('2') {
        failures.push(format!(
            "Next does not show two picked photos (label is {label:?})"
        ));
    }
    if picker.picked_list().first_child().is_none() {
        failures.push("the picked list is empty with two photos picked".to_string());
    }
    picker.clear_selection();
    if picker.selected_count() != 0 {
        failures.push("Esc-equivalent clearing left photos picked".to_string());
    }
    if picker
        .cell_widget(0)
        .is_some_and(|cell| cell.has_css_class("picked"))
    {
        failures.push("clearing the pick left a cell highlighted".to_string());
    }
}

/// At the minimum size the picker's own controls are all still usable
/// (HIG `guidelines/adaptive`).
///
/// What the row used to assert — the utility pane — left with the pane in S13;
/// what takes its place is the stage the window actually opens on.
fn check_picker_minimum(window: &EditorWindow, failures: &mut Vec<String>) {
    let minimum = window.size_request();
    window.set_default_size(minimum.0, minimum.1);
    window.pump(Duration::from_millis(500));
    let picker = window.picker().expect("the window has a picker stage");
    for (name, widget) in [
        ("the photo grid", picker.grid().upcast::<gtk4::Widget>()),
        (
            "the preview pane",
            picker.preview_widget().upcast::<gtk4::Widget>(),
        ),
        (
            "the picked list",
            picker.picked_list().upcast::<gtk4::Widget>(),
        ),
    ] {
        if widget.width() <= 0 || widget.height() <= 0 {
            failures.push(format!(
                "at the minimum size {}x{}, {name} is not allocated ({}x{})",
                minimum.0,
                minimum.1,
                widget.width(),
                widget.height()
            ));
        }
    }

    // And the arrangement the 2026-09-22 ruling fixed, as geometry: the
    // thumbnails below the preview, the picked list to the right of it.
    let root = picker.root().upcast::<gtk4::Widget>();
    let grid = picker.grid().upcast::<gtk4::Widget>();
    let preview = picker.preview_widget().upcast::<gtk4::Widget>();
    let list = picker.picked_list().upcast::<gtk4::Widget>();
    let corner = |widget: &gtk4::Widget, x: f32, y: f32| {
        widget
            .compute_point(&root, &gtk4::graphene::Point::new(x, y))
            .map(|point| (point.x(), point.y()))
    };
    let (grid_top, preview_bottom) = (
        corner(&grid, 0.0, 0.0).map(|(_, y)| y),
        corner(&preview, 0.0, preview.height() as f32).map(|(_, y)| y),
    );
    if let (Some(grid_top), Some(preview_bottom)) = (grid_top, preview_bottom)
        && grid_top < preview_bottom
    {
        failures.push(format!(
            "the thumbnails ({grid_top:.0}) are not below the preview ({preview_bottom:.0})"
        ));
    }
    let (list_left, preview_right) = (
        corner(&list, 0.0, 0.0).map(|(x, _)| x),
        corner(&preview, preview.width() as f32, 0.0).map(|(x, _)| x),
    );
    if let (Some(list_left), Some(preview_right)) = (list_left, preview_right)
        && list_left < preview_right
    {
        failures.push(format!(
            "the picked list ({list_left:.0}) is not to the right of the pane ({preview_right:.0})"
        ));
    }
}

/// At the minimum size the sheet is still drawn in full inside the canvas
/// (HIG `guidelines/adaptive`) — the editor's stage of the same rule.
fn check_editor_minimum(window: &EditorWindow, failures: &mut Vec<String>) {
    let minimum = window.size_request();
    window.set_default_size(minimum.0, minimum.1);
    window.pump(Duration::from_millis(500));

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
}

/// The app starts under both colour schemes, and the canvas pixels do not depend
/// on the style: the sheet is content, not styling (`AGENTS.md`).
fn check_colour_schemes(window: &EditorWindow, failures: &mut Vec<String>) {
    let manager = adw::StyleManager::default();
    let area = window.canvas_widget();
    let mut painted: Vec<(String, support::Image)> = Vec::new();
    for scheme in [adw::ColorScheme::ForceDark, adw::ColorScheme::ForceLight] {
        manager.set_color_scheme(scheme);
        window.pump(Duration::from_millis(300));
        painted.push((format!("{scheme:?}"), support::snapshot(&area)));
    }
    manager.set_color_scheme(adw::ColorScheme::Default);
    let (first, second) = (&painted[0].1, &painted[1].1);
    let difference = support::rmse(first, second);
    if difference > 0.0 {
        failures.push(format!(
            "the canvas changed with the colour scheme (RMSE {difference:.3})"
        ));
    }
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
        i18n::gettext("Pick photos"),
        "Pick photos",
        "an untranslated msgid has to come back as itself"
    );
    // The window opens on the picker's stage (S13), and its title is that stage's
    // own source string.
    assert_eq!(
        window.title().map(|title| title.to_string()).as_deref(),
        Some("Pick photos"),
        "the window title must be the picker's English source string"
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
    // The picker's own copy, and the editor's (whose header is built whether or
    // not its page is the visible one). `Sheet size` and `Resolution` were the
    // export form's rows before S12c/S12d collapsed them into one quality option;
    // `Quality` and `Format` left the window in S13, when ruling 18 removed the
    // pane that held them — S15's `Export…` dialog is where they come back.
    for expected in [
        "Pick photos",
        "Nothing picked yet",
        "Next (0)",
        "Export",
        "Export…",
    ] {
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
