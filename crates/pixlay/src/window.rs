//! The window: the two panes, the actions, and everything that connects them.
//!
//! This is the only place in the GUI that knows about the document. The canvas
//! draws what it is asked to draw, the sidebar emits commands, and both go through
//! the methods here — which is also what makes the whole main path reachable from
//! a test without a pointer: `set_template`, `place_photo`, `set_crop`,
//! `export_to` are the same calls the widgets make.
//!
//! The structure is the one libadwaita's own apps use (HIG `patterns/containers`):
//!
//! ```text
//! AdwToolbarView          top: AdwHeaderBar, bottom: the export progress bar
//!  └ AdwToastOverlay      short-lived feedback
//!     └ GtkBox            the missing-photo banner above the content
//!        └ AdwOverlaySplitView   a utility pane that overlays when narrow
//!           ├ sidebar    (utility pane, F9)
//!           └ canvas     (the document, drawn by pixlay_render::draw)
//! ```
//!
//! Two rules the whole file obeys, both from `AGENTS.md`: a GTK object never
//! leaves the main thread (the decoding and encoding threads send plain data back
//! through `MainContext::invoke`), and nothing here touches a pixel — the canvas
//! hands the document to the renderer and the export hands it to
//! `pixlay-imaging`.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::gio;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt as _;
use gtk4::prelude::*;
use libadwaita as adw;

use pixlay_core::{
    CanvasSpec, CollageDoc, Command, CoreError, CropTransform, PixelSize, Project, templates,
};
use pixlay_imaging::gesture_grid;
use pixlay_render::Images;

use crate::a11y;
use crate::canvas::{self, Gesture};
use crate::decode::{Decoder, Reply};
use crate::export::{self, Event, Progress, Report, Settings, Size};
use crate::i18n::{fill, gettext, ngettext};
use crate::sidebar::Sidebar;
use crate::state::Editor;

/// The template a new document starts from: 4:3 like an album page, five slots,
/// so the main path starts with a layout that does not need ten photos.
pub const DEFAULT_TEMPLATE: &str = "mosaic-5-hero";

/// The long edge of a new document's sheet, in millimetres (A4's).
pub const DEFAULT_LONG_EDGE_MM: f64 = 297.0;

/// The resolution a new export form starts at, in dots per inch.
///
/// The CLI's own default (`render --dpi`, `docs/CONTRACT.md` §5) and what the form
/// goes back to whenever the size mode returns to a resolution. The form has to be
/// seeded with it: a `GtkSpinButton` starts at its adjustment's *lower* bound, so
/// without this a new window would export at 72 dpi (the bottom of the range) —
/// 842x631 px for the default A4 sheet.
pub const DEFAULT_EXPORT_DPI: u32 = 300;

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
        pub sidebar: OnceCell<Sidebar>,
        pub split: OnceCell<adw::OverlaySplitView>,
        pub banner: OnceCell<adw::Banner>,
        pub toast: OnceCell<adw::ToastOverlay>,
        pub progress: OnceCell<gtk::ProgressBar>,
        pub progress_revealer: OnceCell<gtk::Revealer>,
        pub commit_timer: RefCell<Option<glib::SourceId>>,
        pub export_path: RefCell<Option<PathBuf>>,
        pub exporting: Cell<bool>,
        pub missing: RefCell<Vec<usize>>,
        pub actions: RefCell<Vec<gio::SimpleAction>>,
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
                sidebar: OnceCell::new(),
                split: OnceCell::new(),
                banner: OnceCell::new(),
                toast: OnceCell::new(),
                progress: OnceCell::new(),
                progress_revealer: OnceCell::new(),
                commit_timer: RefCell::new(None),
                export_path: RefCell::new(None),
                exporting: Cell::new(false),
                missing: RefCell::new(Vec::new()),
                actions: RefCell::new(Vec::new()),
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
    CollageDoc::new(
        CanvasSpec::with_ratio(template.aspect, DEFAULT_LONG_EDGE_MM),
        template,
    )
}

impl EditorWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn build(&self) {
        let imp = self.imp();

