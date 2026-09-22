//! Photo previews: one whole photo, resampled to a size a picker can hold.
//!
//! The picker's grid (S13) and its fit-and-zoom preview both show *the photo*,
//! and `AGENTS.md`'s rule for a visual claim applies to them too: the pixels the
//! GUI puts on screen have to be a machine-checkable number somewhere, so this is
//! the CLI's `thumb` as well as the widget's texture (`pixlay-render thumb`, and
//! S13 asserts the two are the same picture).
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
//! the photo's ratio (rounded half away from zero, at least 1 px), which is what
//! makes a row of tiles line up.

use crate::decode::Sampler;
use crate::error::ImagingError;
use crate::resample::{Region, resample};

/// A preview-sized copy of a photo: straight sRGB, 8 bits, opaque.
#[derive(Clone, Debug)]
pub struct Thumbnail {
    pub width: i32,
    pub height: i32,
    /// The photo's own pixel size, before the resample.
    ///
    /// Carried because a caller that shows a *scaled* copy still has to be able to
    /// say how big the photo is, and the decode has already read it: the picker's
    /// status line reports these (S13c), and the zoom it shows is a ratio against
    /// them. Free here — the sampled source is in hand — and one decode cheaper
    /// than asking the file a second time.
    pub source_width: u32,
    pub source_height: u32,
    /// `width * height * 3` bytes, row-major, `R`, `G`, `B`.
    pub pixels: Vec<u8>,
}

/// Resamples a whole photo so that its long edge is exactly `long_edge` pixels.
pub fn thumbnail(source: &impl Sampler, long_edge: u32) -> Result<Thumbnail, ImagingError> {
    if long_edge == 0 {
        return Err(ImagingError::EmptyThumbnail);
    }
    let (width, height) = thumb_size(source.width(), source.height(), long_edge);
    // The display grid *is* the destination: a preview shows all of the photo, so
    // the source samples per display pixel is the whole downscale ratio and the
    // resampler scales its kernel by it.
    let linear = resample(
        source,
        Region {
            display: (f64::from(width), f64::from(height)),
            texels: (0, 0, width, height),
        },
    );
    let rgb = linear.over_white();
    Ok(Thumbnail {
        width,
        height,
        source_width: source.width(),
        source_height: source.height(),
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
