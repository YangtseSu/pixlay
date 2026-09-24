//! The two document-level dialogs: `Frame…` and `Export…` (S15, ruling 18).
//!
//! Ruling 18 removed the utility pane, and the two groups that had no other home
//! became dialogs of one shape rather than permanent rows: the frame's three
//! settings, and the export's three questions. Each is an `AdwDialog` with a header
//! bar, a heading that names the action, and rows over a shape the CLI already has —
//! HIG `patterns/feedback/dialogs`, "Action Dialogs" (a header bar, a heading which
//! describes the action, and the affirmative button carrying an imperative verb).
//!
//! **What differs between them is when they write.** `Export…` is an action dialog in
//! the literal sense: the rows are read once, when *Export* is pressed, and pressing
//! it runs the export (one action, one file). `Frame…` writes as the rows move — the
//! canvas behind it redraws, the change becomes one undo step when the value stops
//! moving, and `Ctrl+Z` is the way back — so its only button is *Close*: a dialog that
//! has already applied everything has nothing to confirm, and a Cancel that had to
//! unwind a stack of live edits would be a second undo stack (`S14b · Ruling`, the
//! same reason `+` does not remember the cell it dropped).
//!
//! Neither dialog holds a GTK object across a thread: the export it starts runs on
//! the window's own worker (`crate::export`), and the progress bar and the toast
//! stay the window's.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{Frame, Rgba8};
use pixlay_imaging::encode::Format;

use crate::a11y;
use crate::export::{MAX_EXPORT_PX, MIN_EXPORT_PX, Settings};
use crate::i18n::{fill, gettext};
use crate::window::EditorWindow;

/// The formats the export form offers, in the row's own order.
///
/// The row's index *is* the index into this table, so the two cannot drift.
const FORMATS: [Format; 2] = [Format::Jpeg, Format::Png];

/// The formats' names, which are identifiers and never translated (`AGENTS.md`,
/// "Language conventions"): a user reads "PNG" and a file carries `.png`.
const FORMAT_NAMES: [&str; 2] = ["JPEG", "PNG"];

/// The formats' extensions, for the name a format change rewrites.
const FORMAT_EXTENSIONS: [&str; 2] = ["jpg", "png"];

/// The step the size row moves in, in pixels: 100 is a round number a person can
/// type over, and the bounds are the form's own (`MIN_EXPORT_PX` / `MAX_EXPORT_PX`).
const SIZE_STEP: u32 = 100;

/// The frame's own lengths are typed as per cent of the collage's height.
///
/// The document holds a fraction (`frame.gapRel` / `radiusRel`) and the CLI takes it
/// that way (`--gap 0.02`); a row is a number a person types, and "2" reads as a share
/// where "0.02" reads as a mystery. The conversion lives here and nowhere else.
const PERCENT: f64 = 100.0;

/// The `Frame…` dialog: the document's frame as three rows, in the document's own
/// order — gap, radius, colour (ruling 30, 2026-09-23).
pub struct FrameDialog {
    dialog: adw::Dialog,
    close: gtk::Button,
    gap: adw::SpinRow,
    radius: adw::SpinRow,
    color: gtk::ColorDialogButton,
    /// Set while this module writes the rows, so that seeding the dialog from the
    /// document is not read back as a user editing it (the retired pane's own idiom).
    updating: Rc<Cell<bool>>,
}

impl FrameDialog {
    /// Builds the dialog, which the header bar's `Frame…` button presents.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let gap = percent_row(&gettext("Gap"), &gettext("Between the photos"));
        let radius = percent_row(&gettext("Radius"), &gettext("Rounded corners"));
        // Alpha is off in the chooser itself as well as dropped on the way in: the
        // backdrop is painted, not blended, so a translucent frame is not a document
        // this build can render (`Frame::validate`).
        let color_dialog = gtk::ColorDialog::builder().with_alpha(false).build();
        let color = gtk::ColorDialogButton::new(Some(color_dialog));
        color.set_valign(gtk::Align::Center);
        a11y::label(&color, &gettext("Colour"));
        let color_row = adw::ActionRow::builder()
            .title(gettext("Colour"))
            .subtitle(gettext("Behind and between the photos"))
            .build();
        color_row.add_suffix(&color);

