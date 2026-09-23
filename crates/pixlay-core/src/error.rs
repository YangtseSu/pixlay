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

    /// A number that has to be finite and is not. This is not `OutOfRange`'s case:
    /// there is no range to be outside of, and a message that named one would be
    /// naming a bound the value is not being compared against.
    #[error("{what} must be a finite number, got {value}")]
    NotFinite { what: &'static str, value: f64 },

    /// A command named a slot the template does not have. The command history
    /// reports this instead of panicking: a GUI that loses its selection while a
    /// background command lands is a caller bug, not a reason to abort.
    #[error("slot {slot} does not exist; the template has {slots} slots")]
    NoSuchSlot { slot: usize, slots: usize },

    /// A restore of a removed cell found its slot occupied again: the document
    /// changed while the cell was out, and putting the old contents back would
    /// silently drop whatever a later edit put there (`crate::selection`).
    #[error("slot {slot} already holds a photo; restoring would overwrite it")]
    SlotOccupied { slot: usize },

    /// A photo was added to a document that already holds `max` of them (S14).
    ///
    /// The ceiling is a *request* limit, exactly like the picker's own refusal
    /// (`crate::selection::SelectionError::PhotoCount`): it fires where a photo is
    /// added, and it names the same number. The format's slot limit
    /// ([`MAX_SLOTS`](crate::MAX_SLOTS)) is that number too, so there is no layout
    /// left to grow into.
    #[error("a collage takes at most {max} photos")]
    TooManyPhotos { max: usize },

    /// A batch removal found every cell empty (S14). The GUI's `−` is insensitive
    /// below the floor, so this is reached by a caller that asked anyway —
    /// `edit --remove-photo` on a document with no photos.
    #[error("no photo to remove: every cell is empty")]
    NothingToRemove,

    #[error("canvas would be {pixels} pixels; the limit is {max}")]
    CanvasTooLarge { pixels: u64, max: u64 },
}
