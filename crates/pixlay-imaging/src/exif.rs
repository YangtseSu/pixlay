//! The one EXIF field the product reads out of a photo: when it was taken.
//!
//! S5's text layers substitute `{date}`, and the contract fixes the semantics: the
//! value of EXIF `DateTimeOriginal` **verbatim, with no timezone conversion**
//! (`docs/CONTRACT.md` §1, "Text layers"). That is why this module
//! returns a `String` and not a date type: parsing it would mean choosing a
//! timezone, and the ruling is that there is no choice to make.
//!
//! Orientation is deliberately *not* read here. glycin reports which rotation it
//! applied and has already applied it to the pixels (`crate::decode`), so a second
//! reader of the same tag would only be a second opinion — the one thing that
//! could disagree with the decoder about which way up a photo is.
//!
//! The parser is a plain TIFF/IFD walk over the EXIF block: tag `0x9003`, type
//! ASCII, count-bounded. It reads only what it needs and never allocates a copy of
//! the block.

/// The EXIF tag whose value `{date}` renders.
pub const DATE_TIME_ORIGINAL: u16 = 0x9003;

/// `DateTimeOriginal` as EXIF stores it (`2019:07:14 10:32:00`), verbatim.
///
/// `None` when the file has no EXIF block, no such tag, or a value that is not
/// the format EXIF specifies — the caller then falls back to the project's own
/// string (contract §1, `textFallback`).
pub fn date_time_original(exif: &[u8]) -> Option<String> {
    let value = ascii_tag(exif, DATE_TIME_ORIGINAL)?;
    // EXIF writes `YYYY:MM:DD HH:MM:SS`; anything else is treated as absent rather
    // than rendered as-is, because a wrong-looking date in a watermark is worse
    // than the documented fallback.
    let bytes = value.as_bytes();
    if bytes.len() < 19
        || bytes[4] != b':'
        || bytes[7] != b':'
        || bytes[10] != b' '
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !bytes[..19]
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7 | 10 | 13 | 16) || byte.is_ascii_digit())
    {
        return None;
    }
    Some(value[..19].to_string())
}

/// The ASCII value of `tag` from an EXIF block, without the NUL terminator.
fn ascii_tag(exif: &[u8], tag: u16) -> Option<String> {
    // The block glycin hands over starts at "Exif\0\0"; some files carry only the
    // TIFF header. Both are accepted, because both appear in the wild.
    let tiff = if exif.starts_with(b"Exif\0\0") { 6 } else { 0 };
    let header = exif.get(tiff..tiff + 8)?;
    let little_endian = match &header[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read16 = |at: usize| -> Option<u16> {
        let pair = exif.get(tiff + at..tiff + at + 2)?;
        Some(if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        })
    };
    let read32 = |at: usize| -> Option<u32> {
        let quad = exif.get(tiff + at..tiff + at + 4)?;
        let bytes = [quad[0], quad[1], quad[2], quad[3]];
        Some(if little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };

    let ifd = read32(4)? as usize;
    let count = read16(ifd)? as usize;
    for index in 0..count {
        let entry = ifd + 2 + index * 12;
        if read16(entry)? != tag {
            continue;
        }
        // Type 2 is ASCII, and the count includes the terminating NUL.
        if read16(entry + 2)? != 2 {
            return None;
        }
        let length = read32(entry + 4)? as usize;
        if length == 0 {
            return None;
        }
        // A value of four bytes or fewer is stored in the entry itself, at
        // `entry + 8`; a longer one is at the offset written there. Both are
        // relative to the TIFF header.
        let offset = if length <= 4 {
            entry + 8
        } else {
            read32(entry + 8)? as usize
        };
        let bytes = exif.get(tiff + offset..tiff + offset + length)?;
        let text = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
        return std::str::from_utf8(text).ok().map(str::to_string);
    }
    None
}
