//! Command history: the document only ever changes through a named command.
//!
//! S6.5 (`docs/CONTRACT.md` §7). Two of the contract's decisions — one gesture is one
//! command (§9) and a snapshot is a whole document (§7) — shape this module, and both
//! are structural rather than comments:
//!
//! * **One gesture = one command, committed when the drag ends.** A command is a
//!   whole edit, not a sample of one: the GUI applies it once, when the user lets
//!   go, so there is nothing to coalesce and no "is this the same gesture?" state
//!   to keep.
//! * **A snapshot stores the whole [`CollageDoc`], with no diff.** The undo stack
//!   therefore holds documents, not operations, and undo cannot fail to
//!   reconstruct a state: it *is* the old state. Commands still exist — the GUI
//!   needs a vocabulary to name an edit, which is also what makes the edit
//!   testable without a window.
//!
//! The two halves of that are in [`Command::apply`] and [`History::apply`]: a
//! command writes into a *copy* of the document, and the copy is validated before
//! it becomes current. A command that would leave the document outside the
//! contract — a zoom past `MAX_ZOOM`, a slot the template does not have — is
//! refused and changes nothing, which is the same all-or-nothing rule loading a
//! file follows.

use std::path::PathBuf;

use crate::crop::CropTransform;
use crate::doc::CollageDoc;
use crate::error::CoreError;
use crate::selection::Removed;
use crate::template::Template;

/// One edit to a document.
///
/// Deliberately small: it covers what a user changes — which photo a slot shows,
/// how it is framed, the template. It is not a serialization format (nothing
/// writes a command to disk, and no version tracks it), and it is not an editing
/// language: a command does one thing, and the GUI sends a sequence of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Point a cell at a photo, or empty it (`None`), which renders the slot
    /// white.
    ///
    /// The framing is *not* reset. `CropTransform::zoom` is absolute — displayed
    /// width over slot width — exactly so that swapping the photo does not move
    /// the area the user framed (docs/CONTRACT.md §1).
    SetSource {
        slot: usize,
        source: Option<PathBuf>,
    },
    /// Replace one cell's framing. The request is stored as asked — with the one
    /// normalization a dial needs: a finite `rotation_deg` is wrapped into
    /// `(-180, 180]`, so a long spin cannot accumulate turns in the document. What
    /// gets drawn is the request's fit (`CropTransform::fit`), which `draw`
    /// recomputes, so a request that would leave the cell uncovered is still a legal
    /// document.
    SetCrop { slot: usize, crop: CropTransform },
    /// Replace the document's template geometry (S7).
    ///
    /// A template change is not a relayout of the same document: it changes the
    /// slot count, so it resizes `cells`. There is no second half to carry since
    /// S12d: the sheet's shape is the template's own aspect.
    ///
    /// Retention: the first `min(old, new)` cells keep their photos and framing —
    /// a template with more slots appends empty ones, a smaller one drops the
    /// tail — and the whole command is one undo step.
    ///
    /// S6.5 deliberately had no such command ("choosing a template is how a
    /// document starts"); S7's template picker is what it is for, because a user
    /// who has placed photos must be able to try another layout without starting
    /// over.
    SetTemplate { template: Template },
    /// Put photos into the document (S14).
    ///
    /// Each photo goes to the first empty cell, and one that finds none takes the
    /// layout with one slot more (`selection::layout_for`), so the count control's
    /// `+` never has to choose between "fill" and "grow" — it adds, and the
    /// document follows. Retention is [`SetTemplate`](Self::SetTemplate)'s: the
    /// cells that survive keep their photo and framing. One call is one undo step,
    /// whatever it did.
    ///
    /// Refused past [`MAX_PHOTOS`](crate::MAX_PHOTOS): that is also the format's
    /// slot limit, so there is no layout to grow into.
    AddPhotos { photos: Vec<PathBuf> },
    /// Drop the last *occupied* cell and shrink the layout to what the survivors
    /// need (S14).
    ///
    /// "The last photo" is the last occupied cell rather than the last cell, because
    /// a single cell can be cleared on its own (ruling 7). The layout then shrinks
    /// to the smallest one that still gives every survivor its own cell — never
    /// below [`MIN_PHOTOS`](crate::MIN_PHOTOS) slots — through
    /// [`selection::layout_for`], the same rule `+` grows by.
    RemoveLastPhoto,
    /// Put a removed cell back, with the framing it was taken with (S14: the LIFO
    /// half of ruling 7's batch control).
    ///
    /// The layout grows back first when the document has since shrunk past the
    /// cell's own index, and [`Removed::restore`] then refuses if something else has
    /// taken the slot in the meantime.
    RestorePhoto(Removed),
}

impl Command {
    /// The document this command would produce, without touching a [`History`].
    ///
    /// The GUI draws a gesture while it is happening (S7): a drag sends the whole
    /// command on every motion event, and what is on screen has to be its result
    /// without the history recording forty states. Written as "apply to a copy,
    /// then validate", which is the same rule [`History::apply`] enforces, so a
    /// preview cannot show a document the history would refuse.
    pub fn applied_to(&self, doc: &CollageDoc) -> Result<CollageDoc, CoreError> {
        let mut next = doc.clone();
        self.apply(&mut next)?;
        next.validate()?;
        Ok(next)
    }

