//! Decoding a source photo into straight, upright, sRGB samples.
//!
//! What the decoder is asked for, and why:
//!
//! * **Upright.** glycin applies the EXIF orientation to the pixels and reports
//!   which rotation it applied; the row axis of a rotated photo is therefore
//!   already the displayed one, and no layer above this one has to know that
//!   orientation exists. `apply_transformations` is left at its default (on).
//! * **sRGB.** The source's own ICC profile is converted to sRGB by the loader
//!   (`color_convert_icc_srgb`). Measured 2026-09-21 against ImageMagick: an
//!   Adobe RGB file converts to a mean of 169.10/149.11/88.12 either way, while
//!   not converting leaves 178.7/149.6/95.2 — a visible error, for free.
//! * **Straight, not premultiplied.** The pipeline premultiplies in linear light
//!   itself, after the transfer function, which is the only place the operation
//!   is lossless for a resampler.
//! * **At the source's own depth.** A 16-bit PNG or a 12-bit HEIC arrives as
//!   16-bit samples and stays 16-bit; nothing is narrowed at decode time. The
//!   `Samples` enum is that: the depth is a property of the file.
//! * **Capped.** `DecodeLimits` bounds both the pixel area and the time, because
//!   a decoder is the one stage that can be handed a decompression bomb or a
//!   loader that never answers.

use std::path::{Path, PathBuf};
use std::time::Duration;

use glycin::{Loader, MemoryFormat, MemoryFormatSelection};

use crate::driver;
use crate::error::ImagingError;

/// Largest source image the pipeline decodes, in pixels.
///
/// The ladder in `docs/CONTRACT.md` §4 sizes the decoded buffer: a source is
/// RGBA at its own depth, so 120 MP is 480 MB as 8-bit and 960 MB as 16-bit.
/// With the slot bitmaps (bounded by the output) and one output surface that
/// stays inside the 2.5 GB budget `AGENTS.md` carries from S0. A panorama or a
/// 150 MP medium-format scan is refused with a message rather than decoded.
pub const MAX_DECODE_PIXELS: u64 = 120_000_000;

/// Largest single edge the loader is allowed to hand over, in pixels.
///
/// A second, coarse guard in front of the area cap: it stops a decompression bomb
/// (a 100000x100000 header) before the loader allocates anything, while being
/// wide enough for a 20000x2000 panorama the area cap would allow.
pub const MAX_DECODE_EDGE: u32 = 20_000;

/// How long one decode may take.
///
/// glycin's own default is 60 seconds, which is far too long for a command-line
/// loop that may be decoding ten photos; 20 s is longer than any decode measured
/// here (the slowest was 110 ms for a 12-bit HEIC that had to be woken up) and
/// short enough that a wedged loader is reported as an error.
pub const DECODE_TIMEOUT: Duration = Duration::from_secs(20);

/// What a decode is allowed to spend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Largest source, in pixels.
    pub max_pixels: u64,
    /// Largest single edge.
    pub max_edge: u32,
    /// Longest a single decode may take.
    pub timeout: Duration,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_pixels: MAX_DECODE_PIXELS,
            max_edge: MAX_DECODE_EDGE,
            timeout: DECODE_TIMEOUT,
        }
    }
}

/// How many bits per channel the file carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Eight,
    Sixteen,
}

/// A decoded photo: upright, straight, sRGB, RGBA, at the file's own depth.
#[derive(Clone, Debug)]
pub struct Source {
    width: u32,
    height: u32,
    depth: Depth,
    /// `width * height * 4` samples. 8-bit: one byte per sample. 16-bit: native
    /// endianness, two bytes per sample.
    data: Vec<u8>,
    mime: String,
    exif: Option<Vec<u8>>,
}

impl Source {
    /// Decodes `path` with the default limits.
    pub fn decode(path: &Path) -> Result<Self, ImagingError> {
        Self::decode_with(path, &DecodeLimits::default())
    }

    /// Decodes `path` on the decoder thread.
    ///
    /// Every failure names the path: the caller is a command line or a slot in a
    /// document, and "which file" is the only part of a decode failure the user
    /// can act on.
    pub fn decode_with(path: &Path, limits: &DecodeLimits) -> Result<Self, ImagingError> {
        if !path.is_file() {
            return Err(ImagingError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            });
        }
        let limits = *limits;
        let path: PathBuf = path.to_path_buf();
        driver::run(move |context| driver::block_on(&context, load(&path, &limits)))?
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn depth(&self) -> Depth {
        self.depth
    }

    /// How many bytes these samples occupy — what a cache holding one decoded
    /// photo costs (`crate::preview` budgets its sources in exactly this).
    pub(crate) fn bytes(&self) -> usize {
        self.data.len()
    }

    /// The file's own MIME type, as the loader detected it (`image/jpeg`).
    pub fn mime(&self) -> &str {
        &self.mime
    }

    /// The raw EXIF block, when the file carries one.
    ///
    /// The orientation in it has already been applied to the pixels; S5 reads the
    /// `{date}` field out of this block (`crate::exif`).
    pub fn exif(&self) -> Option<&[u8]> {
        self.exif.as_deref()
    }

    /// `width / height` of the decoded pixels, i.e. after any EXIF rotation.
    pub fn aspect(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }

