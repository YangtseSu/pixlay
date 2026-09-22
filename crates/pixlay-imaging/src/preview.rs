//! The preview's caches, and the grid a live gesture draws at.
//!
//! A gesture step is the most latency-sensitive work this product does: a wheel
//! notch or a drag produces one every few milliseconds, and before S12 every one
//! of them re-decoded the photo it framed and re-resampled it. Two caches remove
//! that work, and both live here rather than in the window because the CLI's
//! `gesture` probe has to measure **the window's own step** (`AGENTS.md`: nothing
//! may be possible only in the GUI).
//!
//! * **Decoded sources, keyed by path and `mtime`.** `Source::decode` measured
//!   11–110 ms per 2400x1600 file (S4, `docs/CONTRACT.md` §8), and a gesture that
//!   framed the same cell twice paid it twice. The key is the file's own identity
//!   as the filesystem reports it, so a *changed* file is decoded again: a stale
//!   cache would be a wrong picture, which is the one thing a cache here may never
//!   be. A file edited in place keeps its path and gets a new modification time;
//!   a replacement that reproduces the same one is not detected, and the cache
//!   says so rather than pretending otherwise.
//! * **One bitmap set per grid.** S7's decision was that the preview grid belongs
//!   to the widget; a live gesture draws at a coarser grid than rest
//!   ([`gesture_grid`]) and returns to the resting one when it ends, so exactly
//!   two grids are in play and a cell nobody touched is carried over instead of
//!   being resampled twice. The identity rule is S7's, plus one thing S12 had to
//!   add: same grid, same template, same canvas, same filter, same cell, same
//!   source — **and the same modification time on that source's file**. Without
//!   the last one a photo edited in another program would keep its old bitmap
//!   until the cell was touched, which is the stale picture this module exists to
//!   prevent. A `stat` per occupied cell is ~1 us next to a 30–110 ms decode
//!   (measured 2026-09-22), so the identity is checked on every build.
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

use crate::decode::{DecodeLimits, Source};
use crate::error::ImagingError;
use crate::layout::{SlotBitmap, slot_bitmap};

/// How many bytes of decoded sources the preview keeps before the least recently
/// used ones are dropped.
///
/// A decoded source is `width * height * 4` bytes at 8 bits per channel and twice
/// that at 16 (`Source` keeps the file's own depth). Measured 2026-09-22: a
/// 4032x3024 (12 MP) 8-bit photo decodes to `4032 * 3024 * 4` = **48.8 MB**, so a
/// whole selection — nine photos, ruling 3 — is **439 MB** at 8 bits and 878 MB at
/// 16, and the verify project's seven photos total **15.7 MB**. 512 MiB therefore
/// holds a full selection of 12 MP photos whole, and a document of larger files
/// does not fall out of the cache entirely: the entry the gesture is using is
/// always the most recent one, and the budget only decides how many of the
/// *others* survive to the next gesture.
pub const MAX_SOURCE_BYTES: usize = 512 * 1024 * 1024;

/// How many grids' bitmaps the preview keeps.
///
/// Two is exactly one gesture's worth: the resting grid and the coarse one it
/// draws at while it moves ([`gesture_grid`]). Each set is the whole canvas in
/// pixels, so keeping more would be holding bitmaps of a grid nobody is looking
/// at — and at 4K a set is already ~40 MB (`3840 * 2560`, four bytes per pixel).
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

/// A decoded source with the file identity it was decoded from.
struct Entry {
    path: PathBuf,
    /// The file's modification time when it was decoded: the second half of the
    /// key, and the reason a file that changed is decoded again.
    modified: SystemTime,
    source: Source,
}

/// Decoded sources, keyed by path and modification time.
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
    uncached: Option<Source>,
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

    /// The decoded photo at `path`: from the cache when the file is the one that
    /// was decoded, from the decoder otherwise.
    ///
    /// `modified` is the file's modification time, read by the caller (which
    /// needed it for the bitmap identity anyway) rather than read again here.
    fn source(
        &mut self,
        path: &Path,
        modified: Option<SystemTime>,
        limits: &DecodeLimits,
    ) -> Result<&Source, ImagingError> {
        if let Some(modified) = modified
            && let Some(index) = self
                .entries
                .iter()
                .position(|entry| entry.modified == modified && entry.path == path)
        {
            // A hit becomes the most recent one, so the photo the user is
            // gesturing stays while the others age out.
            let hit = self.entries.remove(index);
            self.entries.push(hit);
            return Ok(&self.entries.last().expect("the hit was just pushed").source);
        }

        self.decodes += 1;
        let source = Source::decode_with(path, limits)?;
        match modified {
            Some(modified) if source.bytes() <= self.budget => {
                self.bytes += source.bytes();
                self.entries.push(Entry {
                    path: path.to_path_buf(),
                    modified,
                    source,
                });
                self.evict();
                Ok(&self
                    .entries
                    .last()
                    .expect("the entry was just pushed")
                    .source)
            }
            // No modification time to key on, or a photo bigger than the whole
            // budget: decoded, used, and not kept. Both are rare, and a lookup
            // that re-decodes is correct — only slower.
            _ => {
                self.uncached = Some(source);
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
    /// The comparison is S7's, unchanged, and it is the honest one: same template
    /// geometry, same canvas, same filter, and the slot's own cell and source
    /// unchanged. The grid is not compared here because the caller only asks a set
    /// whose grid is the one being built.
    fn reuses(
        &self,
        doc: &CollageDoc,
        sources: &[Option<PathBuf>],
        modified: &[Option<SystemTime>],
        slot: usize,
    ) -> Option<&SlotBitmap> {
        let same_shape = self.doc.template == doc.template
            && self.doc.canvas == doc.canvas
            && self.doc.filter == doc.filter;
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
        // Taken out for the duration so the source cache can be borrowed at the
        // same time, and put back at the front when the build is done.
        let cache = self
            .grids
            .iter()
            .position(|cache| cache.grid == grid)
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
            let built = self
                .sources
                .source(path, modified[slot], &DecodeLimits::default())
                .and_then(|decoded| slot_bitmap(doc, decoded, slot, grid));
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
        }
    }

    /// Files this preview has been asked to decode over its lifetime.
    pub fn decodes(&self) -> u64 {
        self.sources.decodes
    }
}
