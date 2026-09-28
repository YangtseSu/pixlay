// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview-grade source: a decoded photo reduced to a size a preview shows.
//!
//! S12 measured the stutter's cause and its size (`docs/CONTRACT.md` §8, "S12"):
//! one cell's bitmap costs what the **source's** resolution costs, not what the
//! output grid's does, because [`resample`](crate::resample) widens its kernel with
//! the downscale ratio — a cell showing a quarter of a 24 MP photo reads 24 MP of
//! source taps at any preview size, and a gesture step on such a photo came to
//! **199.9–306.8 ms** against a 16.7 ms frame. The ruling that followed (2026-09-22)
//! kept the one renderer and gave the preview less to read: this module reduces a
//! photo **once**, [`crate::preview`] caches the result, and every bitmap a preview
//! builds is resampled from that copy instead of from the file's own pixels.
//!
//! What this is, and what it deliberately is not:
//!
//! * **A box average in linear light, not `resample`.** Each output texel is the
//!   area-weighted mean of the source region it covers: one reading pass, no
//!   ringing, no three-lobe kernel, no grading, no flattening. The picture is the
//!   same, there is just less of it. The colour path is the pipeline's own
//!   (`sRGB → linear → average → sRGB`, 16-bit in between) because averaging sRGB
//!   code values is not averaging light: a 2x2 black-and-white checkerboard would
//!   come back at 0.22 instead of 0.5 of its value, and every fine texture would
//!   darken (`AGENTS.md`, "Resampling must happen in the correct color space").
//! * **Exact for an integer factor.** When the destination divides the source, an
//!   output texel is the plain mean of its `k x k` block — the footprints below
//!   align to whole source pixels, so the weights are `1/k²` and the arithmetic is
//!   the obvious one.
//! * **Smaller, never larger.** A photo at or below the requested long edge is
//!   handed back unchanged, because enlarging it would cost memory and buy no
//!   detail — the preview's own resampler would then enlarge it again.
//! * **The photo's aspect is carried, not recomputed.** [`Sampler::aspect`] returns
//!   the *decoded* photo's `width / height` bit for bit, not the reduced buffer's
//!   own ratio: the fit (`CropTransform::fit`) and the region
//!   (`CropTransform::display_region`) are functions of that number, so carrying it
//!   is what makes a preview's geometry **identical** to the export's, with only
//!   the sampling grid differing. Measured: the fit of a crop is the same transform
//!   to the bit (the criterion allowed 1e-9).
//! * **Deterministic.** One thread, a fixed accumulation order, tables built from
//!   the same formulas: the same file reduced twice is the same bytes.
//!
//! The destination size comes from [`crate::thumb::thumb_size`], the same rule a
//! preview's long edge follows, so "a preview-sized copy" means one thing
//! in this crate.
//!
//! # The buffer ladder
//!
//! | Buffer | Size | Lifetime |
//! |---|---|---|
//! | the decoded source | `src_px * 4` bytes (8-bit) or `* 8` (16-bit) | the caller's, one at a time |
//! | the horizontally reduced strip | `BLOCK_ROWS` worth of source rows x `dst_w * 16` bytes | one block |
//! | the reduced photo | `dst_px * 4` bytes or `* 8` | the cache's |
//!
//! The strip is band-bounded exactly as [`resample`](crate::resample) bounds its own
//! rows, so reducing a 24 MP photo into a 1 MP preview does not allocate a second
//! full-size buffer: measured 2026-09-22, the largest band at that ratio is ~9 MB
//! against the 96 MB source being read.

use crate::decode::{Depth, Sampler, Source};
use crate::thumb::thumb_size;
use crate::transfer::{WGHT, linear_to_srgb16, srgb_to_linear_f32, to_fixed};

/// Output rows reduced per block.
///
/// The vertical pass reads the horizontally reduced source rows within its
/// footprint, so a block bounds that buffer — the same split
/// [`resample`](crate::resample) makes with its own `BLOCK_ROWS`, and for the same
/// reason. Measured 2026-09-22 at the largest ratio this module is asked for (a
/// 24 MP photo into a 1 MP preview, 6x): 64 output rows need ~384 source rows of
/// `dst_w * 4` floats, 9 MB at a 1500-px-wide destination.
const BLOCK_ROWS: usize = 64;

/// A decoded photo reduced to the resolution a preview draws at.
///
/// It is a [`Sampler`], so `resample` and `slot_bitmap` take it exactly as they
/// take a [`Source`]: nothing downstream knows the pixels were reduced, which is
/// what keeps the preview inside the one renderer (`AGENTS.md`).
#[derive(Clone, Debug)]
pub struct PreviewSource {
    /// The reduced pixels, in the decoder's own layout — one sample per channel,
    /// **16 bits each** (a reduction, unlike a photo handed over unchanged, is an
    /// intermediate buffer: S15f, PIX-013) — straight sRGB and not premultiplied.
    photo: Source,
    /// The decoded photo's own `width / height`, carried rather than recomputed.
    /// See the module docs: the fit and the region geometry are functions of it.
    aspect: f64,
}

