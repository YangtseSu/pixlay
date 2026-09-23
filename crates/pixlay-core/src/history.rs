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
use crate::frame::Frame;
use crate::template::Template;

/// One edit to a document.
///
/// Deliberately small: it covers what a user changes — which photo a slot shows,
/// how it is framed, the template, the canvas frame. It is not a serialization
/// format (nothing writes a command to disk, and no version tracks it), and it is
/// not an editing language: a command does one thing, and the GUI sends a sequence
/// of them.
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
    /// Drop the last cell, and take the layout with one slot fewer (S14b).
    ///
    /// The count control's `−`, and [`AddCell`](Self::AddCell)'s exact inverse: the
    /// control addresses the *layout*, so `+` appends an empty cell and `−` removes
    /// the last cell whatever it holds. A photo that was in it goes with the cell —
    /// that is what "one cell fewer" means — and `Ctrl+Z` is what brings it back.
    ///
    /// Never below [`MIN_SLOTS`](crate::MIN_SLOTS) slots: a collage's layout has at
    /// least two cells, and `layout_for` has no answer below that.
    RemoveLastCell,
    /// Take the layout with one slot more, and leave the new cell empty
    /// (S14b).
    ///
    /// The count control's `+`: the count and the layout move together, so "add a
    /// photo" is "switch to the layout of the next count" and the new cell is
    /// empty until a photo lands in it. Retention is
    /// [`SetTemplate`](Self::SetTemplate)'s — every existing cell keeps its photo
    /// and framing — and the layout is [`selection::layout_for`]'s answer for the
    /// new count, so the GUI's `+` and the CLI's `edit --add-cell` cannot disagree
    /// about which layout the document grows into.
    ///
    /// Refused past [`MAX_PHOTOS`](crate::MAX_PHOTOS): that is also the format's
    /// slot limit, so there is no layout to grow into.
    AddCell,
    /// Exchange two cells whole — photo *and* framing (S14b).
    ///
    /// The cell moves, not the photo: the framing is what makes a photo look right
    /// in *that* cell, so swapping the two sources and leaving the crops behind
    /// would reframe both pictures as a side effect of wanting them in each other's
    /// place. One command, one undo step, and `left == right` is refused rather
    /// than silently accepted: an edit that changes nothing is a step the user has
    /// to press `Ctrl+Z` through.
    SwapCells { left: usize, right: usize },
    /// Empty one cell: no photo, and its framing back to the default (S15).
    ///
    /// **One intent, so one command and one undo step.** "Clear this cell" is not two
    /// edits the user makes in sequence — it is the cell returning to what a fresh
    /// cell is — and a surface that sent `SetSource { None }` and then
    /// `SetCrop { IDENTITY }` would leave the user pressing `Ctrl+Z` twice for one
    /// press. The CLI's `edit --slot i --clear` has written both halves since S7
    /// (through two commands, because it keeps no undo stack); S15 made the window's
    /// own clear mean the same thing, and this is the shape that lets both be one
    /// step.
    ///
    /// It is deliberately *not* what [`Command::SetSource`]'s `None` does: that keeps
    /// the framing, which is what makes replacing a photo keep the area the user
    /// framed (`docs/CONTRACT.md` §1). Clearing is the other half of that rule: the
    /// cell starts over.
    ClearCell { slot: usize },
    /// Replace the document's canvas frame (S15).
    ///
    /// The frame is a document field, and since S15 this is its only writer: the
    /// `Frame…` dialog's three rows and the CLI's `edit --gap/--radius/
    /// --border-color` both commit it here, so setting the frame is one undo step
    /// from either surface and the CLI cannot write a frame the window would refuse.
    /// It is *not* a relayout — the geometry the frame clips against is the
    /// template's — which is what makes one whole `Frame` the unit rather than a
    /// field at a time.
    ///
    /// Refused by validation like any other command: a length outside
    /// `0..=MAX_FRAME_REL`, a translucent colour, or a gap that leaves a cell with
    /// nothing visible names the slot it empties.
    SetFrame { frame: Frame },
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
            Self::RemoveLastCell => {
                let slots = doc.cells.len();
                if slots <= crate::MIN_SLOTS {
                    return Err(CoreError::TooFewCells {
                        min: crate::MIN_SLOTS,
                    });
                }
                set_template(doc, layout_with(doc, slots - 1)?);
            }
            Self::AddCell => {
                let slots = doc.cells.len();
                if slots >= crate::MAX_SLOTS {
                    return Err(CoreError::TooManyCells {
                        max: crate::MAX_SLOTS,
                    });
                }
                set_template(doc, layout_with(doc, slots + 1)?);
            }
            Self::SwapCells { left, right } => {
                if left == right {
                    return Err(CoreError::SameSlot { slot: *left });
                }
                let cells = doc.cells.len();
                if *left >= cells {
                    return Err(CoreError::NoSuchSlot {
                        slot: *left,
                        slots: cells,
                    });
                }
                if *right >= cells {
                    return Err(CoreError::NoSuchSlot {
                        slot: *right,
                        slots: cells,
                    });
                }
                doc.cells.swap(*left, *right);
            }
            Self::ClearCell { slot } => {
                let cell = cell_mut(doc, *slot)?;
                cell.source = None;
                cell.crop = CropTransform::IDENTITY;
            }
            Self::SetFrame { frame } => doc.frame = *frame,
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
