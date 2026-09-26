//! The app's own settings, and the surface that edits them (S25, rulings 36 and 39).
//!
//! Ruling 36 moved the export's format and long edge out of the export's own dialog
//! into a **Settings surface** remembered across runs, and ruling 39 allowed the file
//! that remembering needs: `~/.config/pixlay/settings.json` under `XDG_CONFIG_HOME`,
//! carrying the format, the long edge and the folder the last export used, and nothing
//! else. The theme is not one of them (the app is dark by default and offers no
//! switch), and **the CLI does not read the file**: `--long-edge` and `--out` stay a
//! function of the CLI's own command line (`docs/CONTRACT.md` §5).
//!
//! **Read once, written atomically.** A window reads the file when it is built and
//! writes it through [`pixlay_core::atomic::write_atomic`] (S15c) when a row moves or
//! an export remembers its folder. A missing file, an unreadable one and one this
//! build cannot parse are all the same thing: the defaults, and never an error the
//! user sees. A *value* outside the range the surface can show is brought back inside
//! it for the same reason — the file is the app's own, and a hand-edited `999999` is a
//! number to clamp, not a reason to lose the other two settings.
//!
//! **The surface is `AdwPreferencesDialog`** (libadwaita 1.5): `AdwPreferencesWindow`
//! is deprecated since 1.6, and a deprecated constructor is what
//! `cargo clippy -- -D warnings` refuses — the same reason the export's own dialog
//! could not use `GtkFileChooserWidget` (the research of 2026-09-25). It carries one
//! `AdwPreferencesPage` with one group and two rows: the format, and the long edge in
//! pixels with the unit in its accessible name. The dialog's title is this app's own
//! `gettext("Preferences")` rather than libadwaita's default of the same word, so the
//! heading and the menu item are one string in one catalog.

use std::cell::Cell;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::glib;
use libadwaita as adw;
use serde::{Deserialize, Serialize};

use pixlay_imaging::encode::Format;

use crate::a11y;
use crate::export::{MAX_EXPORT_PX, MIN_EXPORT_PX};
use crate::i18n::gettext;
use crate::window::EditorWindow;

/// The formats the settings' row offers, in the row's own order.
///
/// The row's index *is* the index into this table, so the two cannot drift. It is the
/// export form's own table, moved here with the question it answers (S25).
const FORMATS: [Format; 2] = [Format::Jpeg, Format::Png];

/// The formats' names, which are identifiers and never translated (`AGENTS.md`,
/// "Language conventions"): a user reads "PNG" and a file carries `.png`.
const FORMAT_NAMES: [&str; 2] = ["JPEG", "PNG"];

/// The step the long edge moves in, in pixels: 100 is a round number a person can type
/// over, and the bounds are the export form's own (`MIN_EXPORT_PX` / `MAX_EXPORT_PX`).
const SIZE_STEP: u32 = 100;

/// The export settings this app remembers, and nothing else (ruling 39).
///
/// The file's own field names are this struct's, in camelCase — the spelling
/// `docs/CONTRACT.md` §1 already uses for `.pixlay` — and `lastExportDir` is left out
/// until the first export of this account rather than written as a null.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The format an export is written in.
    #[serde(with = "format_name")]
    pub format: Format,
    /// The long edge an export is rendered at, in pixels.
    pub long_edge: u32,
    /// The folder the last export used, which is where the next one's save dialog
    /// opens. `None` until this account has exported once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_export_dir: Option<PathBuf>,
}

impl Default for Settings {
    /// The export's own defaults (S12d): the CLI's default format and the form's own
    /// starting size, so a fresh account exports what a fresh window always exported.
    fn default() -> Self {
        Self {
            format: Format::Jpeg,
            long_edge: crate::window::DEFAULT_EXPORT_PX,
            last_export_dir: None,
        }
    }
}

impl Settings {
    /// Where the settings file lives: `$XDG_CONFIG_HOME/pixlay/settings.json`, or
    /// `~/.config/pixlay/settings.json` — the path `AGENTS.md` locks.
    ///
    /// GLib's own answer rather than a second reading of the environment, so this is
    /// the same directory GTK, dconf and every other part of the platform call the
    /// user's configuration. `None` is an account with no configuration directory at
    /// all: such a process keeps the defaults and writes nothing.
    pub fn path() -> Option<PathBuf> {
        let dir = glib::user_config_dir();
        (!dir.as_os_str().is_empty()).then(|| dir.join("pixlay").join("settings.json"))
    }

