// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

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
    /// Where this bitmap sits inside the *displayed photo*, in displayed-photo
    /// pixels: `(0, 0)` for a bitmap that holds the whole photo, the floored
    /// region origin for one that holds only the part a slot can show.
    origin: (f64, f64),
    /// The size the whole displayed photo would have, in the same pixels, or
    /// `None` for "this bitmap is the whole photo" (then it is its own size).
    ///
    /// It is what `draw` needs to place a partial bitmap: the display scale is
    /// `displayed width / photo width`, and a bitmap holding a sub-rectangle
    /// cannot answer that from its own dimensions.
    display: Option<(f64, f64)>,
}

impl Bitmap {
    /// Wraps raw `ARgb32` premultiplied pixels without copying.
    ///
    /// `data` must be `width * height * 4` bytes in `ARgb32` layout: byte order
    /// `B, G, R, A` on little-endian, colors premultiplied by alpha. Opaque
    /// pixels are identical in both layouts, so callers that composite onto
    /// white can pass straight color.
    pub fn from_argb32(width: i32, height: i32, data: Vec<u8>) -> Result<Self, RenderError> {
        let surface = surface_from_argb32(width, height, data)?;
        Ok(Self {
            surface,
            origin: (0.0, 0.0),
            display: None,
        })
    }

    /// The same, for a bitmap that holds only part of the displayed photo.
    ///
    /// `origin` is the floored region origin and `display` the whole displayed
    /// photo's size, both in displayed-photo pixels — exactly the pair
    /// [`CropTransform::display_region`] returns with the texels it was cut to.
    /// A bitmap that is not marked this way is treated as the whole photo, which
    /// is what every caller before S4 builds, so nothing else changes.
    ///
    /// [`CropTransform::display_region`]: pixlay_core::CropTransform::display_region
    pub fn from_argb32_region(
        width: i32,
        height: i32,
        origin: (f64, f64),
        display: (f64, f64),
        data: Vec<u8>,
    ) -> Result<Self, RenderError> {
        let surface = surface_from_argb32(width, height, data)?;
        Ok(Self {
            surface,
            origin,
            display: Some(display),
        })
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
        Ok(Self {
            surface,
            origin: (0.0, 0.0),
            display: None,
        })
    }

    pub fn width(&self) -> i32 {
        self.surface.width()
    }

    pub fn height(&self) -> i32 {
        self.surface.height()
    }

    /// `width / height` **of the whole displayed photo**, not of this bitmap.
    ///
    /// The fit is taken against the photo's aspect: a bitmap holding a
    /// sub-rectangle has the same aspect only by accident, and passing its own
    /// would re-fit the framing to a shape the user never chose.
    pub fn aspect(&self) -> f64 {
        let (width, height) = self.display_size();
        width / height
    }

    /// The whole displayed photo's size in bitmap pixels.
    pub fn display_size(&self) -> (f64, f64) {
        self.display.unwrap_or_else(|| {
            (
                f64::from(self.surface.width()),
                f64::from(self.surface.height()),
            )
        })
    }

    /// This bitmap's top-left corner inside the displayed photo, in pixels.
    pub fn origin(&self) -> (f64, f64) {
        self.origin
    }

    pub(crate) fn surface(&self) -> &ImageSurface {
        &self.surface
    }
}

fn surface_from_argb32(
    width: i32,
    height: i32,
    data: Vec<u8>,
) -> Result<ImageSurface, RenderError> {
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
    Ok(ImageSurface::create_for_data(
        data,
        Format::ARgb32,
        width,
        height,
        width * 4,
    )?)
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
