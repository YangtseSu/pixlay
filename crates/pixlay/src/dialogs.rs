//! The document-level dialog: `Frame…` (S15, ruling 18).
//!
//! Ruling 18 removed the utility pane, and of its groups the frame's three settings had
//! no other home: they became an `AdwDialog` rather than permanent rows — HIG
//! `patterns/feedback/dialogs`, "Action Dialogs" (a header bar, a heading which
//! describes the action, and the affirmative button carrying an imperative verb).
//!
//! **It writes as its rows move**: the canvas behind it redraws, the change becomes one
//! undo step when the value stops moving, and `Ctrl+Z` is the way back — so its only
//! button is *Close*. A dialog that has already applied everything has nothing to
//! confirm, and a Cancel that had to unwind a stack of live edits would be a second
//! undo stack (`S14b · Ruling`, the same reason `+` does not remember the cell it
//! dropped).
//!
//! **The export's own dialog left in S25** (ruling 36): its two parameters moved into
//! the app's settings surface (`crate::settings`), its name row and folder chooser
//! became the platform's own save dialog (`crate::export::seed`), and the file's
//! extension is the settings' format's rather than a row's. What is left here is the
//! document's own dialog, which the export never was.
//!
//! The dialog holds no GTK object across a thread: the canvas redraws on the main
//! thread, and nothing here starts a worker.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{Frame, Rgba8};

use crate::a11y;
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

/// The `Frame…` dialog: the document's frame as three rows, in the document's own
/// order — gap, radius, colour (ruling 30, 2026-09-23).
pub struct FrameDialog {
    dialog: adw::Dialog,
    close: gtk::Button,
    gap: adw::SpinRow,
    radius: adw::SpinRow,
    color: gtk::ColorDialogButton,
    /// The dialog's own report of a value the document refused (S15h, PIX-020).
    ///
    /// A banner rather than a toast, because a toast would be shown by the *window*,
    /// which is behind this modal dialog and covered by its dim: the row that could
    /// not be applied is where the reason belongs. The rows' own numbers do not carry
    /// it — "100 %" is a legal-looking figure — and a screen reader reaches the
    /// banner as a label, so the refusal is not a colour.
    banner: adw::Banner,
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
        // The refused value is reported here, above the rows that hold it (S15h,
        // PIX-020): the group's own numbers cannot say why they did not apply.
        let banner = adw::Banner::new("");
        banner.set_revealed(false);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.add_top_bar(&banner);
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
            banner,
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
            #[weak]
            window,
            move |_| {
                let Some(dialog) = weak.upgrade() else {
                    return;
                };
                // Closing the dialog is a boundary (S15d, PIX-002): the frame change
                // that is still inside its quiet interval becomes the undo step it
                // looked like, and the window's own commit drops the timer so it
                // cannot fire again a moment later.
                window.commit();
                dialog.dialog.close();
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
                    dialog.apply(&window);
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
                dialog.apply(&window);
            }
        ));
        frame_dialog
    }

    /// Hands the rows' frame to the document, and answers a refusal where the value
    /// came from (S15h, PIX-020).
    ///
    /// On acceptance the banner goes away: the row's number *is* the document's. On a
    /// refusal the rows go back to the document's own frame — what the canvas is
    /// showing — so the dialog cannot display a value the document never took, and the
    /// banner says why with the message the core wrote (which names the offending
    /// slot).
    fn apply(&self, window: &EditorWindow) {
        // **The rows already say what the document says**: this is the echo of a seed
        // rather than an edit, and reporting it would clear the banner the refusal just
        // raised. Measured 2026-09-25: GTK defers the value notification a restore
        // causes to after the handler that restored it, so the echo arrives *after* the
        // refusal was reported and used to put the banner away again — the row snapped
        // back and the user was told nothing.
        if self.echoes_document(window) {
            return;
        }
        match window.set_frame(self.frame()) {
            Ok(()) => self.banner.set_revealed(false),
            Err(error) => {
                self.banner.set_title(&error.to_string());
                self.banner.set_revealed(true);
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

    /// Shows the dialog over `window`, with the document's own frame in its rows.
    pub fn present(&self, window: &EditorWindow) {
        self.banner.set_revealed(false);
        self.seed(window);
        self.dialog.present(Some(window));
    }

    /// The reason the last value was refused, while it is being shown (S15h,
    /// PIX-020); `None` when the dialog has nothing to report.
    ///
    /// The tests' handle on "the row says so": the banner's text is the core's own
    /// message, which names the offending slot.
    pub fn notice(&self) -> Option<String> {
        self.banner
            .is_revealed()
            .then(|| self.banner.title().to_string())
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
