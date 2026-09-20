//! The committed fixtures, checked without a decoder.
//!
//! S1 commits six photos; S4 uses them. Two of their properties are invisible
//! and would break S4 silently if a regeneration lost them, so they are pinned
//! here: the EXIF orientation tag on `oriented-6.jpg`, and the alpha channel on
//! `alpha.png`. Both are read straight out of the file bytes, so this test needs
//! no image crate and no decoder — exactly the shape S1's tests must have.
//!
//! `generate.py` in the same directory produced these files; regenerate with
//! `python3 crates/pixlay-cli/tests/fixtures/generate.py`.

use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every fixture exists, is non-empty, and has the dimensions S4 expects.
#[test]
fn fixtures_are_committed() {
    let expected: [(&str, u32, u32); 6] = [
        ("photos/portrait.jpg", 600, 900),
        ("photos/landscape.jpg", 960, 540),
        ("photos/ratio-4-3.png", 800, 600),
        ("photos/square.png", 640, 640),
        ("photos/alpha.png", 800, 600),
        ("photos/oriented-6.jpg", 600, 1200),
    ];
    for (name, width, height) in expected {
        let bytes = read(name);
        assert!(!bytes.is_empty(), "{name} is empty");
        let size = if name.ends_with(".png") {
            png_size(&bytes)
        } else {
            jpeg_size(&bytes)
        };
        assert_eq!(size, (width, height), "{name} changed size");
    }
    assert!(fixture_dir().join("generate.py").is_file());
}

#[test]
fn the_orientation_fixture_still_carries_orientation_6() {
    let bytes = read("photos/oriented-6.jpg");
    assert_eq!(
        exif_orientation(&bytes),
        Some(6),
        "oriented-6.jpg lost its EXIF orientation tag; S4's rotate-to-upright test \
         would silently pass on unrotated pixels"
    );
    // The other photos must stay untagged: a stray orientation tag would make
    // an S4 comparison depend on a tag nobody meant to add.
    for name in ["photos/portrait.jpg", "photos/landscape.jpg"] {
        assert_eq!(exif_orientation(&read(name)), None, "{name}");
    }

    // Orientation 6 swaps the axes: 600x1200 stored means 1200x600 displayed.
    assert_eq!(jpeg_size(&bytes), (600, 1200));
}

#[test]
fn the_alpha_fixture_really_has_alpha() {
    let bytes = read("photos/alpha.png");
    // IHDR color type 6 is truecolor with alpha.
    assert_eq!(bytes[25], 6, "alpha.png is no longer RGBA");
    assert_ne!(
        read("photos/ratio-4-3.png")[25],
        6,
        "ratio-4-3.png must stay opaque"
    );
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[1..4], b"PNG", "not a PNG");
    assert_eq!(&bytes[12..16], b"IHDR", "PNG without IHDR");
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("4 bytes"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("4 bytes"));
    (width, height)
}

fn jpeg_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[0..2], &[0xff, 0xd8], "not a JPEG");
    let mut cursor = 2;
    while cursor + 3 < bytes.len() {
        assert_eq!(bytes[cursor], 0xff, "malformed marker at {cursor}");
        let marker = bytes[cursor + 1];
        let length = usize::from(u16::from_be_bytes(
            bytes[cursor + 2..cursor + 4].try_into().expect("2 bytes"),
        ));
        // SOF0..SOF3, SOF5..SOF7, SOF9..SOF11, SOF13..SOF15 carry the size.
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let height = u32::from(u16::from_be_bytes(
                bytes[cursor + 5..cursor + 7].try_into().expect("2 bytes"),
            ));
            let width = u32::from(u16::from_be_bytes(
                bytes[cursor + 7..cursor + 9].try_into().expect("2 bytes"),
            ));
            return (width, height);
        }
        // Everything before the scan starts is a length-prefixed segment.
        cursor += 2 + length;
    }
    panic!("JPEG without a start-of-frame marker");
}

/// The EXIF orientation value from the APP1 segment, if the file has one.
fn exif_orientation(bytes: &[u8]) -> Option<u16> {
    let start = find(bytes, b"Exif\0\0")? + 6;
    let tiff = start;
    let little_endian = match &bytes[tiff..tiff + 2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read16 = |at: usize| -> u16 {
        let pair = [bytes[at], bytes[at + 1]];
        if little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        }
    };
    let read32 = |at: usize| -> u32 {
        let quad = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
        if little_endian {
            u32::from_le_bytes(quad)
        } else {
            u32::from_be_bytes(quad)
        }
    };
    let ifd = tiff + read32(tiff + 4) as usize;
    let count = read16(ifd);
    for index in 0..count as usize {
        let entry = ifd + 2 + index * 12;
        if read16(entry) == 0x0112 {
            return Some(read16(entry + 8));
        }
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
