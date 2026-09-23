//! The selection policy: which photos a collage holds, and in which order.
//!
//! Stages 1–2 of the main path (`AGENTS.md`: `open → pick 2–9 photos → pick a
//! layout → adjust → export`) are a list of photos in the order the user picked
//! them, and **order is cell order**: the picker's picked list is where that mapping is
//! visible and re-orderable, not decoration. This module is the mapping itself,
//! and it is pure — no cairo, no GTK, no filesystem — so the picker (S13), the
//! layout stage (S14) and the CLI's `init --photo` share one implementation
//! instead of three that agree by luck.
//!
//! Three rules live here:
//!
//! * **the floor and the ceiling.** A collage needs `2..=9` photos (ruling 3).
//!   The floor is enforced where a selection becomes a document
//!   ([`Selection::document`]) and the ceiling where a photo is added
//!   ([`Selection::push`]), because those are the two moments a user can hit them:
//!   the picked list legitimately holds zero or one photo while the user is still
//!   picking, and a tenth is refused with a message rather than truncated.
//! * **the count filter.** The layouts a selection can use are the library's
//!   templates with exactly that many slots ([`Selection::layouts`]) — the picker
//!   offers a layout exactly when it has as many slots as the user picked photos,
//!   and since S12c the library itself stops at nine, so no layout exists that the
//!   picker could not offer.
//! * **the batch rule (LIFO).** [`remove_last`] clears the last *occupied* cell
//!   and nothing else, and the [`Removed`] it hands back puts that cell back
//!   where it was. A single cell can be cleared on its own (ruling 7), so "the
//!   last photo" is the last occupied cell rather than the last cell, and
//!   restoring means *that* slot — not the first empty one.
//! * **the count rule.** When the photo count changes, the layout changes with it:
//!   [`layout_for`] is the one answer to "which layout, when the choice is not
//!   obvious" (same aspect, then same recipe family, then the nearest aspect, then
//!   library order), so the composition's count control and the CLI's
//!   `edit --add-photo` / `--remove-photo` cannot disagree about it (S14).

use std::path::PathBuf;

use thiserror::Error;

use crate::ASPECT_TOLERANCE;
use crate::doc::{Cell, CollageDoc};
use crate::error::CoreError;
use crate::template::{Family, Template};
use crate::templates;

/// Fewest photos a collage can be made of.
pub const MIN_PHOTOS: usize = 2;

/// Most photos a collage can be made of (ruling 3).
///
/// The format's own ceiling ([`MAX_SLOTS`](crate::MAX_SLOTS)) is the same number
/// since S12c: the ten-slot recipe was the only layout above nine, and removing it
/// left one limit instead of two.
pub const MAX_PHOTOS: usize = 9;

/// Why a selection cannot do what was asked of it.
///
/// Both variants are *request* errors, not document errors: the caller asked for
/// a collage of one photo, or for a layout that does not have the count. The CLI
/// reports them as usage errors (exit 1), and the GUI shows them in place.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SelectionError {
    #[error("a collage needs {min}..={max} photos, got {found}")]
    PhotoCount {
        found: usize,
        min: usize,
        max: usize,
    },
    #[error("template {template} has {slots} slots but the selection has {photos} photos")]
    SlotCount {
        template: String,
        slots: usize,
        photos: usize,
    },
}

/// The photos a user has picked, in the order they will land in cells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    photos: Vec<PathBuf>,
}

impl Selection {
    /// A selection of `photos`, in cell order. Refuses more than [`MAX_PHOTOS`]
    /// (the ceiling is a *request* limit, so it fires here rather than silently
    /// dropping the tail); the floor is checked by [`document`](Self::document).
    pub fn new(photos: Vec<PathBuf>) -> Result<Self, SelectionError> {
        let mut selection = Self::default();
        for photo in photos {
            selection.push(photo)?;
        }
        Ok(selection)
    }