        let group = adw::PreferencesGroup::builder()
            .title(gettext("Frame"))
            // The unit belongs here rather than on both rows: the two lengths are the
            // same measure of the same canvas.
            .description(gettext("Both lengths are a share of the collage's height"))
            .build();
        group.add(&gap);
        group.add(&radius);
        group.add(&color_row);

        let close = gtk::Button::with_label(&gettext("Close"));
        a11y::label(&close, &gettext("Close"));
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(&gettext("Frame"), "")));
        header.pack_end(&close);

        let page = adw::PreferencesPage::new();
        page.add(&group);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(&page));
        let dialog = adw::Dialog::builder()
            .title(gettext("Frame"))
            .content_width(480)
            .child(&view)
            .build();

        let frame_dialog = Rc::new(Self {
            dialog,
            close: close.clone(),
            gap,
            radius,
            color,
            updating: Rc::new(Cell::new(false)),
        });
        // The handlers live on the dialog's own children, so they hold *weak*
        // references to it: a strong one would be a cycle (the dialog owns the
        // buttons, the buttons own the handler) and the dialog would outlive the
        // window that made it.
        let weak = Rc::downgrade(&frame_dialog);
        close.connect_clicked(glib::clone!(
            #[strong]
            weak,
            move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.dialog.close();
                }
            }
        ));
        // Live: every settled change is a document edit, and the canvas behind the
        // dialog redraws (`EditorWindow::set_frame` keeps it pending while the value
        // moves and commits it once it stops).
        for row in [&frame_dialog.gap, &frame_dialog.radius] {
            let weak = Rc::downgrade(&frame_dialog);
            row.connect_value_notify(glib::clone!(
                #[weak]
                window,
                #[strong]
                weak,
                move |_| {
                    let Some(dialog) = weak.upgrade() else {
                        return;
                    };
                    if dialog.updating.get() {
                        return;
                    }
                    window.set_frame(dialog.frame());
                }
            ));
        }
        let weak = Rc::downgrade(&frame_dialog);
        frame_dialog.color.connect_rgba_notify(glib::clone!(
            #[weak]
            window,
            #[strong]
            weak,
            move |_| {
                let Some(dialog) = weak.upgrade() else {
                    return;
                };
                if dialog.updating.get() {
                    return;
                }
                window.set_frame(dialog.frame());
            }
        ));
        frame_dialog
    }

    /// Shows the dialog over `window`, with the document's own frame in its rows.
    pub fn present(&self, window: &EditorWindow) {
        self.seed(window);
        self.dialog.present(Some(window));
    }

    /// The dialog itself, for the tests and the HIG checks.
    pub fn widget(&self) -> adw::Dialog {
        self.dialog.clone()
    }

    pub fn gap_row(&self) -> adw::SpinRow {
        self.gap.clone()
    }

    pub fn radius_row(&self) -> adw::SpinRow {
        self.radius.clone()
    }

    pub fn color_button(&self) -> gtk::ColorDialogButton {
        self.color.clone()
    }

    /// The frame the three rows describe.
    pub fn frame(&self) -> Frame {
        Frame {
            gap_rel: self.gap.value() / PERCENT,
            radius_rel: self.radius.value() / PERCENT,
            color: rgba8(&self.color.rgba()),
        }
    }

    /// The dialog's own way out, for the tests: clicking it is what a person does.
    pub fn close_button(&self) -> gtk::Button {
        self.close.clone()
    }

    /// Puts the document's own frame into the rows.
    ///
    /// The document is the authority rather than the dialog's last state: the frame
    /// can change without the dialog being touched (an undo, a project opened, the
    /// CLI's `edit --gap`), and a row that followed its own history would show a
    /// number the file does not have.
    pub fn seed(&self, window: &EditorWindow) {
        let frame = window.document().frame;
        self.updating.set(true);
        self.gap.set_value(frame.gap_rel * PERCENT);
        self.radius.set_value(frame.radius_rel * PERCENT);
        self.color.set_rgba(&to_rgba(frame.color));
        self.updating.set(false);
    }
}

