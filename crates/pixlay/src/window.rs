//! The window: one document, its actions, and everything that connects them.
//!
//! This is the only place in the GUI that knows about the document. The canvas
//! draws what it is asked to draw, the controls and the dialogs emit commands, and
//! both go through the methods here — which is also what makes the whole main path
//! reachable from a test without a pointer: `open_document`, `add_photos`,
//! `export_to` are the same calls the widgets make.
//!
//! **Since S22 the window opens on the editor** (ruling 31, 2026-09-25): there is no
//! picker stage and no navigation stack, so the shell is one page —
//!
//! ```text
//! AdwToastOverlay                      one place for every toast
//!  └ AdwToolbarView                    header / progress / banner + canvas + band
//! ```
//!
//! — and the photos enter from outside it: `Add photos…` (`Ctrl+I`), an empty cell's
//! own `+`, the selected cell's `Replace`, a drop from the file manager, `Open…` for a
//! `.pixlay` project, and `pixlay a.jpg b.jpg …` on the command line, which is the same
//! `add_photos` call in argument order (`app.rs`).
//!
//! Two rules the whole file obeys, both from `AGENTS.md`: a GTK object never
//! leaves the main thread (the decoding and encoding threads send plain data back
//! through `MainContext::invoke`), and nothing here touches a pixel — the canvas
//! hands the document to the renderer and the export hands it to
//! `pixlay-imaging`.
//!
//! The window's own title is the document's name, dirty marker included, and the
//! header bar carries the same string as its centre widget.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt as _;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{
    CollageDoc, Command, CoreError, CropTransform, Frame, MAX_PHOTOS, MIN_PHOTOS, PixelSize,
    Project, Template, templates,
};
use pixlay_imaging::gesture_grid;
use pixlay_render::Images;

use crate::a11y;
use crate::canvas::{self, Gesture};
use crate::decode::{Decoder, GalleryReply, Reply};
use crate::dialogs;
use crate::export::{self, Progress, Report, Settings};
use crate::i18n::{fill, gettext, ngettext};
use crate::layout::Gallery;
use crate::state::Editor;
use crate::workers::{Down, Kind as WorkerKind, Workers};

/// What happens once a boundary's question has been answered, or a save has a path:
/// the two continuations the window's boundaries hand each other (S15d).
///
/// `Boundary` is "the thing the caller was about to do" — replace the document, or
/// close the window — and `Saved` is the same thing once a file has been written to.
type Boundary = Rc<dyn Fn(&EditorWindow)>;
type Saved = Rc<dyn Fn(&EditorWindow, &Path)>;

/// The template a new document starts from: the sheet itself, one cell (S19).
///
/// Ruling 34 made one photo a legal collage, so a new document opens on the layout
/// of one photo — 4:3, the album page `mosaic-5-hero` was authored for — and the
/// cell's own `+` asks for the photo that goes in it. Growing is the count
/// control's job, and `layout_for` picks the layout each count lands on.
pub const DEFAULT_TEMPLATE: &str = "grid-1-1x1";

/// The long edge a new export form starts at, in pixels: the one size
/// parameter (S12d), shared with the CLI's own default (`render --long-edge`,
/// `docs/CONTRACT.md` §5).
///
/// 4000 px: a square grid of it is 16 MP, an eighth of the 200 MP pixel budget
/// (measured 2026-09-20, `docs/CONTRACT.md` §8), so the default never touches the
/// limit whatever the template's shape. The form has to be seeded with it: a
/// `GtkSpinButton` starts at its adjustment's *lower* bound, so without this a
/// new window would export at the row's minimum.
///
/// The row's own bounds live beside the form's state (`MIN_EXPORT_PX` /
/// `MAX_EXPORT_PX` in `crate::export`): the maximum is 12000 because
/// `12000² = 144 MP < 200 MP`, so every template aspect stays inside the budget.
pub const DEFAULT_EXPORT_PX: u32 = 4000;

/// The grid the window holds before its first bitmaps are in hand.
///
/// One texel, and never a grid a render could use: `placement` stretches it over the
/// whole widget, so `CellControls::sync_in` places nothing against it and waits for the
/// reply that installs the real grid (`EditorWindow::on_decoded`).
pub const PLACEHOLDER_GRID: PixelSize = PixelSize {
    width: 1,
    height: 1,
};

/// How long a test-facing wait pumps for work to *start* before concluding there is
/// none (`EditorWindow::pump_until`).
///
/// Long enough to cover the frames a window needs to lay itself out and draw, short
/// enough that a wait about a document no edit is pending on costs a fraction of a
/// second.
const WORK_GRACE: Duration = Duration::from_millis(1000);

