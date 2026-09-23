//! The canvas: the document, drawn by the single renderer, plus the gestures
//! that edit it.
//!
//! What this module is *not*: a second renderer. The sheet is painted by
//! [`pixlay_render::draw`] — the same call the CLI and the export make — into the
//! widget's own cairo context, so "what the window shows" and "what gets
//! exported" are the same code by construction (`AGENTS.md`, "Hard constraints").
//! Everything this file adds is drawn *over* that: the selection outline and the
//! straightening guides, which are interface, not content.
//!
//! # The grid
//!
//! A preview needs its own pixel grid, and it is the widget's: [`preferred_grid`]
//! is the largest canvas-aspect grid that fits the widget inside [`MARGIN`]. The
//! bitmaps are resampled for exactly that grid by the background decoder, so at
//! rest `scale == 1.0` and the canvas only blits and clips. While the widget has
//! just changed size and the new bitmaps are still being prepared,
//! [`placement`] draws the previous grid with a uniform scale — a *preview* scale
//! in the sense `Target::scale` already has, applied to the whole canvas, never a
//! photo stretched inside its slot; it disappears the moment the decode lands.
//!
//! # Gestures
//!
//! Every gesture edits the *fitted* crop of the selected slot, never the raw
//! request: the fit is what the user is looking at ([`pixlay_core`]'s "a crop is
//! a request; what is drawn is its fit"), so dragging a photo by a hundred pixels
//! moves it by a hundred pixels rather than by a hundred pixels scaled by
//! whatever zoom was stored. Each motion sends a whole command through
//! [`EditorWindow::gesture`], which keeps it pending; the command that reaches the
//! undo stack is the one from the end of the gesture. While one is pending the
//! canvas draws at a coarser grid ([`pixlay_imaging::gesture_grid`]) and the release
//! refines it (S12); a control that produces a single finished step sends
//! [`Gesture::Step`] instead, and is drawn at the resting grid, because one frame
//! the user is meant to look at is worth the pixels.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use pixlay_core::{CollageDoc, CropTransform, PixelSize, Point, Slot};
use pixlay_imaging::GESTURE_STEP_DEG;
use pixlay_render::{Images, RenderError, Target, draw};

use crate::a11y;
use crate::i18n::gettext;
use crate::window::EditorWindow;

/// Space between the sheet and the edge of the widget, in device pixels.
///
/// It is the canvas view's own placement of its content, not styling: no style
/// class or CSS variable describes "how far the paper sits from the pane", and it
/// deliberately does not come from the theme (a sheet of paper has the same
/// margin in dark and light mode).
pub const MARGIN: f64 = 12.0;

/// How the sheet is placed inside a widget of a given size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The canvas pixel grid the bitmaps are sized for. This is the space `draw`
    /// places slots in.
    pub grid: PixelSize,
    /// The sheet's top-left corner in the widget, in device pixels.
    pub origin: (f64, f64),
    /// Device pixels per canvas pixel. `1.0` unless the widget changed size since
    /// the bitmaps were decoded.
    pub scale: f64,
}

impl Placement {
    pub fn width(&self) -> f64 {
        f64::from(self.grid.width) * self.scale
    }

    pub fn height(&self) -> f64 {
        f64::from(self.grid.height) * self.scale
    }

    /// A normalized canvas point in widget coordinates, if it is on the sheet.
    pub fn to_widget(&self, point: Point) -> (f64, f64) {
        (
            self.origin.0 + point.x * self.width(),
            self.origin.1 + point.y * self.height(),
        )
    }

    /// A widget point in normalized canvas coordinates, if it is on the sheet.
    pub fn to_canvas(&self, x: f64, y: f64) -> Option<Point> {
        let (width, height) = (self.width(), self.height());
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let (nx, ny) = ((x - self.origin.0) / width, (y - self.origin.1) / height);
        if !(0.0..=1.0).contains(&nx) || !(0.0..=1.0).contains(&ny) {
            return None;
        }
        Some(Point::new(nx, ny))
    }
}

/// The grid a widget of this size asks for: the largest canvas-aspect grid inside
/// the margin.
///
/// The widget is the request's unit, so a window that grows asks for more pixels
/// and the decoder resamples for them: a preview is a real render at a smaller
/// size, not a big render shrunk by Cairo (`docs/CONTRACT.md` §5, `--preview-px`).
pub fn preferred_grid(canvas_aspect: f64, width: i32, height: i32) -> PixelSize {
    let available_width = (f64::from(width) - 2.0 * MARGIN).max(1.0);
    let available_height = (f64::from(height) - 2.0 * MARGIN).max(1.0);
    let (w, h) = if available_width / available_height > canvas_aspect {
        (available_height * canvas_aspect, available_height)
    } else {
        (available_width, available_width / canvas_aspect)
    };
    PixelSize {
        width: (w.round() as i32).max(1),
        height: (h.round() as i32).max(1),
    }
}

