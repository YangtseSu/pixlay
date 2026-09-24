//! The export destination: the one file an export may not be.
//!
//! `AGENTS.md`'s first hard constraint is that a source image is read-only — every
//! edit is a parameter, never a pixel written back — and decoding a photo and then
//! writing the render to that same path destroys it: the render is not the photo, and
//! the photo is gone. The paths that reach that are ordinary ones: `render --out
//! photos/a.jpg` where `a.jpg` is one of the document's own photos, `thumb --photo
//! a.jpg --out a.jpg`, an export to a name that is a symbolic link to a photo, or one
//! that is a hard link to it (S15c, PIX-001).
//!
//! Two rules answer it, and both are needed:
//!
//! * **the spelling**, with `.` and `..` resolved lexically: `photo/../photo/a.jpg`
//!   is `photo/a.jpg` whatever the filesystem has at either name — no filesystem
//!   access, no symlink resolution, so the answer does not depend on the machine;
//! * **the identity**, device and inode: a symbolic link and a hard link are two
//!   paths to one file, and only the filesystem knows. A path that does not exist has
//!   no identity and cannot be a source — a source is decoded, so it is there — which
//!   is why this compares what exists instead of canonicalizing what might.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Refuses an output path that names one of the document's own source images.
///
/// `sources` is the document's own list — one entry per cell, `None` for an empty
/// one, as `Project::sources` and the window both resolve it. The rule is the
/// product's, not one surface's: `render`, `thumb` and the GUI export all ask this
/// function before they ask anything else, so a render can never be the way a photo
/// is lost.
pub fn refuse_source_alias(out: &Path, sources: &[Option<PathBuf>]) -> Result<(), SourceAlias> {
    let spelling = absolute_normalized(out);
    for source in sources.iter().flatten() {
        if absolute_normalized(source) == spelling || same_file(out, source) {
            return Err(SourceAlias {
                out: out.to_path_buf(),
                photo: source.clone(),
            });
        }
    }
    Ok(())
}

/// The output path is one of the document's source images.
///
/// One message for every caller: the CLI reports it as a usage error and the window
/// as a toast, and both name the file that would have been destroyed.
#[derive(Debug, Error)]
#[error("refusing to write {out}: it is the source image {photo}")]
pub struct SourceAlias {
    /// The path the caller asked to write.
    pub out: PathBuf,
    /// The photo that path names.
    pub photo: PathBuf,
}

/// `path` made absolute and lexically normalized: `.` dropped, `..` resolved against
/// the component before it, nothing else touched.
fn absolute_normalized(path: &Path) -> PathBuf {
    let Ok(absolute) = std::path::absolute(path) else {
        // No current directory to be absolute against: the caller's own spelling is
        // the best available answer, and the identity rule below still applies.
        return path.to_path_buf();
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                // `a/..` is the directory `a` stands in; `/..` is `/`.
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::Prefix(_) | Component::RootDir) => {}
                // Nothing to go back to, or already walking up: keep walking.
                _ => normalized.push(".."),
            },
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Whether the two paths name one file, by the filesystem's own answer: device and
/// inode. A symbolic link is followed (`metadata`, not `symlink_metadata`) and a hard
/// link is the same file, which is the whole point.
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    let (Ok(a), Ok(b)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    a.dev() == b.dev() && a.ino() == b.ino()
}
