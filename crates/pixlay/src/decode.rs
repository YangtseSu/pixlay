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
//!   that produces 60 requests per second therefore costs one build at a time,
//!   never a growing queue, and the canvas converges on where the pointer
//!   actually is.
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
//! not blank the collage the user is working on.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gtk4::glib;
use pixlay_core::{CollageDoc, PixelSize};
use pixlay_imaging::{Preview, SlotBitmap};

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
    /// Files this build decoded. Zero means both caches answered everything,
    /// which is what a step of a live gesture has to look like; the window counts
    /// them so a test can hold the gesture path to it.
    pub decodes: u64,
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
    // One cache pair for as long as the window lives: it is what makes the second
    // step of a gesture cost a fraction of the first.
    let mut preview = Preview::new();
    while let Ok(first) = queue.recv() {
        // Latest wins: everything already waiting is superseded by the newest
        // request, so the work a resize or a drag generates collapses to one job.
        let mut job = first;
        while let Ok(newer) = queue.try_recv() {
            job = newer;
        }
        let built = preview.build(&job.doc, &job.sources, job.grid);
        let done = Reply {
            generation: job.generation,
            bitmaps: built.bitmaps,
            failed: built.failed,
            decodes: built.decodes,
        };
        let reply = std::sync::Arc::clone(&reply);
        // The main context delivers this on the window's thread; `invoke` is the
        // one glib API that takes a `Send` closure, which is why nothing but the
        // reply and a `SendWeakRef` is captured here.
        glib::MainContext::default().invoke(move || reply(done));
    }
}