/// The `Export…` dialog: the format, the one size parameter, and where the file goes.
#[derive(Clone)]
pub struct ExportDialog {
    dialog: adw::Dialog,
    format: adw::ComboRow,
    quality: adw::SpinRow,
    name: adw::EntryRow,
    choose: gtk::Button,
    export: gtk::Button,
    /// The export's directory: the name row holds only the file name, so this is the
    /// other half of [`Settings::path`]. `None` means "the name as it stands", which
    /// is what an unsaved document's export has always used.
    dir: RefCell<Option<PathBuf>>,
    /// The format row's own previous selection, so the extension rewrite knows which
    /// extension to replace.
    last_format: Cell<u32>,
    /// Set while this module writes the rows (see [`FrameDialog::seed`]).
    updating: Rc<Cell<bool>>,
}

impl ExportDialog {
    /// Builds the dialog, which the header bar's `Export…` button presents.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let mut names = Vec::new();
        for name in FORMAT_NAMES {
            names.push(glib::GString::from(name));
        }
        let format = adw::ComboRow::builder()
            .title(gettext("Format"))
            .model(&string_list(names))
            .build();
        a11y::label(&format, &gettext("Format"));
        let quality = adw::SpinRow::with_range(
            f64::from(MIN_EXPORT_PX),
            f64::from(MAX_EXPORT_PX),
            f64::from(SIZE_STEP),
        );
        quality.set_digits(0);
        quality.set_title(&gettext("Long edge"));
        quality.set_subtitle(&gettext("In pixels"));
        a11y::label_spin_row(&quality, &gettext("Long edge in pixels"));
        let name = adw::EntryRow::builder().title(gettext("File name")).build();
        let choose = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text(gettext("Choose where the export is written"))
            .valign(gtk::Align::Center)
            .build();
        choose.add_css_class("flat");
        a11y::label(&choose, &gettext("Choose where the export is written"));
        name.add_suffix(&choose);

        let group = adw::PreferencesGroup::builder()
            .title(gettext("Export"))
            .description(gettext("The format, the size and the file"))
            .build();
        group.add(&format);
        group.add(&quality);
        group.add(&name);

