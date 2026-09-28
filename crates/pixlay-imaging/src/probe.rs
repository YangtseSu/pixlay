// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The probe: numbers instead of "looks right".
//!
//! The probe answers three questions about a render, on the same document the
//! export draws:
//!
//! 1. is the slot's own content where the geometry says it is,
//! 2. is everything the slots do not cover exactly the document's backdrop colour
//!    (`frame.color`, white unless the document says otherwise),
//! 3. how much do two adjacent slots blend into each other along their shared
//!    edge,
//! 4. is the frame's gap the width the document claims, between two photos and
//!    between a photo and the sheet's edge (S20).
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

/// How far a stripe's sample is kept away from a rounded corner, beyond the radius
/// itself, in pixels.
///
/// The arc's antialiasing reaches about a pixel past the arc's own extent, and a
/// blended pixel is neither photo, so it counts into the stripe. Measured (S20): a
/// column 0.5 px inside the flat span measured 42 px against the frame's 40.
const CORNER_EDGE_PX: f64 = 2.0;

/// How far a measured gap stripe may sit from the frame's own number, in pixels.
///
/// The measurement is the stripe's **pixel span** — the pixels between the last one
/// that is exactly the first photo's colour and the first that is exactly the
/// second's — and so it is the geometric width rounded *outward*: a boundary that
/// falls a hundredth of a pixel inside a pixel adds that whole pixel, one at each
/// end. Measured in S20 over the library's six template families at `gapRel`
/// 0.01–0.08 (radius 0 and 0.03) and grids 709–1417, 42 runs: the widest deviation
/// from the stripe the geometry leaves is **1.96 px**, and a border stripe never
/// measured below its number and at most 0.96 px above it.
const GAP_TOLERANCE_PX: f64 = 2.0;

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

/// Blending along one shared edge, and the frame's stripe across it.
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
    /// The frame's stripe across the seam: the run between the two photos, over
    /// the rows the stripe is a stripe on (S20) — the extremes of those runs, and
    /// zero when none was measurable.
    pub gap_min_px: u64,
    pub gap_max_px: u64,
    /// Rows the stripe was measured on.
    pub gap_rows: u64,
    /// Rows where no stripe is separable: the run there meets a crossing seam, a
    /// rounded corner or the canvas's edge instead of the two photos.
    pub gap_skipped: u64,
    /// The worst row where the measured stripe missed the stripe the two cells'
    /// *geometry* leaves there, in pixels.
    ///
    /// The geometry is asked per row rather than once per seam because a concave slot
    /// reaches less far than its visible rectangle's box says: the frame insets the
    /// cell's bounding box (`Frame::inset`), so an interior edge of the outline — the
    /// notch of `mosaic-8-s14`'s L — keeps its own place and the stripe beside it is
    /// the neighbour's half alone. That is the template's geometry as much as the
    /// frame's number is the frame's, so the row is judged against what the geometry
    /// leaves, and the frame's own number stays visible as `GapProbe::expected_px`.
    pub gap_dev_px: f64,
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

    /// True when every measured stripe across this seam was the width the geometry
    /// leaves there, or when no row of it was measurable.
    ///
    /// A seam beside an empty cell has no stripe to measure at all (the two
    /// photos do not meet), which is why "nothing measured" is not a failure —
    /// the interior probe and the background probe are what judge that cell.
    pub fn gap_is_ok(&self) -> bool {
        self.gap_rows == 0 || self.gap_dev_px <= GAP_TOLERANCE_PX
    }
}

/// One side of the sheet's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    /// The four sides, in the order the probe reports them.
    pub const ALL: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

    /// The name the CLI prints, and the word a diagnostic would use.
    pub fn name(self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Right => "right",
            Side::Bottom => "bottom",
            Side::Left => "left",
        }
    }
}

