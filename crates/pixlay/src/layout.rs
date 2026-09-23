//! The layout stage: every candidate layout of the current photo count, drawn with
//! the user's own photos, plus the count control that decides what that count is.
//!
//! Stage 3 of the main path (`AGENTS.md`: `open → pick 2–9 photos → pick a layout →
//! adjust → export`), and **a band on the document's page rather than a third
//! page**: a second `AdwNavigationPage` would have to own a second canvas, and
//! S15's compose controls attach to the canvas this band sits under
//! (`docs/2026-09-22-STEPS.md`, "S14 · The layout stage").
//!
//! # The candidates are real documents
//!
//! The list is `Selection::layouts()` — every template with exactly the photo
//! count, in library order (`S14 · Ruling`, 2026-09-23) — and each one is drawn as
//! a **real `CollageDoc`**: the document in the editor with
//! `Command::SetTemplate` applied, so the cells that survive keep their photo and
//! framing. Its thumbnail is therefore `pixlay_render::render_rgb8` of that
//! document, the same call `pixlay-render render` makes, which is what makes "the
//! gallery is not a second renderer" a comparison of two calls rather than of two
//! implementations.
//!
//! # What it costs, and what it must not cost
//!
//! The gallery renders on the *canvas's* decode worker ([`crate::decode`]), one
//! thread and one [`Preview`](pixlay_imaging::Preview), and it asks that worker for
//! the copies **at the canvas's own preview-grade edge**
//! (`Preview::build_at_source_edge`) — so the band's own builds decode nothing the
//! canvas has not already decoded, however many candidates it lists. Measured
//! 2026-09-23 (`docs/CONTRACT.md` §8, "S14"): **0** decodes for a three-candidate
//! band, 74.6 ms to rebuild it (`--release`), and pixels within **0.083** RMSE of a
//! full-resolution render of the same candidates.
//!
//! # The strip
//!
//! Candidates never wrap: the strip is a horizontal `GtkBox` inside a scroller
//! whose vertical policy is `Never`, so a candidate is one cell tall whatever the
//! window does, and a wide library scrolls sideways. A cell is
//! [`THUMB_BOX`] whatever the layout's aspect — a 2:3 layout and a 16:9 one are the
//! same cell across and the picture is fitted inside it — because the band's height
//! is taken from the canvas, and a cell that followed its layout's own shape would
//! make the band taller than the tallest layout in the library.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;

use pixlay_core::{MAX_PHOTOS, MIN_PHOTOS, PixelSize};
use pixlay_render::Rgb8Image;

use crate::a11y;
use crate::i18n::{fill, gettext, ngettext};
use crate::picture::Picture;
use crate::window::EditorWindow;

/// The box a candidate's thumbnail is drawn inside, in logical pixels.
///
/// One cell for every candidate, whatever the layout's aspect: the band's height is
/// taken from the canvas above it, and the library's tallest shape is 2:3 — a cell
/// that followed its own layout's proportions would make the band 192 px tall for a
/// three-photo column and 72 for a 16:9 strip. 128x96 is the largest box that
/// leaves the canvas the majority of the page at the default 1100x760 window.
pub const THUMB_BOX: (i32, i32) = (128, 96);

/// The grid one candidate is rendered at: the largest grid with `aspect`'s shape
/// that fits inside [`THUMB_BOX`].
///
/// This is `canvas::preferred_grid`'s arithmetic with a fixed box instead of a
/// widget, and for the same reason: the render is a real render at a smaller size
/// (`docs/CONTRACT.md` §5), so a candidate's pixels are comparable with
/// `pixlay-render render` of the same document at the same grid.
pub fn thumb_grid(aspect: f64) -> PixelSize {
    let (box_width, box_height) = (f64::from(THUMB_BOX.0), f64::from(THUMB_BOX.1));
    let (width, height) = if box_width / box_height > aspect {
        (box_height * aspect, box_height)
    } else {
        (box_width, box_width / aspect)
    };
    PixelSize {
        width: (width.round() as i32).max(1),
        height: (height.round() as i32).max(1),
    }
}

/// One rendered candidate, as the worker hands it back.
///
/// Plain data on purpose: it crosses the thread boundary
/// (`glib::MainContext::invoke`), and a GTK object never leaves the main thread.
pub struct Candidate {
    /// The template's name, which is how a caller names it (`edit --template`).
    pub template: String,
    pub image: Rgb8Image,
}

/// The band: the count control, the strip, and the state the strip is showing.
pub struct Gallery {
    /// The band's own root, which the editor page appends.
    root: gtk::Box,
    /// `− / N photos / +`: the count and the layout move together, so they are one
    /// control at the start of the band.
    minus: gtk::Button,
    plus: gtk::Button,
    count: gtk::Label,
    /// The strip's scroller, whose vertical policy is `Never`.
    strip: gtk::ScrolledWindow,
    /// The candidate cells, in library order.
    cells: gtk::Box,
    /// What the strip shows when the count has no layout: the editor cannot be
    /// reached with fewer than two photos, but a per-cell clear can empty the
    /// document underneath it.
    placeholder: gtk::ToggleButton,
    /// The candidate buttons by template name, and the pictures behind them.
    buttons: RefCell<HashMap<String, gtk::ToggleButton>>,
    pictures: RefCell<HashMap<String, Rc<Picture>>>,
    /// The strip's order, which is the library's.
    order: RefCell<Vec<String>>,
    /// Set while this module writes the buttons' own state, so that a highlight
    /// does not read back as a user choosing a layout (the picker's own idiom).
    syncing: Rc<Cell<bool>>,
}

