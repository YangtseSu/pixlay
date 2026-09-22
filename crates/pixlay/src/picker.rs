//! The picker stage: browse a folder, look at a photo, pick 2–9 in order, Next.
//!
//! Stages 1–2 of the main path (`AGENTS.md`: `open → pick 2–9 photos → pick a
//! layout → adjust → export`). This is the root page of the window's
//! `AdwNavigationView`; the editor of S7 is pushed on top of it when Next is
//! pressed.
//!
//! # What this module is, and what it deliberately is not
//!
//! * **A collection view in selection mode** (HIG `patterns/containers/selection-mode`,
//!   which the plan's review turned from "not applicable" into a criteria row):
//!   a `GtkGridView` over the folder's photos with a `GtkMultiSelection`, a cell
//!   whose click *toggles* it, the platform's own round check mark
//!   (`.selection-mode` on a `GtkCheckButton`), `Ctrl+A` selecting all, `Esc`
//!   leaving selection mode, and a header bar whose Next button carries the count
//!   and is the batch action. Past the cap the grid *reports* the refusal instead
//!   of truncating silently, which the guidelines do not cover and ruling 3
//!   requires.
//! * **Not a second renderer, and not a second product.** A tile and the preview
//!   are `pixlay_imaging::thumbnail` pixels — the same function the CLI's `thumb`
//!   writes to a file — so "what the window shows" and "what the CLI writes" are
//!   one implementation, and S13's pixel criterion is a comparison of two calls
//!   rather than of two resamplers. There is no zoom in this stage: ruling 2's
//!   preview fills the pane (`ContentFit::Contain`), and magnification is the
//!   editor's business (its own gestures and `Ctrl+0`).
//! * **Not where the document is made.** [`Picker::document`] builds it through
//!   `pixlay_core::Selection`, the same policy type the CLI's `init --photo`
//!   uses, so "the third photo the user picked is the third cell" is one
//!   implementation with two callers.
//!
//! # Order
//!
//! A `GtkMultiSelection` is a *set*: it does not remember the order items were
//! picked in, and the product's order *is* cell order. So the ordered list is
//! this module's own (`pixlay_core::Selection`), the tray is where it is visible
//! and re-orderable, and every change to the grid's selection is reconciled back
//! into it — a photo already in the list keeps its place, a newly picked one is
//! appended. That is also why the cell's click is handled here rather than left
//! to GTK: the platform's own click *replaces* the selection for a
//! multi-selection model (measured in `gtklistfactorywidget.c`: a plain click
//! sends `modify = false`, which is `select_item(pos, unselect_rest = TRUE)`),
//! while picking three photos by clicking three cells has to accumulate.
//!
//! # Threading
//!
//! A decode is 11–110 ms per photo (S4) and a folder is unbounded, so tiles are
//! built on one worker thread ([`crate::thumbs`]) and cross back as plain bytes
//! through `MainContext::invoke`. No GTK object leaves the main thread.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{CollageDoc, MAX_PHOTOS, MIN_PHOTOS, Selection, SelectionError};
use pixlay_imaging::Thumbnail;

use crate::a11y;
use crate::i18n::{fill, gettext, ngettext};
use crate::window::EditorWindow;

/// Long edge a grid tile is built at, in pixels.
///
/// Two device pixels per displayed pixel of a [`TILE_SIZE`] cell, so a tile is
/// sharp on a HiDPI screen, and 256 px is also the size S9 measured the
/// picker's budget at (`docs/CONTRACT.md` §8, "S9": `thumb --px 256` costs
/// 49–50 ms and `peak_rss_mb` 19.7 — this stage's number).
pub const TILE_PX: u32 = 256;

/// Size of one grid cell, in device pixels.
///
/// Square whatever the photo's aspect, so the grid does not re-flow as it fills,
/// and well past the minimum HIG `guidelines/pointer-touch` asks of a click
/// target.
pub const TILE_SIZE: i32 = 128;

/// Long edge the preview pane's photo is built at, in pixels.
///
/// One picture, not a folder's worth: this is what a "look at the photo" click
/// costs, and the pane shows it `Contain`-fitted, so a larger decode would only
/// be discarded by the widget.
pub const PREVIEW_PX: u32 = 1024;