/// Where the grid `grid` sits in a widget of this size.
pub fn placement(grid: PixelSize, width: i32, height: i32) -> Placement {
    let target = preferred_grid(grid.aspect(), width, height);
    let scale = f64::from(target.width) / f64::from(grid.width);
    let (w, h) = (
        f64::from(grid.width) * scale,
        f64::from(grid.height) * scale,
    );
    Placement {
        grid,
        origin: ((f64::from(width) - w) / 2.0, (f64::from(height) - h) / 2.0),
        scale,
    }
}

/// The slot a widget point falls in (`Template::slot_at`, in widget space).
pub fn slot_at(doc: &CollageDoc, placement: &Placement, x: f64, y: f64) -> Option<usize> {
    doc.template.slot_at(placement.to_canvas(x, y)?)
}

/// Everything one canvas frame needs, so that the draw function and a test are
/// the same call.
pub struct View<'a> {
    pub doc: &'a CollageDoc,
    pub images: &'a Images,
    /// The grid `images` were decoded for.
    pub grid: PixelSize,
    pub selection: Option<usize>,
    /// Draw the straightening guides (true while a rotation is being edited).
    pub guides: bool,
    /// The theme's text colour, which is what the overlays are drawn with: a
    /// hard-coded grey would fail in high contrast mode, where the interface is
    /// black on white or white on black and the canvas is white either way.
    pub foreground: (f64, f64, f64),
}

/// Paints the document and its overlays into `ctx`, sized `width` x `height`.
pub fn render(
    ctx: &cairo::Context,
    view: &View<'_>,
    width: i32,
    height: i32,
) -> Result<(), RenderError> {
    let View {
        doc,
        images,
        grid,
        selection,
        guides,
        foreground,
    } = *view;
    let placement = placement(grid, width, height);

    // A frame around the sheet, so a white collage on a white pane still reads as
    // a page rather than as an empty window. Drawn under the sheet's own pixels.
    ctx.save()?;
    ctx.rectangle(
        placement.origin.0,
        placement.origin.1,
        placement.width(),
        placement.height(),
    );
    ctx.set_source_rgba(foreground.0, foreground.1, foreground.2, 0.25);
    ctx.set_line_width(1.0);
    ctx.stroke()?;
    ctx.restore()?;

    // The document itself: the single `draw`, at the widget's own grid.
    //
    // **Clipped to the sheet**, because `draw` paints its backdrop with
    // `Operator::Source` over the whole of whatever it is drawn into — that is
    // what makes an export opaque to its own edges. Left unclipped the widget
    // would be filled with the *frame's* colour (white by default) instead of
    // showing the window's own background around the sheet, which on a dark
    // desktop reads as a light app (measured 2026-09-23: the whole 1100x575
    // canvas widget read `#FFFFFF` outside the sheet before this clip, and the
    // theme's `#222226` after it). The sheet is still opaque and still
    // style-independent — the clip changes what surrounds it, not what it is.
    ctx.save()?;
    ctx.rectangle(
        placement.origin.0,
        placement.origin.1,
        placement.width(),
        placement.height(),
    );
    ctx.clip();
    ctx.translate(placement.origin.0, placement.origin.1);
    draw(
        doc,
        images,
        &Target {
            ctx,
            scale: placement.scale,
            canvas_px: grid,
            band: None,
        },
    )?;
    ctx.restore()?;

    // Overlays, in device space and over the finished sheet.
    if guides {
        draw_guides(ctx, &placement, foreground)?;
    }
    if let Some(slot) = selection.and_then(|slot| doc.template.slots.get(slot)) {
        draw_outline(ctx, &placement, slot, foreground)?;
    }

    Ok(())
}

