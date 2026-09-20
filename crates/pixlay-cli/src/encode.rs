//! Output encoding.
//!
//! Interim: this uses the `image` crate's PNG and JPEG writers. S6 replaces it
//! with an encoder we control, because chroma sampling, ICC and DPI have to be
//! written in the same pass as the pixels — Cairo's PNG writer drops pHYs and
//! iCCP entirely, which is why this path exists at all.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use pixlay_render::Rgb8Image;

/// JPEG quality until S6 owns the encoder. 90 matches the S0 baseline.
const JPEG_QUALITY: u8 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    /// Format from the output file's extension; `None` for anything else.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
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
}

/// Writes `image` to `path` and returns the number of bytes written.
///
/// The caller owns the path: a missing parent directory is an error that names
/// the file, not something to paper over by creating directories.
pub fn write(path: &Path, format: Format, image: &Rgb8Image) -> Result<u64, String> {
    let file =
        File::create(path).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let (width, height) = (image.width as u32, image.height as u32);
    match format {
        Format::Png => PngEncoder::new(&mut writer).write_image(
            &image.data,
            width,
            height,
            ExtendedColorType::Rgb8,
        ),
        Format::Jpeg => JpegEncoder::new_with_quality(&mut writer, JPEG_QUALITY).encode(
            &image.data,
            width,
            height,
            ExtendedColorType::Rgb8,
        ),
    }
    .map_err(|error| format!("cannot encode {}: {error}", path.display()))?;
    writer
        .flush()
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    let bytes = std::fs::metadata(path)
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))?
        .len();
    Ok(bytes)
}