/// The picker's widgets and its state.
///
/// `Rc`-owned rather than a `GObject`: nothing here needs a property or a
/// subclass of its own, the window owns the picker for as long as it lives, and
/// the closures that outlive a frame hold weak references to the widgets and the
/// window they touch.
pub struct Picker {
    /// The order the photos will land in cells. The picker's own list, not the
    /// grid's selection, because a selection has no order.
    selection: RefCell<Selection>,
    /// The folder the grid lists, kept for the session (ruling 8: no config file).
    folder: RefCell<Option<PathBuf>>,
    /// The folder's photos, in listing order.
    files: RefCell<Vec<PathBuf>>,
    /// A decoded tile per file index, as it arrives.
    tiles: RefCell<HashMap<usize, Tile>>,
    /// The picture each position's cell is currently bound to.
    ///
    /// A tile arrives long after its cell was bound, and GTK re-binds a row only
    /// when the *item object* changes — which a texture arriving is not. So the
    /// cell that is showing a position is remembered here and handed its pixels
    /// directly, and a cell that is recycled (or scrolled away) drops out of the
    /// map again. `GtkMultiSelection` is untouched by this: the pick never moves.
    cells: RefCell<HashMap<usize, glib::WeakRef<gtk::Picture>>>,
    /// Files the decoder refused, with its reason: a cell that cannot show a
    /// photo is reported rather than silently blank (`scan`'s S9 rule).
    failures: RefCell<HashMap<usize, String>>,
    /// The preview pane's own decode, kept with its pixels: this is what the
    /// stage's pixel criterion compares against `pixlay-render thumb` at the same
    /// size, and it is the same buffer the texture above was built from.
    preview_photo: RefCell<Option<(PathBuf, Thumbnail)>>,
    /// The file the preview shows, if any.
    focused: Cell<Option<usize>>,
    /// Set while this module writes the grid's selection, so that its own write
    /// is not read back as a user click.
    syncing: Rc<Cell<bool>>,

    // ---- widgets ---------------------------------------------------------
    /// The grid's model: one `gio::File` per photo, in listing order.
    store: gio::ListStore,
    /// The grid's selection. A set: membership only, never order.
    multi: gtk::MultiSelection,
    /// The grid itself, for the widgets and the HIG checks that read it.
    grid: gtk::GridView,
    /// The page's root widget.
    root: adw::ToolbarView,
    /// The tray: one chip per picked photo, in cell order.
    tray: gtk::Box,
    /// The tray's placeholder, revealed while nothing is picked.
    tray_hint: gtk::Label,
    /// The preview pane: the focused photo, or the empty state.
    preview: gtk::Picture,
    preview_stack: gtk::Stack,
    /// Next, which carries the count.
    next: gtk::Button,
    /// The folder and its photo count, under the header.
    status: gtk::Label,
    /// The page's own title widget, so the subtitle can show the count too.
    title: adw::WindowTitle,
}

/// One decoded tile: the texture the cell paints.
struct Tile {
    texture: gdk::Texture,
}

impl Picker {
    /// Builds the picker page.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let store = gio::ListStore::new::<gio::File>();
        let multi = gtk::MultiSelection::new(Some(store.clone()));

