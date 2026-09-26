//! Background work for the canvas and the layout band.
//!
//! `pixlay-imaging` is synchronous and pure, and a decode is tens of milliseconds
//! per photo — long enough to drop frames if it ran in a draw callback. So one
//! worker thread owns the decoding, and the window only ever receives finished
//! bitmaps.
//!
//! Two kinds of work arrive here, and they share one thread:
//!
//! * **the canvas** wants the whole document at the grid it draws at, and it is
//!   what the thread is for;
//! * **the layout band** (S14) wants every candidate of the document's cell count
//!   — and since S21 a candidate is a **sketch**: its template's cells in paper,
//!   every cell's outline and the ground between them inked, drawn by
//!   `pixlay_render::sketch_rgb8`. The job carries no document and no photo
//!   paths, so the band costs no decode at all (it used to share the canvas's
//!   preview-grade copies, S14's design; the sketch removed the need).
//!
//! Three decisions shape this file:
//!
//! * **Latest wins.** The channel is coalesced: when the worker finishes a batch it
//!   takes the newest request of each kind waiting and forgets the ones in
//!   between. A drag that produces 60 requests per second therefore costs one
//!   build at a time, never a growing queue, and the canvas converges on where the
//!   pointer actually is — while a gallery job queued beside it still builds
//!   rather than being superseded by a kind it is not.
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
//! one candidate at a time: a candidate whose sketch fails is simply not in the
//! reply.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gtk4::glib;
use pixlay_core::{CollageDoc, PixelSize, Template};
use pixlay_imaging::{Preview, SlotBitmap};
use pixlay_render::{Sketch, sketch_rgb8};

use crate::layout::Candidate;
use crate::workers::{Down, Kind, WorkerPlan};

/// One canvas job: everything the worker needs, and nothing that owns a window.
struct CanvasJob {
    generation: u64,
    doc: CollageDoc,
    sources: Vec<Option<PathBuf>>,
    grid: PixelSize,
}

/// One gallery job: every candidate of the document's cell count, as a sketch.
///
/// Since S21 this carries **no document and no photo paths**: a candidate is its
/// template's geometry, so the job is a list of templates, the grid each is drawn
/// at and the three parameters of the drawing. A sketch has no photo in it, which
/// is why the band costs no decode and why `Preview` is not involved.
struct GalleryJob {
    generation: u64,
    /// Each candidate's template and the thumbnail grid it is drawn at.
    candidates: Vec<(Template, PixelSize)>,
    /// The paper, the ink and the stroke width, read from the band's theme on the
    /// main thread.
    style: Sketch,
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
    /// One entry per candidate that drew, in the order it was asked for.
    pub candidates: Vec<Candidate>,
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
    ///
    /// `Err` is the thread that could not be started (S15h, PIX-014): the window
    /// opens without a decoder and reports it where a decode would have been asked
    /// for, rather than panicking on the way into `EditorWindow::new`.
    pub fn spawn(reply: impl Fn(Event) + Send + Sync + 'static) -> Result<Self, Down> {
        Self::spawn_with(reply, WorkerPlan::Run)
    }

    /// [`spawn`](Self::spawn) with the plan `window::Workers` names.
    pub fn spawn_with(
        reply: impl Fn(Event) + Send + Sync + 'static,
        plan: WorkerPlan,
    ) -> Result<Self, Down> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let reply = std::sync::Arc::new(reply);
        plan.start(Kind::Decode, move || work(queue, reply))?;
        Ok(Self {
            jobs,
            canvas_generation: 0,
            gallery_generation: 0,
        })
    }

    /// Queues a canvas job and returns the generation that will identify its reply.
    ///
    /// `Err` is a worker that is not receiving (S15h): the window clears the pending
    /// grid it would otherwise mark and reports the reason, because a request nobody
    /// will answer is a canvas that waits for ever.
    pub fn request(
        &mut self,
        doc: &CollageDoc,
        sources: Vec<Option<PathBuf>>,
        grid: PixelSize,
    ) -> Result<u64, Down> {
        self.canvas_generation += 1;
        let job = Job::Canvas(CanvasJob {
            generation: self.canvas_generation,
            doc: doc.clone(),
            sources,
            grid,
        });
        self.jobs.send(job).map_err(|_| {
            glib::g_warning!("pixlay", "{}", Kind::reason(Kind::Decode, Down::Gone));
            Down::Gone
        })?;
        Ok(self.canvas_generation)
    }

    /// Queues a gallery job and returns the generation that will identify its
    /// reply.
    ///
    /// No document and no sources: a candidate is a template's geometry (S21).
    pub fn request_gallery(
        &mut self,
        candidates: Vec<(Template, PixelSize)>,
        style: Sketch,
    ) -> Result<u64, Down> {
        self.gallery_generation += 1;
        let job = Job::Gallery(GalleryJob {
            generation: self.gallery_generation,
            candidates,
            style,
        });
        self.jobs.send(job).map_err(|_| {
            glib::g_warning!("pixlay", "{}", Kind::reason(Kind::Decode, Down::Gone));
            Down::Gone
        })?;
        Ok(self.gallery_generation)
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
            for (template, grid) in &job.candidates {
                // A candidate is the template's own geometry, drawn — not a render
                // of the document with that template applied — so this branch names
                // no file and cannot decode anything (S21).
                if let Ok(image) = sketch_rgb8(template, *grid, &job.style) {
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
