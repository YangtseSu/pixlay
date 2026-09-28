// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! S12b's preview-grade source: the photo, at fewer samples.
//!
//! The step's criteria that belong to this layer, each as a check rather than a
//! claim: the reduction is a **pure function** (the same file reduces to the same
//! bytes), it is **exact for an integer factor** (a `k x k` block comes out as its
//! plain mean), it **never enlarges** a photo, and — the one the preview's whole
//! geometry rests on — it carries the *decoded* photo's aspect rather than the
//! reduced buffer's, so the fit of a crop is the same transform to the bit.
//!
//! The synthetic PNGs here carry what no committed fixture has: an exact block
//! pattern (nothing else can say "this is a plain mean and not a filter"), an
//! alpha boundary at an odd pixel (a premultiplied average keeps the colour and
//! moves only the alpha), and a fully transparent footprint (which has no colour
//! to report). They are written into a scratch directory and decoded by the real
//! decoder, because `PreviewSource` takes a decoded `Source` — and because a
//! reduction measured on a hand-built buffer would be measuring the test.

use std::path::{Path, PathBuf};

use pixlay_core::{CropTransform, PixelSize, templates};
use pixlay_imaging::{Depth, PreviewSource, Sampler, Source};

/// The template the fit is checked against: two rectangles, so the geometry is
/// the plain case and the numbers are readable.
const TEMPLATE: &str = "strip-2-2x1g";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pixlay-cli/tests/fixtures/photos")
        .join(name)
}

/// Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-reduce-tests/{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test directory");
    dir
}

/// Every texel of a sampler, so two of them can be compared as data.
fn texels(source: &impl Sampler) -> Vec<[u16; 4]> {
    let mut out = Vec::with_capacity((source.width() * source.height()) as usize);
    for y in 0..source.height() {
        for x in 0..source.width() {
            out.push(source.pixel(x, y));
        }
    }
    out
}

/// A PNG of `rgb` (one row-major entry per pixel), 8 bits per channel.
fn write_rgb(path: &Path, width: u32, height: u32, rgb: &[[u8; 3]]) {
    let file = std::fs::File::create(path).expect("create the test PNG");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write the PNG header");
    let flat: Vec<u8> = rgb.iter().flatten().copied().collect();
    writer.write_image_data(&flat).expect("write the pixels");
}

/// A PNG of `rgba` (one row-major entry per pixel), 8 bits per channel.
fn write_rgba(path: &Path, width: u32, height: u32, rgba: &[[u8; 4]]) {
    let file = std::fs::File::create(path).expect("create the test PNG");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write the PNG header");
    let flat: Vec<u8> = rgba.iter().flatten().copied().collect();
    writer.write_image_data(&flat).expect("write the pixels");
}

#[test]
fn the_long_edge_is_exact_and_a_photo_below_the_target_is_untouched() {
    let source = Source::decode(&fixture("resample-source.png")).expect("decode");
    assert_eq!((source.width(), source.height()), (1600, 1200));

    // 1600 -> 400 is a factor of four on the long edge and 1200 -> 300 on the
    // other: the reduction's own ratio is the photo's, rounded as a thumbnail's is.
    let reduced = PreviewSource::new(&source, 400);
    assert_eq!((reduced.width(), reduced.height()), (400, 300));

    // At or above the photo's long edge there is nothing to reduce, and the samples
    // are the decoder's own — bit for bit, not approximately.
    for target in [1600, 2000] {
        let whole = PreviewSource::new(&source, target);
        assert_eq!((whole.width(), whole.height()), (1600, 1200), "at {target}");
        assert_eq!(texels(&whole), texels(&source), "at {target}");
    }
}

