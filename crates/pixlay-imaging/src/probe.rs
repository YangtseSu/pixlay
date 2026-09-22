//! The probe: numbers instead of "looks right".
//!
//! The probe answers three questions about a render, on the same document the
//! export draws:
//!
//! 1. is the slot's own content where the geometry says it is,
//! 2. is everything the slots do not cover exactly the document's backdrop colour
//!    (`frame.color`, white unless the document says otherwise),
//! 3. how much do two adjacent slots blend into each other along their shared
//!    edge.
//!
//! # Why the probe paints the slots itself
//!
//! A probe of a real render cannot ask these questions. A seam blend is only
//! separable from the content when each side of the seam is one flat color
//! (`docs/CONTRACT.md` §5), and "is the photo where the geometry says it is" has
//! no answer at all when the content is the user's own photo: a legitimately
//! white photo leaves a white interior, and a photo with a hard edge beside a
//! seam has no measurable blend. Measured 2026-09-21 on a real eight-photo
//! project: all 12 seams reported unclean and a slot holding a transparent PNG
//! reported 802 unpainted samples — every one of them a false alarm.
//!
//! So the probe renders **its own flat content**: one color per cell from
//! [`palette`], sized exactly the way the real pipeline sizes a bitmap (the fit's
//! display region). That is the shape S1 froze — "`probe` uses flat content (one
//! color per slot)" — and it is what makes each of the three questions exact
//! rather than statistical. The real pixels are checked by the render itself
//! (`pixlay-render render`), the decoder by `pixlay-render image`, and the whole
//! path by the tests that compare a render against the photo it was given.
//!
//! It lives here rather than in the CLI because the GUI needs the same answers
//! (`AGENTS.md`: nothing may be possible only in the GUI), and because the
//! questions are about pixels, which this crate owns.

use std::path::PathBuf;

use pixlay_core::{CollageDoc, PixelSize, Point, Polygon, Rgba8, SharedEdge};

/// The palette, indexed by cell.
///
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

/// The color the probe paints cell `index` with.
pub fn palette(index: usize) -> Rgba8 {
    let [r, g, b] = PALETTE[index % PALETTE.len()];
    Rgba8::rgb(r, g, b)
}

/// Straight 8-bit RGB pixels, the probe's input.
///
/// A borrow rather than an owned buffer on purpose: a probe of an A0 render would
/// otherwise copy 350 MB to ask its questions. `pixlay-render`'s `Rgb8Image` has
/// exactly these fields, and this crate must not depend on the renderer (which
/// owns cairo) to name its pixels.
pub struct Rgb8View<'a> {
    pub width: i32,
    pub height: i32,
    /// `width * height * 3` bytes, row major.
    pub data: &'a [u8],
}

impl Rgb8View<'_> {
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 3] {
        let index = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[index], self.data[index + 1], self.data[index + 2]]
    }
}

/// Flat bitmaps for the probe, one per occupied cell.
///
/// Sized from the same fit and the same display region as
/// [`crate::slot_bitmap`]: a probe whose bitmaps were sized differently would be
/// measuring a render nobody will ever produce.
pub fn probe_bitmaps(
    doc: &CollageDoc,
    canvas_px: PixelSize,
    sources: &[Option<PathBuf>],
) -> Result<Vec<crate::SlotBitmap>, crate::ImagingError> {
    let mut bitmaps = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        if source.is_some() {
            bitmaps.push(crate::layout::flat_bitmap(
                doc,
                index,
                canvas_px,
                palette(index),
            )?);
        }
    }
    Ok(bitmaps)
}

/// Largest residual, in levels, of a blended seam pixel fitted as a convex
/// combination of white and the two slot colors. Measured 0.2 in S0 (see
/// `docs/CONTRACT.md` §5); a blend with a third color or a filtering smear exceeds it.
const MAX_BLEND_RESIDUAL: f64 = 3.0;

/// Widest run of blended pixels allowed across a seam, in pixels. Measured 1 in
/// S0 at both A0 and 1/5 scale: the blend is the antialiased edge itself.
const MAX_BLEND_RUN: u64 = 2;

/// A blended pixel farther than this from the seam line is not the seam; it
/// would be a filter smearing the edge into a slot.
const SEAM_WINDOW: i32 = 3;

/// One slot's interior sample.
#[derive(Debug)]
pub struct SlotProbe {
    pub slot: usize,
    /// A point well inside the slot, in normalized coordinates.
    pub at: Point,
    pub expected: [u8; 3],
    pub actual: [u8; 3],
    /// Distance from the sample to the outline, in output pixels.
    pub depth_px: f64,
}

