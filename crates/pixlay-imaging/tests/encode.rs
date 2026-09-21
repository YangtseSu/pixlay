//! S6's export criteria, one file at a time.
//!
//! Every criterion here is about what the *file* says, so the assertions read the
//! encoded bytes: the PNG chunk stream, the JPEG marker stream, the TIFF IFD.
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

use pixlay_imaging::{Chroma, Export, Format, Rgb8View, icc};

const WIDTH: i32 = 96;
const HEIGHT: i32 = 64;
const DPI: f64 = 300.0;

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
fn encode(dir: &Path, name: &str, format: Format, dpi: f64, chroma: Chroma) -> Vec<u8> {
    let pixels = gradient();
    let path = dir.join(name);
    let export = Export {
        format,
        dpi,
        chroma,
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

/// The little- or big-endian value readers of a TIFF IFD.
struct Ifd {
    bytes: Vec<u8>,
    big_endian: bool,
}

impl Ifd {
    fn new(path: &Path) -> Self {
        let bytes = std::fs::read(path).expect("read the TIFF");
        let big_endian = match &bytes[..2] {
            b"II" => false,
            b"MM" => true,
            other => panic!("neither byte order: {other:?}"),
        };
        Self { bytes, big_endian }
    }

    fn u16(&self, at: usize) -> u16 {
        let pair = [self.bytes[at], self.bytes[at + 1]];
        if self.big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    }

    fn u32(&self, at: usize) -> u32 {
        let quad = [
            self.bytes[at],
            self.bytes[at + 1],
            self.bytes[at + 2],
            self.bytes[at + 3],
        ];
        if self.big_endian {
            u32::from_be_bytes(quad)
        } else {
            u32::from_le_bytes(quad)
        }
    }

    /// One entry: `(type, count, value bytes)`. The value bytes are inline when
    /// the value fits in four bytes and otherwise sit at the offset the entry
    /// names, which is where a TIFF reader goes.
    fn entry(&self, tag: u16) -> Option<(u16, u32, Vec<u8>)> {
        let first = self.u32(4) as usize;
        let count = self.u16(first) as usize;
        for index in 0..count {
            let at = first + 2 + 12 * index;
            if self.u16(at) != tag {
                continue;
            }
            let kind = self.u16(at + 2);
            let count = self.u32(at + 4);
            let size = match kind {
                1 | 2 | 6 | 7 => 1u32,
                3 | 8 => 2,
                4 | 9 | 11 => 4,
                5 | 10 | 12 => 8,
                other => panic!("unknown TIFF field type {other}"),
            } * count;
            let (from, length) = if size <= 4 {
                (at + 8, size as usize)
            } else {
                (self.u32(at + 8) as usize, size as usize)
            };
            return Some((kind, count, self.bytes[from..from + length].to_vec()));
        }
        None
    }

    /// A one-value `SHORT` tag, as the reader should see it.
    fn short(&self, tag: u16) -> u16 {
        let (kind, count, data) = self.entry(tag).unwrap_or_else(|| panic!("tag {tag}"));
        assert_eq!((kind, count), (3, 1), "one SHORT");
        let pair = [data[0], data[1]];
        if self.big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    }

    /// `XResolution` (282) and `YResolution` (283) as pixels per inch.
    fn dpi(&self, tag: u16) -> f64 {
        let (kind, count, data) = self.entry(tag).expect("resolution tag");
        assert_eq!((kind, count), (5, 1), "one RATIONAL");
        let numerator = self.u32_at(&data, 0);
        let denominator = self.u32_at(&data, 4);
        f64::from(numerator) / f64::from(denominator)
    }

    fn u32_at(&self, data: &[u8], at: usize) -> u32 {
        let quad = [data[at], data[at + 1], data[at + 2], data[at + 3]];
        if self.big_endian {
            u32::from_be_bytes(quad)
        } else {
            u32::from_le_bytes(quad)
        }
    }
}

#[test]
fn the_png_carries_its_resolution_and_profile() {
    let dir = out_dir("png");
    let bytes = encode(&dir, "out.png", Format::Png, DPI, Chroma::Full);
    let chunks = png_chunks(&bytes);

    // pHYs: pixels per metre, the unit the chunk itself carries. 300 dpi is
    // 11811 px/m (round(300 * 1000 / 25.4)).
    let phys = png_chunk(&chunks, b"pHYs").expect("pHYs is present");
    assert_eq!(phys.len(), 9);
    assert_eq!(be32(phys, 0), 11_811);
    assert_eq!(be32(phys, 4), 11_811);
    assert_eq!(phys[8], 1, "unit is the metre");

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
    let dims = info.pixel_dims.expect("pHYs as the reader sees it");
    assert_eq!((dims.xppu, dims.yppu), (11_811, 11_811));
    assert_eq!(dims.unit, png::Unit::Meter);
    assert_eq!((info.width, info.height), (WIDTH as u32, HEIGHT as u32));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_jpeg_carries_its_resolution_profile_and_subsampling() {
    let dir = out_dir("jpeg");
    for (chroma, expected) in [
        (Chroma::Full, vec![(1, 1), (1, 1), (1, 1)]),
        (Chroma::HorizontalHalf, vec![(2, 1), (1, 1), (1, 1)]),
        (Chroma::Quarter, vec![(2, 2), (1, 1), (1, 1)]),
    ] {
        let name = format!("out-{}.jpg", chroma.name());
        let bytes = encode(&dir, &name, Format::Jpeg, DPI, chroma);

        // JFIF APP0: density in pixels per inch (unit 1).
        let (_, app0) = jpeg_segments(&bytes)
            .into_iter()
            .find(|(marker, _)| *marker == 0xe0)
            .expect("a JFIF APP0");
        assert_eq!(&app0[..5], b"JFIF\0");
        assert_eq!(app0[7], 1, "density unit is the inch");
        assert_eq!((be16(app0, 8), be16(app0, 10)), (300, 300));

        // APP2 ICC_PROFILE, reassembled across its chunks.
        assert_eq!(jpeg_icc(&bytes), icc::srgb_profile());

        // The sampling factors are in the file's own frame header, so this is the
        // assertion the two-pass trap fails: re-encoding to patch metadata would
        // leave 4:2:0 here whatever the request said.
        assert_eq!(
            jpeg_sampling(&bytes),
            expected,
            "chroma {} in SOF0",
            chroma.name()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tiff_carries_its_tags_and_round_trips() {
    let dir = out_dir("tiff");
    // A fractional resolution: the pixel-count export mode derives one, and the
    // rational tag is what carries it.
    let dpi = 914.4;
    let path = dir.join("out.tif");
    let bytes = encode(&dir, "out.tif", Format::Tiff, dpi, Chroma::Full);
    assert_eq!(&bytes[..4], b"II\x2a\x00", "a standard little-endian TIFF");

    let ifd = Ifd::new(&path);
    assert_eq!(ifd.short(296), 2, "ResolutionUnit is the inch");
    assert!(
        (ifd.dpi(282) - dpi).abs() < 1e-9,
        "XResolution {}",
        ifd.dpi(282)
    );
    assert!(
        (ifd.dpi(283) - dpi).abs() < 1e-9,
        "YResolution {}",
        ifd.dpi(283)
    );
    let (kind, _, profile) = ifd.entry(34_675).expect("ICCProfile");
    assert_eq!(kind, 7, "UNDEFINED");
    assert_eq!(profile, icc::srgb_profile());
    assert_eq!(ifd.short(259), 5, "LZW");
    assert_eq!(ifd.short(317), 2, "the horizontal predictor");

    // The pixels survive the compression, through the crate's own decoder rather
    // than through this test's reading of the file.
    let mut decoder =
        tiff::decoder::Decoder::new(BufReader::new(File::open(&path).unwrap())).expect("decodes");
    assert_eq!(
        decoder.dimensions().expect("dimensions"),
        (WIDTH as u32, HEIGHT as u32)
    );
    let decoded = decoder.read_image().expect("pixels");
    let tiff::decoder::DecodingResult::U8(pixels) = decoded else {
        panic!("8-bit samples");
    };
    assert_eq!(pixels, gradient());
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
fn a_jpeg_resolution_jfif_cannot_hold_is_refused() {
    // JFIF stores the density in 16 bits, so an absurd resolution is a refusal
    // rather than a saturated number that reads as a lie. The pixel-count export
    // mode can derive one: 20000 px on a 1 mm canvas is 508000 dpi.
    let dir = out_dir("limits");
    let pixels = gradient();
    let export = Export {
        format: Format::Jpeg,
        dpi: 508_000.0,
        chroma: Chroma::Full,
        image: Rgb8View {
            width: WIDTH,
            height: HEIGHT,
            data: &pixels,
        },
    };
    let path = dir.join("out.jpg");
    let error = pixlay_imaging::encode::write(&path, &export).expect_err("refused");
    assert!(error.to_string().contains("65535"), "{error}");
    assert!(!path.exists(), "nothing is left behind");
    let _ = std::fs::remove_dir_all(&dir);
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
        dpi: DPI,
        chroma: Chroma::Full,
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
        ("a.tif", Some(Format::Tiff)),
        ("a.tiff", Some(Format::Tiff)),
        ("a.webp", None),
        ("a", None),
    ] {
        assert_eq!(Format::from_path(Path::new(name)), expected, "{name}");
    }
    assert_eq!(Format::Png.name(), "png");
    assert_eq!(Chroma::parse("444"), Some(Chroma::Full));
    assert_eq!(Chroma::parse("422"), Some(Chroma::HorizontalHalf));
    assert_eq!(Chroma::parse("420"), Some(Chroma::Quarter));
    assert_eq!(Chroma::parse("411"), None);
    assert_eq!(Chroma::default(), Chroma::Full);
}
