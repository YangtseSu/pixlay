//! S0 spike: can Cairo render an A0 sheet at 300 dpi?
//!
//! Disposable by design (docs/STEPS.md, S0): hardcoded layout, no document
//! model, no template library, no framing math. Its only job is to produce the
//! numbers that decide whether Cairo stays, and a preview image a human can
//! look at. S1 replaces it with the single `render::draw(doc, target)` and the
//! `probe` subcommand.
//!
//! What it exercises, per the S0 exit criteria plus the review supplement:
//! opaque white base, an irregular polygon clip, a photo blitted through an
//! affine transform, a rotated CJK text line, PNG and JPEG output, and pixel
//! probes that turn "looks right" into numbers (background purity, slot
//! coverage, uncovered-area white, seam blending, text ink ratio).
//!
//! Usage:
//!
//! ```text
//! a0-spike [--size WxH] [--slots 2|10] [--content flat|detail]
//!          [--out PREFIX] [--jpeg-quality Q] [--preview-px N] [--skip-encode]
//! ```
//!
//! stdout carries machine-readable `key = value` lines only; diagnostics go to
//! stderr. Exit codes: 0 all checks pass, 1 a check failed, 2 setup or IO error.

use std::env;
use std::fmt::Display;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Matrix, Operator, SurfacePattern};
use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;
use pangocairo::pango::FontDescription;

/// A0 at 300 dpi. The whole point of the step is this size.
const A0_W: i32 = 9933;
const A0_H: i32 = 14043;

/// Normalized layout box the slots live in. The margin and the bottom band stay
/// white canvas, which is where the background probes sample.
const CONTENT_LEFT: f64 = 0.02;
const CONTENT_RIGHT: f64 = 0.98;
const CONTENT_TOP: f64 = 0.02;
const CONTENT_BOTTOM: f64 = 0.84;

/// Right edge of the irregular slot; also the seam shared with its neighbour.
const SEAM_X: f64 = 0.50;

/// The strip slot's photo is pushed down by this fraction of the canvas height,
/// leaving a bare strip inside the clip: "uncovered area inside a slot is
/// white".
const SLOT1_SHIFT: f64 = 0.04;

const FONT_FAMILY: &str = "Noto Sans CJK SC";
/// Text size as a fraction of canvas height (normalized sizing is an AGENTS
/// hard constraint; the spike keeps it).
const FONT_SIZE_REL: f64 = 0.02;
const TEXT_ORIGIN: (f64, f64) = (0.06, 0.90);
/// Small angle keeps the ink box inside the bottom band.
const TEXT_ROTATION_DEG: f64 = 6.0;
const TEXT: &str = "拼图 Pixlay A0 出货检查 2026";

/// Per-slot colors for `--content flat`: far apart, so a blended seam pixel is
/// identifiable and any cross-slot mixup is visible.
const PALETTE: [[u8; 3]; 10] = [
    [220, 40, 40],
    [40, 70, 220],
    [40, 180, 70],
    [230, 200, 40],
    [220, 50, 200],
    [40, 200, 210],
    [240, 130, 30],
    [120, 60, 200],
    [30, 140, 120],
    [150, 90, 50],
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Content {
    Flat,
    Detail,
}

impl Content {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "flat" => Ok(Self::Flat),
            "detail" => Ok(Self::Detail),
            other => Err(format!("--content must be flat or detail, got {other}")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Detail => "detail",
        }
    }
}

#[derive(Clone, Debug)]
struct Config {
    width: i32,
    height: i32,
    slots: usize,
    content: Content,
    out: PathBuf,
    jpeg_quality: u8,
    preview_px: i32,
    encode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: A0_W,
            height: A0_H,
            slots: 2,
            content: Content::Detail,
            out: PathBuf::from("/var/tmp/pixlay-s0/a0"),
            jpeg_quality: 90,
            preview_px: 0,
            encode: true,
        }
    }
}

impl Config {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut cfg = Self::default();
        let mut next = 0;
        while let Some(arg) = args.get(next).cloned() {
            next += 1;
            let mut value = |name: &str| -> Result<String, String> {
                let v = args
                    .get(next)
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value"))?;
                next += 1;
                Ok(v)
            };
            match arg.as_str() {
                "--size" => {
                    let v = value("--size")?;
                    let (w, h) = v
                        .split_once('x')
                        .ok_or_else(|| format!("--size wants WxH, got {v}"))?;
                    cfg.width = w.parse().map_err(|e| format!("--size width: {e}"))?;
                    cfg.height = h.parse().map_err(|e| format!("--size height: {e}"))?;
                }
                "--slots" => {
                    let v = value("--slots")?;
                    cfg.slots = v.parse().map_err(|e| format!("--slots: {e}"))?;
                }
                "--content" => cfg.content = Content::parse(&value("--content")?)?,
                "--out" => cfg.out = PathBuf::from(value("--out")?),
                "--jpeg-quality" => {
                    let v = value("--jpeg-quality")?;
                    cfg.jpeg_quality = v.parse().map_err(|e| format!("--jpeg-quality: {e}"))?;
                }
                "--preview-px" => {
                    let v = value("--preview-px")?;
                    cfg.preview_px = v.parse().map_err(|e| format!("--preview-px: {e}"))?;
                }
                "--skip-encode" => cfg.encode = false,
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        if cfg.width < 64 || cfg.height < 64 {
            return Err(format!("--size too small: {}x{}", cfg.width, cfg.height));
        }
        if !matches!(cfg.slots, 2 | 10) {
            return Err(format!("--slots must be 2 or 10, got {}", cfg.slots));
        }
        Ok(cfg)
    }
}

/// One template cell: a normalized polygon plus how its photo is placed.
#[derive(Clone, Debug)]
struct Slot {
    poly: Vec<(f64, f64)>,
    /// Probe point well inside the part of the slot the photo covers.
    inside: (f64, f64),
    /// Probe point inside the clip that the photo deliberately leaves bare.
    uncovered: Option<(f64, f64)>,
    /// Cover-fit zoom; > 1 keeps the photo covering the whole bounding box.
    zoom: f64,
    /// Downward shift of the photo as a fraction of canvas height.
    shift_y: f64,
    /// Rotation of the photo about its own center, in degrees.
    rotation_deg: f64,
    color: [u8; 3],
}

impl Slot {
    fn bbox(&self) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in &self.poly {
            b.0 = b.0.min(x);
            b.1 = b.1.min(y);
            b.2 = b.2.max(x);
            b.3 = b.3.max(y);
        }
        b
    }

