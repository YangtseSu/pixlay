// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layout band: the sheet shapes the current count offers, every candidate
//! layout **of the chosen shape**, drawn as a sketch of its geometry, and the count
//! control that decides what that count is.
//!
//! The layout step of the main path (`AGENTS.md`: `open → add photos → pick a
//! layout → adjust → export`), and **a band on the document's page rather than a
//! page of its own** (S14): a separate page would have to own a second canvas, and
//! S15's compose controls attach to the canvas this band sits under. (The
//! `AdwNavigationView` that once held those pages left with the picker in S22; the
//! band is a box at the bottom of the one page that remains.)
//!
//! # The candidates are sketches
//!
//! The list is [`crate::window::EditorWindow::candidate_templates`] — every template
//! with exactly the document's cell count **and its sheet shape**, in library order
//! (the shape is the ratio row's choice, so the strip is the *arrangements* of the
//! sheet the user is looking at) — and each candidate is
//! drawn by `pixlay_render::sketch_rgb8`: its cells in paper, every cell's outline
//! and the sheet's ground between the cells in ink (ruling 32 of the plan of
//! 2026-09-25; S21, S29). A template carries geometry and nothing else, so a sketch
//! is a complete account of it — and the band therefore **decodes nothing**: no
//! photo enters the strip, so the strip's cost is a few hundred microseconds however
//! many candidates it lists (measured 2026-09-26:
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
//! # The ratio row
//!
//! Ahead of the strip sits the **ratio row** (S36): the sheet shapes the current
//! cell count offers, as a radio set — the document's own shape is the one marked,
//! exactly as the strip marks the document's own template, and a count that offers
//! a single shape shows no row because there is nothing to choose. Activating one
//! switches the document to that shape's layout ([`EditorWindow::select_ratio`],
//! `Command::SetTemplate`, one undo step) and the strip then lists only that
//! shape's candidates: the reference's own model (Xiaomi's 布局 band), where a
//! person says "square" or "portrait" and then picks among that shape's
//! arrangements.
//!
//! A ratio button is a ratio — `4:3`, `16:9` — and there is no other text on it:
//! a ratio is not copy and goes through no translation
//! (`pixlay_core::templates::ratio_label`).
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

use pixlay_core::templates::{CANDIDATE_BOX, ratio_label};
use pixlay_core::{ASPECT_TOLERANCE, MAX_PHOTOS, MIN_PHOTOS, Rgba8};
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

/// The band: the count control, the ratio row, the strip, and the state the strip
/// is showing.
pub struct Gallery {
    /// The band's own root, which the editor page appends.
    root: gtk::Box,
    /// `− / N photos / +`: the count and the layout move together, so they are one
    /// control at the start of the band.
    minus: gtk::Button,
    plus: gtk::Button,
    count: gtk::Label,
    /// `− / N photos / +` as one widget, so the whole control can be hidden (S40):
    /// an ID photo sheet holds one photo, so its cell count is not a number to
    /// edit, and there is no third meaning for `+` there.
    control: gtk::Box,
    /// The ratio row: the sheet shapes the current count offers, as a radio set
    /// (S36). Hidden while the count offers one shape, because there is nothing to
    /// choose.
    ratios: gtk::Box,
    /// The row's buttons with the ratio each stands for, in the row's own order.
    ratio_buttons: RefCell<Vec<(f64, gtk::ToggleButton)>>,
    /// The ratios the row is currently built for, so a rebuild happens when the
    /// count changes the set — and not on every refresh.
    ratio_order: RefCell<Vec<f64>>,
    /// The row's ID photo member (S40): built while the document holds one cell,
    /// where the row is where a sheet of one photo is chosen.
    id_button: RefCell<Option<gtk::ToggleButton>>,
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
    /// does not read back as a user choosing a layout.
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

        // The ratio row (S36), above the strip in its own column: the count control
        // is to the left of both, and the row is drawn only while the count offers
        // a choice.
        let ratios = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        // One size for every shape — a radio set reads as a set — and packed at the
        // **start** of its column rather than stretched across it: a homogeneous box
        // fills the width it is given, and a row of five shapes spread over the band
        // is a segmented control, not a radio set (measured 2026-10-16: 974 px of
        // buttons before `halign`, 359 px after — five buttons of 67 px, which is the
        // widest of their labels plus the theme's own padding).
        ratios.set_homogeneous(true);
        ratios.set_halign(gtk::Align::Start);
        ratios.set_margin_top(6);
        ratios.set_margin_start(6);
        ratios.set_margin_end(6);
        ratios.set_visible(false);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // The column takes the band's width, so the **strip** still fills it (S14: a
        // wide library scrolls sideways rather than being clipped) while the row
        // above stays its own natural width (`ratios` packs at the start).
        column.set_hexpand(true);
        column.append(&ratios);
        column.append(&strip);

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.append(&control);
        root.append(&column);

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
            control,
            ratios,
            ratio_buttons: RefCell::new(Vec::new()),
            ratio_order: RefCell::new(Vec::new()),
            id_button: RefCell::new(None),
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

