//! The single rendering path: `draw(doc, images, target)`.
//!
//! Preview and export are the same code with a different `scale` (and, for a
//! sheet that does not fit in memory at once, a different `band`). There is
//! exactly one place that turns a document into pixels; a second one is
//! forbidden by the project's hard constraints.
//!
//! The canvas only blits and clips: every bitmap that arrives here is already
//! the size it is displayed at, and the frames are composited over an opaque
//! white base, so an export is never transparent.

use cairo::{Context, Extend, Filter, Format, ImageSurface, Matrix, Operator, SurfacePattern};
use pixlay_core::{CollageDoc, CropTransform, PixelSize, Polygon, Slot};

use crate::bitmap::{Bitmap, Images};
use crate::error::RenderError;
use crate::text;

/// Where and at what size a document is drawn.
///
/// The cairo target surface must be sized `canvas_px * scale` — or the band's
/// slice of it. The caller owns the surface, whether that is an
/// `ImageSurface` for an export or a widget's surface in the GUI.
pub struct Target<'a> {
    pub ctx: &'a Context,
    /// Output pixels per canvas pixel. `1.0` renders one output pixel per canvas
    /// pixel; previews use `preview_px / long_edge_px`.
    pub scale: f64,
    /// Full canvas size in canvas pixels, from `CanvasSpec::pixel_size(dpi)`.
    pub canvas_px: PixelSize,
    /// Horizontal stripe of the canvas to draw, or `None` for all of it.
    pub band: Option<Band>,
}

/// A horizontal stripe of the canvas.
///
/// Rendering an A0 sheet in bands keeps the peak allocation to a slice of the
/// output instead of the whole sheet. The split is taken in *output* pixels, not
/// canvas pixels: `round` is not additive, so splitting canvas rows and rounding
/// each band's height separately makes the bands sum to more (or fewer) rows
/// than the whole once `scale != 1`. See [`Band::out_rows`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    pub index: u32,
    pub count: u32,
}

impl Band {
    /// The stripe of `total_out_px` output rows this band owns: `(first, count)`.
    ///
    /// The partition is always taken over the *rendered* surface's rows, which is
    /// the only space in which the stripes tile the whole exactly: `round` is not
    /// additive, so partitioning canvas rows and rounding each band's height
    /// separately makes the bands sum to 26 or 27 rows against a 26-row whole at
    /// scale 0.1. With cumulative partitioning the sum of `count` over all bands
    /// equals `total_out_px` for every total, count and scale.
    ///
    /// `count == 0` or an out-of-range index is a caller bug, reported rather than
    /// clamped.
    pub fn out_rows(&self, total_out_px: i32) -> Result<(i32, i32), RenderError> {
        if self.count == 0 || self.index >= self.count {
            return Err(RenderError::InvalidBand {
                index: self.index,
                count: self.count,
            });
        }
        let total = i64::from(total_out_px.max(0));
        let count = i64::from(self.count);
        let first = total * i64::from(self.index) / count;
        let next = total * (i64::from(self.index) + 1) / count;
        Ok((first as i32, (next - first) as i32))
    }
}

