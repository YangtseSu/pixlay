//! Headless renderer and the automation surface every step's verification loop
//! runs against.
//!
//! Boundary: this crate must not depend on gtk4 — it is the fast loop, and its
//! compile time is the iteration cost.
//!
//! The command contract (frozen in S1, `docs/CONTRACT.md`):
//!
//! ```text
//! pixlay-render render --project <file.pixlay> --dpi <n> --out <file>
//! pixlay-render render --template <name> --dpi <n> --out <file>   # no photos
//! pixlay-render probe  --project <file.pixlay>
//! ```
//!
//! * stdout carries only the machine-readable result (sorted `key = value`
//!   lines, or one JSON object with `--json`); diagnostics go to stderr.
//! * the same input produces byte-identical output; no timestamps, no durations
//!   and no absolute paths in the result. `--stats` is the one exception: it
//!   reports measurements, which vary by definition.
//! * stdout and stderr are byte-identical for any `LANG` / `LC_ALL` /
//!   `LANGUAGE` setting — nothing here is localized, and no message is
//!   translated.
//! * zero interaction: stdin is never read, no prompt is ever shown, and a
//!   missing TTY changes nothing.
//! * exit codes: 0 success, 1 usage error, 2 project, decode or render failure.
//!   On failure the offending path is on stderr and stdout stays empty.
//!
//! The binary is a thin shell over [`cli::run`], which is also the entry point
//! the integration tests call directly.

pub mod args;
pub mod cli;
pub mod content;
pub mod encode;
pub mod probe;
pub mod report;
pub mod stats;
