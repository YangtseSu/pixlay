//! The application: identity, actions, accelerators and the two dialogs HIG
//! asks every app for.
//!
//! The window actions live on the window (`window.rs`, group `win`) and the
//! application-wide ones here (`app`), so that `Ctrl+W` and `Ctrl+Q` mean what
//! they mean everywhere. The accelerator table is one constant —
//! [`ACCELERATORS`] — used by `install` and by the shortcuts dialog, so the
//! dialog, the bindings and the tests cannot drift apart.
//!
//! HIG `reference/keyboard`: the required set for an app with files and an undo
//! stack is `Ctrl+Q` / `Ctrl+W` / `Ctrl+O` / `Ctrl+S` / `Shift+Ctrl+S` / `Ctrl+Z` /
//! `Shift+Ctrl+Z` / `Ctrl+?`, plus `F9` for a utility pane (`patterns/containers/utility-panes`)
//! and `Ctrl+N` for a new item. Everything else on that page belongs to features
//! this product does not have (print, send, preferences, help), and the
//! system-reserved combinations (`Alt+*`, `Super+*`, `Ctrl+Alt+*`) are used
//! nowhere — a test asserts both halves.

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::i18n::gettext;
use crate::window::EditorWindow;

/// Every accelerator this app binds, as `(detailed action name, accelerator)`.
///
/// Machine-readable on purpose: the tests read it to check the HIG requirements
/// and the system-reserved set, and the shortcuts dialog reads the same names.
pub const ACCELERATORS: &[(&str, &str)] = &[
    ("app.new", "<Control>n"),
    ("app.open", "<Control>o"),
    ("app.shortcuts", "<Control>question"),
    ("app.quit", "<Control>q"),
    ("win.save", "<Control>s"),
    ("win.save-as", "<Control><Shift>s"),
    ("win.close", "<Control>w"),
    ("win.export", "<Control>e"),
    ("win.add-photo", "<Control>i"),
    ("win.undo", "<Control>z"),
    ("win.redo", "<Control><Shift>z"),
    ("win.reset-framing", "<Control>0"),
    ("win.toggle-sidebar", "F9"),
];

/// How the shortcuts dialog groups them (HIG `reference/keyboard`, "Sections").
pub const SHORTCUT_SECTIONS: &[(&str, &[&str])] = &[
    (
        "General",
        &[
            "app.shortcuts",
            "win.toggle-sidebar",
            "win.close",
            "app.quit",
        ],
    ),
    (
        "Collage",
        &[
            "app.new",
            "app.open",
            "win.save",
            "win.save-as",
            "win.export",
            "win.add-photo",
        ],
    ),
    ("Editing", &["win.undo", "win.redo", "win.reset-framing"]),
];

/// What a shortcut is called in the dialog.
///
/// A `match` on the action rather than a table of `(action, title)` pairs,
/// because the titles are user-visible strings: keeping the `gettext` call
/// beside the name is what lets `xgettext` see it. An action with no arm gets an
/// empty title, which a test catches.
pub fn shortcut_title(action: &str) -> String {
    match action {
        "app.new" => gettext("New collage"),
        "app.open" => gettext("Open…"),
        "app.shortcuts" => gettext("Keyboard shortcuts"),
        "app.quit" => gettext("Quit Pixlay"),
        "win.save" => gettext("Save"),
        "win.save-as" => gettext("Save as…"),
        "win.close" => gettext("Close the window"),
        "win.export" => gettext("Export the collage"),
        "win.add-photo" => gettext("Insert a photo"),
        "win.undo" => gettext("Undo"),
        "win.redo" => gettext("Redo"),
        "win.reset-framing" => gettext("Reset the framing"),
        "win.toggle-sidebar" => gettext("Show or hide the editing controls"),
        _ => String::new(),
    }
}

/// Builds the application with its actions and accelerators, without running it.
///
/// The tests use this to get an application whose accelerator table they can read
/// and whose window they can build without an event loop of its own.
pub fn build() -> adw::Application {
    let app = adw::Application::builder()
        .application_id(crate::APP_ID)
        .build();
    install_actions(&app);
    for (action, accel) in ACCELERATORS {
        app.set_accels_for_action(action, &[accel]);
    }
    app.connect_activate(|app| {
        if let Some(window) = active_window(app) {
            window.present();
            return;
        }
        let window = EditorWindow::new(app);
        window.present();
    });
    app
}

/// Runs the application until the last window closes.
pub fn run() -> glib::ExitCode {
    build().run()
}

/// The application's window, if one exists.
pub fn active_window(app: &adw::Application) -> Option<EditorWindow> {
    app.active_window()
        .and_then(|window| window.downcast::<EditorWindow>().ok())
}

fn install_actions(app: &adw::Application) {
    add_action(app, "new", |app| {
        if let Some(window) = active_window(app) {
            window.new_document();
        }
    });
    add_action(app, "open", |app| {
        if let Some(window) = active_window(app) {
            window.open();
        }
    });
    add_action(app, "shortcuts", |app| {
        shortcuts_dialog().present(active_window(app).as_ref());
    });
    add_action(app, "about", |app| {
        about_dialog().present(active_window(app).as_ref());
    });
    add_action(app, "quit", |app| {
        app.quit();
    });
}

fn add_action(app: &adw::Application, name: &str, run: impl Fn(&adw::Application) + 'static) {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| run(&app)
    ));
    app.add_action(&action);
}

/// The keyboard shortcuts dialog, built from [`SHORTCUT_SECTIONS`].
pub fn shortcuts_dialog() -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::new();
    for (section, actions) in SHORTCUT_SECTIONS {
        let group = adw::ShortcutsSection::new(Some(&gettext(*section)));
        for action in *actions {
            let accelerator = ACCELERATORS
                .iter()
                .find(|(name, _)| name == action)
                .map(|(_, accel)| *accel)
                .unwrap_or_default();
            group.add(adw::ShortcutsItem::new(
                &shortcut_title(action),
                accelerator,
            ));
        }
        dialog.add(group);
    }
    dialog
}

/// The about dialog. Its id and version come from [`crate::APP_ID`] and the
/// crate's own metadata rather than being written out a second time, which is
/// what keeps the window, the desktop file (S8) and the package in agreement.
pub fn about_dialog() -> adw::AboutDialog {
    let dialog = adw::AboutDialog::new();
    dialog.set_application_name("Pixlay");
    dialog.set_application_icon(crate::APP_ID);
    dialog.set_version(env!("CARGO_PKG_VERSION"));
    dialog.set_developer_name("Yangtse Su");
    dialog.set_website("https://yangtse.org/pixlay");
    dialog.set_issue_url("https://github.com/YangtseSu/pixlay/issues");
    dialog.set_license_type(gtk::License::Gpl30);
    dialog.set_comments(&gettext(
        "Make a collage out of two to ten photos and export it for printing.",
    ));
    dialog
}
