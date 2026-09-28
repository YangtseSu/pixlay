// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lanczos3 resampling, in linear light, with the support scaled for downscaling.
//!
//! The pipeline resamples exactly once, and it resamples *from the source* — not
//! from a copy already scaled to the slot: `AGENTS.md` puts every resample
//! upstream of the canvas, and doing it in one step is also what keeps the
//! buffer ladder small.
//!
//! Two properties the criteria depend on:
//!
//! * **The kernel widens when the image shrinks.** A 4000 px photo in a 400 px
//!   slot is downsampled 10x; sampling a fixed 3-tap-radius kernel would read one
//!   source pixel in ten and alias the other nine into moiré. The filter is
//!   therefore scaled the way ImageMagick's `-resize` scales it (support and
//!   weights divided by the shrink ratio), which is what makes the measured RMSE
//!   against `magick -filter Lanczos -resize` a comparison of two implementations
//!   of the same thing rather than of two different things.
//! * **The edge samples are extended.** A covering fit puts the slot's boundary
//!   *on* the photo's boundary, so the kernel reads past it; extending the last
//!   row/column is what keeps that from becoming a transparent veil one pixel
//!   wide inside the slot.
//!
//! Accumulators are `f32`; the buffer that comes out is 16-bit fixed point
//! (`crate::linear::LinearRgba16`). Nothing here quantizes.
//!
//! # Why the weights are tabulated first
//!
//! A tap's weight depends on the tap's *distance*, which for a separable filter
//! depends only on the output index on that axis — not on the other axis. The
//! kernels are therefore built once per axis into flat tables and reused for every
//! row and column. Computing them inline instead means a `sin` per tap per row:
//! measured 2026-09-21 on A0 (148 MP of bitmaps), that alone was 9.5 s of the
//! render, against 1.6 s with the tables.

use crate::decode::Sampler;
use crate::error::ImagingError;
use crate::linear::LinearRgba16;
use crate::transfer::{WGHT, srgb_to_linear_f32, to_fixed};

/// Taps on each side of the centre, for an unscaled kernel.
const LANCZOS_A: f64 = 3.0;

/// Output rows resampled per block.
///
/// The vertical pass needs the horizontally resampled rows within its kernel
/// reach, so a block bounds that buffer: at 256 rows and a 10x downscale it holds
/// ~2600 rows of `dst_w * 4` floats. Blocks are an implementation detail of this
/// module — the split is exact, and the tests pin that a block boundary leaves no
/// seam.
const BLOCK_ROWS: usize = 256;

/// Largest a single bitmap may be, in texels: the canvas pixel budget
/// ([`pixlay_core::MAX_CANVAS_PIXELS`]), applied at the second boundary a size is
/// asked for (S15e, PIX-003).
///
/// A bitmap holds the part of the photo the slot can show, so it is the slot's own
/// extent in output pixels plus the axis-aligned box a framing rotation needs —
/// bounded by the canvas rather than by the zoom, which is what makes the canvas's
/// own budget the right bound for it. **The displayed size needs no bound of its
/// own**: `Region::display` is the size the whole photo is displayed at, and it can
/// legitimately be enormous (a 20000-px-wide source in a 1:20000 aspect is
/// displayed 6e11 px tall), while the region stays the slot's own extent because
/// `display_region` clamps it into that display rectangle. Memory follows the
/// region, so the region is what is budgeted.
///
/// Measured 2026-09-24 over the shipped library at every whole degree and six photo
/// aspects: the worst single slot is **212.8 MP** — `strip-2-2x1g` at A0 (a
/// half-canvas slot at 45 degrees, whose conversion alone would hold 3.9 GB) — and
/// this refuses it; at the largest legal grid the worst is 215.8 MP, the same
/// template and angle. What this bound does **not** cover is the *sum* over slots:
/// measured the same day, `strip-9-9x1` at A0 with every cell at 45 degrees holds
/// 403 MP of bitmaps at once, which is the memory budget's question rather than
/// this pixel budget's.
pub const MAX_BITMAP_PIXELS: u64 = pixlay_core::MAX_CANVAS_PIXELS;

/// Refuses a bitmap past the budget, before a byte is allocated for it.
///
/// `what` names the offender for the message (`slot 3`, `a photo preview`): the
/// whole-photo preview has no cell, and one budget covers both. `bytes` is what
/// the caller's own conversion will hold at its peak — [`Region::conversion_bytes`]
/// for a resampled slot, four bytes per texel for the probe's flat bitmap — and is
/// carried into the message because "how much memory was this asking for" is the
/// question the refusal exists to answer.
///
/// The check is on the destination's texels, which is what the conversion's memory
/// follows: the three destination buffers are 18 bytes per texel, so the texel bound
/// *is* the byte bound. The row strip is the one buffer that does not follow it, and
/// it needs no bound of its own — for a source inside the decoder's caps it is
/// `16 * dst_w * min(src_h, 262 * step)` bytes, `step = src_h / display_h`, and
/// `dst_w <= display_w` with `display_w / display_h <= photo_aspect = src_w / src_h`:
/// at most 4192 bytes per source column, so under 84 MB at `MAX_DECODE_EDGE`.
pub fn check_bitmap(what: &str, region: &Region, bytes: u64) -> Result<(), ImagingError> {
    let pixels = region.pixels();
    if pixels > MAX_BITMAP_PIXELS {
        return Err(ImagingError::BitmapTooLarge {
            what: what.to_string(),
            pixels,
            bytes,
            max: MAX_BITMAP_PIXELS,
        });
    }
    Ok(())
}