/// How long a live gesture waits for quiet before it becomes an undo step.
///
/// A slider has no "drag ended" signal, so the commit is triggered by the value
/// being still: long enough that a slow drag does not produce three commands,
/// short enough that the undo a user reaches for next is the gesture they just
/// finished.
pub const COMMIT_QUIET: Duration = Duration::from_millis(250);

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;
    use libadwaita::subclass::prelude::*;

    pub struct EditorWindow {
        pub editor: RefCell<Editor>,
        /// The bitmaps and the grid they were decoded for.
        pub images: RefCell<(PixelSize, Images)>,
        /// The grid a decode is in flight for, so a resize does not queue one
        /// request per frame.
        pub requested: Cell<Option<PixelSize>>,
        pub decoder: RefCell<Option<Decoder>>,
        pub generation: Cell<u64>,
        /// Files the decoding thread has decoded since the window opened; the
        /// tests hold the gesture path to it (S12).
        pub decoded: Cell<u64>,
        pub selection: Cell<Option<usize>>,
        pub guides: Cell<bool>,
        pub canvas: OnceCell<gtk::DrawingArea>,
        /// The `+` buttons over the empty cells and the selected cell's own strip
        /// (S14b, S15), stacked over the canvas.
        pub cell_controls: OnceCell<Rc<canvas::CellControls>>,
        /// The header bar and the title widget in it: one page, so the bar is the
        /// window's (`update_title` writes the document's name here).
        pub header: OnceCell<adw::HeaderBar>,
        pub title: OnceCell<adw::WindowTitle>,
        /// The two document-level dialogs (S15): `Frame…` and `Export…`, built once
        /// and presented by the header bar's buttons.
        pub frame_dialog: OnceCell<Rc<dialogs::FrameDialog>>,
        pub export_dialog: OnceCell<Rc<dialogs::ExportDialog>>,
        /// How this window's two workers start (S15h, PIX-014): the product's own
        /// plan unless a test named another one.
        pub workers: Cell<Workers>,
        /// Whether the decoding worker's failure has already been reported: the
        /// toast is news once, and the edit that asks again is not a second failure.
        pub decode_reported: Cell<bool>,
        /// The canvas's current accessible name.
        ///
        /// A copy for the tests, because GTK4 has no getter for an accessible
        /// property's *value* (`gtk::test_accessible_has_property` answers only
        /// whether one is set): "the name says which cell the focus is on" (S15h,
        /// PIX-017) is a claim about a string, so the string is kept where a test can
        /// read it, exactly as `last_toast` is.
        pub canvas_label: RefCell<String>,
        /// The layout gallery: the band under the canvas, and the count control
        /// that decides what the candidates are (S14).
        pub gallery: OnceCell<Rc<Gallery>>,
        /// The generation of the gallery build in flight, so a reply for a
        /// document that has moved on is ignored.
        pub gallery_generation: Cell<u64>,
        /// Whether a gallery build is outstanding, which is what the tests wait on.
        pub gallery_pending: Cell<bool>,
        /// Builds of the band that have landed and were accepted (S15).
        ///
        /// The tests' handle on "the band was built": the flag above says whether a
        /// build is *outstanding*, and a wait that only reads it can return before the
        /// request was ever made — see `pump_until`.
        pub gallery_builds: Cell<u64>,
        pub banner: OnceCell<adw::Banner>,
        pub toast: OnceCell<adw::ToastOverlay>,
        pub progress: OnceCell<gtk::ProgressBar>,
        pub progress_revealer: OnceCell<gtk::Revealer>,
        pub commit_timer: RefCell<Option<glib::SourceId>>,
        /// The export form's state, since ruling 18 removed the pane that held
        /// it: the format, the one quality option (a long edge in pixels, S12d)
        /// and the chosen path. S15's `Export…` dialog is the rows over this.
        pub export: RefCell<Settings>,
        pub exporting: Cell<bool>,
        pub missing: RefCell<Vec<usize>>,
        /// The last message a toast carried, for the tests: a refusal the user is
        /// told about is a claim this layer can be held to.
        pub last_toast: RefCell<Option<String>>,
        /// How many toasts this window has shown, for the tests: "the cap is
        /// reported *once* per refused pick" is a claim about a count, and the last
        /// message cannot tell one report from two (`S13c`).
        pub toasts: Cell<u64>,
        pub actions: RefCell<Vec<gio::SimpleAction>>,
        /// What the canvas's draw function last refused, if anything: the other
        /// failure mode a test's wait has to be able to name, where frames do arrive
        /// and the canvas paints nothing.
        pub last_draw_error: RefCell<Option<String>>,
        /// The cell a swap is coming from, if one is marked (S23, ruling 33).
        ///
        /// One mark serves all three paths: the strip's swap control sets it, a
        /// `Shift`+click's press sets it, and a `Shift`+drag sets it when the drag
        /// begins. The canvas draws it as a dashed outline, the strip's toggle mirrors
        /// it, and the mark is spent when a swap is applied or cancelled.
        pub swap: Cell<Option<usize>>,
        /// The cell under the pointer while a swap drag is in flight (S23).
        ///
        /// The drop's highlight, and the cell the release exchanges with. `None` on the
        /// keyboard path, whose target is the selection and carries the selection
        /// outline already.
        pub swap_target: Cell<Option<usize>>,
    }

    impl Default for EditorWindow {
        fn default() -> Self {
            Self {
                editor: RefCell::new(
                    Editor::new(default_document()).expect("the default document is a valid one"),
                ),
                images: RefCell::new((
                    PixelSize {
                        width: 1,
                        height: 1,
                    },
                    Images::new(),
                )),
                requested: Cell::new(None),
                decoder: RefCell::new(None),
                generation: Cell::new(0),
                decoded: Cell::new(0),
                selection: Cell::new(None),
                guides: Cell::new(false),
                canvas: OnceCell::new(),
                cell_controls: OnceCell::new(),
                header: OnceCell::new(),
                title: OnceCell::new(),
                frame_dialog: OnceCell::new(),
                export_dialog: OnceCell::new(),
                workers: Cell::new(Workers::default()),
                decode_reported: Cell::new(false),
                canvas_label: RefCell::new(String::new()),
                gallery: OnceCell::new(),
                gallery_generation: Cell::new(0),
                gallery_pending: Cell::new(false),
                gallery_builds: Cell::new(0),
                banner: OnceCell::new(),
                toast: OnceCell::new(),
                progress: OnceCell::new(),
                progress_revealer: OnceCell::new(),
                commit_timer: RefCell::new(None),
                export: RefCell::new(Settings {
                    long_edge: DEFAULT_EXPORT_PX,
                    format: pixlay_imaging::encode::Format::Jpeg,
                    path: PathBuf::new(),
                }),
                exporting: Cell::new(false),
                missing: RefCell::new(Vec::new()),
                last_toast: RefCell::new(None),
                toasts: Cell::new(0),
                actions: RefCell::new(Vec::new()),
                last_draw_error: RefCell::new(None),
                swap: Cell::new(None),
                swap_target: Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EditorWindow {
        const NAME: &'static str = "PixlayEditorWindow";
        type Type = super::EditorWindow;
        type ParentType = adw::ApplicationWindow;
    }

    impl ObjectImpl for EditorWindow {
        fn constructed(&self) {
            self.parent_constructed();
            // SAFETY of the cast: the object this imp belongs to is the window.
            let window = self.obj();
            window.build();
        }
    }

    impl WidgetImpl for EditorWindow {}
    impl WindowImpl for EditorWindow {}
    impl ApplicationWindowImpl for EditorWindow {}
    impl AdwApplicationWindowImpl for EditorWindow {}
}

glib::wrapper! {
    pub struct EditorWindow(ObjectSubclass<imp::EditorWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager, gio::ActionGroup, gio::ActionMap;
}

/// The document a window starts on.
pub fn default_document() -> CollageDoc {
    let template = templates::get(DEFAULT_TEMPLATE).expect("the default template is registered");
    CollageDoc::new(template)
}

impl EditorWindow {
    pub fn new(app: &adw::Application) -> Self {
        let window: Self = glib::Object::builder().property("application", app).build();
        window.start_workers(Workers::default());
        window
    }

    /// The window a test builds when it wants one of the workers to be down
    /// (S15h, PIX-014): the same window, with the three plans `workers` names.
    ///
    /// The product's own construction is [`new`](Self::new), which is this call with
    /// every plan `WorkerPlan::Run`. The workers are not started in `build()`, so a
    /// caller can choose the plan after the widgets exist and before the window is
    /// presented — which is the moment the first request would be made.
    pub fn with_workers(app: &adw::Application, workers: Workers) -> Self {
        let window: Self = glib::Object::builder().property("application", app).build();
        window.start_workers(workers);
        window
    }

    /// Starts the workers the window holds for its lifetime, under `workers`, and
    /// remembers the plan the other one (an export's own thread) starts under.
    ///
    /// A start that fails is not a panic (S15h, PIX-014): the handle stays `None`,
    /// and the request path that needed it reports the reason where the user can see
    /// it.
    fn start_workers(&self, workers: Workers) {
        let imp = self.imp();
        imp.workers.set(workers);

        // ---- decoding ------------------------------------------------------
        let sender = glib::SendWeakRef::from(self.downgrade());
        let decoder = Decoder::spawn_with(
            move |event| {
                let sender = sender.clone();
                // The reply is plain data; the window is reached on its own thread.
                glib::MainContext::default().invoke(move || {
                    if let Some(window) = sender.upgrade() {
                        window.on_decoder_event(event);
                    }
                });
            },
            workers.decode,
        );
        *imp.decoder.borrow_mut() = decoder.ok();
    }

    fn build(&self) {
        let imp = self.imp();

        // The minimum the layout is designed for (HIG `guidelines/adaptive`): the
        // canvas needs its own space, and below this the window would be showing a
        // strip of sheet.
        self.set_default_size(1100, 760);
        self.set_size_request(560, 420);

        // ---- the header bar -------------------------------------------------
        // One page, so this is the window's own bar: history and the document's
        // settings, the heading, the menu and the export. The title widget is the
        // document's name, dirty marker included (`update_title`); since S22 there is
        // **no Save button** — it sat beside Export and read as the same action
        // (ruling 37) — and the function lives in the menu and on `Ctrl+S`.
        let undo = icon_button("edit-undo-symbolic", &gettext("Undo"));
        undo.set_action_name(Some("win.undo"));
        let redo = icon_button("edit-redo-symbolic", &gettext("Redo"));
        redo.set_action_name(Some("win.redo"));
        // Icon plus label: `AdwButtonContent` is how libadwaita puts both in one
        // button (setting `label` and `icon-name` together keeps only the icon).
        let export_content = adw::ButtonContent::new();
        export_content.set_icon_name("document-save-as-symbolic");
        export_content.set_label(&gettext("Export"));
        let export = gtk::Button::builder()
            .child(&export_content)
            .tooltip_text(gettext("Export the collage"))
            .build();
        export.add_css_class("suggested-action");
        export.set_action_name(Some("win.export"));
        a11y::label(&export, &gettext("Export the collage"));
        // The frame's own settings (S15): a document-level question, so it is a
        // dialog behind a button rather than a permanent row (ruling 18). Icon only,
        // with a tooltip and a name, which is what a header bar holds.
        let frame = icon_button(
            "document-properties-symbolic",
            &gettext("Frame the collage"),
        );
        frame.set_action_name(Some("win.frame"));
        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(gettext("Main menu"))
            .primary(true)
            .menu_model(&main_menu())
            .build();
        a11y::label(&menu, &gettext("Main menu"));

        // HIG `patterns/containers/header-bars`: navigation actions at the *start*,
        // the heading in the centre, the menu at the *end* (S13c; S13b packed every
        // control at the end), and related buttons grouped with a spacer rather than
        // linked. The start slot holds the document-editing controls: undo and redo
        // are one pair, and the frame's own settings are a second concern.
        let spacer = gtk::Separator::new(gtk::Orientation::Vertical);
        spacer.add_css_class("spacer");
        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new(&gettext("Untitled collage"), "");
        header.set_title_widget(Some(&title));
        header.pack_start(&undo);
        header.pack_start(&redo);
        header.pack_start(&spacer);
        header.pack_start(&frame);
        header.pack_end(&menu);
        header.pack_end(&export);

        // ---- progress ------------------------------------------------------
        let progress = gtk::ProgressBar::builder()
            .show_text(true)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();
        progress.update_property(&[gtk::accessible::Property::Label(&gettext(
            "Export progress",
        ))]);
        let progress_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .child(&progress)
            .build();

        // ---- the content -----------------------------------------------------
        // The canvas is wrapped in an overlay (S14b, S15): the empty cells' `+` and
        // the selected cell's own strip are real GTK controls over it (ruling 9), and
        // only the controls claim a press — the canvas keeps every drag and click
        // that is not on one.
        let controls = Rc::new(canvas::CellControls::new(self, &canvas::build(self)));
        let canvas = controls.canvas();
        // The layout band sits under the canvas (S14): the candidates are the
        // editor's own document with another template, and S15's compose controls
        // attach to the canvas above them, so the two are one page.
        let gallery = Gallery::build(self);
        // The banner's action is named once, here: its button exists from the
        // start, and a control with no label is a control a screen reader cannot
        // announce (`docs/HIG-REVIEW.md`, section 1).
        let banner = adw::Banner::new("");
        banner.set_button_label(Some(&gettext("Find it…")));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&banner);
        body.append(&controls.root());
        body.append(&gallery.root());
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.add_bottom_bar(&progress_revealer);
        view.set_content(Some(&body));
        // Content starts below the bar, so the bar is raised rather than flat (S13c,
        // from loupe's `src/widgets/image_window.rs:986-998`).
        view.set_top_bar_style(adw::ToolbarStyle::Raised);

        // **The canvas's size is read where the surface reports it, not only from a
        // draw.** GTK4 has no `size-allocate` and no `width` property, so a window
        // resize is visible only at the surface (`GdkSurface::layout`) — the hook the
        // picker's own pane used (S13c; the pane is gone, the hook is what the canvas
        // still needs): a window is allocated before it is painted, and a window
        // resized while it is not being painted must still decode for its new size.
        // The canvas's own draw asks for its grid too, and this is the half that does
        // not depend on a paint.
        self.connect_realize(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                let Some(surface) = window.surface() else {
                    return;
                };
                surface.connect_layout(glib::clone!(
                    #[weak]
                    window,
                    move |_, _, _| {
                        let area = window.canvas_widget();
                        window.request_grid_for(area.width(), area.height());
                        // The controls over the canvas are placed from the same
                        // allocation, so they follow the layout too.
                        window.refresh();
                    }
                ));
            }
        ));

        // ---- the shell -------------------------------------------------------
        // One page (S22): the toast overlay is the window's content and the toolbar
        // view is its only child, so every toast — a save, an export, a refused
        // decode — has one surface and the header bar belongs to the window itself.
        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&view));

        self.set_content(Some(&toast));
        self.set_title(Some(&gettext("Untitled collage")));

        imp.canvas.set(canvas).ok();
        imp.cell_controls.set(controls).ok();
        imp.header.set(header).ok();
        imp.title.set(title).ok();
        imp.frame_dialog.set(dialogs::FrameDialog::build(self)).ok();
        imp.export_dialog
            .set(dialogs::ExportDialog::build(self))
            .ok();
        imp.banner.set(banner.clone()).ok();
        imp.toast.set(toast).ok();
        imp.progress.set(progress).ok();
        imp.progress_revealer.set(progress_revealer).ok();
        imp.gallery.set(gallery).ok();

        let banner_weak = self.downgrade();
        banner.connect_button_clicked(move |_banner| {
            let Some(window) = banner_weak.upgrade() else {
                return;
            };
            if let Some(slot) = window.imp().missing.borrow().first().copied() {
                window.select(Some(slot));
                window.choose_photo(slot);
            }
        });

        self.install_actions();

        // ---- unsaved work ---------------------------------------------------
        // **One question, every boundary** (PIX-002, 2026-09-24): closing the
        // window, `New` and `Open` all replace or end the document, so all three
        // ask the same question — and each of them commits the pending edit first,
        // so what the question is about is what is on screen.
        let close_weak = self.downgrade();
        self.connect_close_request(glib::clone!(
            #[strong]
            close_weak,
            move |_window| {
                let Some(window) = close_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                window.commit();
                if window.ask_to_save(Rc::new(|window| window.destroy())) {
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        ));

        self.refresh();
    }

    /// The window actions and their sensitivity.
    ///
    /// The accelerators are set on the application (`app::ACCELERATORS`) so that
    /// `Ctrl+W` and `Ctrl+Q` mean the same thing in every window, and so that the
    /// table the shortcuts dialog shows is the table that is bound.
    fn install_actions(&self) {
        let group = gio::SimpleActionGroup::new();
        let mut actions = Vec::new();

        let mut add = |name: &str, enabled: bool, run: Box<dyn Fn(&EditorWindow)>| {
            let action = gio::SimpleAction::new(name, None);
            action.set_enabled(enabled);
            let window = self.downgrade();
            action.connect_activate(move |_, _| {
                if let Some(window) = window.upgrade() {
                    run(&window);
                }
            });
            group.add_action(&action);
            actions.push(action);
        };

        add(
            "undo",
            false,
            Box::new(|window| {
                window.undo();
            }),
        );
        add(
            "redo",
            false,
            Box::new(|window| {
                window.redo();
            }),
        );
        add(
            "save",
            true,
            Box::new(|window| {
                window.save();
            }),
        );
        add(
            "save-as",
            true,
            Box::new(|window| {
                window.save_as();
            }),
        );
        add(
            "close",
            true,
            Box::new(|window| {
                window.close();
            }),
        );
        add(
            "export",
            true,
            Box::new(|window| {
                window.export();
            }),
        );
        add(
            "frame",
            true,
            Box::new(|window| {
                window.frame();
            }),
        );
        // The window's own way in for photos (S22, ruling 31): the multi-file
        // chooser appends what it is given and lets the layout grow
        // (`EditorWindow::choose_photos`). The per-cell paths are the canvas's own
        // controls — an empty cell's `+`, the strip's `Replace`, `Return` — which is
        // why there is no second, selection-scoped menu item beside this one.
        add(
            "add-photos",
            true,
            Box::new(|window| {
                window.choose_photos();
            }),
        );
        add(
            "clear-cell",
            false,
            Box::new(|window| {
                if let Some(slot) = window.selection() {
                    window.clear_cell(slot);
                }
            }),
        );
        add(
            "reset-framing",
            false,
            Box::new(|window| {
                if let Some(slot) = window.selection() {
                    window.reset_framing(slot);
                }
            }),
        );

        self.insert_action_group("win", Some(&group));
        *self.imp().actions.borrow_mut() = actions;
    }

    // ---- what the canvas asks for -----------------------------------------

    pub fn canvas_widget(&self) -> gtk::DrawingArea {
        self.imp().canvas.get().expect("the canvas exists").clone()
    }

    /// The controls over the canvas (S14b, S15): the empty cells' `+` and the
    /// selected cell's own strip, for the widget tree and the tests.
    pub fn cell_controls(&self) -> Option<Rc<canvas::CellControls>> {
        self.imp().cell_controls.get().cloned()
    }

    /// The window's header bar, for the HIG checks.
    pub fn header(&self) -> Option<adw::HeaderBar> {
        self.imp().header.get().cloned()
    }

    /// Whether the window has an action by this name, and whether it is enabled:
    /// `None` is an action this window does not install.
    ///
    /// The tests' handle on the action table, which `update_actions` rewrites on every
    /// refresh — a widget carrying an action can be walked, but a menu item exists only
    /// inside the popover's model, so "the window can do this at all" has to be a
    /// question about the table rather than about the tree.
    pub fn action_enabled(&self, name: &str) -> Option<bool> {
        self.imp()
            .actions
            .borrow()
            .iter()
            .find(|action| action.name() == name)
            .map(|action| action.is_enabled())
    }

    /// The `Frame…` dialog (S15).
    pub fn frame_dialog(&self) -> Option<Rc<dialogs::FrameDialog>> {
        self.imp().frame_dialog.get().cloned()
    }

    /// The `Export…` dialog (S15).
    pub fn export_dialog(&self) -> Option<Rc<dialogs::ExportDialog>> {
        self.imp().export_dialog.get().cloned()
    }

    /// Opens `paths` the way the command line does (S22): the photos go into the
    /// document in argument order, through the same `Command::AddPhotos` the
    /// `Add photos…` chooser sends, so a list longer than the ceiling is trimmed once
    /// with one report and the whole arrival is one undo step.
    ///
    /// `app.rs`'s `open` handler is the caller: `pixlay a.jpg b.jpg …` reaches this
    /// with the shell's own arguments, in order.
    pub fn open_paths(&self, paths: Vec<PathBuf>) {
        self.add_photos(paths);
    }

    /// Opens `doc`: what loading a project and the tests' own setup do.
    ///
    /// The title is not written here: `refresh_document` runs `update_title`, which
    /// derives the window's name and the header's from the editor, and one writer is
    /// what keeps them equal.
    pub fn open_document(&self, doc: CollageDoc) {
        match Editor::new(doc) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                // A marked swap belongs to the document it was marked in (S23): the new
                // one has its own cells, and the mark's slot may not even exist in them.
                self.imp().swap.set(None);
                self.imp().swap_target.set(None);
                self.select(None);
                self.requested_grid_reset();
                self.refresh_document();
            }
            Err(error) => self.toast(&error.to_string()),
        }
    }

    pub fn display_document(&self) -> CollageDoc {
        self.imp().editor.borrow().display_doc()
    }

    pub fn document(&self) -> CollageDoc {
        self.imp().editor.borrow().doc().clone()
    }

    pub fn images(&self) -> (PixelSize, Images) {
        self.imp().images.borrow().clone()
    }

    pub fn selection(&self) -> Option<usize> {
        self.imp().selection.get()
    }

    /// The grid a decode is in flight for, if one is.
    ///
    /// The tests' handle on which grid the canvas is asking for — the coarse one
    /// while a gesture is live, the resting one otherwise. A reply is built for the
    /// grid it was requested at, so this is the grid the next bitmaps will be.
    pub fn requested_grid(&self) -> Option<PixelSize> {
        self.imp().requested.get()
    }

    /// What the canvas's draw function last refused, if it refused anything.
    ///
    /// Set by the canvas and read by the tests: a canvas that is mapped, visible and
    /// allocated but snapshots to nothing is usually a `render` that failed, and this
    /// is the reason.
    pub fn last_draw_error(&self) -> Option<String> {
        self.imp().last_draw_error.borrow().clone()
    }

    /// Records (or clears) the canvas's last draw refusal. The canvas's draw function
    /// is the only writer; everything else reads [`last_draw_error`](Self::last_draw_error).
    pub fn set_last_draw_error(&self, error: Option<String>) {
        *self.imp().last_draw_error.borrow_mut() = error;
    }

    /// Whether an export is in flight.
    ///
    /// The tests' handle on the state the progress bar shows and the flag every wait
    /// reads — a start that could not happen has to leave it false (S15h, PIX-014).
    pub fn exporting(&self) -> bool {
        self.imp().exporting.get()
    }

    /// Files the decoding thread has decoded since the window opened.
    ///
    /// The tests' handle on S12's central claim — a live gesture never touches the
    /// disk — and nothing else reads it: the count is a fact about the worker, and
    /// the window itself has no use for it. A superseded build's decodes count too,
    /// because the disk was touched all the same.
    pub fn decoded_sources(&self) -> u64 {
        self.imp().decoded.get()
    }

    /// Layout-band builds that have landed since the window opened.
    ///
    /// The tests' handle on the *second* half of a layout change (S18): the band's
    /// rebuild goes to the same worker as the canvas's and arrives after it, so a wait
    /// that has to be a condition on the reply — rather than a pump that may cost a
    /// second of grace — reads this. Nothing else reads it.
    pub fn gallery_builds(&self) -> u64 {
        self.imp().gallery_builds.get()
    }

    pub fn guides(&self) -> bool {
        self.imp().guides.get()
    }

    /// The sheet's size in device pixels, as the canvas drew it.
    pub fn sheet_size(&self) -> (f64, f64) {
        let (grid, _) = self.images();
        let area = self.canvas_widget();
        let placement = canvas::placement(grid, area.width(), area.height());
        (placement.width(), placement.height())
    }

    pub fn slot_extent(&self, slot: usize) -> Option<(f64, f64)> {
        let doc = self.display_document();
        let bbox = doc.template.slots.get(slot)?.outline.bbox();
        Some((bbox.width(), bbox.height()))
    }

    pub fn slot_at_widget(&self, x: f64, y: f64) -> Option<usize> {
        let (grid, _) = self.images();
        let area = self.canvas_widget();
        let placement = canvas::placement(grid, area.width(), area.height());
        canvas::slot_at(&self.display_document(), &placement, x, y)
    }

    /// The crop a slot is currently *shown* with: the stored request, fitted.
    ///
    /// Every gesture starts from this rather than from the request, because the
    /// fit is what the user is looking at: dragging a photo by ten pixels has to
    /// move it by ten pixels whatever zoom the document happens to ask for.
    pub fn fitted_crop(&self, slot: usize) -> Option<CropTransform> {
        let doc = self.document();
        let (grid, images) = self.images();
        let aspect = images.get(slot).map(|bitmap| bitmap.aspect())?;
        // The document's own fit, so the frame's gap (S11) is part of what the user
        // is looking at: a gesture starts from the picture on screen, not from a
        // request the canvas is not drawing.
        doc.fitted_crop(slot, grid.aspect(), aspect)
            .ok()
            .map(|fit| fit.transform)
    }

    // ---- edits ------------------------------------------------------------

    /// Applies one command as one undo step.
    pub fn apply(&self, command: Command) -> Result<(), CoreError> {
        let result = self.imp().editor.borrow_mut().apply(command);
        match &result {
            Ok(()) => self.refresh_document(),
            Err(error) => self.toast(&error.to_string()),
        }
        result
    }

    /// One step of a live gesture: the canvas shows it, the history does not have
    /// it yet.
    pub fn gesture(&self, gesture: Gesture) {
        match gesture {
            Gesture::Crop { slot, crop } => {
                let fitted = self.fit_for(slot, crop);
                let rotation_changed = self
                    .fitted_crop(slot)
                    .is_some_and(|current| current.rotation_deg != fitted.rotation_deg);
                self.imp().guides.set(rotation_changed);
                // A refused step is not a gesture in flight: `guides` goes back with
                // it, and no commit is scheduled for a command nobody took.
                if self.live(Command::SetCrop { slot, crop: fitted }).is_err() {
                    self.imp().guides.set(false);
                    return;
                }
                // A slider cannot say "the drag ended"; the commit happens once
                // the value stops moving. A wheel or a drag ends the same way,
                // which is why they do not need a signal of their own either.
                self.schedule_commit();
            }
            // A step that arrives finished: the keyboard, the zoom spin row. It is
            // one frame the user is meant to look at, so it is committed at once
            // and never coarsened (S12).
            Gesture::Step { slot, crop } => self.gesture_step(slot, crop),
            Gesture::End => self.commit(),
        }
    }

    /// One edit that arrives already finished: fitted, applied, committed, drawn
    /// at the resting grid.
    fn gesture_step(&self, slot: usize, crop: CropTransform) {
        let fitted = self.fit_for(slot, crop);
        if self
            .imp()
            .editor
            .borrow_mut()
            .begin(Command::SetCrop { slot, crop: fitted })
            .is_ok()
        {
            self.commit();
        } else {
            self.canvas_widget().queue_draw();
        }
    }

    fn fit_for(&self, slot: usize, crop: CropTransform) -> CropTransform {
        let doc = self.document();
        let (grid, images) = self.images();
        let Some(aspect) = images.get(slot).map(|bitmap| bitmap.aspect()) else {
            return doc.cells.get(slot).map(|cell| cell.crop).unwrap_or(crop);
        };
        // The same reference the renderer clamps against, for the gesture's own
        // numbers: the canvas and the gesture cannot disagree about what covers the
        // cell, frame included.
        doc.fit_crop(slot, crop, grid.aspect(), aspect)
            .map(|fit| fit.transform)
            .unwrap_or(crop)
    }

    /// One step of a live gesture, or a refusal to report (S15h, PIX-020).
    ///
    /// `Err` is the document refusing the command — a frame that empties a cell, a
    /// crop no fitting can cover — and it is the caller's to report: the pending
    /// command is untouched, so nothing is drawn and nothing is scheduled.
    fn live(&self, command: Command) -> Result<(), CoreError> {
        self.imp().editor.borrow_mut().begin(command)?;
        self.canvas_widget().queue_draw();
        // The gesture is live, so this is the grid a gesture draws at: the
        // document is moving, and a frame that keeps up is worth more than a
        // sharp one. The release refines it (S12).
        let grid = self.gesture_aware_grid(self.resting_grid());
        self.request_decode(grid);
        Ok(())
    }

    /// Commits the gesture in flight as one undo step.
    pub fn commit(&self) {
        if let Some(timer) = self.imp().commit_timer.borrow_mut().take() {
            timer.remove();
        }
        self.imp().guides.set(false);
        self.imp().editor.borrow_mut().commit();
        // Committed or not, the canvas goes back to the resting grid: a gesture
        // that ended where it started still drew *coarse* frames, and a refused
        // command must not leave those on screen. The build is a re-use when the
        // command was refused (`Editor::commit` refuses one that changes nothing),
        // and the refinement itself when it was not.
        self.refresh_document();
    }

    fn schedule_commit(&self) {
        let imp = self.imp();
        if let Some(timer) = imp.commit_timer.borrow_mut().take() {
            timer.remove();
        }
        let window = self.downgrade();
        let id = glib::timeout_add_local_once(COMMIT_QUIET, move || {
            if let Some(window) = window.upgrade() {
                window.commit();
            }
        });
        *imp.commit_timer.borrow_mut() = Some(id);
    }

    /// Undo and redo: the command history is one gesture per step (S6.5).
    pub fn undo(&self) {
        if self.imp().editor.borrow_mut().undo() {
            self.refresh_document();
        }
    }

    pub fn redo(&self) {
        if self.imp().editor.borrow_mut().redo() {
            self.refresh_document();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.imp().editor.borrow().can_undo()
    }

    /// How many undoable commands the document's history holds.
    ///
    /// The tests' handle on "one interaction is one undo step" (S23): a swap drag commits
    /// once, on the release, and a *count* is what can tell one step from two.
    pub fn undo_depth(&self) -> usize {
        self.imp().editor.borrow().undo_depth()
    }

    pub fn can_redo(&self) -> bool {
        self.imp().editor.borrow().can_redo()
    }

    pub fn select(&self, slot: Option<usize>) {
        let slot = slot.filter(|slot| *slot < self.document().template.slots.len());
        self.imp().selection.set(slot);
        self.refresh();
    }

    /// Moves the keyboard's focus through the grid, the selection following it (S15h,
    /// PIX-017's ruling of 2026-09-24).
    ///
    /// The focus **is** the selection — the one cell every other control acts on — and
    /// it is visible: the canvas draws the selected cell's outline, so a keyboard-only
    /// user can see where the arrows have taken them, and [`sync_canvas_label`] tells a
    /// screen reader which cell it is.
    ///
    /// The step is geometric and does not wrap: which cell is "to the right" is
    /// `Template::neighbour`'s answer — the same function `Ctrl+Shift+Arrow` and the
    /// CLI's `edit --swap` name a neighbour with — and the edge of the sheet answers
    /// `None`, so a focus that ran off the right edge stays where it is instead of
    /// reappearing on the left as if the key had been a different one. With nothing
    /// focused yet the first arrow picks the first cell, which is where a keyboard user
    /// starts reading the sheet.
    ///
    /// [`sync_canvas_label`]: Self::sync_canvas_label
    pub fn focus_step(&self, dx: i32, dy: i32) {
        let template = &self.document().template;
        let next = match self.selection() {
            Some(slot) => template.neighbour(slot, (dx, dy)),
            None if !template.slots.is_empty() => Some(0),
            None => None,
        };
        if let Some(slot) = next {
            self.select(Some(slot));
        }
    }

    /// Writes the canvas's accessible name: the cell the keyboard's focus is on
    /// (S15h, PIX-017).
    ///
    /// One name for the canvas rather than a proxy widget per cell (the same ruling):
    /// a screen reader announces "Cell 3 of 8" as the focus moves, and the cell's own
    /// state is the `+` button or the photo it shows, both of which are controls of
    /// their own with names of their own.
    fn sync_canvas_label(&self) {
        let area = self.canvas_widget();
        let label = match self.selection() {
            Some(slot) => {
                let cells = self.document().template.slots.len();
                match self.swap_source().filter(|source| *source != slot) {
                    // A marked swap is a state of the canvas rather than of one button, so
                    // the focus's own name carries it: a screen reader that has moved the
                    // selection to the target is told which cell the swap is coming from
                    // (S23, ruling 33's keyboard path).
                    Some(source) => fill(
                        gettext("Collage canvas, cell {} of {}, swapping with cell {}"),
                        &[slot + 1, cells, source + 1],
                    ),
                    None => fill(gettext("Collage canvas, cell {} of {}"), &[slot + 1, cells]),
                }
            }
            None => gettext("Collage canvas"),
        };
        a11y::label(&area, &label);
        *self.imp().canvas_label.borrow_mut() = label;
    }

    /// The canvas's current accessible name: which cell the keyboard's focus is on
    /// (S15h, PIX-017), or the bare "Collage canvas" before one is.
    ///
    /// The tests' handle on it — GTK4 exposes no getter for an accessible property's
    /// value — and nothing else reads it.
    pub fn canvas_label(&self) -> String {
        self.imp().canvas_label.borrow().clone()
    }

    /// Sets the framing zoom (the sidebar's spin row), keeping the fit.
    pub fn set_zoom(&self, zoom: f64) {
        if let (Some(slot), Some(crop)) = (
            self.selection(),
            self.selection().and_then(|slot| self.fitted_crop(slot)),
        ) {
            let next = self.fit_for(
                slot,
                CropTransform {
                    zoom: zoom.max(0.01),
                    ..crop
                },
            );
            self.gesture(Gesture::Step { slot, crop: next });
        }
    }

    /// Multiplies the selected cell's zoom by `factor` (S15: the strip's two zoom
    /// buttons).
    ///
    /// The factor is a ratio of the *fitted* crop, which is what the user is looking
    /// at, and the result is fitted again — the same path the wheel takes, so a
    /// button and a notch cannot land on different numbers.
    pub fn zoom_by(&self, factor: f64) {
        let Some(slot) = self.selection() else {
            return;
        };
        let Some(crop) = self.fitted_crop(slot) else {
            return;
        };
        self.set_zoom(crop.zoom * factor);
    }

    /// Turns the selected cell's photo by `degrees` (S15: the strip's rotate
    /// button).
    ///
    /// A step on the free angle (S11): the value is wrapped into `(-180, 180]` and
    /// never reduced, and the fit is recomputed after it, so the cell stays covered
    /// at whatever angle the button reaches.
    pub fn rotate_by(&self, degrees: f64) {
        let Some(slot) = self.selection() else {
            return;
        };
        let Some(crop) = self.fitted_crop(slot) else {
            return;
        };
        let next = self.fit_for(
            slot,
            CropTransform {
                rotation_deg: crop.rotation_deg + degrees,
                ..crop
            }
            .normalized(),
        );
        self.gesture(Gesture::Step { slot, crop: next });
    }

    /// The straightening slider: a live gesture, so the guides are on while it
    /// moves and the value is committed once it stops.
    pub fn straighten(&self, degrees: f64) {
        let Some(slot) = self.selection() else {
            return;
        };
        let Some(crop) = self.fitted_crop(slot) else {
            return;
        };
        self.imp().guides.set(true);
        let next = self.fit_for(
            slot,
            CropTransform {
                rotation_deg: degrees,
                ..crop
            },
        );
        if self.live(Command::SetCrop { slot, crop: next }).is_err() {
            self.imp().guides.set(false);
            return;
        }
        self.schedule_commit();
    }

    pub fn reset_framing(&self, slot: usize) {
        let _ = self.apply(Command::SetCrop {
            slot,
            crop: CropTransform::IDENTITY,
        });
    }

    /// Empties one cell: no photo, and its framing back to the default (S15).
    ///
    /// One command, so one undo step (`Command::ClearCell`) — and the same meaning
    /// the CLI's `edit --clear` has, because a control the strip offers has to be
    /// expressible on the machine surface too.
    pub fn clear_cell(&self, slot: usize) {
        let _ = self.apply(Command::ClearCell { slot });
    }

    pub fn place_photo(&self, slot: usize, path: PathBuf) {
        let _ = self.apply(Command::SetSource {
            slot,
            source: Some(path),
        });
    }

    /// The photos a caller offers, trimmed to `limit` with one report (S19).
    ///
    /// Ruling 34: a selection or a drop past nine keeps the **first nine in the
    /// order given**, and one report says how many were not used. The truncation is
    /// the *caller's* — `Command::AddPhotos` and `Selection::new` still refuse past
    /// `MAX_PHOTOS`, so nothing in core drops a photo on its own — and it happens
    /// here once so both of the window's list paths trim the same way instead of
    /// twice.
    ///
    /// `limit` is each caller's own capacity, which is why it is a parameter: a
    /// **drop** places into cells (replacing what a cell holds when it has to, so a
    /// full collage can still take nine), while **`Add photos…`** appends through
    /// `Command::AddPhotos`, which is all-or-nothing and grows the layout — so it
    /// can use exactly the photos the ceiling leaves room for, and a longer list
    /// would make the whole command refuse and place nothing.
    fn at_most(&self, paths: Vec<PathBuf>, limit: usize) -> Vec<PathBuf> {
        if paths.len() <= limit {
            return paths;
        }
        let unused = paths.len() - limit;
        self.toast(&fill(
            ngettext(
                "{} photo was not used: a collage takes at most {} photos",
                "{} photos were not used: a collage takes at most {} photos",
                unused as u32,
            ),
            &[unused, MAX_PHOTOS],
        ));
        paths.into_iter().take(limit).collect()
    }

    /// Files dropped on the canvas: the slot under the pointer first, then the
    /// slots after it, so a drop of five photos fills five slots in order. Slots
    /// that already hold a photo are skipped unless there is nothing else left,
    /// which is the rule that keeps a drop from silently replacing work.
    ///
    /// A drop longer than [`MAX_PHOTOS`] is trimmed first, with one report (S19):
    /// the drop places into cells, so nine is the whole list it can use.
    pub fn drop_files(&self, paths: Vec<PathBuf>, at: Option<usize>) {
        let paths = self.at_most(paths, MAX_PHOTOS);
        let slots = self.document().template.slots.len();
        let start = at
            .or_else(|| self.selection())
            .or_else(|| {
                self.document()
                    .cells
                    .iter()
                    .position(|cell| cell.source.is_none())
            })
            .unwrap_or(0);
        let empty: Vec<usize> = (0..slots)
            .map(|offset| (start + offset) % slots)
            .filter(|slot| self.document().cells[*slot].source.is_none())
            .collect();
        let occupied: Vec<usize> = (0..slots).map(|offset| (start + offset) % slots).collect();
        for (index, path) in paths.into_iter().enumerate() {
            let slot = empty
                .get(index)
                .or(if index == 0 { Some(&start) } else { None })
                .or_else(|| occupied.get(index));
            if let Some(slot) = slot {
                self.place_photo(*slot, path);
            }
        }
        self.select(Some(start.min(slots - 1)));
    }

    // ---- the layout stage (S14) -------------------------------------------

    /// The layout gallery: the band under the canvas.
    pub fn gallery(&self) -> Option<Rc<Gallery>> {
        self.imp().gallery.get().cloned()
    }

    /// The template the document is on, by name.
    pub fn current_template(&self) -> String {
        self.document().template.name
    }

    /// How many photos the collage holds. The gallery's candidates are for that
    /// count, and the count control's two bounds are about it.
    pub fn photo_count(&self) -> usize {
        self.document()
            .cells
            .iter()
            .filter(|cell| cell.source.is_some())
            .count()
    }

    /// The layouts the gallery lists: every template with the document's own **cell**
    /// count, in library order.
    ///
    /// The strip follows the layout rather than the photo count (S14b): `+` takes
    /// the layout with one cell more and leaves it empty, so a three-cell document
    /// with two photos in it is still a three-cell document, and a strip filtered
    /// to the photo count would show the layouts of a *different* document — the
    /// user's own next click would then move the sheet somewhere they were not
    /// looking.
    ///
    /// [`templates::with_slots`] is the same query the CLI's `templates --slots`
    /// answers with and the one `Selection::layouts` is expressed in, so the three
    /// cannot disagree about what "the layouts with that count" means.
    pub fn candidate_templates(&self) -> Vec<Template> {
        templates::with_slots(self.document().cells.len())
    }

    /// Switches the document to the layout `name`.
    ///
    /// The click on a candidate, the CLI's `edit --template` and the sidebar this
    /// replaced all end in `Command::SetTemplate`, so a layout change keeps the
    /// surviving cells' photos and framing one way.
    pub fn select_layout(&self, name: &str) {
        if self.current_template() == name {
            // The layout the document is already on: not an edit, and an undo step
            // that changes nothing is a step the user has to press `Ctrl+Z`
            // through.
            self.highlight_gallery();
            return;
        }
        let Some(template) = templates::get(name) else {
            return;
        };
        let _ = self.apply(Command::SetTemplate { template });
    }

    /// Writes the gallery's highlight from the document.
    fn highlight_gallery(&self) {
        if let Some(gallery) = self.imp().gallery.get() {
            let current = self.current_template();
            gallery.highlight(Some(&current));
        }
    }

    /// The count control's `−`: one cell fewer, and the layout follows (S14b).
    ///
    /// The control addresses the *layout* — the same thing [`add_photo`] does — so
    /// this is one command and not two: the last cell leaves with whatever it held,
    /// and `Ctrl+Z` is the way back. The cell's photo is not remembered anywhere,
    /// which is the point: `+` means "switch to a layout with one more cell", not
    /// "undo the last removal".
    ///
    /// [`add_photo`]: Self::add_photo
    pub fn remove_photo(&self) {
        let cells = self.document().cells.len();
        if cells <= MIN_PHOTOS {
            self.toast(&gettext("A collage needs at least one photo"));
            return;
        }
        // The selection can name a cell that is about to stop existing.
        if self.selection().is_some_and(|slot| slot >= cells - 1) {
            self.select(None);
        }
        let _ = self.apply(Command::RemoveLastCell);
    }

    /// The count control's `+`: switch to the layout with one cell more, and leave
    /// the new cell empty (S14b).
    ///
    /// Ruled 2026-09-23: `+`'s job is to change the layout, not to open a file
    /// chooser and not to undo the last removal. The cell it adds is empty on
    /// purpose, and an empty cell is a control of its own — the canvas draws a `+`
    /// over it, and clicking that region is what asks for a photo
    /// (`EditorWindow::choose_photo`, the same path a double click on an empty cell
    /// already took).
    pub fn add_photo(&self) {
        let cells = self.document().cells.len();
        if cells >= MAX_PHOTOS {
            self.toast(&fill(
                gettext("A collage takes at most {} photos"),
                &[MAX_PHOTOS],
            ));
            return;
        }
        if self.apply(Command::AddCell).is_ok() {
            // The user asked for a cell; the next thing they want is a photo in it,
            // so the new cell is selected and the canvas's own `+` is under the
            // pointer they already have.
            self.select(Some(cells));
        }
    }

    /// Exchanges the selected cell with its neighbour in direction `(dx, dy)`
    /// (S14b): `(-1, 0)` is the cell to the left, `(0, 1)` the cell below.
    ///
    /// Geometric rather than index arithmetic, because the library is not one row:
    /// `Template::neighbour` is the rule, and it is in `pixlay-core` so the CLI and
    /// the canvas cannot disagree about which cell is "to the right". `None` from
    /// that function — the edge of the sheet — means there is nothing to swap with,
    /// and the edit is skipped rather than clamped.
    pub fn swap_towards(&self, slot: usize, direction: (i32, i32)) {
        let doc = self.document();
        let Some(target) = doc.template.neighbour(slot, direction) else {
            return;
        };
        self.swap_slots(slot, target);
    }

    /// Exchanges two cells whole — photo *and* framing (S14b).
    ///
    /// The whole [`Cell`](pixlay_core::Cell) moves, so the picture that looked right
    /// in its cell keeps the framing that made it look right. The pairs that cannot
    /// be a swap — the same cell twice, a cell outside the layout — are refused by
    /// the command itself, so the CLI and the window report them the same way.
    pub fn swap_slots(&self, left: usize, right: usize) {
        let _ = self.swap_cells(left, right);
    }

    // ---- the swap's own state (S23, ruling 33) -----------------------------

    /// The cell a swap is marked from, if any.
    ///
    /// One mark serves the whole interaction (S23, ruling 33): the strip's swap
    /// control sets it, a `Shift`+click's press sets it, a `Shift`+drag sets it when
    /// the drag begins, and the canvas draws it as a dashed outline so the cell being
    /// moved is visible while the other half is chosen.
    pub fn swap_source(&self) -> Option<usize> {
        self.imp().swap.get()
    }

    /// The cell a swap drag is over right now, if one is in flight.
    ///
    /// The drop's highlight, and the cell the release exchanges with ([`swap_drag_end`]
    /// is the release). `None` on the keyboard path: its target is the selection, which
    /// the canvas outlines already.
    ///
    /// [`swap_drag_end`]: Self::swap_drag_end
    pub fn swap_target(&self) -> Option<usize> {
        self.imp().swap_target.get()
    }

    /// Marks `slot` as the cell a swap comes from, or takes the mark off with `None`.
    pub fn set_swap_source(&self, slot: Option<usize>) {
        if self.imp().swap.get() == slot {
            return;
        }
        self.imp().swap.set(slot);
        // A target belongs to the drag that found it: a mark that moved or went away
        // must not leave another cell looking like the drop's promise.
        self.imp().swap_target.set(None);
        self.refresh();
    }

    /// Takes the swap mark off, the document untouched: what `Esc` does (S23).
    pub fn cancel_swap(&self) {
        self.set_swap_source(None);
    }

    /// The strip's swap control, on the selected cell: marks it, or takes the mark off.
    pub fn toggle_swap(&self) {
        let Some(slot) = self.selection() else {
            return;
        };
        let next = (self.swap_source() != Some(slot)).then_some(slot);
        self.set_swap_source(next);
    }

    /// `Shift`+click on `target` (S23, ruling 33): the marked cell — or, with none
    /// marked, the selection — and `target` exchange, and the click's own cell is
    /// selected, which is where the photo that moved now is.
    ///
    /// The click lands on the press and the drag on the release, so this is the one-press
    /// form of the drag; a press that names the source itself changes nothing (two cells
    /// are what a swap is).
    pub fn swap_click(&self, target: usize) -> bool {
        let source = self.swap_source().or_else(|| self.selection());
        let swapped = match source {
            Some(source) if source != target => self.swap_cells(source, target),
            _ => false,
        };
        self.select(Some(target));
        swapped
    }

    /// A `Shift`+drag's first motion (S23): the cell under the press is the swap's
    /// source. `false` when the press was not on a cell — the caller then has no swap
    /// to carry, and the drag does nothing.
    pub fn swap_drag_begin(&self, x: f64, y: f64) -> bool {
        let Some(slot) = self.slot_at_widget(x, y) else {
            return false;
        };
        self.select(Some(slot));
        self.set_swap_source(Some(slot));
        true
    }

    /// The pointer moved while a swap drag is in flight: the cell under it becomes the
    /// drop's target, and the canvas fills it while the pointer is over it.
    ///
    /// The source itself is never a target: releasing there changes nothing, and a cell
    /// that lights up under a release that does nothing would be a highlight that lies.
    pub fn swap_drag_update(&self, x: f64, y: f64) {
        let Some(source) = self.swap_source() else {
            return;
        };
        let target = self.slot_at_widget(x, y).filter(|target| *target != source);
        if self.imp().swap_target.get() == target {
            return;
        }
        self.imp().swap_target.set(target);
        self.canvas_widget().queue_draw();
    }

    /// The release of a swap drag (S23, ruling 33): the two cells exchange when the
    /// pointer came down on a cell that is not the source, and **nothing happens
    /// otherwise** — released outside every cell, or on the source itself, the mark goes
    /// back with it.
    pub fn swap_drag_end(&self, x: f64, y: f64) -> bool {
        let Some(source) = self.swap_source() else {
            // Cancelled (`Esc`) while the pointer was still down: the release of a drag
            // that no longer exists is not an edit.
            return false;
        };
        let target = self.slot_at_widget(x, y).filter(|target| *target != source);
        match target {
            Some(target) => self.swap_cells(source, target),
            None => {
                self.set_swap_source(None);
                false
            }
        }
    }

    /// `Return` with a swap marked: the marked cell and the selected one exchange (S23,
    /// ruling 33's keyboard path — mark with the strip's control, choose with the
    /// arrows, `Return` on the target).
    ///
    /// `false` — nothing applied — when nothing is marked, nothing is selected, or the
    /// selection *is* the mark, because a swap of a cell with itself is not an edit.
    pub fn swap_selected(&self) -> bool {
        let (Some(source), Some(target)) = (self.swap_source(), self.selection()) else {
            return false;
        };
        if source == target {
            return false;
        }
        self.swap_cells(source, target)
    }

    /// Exchanges two cells through the one command, and spends the mark.
    ///
    /// Every path that swaps ends here, so the mark cannot survive a swap it did, a
    /// refusal, or a slot a layout change left behind: a mark that cannot act is one the
    /// user has to press `Esc` out of.
    fn swap_cells(&self, left: usize, right: usize) -> bool {
        let cells = self.document().cells.len();
        let swapped = if left >= cells || right >= cells {
            false
        } else {
            self.apply(Command::SwapCells { left, right }).is_ok()
        };
        self.set_swap_source(None);
        swapped
    }

    /// Appends `paths` in the order they arrive, one command.
    ///
    /// A list longer than the room the ceiling leaves is trimmed to what fits,
    /// with one report (S19, ruling 34) — `Command::AddPhotos` is all-or-nothing
    /// and would refuse the whole list, and a chosen folder of twenty photos is not
    /// an error the user should have to answer.
    pub fn add_photos(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let room = MAX_PHOTOS.saturating_sub(self.photo_count());
        let paths = self.at_most(paths, room);
        if paths.is_empty() {
            return;
        }
        let _ = self.apply(Command::AddPhotos { photos: paths });
    }

    /// Asks for photos to append (`GtkFileDialog::open_multiple`, which is the
    /// multi-file half of the chooser the canvas's single-slot path uses).
    pub fn choose_photos(&self) {
        let window = self.clone();
        let filter = photo_filter();
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Add photos to the collage"))
            .filters(&filters)
            .default_filter(&filter)
            .modal(true)
            .build();
        dialog.open_multiple(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[strong]
                window,
                move |result: Result<gio::ListModel, glib::Error>| {
                    let Ok(files) = result else {
                        // A dismissed dialog is not a failure: the user changed
                        // their mind, which is a normal thing to do.
                        return;
                    };
                    let paths: Vec<PathBuf> = (0..files.n_items())
                        .filter_map(|index| files.item(index))
                        .filter_map(|item| item.downcast::<gio::File>().ok())
                        .filter_map(|file| file.path())
                        .collect();
                    window.add_photos(paths);
                }
            ),
        );
    }

    /// Waits until the band's build has arrived, pumping the main context.
    ///
    /// A build that has *landed* is the answer, not "nothing is outstanding": the flag
    /// is false before the request has even gone out, and the request goes out with the
    /// canvas's own — from the first draw that knows the widget's size. So this pumps a
    /// few frames first ([`pump_until`]) and then waits for the count to move, which is
    /// what makes it usable right after a window has been shown or the editor's page
    /// pushed (measured 2026-09-23: without it the HIG walk read a 0x0 canvas and a
    /// band with no candidates, where the same window after a 500 ms pump was laid out).
    ///
    /// A caller that asks about a document no edit is waiting on (a bare selection
    /// change rebuilds nothing) is answered `true` as soon as the frames show that
    /// nothing was asked for.
    pub fn wait_for_gallery(&self, timeout: Duration) -> bool {
        let baseline = self.imp().gallery_builds.get();
        let context = glib::MainContext::default();
        let deadline = Instant::now() + timeout;
        self.pump_until(|| {
            self.imp().gallery_pending.get() || self.imp().gallery_builds.get() > baseline
        });
        loop {
            while context.pending() {
                context.iteration(false);
            }
            if self.imp().gallery_builds.get() > baseline {
                return true;
            }
            if !self.imp().gallery_pending.get() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    /// Pumps the main context for a few frames, or until `started` says the work a wait
    /// is about has been asked for.
    ///
    /// **The requests a wait is about are not queued synchronously.** A decode and the
    /// band's build both go out from `request_grid_for`, which the canvas's draw calls —
    /// and a widget is asked to draw only once it has an allocation, which a window that
    /// has just been shown (or that has just pushed the editor's page) does not have for
    /// its first frames. A wait that only reads the flags therefore returns before the
    /// work exists, and the test that follows reads an empty window: measured
    /// 2026-09-23, the HIG walk saw a 0x0 canvas and a band with no candidates, while the
    /// same window after a 500 ms pump was fully laid out.
    fn pump_until(&self, started: impl Fn() -> bool) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + WORK_GRACE;
        while Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            if started() {
                return;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    // ---- project ----------------------------------------------------------

    pub fn set_template(&self, name: &str) {
        let Some(template) = templates::get(name) else {
            return;
        };
        let _ = self.apply(Command::SetTemplate {
            template: template.clone(),
        });
        self.select(None);
    }

    /// A new collage: the default document, one empty cell, and the unsaved-work
    /// question first (PIX-002, 2026-09-24).
    ///
    /// `Ctrl+N` used to replace the document outright, which meant it destroyed work
    /// that closing the window would have offered to save; since S15d it asks the
    /// same Cancel / Discard / Save question the window's own close asks. Since S22
    /// the new document is what the window opens on, so this is also "start over".
    pub fn new_document(&self) {
        self.commit();
        if self.ask_to_save(Rc::new(|window: &EditorWindow| window.reset_document())) {
            return;
        }
        self.reset_document();
    }

    /// Replaces the document with a fresh one, whatever the current one holds.
    ///
    /// The pending edit is committed rather than dropped even here, where the
    /// document is about to go: an undo after `New` is not a promise this makes,
    /// but a boundary that silently swallowed a gesture would be the same defect
    /// one step earlier (PIX-002's ruling).
    fn reset_document(&self) {
        self.commit();
        match Editor::new(default_document()) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                // S23: a fresh document carries no marked swap (see `open_document`).
                self.imp().swap.set(None);
                self.imp().swap_target.set(None);
                self.select(None);
                self.requested_grid_reset();
                self.refresh_document();
            }
            Err(error) => self.toast(&error.to_string()),
        }
    }

    pub fn open(&self) {
        let window = self.clone();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Pixlay collages")));
        filter.add_pattern("*.pixlay");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open a collage"))
            .filters(&filters)
            .default_filter(&filter)
            .build();
        dialog.open(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[strong]
                window,
                move |result: Result<gio::File, glib::Error>| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        window.open_asking(&path);
                    }
                }
            ),
        );
    }

    /// Opens `path` once the unsaved-work question is answered.
    ///
    /// The file is chosen first and the question asked second, which is the order
    /// that cannot lose anything: a `Cancel` in the chooser never reaches the
    /// question, and a `Discard` here is about a document that is about to be
    /// replaced. Public because it is the half of `open` a test can drive — the
    /// chooser itself is a native dialog with nothing to type into — and because
    /// "open this, asking first" is the honest description of what the chooser's
    /// own callback does.
    pub fn open_asking(&self, path: &Path) {
        self.commit();
        let path = path.to_path_buf();
        let open: Boundary = Rc::new(move |window: &EditorWindow| {
            let _ = window.open_path(&path);
        });
        if self.ask_to_save(Rc::clone(&open)) {
            return;
        }
        open(self);
    }

    pub fn open_path(&self, path: &Path) -> Result<(), CoreError> {
        // The boundary's commit at the innermost level, so no caller — a test, the
        // chooser above — can replace the document and drop the edit the user was in
        // the middle of making.
        self.commit();
        let project = Project::load(path)?;
        match Editor::from_project(project) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                self.select(None);
                self.requested_grid_reset();
                // `refresh_document` names the window after the file (S22: the header's
                // own title is the same string, so it is written in one place).
                self.refresh_document();
                Ok(())
            }
            Err(error) => {
                self.toast(&error.to_string());
                Err(error)
            }
        }
    }

    /// Asks the unsaved-work question when there is unsaved work, and runs `then`
    /// once the user has answered it.
    ///
    /// **One question in one place** (PIX-002, 2026-09-24): the window closing,
    /// `New` and `Open` all end or replace the document, so all three have to offer
    /// the same three answers. `Discard` continues at once, `Save` continues only
    /// when the file was actually written — a save that failed leaves the document
    /// where it is, which is the whole reason for asking — and `Cancel` does
    /// nothing at all.
    ///
    /// Returns whether the question was presented, which is how a caller knows to
    /// stop what it was about to do and wait for the answer. The *commit* is not
    /// here: every caller commits the pending edit before asking, so the document
    /// the question is about is the one on screen.
    fn ask_to_save(&self, then: Boundary) -> bool {
        if !self.is_dirty() {
            return false;
        }
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Save the changes?")),
            Some(&gettext(
                "This collage has changes that are not saved anywhere yet.",
            )),
        );
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("discard", &gettext("Discard"));
        dialog.add_response("save", &gettext("Save"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        let window = self.downgrade();
        dialog.connect_response(None, move |_, response| {
            let Some(window) = window.upgrade() else {
                return;
            };
            match response {
                "discard" => then(&window),
                "save" => window.save_then(then.clone()),
                _ => {}
            }
        });
        dialog.present(Some(self));
        true
    }

    /// Saves the document, and runs `then` once the file is on disk.
    ///
    /// The two halves are what the unsaved-work question needs: `Save` means "the
    /// boundary happens once the save is done", and a document that has no name yet
    /// asks for one first — a dialog, so the continuation has to survive it. The
    /// save itself is the ordinary one (`Editor::save`), which commits the pending
    /// edit and adopts the rebased document; `then` runs only if the document is
    /// no longer dirty, so a failed save continues nothing.
    fn save_then(&self, then: Boundary) {
        if self.imp().editor.borrow().path().is_some() {
            self.save_now(None);
            if !self.is_dirty() {
                then(self);
            }
            return;
        }
        self.choose_save_path(Rc::new(move |window: &EditorWindow, path: &Path| {
            window.save_now(Some(path.to_path_buf()));
            if !window.is_dirty() {
                then(window);
            }
        }));
    }

    /// Presents the save dialog and calls `then` with the path the user chose.
    ///
    /// One dialog for both callers: `Save As` saves there, and the unsaved-work
    /// question's `Save` continues its boundary there. A cancelled chooser runs
    /// nothing.
    fn choose_save_path(&self, then: Saved) {
        let window = self.clone();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Pixlay collages")));
        filter.add_pattern("*.pixlay");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save the collage"))
            .filters(&filters)
            .default_filter(&filter)
            .initial_name(suggested_project_name(self))
            .build();
        dialog.save(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[strong]
                window,
                move |result: Result<gio::File, glib::Error>| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        then(&window, &path);
                    }
                }
            ),
        );
    }

    pub fn save(&self) {
        if self.imp().editor.borrow().path().is_none() {
            self.save_as();
            return;
        }
        self.save_now(None);
    }

    pub fn save_as(&self) {
        self.choose_save_path(Rc::new(|window: &EditorWindow, path: &Path| {
            window.save_now(Some(path.to_path_buf()));
        }));
    }

    /// Saves without a dialog, which is also what the tests and the main path
    /// walk use.
    ///
    /// The name is not written here: `refresh` runs `update_title`, and the window and
    /// the header take the same string from it (S22).
    pub fn save_to(&self, path: &Path) -> Result<PathBuf, CoreError> {
        let written = self.imp().editor.borrow_mut().save(Some(path))?;
        self.refresh();
        Ok(written)
    }

    fn save_now(&self, path: Option<PathBuf>) {
        let written = self.imp().editor.borrow_mut().save(path.as_deref());
        match written {
            Ok(written) => {
                self.refresh();
                self.toast(&fill(gettext("Saved {}"), &[file_name(&written)]));
            }
            Err(error) => self.toast(&error.to_string()),
        }
    }

    // ---- photo chooser ----------------------------------------------------

    pub fn choose_photo(&self, slot: usize) {
        let window = self.clone();
        let filter = photo_filter();
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Choose a photo"))
            .filters(&filters)
            .default_filter(&filter)
            .modal(true)
            .build();
        dialog.open(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[strong]
                window,
                move |result: Result<gio::File, glib::Error>| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        window.select(Some(slot));
                        window.place_photo(slot, path);
                    }
                }
            ),
        );
    }

    // ---- export -----------------------------------------------------------

    /// Presents the `Export…` dialog (S15): the format, the one size parameter and
    /// the file, asked as rows rather than as a permanent form (ruling 18).
    ///
    /// `Ctrl+E` and the header bar's button both land here, so the menu item, the
    /// accelerator and the button cannot ask three different questions.
    pub fn export(&self) {
        if let Some(dialog) = self.export_dialog() {
            dialog.present(self);
        }
    }

    /// Presents the `Frame…` dialog (S15): the document's frame as three rows.
    pub fn frame(&self) {
        if let Some(dialog) = self.frame_dialog() {
            dialog.present(self);
        }
    }

    /// Sets the document's frame as a live edit (S15): the canvas redraws while the
    /// dialog's rows move, and the change becomes one undo step once the value is
    /// quiet.
    ///
    /// The same machinery a drag uses (`live` + `schedule_commit`), and for the same
    /// reason: a spin row emits a value per keystroke, and forty undo steps for one
    /// number is not what the user made. A frame change also moves the clamp every
    /// cell is fitted against, so the commit re-decodes at the resting grid — the
    /// bitmaps a framed cell needs are not the ones an unframed one had.
    ///
    /// **`Err` is the document refusing the value, and it is reported where the value
    /// came from** (S15h, PIX-020): a gap large enough to leave a cell with nothing
    /// visible is refused by `Frame::validate`/`covering`, and a caller that ignored
    /// it would leave its own control showing a number the document does not have.
    /// Nothing is pending and nothing is scheduled when it refuses.
    pub fn set_frame(&self, frame: Frame) -> Result<(), CoreError> {
        self.live(Command::SetFrame { frame })?;
        self.schedule_commit();
        Ok(())
    }

    /// What the export form asks for, with the path this window holds.
    ///
    /// The path is *not* asked for twice: when none has been chosen yet the
    /// suggestion is derived from the format, which is why this builds the
    /// settings in two steps rather than passing a placeholder into the dialog's
    /// own suggestion (that circularity was a real defect, found by
    /// `tests/mainpath.rs`).
    ///
    /// Since ruling 18 removed the utility pane, this state *is* the export form:
    /// the format and the one quality option live here, and S15's `Export…` dialog
    /// is the rows over them.
    pub fn export_settings(&self) -> Settings {
        let mut settings = self.imp().export.borrow().clone();
        if settings.path.as_os_str().is_empty() {
            settings.path = default_export_path(self, settings.format);
        }
        settings
    }

    /// Sets the export form's state, which is also what a test walks the
    /// background export with.
    pub fn set_export_settings(&self, settings: &Settings) {
        *self.imp().export.borrow_mut() = settings.clone();
    }

    /// Whether an export to `path` may start, and whether it would replace a file
    /// that is already there.
    ///
    /// The form asks this before it closes or spawns anything (S15c): a path that is
    /// one of the document's own photos is refused on the spot — the same rule
    /// `render` and `thumb` apply to the same path, with the same message — and a file
    /// that is already there is the user's question to answer, not the writer's.
    pub fn export_destination(&self, path: &Path) -> Result<bool, String> {
        let sources = self.imp().editor.borrow().sources();
        export::destination(path, &sources.paths)
    }

    /// Exports on a worker thread, with the progress bar in the bottom bar.
    pub fn start_export(&self, path: PathBuf) {
        // A boundary like the others (PIX-002's ruling): the export is of the
        // document on screen, so a frame change still inside its quiet interval is
        // committed here — one undo step — rather than exported as a difference
        // between the canvas and the file.
        self.commit();
        if self.imp().exporting.replace(true) {
            return;
        }
        let sources = self.imp().editor.borrow().sources();
        if let Some(slot) = sources.missing.first() {
            self.imp().exporting.set(false);
            self.toast(&fill(
                ngettext(
                    "{} photo is missing and has to be found before the export",
                    "{} photos are missing and have to be found before the export",
                    sources.missing.len() as u32,
                ),
                &[sources.missing.len()],
            ));
            self.select(Some(*slot));
            return;
        }
        let settings = Settings {
            path,
            ..self.export_settings()
        };
        self.show_progress(true);
        self.set_progress(0.0, &gettext("Preparing…"));
        let doc = self.display_document();
        let weak = glib::SendWeakRef::from(self.downgrade());
        let report = move |event: export::Event| {
            let weak = weak.clone();
            glib::MainContext::default().invoke(move || {
                if let Some(window) = weak.upgrade() {
                    window.on_export_event(event);
                }
            });
        };
        *self.imp().export.borrow_mut() = settings.clone();
        // A thread that could not be started is not a progress bar to sit behind
        // (S15h, PIX-014): the pending flag and the bar go back to their resting
        // state and the reason is the toast.
        if let Err(down) = export::spawn(
            doc,
            sources.paths,
            settings,
            report,
            self.imp().workers.get().export,
        ) {
            self.imp().exporting.set(false);
            self.show_progress(false);
            self.toast(&WorkerKind::Export.message(down));
        }
    }

    /// Exports synchronously; the same function the worker calls.
    pub fn export_to(&self, settings: &Settings) -> Result<Report, String> {
        self.commit();
        let sources = self.imp().editor.borrow().sources();
        if !sources.missing.is_empty() {
            return Err(fill(
                ngettext(
                    "{} photo is missing",
                    "{} photos are missing",
                    sources.missing.len() as u32,
                ),
                &[sources.missing.len()],
            ));
        }
        let doc = self.display_document();
        export::run(&doc, &sources.paths, settings, &|_| ())
    }

    fn on_export_event(&self, event: export::Event) {
        match event {
            export::Event::Progress(progress) => {
                let label = match progress {
                    Progress::Decoding { done, total } => {
                        fill(gettext("Preparing photo {} of {}"), &[done, total])
                    }
                    Progress::Rendering => gettext("Compositing…"),
                    Progress::Encoding => gettext("Writing the file…"),
                };
                self.set_progress(progress.fraction(), &label);
            }
            export::Event::Finished(result) => {
                self.imp().exporting.set(false);
                self.show_progress(false);
                match result {
                    Ok(report) => {
                        let megabytes = format!("{:.1}", report.bytes as f64 / 1_048_576.0);
                        self.toast(&fill(
                            gettext("Exported {} ({} MB)"),
                            &[file_name(&report.path), megabytes],
                        ));
                    }
                    Err(message) => self.toast(&fill(gettext("Export failed: {}"), &[message])),
                }
            }
        }
    }

    /// Whether the export's progress bar is on screen.
    ///
    /// The tests' handle on "the dialog's export runs the same background path the
    /// menu's action does": the bar is raised while an export is in flight and taken
    /// away when it ends, whichever surface asked for it.
    pub fn progress_revealed(&self) -> bool {
        self.imp()
            .progress_revealer
            .get()
            .is_some_and(|revealer| revealer.reveals_child())
    }

    fn show_progress(&self, show: bool) {
        if let Some(revealer) = self.imp().progress_revealer.get() {
            revealer.set_reveal_child(show);
        }
    }

    fn set_progress(&self, fraction: f64, label: &str) {
        if let Some(progress) = self.imp().progress.get() {
            progress.set_fraction(fraction);
            progress.set_text(Some(label));
        }
    }

    // ---- decoding ---------------------------------------------------------

    /// The grid the widget would like; asks for a decode when it changed.
    pub fn request_grid_for(&self, width: i32, height: i32) {
        let aspect = self.document().template.aspect;
        let resting = canvas::preferred_grid(aspect, width, height);
        let wanted = self.gesture_aware_grid(resting);
        let (current, _) = self.images();
        if wanted == current || self.imp().requested.get() == Some(wanted) {
            return;
        }
        self.request_decode(wanted);
        // The band follows the *resting* document: its thumbnails are the committed
        // doc's, so a live gesture's coarse grid must not rebuild them (74.6 ms per
        // drag, `docs/CONTRACT.md` §8 "S14"), and at rest the request goes out with
        // the canvas's own — which is what lets the worker put the preview-grade
        // copies in the cache before the band reads them.
        if wanted == resting {
            self.request_gallery();
        }
    }

    /// The grid the canvas rests at: the widget's own, whatever a gesture is doing.
    fn resting_grid(&self) -> PixelSize {
        let area = self.canvas_widget();
        canvas::preferred_grid(self.document().template.aspect, area.width(), area.height())
    }

    /// The resting grid, or the coarse one a live gesture draws at.
    fn gesture_aware_grid(&self, resting: PixelSize) -> PixelSize {
        if self.imp().editor.borrow().gesture_live() {
            gesture_grid(resting)
        } else {
            resting
        }
    }

    fn requested_grid_reset(&self) {
        self.imp().requested.set(None);
        self.imp().images.replace((PLACEHOLDER_GRID, Images::new()));
    }

    /// Queues the gallery's build: every layout with the document's cell count,
    /// drawn as a sketch.
    ///
    /// On the canvas's own worker (`decode.rs`) because that is where the band's
    /// builds have always run — one thread owns every background drawing — but
    /// there is nothing to decode any more (S21): a candidate is its template's
    /// geometry, so the job names no file and `decoded_sources` does not move.
    ///
    /// The two colours come from the band's own widgets ([`Gallery::sketch_style`]),
    /// read here on the main thread and sent as plain data.
    fn request_gallery(&self) {
        let Some(gallery) = self.imp().gallery.get() else {
            return;
        };
        let candidates: Vec<(Template, PixelSize)> = self
            .candidate_templates()
            .into_iter()
            .map(|template| {
                let grid = templates::candidate_grid(template.aspect);
                (template, grid)
            })
            .collect();
        let style = gallery.sketch_style();
        // The request's own borrow of the decoder ends before anything is reported:
        // the failure path calls back into the window, which reads the same field.
        let outcome = {
            let mut decoder = self.imp().decoder.borrow_mut();
            match decoder.as_mut() {
                Some(decoder) => decoder.request_gallery(candidates, style),
                None => Err(Down::Start),
            }
        };
        match outcome {
            Ok(generation) => {
                self.imp().gallery_generation.set(generation);
                self.imp().gallery_pending.set(true);
            }
            // Nothing was queued, so nothing is pending (S15h, PIX-014): a band that
            // stayed marked in flight would keep every wait on it waiting for ever.
            Err(down) => {
                self.imp().gallery_pending.set(false);
                self.report_decode_down(down);
            }
        }
    }

    /// One gallery build arrived.
    fn on_gallery(&self, reply: GalleryReply) {
        // No decode is counted here, and none can be: since S21 the band's job is a
        // list of templates, and a sketch names no file. `decoded_sources` is the
        // one counter of decoded files, and the band's builds do not move it.
        if reply.generation != self.imp().gallery_generation.get() {
            return;
        }
        self.imp().gallery_pending.set(false);
        self.imp()
            .gallery_builds
            .set(self.imp().gallery_builds.get() + 1);
        let Some(gallery) = self.imp().gallery.get() else {
            return;
        };
        gallery.show(self, reply.candidates);
    }

    fn request_decode(&self, grid: PixelSize) {
        let sources = self.imp().editor.borrow().sources();
        let doc = self.display_document();
        // The missing photos are a fact about the document, not about the worker: the
        // banner says so whether or not a decode could be asked for.
        self.imp().missing.replace(sources.missing);
        self.update_banner();
        let outcome = {
            let mut decoder = self.imp().decoder.borrow_mut();
            match decoder.as_mut() {
                Some(decoder) => decoder.request(&doc, sources.paths, grid),
                None => Err(Down::Start),
            }
        };
        match outcome {
            Ok(generation) => {
                self.imp().generation.set(generation);
                self.imp().requested.set(Some(grid));
            }
            // No job was queued, so no grid is in flight (S15h, PIX-014): the canvas
            // keeps the bitmaps it has and the window says why it is not redrawing.
            Err(down) => {
                self.imp().requested.set(None);
                self.report_decode_down(down);
            }
        }
    }

    /// Reports the decoding worker's own failure, once per window (S15h, PIX-014).
    ///
    /// Once, because a canvas asks on every resize and every edit: the first toast is
    /// news, the fortieth is noise, and the state the user is in has not changed. The
    /// log line is not deduplicated — a run that is being debugged wants every
    /// refusal — which is why the reason is logged where the request failed and only
    /// the sentence is held back here.
    fn report_decode_down(&self, down: Down) {
        if self.imp().decode_reported.replace(true) {
            return;
        }
        glib::g_warning!("pixlay", "{}", WorkerKind::reason(WorkerKind::Decode, down));
        self.toast(&WorkerKind::Decode.message(down));
    }

    /// One answer from the decoding thread, routed by what was asked for.
    fn on_decoder_event(&self, event: crate::decode::Event) {
        match event {
            crate::decode::Event::Canvas(reply) => self.on_decoded(reply),
            crate::decode::Event::Gallery(reply) => self.on_gallery(reply),
        }
    }

    fn on_decoded(&self, reply: Reply) {
        // Counted for *every* reply, stale ones included: the count is about what
        // the decoding thread did, and a superseded build decoded the same file
        // whether or not its bitmaps were accepted.
        self.imp()
            .decoded
            .set(self.imp().decoded.get() + reply.decodes);
        if reply.generation != self.imp().generation.get() {
            // A stale reply: the document moved on while this was decoding.
            return;
        }
        let grid = self.imp().requested.get().unwrap_or(PLACEHOLDER_GRID);
        let mut images = Images::new();
        for bitmap in reply.bitmaps {
            let slot = bitmap.slot;
            match pixlay_render::Bitmap::from_argb32_region(
                bitmap.width as i32,
                bitmap.height as i32,
                bitmap.origin,
                bitmap.display,
                bitmap.pixels,
            ) {
                Ok(bitmap) => {
                    images.insert(slot, bitmap);
                }
                Err(error) => glib::g_warning!("pixlay", "a decoded bitmap was refused: {error}"),
            }
        }
        self.imp().images.replace((grid, images));
        self.imp().requested.set(None);
        // The controls are placed from the grid their bitmaps are *for*, so the arrival
        // is what puts them where the sheet actually is: a `sync` that ran while the
        // bitmaps were still the placeholder placed nothing (`CellControls::sync_in`),
        // and the window's other sync sites — a layout, an edit, a selection — are not
        // guaranteed to follow this reply (measured 2026-09-24: a window that had just
        // opened could leave its strip at the `1x1` placement until something else
        // asked for a sync).
        if let Some(controls) = self.cell_controls() {
            controls.sync(self);
        }
        if !reply.failed.is_empty() {
            self.toast(&fill(
                ngettext(
                    "{} photo could not be read",
                    "{} photos could not be read",
                    reply.failed.len() as u32,
                ),
                &[reply.failed.len()],
            ));
        }
        self.canvas_widget().queue_draw();
    }

    /// Waits until nothing is in flight — neither a canvas decode, nor a gallery
    /// build, nor an export — pumping the main context.
    ///
    /// This is the tests' handle on an asynchronous pipeline, and the same loop
    /// the widget's own draw runs in — the window has one thread, and this is it.
    /// The band counts here as work of the same kind (S14): it runs on the same
    /// worker and its decodes land in `decoded_sources`, so a caller that waited
    /// only for the canvas could read a number the band was about to move — the
    /// measurement race `tests/gesture.rs` was seeing as a live gesture decoding.
    pub fn wait_for_idle(&self, timeout: Duration) -> bool {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + timeout;
        // The work this waits for has to have been asked for first (`pump_until`).
        self.pump_until(|| {
            self.imp().requested.get().is_some()
                || self.imp().gallery_pending.get()
                || self.imp().exporting.get()
        });
        while Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            if self.imp().requested.get().is_none()
                && !self.imp().gallery_pending.get()
                && !self.imp().exporting.get()
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        false
    }

    /// Pumps the main context for a while, which is how a test lets a presented
    /// window lay itself out and draw.
    pub fn pump(&self, duration: Duration) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    // ---- presentation -----------------------------------------------------

    /// Rebuilds the title and the buttons from the document and the stage.
    pub fn refresh(&self) {
        self.update_actions();
        self.update_title();
        self.update_banner();
        self.update_gallery_control();
        self.sync_canvas_label();
        // The cells' own controls follow the document and the selection, so they are
        // written here rather than from a draw: showing a widget inside GTK's own
        // traversal leaves it snapshotted before it is allocated
        // (`CellControls::sync`).
        if let Some(controls) = self.cell_controls() {
            controls.sync(self);
        }
        self.canvas_widget().queue_draw();
    }

    /// Writes the count control: the document's own cell count and the two bounds
    /// it acts on (`MIN_PHOTOS` / `MAX_PHOTOS`).
    ///
    /// The number is the **cell** count, because that is the number the two buttons
    /// move (S14b): `+` takes the layout with one cell more, `−` the layout with one
    /// cell fewer, and the control reads out the count it edits. It is also the
    /// number the strip below it filters by, so the three always agree — while the
    /// photo count can legitimately be lower (a `+` whose cell has no photo yet, a
    /// per-cell clear).
    fn update_gallery_control(&self) {
        if let Some(gallery) = self.imp().gallery.get() {
            gallery.update_control(self.document().cells.len());
        }
    }

    /// The same, plus new bitmaps: what every document edit calls.
    pub fn refresh_document(&self) {
        // A committed document is drawn at the resting grid: this is where a
        // gesture's coarse frame is refined, and where every edit made outside a
        // gesture lands.
        let grid = self.resting_grid();
        self.refresh();
        // A canvas with no allocation cannot show a grid, and `preferred_grid`
        // answers an unallocated widget with 1x1: a decode of every photo into a
        // single pixel, plus a preview-grade copy of each at that size (measured
        // 2026-09-23: seven decodes on every open, 21 for eight photos instead of
        // 14). A window that has just been created and given its photos before its
        // first frame is exactly that state, and the first draw asks with a real size
        // (`request_grid_for`), so nothing is lost by not asking now.
        if !self.canvas_allocated() {
            return;
        }
        self.request_decode(grid);
        // The gallery shows the same document with another template, so every
        // committed edit is a new band — on the same worker, at the same edge, and
        // its own builds decode nothing the canvas has not decoded already.
        self.request_gallery();
    }

    /// Whether the canvas has an allocation to draw into.
    fn canvas_allocated(&self) -> bool {
        let area = self.canvas_widget();
        area.width() > 0 && area.height() > 0
    }

    fn update_actions(&self) {
        let state = {
            let editor = self.imp().editor.borrow();
            let doc = editor.doc();
            (
                editor.can_undo(),
                editor.can_redo(),
                self.selection().is_some(),
                doc.cells.iter().any(|cell| cell.source.is_some()),
            )
        };
        let (undo, redo, selected, has_any_photo) = state;
        for action in self.imp().actions.borrow().iter() {
            let enabled = match action.name().as_str() {
                "undo" => undo,
                "redo" => redo,
                "save" | "save-as" | "frame" => true,
                "export" => has_any_photo,
                "clear-cell" | "reset-framing" => selected,
                _ => action.is_enabled(),
            };
            action.set_enabled(enabled);
        }
    }

    fn update_title(&self) {
        let editor = self.imp().editor.borrow();
        let name = match editor.path() {
            Some(path) => file_name(path),
            None => gettext("Untitled collage"),
        };
        let title = if editor.is_dirty() {
            format!("• {name}")
        } else {
            name
        };
        drop(editor);
        // One page (S22): the window and the header bar's title widget carry the same
        // string, so what the shell calls the document and what the bar shows cannot
        // drift apart.
        self.set_title(Some(&title));
        if let Some(widget) = self.imp().title.get() {
            widget.set_title(&title);
        }
    }

    fn update_banner(&self) {
        let Some(banner) = self.imp().banner.get() else {
            return;
        };
        let missing = self.imp().missing.borrow().len();
        if missing == 0 {
            banner.set_revealed(false);
            return;
        }
        banner.set_title(&fill(
            ngettext(
                "{} photo could not be loaded",
                "{} photos could not be loaded",
                missing as u32,
            ),
            &[missing],
        ));
        banner.set_revealed(true);
    }

    /// Short feedback that does not need an answer (HIG `patterns/feedback`).
    pub fn toast(&self, message: &str) {
        *self.imp().last_toast.borrow_mut() = Some(message.to_string());
        self.imp().toasts.set(self.imp().toasts.get() + 1);
        if let Some(overlay) = self.imp().toast.get() {
            overlay.add_toast(adw::Toast::new(message));
        } else {
            glib::g_warning!("pixlay", "{message}");
        }
    }

    /// The last message a toast carried, if any.
    ///
    /// The tests' handle on "a refusal is reported rather than applied silently"
    /// (`AGENTS.md`: a visual conclusion has to become something a test can read,
    /// and a toast is not a widget tree a test can walk).
    pub fn last_toast(&self) -> Option<String> {
        self.imp().last_toast.borrow().clone()
    }

    /// How many toasts this window has shown since it opened.
    ///
    /// The tests' handle on "a refusal is reported *once*": `Ctrl+A` past the cap
    /// fires one selection change, and this is the number that says so — the last
    /// message alone cannot tell one report from two.
    pub fn toasts(&self) -> u64 {
        self.imp().toasts.get()
    }

    /// The window's current notice, when it has one: the banner text shown above
    /// the canvas. `None` means there is nothing to warn about.
    pub fn notice(&self) -> Option<String> {
        let banner = self.imp().banner.get()?;
        banner.is_revealed().then(|| banner.title().to_string())
    }

    /// The slots whose photo could not be found, in slot order.
    pub fn missing_photos(&self) -> Vec<usize> {
        self.imp().missing.borrow().clone()
    }

    pub fn is_dirty(&self) -> bool {
        self.imp().editor.borrow().is_dirty()
    }

    pub fn project_path(&self) -> Option<PathBuf> {
        self.imp()
            .editor
            .borrow()
            .path()
            .map(|path| path.to_path_buf())
    }
}

