//! Which files in a folder are photos: the one list, and the one walk.
//!
//! One answer, not one per surface: the CLI's `scan` (S9) walks a folder with
//! these, and `--help` documents only these extensions
//! (`crates/pixlay-cli/tests/cli.rs` fails when the two drift). So the extension
//! list and the walk live here, once, instead of beside the caller.
//!
//! This module is about *decoding capability* rather than about pixels: the list
//! is the photo formats this build's loaders actually read, which is a fact
//! `pixlay-imaging` owns (`decode.rs`'s limits, `thumb.rs`'s previews).
//!
//! Two rules the walk obeys, and both are load-bearing for a folder browser:
//!
//! * **Lexical by path, and nothing else.** A folder's order must not depend on
//!   the filesystem's iteration order (which is arbitrary and differs between
//!   filesystems), or two runs over an unchanged folder would list differently
//!   and a listing could not be diffed (`scan`'s S9 criterion).
//! * **A refusal is not a skip, but it is also not an error.** The walk reports
//!   only files whose *extension* says photo; whether the decoder can then read
//!   one is the caller's business (`scan` reports it as a row with a reason rather
//!   than dropping it). Deciding by extension is what keeps
//!   a folder's README from becoming an error row.

use std::path::{Path, PathBuf};

use crate::error::ImagingError;

/// Extensions the photo surfaces treat as photos.
///
/// The decoders this build links read more formats than these — the loaders
/// carry GIF, BMP, TGA, DDS and more, and an SVG is not a photo at all — and a
/// listing has to decide *before* it decodes, because reporting every file it
/// cannot read would turn a folder's README into an error row. So this is the
/// photo list: the formats a camera, a phone and a screenshot produce. A file
/// with another extension is not listed, and the CLI's `image` / `render` still
/// accept one when it is named directly.
///
/// The CLI's `--help` documents the same list, and `crates/pixlay-cli/tests/cli.rs`
/// fails if the two drift apart.
pub const PHOTO_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "avif", "jxl", "webp", "tif", "tiff",
];

/// Whether a path's extension is one of [`PHOTO_EXTENSIONS`].
///
/// Case-insensitive, because a camera writes `.JPG` and a phone `.HEIC`, and the
/// user means the same thing by both.
pub fn is_photo(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| PHOTO_EXTENSIONS.contains(&extension.as_str()))
}

/// The photo files in `dir`, lexically sorted.
///
/// `recursive` descends into subdirectories; without it only the folder itself is
/// read, which is what a folder browser opens. Only real directories are descended into:
/// a symlink that points at its own parent would otherwise make a recursive walk
/// run forever, and following links is not what "the photos in this folder"
/// means.
pub fn list_folder(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, ImagingError> {
    let mut found = Vec::new();
    collect(dir, recursive, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<(), ImagingError> {
    let entries = std::fs::read_dir(dir).map_err(|source| ImagingError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ImagingError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| ImagingError::Io {
            path: path.clone(),
            source,
        })?;
        if kind.is_dir() {
            if recursive {
                collect(&path, recursive, out)?;
            }
            continue;
        }
        if is_photo(&path) {
            out.push(path);
        }
    }
    Ok(())
}
