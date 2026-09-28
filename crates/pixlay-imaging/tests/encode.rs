// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S6's export criteria, one file at a time.
//!
//! Every criterion here is about what the *file* says, so the assertions read the
//! encoded bytes: the PNG chunk stream and the JPEG marker stream.
//! That is deliberate — the failure mode this step exists to prevent is a file
//! whose pixels are 4:4:4 while its header says 4:2:0 (the two-pass metadata trap,
//! measured in S0), and only the stream itself can tell the two apart.
//!
//! The profile is checked against an implementation that is not ours: the sRGB
//! profile ImageMagick wrote into the committed fixture
//! `photos/adobe-rgb-srgb.png`, which is the same colorimetry arriving from
//! lcms2. The colorants and the transfer curve must agree with it.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use pixlay_imaging::{Export, Format, Rgb8View, icc};

const WIDTH: i32 = 96;
const HEIGHT: i32 = 64;

fn out_dir(name: &str) -> PathBuf {
    // Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-encode-tests/{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test directory");
    dir
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pixlay-cli/tests/fixtures/photos")
        .join(name)
}

/// Non-flat content, as every measurement rule in this project requires: flat
/// blocks would let a broken row stride or a wrong colour transform hide.
fn gradient() -> Vec<u8> {
    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            data.push((x * 255 / WIDTH) as u8);
            data.push((y * 255 / HEIGHT) as u8);
            data.push(((x + y) * 255 / (WIDTH + HEIGHT)) as u8);
        }
    }
    data
}

/// Encodes the gradient and returns the bytes, so a test reads a real file.
fn encode(dir: &Path, name: &str, format: Format) -> Vec<u8> {
    let pixels = gradient();
    let path = dir.join(name);
    let export = Export {
        format,
        image: Rgb8View {
            width: WIDTH,
            height: HEIGHT,
            data: &pixels,
        },
    };
    let bytes = pixlay_imaging::encode::write(&path, &export).expect("the export is written");
    let written = std::fs::read(&path).expect("read back");
    assert_eq!(bytes, written.len() as u64);
    written
}

fn be16(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The PNG chunk stream, as `(name, data)`.
fn png_chunks(bytes: &[u8]) -> Vec<([u8; 4], &[u8])> {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
    let mut chunks = Vec::new();
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let length = be32(bytes, at) as usize;
        let name: [u8; 4] = bytes[at + 4..at + 8].try_into().unwrap();
        chunks.push((name, &bytes[at + 8..at + 8 + length]));
        at += 12 + length;
        if &name == b"IEND" {
            break;
        }
    }
    chunks
}

fn png_chunk<'a>(chunks: &[([u8; 4], &'a [u8])], name: &[u8; 4]) -> Option<&'a [u8]> {
    chunks
        .iter()
        .find(|(chunk, _)| chunk == name)
        .map(|(_, data)| *data)
}

/// The JPEG marker segments before the scan, as `(marker, data)`.
fn jpeg_segments(bytes: &[u8]) -> Vec<(u8, &[u8])> {
    assert_eq!(&bytes[..2], &[0xff, 0xd8], "JPEG SOI");
    let mut segments = Vec::new();
    let mut at = 2;
    while at + 4 <= bytes.len() {
        assert_eq!(bytes[at], 0xff, "marker at {at}");
        let marker = bytes[at + 1];
        if marker == 0xda {
            // Start of scan: the entropy-coded data follows, and it is not a
            // marker stream.
            break;
        }
        let length = be16(bytes, at + 2) as usize;
        segments.push((marker, &bytes[at + 4..at + 2 + length]));
        at += 2 + length;
    }
    segments
}

/// The ICC profile a JPEG carries, from its `APP2` `ICC_PROFILE` segments.
fn jpeg_icc(bytes: &[u8]) -> Vec<u8> {
    let mut chunks: Vec<(u8, &[u8])> = Vec::new();
    let mut declared = 0;
    for (marker, data) in jpeg_segments(bytes) {
        if marker != 0xe2 || !data.starts_with(b"ICC_PROFILE\0") {
            continue;
        }
        let index = data[12];
        declared = data[13];
        chunks.push((index, &data[14..]));
    }
    assert!(declared > 0, "no ICC_PROFILE segment");
    assert_eq!(chunks.len(), usize::from(declared), "every chunk present");
    chunks.sort_by_key(|(index, _)| *index);
    let mut profile = Vec::new();
    for (_, data) in chunks {
        profile.extend_from_slice(data);
    }
    profile
}