    fn rotated(&self) -> bool {
        self.rotation_deg != 0.0
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// The spike's template: the slots plus the row span over which the shared edge
/// is clean (both neighbours fully covered), so a blended pixel there can only
/// be the seam itself.
struct Layout {
    slots: Vec<Slot>,
    seam_span: (f64, f64),
}

/// Hardcoded layouts. 2 slots: irregular polygon plus one rectangle across the
/// seam. 10 slots: the same irregular polygon plus a 3x3 grid.
fn layout(count: usize) -> Result<Layout, String> {
    // Non-convex L shape: its right edge stays a straight vertical line at
    // SEAM_X, so the shared edge is a single uninterrupted seam.
    let irregular = vec![
        (0.10, CONTENT_TOP),
        (SEAM_X, CONTENT_TOP),
        (SEAM_X, CONTENT_BOTTOM),
        (0.28, CONTENT_BOTTOM),
        (0.28, 0.52),
        (0.10, 0.52),
    ];
    let mut out = vec![Slot {
        poly: irregular,
        inside: (0.20, 0.20),
        uncovered: None,
        zoom: 1.15,
        shift_y: 0.0,
        rotation_deg: 0.0,
        color: PALETTE[0],
    }];

    let cells: Vec<Vec<(f64, f64)>> = match count {
        2 => vec![rect(SEAM_X, CONTENT_TOP, CONTENT_RIGHT, CONTENT_BOTTOM)],
        10 => {
            let mut v = Vec::new();
            for row in 0..3 {
                for col in 0..3 {
                    let x0 = SEAM_X + (CONTENT_RIGHT - SEAM_X) * f64::from(col) / 3.0;
                    let x1 = SEAM_X + (CONTENT_RIGHT - SEAM_X) * f64::from(col + 1) / 3.0;
                    let y0 = CONTENT_TOP + (CONTENT_BOTTOM - CONTENT_TOP) * f64::from(row) / 3.0;
                    let y1 =
                        CONTENT_TOP + (CONTENT_BOTTOM - CONTENT_TOP) * f64::from(row + 1) / 3.0;
                    v.push(rect(x0, y0, x1, y1));
                }
            }
            v
        }
        other => return Err(format!("--slots must be 2 or 10, got {other}")),
    };

    // Slot 1 sits across the seam and has to stay clean (no rotation, and in
    // the 10-slot layout no shift either), otherwise its edge leaves the seam
    // and "mixed pixel" stops meaning "seam blend". The slot that demonstrates
    // a bare strip inside its clip is index 1 in the 2-slot layout and index 2
    // in the grid layout.
    let strip_index = if count == 2 { 1 } else { 2 };
    let seam_span = if count == 2 {
        (0.10, CONTENT_BOTTOM - 0.01)
    } else {
        // Only the first grid row borders the seam, and the cell below it is
        // rotated, so stop before that wedge reaches the seam.
        (0.10, 0.27)
    };

    for (i, poly) in cells.into_iter().enumerate() {
        let index = i + 1;
        let (x0, y0, x1, y1) = bbox_of(&poly);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let mut slot = Slot {
            poly,
            inside: (cx, cy),
            uncovered: None,
            // Small cover margin: with Extend::None a photo whose edge lands
            // exactly on a slot edge samples transparent black there, which
            // would masquerade as seam bleed.
            zoom: 1.02,
            shift_y: 0.0,
            // Rotated cells leave a bare wedge at a corner: a photo fitted to
            // the cell's bounding box cannot cover the corners of the cell it
            // was fitted to once it is rotated.
            rotation_deg: if index == 1 || index == strip_index {
                0.0
            } else if index % 2 == 0 {
                8.0
            } else {
                -8.0
            },
            color: PALETTE[index],
        };
        if index == strip_index {
            slot.shift_y = SLOT1_SHIFT;
            slot.inside = (cx, y0 + 0.08);
            slot.uncovered = Some((cx, y0 + 0.025));
        }
        out.push(slot);
    }
    Ok(Layout {
        slots: out,
        seam_span,
    })
}

fn bbox_of(poly: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in poly {
        b.0 = b.0.min(x);
        b.1 = b.1.min(y);
        b.2 = b.2.max(x);
        b.3 = b.3.max(y);
    }
    b
}

/// Even-odd ray casting on normalized coordinates.
fn poly_contains(poly: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut prev = match poly.last() {
        Some(&p) => p,
        None => return false,
    };
    for &(xi, yi) in poly {
        let (xj, yj) = prev;
        if (yi > y) != (yj > y) {
            let x_at = (xj - xi) * (y - yi) / (yj - yi) + xi;
            if x < x_at {
                inside = !inside;
            }
        }
        prev = (xi, yi);
    }
    inside
}

/// Deterministic 64-bit hash in `[0, 1)`.
fn hash01(x: u32, y: u32, seed: u64) -> f64 {
    let mut h = seed
        ^ u64::from(x).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(y).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 29;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

struct Photo {
    surface: ImageSurface,
    /// Luminance standard deviation: proof the content is not flat.
    luma_stddev: f64,
}

/// Photos are generated at the size the slot displays them (the buffer ladder in
/// the S4 notes: resample to slot display size, then blit). Upscaling would
/// smooth the content and make the encode numbers meaningless.
fn photo_dims(cfg: &Config, slot: &Slot) -> (i32, i32) {
    let (x0, y0, x1, y1) = slot.bbox();
    let w = ((x1 - x0) * f64::from(cfg.width))
        .round()
        .clamp(64.0, 16384.0);
    let h = ((y1 - y0) * f64::from(cfg.height))
        .round()
        .clamp(64.0, 16384.0);
    (w as i32, h as i32)
}

fn make_photo(cfg: &Config, slot: &Slot, index: usize) -> Result<Photo, String> {
    let (w, h) = photo_dims(cfg, slot);
    let stride = w as usize * 4;
    let mut buf = vec![0u8; stride * h as usize];
    let seed = 0x5DEE_CE66_D1CE_u64.wrapping_mul(index as u64 + 1) ^ 0x1234_5678_9ABC_DEF0;

    let luma_stddev = match cfg.content {
        Content::Flat => {
            for px in buf.as_chunks_mut::<4>().0 {
                px[0] = slot.color[2];
                px[1] = slot.color[1];
                px[2] = slot.color[0];
                px[3] = 255;
            }
            0.0
        }
        Content::Detail => {
            let mut sum = 0.0f64;
            let mut sum_sq = 0.0f64;
            let (seed_lo, seed_hi) = (seed, seed ^ 0xA5A5_5A5A_1234_5678);
            for y in 0..h {
                for x in 0..w {
                    // Coarse structure from one hash per 6x6 block, per-pixel
                    // grain from one hash per pixel. The grain is what carries
                    // the entropy the encoder has to deal with.
                    let block = hash01((x / 6) as u32, (y / 6) as u32, seed_lo);
                    let grain = hash01(x as u32, y as u32, seed_hi);
                    // Hard high-frequency banding adds sharp edges on top.
                    let band = if (x / 5 + y / 5) % 7 == 0 { 1.0 } else { 0.45 };
                    let lum = (0.22 + 0.55 * block + 0.20 * grain) * band;
                    let mut rgb = [0u8; 3];
                    for (out, base) in rgb.iter_mut().zip(slot.color) {
                        *out = (f64::from(base) * lum).min(255.0) as u8;
                    }
                    let i = y as usize * stride + x as usize * 4;
                    buf[i] = rgb[2];
                    buf[i + 1] = rgb[1];
                    buf[i + 2] = rgb[0];
                    buf[i + 3] = 255;
                    let luma = f64::from(rgb[0]) * 0.299
                        + f64::from(rgb[1]) * 0.587
                        + f64::from(rgb[2]) * 0.114;
                    sum += luma;
                    sum_sq += luma * luma;
                }
            }
            let count = f64::from(w) * f64::from(h);
            let mean = sum / count;
            (sum_sq / count - mean * mean).max(0.0).sqrt()
        }
    };

    let surface = ImageSurface::create_for_data(buf, Format::ARgb32, w, h, stride as i32)
        .map_err(|e| format!("photo surface {w}x{h}: {e}"))?;
    Ok(Photo {
        surface,
        luma_stddev,
    })
}

fn poly_path(ctx: &Context, poly: &[(f64, f64)], w: f64, h: f64) {
    ctx.new_path();
    for (i, &(x, y)) in poly.iter().enumerate() {
        if i == 0 {
            ctx.move_to(x * w, y * h);
        } else {
            ctx.line_to(x * w, y * h);
        }
    }
    ctx.close_path();
}

/// Blit through a composed affine matrix: translate to the slot center, rotate,
/// scale, then move the photo's own center onto the origin.
///
/// Cairo's pattern matrix maps user space to *pattern* space, so the placement
/// matrix built here is inverted before it is handed over.
fn blit(ctx: &Context, photo: &ImageSurface, slot: &Slot, w: f64, h: f64) -> Result<(), String> {
    let (x0, y0, x1, y1) = slot.bbox();
    let (bx0, by0, bx1, by1) = (x0 * w, y0 * h, x1 * w, y1 * h);
    let (pw, ph) = (f64::from(photo.width()), f64::from(photo.height()));
    let scale = f64::max((bx1 - bx0) / pw, (by1 - by0) / ph) * slot.zoom;
    let (cx, cy) = ((bx0 + bx1) / 2.0, (by0 + by1) / 2.0 + slot.shift_y * h);

    let mut matrix = Matrix::identity();
    matrix.translate(cx, cy);
    if slot.rotated() {
        matrix.rotate(slot.rotation_deg.to_radians());
    }
    matrix.scale(scale, scale);
    matrix.translate(-pw / 2.0, -ph / 2.0);
    let matrix = matrix.try_invert().map_err(|e| e.to_string())?;

    let pattern = SurfacePattern::create(photo);
    pattern.set_matrix(matrix);
    pattern.set_filter(Filter::Good);
    // Outside the photo stays transparent, so the white base shows through.
    pattern.set_extend(Extend::None);
    ctx.set_source(&pattern).map_err(|e| e.to_string())?;
    ctx.paint().map_err(|e| e.to_string())
}

fn draw_text(ctx: &Context, cfg: &Config) -> Result<(), String> {
    let (w, h) = (f64::from(cfg.width), f64::from(cfg.height));
    let layout = pangocairo::functions::create_layout(ctx);
    let mut font = FontDescription::new();
    font.set_family(FONT_FAMILY);
    // pango's absolute size is in device pixels times PANGO_SCALE.
    font.set_absolute_size(FONT_SIZE_REL * h * f64::from(pangocairo::pango::SCALE));
    layout.set_font_description(Some(&font));
    layout.set_text(TEXT);

    ctx.save().map_err(|e| e.to_string())?;
    ctx.translate(TEXT_ORIGIN.0 * w, TEXT_ORIGIN.1 * h);
    ctx.rotate(TEXT_ROTATION_DEG.to_radians());
    ctx.set_source_rgb(0.05, 0.05, 0.08);
    pangocairo::functions::show_layout(ctx, &layout);
    ctx.restore().map_err(|e| e.to_string())
}

/// Compose the whole sheet: opaque white base, one clipped blit per slot, then
/// the canvas-level text layer.
fn compose(cfg: &Config, slots: &[Slot], photos: &[Photo]) -> Result<ImageSurface, String> {
    let surface = ImageSurface::create(Format::ARgb32, cfg.width, cfg.height)
        .map_err(|e| format!("canvas {}x{}: {e}", cfg.width, cfg.height))?;
    let ctx = Context::new(&surface).map_err(|e| e.to_string())?;

    // The export is never transparent, and pixels no photo covers are white.
    ctx.set_operator(Operator::Source);
    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.paint().map_err(|e| e.to_string())?;
    ctx.set_operator(Operator::Over);

    let (w, h) = (f64::from(cfg.width), f64::from(cfg.height));
    for (slot, photo) in slots.iter().zip(photos) {
        ctx.save().map_err(|e| e.to_string())?;
        poly_path(&ctx, &slot.poly, w, h);
        ctx.clip();
        blit(&ctx, &photo.surface, slot, w, h)?;
        ctx.restore().map_err(|e| e.to_string())?;
    }
    draw_text(&ctx, cfg)?;
    surface.flush();
    Ok(surface)
}

/// Read-only view over the BGRA-premultiplied surface. Everything drawn here is
/// opaque, so premultiplied and straight color coincide.
struct Pixels<'a> {
    data: &'a [u8],
    stride: usize,
    width: i32,
    height: i32,
}

impl Pixels<'_> {
    fn rgb(&self, x: i32, y: i32) -> [u8; 3] {
        let i = y as usize * self.stride + x as usize * 4;
        [self.data[i + 2], self.data[i + 1], self.data[i]]
    }

    fn at(&self, p: (f64, f64)) -> [u8; 3] {
        let x = ((p.0 * f64::from(self.width)) as i32).clamp(0, self.width - 1);
        let y = ((p.1 * f64::from(self.height)) as i32).clamp(0, self.height - 1);
        self.rgb(x, y)
    }
}

fn is_white(px: [u8; 3]) -> bool {
    px == [255, 255, 255]
}

fn close_to(px: [u8; 3], want: [u8; 3], tol: u8) -> bool {
    (0..3).all(|c| px[c].abs_diff(want[c]) <= tol)
}

/// A seam pixel is covered by three layers: the white base, the left slot and
/// the right slot. Fit the pixel as a convex combination of those and return the
/// residual; a pixel contaminated by anything else (a third color, a filter
/// smear) cannot be explained and leaves a large residual.
fn blend_residual(p: [u8; 3], left: [u8; 3], right: [u8; 3]) -> f64 {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let u = [
        f64::from(left[0]) - 255.0,
        f64::from(left[1]) - 255.0,
        f64::from(left[2]) - 255.0,
    ];
    let v = [
        f64::from(right[0]) - 255.0,
        f64::from(right[1]) - 255.0,
        f64::from(right[2]) - 255.0,
    ];
    let d = [
        f64::from(p[0]) - 255.0,
        f64::from(p[1]) - 255.0,
        f64::from(p[2]) - 255.0,
    ];
    let (uu, uv, vv) = (dot(u, u), dot(u, v), dot(v, v));
    let (du, dv) = (dot(d, u), dot(d, v));
    let det = uu * vv - uv * uv;
    let (mut a, mut b) = if det.abs() < 1e-6 {
        (0.0, 0.0)
    } else {
        ((du * vv - uv * dv) / det, (uu * dv - uv * du) / det)
    };
    a = a.clamp(0.0, 1.0);
    b = b.clamp(0.0, 1.0);
    if a + b > 1.0 {
        let sum = a + b;
        a /= sum;
        b /= sum;
    }
    let white = 1.0 - a - b;
    let mut residual = 0.0f64;
    for c in 0..3 {
        let fitted = white * 255.0 + a * f64::from(left[c]) + b * f64::from(right[c]);
        residual = residual.max((fitted - f64::from(p[c])).abs());
    }
    residual
}

#[derive(Debug)]
struct Seam {
    rows: u64,
    mixed: u64,
    /// Widest run of blended pixels in any single row: a filter that smears the
    /// seam shows up here, a 1 px antialiased edge does not.
    max_run: u64,
    /// Worst residual of the white/left/right fit over all blended pixels.
    max_residual: f64,
    /// Blended pixels the three-layer model cannot explain.
    foreign: u64,
}

#[derive(Debug)]
struct Ink {
    px: u64,
    bbox: (i32, i32, i32, i32),
    ratio: f64,
    margins: (i32, i32, i32, i32),
}

#[derive(Debug, Default)]
struct Probe {
    bg_nonwhite: u64,
    bg_samples: u64,
    bg_bbox: Option<(i32, i32, i32, i32)>,
    bg_examples: Vec<(i32, i32, [u8; 3])>,
    slot_white: Vec<u64>,
    slot_samples: Vec<u64>,
    /// Mean absolute luma step between horizontally adjacent pixels inside the
    /// slots: the rendered equivalent of "this content is not flat", which is
    /// what makes encode time and size representative.
    activity: f64,
    activity_samples: u64,
    inside: Vec<[u8; 3]>,
    uncovered: Vec<Option<[u8; 3]>>,
    corner: Vec<[u8; 3]>,
    band: Vec<[u8; 3]>,
    seam: Option<Seam>,
    ink: Option<Ink>,
}

fn probe(cfg: &Config, surface: &ImageSurface, layout: &Layout) -> Result<Probe, String> {
    let slots = &layout.slots;
    let stride = surface.stride() as usize;
    let (w, h) = (cfg.width, cfg.height);
    let mut out = Probe {
        slot_white: vec![0; slots.len()],
        slot_samples: vec![0; slots.len()],
        ..Probe::default()
    };

    // Coarse enough to stay fast at A0, fine enough to catch a bare wedge.
    let step = (w / 1200).max(1);
    // Slot edges are antialiased, so the outermost pixel of the content box is
    // a blend, not background: keep a guard band around it.
    let content_top = (CONTENT_TOP * f64::from(h)) as i32 - 2;
    let content_bottom = (CONTENT_BOTTOM * f64::from(h)) as i32 + 2;
    let content_left = (CONTENT_LEFT * f64::from(w)) as i32 - 2;
    let content_right = (CONTENT_RIGHT * f64::from(w)) as i32 + 2;
    let band_top = (0.85 * f64::from(h)) as i32;
    let seam_x = (SEAM_X * f64::from(w)) as i32;
    let seam_top = (layout.seam_span.0 * f64::from(h)) as i32;
    let seam_bottom = (layout.seam_span.1 * f64::from(h)) as i32;
    let left_color = slots[0].color;
    let right_color = slots[1].color;

    let mut seam: Option<Seam> = None;
    let mut ink: Option<Ink> = None;
    let mut activity_sum = 0.0f64;
    let mut activity_samples = 0u64;

    surface
        .with_data(|data| {
            let px = Pixels {
                data,
                stride,
                width: w,
                height: h,
            };

            // Outside every slot the sheet must be pure white; the text band is
            // checked separately through its ink bounding box.
            for y in (0..band_top).step_by(step as usize) {
                let in_content_row = (content_top..content_bottom).contains(&y);
                for x in (0..w).step_by(step as usize) {
                    let in_content_col = (content_left..content_right).contains(&x);
                    if in_content_row && in_content_col {
                        continue;
                    }
                    out.bg_samples += 1;
                    if !is_white(px.rgb(x, y)) {
                        out.bg_nonwhite += 1;
                        let bbox = out.bg_bbox.get_or_insert((x, y, x, y));
                        bbox.0 = bbox.0.min(x);
                        bbox.1 = bbox.1.min(y);
                        bbox.2 = bbox.2.max(x);
                        bbox.3 = bbox.3.max(y);
                        if out.bg_examples.len() < 8 {
                            out.bg_examples.push((x, y, px.rgb(x, y)));
                        }
                    }
                }
            }

            for (i, slot) in slots.iter().enumerate() {
                let (x0, y0, x1, y1) = slot.bbox();
                let xa = (x0 * f64::from(w)) as i32 - 1;
                let xb = (x1 * f64::from(w)) as i32 + 1;
                let ya = (y0 * f64::from(h)) as i32 - 1;
                let yb = (y1 * f64::from(h)) as i32 + 1;
                for y in (ya..yb).step_by(step as usize) {
                    for x in (xa..xb).step_by(step as usize) {
                        let (nx, ny) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
                        if !poly_contains(&slot.poly, nx, ny) {
                            continue;
                        }
                        out.slot_samples[i] += 1;
                        if is_white(px.rgb(x, y)) {
                            out.slot_white[i] += 1;
                        }
                        // Local activity, both endpoints inside the clip.
                        let (nx1, ny1) =
                            (f64::from(x + 1) / f64::from(w), f64::from(y) / f64::from(h));
                        if x + 1 < xb && poly_contains(&slot.poly, nx1, ny1) {
                            let luma = |p: [u8; 3]| {
                                f64::from(p[0]) * 0.299
                                    + f64::from(p[1]) * 0.587
                                    + f64::from(p[2]) * 0.114
                            };
                            let here = luma(px.rgb(x, y));
                            let next = luma(px.rgb(x + 1, y));
                            activity_sum += (here - next).abs();
                            activity_samples += 1;
                        }
                    }
                }
                out.inside.push(px.at(slot.inside));
                out.uncovered.push(slot.uncovered.map(|p| px.at(p)));
            }

            for p in [
                (0.004, 0.004),
                (0.996, 0.004),
                (0.004, 0.996),
                (0.996, 0.996),
            ] {
                out.corner.push(px.at(p));
            }
            for p in [(0.25, 0.86), (0.6, 0.87)] {
                out.band.push(px.at(p));
            }

            // Seam: with flat slot colors a blended pixel matches neither
            // neighbour, so it is countable.
            if cfg.content == Content::Flat {
                let mut s = Seam {
                    rows: 0,
                    mixed: 0,
                    max_run: 0,
                    max_residual: 0.0,
                    foreign: 0,
                };
                for y in seam_top..seam_bottom {
                    s.rows += 1;
                    let mut run = 0;
                    for x in (seam_x - 4)..=(seam_x + 4) {
                        let p = px.rgb(x, y);
                        if close_to(p, left_color, 6) || close_to(p, right_color, 6) {
                            run = 0;
                            continue;
                        }
                        s.mixed += 1;
                        run += 1;
                        s.max_run = s.max_run.max(run);
                        let residual = blend_residual(p, left_color, right_color);
                        s.max_residual = s.max_residual.max(residual);
                        if residual > 12.0 {
                            s.foreign += 1;
                        }
                    }
                }
                seam = Some(s);
            }

            // Text ink: scan the band below the content box.
            let mut min_x = w;
            let mut min_y = h;
            let mut max_x = -1;
            let mut max_y = -1;
            let mut count = 0u64;
            for y in band_top..h {
                for x in 0..w {
                    let p = px.rgb(x, y);
                    let luma =
                        f64::from(p[0]) * 0.299 + f64::from(p[1]) * 0.587 + f64::from(p[2]) * 0.114;
                    if luma < 128.0 {
                        count += 1;
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                }
            }
            if count > 0 {
                let area = f64::from((max_x - min_x + 1) * (max_y - min_y + 1));
                ink = Some(Ink {
                    px: count,
                    bbox: (min_x, min_y, max_x, max_y),
                    ratio: count as f64 / area,
                    margins: (min_y - band_top, h - 1 - max_y, min_x, w - 1 - max_x),
                });
            }
        })
        .map_err(|e| format!("surface data: {e}"))?;

    out.seam = seam;
    out.ink = ink;
    out.activity_samples = activity_samples;
    if activity_samples > 0 {
        out.activity = activity_sum / activity_samples as f64;
    }
    Ok(out)
}

fn encode_png(surface: &ImageSurface, path: &Path) -> Result<(u64, f64), String> {
    let start = Instant::now();
    let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    surface
        .write_to_png(&mut BufWriter::new(file))
        .map_err(|e| format!("png {}: {e}", path.display()))?;
    let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    Ok((bytes, start.elapsed().as_secs_f64() * 1000.0))
}

/// Cairo writes PNG only, and its PNG writer drops pHYs/iCCP, so JPEG comes
/// from an encoder we control. Subsampling and metadata belong to S6.
fn encode_jpeg(surface: &ImageSurface, path: &Path, quality: u8) -> Result<(u64, f64), String> {
    let (w, h) = (surface.width(), surface.height());
    let stride = surface.stride() as usize;
    let mut rgb = vec![0u8; w as usize * h as usize * 3];
    surface
        .with_data(|data| {
            for y in 0..h as usize {
                let src = y * stride;
                let dst = y * w as usize * 3;
                for x in 0..w as usize {
                    let i = src + x * 4;
                    rgb[dst + x * 3] = data[i + 2];
                    rgb[dst + x * 3 + 1] = data[i + 1];
                    rgb[dst + x * 3 + 2] = data[i];
                }
            }
        })
        .map_err(|e| format!("surface data: {e}"))?;

    let start = Instant::now();
    let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    let mut encoder = JpegEncoder::new_with_quality(BufWriter::new(file), quality);
    encoder
        .encode(&rgb, w as u32, h as u32, ExtendedColorType::Rgb8)
        .map_err(|e| format!("jpeg {}: {e}", path.display()))?;
    let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    Ok((bytes, start.elapsed().as_secs_f64() * 1000.0))
}

/// A0 cannot be reviewed whole, so a scaled copy is written for the human eye.
fn write_preview(
    surface: &ImageSurface,
    path: &Path,
    preview_px: i32,
) -> Result<(i32, i32), String> {
    let scale = f64::from(preview_px) / f64::from(surface.width());
    let pw = preview_px;
    let ph = ((f64::from(surface.height()) * scale).round() as i32).max(1);
    let small = ImageSurface::create(Format::ARgb32, pw, ph).map_err(|e| e.to_string())?;
    let ctx = Context::new(&small).map_err(|e| e.to_string())?;
    ctx.set_operator(Operator::Source);
    ctx.scale(scale, scale);
    let pattern = SurfacePattern::create(surface);
    pattern.set_filter(Filter::Best);
    ctx.set_source(&pattern).map_err(|e| e.to_string())?;
    ctx.paint().map_err(|e| e.to_string())?;
    let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    small
        .write_to_png(&mut BufWriter::new(file))
        .map_err(|e| format!("png {}: {e}", path.display()))?;
    Ok((pw, ph))
}

fn vmhwm_mb() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: f64 = rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .unwrap_or(0.0);
            return kb / 1024.0;
        }
    }
    0.0
}

