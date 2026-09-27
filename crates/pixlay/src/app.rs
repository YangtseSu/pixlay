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
//! `Shift+Ctrl+Z` / `Ctrl+?`, plus `Ctrl+N` for a new item and `Ctrl+,` for the
//! preferences — which S25 added with the surface they open (`crate::settings`).
//! Everything else on that page belongs to features this product does not have
//! (print, send, help), and the system-reserved combinations (`Alt+*`, `Super+*`,
//! `Ctrl+Alt+*`) are used nowhere — a test asserts both halves.
//!
//! Since S22 the application is also the place where **the command line enters**:
//! `HANDLES_OPEN` and the `open` handler below turn `pixlay a.jpg b.jpg …` into the
//! window's own `add_photos`, in argument order (ruling 31). `F9` left with the
//! utility pane in S13 (ruling 18) and the picker's own keys left with the picker in
//! S22: one page has no second surface for them to toggle.

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
    // HIG `reference/keyboard`, "Basic Shortcuts": Preferences is `Ctrl+,` — the one
    // standard combination this app had no surface for until S25.
    ("app.settings", "<Control>comma"),
    ("win.save", "<Control>s"),
    ("win.save-as", "<Control><Shift>s"),
    ("win.close", "<Control>w"),
    ("win.export", "<Control>e"),
    ("win.add-photos", "<Control>i"),
    ("win.undo", "<Control>z"),
    ("win.redo", "<Control><Shift>z"),
    ("win.cut", "<Control>x"),
    ("win.copy", "<Control>c"),
    ("win.paste", "<Control>v"),
    ("win.reset-framing", "<Control>0"),
];

/// How the shortcuts dialog groups them (HIG `reference/keyboard`, "Sections").
pub const SHORTCUT_SECTIONS: &[(&str, &[&str])] = &[
    (
        "General",
        &["app.shortcuts", "win.close", "app.quit", "app.settings"],
    ),
    (
        "Collage",
        &[
            "app.new",
            "app.open",
            "win.save",
            "win.save-as",
            "win.export",
            "win.add-photos",
        ],
    ),
    (
        "Editing",
        &[
            "win.undo",
            "win.redo",
            "win.cut",
            "win.copy",
            "win.paste",
            "win.reset-framing",
        ],
    ),
];

/// What a shortcut section is called in the dialog.
///
/// A `match` on the section's own name, for the reason [`shortcut_title`] gives:
/// the names in [`SHORTCUT_SECTIONS`] are data, and `gettext` on a variable is a
/// call no extraction pass can see — every section would keep its English heading
/// in every language while the freshness check stayed green (PIX-026, S15i).
pub fn shortcut_section_title(section: &str) -> String {
    match section {
        "General" => gettext("General"),
        "Collage" => gettext("Collage"),
        "Editing" => gettext("Editing"),
        _ => String::new(),
    }
}

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
        "app.settings" => gettext("Preferences"),
        "win.save" => gettext("Save"),
        "win.save-as" => gettext("Save as…"),
        "win.close" => gettext("Close the window"),
        "win.export" => gettext("Export the collage"),
        "win.add-photos" => gettext("Add photos…"),
        "win.undo" => gettext("Undo"),
        "win.redo" => gettext("Redo"),
        "win.cut" => gettext("Cut the photo out of the cell"),
        "win.copy" => gettext("Copy the photo"),
        "win.paste" => gettext("Paste the photo into the cell"),
        "win.reset-framing" => gettext("Reset the framing"),
        _ => String::new(),
    }
}

