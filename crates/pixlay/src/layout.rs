//! The layout stage: every candidate layout of the current photo count, drawn as a
//! sketch of its geometry, plus the count control that decides what that count is.
//!
//! Stage 3 of the main path (`AGENTS.md`: `open → add photos → pick a layout →
//! adjust → export`), and **a band on the document's page rather than a third
//! page** (S14): a second `AdwNavigationPage` would have to own a second canvas, and
//! S15's compose controls attach to the canvas this band sits under.
//!
//! # The candidates are sketches
//!
//! The list is [`crate::window::EditorWindow::candidate_templates`] — every template
//! with exactly the document's cell count, in library order — and each candidate is
//! drawn by `pixlay_render::sketch_rgb8`: its cells' outlines stroked over the
//! sheet's ground (ruling 32 of the plan of 2026-09-25; S21). A template carries
//! geometry and nothing else, so a sketch is a complete account of it — and the band
//! therefore **decodes nothing**: no photo enters the strip, so the strip's cost is
//! a few hundred microseconds however many candidates it lists (measured 2026-09-26:
//! 0.12–0.14 ms per candidate at the band's grid, `--release`).
//!
//! # The two colours come from the theme
//!
//! A candidate is interface, not content, so its paper and its ink are the theme's
//! own: the two probe widgets below carry the classes `style.css` gives the colours
//! to, and [`Gallery::sketch_style`] reads them back through
//! `GtkWidget::color()` — public, non-deprecated API, and the colours themselves
//! stay in CSS where the rest of the app's colours live.
//!
//! # The strip
//!
//! Candidates never wrap: the strip is a horizontal `GtkBox` inside a scroller
//! whose vertical policy is `Never`, so a candidate is one cell tall whatever the
//! window does, and a wide library scrolls sideways. A cell is
//! [`CANDIDATE_BOX`] whatever the layout's aspect — a 2:3 layout and a 16:9 one are the
//! same cell across and the picture is fitted inside it — because the band's height
//! is taken from the canvas, and a cell that followed its layout's own shape would
//! make the band taller than the tallest layout in the library.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;

use pixlay_core::templates::CANDIDATE_BOX;
use pixlay_core::{MAX_PHOTOS, MIN_PHOTOS, Rgba8};
use pixlay_render::{Rgb8Image, Sketch};

use crate::a11y;
use crate::i18n::{fill, gettext};
use crate::picture::Picture;
use crate::window::EditorWindow;

/// The sketch's stroke width, in the thumbnail's own pixels.
///
/// One pixel: the band's cells are [`CANDIDATE_BOX`] (128x96), and a hairline is
/// what makes a cell's edges read as *lines* rather than as filled bars at that
/// size (S21; the reference's own layout strip draws them the same way). The
/// human's legibility gate is where this is judged; the CLI takes the width as a
/// parameter and defaults to the same one.
const SKETCH_STROKE_PX: f64 = 1.0;

/// One drawn candidate, as the worker hands it back.
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
    /// What the strip shows when the count has no layout: the count is in
    /// `1..=MAX_PHOTOS` whatever the document does, so this stands for the empty
    /// case a per-cell clear can leave behind rather than for a reachable count.
    placeholder: gtk::ToggleButton,
    /// The candidate buttons by template name, and the pictures behind them.
    buttons: RefCell<HashMap<String, gtk::ToggleButton>>,
    pictures: RefCell<HashMap<String, Rc<Picture>>>,
    /// The strip's order, which is the library's.
    order: RefCell<Vec<String>>,
    /// Set while this module writes the buttons' own state, so that a highlight
    /// does not read back as a user choosing a layout (the picker's own idiom).
    syncing: Rc<Cell<bool>>,
    /// The two widgets that carry the sketch's colours, which `style.css` gives
    /// them: `GtkWidget::color()` is the one public way to read a theme colour
    /// back, so the band probes its own stylesheet instead of naming a value.
    paper_probe: gtk::Label,
    ink_probe: gtk::Label,
}

impl Gallery {
    /// Builds the band.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let minus = icon_button("list-remove-symbolic", &gettext("Remove the last cell"));
        let plus = icon_button("list-add-symbolic", &gettext("Add a cell"));
        let count = gtk::Label::new(None);
        count.add_css_class("dim-label");
        // The number is all that is drawn, so what it *counts* is what a screen
        // reader has to hear: without this the control announces "8" between two
        // unlabelled buttons.
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

        // The two colour probes: invisible labels whose only job is to carry the
        // classes `style.css` defines the sketch's colours with. They are never
        // allocated (an invisible child takes no room) and they are the band's own,
        // so the colours resolve through the same stylesheet the rest of the
        // window is drawn from.
        let paper_probe = colour_probe("sketch-paper");
        let ink_probe = colour_probe("sketch-ink");
        root.append(&paper_probe);
        root.append(&ink_probe);

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
            paper_probe,
            ink_probe,
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

    /// One candidate's pixels — its sketch, which is what the pixel criterion
    /// compares.
    pub fn sketch(&self, template: &str) -> Option<(i32, i32, Vec<u8>)> {
        let pictures = self.pictures.borrow();
        let picture = pictures.get(template)?;
        Some((picture.width(), picture.height(), picture.bytes().to_vec()))
    }

    /// The cell bound to a candidate, for the tests and the HIG checks.
    pub fn cell(&self, template: &str) -> Option<gtk::ToggleButton> {
        self.buttons.borrow().get(template).cloned()
    }

