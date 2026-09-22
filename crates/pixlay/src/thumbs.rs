//! The picker's decode worker: small previews of many files, off the main thread.
//!
//! The picker's grid asks for one small picture per file in a folder (S13), and
//! its preview pane for one larger one. Neither is a document render — there is
//! no `CollageDoc` here, and no slot — so this is not [`crate::decode`]'s job:
//! that worker builds a whole document's bitmaps for one grid and coalesces by
//! "latest wins", which is exactly wrong for a folder, where every request is
//! wanted and they are all independent.
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

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use gtk4::glib;

use pixlay_imaging::{Source, Thumbnail, thumbnail};

/// One request: the file, the size, and what to call the answer.
struct Job {
    index: usize,
    path: PathBuf,
    px: u32,
    /// The big preview pane's request rather than a grid tile. The two are
    /// distinct because a reply has to reach the right widget, and because the
    /// preview is re-asked when the focused photo changes.
    preview: bool,
}

/// What one finished job carries back.
pub struct Reply {
    pub index: usize,
    pub preview: bool,
    pub result: Result<Thumbnail, String>,
}

/// The window's handle on the tile worker.
///
/// `request_*` is called from the main thread only, and remembers what has been
/// asked for: a grid that scrolls re-binds its cells, and a rebind must not
/// become a second decode of the same file.
pub struct Thumbs {
    jobs: Sender<Job>,
    seen: Mutex<VecDeque<(usize, bool)>>,
}

impl Thumbs {
    /// Starts the worker. `reply` is called on the main context, in the order the
    /// jobs finish, which is the order they were queued (one thread).
    pub fn spawn(reply: impl Fn(Reply) + Send + Sync + 'static) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        let reply = Arc::new(reply);
        std::thread::Builder::new()
            .name("pixlay-thumbs".to_string())
            .spawn(move || work(queue, reply))
            .expect("the preview thread can be started");
        Self {
            jobs,
            seen: Mutex::new(VecDeque::new()),
        }
    }

    /// Queues a grid tile, unless that file is already asked for or built.
    pub fn request_tile(&self, index: usize, path: &Path, px: u32) {
        self.request(index, path, px, false);
    }

    /// Queues the preview pane's picture, unless it is already asked for.
    pub fn request_preview(&self, index: usize, path: &Path, px: u32) {
        self.request(index, path, px, true);
    }

    fn request(&self, index: usize, path: &Path, px: u32, preview: bool) {
        {
            let mut seen = self.seen.lock().expect("the tile queue is not poisoned");
            if seen.contains(&(index, preview)) {
                return;
            }
            seen.push_back((index, preview));
            // A folder is unbounded, so the record is trimmed: past a few thousand
            // files the oldest entries are forgotten, and forgetting only costs a
            // re-decode if the same folder is listed again.
            while seen.len() > 4096 {
                seen.pop_front();
            }
        }
        // A send failure means the worker died; the grid keeps the files it has
        // and shows its loading icon, which is the same state a decode error
        // leaves a cell in.
        let _ = self.jobs.send(Job {
            index,
            path: path.to_path_buf(),
            px,
            preview,
        });
    }
}

fn work(queue: Receiver<Job>, reply: Arc<dyn Fn(Reply) + Send + Sync>) {
    while let Ok(job) = queue.recv() {
        // A file the decoder refuses is a value, not a panic: the cell reports it
        // and the folder keeps listing (`scan`'s S9 rule, applied to the grid).
        let result = Source::decode(&job.path)
            .and_then(|source| thumbnail(&source, job.px))
            .map_err(|error| error.to_string());
        let reply = Arc::clone(&reply);
        let done = Reply {
            index: job.index,
            preview: job.preview,
            result,
        };
        glib::MainContext::default().invoke(move || reply(done));
    }
}
