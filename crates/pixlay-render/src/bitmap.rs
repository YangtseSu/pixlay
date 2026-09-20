//! The bitmaps the canvas is allowed to see.
//!
//! A bitmap handed to the renderer is already decoded, already resampled to the
//! size it is displayed at, already rotated and already recolored: the canvas
//! only blits and clips. `pixlay-imaging` produces these (S4); until then the
//! CLI and the tests build them directly, which is what makes the golden pixel
//! tests independent of any decoder.

use std::collections::BTreeMap;

use cairo::{Context, Format, ImageSurface, Operator};

use crate::error::RenderError;
use pixlay_core::Rgba8;

/// An owned, immutable pixel buffer in Cairo's native `ARgb32` layout:
/// premultiplied, one `u32` per pixel, native endianness.
///
/// The bitmap owns a Cairo surface, so it is neither `Send` nor `Sync` — a
/// background decode hands over a raw buffer and the receiving thread builds the
/// bitmap, exactly as GTK objects must not cross threads.
#[derive(Clone, Debug)]
pub struct Bitmap {
    surface: ImageSurface,
}

impl Bitmap {
    /// Wraps raw `ARgb32` premultiplied pixels without copying.
    ///
    /// `data` must be `width * height * 4` bytes in `ARgb32` layout: byte order
    /// `B, G, R, A` on little-endian, colors premultiplied by alpha. Opaque
    /// pixels are identical in both layouts, so callers that composite onto
    /// white can pass straight color.
    pub fn from_argb32(width: i32, height: i32, data: Vec<u8>) -> Result<Self, RenderError> {
        check_dimension("width", width)?;
        check_dimension("height", height)?;
        let expected = width as usize * height as usize * 4;
        if data.len() != expected {
            return Err(RenderError::BitmapSize {
                width,
                height,
                expected,
                got: data.len(),
            });
        }
        let surface =
            ImageSurface::create_for_data(data, Format::ARgb32, width, height, width * 4)?;
        Ok(Self { surface })
    }

    /// A bitmap of one color. `a` is composited by Cairo, so the stored pixels
    /// come out premultiplied.
    pub fn filled(width: i32, height: i32, color: Rgba8) -> Result<Self, RenderError> {
        check_dimension("width", width)?;
        check_dimension("height", height)?;
        let surface = ImageSurface::create(Format::ARgb32, width, height)?;
        let ctx = Context::new(&surface)?;
        ctx.set_operator(Operator::Source);
        ctx.set_source_rgba(
            f64::from(color.r) / 255.0,
            f64::from(color.g) / 255.0,
            f64::from(color.b) / 255.0,
            f64::from(color.a) / 255.0,
        );
        ctx.paint()?;
        surface.flush();
        Ok(Self { surface })
    }

    pub fn width(&self) -> i32 {
        self.surface.width()
    }

    pub fn height(&self) -> i32 {
        self.surface.height()
    }

    /// `width / height`.
    pub fn aspect(&self) -> f64 {
        f64::from(self.width()) / f64::from(self.height())
    }

    pub(crate) fn surface(&self) -> &ImageSurface {
        &self.surface
    }
}

/// The bitmaps of one render, keyed by cell index. A slot with no entry stays
/// white — that is the empty cell, not an error.
#[derive(Clone, Debug, Default)]
pub struct Images {
    bitmaps: BTreeMap<usize, Bitmap>,
}

impl Images {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, slot: usize, bitmap: Bitmap) {
        self.bitmaps.insert(slot, bitmap);
    }

    pub fn get(&self, slot: usize) -> Option<&Bitmap> {
        self.bitmaps.get(&slot)
    }

    pub fn len(&self) -> usize {
        self.bitmaps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bitmaps.is_empty()
    }
}

fn check_dimension(what: &'static str, value: i32) -> Result<(), RenderError> {
    if value <= 0 {
        return Err(RenderError::BitmapDimension { what, value });
    }
    Ok(())
}