        // ---- header bar ---------------------------------------------------
        let toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text(gettext("Show or hide the editing controls"))
            .build();
        a11y::label(&toggle, &gettext("Show or hide the editing controls"));
        let undo = icon_button("edit-undo-symbolic", &gettext("Undo"));
        undo.set_action_name(Some("win.undo"));
        let redo = icon_button("edit-redo-symbolic", &gettext("Redo"));
        redo.set_action_name(Some("win.redo"));
        let save = icon_button("document-save-symbolic", &gettext("Save"));
        save.set_action_name(Some("win.save"));
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
        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(gettext("Main menu"))
            .primary(true)
            .menu_model(&main_menu())
            .build();
        a11y::label(&menu, &gettext("Main menu"));

        let header = adw::HeaderBar::new();
        header.pack_start(&toggle);
        header.pack_end(&export);
        header.pack_end(&menu);
        header.pack_end(&save);
        header.pack_end(&redo);
        header.pack_end(&undo);

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

        // ---- content -------------------------------------------------------
        let canvas = canvas::build(self);
        let sidebar = Sidebar::build(self);
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar.root)
            .content(&canvas)
            .min_sidebar_width(300.0)
            .max_sidebar_width(380.0)
            .sidebar_width_fraction(0.28)
            .build();
        split.connect_show_sidebar_notify(glib::clone!(
            #[weak]
            toggle,
            move |view: &adw::OverlaySplitView| {
                toggle.set_active(view.shows_sidebar());
            }
        ));
        toggle.connect_toggled(glib::clone!(
            #[weak]
            split,
            move |button: &gtk::ToggleButton| {
                split.set_show_sidebar(button.is_active());
            }
        ));

        // The banner's action is named once, here: its button exists from the
        // start, and a control with no label is a control a screen reader cannot
        // announce (`docs/HIG-REVIEW.md`, section 1).
        let banner = adw::Banner::new("");
        banner.set_button_label(Some(&gettext("Find it…")));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&banner);
        content.append(&split);

        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&content));
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.add_bottom_bar(&progress_revealer);
        toolbar.set_content(Some(&toast));

        self.set_content(Some(&toolbar));
        self.set_title(Some(&gettext("Untitled collage")));
        self.set_default_size(1100, 760);
        // The minimum the layout is designed for: the pane overlays the canvas
        // below `min_sidebar_width` + a canvas of its own, which is what
        // `AdwOverlaySplitView` is for (HIG `guidelines/adaptive`).
        self.set_size_request(480, 360);

        imp.canvas.set(canvas).ok();
        imp.sidebar.set(sidebar).ok();
        imp.split.set(split).ok();
        imp.banner.set(banner.clone()).ok();
        imp.toast.set(toast).ok();
        imp.progress.set(progress).ok();
        imp.progress_revealer.set(progress_revealer).ok();

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

        // ---- decoding ------------------------------------------------------
        let window = self.downgrade();
        let sender = glib::SendWeakRef::from(window);
        let decoder = Decoder::spawn(move |reply| {
            let sender = sender.clone();
            // The reply is plain data; the window is reached on its own thread.
            glib::MainContext::default().invoke(move || {
                if let Some(window) = sender.upgrade() {
                    window.on_decoded(reply);
                }
            });
        });
        *imp.decoder.borrow_mut() = Some(decoder);

        // ---- unsaved work ---------------------------------------------------
        let close_weak = self.downgrade();
        self.connect_close_request(glib::clone!(
            #[strong]
            close_weak,
            move |_window| {
                let Some(window) = close_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if !window.imp().editor.borrow().is_dirty() {
                    return glib::Propagation::Proceed;
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
                let dialog_weak = window.downgrade();
                dialog.connect_response(
                    None,
                    glib::clone!(
                        #[strong]
                        dialog_weak,
                        move |_, response| {
                            let Some(window) = dialog_weak.upgrade() else {
                                return;
                            };
                            match response {
                                "discard" => window.destroy(),
                                "save" => {
                                    window.save();
                                    if !window.imp().editor.borrow().is_dirty() {
                                        window.destroy();
                                    }
                                }
                                _ => {}
                            }
                        }
                    ),
                );
                dialog.present(Some(&window));
                glib::Propagation::Stop
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
            "choose-export-path",
            true,
            Box::new(|window| {
                window.choose_export_path();
            }),
        );
        add(
            "add-photo",
            false,
            Box::new(|window| {
                if let Some(slot) = window.selection() {
                    window.choose_photo(slot);
                }
            }),
        );
        add(
            "clear-photo",
            false,
            Box::new(|window| {
                if let Some(slot) = window.selection() {
                    window.clear_slot(slot);
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
        add(
            "toggle-sidebar",
            true,
            Box::new(|window| {
                if let Some(split) = window.imp().split.get() {
                    split.set_show_sidebar(!split.shows_sidebar());
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

    /// Files the decoding thread has decoded since the window opened.
    ///
    /// The tests' handle on S12's central claim — a live gesture never touches the
    /// disk — and nothing else reads it: the count is a fact about the worker, and
    /// the window itself has no use for it. A superseded build's decodes count too,
    /// because the disk was touched all the same.
    pub fn decoded_sources(&self) -> u64 {
        self.imp().decoded.get()
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
                self.live(Command::SetCrop { slot, crop: fitted });
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

    fn live(&self, command: Command) {
        if self.imp().editor.borrow_mut().begin(command).is_ok() {
            self.canvas_widget().queue_draw();
            // The gesture is live, so this is the grid a gesture draws at: the
            // document is moving, and a frame that keeps up is worth more than a
            // sharp one. The release refines it (S12).
            let grid = self.gesture_aware_grid(self.resting_grid());
            self.request_decode(grid);
        }
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

    pub fn can_redo(&self) -> bool {
        self.imp().editor.borrow().can_redo()
    }

    pub fn select(&self, slot: Option<usize>) {
        let slot = slot.filter(|slot| *slot < self.document().template.slots.len());
        self.imp().selection.set(slot);
        self.refresh();
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
        self.live(Command::SetCrop { slot, crop: next });
        self.schedule_commit();
    }

    pub fn reset_framing(&self, slot: usize) {
        let _ = self.apply(Command::SetCrop {
            slot,
            crop: CropTransform::IDENTITY,
        });
    }

    pub fn clear_slot(&self, slot: usize) {
        let _ = self.apply(Command::SetSource { slot, source: None });
    }

    pub fn place_photo(&self, slot: usize, path: PathBuf) {
        let _ = self.apply(Command::SetSource {
            slot,
            source: Some(path),
        });
    }

    /// Files dropped on the canvas: the slot under the pointer first, then the
    /// slots after it, so a drop of five photos fills five slots in order. Slots
    /// that already hold a photo are skipped unless there is nothing else left,
    /// which is the rule that keeps a drop from silently replacing work.
    pub fn drop_files(&self, paths: Vec<PathBuf>, at: Option<usize>) {
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

    // ---- project ----------------------------------------------------------

    pub fn set_long_edge_mm(&self, mm: f64) {
        let aspect = self.document().template.aspect;
        let _ = self.apply(Command::SetCanvas {
            canvas: CanvasSpec::with_ratio(aspect, mm),
        });
    }

    pub fn set_template(&self, name: &str) {
        let Some(template) = templates::get(name) else {
            return;
        };
        let long_edge = {
            let doc = self.document();
            doc.canvas.width_mm.max(doc.canvas.height_mm)
        };
        let _ = self.apply(Command::SetTemplate {
            template: template.clone(),
            canvas: CanvasSpec::with_ratio(template.aspect, long_edge),
        });
        self.select(None);
    }

    pub fn new_document(&self) {
        match Editor::new(default_document()) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                self.select(None);
                self.refresh_document();
                self.set_title(Some(&gettext("Untitled collage")));
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
                        let _ = window.open_path(&path);
                    }
                }
            ),
        );
    }

    pub fn open_path(&self, path: &Path) -> Result<(), CoreError> {
        let project = Project::load(path)?;
        match Editor::from_project(project) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                self.select(None);
                self.requested_grid_reset();
                self.refresh_document();
                self.set_title(Some(&path.display().to_string()));
                Ok(())
            }
            Err(error) => {
                self.toast(&error.to_string());
                Err(error)
            }
        }
    }

    pub fn save(&self) {
        if self.imp().editor.borrow().path().is_none() {
            self.save_as();
            return;
        }
        self.save_now(None);
    }

    pub fn save_as(&self) {
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
                        window.save_now(Some(path));
                    }
                }
            ),
        );
    }

    /// Saves without a dialog, which is also what the tests and the main path
    /// walk use.
    pub fn save_to(&self, path: &Path) -> Result<PathBuf, CoreError> {
        let written = self.imp().editor.borrow_mut().save(Some(path))?;
        self.set_title(Some(&written.display().to_string()));
        self.refresh();
        Ok(written)
    }

    fn save_now(&self, path: Option<PathBuf>) {
        let written = self.imp().editor.borrow_mut().save(path.as_deref());
        match written {
            Ok(written) => {
                self.set_title(Some(&written.display().to_string()));
                self.refresh();
                self.toast(&fill(gettext("Saved {}"), &[file_name(&written)]));
            }
            Err(error) => self.toast(&error.to_string()),
        }
    }

    // ---- photo chooser ----------------------------------------------------

    pub fn choose_photo(&self, slot: usize) {
        let window = self.clone();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Photos")));
        for pattern in [
            "*.jpg", "*.jpeg", "*.png", "*.heic", "*.avif", "*.webp", "*.tif", "*.tiff",
        ] {
            filter.add_pattern(pattern);
        }
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

    pub fn export(&self) {
        let Some(path) = self.imp().export_path.borrow().clone() else {
            self.choose_export_path();
            return;
        };
        self.start_export(path);
    }

    pub fn choose_export_path(&self) {
        let window = self.clone();
        let format = self.export_settings().format;
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Images")));
        for pattern in ["*.jpg", "*.jpeg", "*.png", "*.tif", "*.tiff"] {
            filter.add_pattern(pattern);
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Export the collage"))
            .filters(&filters)
            .default_filter(&filter)
            .initial_name(suggested_export_name(self, format))
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
                        *window.imp().export_path.borrow_mut() = Some(path.clone());
                        window.show_export_settings();
                    }
                }
            ),
        );
    }

    /// What the export form asks for, with the path this window holds.
    ///
    /// The path is *not* asked for twice: when none has been chosen yet the
    /// suggestion is derived from the format, which is why this builds the
    /// settings in two steps rather than passing a placeholder into the dialog's
    /// own suggestion (that circularity was a real defect, found by
    /// `tests/mainpath.rs`).
    fn export_settings(&self) -> Settings {
        let chosen = self.imp().export_path.borrow().clone();
        let mut settings = match self.imp().sidebar.get() {
            Some(sidebar) => sidebar.settings(PathBuf::new()),
            None => Settings {
                size: Size::Dpi(DEFAULT_EXPORT_DPI),
                format: pixlay_imaging::encode::Format::Jpeg,
                chroma: pixlay_imaging::encode::Chroma::Full,
                path: PathBuf::new(),
            },
        };
        settings.path =
            chosen.unwrap_or_else(|| PathBuf::from(suggested_export_name(self, settings.format)));
        settings
    }

    /// Sets the export form's state, which is also what a test walks the
    /// background export with.
    pub fn set_export_settings(&self, settings: &Settings) {
        *self.imp().export_path.borrow_mut() = Some(settings.path.clone());
        if let Some(sidebar) = self.imp().sidebar.get() {
            sidebar.show_settings(settings);
        }
    }

    fn show_export_settings(&self) {
        let settings = self.export_settings();
        if let Some(sidebar) = self.imp().sidebar.get() {
            sidebar.show_settings(&settings);
        }
    }

    /// Exports on a worker thread, with the progress bar in the bottom bar.
    pub fn start_export(&self, path: PathBuf) {
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
        let report = move |event: Event| {
            let weak = weak.clone();
            glib::MainContext::default().invoke(move || {
                if let Some(window) = weak.upgrade() {
                    window.on_export_event(event);
                }
            });
        };
        *self.imp().export_path.borrow_mut() = Some(settings.path.clone());
        self.show_export_settings();
        export::spawn(doc, sources.paths, settings, report);
    }

    /// Exports synchronously; the same function the worker calls.
    pub fn export_to(&self, settings: &Settings) -> Result<Report, String> {
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

    fn on_export_event(&self, event: Event) {
        match event {
            Event::Progress(progress) => {
                let label = match progress {
                    Progress::Decoding { done, total } => {
                        fill(gettext("Preparing photo {} of {}"), &[done, total])
                    }
                    Progress::Rendering => gettext("Compositing…"),
                    Progress::Encoding => gettext("Writing the file…"),
                };
                self.set_progress(progress.fraction(), &label);
            }
            Event::Finished(result) => {
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
        let wanted = self.gesture_aware_grid(canvas::preferred_grid(aspect, width, height));
        let (current, _) = self.images();
        if wanted == current || self.imp().requested.get() == Some(wanted) {
            return;
        }
        self.request_decode(wanted);
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
        self.imp().images.replace((
            PixelSize {
                width: 1,
                height: 1,
            },
            Images::new(),
        ));
    }

    fn request_decode(&self, grid: PixelSize) {
        let sources = self.imp().editor.borrow().sources();
        let doc = self.display_document();
        let mut decoder = self.imp().decoder.borrow_mut();
        let Some(decoder) = decoder.as_mut() else {
            return;
        };
        let generation = decoder.request(&doc, sources.paths, grid);
        self.imp().generation.set(generation);
        self.imp().requested.set(Some(grid));
        self.imp().missing.replace(sources.missing);
        self.update_banner();
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
        let grid = self.imp().requested.get().unwrap_or(PixelSize {
            width: 1,
            height: 1,
        });
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

    /// Waits until no decode is outstanding, pumping the main context.
    ///
    /// This is the tests' handle on an asynchronous pipeline, and the same loop
    /// the widget's own draw runs in — the window has one thread, and this is it.
    pub fn wait_for_idle(&self, timeout: Duration) -> bool {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            if self.imp().requested.get().is_none() && !self.imp().exporting.get() {
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

    /// Rebuilds the pane, the title and the buttons from the document.
    pub fn refresh(&self) {
        if let Some(sidebar) = self.imp().sidebar.get() {
            sidebar.update(self);
        }
        self.show_export_settings();
        self.update_actions();
        self.update_title();
        self.update_banner();
        self.canvas_widget().queue_draw();
    }

    /// The same, plus new bitmaps: what every document edit calls.
    pub fn refresh_document(&self) {
        // A committed document is drawn at the resting grid: this is where a
        // gesture's coarse frame is refined, and where every edit made outside a
        // gesture lands.
        let grid = self.resting_grid();
        self.refresh();
        self.request_decode(grid);
    }

    fn update_actions(&self) {
        let state = {
            let editor = self.imp().editor.borrow();
            let doc = editor.doc();
            (
                editor.can_undo(),
                editor.can_redo(),
                self.selection().is_some(),
                self.selection()
                    .and_then(|slot| doc.cells.get(slot))
                    .is_some_and(|cell| cell.source.is_some()),
                doc.cells.iter().any(|cell| cell.source.is_some()),
            )
        };
        let (undo, redo, selected, _has_photo, has_any_photo) = state;
        for action in self.imp().actions.borrow().iter() {
            let enabled = match action.name().as_str() {
                "undo" => undo,
                "redo" => redo,
                "save" | "save-as" => true,
                "export" | "choose-export-path" => has_any_photo,
                "add-photo" | "clear-photo" | "reset-framing" => selected,
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
        self.set_title(Some(&title));
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
        if let Some(overlay) = self.imp().toast.get() {
            overlay.add_toast(adw::Toast::new(message));
        } else {
            glib::g_warning!("pixlay", "{message}");
        }
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

    /// The sidebar, for the tests and for the actions that put a value back.
    pub fn sidebar(&self) -> Option<&Sidebar> {
        self.imp().sidebar.get()
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

/// The primary menu: the actions that do not deserve a button.
fn main_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let file = gio::Menu::new();
    file.append(Some(&gettext("New collage")), Some("app.new"));
    file.append(Some(&gettext("Open…")), Some("app.open"));
    file.append(Some(&gettext("Save as…")), Some("win.save-as"));
    file.append(Some(&gettext("Export…")), Some("win.export"));
    menu.append_submenu(Some(&gettext("Collage")), &file);

    let help = gio::Menu::new();
    help.append(Some(&gettext("Keyboard shortcuts")), Some("app.shortcuts"));
    help.append(Some(&gettext("About Pixlay")), Some("app.about"));
    menu.append_submenu(Some(&gettext("Help")), &help);
    menu
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

/// The name the export dialog opens with: the project's own name, or the
/// template's, with the extension of the format that is selected.
fn suggested_export_name(window: &EditorWindow, format: pixlay_imaging::encode::Format) -> String {
    let doc = window.document();
    let extension = match format {
        pixlay_imaging::encode::Format::Png => "png",
        pixlay_imaging::encode::Format::Tiff => "tif",
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