    /// The settings file this account has, or the defaults.
    ///
    /// Read **once**, when a window is built: nothing re-reads the file while the
    /// window is open, so the rows, the export and the file cannot disagree.
    pub fn load() -> Self {
        Self::path().map_or_else(Self::default, |path| Self::read(&path))
    }

    /// Reads one file, or the defaults.
    ///
    /// Public because it is the half of [`Settings::load`] a test can drive without an
    /// environment of its own. Every failure is the defaults, and a file that is not
    /// there is not even a warning: this account has simply never exported.
    ///
    /// **A field this build does not know is ignored**, not refused: a settings file is
    /// not a document, and refusing one over an extra key would silently reset the
    /// settings that *are* known.
    pub fn read(path: &Path) -> Self {
        let json = match std::fs::read_to_string(path) {
            Ok(json) => json,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                glib::g_warning!("pixlay", "{}: {error}", path.display());
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&json) {
            Ok(settings) => settings.clamped(),
            Err(error) => {
                glib::g_warning!("pixlay", "{}: {error}", path.display());
                Self::default()
            }
        }
    }

    /// The same settings with the long edge inside the range the surface can show.
    pub fn clamped(mut self) -> Self {
        self.long_edge = self.long_edge.clamp(MIN_EXPORT_PX, MAX_EXPORT_PX);
        self
    }

    /// Writes the settings file, atomically.
    ///
    /// `Ok` for an account with no configuration directory: there is nowhere to
    /// remember them, which is not a failure the user could act on.
    pub fn write(&self) -> Result<(), String> {
        match Self::path() {
            Some(path) => self.write_to(&path),
            None => Ok(()),
        }
    }

    /// Writes these settings to `path`, creating the directory they live in.
    ///
    /// The write is [`pixlay_core::atomic::write_atomic`]'s (S15c): a temporary file
    /// beside the target and one rename, so a crash or a full disk leaves the previous
    /// file rather than half of this one. The JSON is pretty and newline-terminated —
    /// a file a person can read is a file a person can fix by hand.
    pub fn write_to(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        let mut json = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        json.push('\n');
        pixlay_core::atomic::write_atomic(path, |file| file.write_all(json.as_bytes()))
            .map(|_bytes| ())
            .map_err(|failure| failure.into_io().to_string())
    }
}

/// The Settings surface: the export's format and long edge as two rows (S25).
///
/// One page, one group, two rows — the whole of what ruling 39 lets the file carry.
/// Each row writes the window's settings as it moves, so a change is on disk before the
/// dialog is closed; there is nothing to confirm and no *Save* button, which is what
/// every libadwaita preferences dialog does.
pub struct Dialog {
    dialog: adw::PreferencesDialog,
    format: adw::ComboRow,
    long_edge: adw::SpinRow,
    /// Set while this module writes the rows, so seeding the surface from the settings
    /// is not read back as a user editing it (the frame dialog's own idiom).
    updating: Rc<Cell<bool>>,
}

impl Dialog {
    /// Builds the surface, which the menu's *Preferences* item and `Ctrl+,` present.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let format = adw::ComboRow::builder()
            .title(gettext("Format"))
            .model(&gtk::StringList::new(&FORMAT_NAMES))
            .build();
        a11y::label(&format, &gettext("Export format"));
        let long_edge = adw::SpinRow::with_range(
            f64::from(MIN_EXPORT_PX),
            f64::from(MAX_EXPORT_PX),
            f64::from(SIZE_STEP),
        );
        long_edge.set_digits(0);
        long_edge.set_title(&gettext("Long edge"));
        long_edge.set_subtitle(&gettext("In pixels"));
        a11y::label_spin_row(&long_edge, &gettext("Long edge in pixels"));

        let group = adw::PreferencesGroup::builder()
            .title(gettext("Export"))
            .description(gettext("The format and the size of an export"))
            .build();
        group.add(&format);
        group.add(&long_edge);

