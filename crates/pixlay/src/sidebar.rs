//! The utility pane: template, photo, framing, colour, text and export — the
//! sheet size is one of the export rows, because a physical size only means
//! something where the pixels are written.
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

use pixlay_core::{Anchor, Command, FilterPreset, Rgba8, TextMode, templates};

use crate::a11y;
use crate::export::{Settings, Size};
use crate::i18n::{fill, gettext, ngettext};
use crate::window::{DEFAULT_EXPORT_DPI, EditorWindow};

/// Long edges the sheet chooser offers, in millimetres.
///
/// The *shape* comes from the template, not from the paper: a template's geometry
/// is authored for one aspect ratio and `CollageDoc::validate` refuses a canvas
/// that disagrees with it, so a preset is a long edge plus the current template's
/// ratio (`CanvasSpec::with_ratio`). "A4" therefore means an A4-wide sheet of the
/// collage's own shape, which is what printing a 4:3 collage on A4 gives.
///
/// The row that offers them is a row of the export form, next to the resolution
/// (2026-09-22), because the long edge only becomes visible as pixels where the
/// export multiplies it by a DPI: the canvas pane draws the template's shape and
/// never shows the millimetres, so as a canvas setting the number looked inert.
pub const CANVAS_SIZES: [(&str, f64); 3] = [("A4", 297.0), ("A3", 420.0), ("A0", 1189.0)];

/// The anchors in the order the combo lists them.
const ANCHORS: [(Anchor, &str); 9] = [
    (Anchor::TopLeft, "Top left"),
    (Anchor::TopCenter, "Top centre"),
    (Anchor::TopRight, "Top right"),
    (Anchor::CenterLeft, "Left"),
    (Anchor::Center, "Centre"),
    (Anchor::CenterRight, "Right"),
    (Anchor::BottomLeft, "Bottom left"),
    (Anchor::BottomCenter, "Bottom centre"),
    (Anchor::BottomRight, "Bottom right"),
];

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

    colour_group: adw::PreferencesGroup,
    exposure_row: adw::SpinRow,
    saturation_row: adw::SpinRow,
    warmth_row: adw::SpinRow,
    filter_combo: adw::ComboRow,

    text_layer_combo: adw::ComboRow,
    text_remove: gtk::Button,
    text_content_row: adw::ActionRow,
    text_content: gtk::Entry,
    text_mode: adw::ComboRow,
    text_x: adw::SpinRow,
    text_y: adw::SpinRow,
    text_anchor: adw::ComboRow,
    text_step_x: adw::SpinRow,
    text_step_y: adw::SpinRow,
    text_size: adw::SpinRow,
    text_rotation: adw::SpinRow,
    text_colour_row: adw::ActionRow,
    text_colour: gtk::ColorDialogButton,
    text_slot: adw::SpinRow,

    size_combo: adw::ComboRow,
    size_mode: adw::ComboRow,
    export_size: adw::SpinRow,
    format_combo: adw::ComboRow,
    chroma_combo: adw::ComboRow,
    export_path: adw::ActionRow,

    /// Set while [`Sidebar::update`] writes into widgets.
    updating: Rc<Cell<bool>>,
    /// Which text layer the editor rows are bound to, or `None`.
    selected_layer: Rc<Cell<Option<usize>>>,
}

