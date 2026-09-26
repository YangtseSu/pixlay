//! The selection policy: which photos a collage holds, and in which order.
//!
//! The list of photos in the order the user gave them is a mapping onto cells, and
//! **order is cell order**: the document's own order is where that mapping is
//! visible and re-orderable, not decoration. This module is the mapping itself,
//! and it is pure — no cairo, no GTK, no filesystem — so the window, the
//! layout stage (S14) and the CLI's `init --photo` share one implementation
//! instead of three that agree by luck.
//!
//! Three rules live here:
//!
//! * **the floor and the ceiling.** A collage needs `1..=9` photos: one since S19
//!   (ruling 34 — a single photo is a legal collage), nine since S12c. The floor is
//!   enforced where a selection becomes a document ([`Selection::document`]) and the
//!   ceiling where a photo is added ([`Selection::push`]), because those are the two
//!   moments a user can hit them: the picked list legitimately holds no photo while
//!   the user is still picking, and a tenth is refused with a message rather than
//!   truncated — the *caller* may trim a longer list, and the window does, but core
//!   never drops input on its own.
//! * **the count filter.** The layouts a selection can use are the library's
//!   templates with exactly that many slots ([`Selection::layouts`]) — the layout
//!   stage offers a layout exactly when it has as many slots as the user picked
//!   photos, and since S12c the library itself stops at nine, so every layout is
//!   offered for some count.
//! * **the count rule.** When the photo count changes, the layout changes with it:
//!   [`layout_for`] is the one answer to "which layout, when the choice is not
//!   obvious" (same aspect, then same recipe family, then the nearest aspect, then
//!   library order), so the composition's count control and the CLI's
//!   `edit --add-cell` / `--remove-photo` cannot disagree about it (S14).
//!
//! What is *not* here any more: S14's batch control dropped the last photo and
//! brought it back by remembering the cell it took (LIFO). S14b removed the
//! add-back with the ruling that `+` switches the layout instead of restoring a
//! photo, so the token, its carried template and [`Selection::pop`] are gone —
//! [`remove_last`] clears a cell and says which, and nothing has to keep it.

use std::path::PathBuf;

use thiserror::Error;

use crate::ASPECT_TOLERANCE;
use crate::doc::{Cell, CollageDoc};
use crate::template::{Family, Template};
use crate::templates;

/// Fewest photos a collage can be made of.
///
/// One since S19: a single photo is a legal collage (ruling 34) — the library's
/// one-slot sheet gives it a layout, and the frame is what gives it a border.
/// The floor was 2 for as long as a "collage" was assumed to be several photos.
pub const MIN_PHOTOS: usize = 1;

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

    /// Whether another photo fits under the ceiling. The count control's `+`
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

    /// Drops the photo at `index` (the window's per-cell clear). `None` past the
    /// end, so a stale index is a no-op rather than a panic.
    pub fn remove(&mut self, index: usize) -> Option<PathBuf> {
        (index < self.photos.len()).then(|| self.photos.remove(index))
    }

    /// The layouts this selection can use: every template with exactly this many
    /// slots, in library order.
    ///
    /// An empty selection asks for nothing and gets nothing — there is no
    /// zero-slot layout to offer, and a caller that wants the whole library asks
    /// `templates::all` itself. This is [`templates::with_slots`]; the layout stage
    /// asks the same function about the *document's* cell count instead, which is
    /// the count its strip follows (S14b: `+` can leave a cell empty, so the two
    /// numbers are no longer always equal).
    pub fn layouts(&self) -> Vec<Template> {
        templates::with_slots(self.photos.len())
    }

    /// The document these photos make on `template`, in cell order.
    ///
    /// This is the one place a selection's photos become a document, so the CLI's
    /// `init --photo` and the window cannot disagree about what "the third photo"
    /// means. Paths are stored exactly as given: making them relative to
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
/// `edit --add-cell` / `--remove-photo` cannot disagree about it.
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

/// Clears the last occupied cell and says which one it was.
///
/// Nothing else moves: the other cells keep their photo and framing, and the
/// cleared cell comes out whole — no photo *and* no framing, which is what
/// [`Cell::default`] is. `None` when every cell is already empty, which is the
/// query "is there anything to drop" answered without changing a document to find
/// out.
///
/// The returned index is the one thing a batch control used to need beyond the
/// edit (S14 kept the cell in a token so `+` could put it back); S14b's `+` switches
/// the layout instead, and since S28 the *document* is what keeps a cell a layout
/// change takes away — this function clears one and keeps nothing, which is the
/// "delete" half of that rule.
pub fn remove_last(doc: &mut CollageDoc) -> Option<usize> {
    let slot = last_photo(doc)?;
    doc.cells[slot] = Cell::default();
    Some(slot)
}