    /// One pixel as straight sRGB, normalized to 16 bits per channel.
    ///
    /// 8-bit samples are widened by `* 257`, which is exact: `0xab * 257 =
    /// 0xabab`, so the full-scale endpoints and every step in between survive.
    pub fn pixel(&self, x: u32, y: u32) -> [u16; 4] {
        // The index is in *samples*, and a sample is one byte or two: getting
        // that factor wrong reads a neighbouring row, which is what a test that
        // compares the two depths against each other catches.
        let sample = y as usize * self.width as usize + x as usize;
        match self.depth {
            Depth::Eight => {
                let at = sample * 4;
                let p = &self.data[at..at + 4];
                [
                    u16::from(p[0]) * 257,
                    u16::from(p[1]) * 257,
                    u16::from(p[2]) * 257,
                    u16::from(p[3]) * 257,
                ]
            }
            Depth::Sixteen => {
                let at = sample * 8;
                let mut out = [0u16; 4];
                for (channel, slot) in out.iter_mut().enumerate() {
                    let byte = at + channel * 2;
                    *slot = u16::from_ne_bytes([self.data[byte], self.data[byte + 1]]);
                }
                out
            }
        }
    }
}

/// Straight sRGB samples, the resampler's input.
///
/// A trait rather than a concrete type so the resampler can be measured on
/// content that no decoder produces — the zone plate the aliasing criterion is
/// about is computed analytically, and computing a 16 MP plate to throw it away
/// would be work for nothing. `Source` is the production implementation; a test
/// is the other one.
pub trait Sampler {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    /// One pixel, straight sRGB, 16 bits per channel.
    fn pixel(&self, x: u32, y: u32) -> [u16; 4];

    /// The file's EXIF block, when there is one.
    ///
    /// Text layers substitute `{date}` from it (S5, `pixlay-render`): asking the
    /// decoder that already has the file beats decoding it a second time, and a
    /// synthetic sampler — the probe's flat content — has no file and says `None`.
    fn exif(&self) -> Option<&[u8]> {
        None
    }

    /// `width / height`.
    fn aspect(&self) -> f64 {
        f64::from(self.width()) / f64::from(self.height())
    }
}

impl Sampler for Source {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn pixel(&self, x: u32, y: u32) -> [u16; 4] {
        Source::pixel(self, x, y)
    }

    fn exif(&self) -> Option<&[u8]> {
        self.exif.as_deref()
    }
}

/// One real decode, on the decoder thread with its context being iterated.
async fn load(path: &Path, limits: &DecodeLimits) -> Result<Source, ImagingError> {
    let file = gio::File::for_path(path);
    let mut loader = Loader::new(file);
    loader
        // Straight sample layouts only: premultiplied input would have to be
        // undone before the resampler premultiplies in linear light, and a
        // premultiplied 8-bit source cannot be un-premultiplied losslessly.
        .accepted_memory_formats(
            MemoryFormatSelection::R8g8b8
                | MemoryFormatSelection::R8g8b8a8
                | MemoryFormatSelection::R16g16b16
                | MemoryFormatSelection::R16g16b16a16,
        )
        // The source ICC is converted to sRGB by the loader (see the module docs).
        .color_convert_icc_srgb(true)
        .limits(
            glycin::Limits::default()
                .timeout(limits.timeout)
                .max_dimensions((limits.max_edge, limits.max_edge)),
        );

    let mut image = loader.load().await.map_err(|error| ImagingError::Decode {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let details = image.details();
    let pixels = u64::from(details.width()) * u64::from(details.height());
    if pixels > limits.max_pixels {
        // Checked between the metadata and the frame: the loader has the header
        // but has not decoded the pixels yet, so the refusal costs nothing.
        return Err(ImagingError::TooLarge {
            path: path.to_path_buf(),
            pixels,
            max: limits.max_pixels,
        });
    }

    let mime = image.mime_type().as_str().to_string();
    let exif = details.metadata_exif().map(<[u8]>::to_vec);
    let frame = image
        .next_frame()
        .await
        .map_err(|error| ImagingError::Decode {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

    let (width, height) = (frame.width(), frame.height());
    let stride = frame.stride() as usize;
    let bytes = frame.buf_slice();
    let (channels, depth) = match frame.memory_format() {
        MemoryFormat::R8g8b8 => (3, Depth::Eight),
        MemoryFormat::R8g8b8a8 => (4, Depth::Eight),
        MemoryFormat::R16g16b16 => (3, Depth::Sixteen),
        MemoryFormat::R16g16b16a16 => (4, Depth::Sixteen),
        other => {
            return Err(ImagingError::Decode {
                path: path.to_path_buf(),
                message: format!(
                    "the loader returned {other:?}, which this pipeline does not accept"
                ),
            });
        }
    };
    let sample_bytes = if depth == Depth::Eight { 1 } else { 2 };
    let opaque: [u8; 2] = if depth == Depth::Eight {
        [u8::MAX, 0]
    } else {
        [u8::MAX, u8::MAX]
    };
    let mut data = vec![0u8; width as usize * height as usize * 4 * sample_bytes];
    for y in 0..height as usize {
        let src_row = &bytes[y * stride..];
        let dst_row = &mut data[y * width as usize * 4 * sample_bytes..];
        for x in 0..width as usize {
            let src = &src_row[x * channels * sample_bytes..];
            let dst = &mut dst_row[x * 4 * sample_bytes..];
            // Copy the channels the file has; a file without alpha is opaque.
            dst[..channels * sample_bytes].copy_from_slice(&src[..channels * sample_bytes]);
            if channels == 3 {
                dst[3 * sample_bytes..4 * sample_bytes].copy_from_slice(&opaque[..sample_bytes]);
            }
        }
    }

    Ok(Source {
        width,
        height,
        depth,
        data,
        mime,
        exif,
    })
}
