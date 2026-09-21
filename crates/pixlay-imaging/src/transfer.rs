//! sRGB ↔ linear conversion tables, and the 16-bit buffers the pipeline uses.
//!
//! The project's rule is `sRGB → linear → process → sRGB`, with 16-bit
//! intermediate buffers and quantization only at the end (`AGENTS.md`, "Hard
//! constraints"). Both directions are therefore looked up in a table: the
//! transfer function is expensive enough to notice over a 148 MP canvas, and a
//! table makes the conversion *exact* rather than merely accurate — the same
//! input bit pattern always produces the same output bit pattern, which is what
//! makes the identity case byte-identical.
//!
//! Fixed point is used for everything that is stored: 0 is 0.0 and 65535 is 1.0,
//! in linear light. Accumulators inside the resampler are `f32`, which is finer
//! than the 16-bit buffers they feed — an accumulator is never the thing that
//! quantizes.

use std::sync::LazyLock;

/// sRGB code value (as a 16-bit normalized sample) to linear 16-bit.
pub fn srgb_to_linear(sample: u16) -> u16 {
    static TABLE: LazyLock<Box<[u16; 65536]>> = LazyLock::new(|| {
        let mut table = Box::new([0u16; 65536]);
        for (value, slot) in table.iter_mut().enumerate() {
            let srgb = f64::from(value as u16) / 65535.0;
            *slot = to_fixed(transfer(srgb));
        }
        table
    });
    TABLE[usize::from(sample)]
}

/// sRGB code value to linear light as `f32`, for the resampler.
///
/// The resampler multiplies every source sample by a filter weight, so it needs
/// the value in floating point; a table of `f32` costs 256 KB and saves a
/// conversion per sample of every tap of every line.
pub fn srgb_to_linear_f32(sample: u16) -> f32 {
    static TABLE: LazyLock<Box<[f32; 65536]>> = LazyLock::new(|| {
        let mut table = Box::new([0f32; 65536]);
        for (value, slot) in table.iter_mut().enumerate() {
            *slot = transfer(f64::from(value as u16) / 65535.0) as f32;
        }
        table
    });
    TABLE[usize::from(sample)]
}

/// Linear 16-bit to the sRGB code value an 8-bit encoder writes.
pub fn linear_to_srgb8(value: u16) -> u8 {
    static TABLE: LazyLock<Box<[u8; 65536]>> = LazyLock::new(|| {
        let mut table = Box::new([0u8; 65536]);
        for (index, slot) in table.iter_mut().enumerate() {
            let linear = f64::from(index as u16) / 65535.0;
            let srgb = inverse_transfer(linear);
            *slot = (srgb * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        table
    });
    TABLE[usize::from(value)]
}

/// The sRGB electro-optical transfer function (IEC 61966-2-1).
pub fn transfer(srgb: f64) -> f64 {
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

/// Its inverse.
pub fn inverse_transfer(linear: f64) -> f64 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Fixed point of `value`, clamped to `[0, 1]`.
pub fn to_fixed(value: f64) -> u16 {
    (value * 65535.0).round().clamp(0.0, 65535.0) as u16
}

/// Fixed point back to `[0, 1]`.
pub fn from_fixed(value: u16) -> f64 {
    f64::from(value) / 65535.0
}

/// `1 / 65535`, the scale from a 16-bit sample to `[0, 1]`, when a division is
/// not wanted in an inner loop.
pub const WGHT: f32 = 1.0 / 65535.0;

/// The Rec. 709 luminance weights, which are the sRGB primaries' weights and the
/// only luminance a saturation control should be measured against.
pub const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];
