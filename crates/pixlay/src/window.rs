//! The window: the two stages, their actions, and everything that connects them.
//!
//! This is the only place in the GUI that knows about the document. The canvas
//! draws what it is asked to draw, the picker emits commands, and both go through
//! the methods here — which is also what makes the whole main path reachable from
//! a test without a pointer: `picker`, `open_document`, `place_photo`,
//! `export_to` are the same calls the widgets make.
//!
//! Since S13 the window is a **sequence of stages**, which is HIG's own shape for
//! a multi-step task (`patterns/nav`) and the 2026-09-22 ruling's answer to "no
//! parallel modes over one document":
//!
//! ```text
//! AdwToastOverlay                      one place for every toast
//!  └ AdwNavigationView
//!     ├ AdwNavigationPage "picker"     the folder, the grid, the tray (S13)
//!     └ AdwNavigationPage "editor"     the canvas, pushed by Next or Open…
//!        └ AdwToolbarView              header / progress / banner + canvas
//! ```
//!
//! Two rules the whole file obeys, both from `AGENTS.md`: a GTK object never
//! leaves the main thread (the decoding and encoding threads send plain data back
//! through `MainContext::invoke`), and nothing here touches a pixel — the canvas
//! hands the document to the renderer and the export hands it to
//! `pixlay-imaging`.
//!
//! Theming the two stages: the window's own title follows the visible stage, so a
//! header bar shows "Pick photos" on the picker and the document's name in the
//! editor.

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

use pixlay_core::{CollageDoc, Command, CoreError, CropTransform, PixelSize, Project, templates};
use pixlay_imaging::gesture_grid;
use pixlay_render::Images;

use crate::a11y;
use crate::canvas::{self, Gesture};
use crate::decode::{Decoder, Reply};
use crate::export::{self, Event, Progress, Report, Settings};
use crate::i18n::{fill, gettext, ngettext};
use crate::picker::Picker;
use crate::state::Editor;
use crate::thumbs;

/// The template a new document starts from: 4:3 like an album page, five slots,
/// so the main path starts with a layout that does not need ten photos.
pub const DEFAULT_TEMPLATE: &str = "mosaic-5-hero";

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

/// How long a live gesture waits for quiet before it becomes an undo step.
///
/// A slider has no "drag ended" signal, so the commit is triggered by the value
/// being still: long enough that a slow drag does not produce three commands,
/// short enough that the undo a user reaches for next is the gesture they just
/// finished.
pub const COMMIT_QUIET: Duration = Duration::from_millis(250);

/// Which stage of the main path the window is showing (S13).
///
/// The stages are a sequence, not two modes over one document: the picker is the
/// root page and the editor is pushed on top of it, so this is simply which page
/// the navigation view is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Stages 1–2: the folder, the grid and the tray.
    Picker,
    /// Stages 3–7: the document itself.
    Editor,
}

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
        /// The two stages, and the picker behind the first of them (S13).
        pub pages: OnceCell<adw::NavigationView>,
        pub picker_page: OnceCell<adw::NavigationPage>,
        pub editor_page: OnceCell<adw::NavigationPage>,
        pub picker: OnceCell<Rc<Picker>>,
        /// The picker's tile worker: one thread, many small pictures.
        pub thumbs: OnceCell<Rc<thumbs::Thumbs>>,
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
                pages: OnceCell::new(),
                picker_page: OnceCell::new(),
                editor_page: OnceCell::new(),
                picker: OnceCell::new(),
                thumbs: OnceCell::new(),
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
    CollageDoc::new(template)
}

impl EditorWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn build(&self) {
        let imp = self.imp();

        // ---- the editor page ------------------------------------------------
        // The header of the stage the document lives in: history, saving and the
        // export. `AdwNavigationView` adds the back button by itself, because this
        // page is pushed on top of the picker (S13).
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

        // ---- the editor page's content --------------------------------------
        let canvas = canvas::build(self);
        // The banner's action is named once, here: its button exists from the
        // start, and a control with no label is a control a screen reader cannot
        // announce (`docs/HIG-REVIEW.md`, section 1).
        let banner = adw::Banner::new("");
        banner.set_button_label(Some(&gettext("Find it…")));
        let editor_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        editor_body.append(&banner);
        editor_body.append(&canvas);
        let editor_view = adw::ToolbarView::new();
        editor_view.add_top_bar(&header);
        editor_view.add_bottom_bar(&progress_revealer);
        editor_view.set_content(Some(&editor_body));
        let editor_page =
            adw::NavigationPage::with_tag(&editor_view, &gettext("Collage"), "editor");