        let grid = gtk::GridView::builder()
            .model(&multi)
            .min_columns(2)
            .max_columns(8)
            .single_click_activate(false)
            .enable_rubberband(true)
            .build();
        grid.set_vexpand(true);
        grid.set_hexpand(true);
        a11y::label(&grid, &gettext("Photos"));

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(glib::clone!(
            #[weak]
            window,
            move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                item.set_child(Some(&tile_widget()));
                // The cell owns its click, and claims the sequence so GTK's own
                // row handling does not *replace* the selection underneath it
                // (the picker accumulates: see the module's "Order").
                let click = gtk::GestureClick::new();
                click.set_button(gdk::BUTTON_PRIMARY);
                click.connect_released(glib::clone!(
                    #[weak]
                    window,
                    #[weak]
                    item,
                    move |gesture: &gtk::GestureClick, _, _, _| {
                        let Some(picker) = window.picker() else {
                            return;
                        };
                        gesture.set_state(gtk::EventSequenceState::Claimed);
                        picker.toggle(&window, item.position());
                    }
                ));
                item.child().unwrap().add_controller(click);
            }
        ));
        factory.connect_bind(glib::clone!(
            #[weak]
            window,
            move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                if let Some(picker) = window.picker() {
                    picker.bind_tile(item);
                }
            }
        ));
        factory.connect_unbind(glib::clone!(
            #[weak]
            window,
            move |_, object| {
                let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                if let Some(picker) = window.picker() {
                    picker.unbind_tile(item);
                }
            }
        ));
        grid.set_factory(Some(&factory));

        // The keyboard path, on the grid so it works wherever the grid has focus:
        // Enter (and the platform's own bindings) toggle through the model, and
        // these two are the picker's own keys.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            window,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, state| {
                let Some(picker) = window.picker() else {
                    return glib::Propagation::Proceed;
                };
                match key {
                    gdk::Key::Escape => {
                        picker.clear_selection();
                        glib::Propagation::Stop
                    }
                    gdk::Key::a | gdk::Key::A
                        if state.contains(gdk::ModifierType::CONTROL_MASK) =>
                    {
                        picker.select_all();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            }
        ));
        grid.add_controller(keys);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .child(&grid)
            .build();
        scroller.set_size_request(320, -1);
        a11y::label(&scroller, &gettext("Photos"));

        let status = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.set_visible(false);

        // ---- the preview ----------------------------------------------------
        let preview = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .can_shrink(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        let empty = adw::StatusPage::builder()
            .icon_name("image-x-generic-symbolic")
            .title(gettext("No photo yet"))
            .description(gettext(
                "Pick the photos for the collage. The order they are picked in is the order of the cells.",
            ))
            .build();
        let preview_stack = gtk::Stack::new();
        preview_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        preview_stack.add_named(&empty, Some("empty"));
        preview_stack.add_named(&preview, Some("photo"));
        preview_stack.set_visible_child_name("empty");
        a11y::label(&preview_stack, &gettext("Photo preview"));

        // Grid beside preview, tray along the bottom: the two decisions a picker
        // makes are "which photos" and "in which order", and both have to be on
        // screen at once for the second to be checkable against the first.
        let split = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&scroller)
            .end_child(&preview_stack)
            .position(400)
            .resize_start_child(false)
            .shrink_start_child(false)
            .build();

        // ---- the tray -------------------------------------------------------
        let tray = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let tray_hint = gtk::Label::builder()
            .label(gettext("Nothing picked yet"))
            .xalign(0.0)
            .build();
        tray_hint.add_css_class("dim-label");
        tray_hint.add_css_class("caption");
        let tray_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&tray)
            .build();
        let tray_bar = gtk::Box::new(gtk::Orientation::Vertical, 4);
        tray_bar.set_margin_start(12);
        tray_bar.set_margin_end(12);
        tray_bar.set_margin_top(4);
        tray_bar.set_margin_bottom(8);
        tray_bar.append(&tray_hint);
        tray_bar.append(&tray_scroller);
        a11y::label(&tray_bar, &gettext("Picked photos"));

        // ---- the header -----------------------------------------------------
        let folder = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text(gettext("Choose a folder of photos"))
            .build();
        a11y::label(&folder, &gettext("Choose a folder of photos"));
        folder.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| {
                if let Some(picker) = window.picker() {
                    picker.choose_folder(&window);
                }
            }
        ));

        let next_content = adw::ButtonContent::new();
        next_content.set_icon_name("go-next-symbolic");
        next_content.set_label(&gettext("Next"));
        let next = gtk::Button::builder()
            .child(&next_content)
            .tooltip_text(gettext("Open the picked photos in a collage"))
            .build();
        next.add_css_class("suggested-action");
        a11y::label(&next, &gettext("Open the picked photos in a collage"));
        next.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| {
                if let Some(picker) = window.picker() {
                    picker.next(&window);
                }
            }
        ));

        let title = adw::WindowTitle::new(&gettext("Pick photos"), "");
        let header = adw::HeaderBar::new();
        header.set_show_back_button(false);
        header.set_title_widget(Some(&title));
        header.pack_end(&next);
        header.pack_end(&folder);

        // ---- the page -------------------------------------------------------
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&status);
        body.append(&split);
        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.add_bottom_bar(&tray_bar);
        root.set_content(Some(&body));
        // The page is handed to the struct below, so the map handler keeps its own
        // handle on it.
        let page = root.clone();

        let picker = Rc::new(Self {
            selection: RefCell::new(Selection::default()),
            folder: RefCell::new(None),
            files: RefCell::new(Vec::new()),
            tiles: RefCell::new(HashMap::new()),
            cells: RefCell::new(HashMap::new()),
            failures: RefCell::new(HashMap::new()),
            preview_photo: RefCell::new(None),
            focused: Cell::new(None),
            syncing: Rc::new(Cell::new(false)),
            store,
            multi: multi.clone(),
            grid: grid.clone(),
            root,
            tray,
            tray_hint,
            preview,
            preview_stack,
            next,
            status,
            title,
        });

        // The grid's selection is the model's business, but the *ordered list*
        // is this module's, and every change has to be folded into it.
        multi.connect_selection_changed(glib::clone!(
            #[weak]
            window,
            move |_, _, _| {
                let Some(picker) = window.picker() else {
                    return;
                };
                if !picker.syncing.get() {
                    picker.reconcile(&window);
                }
            }
        ));

        picker.update_next();

        // The default folder is listed when the page is first shown, not when it
        // is built: the window is usable only once it is on screen, and listing a
        // folder that nothing displays would be work for nothing.
        page.connect_map(glib::clone!(
            #[weak]
            window,
            #[strong]
            picker,
            move |_| {
                if picker.folder.borrow().is_none() {
                    picker.open_default_folder(&window);
                }
            }
        ));

        picker
    }

    /// The page's root widget, for the window's navigation page.
    pub fn root(&self) -> adw::ToolbarView {
        self.root.clone()
    }

    /// The photo grid, whose model the HIG checks read.
    pub fn grid(&self) -> gtk::GridView {
        self.grid.clone()
    }

    /// The preview pane as a widget: the picture when a photo is focused, the
    /// empty state when none is.
    ///
    /// The pane rather than the picture, because with nothing focused the picture
    /// has no allocation at all — it is the stage's *pane* the adaptive check is
    /// about.
    pub fn preview_widget(&self) -> gtk::Stack {
        self.preview_stack.clone()
    }

    /// The picture the preview pane shows.
    pub fn preview_picture(&self) -> gtk::Picture {
        self.preview.clone()
    }

    /// The tray that shows the pick's order.
    pub fn tray_widget(&self) -> gtk::Box {
        self.tray.clone()
    }

    /// The header's Next button, which carries the count.
    pub fn next_button(&self) -> gtk::Button {
        self.next.clone()
    }

    // ---- the folder --------------------------------------------------------

    /// Asks for a folder and lists it, starting from the session's last one.
    pub fn choose_folder(&self, window: &EditorWindow) {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Choose a folder of photos"))
            .modal(true)
            .build();
        if let Some(folder) = self.folder.borrow().clone() {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }
        let weak = window.downgrade();
        dialog.select_folder(
            Some(window),
            gio::Cancellable::NONE,
            move |result: Result<gio::File, glib::Error>| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let Some(picker) = window.picker() else {
                    return;
                };
                match result {
                    Ok(file) => {
                        if let Some(path) = file.path() {
                            picker.open_folder(&window, &path);
                        }
                    }
                    // A dismissed dialog is not a failure: the user changed their
                    // mind, which is a normal thing to do.
                    Err(error) if !error.matches(gtk::DialogError::Dismissed) => {
                        window.toast(&error.to_string());
                    }
                    Err(_) => {}
                }
            },
        );
    }

    /// Lists `folder`, clearing the selection and asking for its tiles.
    pub fn open_folder(&self, window: &EditorWindow, folder: &Path) {
        let files = match pixlay_imaging::list_folder(folder, false) {
            Ok(files) => files,
            Err(error) => {
                window.toast(&error.to_string());
                return;
            }
        };
        *self.folder.borrow_mut() = Some(folder.to_path_buf());
        // The picks belong to the photos that were on screen: a folder change
        // clears them rather than leaving paths in the document's order that the
        // grid no longer shows.
        self.clear_selection();
        self.files.replace(files.clone());
        self.tiles.borrow_mut().clear();
        self.cells.borrow_mut().clear();
        self.focused.set(None);

        self.syncing.set(true);
        self.store.remove_all();
        for path in &files {
            self.store.append(&gio::File::for_path(path));
        }
        self.syncing.set(false);

        self.update_status(folder, files.len());
        self.show_focused();
        self.ask_for_tiles(window);
    }

    /// Opens the session's default folder: `XDG_PICTURES_DIR`, or `~/Pictures`.
    ///
    /// Ruling 8: "`XDG_PICTURES_DIR` by default, plus a folder chooser … no
    /// configuration file". An account with neither gets the empty grid and the
    /// folder button, which is a state to act on rather than a startup failure.
    pub fn open_default_folder(&self, window: &EditorWindow) {
        match default_folder() {
            Some(folder) => self.open_folder(window, &folder),
            None => self.update_status(Path::new(""), 0),
        }
    }

    /// The folder the grid lists, if one is listed.
    pub fn folder(&self) -> Option<PathBuf> {
        self.folder.borrow().clone()
    }

    /// The folder's photos, in listing order.
    pub fn files(&self) -> Vec<PathBuf> {
        self.files.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.files.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The file at `index`, for the tests and for the window's reporting.
    pub fn file(&self, index: usize) -> Option<PathBuf> {
        self.files.borrow().get(index).cloned()
    }

    /// The files whose tile could not be built, with the decoder's reason.
    pub fn failures(&self) -> Vec<(usize, String)> {
        let mut failures: Vec<(usize, String)> = self
            .failures
            .borrow()
            .iter()
            .map(|(index, reason)| (*index, reason.clone()))
            .collect();
        failures.sort();
        failures
    }

    /// How many tiles are in hand, which is what "the grid fills" is measured in.
    pub fn tiles_built(&self) -> usize {
        self.tiles.borrow().len()
    }

    /// The photo the preview pane decoded, with the pixels it shows.
    ///
    /// The stage's pixel criterion — "the preview's pixels equal
    /// `pixlay-render thumb` of the same photo at the same size" — is a comparison
    /// of these bytes against the file that command writes, so the picture on
    /// screen is checkable without a pointer and without a second renderer.
    pub fn preview_pixels(&self) -> Option<(PathBuf, i32, i32, Vec<u8>)> {
        self.preview_photo
            .borrow()
            .as_ref()
            .map(|(path, thumbnail)| {
                (
                    path.clone(),
                    thumbnail.width,
                    thumbnail.height,
                    thumbnail.pixels.clone(),
                )
            })
    }

    // ---- tiles -------------------------------------------------------------

    /// Asks the worker for the tiles that are not in hand yet.
    ///
    /// One request per file: the worker serves them in order on its own thread,
    /// so the grid fills progressively and the main loop never waits for a decode
    /// (`AGENTS.md`'s "Background work, one thread each").
    fn ask_for_tiles(&self, window: &EditorWindow) {
        let Some(worker) = window.thumbs() else {
            return;
        };
        let files = self.files.borrow().clone();
        for (index, path) in files.iter().enumerate() {
            worker.request_tile(index, path, TILE_PX);
        }
    }

    /// One tile arrived.
    pub fn on_tile(&self, index: usize, thumbnail: Result<Thumbnail, String>) {
        match thumbnail {
            Ok(thumbnail) => {
                self.tiles.borrow_mut().insert(
                    index,
                    Tile {
                        texture: texture_from(&thumbnail),
                    },
                );
                // The cell that is showing this position — if there is one — gets
                // its pixels now: GTK does not re-bind a row because a texture
                // arrived, and the item object is untouched on purpose (the pick
                // is keyed on it).
                let texture = self
                    .tiles
                    .borrow()
                    .get(&index)
                    .map(|tile| tile.texture.clone());
                if let (Some(texture), Some(weak)) = (texture, self.cells.borrow().get(&index))
                    && let Some(picture) = weak.upgrade()
                {
                    picture.set_paintable(Some(&texture));
                }
                if self.focused.get() == Some(index) {
                    self.show_focused();
                }
            }
            Err(reason) => {
                // A file the decoder refuses is a cell that cannot show a photo,
                // not a failed folder (`scan`'s S9 rule, applied to the grid). The
                // cell keeps its name and says why in its tooltip.
                self.show_failure(index, &reason);
            }
        }
    }

    /// One preview arrived (a decode at [`PREVIEW_PX`], not a resampled tile).
    pub fn on_preview(&self, index: usize, thumbnail: Result<Thumbnail, String>) {
        if self.focused.get() != Some(index) {
            // The user moved on while this was decoding.
            return;
        }
        match thumbnail {
            Ok(thumbnail) => {
                self.preview.set_paintable(Some(&texture_from(&thumbnail)));
                self.preview_stack.set_visible_child_name("photo");
                let path = self.file(index).unwrap_or_default();
                *self.preview_photo.borrow_mut() = Some((path, thumbnail));
            }
            Err(reason) => self.preview.set_tooltip_text(Some(&reason)),
        }
    }

    fn show_failure(&self, index: usize, reason: &str) {
        self.failures.borrow_mut().insert(index, reason.to_string());
        if let Some(weak) = self.cells.borrow().get(&index)
            && let Some(picture) = weak.upgrade()
        {
            picture.set_paintable(None::<&gdk::Texture>);
            picture.set_tooltip_text(Some(reason));
        }
        if self.focused.get() == Some(index) {
            self.preview.set_tooltip_text(Some(reason));
        }
    }

    /// Fills one cell: the tile if it has arrived, its file name always, and the
    /// platform's own check mark for its selection state.
    fn bind_tile(&self, item: &gtk::ListItem) {
        let position = item.position() as usize;
        let Some(cell) = item.child().and_downcast::<gtk::Overlay>() else {
            return;
        };
        let name = self
            .files
            .borrow()
            .get(position)
            .map(|path| file_name(path))
            .unwrap_or_default();

        if let Some(picture) = cell.child().and_downcast::<gtk::Picture>() {
            match self.tiles.borrow().get(&position) {
                Some(tile) => {
                    picture.set_paintable(Some(&tile.texture));
                    picture.set_tooltip_text(Some(&name));
                }
                None => {
                    // A tile that has not arrived yet: an empty cell, which reads
                    // as "coming" rather than as a broken photo.
                    picture.set_paintable(None::<&gdk::Texture>);
                    picture.set_tooltip_text(Some(
                        &self
                            .failures
                            .borrow()
                            .get(&position)
                            .cloned()
                            .unwrap_or_else(|| name.clone()),
                    ));
                }
            }
            let weak = glib::WeakRef::new();
            weak.set(Some(&picture));
            self.cells.borrow_mut().insert(position, weak);
        }
        // The check mark is the platform's own (`.selection-mode`), and it is a
        // *state*, not a control: the cell's click owns the toggle (see "Order").
        if let Some(check) = overlay_check(&cell) {
            check.set_active(item.is_selected());
        }
        // HIG `guidelines/accessibility`: the tile is what a screen reader
        // announces, and it has to say which photo it is.
        a11y::label(&cell, &name);
    }

    /// A cell is being recycled: drop the check mark's state so a later bind does
    /// not inherit it.
    fn unbind_tile(&self, item: &gtk::ListItem) {
        self.cells.borrow_mut().remove(&(item.position() as usize));
        if let Some(cell) = item.child().and_downcast::<gtk::Overlay>()
            && let Some(check) = overlay_check(&cell)
        {
            check.set_active(false);
        }
    }

    // ---- picking -----------------------------------------------------------

    /// The ordered pick, in cell order.
    pub fn selection(&self) -> Selection {
        self.selection.borrow().clone()
    }

    pub fn selected_count(&self) -> usize {
        self.selection.borrow().len()
    }

    /// Whether Next can be pressed: the product's floor of two photos.
    pub fn can_continue(&self) -> bool {
        self.selected_count() >= MIN_PHOTOS
    }

    /// Toggles the photo at `position`, which is what a cell's click does.
    ///
    /// Selection is driven through the model, so the grid's own check marks, the
    /// model's `selected` state and this module's ordered list cannot disagree.
    pub fn toggle(&self, window: &EditorWindow, position: u32) {
        if position as usize >= self.len() {
            return;
        }
        if self.multi.is_selected(position) {
            self.multi.unselect_item(position);
        } else {
            // `unselect_rest = false`: picking a second photo must not drop the
            // first (that is the whole difference from GTK's own row click).
            self.multi.select_item(position, false);
        }
        self.focus(window, position as usize);
    }

    /// Picks every photo in the folder, reporting the overflow past the cap.
    ///
    /// HIG `patterns/containers/selection-mode`: `Ctrl+A` selects all of a
    /// collection view; the cap on top of it is the product's own rule, and it is
    /// announced rather than applied silently.
    pub fn select_all(&self) {
        if self.is_empty() {
            return;
        }
        self.multi.select_all();
    }

    /// Clears the pick (`Esc` leaves selection mode).
    pub fn clear_selection(&self) {
        self.syncing.set(true);
        self.multi.unselect_all();
        self.syncing.set(false);
        *self.selection.borrow_mut() = Selection::default();
        self.update_next();
    }

    /// Drops one photo from the pick (a tray chip's remove button).
    pub fn remove_at(&self, window: &EditorWindow, index: usize) {
        let removed = self.selection.borrow_mut().remove(index);
        let Some(photo) = removed else {
            return;
        };
        self.syncing.set(true);
        if let Some(position) = self.files.borrow().iter().position(|file| file == &photo) {
            self.multi.unselect_item(position as u32);
        }
        self.syncing.set(false);
        self.update_tray(window);
        self.update_next();
    }

    /// Moves the pick one place earlier (`delta = -1`) or later (`+1`).
    ///
    /// The tray is where order is re-arranged, because order *is* cell order and
    /// the grid has no order to drag.
    pub fn move_photo(&self, window: &EditorWindow, index: usize, delta: isize) {
        let photos = self.selection.borrow().photos().to_vec();
        let Some(target) = index.checked_add_signed(delta) else {
            return;
        };
        if target >= photos.len() || index >= photos.len() {
            return;
        }
        let mut reordered = photos.clone();
        reordered.swap(index, target);
        let mut selection = Selection::default();
        for photo in reordered {
            // A permutation of a legal selection: pushing cannot pass the cap, and
            // anything unexpected leaves the pick exactly as it was.
            if selection.push(photo).is_err() {
                return;
            }
        }
        *self.selection.borrow_mut() = selection;
        self.update_tray(window);
    }

    /// Rebuilds the ordered list from the grid's selection, enforcing the cap.
    ///
    /// The rule that keeps the list *ordered*: a photo already in it keeps its
    /// place — so a re-order in the tray survives a later click — and a newly
    /// selected one is appended in grid order.
    fn reconcile(&self, window: &EditorWindow) {
        let files = self.files.borrow().clone();
        let selected = selection_positions(&self.multi);

        let mut ordered = Selection::default();
        // First the photos that were already picked, in their established order.
        for photo in self.selection.borrow().photos() {
            if files.iter().any(|file| file == photo)
                && files
                    .iter()
                    .position(|file| file == photo)
                    .is_some_and(|position| selected.contains(&position))
            {
                let _ = ordered.push(photo.clone());
            }
        }
        // Then the ones this change picked, in grid order.
        let mut overflow = 0usize;
        for position in &selected {
            let Some(path) = files.get(*position) else {
                continue;
            };
            if ordered.photos().contains(path) {
                continue;
            }
            if ordered.push(path.clone()).is_err() {
                overflow += 1;
            }
        }

        *self.selection.borrow_mut() = ordered;
        if overflow > 0 {
            // The refused cells are unchecked again, visibly: the model and the
            // list must agree, and the user has to be told why.
            self.syncing.set(true);
            for position in &selected {
                if let Some(path) = files.get(*position)
                    && !self.selection.borrow().photos().contains(path)
                {
                    self.multi.unselect_item(*position as u32);
                }
            }
            self.syncing.set(false);
            window.toast(&fill(
                gettext("A collage takes at most {} photos"),
                &[MAX_PHOTOS],
            ));
        }
        self.update_tray(window);
        self.update_next();
    }

    /// Shows `index` in the preview pane, decoding it at the pane's own size.
    pub fn focus(&self, window: &EditorWindow, index: usize) {
        if index >= self.len() {
            return;
        }
        self.focused.set(Some(index));
        self.show_focused();
        if let (Some(worker), Some(path)) = (window.thumbs(), self.file(index)) {
            worker.request_preview(index, &path, PREVIEW_PX);
        }
    }

    fn show_focused(&self) {
        let Some(index) = self.focused.get() else {
            self.preview_stack.set_visible_child_name("empty");
            return;
        };
        if let Some(tile) = self.tiles.borrow().get(&index) {
            self.preview.set_paintable(Some(&tile.texture));
            self.preview_stack.set_visible_child_name("photo");
        } else {
            self.preview_stack.set_visible_child_name("empty");
        }
    }

    /// The photo the preview shows, if any.
    pub fn focused(&self) -> Option<usize> {
        self.focused.get()
    }

    /// A reply from the tile worker, routed by what it was asked for.
    pub fn on_reply(&self, reply: crate::thumbs::Reply) {
        if reply.preview {
            self.on_preview(reply.index, reply.result);
        } else {
            self.on_tile(reply.index, reply.result);
        }
    }

    // ---- the document ------------------------------------------------------

    /// The document the pick makes, on the first layout that fits its count.
    ///
    /// This is the picker's handover to the layout stage, and the one place a
    /// pick becomes a document: `Selection::document` is the policy the CLI's
    /// `init --photo` also uses, so the tray's order and the CLI's argument order
    /// cannot disagree. S14 replaces "the first layout" with the gallery.
    pub fn document(&self) -> Result<CollageDoc, SelectionError> {
        let selection = self.selection.borrow().clone();
        let template =
            selection
                .layouts()
                .into_iter()
                .next()
                .ok_or(SelectionError::PhotoCount {
                    found: selection.len(),
                    min: MIN_PHOTOS,
                    max: MAX_PHOTOS,
                })?;
        selection.document(&template)
    }

    /// Next: opens the editor on the picked photos.
    pub fn next(&self, window: &EditorWindow) {
        match self.document() {
            Ok(doc) => window.open_document(doc),
            Err(error) => window.toast(&error.to_string()),
        }
    }

    // ---- presentation ------------------------------------------------------

    fn update_status(&self, folder: &Path, count: usize) {
        if folder.as_os_str().is_empty() {
            self.status.set_label(&gettext("No folder chosen yet"));
            self.status.set_visible(true);
            self.title.set_subtitle("");
            return;
        }
        let name = folder
            .file_name()
            .map(file_name_from)
            .unwrap_or_else(|| folder.display().to_string());
        self.status.set_label(&fill(
            ngettext("{} · {} photo", "{} · {} photos", count as u32),
            &[&name, &count.to_string()],
        ));
        self.status.set_visible(true);
    }

    /// The count on the Next button and in the header's subtitle: HIG's
    /// selection-mode page asks the header to show how many items are selected.
    fn update_next(&self) {
        let count = self.selected_count();
        self.next.set_label(&fill(gettext("Next ({})"), &[count]));
        self.next.set_sensitive(self.can_continue());
        self.next.set_tooltip_text(Some(&if self.can_continue() {
            gettext("Open the picked photos in a collage")
        } else {
            fill(gettext("Pick at least {} photos"), &[MIN_PHOTOS])
        }));
        self.title.set_subtitle(&fill(
            ngettext("{} photo picked", "{} photos picked", count as u32),
            &[count],
        ));
    }

    /// Rebuilds the tray: one chip per photo, in cell order.
    fn update_tray(&self, window: &EditorWindow) {
        let photos = self.selection.borrow().photos().to_vec();
        while let Some(child) = self.tray.first_child() {
            self.tray.remove(&child);
        }
        self.tray_hint.set_visible(photos.is_empty());
        for (index, path) in photos.iter().enumerate() {
            self.tray.append(&chip(window, index, path));
        }
        self.update_next();
    }
}