#[derive(Debug)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Debug, Default)]
struct Report {
    lines: Vec<(String, String)>,
    checks: Vec<Check>,
}

impl Report {
    fn line(&mut self, key: impl Display, value: impl Display) {
        self.lines.push((key.to_string(), value.to_string()));
    }

    fn check(&mut self, name: &'static str, ok: bool, detail: impl Display) {
        self.checks.push(Check {
            name,
            ok,
            detail: detail.to_string(),
        });
    }

    fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    fn render(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        for (k, v) in &self.lines {
            let _ = writeln!(s, "{k} = {v}");
        }
        for c in &self.checks {
            let _ = writeln!(
                s,
                "check.{} = {} ({})",
                c.name,
                if c.ok { "ok" } else { "FAIL" },
                c.detail
            );
        }
        let _ = writeln!(s, "verdict = {}", if self.all_ok() { "ok" } else { "fail" });
        s
    }
}

fn run(cfg: &Config) -> Result<Report, String> {
    let layout = layout(cfg.slots)?;
    let slots = &layout.slots;
    if let Some(dir) = cfg.out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }

    let photos_start = Instant::now();
    let photos: Vec<Photo> = slots
        .iter()
        .enumerate()
        .map(|(i, slot)| make_photo(cfg, slot, i))
        .collect::<Result<_, _>>()?;
    let ms_photos = photos_start.elapsed().as_secs_f64() * 1000.0;
    let photos_bytes: u64 = photos
        .iter()
        .map(|p| p.surface.stride() as u64 * p.surface.height() as u64)
        .sum();
    let min_stddev = photos
        .iter()
        .map(|p| p.luma_stddev)
        .fold(f64::MAX, f64::min);

    let start = Instant::now();
    let surface = compose(cfg, slots, &photos)?;
    let ms_compose = start.elapsed().as_secs_f64() * 1000.0;
    let vmhwm_compose = vmhwm_mb();

    let start = Instant::now();
    let probed = probe(cfg, &surface, &layout)?;
    let ms_probe = start.elapsed().as_secs_f64() * 1000.0;

    let mut report = Report::default();
    report.line("size", format!("{}x{}", cfg.width, cfg.height));
    report.line("slots", cfg.slots);
    report.line("content", cfg.content.name());
    report.line("ms_photos", format!("{ms_photos:.1}"));
    report.line("ms_compose", format!("{ms_compose:.1}"));
    report.line("ms_probe", format!("{ms_probe:.1}"));
    report.line("photos_bytes", photos_bytes);
    report.line("canvas_bytes", cfg.width as u64 * cfg.height as u64 * 4);
    report.line("vmhwm_mb_compose", format!("{vmhwm_compose:.1}"));
    report.line("photo_stddev_min", format!("{min_stddev:.2}"));
    report.line("render_activity", format!("{:.2}", probed.activity));
    report.line("render_activity_samples", probed.activity_samples);

    // Threshold source: 2026-09-20, 2483x3510 probe runs — detail content
    // measures 3.61 (2 slots) and 4.83 (10 slots) luma per pixel, flat content
    // measures 0.00. 2.0 separates the two with margin.
    report.check(
        "content_non_flat",
        cfg.content == Content::Flat || probed.activity > 2.0,
        match cfg.content {
            Content::Flat => "n/a (flat mode feeds the color probes)".to_string(),
            Content::Detail => format!("luma step {:.2}/px", probed.activity),
        },
    );
    // Budget from docs/STEPS.md (S0 review): A0 compose peak <= 2.5 GB.
    report.check(
        "compose_memory_budget",
        vmhwm_compose <= 2560.0,
        format!("{vmhwm_compose:.1} MB VmHWM"),
    );

    // Background: every sampled pixel outside the slots is pure white.
    report.line("bg_samples", probed.bg_samples);
    report.line("bg_nonwhite", probed.bg_nonwhite);
    if let Some(b) = probed.bg_bbox {
        report.line(
            "bg_nonwhite_bbox",
            format!("{},{},{},{}", b.0, b.1, b.2, b.3),
        );
        report.line("bg_nonwhite_examples", format!("{:?}", probed.bg_examples));
    }
    report.check(
        "bg_white",
        probed.bg_nonwhite == 0 && probed.bg_samples > 0,
        format!("{}/{} non-white", probed.bg_nonwhite, probed.bg_samples),
    );

    // Slot coverage: a slot whose photo is meant to cover it has no white pixel
    // at all; the deliberately under-covered ones do.
    for (i, slot) in slots.iter().enumerate() {
        let white = probed.slot_white[i];
        let samples = probed.slot_samples[i];
        report.line(
            format!("slot{i}_white_samples"),
            format!("{white}/{samples}"),
        );
        let under_covered = slot.rotated() || slot.uncovered.is_some();
        if under_covered {
            report.check(
                if slot.rotated() {
                    "rotated_slot_bare_corner"
                } else {
                    "slot_bare_strip"
                },
                white > 0,
                format!("slot{i}: {white}/{samples} white"),
            );
        } else {
            report.check(
                "slot_fully_covered",
                white == 0 && samples > 0,
                format!("slot{i}: {white}/{samples} white"),
            );
        }
    }

    // Point probes: interior color, deliberately bare area, canvas corners.
    let mut interior_ok = true;
    for (i, slot) in slots.iter().enumerate() {
        let px = probed.inside[i];
        let ok = match cfg.content {
            Content::Flat => close_to(px, slot.color, 6),
            Content::Detail => !is_white(px),
        };
        interior_ok &= ok;
        report.line(
            format!("slot{i}_inside_rgb"),
            format!("{},{},{}", px[0], px[1], px[2]),
        );
    }
    report.check(
        "slot_interior",
        interior_ok,
        format!("{} slots probed", slots.len()),
    );

    let mut bare_ok = true;
    for (i, px) in probed.uncovered.iter().enumerate() {
        if let Some(px) = px {
            bare_ok &= is_white(*px);
            report.line(
                format!("slot{i}_uncovered_rgb"),
                format!("{},{},{}", px[0], px[1], px[2]),
            );
        }
    }
    report.check("uncovered_is_white", bare_ok, "strip inside slot 1");

    let corner_ok = probed.corner.iter().all(|p| is_white(*p));
    let band_ok = probed.band.iter().all(|p| is_white(*p));
    report.check("canvas_corners_white", corner_ok, "4 corners");
    report.check("canvas_band_white", band_ok, "2 band points");

    // Seam: only measurable when the two slot colors are known constants.
    if let Some(seam) = &probed.seam {
        let ratio = seam.mixed as f64 / seam.rows as f64;
        report.line("seam_rows", seam.rows);
        report.line("seam_mixed_px", seam.mixed);
        report.line("seam_mixed_per_row", format!("{ratio:.3}"));
        report.line("seam_max_run_px", seam.max_run);
        report.line("seam_max_residual", format!("{:.2}", seam.max_residual));
        report.line("seam_foreign_px", seam.foreign);
        report.check(
            "seam_blend_bounded",
            ratio <= 2.0,
            format!("{ratio:.3} px/row"),
        );
        report.check("seam_no_wide_bleed", seam.max_run <= 2, seam.max_run);
        report.check(
            "seam_explained_by_three_layers",
            seam.foreign == 0,
            format!(
                "{} unexplained, worst residual {:.2}",
                seam.foreign, seam.max_residual
            ),
        );
    } else {
        report.line("seam_mixed_per_row", "n/a");
        report.check("seam_blend_bounded", true, "n/a for detail content");
    }

    // Text: readable means ink exists, is dense enough to be glyphs rather than
    // a blob, and stays inside the band.
    match &probed.ink {
        Some(ink) => {
            let (x0, y0, x1, y1) = ink.bbox;
            report.line("text_ink_px", ink.px);
            report.line("text_ink_ratio", format!("{:.4}", ink.ratio));
            report.line(
                "text_bbox",
                format!("{x0},{y0},{x1},{y1} ({}x{})", x1 - x0 + 1, y1 - y0 + 1),
            );
            report.line(
                "text_margins_px",
                format!(
                    "{},{},{},{}",
                    ink.margins.0, ink.margins.1, ink.margins.2, ink.margins.3
                ),
            );
            let font_px = FONT_SIZE_REL * f64::from(cfg.height);
            let (bw, bh) = (f64::from(x1 - x0 + 1), f64::from(y1 - y0 + 1));
            report.check(
                "text_ink_ratio",
                (0.02..=0.60).contains(&ink.ratio),
                format!("{:.4}", ink.ratio),
            );
            report.check(
                "text_glyph_scale",
                bw >= 2.0 * font_px && bh >= 0.4 * font_px,
                format!("{bw:.0}x{bh:.0} px at font {font_px:.0}"),
            );
            report.check(
                "text_inside_band",
                [ink.margins.0, ink.margins.1, ink.margins.2, ink.margins.3]
                    .iter()
                    .all(|m| *m >= 4),
                format!("margins {:?}", ink.margins),
            );
        }
        None => report.check("text_ink_ratio", false, "no ink found in band"),
    }

    // Encoding.
    if cfg.encode {
        let png_path = cfg.out.with_extension("png");
        let (png_bytes, png_ms) = encode_png(&surface, &png_path)?;
        report.line("ms_png", format!("{png_ms:.1}"));
        report.line("bytes_png", png_bytes);
        let jpg_path = cfg.out.with_extension("jpg");
        let (jpg_bytes, jpg_ms) = encode_jpeg(&surface, &jpg_path, cfg.jpeg_quality)?;
        report.line("ms_jpeg", format!("{jpg_ms:.1}"));
        report.line("bytes_jpeg", jpg_bytes);
        report.line("jpeg_quality", cfg.jpeg_quality);
        report.line("vmhwm_mb_final", format!("{:.1}", vmhwm_mb()));
        let dims =
            |p: &Path| image::image_dimensions(p).map_err(|e| format!("{}: {e}", p.display()));
        let png_dims = dims(&png_path)?;
        let jpg_dims = dims(&jpg_path)?;
        report.line("png_dims", format!("{}x{}", png_dims.0, png_dims.1));
        report.line("jpg_dims", format!("{}x{}", jpg_dims.0, jpg_dims.1));
        report.check(
            "encoded_dimensions",
            png_dims == (cfg.width as u32, cfg.height as u32)
                && jpg_dims == (cfg.width as u32, cfg.height as u32),
            format!("png {png_dims:?} jpg {jpg_dims:?}"),
        );
    } else {
        report.line("vmhwm_mb_final", format!("{:.1}", vmhwm_mb()));
    }

    if cfg.preview_px > 0 {
        let path = cfg.out.with_extension("preview.png");
        let (pw, ph) = write_preview(&surface, &path, cfg.preview_px)?;
        report.line("preview", format!("{} {pw}x{ph}", path.display()));
    }

    Ok(report)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cfg = match Config::parse(args) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("a0-spike: {e}");
            return ExitCode::from(2);
        }
    };
    let started = Instant::now();
    match run(&cfg) {
        Ok(report) => {
            print!("{}", report.render());
            eprintln!(
                "a0-spike: {} slots, {} content, total {:.0} ms",
                cfg.slots,
                cfg.content.name(),
                started.elapsed().as_secs_f64() * 1000.0
            );
            if report.all_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("a0-spike: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(content: Content, slots: usize) -> Config {
        Config {
            width: A0_W / 4,
            height: A0_H / 4,
            slots,
            content,
            out: PathBuf::from("/var/tmp/pixlay-s0-test/a0"),
            encode: true,
            ..Config::default()
        }
    }

    fn run_ok(cfg: &Config) {
        let report = run(cfg).expect("spike run");
        assert!(report.all_ok(), "checks failed:\n{}", report.render());
    }

    /// Cheap correctness gate: the probes cover white base, slot coverage,
    /// uncovered white, seam blending and text ink at a size that runs fast.
    #[test]
    fn flat_two_slots_probes_pass() {
        run_ok(&small(Content::Flat, 2));
    }

    #[test]
    fn flat_ten_slots_probes_pass() {
        run_ok(&small(Content::Flat, 10));
    }

    /// Detail content cannot be seam-probed (colors are not constants) but must
    /// still render, encode, and keep the background clean.
    #[test]
    fn detail_two_slots_probes_pass() {
        run_ok(&small(Content::Detail, 2));
    }

    #[test]
    fn detail_ten_slots_probes_pass() {
        run_ok(&small(Content::Detail, 10));
    }

    #[test]
    fn report_carries_the_numbers_the_decision_needs() {
        let cfg = small(Content::Detail, 2);
        let report = run(&cfg).expect("spike run");
        let text = report.render();
        for key in [
            "ms_compose",
            "vmhwm_mb_compose",
            "ms_png",
            "bytes_png",
            "ms_jpeg",
            "bytes_jpeg",
            "text_ink_ratio",
            "verdict",
        ] {
            assert!(
                text.contains(&format!("{key} = ")),
                "missing {key}:\n{text}"
            );
        }
        assert!(text.contains("verdict = ok"), "{text}");
    }
}
