// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The app's one dialog: the document's frame above the app's export settings
//! (S15, ruling 18; merged by S25b).
//!
//! Ruling 18 removed the utility pane, and the frame's three settings had no other home:
//! they became rows in a dialog rather than permanent controls. S25 moved the export's
//! two parameters into a surface of their own, and **S25b merged the two** (the human's
//! ruling of 2026-09-26): one `AdwPreferencesDialog` titled *Preferences*, the frame's
//! group above the export's, behind both entry points — the header bar's frame button
//! (`win.frame`) and the menu's *Preferences* item (`app.settings`, `Ctrl+,`). HIG
//! `patterns/containers/windows` allows exactly this: a secondary window "can contain
//! information and preferences that are relevant to the entire app, or … information and
//! options for a single content item".
//!
//! **The two groups write differently, and that is the point.** The frame's rows write
//! *live*: the canvas redraws behind the dialog, the change becomes one undo step when
//! the value stops moving, and `Ctrl+Z` is the way back. A value the document refuses is
//! reported inside the dialog — libadwaita's own toast surface, `add_toast`, because a
//! window toast would be behind the modal — and the row goes back to the document's own
//! number. The export's rows are the app's settings: a move writes the settings file
//! (`crate::settings`) and touches no document.
//!
//! The dialog holds no GTK object across a thread: the canvas redraws on the main
//! thread, and nothing here starts a worker.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{Frame, Rgba8};
use pixlay_imaging::encode::Format;

use crate::a11y;
use crate::export::{MAX_EXPORT_PX, MIN_EXPORT_PX};
use crate::i18n::{fill, gettext};
use crate::window::EditorWindow;

/// The frame's own lengths are typed as per cent of the collage's height.
///
/// The document holds a fraction (`frame.gapRel` / `radiusRel`) and the CLI takes it
/// that way (`--gap 0.02`); a row is a number a person types, and "2" reads as a share
/// where "0.02" reads as a mystery. The conversion lives here and nowhere else.
const PERCENT: f64 = 100.0;

/// How close a row's value has to be to the document's for the two to be the same
/// frame (S15h, PIX-020).
///
/// A row is a whole percentage with one digit, so a fraction seeded into it and read
/// back differs by at most 5e-5 — and the smallest move a user can make is 0.5 %, a
/// hundred times that. A comparison without one would call a seed's own echo a change.
const ROW_EPSILON: f64 = 1e-6;

/// The formats the export's row offers, in the row's own order.
///
/// The row's index *is* the index into this table, so the two cannot drift.
const FORMATS: [Format; 2] = [Format::Jpeg, Format::Png];

/// The formats' names, which are identifiers and never translated (`AGENTS.md`,
/// "Language conventions"): a user reads "PNG" and a file carries `.png`.
const FORMAT_NAMES: [&str; 2] = ["JPEG", "PNG"];

/// The step the long edge moves in, in pixels: 100 is a round number a person can type
/// over, and the bounds are the export's own (`MIN_EXPORT_PX` / `MAX_EXPORT_PX`).
const SIZE_STEP: u32 = 100;

/// The app's one dialog: the frame's three rows and the export's two (S25b).
///
/// The frame's rows are the document's, in the document's own order — gap, radius,
/// colour (ruling 30, 2026-09-23) — and the export's are the app's own settings
/// (S25, rulings 36 and 39). The document's group comes first, because the collage is
/// what the window is about.
pub struct SettingsDialog {
    dialog: adw::PreferencesDialog,
    gap: adw::SpinRow,
    radius: adw::SpinRow,
    color: gtk::ColorDialogButton,
    format: adw::ComboRow,
    long_edge: adw::SpinRow,
    /// The last message this dialog showed about a value the document refused (S15h,
    /// PIX-020).
    ///
    /// The toast itself is libadwaita's own surface *inside* the dialog
    /// (`AdwPreferencesDialog::add_toast`), and a toast is transient — so the message
    /// is kept here as well, exactly as the window keeps `last_toast`, because "the
    /// row says why it did not apply" is a claim a test has to be able to read.
    notice: RefCell<Option<String>>,
    /// Set while this module writes the rows, so that seeding the dialog from the
    /// document and the settings is not read back as a user editing it (the retired
    /// pane's own idiom).
    updating: Rc<Cell<bool>>,
}