/// The positions a selection holds, ascending.
///
/// `GtkBitset` has no `iter()`; its iterator is built by `BitsetIter::init_first`,
/// which hands back the first value together with the iterator itself.
fn selection_positions(multi: &gtk::MultiSelection) -> Vec<usize> {
    let set = multi.selection();
    let Some((iter, first)) = gtk::BitsetIter::init_first(&set) else {
        return Vec::new();
    };
    let mut positions = vec![first as usize];
    positions.extend(iter.map(|position| position as usize));
    positions
}

/// The folder name of a path, for display.
fn file_name(path: &Path) -> String {
    path.file_name().map(file_name_from).unwrap_or_default()
}

fn file_name_from(name: &std::ffi::OsStr) -> String {
    name.to_string_lossy().into_owned()
}

/// One grid cell: the picture, with the platform's own selection check over it.
fn tile_widget() -> gtk::Overlay {
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .can_shrink(true)
        .width_request(TILE_SIZE)
        .height_request(TILE_SIZE)
        .build();
    // The check mark is libadwaita's own selection-mode check button — and it is
    // an indicator, not a second control: the cell's click toggles (see "Order"),
    // so the check must not take the pointer or the keyboard.
    let check = gtk::CheckButton::new();
    check.add_css_class("selection-mode");
    check.set_halign(gtk::Align::Start);
    check.set_valign(gtk::Align::Start);
    check.set_margin_start(6);
    check.set_margin_top(6);
    check.set_can_focus(false);
    check.set_can_target(false);
    a11y::label(&check, &gettext("Picked"));
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&picture));
    overlay.add_overlay(&check);
    overlay
}