impl Gallery {
    /// Builds the band.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let minus = icon_button("list-remove-symbolic", &gettext("Remove the last photo"));
        let plus = icon_button("list-add-symbolic", &gettext("Add a photo"));
        let count = gtk::Label::new(None);
        count.add_css_class("dim-label");
        // The count is a readout, not a control, but it is also the only place the
        // number of photos in the collage is written down.
        count.update_property(&[gtk::accessible::Property::Label(&gettext(
            "Photos in the collage",
        ))]);

        let control = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        control.set_valign(gtk::Align::Center);
        control.set_margin_start(12);
        control.set_margin_end(12);
        control.append(&minus);
        control.append(&count);
        control.append(&plus);

        let cells = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        cells.set_margin_top(6);
        cells.set_margin_bottom(6);
        cells.set_margin_start(6);
        cells.set_margin_end(6);

        let placeholder = placeholder_cell();
        cells.append(&placeholder);

        let strip = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&cells)
            .hexpand(true)
            .build();
        a11y::label(&strip, &gettext("Layouts"));

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.append(&control);
        root.append(&strip);

        let gallery = Rc::new(Self {
            root,
            minus,
            plus,
            count,
            strip,
            cells,
            placeholder,
            buttons: RefCell::new(HashMap::new()),
            pictures: RefCell::new(HashMap::new()),
            order: RefCell::new(Vec::new()),
            syncing: Rc::new(Cell::new(false)),
        });

        gallery.minus.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| window.remove_photo()
        ));
        gallery.plus.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| window.add_photo()
        ));
        gallery
    }

    // ---- what the window and the tests read --------------------------------

    /// The band's root widget.
    pub fn root(&self) -> gtk::Box {
        self.root.clone()
    }

    /// The strip's scroller, for the geometry a test may measure.
    pub fn strip(&self) -> gtk::ScrolledWindow {
        self.strip.clone()
    }

    /// The `−` control.
    pub fn minus_button(&self) -> gtk::Button {
        self.minus.clone()
    }

    /// The `+` control.
    pub fn plus_button(&self) -> gtk::Button {
        self.plus.clone()
    }

    /// The count readout.
    pub fn count_label(&self) -> gtk::Label {
        self.count.clone()
    }

    /// The candidates the strip lists, in library order.
    pub fn candidates(&self) -> Vec<String> {
        self.order.borrow().clone()
    }

    /// The candidate the document is on, if the strip holds it.
    ///
    /// `None` when the current layout is not one of the candidates, which a
    /// per-cell clear can bring about: the strip lists the layouts with the *photo*
    /// count, and a document with an empty cell has a layout with more slots.
    pub fn selected(&self) -> Option<String> {
        self.buttons
            .borrow()
            .values()
            .find(|button| button.is_active())
            .map(|button| button.widget_name().to_string())
    }

    /// One candidate's pixels, which is what the pixel criterion compares.
    pub fn thumbnail(&self, template: &str) -> Option<(i32, i32, Vec<u8>)> {
        let pictures = self.pictures.borrow();
        let picture = pictures.get(template)?;
        Some((picture.width(), picture.height(), picture.bytes().to_vec()))
    }

    /// The cell bound to a candidate, for the tests and the HIG checks.
    pub fn cell(&self, template: &str) -> Option<gtk::ToggleButton> {
        self.buttons.borrow().get(template).cloned()
    }

    // ---- what the window writes -------------------------------------------

    /// Replaces the strip with `candidates`, and highlights the document's layout.
    pub fn show(&self, window: &EditorWindow, candidates: Vec<Candidate>) {
        self.syncing.set(true);
        while let Some(child) = self.cells.first_child() {
            self.cells.remove(&child);
        }
        self.buttons.borrow_mut().clear();
        self.pictures.borrow_mut().clear();
        let mut order = Vec::with_capacity(candidates.len());
        if candidates.is_empty() {
            self.cells.append(&self.placeholder);
        }
        for candidate in candidates {
            let picture = Rc::new(Picture::rgb8(
                candidate.image.width,
                candidate.image.height,
                candidate.image.data,
            ));
            let button = candidate_cell(window, &candidate.template, &picture, &self.syncing);
            self.cells.append(&button);
            order.push(candidate.template.clone());
            self.buttons
                .borrow_mut()
                .insert(candidate.template.clone(), button);
            self.pictures
                .borrow_mut()
                .insert(candidate.template, picture);
        }
        self.order.replace(order);
        self.syncing.set(false);
        let current = window.current_template();
        self.highlight(Some(&current));
    }

    /// Marks the candidate the document is on, and only it.
    ///
    /// The document is the authority rather than the click: a command can change
    /// the template without the strip being touched (an undo, a project opened, the
    /// CLI's `edit`), and a highlight that followed the click would then be lying.
    pub fn highlight(&self, template: Option<&str>) {
        self.syncing.set(true);
        for (name, button) in self.buttons.borrow().iter() {
            let current = template == Some(name.as_str());
            button.set_active(current);
            // The platform's own checked look *and* the app's accent border: the
            // state is the button's, the reading is the theme's accent
            // (`style.css`, `.layout-cell.picked`).
            if current {
                button.add_css_class("picked");
            } else {
                button.remove_css_class("picked");
            }
        }
        self.syncing.set(false);
    }

    /// Writes the count and the two controls: the floor and the ceiling are the
    /// picker's own (`MIN_PHOTOS` / `MAX_PHOTOS`), so the control is insensitive
    /// exactly where the refusal would be.
    pub fn update_control(&self, photos: usize) {
        self.count.set_label(&fill(
            ngettext("{} photo", "{} photos", photos as u32),
            &[photos],
        ));
        self.minus.set_sensitive(photos > MIN_PHOTOS);
        self.plus.set_sensitive(photos < MAX_PHOTOS);
        self.minus.set_tooltip_text(Some(&if photos > MIN_PHOTOS {
            gettext("Remove the last photo")
        } else {
            fill(gettext("A collage needs at least {} photos"), &[MIN_PHOTOS])
        }));
        self.plus.set_tooltip_text(Some(&if photos < MAX_PHOTOS {
            gettext("Add a photo")
        } else {
            fill(gettext("A collage takes at most {} photos"), &[MAX_PHOTOS])
        }));
    }
}