/// The straightening guides: thirds both ways, so a horizon can be lined up
/// against a line instead of by feel.
fn draw_guides(
    ctx: &cairo::Context,
    placement: &Placement,
    foreground: (f64, f64, f64),
) -> Result<(), cairo::Error> {
    ctx.save()?;
    ctx.set_source_rgba(foreground.0, foreground.1, foreground.2, 0.5);
    ctx.set_line_width(1.0);
    for fraction in [1.0 / 3.0, 2.0 / 3.0] {
        let x = placement.origin.0 + placement.width() * fraction;
        ctx.move_to(x, placement.origin.1 + 2.0);
        ctx.line_to(x, placement.origin.1 + placement.height() - 2.0);
        let y = placement.origin.1 + placement.height() * fraction;
        ctx.move_to(placement.origin.0 + 2.0, y);
        ctx.line_to(placement.origin.0 + placement.width() - 2.0, y);
    }
    ctx.stroke()?;
    ctx.restore()?;
    Ok(())
}

fn draw_outline(
    ctx: &cairo::Context,
    placement: &Placement,
    slot: &Slot,
    foreground: (f64, f64, f64),
) -> Result<(), cairo::Error> {
    ctx.save()?;
    ctx.set_source_rgba(foreground.0, foreground.1, foreground.2, 0.9);
    ctx.set_line_width(2.0);
    for (index, point) in slot.outline.points.iter().enumerate() {
        let (x, y) = placement.to_widget(*point);
        if index == 0 {
            ctx.move_to(x, y);
        } else {
            ctx.line_to(x, y);
        }
    }
    ctx.close_path();
    ctx.stroke()?;
    ctx.restore()?;
    Ok(())
}

/// What a gesture asks the window to do to the selected slot.
///
/// The canvas owns the arithmetic (widget pixels to slot-relative offsets) and the
/// window owns the document, which is why this carries a finished transform
/// rather than a delta: nothing downstream has to remember where the gesture
/// started.
pub enum Gesture {
    /// A whole new transform, already fitted to what the user is looking at.
    Crop { slot: usize, crop: CropTransform },
    /// The same, from a control that produces one *finished* step rather than a
    /// stream of them (the keyboard, the zoom spin row): it is committed at once
    /// and the canvas is never coarsened for it (S12).
    Step { slot: usize, crop: CropTransform },
    /// The gesture ended: commit the pending command.
    End,
}

/// Builds the canvas widget: the drawing area, its gestures, and its keyboard.
///
/// The `+` buttons over the empty cells are a sibling widget
/// ([`EmptyCells`]), stacked by the window's overlay; the draw function asks the
/// window for them, because the draw is the one moment that knows the widget's own
/// size and the current document at once.
pub fn build(window: &EditorWindow) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_focusable(true);
    area.set_hexpand(true);
    area.set_vexpand(true);
    a11y::label(&area, &gettext("Collage canvas"));
    area.set_tooltip_text(Some(&gettext(
        "Drag to move the photo, scroll to zoom, Ctrl+scroll to straighten, \
         Ctrl+Shift+Left/Right to swap two photos",
    )));

    area.set_draw_func(glib::clone!(
        #[weak]
        window,
        move |area, ctx, width, height| {
            let (grid, images) = window.images();
            let rgba = area.color();
            let foreground = (
                f64::from(rgba.red()),
                f64::from(rgba.green()),
                f64::from(rgba.blue()),
            );
            let doc = window.display_document();
            let view = View {
                doc: &doc,
                images: &images,
                grid,
                selection: window.selection(),
                guides: window.guides(),
                foreground,
            };
            if let Err(error) = render(ctx, &view, width, height) {
                glib::g_warning!("pixlay", "the canvas could not be drawn: {error}");
            }
            window.request_grid_for(width, height);
        }
    ));

    add_click(&area, window);
    add_drag(&area, window);
    add_scroll(&area, window);
    add_keys(&area, window);
    add_drop(&area, window);
    area
}