impl SlotProbe {
    pub fn matches(&self) -> bool {
        channels_match(self.actual, self.expected)
    }
}

/// Background purity: samples on the canvas that no slot covers, against the
/// document's own backdrop colour (`frame.color`, white unless the document says
/// otherwise).
#[derive(Debug)]
pub struct BackgroundProbe {
    pub samples: u64,
    /// Samples whose pixel is not the backdrop colour.
    pub off_backdrop: u64,
    /// Up to a few offending coordinates, so a failure is localizable.
    pub examples: Vec<(i32, i32, [u8; 3])>,
}

impl BackgroundProbe {
    pub fn is_clean(&self) -> bool {
        self.off_backdrop == 0
    }
}

/// Blending along one shared edge.
#[derive(Debug)]
pub struct SeamProbe {
    pub a: usize,
    pub b: usize,
    /// Rows examined (the seam's span, inset by a guard band at both ends).
    pub rows: u64,
    /// Pixels that are neither the left color, the right color nor the backdrop.
    pub blended: u64,
    /// Widest run of blended pixels in one row.
    pub max_run: u64,
    /// Worst residual of the backdrop/left/right fit over the blended pixels.
    pub max_residual: f64,
    /// Blended pixels the three-layer model cannot explain.
    pub foreign: u64,
    /// Geometric length of the seam, in output pixels.
    pub length_px: f64,
}

impl SeamProbe {
    /// Pixels of blend per pixel of seam. 1.0 is a one-pixel antialiased edge.
    pub fn per_px(&self) -> f64 {
        if self.length_px <= 0.0 {
            return 0.0;
        }
        self.blended as f64 / self.length_px
    }

    pub fn is_clean(&self) -> bool {
        self.foreign == 0
            && self.max_run <= MAX_BLEND_RUN
            && self.max_residual <= MAX_BLEND_RESIDUAL
    }
}

#[derive(Debug)]
pub struct ProbeReport {
    pub width: i32,
    pub height: i32,
    pub slots: usize,
    pub occupied: Vec<usize>,
    pub interiors: Vec<SlotProbe>,
    pub background: BackgroundProbe,
    pub seams: Vec<SeamProbe>,
}

impl ProbeReport {
    /// True when the probe actually examined something.
    ///
    /// Every question the probe answers is about a *cell*: is the photo where the
    /// geometry says, does it blend with its neighbour, does the canvas stay
    /// white around it. A document whose cells are all empty has none of those —
    /// its render is a blank sheet — so reporting `ok` for it would claim a
    /// verdict about content that was never drawn. Before this floor existed, an
    /// all-empty project reported `status = ok`, `occupied = 0`,
    /// `seam.0.blended = 0`, exit 0.
    pub fn is_meaningful(&self) -> bool {
        !self.occupied.is_empty()
    }

    pub fn ok(&self) -> bool {
        self.is_meaningful()
            && self.interiors.iter().all(SlotProbe::matches)
            && self.background.is_clean()
            && self.seams.iter().all(SeamProbe::is_clean)
    }

    /// Why the probe failed, for the summary line on stderr. `None` when it
    /// passed.
    pub fn failure(&self) -> Option<String> {
        if self.ok() {
            return None;
        }
        if !self.is_meaningful() {
            return Some(
                "no cell is occupied, so there is nothing to probe: fill a cell \
                 with a photo, or render a template instead"
                    .to_string(),
            );
        }
        Some(format!(
            "{} background pixel(s) are not the canvas colour, {} of {} slot samples matched, {} of {} seams unclean",
            self.background.off_backdrop,
            self.interiors.iter().filter(|slot| slot.matches()).count(),
            self.interiors.len(),
            self.seams.iter().filter(|seam| !seam.is_clean()).count(),
            self.seams.len()
        ))
    }
}

/// Samples `image`, which must be a full-size render of `doc` painted with [`palette`].
pub fn probe(doc: &CollageDoc, image: &Rgb8View<'_>) -> ProbeReport {
    let canvas = pixlay_core::PixelSize {
        width: image.width,
        height: image.height,
    };
    let mut occupied = Vec::new();
    let mut interiors = Vec::new();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        let filled = doc
            .cells
            .get(index)
            .map(|cell| cell.source.is_some())
            .unwrap_or(false);
        if !filled {
            continue;
        }
        let expected = rgb_of(palette(index));
        let at = deepest_point(&slot.outline);
        let (x, y) = pixel_of(at, canvas);
        let actual = image.pixel(x, y);
        interiors.push(SlotProbe {
            slot: index,
            at,
            expected,
            actual,
            depth_px: slot.outline.distance_to_boundary(at) * f64::from(canvas.width),
        });
        occupied.push(index);
    }

    let background = sample_background(doc, image);
    let seams = doc
        .template
        .shared_edges()
        .into_iter()
        .map(|seam| sample_seam(&seam, doc, image, canvas))
        .collect();
    ProbeReport {
        width: image.width,
        height: image.height,
        slots: doc.template.slots.len(),
        occupied,
        interiors,
        background,
        seams,
    }
}

