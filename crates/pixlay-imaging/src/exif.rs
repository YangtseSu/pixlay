//! The one EXIF field the product reads out of a photo: when it was taken.
//!
//! It was written for S5's `{date}` text layer, which S12c removed; what is left
//! is the machine surface — `scan` and `image` report the date next to a photo's
//! size and mime type, so a folder browser can show it and a caller can sort by it. The
//! contract fixes the semantics anyway: the value of EXIF `DateTimeOriginal`
//! **verbatim, with no timezone conversion** (`docs/CONTRACT.md` §5, "`image`").
//! That is why this module returns a `String` and not a date type: parsing it
//! would mean choosing a timezone, and there is no choice to make.
//!
//! Orientation is deliberately *not* read here. glycin reports which rotation it
//! applied and has already applied it to the pixels (`crate::decode`), so a second
//! reader of the same tag would only be a second opinion — the one thing that
//! could disagree with the decoder about which way up a photo is.
//!
//! The parser is a plain TIFF/IFD walk over the EXIF block: tag `0x9003`, type
//! ASCII, count-bounded. **The tag belongs to the Exif SubIFD**, which IFD0 reaches
//! through its own `0x8769` pointer (EXIF 2.32 §4.6.5: `0x9003`'s IFD is the Exif
//! IFD and `0x8769`'s is IFD0) — that is where a camera writes it. A walk that
//! scanned IFD0 alone therefore found the date in the synthetic fixture and missed
//! it in a real photo (PIX-023, S15i). IFD0 stays as the fallback, because writers
//! that put the tag there exist (Pillow's `exif[0x9003]`, which wrote the committed
//! `dated.jpg`) and the fallback is one bounds-checked lookup. It reads only what
//! it needs and never allocates a copy of the block.

/// The EXIF tag this module reads (`DateTimeOriginal`, an Exif-IFD tag).
pub const DATE_TIME_ORIGINAL: u16 = 0x9003;

/// IFD0's pointer at the Exif SubIFD, where [`DATE_TIME_ORIGINAL`] belongs.
pub const EXIF_IFD_POINTER: u16 = 0x8769;

/// `DateTimeOriginal` as EXIF stores it (`2019:07:14 10:32:00`), verbatim.
///
/// `None` when the file has no EXIF block, no such tag, or a value that is not the
/// format EXIF specifies — a listing then simply has no date for that row.
pub fn date_time_original(exif: &[u8]) -> Option<String> {
    let tiff = Tiff::new(exif)?;
    // The standard place first, IFD0 second: the two can disagree (a writer that
    // moves the tag and leaves a stale copy behind), and the standard is what a
    // camera meant.
    let value = tiff
        .exif_ifd()
        .and_then(|ifd| tiff.ascii_value(ifd, DATE_TIME_ORIGINAL))
        .or_else(|| tiff.ascii_value(tiff.ifd0()?, DATE_TIME_ORIGINAL))?;
    // EXIF writes `YYYY:MM:DD HH:MM:SS`; anything else is treated as absent rather
    // than reported as-is, because a wrong-looking date in a listing is worse than
    // an empty field.
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

/// The EXIF block as the TIFF header it is: every offset in it — IFD0's, the
/// SubIFD's, an entry's data — is relative to the header, so the block is borrowed
/// once and read through these accessors.
///
/// Nothing here may panic on a malformed block: an offset is data from a file, so
/// every read is a `get` and every address arithmetic step is checked.
struct Tiff<'a> {
    bytes: &'a [u8],
    little_endian: bool,
}

impl<'a> Tiff<'a> {
    /// The block, or `None` when it does not start with either the preamble glycin
    /// hands over or a TIFF header.
    fn new(exif: &'a [u8]) -> Option<Self> {
        // The block starts at "Exif\0\0"; some files carry only the header. Both
        // are accepted, because both appear in the wild.
        let bytes = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
        let little_endian = match bytes.get(..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        // 2..4 the answer to "is this TIFF" (42), 4..8 where IFD0 is.
        bytes.get(2..8)?;
        Some(Self {
            bytes,
            little_endian,
        })
    }

    fn read16(&self, at: usize) -> Option<u16> {
        let pair = self.bytes.get(at..at.checked_add(2)?)?;
        Some(if self.little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        })
    }

