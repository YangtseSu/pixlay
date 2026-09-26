//! The window's two background workers: how they start, and what a window does
//! when one of them cannot.
//!
//! Decoding (`decode.rs`) and exporting (`export.rs`) each run on a thread of their
//! own, and each of them is one `std::thread::Builder::spawn` away from the window.
//! That call can fail — a process at its thread limit — and a thread that has started
//! can be gone by the time the next request arrives. Until S15h neither case ended
//! anywhere: the spawn `expect`ed and took the window down with it, and a `send` whose
//! failure was discarded left the request marked pending, so the canvas or the
//! progress bar waited for a reply that could never come (PIX-014).
//!
//! **A worker that cannot start or is gone is a report, not a panic.** Starting
//! answers [`Result`]; every request answers it too; and the caller clears the state it
//! had marked pending *before* it shows the reason — a request that was never queued is
//! not one to wait for.
//!
//! # What the tests name
//!
//! The product starts both workers with [`WorkerPlan::Run`]. The plan's other two
//! arms exist so the failure branches have a test that needs no process to run out of
//! threads: `Fail` makes the start return an error, and `Vanish` runs the body nowhere,
//! so the request channel has no receiver and the first `send` fails — the state a
//! worker that died leaves behind. [`crate::window::EditorWindow::with_workers`] is the
//! way in, `EditorWindow::new` is the product's own.

use gtk4::glib;

use crate::i18n::gettext;

/// Why a worker is not answering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Down {
    /// The worker's thread could not be started.
    Start,
    /// The worker's thread is gone: nothing is receiving its requests.
    Gone,
}

/// Which worker a report is about.
///
/// The two are told apart because the user's next move differs: the canvas's decoder
/// is what the window is showing, and an export is a thing to try again. The copy
/// lives here rather than at the two call sites so that one of them cannot end up
/// worded differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Decode,
    Export,
}

impl Kind {
    /// The sentence to show when this worker does not answer.
    pub fn message(self, down: Down) -> String {
        match (self, down) {
            (Self::Decode, Down::Start) => gettext("The photo decoder could not be started"),
            (Self::Decode, Down::Gone) => gettext("The photo decoder stopped"),
            (Self::Export, Down::Start) => gettext("The export worker could not be started"),
            (Self::Export, Down::Gone) => gettext("The export worker stopped"),
        }
    }

    /// The diagnostics line's own words, which are English and never translated.
    pub fn reason(self, down: Down) -> &'static str {
        match (self, down) {
            (Self::Decode, Down::Start) => "the decoding thread could not be started",
            (Self::Decode, Down::Gone) => "the decoding thread is gone",
            (Self::Export, Down::Start) => "the export thread could not be started",
            (Self::Export, Down::Gone) => "the export thread is gone",
        }
    }
}

/// How a window starts each of its background workers (S15h, PIX-014).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkerPlan {
    /// Start the thread — the product's own plan.
    #[default]
    Run,
    /// The start fails, as it does when the process cannot make another thread.
    Fail,
    /// The thread starts and is already gone: nothing receives the requests.
    Vanish,
}

/// One plan per worker: the two fail independently, and the tests check them one at a
/// time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Workers {
    pub decode: WorkerPlan,
    pub export: WorkerPlan,
}

impl WorkerPlan {
    /// Runs `body` on a thread of its own, or does not, per the plan.
    ///
    /// `body` owns the worker's request receiver, so `Vanish` leaving it unrun is
    /// exactly "the other end of the channel is gone": the caller's first `send`
    /// fails, which is the branch a thread that died takes.
    pub(crate) fn start(
        self,
        kind: Kind,
        body: impl FnOnce() + Send + 'static,
    ) -> Result<(), Down> {
        match self {
            Self::Run => std::thread::Builder::new()
                .name(format!("pixlay-{}", thread_name(kind)))
                .spawn(body)
                .map(|_handle| ())
                .map_err(|error| {
                    glib::g_warning!("pixlay", "{}: {error}", Kind::reason(kind, Down::Start));
                    Down::Start
                }),
            Self::Fail => Err(Down::Start),
            Self::Vanish => {
                drop(body);
                Ok(())
            }
        }
    }
}

/// The thread's own name, for `ps` and the crash reports.
fn thread_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Decode => "decode",
        Kind::Export => "export",
    }
}