    /// The ratios the row holds, in the row's own order — which is the library's
    /// (`templates::ratios_for`: the two most-used shapes first, then ascending; S36
    /// had the ascending order alone). A count that offers one shape has a
    /// one-button row, and the row is not drawn at all while there is nothing to
    /// choose (the widget's visibility is the caller's question, not this list's).
    pub fn ratios(&self) -> Vec<f64> {
        self.ratio_order.borrow().clone()
    }

    /// Whether the row is drawn: false while the count offers a single shape.
    pub fn ratios_visible(&self) -> bool {
        self.ratios.is_visible()
    }

    /// The row's own widget: the geometry a caller measures it by.
    pub fn ratio_row(&self) -> gtk::Box {
        self.ratios.clone()
    }

    /// The row's button for `aspect`, which is how the tests choose a shape.
    /// The row's ID photo member, when the document holds one cell (S40).
    pub fn id_photo_button(&self) -> Option<gtk::ToggleButton> {
        self.id_button.borrow().clone()
    }

    /// Whether the count control is shown: it is not, on a sheet (S40).
    pub fn count_visible(&self) -> bool {
        self.control.is_visible()
    }

    pub fn ratio_button(&self, aspect: f64) -> Option<gtk::ToggleButton> {
        self.ratio_buttons
            .borrow()
            .iter()
            .find(|(ratio, _)| (ratio - aspect).abs() <= ASPECT_TOLERANCE)
            .map(|(_, button)| button.clone())
    }