    fn read32(&self, at: usize) -> Option<u32> {
        let quad = self.bytes.get(at..at.checked_add(4)?)?;
        let bytes = [quad[0], quad[1], quad[2], quad[3]];
        Some(if self.little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    /// Where IFD0 is, as the header declares it.
    fn ifd0(&self) -> Option<usize> {
        Some(self.read32(4)? as usize)
    }

    /// The offset of `tag`'s entry in the IFD at `ifd`, or `None`.
    ///
    /// An IFD is `count`, then `count` 12-byte entries; a count the block cannot
    /// hold is a read that fails rather than an iterator that runs off the end.
    fn entry(&self, ifd: usize, tag: u16) -> Option<usize> {
        let count = self.read16(ifd)? as usize;
        for index in 0..count {
            let entry = ifd.checked_add(2 + index.checked_mul(12)?)?;
            if self.read16(entry)? == tag {
                return Some(entry);
            }
        }
        None
    }

    /// The Exif SubIFD, through IFD0's `0x8769` pointer.
    ///
    /// A missing, mistyped or dangling pointer is `None` rather than an error: the
    /// caller's fallback is IFD0 itself.
    fn exif_ifd(&self) -> Option<usize> {
        let entry = self.entry(self.ifd0()?, EXIF_IFD_POINTER)?;
        // The type is LONG (4) or IFD (13), one value — four bytes, so it is stored
        // in the entry's own value field.
        if !matches!(self.read16(entry + 2)?, 4 | 13) || self.read32(entry + 4)? != 1 {
            return None;
        }
        Some(self.read32(entry + 8)? as usize)
    }

    /// The ASCII value of `tag` in the IFD at `ifd`, without the NUL terminator.
    fn ascii_value(&self, ifd: usize, tag: u16) -> Option<String> {
        let entry = self.entry(ifd, tag)?;
        // Type 2 is ASCII, and the count includes the terminating NUL.
        if self.read16(entry + 2)? != 2 {
            return None;
        }
        let length = self.read32(entry + 4)? as usize;
        if length == 0 {
            return None;
        }
        // A value of four bytes or fewer is stored in the entry itself, at
        // `entry + 8`; a longer one is at the offset written there.
        let offset = if length <= 4 {
            entry + 8
        } else {
            self.read32(entry + 8)? as usize
        };
        let bytes = self.bytes.get(offset..offset.checked_add(length)?)?;
        let text = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
        std::str::from_utf8(text).ok().map(str::to_string)
    }
}

/// TIFF/EXIF blocks written by hand, so the tests do not depend on a camera, on
/// another library's idea of where the tag goes, or on the fixture generator
/// (`crates/pixlay-cli/tests/fixtures/generate.py`): Pillow puts a plain
/// `exif[0x9003]` in IFD0, and that is exactly the shape that hid PIX-023.
#[cfg(test)]
mod tests {
    use super::*;

    /// The date a hand-built block's SubIFD carries, and what a caller sees.
    const DATE: &str = "2019:07:14 10:32:00";
    /// A second date, for the case where IFD0 and the SubIFD disagree.
    const OTHER: &str = "2024:01:02 03:04:05";

    /// Where a built block put the entries a malformed case overwrites, as the
    /// position of each entry's first byte; its fields are at `+2` (type), `+4`
    /// (count) and `+8` (value).
    struct At {
        /// IFD0's `0x8769` entry.
        pointer: usize,
        /// The `0x9003` entry of the IFD that carries the date.
        date: usize,
    }

    fn put16(bytes: &mut Vec<u8>, little: bool, value: u16) {
        bytes.extend_from_slice(&if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }

    fn put32(bytes: &mut Vec<u8>, little: bool, value: u32) {
        bytes.extend_from_slice(&if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }

    fn set16(bytes: &mut [u8], little: bool, at: usize, value: u16) {
        let word = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        bytes[at..at + 2].copy_from_slice(&word);
    }

    fn set32(bytes: &mut [u8], little: bool, at: usize, value: u32) {
        let word = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        bytes[at..at + 4].copy_from_slice(&word);
    }

    /// A TIFF header whose IFD0 is at offset 8, the usual arrangement.
    fn header(little: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(if little { b"II" } else { b"MM" });
        put16(&mut bytes, little, 42);
        put32(&mut bytes, little, 8);
        bytes
    }

    /// One 12-byte entry, its 4-byte value field left zero; returns where the entry
    /// starts, since a builder usually computes the value afterwards.
    fn entry(bytes: &mut Vec<u8>, little: bool, tag: u16, type_: u16, count: u32) -> usize {
        let at = bytes.len();
        put16(bytes, little, tag);
        put16(bytes, little, type_);
        put32(bytes, little, count);
        put32(bytes, little, 0);
        at
    }

    /// A string at the block's end, and the offset it landed at.
    fn push_text(bytes: &mut Vec<u8>, text: &str) -> u32 {
        let at = bytes.len() as u32;
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
        at
    }

    /// IFD0 at 8 with one `0x8769` entry pointing at an Exif SubIFD that carries
    /// `date` — the shape a camera writes. `ifd0_date` adds IFD0's own copy of the
    /// tag, which is the shape an editor that moves a tag can leave behind.
    fn block(little: bool, date: &str, ifd0_date: Option<&str>) -> (Vec<u8>, At) {
        let mut bytes = header(little);
        let entries = 1 + usize::from(ifd0_date.is_some());
        put16(&mut bytes, little, entries as u16);
        let pointer = entry(&mut bytes, little, EXIF_IFD_POINTER, 4, 1);
        let mut ifd0_date_value = None;
        if let Some(ifd0_date) = ifd0_date {
            let at = entry(
                &mut bytes,
                little,
                DATE_TIME_ORIGINAL,
                2,
                ifd0_date.len() as u32 + 1,
            );
            ifd0_date_value = Some(at + 8);
        }
        put32(&mut bytes, little, 0); // no next IFD

        let sub_ifd = bytes.len() as u32;
        set32(&mut bytes, little, pointer + 8, sub_ifd);
        put16(&mut bytes, little, 1);
        let date_at = entry(
            &mut bytes,
            little,
            DATE_TIME_ORIGINAL,
            2,
            date.len() as u32 + 1,
        );
        put32(&mut bytes, little, 0); // the SubIFD's own next-IFD field

        if let Some(value) = ifd0_date_value {
            let at = push_text(&mut bytes, ifd0_date.expect("the entry exists"));
            set32(&mut bytes, little, value, at);
        }
        let at = push_text(&mut bytes, date);
        set32(&mut bytes, little, date_at + 8, at);
        (
            bytes,
            At {
                pointer,
                date: date_at,
            },
        )
    }

    /// IFD0 carrying the date itself and no pointer: Pillow's `exif[0x9003]`, the
    /// shape the committed `dated.jpg` has.
    fn ifd0_block(little: bool, date: &str) -> Vec<u8> {
        let mut bytes = header(little);
        put16(&mut bytes, little, 1);
        let at = entry(
            &mut bytes,
            little,
            DATE_TIME_ORIGINAL,
            2,
            date.len() as u32 + 1,
        );
        put32(&mut bytes, little, 0);
        let text = push_text(&mut bytes, date);
        set32(&mut bytes, little, at + 8, text);
        bytes
    }

    #[test]
    fn the_date_is_read_through_the_sub_ifd_pointer_in_both_byte_orders() {
        for little in [true, false] {
            let (bytes, _) = block(little, DATE, None);
            assert_eq!(
                date_time_original(&bytes).as_deref(),
                Some(DATE),
                "little endian: {little}"
            );
            // The block as it sits in a file, behind the "Exif\0\0" preamble glycin
            // hands over.
            let mut prefixed = b"Exif\0\0".to_vec();
            prefixed.extend_from_slice(&bytes);
            assert_eq!(date_time_original(&prefixed).as_deref(), Some(DATE));
        }
    }

    #[test]
    fn ifd0_is_the_fallback_and_the_sub_ifd_wins_when_both_carry_a_date() {
        for little in [true, false] {
            assert_eq!(
                date_time_original(&ifd0_block(little, DATE)).as_deref(),
                Some(DATE),
                "a tag in IFD0 only: little endian {little}"
            );
            // Both IFDs carry one and they disagree: the SubIFD's is the answer.
            let (both, _) = block(little, DATE, Some(OTHER));
            assert_ne!(DATE, OTHER);
            assert_eq!(
                date_time_original(&both).as_deref(),
                Some(DATE),
                "the standard place wins: little endian {little}"
            );
        }
    }

    #[test]
    fn a_dangling_pointer_is_no_date_rather_than_a_failure() {
        for pointer in [0, 4, 0xFFFF_FFFF] {
            let (bytes, at) = block(true, DATE, None);
            let mut damaged = bytes.clone();
            set32(&mut damaged, true, at.pointer + 8, pointer);
            assert_eq!(
                date_time_original(&damaged),
                None,
                "pointer {pointer:#x} into a block of {} bytes",
                bytes.len()
            );
        }
        // The pointer is right but its type is not a pointer's: EXIF's `0x8769` is
        // LONG or IFD, and an ASCII field that happens to hold an offset is not a
        // pointer.
        let (mut bytes, at) = block(true, DATE, None);
        set16(&mut bytes, true, at.pointer + 2, 2);
        assert_eq!(date_time_original(&bytes), None);
        // And a pointer of the right type declaring more than one value.
        let (mut bytes, at) = block(true, DATE, None);
        set32(&mut bytes, true, at.pointer + 4, 2);
        assert_eq!(date_time_original(&bytes), None);
    }

    #[test]
    fn a_field_that_cannot_hold_its_value_is_no_date_rather_than_a_panic() {
        // The date's own entry: a type that is not ASCII, a count of zero, a count
        // and an offset the block cannot hold.
        type Damage = fn(&mut Vec<u8>, &At);
        let cases: &[(&str, Damage)] = &[
            ("type 3 (SHORT)", |bytes, at| {
                set16(bytes, true, at.date + 2, 3)
            }),
            ("type 2 but count 0", |bytes, at| {
                set32(bytes, true, at.date + 4, 0)
            }),
            ("count 0xFFFF_FFFF", |bytes, at| {
                set32(bytes, true, at.date + 4, 0xFFFF_FFFF)
            }),
            ("offset 0xFFFF_FFFF", |bytes, at| {
                set32(bytes, true, at.date + 8, 0xFFFF_FFFF)
            }),
            ("offset 0 (the TIFF header)", |bytes, at| {
                set32(bytes, true, at.date + 8, 0)
            }),
        ];
        for (what, damage) in cases {
            let (mut bytes, at) = block(true, DATE, None);
            damage(&mut bytes, &at);
            assert_eq!(date_time_original(&bytes), None, "{what}");
        }
    }

    #[test]
    fn a_truncated_or_unrecognisable_block_is_no_date_rather_than_a_panic() {
        let (bytes, _) = block(true, DATE, None);
        for cut in 0..bytes.len() {
            assert_eq!(
                date_time_original(&bytes[..cut]),
                None,
                "cut to {cut} of {} bytes",
                bytes.len()
            );
        }
        assert_eq!(date_time_original(&[]), None);
        // The header itself: not a TIFF, or an IFD0 offset the block cannot hold.
        assert_eq!(date_time_original(b"XX\x2a\x00\x08\x00\x00\x00"), None);
        assert_eq!(date_time_original(b"I"), None);
        let mut bytes = header(true);
        set32(&mut bytes, true, 4, 0xFFFF_FFFF);
        assert_eq!(date_time_original(&bytes), None);
        // An IFD0 count far past the block.
        let mut bytes = header(true);
        put16(&mut bytes, true, 0xFFFF);
        assert_eq!(date_time_original(&bytes), None);
    }

    #[test]
    fn a_value_that_is_not_the_exif_format_is_no_date() {
        for text in [
            "2019-07-14 10:32:00",
            "2019:07:14T10:32:00",
            "2019:07:14 10:32",
            "",
            "whenever",
        ] {
            let (bytes, _) = block(true, text, None);
            assert_eq!(date_time_original(&bytes), None, "{text:?}");
        }
        // A longer value is the date and whatever the writer put after it: the
        // contract's verbatim rule is about the date, not about the tail.
        let (bytes, _) = block(true, "2019:07:14 10:32:00+02:00", None);
        assert_eq!(
            date_time_original(&bytes).as_deref(),
            Some("2019:07:14 10:32:00")
        );
    }
}
