//! The picker's decode worker: small pictures of many files, off the main thread.
//!
//! The picker's grid asks for one square tile per *visible* photo (S13b) and its
//! preview pane for one larger picture at a time. Neither is a document render —
//! there is no `CollageDoc` here, and no slot — so this is not [`crate::decode`]'s
//! job: that worker builds a whole document's bitmaps for one grid and coalesces
//! by "latest wins", which is exactly wrong here, where every request is wanted
//! and they are all independent.
//!
//! What is the same as `decode.rs`, and is the whole point of the file: a decode
//! is 11–110 ms per photo (S4) and a folder is unbounded, so the work happens on
//! one worker thread and crosses back as **plain bytes**
//! (`glib::MainContext::invoke`). A GTK object never leaves the main thread.
//!
//! And what makes the numbers the *pipeline's* numbers: a tile and the pane's picture
//! are [`pixlay_imaging::thumbnail`] (a whole photo at a long edge) or
//! [`pixlay_imaging::thumbnail_region`] (a rectangle of one at a long edge, which is the
//! pane's 1:1 view), and the CLI's `thumb` writes through those same two calls — so
//! "what the pane shows" and "what `pixlay-render thumb` writes" are one implementation,
//! and S13's and S15j's pixel criteria are comparisons of two calls rather than of two
//! resamplers.
//!
//! # What this file does not do (S13b)
//!
//! It does not remember what has been asked for, which S13's `seen` deque did: the
//! picker owns that now, because a request's identity is `(file index, what is
//! wanted)` — the size for a tile, the [`View`] for the pane (`docs/CONTRACT.md` §9)
//! — and only the picker knows which of those answers it still has or still wants.
//! What is left here is **cancellation**: a cell that scrolls away is dropped from
//! [`Thumbs::wanted`], and a job that is still queued when its key is gone is
//! discarded without a decode — gthumb's own policy (`src/Thumbnailer.vala`'s
//! priority queue with `remove`/`cancel`), which is what keeps a folder scrolled
//! quickly from queueing work nobody will look at, and what keeps a pan at 1:1 from
//! queueing a decode per pointer event.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use gtk4::glib;

use crate::workers::{Down, Kind as WorkerKind, WorkerPlan};
use pixlay_imaging::{Source, Thumbnail, thumbnail, thumbnail_region};

/// What the preview pane is asking a photo for.
///
/// Two states and no more (ruled 2026-09-24, PIX-028: "fit ↔ 1:1, panning at 1:1, no
/// free zoom and no view rotation"), and the request *is* the identity: the picker's
/// cache and this worker's dedup and cancellation are keyed by it, because the same
/// photo at another size, or at another place, is another picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    /// The `Contain` fit: the whole photo, resampled so its long edge is `px` pixels.
    ///
    /// The size is carried rather than derived, because the first request for a photo
    /// cannot know it: nothing has decoded the file yet, so the pane asks at its own
    /// long edge (the largest size it could need) and re-asks at the fitted size when
    /// the reply reports the photo's own pixels.
    Fit { px: u32 },
    /// 1:1 — one image pixel per device pixel — showing `rect` of the photo.
    ///
    /// The decode is the rectangle's own size (`max(width, height)` on its long edge,
    /// which `thumb_size` maps back to exactly `width x height`), so the pane's picture
    /// at 1:1 is the photo's own pixels rather than an enlarged fit.
    Actual(pixlay_imaging::Rect),
}

impl View {
    /// The long edge this view is decoded at, in pixels.
    pub fn px(&self) -> u32 {
        match self {
            View::Fit { px } => *px,
            View::Actual(rect) => rect.width.max(rect.height),
        }
    }
}

/// What is wanted of one photo.
///
/// Two kinds, because an answer has to reach the right widget and because the two are
/// asked for at different sizes and re-asked for different reasons: a tile follows the
/// cell it is bound to, the pane's picture follows the focused photo and the pane's own
/// size. The whole request is the key — see [`View`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Want {
    /// One grid cell's square tile, `px` device pixels on its long edge.
    Tile { px: u32 },
    /// The preview pane's picture, in one of its two states.
    Preview(View),
}

/// A request's identity: the folder it belongs to, what is wanted, and for which file.
///
/// **The generation is part of it**, and that is not decoration: `forget` clears
/// the wanted set when the folder changes, and without the generation a request
/// made afterwards for the same file index and size would be the *same* key — it
/// would revive a job still queued from the folder before it, whose reply the
/// picker then rejects as stale while its own request is skipped for looking
/// already answered. Measured 2026-09-22: that left six tiles "in flight" for
/// three minutes on a folder of fourteen photos.
type Key = (u64, usize, Want);

/// The key for one request, as the worker and the canceller both spell it.
fn key(epoch: u64, index: usize, want: Want) -> Key {
    (epoch, index, want)
}

/// One request: the file, what is wanted of it, and what to call the answer.
struct Job {
    /// The folder generation this job belongs to (`Thumbs::epoch`): a reply that
    /// arrives after the folder changed is not an answer to anything any more.
    epoch: u64,
    index: usize,
    path: PathBuf,
    want: Want,
}

/// What one finished job carries back.
pub struct Reply {
    pub epoch: u64,
    pub index: usize,
    pub want: Want,
    pub result: Result<Thumbnail, String>,
}

/// The window's handle on the tile worker.
///
/// Every method is called from the main thread. Sending is the only way in; the
/// caches, the dedup and the routing of the answers are the picker's (S13b).
pub struct Thumbs {
    jobs: Sender<Job>,
    /// The keys whose answer is still wanted: the worker skips a job whose key has
    /// been removed while it waited in the queue.
    wanted: Arc<Mutex<HashSet<Key>>>,
    /// The folder generation, bumped by [`forget`](Self::forget). Main thread only,
    /// which is why a `Cell` is enough.
    epoch: Cell<u64>,
}