    /// The ratio the row has marked, if any: the document's own shape while the row
    /// is showing.
    pub fn selected_ratio(&self) -> Option<f64> {
        self.ratio_buttons
            .borrow()
            .iter()
            .find(|(_, button)| button.is_active())
            .map(|(ratio, _)| *ratio)
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

    /// Writes the ratio row: the shapes `ratios` offers, with `current` marked.
    ///
    /// The buttons are rebuilt only when the **set** changes — a count change, or a
    /// project opened on another count — so the common case (a layout switch, an
    /// undo, a count that keeps its shapes) costs one pass over the buttons and no
    /// widget churn. The document is the authority here too: `current` comes from
    /// the document and not from the click (`highlight_ratios`).
    ///
    /// The row hides itself while there is nothing to choose: a count that offers a
    /// single shape has no ratio decision in it, and a one-button radio set would be
    /// a control that cannot do anything (count 1 is the case, and it is the count
    /// the window opens on).
    pub fn show_ratios(&self, window: &EditorWindow, ratios: &[f64], current: f64) {
        // The row's ID photo member exists while the document holds **one cell**
        // (S40, the human's ruling of 2026-10-17: "在1张照片的时候，在16:9的右边增加
        // 证件照"): at one photo the sheet's kind is a real choice, and at two or more
        // there is no third meaning for it.
        let doc = window.document();
        // One cell **and nothing kept**: the sheet has one cell to hold a photo in, so a
        // document that keeps a cell off the sheet is one the core refuses the switch for
        // (`OnePhotoLayout`) — and a control that would be refused should not be there.
        let with_id = doc.cells.len() == 1 && doc.kept.is_empty();
        let changed = {
            let order = self.ratio_order.borrow();
            order.len() != ratios.len()
                || order
                    .iter()
                    .zip(ratios)
                    .any(|(now, wanted)| (now - wanted).abs() > ASPECT_TOLERANCE)
                || self.id_button.borrow().is_some() != with_id
        };
        if changed {
            self.syncing.set(true);
            while let Some(child) = self.ratios.first_child() {
                self.ratios.remove(&child);
            }
            self.id_button.replace(None);
            let mut buttons = self.ratio_buttons.borrow_mut();
            buttons.clear();
            let mut first: Option<gtk::ToggleButton> = None;
            for &aspect in ratios {
                let button = ratio_cell(window, aspect, first.as_ref(), &self.syncing);
                self.ratios.append(&button);
                if first.is_none() {
                    first = Some(button.clone());
                }
                buttons.push((aspect, button));
            }
            drop(buttons);
            if with_id {
                let button = id_photo_cell(window, first.as_ref(), &self.syncing);
                self.ratios.append(&button);
                self.id_button.replace(Some(button));
            }
            self.ratio_order.replace(ratios.to_vec());
            self.ratios.set_visible(ratios.len() > 1 || with_id);
            self.syncing.set(false);
        }
        self.highlight_ratios(current, doc.template.one_photo);
    }

    /// Marks the ratio the document is on, and only it — the row's half of
    /// [`highlight`](Self::highlight), written from the same document on the same
    /// refreshes.
    pub fn highlight_ratios(&self, current: f64, one_photo: bool) {
        self.syncing.set(true);
        for (ratio, button) in self.ratio_buttons.borrow().iter() {
            // An ID photo sheet's shape is 2:3 like three other members' (S40), so
            // the document's *kind* decides which member is marked: `one_photo` takes
            // the ID member, and the shape buttons stay clear.
            let picked = !one_photo && (ratio - current).abs() <= ASPECT_TOLERANCE;
            button.set_active(picked);
            if picked {
                button.add_css_class("picked");
            } else {
                button.remove_css_class("picked");
            }
        }
        if let Some(button) = self.id_button.borrow().as_ref() {
            button.set_active(one_photo);
            if one_photo {
                button.add_css_class("picked");
            } else {
                button.remove_css_class("picked");
            }
        }
        self.syncing.set(false);
    }

    /// Writes the count and the two controls: the floor and the ceiling are
    /// `MIN_PHOTOS` / `MAX_PHOTOS`, so the control is insensitive exactly where the
    /// refusal would be.
    ///
    /// **The label is the number alone** (ruled 2026-09-23): the control sits beside
    /// a strip of layout thumbnails and under a canvas, so "8" between `−` and `+`
    /// needs no noun, and the word "photos" next to a picture of the collage reads
    /// as a caption rather than as the quantity. What the number *counts* is the
    /// accessible name and the tooltips' business — a screen reader announces
    /// "Photos in the collage: 8" — which is where HIG `guidelines/accessibility`
    /// asks for it and where a two-word caption costs nothing.
    ///
    /// `kept` is the file name of the photo a growth would place back first, when the
    /// document keeps one (S28, ruling 43): `+` is then the way that photo returns,
    /// so its own hint says so instead of promising an empty cell — and the hint is
    /// the accessible name too, because that is what pressing the button does.
    ///
    /// The `+`'s sensitivity is `cells < MAX_PHOTOS`, which is exactly where
    /// `AddCell` itself refuses: a growth with a kept cell waiting places that cell
    /// instead of appending one, so it never takes the document past the ceiling
    /// (and `cells` can only be `MAX_PHOTOS` with nothing kept).
    /// Shows or hides the count control (S40): an ID photo sheet holds one photo, so
    /// its cell count is not a number to edit and the control has nothing to say.
    pub fn show_count(&self, visible: bool) {
        self.control.set_visible(visible);
    }

    pub fn update_control(&self, cells: usize, kept: Option<&str>) {
        self.count.set_label(&cells.to_string());
        self.minus.set_sensitive(cells > MIN_PHOTOS);
        self.plus.set_sensitive(cells < MAX_PHOTOS);
        self.minus.set_tooltip_text(Some(&if cells > MIN_PHOTOS {
            gettext("Remove the last cell")
        } else {
            gettext("A collage needs at least one photo")
        }));
        let add = match kept {
            Some(name) => fill(gettext("Add a cell and bring back “{}”"), &[name]),
            None if cells < MAX_PHOTOS => gettext("Add a cell"),
            None => fill(gettext("A collage takes at most {} photos"), &[MAX_PHOTOS]),
        };
        self.plus.set_tooltip_text(Some(&add));
        a11y::label(&self.plus, &add);
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
    // An ID photo sheet's size is **copy** (S40, the human's ruling: "不知道哪个是具体
    // 是多大的尺寸……显示『一寸』"): the sketch of a 25x35 cell and a 35x49 one are
    // pixels apart at this box, so the size name is the tooltip and the front of the
    // accessible name. A collage candidate keeps the positional name ruling 40 gave
    // it, because there the name is machine identity rather than something a person
    // reads.
    match photo_size_name(template) {
        Some(size) => {
            button.set_tooltip_text(Some(&size));
            a11y::label(
                &button,
                &fill(
                    gettext("{}: layout {} of {}"),
                    &[size, position.0.to_string(), position.1.to_string()],
                ),
            );
        }
        None => a11y::label(
            &button,
            &fill(gettext("Layout {} of {}"), &[position.0, position.1]),
        ),
    }
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

/// One ratio: the toggle that chooses a sheet shape, as one member of a radio set.
///
/// A `GtkToggleButton` carrying its own label, with `group` set for every member
/// after the first: GTK's own radio behaviour is what makes the row a
/// single-choice set (HIG `patterns/controls/radio-buttons`), so a click on one
/// shape unchecks the shape that was marked and the platform's checked look does
/// the reading. The label is a ratio — `4:3` — and a ratio is not copy: it is the
/// same text in every language, and GTK announces a text button by its text
/// (`crate::a11y`'s own doctrine), so this control needs no accessible name set by
/// hand.
fn ratio_cell(
    window: &EditorWindow,
    aspect: f64,
    group: Option<&gtk::ToggleButton>,
    syncing: &Rc<Cell<bool>>,
) -> gtk::ToggleButton {
    let label = ratio_label(aspect);
    let button = gtk::ToggleButton::with_label(&label);
    button.add_css_class("ratio-cell");
    button.set_widget_name(&label);
    if let Some(group) = group {
        button.set_group(Some(group));
    }
    button.connect_toggled(glib::clone!(
        #[weak]
        window,
        #[strong]
        syncing,
        #[strong]
        aspect,
        move |button| {
            if syncing.get() {
                // The row writing its own mark is not a user choosing a shape.
                return;
            }
            if button.is_active() {
                window.select_ratio(aspect);
            } else if let Some(gallery) = window.gallery() {
                // There is no "no shape" state to be in, so the mark is restored
                // from the document — but **only when the row is unmarked**.
                // GTK's group bookkeeping deactivates the member that was marked
                // *before* it emits the new one's own toggled, so a re-mark here
                // would switch the row straight back to the document's old shape
                // and the click would do nothing; a plain click on the marked
                // member takes the row to no active member at all, which is the
                // case this answers.
                if gallery.selected_ratio().is_none() {
                    let doc = window.document();
                    gallery.highlight_ratios(doc.template.aspect, doc.template.one_photo);
                }
            }
        }
    ));
    button
}

/// The row's ID photo member (S40): the last choice, after the six shapes.
///
/// A `GtkToggleButton` carrying its own label, grouped with the shape buttons so the
/// row stays one radio set. Its label is copy rather than a ratio, so it is the one
/// member of the row that goes through `gettext`; GTK announces a text button by its
/// text (`crate::a11y`'s own doctrine), so no accessible name is set by hand.
fn id_photo_cell(
    window: &EditorWindow,
    group: Option<&gtk::ToggleButton>,
    syncing: &Rc<Cell<bool>>,
) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::with_label(&gettext("ID photo"));
    button.add_css_class("ratio-cell");
    button.set_widget_name("id-photo");
    if let Some(group) = group {
        button.set_group(Some(group));
    }
    button.connect_toggled(glib::clone!(
        #[weak]
        window,
        #[strong]
        syncing,
        move |button| {
            if syncing.get() {
                // The row writing its own mark is not a user choosing the sheet.
                return;
            }
            if button.is_active() {
                window.select_id_photo();
            } else if let Some(gallery) = window.gallery() {
                // The same rule as a shape's own cell: a row that went blank is
                // written back from the document, and GTK's group bookkeeping (which
                // deactivates the old member before emitting the new one's) is why the
                // mark is only restored when nothing at all is active.
                if gallery.selected_ratio().is_none() {
                    let doc = window.document();
                    gallery.highlight_ratios(doc.template.aspect, doc.template.one_photo);
                }
            }
        }
    ));
    button
}

/// The size name an ID photo sheet's member is read by, or `None` for a collage
/// layout (S40).
///
/// A `match` on the member's name, like `app.rs::shortcut_title`: the strings are
/// user-visible, and keeping the `gettext` call beside the name is what lets
/// `xgettext` see it (`po/`). The names are the machine identity; the words are copy
/// — a Chinese studio reads 一寸 / 小一寸 / 大一寸 / 二寸 / 大二寸 / 小二寸, and the
/// source language is English (`AGENTS.md`, "Language conventions").
pub fn photo_size_name(template: &str) -> Option<String> {
    match template {
        "id-1cun-12-2x3" => Some(gettext("1-inch photo")),
        "id-1cun-small-16-2x3" => Some(gettext("Small 1-inch photo")),
        "id-1cun-large-6-2x3" => Some(gettext("Large 1-inch photo")),
        "id-2cun-6-2x3" => Some(gettext("2-inch photo")),
        "id-2cun-large-4-2x3" => Some(gettext("Large 2-inch photo")),
        "id-2cun-small-6-2x3" => Some(gettext("Small 2-inch photo")),
        _ => None,
    }
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