/// The run of backdrop from one side of the sheet to the outermost photo that
/// reaches it.
///
/// The frame's number is the distance between two photos *and* the distance from
/// the photos to the sheet's edge (ruled 2026-09-25, S20), so this is the second
/// half of the same claim. One sample per cell per side, at the middle of the cell's
/// own visible span, where the cell's **outline** reaches the canvas's edge — there
/// the distance from the sheet to the photo is the frame's own, and it has to be the
/// frame's number.
#[derive(Debug)]
pub struct BorderProbe {
    pub side: Side,
    pub min_px: u64,
    pub max_px: u64,
    /// Samples that found the photo this side is about.
    pub samples: u64,
    /// Samples where it was not found: the slot's outline does not reach that
    /// point, or the walk ran off the canvas before any of its own color.
    pub skipped: u64,
}

impl BorderProbe {
    /// True when the border's stripe is `expected_px` wide, or when no photo
    /// reaches this side at all.
    pub fn is_ok(&self, expected_px: f64) -> bool {
        self.samples == 0 || within_tolerance(self.min_px, self.max_px, expected_px)
    }
}

/// The frame's own number, and what the sheet's own edge shows for it.
#[derive(Debug)]
pub struct GapProbe {
    /// `frame.gapRel × canvas height`: the number the document claims, which is what a
    /// border stripe must measure. A *seam* is judged against the stripe the two
    /// cells' geometry leaves there, which is this number unless a concave slot's
    /// notch is the boundary (`SeamProbe::gap_dev_px`).
    pub expected_px: f64,
    pub borders: Vec<BorderProbe>,
}

impl GapProbe {
    pub fn is_ok(&self) -> bool {
        self.borders
            .iter()
            .all(|border| border.is_ok(self.expected_px))
    }
}

