//! Decoding, resampling, color adjustment, EXIF, color space handling and
//! encoding.
//!
//! Boundary: no gtk, and no cairo either — a bitmap leaves this crate as a bare
//! buffer, and the receiving thread (or the caller) wraps it. Threading is the
//! caller's, with one exception the decoder cannot avoid: `decode` runs the
//! loader on a private thread with its own main context, because a glycin frame
//! only completes while a main context is being iterated ([`driver`]). From the
//! outside every function here is synchronous and pure.
//!
//! # What this crate guarantees about a pixel
//!
//! * **Where it points**: the pixels are the photo's, upright (EXIF orientation
//!   applied), cropped and resampled to the exact size the canvas displays, with
//!   the region origin the canvas places them at. The canvas never resamples.
//! * **What it means**: linear light, up to the final quantization. The transfer
//!   function is applied once on the way in ([`transfer`]) and once on the way out
//!   ([`LinearRgb16::to_srgb8`]); everything between is 16-bit.
//! * **What color it is**: sRGB. A source carrying its own ICC profile is
//!   converted by the loader (measured against ImageMagick: 0.03 levels apart);
//!   a source with none is interpreted as sRGB, which is v1's documented
//!   limitation for unprofiled data.
//! * **What alpha it has**: none. The source's alpha is composited onto the
//!   slot's opaque white base inside the pipeline, so an export is never
//!   transparent and `pixlay-render` sees only opaque pixels.
//! * **Whether it is the user's file**: untouched. Every stage reads; nothing
//!   here opens a source for writing, ever (`AGENTS.md`, "Source images are
//!   read-only").
//!
//! # The pipeline, in the frozen order
//!
//! ```text
//! decode (upright, sRGB, straight)            decode.rs
//!   → crop to what the slot shows             layout.rs + resample.rs
//!   → resample in linear light, Lanczos3      resample.rs  (16-bit from here on)
//!   → flatten onto the slot's white base      linear.rs
//!   → per-slot grading                        linear.rs
//!   → global filter                           linear.rs
//!   → 8-bit sRGB in Cairo's layout            linear.rs
//! ```
//!
//! [`slot_bitmaps`] does all of it for a whole document; [`slot_bitmap`] does it
//! for one slot, and [`resample`] is public because the tests measure it directly
//! (the aliasing and RMSE criteria are about that one stage).
//!
//! The mirror image of the pipeline is the export: [`encode`] writes PNG, JPEG or
//! TIFF with the resolution and the sRGB profile ([`icc`]) in the same pass as the
//! pixels. It lives here because it is pixels in and pixels out — no cairo, no gtk
//! — and because both the CLI and the GUI export through it.

pub mod decode;
mod driver;
pub mod encode;
pub mod exif;
pub mod icc;
pub mod layout;
pub mod linear;
pub mod probe;
pub mod resample;
pub mod thumb;
pub mod transfer;

mod error;

pub use decode::{
    DECODE_TIMEOUT, DecodeLimits, Depth, MAX_DECODE_EDGE, MAX_DECODE_PIXELS, Sampler, Source,
};
pub use encode::{Chroma, Export, Format};
pub use error::ImagingError;
pub use layout::{REGION_GUARD_PX, SlotBitmap, slot_bitmap, slot_bitmaps};
pub use linear::{LinearRgb16, LinearRgba16};
pub use probe::{ProbeReport, Rgb8View, probe};
pub use resample::{Region, resample};
pub use thumb::{Thumbnail, thumbnail};