/// Builds the application with its actions and accelerators, without running it.
///
/// The tests use this to get an application whose accelerator table they can read
/// and whose window they can build without an event loop of its own.
///
/// **`HANDLES_OPEN` is what makes `pixlay a.jpg b.jpg …` work** (S22): with
/// arguments the platform emits `open` with the files instead of `activate`, and the
/// handler below puts them in the window in the order it received them. Without
/// arguments nothing changes — `activate` builds the one window on the default
/// document.
pub fn build() -> adw::Application {
    let app = adw::Application::builder()
        .application_id(pixlay_core::APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    install_actions(&app);
    for (action, accel) in ACCELERATORS {
        app.set_accels_for_action(action, &[accel]);
    }
    // At startup rather than at build time: the display exists once GTK has
    // initialised, which is what `GtkApplication`'s `startup` runs after — and
    // `build()` is called before the application is run at all.
    app.connect_startup(|_| {
        install_style();
        // **The app is dark by default** (ruled 2026-09-22, ruling 23). HIG
        // `guidelines/ui-styling` recommends the dark style for "apps which display
        // rich visual content like images or video", which is what a photo collage is,
        // and both reference apps do the same thing unconditionally (gthumb
        // `src/Application.vala:676`, loupe `src/application.rs:76-79`). There is no
        // per-app switch: neither reference app has one, and a stored preference
        // would need the settings file ruling 8 forbids. The canvas and the export
        // are unaffected — they are document content, not styling.
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    });
    app.connect_activate(|app| {
        show_window(app).present();
    });
    app.connect_open(|app, files, _hint| {
        open_files(app, files);
    });
    app
}

/// The command line's own entry (S22, ruling 31): the arguments are photos, in the
/// order given, and they take the same `Command::AddPhotos` the `Add photos…` chooser
/// sends — which is what makes "the order is the argument order" one rule with one
/// implementation.
///
/// It is public because it *is* what `pixlay a.jpg b.jpg …` does: `connect_open` above
/// is the signal `HANDLES_OPEN` routes those arguments to, and the machine walk drives
/// this function with the same files a command line would pass (`tests/mainpath.rs`).
pub fn open_files(app: &adw::Application, files: &[gio::File]) {
    let paths: Vec<std::path::PathBuf> = files.iter().filter_map(|file| file.path()).collect();
    let window = show_window(app);
    window.present();
    window.open_paths(paths);
}

/// The application's one window, built if it does not exist yet.
///
/// `activate` and `open` both need it: the product is a single-window application, so
/// a second entry into the same process — a second `pixlay` invocation, which reaches
/// the running instance's `open` — adds to the window that is already there.
fn show_window(app: &adw::Application) -> EditorWindow {
    match active_window(app) {
        Some(window) => window,
        None => EditorWindow::new(app),
    }
}

/// The app's own stylesheet: the layout band's cell and sketch colours, and the
/// status bar's padding (`style.css`).
///
/// It is loaded from a string baked into the binary (`include_str!`) rather than
/// from a file on disk: the shell has no runtime data directory, and a handful of
/// rules does not need one. The provider goes on the display at the application's own
/// priority, so it can use the theme's variables but cannot restyle the platform.
fn install_style() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
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
    // The preferences dialog is a *secondary window* belonging to the primary one
    // (HIG `patterns/containers/windows`), so it is presented over the window that is
    // there rather than opening one of its own.
    add_action(app, "settings", |app| {
        if let Some(window) = active_window(app) {
            window.show_settings();
        }
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
        let group = adw::ShortcutsSection::new(Some(&shortcut_section_title(section)));
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

/// The about dialog. Its id and version come from [`pixlay_core::APP_ID`] and the
/// crate's own metadata rather than being written out a second time, which is
/// what keeps the window, the desktop file (S8) and the package in agreement.
pub fn about_dialog() -> adw::AboutDialog {
    let dialog = adw::AboutDialog::new();
    dialog.set_application_name("Pixlay");
    dialog.set_application_icon(pixlay_core::APP_ID);
    dialog.set_version(env!("CARGO_PKG_VERSION"));
    dialog.set_developer_name("Yangtse Su");
    dialog.set_website("https://yangtse.org/pixlay");
    dialog.set_issue_url("https://github.com/YangtseSu/pixlay/issues");
    dialog.set_license_type(gtk::License::Gpl30);
    dialog.set_comments(&gettext(
        "Make a collage out of one to nine photos and export it for printing.",
    ));
    dialog
}