/// True when every measurement of a stripe is within [`GAP_TOLERANCE_PX`] of the
/// number the frame claims. `min`/`max` are over the measurements of one stripe.
fn within_tolerance(min_px: u64, max_px: u64, expected_px: f64) -> bool {
    min_px as f64 >= expected_px - GAP_TOLERANCE_PX
        && max_px as f64 <= expected_px + GAP_TOLERANCE_PX
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
    /// The frame's number and what the sheet's own edge shows for it (S20).
    pub gap: GapProbe,
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
            && self.seams.iter().all(SeamProbe::gap_is_ok)
            && self.gap.is_ok()
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
            "{} background pixel(s) are not the canvas colour, {} of {} slot samples matched, {} of {} seams unclean, {} of {} gap measurements are not the frame's number ({:.1} px)",
            self.background.off_backdrop,
            self.interiors.iter().filter(|slot| slot.matches()).count(),
            self.interiors.len(),
            self.seams.iter().filter(|seam| !seam.is_clean()).count(),
            self.seams.len(),
            self.gap_failures(),
            self.gap_measurements(),
            self.gap.expected_px
        ))
    }

    /// Stripe measurements that are not the frame's number: one per seam whose
    /// stripe is wrong, and one per side of the sheet whose border stripe is.
    fn gap_failures(&self) -> usize {
        self.seams.iter().filter(|seam| !seam.gap_is_ok()).count()
            + self
                .gap
                .borders
                .iter()
                .filter(|border| !border.is_ok(self.gap.expected_px))
                .count()
    }

    /// Stripe measurements, counted the same way as [`Self::gap_failures`], so the
    /// two numbers in the message are comparable.
    fn gap_measurements(&self) -> usize {
        self.seams.len() + self.gap.borders.len()
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
        // The sample is taken inside the region the frame leaves *visible*: the point
        // farthest from that region's own boundary. Before S20 the search was over the
        // outline alone, so a cell at the sheet's edge at a large gap could be sampled
        // inside the frame's band — where the backdrop is, and rightly so.
        let Some(visible) = doc.frame.covering(slot, canvas.aspect()) else {
            continue;
        };
        let at = deepest_point(&visible);
        let (x, y) = pixel_of(at, canvas);
        let actual = image.pixel(x, y);
        interiors.push(SlotProbe {
            slot: index,
            at,
            expected,
            actual,
            depth_px: visible.distance_to_boundary(at) * f64::from(canvas.width),
        });
        occupied.push(index);
    }

    let background = sample_background(doc, image);
    let seams: Vec<SeamProbe> = doc
        .template
        .shared_edges()
        .into_iter()
        .map(|seam| sample_seam(&seam, doc, image, canvas))
        .collect();
    // The frame's own number, and the sheet's own edge read against it (S20).
    let gap = GapProbe {
        expected_px: doc.frame.gap_rel * f64::from(canvas.height),
        borders: sample_borders(doc, image, canvas),
    };
    ProbeReport {
        width: image.width,
        height: image.height,
        slots: doc.template.slots.len(),
        occupied,
        interiors,
        background,
        seams,
        gap,
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
        gap_min_px: 0,
        gap_max_px: 0,
        gap_rows: 0,
        gap_skipped: 0,
        gap_dev_px: 0.0,
    };

    if !occupied(seam.a) || !occupied(seam.b) {
        return probe;
    }

    // The frame's stripe across this seam, and the span of it the stripe is the
    // frame's: a rounded corner recedes from the cell's own edge, so the samples
    // between the flat parts of the two visible rectangles are the ones that measure
    // the number (S20). The blend walk keeps its own guard; this one is only about
    // the stripe.
    let aspect = canvas.aspect();
    let vertical_axis = (y1 - y0).abs() >= (x1 - x0).abs();
    let (mut flat_lo, mut flat_hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for index in [seam.a, seam.b] {
        let Some(slot) = doc.template.slots.get(index) else {
            continue;
        };
        let (rect, radius) = doc.frame.clip(slot, aspect);
        let radius_px = radius * f64::from(canvas.height);
        let (lo, hi, axis_px) = if vertical_axis {
            (rect.y0, rect.y1, f64::from(canvas.height))
        } else {
            (rect.x0, rect.x1, f64::from(canvas.width))
        };
        flat_lo = flat_lo.max(lo * axis_px + radius_px + CORNER_EDGE_PX);
        flat_hi = flat_hi.min(hi * axis_px - radius_px - CORNER_EDGE_PX);
    }

    // Walk the seam row by row (or column by column for a horizontal seam),
    // skipping a guard band at both ends where the seam meets a corner.
    let guard = 2.0;
    let (dx, dy) = (x1 - x0, y1 - y0);
    let steps = length_px.round().max(1.0) as i32;
    let vertical = vertical_axis;
    for step in 0..steps {
        let t = (f64::from(step) + 0.5) / f64::from(steps);
        if t * length_px < guard || (1.0 - t) * length_px < guard {
            continue;
        }
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        probe.rows += 1;
        // The frame's stripe where both cells' visible edges are flat: along the
        // seam that means between their own corners.
        let along = if vertical { cy } else { cx };
        if along >= flat_lo && along <= flat_hi {
            let expected = expected_stripe(doc, seam, vertical, along, canvas);
            let measured = if vertical {
                let y = cy.round() as i32;
                stripe_px(image.width, cx.round() as i32, left, right, |i| {
                    image.pixel(i, y)
                })
            } else {
                let x = cx.round() as i32;
                stripe_px(image.height, cy.round() as i32, left, right, |i| {
                    image.pixel(x, i)
                })
            };
            match (measured, expected) {
                (Some(width), Some(expected)) => {
                    observe(
                        &mut probe.gap_min_px,
                        &mut probe.gap_max_px,
                        &mut probe.gap_rows,
                        width,
                    );
                    probe.gap_dev_px = probe.gap_dev_px.max((width as f64 - expected).abs());
                }
                _ => probe.gap_skipped += 1,
            }
        } else {
            probe.gap_skipped += 1;
        }
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

/// Files one stripe measurement, seeding the extremes on the first.
fn observe(min_px: &mut u64, max_px: &mut u64, rows: &mut u64, width: u64) {
    if *rows == 0 {
        *min_px = width;
        *max_px = width;
    } else {
        *min_px = (*min_px).min(width);
        *max_px = (*max_px).max(width);
    }
    *rows += 1;
}

/// The stripe the two cells' geometry leaves across one seam, on one row, in pixels:
/// the distance between the two visible polygons along the direction the stripe is
/// measured in.
///
/// `None` when either cell's visible region does not reach that row at all — the two
/// photos do not face each other there, so there is no stripe between them to check.
fn expected_stripe(
    doc: &CollageDoc,
    seam: &SharedEdge,
    vertical: bool,
    along: f64,
    canvas: PixelSize,
) -> Option<f64> {
    let aspect = canvas.aspect();
    let mut spans = Vec::new();
    for index in [seam.a, seam.b] {
        let slot = doc.template.slots.get(index)?;
        let visible = doc.frame.covering(slot, aspect)?;
        // The coordinate the stripe is measured in, for this row: x when the seam
        // runs down the sheet, y when it runs across it.
        let scale = if vertical {
            f64::from(canvas.width)
        } else {
            f64::from(canvas.height)
        };
        let line = crossings(&visible, vertical, along, canvas);
        if line.is_empty() {
            return None;
        }
        let lo = line.iter().cloned().fold(f64::INFINITY, f64::min) * scale;
        let hi = line.iter().cloned().fold(f64::NEG_INFINITY, f64::max) * scale;
        spans.push((lo, hi));
    }
    // The nearer edges of the two regions: the highest lower bound against the lowest
    // upper bound, which is the stripe whichever cell is on which side.
    let (a, b) = (spans[0], spans[1]);
    let stripe = a.0.max(b.0) - a.1.min(b.1);
    (stripe >= 0.0).then_some(stripe)
}

/// Where a polygon's boundary crosses one line of the grid, as coordinates along the
/// line's direction, normalized.
///
/// `vertical` means the seam runs down the sheet, so the line is the **row** `along`
/// and the coordinates are x. An edge that lies on the line contributes both of its
/// ends.
fn crossings(polygon: &Polygon, vertical: bool, along: f64, canvas: PixelSize) -> Vec<f64> {
    let line = if vertical {
        along / f64::from(canvas.height)
    } else {
        along / f64::from(canvas.width)
    };
    let points = &polygon.points;
    let mut found = Vec::new();
    for index in 0..points.len() {
        let (p, q) = (points[index], points[(index + 1) % points.len()]);
        let (a, b) = if vertical { (p.y, q.y) } else { (p.x, q.x) };
        let (c, d) = if vertical { (p.x, q.x) } else { (p.y, q.y) };
        if (a - line).abs() <= 1e-12 && (b - line).abs() <= 1e-12 {
            found.push(c);
            found.push(d);
            continue;
        }
        if (a - line) * (b - line) > 0.0 {
            continue;
        }
        let t = (line - a) / (b - a);
        found.push(c + (d - c) * t);
    }
    found
}

/// The width of the frame's stripe across one seam, on one row, in pixels.
///
/// The stripe is the run of pixels that are neither of the two cells' colours, the
/// run the seam line falls in. Between two photos that run is the backdrop the
/// frame leaves visible, so its width is the frame's number; where it is not
/// bounded by both cells — a crossing of two seams merges the two stripes, a
/// rounded corner has taken one side back, the canvas's edge leaves no photo to
/// stop it — this is `None`, and the row counts as skipped rather than as a
/// measurement of a gap that is not there.
///
/// `pixel` reads the perpendicular axis: `i` is the pixel index along the direction
/// the stripe is measured in, and it must be in `0..count`.
fn stripe_px(
    count: i32,
    center: i32,
    a: [u8; 3],
    b: [u8; 3],
    pixel: impl Fn(i32) -> [u8; 3],
) -> Option<u64> {
    if center < 0 || center >= count {
        return None;
    }
    let is_photo = |value: [u8; 3]| channels_match(value, a) || channels_match(value, b);
    // The two cells meeting: the seam line falls in a photo pixel, so the stripe
    // has no width — and it is still the two of them that meet there.
    let meets = |p: i32, q: i32| {
        p >= 0
            && q < count
            && ((channels_match(pixel(p), a) && channels_match(pixel(q), b))
                || (channels_match(pixel(p), b) && channels_match(pixel(q), a)))
    };
    if is_photo(pixel(center)) {
        return (meets(center - 1, center) || meets(center, center + 1)).then_some(0);
    }
    let mut lo = center;
    while lo > 0 && !is_photo(pixel(lo - 1)) {
        lo -= 1;
    }
    let mut hi = center;
    while hi + 1 < count && !is_photo(pixel(hi + 1)) {
        hi += 1;
    }
    // Both ends have to be a photo, and they have to be the two different cells:
    // one end alone is a run that ends at the canvas's edge or inside one cell.
    meets(lo - 1, hi + 1).then_some((hi - lo + 1) as u64)
}

/// Reads the sheet's own edge: for each side, the run of pixels from the canvas to
/// the outermost photo that reaches it (S20).
///
/// One sample per cell per side, at the middle of the cell's own visible span along
/// that side — the flattest part of a rounded cell's edge, and the one place a
/// corner cannot have receded from. A cell is sampled where its **outline** reaches
/// the canvas's edge: there the distance from the sheet to the photo is the
/// frame's own, and it has to be the frame's number. A template that bakes its own
/// margin (S2's gutter layouts) keeps its cells away from the edge, and the wider
/// stripe that leaves is that template's geometry rather than the frame's number —
/// which is why those cells are not sampled at all.
fn sample_borders(doc: &CollageDoc, image: &Rgb8View<'_>, canvas: PixelSize) -> Vec<BorderProbe> {
    let aspect = canvas.aspect();
    let mut probes: Vec<BorderProbe> = Side::ALL
        .iter()
        .map(|side| BorderProbe {
            side: *side,
            min_px: 0,
            max_px: 0,
            samples: 0,
            skipped: 0,
        })
        .collect();
    for (index, slot) in doc.template.slots.iter().enumerate() {
        let filled = doc
            .cells
            .get(index)
            .map(|cell| cell.source.is_some())
            .unwrap_or(false);
        if !filled {
            continue;
        }
        // The region the frame leaves visible — the same rectangle the renderer
        // clips to — and the middle of each of its sides.
        let (rect, _radius) = doc.frame.clip(slot, aspect);
        let middle = rect.center();
        let (half_x, half_y) = (
            0.5 / f64::from(canvas.width),
            0.5 / f64::from(canvas.height),
        );
        let last = (f64::from(image.width) - 1.0, f64::from(image.height) - 1.0);
        for probe in &mut probes {
            // The sample is half a pixel inside the canvas, on the line through
            // the middle of the cell's visible span: the point the outline has to
            // reach, and the pixel the walk starts from (which may be a pixel back
            // from the canvas's own edge).
            let (point, edge, step) = match probe.side {
                Side::Top => (
                    Point::new(middle.x, half_y),
                    (middle.x * f64::from(image.width), 0.0),
                    (0, 1),
                ),
                Side::Bottom => (
                    Point::new(middle.x, 1.0 - half_y),
                    (middle.x * f64::from(image.width), last.1),
                    (0, -1),
                ),
                Side::Left => (
                    Point::new(half_x, middle.y),
                    (0.0, middle.y * f64::from(image.height)),
                    (1, 0),
                ),
                Side::Right => (
                    Point::new(1.0 - half_x, middle.y),
                    (last.0, middle.y * f64::from(image.height)),
                    (-1, 0),
                ),
            };
            // A cell that does not reach the canvas there has no stripe of the
            // frame's own to measure — a concave cell can miss the middle of its
            // own bounding box, and a baked margin keeps the cell off the edge.
            if !slot.outline.contains(point) {
                continue;
            }
            match run_to_color(
                image,
                edge.0.round() as i32,
                edge.1.round() as i32,
                step,
                rgb_of(palette(index)),
            ) {
                Some(run) => observe(
                    &mut probe.min_px,
                    &mut probe.max_px,
                    &mut probe.samples,
                    run,
                ),
                None => probe.skipped += 1,
            }
        }
    }
    probes
}

/// How many pixels from `(x, y)`, stepping by `step`, until `colour` — the run the
/// frame leaves between the canvas's edge and the photo.
///
/// `None` when the colour is not on that line before the canvas ends.
fn run_to_color(
    image: &Rgb8View<'_>,
    mut x: i32,
    mut y: i32,
    step: (i32, i32),
    colour: [u8; 3],
) -> Option<u64> {
    let mut run = 0u64;
    loop {
        if x < 0 || y < 0 || x >= image.width || y >= image.height {
            return None;
        }
        if channels_match(image.pixel(x, y), colour) {
            return Some(run);
        }
        run += 1;
        x += step.0;
        y += step.1;
    }
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

/// The point inside `polygon` farthest from its boundary, found on a coarse grid
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