#[test]
fn an_integer_factor_is_the_plain_mean_of_the_block() {
    // A 16x16 photo of 4x4 flat blocks, each a different colour: reduced to a long
    // edge of 4, every output texel is the mean of a 4x4 block, which is the
    // block's own colour. Nothing with a filter kernel comes out of that exact.
    let dir = scratch("blocks");
    let blocks = [
        [255u8, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [0, 255, 255],
        [255, 0, 255],
        [255, 255, 255],
        [0, 0, 0],
        [128, 64, 32],
        [32, 128, 64],
        [64, 32, 128],
        [200, 100, 50],
        [10, 20, 30],
        [240, 230, 220],
        [90, 90, 90],
        [170, 180, 190],
    ];
    let mut rgb = Vec::with_capacity(16 * 16);
    for y in 0..16 {
        for x in 0..16 {
            rgb.push(blocks[(y / 4) * 4 + x / 4]);
        }
    }
    let path = dir.join("blocks.png");
    write_rgb(&path, 16, 16, &rgb);
    let source = Source::decode(&path).expect("decode");

    let reduced = PreviewSource::new(&source, 4);
    assert_eq!((reduced.width(), reduced.height()), (4, 4));
    for (block, color) in blocks.iter().enumerate() {
        let (x, y) = (block % 4, block / 4);
        let texel = reduced.pixel(x as u32, y as u32);
        for (channel, want) in color.iter().enumerate() {
            let want = u16::from(*want) * 257;
            let got = texel[channel];
            // One level, which is the 16-bit accumulator's own quantization: the
            // footprints are whole pixels, so nothing else can move the value.
            assert!(
                got.abs_diff(want) <= 257,
                "block {block} channel {channel}: {got} is not {want}"
            );
        }
        assert_eq!(texel[3], 65535, "an opaque block stays opaque");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_file_reduces_to_the_same_bytes() {
    let source = Source::decode(&fixture("landscape.jpg")).expect("decode");
    let once = PreviewSource::new(&source, 300);
    let twice = PreviewSource::new(&source, 300);
    assert_eq!((once.width(), once.height()), (300, 169));
    assert_eq!(texels(&once), texels(&twice));
}

#[test]
fn the_aspect_is_the_decoded_photos_and_the_fit_does_not_move() {
    let source = Source::decode(&fixture("resample-source.png")).expect("decode");
    // 350 is deliberately not a divisor of 1600: the reduction is 350x263, whose
    // ratio is *not* the photo's in `f64`, which is the case this test is about.
    let reduced = PreviewSource::new(&source, 350);
    assert_eq!((reduced.width(), reduced.height()), (350, 263));
    // Carried, not recomputed.
    assert_eq!(reduced.aspect(), source.aspect());
    assert_ne!(
        f64::from(350) / f64::from(263),
        source.aspect(),
        "the two ratios have to differ for this test to mean anything"
    );

    // The criterion: the fit of a crop moves by less than 1e-9 between the
    // reduction and the original. It is not "less than": it is *zero*, because the
    // clamp reads the aspect and nothing else about the source, and the region the
    // bitmap holds is derived from the same number.
    let template = templates::get(TEMPLATE).expect("in the library");
    let slot = &template.slots[0];
    let canvas_aspect = template.aspect;
    let crop = CropTransform {
        zoom: 0.7,
        offset: (0.13, -0.07),
        rotation_deg: 17.5,
    };
    let from_original = crop.fit(slot, &slot.outline, canvas_aspect, source.aspect());
    let from_reduction = crop.fit(slot, &slot.outline, canvas_aspect, reduced.aspect());
    assert_eq!(from_original, from_reduction);

    let canvas = PixelSize::for_long_edge(canvas_aspect, 800).expect("a grid");
    let region = |aspect: f64| {
        from_original
            .transform
            .display_region(slot, canvas, aspect, 3.0)
    };
    assert_eq!(region(source.aspect()), region(reduced.aspect()));
}

#[test]
fn a_sixteen_bit_photo_stays_sixteen_bit() {
    let source = Source::decode(&fixture("photo-16bit.png")).expect("decode");
    assert_eq!(source.depth(), Depth::Sixteen);
    let reduced = PreviewSource::new(&source, 200);
    assert_eq!((reduced.width(), reduced.height()), (200, 150));

    // The reduction is not a narrowing: the values are the depth's own, and a
    // reduction of a photo that is not flat is not flat itself.
    let values = texels(&reduced);
    let low_bytes: Vec<u8> = values.iter().map(|texel| (texel[0] & 0xff) as u8).collect();
    assert!(
        low_bytes.iter().any(|byte| *byte != 0),
        "a 16-bit reduction has to keep the depth's low bits"
    );
    assert!(
        values.iter().any(|texel| texel[0] != values[0][0]),
        "the reduction of a photo that varies has to vary"
    );
}

#[test]
fn a_reduction_of_an_eight_bit_photo_keeps_more_than_eight_bits() {
    // S15f, PIX-013. A reduction is an intermediate buffer — the resampler reads it
    // and the only quantization the pipeline allows is the final 8-bit write — so an
    // 8-bit file's reduction is stored at 16 bits, whatever the file carried.
    //
    // The pattern says it exactly: 2-pixel columns alternating between the codes 100
    // and 101, reduced 16 -> 4, so every output texel is the 50/50 mean of two
    // *adjacent* codes. In linear light that is a value strictly between them, and
    // its sRGB code is therefore between `100 * 257` and `101 * 257` — an interval
    // no multiple of 257 lies in, which is the whole of an 8-bit store's vocabulary
    // (`Source::pixel` widens an 8-bit sample by exactly 257: `0xab * 257 = 0xabab`).
    let dir = scratch("depth");
    let mut rgb = Vec::with_capacity(16 * 16);
    for _y in 0..16 {
        for x in 0..16 {
            let code = if (x / 2) % 2 == 0 { 100 } else { 101 };
            rgb.push([code, code, code]);
        }
    }
    let path = dir.join("depth.png");
    write_rgb(&path, 16, 16, &rgb);
    let source = Source::decode(&path).expect("decode");
    assert_eq!(source.depth(), Depth::Eight);

    let reduced = PreviewSource::new(&source, 4);
    assert_eq!((reduced.width(), reduced.height()), (4, 4));
    assert_eq!(reduced.depth(), Depth::Sixteen);
    for y in 0..4 {
        for x in 0..4 {
            let texel = reduced.pixel(x, y);
            assert!(
                texel[0] > 100 * 257 && texel[0] < 101 * 257,
                "texel ({x}, {y}) is {}, not a mean between the two codes",
                texel[0]
            );
            // The channel is the same value three times: nothing here is coloured.
            assert_eq!([texel[0], texel[0]], [texel[1], texel[2]]);
            assert_eq!(texel[3], 65535, "an opaque block stays opaque");
        }
    }
    assert_eq!(
        reduced.bytes(),
        4 * 4 * 4 * 2,
        "the copy the cache holds is the 16-bit one"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn alpha_is_averaged_without_bleeding_color_into_it() {
    // 100x100: the left 51 columns opaque white, the rest fully transparent black.
    // Reducing to a long edge of 50 makes every footprint fall on whole pixels, and
    // the texel covering columns 50..51 reads one opaque white column and one
    // transparent black one — a 50% coverage.
    let dir = scratch("alpha");
    let mut rgba = Vec::with_capacity(100 * 100);
    for _y in 0..100 {
        for x in 0..100 {
            rgba.push(if x < 51 {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 0]
            });
        }
    }
    let path = dir.join("alpha.png");
    write_rgba(&path, 100, 100, &rgba);
    let source = Source::decode(&path).expect("decode");
    let reduced = PreviewSource::new(&source, 50);
    assert_eq!((reduced.width(), reduced.height()), (50, 50));

    // The opaque side is untouched: a premultiplied average cannot pull the
    // transparent black's colour into it.
    let white = reduced.pixel(0, 0);
    assert_eq!(white, [65535, 65535, 65535, 65535], "the opaque side");

    // The mixed column: the *colour* stays white and only the alpha falls. A
    // straight average would have reported 50% grey here instead.
    let mixed = reduced.pixel(25, 0);
    for (channel, got) in mixed.iter().take(3).enumerate() {
        assert_eq!(*got, 65535, "channel {channel} of the mixed texel");
    }
    assert!(
        mixed[3].abs_diff(32768) < 500,
        "the mixed texel's coverage is about half, not {}",
        mixed[3]
    );

    // A fully transparent footprint has no colour to report, and says so.
    assert_eq!(reduced.pixel(49, 0), [0, 0, 0, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}
