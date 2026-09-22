//! The picker stage: browse a folder, look at a photo, pick 2–9 in order, Next.
//!
//! Stages 1–2 of the main path (`AGENTS.md`: `open → pick 2–9 photos → pick a
//! layout → adjust → export`). This is the root page of the window's
//! `AdwNavigationView`; the editor of S7 is pushed on top of it when Next is
//! pressed.
//!
//! # The shape (the 2026-09-22 ruling "the picker, as gthumb has it")
//!
//! Three bands, measured off the reference app's own window
//! (`docs/2026-09-22-STEPS.md`, `S13c`): **the media area takes the vast majority**
//! — the preview pane with the picked list down its right edge at the pane's own
//! height — **one row of thumbnails spans the page's width** under it, and **a
//! status bar closes the window** with gthumb's four fields (`picked / total`,
//! the photo's pixels, its file size, the zoom). The picked list's divider is the
//! session's ([`SPLITS`]); the strip is one cell tall because a 128 px cell is what
//! gthumb's `thumbnail-size` of 256 *device* pixels measures on a 2× display.
//!
//! A cell is [`TILE_SIZE`] square and a picked cell is shown by a **highlight**
//! rather than by the platform's check box: the `.picker-cell` / `.picked` classes
//! in the app's only stylesheet (`style.css`, installed by `app.rs`), which uses
//! the theme's own `--accent-bg-color` and no literal colour. That is a deliberate
//! deviation from HIG `patterns/containers/selection-mode`, recorded in
//! `docs/HIG-REVIEW.md` §3.
//!
//! The strip is a `GtkGridView` that reflows **horizontally** — GTK's own shape for
//! a single-row, horizontally-scrolling filmstrip (the list base's orientation
//! decides which axis the items flow along), with one item per vertical slice so
//! the row stays single at any widget height.
//!
//! # What this module is, and what it deliberately is not
//!
//! * **A collection view in selection mode** (HIG `patterns/containers/selection-mode`):
//!   a `GtkGridView` over the folder's photos with a `GtkMultiSelection`, a cell
//!   whose click *toggles* it, `Ctrl+A` selecting all, `Esc` clearing the pick,
//!   and a header bar whose Next button carries the count and is the batch action.
//!   Past the cap the grid *reports* the refusal instead of truncating silently,
//!   which the guidelines do not cover and ruling 3 requires.
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
//! this module's own (`pixlay_core::Selection`), the picked list down the right
//! edge is where it is visible, re-orderable (a row drag, or `Ctrl+Up`/
//! `Ctrl+Down` on the focused row — both end in [`Picker::move_row`]) and
//! **clickable: a row switches the pane to that photo** (ruling 21), and every
//! change to the grid's selection is reconciled back into it — a photo already in
//! the list keeps its place, a newly picked one is appended. That is also why the
//! cell's click is handled here rather than left to GTK: the platform's own click
//! *replaces* the selection for a multi-selection model (measured in
//! `gtklistfactorywidget.c`: a plain click sends `modify = false`, which is
//! `select_item(pos, unselect_rest = TRUE)`), while picking three photos by
//! clicking three cells has to accumulate.
//!
//! # What is decoded, and when: visible first
//!
//! S13 asked for a tile per *listed file* as soon as a folder was opened, which is
//! one decode per file in the folder: on a folder of several thousand photos that
//! is thousands of decodes for the dozen cells on screen. Now a tile is asked for
//! when its cell is **on screen** — [`Picker::bind_tile`] and the grid's own
//! adjustment both end in [`Picker::refresh_visible`], and `unbind_tile` drops the
//! request again — which is gthumb's policy (`src/FileGrid.vala`'s bind/unbind
//! callbacks with `src/Thumbnailer.vala`'s cancellable queue), with one
//! distinction GTK makes necessary: **GTK binds far more items than it shows**
//! (measured 2026-09-22: 257 items for a 1000-photo model in a 536x396 viewport,
//! the same 257 a 300-photo folder gets, and it does not trim them afterwards), so
//! "bound" is not "visible" and asking per bound cell would be 257 decodes of
//! photos nobody has scrolled to. [`Picker::visible`] is that test: the cell's own
//! allocation, intersected with the scroller's. Measured: a 300-photo folder opens
//! with **4** requests and a scroll to its 200th photo costs **7** more.
//!
//! # Threading
//!
//! A decode is 11–110 ms per photo (S4) and a folder is unbounded, so pictures are
//! built on one worker thread ([`crate::thumbs`]) and cross back as plain bytes
//! through `MainContext::invoke`. No GTK object leaves the main thread.
//!
//! # The preview pane's own size: the size it draws
//!
//! S13 decoded the pane's photo at a constant `PREVIEW_PX` = 1024 and fell back to
//! painting the 256 px *tile* when it had no preview in hand, which is the blur the
//! 2026-09-22 ruling called a defect; S13b decoded it at the pane's long edge and
//! left the ruling's second defect in place — a portrait photo in a landscape pane
//! was decoded at **1152 px** and drawn at **760 px**, 2.25× its own pixels
//! (`S13c`'s measurements). Both are gone: the pane's photo is decoded at the long
//! edge it is *drawn* at — the `Contain` fit of the pane's device size against the
//! photo's own pixels ([`Picker::preview_px`]) — rounded up to
//! [`PREVIEW_PX_STEP`] and capped at [`PREVIEW_MAX_PX`], and it never paints a
//! tile: while it waits it shows a spinner.
//!
//! That fit needs the photo's own size before the decode, and the answer is free:
//! every reply from the picture worker carries the decoded source's width and
//! height ([`pixlay_imaging::Thumbnail`]), so a photo whose *tile* is on screen —
//! which is every photo the grid can show — has its size known before the pane asks
//! for it. A photo whose size is still unknown is decoded at the pane's own long
//! edge (S13b's behaviour, the largest size the pane can use) and re-decoded at the
//! fitted size the moment the answer arrives; the second decode happens once per
//! photo, and only for a photo focused before its tile was.
//!
//! The allocation the pane follows is observed where GTK4 allows it
//! (`GdkSurface::layout` for a window resize, `GtkPaned::position` for a divider
//! drag, `GtkWidget::scale-factor` for a screen change: GTK4 has no `size-allocate`
//! signal and no `width` property).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{CollageDoc, MAX_PHOTOS, MIN_PHOTOS, Selection, SelectionError};
use pixlay_imaging::Thumbnail;

use crate::a11y;
use crate::i18n::{fill, gettext};
use crate::thumbs::Kind;
use crate::window::EditorWindow;

/// Size of one grid cell, in logical pixels.
///
/// Square whatever the photo's aspect, so the strip does not re-flow as it fills.
/// 128 is gthumb's own size — its `thumbnail-size` default of 256
/// (`data/schemas/org.gnome.gthumb.gschema.xml`) is in **device** pixels, and the
/// reference's own cells measure 250 device = **125 logical** on a 2× display
/// (measured off the two screenshots in `docs/2026-09-22-STEPS.md`, `S13c · What
/// the reference actually measures`), which is why the 2026-09-22 ruling moved it
/// back down from S13b's 256 *logical*. Well past the minimum click target HIG
/// `guidelines/pointer-touch` asks of a click target.
pub const TILE_SIZE: i32 = 128;

/// Step the preview pane's decode is rounded up to, in pixels.
///
/// The pane's fitted size in device pixels *is* the request, rounded up so that
/// dragging a divider re-decodes once per step rather than once per pixel.
pub const PREVIEW_PX_STEP: u32 = 128;

/// The pane's decode is never larger than this, in pixels.
///
/// Measured (`--release`, this machine, 2026-09-22, `pixlay-render thumb` of a
/// 1 MP photo including the ~25 ms process start): 1024 px costs 229 ms,
/// 2048 px costs 593 ms and 3840 px costs 1112 ms. 2048 is one step below the
/// display's own long edge (3840x2160, `/sys/class/drm/*/modes`), so a maximized
/// window upscales by at most 1.9x and one focus costs at most 0.59 s
/// (`docs/2026-09-22-STEPS.md`, `S13 · Ruling`).
pub const PREVIEW_MAX_PX: u32 = 2048;

/// How many fields the status bar has, in gthumb's order (`S13c`): the pick's
/// count against the folder's, the photo's pixels, its file size, the zoom.
pub const STATUS_FIELDS: usize = 4;

/// The status bar's fields, as indices into [`Picker::status`].
const STATUS_COUNT: usize = 0;
const STATUS_PIXELS: usize = 1;
const STATUS_SIZE: usize = 2;
const STATUS_ZOOM: usize = 3;

