//! The preview's caches, and the grid a live gesture draws at.
//!
//! A gesture step is the most latency-sensitive work this product does: a wheel
//! notch or a drag produces one every few milliseconds, and before S12 every one
//! of them re-decoded the photo it framed and re-resampled it. S12 removed the
//! decode; what it could not remove was the *resample*, whose cost follows the
//! **source's** resolution rather than the output grid's (`resample` widens its
//! kernel with the downscale ratio, S4) — so a 24 MP photo still cost ~200 ms per
//! step against a 16.7 ms frame. S12b answers that with a second cache: a
//! **preview-grade source** per photo, reduced once by [`crate::reduce`] to the
//! size this grid needs, and every bitmap the preview builds is resampled from
//! that copy. Measured 2026-09-22 (`docs/CONTRACT.md` §8, "S12b"): the same 24 MP
//! document steps in **2.20 ms** at the editor's own grid and **9.12 ms** at 1600,
//! against 199.9 and 321.1 ms before.
//!
//! Both caches live here rather than in the window because the CLI's `gesture`
//! probe has to measure **the window's own step** (`AGENTS.md`: nothing may be
//! possible only in the GUI).
//!
//! * **Preview-grade sources, keyed by path, `mtime` and target size.** A changed
//!   file must be reduced again — a stale cache would be a wrong picture, which is
//!   the one thing a cache here may never be — and the key is the file's own
//!   identity as the filesystem reports it, plus **the long edge the reduction was
//!   made for**, because the resting grid changes with the window and a coarse
//!   gesture frame is served by a smaller copy (that is the point of it). A file
//!   edited in place keeps its path and gets a new modification time; a replacement
//!   that reproduces the same one is not detected, and the cache says so rather
//!   than pretending otherwise.
//! * **One bitmap set per grid and source edge.** S7's decision was that the
//!   preview grid belongs to the widget; a live gesture draws at a coarser grid
//!   than rest ([`gesture_grid`]) and returns to the resting one when it ends, so
//!   exactly two grids are in play and a cell nobody touched is carried over
//!   instead of being resampled twice. The identity rule is S7's, plus what S12
//!   and S15f had to add: same grid, same template, same cell, same source —
//!   **the same modification time on that source's file**, the **same frame gap**
//!   and the **same source edge**. The modification time keeps a
//!   photo edited in another program from keeping its old bitmap until the cell is
//!   touched; the gap keeps a set built before a frame change from carrying the
//!   region that change moved; the edge keeps a set built from coarse copies from
//!   answering a finer request. A `stat` per occupied cell is ~1 us next to a
//!   30–110 ms decode (measured 2026-09-22), so the identity is checked on every
//!   build.
//!
//! Both caches are bounded by a constant with its source, and neither is a promise
//! about *pixels*: everything here is rebuilt from the file when the comparison
//! says the cached answer no longer applies, so a wrong entry is not a possibility
//! to reason about — it is a miss.
//!
//! A file that cannot be decoded is held by neither cache: the failure is reported
//! ([`Built::failed`]) and attempted again on the next build, which is the same
//! behaviour the window had before this module existed.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use pixlay_core::{CollageDoc, PixelSize};

use crate::decode::{DecodeLimits, Sampler, Source};
use crate::error::ImagingError;
use crate::layout::{SlotBitmap, slot_bitmap};
use crate::reduce::PreviewSource;

/// How many bytes of preview-grade sources the preview keeps before the least
/// recently used ones are dropped.
///
/// A preview-grade source is a [`crate::reduce::PreviewSource`] — straight sRGB,
/// `width * height * 4` bytes at the source's own depth — and it is one to two
/// orders of magnitude smaller than the decoded photo it came from, which is what
/// makes this budget a promise about *documents* rather than about photos.
/// Measured 2026-09-22: a 24 MP photo (6000x4000, **96 MB** decoded) reduces to
/// **975x650 = 2.5 MB** at the editor's canvas (a 780-px grid) and to
/// **2000x1333 = 10.7 MB** at a 1600-px one, so nine of them — a whole selection,
/// ruling 3 — are 23 MB at the editor's grid and 96 MB at 1600, where both targets
/// in play together still come to 119 MB. The verify project's seven photos
/// (0.35–0.72 MP each) come to 13.7 MB at that grid. Nine 12 MP photos would have
/// been 439 MB *decoded* — the budget that used to hold photos holds whole
/// documents now, several times over, and the entry the gesture is using is always
/// the most recent one: the budget only decides how many of the *others* survive to
/// the next gesture.
pub const MAX_SOURCE_BYTES: usize = 512 * 1024 * 1024;