        // HIG `patterns/feedback/dialogs`: the cancel button comes first, before the
        // affirmative, and the affirmative carries the verb the action is.
        let cancel = gtk::Button::with_label(&gettext("Cancel"));
        a11y::label(&cancel, &gettext("Cancel"));
        let export = gtk::Button::with_label(&gettext("Export"));
        export.add_css_class("suggested-action");
        a11y::label(&export, &gettext("Export"));
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(&gettext("Export"), "")));
        header.pack_start(&cancel);
        header.pack_end(&export);

        let page = adw::PreferencesPage::new();
        page.add(&group);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(&page));
        let dialog = adw::Dialog::builder()
            .title(gettext("Export"))
            .content_width(480)
            .child(&view)
            .build();

        let export_dialog = Rc::new(Self {
            dialog,
            format,
            quality,
            name,
            choose,
            export: export.clone(),
            dir: RefCell::new(None),
            last_format: Cell::new(0),
            updating: Rc::new(Cell::new(false)),
        });
        // Weak self-references, for the reason the frame dialog's handlers are weak:
        // these closures live on widgets the dialog owns.
        let weak = Rc::downgrade(&export_dialog);
        cancel.connect_clicked(glib::clone!(
            #[strong]
            weak,
            move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.dialog.close();
                }
            }
        ));
        let weak = Rc::downgrade(&export_dialog);
        export.connect_clicked(glib::clone!(
            #[weak]
            window,
            #[strong]
            weak,
            move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.export(&window);
                }
            }
        ));
        // The format row owns the file's extension: a name that still carries the
        // other format's is rewritten, and one the user typed is left alone.
        let weak = Rc::downgrade(&export_dialog);
        export_dialog.format.connect_selected_notify(glib::clone!(
            #[strong]
            weak,
            move |row| {
                let Some(dialog) = weak.upgrade() else {
                    return;
                };
                if dialog.updating.get() {
                    return;
                }
                let previous = dialog.last_format.replace(row.selected());
                let (Some(old), Some(new)) = (
                    FORMAT_EXTENSIONS.get(previous as usize),
                    FORMAT_EXTENSIONS.get(row.selected() as usize),
                ) else {
                    return;
                };
                let current = dialog.name.text().to_string();
                if let Some(stem) = current.strip_suffix(&format!(".{old}")) {
                    dialog.name.set_text(&format!("{stem}.{new}"));
                }
            }
        ));
        let weak = Rc::downgrade(&export_dialog);
        export_dialog.choose.connect_clicked(glib::clone!(
            #[weak]
            window,
            #[strong]
            weak,
            move |_| {
                if let Some(dialog) = weak.upgrade() {
                    dialog.choose_path(&window);
                }
            }
        ));
        export_dialog
    }

    /// Shows the dialog over `window`, seeded from the export form the window holds.
    pub fn present(&self, window: &EditorWindow) {
        self.seed(window);
        self.dialog.present(Some(window));
    }

    /// The dialog itself, for the tests and the HIG checks.
    pub fn widget(&self) -> adw::Dialog {
        self.dialog.clone()
    }

    pub fn format_row(&self) -> adw::ComboRow {
        self.format.clone()
    }

    pub fn quality_row(&self) -> adw::SpinRow {
        self.quality.clone()
    }

    pub fn name_row(&self) -> adw::EntryRow {
        self.name.clone()
    }

    pub fn choose_button(&self) -> gtk::Button {
        self.choose.clone()
    }

    /// The dialog's affirmative, for the tests: clicking it is what a person does.
    pub fn export_button(&self) -> gtk::Button {
        self.export.clone()
    }

    /// The settings the three rows describe.
    pub fn settings(&self) -> Settings {
        Settings {
            long_edge: self.quality.value().round() as u32,
            format: FORMATS[self.format_index()],
            path: self.path(),
        }
    }

    /// Runs the export these rows describe, or refuses to.
    ///
    /// The one path an export takes from the GUI: the window stores the form's state
    /// and starts the same background export the menu's action does, so the progress
    /// bar, the toast and the worker thread are unchanged by the dialog that asked.
    ///
    /// Three answers, and the dialog stays open for the first two (S15c):
    ///
    /// * the path names one of the document's own photos — refused with the same
    ///   message `render` and `thumb` give, and nothing is written;
    /// * a file is already there — asked about, because replacing a file the user
    ///   already has is their decision (ruling 2026-09-24);
    /// * otherwise, the export starts.
    pub fn export(&self, window: &EditorWindow) {
        let settings = self.settings();
        match window.export_destination(&settings.path) {
            Err(reason) => {
                window.toast(&reason);
            }
            Ok(false) => self.start(window, &settings),
            Ok(true) => self.confirm_replacement(window, settings),
        }
    }

    /// Closes the dialog and starts the export.
    fn start(&self, window: &EditorWindow, settings: &Settings) {
        self.dialog.close();
        window.set_export_settings(settings);
        window.start_export(settings.path.clone());
    }

    /// Asks before an export replaces a file that is already there.
    ///
    /// `AdwAlertDialog` over this dialog rather than over the window, so cancelling
    /// leaves the rows exactly as they were — the name is still there to edit — and
    /// only *Replace* is destructive, which is what the response's appearance says.
    fn confirm_replacement(&self, window: &EditorWindow, settings: Settings) {
        let alert = adw::AlertDialog::new(
            Some(&gettext("Replace the existing file?")),
            Some(&fill(
                gettext("{} is already there. The export replaces it."),
                &[settings.path.display().to_string()],
            )),
        );
        alert.add_response("cancel", &gettext("Cancel"));
        alert.add_response("replace", &gettext("Replace"));
        alert.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        alert.set_default_response(Some("cancel"));
        alert.set_close_response("cancel");
        let this = self.clone();
        let window = window.clone();
        alert.connect_response(
            None,
            glib::clone!(
                #[strong]
                this,
                #[strong]
                window,
                move |_, response| {
                    if response == "replace" {
                        this.start(&window, &settings);
                    }
                }
            ),
        );
        alert.present(Some(&self.dialog));
    }

    /// Puts the window's own export form into the rows.
    pub fn seed(&self, window: &EditorWindow) {
        let settings = window.export_settings();
        let index = FORMATS
            .iter()
            .position(|format| *format == settings.format)
            .unwrap_or(0) as u32;
        self.updating.set(true);
        self.format.set_selected(index);
        self.last_format.set(index);
        self.quality.set_value(f64::from(settings.long_edge));
        self.dir.replace(
            settings
                .path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf),
        );
        self.name.set_text(&file_name(&settings.path));
        self.updating.set(false);
    }

    /// The index of the selected format, clamped to the table.
    fn format_index(&self) -> usize {
        (self.format.selected() as usize).min(FORMATS.len() - 1)
    }

    /// The path the rows describe: the name in the directory the chooser last set.
    fn path(&self) -> PathBuf {
        let name = PathBuf::from(self.name.text().to_string());
        match self.dir.borrow().as_deref() {
            Some(dir) => dir.join(name),
            None => name,
        }
    }

    /// Asks where the export goes, seeded with what the rows already say.
    ///
    /// The platform's own `GtkFileDialog` (which takes no custom widgets, and that is
    /// why the rest of the form is rows), and it answers *both* halves of the path:
    /// what it returns is a directory and a name, which is what the two controls hold.
    fn choose_path(&self, window: &EditorWindow) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Images")));
        // The filter follows the format row: the file this dialog is about to write
        // is the one pattern.
        for pattern in patterns(FORMATS[self.format_index()]) {
            filter.add_pattern(pattern);
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Export the collage"))
            .filters(&filters)
            .default_filter(&filter)
            .initial_name(self.name.text().to_string())
            .build();
        let this = self.clone();
        dialog.save(
            Some(window),
            gio::Cancellable::NONE,
            move |result: Result<gio::File, glib::Error>| {
                let Ok(file) = result else {
                    // A dismissed chooser is not a failure: the user changed their
                    // mind, and the rows keep what they had.
                    return;
                };
                let Some(path) = file.path() else {
                    return;
                };
                this.dir.replace(
                    path.parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                        .map(Path::to_path_buf),
                );
                this.name.set_text(&file_name(&path));
            },
        );
    }
}