/// The check button inside a cell.
fn overlay_check(cell: &gtk::Overlay) -> Option<gtk::CheckButton> {
    let mut child = cell.first_child();
    while let Some(widget) = child {
        if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
            return Some(check.clone());
        }
        child = widget.next_sibling();
    }
    None
}

/// One tray chip: its place in the order, its file name, and the two things a
/// pick needs — move it, or drop it.
///
/// Real GTK buttons, so HIG's accessible-name and keyboard-reachability rules
/// cover them for free (the principle ruling 9 chose for the editor's buttons).
fn chip(window: &EditorWindow, index: usize, path: &Path) -> gtk::Widget {
    let name = file_name(path);
    let label = gtk::Label::builder()
        .label(fill(gettext("{}. {}"), &[&(index + 1).to_string(), &name]))
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .max_width_chars(22)
        .build();

    let earlier = gtk::Button::builder()
        .icon_name("go-previous-symbolic")
        .has_frame(false)
        .tooltip_text(gettext("Move earlier"))
        .sensitive(index > 0)
        .build();
    a11y::label(&earlier, &fill(gettext("Move {} earlier"), &[index + 1]));
    earlier.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| {
            if let Some(picker) = window.picker() {
                picker.move_photo(&window, index, -1);
            }
        }
    ));

    let later = gtk::Button::builder()
        .icon_name("go-next-symbolic")
        .has_frame(false)
        .tooltip_text(gettext("Move later"))
        .build();
    a11y::label(&later, &fill(gettext("Move {} later"), &[index + 1]));
    later.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| {
            if let Some(picker) = window.picker() {
                picker.move_photo(&window, index, 1);
            }
        }
    ));

    let remove = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .has_frame(false)
        .tooltip_text(gettext("Remove from the collage"))
        .build();
    a11y::label(
        &remove,
        &fill(gettext("Remove {} from the collage"), &[index + 1]),
    );
    remove.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| {
            if let Some(picker) = window.picker() {
                picker.remove_at(&window, index);
            }
        }
    ));

    let chip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    chip.add_css_class("card");
    chip.append(&label);
    chip.append(&earlier);
    chip.append(&later);
    chip.append(&remove);
    chip.upcast()
}