/// How much larger a preview-grade source is than the grid it serves.
///
/// The reduction is read by the resampler, so a copy at the grid's own size would
/// already be used at 1:1 by a cell that fills the canvas; this is the headroom
/// that keeps a preview from showing the reduction's own sampling when a cell is
/// zoomed in or rotated. It is also what the step costs — the kernel widens with
/// `source / displayed`, so the cost of one step is roughly this factor times the
/// bitmap's own pixels — and it is therefore the largest value that keeps the
/// measured step inside the frame budget. Measured 2026-09-22 on the 24 MP
/// document (`mosaic-8-s14`, one 6000x4000 photo in every cell), warm step at grid
/// 780 / 1600:
///
/// | this constant | the copies | warm step | against 16.666667 ms |
/// |---|---|---|---|
/// | 1.5 | 585 / 1200 px | 2.84–4.53 / **18.40–18.50 ms** | holds / **misses** |
/// | **1.25** | 488 / 1000 px | **2.20 / 9.12 ms** | holds, by 7.6x and 1.8x |
/// | 1.0 | 390 / 800 px | 1.71 / 7.94 ms | holds, with a larger drift |
/// | 2.5 (S12's own measurement) | 2048 px | 39.08 ms | misses |
///
/// What the ladder does *not* decide is fidelity: the drift this step introduces
/// barely moves across it on photo content — 0.77 / 1.01 / 1.59 RMSE at 1.5 / 1.25
/// / 1.0 in `crates/pixlay-cli/tests/preview.rs` (`docs/CONTRACT.md` §8, "S12b") —
/// which is why the constant is set by the frame budget and not by a quality
/// argument, and why it is the largest value that holds rather than a round number.
pub const PREVIEW_SOURCE_SCALE: f64 = 1.25;

/// The long edge a preview-grade source is reduced to for a canvas grid.
///
/// The reduction's own size is a property of the grid, not of the photo: the
/// coarser grid a live gesture draws at asks for half of this, which is cheaper to
/// resample *and* cheaper to hold, and the release goes back to the resting grid's
/// copy. Neither is ever enlarged — [`crate::reduce`] hands a photo at or below
/// the target back unchanged.
pub fn preview_source_long_edge(grid: PixelSize) -> u32 {
    let long = f64::from(grid.width.max(grid.height)) * PREVIEW_SOURCE_SCALE;
    (long.round() as i64).clamp(1, i64::from(i32::MAX)) as u32
}

/// How many grids' bitmaps the preview keeps.
///
/// Two is exactly one gesture's worth: the resting grid and the coarse one it
/// draws at ([`gesture_grid`]) — each at its own source edge, which is a function
/// of the grid. Each set is the whole canvas in pixels, so keeping more would be
/// holding bitmaps of a grid nobody is looking at — and at 4K a set is already
/// ~40 MB (`3840 * 2560`, four bytes per pixel).
pub const MAX_GRIDS: usize = 2;

