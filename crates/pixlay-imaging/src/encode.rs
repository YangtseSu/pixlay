//! Output encoding: pixels and metadata in one pass.
//!
//! A collage is exported to be printed, so the file has to say what it is: the
//! resolution it was rendered for and the colour space it is in. Both are written
//! by the encoder while the pixels go out, never by a second pass over the
//! finished file — the trap this rule exists for is measured: re-encoding an
//! already-encoded JPEG to patch its metadata silently drops 4:4:4 to 4:2:0
//! (2.71 MB to 1.49 MB, S0). Cairo can supply the pixels (`pixlay-render`) but not
//! the metadata: `cairo_surface_write_to_png` emits only IHDR/bKGD/IDAT — no pHYs
//! and no iCCP — so an export written through it necessarily loses its DPI.
//!
//! # What each format carries
//!
//! | Format | Resolution | Profile |
//! |---|---|---|
//! | PNG | `pHYs`, pixels per metre | `iCCP` (deflate) |
//! | JPEG | JFIF `APP0` density, pixels per inch | `APP2` `ICC_PROFILE` segments |
//! | TIFF | `XResolution` / `YResolution`, unit 2 (inch) | tag 34675 |
//!
//! The PNG `sRGB` chunk is deliberately **not** written alongside `iCCP`: the
//! specification says the two should not both be present, and the profile is the
//! one that carries the actual colorimetry.
//!
//! # Rounding rules
//!
//! Written once and frozen, so an export's metadata cannot drift between builds:
//!
//! * PNG: `round(dpi * 1000 / 25.4)` pixels per metre (the chunk's own unit).
//! * JPEG: `round(dpi)` as a 16-bit number of pixels per inch. A resolution past
//!   65535 dpi cannot be written into JFIF and is refused rather than saturated.
//! * TIFF: `round(dpi * 100) / 100` as a rational, so a fractional resolution
//!   (the one a pixel-count export derives) survives.
//!
//! JPEG quality is 90, the S0/S4 baseline. Chroma subsampling is the caller's
//! choice and 4:4:4 by default (`AGENTS.md`: the default is libjpeg-turbo 4:4:4).

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

/// The resolution JFIF can hold: 16 bits of pixels per inch.
const MAX_JPEG_DPI: u32 = 65_535;

/// What `--out`'s extension selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Tiff,
}

impl Format {
    /// The format an output path names, or `None` for an extension this build
    /// does not write. The extension is the whole interface: a `.jpg` that is
    /// really a PNG is worse than a refusal.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "tif" | "tiff" => Some(Self::Tiff),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Tiff => "tiff",
        }
    }

    /// The extensions `from_path` accepts, for the usage message.
    pub const EXTENSIONS: &'static str = ".png, .jpg, .jpeg, .tif or .tiff";
}

/// How a JPEG encodes colour relative to luminance.
///
/// The names are the conventional `J:a:b` notation; `Chroma::Full` is what
/// `AGENTS.md` fixes as the default. Each value maps to one sampling factor of
/// the encoded frame, so the request is visible in the file's own `SOF0` header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Chroma {
    /// 4:4:4 — three samples per pixel, no subsampling.
    #[default]
    Full,
    /// 4:2:2 — chroma halved horizontally.
    HorizontalHalf,
    /// 4:2:0 — chroma halved in both directions.
    Quarter,
}

impl Chroma {
    /// The notation the flag and the report use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "444",
            Self::HorizontalHalf => "422",
            Self::Quarter => "420",
        }
    }

    /// Parses the notation the flag takes.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "444" => Some(Self::Full),
            "422" => Some(Self::HorizontalHalf),
            "420" => Some(Self::Quarter),
            _ => None,
        }
    }
}