        let page = adw::PreferencesPage::new();
        page.set_title(&gettext("Export"));
        page.add(&group);

        let dialog = adw::PreferencesDialog::new();
        dialog.set_title(&gettext("Preferences"));
        dialog.add(&page);

        let surface = Rc::new(Self {
            dialog,
            format,
            long_edge,
            updating: Rc::new(Cell::new(false)),
        });
        // Weak self-references, for the reason the frame dialog's handlers are weak:
        // these closures live on widgets the dialog owns.
        let weak = Rc::downgrade(&surface);
        surface.format.connect_selected_notify(glib::clone!(
            #[weak]
            window,
            #[strong]
            weak,
            move |row| {
                let Some(dialog) = weak.upgrade() else {
                    return;
                };
                if dialog.updating.get() {
                    return;
                }
                let Some(format) = FORMATS.get(row.selected() as usize) else {
                    return;
                };
                let mut settings = window.settings();
                settings.format = *format;
                window.remember_settings(&settings);
            }
        ));
        let weak = Rc::downgrade(&surface);
        surface.long_edge.connect_value_notify(glib::clone!(
            #[weak]
            window,
            #[strong]
            weak,
            move |row| {
                let Some(dialog) = weak.upgrade() else {
                    return;
                };
                if dialog.updating.get() {
                    return;
                }
                let mut settings = window.settings();
                settings.long_edge = row.value().round() as u32;
                window.remember_settings(&settings);
            }
        ));
        surface
    }

    /// Shows the surface over `window`, with the window's own settings in its rows.
    pub fn present(&self, window: &EditorWindow) {
        self.seed(window);
        self.dialog.present(Some(window));
    }

    /// Puts the window's own settings into the rows.
    ///
    /// The settings are the authority rather than the dialog's last state, and the seed
    /// is guarded: writing the rows is not a user editing them, and without the guard
    /// opening the surface would write the settings file again.
    pub fn seed(&self, window: &EditorWindow) {
        let settings = window.settings();
        let index = FORMATS
            .iter()
            .position(|format| *format == settings.format)
            .unwrap_or(0) as u32;
        self.updating.set(true);
        self.format.set_selected(index);
        self.long_edge.set_value(f64::from(settings.long_edge));
        self.updating.set(false);
    }

    /// The dialog itself, for the tests and the HIG checks.
    pub fn widget(&self) -> adw::PreferencesDialog {
        self.dialog.clone()
    }

    pub fn format_row(&self) -> adw::ComboRow {
        self.format.clone()
    }

    pub fn long_edge_row(&self) -> adw::SpinRow {
        self.long_edge.clone()
    }
}