/// A row whose value is a share of the canvas height, in per cent.
///
/// The accessible name says the unit, because the row's own number does not: a screen
/// reader announces "Gap in per cent: 2" rather than a bare figure.
fn percent_row(title: &str, subtitle: &str) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(0.0, PERCENT, 0.5);
    row.set_digits(1);
    row.set_title(title);
    row.set_subtitle(subtitle);
    a11y::label_spin_row(&row, &fill(gettext("{} in per cent"), &[title]));
    row
}

/// A `GtkStringList` of `labels`, for a combo row's model.
fn string_list(labels: impl IntoIterator<Item = glib::GString>) -> gtk::StringList {
    let list = gtk::StringList::new(&[]);
    for label in labels {
        list.append(&label);
    }
    list
}

/// The file patterns a format's chooser filter takes.
fn patterns(format: Format) -> [&'static str; 2] {
    match format {
        Format::Jpeg => ["*.jpg", "*.jpeg"],
        Format::Png => ["*.png", "*.PNG"],
    }
}

/// A path's file name as text, or the whole path when it has none.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A `Rgba8` in the float channels GTK's colour dialog uses.
fn to_rgba(color: Rgba8) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        1.0,
    )
}

/// The opaque `Rgba8` a colour dialog's value means.
///
/// Opaque by construction: the frame's backdrop is painted, not blended, so an export
/// is never transparent and the document refuses a translucent one (`Frame::validate`).
/// The chooser's alpha channel is therefore dropped rather than carried.
fn rgba8(color: &gtk::gdk::RGBA) -> Rgba8 {
    Rgba8 {
        r: (color.red() * 255.0).round() as u8,
        g: (color.green() * 255.0).round() as u8,
        b: (color.blue() * 255.0).round() as u8,
        a: 255,
    }
}
