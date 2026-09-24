//! The document itself, and the project file around it.

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::crop::{CropFit, CropTransform};
use crate::error::CoreError;
use crate::frame::Frame;
use crate::template::Template;
use crate::{DOC_VERSION, DOC_VERSION_MIN};

/// What one slot shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cell {
    /// Source image as a path relative to the project file. `None` is an empty
    /// cell: the slot stays white. Absolute paths are accepted as they are.
    #[serde(default)]
    pub source: Option<PathBuf>,
    #[serde(default)]
    pub crop: CropTransform,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            source: None,
            crop: CropTransform::IDENTITY,
        }
    }
}

/// The whole document: the only shape that is ever written to a `.pixlay`.
///
/// The template geometry is embedded rather than referenced by name, so editing
/// the template library cannot change the layout of an existing project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollageDoc {
    /// Format version of this document. A file from a newer version is rejected.
    pub doc_version: u32,
    pub template: Template,
    /// One cell per slot, in template order.
    pub cells: Vec<Cell>,
    /// The canvas frame (S11): the gap between cells, their corner radius, and the
    /// colour the canvas is painted with where no photo covers it.
    ///
    /// No gap, square corners and white are the defaults, so a project written
    /// before this field existed renders byte-identically and `DOC_VERSION` did
    /// not move for it.
    #[serde(default)]
    pub frame: Frame,
}

impl CollageDoc {
    /// A document with the slot count of `template` and every cell empty.
    ///
    /// The document carries no size (S12d): the template's aspect is the sheet's
    /// shape, and how many pixels a render or export puts on it is a parameter of
    /// the render, never of the document.
    pub fn new(template: Template) -> Self {
        let cells = vec![Cell::default(); template.slots.len()];
        Self {
            doc_version: DOC_VERSION,
            template,
            cells,
            frame: Frame::default(),
        }
    }

