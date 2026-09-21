//! Per-slot color grading and the one-click canvas-wide filter.
//!
//! Both are the same mechanism: a [`Grade`] is three numbers applied in linear
//! light, and a one-click [`FilterPreset`] is a named grade applied to every
//! slot. There is no second color path.
//!
//! The three parameters are the ones the contract's identity criterion names
//! (`factor = 1`, `s = 1`, `delta = 0` is pixel-identical to the input):
//!
//! * `factor` — an exposure multiplier on linear RGB;
//! * `saturation` — a multiplier on the distance from the linear luminance;
//! * `delta` — a warmth shift, applied as `r *= 1 + delta`, `b *= 1 - delta`.
//!
//! The order is fixed and tested: exposure, warmth, saturation. Saturation runs
//! on the luminance of the already-exposed pixel, so raising the exposure cannot
//! change the hue a saturation setting produces.
//!
//! Where the math lives: this module holds the *parameters* (they are document
//! data, and a `.pixlay` stores them); `pixlay-imaging` applies them to 16-bit
//! linear samples. Core never touches pixels.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Smallest and largest exposure multiplier accepted.
///
/// Bounded on both sides because the value multiplies linear light: at 0 the
/// image is black, and an unbounded value is only a way to saturate every
/// channel to white. 0.2..=5.0 covers "two and a half stops down" to "two stops
/// up", which is more than a slide of the control can express usefully.
pub const GRADE_FACTOR_RANGE: (f64, f64) = (0.2, 5.0);

/// Saturation multiplier range: 0 is greyscale, 1 leaves the pixel alone.
pub const GRADE_SATURATION_RANGE: (f64, f64) = (0.0, 4.0);

/// Widest warmth shift. At 1 the red channel is doubled and the blue channel
/// removed; past that the mapping stops being monotone.
pub const GRADE_DELTA_RANGE: (f64, f64) = (-1.0, 1.0);

/// How one cell's color is adjusted, and what a filter preset expands to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grade {
    /// Exposure multiplier on linear RGB. 1.0 leaves the image unchanged.
    #[serde(default = "one")]
    pub factor: f64,
    /// Saturation multiplier around the linear luminance. 1.0 leaves the image
    /// unchanged, 0.0 is greyscale.
    #[serde(default = "one")]
    pub saturation: f64,
    /// Warmth (`Δ`): `r *= 1 + delta`, `b *= 1 - delta`. 0.0 is unchanged.
    #[serde(default)]
    pub delta: f64,
}

fn one() -> f64 {
    1.0
}

impl Default for Grade {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Grade {
    /// The grade that changes nothing, and therefore must be pixel-identical.
    pub const IDENTITY: Self = Self {
        factor: 1.0,
        saturation: 1.0,
        delta: 0.0,
    };

    /// True for the exact identity. `pixlay-imaging` skips the pass entirely when
    /// this holds, which is what makes the contract's identity criterion exact
    /// rather than "within round-off".
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        for (what, value, (min, max)) in [
            ("grade factor", self.factor, GRADE_FACTOR_RANGE),
            ("grade saturation", self.saturation, GRADE_SATURATION_RANGE),
            ("grade delta (warmth)", self.delta, GRADE_DELTA_RANGE),
        ] {
            if !value.is_finite() || value < min || value > max {
                return Err(CoreError::OutOfRange {
                    what,
                    value,
                    min,
                    max,
                });
            }
        }
        Ok(())
    }
}

/// The canvas-wide filter: one click, applied to every slot.
///
/// A preset is a named [`Grade`] applied *after* the cell's own grade (the frozen
/// evaluation order is "per-slot grading → global filter"), which is why a
/// document can carry both without a second color pipeline. The names are stable
/// English identifiers and are stored in the `.pixlay` as they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterPreset {
    /// No filter: the identity grade.
    #[default]
    None,
    /// A warm cast, `delta = +0.25`.
    Warm,
    /// A cool cast, `delta = -0.25`.
    Cool,
    /// Greyscale, `saturation = 0`.
    Mono,
    /// More contrast in the color: `saturation = 1.35`, `factor = 1.05`.
    Vivid,
    /// A softer, flatter look: `saturation = 0.8`, `factor = 1.1`.
    Fade,
}

impl FilterPreset {
    /// Every preset, in the order the GUI lists them.
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Warm,
        Self::Cool,
        Self::Mono,
        Self::Vivid,
        Self::Fade,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Warm => "warm",
            Self::Cool => "cool",
            Self::Mono => "mono",
            Self::Vivid => "vivid",
            Self::Fade => "fade",
        }
    }

    /// The grade this preset expands to. The numbers are the product decision and
    /// live here so the CLI and the GUI cannot disagree about them.
    pub fn grade(self) -> Grade {
        match self {
            Self::None => Grade::IDENTITY,
            Self::Warm => Grade {
                delta: 0.25,
                ..Grade::IDENTITY
            },
            Self::Cool => Grade {
                delta: -0.25,
                ..Grade::IDENTITY
            },
            Self::Mono => Grade {
                saturation: 0.0,
                ..Grade::IDENTITY
            },
            Self::Vivid => Grade {
                factor: 1.05,
                saturation: 1.35,
                ..Grade::IDENTITY
            },
            Self::Fade => Grade {
                factor: 1.1,
                saturation: 0.8,
                ..Grade::IDENTITY
            },
        }
    }
}
