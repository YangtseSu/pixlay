//! The utility pane: template, photo, framing and export.
//!
//! HIG `patterns/containers/utility-panes`: a vertical panel beside the main
//! view, hidden with `F9`, overlaying the content when the window is too narrow
//! for both (`AdwOverlaySplitView` is what makes that automatic). The controls are
//! libadwaita rows inside an `AdwPreferencesPage`, so the styling, the row rhythm
//! and the accessible names come from the platform instead of from classes of our
//! own.
//!
//! Every control does one of two things when the user changes it: it applies a
//! command on the window (one undo step, [`EditorWindow::apply`]) or it starts a
//! live gesture ([`EditorWindow::gesture`]) for the controls that produce a stream
//! of values while they are dragged. [`Sidebar::update`] then writes the document
//! back into the widgets, and `updating` is the flag that keeps that write from
//! reading as user input.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::templates;

use crate::a11y;
use crate::export::Settings;
use crate::i18n::{fill, gettext, ngettext};
use crate::window::{DEFAULT_EXPORT_PX, EditorWindow};

/// Smallest long edge the export form offers, in pixels. A floor for the row,
/// not a limit of the format: any positive grid is valid, and a minimum below
/// the picker's own `thumb` grid would make an export smaller than a preview.
pub const MIN_EXPORT_PX: u32 = 256;

/// Largest long edge the export form offers, in pixels: `12000² = 144 MP`, under
/// the 200 MP pixel budget (`MAX_CANVAS_PIXELS`) for a square grid — so every
/// template aspect the row can produce is inside the budget, whatever the shape.
pub const MAX_EXPORT_PX: u32 = 12000;

pub struct Sidebar {
    pub root: gtk::ScrolledWindow,

    template_combo: adw::ComboRow,
    /// Names of the templates the combo currently lists, in combo order.
    template_names: Rc<RefCell<Vec<String>>>,

    photo_group: adw::PreferencesGroup,
    photo_row: adw::ActionRow,
    clear_button: gtk::Button,

    framing_group: adw::PreferencesGroup,
    zoom_row: adw::SpinRow,
    rotation_scale: gtk::Scale,

    export_size: adw::SpinRow,
    format_combo: adw::ComboRow,
    export_path: adw::ActionRow,

    /// Set while [`Sidebar::update`] writes into widgets.
    updating: Rc<Cell<bool>>,
}

