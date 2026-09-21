//! S4's decode criteria: the file's orientation, its colour, its depth, its cap.
//!
//! Every image here is a committed fixture, so these tests need no network and no
//! image tool: they decode real bytes through the real backend.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pixlay_imaging::{DecodeLimits, Depth, ImagingError, Sampler, Source, exif};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pixlay-cli/tests/fixtures/photos")
        .join(name)
}

/// Root-mean-square difference between two RGB8 images, as the tests in the
/// renderer measure it.
fn rmse(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| {
            let difference = f64::from(*x) - f64::from(*y);
            difference * difference
        })
        .sum();
    (sum / a.len() as f64).sqrt()
}

/// The alpha.png fixture's pixel as straight sRGB, without a decoder: the test
/// asserts the decode agrees with the file's own bytes where they are simple
/// enough to read by hand.
#[test]
fn heic_decodes_at_its_own_depth() {
    // S4's backend decision was made on this format: the sandboxed loader decodes
    // HEIC and AVIF, and the in-process one covers neither (docs/STEPS.md, "S4 ·
    // decisions"). This is the criterion as a test.
    let source = Source::decode(&fixture("photo.heic")).expect("HEIC decodes");
    assert_eq!(source.mime(), "image/heif");
    assert_eq!((source.width(), source.height()), (800, 600));
    // The fixture is 12-bit, and it arrives at 16 rather than being narrowed at
    // the door: an 8-bit source buffer would quantize before the transfer
    // function, which is the thing the 16-bit rule exists to prevent.
    assert_eq!(source.depth(), Depth::Sixteen);

    // Not flat: a decode that returned a uniform buffer would pass a "size only"
    // check.
    assert!(spread(&source) > 10_000, "the HEIC decoded flat");
}

#[test]
fn the_depth_is_the_files_not_a_fixed_choice() {
    // A 16-bit PNG keeps its 16 bits; an 8-bit JPEG does not pay for them.
    // Measured 2026-09-21: `photo-16bit.png` (written at 16 bits by ImageMagick)
    // arrives as 16-bit samples, `landscape.jpg` as 8.
    let deep = Source::decode(&fixture("photo-16bit.png")).expect("decodes");
    assert_eq!(deep.depth(), Depth::Sixteen);
    assert!(spread(&deep) > 10_000);
    // Widening an 8-bit sample to 16 is exact (`* 257`), so an 8-bit file's
    // samples are the same numbers at either depth.
    let shallow = Source::decode(&fixture("landscape.jpg")).expect("decodes");
    assert_eq!(shallow.depth(), Depth::Eight);
}