    /// Checks everything the model promises, and reports the first violation.
    ///
    /// Loaders call this; the renderer trusts a validated document.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.doc_version > DOC_VERSION {
            return Err(CoreError::VersionTooNew {
                found: self.doc_version,
                supported: DOC_VERSION,
            });
        }
        // Read at exactly one version: adding a field does not bump DOC_VERSION,
        // so an older version here means a breaking change happened and this
        // document cannot be interpreted. There is no migration by decision.
        if self.doc_version < DOC_VERSION_MIN {
            return Err(CoreError::VersionUnsupported {
                found: self.doc_version,
                supported: DOC_VERSION,
            });
        }
        // The template's aspect is the sheet's shape (S12d removed the canvas that
        // used to have to agree with it): the normalized geometry is stretched onto
        // the render grid, whose long edge is the one parameter a caller gives.
        let canvas_aspect = self.template.aspect;
        self.template.validate()?;
        if self.cells.len() != self.template.slots.len() {
            return Err(CoreError::CellCount {
                cells: self.cells.len(),
                slots: self.template.slots.len(),
            });
        }
        // The frame is checked against the geometry it decorates, slot by slot: a
        // gap that leaves a cell with nothing visible is not a document this build
        // can render, and the error has to name which cell. The check is the same
        // `Frame::covering` the clamp takes its reference from, so "validates" and
        // "has a region to cover" cannot mean two different things.
        self.frame.validate()?;
        for (slot, geometry) in self.template.slots.iter().enumerate() {
            if self.frame.covering(geometry, canvas_aspect).is_none() {
                return Err(CoreError::InvalidSlot {
                    slot,
                    reason: "the frame's gap leaves this slot with no visible area",
                });
            }
        }
        for cell in &self.cells {
            cell.crop.validate()?;
        }
        Ok(())
    }

    /// Reads a document, wrapping every cell's rotation into `(-180, 180]` on the
    /// way in.
    ///
    /// Normalizing here rather than in `validate` is what makes it invisible to a
    /// caller: a file may say any finite angle, and what this build holds — and
    /// writes back — is the equivalent angle inside the range. Every angle the old
    /// ±45° cap allowed is already inside it, so a project written before
    /// 2026-09-22 loads unchanged.
    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        // The version is read on its own first. `deny_unknown_fields` means a
        // document from another format hits "unknown field" while it is being
        // parsed — before `validate` ever sees it — so the message a user got would
        // name a key instead of telling them the file is from another version.
        // Since S12c that is the normal case for a version-1 project (it carries
        // `grade`, `filter`, `text` and `textFallback`), and the version policy
        // promises an actionable refusal rather than a field error.
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct VersionProbe {
            doc_version: u32,
        }
        let probe: VersionProbe = serde_json::from_str(json)?;
        if probe.doc_version > DOC_VERSION {
            return Err(CoreError::VersionTooNew {
                found: probe.doc_version,
                supported: DOC_VERSION,
            });
        }
        if probe.doc_version < DOC_VERSION_MIN {
            return Err(CoreError::VersionUnsupported {
                found: probe.doc_version,
                supported: DOC_VERSION,
            });
        }

        let mut doc: Self = serde_json::from_str(json)?;
        doc.normalize();
        doc.validate()?;
        Ok(doc)
    }

    /// Wraps every cell's rotation into `(-180, 180]` (`CropTransform::normalized`).
    ///
    /// Idempotent, and a cell whose rotation is already inside the range is left
    /// bit-identical — which is what leaves the JSON of an old project unchanged.
    pub fn normalize(&mut self) {
        for cell in &mut self.cells {
            cell.crop = cell.crop.normalized();
        }
    }

    /// The framing a cell is drawn with: the stored request fitted to the region
    /// the document's frame leaves visible in that slot.
    ///
    /// This is the one place the frame and the clamp meet, so a caller cannot
    /// forget one of the two: `render`, `pixlay-imaging`'s bitmaps and the GUI all
    /// ask this question, and all of them get the same answer about the same cell.
    /// `canvas_aspect` and `photo_aspect` are [`CropTransform::fit`]'s own two, and
    /// `canvas_aspect` is also what the frame's inset is measured in.
    ///
    /// `Err` when there is no framing to compute: no such cell, no such slot, or a
    /// frame whose gap empties the cell — which `validate` refuses, so only a
    /// document mutated in memory reaches it.
    pub fn fitted_crop(
        &self,
        slot: usize,
        canvas_aspect: f64,
        photo_aspect: f64,
    ) -> Result<CropFit, CoreError> {
        let crop = self
            .cells
            .get(slot)
            .ok_or(CoreError::NoSuchSlot {
                slot,
                slots: self.cells.len(),
            })?
            .crop;
        self.fit_crop(slot, crop, canvas_aspect, photo_aspect)
    }

    /// The fit of `crop` for `slot` — the same reference [`fitted_crop`] uses, for
    /// a request the caller is holding rather than one the document stores.
    ///
    /// This is what a gesture needs: it is editing the *fit* the user is looking at
    /// (S7), so it must clamp its own candidate numbers against the same cell the
    /// canvas is drawing, frame included.
    ///
    /// [`fitted_crop`]: Self::fitted_crop
    pub fn fit_crop(
        &self,
        slot: usize,
        crop: CropTransform,
        canvas_aspect: f64,
        photo_aspect: f64,
    ) -> Result<CropFit, CoreError> {
        let geometry = self
            .template
            .slots
            .get(slot)
            .ok_or(CoreError::InvalidSlot {
                slot,
                reason: "the template has no such slot",
            })?;
        let covering =
            self.frame
                .covering(geometry, canvas_aspect)
                .ok_or(CoreError::InvalidSlot {
                    slot,
                    reason: "the frame's gap leaves this slot with no visible area",
                })?;
        Ok(crop.fit(geometry, &covering, canvas_aspect, photo_aspect))
    }

    pub fn to_json(&self) -> Result<String, CoreError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let json = std::fs::read_to_string(path).map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json(&json)
    }

    /// Writes the document to `path`, atomically (S6.5).
    ///
    /// The document is validated first: a file this build writes has to be a file
    /// this build can read back, and `validate` is the only thing that knows.
    ///
    /// The write is [`atomic::write_atomic`]'s: the JSON goes to a temporary file in
    /// the target's own directory and is renamed over it, so a crash, a full disk or
    /// a kill leaves either the previous file or the new one, never half of either.
    /// A file that is already there keeps its mode, so a project saved while it is
    /// readable only by its owner does not come back world-readable because it was
    /// saved again (S15c, PIX-016). This is the one place a `.pixlay` the user
    /// changed is written (`docs/CONTRACT.md` §6).
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        self.validate()?;
        let json = self.to_json()?;
        atomic::write_atomic(path, |file| file.write_all(json.as_bytes())).map_err(|failure| {
            CoreError::Io {
                path: path.to_path_buf(),
                source: failure.into_io(),
            }
        })?;
        Ok(())
    }
}