/// The grid a live gesture draws at, as a fraction of the resting grid.
///
/// A gesture is judged by whether it keeps up, not by whether its intermediate
/// frames are sharp, and the refinement is exact the moment it ends (the released
/// frame is drawn at the resting grid from bitmaps built for it). Halving the
/// linear size quarters the pixels a step resamples and blits, and measured
/// 2026-09-22 (`docs/CONTRACT.md` §8, "S12") that is worth between nothing and
/// half of a step, depending on one thing — how much larger the source is than the
/// part of it the cell shows:
///
/// | source | one cell's bitmap at the resting grid | at this grid |
/// |---|---|---|
/// | the verify project's photos (≤ 1200 px) | 9.5 ms at a 1600-px grid | **7.5 ms** |
/// | a 24 MP photo (6000x4000) | 289 ms at a 780-px grid | 283 ms |
///
/// The 24 MP column is why this constant cannot rescue a large photo: `resample`'s
/// kernel widens with the downscale ratio, so its cost follows the *source's*
/// resolution and not the output's. What the coarse grid still buys there is the
/// blit — a quarter of the bitmap bytes — which is what the second column's
/// difference is made of.
pub const GESTURE_GRID_SCALE: f64 = 0.5;

/// How far one step of a straightening gesture turns a cell, in degrees.
///
/// One degree per notch, and it is the same number in the editor's own control
/// (the canvas's Ctrl+scroll straightening) and in the CLI's `gesture` probe, so
/// the thing measured and the thing used cannot drift apart.
pub const GESTURE_STEP_DEG: f64 = 1.0;

/// The grid a live gesture draws at: `grid` scaled by [`GESTURE_GRID_SCALE`], with
/// neither edge below one pixel.
///
/// The canvas aspect is preserved only to the rounding: the result is a grid, and
/// a grid's aspect is its own two integers. The fit is taken against whichever
/// grid is drawn (S11's reference is the space `draw` places into), so the two
/// grids can differ in the last decimal of the covering zoom — which is invisible
/// and, unlike a wrong pixel, not a claim the product makes. The pixels after the
/// release are the resting grid's own, built by the same function at the same
/// size as a draw that never went coarse at all.
pub fn gesture_grid(grid: PixelSize) -> PixelSize {
    let scaled = |edge: i32| ((f64::from(edge) * GESTURE_GRID_SCALE).round() as i32).max(1);
    PixelSize {
        width: scaled(grid.width),
        height: scaled(grid.height),
    }
}

/// The file's modification time, or `None` when it cannot be read at all.
///
/// It is the second half of both keys in this module: a path alone is not an
/// identity, and a cache that answered for a file that changed would be showing
/// the user a picture that is not the file's.
fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// A preview-grade source with the file identity it was reduced from.
struct Entry {
    path: PathBuf,
    /// The file's modification time when it was reduced: the second half of the
    /// key, and the reason a file that changed is reduced again.
    modified: SystemTime,
    /// The long edge the reduction was made for — the third half of the key: the
    /// resting grid moves with the window, and a live gesture asks for its own,
    /// smaller copy.
    long_edge: u32,
    source: PreviewSource,
}

/// Preview-grade sources, keyed by path, modification time and target size.
struct Sources {
    /// Least recently used first, most recently used last.
    entries: Vec<Entry>,
    bytes: usize,
    budget: usize,
    /// Files this cache has been asked to decode, refusals included: a file that
    /// cannot be decoded is not cached, so it is attempted again.
    decodes: u64,
    /// Where the result of a lookup that could not be cached lives, so that
    /// "decoded but not kept" can still be borrowed.
    uncached: Option<PreviewSource>,
}

impl Sources {
    fn new(budget: usize) -> Self {
        Self {
            entries: Vec::new(),
            bytes: 0,
            budget,
            decodes: 0,
            uncached: None,
        }
    }