impl PreviewSource {
    /// `source` reduced so that its long edge is `long_edge` pixels.
    ///
    /// `long_edge` at or above the photo's own long edge means no reduction at
    /// all, and the samples are handed over unchanged — bit for bit, which is what
    /// makes "the preview of a photo the preview can already show" cost nothing
    /// but the copy a cache has to own anyway.
    pub fn new(source: &Source, long_edge: u32) -> Self {
        let aspect = source.aspect();
        let (width, height) = thumb_size(source.width(), source.height(), long_edge.max(1));
        let (width, height) = (width.max(1) as u32, height.max(1) as u32);
        if width >= source.width() && height >= source.height() {
            return Self {
                photo: source.clone(),
                aspect,
            };
        }
        let data = reduce(source, width as usize, height as usize);
        Self {
            // **Sixteen bits, whatever the file carried** (S15f, PIX-013): this is
            // an intermediate buffer — the resampler reads it, and the quantization
            // that matters is the final 8-bit write (`AGENTS.md`, "Resampling must
            // happen in the correct color space") — so an 8-bit source reduced here
            // and stored back at 8 bits would quantize the picture *before* the
            // resample that is supposed to be the only lossy step. The copy the
            // cache holds is therefore twice the bytes of an 8-bit photo's own
            // samples, which is what [`MAX_SOURCE_BYTES`](crate::preview) counts. A
            // photo at or below the target is handed over above, untouched: those
            // are the decoder's own samples, not a reduction, and widening them
            // would buy nothing.
            photo: Source::from_samples(source, width, height, Depth::Sixteen, data),
            aspect,
        }
    }

    /// How many bytes this reduction occupies — what the preview's cache budgets.
    pub fn bytes(&self) -> usize {
        self.photo.bytes()
    }

    /// The depth these samples are at: **16 bits** for a reduction, and the file's
    /// own for a photo that was already small enough to hand over unchanged
    /// (S15f, PIX-013).
    pub fn depth(&self) -> Depth {
        self.photo.depth()
    }
}

impl Sampler for PreviewSource {
    fn width(&self) -> u32 {
        self.photo.width()
    }

    fn height(&self) -> u32 {
        self.photo.height()
    }

    fn pixel(&self, x: u32, y: u32) -> [u16; 4] {
        self.photo.pixel(x, y)
    }

    /// The decoded photo's aspect, not this buffer's: see the module docs.
    fn aspect(&self) -> f64 {
        self.aspect
    }

    fn exif(&self) -> Option<&[u8]> {
        self.photo.exif()
    }
}