/// Bytes of decoded tiles kept in memory.
///
/// Tiles are bounded by *bytes* rather than by count, because their size follows
/// the screen: a 128 px square is 0.05 MB, and the same cell on a 2× screen is
/// 0.2 MB. 64 MB is many hundreds of tiles — far more than a strip's worth of
/// scrolling history, which is what the cache is for (`S13 · Ruling`: "a bounded
/// in-memory cache keeps a scrolled-back row instant") — and small enough to be
/// irrelevant against the render budget measured in `docs/CONTRACT.md` §8 (A0
/// compositing peaks at 941 MB).
const TILE_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Bytes of decoded previews kept in memory.
///
/// A preview is one photo at the pane's own size: 9.4 MB for the worst case here
/// (2048x1536), so this holds six of them and re-focusing any of the last few is
/// instant, which is the defect the 2026-09-22 ruling named (the pane used to keep
/// one, and a photo focused twice showed its tile).
const PREVIEW_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// How many tile requests an opened folder, or one scroll to another photo, may
/// make.
///
/// The bound the visible-first policy is held to (S13b's criterion 5): the cells
/// that are *on screen* — eight of them in the default 1100x760 window, whose
/// strip fits a row of 130 px cells across its width — and never one per file.
/// Measured 2026-09-22 (`--release`-sized decodes, debug test build): a 300-photo
/// folder opened with **9** requests and scrolling to its 200th photo cost **4**
/// more. 64 is several times the observed count, which leaves a much wider window
/// and a much larger screen room without the criterion becoming decorative; the
/// strip's own bound is GTK's, which creates at most `30 x max_columns` = 30 cells
/// for it (`gtkgridview.c`, measured 2026-09-22).
pub const TILE_REQUEST_MAX: usize = 64;

/// The picked list's initial width, in pixels.
///
/// HIG `guidelines/adaptive`: the list is a column of file names with a remove
/// button, and 260 px is what a middle-ellipsized name of a typical length needs
/// before the button. It is the width the media area gives it on a fresh window —
/// the divider moves, and where it is left is the session's.
const PICKED_LIST_WIDTH: i32 = 260;

/// The picked list's minimum width, in pixels, so the divider cannot hide it.
const PICKED_LIST_MIN_WIDTH: i32 = 180;

thread_local! {
    /// The divider position the last picker left behind, as `GtkPaned` means it:
    /// **the media area's width**, since the pane is the paned's start child. The
    /// picked list, its end child, gets the rest — which is what makes the list's
    /// height the pane's by construction.
    ///
    /// A `GtkPaned` keeps its own position for as long as the widget exists, which
    /// is the whole session for one window; this carries it to the *next* window,
    /// whose picker is a new set of widgets. It is a session value by the
    /// 2026-09-22 ruling ("its position is still the session's `SPLITS`") and not a
    /// configuration file (ruling 8: no config file), so it dies with the process.
    /// `None` means no window has moved it yet: a fresh window starts at
    /// [`PICKED_LIST_WIDTH`].
    static SPLITS: Cell<Option<i32>> = const { Cell::new(None) };
}

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
    /// Tiles in hand, keyed by the file index **and** the device size they were
    /// built at: a screen change asks for a different one, and the old size's
    /// tiles become useless rather than wrong.
    tiles: RefCell<Cache<(usize, u32)>>,
    /// Tile requests in flight, keyed the same way: the "in hand or in flight"
    /// that keeps a re-bind from becoming a second decode. The key carries the
    /// folder generation too, so a reply from the folder before this one clears
    /// its own entry rather than the live one's (`crate::thumbs`' `Key`).
    inflight: RefCell<HashSet<(u64, usize, u32)>>,
    /// The cell GTK has bound to each position.
    ///
    /// A tile arrives long after its cell was bound, and GTK re-binds a row only
    /// when the *item object* changes — which a texture arriving is not. So the
    /// cell that is showing a position is remembered here and handed its pixels
    /// directly, and a cell that is recycled (or scrolled away) drops out of the
    /// map again. `GtkMultiSelection` is untouched by this: the pick never moves.
    cells: RefCell<HashMap<usize, glib::WeakRef<gtk::Stack>>>,
    /// Files the decoder refused, with its reason: a cell that cannot show a
    /// photo is reported rather than silently blank (`scan`'s S9 rule).
    failures: RefCell<HashMap<usize, String>>,
    /// The pane's own decodes, keyed by photo and the size the pane was at: this is
    /// what a re-focus is answered from, and what the stage's pixel criterion
    /// compares against `pixlay-render thumb` at the same size.
    previews: RefCell<Cache<(usize, u32)>>,
    /// Preview requests in flight, keyed the same way.
    preview_inflight: RefCell<HashSet<(u64, usize, u32)>>,
    /// The photos' own pixel sizes, as every reply reports them (the decode has the
    /// source in hand, so this costs nothing): the pane's fitted decode and the
    /// status bar's zoom are both ratios against these. A photo whose size is not
    /// known yet is decoded at the pane's own long edge and re-decoded at its fitted
    /// size once the answer arrives.
    sizes: RefCell<HashMap<usize, (u32, u32)>>,
    /// The preview on screen, and the photo it belongs to. `None` while the pane is
    /// waiting for one.
    shown: RefCell<Option<(PathBuf, Rc<Picture>, u32)>>,
    /// The photo the pane is focused on, if any.
    focused: Cell<Option<usize>>,
    /// How many tiles have been asked for since the folder was listed. The number
    /// the visible-first policy is measured in (S13b's criterion 5).
    requested: Cell<usize>,
    /// The folder generation this picker is answering for: a reply carrying any
    /// other one belongs to a folder that is no longer on screen.
    epoch: Cell<u64>,
    /// Set while this module writes the grid's selection, so that its own write
    /// is not read back as a user click.
    syncing: Rc<Cell<bool>>,

    // ---- widgets ---------------------------------------------------------
    /// The grid's model: one `gio::File` per photo, in listing order.
    store: gio::ListStore,
    /// The grid's selection. A set: membership only, never order.
    multi: gtk::MultiSelection,
    /// The grid itself, for the widgets and the HIG checks that read it. It reflows
    /// horizontally, so the strip is one row.
    grid: gtk::GridView,
    /// The page's root widget.
    root: adw::ToolbarView,
    /// The strip's scroller, for the geometry the ruling fixes (one row, the page's
    /// full width).
    strip: gtk::ScrolledWindow,
    /// The preview pane: the focused photo, or the empty state.
    preview: gtk::Picture,
    preview_stack: gtk::Stack,
    /// The picked list, whose placeholder says "nothing picked yet" while it is
    /// empty.
    list: gtk::ListBox,
    /// The one divider: the picked list against the pane, so the list's height *is*
    /// the pane's.
    columns: gtk::Paned,
    /// The header bar, whose slots the ruled chrome fixes (the folder button at the
    /// start, the heading in the centre, the menu and Next at the end).
    header: adw::HeaderBar,
    /// The primary menu (`picker_menu`), whose model the chrome check reads.
    menu: gtk::MenuButton,
    /// The status bar: the page's bottom bar, the last band.
    status_bar: gtk::Box,
    /// Next, which carries the count, and the content object that carries it — the
    /// label has to go on the content, because `GtkButton::set_label` *replaces* the
    /// button's child (`gtkbutton.c`) and would destroy the icon.
    next: gtk::Button,
    next_content: adw::ButtonContent,
    /// The status bar's four fields, in gthumb's order (`STATUS_*`).
    status: [gtk::Label; STATUS_FIELDS],
    /// The page's own title widget: the heading in the header's centre, with the
    /// folder as its subtitle (gthumb keeps the location there too).
    title: adw::WindowTitle,
}

/// One decoded picture, ready to paint.
///
/// The texture and the bytes behind it are the *same* buffer — a
/// `GdkMemoryTexture` holds a reference to the `glib::Bytes` it was built from — so
/// a picture costs one copy of its pixels rather than two, and `texture()` is a
/// refcount bump where the previous code copied the whole image on every paint.
struct Picture {
    texture: gdk::Texture,
    bytes: glib::Bytes,
    width: i32,
    height: i32,
}

impl Picture {
    /// Decodes a thumbnail's pixels into a texture and keeps the buffer.
    fn new(thumbnail: Thumbnail) -> Self {
        let bytes = glib::Bytes::from_owned(thumbnail.pixels);
        let texture = gdk::MemoryTexture::new(
            thumbnail.width,
            thumbnail.height,
            gdk::MemoryFormat::R8g8b8,
            &bytes,
            (thumbnail.width * 3) as usize,
        )
        .upcast();
        Self {
            texture,
            bytes,
            width: thumbnail.width,
            height: thumbnail.height,
        }
    }

    /// The bytes this picture costs in a cache.
    fn bytes(&self) -> usize {
        self.bytes.len()
    }
}

/// A bounded cache of decoded pictures, keyed by what was asked for.
///
/// Two of them live in the picker — the grid's tiles and the pane's previews — and
/// both are bounded by **bytes**, not by entries: a preview at 2048 px is 9.4 MB
/// and a tile at 128 px is 0.05 MB, so counting entries would mean nothing. The
/// oldest entry is evicted first, least recently used: a row scrolled back *to* is
/// what a cache is for, and a row scrolled past is what it can afford to lose.
struct Cache<K: Eq + std::hash::Hash + Clone> {
    entries: HashMap<K, Rc<Picture>>,
    order: VecDeque<K>,
    bytes: usize,
    limit: usize,
}

