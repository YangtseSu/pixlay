//! Typed errors for the document model.
//!
//! Every message is English and identifier-like: `pixlay-cli` prints them as
//! they are, and the GUI shows them next to the offending slot. Nothing here is
//! localized, and nothing here panics.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("project JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// A document written by a newer version is never guessed at or downgraded:
    /// the user is told which version is needed.
    #[error("document version {found} is newer than the supported version {supported}")]
    VersionTooNew { found: u32, supported: u32 },

    /// An older version is only reached when a breaking change bumped
    /// `DOC_VERSION`. There is no migration by decision, so the message has to say
    /// what the user can do about it.
    #[error(
        "document version {found} predates the current format {supported} and cannot be \
         opened: rebuild the project with this version"
    )]
    VersionUnsupported { found: u32, supported: u32 },

    #[error("source image does not exist: {path}")]
    MissingSource { path: PathBuf },

    #[error("template name must not be empty")]
    EmptyTemplateName,

    #[error("template has {found} slots; the limit is {min}..={max}")]
    SlotCount {
        found: usize,
        min: usize,
        max: usize,
    },

    #[error("document has {cells} cells but its template has {slots} slots")]
    CellCount { cells: usize, slots: usize },

    #[error("slot {slot}: {reason}")]
    InvalidSlot { slot: usize, reason: &'static str },

    #[error("canvas aspect {canvas} does not match the template aspect {template}")]
    AspectMismatch { canvas: f64, template: f64 },

    #[error("template slot {slot} declares area {declared} but its outline covers {outline}")]
    SlotAreaMismatch {
        slot: usize,
        declared: f64,
        outline: f64,
    },

    #[error("{what} is {value} but must be in {min}..={max}")]
    OutOfRange {
        what: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },

    #[error("dpi {dpi} is outside {min}..={max}")]
    DpiOutOfRange { dpi: u32, min: u32, max: u32 },

    #[error("canvas would be {pixels} pixels; the limit is {max}")]
    CanvasTooLarge { pixels: u64, max: u64 },

    #[error("text layer {layer} uses unknown token {{{token}}}; v1 knows {known}")]
    UnknownTextToken {
        layer: usize,
        token: String,
        known: &'static str,
    },

    /// A tiled layer's step must be positive in both components or the tiling
    /// never terminates. There is no upper bound, so this is its own error rather
    /// than an `OutOfRange` whose range would be misleading.
    #[error("tiled text step must be positive in both components, got ({x}, {y})")]
    InvalidTiledStep { x: f64, y: f64 },

    /// The step has no upper *bound*, but a small one asks for an unbounded amount
    /// of work. Refused at load time so the document is rejected where the user can
    /// still do something about it.
    #[error("tiled text step ({x}, {y}) asks for more than {max} tiles")]
    TooManyTiles { x: f64, y: f64, max: usize },
}
