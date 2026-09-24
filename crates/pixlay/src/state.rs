//! The document as the window holds it: a command history, where it came from,
//! and the gesture that is in flight.
//!
//! `pixlay-core` owns the document, the commands and the two undo stacks
//! (S6.5); this module adds what only a window has — a project path, the document
//! as the file on disk holds it (so that "unsaved" is a difference rather than a
//! flag, PIX-022), resolving a cell's `source` against the project directory, and
//! **one gesture in flight**.
//!
//! The gesture is the interesting part. "One gesture = one command, committed
//! when the drag ends" (S6.5) means a drag may not push forty commands, so while
//! a drag is happening the command is kept *pending* here: [`Editor::begin`]
//! applies it to a copy for the canvas to draw, [`Editor::commit`] turns it into
//! the single undo step the user expects, and [`Editor::cancel`] drops it. The
//! history never sees a state the user did not finish making.
//!
//! **A boundary commits the pending edit before it does anything else** (S15d,
//! PIX-002's ruling of 2026-09-24): save, close, New, Open and export all mean "the
//! document as it is on screen", so [`Editor::save`] and the window's own
//! boundaries commit first and only then ask whether there is unsaved work.
//!
//! Nothing here touches GTK, and nothing here decodes anything.

use std::path::{Path, PathBuf};

use pixlay_core::{CollageDoc, Command, CoreError, History, Project};

/// What a slot's `source` resolves to, and which slots did not resolve.
///
/// A missing file is not an error here the way it is for `Project::sources`: the
/// user is looking at the collage, and a photo that moved has to be visible as a
/// white slot plus a banner that offers to fix it, not as a window that fails to
/// open.
pub struct Sources {
    pub paths: Vec<Option<PathBuf>>,
    pub missing: Vec<usize>,
}

pub struct Editor {
    history: History,
    /// The project file, or `None` for a document that has never been saved.
    path: Option<PathBuf>,
    /// What a relative `source` resolves against: the project's directory, or the
    /// current directory for an unsaved document.
    dir: PathBuf,
    /// The document as the file on disk holds it, or as the window started: the
    /// baseline [`Editor::is_dirty`] is a difference from.
    ///
    /// The whole document rather than a flag (PIX-022, 2026-09-24): "the document
    /// differs from the file" is a comparison, and only the saved document can
    /// answer it — a save, an edit and an undo back to the saved state is not an
    /// unsaved edit, and a boolean that is set by every command cannot say so.
    saved: CollageDoc,
    /// The command a gesture is currently making, if any.
    pending: Option<Command>,
}

impl Editor {
    pub fn new(doc: CollageDoc) -> Result<Self, CoreError> {
        Ok(Self {
            history: History::new(doc.clone())?,
            path: None,
            dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            saved: doc,
            pending: None,
        })
    }

    pub fn from_project(project: Project) -> Result<Self, CoreError> {
        Ok(Self {
            history: History::new(project.doc().clone())?,
            path: Some(project.path().to_path_buf()),
            dir: project.dir().to_path_buf(),
            saved: project.doc().clone(),
            pending: None,
        })
    }

