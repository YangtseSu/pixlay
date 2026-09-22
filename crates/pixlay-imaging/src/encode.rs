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
//!
//! What a file deliberately does **not** carry is a resolution (S12d): a raster's
//! only intrinsic size is its pixels, and the product has no concept of paper for
//! a density number to describe. A PNG has no `pHYs`, and the JPEG's JFIF density
//! stays at the encoder's default — square pixels, no unit.
//!
//! Two formats, not three: TIFF left with S12c (the purity ruling — PNG and JPEG
//! are what a collage is exported as), and with it the `tiff` dependency.
//!
//! The PNG `sRGB` chunk is deliberately **not** written alongside `iCCP`: the
//! specification says the two should not both be present, and the profile is the
//! one that carries the actual colorimetry.
//!
//! JPEG quality is 90, the S0/S4 baseline. Chroma subsampling is **4:4:4**, fixed
//! rather than chosen: `AGENTS.md` fixes libjpeg-turbo 4:4:4 as the product's
//! sampling, and S12c removed the `--chroma` flag that made it a request.

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::Rgb8View;
use crate::icc;

/// JPEG quality, as a percentage. 90 is fixed rather than a flag
/// (`docs/CONTRACT.md` §5), which is what keeps every measurement in §8 comparable.
pub const JPEG_QUALITY: u8 = 90;

/// What `--out`'s extension selects.
///
/// PNG and JPEG only: TIFF left with S12c, so an extension this build does not
/// write is a usage error rather than a silent fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    /// The format an output path names, or `None` for an extension this build
    /// does not write. The extension is the whole interface: a `.jpg` that is
    /// really a PNG is worse than a refusal.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }

    /// The extensions `from_path` accepts, for the usage message.
    pub const EXTENSIONS: &'static str = ".png, .jpg or .jpeg";
}

/// One image to write, with the metadata the file has to carry.
pub struct Export<'a> {
    pub format: Format,
    pub image: Rgb8View<'a>,
}

/// Writes `export` to `path` in one pass and returns the number of bytes written.
pub fn write(path: &Path, export: &Export<'_>) -> Result<u64, EncodeError> {
    let (width, height) = (export.image.width, export.image.height);
    if width <= 0 || height <= 0 {
        return Err(EncodeError::Empty { width, height });
    }
    let expected = width as usize * height as usize * 3;
    if export.image.data.len() != expected {
        return Err(EncodeError::BufferSize {
            width,
            height,
            found: export.image.data.len(),
        });
    }
    let file = File::create(path).map_err(|error| EncodeError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    // Buffered for the formats that write in many small pieces, flushed by hand
    // because `BufWriter`'s own drop swallows the error.
    let mut buffered = BufWriter::new(file);
    let result = match export.format {
        Format::Png => write_png(&mut buffered, export),
        Format::Jpeg => write_jpeg(&mut buffered, export),
    };
    result.map_err(|error| error.at(path))?;
    buffered.flush().map_err(|error| EncodeError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    let bytes = std::fs::metadata(path)
        .map_err(|error| EncodeError::Io {
            path: path.to_path_buf(),
            source: error,
        })?
        .len();
    Ok(bytes)
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
    encoder
        .encode(
            export.image.data,
            export.image.width as u16,
            export.image.height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(Failure::Jpeg)
}

/// What the encoder libraries report, before the path is known.
enum Failure {
    Png(png::EncodingError),
    Jpeg(jpeg_encoder::EncodingError),
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

    #[error("{width}x{height} needs {} bytes, the buffer holds {found}", width * height * 3)]
    BufferSize {
        width: i32,
        height: i32,
        found: usize,
    },

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
}
