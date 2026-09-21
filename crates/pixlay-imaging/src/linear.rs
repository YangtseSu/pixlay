//! 16-bit linear buffers: the pipeline's intermediate representation.
//!
//! Two shapes, in the order the frozen evaluation order uses them
//! (`AGENTS.md`, "Hard constraints": `geometry → per-slot grading → global
//! filter → slot compositing`):
//!
//! * [`LinearRgba16`] — premultiplied, the resampler's output. Alpha has to
//!   survive the resample, and premultiplying first is what keeps a transparent
//!   neighbourhood from bleeding into an opaque pixel.
//! * [`LinearRgb16`] — opaque, the shape grading works on: the photo has been
//!   composited onto the slot's white base, so what the grade sees is what the
//!   user sees.
//!
//! Both store linear light as `u16` fixed point (`0` = 0.0, `65535` = 1.0). The
//! only quantization in the whole pipeline is [`LinearRgb16::to_argb32`] at the
//! end; everything in between is 16-bit or finer.

use pixlay_core::Grade;

use crate::transfer::{LUMA, from_fixed, linear_to_srgb8, to_fixed};

/// Premultiplied linear RGBA, 16 bits per channel.
#[derive(Clone, Debug)]
pub struct LinearRgba16 {
    width: u32,
    height: u32,
    data: Vec<u16>,
}

impl LinearRgba16 {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u16] {
        &self.data
    }

    /// One pixel as `[r, g, b, a]`, premultiplied.
    pub fn pixel(&self, x: u32, y: u32) -> [u16; 4] {
        let index = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.data[index],
            self.data[index + 1],
            self.data[index + 2],
            self.data[index + 3],
        ]
    }

    pub(crate) fn from_parts(width: u32, height: u32, data: Vec<u16>) -> Self {
        debug_assert_eq!(data.len(), width as usize * height as usize * 4);
        Self {
            width,
            height,
            data,
        }
    }

    /// Flattens onto the opaque white base, dropping alpha.
    ///
    /// `c = c_premultiplied + (1 - alpha)`. This is the project's rule that an
    /// export is never transparent, applied where it is lossless: after the
    /// resample (so the transparent parts are not smeared into the photo) and
    /// before grading (so the grade acts on the visible pixel).
    pub fn over_white(&self) -> LinearRgb16 {
        let mut data = vec![0u16; self.width as usize * self.height as usize * 3];
        for (source, target) in self
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .zip(data.as_chunks_mut::<3>().0)
        {
            let alpha = from_fixed(source[3]);
            for channel in 0..3 {
                let value = from_fixed(source[channel]) + (1.0 - alpha);
                target[channel] = to_fixed(value);
            }
        }
        LinearRgb16 {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

/// Opaque linear RGB, 16 bits per channel.
#[derive(Clone, Debug)]
pub struct LinearRgb16 {
    width: u32,
    height: u32,
    data: Vec<u16>,
}

impl LinearRgb16 {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u16] {
        &self.data
    }

    /// One pixel as `[r, g, b]` in linear light.
    pub fn pixel(&self, x: u32, y: u32) -> [u16; 3] {
        let index = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[index], self.data[index + 1], self.data[index + 2]]
    }

    /// Builds a buffer from straight sRGB samples, for tests and for callers that
    /// already have sRGB pixels.
    pub fn from_srgb8(width: u32, height: u32, srgb: &[u8]) -> Self {
        let mut data = vec![0u16; width as usize * height as usize * 3];
        for (source, target) in srgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(data.as_chunks_mut::<3>().0)
        {
            for channel in 0..3 {
                target[channel] = crate::transfer::srgb_to_linear(u16::from(source[channel]) * 257);
            }
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Applies a grade in linear light: exposure, then warmth, then saturation.
    ///
    /// The order is the one the contract fixes (docs/CONTRACT.md §4). The
    /// identity grade returns without touching a sample — that is what makes
    /// "factor = 1, s = 1, delta = 0 is pixel-identical" an equality instead of a
    /// tolerance, which matters because every other step of the pipeline is
    /// exact too.
    pub fn apply(&mut self, grade: &Grade) {
        if grade.is_identity() {
            return;
        }
        let (factor, saturation, delta) = (grade.factor, grade.saturation, grade.delta);
        let (warm_red, warm_blue) = (1.0 + delta, 1.0 - delta);
        for pixel in self.data.as_chunks_mut::<3>().0 {
            // Exposure and warmth first, clamped to the range the sensor could
            // have captured: a saturation control operates on what is visible,
            // and an unclamped exposure would let it darken a blown highlight.
            let mut rgb = [0.0f64; 3];
            for channel in 0..3 {
                rgb[channel] = from_fixed(pixel[channel]) * factor;
            }
            rgb[0] = (rgb[0] * warm_red).clamp(0.0, 1.0);
            rgb[1] = rgb[1].clamp(0.0, 1.0);
            rgb[2] = (rgb[2] * warm_blue).clamp(0.0, 1.0);

            let luma = LUMA[0] * rgb[0] + LUMA[1] * rgb[1] + LUMA[2] * rgb[2];
            for channel in 0..3 {
                pixel[channel] =
                    to_fixed((luma + saturation * (rgb[channel] - luma)).clamp(0.0, 1.0));
            }
        }
    }

    /// Quantizes to sRGB 8-bit, straight (the buffer is opaque).
    pub fn to_srgb8(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.data.len()];
        for (source, target) in self.data.iter().zip(out.iter_mut()) {
            *target = linear_to_srgb8(*source);
        }
        out
    }

    /// Quantizes to Cairo's `ARgb32` layout: `B, G, R, A` bytes on little-endian,
    /// premultiplied. The buffer is opaque, so premultiplied and straight agree.
    pub fn to_argb32(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.width as usize * self.height as usize * 4];
        for (source, target) in self
            .data
            .as_chunks::<3>()
            .0
            .iter()
            .zip(out.as_chunks_mut::<4>().0)
        {
            target[0] = linear_to_srgb8(source[2]);
            target[1] = linear_to_srgb8(source[1]);
            target[2] = linear_to_srgb8(source[0]);
            target[3] = 255;
        }
        out
    }
}