    /// The preview-grade source for `path` at `long_edge`: from the cache when the
    /// file is the one that was reduced for that size, from the decoder and
    /// [`PreviewSource::new`] otherwise.
    ///
    /// `modified` is the file's modification time, read by the caller (which
    /// needed it for the bitmap identity anyway) rather than read again here.
    fn source(
        &mut self,
        path: &Path,
        modified: Option<SystemTime>,
        long_edge: u32,
        limits: &DecodeLimits,
    ) -> Result<&PreviewSource, ImagingError> {
        if let Some(modified) = modified
            && let Some(index) = self.entries.iter().position(|entry| {
                entry.modified == modified && entry.path == path && entry.long_edge == long_edge
            })
        {
            // A hit becomes the most recent one, so the photo the user is
            // gesturing stays while the others age out.
            let hit = self.entries.remove(index);
            self.entries.push(hit);
            return Ok(&self.entries.last().expect("the hit was just pushed").source);
        }

        self.decodes += 1;
        // One photo's buffer at a time: the decoded file is read once into the
        // reduction and dropped, so what the cache holds (and what the budget
        // counts) is the small copy rather than the 96 MB the decoder produced.
        let reduced = PreviewSource::new(&Source::decode_with(path, limits)?, long_edge);
        match modified {
            Some(modified) if reduced.bytes() <= self.budget => {
                self.bytes += reduced.bytes();
                self.entries.push(Entry {
                    path: path.to_path_buf(),
                    modified,
                    long_edge,
                    source: reduced,
                });
                self.evict();
                Ok(&self
                    .entries
                    .last()
                    .expect("the entry was just pushed")
                    .source)
            }
            // No modification time to key on, or a reduction bigger than the whole
            // budget: reduced, used, and not kept. Both are rare, and a lookup
            // that re-decodes is correct — only slower.
            _ => {
                self.uncached = Some(reduced);
                Ok(self.uncached.as_ref().expect("just set"))
            }
        }
    }

    fn evict(&mut self) {
        // The newest entry stays even when it alone is over budget: it is the one
        // the next step will ask for, and dropping it would re-decode on every
        // motion, which is the cost this cache exists to remove.
        while self.entries.len() > 1 && self.bytes > self.budget {
            let oldest = self.entries.remove(0);
            self.bytes -= oldest.source.bytes();
        }
    }
}

/// One grid's bitmaps, with the document they were built for.
struct Grid {
    grid: PixelSize,
    /// The long edge the preview-grade copies behind these bitmaps were reduced
    /// to: a set built from a coarse edge is not the set a finer edge asks for.
    source_edge: u32,
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    /// Each source's modification time when its bitmap was built: an edited file
    /// invalidates the cell that shows it, without the cell itself changing.
    modified: Vec<Option<SystemTime>>,
    bitmaps: Vec<SlotBitmap>,
}

impl Grid {
    /// Whether a bitmap of this set can stand in for `slot` of a build against
    /// `doc`.
    ///
    /// The comparison is S7's, minus the canvas-wide filter it used to carry
    /// (S12c removed it) and minus the canvas itself (S12d removed that too):
    /// same template geometry, the same **frame gap**, and the slot's own cell and
    /// source unchanged. The grid and the source edge are not compared here because
    /// the caller only asks a set whose both are the ones being built.
    ///
    /// **The frame gap is part of that identity** (S15f, PIX-004): the fit a bitmap
    /// is built from reads the frame's `covering`, which is the cell's outline
    /// clipped to its inset rectangle — so a gap change moves the region the bitmap
    /// holds, and a set built before it carries another region's pixels *and*
    /// another displayed-photo size. The other two frame fields are deliberately
    /// **not** here:
    ///
    /// * `radius_rel` is the renderer's clip, never the bitmap's region — the
    ///   corner radius is not subtracted from the covering polygon (`frame.rs`), so
    ///   a radius-only change leaves every bitmap this pipeline builds bit for bit
    ///   identical;
    /// * `color` is painted after the slots (`AGENTS.md`'s canvas decoration
    ///   stage), so it moves no slot's pixels either.
    ///
    /// Both would be rebuilt for nothing, which on a slider is every motion.
    fn reuses(
        &self,
        doc: &CollageDoc,
        sources: &[Option<PathBuf>],
        modified: &[Option<SystemTime>],
        slot: usize,
    ) -> Option<&SlotBitmap> {
        let same_shape =
            self.doc.template == doc.template && self.doc.frame.gap_rel == doc.frame.gap_rel;
        let same_cell = self.doc.cells.get(slot) == doc.cells.get(slot)
            && self.sources.get(slot) == sources.get(slot)
            && self.modified.get(slot) == modified.get(slot);
        if !(same_shape && same_cell) {
            return None;
        }
        self.bitmaps.iter().find(|bitmap| bitmap.slot == slot)
    }
}