/// Press selects; a second press acts on the slot (photo chooser or reframing).
fn add_click(area: &gtk::DrawingArea, window: &EditorWindow) {
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    click.connect_pressed(glib::clone!(
        #[weak]
        window,
        #[weak]
        area,
        move |gesture: &gtk::GestureClick, presses: i32, x: f64, y: f64| {
            let slot = window.slot_at_widget(x, y);
            window.select(slot);
            let Some(slot) = slot else {
                return;
            };
            if presses == 2 {
                // The gesture a photo editor has always had: an empty slot asks
                // for a photo, an occupied one goes back to its whole photo.
                if window.document().cells[slot].source.is_none() {
                    window.choose_photo(slot);
                } else {
                    window.reset_framing(slot);
                }
                // Keep the double click from also starting a drag.
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
            area.grab_focus();
        }
    ));
    area.add_controller(click);
}

/// Dragging inside the selected slot pans the photo.
fn add_drag(area: &gtk::DrawingArea, window: &EditorWindow) {
    let drag = gtk::GestureDrag::new();
    drag.set_button(gdk::BUTTON_PRIMARY);
    let base: Rc<RefCell<Option<(usize, CropTransform)>>> = Rc::new(RefCell::new(None));

    drag.connect_drag_begin(glib::clone!(
        #[weak]
        window,
        #[strong]
        base,
        move |_gesture: &gtk::GestureDrag, x: f64, y: f64| {
            let Some(slot) = window.slot_at_widget(x, y) else {
                return;
            };
            window.select(Some(slot));
            *base.borrow_mut() = window.fitted_crop(slot).map(|crop| (slot, crop));
        }
    ));

    drag.connect_drag_update(glib::clone!(
        #[weak]
        window,
        #[strong]
        base,
        move |_gesture: &gtk::GestureDrag, offset_x: f64, offset_y: f64| {
            let borrowed = base.borrow();
            let Some((slot, crop)) = *borrowed else {
                return;
            };
            let Some((step_x, step_y)) = window.slot_extent(slot) else {
                return;
            };
            // Offsets are in slot widths and heights (the document's unit), and a
            // drag is in device pixels, so the conversion is through the slot's own
            // size on screen.
            let (width, height) = window.sheet_size();
            let next = CropTransform {
                offset: (
                    crop.offset.0 + offset_x / (step_x * width),
                    crop.offset.1 + offset_y / (step_y * height),
                ),
                ..crop
            };
            window.gesture(Gesture::Crop { slot, crop: next });
        }
    ));

    drag.connect_drag_end(glib::clone!(
        #[weak]
        window,
        #[strong]
        base,
        move |_gesture: &gtk::GestureDrag, _offset_x: f64, _offset_y: f64| {
            base.borrow_mut().take();
            window.gesture(Gesture::End);
        }
    ));

    area.add_controller(drag);
}

/// Scrolling zooms the selected slot; Ctrl+scrolling straightens it.
fn add_scroll(area: &gtk::DrawingArea, window: &EditorWindow) {
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(glib::clone!(
        #[weak]
        window,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |controller: &gtk::EventControllerScroll, _dx: f64, dy: f64| -> glib::Propagation {
            let Some(slot) = window.selection() else {
                return glib::Propagation::Proceed;
            };
            let Some(crop) = window.fitted_crop(slot) else {
                return glib::Propagation::Proceed;
            };
            // One notch per event; the direction follows the wheel, which GTK
            // reports as "up is negative".
            let up = dy < 0.0;
            let control = controller
                .current_event_state()
                .contains(gdk::ModifierType::CONTROL_MASK);
            let next = if control {
                let step = if up {
                    GESTURE_STEP_DEG
                } else {
                    -GESTURE_STEP_DEG
                };
                // The angle is free (S11): no cap, and the value is wrapped into
                // (-180, 180] so a long spin cannot walk the number away.
                CropTransform {
                    rotation_deg: (crop.rotation_deg + step),
                    ..crop
                }
                .normalized()
            } else {
                let factor = if up { 1.06 } else { 1.0 / 1.06 };
                CropTransform {
                    zoom: crop.zoom * factor,
                    ..crop
                }
            };
            window.gesture(Gesture::Crop { slot, crop: next });
            glib::Propagation::Stop
        }
    ));
    scroll.connect_scroll_end(glib::clone!(
        #[weak]
        window,
        move |_controller: &gtk::EventControllerScroll| {
            window.gesture(Gesture::End);
        }
    ));
    area.add_controller(scroll);
}

/// The keyboard path for the whole canvas: the main path must be walkable without
/// a pointer (`AGENTS.md`, "GNOME HIG").
fn add_keys(area: &gtk::DrawingArea, window: &EditorWindow) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        window,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_controller: &gtk::EventControllerKey,
              key: gdk::Key,
              _code: u32,
              state: gdk::ModifierType|
              -> glib::Propagation {
            let Some(slot) = window.selection() else {
                // With nothing selected the canvas has nothing to edit; Tab still
                // reaches every control in the pane.
                return glib::Propagation::Proceed;
            };
            // Swapping two cells is about the layout rather than about one photo's
            // framing, so it is answered before the crop is read: an *empty* cell
            // has no crop and is still a legal half of a swap (the whole point of
            // `+` is that the new cell starts empty).
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
            let control = state.contains(gdk::ModifierType::CONTROL_MASK);
            if control && shift {
                let direction = match key {
                    gdk::Key::Left => (-1, 0),
                    gdk::Key::Right => (1, 0),
                    gdk::Key::Up => (0, -1),
                    gdk::Key::Down => (0, 1),
                    _ => (0, 0),
                };
                if direction != (0, 0) {
                    window.swap_towards(slot, direction);
                    return glib::Propagation::Stop;
                }
            }
            let Some(crop) = window.fitted_crop(slot) else {
                return glib::Propagation::Proceed;
            };
            let coarse = control;
            let step = if coarse { 0.1 } else { 0.02 };
            let next = match key {
                gdk::Key::Left => Some(CropTransform {
                    offset: (crop.offset.0 - step, crop.offset.1),
                    ..crop
                }),
                gdk::Key::Right => Some(CropTransform {
                    offset: (crop.offset.0 + step, crop.offset.1),
                    ..crop
                }),
                gdk::Key::Up => Some(CropTransform {
                    offset: (crop.offset.0, crop.offset.1 - step),
                    ..crop
                }),
                gdk::Key::Down => Some(CropTransform {
                    offset: (crop.offset.0, crop.offset.1 + step),
                    ..crop
                }),
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => Some(CropTransform {
                    zoom: crop.zoom * 1.06,
                    ..crop
                }),
                gdk::Key::minus | gdk::Key::KP_Subtract => Some(CropTransform {
                    zoom: crop.zoom / 1.06,
                    ..crop
                }),
                gdk::Key::_0 | gdk::Key::KP_0 => Some(CropTransform::IDENTITY),
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    if window.document().cells[slot].source.is_none() {
                        window.choose_photo(slot);
                    }
                    return glib::Propagation::Stop;
                }
                gdk::Key::Delete => {
                    window.clear_slot(slot);
                    return glib::Propagation::Stop;
                }
                _ => None,
            };
            let Some(crop) = next else {
                return glib::Propagation::Proceed;
            };
            // A key press is one finished step, not a gesture in flight: it is
            // committed — and drawn — at the resting grid.
            window.gesture(Gesture::Step { slot, crop });
            glib::Propagation::Stop
        }
    ));
    area.add_controller(keys);
}