/// A source rectangle and its destination, both in the *displayed* photo's pixel
/// grid, plus the source size that grid was derived from.
///
/// `display` is the size the whole photo would have at the slot's zoom, and
/// `texels` the `(x, y, width, height)` the bitmap holds — exactly what
/// [`pixlay_core::CropTransform::display_region`] and its `texels()` produce.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub display: (f64, f64),
    pub texels: (i32, i32, i32, i32),
}

impl From<pixlay_core::DisplayRegion> for Region {
    fn from(region: pixlay_core::DisplayRegion) -> Self {
        Self {
            display: region.display,
            texels: region.texels(),
        }
    }
}

impl Region {
    fn destination(&self) -> (u32, u32) {
        (self.texels.2.max(1) as u32, self.texels.3.max(1) as u32)
    }

    /// The bitmap's own texel count: its width times its height, and zero for a
    /// region with no area.
    pub fn pixels(&self) -> u64 {
        let (_, _, width, height) = self.texels;
        u64::from(width.max(0) as u32) * u64::from(height.max(0) as u32)
    }

    /// What this region's conversion holds at its peak, in bytes.
    ///
    /// Everything [`resample`] allocates for one destination, plus the flattened
    /// buffer and the bitmap that follow it — all alive at the last step:
    ///
    /// | Buffer | Bytes | Source |
    /// |---|---|---|
    /// | the resampler's output, `LinearRgba16` | `8 * texels` | two bytes per channel, four channels |
    /// | the row strip, `f32` RGBA | `16 * dst_w * rows` | one block's kernel reach, `rows` source rows |
    /// | the flattened `LinearRgb16` | `6 * texels` | two bytes, three channels |
    /// | the ARgb32 bitmap the canvas takes | `4 * texels` | one byte per channel |
    ///
    /// The strip is the one that does not follow the texel count: a block reaches
    /// `BLOCK_ROWS * step` source rows plus the filter's own support on both ends
    /// (`2 * LANCZOS_A * step`), with `step = src_h / display_h` and clamped by the
    /// source's height. It is reported as part of the peak rather than checked on
    /// its own — the three destination buffers above it are what the budget is
    /// about, and they grow 18 bytes per texel where the strip grows 16 bytes per
    /// destination *column*.
    pub fn conversion_bytes(&self, source_height: u32) -> u64 {
        let pixels = self.pixels();
        let width = self.texels.2.max(1) as u64;
        let step = f64::from(source_height) / self.display.1.max(f64::MIN_POSITIVE);
        let reach = (BLOCK_ROWS as f64 + 2.0 * LANCZOS_A) * step.max(1.0);
        let rows = reach.min(f64::from(source_height)).max(1.0) as u64;
        18 * pixels + 16 * width * rows
    }
}

/// The Lanczos3 kernel, `sinc(x) * sinc(x/3)`.
fn lanczos(x: f64) -> f64 {
    let x = x.abs();
    if x < 1e-9 {
        return 1.0;
    }
    if x >= LANCZOS_A {
        return 0.0;
    }
    let px = std::f64::consts::PI * x;
    let narrow = px.sin() / px;
    let wide = (px / LANCZOS_A).sin() / (px / LANCZOS_A);
    narrow * wide
}

/// The taps of one axis, tabulated once and reused for every line.
///
/// For output index `i`, the taps are `indices[start[i]..start[i + 1]]` with the
/// matching `weights`; indices are already clamped into the source, so the edge is
/// extended rather than treated as transparent.
struct Kernel {
    start: Vec<u32>,
    indices: Vec<u32>,
    weights: Vec<f32>,
}

