//! S7's machine-checkable GNOME HIG subset, and the interface's language.
//!
//! One test: GTK lives on one thread (see `support`). What is checked here is the
//! part of `docs/HIG-REVIEW.md` section 1 that a machine can answer — the
//! accelerator table against `reference/keyboard`, accessible names
//! (`guidelines/accessibility`), the adaptive minimum (`guidelines/adaptive`), the
//! two colour schemes (`guidelines/ui-styling`), the about dialog's metadata — and
//! the step's own criterion that the interface is English when the locale is
//! missing, `C`, or unknown.

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;

use pixlay::{APP_ID, app, canvas, i18n, window::EditorWindow};

/// The accelerator combinations HIG `reference/keyboard` requires *for the
/// features this product has*: quit, close, open, save, save as, undo, redo, the
/// shortcuts dialog, a new item, and `F9` for the utility pane
/// (`patterns/containers/utility-panes`). Print, send, preferences and help belong
/// to features v1 does not have.
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
    "F9",
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

    check_shortcuts(&application, &window, &mut failures);
    check_accessible_names(&window, &mut failures);
    check_adaptive_minimum(&window, &mut failures);
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

/// At the minimum size the canvas is still drawn in full and the pane's controls
/// are all still there (HIG `guidelines/adaptive`).
fn check_adaptive_minimum(window: &EditorWindow, failures: &mut Vec<String>) {
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
    let sidebar = window
        .sidebar()
        .expect("the window has a utility pane")
        .root
        .clone();
    if sidebar.width() <= 0 || sidebar.height() <= 0 {
        failures.push("the utility pane has no size at the minimum window size".to_string());
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
        i18n::gettext("Untitled collage"),
        "Untitled collage",
        "an untranslated msgid has to come back as itself"
    );
    assert_eq!(
        window.title().map(|title| title.to_string()).as_deref(),
        Some("Untitled collage"),
        "the window title must be the English source string"
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
    for expected in ["Export", "Sheet size", "Template"] {
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