/// The photos of a `pixlay_imaging::Thumbnail` as a texture.
///
/// Straight sRGB 8-bit, which is what `MemoryFormat::R8g8b8` describes, and
/// `Thumbnail::pixels` is already exactly that (`thumb.rs`'s documented layout).
fn texture_from(thumbnail: &Thumbnail) -> gdk::Texture {
    let bytes = glib::Bytes::from_owned(thumbnail.pixels.clone());
    gdk::MemoryTexture::new(
        thumbnail.width,
        thumbnail.height,
        gdk::MemoryFormat::R8g8b8,
        &bytes,
        (thumbnail.width * 3) as usize,
    )
    .upcast()
}

/// The folder the picker opens on: `XDG_PICTURES_DIR`, or the plain `~/Pictures`.
///
/// `glib::user_special_dir` is the XDG answer (it reads `user-dirs.dirs`); the
/// fallback covers an account whose XDG configuration does not name one, where
/// the directory usually exists anyway.
pub fn default_folder() -> Option<PathBuf> {
    if let Some(dir) = glib::user_special_dir(glib::UserDirectory::Pictures)
        && dir.is_dir()
    {
        return Some(dir);
    }
    let fallback = glib::home_dir().join("Pictures");
    fallback.is_dir().then_some(fallback)
}
