//! The document itself, and the project file around it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::DOC_VERSION;
use crate::canvas::CanvasSpec;
use crate::crop::CropTransform;
use crate::error::CoreError;
use crate::template::Template;
use crate::text::{TextFallback, TextLayer};

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
    pub canvas: CanvasSpec,
    pub template: Template,
    /// One cell per slot, in template order.
    pub cells: Vec<Cell>,
    #[serde(default)]
    pub text: Vec<TextLayer>,
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
        if self.doc_version != DOC_VERSION {
            return Err(CoreError::VersionUnsupported {
                found: self.doc_version,
                supported: DOC_VERSION,
            });
        }
        self.canvas.validate()?;
        self.template.validate()?;
        if self.cells.len() != self.template.slots.len() {
            return Err(CoreError::CellCount {
                cells: self.cells.len(),
                slots: self.template.slots.len(),
            });
        }
        for cell in &self.cells {
            cell.crop.validate()?;
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
}

/// A `.pixlay` that has been read, parsed and validated, plus the directory its
/// relative paths resolve against.
#[derive(Clone, Debug)]
pub struct Project {
    doc: CollageDoc,
    dir: PathBuf,
}

impl Project {
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let doc = CollageDoc::load(path)?;
        let dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Ok(Self { doc, dir })
    }

    pub fn doc(&self) -> &CollageDoc {
        &self.doc
    }

    pub fn dir(&self) -> &Path {
        &self.dir
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
