//! The per-slot pipeline: one decoded photo to one bitmap the canvas can blit.
//!
//! This is where the frozen evaluation order is executed:
//!
//! ```text
//! decode + color normalization   (decode.rs)
//!   → geometry: crop to the displayed region, resample in linear light (resample.rs)
//!   → flatten onto the slot's white base
//!   → per-slot grading
//!   → global filter
//!   → 8-bit sRGB, Cairo's layout
//! ```
//!
//! The canvas then only blits and clips — it never resamples, rotates or
//! recolors (`AGENTS.md`).
//!
//! # The buffer ladder
//!
//! What exists at the same time, largest first:
//!
//! | Buffer | Size | Lifetime |
//! |---|---|---|
//! | the decoded source | `src_px * 4` bytes (8-bit) or `* 8` (16-bit) | one slot |
//! | the resampler's row strip | `block_rows * dst_w * 16` bytes, block-bounded | one slot |
//! | the output bitmap | `dst_px * 4` bytes | until the render ends |
//!
//! with `src_px ≤ MAX_DECODE_PIXELS` and `Σ dst_px = O(output pixels)`, because
//! the destination is the part of the photo the slot can show and its area is
//! the slot's own. A slot in the ten-column strip template needs a photo six
//! times its width — passing the whole displayed photo would allocate six times
//! the memory to display a tenth of it, and ten of those is 4.7 GB at A0 instead
//! of 0.5 GB.
//!
//! Peak = one source + Σ bitmaps + the output surface, and the decoder thread
//! holds one source at a time, so `N` concurrent slots need
//! `N * source + Σ bitmaps + output ≤ budget`.

use std::path::PathBuf;

use pixlay_core::CollageDoc;

use crate::decode::{Sampler, Source};
use crate::error::ImagingError;
use crate::resample::{Region, resample};

/// Pixels of margin around the exact displayed region, on every side.
///
/// Cairo's filter reads a texel or so outside the pixel it writes, and the clip
/// edge is antialiased; a bitmap cut exactly at the slot's bounding box would
/// therefore show transparent slivers inside the slot. Measured against the S3
/// sweep's own guard (3 px), which samples pixels at least 3 px inside a slot and
/// expects them covered.
pub const REGION_GUARD_PX: f64 = 3.0;

/// One slot's bitmap, ready for `pixlay_render::Bitmap::from_argb32_region`.
#[derive(Clone, Debug)]
pub struct SlotBitmap {
    /// EXIF `DateTimeOriginal` of the file this bitmap came from, verbatim.
    ///
    /// The decoder is the only stage that has seen the file, so the one field a
    /// text layer needs from a photo travels out with the pixels instead of
    /// costing a second decode (S5, `{date}`). `None` for a file with no usable
    /// date, which is what makes a layer fall back to the document's own string.
    pub date: Option<String>,
    /// Cell index this bitmap belongs to.
    pub slot: usize,
    pub width: u32,
    pub height: u32,
    /// This bitmap's top-left corner inside the displayed photo, in pixels.
    pub origin: (f64, f64),
    /// The whole displayed photo's size, in pixels (`zoom * slot width`).
    pub display: (f64, f64),
    /// `width * height * 4` bytes in Cairo's `ARgb32` layout: `B, G, R, A` on
    /// little-endian, premultiplied. The bitmap is opaque, so premultiplied and
    /// straight color agree.
    pub pixels: Vec<u8>,
}

/// Decodes every occupied cell and builds its bitmap.
///
/// `sources` is `Project::sources()`'s result: one entry per cell, resolved
/// against the project file. A cell with no source stays white and produces no
/// bitmap.
///
/// `canvas_px` is the space the bitmaps are sized in — the *output* size for the
/// render about to happen, not necessarily the export size. A 1200 px preview
/// asks for 1200-px bitmaps: `draw` blits a bitmap at the size it already is, so
/// sizing one for a full A0 export and letting Cairo shrink it would pay the
/// whole decode and resample cost for a thumbnail, and would do the shrinking
/// with Cairo's filter instead of this crate's.
pub fn slot_bitmaps(
    doc: &CollageDoc,
    canvas_px: pixlay_core::PixelSize,
    sources: &[Option<PathBuf>],
) -> Result<Vec<SlotBitmap>, ImagingError> {
    let mut bitmaps = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let Some(path) = source else {
            continue;
        };
        // One source at a time: the decoded buffer is the largest allocation in
        // the pipeline, and holding two of them is a decision the caller's
        // budget makes, not this function's.
        let decoded = Source::decode(path)?;
        bitmaps.push(slot_bitmap(doc, &decoded, index, canvas_px)?);
    }
    Ok(bitmaps)
}

