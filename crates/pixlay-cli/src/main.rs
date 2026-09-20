//! Headless render entry point and the automation tool used by every step's
//! verification loop.
//!
//! Boundary: this crate must not depend on gtk4 — it is the fast loop, and its
//! compile time is the iteration cost.
//!
//! S1 implements the two command faces:
//!
//! ```text
//! pixlay-render render --template <name> --dpi <n> --out <file>   # smoke, no photos
//! pixlay-render render --project <file.pixlay> --dpi <n> --out <file>
//! ```
//!
//! Exit codes: 0 success, 1 usage error, 2 decode or render failure.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("pixlay-render: not implemented yet; the CLI arrives in S1 (docs/STEPS.md)");
    ExitCode::from(2)
}