/// Draws `doc` into the target.
///
/// A document that was loaded through `pixlay_core::Project` is validated; a
/// document mutated in memory is drawn as it is. Framing rotations are applied
/// as stored: the clamp that guarantees a photo covers its slot arrives with S3.
pub fn draw(doc: &CollageDoc, images: &Images, target: &Target) -> Result<(), RenderError> {
    if !target.scale.is_finite() || target.scale <= 0.0 {
        return Err(RenderError::InvalidScale(target.scale));
    }

    // The band's offset is taken in output pixels and divided back into canvas
    // pixels, so a band's top row is the row the caller asked for whatever the
    // scale. Splitting canvas rows instead would drift by up to a pixel per band
    // once `scale != 1`.
    let first_row = match target.band {
        Some(band) => {
            let total_out = output_px(target.canvas_px.height, target.scale);
            f64::from(band.out_rows(total_out)?.0) / target.scale
        }
        None => 0.0,
    };

    let ctx = target.ctx;
    // The canvas is drawn in canvas pixels; the scale and the band offset are
    // the only places device pixels appear.
    ctx.save()?;
    ctx.scale(target.scale, target.scale);
    ctx.translate(0.0, -first_row);

    // Opaque white base: the export never has alpha, and everything a photo does
    // not cover stays white.
    ctx.set_operator(Operator::Source);
    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.paint()?;
    ctx.set_operator(Operator::Over);

    for (index, cell) in doc.cells.iter().enumerate() {
        let Some(bitmap) = images.get(index) else {
            continue;
        };
        let Some(slot) = doc.template.slots.get(index) else {
            continue;
        };
        // The stored crop is a request; what gets drawn is its fit (S3). The fit
        // is taken in the space this placement uses (the output canvas pixels,
        // whose aspect is the rounded one), so "covers the slot" is exact for the
        // arithmetic below and not only for the document's millimetres.
        let fit = cell
            .crop
            .fit(slot, target.canvas_px.aspect(), bitmap.aspect());
        draw_slot(ctx, index, slot, bitmap, &fit.transform, target.canvas_px)?;
    }

    // Text last: it is a canvas-level content layer, so it covers the cells and a
    // photo's framing can never move it (docs/CONTRACT.md §4, S5).
    text::draw_layers(ctx, doc, images, target.canvas_px)?;

    ctx.restore()?;
    Ok(())
}

/// Clip to the slot outline, then place the bitmap inside it.
///
/// `crop` is already fitted ([`CropTransform::fit`]): its zoom is at least the
/// one that covers the slot, its rotation is one the zoom can afford, and its
/// offset keeps the photo over the slot. The photo's displayed width is
/// `crop.zoom * slot_width` — the absolute zoom the document stores.
fn draw_slot(
    ctx: &Context,
    index: usize,
    slot: &Slot,
    bitmap: &Bitmap,
    crop: &CropTransform,
    canvas: PixelSize,
) -> Result<(), RenderError> {
    if slot.outline.points.len() < Polygon::MIN_VERTICES {
        return Err(RenderError::DegenerateSlot { slot: index });
    }
    let (canvas_w, canvas_h) = (f64::from(canvas.width), f64::from(canvas.height));
    let bbox = slot.outline.bbox();
    let slot_w = bbox.width() * canvas_w;
    let slot_h = bbox.height() * canvas_h;

    // The whole displayed photo, not this bitmap: a bitmap may hold only the part
    // of the photo the slot can show (`Bitmap::from_argb32_region`), and the
    // display scale has to be the one the fit produced either way.
    let (photo_w, photo_h) = bitmap.display_size();
    let displayed_w = crop.zoom * slot_w;
    let displayed_h = displayed_w * photo_h / photo_w;
    let center_x = (bbox.center().x * canvas_w) + crop.offset.0 * slot_w;
    let center_y = (bbox.center().y * canvas_h) + crop.offset.1 * slot_h;

    // Cairo's pattern matrix maps user space to *pattern* space, so the
    // placement matrix is inverted before it is handed over. Getting the
    // direction wrong is silent: cairo draws nothing. The pattern space of a
    // partial bitmap is its own top-left corner, hence the extra translation by
    // the region origin inside the displayed photo.
    let (origin_x, origin_y) = bitmap.origin();
    let mut placement = Matrix::identity();
    placement.translate(center_x, center_y);
    placement.rotate(crop.rotation_deg.to_radians());
    placement.scale(displayed_w / photo_w, displayed_h / photo_h);
    placement.translate(-photo_w / 2.0 + origin_x, -photo_h / 2.0 + origin_y);
    let pattern_matrix = placement.try_invert()?;

    ctx.save()?;
    outline_path(ctx, &slot.outline, canvas);
    ctx.clip();
    let pattern = SurfacePattern::create(bitmap.surface());
    pattern.set_matrix(pattern_matrix);
    pattern.set_filter(Filter::Good);
    // Anything the photo does not cover stays transparent, so the white base
    // shows through: rotation crops edges instead of extending the canvas.
    pattern.set_extend(Extend::None);
    ctx.set_source(&pattern)?;
    ctx.paint()?;
    ctx.restore()?;
    Ok(())
}

fn outline_path(ctx: &Context, outline: &Polygon, canvas: PixelSize) {
    let (w, h) = (f64::from(canvas.width), f64::from(canvas.height));
    ctx.new_path();
    for (index, point) in outline.points.iter().enumerate() {
        let (x, y) = (point.x * w, point.y * h);
        if index == 0 {
            ctx.move_to(x, y);
        } else {
            ctx.line_to(x, y);
        }
    }
    ctx.close_path();
}