impl Kernel {
    /// Builds the kernel for `outputs` texels along an axis of `len` samples.
    ///
    /// `origin` is the axis coordinate of output texel 0's centre in the source's
    /// index space (pixel `k` covers `[k, k+1)`), and `step` the source samples per
    /// output texel — at least 1, which is what widens the kernel.
    fn new(len: u32, outputs: u32, origin: f64, step: f64) -> Self {
        let scale = step.max(1.0);
        let support = LANCZOS_A * scale;
        let mut start = Vec::with_capacity(outputs as usize + 1);
        let mut indices = Vec::new();
        let mut weights = Vec::new();
        for i in 0..outputs {
            start.push(indices.len() as u32);
            let centre = origin + f64::from(i) * step;
            let first = (centre - support - 0.5).ceil() as i64;
            let last = (centre + support - 0.5).floor() as i64;
            let mut sum = 0.0;
            let base = indices.len();
            for k in first..=last {
                let weight = lanczos((centre - (k as f64 + 0.5)) / scale);
                if weight == 0.0 {
                    continue;
                }
                indices.push(k.clamp(0, i64::from(len) - 1) as u32);
                weights.push(weight as f32);
                sum += weight;
            }
            // Normalized per output texel, which is what makes a flat area
            // exactly flat however the taps fall at the edges.
            if sum != 0.0 {
                let scale = 1.0 / sum as f32;
                for weight in &mut weights[base..] {
                    *weight *= scale;
                }
            }
        }
        start.push(indices.len() as u32);
        Self {
            start,
            indices,
            weights,
        }
    }

    fn taps(&self, index: usize) -> (&[u32], &[f32]) {
        let (from, to) = (self.start[index] as usize, self.start[index + 1] as usize);
        (&self.indices[from..to], &self.weights[from..to])
    }
}

/// Resamples the source's `region` into linear 16-bit premultiplied pixels.
pub fn resample(source: &impl Sampler, region: Region) -> LinearRgba16 {
    let (dst_w, dst_h) = region.destination();
    let (x0, y0) = (f64::from(region.texels.0), f64::from(region.texels.1));
    let src_w = source.width();
    let src_h = source.height();
    // Source pixels per display pixel on each axis.
    let sx = f64::from(src_w) / region.display.0;
    let sy = f64::from(src_h) / region.display.1;

    let columns = Kernel::new(src_w, dst_w, (x0 + 0.5) * sx, sx);
    let rows = Kernel::new(src_h, dst_h, (y0 + 0.5) * sy, sy);

    let mut out = vec![0u16; dst_w as usize * dst_h as usize * 4];
    let stride = dst_w as usize * 4;
    // One horizontally resampled row per source row a block needs, linear
    // premultiplied RGBA.
    let mut strip = Vec::<f32>::new();

    let mut block_start = 0usize;
    while block_start < dst_h as usize {
        let block_rows = BLOCK_ROWS.min(dst_h as usize - block_start);
        // The source rows this block's kernel can reach, clamped to the source.
        let (first_rows, _) = rows.taps(block_start);
        let (last_rows, _) = rows.taps(block_start + block_rows - 1);
        let k0 = *first_rows.iter().min().unwrap_or(&0) as usize;
        let k1 = *last_rows.iter().max().unwrap_or(&0) as usize + 1;
        let source_rows = k1 - k0;
        strip.resize(source_rows * stride, 0.0);

        for (offset, row) in (k0..k1).enumerate() {
            let line = &mut strip[offset * stride..(offset + 1) * stride];
            for i in 0..dst_w as usize {
                let (indices, weights) = columns.taps(i);
                let mut sums = [0.0f32; 4];
                for (&x, &weight) in indices.iter().zip(weights) {
                    let pixel = source.pixel(x, row as u32);
                    let alpha = f32::from(pixel[3]) * WGHT;
                    // sRGB -> linear, then premultiply: doing it here rather than
                    // at decode time keeps 8-bit sources at 8-bit in memory and
                    // still premultiplies in linear light, before any filtering.
                    sums[0] += weight * alpha * srgb_to_linear_f32(pixel[0]);
                    sums[1] += weight * alpha * srgb_to_linear_f32(pixel[1]);
                    sums[2] += weight * alpha * srgb_to_linear_f32(pixel[2]);
                    sums[3] += weight * alpha;
                }
                line[i * 4..i * 4 + 4].copy_from_slice(&sums);
            }
        }

        for row in 0..block_rows {
            let (indices, weights) = rows.taps(block_start + row);
            let target = &mut out[(block_start + row) * stride..(block_start + row + 1) * stride];
            for i in 0..dst_w as usize {
                let mut sums = [0.0f32; 4];
                for (&y, &weight) in indices.iter().zip(weights) {
                    let line = &strip[(y as usize - k0) * stride + i * 4..][..4];
                    for (sum, sample) in sums.iter_mut().zip(line) {
                        *sum += weight * sample;
                    }
                }
                for (slot, sum) in target[i * 4..i * 4 + 4].iter_mut().zip(sums) {
                    // Clamped here, not earlier: Lanczos overshoots at an edge,
                    // and a premultiplied channel cannot leave [0, 1] (alpha
                    // bounds it above).
                    *slot = to_fixed(f64::from(sum).clamp(0.0, 1.0));
                }
            }
        }

        block_start += block_rows;
    }

    LinearRgba16::from_parts(dst_w, dst_h, out)
}
