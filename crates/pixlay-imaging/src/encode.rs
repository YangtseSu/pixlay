// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Output encoding: pixels and metadata in one pass.
//!
//! A collage is exported as a picture, so the file has to say what colour space it
//! is in — written by the encoder while the pixels go out, never by a second pass
//! over the finished file. The trap this rule exists for is measured: re-encoding
//! an already-encoded JPEG to patch its metadata silently drops 4:4:4 to 4:2:0
//! (2.71 MB to 1.49 MB, S0). Cairo can supply the pixels (`pixlay-render`) but not
//! the metadata: `cairo_surface_write_to_png` emits only IHDR/bKGD/IDAT and no
//! iCCP, so an export written through it loses its colour space.
//!
//! # What each format carries
//!
//! | Format | Profile |
//! |---|---|
//! | PNG | `iCCP` (deflate) |
//! | JPEG | `APP2` `ICC_PROFILE` segments |
//! | AVIF | a `colr` box of type `prof` |
//!
//! What a file deliberately does **not** carry is a resolution (S12d): a raster's
//! only intrinsic size is its pixels, and the product has no concept of paper for
//! a density number to describe. A PNG has no `pHYs`, and the JPEG's JFIF density
//! stays at the encoder's default — square pixels, no unit.
//!
//! TIFF left with S12c (the purity ruling — raster formats a collage is exported
//! as), and with it the `tiff` dependency. **AVIF joined PNG and JPEG in S34**
//! (2026-10-08), and it is the one format whose writer is not this crate's own:
//! libheif writes it, reached through glycin's encoder API — the same backend, and
//! the same `glycin-heif` loader, that already decodes the AVIF and HEIC *sources*
//! the product opens. Pixels, profile and quality go into one `create` call, so the
//! one-pass rule holds for it as it does for the other two. The price of that reuse
//! is that a machine without the heif loader has no AVIF encoder: [`write`] refuses
//! with a sentence saying so rather than falling back to another format, because the
//! extension is the interface (a `.avif` that is really a JPEG is worse than a
//! refusal).
//!
//! The PNG `sRGB` chunk is deliberately **not** written alongside `iCCP`: the
//! specification says the two should not both be present, and the profile is the
//! one that carries the actual colorimetry.
//!
//! JPEG quality is 90, the S0/S4 baseline. Chroma subsampling is **4:4:4**, fixed
//! rather than chosen: `AGENTS.md` fixes libjpeg-turbo 4:4:4 as the product's
//! sampling, and S12c removed the `--chroma` flag that made it a request.
//!
//! AVIF quality is 90 for the same reason ([`AVIF_QUALITY`]): one number, fixed,
//! so an export's cost and its bytes are comparable across steps.
//!
//! # The write itself (S15c)
//!
//! Everything above is about what a *successful* export contains. Two rules are about
//! whether it happens at all: the destination may not be one of the document's own
//! photos ([`crate::destination`], checked by the caller before anything is decoded),
//! and the pixels reach the file through [`pixlay_core::atomic`] — a temporary file
//! beside the target, one rename — so a failure in the middle of encoding leaves the
//! export that was already there exactly as it was (PIX-011). The JPEG's own grid is
//! refused past 65535 px on either edge before the temporary file exists, because the
//! format's size fields are 16 bits (PIX-024).

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use glycin::{Creator, MemoryFormat, MimeType};
use pixlay_core::atomic;
use thiserror::Error;

use crate::Rgb8View;
use crate::driver;
use crate::icc;

/// JPEG quality, as a percentage. 90 is fixed rather than a flag
/// (`docs/CONTRACT.md` §5), which is what keeps every measurement in §8 comparable.
pub const JPEG_QUALITY: u8 = 90;

/// AVIF quality, as a percentage, on libheif's own scale.
///
/// 90, the same number as the JPEG's, fixed rather than a flag for the same reason.
/// The two numbers do not buy the same thing: measured (S34, 2026-10-08, a 3000x2000
/// photograph against its own PNG), the JPEG's q90 is 499,933 bytes at RMSE 0.0032
/// and its q50 is 234,977 bytes at 0.0068, while AVIF's q90 is **185,877 bytes at
/// 0.0066** — 2.7x smaller than the JPEG's q90, and at least as close to the source
/// as the JPEG at half its quality.
pub const AVIF_QUALITY: u8 = 90;

/// What `--out`'s extension selects.
///
/// PNG, JPEG and AVIF only: TIFF left with S12c, so an extension this build does
/// not write is a usage error rather than a silent fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Avif,
}

