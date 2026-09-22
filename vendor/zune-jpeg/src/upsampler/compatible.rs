//! libjpeg-compatible chroma sampling using actual component dimensions.
//!
//! The filter weights, alternating rounding bias, small-width box sampling,
//! and edge extension follow libjpeg-turbo 3.1.4.1 src/jdsample.c. This safe
//! scalar implementation is shared by all CPU paths; IDCT and color-conversion
//! SIMD dispatch remain upstream's. See RESHIKI-PATCH.md for source provenance.
use crate::errors::DecodeErrors;

pub(crate) struct Frame {
    pub width: usize,
    pub height: usize,
    pub row: usize,
    pub horizontal: bool,
    pub vertical: bool,
}
fn invalid() -> DecodeErrors {
    DecodeErrors::FormatStatic("Invalid component bounds for chroma sampling")
}
fn at(input: &[i16], index: usize) -> Result<i32, DecodeErrors> {
    input.get(index).copied().map(i32::from).ok_or_else(invalid)
}

pub(crate) fn sample(
    current: &[i16],
    above: &[i16],
    below: &[i16],
    output: &mut [i16],
    frame: Frame,
) -> Result<(), DecodeErrors> {
    let Frame {
        width,
        height,
        row,
        horizontal,
        vertical,
    } = frame;
    let hs = if horizontal { 2 } else { 1 };
    let vs = if vertical { 2 } else { 1 };
    let stride = current.len().checked_mul(hs).ok_or_else(invalid)?;
    let length = stride.checked_mul(vs).ok_or_else(invalid)?;
    if width == 0 || height == 0 || width > current.len() || stride == 0 {
        return Err(invalid());
    }
    let output = output.get_mut(..length).ok_or_else(invalid)?;
    let current = current.get(..width).ok_or_else(invalid)?;
    // libjpeg's horizontal fancy routines require at least three real samples.
    // A <=2-wide H2V2 component uses box sampling in both dimensions.
    let fancy = !horizontal || width > 2;
    for (v, target) in output.chunks_exact_mut(stride).enumerate() {
        let far = if !vertical
            || !fancy
            || (v == 0 && row == 0)
            || (v == 1 && row.saturating_add(1) >= height)
        {
            current
        } else if v == 0 {
            above.get(..width).ok_or_else(invalid)?
        } else {
            below.get(..width).ok_or_else(invalid)?
        };
        for (x, value) in target.iter_mut().take(width * hs).enumerate() {
            let near = x / hs;
            let other = if x % 2 == 0 {
                near.saturating_sub(1)
            } else {
                near.saturating_add(1).min(width - 1)
            };
            let a = at(current, near)?;
            let result = if !fancy {
                a
            } else if horizontal && vertical {
                let b = at(current, other)?;
                let near_sum = 3 * a + at(far, near)?;
                let far_sum = 3 * b + at(far, other)?;
                (3 * near_sum + far_sum + if x % 2 == 0 { 8 } else { 7 }) >> 4
            } else if horizontal {
                (3 * a + at(current, other)? + if x % 2 == 0 { 1 } else { 2 }) >> 2
            } else if vertical {
                (3 * a + at(far, near)? + if v == 0 { 1 } else { 2 }) >> 2
            } else {
                a
            };
            *value = i16::try_from(result).map_err(|_| invalid())?;
        }
    }
    Ok(())
}