    pub fn doc(&self) -> &CollageDoc {
        self.history.doc()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether the document differs from the file on disk.
    ///
    /// A comparison, not a flag (PIX-022, 2026-09-24). "Dirty" has to mean "there is
    /// work here that the file does not have", and a boolean set by every command
    /// gets three cases wrong: a command that changed nothing, a save followed by an
    /// edit and an undo back to the saved state, and a save — the document is the
    /// file's again. A document that has never been saved is compared against the
    /// document the window started with, so an untouched one is nothing to ask
    /// about and the first edit is.
    pub fn is_dirty(&self) -> bool {
        self.history.doc() != &self.saved
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Applies one command as one undo step. A refused command changes nothing,
    /// including the pending gesture (it is cleared, because the caller is about
    /// to draw the document the history holds). A command that changes nothing is
    /// not a step either — the history's own rule (PIX-022).
    pub fn apply(&mut self, command: Command) -> Result<(), CoreError> {
        self.pending = None;
        self.history.apply(command)?;
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        self.pending = None;
        self.history.undo()
    }

    pub fn redo(&mut self) -> bool {
        self.pending = None;
        self.history.redo()
    }

    /// Replaces the pending gesture with `command` and returns the document the
    /// canvas should draw while the gesture lasts.
    ///
    /// Replacing rather than appending is what makes a drag cheap: every motion
    /// event produces the *whole* command from the state the gesture started in,
    /// so the preview is always the current pointer position and never a sum of
    /// deltas. The command is validated, so a preview cannot show something the
    /// history would refuse.
    pub fn begin(&mut self, command: Command) -> Result<CollageDoc, CoreError> {
        let preview = command.applied_to(self.history.doc())?;
        self.pending = Some(command);
        Ok(preview)
    }

    /// Turns the pending gesture into one undo step. `false` when there was
    /// nothing pending, when the history refused it, or when the gesture ended on
    /// the value it started from.
    ///
    /// The last case is the history's own rule now (PIX-022, 2026-09-24): a wheel
    /// or a slider can land on the value it began with (a rotation already at its
    /// limit, a drag that came back), and an undo step that changes nothing is a
    /// step the user has to press `Ctrl+Z` through for no reason — S6.5's own walk
    /// asserts that every command in a sequence changes the document.
    pub fn commit(&mut self) -> bool {
        let Some(command) = self.pending.take() else {
            return false;
        };
        // A refusal cannot happen for a command `begin` accepted — the document it
        // was applied to has not changed since — so the error is the same "no step"
        // answer the equality case gets.
        self.history.apply(command).unwrap_or(false)
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Whether a gesture is in flight.
    ///
    /// The canvas asks this to decide which grid it draws at: while a gesture is
    /// live the document is moving, so the frames are coarse and the release is
    /// refined (S12). It is a question about the *pending* command and nothing
    /// else — a committed edit is not a gesture.
    pub fn gesture_live(&self) -> bool {
        self.pending.is_some()
    }

    /// The document the canvas draws: the committed one, plus the gesture in
    /// flight when there is one.
    pub fn display_doc(&self) -> CollageDoc {
        match &self.pending {
            Some(command) => command
                .applied_to(self.history.doc())
                .unwrap_or_else(|_| self.history.doc().clone()),
            None => self.history.doc().clone(),
        }
    }

    /// Each cell's resolved source, plus the slots whose file is gone.
    pub fn sources(&self) -> Sources {
        let mut paths = Vec::with_capacity(self.doc().cells.len());
        let mut missing = Vec::new();
        for (slot, cell) in self.doc().cells.iter().enumerate() {
            match &cell.source {
                None => paths.push(None),
                Some(source) => {
                    let resolved = if source.is_absolute() {
                        source.clone()
                    } else {
                        self.dir.join(source)
                    };
                    if resolved.is_file() {
                        paths.push(Some(resolved));
                    } else {
                        missing.push(slot);
                        paths.push(None);
                    }
                }
            }
        }
        Sources { paths, missing }
    }

    /// Writes the document to the file it came from, or to `path` when it has
    /// none; returns the path that was written.
    ///
    /// **A boundary commits the pending edit first** (PIX-002's ruling,
    /// 2026-09-24): what the canvas is showing is what the file has to hold, so a
    /// frame or crop change that is still inside its quiet interval is committed
    /// here rather than dropped.
    ///
    /// A document that has never been saved needs a path from the caller (the
    /// GUI's save dialog), and one that has a path is written through the same
    /// atomic write the CLI uses. A save into another directory rebases every
    /// relative source, which is what `Project::save_as` does and why this goes
    /// through `Project` instead of `CollageDoc::save` — and the document that
    /// comes back is the one the file holds, so the window adopts it: the history's
    /// own states are rebased the same way, and an undo back into the old spelling
    /// cannot resolve the photos against the directory they were moved away from
    /// (PIX-005, PIX-006).
    pub fn save(&mut self, path: Option<&Path>) -> Result<PathBuf, CoreError> {
        self.commit();
        let target = match (path, &self.path) {
            (Some(path), _) => path.to_path_buf(),
            (None, Some(existing)) => existing.clone(),
            (None, None) => {
                return Err(CoreError::Io {
                    path: PathBuf::from("<unsaved>"),
                    source: std::io::Error::other("this document has no path yet"),
                });
            }
        };
        let anchor = match &self.path {
            Some(existing) => existing.clone(),
            // An unsaved document has no directory of its own, so nothing needs
            // rebasing: its sources are already absolute or are relative to the
            // process's working directory.
            None => target.clone(),
        };
        let project = Project::new(self.doc().clone(), anchor)?;
        let written = project.save_as(&target)?;
        self.history.rebase(project.dir(), written.dir());
        self.path = Some(written.path().to_path_buf());
        self.dir = written.dir().to_path_buf();
        self.saved = self.history.doc().clone();
        Ok(written.path().to_path_buf())
    }
}