/// The component sampling factors a JPEG's `SOF0` declares, as `(h, v)`.
fn jpeg_sampling(bytes: &[u8]) -> Vec<(u8, u8)> {
    let (_, frame) = jpeg_segments(bytes)
        .into_iter()
        .find(|(marker, _)| *marker == 0xc0)
        .expect("a baseline SOF0");
    assert_eq!(frame[0], 8, "8-bit samples");
    assert_eq!(
        (i32::from(be16(frame, 1)), i32::from(be16(frame, 3))),
        (HEIGHT, WIDTH)
    );
    let components = frame[5] as usize;
    (0..components)
        .map(|index| {
            let at = 6 + 3 * index;
            (frame[at + 1] >> 4, frame[at + 1] & 0x0f)
        })
        .collect()
}

#[test]
fn the_png_carries_its_profile_and_no_resolution() {
    let dir = out_dir("png");
    let bytes = encode(&dir, "out.png", Format::Png);
    let chunks = png_chunks(&bytes);

    // No `pHYs` (S12d): the file's size is its pixels, and no resolution is
    // claimed for it.
    assert!(
        png_chunk(&chunks, b"pHYs").is_none(),
        "the PNG must not claim a resolution"
    );

    // iCCP, deflate-compressed, and never alongside the sRGB chunk: the
    // specification says the two should not both be present, and the profile is
    // the one carrying the colorimetry.
    let iccp = png_chunk(&chunks, b"iCCP").expect("iCCP is present");
    assert_eq!(
        iccp[iccp.iter().position(|byte| *byte == 0).unwrap() + 1],
        0
    );
    assert!(
        png_chunk(&chunks, b"sRGB").is_none(),
        "the sRGB chunk is not written next to iCCP"
    );

    // The profile the file holds decodes back to exactly the bytes the encoder
    // embedded, through a reader that is not the writer.
    let decoder = png::Decoder::new(BufReader::new(File::open(dir.join("out.png")).unwrap()));
    let reader = decoder.read_info().expect("the PNG decodes");
    let info = reader.info();
    assert_eq!(info.icc_profile.as_deref(), Some(icc::srgb_profile()));
    assert!(
        info.pixel_dims.is_none(),
        "the reader must see no pHYs either"
    );
    assert_eq!((info.width, info.height), (WIDTH as u32, HEIGHT as u32));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_jpeg_carries_its_profile_and_subsampling_with_no_resolution() {
    let dir = out_dir("jpeg");
    let bytes = encode(&dir, "out.jpg", Format::Jpeg);

    // JFIF APP0: the density unit is 0 (square pixels, aspect ratio only), the
    // encoder's default — no resolution is claimed for a file whose size is only
    // its pixels (S12d).
    let (_, app0) = jpeg_segments(&bytes)
        .into_iter()
        .find(|(marker, _)| *marker == 0xe0)
        .expect("a JFIF APP0");
    assert_eq!(&app0[..5], b"JFIF\0");
    assert_eq!(app0[7], 0, "density unit is none, not the inch");
    assert_eq!((be16(app0, 8), be16(app0, 10)), (1, 1));

    // APP2 ICC_PROFILE, reassembled across its chunks.
    assert_eq!(jpeg_icc(&bytes), icc::srgb_profile());

    // The sampling factors are in the file's own frame header, and they are 4:4:4
    // since S12c removed the request: this is the assertion the two-pass trap
    // fails, because re-encoding to patch metadata would leave 4:2:0 here.
    assert_eq!(
        jpeg_sampling(&bytes),
        vec![(1, 1), (1, 1), (1, 1)],
        "the JPEG is 4:4:4 in its own SOF0"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_embedded_profile_is_the_srgb_colorimetry_and_says_so() {
    let profile = icc::srgb_profile();

    // A structurally valid ICC profile: the size the header declares is the size
    // of the blob, the mandatory signature and the class / space / PCS fields are
    // the ones a display profile has, and every tag's range lies inside the blob
    // without overlapping another's.
    assert_eq!(be32(profile, 0) as usize, profile.len());
    assert_eq!(&profile[36..40], b"acsp");
    assert_eq!(&profile[12..16], b"mntr");
    assert_eq!(&profile[16..20], b"RGB ");
    assert_eq!(&profile[20..24], b"XYZ ");
    assert_eq!(
        &profile[68..80],
        &fixed_bytes(&[0.9642, 1.0, 0.8249]),
        "D50 PCS illuminant"
    );

    let count = be32(profile, 128) as usize;
    assert_eq!(count, 10, "the tags the profile declares");
    let table = 132 + 12 * count;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for index in 0..count {
        let at = 132 + 12 * index;
        let offset = be32(profile, at + 4) as usize;
        let size = be32(profile, at + 8) as usize;
        assert!(offset >= table, "tag data starts after the table");
        assert!(
            offset + size <= profile.len(),
            "tag data ends inside the blob"
        );
        ranges.push((offset, offset + size));
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        assert!(pair[0].1 <= pair[1].0, "tag data does not overlap");
    }

    // The colorimetry, against the sRGB profile ImageMagick wrote into the
    // committed fixture: another implementation's answer to the same question.
    // Measured 2026-09-21, the largest difference over the three colorants is
    // 2.2e-4, which is the 15.16 fixed point the tags are stored in plus the
    // chromaticity precision both sides started from.
    let reference = reference_profile();
    assert_eq!(tag(profile, b"wtpt"), tag(&reference, b"wtpt"));
    for name in [b"rXYZ", b"gXYZ", b"bXYZ"] {
        let ours = fixed_values(profile, name);
        let theirs = fixed_values(&reference, name);
        assert_eq!(ours.len(), 3);
        for (index, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
            assert!(
                (ours - theirs).abs() < 1e-3,
                "{name:?} component {index}: {ours} against {theirs}"
            );
        }
    }

    // The transfer curve, as the parameters of an ICC parametric curve of type
    // 3: `Y = (aX + b)^g` above `d`, `Y = cX` below it. One unit in the last
    // place of 15.16 is 1.5e-5, and the two implementations round differently
    // there, so the comparison is at that scale.
    let ours = fixed_values(profile, b"rTRC")[1..].to_vec();
    let theirs = fixed_values(&reference, b"rTRC")[1..].to_vec();
    assert_eq!(ours.len(), 5, "g, a, b, c, d");
    for (index, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
        assert!(
            (ours - theirs).abs() < 1e-4,
            "curve parameter {index}: {ours} against {theirs}"
        );
    }
    assert!((ours[0] - 2.4).abs() < 1e-4, "the sRGB exponent");
    // One unit in the last place of the profile's 15.16 fixed point is 1.5e-5, so
    // the knee cannot be compared more tightly than that.
    assert!((ours[4] - 0.04045).abs() < 2e-5, "the sRGB knee");

    // The name `--stats` reports is the profile's own description.
    assert_eq!(description(profile), icc::DESCRIPTION);
}

#[test]
fn the_profile_header_carries_a_valid_creation_date() {
    let profile = icc::srgb_profile();
    // Header bytes 24..36 are the creation time as six `dateTimeNumber`s: year,
    // month, day, hour, minute, second, each big-endian (ICC.1:2010 §4.2, §7.2.8),
    // and it is a field a strict validator reads rather than free space — a zeroed
    // one is a date no reader can accept (PIX-025, S15i). Zero is the sentinel for
    // the profile id at 84..100 and for nothing else.
    let field =
        |index: usize| u16::from_be_bytes([profile[24 + 2 * index], profile[25 + 2 * index]]);
    let (year, month, day, hour, minute, second) =
        (field(0), field(1), field(2), field(3), field(4), field(5));
    assert!(
        (1..=12).contains(&month),
        "month {month} is outside the specification's 1..=12"
    );
    assert!(
        (1..=31).contains(&day),
        "day {day} is outside the specification's 1..=31"
    );
    assert!((1900..=9999).contains(&year), "year {year}");
    assert!(
        hour < 24 && minute < 60 && second < 60,
        "the time of day is not a time: {hour}:{minute}:{second}"
    );
    // And it is a constant, not the clock: the profile's whole promise is that two
    // builds write the same bytes.
    assert_eq!(
        (year, month, day, hour, minute, second),
        (2026, 9, 21, 0, 0, 0)
    );
}

/// The sRGB profile embedded in the committed fixture — written by ImageMagick
/// from colord's `sRGB.icc`, so it is lcms2's answer, not this crate's.
fn reference_profile() -> Vec<u8> {
    let decoder = png::Decoder::new(BufReader::new(
        File::open(fixture("adobe-rgb-srgb.png")).expect("fixture"),
    ));
    let reader = decoder.read_info().expect("valid PNG");
    reader
        .info()
        .icc_profile
        .as_ref()
        .expect("the fixture carries a profile")
        .to_vec()
}

fn tag<'a>(profile: &'a [u8], signature: &[u8; 4]) -> Option<&'a [u8]> {
    let count = be32(profile, 128) as usize;
    for index in 0..count {
        let at = 132 + 12 * index;
        if &profile[at..at + 4] != signature {
            continue;
        }
        let offset = be32(profile, at + 4) as usize;
        let size = be32(profile, at + 8) as usize;
        return Some(&profile[offset..offset + size]);
    }
    None
}

/// A tag's payload as 15.16 fixed point numbers.
fn fixed_values(profile: &[u8], signature: &[u8; 4]) -> Vec<f64> {
    let data = tag(profile, signature).unwrap_or_else(|| panic!("tag {signature:?} missing"));
    // The first eight bytes are the tag's *type* header (`XYZ `, `sf32`, `para`),
    // which is not the same four characters as the tag's name.
    assert!(
        data[..4]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b' '),
        "the tag has a type signature, not data"
    );
    data[8..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f64::from(i32::from_be_bytes(*bytes)) / 65_536.0)
        .collect()
}