impl Thumbs {
    /// Starts the worker. `reply` is called on the main context, in the order the
    /// jobs finish, which is the order they were queued (one thread).
    ///
    /// `Err` is a thread that could not be started (S15h, PIX-014): the picker then
    /// reports it in the cell that asked, rather than panicking on the way into the
    /// window.
    pub fn spawn(reply: impl Fn(Reply) + Send + Sync + 'static) -> Result<Self, Down> {
        Self::spawn_with(reply, WorkerPlan::Run)
    }

    /// [`spawn`](Self::spawn) with the plan `window::Workers` names.
    pub fn spawn_with(
        reply: impl Fn(Reply) + Send + Sync + 'static,
        plan: WorkerPlan,
    ) -> Result<Self, Down> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let wanted = Arc::new(Mutex::new(HashSet::new()));
        let reply = Arc::new(reply);
        let worker_wanted = Arc::clone(&wanted);
        plan.start(WorkerKind::Thumbs, move || {
            work(queue, worker_wanted, reply)
        })?;
        Ok(Self {
            jobs,
            wanted,
            epoch: Cell::new(0),
        })
    }

    /// The folder generation a request sent now would carry.
    pub fn epoch(&self) -> u64 {
        self.epoch.get()
    }

    /// Forgets every outstanding request, because the files they were asked about
    /// are not the files on screen any more.
    ///
    /// The queued jobs are dropped when the worker reaches them, and a job already
    /// decoding replies with the epoch it was sent under, which the picker
    /// recognises as stale and ignores (`docs/2026-09-22-STEPS.md`, `S13 · Ruling`:
    /// "a folder change invalidates all of it").
    pub fn forget(&self) {
        self.wanted
            .lock()
            .expect("the request set is not poisoned")
            .clear();
        self.epoch.set(self.epoch.get().wrapping_add(1));
    }

    /// Queues a grid tile.
    ///
    /// `Err` is a worker that is not receiving (S15h, PIX-014): the picker drops the
    /// request it had marked in flight and shows the reason in the cell, because a
    /// cell that waits for a reply that cannot come spins for ever.
    pub fn request_tile(&self, index: usize, path: &Path, px: u32) -> Result<(), Down> {
        self.request(index, path, Want::Tile { px })
    }

    /// Queues the preview pane's picture, at one of its two states.
    pub fn request_preview(&self, index: usize, path: &Path, view: View) -> Result<(), Down> {
        self.request(index, path, Want::Preview(view))
    }

    /// Drops a tile request: its cell has been unbound, scrolled away or covered
    /// by a different size.
    ///
    /// A cancel is always about the folder being listed now — what an earlier
    /// folder left behind was dropped by [`forget`](Self::forget).
    pub fn cancel_tile(&self, index: usize, px: u32) {
        self.cancel(index, Want::Tile { px });
    }

    /// Drops the preview request for a photo in one view — the pane has moved on to
    /// another one, and a decode nobody will see is the cost of not dropping it.
    pub fn cancel_preview(&self, index: usize, view: View) {
        self.cancel(index, Want::Preview(view));
    }

    fn request(&self, index: usize, path: &Path, want: Want) -> Result<(), Down> {
        let key = key(self.epoch.get(), index, want);
        self.wanted
            .lock()
            .expect("the request set is not poisoned")
            .insert(key);
        let job = Job {
            epoch: self.epoch.get(),
            index,
            path: path.to_path_buf(),
            want,
        };
        if self.jobs.send(job).is_err() {
            // The key goes with the request nobody will answer: leaving it would
            // make the canceller's `remove` a no-op and the caller's own in-flight
            // entry the only trace of a job that never existed.
            self.wanted
                .lock()
                .expect("the request set is not poisoned")
                .remove(&key);
            glib::g_warning!(
                "pixlay",
                "{}",
                crate::workers::Kind::reason(crate::workers::Kind::Thumbs, Down::Gone)
            );
            return Err(Down::Gone);
        }
        Ok(())
    }

    fn cancel(&self, index: usize, want: Want) {
        self.wanted
            .lock()
            .expect("the request set is not poisoned")
            .remove(&key(self.epoch.get(), index, want));
    }
}

fn work(
    queue: Receiver<Job>,
    wanted: Arc<Mutex<HashSet<Key>>>,
    reply: Arc<dyn Fn(Reply) + Send + Sync>,
) {
    while let Ok(job) = queue.recv() {
        let job_key = key(job.epoch, job.index, job.want);
        let wanted_still = wanted
            .lock()
            .expect("the request set is not poisoned")
            .contains(&job_key);
        if !wanted_still {
            continue;
        }
        // A file the decoder refuses is a value, not a panic: the cell reports it
        // and the folder keeps listing (`scan`'s S9 rule, applied to the grid).
        let result = Source::decode(&job.path)
            .and_then(|source| match job.want {
                Want::Tile { px } => thumbnail(&source, px),
                // At 1:1 the decode is the rectangle's own size, so the pane's
                // picture is the photo's own pixels; the fit is the whole photo at a
                // long edge. One preview pipeline, two requests.
                Want::Preview(View::Fit { px }) => thumbnail(&source, px),
                Want::Preview(View::Actual(rect)) => {
                    thumbnail_region(&source, rect, rect.width.max(rect.height))
                }
            })
            .map_err(|error| error.to_string());
        wanted
            .lock()
            .expect("the request set is not poisoned")
            .remove(&job_key);
        let reply = Arc::clone(&reply);
        let done = Reply {
            epoch: job.epoch,
            index: job.index,
            want: job.want,
            result,
        };
        glib::MainContext::default().invoke(move || reply(done));
    }
}