/// A bitmap of one flat color, sized exactly where the real pipeline would put a
/// photo: the probe's content (`crate::probe`).
///
/// The geometry comes from the same [`CropTransform::fit`] and
/// [`display_region`](CropTransform::display_region) call as [`slot_bitmap`], so
/// the probe measures the placement the real render uses; only the pixels differ.
pub(crate) fn flat_bitmap(
    doc: &CollageDoc,
    slot_index: usize,
    canvas_px: pixlay_core::PixelSize,
    color: pixlay_core::Rgba8,
) -> Result<SlotBitmap, ImagingError> {
    // The cell's own existence is `fitted_crop`'s question too, and it answers with
    // the same typed error, so it is asked once.
    let slot = doc
        .template
        .slots
        .get(slot_index)
        .ok_or(ImagingError::MissingCell { slot: slot_index })?;
    // A flat bitmap has no photo aspect of its own: the cell's own request is
    // fitted against the slot's shape, which is the aspect a photo of the slot's
    // shape would have.
    let slot_bbox = slot.outline.bbox();
    let photo_aspect =
        slot_bbox.width() * canvas_px.aspect() / slot_bbox.height().max(f64::MIN_POSITIVE);
    let fit = doc.fitted_crop(slot_index, canvas_px.aspect(), photo_aspect)?;
    let region = fit
        .transform
        .display_region(slot, canvas_px, photo_aspect, REGION_GUARD_PX);
    let texels = region.texels();
    if texels.2 <= 0 || texels.3 <= 0 {
        return Err(ImagingError::DegenerateSlot { slot: slot_index });
    }
    let mut pixels = vec![0u8; texels.2 as usize * texels.3 as usize * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        // Cairo's `ARgb32` on little-endian is B, G, R, A; the color is opaque,
        // so premultiplied and straight agree.
        pixel[0] = color.b;
        pixel[1] = color.g;
        pixel[2] = color.r;
        pixel[3] = 255;
    }
    Ok(SlotBitmap {
        slot: slot_index,
        date: None,
        width: texels.2 as u32,
        height: texels.3 as u32,
        origin: (f64::from(texels.0), f64::from(texels.1)),
        display: region.display,
        pixels,
    })
}

/// Resamples, grades and quantizes one decoded photo for one slot.
///
/// The framing comes from [`CropTransform::fit`], exactly as the renderer takes
/// it: sizing the bitmap from the *stored request* instead of the fit would leave
/// the canvas resampling, which it must never do (S3 measured a request 11.6x
/// below the fitted zoom smearing a transparent edge 6 px into the slot).
pub fn slot_bitmap(
    doc: &CollageDoc,
    source: &impl Sampler,
    slot_index: usize,
    canvas_px: pixlay_core::PixelSize,
) -> Result<SlotBitmap, ImagingError> {
    // The cell is needed for its own grade below; `fitted_crop` reads the same
    // cell for the framing.
    let cell = doc
        .cells
        .get(slot_index)
        .ok_or(ImagingError::MissingCell { slot: slot_index })?;
    let slot = doc
        .template
        .slots
        .get(slot_index)
        .ok_or(ImagingError::MissingCell { slot: slot_index })?;

    let photo_aspect = Sampler::aspect(source);
    let fit = doc.fitted_crop(slot_index, canvas_px.aspect(), photo_aspect)?;
    let region = fit
        .transform
        .display_region(slot, canvas_px, photo_aspect, REGION_GUARD_PX);
    let texels = region.texels();
    if texels.2 <= 0 || texels.3 <= 0 {
        return Err(ImagingError::DegenerateSlot { slot: slot_index });
    }

    let linear = resample(
        source,
        Region {
            display: region.display,
            texels,
        },
    );
    // Flatten onto white, then grade: the grade acts on what the slot shows.
    let mut rgb = linear.over_white();
    rgb.apply(&cell.grade);
    rgb.apply(&doc.filter.grade());

    Ok(SlotBitmap {
        slot: slot_index,
        date: source.exif().and_then(crate::exif::date_time_original),
        width: texels.2 as u32,
        height: texels.3 as u32,
        origin: (f64::from(texels.0), f64::from(texels.1)),
        display: region.display,
        pixels: rgb.to_argb32(),
    })
}