/// What one [`Preview::build`] produced.
pub struct Built {
    /// One bitmap per occupied cell that could be built. A cell with no source
    /// produces none, and a cell whose file could not be decoded is in `failed`.
    pub bitmaps: Vec<SlotBitmap>,
    /// Slots whose file could not be decoded, with the reason.
    pub failed: Vec<(usize, String)>,
    /// Files this build decoded, refusals included. Zero means both caches
    /// answered every cell, which is what a live gesture step has to look like.
    pub decodes: u64,
    /// The largest preview-grade source any cell **of this build** was resampled
    /// from, or `0 x 0` when every cell came from the bitmap cache.
    ///
    /// This is the number that says the reduction happened rather than the
    /// reduction being assumed (S12b): the CLI's `gesture` prints the step's own
    /// copy as `src_w`/`src_h`, and against a 6000-px photo it is the difference
    /// between a step that reads 24 MP and one that reads one. It is the **maximum**
    /// over the cells this build rebuilt — cells carried over from the bitmap cache
    /// were not resampled — because with sources of different aspects the last one
    /// processed is a different number (S15f, PIX-027C).
    pub source_px: PixelSize,
}

/// The preview pipeline's two caches.
///
/// One instance is one editing session: the window's decoding thread keeps one
/// for as long as the window lives, and the CLI's `gesture` probe builds one per
/// measurement, so both measure and use the same thing.
pub struct Preview {
    sources: Sources,
    /// Most recently used grid first.
    grids: Vec<Grid>,
}

impl Default for Preview {
    fn default() -> Self {
        Self::new()
    }
}

impl Preview {
    pub fn new() -> Self {
        Self::with_budget(MAX_SOURCE_BYTES)
    }

    /// The same with a different source budget, in bytes — the tests' handle on
    /// eviction. Nothing in the product passes a budget of its own.
    pub fn with_budget(bytes: usize) -> Self {
        Self {
            sources: Sources::new(bytes),
            grids: Vec::new(),
        }
    }

    /// Builds every occupied cell's bitmap for `grid`.
    ///
    /// A cell whose bitmap the caches can answer is not decoded, not resampled and
    /// not quantized; everything else is built by the one pipeline
    /// ([`slot_bitmap`]), so a cached build and a fresh one are the same pixels.
    pub fn build(
        &mut self,
        doc: &CollageDoc,
        sources: &[Option<PathBuf>],
        grid: PixelSize,
    ) -> Built {
        self.build_at_source_edge(doc, sources, grid, preview_source_long_edge(grid))
    }