/// The strip's empty state: a candidate cell with nothing in it.
///
/// **The same widgets as a candidate** — a `GtkToggleButton` with the same class and
/// the same content shape, only insensitive and with an empty thumbnail — because
/// the band's height decides how much the canvas above it gets, and the candidates
/// arrive from a **background build**: a placeholder of a different size means the
/// canvas is laid out twice on every open, which is one more grid and one more
/// decode of every photo (measured 2026-09-23: the canvas lost 105 px when the band
/// filled, and the eight-photo verification document was decoded 21 times instead of
/// 14). Same widgets, same height, whatever the theme and whatever the font size.
///
/// The caption is short because a cell is [`THUMB_BOX`] wide; the whole sentence is
/// the tooltip and the accessible name.
fn placeholder_cell() -> gtk::ToggleButton {
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_width_request(THUMB_BOX.0);
    spacer.set_height_request(THUMB_BOX.1);
    let caption = gtk::Label::builder()
        .label(gettext("No layout"))
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.append(&spacer);
    content.append(&caption);

    let cell = gtk::ToggleButton::builder().child(&content).build();
    cell.add_css_class("layout-cell");
    cell.set_sensitive(false);
    cell.set_tooltip_text(Some(&gettext("No layout has this many photos")));
    a11y::label(&cell, &gettext("No layout has this many photos"));
    cell
}

/// One candidate: its thumbnail, its name, and the toggle that chooses it.
///
/// A `GtkToggleButton`, not a custom-drawn cell: HIG `guidelines/accessibility`
/// and `guidelines/pointer-touch` then cover it for free (it is focusable, it is
/// named, and `Space` activates it), which is the same reasoning ruling 9 uses for
/// S15's floating buttons.
fn candidate_cell(
    window: &EditorWindow,
    template: &str,
    picture: &Rc<Picture>,
    syncing: &Rc<Cell<bool>>,
) -> gtk::ToggleButton {
    let image = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .can_shrink(true)
        .width_request(THUMB_BOX.0)
        .height_request(THUMB_BOX.1)
        .paintable(picture.texture())
        .build();
    let caption = gtk::Label::builder()
        .label(template)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.append(&image);
    content.append(&caption);

    let button = gtk::ToggleButton::builder().child(&content).build();
    button.set_widget_name(template);
    button.add_css_class("layout-cell");
    a11y::label(&button, &fill(gettext("Layout {}"), &[template]));
    // Owned, because the handler outlives this call.
    let template = template.to_string();
    button.connect_toggled(glib::clone!(
        #[weak]
        window,
        #[strong]
        syncing,
        #[strong]
        template,
        move |button| {
            if syncing.get() {
                // The highlight writing the buttons is not a user choosing one.
                return;
            }
            if button.is_active() {
                window.select_layout(&template);
            } else {
                // A toggle can be turned off by a click on the cell that is already
                // current. There is no "no layout" state to be in, so the document's
                // own answer is written back.
                if let Some(gallery) = window.gallery() {
                    let current = window.current_template();
                    gallery.highlight(Some(&current));
                }
            }
        }
    ));
    button
}

/// An icon button with a tooltip and an accessible name — the shell's own idiom
/// (`window.rs`), repeated here because the two controls are the band's.
fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(label)
        .valign(gtk::Align::Center)
        .build();
    a11y::label(&button, label);
    button
}
