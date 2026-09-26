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

    /// A project file this build read but could not accept (S15h, PIX-021).
    ///
    /// A parse or validation message on its own does not say *which* file was
    /// wrong, and one run can name several project paths; the file-backed loaders
    /// wrap their failure with the path they read and keep the inner message as it
    /// is. [`Io`](Self::Io) is deliberately not wrapped: it already carries a path,
    /// and wrapping it would print that path twice.
    #[error("{path}: {source}")]
    AtPath {
        path: PathBuf,
        #[source]
        source: Box<CoreError>,
    },

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

    /// A document whose own cell total exceeds the format's ceiling (S28).
    ///
    /// Since S28 a layout change keeps the cells it takes away, so the ceiling
    /// counts both lists: `placed + kept` is what the document holds — on the sheet
    /// and off it — and `MAX_SLOTS` is what bounds it. A *document* limit rather
    /// than a request one: no command builds such a document, and this is what
    /// refuses a hand-written file that does.
    #[error("document has {placed} cells and keeps {kept}; a collage takes at most {max} cells")]
    CellTotalOverLimit {
        placed: usize,
        kept: usize,
        max: usize,
    },

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

    /// A photo was added to a document that already holds `max` of them (S14).
    ///
    /// The ceiling is a *request* limit, exactly like `Selection`'s own refusal
    /// (`crate::selection::SelectionError::PhotoCount`): it fires where a photo is
    /// added, and it names the same number. The format's slot limit
    /// ([`MAX_SLOTS`](crate::MAX_SLOTS)) is that number too, so there is no layout
    /// left to grow into.
    #[error("a collage takes at most {max} photos")]
    TooManyPhotos { max: usize },

    /// The count control's `−` on a document already at the floor (S14b).
    ///
    /// A *request* limit like [`TooManyPhotos`](Self::TooManyPhotos), and the same
    /// number the selection's floor names: `MIN_SLOTS` is `MIN_PHOTOS`, so a layout
    /// cannot shrink into a collage the selection would not make. The floor is 1
    /// since S19, so the message is written for either number of cells —
    /// "at least 1 cells" is not a sentence.
    #[error("a collage's layout has at least {min} cell{}", if *min == 1 { "" } else { "s" })]
    TooFewCells { min: usize },

    /// The count control's `+` on a document at the format's slot limit (S14b).
    #[error("a collage's layout takes at most {max} cells")]
    TooManyCells { max: usize },

    /// A swap named the same slot twice (S14b). Not a `NoSuchSlot`: both indexes
    /// exist, and what is wrong is that the edit would change nothing.
    #[error("slot {slot} cannot be swapped with itself")]
    SameSlot { slot: usize },

    #[error("canvas would be {pixels} pixels; the limit is {max}")]
    CanvasTooLarge { pixels: u64, max: u64 },

    /// Two slots of a hand-authored template cover the same point (PIX-007, S15g).
    ///
    /// A document's geometry is embedded data and may be hand-written or generated,
    /// so the invariants the library's test alone used to hold have to be checked
    /// where the file is read: `draw` paints cells in index order while
    /// [`Template::slot_at`] answers with the first containing slot, so an overlap
    /// makes the painted pixels and the hit test disagree about who owns a region.
    ///
    /// [`Template::slot_at`]: crate::Template::slot_at
    #[error("template slots {a} and {b} overlap at ({x}, {y})")]
    SlotsOverlap { a: usize, b: usize, x: f64, y: f64 },

    /// The slots of a hand-authored template leave a region sealed off from the
    /// canvas border (PIX-007, S15g).
    ///
    /// Not the gutter a `g` layout ships: a gutter is uncovered too, and it reaches
    /// the border. A pocket that does not is a region no cell can show, which no
    /// frame expresses and the library's own invariant test calls a hole.
    #[error("template slots leave an interior hole at ({x}, {y})")]
    InteriorHole { x: f64, y: f64 },

    /// A template's slots declare more of the canvas than there is (PIX-007, S15g).
    ///
    /// Each declared area is cross-checked against its own outline before the sum
    /// is taken, so this can only be geometry that overlaps — but the sum is the
    /// number a caller can compare against the canvas, so it is the one the message
    /// names. A sum *below* 1.0 is not an error: that is what a gutter layout is.
    #[error("template slots declare {sum} of the canvas; at most 1.0 can be covered")]
    SlotAreasOverCanvas { sum: f64 },
}
