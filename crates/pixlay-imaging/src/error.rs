//! Typed errors for the image pipeline.
//!
//! English identifiers, no localization and no panics (project language rules):
//! the CLI prints these as they are, the GUI attaches them to the failing cell.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImagingError {
    /// A document whose canvas or geometry cannot be laid out. The message is
    /// core's own: it is already English and already identifiers the offender.
    #[error("{0}")]
    Document(#[from] pixlay_core::CoreError),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The decoder refused the file, or the file is not an image at all. The
    /// message is the decoder's own text: it names the format failure better
    /// than a summary can, and it is English.
    #[error("{path}: cannot decode: {message}")]
    Decode { path: PathBuf, message: String },

    /// The size cap. A 120 MP cap bounds the decoded buffer at 960 MB
    /// (`docs/CONTRACT.md` §4), so an image past it is refused instead of
    /// decoding into a memory spike the machine may not have.
    #[error("{path}: {pixels} pixels exceeds the {max} pixel decode cap")]
    TooLarge {
        path: PathBuf,
        pixels: u64,
        max: u64,
    },

    /// The imaging thread is gone (it panicked, or the process is shutting
    /// down). Nothing can be decoded after that, so it is reported rather than
    /// retried.
    #[error("the imaging thread is not available: {0}")]
    Driver(String),

    /// A document that does not fit the slot count of its own template cannot be
    /// laid out. `CollageDoc::validate` catches this on load; an in-memory
    /// document reaches here.
    #[error("slot {slot} has no cell")]
    MissingCell { slot: usize },

    /// A slot with no shape to fit against: fewer than three vertices, or a
    /// degenerate bounding box. The clamp returns such a request untouched, so
    /// there is nothing sensible to size a bitmap from.
    #[error("slot {slot} cannot be laid out: empty geometry")]
    DegenerateSlot { slot: usize },

    /// The displayed size does not fit Cairo's 32-bit surface dimensions.
    #[error("slot {slot}: displayed bitmap {width}x{height} is out of range")]
    BitmapTooLarge {
        slot: usize,
        width: f64,
        height: f64,
    },
}
