//! The committed fixtures, checked without a decoder.
//!
//! S1 committed six photos; S4 added the rest — a HEIC, a 12-bit-capable 16-bit
//! PNG, a grayscale one, an Adobe RGB file with the sRGB conversion ImageMagick
//! made of it, a photo with a date, a resampling source with the Lanczos
//! reduction ImageMagick made of it, and the project `AGENTS.md`'s verification
//! entry renders.
//!
//! The properties pinned here are the ones that are invisible and would break a
//! later step silently if a regeneration lost them: the EXIF orientation tag, the
//! alpha channel, the EXIF date, the ICC profile, the 16-bit depth, the container
//! of the HEIC. They are read straight out of the file bytes, so this test needs
//! no decoder — that is the other half of its job: it checks what the fixtures
//! *are*, while `pixlay-imaging`'s tests check what they decode to.
//!
//! `generate.py` in the same directory produced these files; regenerate with
//! `python3 crates/pixlay-cli/tests/fixtures/generate.py`.

use std::path::{Path, PathBuf};

use pixlay_core::Project;

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
    let expected: [(&str, u32, u32); 12] = [
        ("photos/portrait.jpg", 600, 900),
        ("photos/landscape.jpg", 960, 540),
        ("photos/ratio-4-3.png", 800, 600),
        ("photos/square.png", 640, 640),
        ("photos/alpha.png", 800, 600),
        ("photos/oriented-6.jpg", 600, 1200),
        ("photos/dated.jpg", 960, 540),
        ("photos/photo-16bit.png", 800, 600),
        ("photos/photo-gray.png", 800, 600),
        ("photos/adobe-rgb.jpg", 800, 600),
        ("photos/adobe-rgb-srgb.png", 800, 600),
        ("photos/resample-source.png", 1600, 1200),
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
    assert_eq!(
        png_size(&read("photos/resample-lanczos-200.png")),
        (200, 150)
    );
    assert!(fixture_dir().join("generate.py").is_file());
}

/// The HEIC is a HEIC, and it is one the decoder can be asked to read.
#[test]
fn the_heic_fixture_is_a_heic() {
    let bytes = read("photos/photo.heic");
    // An ISO base media file: a 4-byte size, then `ftyp`, then the major brand.
    assert_eq!(&bytes[4..8], b"ftyp", "photo.heic is not an ISO media file");
    let brand = &bytes[8..12];
    assert!(
        brand == b"heic" || brand == b"heix" || brand == b"mif1",
        "unexpected HEIC brand {brand:?}"
    );
}

/// The ICC fixture carries a profile, and the reference next to it does not: the
/// pair is what makes "the source profile is honoured" measurable.
#[test]
fn the_icc_fixture_pair_is_intact() {
    // JPEG carries the profile in APP2 segments, under the `ICC_PROFILE`
    // signature; PNG carries it in an `iCCP` chunk. Each file must have its own
    // container's marker and not the other's — a fixture that lost its profile
    // would make the colour test pass vacuously, because both sides would then be
    // raw numbers.
    let jpeg = read("photos/adobe-rgb.jpg");
    assert!(
        find(&jpeg, b"ICC_PROFILE").is_some(),
        "adobe-rgb.jpg lost its ICC profile"
    );
    let png = read("photos/adobe-rgb-srgb.png");
    assert!(
        find(&png, b"iCCP").is_some(),
        "adobe-rgb-srgb.png lost the sRGB profile ImageMagick converted to"
    );
    assert!(
        find(&png, b"ICC_PROFILE").is_none(),
        "adobe-rgb-srgb.png is not a JPEG"
    );
}

/// The 16-bit fixture is 16 bits deep: the pipeline's "the depth is the file's"
/// test would otherwise compare two 8-bit files.
#[test]
fn the_deep_fixture_is_really_sixteen_bit() {
    let bytes = read("photos/photo-16bit.png");
    assert_eq!(png_size(&bytes), (800, 600));
    // IHDR: bit depth is the byte after the colour type.
    assert_eq!(bytes[24], 16, "photo-16bit.png is no longer 16-bit");
    assert_eq!(bytes[25], 2, "photo-16bit.png is no longer truecolour");
}

/// The date fixture carries a date, in the format EXIF specifies.
#[test]
fn the_date_fixture_carries_the_date() {
    let bytes = read("photos/dated.jpg");
    assert!(
        find(&bytes, b"2019:07:14 10:32:00").is_some(),
        "dated.jpg lost its DateTimeOriginal"
    );
}

/// `AGENTS.md`'s verification entry renders this project, so it has to load and
/// every path in it has to exist.
#[test]
fn the_verification_project_loads_and_its_photos_exist() {
    let path = fixture_dir().join("verify.pixlay");
    let project =
        Project::load(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(project.doc().template.name, "mosaic-8-s14");
    let sources = project.sources().expect("every photo exists");
    assert_eq!(sources.len(), 8);
    assert!(sources.iter().all(Option::is_some), "{sources:?}");
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