impl Format {
    /// The format an output path names, or `None` for an extension this build
    /// does not write. The extension is the whole interface: a `.jpg` that is
    /// really a PNG is worse than a refusal.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Avif => "avif",
        }
    }

    /// The extensions `from_path` accepts, for the usage message.
    pub const EXTENSIONS: &'static str = ".png, .jpg, .jpeg or .avif";
}

/// One image to write, with the metadata the file has to carry.
pub struct Export<'a> {
    pub format: Format,
    pub image: Rgb8View<'a>,
}

/// Writes `export` to `path` in one pass and returns the number of bytes written.
///
/// The size is validated against the format's own limits before anything is
/// created, and the pixels then go to a temporary file that is renamed over `path`:
/// an export that fails in the middle — a full disk, a broken encoder, a kill —
/// leaves the file that was already there exactly as it was (S15c, PIX-011).
/// Whether `path` may be written at all is the caller's question, and the answer for
/// a source image is [`crate::destination::refuse_source_alias`].
pub fn write(path: &Path, export: &Export<'_>) -> Result<u64, EncodeError> {
    let (width, height) = (export.image.width, export.image.height);
    if width <= 0 || height <= 0 {
        return Err(EncodeError::Empty { width, height });
    }
    // JPEG's own size fields hold 16 bits, so a grid wider or taller than 65535 px
    // has no representation in the file it would be written to (S15c, PIX-024). It
    // is refused here, before the temporary file exists, rather than truncated on
    // the way into the encoder. PNG's fields are 32-bit, so its only bound is the
    // buffer length below.
    if export.format == Format::Jpeg
        && (u16::try_from(width).is_err() || u16::try_from(height).is_err())
    {
        return Err(EncodeError::JpegSize { width, height });
    }
    // 64-bit arithmetic: `width * height * 3` in `i32` overflows for the largest
    // grids a caller may legally ask about, and the answer would be a panic in a
    // debug build.
    let expected = width as u64 * height as u64 * 3;
    if expected != export.image.data.len() as u64 {
        return Err(EncodeError::BufferSize {
            width,
            height,
            expected,
            found: export.image.data.len(),
        });
    }
    match atomic::write_atomic(path, |writer| match export.format {
        Format::Png => write_png(writer, export),
        Format::Jpeg => write_jpeg(writer, export),
        Format::Avif => write_avif(writer, export),
    }) {
        Ok(bytes) => Ok(bytes),
        Err(atomic::Failure::Io(source)) => Err(EncodeError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Err(atomic::Failure::Body(failure)) => Err(failure.at(path)),
    }
}

fn write_png(writer: &mut BufWriter<File>, export: &Export<'_>) -> Result<(), Failure> {
    let mut info = png::Info::with_size(export.image.width as u32, export.image.height as u32);
    info.bit_depth = png::BitDepth::Eight;
    info.color_type = png::ColorType::Rgb;
    // No `pHYs`: the file carries pixels and a colour space, not a resolution
    // (S12d); leaving `pixel_dims` unset keeps the chunk out entirely.
    info.icc_profile = Some(Cow::Borrowed(icc::srgb_profile()));
    let mut encoder = png::Encoder::with_info(writer, info).map_err(Failure::Png)?;
    // `Balanced` is the library's default level; stated here because the size and
    // time an export costs are part of what S6 measures against the S0 baseline.
    encoder.set_compression(png::Compression::Balanced);
    let mut png = encoder.write_header().map_err(Failure::Png)?;
    png.write_image_data(export.image.data)
        .map_err(Failure::Png)?;
    png.finish().map_err(Failure::Png)
}

fn write_jpeg(writer: &mut BufWriter<File>, export: &Export<'_>) -> Result<(), Failure> {
    let mut encoder = jpeg_encoder::Encoder::new(writer, JPEG_QUALITY);
    // The JFIF density is left at the encoder's default (unit 0, square pixels):
    // no resolution is claimed for a file whose size is only its pixels (S12d).
    // 4:4:4, unrequested and unwritable by any flag: `AGENTS.md` fixes it, and a
    // collage's hard colour edges are exactly what subsampling ruins (S0 measured
    // a metadata re-encode dropping 4:4:4 to 4:2:0 and 2.71 MB to 1.49 MB).
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
    encoder
        .add_icc_profile(icc::srgb_profile())
        .map_err(Failure::Jpeg)?;
    // The pair fits by construction: `write` refuses a JPEG past 65535 px on either
    // edge before the temporary file exists, because these are the format's own
    // 16-bit fields (PIX-024).
    encoder
        .encode(
            export.image.data,
            export.image.width as u16,
            export.image.height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(Failure::Jpeg)
}

/// Writes the one format this crate does not encode itself: libheif does, through
/// glycin's encoder API, on the driver thread (its own main context is what lets the
/// loader answer at all — `crate::driver`).
///
/// The pixels go over as one owned copy: a job crosses the thread boundary, so it
/// cannot borrow the caller's buffer. The encoded file comes back in one piece for
/// the same reason libheif writes it that way, and is then written through the
/// atomic path like the other two formats' streams — a failed AVIF export leaves the
/// file that was already there exactly as it was.
fn write_avif(writer: &mut BufWriter<File>, export: &Export<'_>) -> Result<(), Failure> {
    let width = export.image.width as u32;
    let height = export.image.height as u32;
    let pixels = export.image.data.to_vec();
    let encoded =
        driver::run(move |context| driver::block_on(&context, encode_avif(width, height, pixels)))
            .map_err(|error| Failure::Avif(error.to_string()))?;
    let encoded = encoded.map_err(Failure::Avif)?;
    writer.write_all(&encoded).map_err(Failure::Io)
}

/// One AVIF through glycin's encoder: pixels, the sRGB profile and the quality in
/// the one `create` call that writes the file.
async fn encode_avif(width: u32, height: u32, pixels: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut creator = Creator::new(MimeType::new("image/avif".to_string()))
        .await
        .map_err(avif_reason)?;
    creator
        .set_encoding_quality(AVIF_QUALITY)
        .map_err(|_| "the AVIF encoder does not take a quality setting".to_string())?;
    let frame = creator
        .add_frame(width, height, MemoryFormat::R8g8b8, pixels)
        .map_err(avif_reason)?;
    frame
        .set_color_icc_profile(Some(icc::srgb_profile().to_vec()))
        .map_err(|_| "the AVIF encoder does not take a colour profile".to_string())?;
    let encoded = creator.create().await.map_err(avif_reason)?;
    Ok(encoded.data_full())
}

/// glycin's own words for a failed encode, with the one case a user can act on
/// spelled out.
///
/// A machine without the heif loader answers `UnknownImageFormat`, whose own message
/// is a mime type followed by a debug dump of glycin's config; what a user needs
/// instead is what is missing and what installs it. Every other failure — a sandbox
/// that will not start, a loader that died — is glycin's text as it stands.
fn avif_reason(error: glycin::Error) -> String {
    if error.unsupported_format().is_some() {
        "this machine has no AVIF encoder: glycin's heif loader (with libheif) is what writes AVIF"
            .to_string()
    } else if error.has_no_processor_configured() {
        "no glycin loaders are installed".to_string()
    } else {
        error.to_string()
    }
}

/// What the encoder libraries report, before the path is known.
enum Failure {
    Png(png::EncodingError),
    Jpeg(jpeg_encoder::EncodingError),
    /// The AVIF encoder's own report, already a sentence ([`avif_reason`]), or the
    /// driver thread's.
    Avif(String),
    /// The destination writer, for the one format whose bytes arrive in one piece
    /// rather than through a library's own writer.
    Io(std::io::Error),
}

impl Failure {
    fn at(self, path: &Path) -> EncodeError {
        match self {
            Self::Png(source) => EncodeError::Png {
                path: path.to_path_buf(),
                source,
            },
            Self::Jpeg(source) => EncodeError::Jpeg {
                path: path.to_path_buf(),
                source,
            },
            Self::Avif(reason) => EncodeError::Avif {
                path: path.to_path_buf(),
                reason,
            },
            Self::Io(source) => EncodeError::Io {
                path: path.to_path_buf(),
                source,
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("cannot write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("image is {width}x{height}, which is not a size to encode")]
    Empty { width: i32, height: i32 },

    #[error("{width}x{height} needs {expected} bytes, the buffer holds {found}")]
    BufferSize {
        width: i32,
        height: i32,
        /// `width * height * 3` in 64-bit arithmetic, so the number in this message
        /// is the one the check compared against even for the largest grids.
        expected: u64,
        found: usize,
    },

    /// JPEG's size fields are 16 bits wide, so a larger grid has no representation
    /// in the file (S15c, PIX-024). Refused before anything is created.
    #[error("{width}x{height} is too large for JPEG, whose size fields hold 65535 px")]
    JpegSize { width: i32, height: i32 },

    #[error("{path}: cannot write PNG: {source}")]
    Png {
        path: PathBuf,
        #[source]
        source: png::EncodingError,
    },

    #[error("{path}: cannot write JPEG: {source}")]
    Jpeg {
        path: PathBuf,
        #[source]
        source: jpeg_encoder::EncodingError,
    },

    /// AVIF is written by a loader process rather than by a library of ours, so the
    /// failure is a sentence rather than a typed source: the encoder's own report,
    /// already mapped by [`avif_reason`], or the driver thread's.
    #[error("{path}: cannot write AVIF: {reason}")]
    Avif { path: PathBuf, reason: String },
}