    /// Applies the command to `doc`, which is left *unvalidated*.
    ///
    /// Private on purpose: [`History::apply`] is the validating entry point, and
    /// an unvalidated document must not be something a caller can produce by
    /// accident. Only the index checks that no `validate` call could make live
    /// here — those are about the command's own references.
    fn apply(&self, doc: &mut CollageDoc) -> Result<(), CoreError> {
        match self {
            Self::SetSource { slot, source } => {
                cell_mut(doc, *slot)?.source = source.clone();
            }
            Self::SetCrop { slot, crop } => {
                cell_mut(doc, *slot)?.crop = crop.normalized();
            }
            Self::SetTemplate { template } => {
                set_template(doc, template.clone());
            }
            Self::AddPhotos { photos } => {
                for photo in photos {
                    match doc.cells.iter().position(|cell| cell.source.is_none()) {
                        Some(slot) => doc.cells[slot].source = Some(photo.clone()),
                        None => {
                            let slots = doc.cells.len();
                            if slots >= crate::MAX_PHOTOS {
                                return Err(CoreError::TooManyPhotos {
                                    max: crate::MAX_PHOTOS,
                                });
                            }
                            set_template(doc, layout_with(doc, slots + 1)?);
                            let appended = doc.cells.len() - 1;
                            doc.cells[appended].source = Some(photo.clone());
                        }
                    }
                }
            }
            Self::RemoveLastPhoto => {
                crate::selection::remove_last(doc).ok_or(CoreError::NothingToRemove)?;
                // What the survivors need: one cell per slot up to the last one that
                // still holds a photo, and never below the floor.
                let kept = crate::selection::last_photo(doc).map_or(0, |slot| slot + 1);
                let target = kept.clamp(crate::MIN_PHOTOS, doc.cells.len());
                if target < doc.cells.len() {
                    set_template(doc, layout_with(doc, target)?);
                }
            }
            Self::RestorePhoto(removed) => {
                // The token carries the layout as well as the cell: a document that
                // shrank past the cell's own index grows back to the layout the cell
                // was taken from, which is the only way "brings it back" can be
                // exact (no layout may exist at the intermediate count with the
                // document's own family and aspect).
                removed.clone().restore(doc)?;
            }
        }
        Ok(())
    }
}

/// Replaces the document's template, keeping the cells that still exist.
///
/// `SetTemplate`'s retention rule, factored out so that the batch commands and the
/// command the layout gallery sends cannot drift: one cell per slot, in template
/// order, with the states the surviving cells already had.
fn set_template(doc: &mut CollageDoc, template: Template) {
    doc.template = template;
    let slots = doc.template.slots.len();
    doc.cells.resize(slots, crate::Cell::default());
}

/// The layout with `count` slots that follows this document's own shape.
///
/// `selection::layout_for` decides which one (same aspect, then same recipe family,
/// then the nearest aspect, then library order), and the refusal names the count
/// when the library has no layout for it.
fn layout_with(doc: &CollageDoc, count: usize) -> Result<Template, CoreError> {
    crate::selection::layout_for(count, doc.template.aspect, doc.template.family()).ok_or(
        CoreError::SlotCount {
            found: count,
            min: crate::MIN_SLOTS,
            max: crate::MAX_SLOTS,
        },
    )
}

fn cell_mut(doc: &mut CollageDoc, slot: usize) -> Result<&mut crate::Cell, CoreError> {
    let slots = doc.cells.len();
    doc.cells
        .get_mut(slot)
        .ok_or(CoreError::NoSuchSlot { slot, slots })
}

/// A document and the two stacks of states it has been in.
///
/// The pointers are the stacks themselves: `undo` holds the states before each
/// applied command (`undo_depth()` of them, most recent last) and `redo` the ones
/// an undo left behind. Nothing invalidates either stack, so any sequence of
/// commands, undos and redos lands on a state the document really had.
#[derive(Clone, Debug)]
pub struct History {
    doc: CollageDoc,
    undo: Vec<CollageDoc>,
    redo: Vec<CollageDoc>,
}

impl History {
    /// A history over `doc`, which has to be a valid document already.
    pub fn new(doc: CollageDoc) -> Result<Self, CoreError> {
        doc.validate()?;
        Ok(Self {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// The current document. There is no mutable accessor: a command is the only
    /// way in, which is what keeps every state in the stacks a state the document
    /// actually had.
    pub fn doc(&self) -> &CollageDoc {
        &self.doc
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// How many commands can be undone (`0` at the initial state).
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// How many undone commands can be redone.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Applies one command as one undoable step.
    ///
    /// All-or-nothing: the command is applied to a copy, the copy is validated,
    /// and only then does it become the current document and the old one becomes
    /// the top of the undo stack. An error therefore leaves the document and both
    /// stacks exactly as they were.
    pub fn apply(&mut self, command: Command) -> Result<(), CoreError> {
        let next = command.applied_to(&self.doc)?;
        self.undo.push(std::mem::replace(&mut self.doc, next));
        // The redo stack is a path, and applying a command after an undo forks
        // it: what was undone is no longer reachable from here.
        self.redo.clear();
        Ok(())
    }

    /// Steps back one command; `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(previous) => {
                self.redo.push(std::mem::replace(&mut self.doc, previous));
                true
            }
            None => false,
        }
    }

    /// Steps forward one command; `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.doc, next));
                true
            }
            None => false,
        }
    }
}
