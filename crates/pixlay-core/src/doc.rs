//! The document itself, and the project file around it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::canvas::CanvasSpec;
use crate::crop::CropTransform;
use crate::error::CoreError;
use crate::grade::{FilterPreset, Grade};
use crate::template::Template;
use crate::text::{TextFallback, TextLayer};
use crate::{ASPECT_TOLERANCE, DOC_VERSION, DOC_VERSION_MIN};

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
    /// Per-slot color grading, applied in linear light after the photo has been
    /// placed. The default is the identity, so a document written before grading
    /// existed loads unchanged and "adding a field does not bump the version"
    /// holds (docs/CONTRACT.md §1, version policy).
    #[serde(default)]
    pub grade: Grade,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            source: None,
            crop: CropTransform::IDENTITY,
            grade: Grade::IDENTITY,
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
    pub canvas: CanvasSpec,
    pub template: Template,
    /// One cell per slot, in template order.
    pub cells: Vec<Cell>,
    /// Canvas-level text layers, painted in vector order: a later layer draws over
    /// an earlier one, and all of them draw over every cell.
    #[serde(default)]
    pub text: Vec<TextLayer>,
    /// The one-click canvas-wide filter. A preset expands to a [`Grade`] applied
    /// to every slot after its own grade (docs/CONTRACT.md §4); `none` is the
    /// default, so this field costs a project nothing until it is used.
    #[serde(default)]
    pub filter: FilterPreset,
    #[serde(default)]
    pub text_fallback: TextFallback,
}

impl CollageDoc {
    /// A document with the slot count of `template` and every cell empty.
    pub fn new(canvas: CanvasSpec, template: Template) -> Self {
        let cells = vec![Cell::default(); template.slots.len()];
        Self {
            doc_version: DOC_VERSION,
            canvas,
            template,
            cells,
            text: Vec::new(),
            filter: FilterPreset::None,
            text_fallback: TextFallback::default(),
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
        self.canvas.validate()?;
        // The canvas aspect and the template aspect are both part of the layout:
        // the template's normalized geometry is stretched onto the canvas, so a
        // mismatch silently distorts every slot. Nothing downstream can detect it,
        // because normalized coordinates carry no aspect of their own.
        let canvas_aspect = self.canvas.aspect();
        if (canvas_aspect - self.template.aspect).abs() > ASPECT_TOLERANCE {
            return Err(CoreError::AspectMismatch {
                canvas: canvas_aspect,
                template: self.template.aspect,
            });
        }
        self.template.validate()?;
        if self.cells.len() != self.template.slots.len() {
            return Err(CoreError::CellCount {
                cells: self.cells.len(),
                slots: self.template.slots.len(),
            });
        }
        for cell in &self.cells {
            cell.crop.validate()?;
            cell.grade.validate()?;
        }
        for (index, layer) in self.text.iter().enumerate() {
            layer.validate(index, self.template.slots.len())?;
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        let doc: Self = serde_json::from_str(json)?;
        doc.validate()?;
        Ok(doc)
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
    /// The JSON goes to a temporary file *in the same directory* — `rename` is
    /// only atomic within one filesystem — is flushed to disk, and is then
    /// renamed over `path`. A crash, a full disk or a kill in the middle
    /// therefore leaves either the previous file or the new one, never half of
    /// either; this is the one place a `.pixlay` the user changed is written
    /// (docs/CONTRACT.md §6).
    ///
    /// The document is validated first: a file this build writes has to be a file
    /// this build can read back, and `validate` is the only thing that knows.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        self.validate()?;
        let json = self.to_json()?;
        write_atomic(path, json.as_bytes())
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

/// Writes `bytes` to `path` through a temporary file and a rename.
///
/// Errors name `path`, the file the caller asked for; the temporary file is an
/// implementation detail, and a message that named it would send the user looking
/// for something they never asked to write. A failed write removes it.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    use std::io::Write as _;

    let dir = project_dir(path);
    let name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("project"));
    // Unique per process, and a dotfile inside the target's own directory: two
    // saves cannot collide, and the rename cannot land on another filesystem.
    // Assembled as an `OsString` so a path that is not valid UTF-8 stays exact.
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = dir.join(temp_name);
    let failed = |source: std::io::Error| CoreError::Io {
        path: path.to_path_buf(),
        source,
    };

    let mut file = std::fs::File::create(&temp).map_err(failed)?;
    // `sync_all` before the rename: without it a crash can leave the new name
    // pointing at a file whose bytes never reached the disk, which is the one way
    // an atomic rename can still lose a document.
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(source) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(failed(source));
    }
    std::fs::rename(&temp, path).map_err(|source| {
        // The old file is untouched; only the temporary one is litter.
        let _ = std::fs::remove_file(&temp);
        failed(source)
    })
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

    /// Writes this project to `path`: the same document, for a copy.
    ///
    /// A relative `source` is relative to the *project file*, so a copy in
    /// another directory would otherwise quietly point at nothing. Every relative
    /// source is therefore rebased onto the new directory; an absolute source is
    /// left alone, as the contract accepts it as it stands. The rebase is part of
    /// writing the copy and does not change this project.
    pub fn save_as(&self, path: &Path) -> Result<(), CoreError> {
        let dir = project_dir(path);
        if dir == self.dir {
            return self.doc.save(path);
        }
        let mut doc = self.doc.clone();
        rebase_sources(&mut doc, &self.dir, dir);
        doc.save(path)
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
/// is read from `to_dir` instead of `from_dir`.
///
/// Lexical, and deliberately so: the filesystem resolves `..` against the
/// directory it is standing in, so a purely lexical answer is the same path, and
/// nothing here needs the filesystem — a project can be copied while its photos
/// are on a drive that is not mounted. A path whose `..` components cancel stays
/// correct for the same reason.
fn rebase_sources(doc: &mut CollageDoc, from_dir: &Path, to_dir: &Path) {
    // Both are absolutized with `std::path::absolute` (lexical: no symlink
    // resolution, no filesystem access) because a relative answer needs a common
    // root to walk up from. Without one — no current directory — the paths are
    // left exactly as they are, which is at worst a copy that needs its photos
    // moved in beside it.
    let (Ok(from), Ok(to)) = (std::path::absolute(from_dir), std::path::absolute(to_dir)) else {
        return;
    };
    for cell in &mut doc.cells {
        let Some(source) = cell.source.as_deref() else {
            continue;
        };
        if source.is_absolute() {
            continue;
        }
        if let Some(relative) = relative_to(&to, &from.join(source)) {
            cell.source = Some(relative);
        }
    }
}

/// `target` expressed relative to the directory `from`.
///
/// `None` when the two share no root, which two absolute paths cannot. The
/// components of both sides come from [`Path::components`], so `.` and repeated
/// separators are already gone and `..` is compared literally — the same thing the
/// filesystem does with it.
///
/// Public because it is the rule a *written* `source` follows, and two writers
/// apply it: [`Project::save_as`] rebasing a copy, and the CLI's `init --photo`
/// storing the photos a user picked next to the project it is creating. Written
/// lexically so neither has to touch the filesystem, and so a project can be
/// expressed while its photos are on a drive that is not mounted.
pub fn relative_to(from: &Path, target: &Path) -> Option<PathBuf> {
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