/// The `+` controls over the empty cells (S14b).
///
/// **Real GTK buttons over the canvas rather than a glyph drawn into it** (ruling
/// 9): `tests/hig.rs` walks the widget tree for accessible names and the Tab order,
/// and a cairo-drawn `+` is invisible to both. They are the visible control the
/// ruling asked for — `+` grows the layout, and a *cell* that is empty is what asks
/// for a photo — so this is where "click the empty cell to give it a picture" lives
/// for a pointer.
///
/// **Each button is a child of the canvas's own `GtkOverlay`**, placed by its own
/// margins. That is deliberate: a `GtkFixed` holding them measures only its
/// children, so a document whose empty cells come and go leaves the container
/// 0x0 — and GTK snapshots an unallocated child with a warning (measured
/// 2026-09-23). An overlay child is always allocated the overlay's own area, so a
/// button that is shown on one frame is positioned and allocated on that same
/// frame, whatever the document did.
///
/// **One button per slot is built once**, at construction, and shown or hidden as
/// the document changes. Nine is the format's own slot ceiling, so the set is
/// complete.
pub struct EmptyCells {
    overlay: gtk::Overlay,
    /// One button per slot index, built at construction and never replaced.
    buttons: Vec<gtk::Button>,
}

impl EmptyCells {
    /// Builds the overlay around `canvas`, with its nine hidden buttons.
    pub fn new(window: &EditorWindow, canvas: &gtk::DrawingArea) -> Self {
        let overlay = gtk::Overlay::builder().child(canvas).build();
        let buttons: Vec<gtk::Button> = (0..pixlay_core::MAX_SLOTS)
            .map(|slot| {
                let button = empty_cell_button(window, slot);
                button.set_visible(false);
                overlay.add_overlay(&button);
                button
            })
            .collect();
        Self { overlay, buttons }
    }

    /// The widget the editor page appends: the canvas, with the buttons over it.
    pub fn root(&self) -> gtk::Overlay {
        self.overlay.clone()
    }

    /// The canvas inside the overlay: every caller in the window asks for a
    /// `GtkDrawingArea` (its `color()`, its size, its `queue_draw`), and the overlay
    /// adds nothing to any of those.
    pub fn canvas(&self) -> gtk::DrawingArea {
        self.overlay
            .child()
            .and_then(|child| child.downcast::<gtk::DrawingArea>().ok())
            .expect("the overlay's child is the canvas this was built with")
    }

