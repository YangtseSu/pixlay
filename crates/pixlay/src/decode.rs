//! Background decoding: the one thing the window may not do on its own thread.
//!
//! `pixlay-imaging` is synchronous and pure, and a decode is tens of milliseconds
//! per photo — long enough to drop frames if it ran in a draw callback. So one
//! worker thread owns the decoding, and the window only ever receives finished
//! bitmaps.
//!
//! Two kinds of work arrive here, and they share one thread because they share one
//! [`Preview`] — and with it one set of preview-grade copies and one set of decodes
//! per photo:
//!
//! * **the canvas** wants the whole document at the grid it draws at;
//! * **the layout gallery** (S14) wants every candidate layout at a thumbnail grid.
//!   It names **the canvas's own preview-grade edge**
//!   ([`Preview::build_at_source_edge`]), so the band's copies are the copies the
//!   canvas already reduced: measured 2026-09-23, its own builds decode **0** files
//!   however many candidates the band lists — C cheap per-cell resamples, never a
//!   decode per thumbnail. The edge is taken while the canvas is at rest, so the
//!   two jobs go out as one batch and the canvas job (which the worker runs first)
//!   puts the copies in the cache before the band reads them.
//!
//! Three decisions shape this file:
//!
//! * **Latest wins.** The channel is coalesced: when the worker finishes a batch it
//!   takes the newest request of each kind waiting and forgets the ones in
//!   between. A drag that produces 60 requests per second therefore costs one
//!   build at a time, never a growing queue, and the canvas converges on where the
//!   pointer actually is — while a gallery job queued beside it still builds, on
//!   the same [`Preview`], rather than being superseded by a kind it is not.
//! * **The worker keeps what it can reuse, in `pixlay-imaging`.** Framing one slot
//!   rebuilds exactly that slot: the decoded sources and the other cells' bitmaps
//!   come from [`pixlay_imaging::Preview`], which is the same type the CLI's
//!   `gesture` probe drives (S12). Both caches live there rather than here because
//!   the probe has to measure **this** thread's step, and because a cache tested
//!   without a window is a cache the display-free tests can pin.
//! * **A reply crosses the thread boundary as plain data** and is delivered on
//!   the main context ([`glib::MainContext::invoke`]). A GTK object never leaves
//!   the main thread — the payload here is `Bitmaps` (buffers) plus the window's
//!   `SendWeakRef`, which is what makes the closure `Send` at all.
//!
//! A photo that cannot be decoded does not fail the job: the slot stays white and
//! its index comes back in [`Reply::failed`], because one unreadable file must
//! not blank the collage the user is working on. The gallery reports the same way,
//! one candidate at a time: a candidate whose render fails is simply not in the
//! reply.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gtk4::glib;
use pixlay_core::{CollageDoc, PixelSize, Template};
use pixlay_imaging::{Preview, SlotBitmap};
use pixlay_render::{Bitmap, Images, render_rgb8};

use crate::layout::Candidate;

/// One canvas job: everything the worker needs, and nothing that owns a window.
struct CanvasJob {
    generation: u64,
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    grid: PixelSize,
}

/// One gallery job: the same document, one request per candidate layout.
struct GalleryJob {
    generation: u64,
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    /// Each candidate's template and the thumbnail grid it is drawn at.
    candidates: Vec<(Template, PixelSize)>,
    /// The long edge the preview-grade copies are taken at — **the canvas's own**,
    /// so that the two jobs share one set of copies and the band costs no decode
    /// the canvas does not already pay for.
    source_edge: u32,
}

/// What one canvas build sends back.
pub struct Reply {
    pub generation: u64,
    pub bitmaps: Vec<SlotBitmap>,
    /// Slots whose file could not be decoded, with the reason.
    pub failed: Vec<(usize, String)>,
    /// Files this build decoded. Zero means both caches answered everything,
    /// which is what a step of a live gesture has to look like; the window counts
    /// them so a test can hold the gesture path to it.
    pub decodes: u64,
}

/// What one gallery build sends back.
pub struct GalleryReply {
    pub generation: u64,
    /// One entry per candidate that rendered, in the order it was asked for.
    pub candidates: Vec<Candidate>,
    /// Files this build decoded — one per photo on the band's first build, zero
    /// whenever the sources were already reduced for the grids asked for, which is
    /// what the gallery's decode criterion holds it to ("N decodes, never N×C").
    pub decodes: u64,
}

/// Either answer, which is what the window's one callback receives.
pub enum Event {
    Canvas(Reply),
    Gallery(GalleryReply),
}

/// The window's handle on the decoding thread.
pub struct Decoder {
    jobs: Sender<Job>,
    canvas_generation: u64,
    gallery_generation: u64,
}

/// What the worker is asked for.
enum Job {
    Canvas(CanvasJob),
    Gallery(GalleryJob),
}

impl Decoder {
    /// Starts the worker. `reply` runs on the main context, in request order of
    /// completion (not of sending — a superseded job may still be running).
    pub fn spawn(reply: impl Fn(Event) + Send + Sync + 'static) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let reply = std::sync::Arc::new(reply);
        std::thread::Builder::new()
            .name("pixlay-decode".to_string())
            .spawn(move || work(queue, reply))
            .expect("the decoding thread can be started");
        Self {
            jobs,
            canvas_generation: 0,
            gallery_generation: 0,
        }
    }