impl Sidebar {
    pub fn build(window: &EditorWindow) -> Self {
        let updating = Rc::new(Cell::new(false));

        // ---- template ------------------------------------------------------
        let template_combo = adw::ComboRow::builder()
            .title(gettext("Template"))
            .subtitle(gettext(
                "Only layouts matching the sheet's shape are listed",
            ))
            .build();
        a11y::label(&template_combo, &gettext("Template"));
        // The combo lists a *subset* of the library (the canvas's aspect ratio),
        // so the row index has to be mapped back to a name.
        let template_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        connect_combo(&template_combo, &updating, {
            let window = window.downgrade();
            let names = Rc::clone(&template_names);
            move |index| {
                let Some(name) = names.borrow().get(index).cloned() else {
                    return;
                };
                if let Some(window) = window.upgrade() {
                    window.set_template(&name);
                }
            }
        });

        // ---- photo ---------------------------------------------------------
        let photo_row = adw::ActionRow::builder().title(gettext("No photo")).build();
        let choose_button = gtk::Button::with_label(&gettext("Choose photo…"));
        choose_button.set_action_name(Some("win.add-photo"));
        a11y::label(
            &choose_button,
            &gettext("Choose a photo for the selected slot"),
        );
        let clear_button = gtk::Button::with_label(&gettext("Remove"));
        clear_button.set_action_name(Some("win.clear-photo"));
        a11y::label(
            &clear_button,
            &gettext("Remove the photo from the selected slot"),
        );
        let photo_buttons = linked_box(&[&choose_button, &clear_button]);
        let photo_actions = adw::ActionRow::new();
        photo_actions.add_suffix(&photo_buttons);
        let photo_group = group(gettext("Photo"), "");
        photo_group.add(&photo_row);
        photo_group.add(&photo_actions);

        // ---- framing -------------------------------------------------------
        let zoom_row = spin_row(gettext("Zoom"), &gettext("Zoom"), 0.1, 10.0, 0.05, 2);
        connect_spin(&zoom_row, &updating, {
            let window = window.downgrade();
            move |value| {
                if let Some(window) = window.upgrade() {
                    window.set_zoom(value);
                }
            }
        });

        let rotation_scale =
            gtk::Scale::with_range(gtk::Orientation::Horizontal, -180.0, 180.0, 0.5);
        rotation_scale.set_digits(1);
        rotation_scale.set_draw_value(true);
        rotation_scale.set_hexpand(true);
        rotation_scale.set_size_request(160, -1);
        a11y::label(&rotation_scale, &gettext("Straighten"));
        rotation_scale.update_property(&[
            gtk::accessible::Property::ValueMin(-180.0),
            gtk::accessible::Property::ValueMax(180.0),
        ]);
        let rotation_row = adw::ActionRow::builder()
            .title(gettext("Straighten"))
            .build();
        rotation_row.add_suffix(&rotation_scale);
        rotation_row.set_activatable_widget(Some(&rotation_scale));
        rotation_scale.connect_value_changed(glib::clone!(
            #[weak]
            window,
            #[weak]
            updating,
            move |scale: &gtk::Scale| {
                if updating.get() {
                    return;
                }
                // A slider produces a stream of values while it is dragged, so this
                // is a live gesture: the canvas follows it, the straightening
                // guides appear, and one command is committed once the value has
                // been quiet (`EditorWindow::straighten`).
                window.straighten(scale.value());
            }
        ));

        let reset_button = gtk::Button::with_label(&gettext("Reset framing"));
        reset_button.set_action_name(Some("win.reset-framing"));
        a11y::label(
            &reset_button,
            &gettext("Reset the framing of the selected slot"),
        );
        let reset_row = adw::ActionRow::new();
        reset_row.add_suffix(&reset_button);
        let framing_group = group(gettext("Framing"), "");
        framing_group.add(&zoom_row);
        framing_group.add(&rotation_row);
        framing_group.add(&reset_row);

        // Three rows, because the form answers three questions (S12c): which
        // format, how large a picture, and where. The quality row is the one size
        // parameter S12d leaves — a pixel count, which is how large the file is.
        let format_combo = adw::ComboRow::builder()
            .title(gettext("Format"))
            .model(&string_list(["JPEG", "PNG"]))
            .build();
        a11y::label(&format_combo, &gettext("Export format"));
        let export_size = spin_row(
            gettext("Quality"),
            &gettext("Export long edge in pixels"),
            f64::from(MIN_EXPORT_PX),
            f64::from(MAX_EXPORT_PX),
            64.0,
            0,
        );
        // A spin button otherwise starts at the bottom of its range, which would
        // make the row's minimum a new window's export size.
        export_size.set_value(f64::from(DEFAULT_EXPORT_PX));
        let export_path = adw::ActionRow::builder()
            .title(gettext("File"))
            .subtitle(gettext("Not chosen yet"))
            .build();
        let export_path_button = gtk::Button::with_label(&gettext("Choose…"));
        export_path_button.set_action_name(Some("win.choose-export-path"));
        a11y::label(
            &export_path_button,
            &gettext("Choose where the export is written"),
        );
        let export_path_row = adw::ActionRow::new();
        export_path_row.add_suffix(&export_path_button);
        let export_group = group(gettext("Export"), "");
        export_group.add(&format_combo);
        export_group.add(&export_size);
        export_group.add(&export_path);
        export_group.add(&export_path_row);

        // ---- the page -------------------------------------------------------
        let page = adw::PreferencesPage::new();
        page.add(&group_with(gettext("Template"), &[&template_combo]));
        page.add(&photo_group);
        page.add(&framing_group);
        page.add(&export_group);

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&page)
            .build();
        root.update_property(&[gtk::accessible::Property::Label(&gettext(
            "Editing controls",
        ))]);