fn fixed_bytes(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| ((value * 65_536.0).round() as i32).to_be_bytes())
        .collect()
}

/// The first `en-US` record of an `mluc` tag.
fn description(profile: &[u8]) -> String {
    let data = tag(profile, b"desc").expect("a description");
    assert_eq!(&data[..4], b"mluc");
    let length = be32(data, 16 + 4) as usize;
    let offset = be32(data, 16 + 8) as usize;
    let utf16: Vec<u16> = data[offset..offset + length]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    String::from_utf16(&utf16).expect("the description is UTF-16")
}

#[test]
fn a_buffer_that_does_not_match_its_size_is_refused() {
    // The encoders index the buffer by the dimensions they were given; a
    // mismatched view is a caller bug, and it is caught before anything is
    // written.
    let dir = out_dir("buffer");
    let mut pixels = gradient();
    pixels.truncate(pixels.len() - 3);
    let export = Export {
        format: Format::Png,
        image: Rgb8View {
            width: WIDTH,
            height: HEIGHT,
            data: &pixels,
        },
    };
    let path = dir.join("out.png");
    let error = pixlay_imaging::encode::write(&path, &export).expect_err("refused");
    assert!(error.to_string().contains("the buffer holds"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_format_follows_the_extension() {
    for (name, expected) in [
        ("a.png", Some(Format::Png)),
        ("a.JPG", Some(Format::Jpeg)),
        ("a.jpeg", Some(Format::Jpeg)),
        ("a.tif", None),
        ("a.tiff", None),
        ("a.webp", None),
        ("a", None),
    ] {
        assert_eq!(Format::from_path(Path::new(name)), expected, "{name}");
    }
    assert_eq!(Format::Png.name(), "png");
    // TIFF is not a format this build writes any more, so `.tif` is refused rather
    // than falling back to a format the caller did not ask for.
    assert_eq!(
        Format::EXTENSIONS,
        ".png, .jpg or .jpeg",
        "the usage message lists what this build writes"
    );
}

#[test]
fn a_jpeg_past_its_own_size_fields_is_refused_before_anything_is_written() {
    // JPEG records its width and height in 16-bit fields, so 65535 px is the widest
    // grid the format can name. A larger one used to be truncated into those fields
    // and written as a file whose header disagreed with its pixels (S15c, PIX-024);
    // it is now refused, and refused *before* the file exists.
    let dir = out_dir("jpeg-boundary");
    let pixels = vec![0u8; 65535 * 3];
    let export = |width: i32, height: i32| Export {
        format: Format::Jpeg,
        image: Rgb8View {
            width,
            height,
            data: &pixels,
        },
    };

    // The boundary itself is a file: one pixel column of 65535 rows, small enough to
    // write in a test.
    let path = dir.join("widest.jpg");
    let bytes = pixlay_imaging::encode::write(&path, &export(1, 65535)).expect("65535 fits");
    assert!(bytes > 0);
    let written = std::fs::read(&path).expect("read back");
    let (_, frame) = jpeg_segments(&written)
        .into_iter()
        .find(|(marker, _)| *marker == 0xc0)
        .expect("a baseline SOF0");
    assert_eq!(
        (i32::from(be16(frame, 1)), i32::from(be16(frame, 3))),
        (65535, 1),
        "the file records the size it was given"
    );

    for (width, height) in [(65536, 1), (1, 65536), (70000, 1)] {
        let refused = dir.join(format!("refused-{width}x{height}.jpg"));
        let error = pixlay_imaging::encode::write(&refused, &export(width, height))
            .expect_err("past the format's fields");
        assert!(error.to_string().contains("65535"), "{error}");
        assert!(
            !refused.exists(),
            "{width}x{height} was refused but a file was created anyway"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_export_replaces_the_file_rather_than_rewriting_it() {
    // The write goes through a temporary file and one rename, which is what makes a
    // failure in the middle harmless (S15c, PIX-011). The visible consequence is that
    // the target is a *new* file: another name that pointed at the old one — a hard
    // link — still has the old bytes, where rewriting the target in place would have
    // changed both.
    let dir = out_dir("replace");
    let path = dir.join("export.jpg");
    let first = encode(&dir, "export.jpg", Format::Jpeg);

    let other = dir.join("linked.jpg");
    std::fs::hard_link(&path, &other).expect("hard link");
    assert_eq!(std::fs::read(&other).expect("read"), first);

    // The second export carries different pixels, so "the file was replaced" is a
    // claim about the bytes rather than about the name.
    let mut inverted = gradient();
    for byte in &mut inverted {
        *byte = 255 - *byte;
    }
    let export = Export {
        format: Format::Jpeg,
        image: Rgb8View {
            width: WIDTH,
            height: HEIGHT,
            data: &inverted,
        },
    };
    let bytes = pixlay_imaging::encode::write(&path, &export).expect("the second export");
    let second = std::fs::read(&path).expect("read back");
    assert_eq!(bytes, second.len() as u64);
    assert_ne!(second, first, "the second export is a different file");
    assert_eq!(
        std::fs::read(&other).expect("read"),
        first,
        "rewriting the target changed a file that merely pointed at it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_export_leaves_the_previous_file_and_no_litter() {
    // Every failure mode this writer has, against a target that is already there:
    // a refusal (the buffer does not match the size it claims), and a write that
    // cannot be completed at all (a directory sits at the target's name, so the
    // pixels are encoded and the rename cannot happen).
    let dir = out_dir("failed-export");
    let path = dir.join("export.png");
    let first = encode(&dir, "export.png", Format::Png);

    let short = gradient();
    let mismatched = Export {
        format: Format::Png,
        image: Rgb8View {
            width: WIDTH,
            height: HEIGHT,
            data: &short[..short.len() - 3],
        },
    };
    let error = pixlay_imaging::encode::write(&path, &mismatched).expect_err("refused");
    assert!(error.to_string().contains("the buffer holds"), "{error}");
    assert_eq!(
        std::fs::read(&path).expect("read"),
        first,
        "a refused export changed the file that was already there"
    );

    let blocked = dir.join("blocked.png");
    std::fs::create_dir(&blocked).expect("create directory");
    let pixels = gradient();
    let export = Export {
        format: Format::Png,
        image: Rgb8View {
            width: WIDTH,
            height: HEIGHT,
            data: &pixels,
        },
    };
    let error = pixlay_imaging::encode::write(&blocked, &export)
        .expect_err("a directory cannot be replaced");
    assert!(
        error.to_string().contains("blocked.png"),
        "the error names the file that was asked for, not the temporary one: {error}"
    );
    assert!(blocked.is_dir(), "the directory was replaced by a file");
    let litter: Vec<String> = std::fs::read_dir(&dir)
        .expect("read directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert_eq!(litter, Vec::<String>::new(), "a failed export left litter");
    assert_eq!(
        std::fs::read(&path).expect("read"),
        first,
        "a failed export changed another file in the same directory"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