    /// The photos, in cell order.
    pub fn photos(&self) -> &[PathBuf] {
        &self.photos
    }

    pub fn len(&self) -> usize {
        self.photos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.photos.is_empty()
    }

    /// Whether another photo fits under the ceiling. The picker's "add" control
    /// reads this; below the floor is not this question.
    pub fn accepts_more(&self) -> bool {
        self.photos.len() < MAX_PHOTOS
    }

    /// Appends a photo and returns the cell index it landed in.
    pub fn push(&mut self, photo: PathBuf) -> Result<usize, SelectionError> {
        if !self.accepts_more() {
            return Err(SelectionError::PhotoCount {
                found: self.photos.len() + 1,
                min: MIN_PHOTOS,
                max: MAX_PHOTOS,
            });
        }
        self.photos.push(photo);
        Ok(self.photos.len() - 1)
    }

    /// Drops the last photo — the LIFO half of the batch control, on the
    /// selection. The document-side half is [`remove_last`], which also keeps the
    /// cell's framing so it can come back.
    pub fn pop(&mut self) -> Option<PathBuf> {
        self.photos.pop()
    }

    /// Drops the photo at `index` (ruling 7's per-cell clear). `None` past the
    /// end, so a stale index is a no-op rather than a panic.
    pub fn remove(&mut self, index: usize) -> Option<PathBuf> {
        (index < self.photos.len()).then(|| self.photos.remove(index))
    }

    /// The layouts this selection can use: every template with exactly this many
    /// slots, in library order.
    ///
    /// An empty selection asks for nothing and gets nothing — there is no
    /// zero-slot layout to offer, and a caller that wants the whole library asks
    /// `templates::all` itself.
    pub fn layouts(&self) -> Vec<Template> {
        templates::all()
            .into_iter()
            .filter(|template| template.slots.len() == self.photos.len())
            .collect()
    }

    /// The document these photos make on `template`, in cell order.
    ///
    /// This is the one place the picker's list becomes a document, so the CLI's
    /// `init --photo` and the GUI's Next cannot disagree about what "the third
    /// photo" means. Paths are stored exactly as given: making them relative to
    /// the project file is a *writing* concern (`Project::save_as`), not a
    /// selection one, and this module has no filesystem.
    pub fn document(&self, template: &Template) -> Result<CollageDoc, SelectionError> {
        let photos = self.photos.len();
        if !(MIN_PHOTOS..=MAX_PHOTOS).contains(&photos) {
            return Err(SelectionError::PhotoCount {
                found: photos,
                min: MIN_PHOTOS,
                max: MAX_PHOTOS,
            });
        }
        let slots = template.slots.len();
        if slots != photos {
            return Err(SelectionError::SlotCount {
                template: template.name.clone(),
                slots,
                photos,
            });
        }
        let mut doc = templates::document(template);
        for (cell, photo) in doc.cells.iter_mut().zip(&self.photos) {
            cell.source = Some(photo.clone());
        }
        Ok(doc)
    }
}

/// The last cell that holds a photo, if any.
///
/// A batch control has to know whether there is anything to drop before it acts,
/// and asking by removing would change the document to answer a question.
pub fn last_photo(doc: &CollageDoc) -> Option<usize> {
    doc.cells.iter().rposition(|cell| cell.source.is_some())
}