    /// The sketch's three parameters, with the two colours read from the theme
    /// (see the module docs).
    ///
    /// Read on the main thread, at the moment a build is asked for, and handed to
    /// the worker as plain data: a colour resolved here is a colour resolved
    /// against the widgets the band is actually drawn with.
    pub fn sketch_style(&self) -> Sketch {
        Sketch {
            paper: probe_color(&self.paper_probe),
            ink: probe_color(&self.ink_probe),
            stroke_px: SKETCH_STROKE_PX,
        }
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
        let count = candidates.len();
        for (index, candidate) in candidates.into_iter().enumerate() {
            let picture = Rc::new(Picture::rgb8(
                candidate.image.width,
                candidate.image.height,
                candidate.image.data,
            ));
            // The cell's accessible name is **positional** (ruling 40): a
            // candidate is its sketch, and the template's name is machine identity
            // (`edit --template`), never text a user reads.
            let button = candidate_cell(
                window,
                &candidate.template,
                &picture,
                &self.syncing,
                (index + 1, count),
            );
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
    ///
    /// **The label is the number alone** (ruled 2026-09-23): the control sits beside
    /// a strip of layout thumbnails and under a canvas, so "8" between `−` and `+`
    /// needs no noun, and the word "photos" next to a picture of the collage reads
    /// as a caption rather than as the quantity. What the number *counts* is the
    /// accessible name and the tooltips' business — a screen reader announces
    /// "Photos in the collage: 8" — which is where HIG `guidelines/accessibility`
    /// asks for it and where a two-word caption costs nothing.
    pub fn update_control(&self, cells: usize) {
        self.count.set_label(&cells.to_string());
        self.minus.set_sensitive(cells > MIN_PHOTOS);
        self.plus.set_sensitive(cells < MAX_PHOTOS);
        self.minus.set_tooltip_text(Some(&if cells > MIN_PHOTOS {
            gettext("Remove the last cell")
        } else {
            gettext("A collage needs at least one photo")
        }));
        self.plus.set_tooltip_text(Some(&if cells < MAX_PHOTOS {
            gettext("Add a cell")
        } else {
            fill(gettext("A collage takes at most {} photos"), &[MAX_PHOTOS])
        }));
    }
}

/// The strip's empty state: a candidate cell with nothing drawn in it.
///
/// **The same widgets as a candidate** — a `GtkToggleButton` holding a box of
/// [`CANDIDATE_BOX`], only insensitive and with no picture — because the band's
/// height decides how much the canvas above it gets, and the candidates arrive
/// from a **background build**: a placeholder of a different size means the canvas
/// is laid out twice on every open, which is one more grid and one more decode of
/// every photo (measured 2026-09-23: the canvas lost 105 px when the band filled,
/// and the eight-photo verification document was decoded 21 times instead of 14).
/// Same widgets, same height, whatever the theme and whatever the font size.
///
/// Since S21 a candidate has no caption (ruling 40), so neither has this: the
/// sentence is the tooltip and the accessible name, and the box is the height.
fn placeholder_cell() -> gtk::ToggleButton {
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_width_request(CANDIDATE_BOX.0);
    spacer.set_height_request(CANDIDATE_BOX.1);

    let cell = gtk::ToggleButton::builder().child(&spacer).build();
    cell.add_css_class("layout-cell");
    cell.set_sensitive(false);
    cell.set_tooltip_text(Some(&gettext("No layout has this many photos")));
    a11y::label(&cell, &gettext("No layout has this many photos"));
    cell
}

/// One candidate: its sketch, the toggle that chooses it, and where it sits in the
/// strip.
///
/// A `GtkToggleButton`, not a custom-drawn cell: HIG `guidelines/accessibility`
/// and `guidelines/pointer-touch` then cover it for free (it is focusable, it is
/// named, and `Space` activates it), which is the same reasoning ruling 9 uses for
/// S15's floating buttons.
///
/// `position` is `(the candidate's place, how many candidates there are)` and it is
/// the cell's **whole** accessible name: a candidate is its sketch, so "Layout 3 of
/// 5" is what a screen reader has to say about it, and no template name is text a
/// user reads (ruling 40). The widget's own name stays the template's — that is how
/// the window and the tests find the cell (`Gallery::cell`), not user-visible copy.
fn candidate_cell(
    window: &EditorWindow,
    template: &str,
    picture: &Rc<Picture>,
    syncing: &Rc<Cell<bool>>,
    position: (usize, usize),
) -> gtk::ToggleButton {
    let image = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .can_shrink(true)
        .width_request(CANDIDATE_BOX.0)
        .height_request(CANDIDATE_BOX.1)
        .paintable(picture.texture())
        .build();

    let button = gtk::ToggleButton::builder().child(&image).build();
    button.set_widget_name(template);
    button.add_css_class("layout-cell");
    a11y::label(
        &button,
        &fill(gettext("Layout {} of {}"), &[position.0, position.1]),
    );
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

/// An invisible label whose only job is to resolve one CSS colour.
///
/// `style.css` gives the class a `color:` declaration, and [`probe_color`] reads
/// it back. It costs nothing in layout (an invisible child is not allocated) and
/// nothing in the tree (it holds no content), and it is how the band gets a
/// *theme* colour without naming one.
fn colour_probe(class: &str) -> gtk::Label {
    let probe = gtk::Label::new(None);
    probe.add_css_class(class);
    probe.set_visible(false);
    probe
}

/// One probe's colour, as the sketch's own opaque RGB.
///
/// `GtkWidget::color()` is the CSS `color` property as the widget resolved it: the
/// theme's variable, the app's stylesheet and the widget's own state included. The
/// alpha is forced opaque because that is what a sketch's two colours are (see
/// `pixlay_render::Sketch`).
fn probe_color(probe: &gtk::Label) -> Rgba8 {
    let rgba = probe.color();
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgba8::rgb(
        channel(rgba.red()),
        channel(rgba.green()),
        channel(rgba.blue()),
    )
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