    /// [`build`](Self::build) with its preview-grade copies taken at `long_edge`
    /// instead of at the one `grid` would ask for.
    ///
    /// **Both the grid and the edge are the cache's key** (S15f, PIX-004): a set of
    /// bitmaps built from coarse copies is not the set a finer edge asks for, so a
    /// request naming another edge rebuilds the cells rather than answering a
    /// higher-quality question with lower-quality pixels.
    ///
    /// The copies are keyed by **the edge they were reduced to**, so a caller that
    /// names the edge another consumer is already using shares that consumer's
    /// entries and decodes nothing at all. That is what S14's layout gallery does:
    /// it renders several *small* candidates of the same document while the canvas
    /// renders one big one, and it names the canvas's edge so that the band's own
    /// builds cost **0** decodes however many candidates it lists. Measured
    /// 2026-09-23 (`--release`, this machine, the eight-photo verification project's
    /// 8-slot template and its three candidates at a 128x96 thumbnail grid, canvas
    /// grid 780): sharing the canvas's 975-px edge, the gallery's own builds decode
    /// **0** files, the whole band takes **74.6 ms**, and its pixels differ from a
    /// full-resolution `slot_bitmaps` render at the same grid by **0.083** RMSE.
    /// Reducing the gallery's own 128-px copies instead costs **7** further decodes
    /// — one per distinct photo file, on top of the canvas's own — rebuilds the band
    /// in 4.0 ms, and differs from the same render by **3.42**.
    ///
    /// So the cost of sharing is a wider resampling kernel — the copy is
    /// `PREVIEW_SOURCE_SCALE` times the *canvas* grid, not times the thumbnail's —
    /// and it buys both the decodes and the fidelity: a copy at the thumbnail's own
    /// size is one the resampler reads at 1:1, which is where the 3.42 comes from.
    pub fn build_at_source_edge(
        &mut self,
        doc: &CollageDoc,
        sources: &[Option<PathBuf>],
        grid: PixelSize,
        long_edge: u32,
    ) -> Built {
        // One normal form for the edge, because it is half of the key these
        // bitmaps are cached under: a build that asks for edge 0 asks for edge 1,
        // and two spellings of one request must not be two cache entries.
        let long_edge = long_edge.max(1);
        // Taken out for the duration so the source cache can be borrowed at the
        // same time, and put back at the front when the build is done. **The source
        // edge is part of the key** (S15f, PIX-004): a set built from a coarse edge
        // holds bitmaps made from coarse copies, and handing them to a build that
        // asked for a finer edge — the layout gallery and the canvas do ask for the
        // same grid at the same edge, but a caller may ask for another — would
        // answer a higher-quality request with lower-quality pixels and then store
        // them as the new edge's own.
        let cache = self
            .grids
            .iter()
            .position(|cache| cache.grid == grid && cache.source_edge == long_edge)
            .map(|index| self.grids.remove(index));
        // One `stat` per occupied cell for the whole build: both caches key on the
        // file's identity, and reading it once is also what keeps a file edited in
        // another program out of the picture the user is looking at.
        let modified: Vec<Option<SystemTime>> = sources
            .iter()
            .map(|source| source.as_deref().and_then(modified_time))
            .collect();

        let mut bitmaps = Vec::with_capacity(sources.iter().filter(|s| s.is_some()).count());
        let mut failed = Vec::new();
        let mut decodes = 0;
        let mut source_px = PixelSize {
            width: 0,
            height: 0,
        };
        for (slot, source) in sources.iter().enumerate() {
            let Some(path) = source else {
                continue;
            };
            if let Some(reused) = cache
                .as_ref()
                .and_then(|cache| cache.reuses(doc, sources, &modified, slot))
            {
                bitmaps.push(reused.clone());
                continue;
            }
            // One source at a time: the decoded buffer is the largest allocation
            // in the pipeline, and this thread is where it lives. A refusal does
            // not fail the build — one unreadable file must not blank the collage
            // — and the slot it belongs to is reported instead.
            let before = self.sources.decodes;
            let built =
                match self
                    .sources
                    .source(path, modified[slot], long_edge, &DecodeLimits::default())
                {
                    Ok(reduced) => {
                        // The **maximum** over the cells this build rebuilt
                        // (S15f, PIX-027C): the field is a claim about the work the
                        // step did, and with cells of different aspects the last
                        // processed one understates it. The pair is one copy's own
                        // two edges — compared by area, so the report never shows a
                        // width from one copy beside a height from another.
                        let copy = PixelSize {
                            width: reduced.width() as i32,
                            height: reduced.height() as i32,
                        };
                        if copy.width as i64 * copy.height as i64
                            > source_px.width as i64 * source_px.height as i64
                        {
                            source_px = copy;
                        }
                        slot_bitmap(doc, reduced, slot, grid)
                    }
                    Err(error) => Err(error),
                };
            decodes += self.sources.decodes - before;
            match built {
                Ok(bitmap) => bitmaps.push(bitmap),
                Err(error) => failed.push((slot, error.to_string())),
            }
        }

        self.grids.insert(
            0,
            Grid {
                grid,
                source_edge: long_edge,
                doc: doc.clone(),
                sources: sources.to_vec(),
                modified,
                bitmaps: bitmaps.clone(),
            },
        );
        self.grids.truncate(MAX_GRIDS);
        Built {
            bitmaps,
            failed,
            decodes,
            source_px,
        }
    }

    /// Files this preview has been asked to decode over its lifetime.
    pub fn decodes(&self) -> u64 {
        self.sources.decodes
    }
}