impl SettingsDialog {
    /// Builds the dialog, which the header bar's frame button and the menu's
    /// *Preferences* item both present.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        // ---- the document's frame (S15) --------------------------------------
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
        let frame_group = adw::PreferencesGroup::builder()
            .title(gettext("Frame"))
            // The unit belongs here rather than on both rows: the two lengths are the
            // same measure of the same canvas.
            .description(gettext("Both lengths are a share of the collage's height"))
            .build();
        frame_group.add(&gap);
        frame_group.add(&radius);
        frame_group.add(&color_row);

        // ---- the app's export settings (S25) ---------------------------------
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
        let export_group = adw::PreferencesGroup::builder()
            .title(gettext("Export"))
            .description(gettext("The format and the size of an export"))
            .build();
        export_group.add(&format);
        export_group.add(&long_edge);

        let page = adw::PreferencesPage::new();
        page.set_title(&gettext("Preferences"));
        page.add(&frame_group);
        page.add(&export_group);

        let dialog = adw::PreferencesDialog::new();
        dialog.set_title(&gettext("Preferences"));
        dialog.add(&page);

        let settings_dialog = Rc::new(Self {
            dialog,
            gap,
            radius,
            color,
            format,
            long_edge,
            notice: RefCell::new(None),
            updating: Rc::new(Cell::new(false)),
        });
        // The handlers live on the dialog's own children, so they hold *weak*
        // references to it: a strong one would be a cycle (the dialog owns the rows, the
        // rows own the handler) and the dialog would outlive the window that made it.
        //
        // The frame's rows are live: every settled change is a document edit, and the
        // canvas behind the dialog redraws (`EditorWindow::set_frame` keeps it pending
        // while the value moves and commits it once it stops).
        for row in [&settings_dialog.gap, &settings_dialog.radius] {
            let weak = Rc::downgrade(&settings_dialog);
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
                    dialog.apply(&window);
                }
            ));
        }
        let weak = Rc::downgrade(&settings_dialog);
        settings_dialog.color.connect_rgba_notify(glib::clone!(
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
                dialog.apply(&window);
            }
        ));
        // The export's rows are the app's settings: a move is written to the settings
        // file and nothing about the document changes.
        let weak = Rc::downgrade(&settings_dialog);
        settings_dialog.format.connect_selected_notify(glib::clone!(
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
                dialog.remember(&window);
            }
        ));
        let weak = Rc::downgrade(&settings_dialog);
        settings_dialog.long_edge.connect_value_notify(glib::clone!(
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
                dialog.remember(&window);
            }
        ));
        // Closing the dialog is a boundary (S15d, PIX-002): the frame change that is
        // still inside its quiet interval becomes the undo step it looked like. The
        // quiet timer would commit it anyway; this is what makes "close" immediate.
        let weak = Rc::downgrade(&settings_dialog);
        settings_dialog.dialog.connect_closed(glib::clone!(
            #[weak]
            window,
            #[strong]
            weak,
            move |_| {
                if weak.upgrade().is_some() {
                    window.commit();
                }
            }
        ));
        settings_dialog
    }

    /// Hands the rows' frame to the document, and answers a refusal where the value
    /// came from (S15h, PIX-020).
    ///
    /// On acceptance the report goes away: the row's number *is* the document's. On a
    /// refusal the rows go back to the document's own frame — what the canvas is
    /// showing — so the dialog cannot display a value the document never took, and the
    /// toast says why with the message the core wrote (which names the offending slot).
    fn apply(&self, window: &EditorWindow) {
        // **The rows already say what the document says**: this is the echo of a seed
        // rather than an edit, and reporting it would clear the notice the refusal just
        // raised. Measured 2026-09-25: GTK defers the value notification a restore
        // causes to after the handler that restored it, so the echo arrives *after* the
        // refusal was reported and used to put the notice away again — the row snapped
        // back and the user was told nothing.
        if self.echoes_document(window) {
            return;
        }
        match window.set_frame(self.frame()) {
            Ok(()) => *self.notice.borrow_mut() = None,
            Err(error) => {
                let message = error.to_string();
                *self.notice.borrow_mut() = Some(message.clone());
                self.dialog.add_toast(adw::Toast::new(&message));
                self.seed(window);
            }
        }
    }

    /// Whether the rows carry the document's own frame, within what a row can express.
    ///
    /// The comparison needs a tolerance because a row is a whole percentage with one
    /// digit: seeding the rows from the document and reading them back can differ from
    /// the document's fraction by at most 5e-5, while the smallest move a user can make
    /// is 100 times that — so nothing a user did is swallowed.
    fn echoes_document(&self, window: &EditorWindow) -> bool {
        let frame = self.frame();
        let document = window.document().frame;
        (frame.gap_rel - document.gap_rel).abs() < ROW_EPSILON
            && (frame.radius_rel - document.radius_rel).abs() < ROW_EPSILON
            && frame.color == document.color
    }

    /// Writes the export rows into the app's settings (S25).
    ///
    /// The settings are the window's, written through the one writer
    /// (`EditorWindow::remember_settings`), so the rows, the window and the file cannot
    /// drift apart.
    fn remember(&self, window: &EditorWindow) {
        let mut settings = window.settings();
        settings.format = FORMATS[self.format_index()];
        settings.long_edge = self.long_edge.value().round() as u32;
        window.remember_settings(&settings);
    }

    /// Shows the dialog over `window`, with the document's frame and the app's settings
    /// in its rows.
    pub fn present(&self, window: &EditorWindow) {
        *self.notice.borrow_mut() = None;
        self.seed(window);
        self.dialog.present(Some(window));
    }

    /// The reason the last value was refused, while it is being shown (S15h,
    /// PIX-020); `None` when the dialog has nothing to report.
    ///
    /// The tests' handle on "the row says so": the message is the core's own, which
    /// names the offending slot.
    pub fn notice(&self) -> Option<String> {
        self.notice.borrow().clone()
    }

    /// The dialog itself, for the tests and the HIG checks.
    pub fn widget(&self) -> adw::PreferencesDialog {
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

    pub fn format_row(&self) -> adw::ComboRow {
        self.format.clone()
    }

    pub fn long_edge_row(&self) -> adw::SpinRow {
        self.long_edge.clone()
    }

    /// The frame the three document rows describe.
    pub fn frame(&self) -> Frame {
        Frame {
            gap_rel: self.gap.value() / PERCENT,
            radius_rel: self.radius.value() / PERCENT,
            color: rgba8(&self.color.rgba()),
        }
    }

    /// Puts the document's own frame and the app's own settings into the rows.
    ///
    /// The document and the settings are the authority rather than the dialog's last
    /// state: either can change without the dialog being touched (an undo, a project
    /// opened, the CLI's `edit --gap`, an export that remembered its folder), and a row
    /// that followed its own history would show a number nothing else has.
    pub fn seed(&self, window: &EditorWindow) {
        let frame = window.document().frame;
        let settings = window.settings();
        let index = FORMATS
            .iter()
            .position(|format| *format == settings.format)
            .unwrap_or(0) as u32;
        self.updating.set(true);
        self.gap.set_value(frame.gap_rel * PERCENT);
        self.radius.set_value(frame.radius_rel * PERCENT);
        self.color.set_rgba(&to_rgba(frame.color));
        self.format.set_selected(index);
        self.long_edge.set_value(f64::from(settings.long_edge));
        self.updating.set(false);
    }

    /// The index of the selected format, clamped to the table.
    fn format_index(&self) -> usize {
        (self.format.selected() as usize).min(FORMATS.len() - 1)
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
