//! Canvas-level text layers.
//!
//! Text is a canvas-level layer placed after the slots are composited: it is
//! positioned in canvas space, not in slot space, so rotating or reframing a
//! photo never moves it. The tiled watermark is the same layer with a different
//! mode, not a second mechanism.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geometry::Point;

/// A straight (non-premultiplied) color with an alpha channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// `255` is fully opaque.
    pub a: u8,
}

impl Rgba8 {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// Where a text layer's anchor points inside its own box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// How a text layer is laid out on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum TextMode {
    /// Placed once; `position` is in normalized canvas coordinates.
    Free { position: Point, anchor: Anchor },
    /// Repeated on a grid of `step` (normalized canvas units per repeat).
    Tiled { step: (f64, f64) },
}

/// Dynamic fields v1 understands inside `content`.
///
/// v1 knows exactly these three; any other `{name}` is rejected when the
/// document is loaded, so a typo cannot silently render as literal text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextToken {
    Date,
    Filename,
    Index,
}

impl TextToken {
    pub const ALL: [Self; 3] = [Self::Date, Self::Filename, Self::Index];

    pub fn name(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Filename => "filename",
            Self::Index => "index",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|token| token.name() == name)
    }
}

/// A `{name}` token in `content`, with the byte range it occupies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextTokenUse {
    pub token: TextToken,
    pub start: usize,
    pub end: usize,
}

/// Scans `content` for `{name}` tokens.
///
/// A token is a `{`, an ASCII alphabetic name and a `}`; anything else —
/// including an unclosed `{` or a name that is not alphabetic, such as
/// `{2 of 3}` — is literal text. Unknown alphabetic names are errors.
pub fn scan_tokens(content: &str) -> Result<Vec<TextTokenUse>, String> {
    let bytes = content.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        let Some(close) = content[i + 1..].find('}').map(|offset| i + 1 + offset) else {
            break;
        };
        let name = &content[i + 1..close];
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphabetic()) {
            match TextToken::from_name(name) {
                Some(token) => found.push(TextTokenUse {
                    token,
                    start: i,
                    end: close + 1,
                }),
                None => return Err(name.to_string()),
            }
        }
        i = close + 1;
    }
    Ok(found)
}

/// One canvas-level text layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextLayer {
    /// Text with optional `{date}` / `{filename}` / `{index}` tokens.
    pub content: String,
    pub mode: TextMode,
    /// Font size as a fraction of the canvas height.
    ///
    /// Normalized on purpose: preview and export must render the same layout, so
    /// a device-pixel size would break the preview/export RMSE invariant.
    pub size_rel: f64,
    /// Rotation in degrees about the anchor.
    #[serde(default)]
    pub rotation_deg: f64,
    pub color: Rgba8,
    /// Which slot's EXIF feeds `{date}` / `{filename}` / `{index}`. `None`
    /// resolves against the document's `textFallback`.
    #[serde(default)]
    pub source_slot: Option<usize>,
}

impl TextLayer {
    /// Largest font size accepted, as a fraction of the canvas height.
    pub const MAX_SIZE_REL: f64 = 1.0;

    pub fn validate(&self, layer: usize, slots: usize) -> Result<(), CoreError> {
        if !self.size_rel.is_finite() || self.size_rel <= 0.0 || self.size_rel > Self::MAX_SIZE_REL
        {
            return Err(CoreError::OutOfRange {
                what: "text size (fraction of canvas height)",
                value: self.size_rel,
                min: f64::MIN_POSITIVE,
                max: Self::MAX_SIZE_REL,
            });
        }
        if !self.rotation_deg.is_finite() {
            return Err(CoreError::OutOfRange {
                what: "text rotation (degrees)",
                value: self.rotation_deg,
                min: f64::MIN,
                max: f64::MAX,
            });
        }
        if let Some(slot) = self.source_slot
            && slot >= slots
        {
            return Err(CoreError::InvalidSlot {
                slot,
                reason: "text layer references a slot the template does not have",
            });
        }
        match self.mode {
            TextMode::Free { position, .. } => {
                if !position.is_finite() {
                    return Err(CoreError::OutOfRange {
                        what: "text position",
                        value: f64::NAN,
                        min: 0.0,
                        max: 1.0,
                    });
                }
            }
            TextMode::Tiled { step } => {
                if !step.0.is_finite() || !step.1.is_finite() || step.0 <= 0.0 || step.1 <= 0.0 {
                    return Err(CoreError::OutOfRange {
                        what: "tiled text step",
                        value: step.0.min(step.1),
                        min: f64::MIN_POSITIVE,
                        max: 1.0,
                    });
                }
            }
        }
        if let Err(token) = scan_tokens(&self.content) {
            return Err(CoreError::UnknownTextToken {
                layer,
                token,
                known: "{date}, {filename}, {index}",
            });
        }
        Ok(())
    }
}

/// Values for the dynamic fields when the source photo cannot supply them.
///
/// `{date}` reads EXIF `DateTimeOriginal` verbatim (no timezone conversion);
/// when it is missing, the project's stored string is used so an export stays
/// reproducible. `{filename}` falls back to the source file name and `{index}`
/// is always the slot index, so neither needs a stored value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFallback {
    pub date: String,
}
