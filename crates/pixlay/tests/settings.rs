// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S25: the app's settings — the surface, the file, and the export that follows them
//! (rulings 36 and 39).
//!
//! One walk, on an account of its own: the harness sets `XDG_CONFIG_HOME` to a directory
//! of this binary's (see `support::start`), so this is a fresh account with no settings
//! file, and everything below is what a first run does — a change in the surface, an
//! export at what the settings say, the folder remembered, a second window that reads the
//! file back, and the CLI, which does not read it at all (`docs/CONTRACT.md` §5).
//!
//! The file's own shape — the fields, the defaults, the clamp, the atomic write — is
//! `crates/pixlay/src/settings.rs`'s unit tests; this is the wiring, on a real window.

mod support;

use std::ffi::OsString;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use pixlay::settings;
use pixlay_imaging::encode::Format;

#[test]
fn the_settings_surface_remembers_the_export_and_the_cli_ignores_the_file() {
    support::start();
    let app = support::app();
    let window = support::window(&app);

    // ---- a fresh account --------------------------------------------------
    // The path is `$XDG_CONFIG_HOME/pixlay/settings.json`, and this binary's own
    // `XDG_CONFIG_HOME` is the harness's, never the session's.
    let path = settings::Settings::path().expect("this account has a configuration directory");
    assert!(
        path.starts_with(support::config_dir()),
        "the settings live under this run's XDG_CONFIG_HOME, got {path:?}"
    );
    assert!(path.ends_with("pixlay/settings.json"), "{path:?}");
    assert!(!path.exists(), "a fresh account has no file: {path:?}");
    let defaults = window.settings();
    assert_eq!(defaults.format, Format::Jpeg);
    assert_eq!(defaults.long_edge, pixlay::window::DEFAULT_EXPORT_PX);
    assert_eq!(defaults.last_export_dir, None);

    // ---- the surface ------------------------------------------------------
    // `Ctrl+,`'s action (HIG `reference/keyboard`: Preferences is `Ctrl+,`), which the
    // menu's own item activates too.
    assert!(
        gtk4::gio::prelude::ActionGroupExt::has_action(&app, "settings"),
        "the app has a settings action"
    );
    gtk4::gio::prelude::ActionGroupExt::activate_action(&app, "settings", None);
    let dialog = window
        .settings_dialog()
        .expect("the window has a settings surface");
    assert!(dialog.widget().is_visible(), "the surface is presented");
    support::pump(Duration::from_millis(50));
    // Both rows carry an accessible name, and the long edge's carries its unit: a screen
    // reader announces "Long edge in pixels" rather than a bare figure.
    let format_row = dialog.format_row().upcast::<gtk4::Widget>();
    let long_edge_row = dialog.long_edge_row().upcast::<gtk4::Widget>();
    assert!(support::has_accessible_name(&format_row), "the format row");
    assert!(
        support::has_accessible_name(&long_edge_row),
        "the long edge row"
    );

    // A person moves both rows. Each change is the window's settings and the file's at
    // once: there is no *Save* to press, which is what a libadwaita preferences dialog
    // does everywhere.
    dialog.format_row().set_selected(1);
    dialog.long_edge_row().set_value(1234.0);
    support::pump(Duration::from_millis(50));
    let stored = settings::Settings::read(&path);
    assert_eq!(stored.format, Format::Png);
    assert_eq!(stored.long_edge, 1234);
    assert_eq!(
        window.settings(),
        stored,
        "the window holds what the file says"
    );
    // The file's own bytes: the contract's field names, in the contract's spelling
    // (`docs/CONTRACT.md` §9).
    let json = std::fs::read_to_string(&path).expect("the settings file is written");
    assert!(json.contains("\"format\": \"png\""), "{json}");
    assert!(json.contains("\"longEdge\": 1234"), "{json}");
    support::close_dialog(&dialog.widget().upcast::<adw::Dialog>(), &window);

    // ---- the export follows the settings (ruling 36) ----------------------
    let dir = support::out_dir().join("s25-settings");
    std::fs::create_dir_all(&dir).expect("the export directory can be created");
    let out = dir.join("shot.png");
    let _ = std::fs::remove_file(&out);
    window.export_to_chosen(&out);
    assert!(
        window.wait_for_idle(support::WAIT),
        "the export finished (toast {:?})",
        window.last_toast()
    );
    // The file that landed is the settings' format at the settings' long edge — read
    // back from its own bytes and its own pixels, not from what was asked for.
    let bytes = std::fs::read(&out).expect("the export landed");
    assert_eq!(
        &bytes[..4],
        b"\x89PNG",
        "the settings' format decided the file"
    );
    let written = pixlay_imaging::Source::decode(&out).expect("the export decodes");
    let expected = pixlay_core::PixelSize::for_long_edge(window.document().template.aspect, 1234)
        .expect("the grid the settings ask for");
    assert_eq!(
        (written.width(), written.height()),
        (expected.width as u32, expected.height as u32),
        "the export is the template's shape at the settings' long edge"
    );

    // ---- the folder is remembered across runs -----------------------------
    let stored = settings::Settings::read(&path);
    assert_eq!(
        stored.last_export_dir,
        Some(dir.clone()),
        "the export remembered the folder it landed in"
    );
    // A second window is a restart in miniature: it reads the file when it is built, so
    // what it holds and where it would export are the file's own answers.
    let restarted = support::second_window(&app);
    assert_eq!(restarted.settings(), stored, "a new window reads the file");
    assert_eq!(
        restarted.export_seed().folder,
        Some(dir.clone()),
        "and its save dialog opens where the last export landed"
    );
    // The surface opens on the file's values too, not on its own last state.
    restarted.show_settings();
    let restarted_dialog = restarted
        .settings_dialog()
        .expect("the window has a settings surface");
    support::pump(Duration::from_millis(50));
    assert_eq!(
        restarted_dialog.format_row().selected(),
        1,
        "PNG is the stored format"
    );
    assert_eq!(restarted_dialog.long_edge_row().value(), 1234.0);

    // ---- the CLI does not read the file (§5) ------------------------------
    // The same account, with the settings file asking for PNG at 1234: the CLI's output
    // is a function of its own command line alone, so `--long-edge 800 --out x.jpg` is
    // an 800 px JPEG — and the file it ignores is still there afterwards.
    let cli_out = dir.join("cli.jpg");
    let _ = std::fs::remove_file(&cli_out);
    let project = support::verify_project();
    let argv: Vec<OsString> = [
        "render",
        "--project",
        project.to_str().expect("the fixture's path is text"),
        "--long-edge",
        "800",
        "--out",
        cli_out.to_str().expect("the artifact's path is text"),
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let status = pixlay_cli::cli::run(&argv).expect("the CLI renders");
    assert_eq!(status, 0, "the CLI's own command line succeeds");
    assert!(path.exists(), "the CLI did not touch the settings file");
    let cli_written = pixlay_imaging::Source::decode(&cli_out).expect("the CLI's export decodes");
    assert_eq!(
        cli_written.width(),
        800,
        "the CLI's long edge is its own flag, not the settings'"
    );
    assert_eq!(
        &std::fs::read(&cli_out).expect("read")[..2],
        &[0xff, 0xd8],
        "and its format is its own --out, not the settings'"
    );
}