    /// One cell's `+` button, for the tests and for the position sync.
    pub fn button(&self, slot: usize) -> Option<gtk::Button> {
        self.buttons.get(slot).cloned()
    }

    /// Puts every `+` over its own empty cell, and hides the rest (S14b).
    ///
    /// **Called from the window's `refresh`, never from a draw or a snapshot.**
    /// Showing a widget changes the tree, and a tree that changes while GTK is
    /// walking it produces an unallocated child in the very frame it appears in
    /// (measured 2026-09-23: "Trying to snapshot GtkButton … without a current
    /// allocation" when this ran inside `canvas::build`'s draw function). The window
    /// calls it after an edit and on a resize, which is exactly when the answer can
    /// change, and GTK then has a layout pass to allocate what it revealed.
    ///
    /// The size is the canvas's own allocation: `placement` is the one description
    /// of where the sheet is, and the canvas is what the placement is measured on.
    ///
    /// A button past the document's own cell count is hidden — a layout with fewer
    /// cells leaves the rest of them with nowhere to be. A slot is also hidden when
    /// its cell holds a photo: an occupied cell is dragged and clicked to reframe,
    /// and a `+` on top of a photo would take that press for itself.
    pub fn sync(&self, window: &EditorWindow) {
        let canvas = self.canvas();
        self.sync_in(window, canvas.width(), canvas.height());
    }

    /// [`sync`](Self::sync) against an explicit size, which is what a test can drive
    /// without an allocation.
    pub fn sync_in(&self, window: &EditorWindow, width: i32, height: i32) {
        let doc = window.document();
        let (grid, _) = window.images();
        let placement = placement(grid, width, height);
        // One cell tall/wide enough to hit: HIG `guidelines/pointer-touch` asks for
        // 24x24 at least, and 32 is a comfortable pointer target at the cell sizes
        // the library ships.
        const SIZE: f64 = 32.0;
        for (slot, button) in self.buttons.iter().enumerate() {
            let empty = doc
                .cells
                .get(slot)
                .is_some_and(|cell| cell.source.is_none());
            let Some(geometry) = doc.template.slots.get(slot).filter(|_| empty) else {
                button.set_visible(false);
                continue;
            };
            let box_ = geometry.outline.bbox();
            let centre = placement.to_widget(Point::new(
                (box_.x0 + box_.x1) / 2.0,
                (box_.y0 + box_.y1) / 2.0,
            ));
            // The overlay aligns a child to its start corner and lets its margins
            // place it, so the cell's centre in device pixels is the margin the
            // button needs: rounded, because a margin is an integer.
            button.set_margin_start((centre.0 - SIZE / 2.0).round() as i32);
            button.set_margin_top((centre.1 - SIZE / 2.0).round() as i32);
            button.set_visible(true);
        }
    }
}

/// One cell's `+`: the cell has no photo, and this is how a pointer gives it one.
fn empty_cell_button(window: &EditorWindow, slot: usize) -> gtk::Button {
    let label = gettext("Add a photo");
    let button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(&label)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .width_request(32)
        .height_request(32)
        .build();
    // `osd` is the platform's own class for a control over content, which is what
    // this is: hard-coded colours would fail in high contrast mode, and the sheet
    // under the button is the user's own photo, not the theme (`AGENTS.md`,
    // "GNOME HIG": styling uses libadwaita's classes and nothing else).
    button.add_css_class("osd");
    button.add_css_class("circular");
    a11y::label(&button, &label);
    button.set_can_focus(true);
    button.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| window.choose_photo(slot)
    ));
    button
}

/// Dropping image files places them, starting at the slot under the pointer.
fn add_drop(area: &gtk::DrawingArea, window: &EditorWindow) {
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop(glib::clone!(
        #[weak]
        window,
        #[upgrade_or]
        false,
        move |_target: &gtk::DropTarget, value: &glib::Value, x: f64, y: f64| -> bool {
            let Ok(files) = value.get::<gdk::FileList>() else {
                return false;
            };
            let paths: Vec<PathBuf> = files
                .files()
                .into_iter()
                .filter_map(|file| file.path())
                .collect();
            if paths.is_empty() {
                return false;
            }
            window.drop_files(paths, window.slot_at_widget(x, y));
            true
        }
    ));
    area.add_controller(drop);
}