/// The window's primary menu: the actions that do not deserve a button.
///
/// The same three-section shape as the rest of the shell, which is the shape both
/// reference apps use and ruling 24 asks for: the file items, the editor's own
/// commands, then the help items. **Save has a menu item here** (S22, ruling 37): its
/// header-bar button went because it sat beside Export and read as the same action,
/// and the menu and `Ctrl+S` are what the function lives in.
fn main_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let collage = gio::Menu::new();
    collage.append(Some(&gettext("New collage")), Some("app.new"));
    collage.append(Some(&gettext("Open…")), Some("app.open"));
    collage.append(Some(&gettext("Save")), Some("win.save"));
    collage.append(Some(&gettext("Save as…")), Some("win.save-as"));
    collage.append(Some(&gettext("Export…")), Some("win.export"));
    menu.append_section(None, &collage);

    let edit = gio::Menu::new();
    edit.append(Some(&gettext("Add photos…")), Some("win.add-photos"));
    edit.append(
        Some(&gettext("Reset the framing")),
        Some("win.reset-framing"),
    );
    menu.append_section(None, &edit);

    let help = gio::Menu::new();
    help.append(Some(&gettext("Keyboard shortcuts")), Some("app.shortcuts"));
    help.append(Some(&gettext("About Pixlay")), Some("app.about"));
    menu.append_section(None, &help);

    menu
}

