//! Deterministic placeholder content.
//!
//! S1 has no decoder — that is S4 — so a cell's photo cannot be read yet. Until
//! then every occupied cell is filled with content generated from the slot
//! index, which is what makes the CLI, the probes and the golden tests
//! end-to-end exercises of the geometry, the clip and the output path.
//!
//! This module and the `--content` flag are deleted in S4, when
//! `pixlay-imaging` supplies the real pixels.

use std::path::PathBuf;

use pixlay_core::{CollageDoc, Rgba8};
use pixlay_render::{Bitmap, Images};

use crate::cli::Failure;

/// Which placeholder content to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Structured, non-flat content: what a preview should look like, and what
    /// catches a wrong scale or rotation.
    Detail,
    /// One solid color per slot. The probes use this mode: a seam blend is only
    /// separable from the content when the content is flat.
    Flat,
}

impl Mode {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "detail" => Some(Self::Detail),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Detail => "detail",
            Self::Flat => "flat",
        }
    }
}

/// Ten colors far apart from each other, so a mixed pixel between two slots is
/// identifiable and a swapped cell is obvious.
const PALETTE: [[u8; 3]; 10] = [
    [200, 30, 40],
    [30, 160, 60],
    [40, 60, 220],
    [230, 170, 20],
    [150, 40, 190],
    [20, 190, 190],
    [240, 120, 60],
    [90, 90, 90],
    [10, 60, 120],
    [120, 200, 40],
];

pub fn color(index: usize) -> Rgba8 {
    let [r, g, b] = PALETTE[index % PALETTE.len()];
    Rgba8::rgb(r, g, b)
}

/// Which cells get placeholder content.
pub enum Fill<'a> {
    /// Only the cells that name a photo. The rest stay white.
    NamedCells(&'a [Option<PathBuf>]),
    /// Every slot: the photo-free smoke render of a template.
    AllSlots,
}

impl Fill<'_> {
    fn covers(&self, index: usize) -> bool {
        match self {
            Self::NamedCells(sources) => sources.get(index).and_then(Option::as_ref).is_some(),
            Self::AllSlots => true,
        }
    }
}

/// One bitmap per covered cell, sized the way `pixlay-imaging` will size them:
/// at the size the slot displays them, so the canvas only has to blit.
pub fn images(doc: &CollageDoc, fill: Fill<'_>, mode: Mode, dpi: u32) -> Result<Images, Failure> {
    let canvas = doc
        .canvas
        .pixel_size(dpi)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    let mut images = Images::new();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        if !fill.covers(index) {
            continue;
        }
        let crop = doc
            .cells
            .get(index)
            .map(|cell| cell.crop)
            .unwrap_or_default();
        let bbox = slot.outline.bbox();
        // One pixel of slack on each axis: the placeholder must cover the slot
        // outright, and the displayed size is never an integer.
        let width = (bbox.width() * f64::from(canvas.width) * crop.zoom).ceil() as i32 + 1;
        let height = (bbox.height() * f64::from(canvas.height) * crop.zoom).ceil() as i32 + 1;
        let bitmap = match mode {
            Mode::Flat => Bitmap::filled(width, height, color(index)),
            Mode::Detail => detail_bitmap(index, width, height),
        };
        images.insert(
            index,
            bitmap.map_err(|error| Failure::Failed(error.to_string()))?,
        );
    }
    Ok(images)
}

/// Coarse blocks plus per-pixel grain: non-flat at both scales, deterministic,
/// and cheap.
fn detail_bitmap(
    index: usize,
    width: i32,
    height: i32,
) -> Result<Bitmap, pixlay_render::RenderError> {
    let base = PALETTE[index % PALETTE.len()];
    let seed = 0x9e37_79b9_7f4a_7c15u64.wrapping_mul(index as u64 + 1);
    let mut data = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height {
        for x in 0..width {
            let coarse = hash01((x / 16) as u32, (y / 16) as u32, seed);
            let fine = hash01((x / 2) as u32, (y / 2) as u32, seed ^ 0x517c_c1b7_2722_0a95);
            let ramp = 0.8 + 0.2 * (f64::from(x) / f64::from(width.max(1)));
            let level = (0.45 + 0.55 * (0.75 * coarse + 0.25 * fine)) * ramp;
            let index = (y as usize * width as usize + x as usize) * 4;
            // ARgb32: premultiplied, byte order B, G, R, A on little-endian. The
            // content is opaque, so premultiplied and straight color agree.
            for (channel, byte) in base.iter().enumerate() {
                data[index + 2 - channel] = (f64::from(*byte) * level).clamp(0.0, 255.0) as u8;
            }
            data[index + 3] = 255;
        }
    }
    Bitmap::from_argb32(width, height, data)
}

/// Deterministic value in `[0, 1)`; the same generator the S0 spike used.
fn hash01(x: u32, y: u32, seed: u64) -> f64 {
    let mut h = seed ^ (u64::from(x) << 32) ^ u64::from(y);
    h = h.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 29;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}