/// The format's own spelling in the file: the encoder's `Format::name()` (`png` /
/// `jpeg`), rather than a second vocabulary for the same two formats.
///
/// `serde` still does the reading and the refusing: a name that is not one of these is
/// a parse error, which is what makes a hand-edited `"format": "tiff"` the defaults
/// instead of a panic or a silent fallback to JPEG.
mod format_name {
    use super::Format;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(format: &Format, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(format.name())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Format, D::Error> {
        let name = String::deserialize(deserializer)?;
        match name.as_str() {
            "png" => Ok(Format::Png),
            // Both spellings of the JPEG extension are the JPEG format, which is the
            // rule `Format::from_path` applies to an output path.
            "jpg" | "jpeg" => Ok(Format::Jpeg),
            other => Err(serde::de::Error::unknown_variant(other, &["png", "jpeg"])),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The file's own tests: its shape, its defaults, the clamp and the atomic write.
    //!
    //! They name the file they write and touch no environment, so they need neither a
    //! display nor a window — and the GUI's own walk of the same file (the settings
    //! surface, the export that remembers its folder, a second window that reads it
    //! back) is `crates/pixlay/tests/settings.rs`.

    use super::*;

    /// A directory of this test's own, on a disk path: `/tmp` is tmpfs on this machine
    /// (`AGENTS.md`, "Measurement rules").
    fn scratch(name: &str) -> PathBuf {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/tmp"));
        let dir = base.join("pixlay-s25").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch directory can be created");
        dir
    }

    #[test]
    fn the_file_round_trips_and_carries_these_three_fields() {
        let dir = scratch("round-trip");
        let path = dir.join("settings.json");
        let settings = Settings {
            format: Format::Png,
            long_edge: 1234,
            last_export_dir: Some(dir.join("exports")),
        };
        settings.write_to(&path).expect("the settings are written");
        assert_eq!(Settings::read(&path), settings, "the file reads back");
        let json = std::fs::read_to_string(&path).expect("the file is readable");
        // The shape a person reads and a hand-edit meets: camelCase, the encoder's own
        // format name, and the folder as the path it is.
        assert!(json.contains("\"format\": \"png\""), "{json}");
        assert!(json.contains("\"longEdge\": 1234"), "{json}");
        assert!(
            json.contains(&format!(
                "\"lastExportDir\": {:?}",
                dir.join("exports").display().to_string()
            )),
            "{json}"
        );
        assert!(
            json.ends_with("}\n"),
            "the file ends with a newline: {json}"
        );
        // Nothing of the write is left beside it: the temporary file is renamed, not
        // kept (`pixlay_core::atomic`).
        let entries: Vec<String> = std::fs::read_dir(&dir)
            .expect("the directory is readable")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(entries, vec!["settings.json".to_string()], "{entries:?}");
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_so_is_a_broken_one() {
        let dir = scratch("defaults");
        let path = dir.join("settings.json");
        assert_eq!(Settings::read(&path), Settings::default());
        assert_eq!(Settings::default().format, Format::Jpeg);
        assert_eq!(
            Settings::default().long_edge,
            crate::window::DEFAULT_EXPORT_PX
        );

        // A file that is not JSON at all, and one whose format this build does not
        // write: both are the defaults, because a settings file is not a document and
        // a window has to open either way.
        for body in ["{ half a file", "{\"format\": \"tiff\", \"longEdge\": 800}"] {
            std::fs::write(&path, body).expect("the file is written");
            assert_eq!(Settings::read(&path), Settings::default(), "{body}");
        }
        // A field this build does not know is ignored rather than refused: the three
        // it does know are read.
        std::fs::write(
            &path,
            "{\"format\": \"png\", \"longEdge\": 800, \"future\": true}",
        )
        .expect("the file is written");
        let read = Settings::read(&path);
        assert_eq!(read.format, Format::Png);
        assert_eq!(read.long_edge, 800);
        assert_eq!(read.last_export_dir, None);
    }

    #[test]
    fn an_edge_outside_the_forms_range_comes_back_inside_it() {
        let dir = scratch("clamp");
        let path = dir.join("settings.json");
        for (written, expected) in [(0u32, MIN_EXPORT_PX), (999_999, MAX_EXPORT_PX)] {
            std::fs::write(
                &path,
                format!("{{\"format\": \"jpeg\", \"longEdge\": {written}}}"),
            )
            .expect("the file is written");
            assert_eq!(
                Settings::read(&path).long_edge,
                expected,
                "a hand-edited {written} is brought back inside the range"
            );
        }
    }

    #[test]
    fn the_write_replaces_the_previous_file_and_creates_its_directory() {
        let dir = scratch("replace");
        // A directory that is not there yet: the file's own directory is created, which
        // is what a first export on a fresh account needs.
        let path = dir.join("pixlay").join("settings.json");
        let first = Settings::default();
        first.write_to(&path).expect("the directory is created");
        assert_eq!(Settings::read(&path), first);

        let second = Settings {
            format: Format::Png,
            long_edge: 900,
            last_export_dir: None,
        };
        second.write_to(&path).expect("the file is replaced");
        assert_eq!(Settings::read(&path), second);
        // A write that cannot happen leaves the previous file exactly as it was: the
        // target's own directory is a *file* here, so even creating the temporary fails.
        let blocked = dir.join("blocked");
        std::fs::write(&blocked, b"a file").expect("the blocker is written");
        assert!(
            second.write_to(&blocked.join("settings.json")).is_err(),
            "a write under a file is refused"
        );
        assert_eq!(std::fs::read(&blocked).expect("read"), b"a file");
        assert_eq!(Settings::read(&path), second, "the previous file survives");
    }
}