/// The layout to switch to when the photo count changes (S14).
///
/// The count and the layout move together: `−` on the count control leaves a
/// document whose template has one cell too many, and `+` needs one back. This is
/// the one rule that answers "which layout, when there is no longer an obvious
/// one", and it is pure, so the GUI's count control and the CLI's
/// `edit --add-photo` / `--remove-photo` cannot disagree about it.
///
/// The preference, in the order it decides:
///
/// 1. **the same aspect** as `aspect`, within [`ASPECT_TOLERANCE`] — the sheet
///    keeps its shape, which is the thing the user has been looking at;
/// 2. **the same recipe family** as `family` (a strip stays a strip where the
///    library has one at that count);
/// 3. **the nearest aspect**, by absolute difference;
/// 4. **library order** — `templates::all`'s canonical order, so the answer is
///    deterministic where two candidates tie on everything above.
///
/// `None` when no template has `count` slots, which is every count outside
/// `MIN_PHOTOS..=MAX_PHOTOS` (S10 ships at least three layouts for every count in
/// that range, and S12c capped the format at the same nine).
pub fn layout_for(count: usize, aspect: f64, family: Option<Family>) -> Option<Template> {
    // Strictly-less comparison, so a tie on all three keys keeps the first
    // candidate the library offered — which is `templates::all`'s canonical order,
    // the fourth preference.
    let mut best: Option<(u8, u8, f64, Template)> = None;
    for template in templates::all()
        .into_iter()
        .filter(|template| template.slots.len() == count)
    {
        let key = (
            u8::from((template.aspect - aspect).abs() > ASPECT_TOLERANCE),
            u8::from(family.is_some_and(|family| template.family() != Some(family))),
            (template.aspect - aspect).abs(),
        );
        let closer = match &best {
            Some((aspect_rank, family_rank, distance, _)) => {
                (key.0, key.1, key.2) < (*aspect_rank, *family_rank, *distance)
            }
            None => true,
        };
        if closer {
            best = Some((key.0, key.1, key.2, template));
        }
    }
    best.map(|(_, _, _, template)| template)
}

/// A cell a batch removal cleared, kept whole so it can come back unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct Removed {
    /// The cell index it was taken from.
    pub slot: usize,
    /// The cell as it was: photo and framing.
    pub cell: Cell,
    /// The layout the document was on when the cell was taken.
    ///
    /// Ruling 7's control "drops the last photo and brings it back", and the count
    /// moves the layout with it (`layout_for`): pressing `−` on a four-photo
    /// `grid-4-2x2` leaves a three-slot document, and no three-slot *grid* exists
    /// for the count rule to grow back into — the nearest aspect would return a
    /// strip. So the token carries the layout too, and [`Removed::restore`] puts it
    /// back when the document can no longer host the cell. A layout the user chose
    /// *while the cell was out* is kept when it can host it (the document has the
    /// slot), because that pick is newer than this token.
    pub template: Template,
}

impl Removed {
    /// Puts the cell back where it came from (LIFO, the other half of
    /// [`remove_last`]).
    ///
    /// Refused when the cell has been taken since — restoring would overwrite
    /// whatever is there now — or when the document no longer has that cell. The
    /// caller then has to decide; guessing which photo to lose is not this
    /// function's call.
    pub fn restore(self, doc: &mut CollageDoc) -> Result<(), CoreError> {
        // A document that shrank past the cell's own index has to grow back first,
        // and only a layout can do that: this is the document the cell was taken
        // from, so its layout comes back with it.
        if self.slot >= doc.cells.len() {
            let slots = self.template.slots.len();
            doc.template = self.template;
            doc.cells.resize(slots, Cell::default());
        }
        let slots = doc.cells.len();
        let cell = doc.cells.get_mut(self.slot).ok_or(CoreError::NoSuchSlot {
            slot: self.slot,
            slots,
        })?;
        if cell.source.is_some() {
            return Err(CoreError::SlotOccupied { slot: self.slot });
        }
        *cell = self.cell;
        Ok(())
    }
}

/// Clears the last occupied cell and returns what it held.
///
/// Nothing else moves: the other cells keep their photo and framing, and the
/// removed cell's own framing travels out with it, so a later
/// [`Removed::restore`] is the exact inverse rather than a re-placement with
/// defaults.
pub fn remove_last(doc: &mut CollageDoc) -> Option<Removed> {
    let slot = last_photo(doc)?;
    let cell = std::mem::take(&mut doc.cells[slot]);
    Some(Removed {
        slot,
        cell,
        template: doc.template.clone(),
    })
}