/// The directory a project file's relative `source` paths resolve against: the
/// file's own directory, or the current directory when it has none (`x.pixlay`
/// names a file in `.`).
fn project_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// A `.pixlay` that has been read, parsed and validated, plus where it came from.
#[derive(Clone, Debug)]
pub struct Project {
    doc: CollageDoc,
    path: PathBuf,
    dir: PathBuf,
}

impl Project {
    /// A project around a document that has not come from a file yet (S7).
    ///
    /// The GUI edits a document in memory and only then decides where it lives;
    /// this is what lets it use [`Project::save_as`] — and with it the relative
    /// `source` rebasing — instead of writing the file itself. The document is
    /// validated, so a project cannot exist around a document this build would
    /// refuse to load.
    pub fn new(doc: CollageDoc, path: impl Into<PathBuf>) -> Result<Self, CoreError> {
        doc.validate()?;
        let path = path.into();
        Ok(Self {
            doc,
            dir: project_dir(&path).to_path_buf(),
            path,
        })
    }

    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let doc = CollageDoc::load(path)?;
        Ok(Self {
            doc,
            path: path.to_path_buf(),
            dir: project_dir(path).to_path_buf(),
        })
    }

    pub fn doc(&self) -> &CollageDoc {
        &self.doc
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file this project was read from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the document back to the file it was read from, atomically.
    ///
    /// Nothing is rebased: the relative `source` paths were resolved against that
    /// same directory, so the document is already expressed in the right terms —
    /// which is also why save → load → save is byte-identical.
    pub fn save(&self) -> Result<(), CoreError> {
        self.doc.save(&self.path)
    }

    /// Writes this project to `path` and returns the project that was written: the
    /// same document, for a copy.
    ///
    /// A relative `source` is relative to the *project file*, so a copy in
    /// another directory would otherwise quietly point at nothing. Every relative
    /// source is therefore rebased onto the new directory; an absolute source is
    /// left alone, as the contract accepts it as it stands. The rebase is part of
    /// writing the copy and does not change this project — but the copy it returns
    /// is the document the file now holds, spelling included, which is what a
    /// caller that keeps the document in memory has to adopt (PIX-005,
    /// 2026-09-24: the window used to keep the old spellings and resolve the photos
    /// against the wrong directory).
    pub fn save_as(&self, path: &Path) -> Result<Project, CoreError> {
        let dir = project_dir(path);
        let mut doc = self.doc.clone();
        if dir != self.dir {
            rebase_sources(&mut doc, &self.dir, dir);
        }
        doc.save(path)?;
        Ok(Project {
            doc,
            path: path.to_path_buf(),
            dir: dir.to_path_buf(),
        })
    }

    /// Resolves every cell's source path against the project directory.
    ///
    /// A missing file is an error, not a skipped cell: a project that points at
    /// a deleted photo must fail loudly instead of exporting a white hole.
    pub fn sources(&self) -> Result<Vec<Option<PathBuf>>, CoreError> {
        let mut resolved = Vec::with_capacity(self.doc.cells.len());
        for cell in &self.doc.cells {
            match &cell.source {
                None => resolved.push(None),
                Some(source) => {
                    let path = if source.is_absolute() {
                        source.clone()
                    } else {
                        self.dir.join(source)
                    };
                    if !path.is_file() {
                        return Err(CoreError::MissingSource { path });
                    }
                    resolved.push(Some(path));
                }
            }
        }
        Ok(resolved)
    }
}

