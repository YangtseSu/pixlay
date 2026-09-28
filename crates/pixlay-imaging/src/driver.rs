// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one thread that talks to the decoder.
//!
//! A glycin frame request only completes while a `MainContext` is being
//! iterated: the loader delivers its work back through the context, and a future
//! driven by a plain executor (`futures_lite::block_on`, measured 2026-09-21)
//! simply never resolves — `glycin-builtin` hung on every file that way and was
//! only stopped by glycin's own 60-second limit. So the pipeline owns exactly one
//! thread, gives it its own `MainContext`, and drives each job with
//! `MainContext::block_on`.
//!
//! Why not the global default context: in the GUI that context belongs to GTK's
//! main loop, which runs on the main thread. A decode must not need it, must not
//! be iterated by it, and must not be able to block it. A private context on a
//! private thread also means `MainContextSelector::Auto` inside glycin picks
//! *this* context (`with_thread_default`), instead of spawning a second hidden
//! loop of its own.
//!
//! Jobs are serialized by construction. That is the design, not a limitation:
//! decoding is the memory-heaviest stage (a 120 MP source is 960 MB as 16-bit
//! linear), and the caller's budget is what decides how many sources may exist at
//! once — see `docs/CONTRACT.md` §4's ladder. This thread never holds more than
//! the one source a job is decoding.

use std::future::Future;
use std::sync::mpsc::{Sender, channel};
use std::sync::{LazyLock, mpsc};

use glib::MainContext;

use crate::error::ImagingError;

/// A unit of work for the decoder thread. It runs on that thread, with its main
/// context set as the thread default.
type Job = Box<dyn FnOnce() + Send>;

/// The decoder thread's job sender and the main context it iterates.
///
/// The context is created here, on the caller's thread, so that a job can be
/// built with it in hand; the thread receives a clone and makes it that thread's
/// default. `MainContext` is reference counted, so both refer to the same one.
static DRIVER: LazyLock<Option<(Sender<Job>, MainContext)>> = LazyLock::new(start);

fn start() -> Option<(Sender<Job>, MainContext)> {
    let (sender, receiver) = channel::<Job>();
    let context = MainContext::new();
    let worker_context = context.clone();
    std::thread::Builder::new()
        .name("pixlay-imaging".to_string())
        .spawn(move || {
            // Everything on this thread runs with the context as its thread
            // default, so glycin loads its images on the context this thread
            // iterates (and does not start a hidden loop of its own).
            let _ = worker_context.with_thread_default(|| {
                for job in receiver {
                    job();
                }
            });
        })
        .ok()?;
    Some((sender, context))
}

/// Runs `job` on the decoder thread and returns its result.
///
/// `job` receives the thread's main context; it is expected to drive async work
/// with [`MainContext::block_on`]. It *must not* return before that work is done,
/// because the context is not iterated between jobs.
pub(crate) fn run<R, F>(job: F) -> Result<R, ImagingError>
where
    R: Send + 'static,
    F: FnOnce(MainContext) -> R + Send + 'static,
{
    let (sender, context) = DRIVER
        .as_ref()
        .ok_or_else(|| ImagingError::Driver("the decoder thread did not start".to_string()))?;
    let (done_tx, done_rx) = mpsc::channel();
    let context = context.clone();
    let sent = sender.send(Box::new(move || {
        // `block_on` iterates the context while it polls, which is what lets the
        // loader complete.
        let _ = done_tx.send(job(context));
    }));
    if sent.is_err() {
        return Err(ImagingError::Driver(
            "the decoder thread has stopped".to_string(),
        ));
    }
    done_rx
        .recv()
        .map_err(|_| ImagingError::Driver("the decoder thread panicked".to_string()))
}

/// Drives `future` to completion on this thread's main context.
pub(crate) fn block_on<F: Future>(context: &MainContext, future: F) -> F::Output {
    context.block_on(future)
}
