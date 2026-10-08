// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless renderer and the automation surface every step's verification loop
//! runs against.
//!
//! Boundary: this crate must not depend on gtk4 — it is the fast loop, and its
//! compile time is the iteration cost.
//!
//! The command contract (frozen in S1, `docs/CONTRACT.md`; the export modes are
//! S6's):
//!
//! ```text
//! pixlay-render render    --project <file.pixlay> --long-edge <n> --out <file>
//! pixlay-render render    --template <name> --long-edge <n> --out <file>   # no photos
//! pixlay-render render    --project <file.pixlay> --long-edge <px> --out <file>
//! pixlay-render probe     --project <file.pixlay>
//! pixlay-render image     --photo <file>
//! pixlay-render templates [--aspect <ratio>] [--json]
//! pixlay-render init      --template <name> --out <file.pixlay>
//! ```
//!
//! The output format follows `--out`'s extension (`.png`, `.jpg`, `.jpeg`,
//! `.avif` — three formats since S34 added AVIF to S12c's two, which had removed
//! TIFF), and the encoder writes the sRGB profile and the JPEG sampling factors in
//! the same pass as the pixels (`pixlay_imaging::encode`).
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
pub mod report;
pub mod stats;
