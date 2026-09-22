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
//! And what makes the numbers the *pipeline's* numbers: a tile is
//! [`pixlay_imaging::thumbnail`], the same function the CLI's `thumb` writes to a
//! file, so "what the grid shows" and "what `pixlay-render thumb` writes" are one
//! implementation — S13's pixel criterion is a comparison of two calls, not of
//! two resamplers.
//!
//! # What this file does not do (S13b)
//!
//! It does not remember what has been asked for, which S13's `seen` deque did: the
//! picker owns that now, because a request's identity is
//! `(kind, file index, device pixels)` and only the picker knows which of those
//! answers it still has or still wants. What is left here is **cancellation**: a
//! cell that scrolls away is dropped from [`Thumbs::wanted`], and a job that is
//! still queued when its key is gone is discarded without a decode — gthumb's own
//! policy (`src/Thumbnailer.vala`'s priority queue with `remove`/`cancel`), which
//! is what keeps a folder scrolled quickly from queueing work nobody will look at.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use gtk4::glib;

use pixlay_imaging::{Source, Thumbnail, thumbnail};

/// What a request is for.
///
/// Two kinds, because an answer has to reach the right widget, and because the
/// two are asked for at different sizes and re-asked for different reasons: a tile
/// follows the cell it is bound to, a preview follows the focused photo and the
/// pane's own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// One grid cell's square tile.
    Tile,
    /// The preview pane's picture.
    Preview,
}

/// A request's identity: the folder it belongs to, what is wanted, for which file,
/// at which size.
///
/// **The generation is part of it**, and that is not decoration: `forget` clears
/// the wanted set when the folder changes, and without the generation a request
/// made afterwards for the same file index and size would be the *same* key — it
/// would revive a job still queued from the folder before it, whose reply the
/// picker then rejects as stale while its own request is skipped for looking
/// already answered. Measured 2026-09-22: that left six tiles "in flight" for
/// three minutes on a folder of fourteen photos.
type Key = (u64, Kind, usize, u32);

/// The key for one request, as the worker and the canceller both spell it.
fn key(epoch: u64, kind: Kind, index: usize, px: u32) -> Key {
    (epoch, kind, index, px)
}

/// One request: the file, the size, and what to call the answer.
struct Job {
    /// The folder generation this job belongs to (`Thumbs::epoch`): a reply that
    /// arrives after the folder changed is not an answer to anything any more.
    epoch: u64,
    kind: Kind,
    index: usize,
    path: PathBuf,
    px: u32,
}

/// What one finished job carries back.
pub struct Reply {
    pub epoch: u64,
    pub kind: Kind,
    pub index: usize,
    pub px: u32,
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
    pub fn spawn(reply: impl Fn(Reply) + Send + Sync + 'static) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let wanted = Arc::new(Mutex::new(HashSet::new()));
        let reply = Arc::new(reply);
        let worker_wanted = Arc::clone(&wanted);
        std::thread::Builder::new()
            .name("pixlay-thumbs".to_string())
            .spawn(move || work(queue, worker_wanted, reply))
            .expect("the preview thread can be started");
        Self {
            jobs,
            wanted,
            epoch: Cell::new(0),
        }
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
    pub fn request_tile(&self, index: usize, path: &Path, px: u32) {
        self.request(Kind::Tile, index, path, px);
    }

    /// Queues the preview pane's picture.
    pub fn request_preview(&self, index: usize, path: &Path, px: u32) {
        self.request(Kind::Preview, index, path, px);
    }

    /// Drops a tile request: its cell has been unbound, scrolled away or covered
    /// by a different size.
    ///
    /// A cancel is always about the folder being listed now — what an earlier
    /// folder left behind was dropped by [`forget`](Self::forget).
    pub fn cancel_tile(&self, index: usize, px: u32) {
        self.cancel(Kind::Tile, index, px);
    }

    /// Drops the preview request for a photo at a size.
    pub fn cancel_preview(&self, index: usize, px: u32) {
        self.cancel(Kind::Preview, index, px);
    }

    fn request(&self, kind: Kind, index: usize, path: &Path, px: u32) {
        self.wanted
            .lock()
            .expect("the request set is not poisoned")
            .insert(key(self.epoch.get(), kind, index, px));
        // A send failure means the worker died; the grid keeps the files it has
        // and shows its loading state, which is the same state a decode error
        // leaves a cell in.
        let _ = self.jobs.send(Job {
            epoch: self.epoch.get(),
            kind,
            index,
            path: path.to_path_buf(),
            px,
        });
    }

    fn cancel(&self, kind: Kind, index: usize, px: u32) {
        self.wanted
            .lock()
            .expect("the request set is not poisoned")
            .remove(&key(self.epoch.get(), kind, index, px));
    }
}

fn work(
    queue: Receiver<Job>,
    wanted: Arc<Mutex<HashSet<Key>>>,
    reply: Arc<dyn Fn(Reply) + Send + Sync>,
) {
    while let Ok(job) = queue.recv() {
        let job_key = key(job.epoch, job.kind, job.index, job.px);
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
            .and_then(|source| thumbnail(&source, job.px))
            .map_err(|error| error.to_string());
        wanted
            .lock()
            .expect("the request set is not poisoned")
            .remove(&job_key);
        let reply = Arc::clone(&reply);
        let done = Reply {
            epoch: job.epoch,
            kind: job.kind,
            index: job.index,
            px: job.px,
            result,
        };
        glib::MainContext::default().invoke(move || reply(done));
    }
}
