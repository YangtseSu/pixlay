//! Background decoding: the one thing the window may not do on its own thread.
//!
//! `pixlay-imaging` is synchronous and pure, and a decode is tens of milliseconds
//! per photo — long enough to drop frames if it ran in a draw callback. So one
//! worker thread owns the decoding, and the window only ever receives finished
//! bitmaps.
//!
//! Three decisions shape this file:
//!
//! * **Latest wins.** The channel is coalesced: when the worker finishes a job it
//!   takes the newest request waiting and forgets the ones in between. A drag
//!   that produces 60 requests per second therefore costs one decode at a time,
//!   never a growing queue, and the canvas converges on where the pointer
//!   actually is.
//! * **The worker keeps the previous result and reuses what it can.** Framing one
//!   slot re-decodes exactly that slot: every other slot's bitmap is carried over
//!   when its cell, its source, the grid and the template still match. Without
//!   this, reframing one photo of eight would re-decode all eight.
//! * **A reply crosses the thread boundary as plain data** and is delivered on
//!   the main context ([`glib::MainContext::invoke`]). A GTK object never leaves
//!   the main thread — the payload here is `Bitmaps` (buffers) plus the window's
//!   `SendWeakRef`, which is what makes the closure `Send` at all.
//!
//! A photo that cannot be decoded does not fail the job: the slot stays white and
//! its index comes back in [`Reply::failed`], because one unreadable file must
//! not blank the collage the user is working on.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gtk4::glib;
use pixlay_core::{CollageDoc, PixelSize};
use pixlay_imaging::{SlotBitmap, Source, slot_bitmap};

/// One decode request: everything the worker needs, and nothing that owns a
/// window.
struct Job {
    generation: u64,
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    grid: PixelSize,
}

/// What the worker sends back.
pub struct Reply {
    pub generation: u64,
    pub bitmaps: Vec<SlotBitmap>,
    /// Slots whose file could not be decoded, with the reason.
    pub failed: Vec<(usize, String)>,
}

/// The previous job and its result, kept so the next job can reuse it.
struct Cache {
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    grid: PixelSize,
    bitmaps: Vec<SlotBitmap>,
}

impl Cache {
    /// Whether a bitmap of `cache` can stand in for `slot` of `job`.
    ///
    /// The comparison is the honest one: same grid, same template geometry, same
    /// canvas, same filter, and the slot's own cell and source unchanged. Anything
    /// else — a different photo, a different crop, a different size — has to be
    /// rebuilt, and a stale bitmap would be a wrong pixel in the product.
    fn reuses(&self, job: &Job, slot: usize) -> Option<&SlotBitmap> {
        let same_shape = self.grid == job.grid
            && self.doc.template == job.doc.template
            && self.doc.canvas == job.doc.canvas
            && self.doc.filter == job.doc.filter;
        let same_cell = self.doc.cells.get(slot) == job.doc.cells.get(slot)
            && self.sources.get(slot) == job.sources.get(slot);
        if !(same_shape && same_cell) {
            return None;
        }
        self.bitmaps.iter().find(|bitmap| bitmap.slot == slot)
    }
}

/// The window's handle on the decoding thread.
pub struct Decoder {
    jobs: Sender<Job>,
    generation: u64,
}

impl Decoder {
    /// Starts the worker. `reply` runs on the main context, in request order of
    /// completion (not of sending — a superseded job may still be running).
    pub fn spawn(reply: impl Fn(Reply) + Send + Sync + 'static) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let reply = std::sync::Arc::new(reply);
        std::thread::Builder::new()
            .name("pixlay-decode".to_string())
            .spawn(move || work(queue, reply))
            .expect("the decoding thread can be started");
        Self {
            jobs,
            generation: 0,
        }
    }

    /// Queues a job and returns the generation that will identify its reply.
    pub fn request(
        &mut self,
        doc: &CollageDoc,
        sources: Vec<Option<PathBuf>>,
        grid: PixelSize,
    ) -> u64 {
        self.generation += 1;
        let job = Job {
            generation: self.generation,
            doc: doc.clone(),
            sources,
            grid,
        };
        // A send failure means the worker died; the window keeps the bitmaps it
        // has, which is the same state a decode error leaves it in.
        let _ = self.jobs.send(job);
        self.generation
    }
}

fn work(queue: Receiver<Job>, reply: std::sync::Arc<dyn Fn(Reply) + Send + Sync>) {
    let mut cache: Option<Cache> = None;
    while let Ok(first) = queue.recv() {
        // Latest wins: everything already waiting is superseded by the newest
        // request, so the work a resize or a drag generates collapses to one job.
        let mut job = first;
        while let Ok(newer) = queue.try_recv() {
            job = newer;
        }
        let done = build(&job, &mut cache);
        let reply = std::sync::Arc::clone(&reply);
        // The main context delivers this on the window's thread; `invoke` is the
        // one glib API that takes a `Send` closure, which is why nothing but the
        // reply and a `SendWeakRef` is captured here.
        glib::MainContext::default().invoke(move || reply(done));
    }
}

fn build(job: &Job, cache: &mut Option<Cache>) -> Reply {
    let mut bitmaps = Vec::with_capacity(job.sources.iter().filter(|s| s.is_some()).count());
    let mut failed = Vec::new();
    for (slot, source) in job.sources.iter().enumerate() {
        let Some(path) = source else {
            continue;
        };
        if let Some(reused) = cache.as_ref().and_then(|cache| cache.reuses(job, slot)) {
            bitmaps.push(reused.clone());
            continue;
        }
        // One source at a time: the buffer ladder's first row is the largest
        // allocation in the pipeline, and this thread is where it lives.
        match Source::decode(path)
            .and_then(|decoded| slot_bitmap(&job.doc, &decoded, slot, job.grid))
        {
            Ok(bitmap) => bitmaps.push(bitmap),
            Err(error) => failed.push((slot, error.to_string())),
        }
    }
    *cache = Some(Cache {
        doc: job.doc.clone(),
        sources: job.sources.clone(),
        grid: job.grid,
        bitmaps: bitmaps.clone(),
    });
    Reply {
        generation: job.generation,
        bitmaps,
        failed,
    }
}