/// Rewrites every relative `source` so it means the same file when the document
/// is read from `to_dir` instead of `from_dir`; `true` when any spelling moved.
///
/// Lexical, and deliberately so: the filesystem resolves `..` against the
/// directory it is standing in, so a purely lexical answer is the same path, and
/// nothing here needs the filesystem — a project can be copied while its photos
/// are on a drive that is not mounted. A path whose `..` components cancel stays
/// correct for the same reason.
pub(crate) fn rebase_sources(doc: &mut CollageDoc, from_dir: &Path, to_dir: &Path) -> bool {
    // Both are absolutized with `std::path::absolute` (lexical: no symlink
    // resolution, no filesystem access) because a relative answer needs a common
    // root to walk up from. Without one — no current directory — the paths are
    // left exactly as they are, which is at worst a copy that needs its photos
    // moved in beside it.
    let (Ok(from), Ok(to)) = (std::path::absolute(from_dir), std::path::absolute(to_dir)) else {
        return false;
    };
    let mut changed = false;
    for cell in &mut doc.cells {
        let Some(source) = cell.source.as_deref() else {
            continue;
        };
        if source.is_absolute() {
            continue;
        }
        if let Some(relative) = relative_to(&to, &from.join(source))
            && relative != source
        {
            cell.source = Some(relative);
            changed = true;
        }
    }
    changed
}

/// `target` expressed relative to the directory `from`.
///
/// `None` when the two share no root, which two absolute paths cannot. Both sides
/// are lexically normalized first ([`normalize_lexical`]) — `.` dropped and `..`
/// resolved against the component before it — so that a `..` in either spelling
/// cannot produce an answer that points somewhere else: the common prefix has to
/// be counted on the directories the paths *mean*, not on the way they are spelled
/// (PIX-006, 2026-09-24: `/a/b` to `/a/b/../copy.pixlay` used to hand back one
/// `..` too many, and the copy pointed at a file that was not there).
///
/// Public because it is the rule a *written* `source` follows, and two writers
/// apply it: [`Project::save_as`] rebasing a copy, and the CLI's `init --photo`
/// storing the photos a user picked next to the project it is creating. Written
/// lexically so neither has to touch the filesystem, and so a project can be
/// expressed while its photos are on a drive that is not mounted.
pub fn relative_to(from: &Path, target: &Path) -> Option<PathBuf> {
    let from = normalize_lexical(from);
    let target = normalize_lexical(target);
    let from: Vec<_> = from.components().collect();
    let target: Vec<_> = target.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None;
    }
    let mut relative = PathBuf::new();
    for _ in &from[common..] {
        relative.push("..");
    }
    for component in &target[common..] {
        relative.push(component.as_os_str());
    }
    Some(if relative.as_os_str().is_empty() {
        // The target *is* the directory being described; `.` is the path that
        // means that.
        PathBuf::from(".")
    } else {
        relative
    })
}

/// A path made lexical: `.` dropped, `..` resolved against the component before
/// it, nothing else touched.
///
/// **Lexical, and deliberately so**: no filesystem access and no symlink
/// resolution, so `a/../b` is `b` whether or not `a` exists, and a project whose
/// photos are behind an unmounted drive still has an answer. `..` with nothing to
/// go back to stays where it is — `..` in a relative path, `/` at the root — which
/// is what walking up from there means. `a/..` is the directory `a` stands in, and
/// `.` is how a path says that; an empty path names nothing and stays empty.
///
/// Two rules mean this by a path: the alias refusal that stops an export from
/// being written over a source image (`pixlay_imaging::destination`) and the
/// rebasing a project copy's sources go through ([`relative_to`]).
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                // `a/..` is the directory `a` stands in; `/..` is `/`.
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::Prefix(_) | Component::RootDir) => {}
                // Nothing to go back to, or already walking up: keep walking.
                _ => normalized.push(".."),
            },
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.as_os_str().is_empty() && !path.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    normalized
}
