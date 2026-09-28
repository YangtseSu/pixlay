// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Photo previews: a photo (or a rectangle of one) resampled to a preview size.
//!
//! A preview shows *the photo*: a `Contain` fit at rest and the photo's own pixels at
//! 1:1 (`S15j`, ruling 2's "a large preview that can zoom and pan" in the bounded form
//! the 2026-09-24 ruling fixed). `AGENTS.md`'s rule for a visual claim applies: the
//! pixels shown have to be a machine-checkable number somewhere, so this is the CLI's
//! `thumb` (`pixlay-render thumb`, whose `--region` is the same call a 1:1 preview
//! makes); S13 held the widget's texture to exactly these pixels.
//!
//! Two decisions worth stating, because they are what make the result *the
//! pipeline's* picture rather than a second one:
//!
//! * **It is the same resampler.** A preview is `Region` covering the whole photo
//!   at the preview's own grid (`crate::resample`), so the kernel widens by the
//!   downscale ratio exactly as it does for a slot: no preview aliases a photo the
//!   export would filter properly, and there is one Lanczos3 implementation in the
//!   project instead of two that drift.
//! * **It is the same color path.** Premultiplied linear light, flattened onto
//!   opaque white, quantized to sRGB 8-bit at the end — a preview of a photo with
//!   an alpha channel shows what the slot will show, and neither one is
//!   transparent.
//!
//! The long edge is *exact*: the caller asks for a grid, and the other edge keeps
//! the photo's ratio (rounded half away from zero, at least 1 px), so every preview
//! of a photo has the same shape.

use crate::decode::Sampler;
use crate::error::ImagingError;
use crate::resample::{Region, check_bitmap, resample};

/// A rectangle of a photo, in the photo's own pixels.
///
/// The part of a photo a viewer shows, before any scaling: [`thumbnail_region`]
/// resamples one, the CLI's `--region` is one, and a 1:1 preview is one. It is
/// also a request's identity where one is made, so it is `Eq + Hash` and `Copy`
/// (S13's preview worker deduped and cancelled by it).
///
/// The grid is the one a decode reports — the photo's own pixels with EXIF orientation
/// already applied ([`Thumbnail::source_width`]) — and a rectangle the photo does not
/// contain is refused rather than clamped: a rectangle is a request, and one that hangs
/// off the edge is a caller that computed it from the wrong grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// The whole of a `width x height` photo.
    pub fn whole(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Whether this rectangle has an area and is inside a `width x height` photo.
    pub fn inside(&self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height)
    }
}

/// A preview-sized copy of a photo: straight sRGB, 8 bits, opaque.
#[derive(Clone, Debug)]
pub struct Thumbnail {
    pub width: i32,
    pub height: i32,
    /// The photo's own pixel size, before the resample.
    ///
    /// Carried because a caller that shows a *scaled* copy still has to be able to
    /// say how big the photo is, and the decode has already read it: `thumb`
    /// reports these as `src_w` / `src_h` (S13c), and a zoom is a ratio against
    /// them. Free here — the sampled source is in hand — and one decode cheaper
    /// than asking the file a second time.
    pub source_width: u32,
    pub source_height: u32,
    /// `width * height * 3` bytes, row-major, `R`, `G`, `B`.
    pub pixels: Vec<u8>,
}

/// Resamples a whole photo so that its long edge is exactly `long_edge` pixels.
pub fn thumbnail(source: &impl Sampler, long_edge: u32) -> Result<Thumbnail, ImagingError> {
    thumbnail_region(
        source,
        Rect::whole(source.width(), source.height()),
        long_edge,
    )
}

/// Resamples one rectangle of a photo to the size it is drawn at.
///
/// `long_edge` is the destination's long edge and the destination's *aspect* is the
/// rectangle's, so a `long_edge` equal to the rectangle's own long edge is a **1:1**
/// resample: the Lanczos taps degenerate to the identity at exactly 1:1 (`lanczos(0)`
/// is 1 and every other tap is 0), which is what makes a 1:1 preview the
/// photo's own pixels rather than a scaled copy of them. A smaller `long_edge` is a fit
/// of that rectangle, through the same `resample` and the same colour path as a
/// whole-photo preview — there is one preview pipeline, not two.
///
/// The rectangle is in the photo's own pixel grid, and one the photo does not contain
/// is [`ImagingError::RegionOutside`]: `resample` clamps its taps into the source, so a
/// region that hung off the edge would not fail but would smear the last row into a
/// picture of the wrong size.
pub fn thumbnail_region(
    source: &impl Sampler,
    region: Rect,
    long_edge: u32,
) -> Result<Thumbnail, ImagingError> {
    if long_edge == 0 {
        return Err(ImagingError::EmptyThumbnail);
    }
    let (source_width, source_height) = (source.width(), source.height());
    if !region.inside(source_width, source_height) {
        return Err(ImagingError::RegionOutside {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
            source_width,
            source_height,
        });
    }
    let (width, height) = thumb_size(region.width, region.height, long_edge);
    // The display grid the resampler works in: the size the *whole* photo would have if
    // the rectangle were drawn at `long_edge`. The destination is the rectangle's own
    // size, so the display step per output texel is the source step — the rectangle's
    // first output texel covers `step` source pixels starting at the rectangle's own x
    // (exactly, at 1:1, where `step` is 1).
    let step = (
        f64::from(region.width) / f64::from(width),
        f64::from(region.height) / f64::from(height),
    );
    let display = (
        f64::from(source_width) / step.0,
        f64::from(source_height) / step.1,
    );
    let view = Region {
        display,
        texels: (
            (f64::from(region.x) / step.0).round() as i32,
            (f64::from(region.y) / step.1).round() as i32,
            width,
            height,
        ),
    };
    // A preview is a bitmap like a slot's, so it answers to the same budget — the
    // request is a caller's, and this is a public entry point (S15e, PIX-003). The
    // estimate is the slot-shaped one, whose last buffer is four bytes per texel
    // where a thumbnail's is three: an over-estimate, which is the safe direction
    // for a message about memory.
    check_bitmap(
        "a photo preview",
        &view,
        view.conversion_bytes(source_height),
    )?;
    let linear = resample(source, view);
    let rgb = linear.over_white();
    Ok(Thumbnail {
        width,
        height,
        source_width,
        source_height,
        pixels: rgb.to_srgb8(),
    })
}

/// The destination grid for a `src_w x src_h` photo whose long edge should be
/// `long_edge` pixels.
///
/// The long edge is exact by construction (`round(src_long * long / src_long)`),
/// and the short one keeps the photo's ratio: both are rounded half away from
/// zero, the same rule the render grid follows (`PixelSize::for_long_edge`), and
/// neither drops below one pixel — a 10000x1 pano previewed at 100 px is 100x1,
/// not 100x0.
///
/// Crate-internal because the preview-grade reduction sizes itself with the same
/// rule ([`crate::reduce`]): "a preview-sized copy" is one thing in this crate.
pub(crate) fn thumb_size(src_w: u32, src_h: u32, long_edge: u32) -> (i32, i32) {
    let longest = f64::from(src_w.max(src_h).max(1));
    let scale = f64::from(long_edge) / longest;
    (
        (f64::from(src_w) * scale).round().max(1.0) as i32,
        (f64::from(src_h) * scale).round().max(1.0) as i32,
    )
}
