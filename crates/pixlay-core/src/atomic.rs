// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Replacing a file the way a crash cannot damage it: a temporary file beside the
//! target, one `rename`, and nothing at the target's own name until the content is
//! complete and on the disk.
//!
//! `rename` is atomic within one filesystem, so a reader — and a crash, and a kill —
//! sees either the previous file or the new one, never half of either. And because
//! the bytes only ever go into a temporary file, a failure at any step (the
//! caller's own writer, the flush, the sync, the rename) leaves the previous file
//! exactly as it was: an export that hits a full disk may not destroy the export
//! that was there (S15c, PIX-011).
//!
//! It lives in `pixlay-core` because two crates write files through it and they must
//! write them the same way: a project ([`crate::CollageDoc::save`], `docs/CONTRACT.md` §6)
//! and an image export (`pixlay_imaging::encode`).

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes the temporary files one process writes at the same time.
///
/// The process id alone would not: two threads writing two files into one directory
/// would name the same temporary file and truncate each other's half-written
/// content, which is the corruption this module exists to prevent.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// What a replacement failed at, with the caller's own writer error kept typed.
#[derive(Debug)]
pub enum Failure<E> {
    /// Creating, setting up, syncing or renaming the temporary file. The target is
    /// as it was and the temporary file is gone. The caller names the path — the
    /// one the user asked for, never this module's litter.
    Io(std::io::Error),
    /// The caller's own writer, as `body` reported it. The temporary file is gone
    /// and the target is as it was.
    Body(E),
}

impl<E: Into<std::io::Error>> Failure<E> {
    /// The whole failure as one `io::Error`, for a caller whose writer reports
    /// `io::Error` as well (a project save writes a string).
    pub fn into_io(self) -> std::io::Error {
        match self {
            Self::Io(source) => source,
            Self::Body(source) => source.into(),
        }
    }
}

/// Writes `target` through a temporary file beside it and renames that over it,
/// returning the number of bytes written.
///
/// `body` writes the content into the temporary file. Nothing is written at
/// `target`'s name until that content is complete, flushed and on the disk, so the
/// previous file survives every failure this can report.
///
/// **An existing target keeps its mode**, applied to the temporary file before any
/// content goes in: a project that is readable only by its owner may not come back
/// world-readable because it was written through a temporary file (S15c, PIX-016).
/// A target that is not there yet keeps the process's own umask default, which is
/// the statement of intent a new file already gets everywhere else.
///
/// **The path is what is replaced**: a symbolic link at `target` is replaced by the
/// regular file this wrote rather than followed, as `mv` does. A caller that must
/// not write over a *particular* file compares identity before asking
/// (`pixlay_imaging::destination`, the source-image rule).
pub fn write_atomic<E>(
    target: &Path,
    body: impl FnOnce(&mut BufWriter<File>) -> Result<(), E>,
) -> Result<u64, Failure<E>> {
    let dir = parent_of(target);
    let name = target.file_name().unwrap_or_else(|| OsStr::new("file"));
    // A dotfile in the target's own directory, so the rename cannot land on another
    // filesystem (the write is atomic only within one), unique per process and per
    // call (`SEQUENCE`) so two writes cannot collide. Assembled as an `OsString`
    // from parts, so a path that is not valid UTF-8 stays exact.
    let mut temp_name = OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let temp = dir.join(temp_name);

    let file = File::create(&temp).map_err(Failure::Io)?;
    if let Ok(existing) = std::fs::metadata(target) {
        // Before the content, not after: the mode is set while the file is still
        // the temporary one, so there is no moment where a private document is
        // readable by anyone but its owner.
        std::fs::set_permissions(&temp, existing.permissions()).map_err(Failure::Io)?;
    }
    // Buffered for the formats that arrive in many small pieces, and flushed by hand
    // because `BufWriter`'s own drop swallows the error.
    let mut writer = BufWriter::new(file);
    let outcome = body(&mut writer)
        .map_err(Failure::Body)
        .and_then(|()| writer.flush().map_err(Failure::Io))
        // `sync_all` before the rename: without it a crash can leave the new name
        // pointing at a file whose bytes never reached the disk, which is the one
        // way an atomic rename can still lose a document.
        .and_then(|()| writer.get_ref().sync_all().map_err(Failure::Io))
        .and_then(|()| std::fs::rename(&temp, target).map_err(Failure::Io));
    // The handle's own length rather than the path's: after the rename the two are
    // the same file, and this cannot be confused by anything that happened to the
    // name in between.
    let bytes = writer
        .get_ref()
        .metadata()
        .map(|metadata| metadata.len())
        .map_err(Failure::Io);
    drop(writer);
    match outcome.and(bytes) {
        Ok(bytes) => Ok(bytes),
        Err(failure) => {
            // The old file is untouched; only the temporary one is litter, and it
            // is this module's own name rather than the caller's.
            let _ = std::fs::remove_file(&temp);
            Err(failure)
        }
    }
}

/// The directory a path's file name lives in: its parent, or `.` when it has none
/// (`x.pixlay` names a file in the working directory).
fn parent_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}
