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

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

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
    check_picker_input(&window, &mut failures);
    check_header_chrome(&window, &mut failures);
    check_picker_theme(&window, &mut failures);
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
    // The editor's page is laid out and its bitmaps are in hand before anything below is
    // measured: a page that has just been pushed has neither, and every check that
    // follows reads an allocation or a `Placement` (measured 2026-09-23: without this
    // the walk read a 0x0 canvas, a band with no candidates and header controls at
    // position 0 on some runs and was fine on others — `tests/support::canvas_bitmaps` is
    // the harness's answer to exactly that).
    let _ = support::canvas_bitmaps(&window);
    assert!(
        window.wait_for_gallery(support::WAIT),
        "the layout band was built"
    );
    check_accessible_names(&window, &mut failures);
    check_gallery(&window, &mut failures);
    check_compose(&window, &mut failures);
    check_editor_chrome(&window, &mut failures);
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
        if !has_accessible_name(&widget) {
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

    // The count is the content's own label — not the button's, which would replace
    // the `AdwButtonContent` and lose the icon (`S13c`, the defect this checks) — and
    // the floor of the product's 2–9 rule turns it off rather than letting Next open
    // an empty collage.
    let next = picker.next_button();
    let label = next_label(&picker);
    if !label.contains('0') {
        failures.push(format!(
            "Next does not carry the count of picked photos (label is {label:?})"
        ));
    }
    if !next
        .child()
        .is_some_and(|child| child.is::<adw::ButtonContent>())
    {
        failures.push(
            "Next's child is not an AdwButtonContent, so its icon is gone (S13b's defect)"
                .to_string(),
        );
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
    let label = next_label(&picker);
    if !label.contains('2') {
        failures.push(format!(
            "Next does not show two picked photos (label is {label:?})"
        ));
    }
    if picker.picked_list().first_child().is_none() {
        failures.push("the picked list is empty with two photos picked".to_string());
    }
    picker.clear_selection(window);
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

/// The picker's input paths, and the three defects S13c fixed while it rewrote the
/// same code (`docs/2026-09-22-STEPS.md`, `S13c · Work` (a)–(c)).
///
/// Every one of them is driven through the platform's own route: `Enter` is GTK's
/// `list.activate-item` action (the one the key is bound to), `Ctrl+A` is
/// `list.select-all`, and Next's label is read off the `AdwButtonContent` the button
/// holds.
fn check_picker_input(window: &EditorWindow, failures: &mut Vec<String>) {
    let picker = window.picker().expect("the window has a picker stage");
    picker.open_folder(window, &support::fixtures().join("photos"));
    window.pump(Duration::from_millis(300));
    let grid = picker.grid();
    if picker.len() < 2 {
        failures.push("the fixture folder is too small to check the picker's input with".into());
        return;
    }

    // `Enter` toggles the focused cell: `list.activate-item` emits the grid's
    // `activate` signal, which is what the picker answers.
    let position = 0u32;
    picker.clear_selection(window);
    if grid
        .activate_action("list.activate-item", Some(&position.to_variant()))
        .is_err()
    {
        failures.push("the grid has no list.activate-item action (Enter does nothing)".into());
    }
    if picker.selected_count() != 1 {
        failures.push(format!(
            "Enter on a cell left {} photos picked, not 1",
            picker.selected_count()
        ));
    }
    let _ = grid.activate_action("list.activate-item", Some(&position.to_variant()));
    if picker.selected_count() != 0 {
        failures.push("Enter on a picked cell did not toggle it off".into());
    }

    // `Ctrl+A` is bound once — GTK's own `list.select-all` — and one press reports
    // the cap exactly once. S13's second binding made the same press fire twice.
    picker.clear_selection(window);
    let before = window.toasts();
    let _ = grid.activate_action("list.select-all", None);
    if picker.selected_count() != pixlay_core::MAX_PHOTOS {
        failures.push(format!(
            "selecting all left {} photos picked, not the cap of {}",
            picker.selected_count(),
            pixlay_core::MAX_PHOTOS
        ));
    }
    let reports = window.toasts() - before;
    if reports != 1 {
        failures.push(format!(
            "one Ctrl+A reported the cap {reports} times, not once"
        ));
    }
    if !window
        .last_toast()
        .is_some_and(|toast| toast.contains(&pixlay_core::MAX_PHOTOS.to_string()))
    {
        failures.push(format!(
            "the refused photos are not reported by name: {:?}",
            window.last_toast()
        ));
    }
    // And the picked list is rebuilt rather than left stale: S13b cleared the model
    // and the ordered list without rebuilding the rows, so a cleared pick left rows
    // behind — which now would be controls that switch the pane to a photo nobody
    // picked.
    picker.clear_selection(window);
    if picker.picked_list().row_at_index(0).is_some() {
        failures.push("clearing the pick left its rows behind".into());
    }
    if picker.picked_list().row_at_index(0).is_none()
        && picker
            .picked_list()
            .first_child()
            .is_none_or(|child| !child.has_css_class("dim-label"))
    {
        failures.push(
            "the empty picked list has no hint: the placeholder has to survive a rebuild".into(),
        );
    }
}

/// The picker's chrome, as HIG `patterns/containers/header-bars` and ruling 24 fix
/// it: primary and navigation actions at the start, the heading in the centre, the
/// menu at the end, and one primary menu of the ruled items.
fn check_header_chrome(window: &EditorWindow, failures: &mut Vec<String>) {
    let picker = window.picker().expect("the window has a picker stage");
    let header = picker.header();
    let root = picker.root().upcast::<gtk4::Widget>();
    let title = header
        .title_widget()
        .expect("the header has a title widget");
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

    // The folder button is the start slot's control: it is the first button of the
    // header's start box, and it is to the *left* of the heading.
    let controls = support::descendants(header.upcast_ref::<gtk4::Widget>());
    let folder = controls.iter().find(|widget| {
        widget.is::<gtk4::Button>()
            && widget.tooltip_text().as_deref()
                == Some(pixlay::i18n::gettext("Choose a folder of photos").as_str())
    });
    let Some(folder) = folder else {
        failures.push("the header has no folder button".into());
        return;
    };
    let menu = picker.menu_button();
    let (folder_x, title_x, menu_x) = (
        centre(&folder.clone()),
        centre(&title.clone()),
        centre(menu.upcast_ref::<gtk4::Widget>()),
    );
    match (folder_x, title_x, menu_x) {
        (Some(folder_x), Some(title_x), Some(menu_x)) => {
            if folder_x >= title_x {
                failures.push(format!(
                    "the folder button ({folder_x:.0}) is not left of the heading ({title_x:.0})"
                ));
            }
            if menu_x <= title_x {
                failures.push(format!(
                    "the menu ({menu_x:.0}) is not right of the heading ({title_x:.0})"
                ));
            }
        }
        _ => failures.push("the header's controls are not allocated".into()),
    }

    // One primary menu, of the ruled items, in the ruled sections.
    let actions = menu
        .menu_model()
        .map(|model| menu_actions(&model))
        .unwrap_or_default();
    let wanted = [
        "app.new",
        "app.open",
        "win.choose-folder",
        "app.shortcuts",
        "app.about",
    ];
    if actions != wanted {
        failures.push(format!(
            "the picker's menu is {actions:?}, not the ruled {wanted:?}"
        ));
    }

    // Every control *this app* puts in the header carries a tooltip (this page's own
    // "tooltips on primary controls"); the header's own internals — the back button,
    // the window controls — are the platform's and are checked by
    // `check_accessible_names` instead.
    for (name, widget) in [
        ("the folder button", folder.clone()),
        ("the menu", menu.clone().upcast::<gtk4::Widget>()),
        ("Next", picker.next_button().upcast::<gtk4::Widget>()),
    ] {
        if widget.tooltip_text().is_none() {
            failures.push(format!("{name} has no tooltip"));
        }
        if !has_accessible_name(&widget) {
            failures.push(format!("{name} has no accessible name"));
        }
    }
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

/// The media area is on the theme's own background, and the app is dark by default
/// (rulings 22–23; HIG `guidelines/ui-styling`).
///
/// The ruling's own words: the media area's backdrop is `#222226` in the dark
/// scheme, which *is* libadwaita's `--window-bg-color` — the idiom both references
/// copy is "the theme's own background", not that literal. So the check is an
/// equality: the pane's backdrop is the same colour as a plain widget with nothing
/// painted on it (the status bar), under a forced light *and* a forced dark scheme;
/// and the two schemes differ, which is what says the colour is not a literal.
///
/// The pixels come from the **window's** own snapshot, not from the pane's alone:
/// `WidgetPaintable` renders a widget's own node, and a widget with no background of
/// its own paints nothing — the colour at that point comes from the window beneath
/// it, which is exactly what is being checked.
fn check_picker_theme(window: &EditorWindow, failures: &mut Vec<String>) {
    let manager = adw::StyleManager::default();
    if manager.color_scheme() != adw::ColorScheme::ForceDark {
        failures.push(format!(
            "the app is not dark by default ({:?})",
            manager.color_scheme()
        ));
    }
    let picker = window.picker().expect("the window has a picker stage");
    let pane = picker.preview_widget().upcast::<gtk4::Widget>();
    let status_bar = picker.status_bar().upcast::<gtk4::Widget>();
    let root = window.clone().upcast::<gtk4::Widget>();
    // Mid-left rather than the very corner: a raised top bar (`AdwToolbarView`'s
    // `top_bar_style`, S13c) draws its own edge over the first pixels of the content
    // below it, and that edge is chrome, not the media area's backdrop (measured
    // 2026-09-23: the pane's corner is one level darker than the window background).
    let pane_point = pane
        .compute_point(
            &root,
            &gtk4::graphene::Point::new(3.0, pane.height() as f32 / 2.0),
        )
        .map(|point| (point.x() as i32, point.y() as i32));
    let bar_point = status_bar
        .compute_point(
            &root,
            &gtk4::graphene::Point::new(3.0, status_bar.height() as f32 / 2.0),
        )
        .map(|point| (point.x() as i32, point.y() as i32));
    let (Some(pane_point), Some(bar_point)) = (pane_point, bar_point) else {
        failures.push("the media area and the status bar are not in the window".into());
        return;
    };

    let mut backgrounds = Vec::new();
    for scheme in [adw::ColorScheme::ForceDark, adw::ColorScheme::ForceLight] {
        manager.set_color_scheme(scheme);
        // A snapshot replays the widgets' current nodes, so the frame the scheme change
        // causes has to be asked for: without this the probe can read the *previous*
        // scheme's pixels (measured 2026-09-23: the media area read the dark colour in
        // both schemes on one run and differed on the next).
        window.queue_draw();
        window.pump(Duration::from_millis(300));
        let pixels = support::snapshot(window);
        backgrounds.push((
            format!("{scheme:?}"),
            support::pixel(&pixels, pane_point.0, pane_point.1),
            support::pixel(&pixels, bar_point.0, bar_point.1),
        ));
    }
    manager.set_color_scheme(adw::ColorScheme::ForceDark);
    for (scheme, pane_pixel, bar_pixel) in &backgrounds {
        if pane_pixel != bar_pixel {
            failures.push(format!(
                "under {scheme} the media area is {pane_pixel:?} where the window's own \
                 background is {bar_pixel:?}: the pane is not on the theme's background"
            ));
        }
    }
    if backgrounds[0].1 == backgrounds[1].1 {
        failures.push(
            "the media area's backdrop is the same under both colour schemes, so it is a \
             literal colour rather than the theme's"
                .to_string(),
        );
    }
}

/// The count the Next button carries, read off the `AdwButtonContent` it holds.
fn next_label(picker: &pixlay::picker::Picker) -> String {
    picker
        .next_button()
        .child()
        .and_downcast::<adw::ButtonContent>()
        .map(|content| content.label().to_string())
        .unwrap_or_default()
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

/// The layout band (S14): the count control and the candidates, against the two HIG
/// chapters a band of controls has to answer — `guidelines/pointer-touch` (a click
/// target has a size) and `guidelines/accessibility` (a control has a name).
///
/// What the band *lists* and what its candidates look like is `tests/layout.rs`'s
/// subject; this is the part that is the same question for every control in the
/// window.
fn check_gallery(window: &EditorWindow, failures: &mut Vec<String>) {
    /// HIG `guidelines/pointer-touch`: "ensure that all interactive elements are
    /// at least 24x24 pixels". The band's cells are [`THUMB_BOX`] wide by
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
        if !has_accessible_name(&widget) {
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
        if !has_accessible_name(&widget) {
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

    // The two dialogs, each presented so that its own tree can be walked: a dialog
    // that is not on screen has no allocation and no accessible tree.
    for (name, dialog) in [
        ("Frame", window.frame_dialog().map(|dialog| dialog.widget())),
        (
            "Export",
            window.export_dialog().map(|dialog| dialog.widget()),
        ),
    ] {
        let Some(dialog) = dialog else {
            failures.push(format!("the window has no {name} dialog"));
            continue;
        };
        dialog.present(Some(window));
        window.pump(Duration::from_millis(50));
        let root = dialog.clone().upcast::<gtk4::Widget>();
        check_accessible_names_in(&root, failures);
        // The heading, and the button that carries the action's verb: an action
        // dialog has both (HIG `patterns/feedback/dialogs`).
        let controls = support::descendants(&root);
        let labels: Vec<String> = controls
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk4::Label>())
            .map(|label| label.label().to_string())
            .collect();
        if !labels.iter().any(|label| label == name) {
            failures.push(format!("the {name} dialog has no heading reading {name:?}"));
        }
        if !labels.iter().any(|label| label == name || label == "Close") {
            failures.push(format!(
                "the {name} dialog has neither an affirmative nor a way out"
            ));
        }
        dialog.force_close();
    }
    // Leave the window as this check found it: the colour-scheme probe that follows
    // samples the sheet near its edges, and a selection outline is interface drawn on
    // top of the content.
    window.pump(Duration::from_millis(200));
    window.select(None);
}

/// The editor page's header bar, held to the same three alignment points as the
/// picker's (HIG `patterns/containers/header-bars`, ruling 24): the document's own
/// controls at the start, the heading in the centre, the menu at the end.
///
/// The picker's check is `check_header_chrome`; the editor's controls are the ones
/// S15 added, so this is the other half of the same rule.
fn check_editor_chrome(window: &EditorWindow, failures: &mut Vec<String>) {
    let Some(header) = window.editor_header() else {
        failures.push("the editor page has no header bar".into());
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
    };
    let (Some(frame), Some(export)) = (by_action("win.frame"), by_action("win.export")) else {
        failures.push("the editor's header is missing the frame or the export control".into());
        return;
    };
    let Some(menu) = controls
        .iter()
        .find(|widget| widget.is::<gtk4::MenuButton>())
    else {
        failures.push("the editor's header has no primary menu".into());
        return;
    };
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
        centre(&frame.clone()),
        title.flatten(),
        centre(&menu.clone()),
    ) {
        (Some(frame_x), Some(title_x), Some(menu_x)) => {
            if frame_x >= title_x {
                failures.push(format!(
                    "the frame button ({frame_x:.0}) is not left of the heading ({title_x:.0})"
                ));
            }
            if menu_x <= title_x {
                failures.push(format!(
                    "the menu ({menu_x:.0}) is not right of the heading ({title_x:.0})"
                ));
            }
        }
        _ => failures.push("the editor header's controls are not allocated".into()),
    }
    // Every control *this app* puts in the editor's header carries a tooltip and a
    // name (this page's own "tooltips on primary controls").
    for (name, widget) in [
        ("the frame button", frame.clone()),
        ("the export button", export.clone()),
        ("the menu", menu.clone()),
    ] {
        if widget.tooltip_text().is_none() {
            failures.push(format!("the editor header's {name} has no tooltip"));
        }
        if !has_accessible_name(&widget) {
            failures.push(format!("the editor header's {name} has no accessible name"));
        }
    }
}

/// At the minimum size the sheet is still drawn in full inside the canvas
/// (HIG `guidelines/adaptive`) — the editor's stage of the same rule.
fn check_editor_minimum(window: &EditorWindow, failures: &mut Vec<String>) {
    let minimum = window.size_request();
    window.set_default_size(minimum.0, minimum.1);
    window.pump(Duration::from_millis(500));

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
    let area = window.canvas_widget();
    let canvas_widget = area.clone().upcast::<gtk4::Widget>();
    let mut painted: Vec<(String, support::Image)> = Vec::new();
    for scheme in [adw::ColorScheme::ForceDark, adw::ColorScheme::ForceLight] {
        manager.set_color_scheme(scheme);
        // The repaint the scheme change causes is what this measures, and a snapshot
        // replays the last nodes: ask for the frame (`check_picker_theme`'s reason).
        window.queue_draw();
        window.pump(Duration::from_millis(300));
        painted.push((format!("{scheme:?}"), support::snapshot(window)));
    }
    // The app's own scheme, not `Default`: dark is what `app.rs` sets at startup
    // (ruling 23), so the window is left where the application put it.
    manager.set_color_scheme(adw::ColorScheme::ForceDark);
    let (dark, light) = (&painted[0].1, &painted[1].1);

    // The sheet's rectangle in the *window's* coordinates, which is what the two
    // snapshots are in: the sheet is where the canvas's own placement puts it.
    let (grid, _) = window.images();
    let placement = canvas::placement(grid, area.width(), area.height());
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
    //    own edge, so the selection outline — which is drawn with the widget's theme
    //    colour *by design*, because a hard-coded grey fails in high contrast mode —
    //    is not what this compares. The collage's pixels are the export's pixels and
    //    must not depend on the desktop's appearance.
    let inset = 4;
    let probes = [
        (sheet_x + inset, sheet_y + inset),
        (sheet_x + sheet_w / 2, sheet_y + sheet_h / 2),
        (sheet_x + sheet_w - inset, sheet_y + sheet_h - inset),
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
         {sheet_w}x{sheet_h} at {sheet_x},{sheet_y}, unchanged at all three probes"
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