        let sidebar = Self {
            root,
            template_combo,
            template_names,
            photo_group,
            photo_row,
            clear_button,
            framing_group,
            zoom_row,
            rotation_scale,
            export_size,
            format_combo,
            export_path,
            updating,
        };
        sidebar.update(window);
        sidebar
    }

    /// Pushes the document into the widgets.
    pub fn update(&self, window: &EditorWindow) {
        let doc = window.document();
        let selection = window.selection();
        self.updating.set(true);

        self.update_templates(&doc);

        let cell = selection.and_then(|slot| doc.cells.get(slot));
        self.photo_group.set_visible(selection.is_some());
        self.framing_group.set_visible(selection.is_some());
        if let (Some(slot), Some(cell)) = (selection, cell) {
            self.photo_row.set_title(&match &cell.source {
                Some(source) => source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| source.to_string_lossy().into_owned()),
                None => gettext("No photo"),
            });
            self.photo_row.set_subtitle(&match &cell.source {
                Some(source) => source.to_string_lossy().into_owned(),
                None => fill(gettext("Slot {} is empty"), &[slot]),
            });
            self.clear_button.set_sensitive(cell.source.is_some());
            let fitted = window.fitted_crop(slot).unwrap_or(cell.crop);
            self.zoom_row.set_value(fitted.zoom);
            self.rotation_scale.set_value(fitted.rotation_deg);
        }

        self.updating.set(false);
    }

    /// Lists the templates whose aspect ratio matches the document's own.
    fn update_templates(&self, doc: &pixlay_core::CollageDoc) {
        let wanted: Vec<String> = templates::names()
            .into_iter()
            .filter(|name| {
                templates::get(name)
                    .is_some_and(|template| (template.aspect - doc.template.aspect).abs() <= 1e-6)
            })
            .map(str::to_string)
            .collect();
        if *self.template_names.borrow() != wanted {
            let labels: Vec<String> = wanted
                .iter()
                .filter_map(|name| templates::get(name))
                .map(|template| {
                    format!(
                        "{} · {} · {}",
                        name_of(&template.name),
                        fill(
                            ngettext("{} slot", "{} slots", template.slots.len() as u32),
                            &[template.slots.len()],
                        ),
                        format_aspect(template.aspect)
                    )
                })
                .collect();
            self.template_combo.set_model(Some(&string_list(labels)));
            *self.template_names.borrow_mut() = wanted;
        }
        let selected = self
            .template_names
            .borrow()
            .iter()
            .position(|name| name == &doc.template.name)
            .unwrap_or(0);
        self.template_combo.set_selected(selected as u32);
    }

    /// The export settings as the widgets show them, with the path the window
    /// holds.
    pub fn settings(&self, path: PathBuf) -> Settings {
        Settings {
            long_edge: self.export_size.value().round() as u32,
            format: match self.format_combo.selected() {
                1 => pixlay_imaging::encode::Format::Png,
                _ => pixlay_imaging::encode::Format::Jpeg,
            },
            path,
        }
    }

    /// Puts the export form back into the state `settings` describes.
    ///
    /// The window calls this whenever the document or the chosen path changes, so
    /// the form, the path and what `settings()` reports cannot disagree; a test
    /// uses the same call to set a size before exporting from the background.
    pub fn show_settings(&self, settings: &Settings) {
        self.updating.set(true);
        self.export_size.set_value(f64::from(settings.long_edge));
        self.format_combo.set_selected(match settings.format {
            pixlay_imaging::encode::Format::Jpeg => 0,
            pixlay_imaging::encode::Format::Png => 1,
        });
        self.export_path
            .set_subtitle(&match settings.path.file_name() {
                Some(name) if !settings.path.as_os_str().is_empty() => {
                    name.to_string_lossy().into_owned()
                }
                _ => gettext("Not chosen yet"),
            });
        self.updating.set(false);
    }
}

fn group(title: String, description: impl Into<glib::GString>) -> adw::PreferencesGroup {
    adw::PreferencesGroup::builder()
        .title(title)
        .description(description)
        .build()
}

fn group_with(title: String, rows: &[&impl IsA<gtk::Widget>]) -> adw::PreferencesGroup {
    let group = group(title, "");
    for row in rows {
        group.add(*row);
    }
    group
}

fn linked_box(buttons: &[&gtk::Button]) -> gtk::Box {
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    layout.add_css_class("linked");
    for button in buttons {
        layout.append(*button);
    }
    layout
}

/// A spin row with a title, an accessible name on the spin button inside it, and
/// a range.
fn spin_row(
    title: String,
    label: &str,
    min: f64,
    max: f64,
    step: f64,
    digits: u32,
) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, step);
    row.set_digits(digits);
    row.set_title(&title);
    a11y::label_spin_row(&row, label);
    row
}

fn connect_spin(row: &adw::SpinRow, updating: &Rc<Cell<bool>>, changed: impl Fn(f64) + 'static) {
    let updating = Rc::clone(updating);
    row.connect_value_notify(move |row| {
        if updating.get() {
            return;
        }
        changed(row.value());
    });
}

fn connect_combo(
    row: &adw::ComboRow,
    updating: &Rc<Cell<bool>>,
    changed: impl Fn(usize) + 'static,
) {
    let updating = Rc::clone(updating);
    let selected = Cell::new(row.selected());
    row.connect_selected_notify(move |row| {
        if updating.get() {
            return;
        }
        // A combo emits a notification on every programmatic write as well; only
        // a real change is a user action.
        let index = row.selected();
        if selected.replace(index) == index {
            return;
        }
        changed(index as usize);
    });
}

fn string_list(labels: impl IntoIterator<Item = impl Into<glib::GString>>) -> gtk::StringList {
    let list = gtk::StringList::new(&[]);
    for label in labels {
        let label: glib::GString = label.into();
        list.append(&label);
    }
    list
}

fn name_of(name: &str) -> String {
    // Template names are identifiers from the library, not copy: they are never
    // translated (`AGENTS.md`, "Language conventions").
    name.to_string()
}

fn format_aspect(aspect: f64) -> String {
    for (width, height) in [(1.0, 1.0), (3.0, 2.0), (4.0, 3.0), (16.0, 9.0), (2.0, 3.0)] {
        if (width / height - aspect).abs() <= 1e-12 {
            return format!("{}:{}", width as i32, height as i32);
        }
    }
    format!("{aspect:.3}")
}