    /// Queues a canvas job and returns the generation that will identify its reply.
    pub fn request(
        &mut self,
        doc: &CollageDoc,
        sources: Vec<Option<PathBuf>>,
        grid: PixelSize,
    ) -> u64 {
        self.canvas_generation += 1;
        let job = Job::Canvas(CanvasJob {
            generation: self.canvas_generation,
            doc: doc.clone(),
            sources,
            grid,
        });
        // A send failure means the worker died; the window keeps the bitmaps it
        // has, which is the same state a decode error leaves it in.
        let _ = self.jobs.send(job);
        self.canvas_generation
    }

    /// Queues a gallery job and returns the generation that will identify its
    /// reply.
    pub fn request_gallery(
        &mut self,
        doc: &CollageDoc,
        sources: Vec<Option<PathBuf>>,
        candidates: Vec<(Template, PixelSize)>,
        source_edge: u32,
    ) -> u64 {
        self.gallery_generation += 1;
        let job = Job::Gallery(GalleryJob {
            generation: self.gallery_generation,
            doc: doc.clone(),
            sources,
            candidates,
            source_edge,
        });
        let _ = self.jobs.send(job);
        self.gallery_generation
    }
}

fn work(queue: Receiver<Job>, reply: std::sync::Arc<dyn Fn(Event) + Send + Sync>) {
    // One cache pair for as long as the window lives: it is what makes the second
    // step of a gesture cost a fraction of the first, and what keeps the band's
    // copies one per photo however many candidates it lists.
    let mut preview = Preview::new();
    while let Ok(first) = queue.recv() {
        // Latest wins: everything already waiting is superseded by the newest
        // request, so the work a resize or a drag generates collapses to one build
        // — and the gallery never drops the canvas's bitmaps for a thumbnail.
        // Generations only move forward, so a newer same-kind request always
        // replaces the one held, whatever kind arrived first.
        let mut canvas: Option<CanvasJob> = None;
        let mut gallery: Option<GalleryJob> = None;
        let mut take = |job: Job| match job {
            Job::Canvas(newer)
                if newer.generation > canvas.as_ref().map_or(0, |held| held.generation) =>
            {
                canvas = Some(newer);
            }
            Job::Gallery(newer)
                if newer.generation > gallery.as_ref().map_or(0, |held| held.generation) =>
            {
                gallery = Some(newer);
            }
            _ => {}
        };
        take(first);
        while let Ok(newer) = queue.try_recv() {
            take(newer);
        }
        // The canvas first: it is the one the window is waiting for, and its build
        // warms the source cache the gallery's candidates then read against.
        if let Some(job) = canvas {
            let built = preview.build(&job.doc, &job.sources, job.grid);
            send(
                &reply,
                Event::Canvas(Reply {
                    generation: job.generation,
                    bitmaps: built.bitmaps,
                    failed: built.failed,
                    decodes: built.decodes,
                }),
            );
        }
        if let Some(job) = gallery {
            let mut candidates = Vec::with_capacity(job.candidates.len());
            let mut decodes = 0;
            for (template, grid) in &job.candidates {
                // The candidate is the editor's own document with `SetTemplate`
                // applied, which is exactly what clicking the candidate does — so
                // the thumbnail shows the document the click would produce, not an
                // approximation of it.
                let mut doc = job.doc.clone();
                doc.template = template.clone();
                doc.cells
                    .resize(template.slots.len(), pixlay_core::Cell::default());
                let sources = &job.sources[..doc.cells.len().min(job.sources.len())];
                let built = preview.build_at_source_edge(&doc, sources, *grid, job.source_edge);
                decodes += built.decodes;
                if let Ok(image) = render_rgb8(&doc, &images_from(built.bitmaps), *grid, 1.0, None)
                {
                    candidates.push(Candidate {
                        template: template.name.clone(),
                        image,
                    });
                }
            }
            send(
                &reply,
                Event::Gallery(GalleryReply {
                    generation: job.generation,
                    candidates,
                    decodes,
                }),
            );
        }
    }
}

/// Hands one event to the main context, where the window lives.
fn send(reply: &std::sync::Arc<dyn Fn(Event) + Send + Sync>, event: Event) {
    let reply = std::sync::Arc::clone(reply);
    // The main context delivers this on the window's thread; `invoke` is the one
    // glib API that takes a `Send` closure, which is why nothing but the reply and
    // a `SendWeakRef` is captured here.
    glib::MainContext::default().invoke(move || reply(event));
}

/// Wraps a build's bitmaps for the renderer, dropping the ones it cannot take.
///
/// A bitmap that `draw` refuses is a slot that stays white; it must not fail the
/// candidate, which is a whole layout.
fn images_from(bitmaps: Vec<SlotBitmap>) -> Images {
    let mut images = Images::new();
    for bitmap in bitmaps {
        let slot = bitmap.slot;
        match Bitmap::from_argb32_region(
            bitmap.width as i32,
            bitmap.height as i32,
            bitmap.origin,
            bitmap.display,
            bitmap.pixels,
        ) {
            Ok(bitmap) => images.insert(slot, bitmap),
            Err(error) => glib::g_warning!("pixlay", "a decoded bitmap was refused: {error}"),
        }
    }
    images
}