/// The two depths must agree, sample for sample.
///
/// `photo-16bit.png` was written from the same content as `ratio-4-3.png`
/// (`generate.py` widens the 8-bit one with ImageMagick), and widening an 8-bit
/// sample to 16 is `* 257` on both sides — ImageMagick's `-depth 16` and
/// `Source::pixel`. So the two decodes must agree sample for sample, exactly.
///
/// That makes this the test that pins the *byte offset* of a 16-bit buffer:
/// reading it with an 8-bit stride returns a neighbouring row, which agrees with
/// nothing here.
#[test]
fn the_two_depths_agree_sample_for_sample() {
    let deep = Source::decode(&fixture("photo-16bit.png")).expect("decodes");
    let shallow = Source::decode(&fixture("ratio-4-3.png")).expect("decodes");
    assert_eq!(deep.depth(), Depth::Sixteen);
    assert_eq!(shallow.depth(), Depth::Eight);
    assert_eq!(
        (deep.width(), deep.height()),
        (shallow.width(), shallow.height())
    );
    let mut checked = 0;
    for y in (0..shallow.height()).step_by(7) {
        for x in (0..shallow.width()).step_by(5) {
            let wide = deep.pixel(x, y);
            for (channel, narrow) in shallow.pixel(x, y).iter().enumerate() {
                assert_eq!(
                    wide[channel], *narrow,
                    "({x}, {y}) channel {channel}: {} against {narrow}",
                    wide[channel]
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 10_000, "only {checked} pixels compared");
}

#[test]
fn a_grayscale_source_arrives_with_equal_channels() {
    // The pipeline carries RGBA, so a one-channel source has to be widened
    // somewhere; doing it at decode is what keeps every later stage from having a
    // grayscale case.
    let source = Source::decode(&fixture("photo-gray.png")).expect("decodes");
    assert_eq!((source.width(), source.height()), (800, 600));
    for (x, y) in [(0, 0), (400, 300), (799, 599), (37, 512)] {
        let pixel = source.pixel(x, y);
        assert_eq!(
            (pixel[0], pixel[1], pixel[2]),
            (pixel[0], pixel[0], pixel[0]),
            "grayscale widened unevenly at ({x}, {y}): {pixel:?}"
        );
        assert_eq!(pixel[3], u16::MAX, "the file is opaque");
    }
}

/// The range of one channel over the whole image: "is this decode flat?".
fn spread(source: &Source) -> u16 {
    let (mut min, mut max) = (u16::MAX, 0u16);
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = source.pixel(x, y)[1];
            min = min.min(value);
            max = max.max(value);
        }
    }
    max - min
}

#[test]
fn exif_orientation_is_applied_to_the_pixels() {
    // `oriented-6.jpg` stores 600x1200 with EXIF Orientation=6 ("rotate 90 CW"),
    // and its content is `bands(600, 1200, 4)` — a pattern with one bright
    // 150x400 rectangle at [150, 400]-[300, 800] in *stored* coordinates.
    //
    // Orientation 6 maps stored (x, y) to displayed (1199 - y, x), so after the
    // rotation the rectangle must sit at [399, 150]-[799, 300]: rotated, not
    // merely resized, and not left where it started.
    let source = Source::decode(&fixture("oriented-6.jpg")).expect("decodes");
    assert_eq!(
        (source.width(), source.height()),
        (1200, 600),
        "orientation 6 swaps the axes; the decoded size must be the displayed one"
    );

    let bright = |x: u32, y: u32| {
        let pixel = source.pixel(x, y);
        u16::from((pixel[0] > 230 * 257) as u8)
            + u16::from((pixel[1] > 230 * 257) as u8)
            + u16::from((pixel[2] > 230 * 257) as u8)
            == 3
    };
    let mut bounds = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut count = 0u32;
    for y in 0..source.height() {
        for x in 0..source.width() {
            if bright(x, y) {
                bounds = (
                    bounds.0.min(x),
                    bounds.1.min(y),
                    bounds.2.max(x),
                    bounds.3.max(y),
                );
                count += 1;
            }
        }
    }
    // A JPEG edge is soft, so the bounds are compared within a few pixels of the
    // 400x150 rectangle the rotation predicts.
    let (x0, y0, x1, y1) = bounds;
    assert!(
        (x0 as i64 - 399).abs() <= 8
            && (y0 as i64 - 150).abs() <= 8
            && (x1 as i64 - 799).abs() <= 8
            && (y1 as i64 - 300).abs() <= 8,
        "bright rectangle at {bounds:?}, expected about (399, 150)-(799, 300)"
    );
    assert!(
        (count as i64 - 60_000).abs() < 4_000,
        "bright area is {count} px, expected about 60000"
    );
    // The control: rotated content is not content that stayed put.
    assert!(
        !(300..400).any(|x| bright(x, 500)),
        "the rectangle is still at its stored position: the rotation was not applied"
    );
}

#[test]
fn a_missing_file_is_an_error_that_names_the_path() {
    let missing = fixture("no-such-photo.jpg");
    let error = Source::decode(&missing).expect_err("must fail");
    let message = error.to_string();
    assert!(message.contains("no-such-photo.jpg"), "{message}");
    assert!(matches!(error, ImagingError::Io { .. }), "{error:?}");
}

#[test]
fn the_pixel_cap_refuses_before_decoding() {
    // The cap is checked between the loader's metadata and its frame, so a
    // decompression bomb costs nothing. A 1600x1200 fixture with a 1000-pixel cap
    // is the same code path without a 120 MP file in the repository.
    let limits = DecodeLimits {
        max_pixels: 1_000,
        ..DecodeLimits::default()
    };
    let error = Source::decode_with(&fixture("resample-source.png"), &limits)
        .expect_err("the cap must refuse");
    match error {
        ImagingError::TooLarge { pixels, max, .. } => {
            assert_eq!(pixels, 1600 * 1200);
            assert_eq!(max, 1_000);
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
    // The same file under the default cap decodes: the cap is a cap, not a bug.
    assert!(Source::decode(&fixture("resample-source.png")).is_ok());
}

#[test]
fn a_decode_that_will_not_finish_is_reported() {
    // glycin's own default limit is 60 seconds, which is far too long for a
    // command-line loop; the pipeline lowers it (docs/CONTRACT.md §4). Zero is the
    // only timeout that always fires, so this pins the mechanism rather than the
    // number.
    let limits = DecodeLimits {
        timeout: Duration::from_nanos(1),
        ..DecodeLimits::default()
    };
    let error = Source::decode_with(&fixture("photo.heic"), &limits).expect_err("must time out");
    let message = error.to_string();
    assert!(message.contains("photo.heic"), "{message}");
}

#[test]
fn the_source_icc_profile_is_honoured() {
    // `adobe-rgb.jpg` is `bands(800, 600, 2)` with the Adobe RGB (1998) profile
    // attached: the same numbers as `ratio-4-3.png`, meaning something else.
    // `adobe-rgb-srgb.png` is that file converted to sRGB by ImageMagick — an
    // implementation that is not ours — so this is a comparison against an
    // independent decoder, not against our own output.
    //
    // Decision recorded in docs/CONTRACT.md §6: the source profile is honoured
    // (the loader converts it; no lcms2 and no colour code of our own).
    let tagged = Source::decode(&fixture("adobe-rgb.jpg")).expect("decodes");
    let reference = Source::decode(&fixture("adobe-rgb-srgb.png")).expect("decodes");
    let untagged = Source::decode(&fixture("ratio-4-3.png")).expect("decodes");
    assert_eq!((tagged.width(), tagged.height()), (800, 600));

    let rgb8 = |source: &Source| -> Vec<u8> {
        let mut out = Vec::with_capacity((source.width() * source.height() * 3) as usize);
        for y in 0..source.height() {
            for x in 0..source.width() {
                let pixel = source.pixel(x, y);
                for sample in &pixel[..3] {
                    out.push((sample >> 8) as u8);
                }
            }
        }
        out
    };
    let ours = rgb8(&tagged);
    let converted = rmse(&ours, &rgb8(&reference));
    let unconverted = rmse(&ours, &rgb8(&untagged));
    // Measured 2026-09-21: 0.04 against ImageMagick's conversion, 15.4 against the
    // same numbers left unconverted. The threshold sits between the two, so a
    // decoder that ignored the profile fails and one that rounds differently
    // passes.
    assert!(
        converted <= 1.0,
        "RMSE {converted} against the sRGB reference"
    );
    assert!(
        unconverted > 5.0,
        "RMSE {unconverted} against the unconverted file: no conversion happened"
    );
}

#[test]
fn alpha_and_dated_exif_survive_the_decode() {
    // Source alpha is the pipeline's business (`over_white`), so the decoder has
    // to hand it over untouched.
    let alpha = Source::decode(&fixture("alpha.png")).expect("decodes");
    assert_eq!(alpha.pixel(0, 0)[3], 0, "the corner is transparent");
    assert_eq!(alpha.pixel(400, 300)[3], u16::MAX, "the disc is opaque");

    // The date is the value `{date}` renders (S5). The contract fixes it as
    // verbatim, with no timezone conversion, so the string is compared as it is.
    let dated = Source::decode(&fixture("dated.jpg")).expect("decodes");
    let exif = dated.exif().expect("the fixture carries an EXIF block");
    assert_eq!(
        exif::date_time_original(exif).as_deref(),
        Some("2019:07:14 10:32:00")
    );
    // And a file without the tag falls back rather than inventing a date.
    let plain = Source::decode(&fixture("landscape.jpg")).expect("decodes");
    let date = plain.exif().and_then(exif::date_time_original);
    assert_eq!(date, None, "landscape.jpg must not carry a date");
    let _ = Sampler::aspect(&plain);
}