        // ---- the picker page ------------------------------------------------
        let picker = Picker::build(self);
        let picker_page =
            adw::NavigationPage::with_tag(&picker.root(), &gettext("Pick photos"), "picker");

        // ---- the shell ------------------------------------------------------
        // A sequence of stages rather than two modes over one document (the
        // 2026-09-22 ruling): the picker is the root, the editor is pushed on
        // Next, and Back is how a user returns to the photos.
        let pages = adw::NavigationView::new();
        pages.add(&picker_page);
        pages.add(&editor_page);
        // One toast surface for both stages: the picker reports a refused pick
        // and the editor reports a save or an export, and neither outlives the
        // other.
        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&pages));

        self.set_content(Some(&toast));
        self.set_title(Some(&gettext("Untitled collage")));
        self.set_default_size(1100, 760);
        // The minimum the layout is designed for (HIG `guidelines/adaptive`): the
        // picker's grid needs its column and the editor's canvas its own space,
        // and below this the window would be showing neither.
        self.set_size_request(560, 420);

        imp.canvas.set(canvas).ok();
        imp.pages.set(pages).ok();
        imp.picker_page.set(picker_page).ok();
        imp.editor_page.set(editor_page).ok();
        imp.banner.set(banner.clone()).ok();
        imp.toast.set(toast).ok();
        imp.progress.set(progress).ok();
        imp.progress_revealer.set(progress_revealer).ok();
        imp.picker.set(picker).ok();

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

        // ---- the picker's tiles ---------------------------------------------
        // A second worker, for a different question: the decoder above builds one
        // document's bitmaps for one grid, while this one answers many independent
        // "what does this file look like" requests for the picker's grid.
        let sender = glib::SendWeakRef::from(self.downgrade());
        let thumbs = thumbs::Thumbs::spawn(move |reply| {
            let sender = sender.clone();
            glib::MainContext::default().invoke(move || {
                if let Some(window) = sender.upgrade() {
                    window.on_thumb(reply);
                }
            });
        });
        imp.thumbs.set(Rc::new(thumbs)).ok();

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

        self.insert_action_group("win", Some(&group));
        *self.imp().actions.borrow_mut() = actions;
    }

    // ---- what the canvas asks for -----------------------------------------

    pub fn canvas_widget(&self) -> gtk::DrawingArea {
        self.imp().canvas.get().expect("the canvas exists").clone()
    }

    /// The picker, for the tests and for the widgets that call into it.
    pub fn picker(&self) -> Option<Rc<Picker>> {
        self.imp().picker.get().cloned()
    }

    /// The picker's tile worker, if the window has one.
    pub fn thumbs(&self) -> Option<Rc<thumbs::Thumbs>> {
        self.imp().thumbs.get().cloned()
    }

    /// One tile or preview arrived from the picker's worker.
    pub fn on_thumb(&self, reply: thumbs::Reply) {
        if let Some(picker) = self.imp().picker.get() {
            picker.on_reply(reply);
        }
    }

    /// Whether the picker's stage is the one on screen.
    pub fn stage(&self) -> Stage {
        match self.imp().pages.get() {
            Some(pages) if pages.visible_page_tag().as_deref() == Some("editor") => Stage::Editor,
            _ => Stage::Picker,
        }
    }

    /// Shows the picker: the flow's first stage, and where Back returns to.
    pub fn show_picker(&self) {
        let imp = self.imp();
        if let (Some(pages), Some(page)) = (imp.pages.get(), imp.picker_page.get()) {
            pages.pop_to_page(page);
        }
        self.refresh();
    }

    /// Pushes the editor's stage (the picker stays below it).
    fn show_editor(&self) {
        let imp = self.imp();
        if let (Some(pages), Some(page)) = (imp.pages.get(), imp.editor_page.get())
            && pages.visible_page() != Some(page.clone())
        {
            pages.push(page);
        }
    }

    /// Opens `doc` in the editor's stage: what Next and opening a project both do.
    pub fn open_document(&self, doc: CollageDoc) {
        match Editor::new(doc) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                self.select(None);
                self.requested_grid_reset();
                self.set_title(Some(&gettext("Untitled collage")));
                self.show_editor();
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

    pub fn set_template(&self, name: &str) {
        let Some(template) = templates::get(name) else {
            return;
        };
        let _ = self.apply(Command::SetTemplate {
            template: template.clone(),
        });
        self.select(None);
    }

    /// A new collage, which is the flow's first stage again.
    ///
    /// `Ctrl+N` used to hand the user an empty sheet of the default template; on
    /// the re-routed path (ruling 12) a new collage starts by picking photos, so
    /// this resets the document and shows the picker.
    pub fn new_document(&self) {
        match Editor::new(default_document()) {
            Ok(editor) => {
                *self.imp().editor.borrow_mut() = editor;
                if let Some(picker) = self.picker() {
                    picker.clear_selection();
                }
                self.select(None);
                self.set_title(Some(&gettext("Untitled collage")));
                self.show_picker();
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
                self.show_editor();
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
        let chosen = self.imp().export.borrow().path.clone();
        if chosen.as_os_str().is_empty() {
            self.choose_export_path();
            return;
        }
        self.start_export(chosen);
    }

    /// Asks where the export goes and starts it.
    ///
    /// Since ruling 18 removed the pane that held the export form, choosing a
    /// file is the last question the window asks, so it *starts* the export rather
    /// than filling a row that no longer exists.
    pub fn choose_export_path(&self) {
        let window = self.clone();
        let format = self.export_settings().format;
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Images")));
        for pattern in ["*.jpg", "*.jpeg", "*.png"] {
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
                        window.imp().export.borrow_mut().path = path.clone();
                        window.start_export(path);
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
    ///
    /// Since ruling 18 removed the utility pane, this state *is* the export form:
    /// the format and the one quality option live here, and S15's `Export…` dialog
    /// is the rows over them.
    pub fn export_settings(&self) -> Settings {
        let mut settings = self.imp().export.borrow().clone();
        if settings.path.as_os_str().is_empty() {
            settings.path = PathBuf::from(suggested_export_name(self, settings.format));
        }
        settings
    }

    /// Sets the export form's state, which is also what a test walks the
    /// background export with.
    pub fn set_export_settings(&self, settings: &Settings) {
        *self.imp().export.borrow_mut() = settings.clone();
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
        *self.imp().export.borrow_mut() = settings.clone();
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

    /// Waits until every listed photo has a tile or a reported refusal.
    ///
    /// The picker's counterpart of [`wait_for_idle`](Self::wait_for_idle), and the
    /// same shape: pump the context the worker delivers into, and stop when there
    /// is nothing left to arrive.
    pub fn wait_for_tiles(&self, timeout: Duration) -> bool {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + timeout;
        loop {
            while context.pending() {
                context.iteration(false);
            }
            match self.picker() {
                Some(picker) if picker.tiles_built() + picker.failures().len() >= picker.len() => {
                    return true;
                }
                None => return false,
                _ => {}
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    /// Waits until the preview pane has decoded a photo.
    pub fn wait_for_preview(&self, timeout: Duration) -> bool {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + timeout;
        loop {
            while context.pending() {
                context.iteration(false);
            }
            match self.picker() {
                Some(picker) if picker.preview_pixels().is_some() => return true,
                None => return false,
                _ => {}
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    // ---- presentation -----------------------------------------------------

    /// Rebuilds the title and the buttons from the document and the stage.
    pub fn refresh(&self) {
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
                doc.cells.iter().any(|cell| cell.source.is_some()),
            )
        };
        let (undo, redo, selected, has_any_photo) = state;
        // The document's own actions belong to the stage that shows the document:
        // Save with the picker on screen would save a collage the user has not
        // finished choosing (`AGENTS.md`: a document edit only happens through the
        // editor's own page).
        let editing = self.stage() == Stage::Editor;
        for action in self.imp().actions.borrow().iter() {
            let enabled = match action.name().as_str() {
                "undo" => undo,
                "redo" => redo,
                "save" | "save-as" => editing,
                "export" => editing && has_any_photo,
                "add-photo" | "clear-photo" | "reset-framing" => editing && selected,
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
        // The visible page decides what the header bar and the window are called:
        // the picker is a titled page of its own, and the editor's page shows the
        // document's name (S13).
        match self
            .imp()
            .pages
            .get()
            .and_then(|pages| pages.visible_page_tag())
        {
            Some(tag) if tag.as_str() == "editor" => {
                if let Some(page) = self.imp().editor_page.get() {
                    page.set_title(&title);
                }
                self.set_title(Some(&title));
            }
            _ => self.set_title(Some(&gettext("Pick photos"))),
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