/// Every probe row is sampled on a stride, so an A0 probe stays in the seconds
/// range while still catching a wedge or a band of contamination.
fn stride(width: i32) -> i32 {
    (width / 1200).max(1)
}

fn sample_background(doc: &CollageDoc, image: &Rgb8View<'_>) -> BackgroundProbe {
    let step = stride(image.width);
    let backdrop = rgb_of(doc.frame.color);
    let mut probe = BackgroundProbe {
        samples: 0,
        off_backdrop: 0,
        examples: Vec::new(),
    };
    for y in (0..image.height).step_by(step as usize) {
        for x in (0..image.width).step_by(step as usize) {
            let point = Point::new(
                f64::from(x) / f64::from(image.width),
                f64::from(y) / f64::from(image.height),
            );
            let covered = doc.template.slots.iter().enumerate().any(|(index, slot)| {
                doc.cells
                    .get(index)
                    .map(|cell| cell.source.is_some())
                    .unwrap_or(false)
                    && slot.outline.contains(point)
            });
            if covered {
                continue;
            }
            // Sampling on a lattice can land inside a slot while the normalized
            // point falls just outside it; the outline is antialiased, so only
            // ask pixels that are unambiguously covered by a slot.
            if near_slot(doc, point, image) {
                continue;
            }
            probe.samples += 1;
            let pixel = image.pixel(x, y);
            if pixel != backdrop {
                probe.off_backdrop += 1;
                if probe.examples.len() < 8 {
                    probe.examples.push((x, y, pixel));
                }
            }
        }
    }
    probe
}

/// True when `point` is within two pixels of a slot boundary: those pixels are
/// legitimately blended and are not background samples.
fn near_slot(doc: &CollageDoc, point: Point, image: &Rgb8View<'_>) -> bool {
    let margin_x = 2.0 / f64::from(image.width);
    let margin_y = 2.0 / f64::from(image.height);
    doc.template.slots.iter().enumerate().any(|(index, slot)| {
        if doc
            .cells
            .get(index)
            .map(|cell| cell.source.is_none())
            .unwrap_or(true)
        {
            return false;
        }
        slot.outline.distance_to_boundary(point) <= margin_x.max(margin_y) * 2.0
    })
}

fn sample_seam(
    seam: &SharedEdge,
    doc: &CollageDoc,
    image: &Rgb8View<'_>,
    canvas: pixlay_core::PixelSize,
) -> SeamProbe {
    // Only two painted slots share a blend. A seam beside an empty cell is the
    // canvas edge, and probing it as a seam would report a blend between a slot
    // and white.
    let occupied = |index: usize| {
        doc.cells
            .get(index)
            .map(|cell| cell.source.is_some())
            .unwrap_or(false)
    };
    let left = rgb_of(palette(seam.a));
    let right = rgb_of(palette(seam.b));
    let backdrop = rgb_of(doc.frame.color);
    let (x0, y0) = pixel_of_f(seam.from, canvas);
    let (x1, y1) = pixel_of_f(seam.to, canvas);
    let length_px = (x1 - x0).hypot(y1 - y0);

    let mut probe = SeamProbe {
        a: seam.a,
        b: seam.b,
        rows: 0,
        blended: 0,
        max_run: 0,
        max_residual: 0.0,
        foreign: 0,
        length_px,
    };

    if !occupied(seam.a) || !occupied(seam.b) {
        return probe;
    }

    // Walk the seam row by row (or column by column for a horizontal seam),
    // skipping a guard band at both ends where the seam meets a corner.
    let guard = 2.0;
    let (dx, dy) = (x1 - x0, y1 - y0);
    let steps = length_px.round().max(1.0) as i32;
    let vertical = dy.abs() >= dx.abs();
    for step in 0..steps {
        let t = (f64::from(step) + 0.5) / f64::from(steps);
        if t * length_px < guard || (1.0 - t) * length_px < guard {
            continue;
        }
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        probe.rows += 1;
        let mut run = 0u64;
        for offset in -SEAM_WINDOW..=SEAM_WINDOW {
            let (x, y) = if vertical {
                ((cx.round() as i32) + offset, cy.round() as i32)
            } else {
                (cx.round() as i32, (cy.round() as i32) + offset)
            };
            if x < 0 || y < 0 || x >= image.width || y >= image.height {
                continue;
            }
            let pixel = image.pixel(x, y);
            if channels_match(pixel, left) || channels_match(pixel, right) {
                run = 0;
                continue;
            }
            if pixel == backdrop {
                // Pure backdrop inside the seam window means the two slots do not
                // actually meet here (a frame's gap, most often).
                run = 0;
                continue;
            }
            probe.blended += 1;
            run += 1;
            probe.max_run = probe.max_run.max(run);
            let residual = blend_residual(pixel, left, right, backdrop);
            probe.max_residual = probe.max_residual.max(residual);
            if residual > MAX_BLEND_RESIDUAL {
                probe.foreign += 1;
            }
        }
    }
    probe
}