/// One image to write, with everything the file has to say about itself.
pub struct Export<'a> {
    pub format: Format,
    /// Resolution written into the file, both axes, in pixels per inch.
    pub dpi: f64,
    /// JPEG chroma subsampling; ignored by the two lossless formats, which store
    /// three samples per pixel by construction.
    pub chroma: Chroma,
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
    if export.format == Format::Jpeg
        && (!export.dpi.is_finite()
            || export.dpi <= 0.0
            || export.dpi.round() > f64::from(MAX_JPEG_DPI))
    {
        // Refused before the file exists: a rejected export leaves nothing behind,
        // the same discipline `init` follows for an existing project.
        return Err(EncodeError::JpegResolution {
            path: path.to_path_buf(),
            dpi: export.dpi,
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
        Format::Tiff => write_tiff(&mut buffered, export),
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
    info.pixel_dims = Some(png::PixelDimensions {
        xppu: pixels_per_metre(export.dpi),
        yppu: pixels_per_metre(export.dpi),
        unit: png::Unit::Meter,
    });
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
    let density = export.dpi.round() as u16;
    let mut encoder = jpeg_encoder::Encoder::new(writer, JPEG_QUALITY);
    encoder.set_density(jpeg_encoder::PixelDensity::dpi(density));
    encoder.set_sampling_factor(match export.chroma {
        Chroma::Full => jpeg_encoder::SamplingFactor::R_4_4_4,
        Chroma::HorizontalHalf => jpeg_encoder::SamplingFactor::R_4_2_2,
        Chroma::Quarter => jpeg_encoder::SamplingFactor::R_4_2_0,
    });
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

fn write_tiff(writer: &mut BufWriter<File>, export: &Export<'_>) -> Result<(), Failure> {
    // LZW + the horizontal predictor: what the S0 baseline measured for TIFF
    // (ImageMagick's LZW), and the compression every TIFF reader understands.
    let mut encoder = tiff::encoder::TiffEncoder::new(writer)
        .map_err(Failure::Tiff)?
        .with_compression(tiff::encoder::Compression::Lzw)
        .with_predictor(tiff::encoder::Predictor::Horizontal);
    let mut image = encoder
        .new_image::<tiff::encoder::colortype::RGB8>(
            export.image.width as u32,
            export.image.height as u32,
        )
        .map_err(Failure::Tiff)?;
    // The rational keeps a derived resolution's fraction; `XResolution` and
    // `YResolution` are set per axis even though both carry the same number.
    let resolution = tiff::encoder::Rational {
        n: (export.dpi * 100.0).round() as u32,
        d: 100,
    };
    image.resolution(tiff::tags::ResolutionUnit::Inch, resolution);
    // Tag 34675 is typed `UNDEFINED`, which `write_tag` cannot express for a
    // payload this size (a byte slice's `TiffValue` type is `BYTE`): the data is
    // written first and the entry is built from where it landed.
    let icc = image
        .encoder()
        .write_entry_bytes(tiff::tags::Type::UNDEFINED, icc::srgb_profile())
        .map_err(Failure::Tiff)?;
    let mut directory = tiff::Directory::empty();
    directory.extend([(tiff::tags::Tag::Unknown(34_675), icc)]);
    image.encoder().extend_from(&directory);
    image.write_data(export.image.data).map_err(Failure::Tiff)
}

/// The PNG `pHYs` unit is the metre: `round(dpi * 1000 / 25.4)`, as the module
/// docs freeze it.
fn pixels_per_metre(dpi: f64) -> u32 {
    (dpi * 1000.0 / 25.4)
        .round()
        .clamp(0.0, f64::from(u32::MAX)) as u32
}

/// What the encoder libraries report, before the path is known.
enum Failure {
    Png(png::EncodingError),
    Jpeg(jpeg_encoder::EncodingError),
    Tiff(tiff::TiffError),
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
            Self::Tiff(source) => EncodeError::Tiff {
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

    #[error("{path}: cannot write TIFF: {source}")]
    Tiff {
        path: PathBuf,
        #[source]
        source: tiff::TiffError,
    },

    #[error(
        "{path}: resolution {dpi} cannot be written into a JPEG, whose JFIF header stores at \
         most {MAX_JPEG_DPI} pixels per inch"
    )]
    JpegResolution { path: PathBuf, dpi: f64 },
}
