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
    ctx.save()?;
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
pub fn build(window: &EditorWindow) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_focusable(true);
    area.set_hexpand(true);
    area.set_vexpand(true);
    a11y::label(&area, &gettext("Collage canvas"));
    area.set_tooltip_text(Some(&gettext(
        "Drag to move the photo, scroll to zoom, Ctrl+scroll to straighten",
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
            let Some(crop) = window.fitted_crop(slot) else {
                return glib::Propagation::Proceed;
            };
            let coarse = state.contains(gdk::ModifierType::CONTROL_MASK);
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