impl<K: Eq + std::hash::Hash + Clone> Cache<K> {
    fn new(limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            limit,
        }
    }

    /// The picture for a key, marking it as the most recently used.
    fn get(&mut self, key: &K) -> Option<Rc<Picture>> {
        if self.entries.contains_key(key) {
            self.touch(key);
        }
        self.entries.get(key).cloned()
    }

    fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    fn insert(&mut self, key: K, picture: Rc<Picture>) {
        self.remove(&key);
        self.bytes += picture.bytes();
        self.order.push_back(key.clone());
        self.entries.insert(key, picture);
        while self.bytes > self.limit {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(picture) = self.entries.remove(&oldest) {
                self.bytes -= picture.bytes();
            }
        }
    }

    fn remove(&mut self, key: &K) {
        if let Some(picture) = self.entries.remove(key) {
            self.bytes -= picture.bytes();
            self.order.retain(|held| held != key);
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn touch(&mut self, key: &K) {
        self.order.retain(|held| held != key);
        self.order.push_back(key.clone());
    }
}

impl Picker {
    /// Builds the picker page.
    pub fn build(window: &EditorWindow) -> Rc<Self> {
        let store = gio::ListStore::new::<gio::File>();
        let multi = gtk::MultiSelection::new(Some(store.clone()));

        // The strip: one row along the page's bottom, spanning its full width (the
        // 2026-09-22 ruling). A `GtkGridView` told to reflow **horizontally** is
        // exactly that — the list base's orientation decides which axis the items
        // flow along, so a horizontal grid puts column after column along x and
        // scrolls along x — and `max_columns(1)` keeps it a *single* row: GTK takes
        // the items per vertical slice to be `height / cell`, clamped to
        // `[min_columns, max_columns]` (`gtkgridview.c`, `compute_n_columns`), so 1
        // is one row at any height. It also caps the cells GTK keeps around the
        // anchor at `30 x max_columns` (`gtk_grid_view_init`), which is what makes
        // the strip's own bound small.
        let grid = gtk::GridView::builder()
            .model(&multi)
            .orientation(gtk::Orientation::Horizontal)
            .min_columns(1)
            .max_columns(1)
            .single_click_activate(false)
            .enable_rubberband(true)
            .build();
        grid.set_vexpand(false);
        grid.set_hexpand(false);
        // The grid's own style class, for the one rule that sizes its items
        // (`.thumbnail-grid > child`, `style.css`) — and the name the reference uses
        // for the same node (`gthumb/data/css/style.css:26-42`).
        grid.add_css_class("thumbnail-grid");
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
                // The highlight follows the model's own selection state, which
                // changes without a re-bind: a click that toggles a cell has to
                // show at once (`GtkListItem:selected` is the model's answer).
                item.connect_notify_local(Some("selected"), move |item, _| {
                    highlight(item.child().as_ref(), item.is_selected());
                });
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
                    picker.bind_tile(&window, item);
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
                    picker.unbind_tile(&window, item);
                }
            }
        ));
        grid.set_factory(Some(&factory));

        // The keyboard path, on the grid so it works wherever the strip has focus:
        // `Esc` clears the pick, and `Enter` and `Space` both *toggle* the focused
        // cell — `Enter` through GTK's own `list.activate-item`, which emits the
        // grid's `activate` signal (S13 claimed this in a comment and answered
        // neither), and `Space` through the list item's `listitem.select`. `Ctrl+A`
        // is not bound here: `GtkListBase` already binds it to `list.select-all`
        // (`gtklistbase.c:1389`), and the model's own answer is enough — the picker
        // reconciles the changed selection and reports the cap. Two bindings for one
        // key meant the second fired on a selection that was already capped.
        grid.connect_activate(glib::clone!(
            #[weak]
            window,
            move |_, position| {
                if let Some(picker) = window.picker() {
                    picker.toggle(&window, position);
                }
            }
        ));
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            window,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                let Some(picker) = window.picker() else {
                    return glib::Propagation::Proceed;
                };
                match key {
                    gdk::Key::Escape => {
                        picker.clear_selection(&window);
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            }
        ));
        grid.add_controller(keys);

        let strip = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&grid)
            .build();
        a11y::label(&strip, &gettext("Photos"));

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
        // The pane waits behind a spinner rather than painting the cell's tile:
        // a 128 px tile stretched over the pane is the blur the 2026-09-22 ruling
        // called a defect (`S13 · Ruling`).
        let loading = gtk::Spinner::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        let preview_stack = gtk::Stack::new();
        // No crossfade: measured 2026-09-23, a transition that is still running
        // paints *both* children at a partial opacity, and in a window snapshot the
        // pane came out empty while the widget held the photo — the picture has to be
        // drawn because it is there, not because an animation finished.
        preview_stack.set_transition_type(gtk::StackTransitionType::None);
        preview_stack.add_named(&empty, Some("empty"));
        preview_stack.add_named(&loading, Some("loading"));
        preview_stack.add_named(&preview, Some("photo"));
        preview_stack.set_visible_child_name("empty");
        a11y::label(&preview_stack, &gettext("Photo preview"));

        // ---- the picked list ------------------------------------------------
        // A single-selection list, not a plain one: its rows have to be focusable
        // for `Ctrl+Up`/`Ctrl+Down` to mean anything (HIG `guidelines/pointer-touch`
        // asks that every pointer action have a keyboard path), and the selection
        // itself is only the platform's own way of showing the focused row.
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        list.add_css_class("boxed-list");
        a11y::label(&list, &gettext("Picked photos"));

        let list_hint = gtk::Label::builder()
            .label(gettext("Nothing picked yet"))
            .xalign(0.0)
            .build();
        list_hint.add_css_class("dim-label");
        list_hint.add_css_class("caption");
        // The hint is the list's own placeholder rather than a sibling above it: a
        // sibling would take height from the list, and the ruling's arrangement is
        // that the list *is* the media area's height (S13c).
        list.set_placeholder(Some(&list_hint));

        let list_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        a11y::label(&list_scroller, &gettext("Picked photos"));

        // The paned's end child, and nothing else in it: whatever else lived here
        // would come out of the list's height.
        let picked = gtk::Box::new(gtk::Orientation::Vertical, 0);
        picked.set_size_request(PICKED_LIST_MIN_WIDTH, -1);
        picked.set_margin_start(12);
        picked.set_margin_end(12);
        picked.append(&list_scroller);

        // ---- the one divider ------------------------------------------------
        // The shape the 2026-09-22 ruling fixes: the media area takes the page's
        // majority, and the picked list runs down its right edge **at the pane's own
        // height** — which is what putting it in the paned's end slot means, so the
        // two cannot disagree. Its position is the session's (`SPLITS`).
        let columns = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&preview_stack)
            .end_child(&picked)
            .position(split_position(window))
            // The extra space a wider window gets goes to the media area, which is
            // the surface the ruling makes the majority: `GtkPaned` grows the start
            // child only, and the end child keeps the width it was given
            // (`gtkpaned.c`: `resize_start_child && !resize_end_child` → the end
            // child's own request, the start child the rest).
            .resize_start_child(true)
            .resize_end_child(false)
            .shrink_start_child(true)
            // `shrink-end-child` **must** be true, and this is the trap of putting a
            // scrolled list in a paned: a `GtkScrolledWindow`'s *natural* size is its
            // whole content, and with the property left at its default the paned's
            // own minimum becomes that width. The list is then allocated all of its
            // content, and the pane beside it loses the space the ruling gives it.
            .shrink_end_child(true)
            .build();

        // ---- the status bar -------------------------------------------------
        // gthumb's own four fields, in its order (`S13c`), in the cluster shape it
        // uses: `GthStatus` is `spacing 24` and right-aligned in the bottom bar
        // (`data/ui/browser.ui:382-391`), and the bar itself is `padding: 4px`. What
        // each field shows is `update_status_line`'s; this is where they live.
        let statusbar = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        statusbar.add_css_class("statusbar");
        statusbar.set_halign(gtk::Align::End);
        statusbar.set_valign(gtk::Align::Center);
        let status: [gtk::Label; STATUS_FIELDS] = std::array::from_fn(|_| {
            let label = gtk::Label::new(None);
            label.add_css_class("dim-label");
            statusbar.append(&label);
            label
        });

        // ---- the header -----------------------------------------------------
        // HIG `patterns/containers/header-bars`: primary and navigation actions at
        // the **start**, the heading in the **centre**, the menu at the **end**.
        // S13b put every control at the end and styled Next `suggested-action`,
        // which that page asks header bars to avoid; both are corrected here, and
        // the picker gains the primary menu it never had (both reference apps end
        // their headers in one).
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

        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(gettext("Main menu"))
            .primary(true)
            .menu_model(&picker_menu())
            .build();
        a11y::label(&menu, &gettext("Main menu"));

        // The label goes on the `AdwButtonContent`, never on the button: laying it
        // on the button *replaces* the child (`gtkbutton.c`, `gtk_button_set_label`
        // swaps anything that is not a `GtkLabel`), which is how S13b's Next lost its
        // icon on the first `update_next`.
        let next_content = adw::ButtonContent::new();
        next_content.set_icon_name("go-next-symbolic");
        next_content.set_label(&gettext("Next"));
        let next = gtk::Button::builder()
            .child(&next_content)
            .tooltip_text(gettext("Open the picked photos in a collage"))
            .build();
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
        header.pack_start(&folder);
        header.pack_end(&menu);
        header.pack_end(&next);

        // ---- the page -------------------------------------------------------
        // The three bands, top to bottom: the media area with the picked list
        // beside it (it takes every pixel the strip and the status bar do not), one
        // row of thumbnails across the page's width, and the status bar as the
        // page's own bottom bar — so the band the ruling measures the proportions
        // against is exactly the content above it.
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        columns.set_vexpand(true);
        body.append(&columns);
        body.append(&strip);
        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.add_bottom_bar(&statusbar);
        root.set_content(Some(&body));
        // loupe's idiom (`src/widgets/image_window.rs:986-998`): the bar is `raised`
        // while content starts *below* it — which it does here, since the media area
        // does not extend under the header (`AdwToolbarView`'s own default is flat).
        root.set_top_bar_style(adw::ToolbarStyle::Raised);
        // The page is handed to the struct below, so the handlers that follow keep
        // their own handle on it.
        let page = root.clone();

        let picker = Rc::new(Self {
            selection: RefCell::new(Selection::default()),
            folder: RefCell::new(None),
            files: RefCell::new(Vec::new()),
            tiles: RefCell::new(Cache::new(TILE_CACHE_BYTES)),
            inflight: RefCell::new(HashSet::new()),
            cells: RefCell::new(HashMap::new()),
            failures: RefCell::new(HashMap::new()),
            previews: RefCell::new(Cache::new(PREVIEW_CACHE_BYTES)),
            preview_inflight: RefCell::new(HashSet::new()),
            sizes: RefCell::new(HashMap::new()),
            shown: RefCell::new(None),
            focused: Cell::new(None),
            requested: Cell::new(0),
            epoch: Cell::new(0),
            syncing: Rc::new(Cell::new(false)),
            store,
            multi: multi.clone(),
            grid: grid.clone(),
            root,
            strip: strip.clone(),
            preview,
            preview_stack,
            list,
            columns,
            header: header.clone(),
            menu: menu.clone(),
            status_bar: statusbar.clone(),
            next,
            next_content,
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

        picker.follow_the_pane(window);
        picker.connect_picked_list(window);
        picker.update_next();
        picker.update_status_line();

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

    // ---- the page's parts --------------------------------------------------

    /// The page's root widget, for the window's navigation page.
    pub fn root(&self) -> adw::ToolbarView {
        self.root.clone()
    }

    /// The photo grid, whose model the HIG checks read.
    pub fn grid(&self) -> gtk::GridView {
        self.grid.clone()
    }

    /// The strip's scroller: one row of cells across the page's width.
    pub fn strip(&self) -> gtk::ScrolledWindow {
        self.strip.clone()
    }

    /// The picked list, whose rows are the pick's order.
    pub fn picked_list(&self) -> gtk::ListBox {
        self.list.clone()
    }

    /// The status bar's four fields, as text, in gthumb's order: the pick's count
    /// against the folder's, the photo's pixels, its file size, the zoom.
    ///
    /// The stage's own numbers, readable without a widget tree — which is how the
    /// ruling's "the status line is right" is checked (`tests/picker.rs`).
    pub fn status_line(&self) -> [String; STATUS_FIELDS] {
        std::array::from_fn(|field| self.status[field].label().to_string())
    }

    /// The status bar, the page's last band.
    pub fn status_bar(&self) -> gtk::Box {
        self.status_bar.clone()
    }

    /// The page's header bar, whose slots the ruled chrome fixes.
    pub fn header(&self) -> adw::HeaderBar {
        self.header.clone()
    }

    /// The page's primary menu, whose model the chrome check reads.
    pub fn menu_button(&self) -> gtk::MenuButton {
        self.menu.clone()
    }

    /// The preview pane as a widget: the picture when a photo is focused, the
    /// empty or waiting state when none is.
    ///
    /// The pane rather than the picture, because with nothing focused the picture
    /// has no allocation at all — it is the stage's *pane* the adaptive check is
    /// about, and the pane's own size is what the preview is decoded for.
    pub fn preview_widget(&self) -> gtk::Stack {
        self.preview_stack.clone()
    }

    /// The picture the preview pane shows.
    pub fn preview_picture(&self) -> gtk::Picture {
        self.preview.clone()
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

    /// Lists `folder`, clearing the selection and forgetting the folder before it.
    pub fn open_folder(&self, window: &EditorWindow, folder: &Path) {
        let files = match pixlay_imaging::list_folder(folder, false) {
            Ok(files) => files,
            Err(error) => {
                window.toast(&error.to_string());
                return;
            }
        };
        // Nothing about the old folder survives, and it goes first so that what the
        // status bar and the picked list report below is about the new one: the
        // tiles, the previews, the sizes and the requests in flight were all keyed by
        // an index into a listing that no longer exists, and a request that is still
        // queued is dropped before it costs a decode. The generation makes the
        // answers that are already being decoded recognisable, and they are ignored
        // on arrival.
        if let Some(worker) = window.thumbs() {
            worker.forget();
            self.epoch.set(worker.epoch());
        }
        self.tiles.borrow_mut().clear();
        self.previews.borrow_mut().clear();
        self.cells.borrow_mut().clear();
        self.inflight.borrow_mut().clear();
        self.preview_inflight.borrow_mut().clear();
        self.failures.borrow_mut().clear();
        self.sizes.borrow_mut().clear();
        *self.shown.borrow_mut() = None;
        self.focused.set(None);
        self.requested.set(0);

        *self.folder.borrow_mut() = Some(folder.to_path_buf());
        self.files.replace(files.clone());
        self.syncing.set(true);
        self.store.remove_all();
        for path in &files {
            self.store.append(&gio::File::for_path(path));
        }
        self.syncing.set(false);

        // The picks belong to the photos that were on screen: a folder change
        // clears them rather than leaving paths in the document's order that the
        // strip no longer shows. This also rewrites the picked list, the heading's
        // subtitle and the status bar, so nothing on the page describes the folder
        // that was open before it.
        self.clear_selection(window);
        self.show_focused();
    }

    /// Opens the session's default folder: `XDG_PICTURES_DIR`, or `~/Pictures`.
    ///
    /// Ruling 8: "`XDG_PICTURES_DIR` by default, plus a folder chooser … no
    /// configuration file". An account with neither gets the empty strip and the
    /// folder button, which is a state to act on rather than a startup failure.
    pub fn open_default_folder(&self, window: &EditorWindow) {
        match default_folder() {
            Some(folder) => self.open_folder(window, &folder),
            None => {
                self.update_next();
                self.update_status_line();
            }
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

    /// How many tile requests this folder has cost. Bounded by the visible cells
    /// plus GTK's own margin, never by the folder (`TILE_REQUEST_MAX`).
    pub fn tile_requests(&self) -> usize {
        self.requested.get()
    }

    /// How many tiles are still being decoded.
    pub fn pending_tiles(&self) -> usize {
        self.inflight.borrow().len()
    }

    /// The long edge a tile is built at, in device pixels: the cell's own size on
    /// this screen, so a HiDPI display is sharp without guessing a factor.
    pub fn tile_px(&self) -> u32 {
        TILE_SIZE as u32 * self.scale_factor()
    }

    /// The long edge the preview pane's photo is decoded at, in device pixels.
    ///
    /// **The size it is drawn at** (ruling 22): the `Contain` fit of the pane's
    /// device size against the focused photo's own pixels, rounded up to
    /// [`PREVIEW_PX_STEP`] and capped at [`PREVIEW_MAX_PX`]. S13b asked for the
    /// pane's long edge whatever the photo's aspect, which decoded a portrait
    /// **1152 px** for a **760 px** draw; the number here never exceeds the pane's
    /// own long edge by more than the rounding step. A photo whose size is not known
    /// yet gets the pane's long edge — the largest size the pane could need — and is
    /// re-asked for at its fitted size when the answer arrives
    /// ([`Picker::on_preview`]).
    pub fn preview_px(&self) -> u32 {
        let edge = match self.focused_size() {
            Some(size) => fitted_long_edge(self.pane_device_size(), size),
            None => {
                let (width, height) = self.pane_device_size();
                f64::from(width.max(height))
            }
        };
        let rounded = (edge.ceil().max(0.0) as u32).div_ceil(PREVIEW_PX_STEP) * PREVIEW_PX_STEP;
        rounded.clamp(PREVIEW_PX_STEP, PREVIEW_MAX_PX)
    }

    /// The focused photo's own pixel size, once a reply has reported it.
    ///
    /// Every reply from the picture worker carries it — the decode had the source
    /// in hand ([`pixlay_imaging::Thumbnail`]) — so a photo whose tile is on screen
    /// (which is every photo the strip can show) has its size before the pane asks
    /// for its preview.
    pub fn focused_size(&self) -> Option<(u32, u32)> {
        self.sizes
            .borrow()
            .get(&self.focused.get()?)
            .copied()
            .filter(|(width, height)| *width > 0 && *height > 0)
    }

    /// The zoom the status bar shows, in whole percent.
    ///
    /// `round(100 × drawn / photo long edge)`, where the drawn edge is the `Contain`
    /// fit the pane paints (never the rounded-up *decode* size): 100 % is one image
    /// pixel per device pixel, which is what both references mean by it (gthumb
    /// `ImageViewer.vala:665`, loupe `apply_zoom`).
    pub fn zoom_percent(&self) -> Option<u32> {
        let (source_width, source_height) = self.focused_size()?;
        let drawn = fitted_long_edge(self.pane_device_size(), (source_width, source_height));
        let longest = f64::from(source_width.max(source_height));
        Some((100.0 * drawn / longest).round().max(0.0) as u32)
    }

    /// The pane's size in device pixels, zero while it is unallocated.
    fn pane_device_size(&self) -> (u32, u32) {
        let scale = self.scale_factor();
        (
            self.preview_stack.width().max(0) as u32 * scale,
            self.preview_stack.height().max(0) as u32 * scale,
        )
    }

    /// The screen's scale factor, never zero.
    fn scale_factor(&self) -> u32 {
        self.grid.scale_factor().max(1) as u32
    }

    /// The photo the preview pane decoded, with the pixels it shows.
    ///
    /// The stage's pixel criterion — "the preview's pixels equal
    /// `pixlay-render thumb` of the same photo at the same size" — is a comparison
    /// of these bytes against the file that command writes, so the picture on
    /// screen is checkable without a pointer and without a second renderer. It is
    /// `None` while the pane is waiting for the focused photo's own preview, which
    /// is the state S13 could not distinguish from a blurred one.
    pub fn preview_pixels(&self) -> Option<(PathBuf, i32, i32, Vec<u8>)> {
        let shown = self.shown.borrow();
        let (path, picture, _) = shown.as_ref()?;
        Some((
            path.clone(),
            picture.width,
            picture.height,
            picture.bytes.as_ref().to_vec(),
        ))
    }

    /// Whether the pane is showing the focused photo at the pane's current size.
    pub fn preview_current(&self) -> bool {
        let Some(index) = self.focused.get() else {
            return false;
        };
        let shown = self.shown.borrow();
        let Some((path, _, px)) = shown.as_ref() else {
            return false;
        };
        *px == self.preview_px() && self.file(index).as_ref() == Some(path)
    }

    /// The widget GTK has bound to a position, if it is on screen.
    pub fn cell_widget(&self, position: usize) -> Option<gtk::Widget> {
        self.cells
            .borrow()
            .get(&position)
            .and_then(|weak| weak.upgrade())
            .map(|stack| stack.upcast())
    }

    /// The divider's position in pixels: the picked list's width, and so the width
    /// the media pane keeps.
    pub fn split_position(&self) -> i32 {
        self.columns.position()
    }

    /// Moves the divider, which is also where the next window starts from
    /// (`SPLITS`).
    pub fn set_split_position(&self, columns: i32) {
        self.columns.set_position(columns);
    }

    // ---- tiles -------------------------------------------------------------

    /// One tile arrived.
    pub fn on_tile(&self, index: usize, px: u32, thumbnail: Result<Thumbnail, String>) {
        match thumbnail {
            Ok(thumbnail) => {
                // The reply carries the photo's own size, which is where the pane's
                // fitted decode and the status bar's zoom come from — free here,
                // because the decode had the source in hand.
                self.sizes
                    .borrow_mut()
                    .insert(index, (thumbnail.source_width, thumbnail.source_height));
                let picture = Rc::new(Picture::new(thumbnail));
                self.tiles
                    .borrow_mut()
                    .insert((index, px), Rc::clone(&picture));
                // The cell that is showing this position — if there is one — gets
                // its pixels now: GTK does not re-bind a row because a texture
                // arrived, and the item object is untouched on purpose (the pick is
                // keyed on it). A tile built for another screen's size is kept for
                // that size and not painted here.
                if px == self.tile_px()
                    && let Some(stack) = self.bound_cell(index)
                {
                    paint_tile(&stack, &picture);
                }
                if self.focused.get() == Some(index) {
                    // The photo the pane is on just became measurable: its pixels
                    // and its zoom are numbers the status bar did not have.
                    self.update_status_line();
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

    /// Whether a cell is inside the part of the grid the user can see.
    ///
    /// GTK's item manager binds more items than are visible — measured 2026-09-22:
    /// a fresh 1000-photo model in a 536x396 viewport binds **257** items, the same
    /// 257 a 300-photo folder binds, so "bound" is a constant of GTK and not a
    /// statement about the folder — and it does not trim them afterwards. Asking
    /// for a decode per *bound* cell would therefore be 257 decodes of photos
    /// nobody has scrolled to; the 2026-09-22 ruling's own policy is "visible
    /// first" (`src/FileGrid.vala` asks only for the rows that are on screen), so
    /// this is the test that makes the requests match the screen: the cell's own
    /// bounds, against the scroller's allocation.
    fn visible(&self, cell: &gtk::Widget) -> bool {
        let Some(scroller) = self.grid.parent().and_downcast::<gtk::ScrolledWindow>() else {
            return true;
        };
        let (width, height) = (scroller.width(), scroller.height());
        if width <= 0 || height <= 0 {
            // Nothing is on screen until the widget has been allocated, and the
            // allocation asks again (`refresh_visible`, from the grid's own
            // adjustment and from the window's layout).
            return false;
        }
        // A bound cell that has not been allocated yet has no position to test, and
        // `compute_bounds` answers for it with its CSS border alone — a 6x6 rect at
        // (-3,-3), which *does* intersect the viewport. Measured 2026-09-22: that
        // alone made every one of GTK's 257 bound cells "visible".
        if cell.width() <= 0 || cell.height() <= 0 {
            return false;
        }
        let Some(bounds) = cell.compute_bounds(&scroller) else {
            return false;
        };
        let viewport = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
        bounds.intersection(&viewport).is_some()
    }

    /// Asks for the tiles of every bound cell that is on screen now.
    ///
    /// This is what a scroll is, and what the first layout is: GTK binds the cells
    /// before it has an allocation, so nothing is visible yet at that point.
    fn refresh_visible(&self, window: &EditorWindow) {
        let positions: Vec<usize> = self.cells.borrow().keys().copied().collect();
        for position in positions {
            let Some(cell) = self.bound_cell(position) else {
                continue;
            };
            if self.visible(&cell.clone().upcast()) {
                self.ask_for_tile(window, position);
            }
        }
    }

    /// Asks for a position's tile, unless it is off screen, in hand, or already on
    /// its way.
    fn ask_for_tile_if_visible(&self, window: &EditorWindow, position: usize) {
        let Some(cell) = self.bound_cell(position) else {
            return;
        };
        if self.visible(&cell.clone().upcast()) {
            self.ask_for_tile(window, position);
        }
    }

    /// Asks for a position's tile, unless it is in hand or already on its way.
    fn ask_for_tile(&self, window: &EditorWindow, position: usize) {
        let Some(worker) = window.thumbs() else {
            return;
        };
        let px = self.tile_px();
        let epoch = self.epoch.get();
        if self.tiles.borrow().contains(&(position, px))
            || !self.inflight.borrow_mut().insert((epoch, position, px))
        {
            return;
        }
        let Some(path) = self.file(position) else {
            self.inflight.borrow_mut().remove(&(epoch, position, px));
            return;
        };
        self.requested.set(self.requested.get() + 1);
        worker.request_tile(position, &path, px);
    }

    /// Drops a position's tile request, because its cell is gone from the screen.
    fn drop_tile(&self, window: &EditorWindow, position: usize) {
        let px = self.tile_px();
        if self
            .inflight
            .borrow_mut()
            .remove(&(self.epoch.get(), position, px))
            && let Some(worker) = window.thumbs()
        {
            worker.cancel_tile(position, px);
        }
    }

    /// Fills one cell: its tile, its name, and its selection highlight.
    fn bind_tile(&self, window: &EditorWindow, item: &gtk::ListItem) {
        let position = item.position() as usize;
        let Some(stack) = item.child().and_downcast::<gtk::Stack>() else {
            return;
        };
        let weak = glib::WeakRef::new();
        weak.set(Some(&stack));
        self.cells.borrow_mut().insert(position, weak);
        self.show_cell(window, position, item.is_selected());
    }

    /// A cell is being recycled: drop its request and its highlight.
    fn unbind_tile(&self, window: &EditorWindow, item: &gtk::ListItem) {
        let position = item.position() as usize;
        self.cells.borrow_mut().remove(&position);
        self.drop_tile(window, position);
        if let Some(stack) = item.child().and_downcast::<gtk::Stack>() {
            highlight(item.child().as_ref(), false);
            stack.set_visible_child_name("loading");
        }
    }

    /// Puts one bound cell into the state its file is in: its tile, the decoder's
    /// refusal, or the loading state while it waits.
    fn show_cell(&self, window: &EditorWindow, position: usize, selected: bool) {
        let Some(stack) = self.bound_cell(position) else {
            return;
        };
        let name = self
            .files
            .borrow()
            .get(position)
            .map(|path| file_name(path))
            .unwrap_or_default();
        highlight(Some(&stack.clone().upcast::<gtk::Widget>()), selected);
        // HIG `guidelines/accessibility`: the tile is what a screen reader
        // announces, and it has to say which photo it is.
        a11y::label(&stack, &name);

        let failure = self.failures.borrow().get(&position).cloned();
        if let Some(reason) = failure {
            stack.set_visible_child_name("failed");
            tooltip(&stack, &reason);
            return;
        }
        let px = self.tile_px();
        // The borrow ends here: `ask_for_tile` reads the same cache.
        let tile = self.tiles.borrow_mut().get(&(position, px));
        match tile {
            Some(picture) => paint_tile(&stack, &picture),
            None => {
                stack.set_visible_child_name("loading");
                tooltip(&stack, &name);
                self.ask_for_tile_if_visible(window, position);
            }
        }
    }

    /// The cell bound to a position, if it is still on screen.
    fn bound_cell(&self, position: usize) -> Option<gtk::Stack> {
        self.cells
            .borrow()
            .get(&position)
            .and_then(|weak| weak.upgrade())
    }

    /// A file the decoder refused: the cell says so rather than staying blank.
    fn show_failure(&self, index: usize, reason: &str) {
        self.failures.borrow_mut().insert(index, reason.to_string());
        if let Some(stack) = self.bound_cell(index) {
            stack.set_visible_child_name("failed");
            tooltip(&stack, reason);
        }
        if self.focused.get() == Some(index) {
            self.preview.set_tooltip_text(Some(reason));
        }
    }

    // ---- the preview -------------------------------------------------------

    /// Shows `index` in the preview pane, decoding it at the size it is drawn at.
    pub fn focus(&self, window: &EditorWindow, index: usize) {
        if index >= self.len() {
            return;
        }
        self.focused.set(Some(index));
        self.show_focused();
        self.ask_for_preview(window);
        // The photo's pixels, its size and the zoom are all about the focused
        // photo, so the status bar moves with the pane.
        self.update_status_line();
    }

    /// Shows the photo a picked row stands for, which is what clicking one does
    /// (ruling 21; S13b's list only selected).
    ///
    /// The row's index is a place in the *pick's order*, which is not a position in
    /// the folder: the photo is looked up and focused by where the strip lists it.
    pub fn focus_picked(&self, window: &EditorWindow, row: usize) {
        let photo = self.selection.borrow().photos().get(row).cloned();
        let Some(photo) = photo else {
            return;
        };
        if let Some(position) = self.files.borrow().iter().position(|file| *file == photo) {
            self.focus(window, position);
        }
    }

    /// Follows the pane's own allocation, because that is the size its photo is
    /// decoded at.
    ///
    /// Called from the three places GTK4 can report it (see the module docs), and
    /// cheap when nothing changed: the decode is asked for by
    /// `(photo, device pixels)`, so an unchanged size asks for nothing.
    pub fn refresh_pane(&self, window: &EditorWindow) {
        self.show_focused();
        self.ask_for_preview(window);
        self.update_status_line();
        // A resize moves the strip's viewport too, so what is visible can have
        // changed without a scroll.
        self.refresh_visible(window);
    }

    /// The screen's scale changed: every tile and the pane's preview were built for
    /// the old one.
    fn refresh_scale(&self, window: &EditorWindow) {
        let bound: Vec<usize> = self.cells.borrow().keys().copied().collect();
        for position in bound {
            if let Some(stack) = self.bound_cell(position) {
                stack.set_visible_child_name("loading");
            }
        }
        self.refresh_pane(window);
    }

    /// Asks for the focused photo's preview at the pane's own size.
    fn ask_for_preview(&self, window: &EditorWindow) {
        let Some(index) = self.focused.get() else {
            return;
        };
        let Some(worker) = window.thumbs() else {
            return;
        };
        let px = self.preview_px();
        // A request for the same photo at another size is not the picture this pane
        // needs any more: dropping it is what keeps a divider drag from decoding a
        // size nobody will see.
        let stale: Vec<u32> = self
            .preview_inflight
            .borrow()
            .iter()
            .filter(|(_, other, size)| *other == index && *size != px)
            .map(|(_, _, size)| *size)
            .collect();
        for size in stale {
            self.preview_inflight
                .borrow_mut()
                .remove(&(self.epoch.get(), index, size));
            worker.cancel_preview(index, size);
        }
        let epoch = self.epoch.get();
        if self.previews.borrow().contains(&(index, px))
            || !self
                .preview_inflight
                .borrow_mut()
                .insert((epoch, index, px))
        {
            return;
        }
        let Some(path) = self.file(index) else {
            self.preview_inflight
                .borrow_mut()
                .remove(&(epoch, index, px));
            return;
        };
        worker.request_preview(index, &path, px);
    }

    /// One preview arrived (a decode at the size the pane draws, not a resampled
    /// tile).
    pub fn on_preview(
        &self,
        window: &EditorWindow,
        index: usize,
        px: u32,
        thumbnail: Result<Thumbnail, String>,
    ) {
        match thumbnail {
            Ok(thumbnail) => {
                // The reply reports the photo's own size, which is what the fitted
                // decode and the status bar's zoom are ratios against. Learning it
                // can *move* the size the pane wants — a photo focused before its
                // tile was decoded was asked for at the pane's long edge — so the
                // request is made again for the fitted size, once.
                let learned = (thumbnail.source_width, thumbnail.source_height);
                let first_time = self.sizes.borrow_mut().insert(index, learned).is_none();
                let picture = Rc::new(Picture::new(thumbnail));
                self.previews
                    .borrow_mut()
                    .insert((index, px), Rc::clone(&picture));
                if self.focused.get() == Some(index) {
                    if px == self.preview_px() {
                        let path = self.file(index).unwrap_or_default();
                        self.preview.set_paintable(Some(&picture.texture));
                        self.preview.set_tooltip_text(None);
                        self.preview_stack.set_visible_child_name("photo");
                        *self.shown.borrow_mut() = Some((path, picture, px));
                    } else if first_time {
                        self.ask_for_preview(window);
                    }
                    self.update_status_line();
                }
            }
            Err(reason) => {
                if self.focused.get() == Some(index) {
                    self.preview_stack.set_visible_child_name("empty");
                    self.preview.set_tooltip_text(Some(&reason));
                }
            }
        }
    }

    /// Paints the pane from what it has: the focused photo's preview at the size the
    /// pane draws it, or the state that says it is still coming.
    fn show_focused(&self) {
        let Some(index) = self.focused.get() else {
            self.preview_stack.set_visible_child_name("empty");
            *self.shown.borrow_mut() = None;
            return;
        };
        let px = self.preview_px();
        let picture = self.previews.borrow_mut().get(&(index, px));
        match picture {
            Some(picture) => {
                self.preview.set_paintable(Some(&picture.texture));
                self.preview.set_tooltip_text(None);
                self.preview_stack.set_visible_child_name("photo");
                let path = self.file(index).unwrap_or_default();
                *self.shown.borrow_mut() = Some((path, picture, px));
            }
            // Never the tile: the pane waits behind a spinner instead of showing a
            // 128 px picture stretched over it (`S13 · Ruling`).
            None => {
                self.preview_stack.set_visible_child_name("loading");
                *self.shown.borrow_mut() = None;
            }
        }
    }

    /// The photo the preview shows, if any.
    pub fn focused(&self) -> Option<usize> {
        self.focused.get()
    }

    /// A reply from the picture worker, routed by what it was asked for.
    pub fn on_reply(&self, window: &EditorWindow, reply: crate::thumbs::Reply) {
        // The entry going away is the reply's *own* — it is keyed by the folder it
        // was asked for under — so this happens before the answer is judged, and a
        // reply from the folder before this one cannot clear a live request's
        // entry or leave its own behind.
        match reply.kind {
            Kind::Tile => {
                self.inflight
                    .borrow_mut()
                    .remove(&(reply.epoch, reply.index, reply.px));
            }
            Kind::Preview => {
                self.preview_inflight
                    .borrow_mut()
                    .remove(&(reply.epoch, reply.index, reply.px));
            }
        }
        // An answer from the folder before this one answers a question that is not
        // being asked any more (see `open_folder`).
        if reply.epoch != self.epoch.get() {
            return;
        }
        match reply.kind {
            Kind::Tile => self.on_tile(reply.index, reply.px, reply.result),
            Kind::Preview => self.on_preview(window, reply.index, reply.px, reply.result),
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
    /// Selection is driven through the model, so the grid's own highlight, the
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

    /// Clears the pick (`Esc` leaves selection mode).
    ///
    /// The picked list is rebuilt, not just the numbers: since S13c a row is a
    /// control that switches the pane, and a row left behind by a cleared pick would
    /// be a control that lies. (S13b's version left them: it cleared the model and
    /// the ordered list and refreshed the button only.)
    pub fn clear_selection(&self, window: &EditorWindow) {
        self.syncing.set(true);
        self.multi.unselect_all();
        self.syncing.set(false);
        *self.selection.borrow_mut() = Selection::default();
        self.update_picked(window);
    }

    /// Drops one photo from the pick (a row's remove button).
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
        self.update_picked(window);
    }

    /// Moves the pick one place earlier (`delta = -1`) or later (`+1`), which is
    /// what `Ctrl+Up`/`Ctrl+Down` on a picked row does.
    pub fn move_photo(&self, window: &EditorWindow, index: usize, delta: isize) {
        let Some(target) = index.checked_add_signed(delta) else {
            return;
        };
        self.move_row(window, index, target);
    }

    /// Moves one entry of the pick to another position, which is what a row drag
    /// does.
    ///
    /// The entry is lifted out and put back at `to`, so "row 3 dropped on row 1"
    /// means what it says whatever the direction; `to` past the end appends. The
    /// row that moved keeps the focus, because the next `Ctrl+Up` acts on it.
    pub fn move_row(&self, window: &EditorWindow, from: usize, to: usize) {
        let mut photos = self.selection.borrow().photos().to_vec();
        if from >= photos.len() {
            return;
        }
        let photo = photos.remove(from);
        let to = to.min(photos.len());
        photos.insert(to, photo);
        let mut selection = Selection::default();
        for photo in photos {
            // A permutation of a legal selection: pushing cannot pass the cap, and
            // anything unexpected leaves the pick exactly as it was.
            if selection.push(photo).is_err() {
                return;
            }
        }
        *self.selection.borrow_mut() = selection;
        self.update_picked(window);
        if let Some(row) = self.list.row_at_index(to as i32) {
            row.grab_focus();
        }
    }

    /// Rebuilds the ordered list from the grid's selection, enforcing the cap.
    ///
    /// The rule that keeps the list *ordered*: a photo already in it keeps its
    /// place — so a re-order in the picked list survives a later click — and a
    /// newly selected one is appended in grid order.
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
        self.update_picked(window);
    }

    // ---- the document ------------------------------------------------------

    /// The document the pick makes, on the first layout that fits its count.
    ///
    /// This is the picker's handover to the layout stage, and the one place a
    /// pick becomes a document: `Selection::document` is the policy the CLI's
    /// `init --photo` also uses, so the picked list's order and the CLI's argument
    /// order cannot disagree. S14 replaces "the first layout" with the gallery.
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

    /// The folder's name, for the heading's subtitle (gthumb keeps the location in
    /// the header's centre widget too; S13b had it in a caption under the header
    /// *and* the pick's count in the subtitle, so the same fact lived in two
    /// places).
    fn folder_name(&self) -> Option<String> {
        let folder = self.folder.borrow().clone()?;
        Some(
            folder
                .file_name()
                .map(file_name_from)
                .unwrap_or_else(|| folder.display().to_string()),
        )
    }

    /// The count on the Next button, and the heading's subtitle.
    ///
    /// `HIG patterns/containers/selection-mode` asks the header to show how many
    /// items are selected, and that is the button's own label; the subtitle carries
    /// *where* the photos come from, which is the other half of the heading. HIG
    /// `patterns/containers/header-bars` wanted the heading in the centre, which is
    /// where `AdwWindowTitle` already is.
    fn update_next(&self) {
        let count = self.selected_count();
        // The label goes on the content object, not the button: `set_label` on the
        // button would replace the `AdwButtonContent` and with it the icon.
        self.next_content
            .set_label(&fill(gettext("Next ({})"), &[count]));
        self.next.set_sensitive(self.can_continue());
        self.next.set_tooltip_text(Some(&if self.can_continue() {
            gettext("Open the picked photos in a collage")
        } else {
            fill(gettext("Pick at least {} photos"), &[MIN_PHOTOS])
        }));
        self.title.set_subtitle(&match self.folder_name() {
            Some(name) => name,
            None => gettext("No folder chosen yet"),
        });
    }

    /// Writes the status bar's four fields, in gthumb's order (`S13c`).
    ///
    /// Every number is one the stage already knows: the pick's count is
    /// [`Selection`]'s and the folder's, the pixels and the size are the focused
    /// photo's (its size from the decode that reported it, its file size from one
    /// `stat`), and the zoom is `round(100 x drawn / photo long edge)`. Nothing here
    /// decodes anything, and a field with no photo behind it is empty rather than
    /// stale.
    fn update_status_line(&self) {
        self.status[STATUS_COUNT].set_label(&fill(
            gettext("{} / {}"),
            &[self.selected_count(), self.len()],
        ));
        self.status[STATUS_PIXELS].set_label(&match self.focused_size() {
            Some((width, height)) => fill(gettext("{} × {}"), &[width, height]),
            None => String::new(),
        });
        let bytes = self
            .focused
            .get()
            .and_then(|index| self.file(index))
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| metadata.len());
        self.status[STATUS_SIZE].set_label(&match bytes {
            // `GLib.format_size` is what gthumb's status line uses for this field,
            // so the two read the same way.
            Some(bytes) => glib::format_size(bytes).to_string(),
            None => String::new(),
        });
        self.status[STATUS_ZOOM].set_label(&match self.zoom_percent() {
            Some(percent) => fill(gettext("{}%"), &[percent]),
            None => String::new(),
        });
    }

    /// Rebuilds the picked list: one row per photo, in cell order.
    fn update_picked(&self, window: &EditorWindow) {
        let photos = self.selection.borrow().photos().to_vec();
        // The rows go one at a time rather than through `remove_all`. The list's
        // placeholder is a *child* of the box, and `GtkListBox::remove_all` takes
        // every child — the placeholder included — and then forgets it
        // (`gtklistbox.c`: `if (child == box->placeholder) box->placeholder = NULL`,
        // measured 2026-09-23: after the first rebuild the empty hint never came
        // back). `remove_all` had no such trap before S13c, because there was no
        // placeholder.
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        for (index, path) in photos.iter().enumerate() {
            self.list.append(&picked_row(window, index, path));
        }
        self.update_next();
        self.update_status_line();
    }

    // ---- the pane's own size, observed -------------------------------------

    /// Follows the pane's allocation: a window resize
    /// (`GdkSurface::layout`), a divider drag (`GtkPaned::position`) and a screen
    /// change (`GtkWidget::scale-factor`) are the three ways it moves.
    ///
    /// A divider drag does two things at once: it moves the pane the preview is
    /// decoded for, and it is a layout the user chose, so the position is kept for
    /// the session (`SPLITS`).
    fn follow_the_pane(&self, window: &EditorWindow) {
        let pane = self.preview_stack.clone();
        pane.connect_scale_factor_notify(glib::clone!(
            #[weak]
            window,
            move |_| {
                if let Some(picker) = window.picker() {
                    picker.refresh_scale(&window);
                }
            }
        ));
        self.columns.connect_position_notify(glib::clone!(
            #[weak]
            window,
            move |_| {
                let Some(picker) = window.picker() else {
                    return;
                };
                SPLITS.with(|splits| splits.set(Some(picker.split_position())));
                picker.refresh_pane(&window);
            }
        ));
        // A scroll, and the first moment the strip knows how wide its content is:
        // both are the *horizontal* adjustment's own signals — the strip reflows
        // along x, so that is the axis it scrolls on — and both change what is on
        // screen.
        let adjustment = self.strip.hadjustment();
        for signal in ["value-changed", "changed"] {
            adjustment.connect_local(
                signal,
                false,
                glib::clone!(
                    #[weak]
                    window,
                    #[upgrade_or]
                    None,
                    move |_| {
                        if let Some(picker) = window.picker() {
                            picker.refresh_visible(&window);
                        }
                        None
                    }
                ),
            );
        }
        // GTK4 has no `size-allocate` signal and no `width` property, so a window
        // resize is only visible where the surface itself reports its new size.
        self.root.connect_realize(glib::clone!(
            #[weak]
            window,
            move |root| {
                let Some(surface) = root.native().and_then(|native| native.surface()) else {
                    return;
                };
                surface.connect_layout(glib::clone!(
                    #[weak]
                    window,
                    move |_, _, _| {
                        if let Some(picker) = window.picker() {
                            picker.refresh_pane(&window);
                        }
                    }
                ));
            }
        ));
    }

    /// The picked list's three interactions: a row clicked (*switches the pane*,
    /// ruling 21), a row dragged onto a position, and `Ctrl+Up`/`Ctrl+Down` on the
    /// focused row.
    fn connect_picked_list(&self, window: &EditorWindow) {
        let list = self.list.clone();
        // A row's click selects it, and the selection is what switches the preview:
        // routing it through the list's own signal means the mouse and the keyboard
        // take one path (a `GtkListBox` in `Single` mode answers both).
        list.connect_row_selected(glib::clone!(
            #[weak]
            window,
            move |_, row| {
                let Some(row) = row else {
                    return;
                };
                let Some(picker) = window.picker() else {
                    return;
                };
                picker.focus_picked(&window, row.index() as usize);
            }
        ));

        let target = gtk::DropTarget::new(i32::static_type(), gdk::DragAction::MOVE);
        target.connect_drop(glib::clone!(
            #[weak]
            window,
            #[weak]
            list,
            #[upgrade_or]
            false,
            move |_, value, _, y| {
                let Some(from) = value.get::<i32>().ok().filter(|from| *from >= 0) else {
                    return false;
                };
                let Some(picker) = window.picker() else {
                    return false;
                };
                // Below the last row appends, which is what dropping past the end
                // means everywhere else.
                let to = list
                    .row_at_y(y as i32)
                    .map(|row| row.index() as usize)
                    .unwrap_or(picker.selected_count());
                picker.move_row(&window, from as usize, to);
                true
            }
        ));
        list.add_controller(target);

        // `Ctrl+Up`/`Ctrl+Down` rather than an application accelerator: the action
        // belongs to whichever row has focus, and a global binding would fire it in
        // the editor too. Neither combination is in HIG `reference/keyboard`'s
        // standard or reserved sets.
        let controller = gtk::ShortcutController::new();
        controller.set_scope(gtk::ShortcutScope::Local);
        for (trigger, delta) in [("<Control>Up", -1isize), ("<Control>Down", 1isize)] {
            let Some(trigger) = gtk::ShortcutTrigger::parse_string(trigger) else {
                continue;
            };
            let action = gtk::CallbackAction::new(glib::clone!(
                #[weak]
                window,
                #[weak]
                list,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, _| {
                    let Some(picker) = window.picker() else {
                        return glib::Propagation::Proceed;
                    };
                    let Some(index) = list
                        .focus_child()
                        .and_downcast::<gtk::ListBoxRow>()
                        .map(|row| row.index())
                    else {
                        return glib::Propagation::Proceed;
                    };
                    picker.move_photo(&window, index as usize, delta);
                    glib::Propagation::Stop
                }
            ));
            controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
        }
        list.add_controller(controller);
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

/// The divider's position for a window with no session value behind it: the media
/// area keeps everything but [`PICKED_LIST_WIDTH`].
///
/// The window's own default size is the reference — `GtkPaned`'s position is a
/// pixel count, and the pane is the paned's *start* child, so "the list is 260 px
/// wide" is `width - 260`. A window that has been used before starts where the user
/// left the divider instead ([`SPLITS`]); below the two children's minimums
/// `GtkPaned` clamps it, which is what keeps the list on screen in a narrow window.
fn split_position(window: &EditorWindow) -> i32 {
    if let Some(position) = SPLITS.with(Cell::get) {
        return position;
    }
    let width = window.default_size().0.max(0);
    (width - PICKED_LIST_WIDTH).max(PICKED_LIST_MIN_WIDTH)
}

/// The picker's primary menu — the one S13b's header never had.
///
/// The shape is the one both reference apps use and ruling 24 asks for: one menu
/// of sections, `[new / open] · [view options] · [shortcuts, about]` (gthumb's
/// hamburger is ten items in three sections, `data/ui/browser.ui:243-315`; loupe's
/// is five in four). The picker's own "view option" is the folder it lists.
fn picker_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let collage = gio::Menu::new();
    collage.append(Some(&gettext("New collage")), Some("app.new"));
    collage.append(Some(&gettext("Open…")), Some("app.open"));
    menu.append_section(None, &collage);

    let view = gio::Menu::new();
    view.append(Some(&gettext("Choose folder…")), Some("win.choose-folder"));
    menu.append_section(None, &view);

    let help = gio::Menu::new();
    help.append(Some(&gettext("Keyboard shortcuts")), Some("app.shortcuts"));
    help.append(Some(&gettext("About Pixlay")), Some("app.about"));
    menu.append_section(None, &help);

    menu
}

/// The long edge a photo is drawn at when `Contain`-fitted into a pane, in device
/// pixels.
///
/// The scale is `min(pane width / photo width, pane height / photo height)`, which
/// is what `GtkPicture`'s `Contain` does, and the answer is that scale against the
/// photo's longer edge. This is the number the status bar's zoom is a percentage
/// of, and the number the pane's decode is rounded up from — the pane always knows
/// the size it is *drawing* at.
///
/// Both sizes are at least one pixel, so an unallocated pane (a zero-size widget,
/// before its first layout) still divides by something and answers with something.
fn fitted_long_edge(pane: (u32, u32), photo: (u32, u32)) -> f64 {
    let (pane_width, pane_height) = (f64::from(pane.0.max(1)), f64::from(pane.1.max(1)));
    let (photo_width, photo_height) = (f64::from(photo.0.max(1)), f64::from(photo.1.max(1)));
    f64::min(pane_width / photo_width, pane_height / photo_height) * photo_width.max(photo_height)
}

/// The folder name of a path, for display.
fn file_name(path: &Path) -> String {
    path.file_name().map(file_name_from).unwrap_or_default()
}

fn file_name_from(name: &std::ffi::OsStr) -> String {
    name.to_string_lossy().into_owned()
}

/// One grid cell: the picture, the waiting state, and the refusal.
///
/// A `GtkStack` rather than a bare picture, because a cell has three states and
/// gthumb's `ThumbnailState` has the same four: not asked for yet, in flight
/// (**loading**), decoded (**photo**), or refused by the decoder (**failed**).
/// Its size is [`TILE_SIZE`] whatever the photo's aspect, so the strip does not
/// re-flow as it fills.
fn tile_widget() -> gtk::Stack {
    // A *static* loading icon rather than a `GtkSpinner`: GTK keeps many more cells
    // alive than it shows (measured 2026-09-22: 257 items for a 1000-photo folder in
    // a 536x396 viewport, a constant of its item manager; the strip's own bound is
    // `30 x max_columns` = 30), a spinner animates while it is mapped even when it
    // is clipped away, and the cells that are never asked for would spin for as long
    // as the folder is open.
    let waiting = gtk::Image::builder()
        .icon_name("image-loading-symbolic")
        .pixel_size(TILE_SIZE / 4)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .can_shrink(true)
        .width_request(TILE_SIZE)
        .height_request(TILE_SIZE)
        .build();
    let failed = gtk::Image::builder()
        .icon_name("image-missing-symbolic")
        .pixel_size(TILE_SIZE / 4)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    let stack = gtk::Stack::new();
    stack.add_css_class("picker-cell");
    stack.add_named(&waiting, Some("loading"));
    stack.add_named(&picture, Some("photo"));
    stack.add_named(&failed, Some("failed"));
    stack.set_visible_child_name("loading");
    stack
}

/// Shows a cell as picked, or not: the highlight is the only state a cell carries
/// (`style.css`, `.picker-cell.picked`).
fn highlight(cell: Option<&gtk::Widget>, selected: bool) {
    let Some(cell) = cell else {
        return;
    };
    if selected {
        cell.add_css_class("picked");
    } else {
        cell.remove_css_class("picked");
    }
}

/// Puts a decoded picture into a cell.
fn paint_tile(stack: &gtk::Stack, picture: &Rc<Picture>) {
    if let Some(picture_widget) = stack.child_by_name("photo").and_downcast::<gtk::Picture>() {
        picture_widget.set_paintable(Some(&picture.texture));
    }
    stack.set_visible_child_name("photo");
}

/// Sets a cell's tooltip, whatever it is currently showing.
fn tooltip(stack: &gtk::Stack, text: &str) {
    if let Some(picture) = stack.child_by_name("photo").and_downcast::<gtk::Picture>() {
        picture.set_tooltip_text(Some(text));
    }
    stack.set_tooltip_text(Some(text));
}

/// One row of the picked list: its place in the order, its file name, and the two
/// things a pick needs — a way to drop it, and a way to move it.
///
/// The remove button is a real GTK control at the row's right end (HIG
/// `guidelines/accessibility` names it, `guidelines/pointer-touch` requires a
/// keyboard path, which is `Ctrl+Up`/`Ctrl+Down` on the focused row); moving is the
/// row drag the list's own drop target answers.
fn picked_row(window: &EditorWindow, index: usize, path: &Path) -> adw::ActionRow {
    let name = file_name(path);
    let row = adw::ActionRow::builder()
        .title(&name)
        .activatable(false)
        .build();
    let position = gtk::Label::new(Some(&(index + 1).to_string()));
    position.add_css_class("dim-label");
    row.add_prefix(&position);

    let remove = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .has_frame(false)
        .valign(gtk::Align::Center)
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
    row.add_suffix(&remove);

    // The row carries its own place in the order, and the list is where it lands,
    // so a drag needs nothing but the two indices.
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    source.connect_prepare(move |_, _, _| {
        Some(gdk::ContentProvider::for_value(&(index as i32).to_value()))
    });
    row.add_controller(source);
    row
}

/// The folder the picker opens on: `XDG_PICTURES_DIR`, or the plain `~/Pictures`.
///
/// `glib::user_special_dir` is the XDG answer (it reads `user-dirs.dirs`); the
/// fallback covers an account whose XDG configuration does not name one, where the
/// directory usually exists anyway.
pub fn default_folder() -> Option<PathBuf> {
    if let Some(dir) = glib::user_special_dir(glib::UserDirectory::Pictures)
        && dir.is_dir()
    {
        return Some(dir);
    }
    let fallback = glib::home_dir().join("Pictures");
    fallback.is_dir().then_some(fallback)
}
