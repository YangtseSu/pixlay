//! Typed errors for the rendering path.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("cairo: {0}")]
    Cairo(#[from] cairo::Error),

    #[error("cairo surface: {0}")]
    Surface(#[from] cairo::BorrowError),

    #[error("document: {0}")]
    Document(#[from] pixlay_core::CoreError),

    #[error("bitmap is {width}x{height} but {expected} bytes were supplied, got {got}")]
    BitmapSize {
        width: i32,
        height: i32,
        expected: usize,
        got: usize,
    },

    #[error("bitmap {what} must be positive, got {value}")]
    BitmapDimension { what: &'static str, value: i32 },

    #[error("render scale must be finite and positive, got {0}")]
    InvalidScale(f64),

    #[error("band {index} of {count} does not exist")]
    InvalidBand { index: u32, count: u32 },

    #[error("slot {slot} has a degenerate outline")]
    DegenerateSlot { slot: usize },

    #[error("cannot read pixels from a {format} surface")]
    SurfaceFormat { format: String },
}
