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
    /// Placed once; `position` is in normalized canvas coordinates and is the
    /// point `anchor` names (so `BottomCenter` puts the text's baseline area
    /// above the point).
    Free { position: Point, anchor: Anchor },
    /// Repeated on a grid of `step` (normalized canvas units per repeat).
    ///
    /// The tile grid starts at the canvas origin `(0, 0)` — the first tile's
    /// anchor is the top-left corner of the canvas — so the pattern is a function
    /// of `step` alone and does not move when the tiles are re-laid out. The
    /// layer's `rotation_deg` rotates each tile about its own anchor, and its
    /// `size_rel` sizes every tile identically; there is no per-tile variation
    /// (a watermark that gets denser toward one corner is not v1).
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

/// What a slot's photo can tell a text layer that the document does not store.
///
/// `{date}` is EXIF `DateTimeOriginal` and `{filename}` is the source path's file
/// name: both belong to the decoder and the loader, so they arrive here as values
/// instead of being something the layer could read for itself (contract §1).
/// `{index}` is the layer's own `source_slot` and needs no value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextValues {
    /// EXIF `DateTimeOriginal`, verbatim; `None` when the file has no usable one.
    pub date: Option<String>,
    pub filename: Option<String>,
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

    /// Largest number of tiles one layer may ask for.
    ///
    /// A tiled step has no upper bound, so a small enough step is a request for an
    /// unbounded amount of work: `step = 1e-9` is a billion by a billion tiles.
    /// 10,000 is far past any watermark a person would draw — a 1/100 step is a
    /// 101x101 grid of marks — and it is checked when the document loads, so a
    /// project with such a hole is refused rather than discovered at export time.
    pub const MAX_TILES: usize = 10_000;

    /// `content` with every `{date}`, `{filename}` and `{index}` replaced.
    ///
    /// A token with no value renders as an empty string: the alternative — leaving
    /// `{date}` on the finished collage because a phone stripped the EXIF block —
    /// puts a debugging token in the user's product. The one fallback is `{date}`'s,
    /// and it is the document's own [`TextFallback::date`] (contract §1).
    ///
    /// `values` are what the layer's `source_slot` has to offer; pass
    /// `TextValues::default()` when the layer names no slot.
    ///
    /// `{index}` is the slot index as the document numbers slots — `0` for the
    /// first slot in template order, the same number the `template.<i>` and
    /// `probe` rows of the CLI use.
    ///
    /// `values` are ignored when the layer names no slot: a layer without a
    /// `sourceSlot` has no photo to say what the file name or the date is, and
    /// taking them anyway would let a caller who looked up the wrong slot change
    /// what a watermark says.
    ///
    /// An unknown `{token}` is left as literal text. It cannot reach here from a
    /// loaded project (`validate` refuses one), only from a document mutated in
    /// memory, and the renderer's policy for those is to draw what they say.
    pub fn resolve(&self, values: &TextValues, fallback: &TextFallback) -> String {
        let values = match self.source_slot {
            Some(_) => values,
            None => &TextValues::default(),
        };
        let Ok(uses) = scan_tokens(&self.content) else {
            return self.content.clone();
        };
        if uses.is_empty() {
            return self.content.clone();
        }
        let mut out = String::with_capacity(self.content.len() + 16);
        let mut cursor = 0;
        for use_ in &uses {
            out.push_str(&self.content[cursor..use_.start]);
            match use_.token {
                TextToken::Date => match values.date.as_deref().filter(|date| !date.is_empty()) {
                    Some(date) => out.push_str(date),
                    None => out.push_str(&fallback.date),
                },
                TextToken::Filename => {
                    out.push_str(values.filename.as_deref().unwrap_or_default());
                }
                TextToken::Index => {
                    if let Some(slot) = self.source_slot {
                        out.push_str(&slot.to_string());
                    }
                }
            }
            cursor = use_.end;
        }
        out.push_str(&self.content[cursor..]);
        out
    }

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
                // A free layer is placed in normalized canvas coordinates, so a
                // position outside `[0, 1]` is off the canvas by definition. It
                // has to be refused here: nothing downstream can tell an
                // off-canvas placement from an intentional one, and S5 would
                // simply paint nothing where the user expects text.
                for (what, value) in [
                    ("text position x", position.x),
                    ("text position y", position.y),
                ] {
                    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                        return Err(CoreError::OutOfRange {
                            what,
                            value,
                            min: 0.0,
                            max: 1.0,
                        });
                    }
                }
            }
            TextMode::Tiled { step } => {
                // The contract's rule is "both components > 0": a step of 0 or a
                // negative value would make the tiling loop forever, and that is
                // the whole precondition. There is no upper bound to advertise,
                // so this must not reuse `OutOfRange`, whose message names a
                // range that would then be a lie.
                if !step.0.is_finite() || !step.1.is_finite() || step.0 <= 0.0 || step.1 <= 0.0 {
                    return Err(CoreError::InvalidTiledStep {
                        x: step.0,
                        y: step.1,
                    });
                }
                // The step has no *upper* bound, but a small enough one is an
                // unbounded amount of work: the grid starts at the canvas origin
                // and covers it, so the count follows from the step alone.
                if tiled_grid(step).is_none() {
                    return Err(CoreError::TooManyTiles {
                        x: step.0,
                        y: step.1,
                        max: Self::MAX_TILES,
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

/// The tile grid a `step` asks for: `(columns, rows)`, or `None` when it would need
/// more than [`TextLayer::MAX_TILES`] tiles.
///
/// The grid starts at the canvas origin (`(0, 0)` is the first tile's anchor), so
/// each axis has one anchor per step up to and including the far edge:
/// `floor(1 / step) + 1` of them. The far-edge anchor is kept deliberately — its
/// tile is invisible unrotated, but a rotated watermark swings ink back over the
/// canvas, and dropping it would leave a stripe with no watermark on it.
///
/// Both the loader (`TextLayer::validate`) and the renderer ask this one function,
/// so a refused document and a drawn one cannot disagree about what a step means.
pub fn tiled_grid(step: (f64, f64)) -> Option<(usize, usize)> {
    fn axis(step: f64) -> Option<usize> {
        if !step.is_finite() || step <= 0.0 {
            return None;
        }
        let anchors = (1.0 / step).floor() + 1.0;
        if anchors >= TextLayer::MAX_TILES as f64 {
            // Saturate instead of wrapping: a step of 1e-300 is a number, and the
            // product below has to stay comparable against the cap.
            return Some(TextLayer::MAX_TILES);
        }
        Some(anchors as usize)
    }
    let columns = axis(step.0)?;
    let rows = axis(step.1)?;
    (columns * rows <= TextLayer::MAX_TILES).then_some((columns, rows))
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
