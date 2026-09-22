//! Windows x64 CRT's non-FMA rotation arithmetic, used by the reference worker
//! under Windows ARM emulation. Adapted from AMD win-libm's SSE2 sin/cos paths;
//! see licenses/amd-win-libm. Only the [-2π, 2π] attachment-rotation domain is
//! supported. Keep the separate multiply/add operations and their order.

const SIN: [f64; 6] = [
    f64::from_bits(0xbfc5555555555555),
    f64::from_bits(0x3f81111111110bb3),
    f64::from_bits(0xbf2a01a019e83e5c),
    f64::from_bits(0x3ec71de3796cde01),
    f64::from_bits(0xbe5ae600b42fdfa7),
    f64::from_bits(0x3de5e0b2f9a43bb8),
];
const COS: [f64; 6] = [
    f64::from_bits(0x3fa5555555555555),
    f64::from_bits(0xbf56c16c16c16967),
    f64::from_bits(0x3efa01a019f4ec91),
    f64::from_bits(0xbe927e4fa17f667b),
    f64::from_bits(0x3e21eeb690382eec),
    f64::from_bits(0xbda907db47258aa7),
];

fn sine_polynomial(x: f64) -> f64 {
    let x2 = x * x;
    let x6 = (x2 * x2) * x2;
    let high = ((SIN[5] * x2 + SIN[4]) * x2 + SIN[3]) * x6;
    let low = (SIN[2] * x2 + SIN[1]) * x2 + SIN[0];
    (x * x2) * (high + low)
}

fn reduced_sine(x: f64, tail: f64) -> f64 {
    let correction = (0.5 * (x * x)) * tail;
    (tail + (sine_polynomial(x) - correction)) + x
}

fn reduced_cosine(x: f64, tail: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x6 = x4 * x2;
    let high = ((COS[5] * x2 + COS[4]) * x2 + COS[3]) * x6;
    let low = (COS[2] * x2 + COS[1]) * x2 + COS[0];
    let half = 0.5 * x2;
    let negative_t = half - 1.0;
    let correction = ((negative_t + 1.0) - half) - tail * x;
    ((low + high) * x4 + correction) - negative_t
}

fn small_cosine(x: f64) -> f64 {
    let absolute = x.abs();
    if absolute < f64::from_bits(0x3f20000000000000) {
        if absolute < f64::from_bits(0x3e40000000000000) {
            return 1.0;
        }
        return 1.0 - (x * x) * 0.5;
    }
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    let low = (COS[1] * x2 + COS[0]) * x4;
    let middle = (COS[3] * x2 + COS[2]) * x8;
    let high = (x4 * x8) * (COS[5] * x2 + COS[4]);
    let negative_half = x2 * -0.5;
    let t = negative_half + 1.0;
    let correction = (1.0 - t) + negative_half;
    (correction + ((low + middle) + high)) + t
}

/// Return (sine, cosine), retaining the reference CRT's f64 rounding. A caller
/// can only obtain this domain by subtracting two finite atan2 results.
pub(super) fn sin_cos(angle: f64) -> Option<(f64, f64)> {
    if !angle.is_finite() || angle.abs() > std::f64::consts::TAU {
        return None;
    }
    if angle == 0.0 {
        return Some((angle, 1.0));
    }
    let absolute = angle.abs();
    if absolute < std::f64::consts::FRAC_PI_4 {
        return Some((angle + sine_polynomial(angle), small_cosine(angle)));
    }
    let multiple = (absolute * f64::from_bits(0x3fe45f306dc9c883) + 0.5).trunc();
    // The checked attachment domain bounds this integer to 0..=4.
    let quadrant = multiple as u8;
    let mut head = absolute - f64::from_bits(0x3ff921fb54400000) * multiple;
    let mut tail = f64::from_bits(0x3dd0b4611a626331) * multiple;
    let mut reduced = head - tail;
    let exponent = ((absolute.to_bits() >> 52) & 0x7ff) as i32;
    let reduced_exponent = ((reduced.to_bits() >> 52) & 0x7ff) as i32;
    if exponent - reduced_exponent > 15 {
        let previous_head = head;
        let product = f64::from_bits(0x3dd0b4611a600000) * multiple;
        head -= product;
        tail = f64::from_bits(0x3ba3198a2e037073) * multiple - ((previous_head - head) - product);
        reduced = head - tail;
    }
    let remainder = (head - reduced) - tail;
    let sine = reduced_sine(reduced, remainder);
    let cosine = reduced_cosine(reduced, remainder);
    let (sine, cosine) = if quadrant & 1 == 0 {
        (sine, cosine)
    } else {
        (cosine, sine)
    };
    let sine = if (quadrant & 2 != 0) != angle.is_sign_negative() {
        0.0 - sine
    } else {
        sine
    };
    let cosine = if (quadrant + 1) & 2 != 0 {
        0.0 - cosine
    } else {
        cosine
    };
    Some((sine, cosine))
}

#[cfg(test)]
mod tests {
    use super::sin_cos;
    use anyhow::Context;

    #[test]
    fn bounded_rotations_match_independent_windows_crt() -> anyhow::Result<()> {
        let data =
            include_bytes!("../../../../tests/fixtures/abbreviation-trigonometry-windows.bin");
        let header: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/abbreviation-trigonometry-windows.json"
        ))?;
        anyhow::ensure!(header["fma3"] == false, "Wrong native math algorithm");
        anyhow::ensure!(data.len().is_multiple_of(24), "Truncated native corpus");
        let mut count = 0_u64;
        let mut differences = Vec::new();
        for record in data.chunks_exact(24) {
            let mut values = record.chunks_exact(8);
            let mut next = || -> anyhow::Result<u64> {
                Ok(u64::from_be_bytes(
                    values.next().context("Missing native value")?.try_into()?,
                ))
            };
            let angle = f64::from_bits(next()?);
            let sine = next()?;
            let cosine = next()?;
            let (actual_sine, actual_cosine) = sin_cos(angle).context("Valid angle rejected")?;
            if (actual_sine.to_bits(), actual_cosine.to_bits()) != (sine, cosine)
                && differences.len() < 20
            {
                differences.push(format!(
                    "angle={angle:.17e}/{:016x} sin={:016x}/{sine:016x} cos={:016x}/{cosine:016x}",
                    angle.to_bits(),
                    actual_sine.to_bits(),
                    actual_cosine.to_bits()
                ));
            }
            count += 1;
        }
        anyhow::ensure!(differences.is_empty(), "{}", differences.join("\n"));
        anyhow::ensure!(
            count > 40_000 && header["cases"] == count,
            "Incomplete native corpus"
        );
        eprintln!("Verified {count} exact native non-FMA rotation pairs");
        Ok(())
    }

    #[test]
    fn rejects_values_outside_the_attachment_domain() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 7.0, -7.0] {
            assert!(sin_cos(value).is_none());
        }
    }
}
