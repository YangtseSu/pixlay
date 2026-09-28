// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Regenerates `crates/pixlay-core/src/templates/frozen.rs`.
//!
//! A committed bin, not a `build.rs` (`docs/CONTRACT.md` §3): the frozen
//! geometry is an interface — a document embeds a copy of it, so changing it
//! changes the layout of saved projects — and that change has to be a reviewed
//! commit rather than something a build rewrites silently.
//!
//! ```text
//! cargo run -p pixlay-core --bin pixlay-gen-templates [<out.rs>]
//! ```
//!
//! With no argument it writes the committed path. The determinism test passes a
//! scratch path and compares the result byte for byte with the committed file.

use std::path::PathBuf;
use std::process::ExitCode;

use pixlay_core::templates::generator;

fn main() -> ExitCode {
    let out = match std::env::args_os().nth(1) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/templates/frozen.rs"),
    };
    let source = generator::emit_source(&generator::generate());
    if let Err(error) = std::fs::write(&out, &source) {
        eprintln!("pixlay-gen-templates: {}: {error}", out.display());
        return ExitCode::FAILURE;
    }
    println!("{} ({} bytes)", out.display(), source.len());
    ExitCode::SUCCESS
}