/// Renders `doc` into a fresh image surface.
///
/// A thin wrapper: it allocates the surface, builds the context and calls
/// [`draw`]. Previews and exports differ only in `scale` and `band`.
pub fn render_surface(
    doc: &CollageDoc,
    images: &Images,
    dpi: u32,
    scale: f64,
    band: Option<Band>,
) -> Result<ImageSurface, RenderError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(RenderError::InvalidScale(scale));
    }
    let canvas_px = doc.canvas.pixel_size(dpi)?;
    // The whole render's size, rounded once. Band sizes are then carved out of
    // it in output pixels, which is the only partition whose parts sum to the
    // whole (rounding is not additive).
    let width = output_px(canvas_px.width, scale);
    let full_height = output_px(canvas_px.height, scale);
    let height = match band {
        Some(band) => band.out_rows(full_height)?.1,
        None => full_height,
    };
    let surface = ImageSurface::create(Format::ARgb32, width, height)?;
    let ctx = Context::new(&surface)?;
    draw(
        doc,
        images,
        &Target {
            ctx: &ctx,
            scale,
            canvas_px,
            band,
        },
    )?;
    surface.flush();
    Ok(surface)
}

/// Straight, opaque RGB pixels — the shape every encoder wants.
pub struct Rgb8Image {
    pub width: i32,
    pub height: i32,
    /// `width * height * 3` bytes, row-major, top-left first.
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Rgb8Image {
    /// Dimensions and length only: a debug print of a 139.5 MP buffer is not
    /// something anyone wants to read.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rgb8Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl Rgb8Image {
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 3] {
        let index = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[index], self.data[index + 1], self.data[index + 2]]
    }
}

/// Converts a rendered surface to straight RGB, compositing anything
/// translucent over white.
///
/// The base is opaque, so this is a straight copy in practice; it stays correct
/// if a bitmap with alpha ever reaches the canvas.
pub fn rgb8(surface: &ImageSurface) -> Result<Rgb8Image, RenderError> {
    let (width, height, stride) = (surface.width(), surface.height(), surface.stride() as usize);
    let alpha_aware = match surface.format() {
        Format::ARgb32 => true,
        // Cairo's PNG reader hands back RGB24 for opaque images.
        Format::Rgb24 => false,
        other => {
            return Err(RenderError::SurfaceFormat {
                format: format!("{other:?}"),
            });
        }
    };
    let mut data = vec![0u8; width as usize * height as usize * 3];
    surface.with_data(|bytes| {
        for y in 0..height as usize {
            for x in 0..width as usize {
                let source = y * stride + x * 4;
                let alpha = if alpha_aware {
                    u32::from(bytes[source + 3])
                } else {
                    255
                };
                let (b, g, r) = (
                    u32::from(bytes[source]),
                    u32::from(bytes[source + 1]),
                    u32::from(bytes[source + 2]),
                );
                // Premultiplied over white.
                let target = (y * width as usize + x) * 3;
                data[target] = over_white(r, alpha);
                data[target + 1] = over_white(g, alpha);
                data[target + 2] = over_white(b, alpha);
            }
        }
    })?;
    Ok(Rgb8Image {
        width,
        height,
        data,
    })
}

fn over_white(channel: u32, alpha: u32) -> u8 {
    let value = channel + 255 * (255 - alpha) / 255;
    value.min(255) as u8
}

/// [`render_surface`] followed by [`rgb8`]: the path the CLI and the encoders
/// use.
pub fn render_rgb8(
    doc: &CollageDoc,
    images: &Images,
    dpi: u32,
    scale: f64,
    band: Option<Band>,
) -> Result<Rgb8Image, RenderError> {
    let surface = render_surface(doc, images, dpi, scale, band)?;
    rgb8(&surface)
}

/// Output pixels for `canvas_px` at `scale`, rounding half away from zero — the
/// same rule `CanvasSpec::pixel_size` uses for its own rounding.
pub fn output_px(canvas_px: i32, scale: f64) -> i32 {
    (f64::from(canvas_px) * scale).round().max(1.0) as i32
}