/// A blended seam pixel is covered by three layers — the canvas backdrop and the
/// two slot colors — so it must be a convex combination of them. A pixel
/// contaminated by anything else (a third color, a filter smear) cannot be
/// explained and leaves a large residual.
///
/// "Between the two colors" is not the criterion: a seam pixel blended with the
/// *backdrop* lands outside the interval of the two colors, which is exactly what
/// S0 measured for red/blue seams.
fn blend_residual(pixel: [u8; 3], left: [u8; 3], right: [u8; 3], backdrop: [u8; 3]) -> f64 {
    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    let channel = |value: [u8; 3]| {
        [
            f64::from(value[0]) - f64::from(backdrop[0]),
            f64::from(value[1]) - f64::from(backdrop[1]),
            f64::from(value[2]) - f64::from(backdrop[2]),
        ]
    };
    let (u, v, d) = (channel(left), channel(right), channel(pixel));
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
    let rest = 1.0 - a - b;
    let mut residual = 0.0f64;
    for c in 0..3 {
        let fitted =
            rest * f64::from(backdrop[c]) + a * f64::from(left[c]) + b * f64::from(right[c]);
        residual = residual.max((fitted - f64::from(pixel[c])).abs());
    }
    residual
}

/// The point inside `outline` farthest from its boundary, found on a coarse grid
/// then refined. For an L-shaped slot the bounding-box centre is outside the
/// outline, so the sample point cannot come from the bbox.
fn deepest_point(outline: &Polygon) -> Point {
    let bbox = outline.bbox();
    let mut best = (f64::NEG_INFINITY, bbox.center());
    let mut grid = 33;
    let mut lo = (bbox.x0, bbox.y0);
    let mut hi = (bbox.x1, bbox.y1);
    for _ in 0..8 {
        let (step_x, step_y) = (
            (hi.0 - lo.0) / f64::from(grid),
            (hi.1 - lo.1) / f64::from(grid),
        );
        for iy in 0..=grid {
            for ix in 0..=grid {
                let point =
                    Point::new(lo.0 + step_x * f64::from(ix), lo.1 + step_y * f64::from(iy));
                if !outline.contains(point) {
                    continue;
                }
                let depth = outline.distance_to_boundary(point);
                if depth > best.0 {
                    best = (depth, point);
                }
            }
        }
        // Zoom into the winning cell.
        let (cx, cy) = (best.1.x, best.1.y);
        lo = (cx - step_x, cy - step_y);
        hi = (cx + step_x, cy + step_y);
        grid = 8;
    }
    best.1
}

fn pixel_of(point: Point, canvas: pixlay_core::PixelSize) -> (i32, i32) {
    let (x, y) = pixel_of_f(point, canvas);
    (
        (x as i32).clamp(0, canvas.width - 1),
        (y as i32).clamp(0, canvas.height - 1),
    )
}

fn pixel_of_f(point: Point, canvas: pixlay_core::PixelSize) -> (f64, f64) {
    (
        point.x * f64::from(canvas.width),
        point.y * f64::from(canvas.height),
    )
}

fn rgb_of(color: Rgba8) -> [u8; 3] {
    [color.r, color.g, color.b]
}

/// Slot samples are taken well inside the outline, away from the antialiased
/// boundary, so they must match the slot color exactly. Measured 0 differing
/// channels since S1 (2026-09-20); a nonzero tolerance would hide a blend that
/// has crept into the interior.
fn channels_match(a: [u8; 3], b: [u8; 3]) -> bool {
    a == b
}