/// The filter both photo choosers use: the extensions `pixlay-imaging` lists as
/// photos (`PHOTO_EXTENSIONS`, which `scan` walks with).
fn photo_filter() -> gtk::FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("Photos")));
    for extension in pixlay_imaging::PHOTO_EXTENSIONS {
        filter.add_pattern(&format!("*{extension}"));
    }
    filter
}

fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(label)
        .build();
    a11y::label(&button, label);
    button
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn suggested_project_name(window: &EditorWindow) -> String {
    match window.project_path() {
        Some(path) => file_name(&path),
        None => format!("{}.pixlay", gettext("collage")),
    }
}

/// Where an export goes when the form has never been told (S15h, PIX-010).
///
/// Ruling 2026-09-24: **the first export lands in the pictures directory** —
/// `XDG_PICTURES_DIR` or `~/Pictures` — and a later one in the directory the last
/// export used, which is what the stored path carries from then on. No chooser step
/// is added to the main path, and nothing is written until the export itself runs, so
/// a project that is never exported leaves no trace anywhere.
///
/// An account with no pictures directory at all falls back to the bare name in the
/// process's own directory.
fn default_export_path(window: &EditorWindow, format: pixlay_imaging::encode::Format) -> PathBuf {
    let name = suggested_export_name(window, format);
    match crate::export::default_folder() {
        Some(dir) => dir.join(name),
        None => PathBuf::from(name),
    }
}

/// The name the export dialog opens with: the project's own name, or the
/// template's, with the extension of the format that is selected.
fn suggested_export_name(window: &EditorWindow, format: pixlay_imaging::encode::Format) -> String {
    let doc = window.document();
    let extension = match format {
        pixlay_imaging::encode::Format::Png => "png",
        pixlay_imaging::encode::Format::Jpeg => "jpg",
    };
    match window.project_path() {
        Some(path) => {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| gettext("collage"));
            format!("{stem}.{extension}")
        }
        None => format!("{}.{extension}", doc.template.name),
    }
}