impl Sidebar {
    pub fn build(window: &EditorWindow) -> Self {
        let updating = Rc::new(Cell::new(false));
        let selected_layer = Rc::new(Cell::new(None));

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

        // ---- colour --------------------------------------------------------
        let exposure_row = spin_row(gettext("Exposure"), &gettext("Exposure"), 0.2, 5.0, 0.05, 2);
        let saturation_row = spin_row(
            gettext("Saturation"),
            &gettext("Saturation"),
            0.0,
            4.0,
            0.05,
            2,
        );
        let warmth_row = spin_row(gettext("Warmth"), &gettext("Warmth"), -1.0, 1.0, 0.05, 2);
        for (row, part) in [
            (&exposure_row, GradePart::Exposure),
            (&saturation_row, GradePart::Saturation),
            (&warmth_row, GradePart::Warmth),
        ] {
            connect_spin(row, &updating, {
                let window = window.downgrade();
                move |value| {
                    if let Some(window) = window.upgrade() {
                        window.set_grade_part(part, value);
                    }
                }
            });
        }
        let filter_combo = adw::ComboRow::builder()
            .title(gettext("Filter"))
            .model(&string_list(
                FilterPreset::ALL.into_iter().map(preset_label),
            ))
            .build();
        a11y::label(&filter_combo, &gettext("Filter"));
        connect_combo(&filter_combo, &updating, {
            let window = window.downgrade();
            move |index| {
                let Some(preset) = FilterPreset::ALL.get(index) else {
                    return;
                };
                if let Some(window) = window.upgrade() {
                    // A refused command has already been reported to the user.
                    let _ = window.apply(Command::SetFilter { filter: *preset });
                }
            }
        });
        let colour_group = group(gettext("Colour"), "");
        colour_group.add(&exposure_row);
        colour_group.add(&saturation_row);
        colour_group.add(&warmth_row);
        colour_group.add(&filter_combo);

        // ---- text ----------------------------------------------------------
        let text_layer_combo = adw::ComboRow::builder().title(gettext("Layer")).build();
        a11y::label(&text_layer_combo, &gettext("Text layer"));
        connect_combo(&text_layer_combo, &updating, {
            let window = window.downgrade();
            let selected_layer = Rc::clone(&selected_layer);
            move |index| {
                selected_layer.set(Some(index));
                if let Some(window) = window.upgrade() {
                    window.select_text(Some(index));
                }
            }
        });
        let text_add = gtk::Button::with_label(&gettext("Add text"));
        text_add.set_action_name(Some("win.add-text"));
        a11y::label(&text_add, &gettext("Add a text layer"));
        let text_remove = gtk::Button::with_label(&gettext("Remove text"));
        text_remove.set_action_name(Some("win.remove-text"));
        a11y::label(&text_remove, &gettext("Remove the selected text layer"));
        let text_buttons = linked_box(&[&text_add, &text_remove]);
        let text_buttons_row = adw::ActionRow::new();
        text_buttons_row.add_suffix(&text_buttons);

        let text_content = gtk::Entry::new();
        text_content.set_hexpand(true);
        text_content.update_property(&[
            gtk::accessible::Property::Label(&gettext("Text")),
            gtk::accessible::Property::Placeholder(&gettext(
                "Caption, with {date}, {filename} or {index}",
            )),
        ]);
        let text_content_row = adw::ActionRow::builder().title(gettext("Text")).build();
        text_content_row.add_suffix(&text_content);
        text_content_row.set_activatable_widget(Some(&text_content));
        // The layer is rewritten when the entry is done with — on Enter or when
        // the focus leaves — and not on every keystroke: one edit is one undo step.
        text_content.connect_activate(glib::clone!(
            #[weak]
            window,
            #[weak]
            updating,
            move |entry: &gtk::Entry| {
                if updating.get() {
                    return;
                }
                let text = entry.text().to_string();
                window.edit_text(move |layer| layer.content = text);
            }
        ));
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(glib::clone!(
            #[weak]
            window,
            #[weak]
            updating,
            #[weak]
            text_content,
            move |_focus: &gtk::EventControllerFocus| {
                if updating.get() {
                    return;
                }
                let text = text_content.text().to_string();
                window.edit_text(move |layer| layer.content = text);
            }
        ));
        text_content.add_controller(focus);

        let text_mode = adw::ComboRow::builder()
            .title(gettext("Layout"))
            .model(&string_list([gettext("Placed"), gettext("Tiled")]))
            .build();
        a11y::label(&text_mode, &gettext("Layout"));

        let text_x = spin_row(
            gettext("Horizontal"),
            &gettext("Horizontal position"),
            0.0,
            1.0,
            0.01,
            2,
        );
        let text_y = spin_row(
            gettext("Vertical"),
            &gettext("Vertical position"),
            0.0,
            1.0,
            0.01,
            2,
        );
        let text_anchor = adw::ComboRow::builder()
            .title(gettext("Anchor"))
            .model(&string_list(
                ANCHORS.into_iter().map(|(_, label)| gettext(label)),
            ))
            .build();
        a11y::label(&text_anchor, &gettext("Anchor"));
        let text_step_x = spin_row(
            gettext("Horizontal step"),
            &gettext("Horizontal step"),
            0.05,
            1.0,
            0.05,
            2,
        );
        let text_step_y = spin_row(
            gettext("Vertical step"),
            &gettext("Vertical step"),
            0.05,
            1.0,
            0.05,
            2,
        );
        let text_size = spin_row(gettext("Size"), &gettext("Text size"), 0.005, 0.5, 0.005, 3);
        let text_rotation = spin_row(
            gettext("Rotation"),
            &gettext("Text rotation"),
            -180.0,
            180.0,
            1.0,
            0,
        );
        let text_colour = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
        a11y::label(&text_colour, &gettext("Text colour"));
        let text_colour_row = adw::ActionRow::builder().title(gettext("Colour")).build();
        text_colour_row.add_suffix(&text_colour);
        text_colour_row.set_activatable_widget(Some(&text_colour));
        let text_slot = spin_row(
            gettext("Photo for {date}"),
            &gettext("Photo the date and file name come from"),
            -1.0,
            9.0,
            1.0,
            0,
        );
        // Every one of these rows rewrites the whole layer, which is one command.
        for row in [
            &text_x,
            &text_y,
            &text_step_x,
            &text_step_y,
            &text_size,
            &text_rotation,
            &text_slot,
        ] {
            connect_spin(row, &updating, {
                let window = window.downgrade();
                move |_| {
                    if let Some(window) = window.upgrade() {
                        window.edit_text_from_controls();
                    }
                }
            });
        }
        connect_combo(&text_mode, &updating, {
            let window = window.downgrade();
            move |_| {
                if let Some(window) = window.upgrade() {
                    window.edit_text_from_controls();
                }
            }
        });
        connect_combo(&text_anchor, &updating, {
            let window = window.downgrade();
            move |_| {
                if let Some(window) = window.upgrade() {
                    window.edit_text_from_controls();
                }
            }
        });
        text_colour.connect_rgba_notify(glib::clone!(
            #[weak]
            window,
            #[weak]
            updating,
            move |_button: &gtk::ColorDialogButton| {
                if updating.get() {
                    return;
                }
                window.edit_text_from_controls();
            }
        ));

        let text_group = group(gettext("Text"), "");
        text_group.add(&text_layer_combo);
        text_group.add(&text_buttons_row);
        text_group.add(&text_content_row);
        text_group.add(&text_mode);
        text_group.add(&text_x);
        text_group.add(&text_y);
        text_group.add(&text_anchor);
        text_group.add(&text_step_x);
        text_group.add(&text_step_y);
        text_group.add(&text_size);
        text_group.add(&text_rotation);
        text_group.add(&text_colour_row);
        text_group.add(&text_slot);

        // ---- export --------------------------------------------------------
        // The sheet size leads the form: with the resolution below it, it is what
        // the exported pixel grid is made of (`mm * dpi`), and in the long-edge
        // mode it is the DPI the file carries.
        let size_combo = adw::ComboRow::builder()
            .title(gettext("Sheet size"))
            .subtitle(gettext(
                "The physical size of the export; the template decides the shape",
            ))
            .model(&string_list(
                CANVAS_SIZES
                    .iter()
                    .map(|(name, mm)| format!("{name} — {mm:.0} mm")),
            ))
            .build();
        a11y::label(&size_combo, &gettext("Sheet size"));
        connect_combo(&size_combo, &updating, {
            let window = window.downgrade();
            move |index| {
                let Some((_, long_edge)) = CANVAS_SIZES.get(index) else {
                    return;
                };
                if let Some(window) = window.upgrade() {
                    window.set_long_edge_mm(*long_edge);
                }
            }
        });

        let size_mode = adw::ComboRow::builder()
            .title(gettext("Size"))
            .model(&string_list([
                gettext("Resolution (dpi)"),
                gettext("Long edge (pixels)"),
            ]))
            .build();
        a11y::label(&size_mode, &gettext("Export size"));
        let export_size = spin_row(
            gettext("Resolution"),
            &gettext("Export resolution in dots per inch"),
            72.0,
            600.0,
            1.0,
            0,
        );
        // A spin button otherwise starts at the bottom of its range, which would
        // make 72 dpi a new window's export resolution.
        export_size.set_value(f64::from(DEFAULT_EXPORT_DPI));

        let format_combo = adw::ComboRow::builder()
            .title(gettext("Format"))
            .model(&string_list(["JPEG", "PNG", "TIFF"]))
            .build();
        a11y::label(&format_combo, &gettext("Export format"));
        let chroma_combo = adw::ComboRow::builder()
            .title(gettext("Colour detail"))
            .model(&string_list(["4:4:4", "4:2:2", "4:2:0"]))
            .build();
        a11y::label(&chroma_combo, &gettext("JPEG colour detail"));
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
        export_group.add(&size_combo);
        export_group.add(&size_mode);
        export_group.add(&export_size);
        export_group.add(&format_combo);
        export_group.add(&chroma_combo);
        export_group.add(&export_path);
        export_group.add(&export_path_row);

        // The two sizing modes are two different requests, so the row switches
        // between them: a DPI range and a pixel range, and the label follows.
        size_mode.connect_selected_notify(glib::clone!(
            #[weak]
            updating,
            #[weak]
            export_size,
            move |row: &adw::ComboRow| {
                if updating.get() {
                    return;
                }
                let dpi = row.selected() == 0;
                export_size.set_range(
                    if dpi { 72.0 } else { 1.0 },
                    if dpi { 600.0 } else { 30000.0 },
                );
                export_size.set_value(if dpi {
                    f64::from(DEFAULT_EXPORT_DPI)
                } else {
                    4000.0
                });
                export_size.set_title(&if dpi {
                    gettext("Resolution")
                } else {
                    gettext("Long edge")
                });
                export_size.update_property(&[gtk::accessible::Property::Label(&if dpi {
                    gettext("Export resolution in dots per inch")
                } else {
                    gettext("Export long edge in pixels")
                })]);
            }
        ));
        // The chroma row only means something for JPEG; the CLI refuses the flag
        // for the other formats rather than dropping it, so the GUI hides it.
        format_combo.connect_selected_notify(glib::clone!(
            #[weak]
            updating,
            #[weak]
            chroma_combo,
            move |row: &adw::ComboRow| {
                if updating.get() {
                    return;
                }
                chroma_combo.set_visible(row.selected() == 0);
            }
        ));

        // ---- the page -------------------------------------------------------
        let page = adw::PreferencesPage::new();
        page.add(&group_with(gettext("Template"), &[&template_combo]));
        page.add(&photo_group);
        page.add(&framing_group);
        page.add(&colour_group);
        page.add(&text_group);
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
            colour_group,
            exposure_row,
            saturation_row,
            warmth_row,
            filter_combo,
            text_layer_combo,
            text_remove,
            text_content_row,
            text_content,
            text_mode,
            text_x,
            text_y,
            text_anchor,
            text_step_x,
            text_step_y,
            text_size,
            text_rotation,
            text_colour_row,
            text_colour,
            text_slot,
            size_combo,
            size_mode,
            export_size,
            format_combo,
            chroma_combo,
            export_path,
            updating,
            selected_layer,
        };
        sidebar.update(window);
        sidebar
    }

    /// Pushes the document into the widgets.
    pub fn update(&self, window: &EditorWindow) {
        let doc = window.document();
        let selection = window.selection();
        self.updating.set(true);

        let long_edge = doc.canvas.width_mm.max(doc.canvas.height_mm);
        let size_index = CANVAS_SIZES
            .iter()
            .position(|(_, mm)| (mm - long_edge).abs() < 0.5)
            .unwrap_or(0);
        self.size_combo.set_selected(size_index as u32);
        self.update_templates(&doc);

        let cell = selection.and_then(|slot| doc.cells.get(slot));
        self.photo_group.set_visible(selection.is_some());
        self.framing_group.set_visible(selection.is_some());
        self.colour_group.set_visible(selection.is_some());
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
            self.exposure_row.set_value(cell.grade.factor);
            self.saturation_row.set_value(cell.grade.saturation);
            self.warmth_row.set_value(cell.grade.delta);
        }
        self.filter_combo.set_selected(
            FilterPreset::ALL
                .iter()
                .position(|preset| *preset == doc.filter)
                .unwrap_or(0) as u32,
        );

        self.update_text(&doc);

        self.updating.set(false);
    }

    /// Lists the templates whose aspect ratio matches the canvas.
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

    fn update_text(&self, doc: &pixlay_core::CollageDoc) {
        let layers: Vec<String> = doc
            .text
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let kind = match layer.mode {
                    TextMode::Free { .. } => gettext("Placed"),
                    TextMode::Tiled { .. } => gettext("Tiled"),
                };
                format!(
                    "{} {index} — {kind}: {}",
                    gettext("Text"),
                    preview(&layer.content)
                )
            })
            .collect();
        self.text_layer_combo.set_model(Some(&string_list(layers)));

        let selected = self
            .selected_layer
            .get()
            .filter(|index| *index < doc.text.len());
        self.text_layer_combo
            .set_selected(selected.map_or(u32::MAX, |index| index as u32));
        self.text_remove.set_sensitive(selected.is_some());
        for widget in [
            self.text_content_row.clone().upcast::<gtk::Widget>(),
            self.text_mode.clone().upcast(),
            self.text_x.clone().upcast(),
            self.text_y.clone().upcast(),
            self.text_anchor.clone().upcast(),
            self.text_step_x.clone().upcast(),
            self.text_step_y.clone().upcast(),
            self.text_size.clone().upcast(),
            self.text_rotation.clone().upcast(),
            self.text_colour_row.clone().upcast(),
            self.text_slot.clone().upcast(),
        ] {
            widget.set_visible(selected.is_some());
        }
        let Some(layer) = selected.and_then(|index| doc.text.get(index)) else {
            return;
        };
        self.text_content.set_text(&layer.content);
        let tiled = matches!(layer.mode, TextMode::Tiled { .. });
        self.text_mode.set_selected(u32::from(tiled));
        self.text_x.set_visible(!tiled);
        self.text_y.set_visible(!tiled);
        self.text_anchor.set_visible(!tiled);
        self.text_step_x.set_visible(tiled);
        self.text_step_y.set_visible(tiled);
        match layer.mode {
            TextMode::Free { position, anchor } => {
                self.text_x.set_value(position.x);
                self.text_y.set_value(position.y);
                let index = ANCHORS
                    .iter()
                    .position(|(candidate, _)| *candidate == anchor)
                    .unwrap_or(4);
                self.text_anchor.set_selected(index as u32);
            }
            TextMode::Tiled { step } => {
                self.text_step_x.set_value(step.0);
                self.text_step_y.set_value(step.1);
            }
        }
        self.text_size.set_value(layer.size_rel);
        self.text_rotation.set_value(layer.rotation_deg);
        self.text_colour.set_rgba(&rgba_to_gdk(layer.color));
        self.text_slot
            .set_range(-1.0, (doc.template.slots.len() as f64) - 1.0);
        self.text_slot
            .set_value(layer.source_slot.map_or(-1.0, |slot| slot as f64));
    }

    /// The export settings as the widgets show them, with the path the window
    /// holds.
    pub fn settings(&self, path: PathBuf) -> Settings {
        Settings {
            size: if self.size_mode.selected() == 0 {
                Size::Dpi(self.export_size.value().round() as u32)
            } else {
                Size::LongEdge(self.export_size.value().round() as u32)
            },
            format: match self.format_combo.selected() {
                1 => pixlay_imaging::encode::Format::Png,
                2 => pixlay_imaging::encode::Format::Tiff,
                _ => pixlay_imaging::encode::Format::Jpeg,
            },
            chroma: match self.chroma_combo.selected() {
                1 => pixlay_imaging::encode::Chroma::HorizontalHalf,
                2 => pixlay_imaging::encode::Chroma::Quarter,
                _ => pixlay_imaging::encode::Chroma::Full,
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
        let dpi = matches!(settings.size, Size::Dpi(_));
        self.size_mode.set_selected(u32::from(!dpi));
        self.export_size.set_range(
            if dpi { 72.0 } else { 1.0 },
            if dpi { 600.0 } else { 30000.0 },
        );
        self.export_size.set_value(match settings.size {
            Size::Dpi(dpi) => f64::from(dpi),
            Size::LongEdge(pixels) => f64::from(pixels),
        });
        self.export_size.set_title(&if dpi {
            gettext("Resolution")
        } else {
            gettext("Long edge")
        });
        self.format_combo.set_selected(match settings.format {
            pixlay_imaging::encode::Format::Jpeg => 0,
            pixlay_imaging::encode::Format::Png => 1,
            pixlay_imaging::encode::Format::Tiff => 2,
        });
        self.chroma_combo
            .set_visible(settings.format == pixlay_imaging::encode::Format::Jpeg);
        self.chroma_combo.set_selected(match settings.chroma {
            pixlay_imaging::encode::Chroma::Full => 0,
            pixlay_imaging::encode::Chroma::HorizontalHalf => 1,
            pixlay_imaging::encode::Chroma::Quarter => 2,
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

    pub fn selected_layer(&self) -> Option<usize> {
        self.selected_layer.get()
    }

    pub fn select_layer(&self, index: Option<usize>) {
        self.selected_layer.set(index);
    }

    /// The text editor's controls, as one layer.
    pub fn text_controls(&self) -> pixlay_core::TextLayer {
        let mode = if self.text_mode.selected() == 0 {
            let anchor = ANCHORS
                .get(self.text_anchor.selected() as usize)
                .map(|(anchor, _)| *anchor)
                .unwrap_or(Anchor::Center);
            TextMode::Free {
                position: pixlay_core::Point::new(self.text_x.value(), self.text_y.value()),
                anchor,
            }
        } else {
            TextMode::Tiled {
                step: (self.text_step_x.value(), self.text_step_y.value()),
            }
        };
        let rgba = self.text_colour.rgba();
        let slot = self.text_slot.value().round();
        pixlay_core::TextLayer {
            content: self.text_content.text().to_string(),
            mode,
            size_rel: self.text_size.value(),
            rotation_deg: self.text_rotation.value(),
            color: Rgba8 {
                r: (rgba.red() * 255.0).round() as u8,
                g: (rgba.green() * 255.0).round() as u8,
                b: (rgba.blue() * 255.0).round() as u8,
                a: (rgba.alpha() * 255.0).round() as u8,
            },
            source_slot: if slot < 0.0 {
                None
            } else {
                Some(slot as usize)
            },
        }
    }
}

/// Which of a cell's three grading numbers a control edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradePart {
    Exposure,
    Saturation,
    Warmth,
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

fn preset_label(preset: FilterPreset) -> String {
    match preset {
        FilterPreset::None => gettext("None"),
        FilterPreset::Warm => gettext("Warm"),
        FilterPreset::Cool => gettext("Cool"),
        FilterPreset::Mono => gettext("Black and white"),
        FilterPreset::Vivid => gettext("Vivid"),
        FilterPreset::Fade => gettext("Fade"),
    }
}

fn name_of(name: &str) -> String {
    // Template names are identifiers from the library, not copy: they are never
    // translated (`AGENTS.md`, "Language conventions").
    name.to_string()
}

fn preview(content: &str) -> String {
    let text = content.replace('\n', " ");
    if text.chars().count() > 24 {
        format!("{}…", text.chars().take(24).collect::<String>())
    } else {
        text
    }
}

fn format_aspect(aspect: f64) -> String {
    for (width, height) in [(1.0, 1.0), (3.0, 2.0), (4.0, 3.0), (16.0, 9.0), (2.0, 3.0)] {
        if (width / height - aspect).abs() <= 1e-12 {
            return format!("{}:{}", width as i32, height as i32);
        }
    }
    format!("{aspect:.3}")
}

fn rgba_to_gdk(color: Rgba8) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    )
}