/// The samples of `source` reduced to `dst_w` x `dst_h`, **16 bits per sample**.
///
/// The output depth is the caller's invariant, not the file's (S15f, PIX-013): the
/// source's depth is read where the pixels come in ([`read`]) and nowhere after it.
fn reduce(source: &Source, dst_w: usize, dst_h: usize) -> Vec<u8> {
    let src_w = source.width() as usize;
    let src_h = source.height() as usize;
    let depth = source.depth();
    let samples = source.samples();
    let sample = match depth {
        Depth::Eight => 1,
        Depth::Sixteen => 2,
    };
    let columns = Weights::new(src_w, dst_w);
    let rows = Weights::new(src_h, dst_h);
    let mut out = vec![0u8; dst_w * dst_h * 4 * 2];
    // One horizontally reduced row per output row a block needs, premultiplied in
    // linear light: the same accumulator layout `resample` uses, for the same
    // reason — alpha has to bound the colour, or a transparent footprint would
    // drag its own undefined colour into the average.
    let mut strip = Vec::<f32>::new();

    let mut block_start = 0usize;
    while block_start < dst_h {
        let block_rows = BLOCK_ROWS.min(dst_h - block_start);
        let first = rows.first(block_start);
        let last = rows.last(block_start + block_rows - 1) + 1;
        strip.resize((last - first) * dst_w * 4, 0.0);

        for (offset, row) in (first..last).enumerate() {
            let line = &mut strip[offset * dst_w * 4..(offset + 1) * dst_w * 4];
            for texel in 0..dst_w {
                let mut sums = [0.0f32; 4];
                for (&x, &weight) in columns.taps(texel) {
                    let pixel = read(samples, sample, src_w, x, row as u32);
                    let alpha = f32::from(pixel[3]) * WGHT;
                    sums[0] += weight * alpha * srgb_to_linear_f32(pixel[0]);
                    sums[1] += weight * alpha * srgb_to_linear_f32(pixel[1]);
                    sums[2] += weight * alpha * srgb_to_linear_f32(pixel[2]);
                    sums[3] += weight * alpha;
                }
                line[texel * 4..texel * 4 + 4].copy_from_slice(&sums);
            }
        }

        for row in 0..block_rows {
            let dst_row = block_start + row;
            let mut sums = vec![0.0f32; dst_w * 4];
            for (&src_row, &weight) in rows.taps(dst_row) {
                let line = &strip[(src_row as usize - first) * dst_w * 4..][..dst_w * 4];
                for (sum, value) in sums.iter_mut().zip(line) {
                    *sum += weight * value;
                }
            }
            for texel in 0..dst_w {
                let alpha = sums[texel * 4 + 3].clamp(0.0, 1.0);
                // Straight from premultiplied: the weights sum to 1 on both axes,
                // so `sums[3]` is the footprint's mean alpha and the quotient is the
                // alpha-weighted mean colour. A fully transparent footprint has no
                // colour to report and says so.
                let straight = |channel: usize| {
                    if alpha > 0.0 {
                        (sums[texel * 4 + channel] / sums[texel * 4 + 3]).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                };
                store(
                    &mut out,
                    (dst_row * dst_w + texel) * 4 * 2,
                    [straight(0), straight(1), straight(2), alpha],
                );
            }
        }

        block_start += block_rows;
    }
    out
}

/// One source pixel, out of the buffer the decoder produced.
///
/// The channel order is the decoder's (`R`, `G`, `B`, `A`) and 16-bit samples are
/// native-endian, exactly as [`Source::pixel`] reads them — this is the same
/// arithmetic without that method's per-call match.
fn read(samples: &[u8], sample: usize, width: usize, x: u32, y: u32) -> [u16; 4] {
    let at = (y as usize * width + x as usize) * 4 * sample;
    let mut out = [0u16; 4];
    for (channel, value) in out.iter_mut().enumerate() {
        let at = at + channel * sample;
        *value = match sample {
            1 => u16::from(samples[at]) * 257,
            _ => u16::from_ne_bytes([samples[at], samples[at + 1]]),
        };
    }
    out
}

/// Writes one output texel: the three colours through the 16-bit sRGB table, the
/// alpha — which is linear coverage, not a transfer-encoded value — rounded to the
/// same width.
fn store(out: &mut [u8], at: usize, rgba: [f32; 4]) {
    for (channel, value) in rgba.iter().enumerate() {
        let fixed = if channel == 3 {
            to_fixed(f64::from(*value))
        } else {
            linear_to_srgb16(to_fixed(f64::from(*value)))
        };
        out[at + channel * 2..at + channel * 2 + 2].copy_from_slice(&fixed.to_ne_bytes());
    }
}

/// One axis's weights: for every destination texel, the source indices it reads
/// and their weights, which sum to one.
///
/// The footprints are the destination texel's own interval in source coordinates
/// (`[j * src / dst, (j + 1) * src / dst)`), so the weights are areas and the
/// destination covers the source exactly once. A destination that divides the
/// source therefore lands on whole pixels with `1/k²` weights — the integer-factor
/// case the module docs describe — and a `dst == src` axis is a plain copy with a
/// single tap of weight 1.
struct Weights {
    start: Vec<u32>,
    indices: Vec<u32>,
    weights: Vec<f32>,
}

impl Weights {
    fn new(src: usize, dst: usize) -> Self {
        let mut start = Vec::with_capacity(dst + 1);
        let mut indices = Vec::new();
        let mut weights = Vec::new();
        for texel in 0..dst {
            start.push(indices.len() as u32);
            let lo = texel as f64 * src as f64 / dst as f64;
            let hi = (texel + 1) as f64 * src as f64 / dst as f64;
            let base = indices.len();
            let mut sum = 0.0f64;
            for k in (lo.floor() as usize)..(hi.ceil() as usize).min(src) {
                let overlap = hi.min((k + 1) as f64) - lo.max(k as f64);
                if overlap <= 0.0 {
                    continue;
                }
                indices.push(k as u32);
                weights.push(overlap as f32);
                sum += overlap;
            }
            // Normalized per destination texel, so a flat area stays exactly flat
            // and the alpha of the footprint is its mean.
            if sum > 0.0 {
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

    fn taps(&self, texel: usize) -> impl Iterator<Item = (&u32, &f32)> {
        let (from, to) = (self.start[texel] as usize, self.start[texel + 1] as usize);
        self.indices[from..to].iter().zip(&self.weights[from..to])
    }

    /// The first source index any of destination texels `..=last` reads.
    fn first(&self, texel: usize) -> usize {
        self.indices[self.start[texel] as usize] as usize
    }

    /// The last source index the destination texel `texel` reads.
    fn last(&self, texel: usize) -> usize {
        self.indices[self.start[texel + 1] as usize - 1] as usize
    }
}
