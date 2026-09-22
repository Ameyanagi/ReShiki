//! Validate the lexical_cast<double> coordinate spellings used by CX readers.
//! Hexadecimal input needs separate range checks; no FFI or locale state.
pub(super) fn valid(text: &str) -> bool {
    parse(text).is_some()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reader {
    Mac,
    Windows,
    Linux,
}

/// Read the native coordinate spelling without FFI or process locale changes.
pub fn parse(text: &str) -> Option<f64> {
    if text.len() > 1024 * 1024 {
        return None;
    }
    let reader = if cfg!(windows) {
        Reader::Windows
    } else if cfg!(target_os = "linux") {
        Reader::Linux
    } else {
        Reader::Mac
    };
    parse_for(text, reader)
}

#[cfg(test)]
fn valid_for(text: &str, reader: Reader) -> bool {
    parse_for(text, reader).is_some()
}

fn parse_for(text: &str, reader: Reader) -> Option<f64> {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
        // The Linux reference's stream conversion rejects hexadecimal input.
        if reader == Reader::Linux {
            return None;
        }
        return hexadecimal(
            unsigned.as_bytes().get(2..).unwrap_or_default(),
            reader == Reader::Windows,
        )
        .map(|value| if text.starts_with('-') { -value } else { value });
    }
    let Ok(value) = text.parse::<f64>() else {
        return None;
    };
    if value.is_nan() || value.is_infinite() {
        return (unsigned.eq_ignore_ascii_case("nan")
            || unsigned.eq_ignore_ascii_case("inf")
            || unsigned.eq_ignore_ascii_case("infinity"))
        .then_some(value);
    }
    if value.is_subnormal() {
        return (reader != Reader::Mac).then_some(value);
    }
    // Linux accepts decimal underflow to zero; the other readers reject it.
    (reader == Reader::Linux
        || value != 0.0
        || !unsigned
            .bytes()
            .take_while(|b| !matches!(b, b'e' | b'E'))
            .any(|b| matches!(b, b'1'..=b'9')))
    .then_some(value)
}

fn hexadecimal(text: &[u8], rounded_subnormals: bool) -> Option<f64> {
    let (mut pos, mut digits, mut fraction) = (0usize, 0i64, 0i64);
    let (mut point, mut first, mut last) = (false, None, None);
    let (mut head, mut head_digits) = (0u64, 0u32);
    while let Some(&byte) = text.get(pos) {
        if byte == b'.' && !point {
            point = true;
            pos += 1;
            continue;
        }
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => break,
        };
        if digit != 0 {
            first.get_or_insert((digits, digit));
            last = Some((digits, digit));
        }
        if first.is_some() && head_digits < 16 {
            head = (head << 4) | u64::from(digit);
            head_digits += 1;
        }
        digits += 1;
        fraction += i64::from(point);
        pos += 1;
    }
    if digits == 0 {
        return None;
    }
    let mut exponent = 0i64;
    if matches!(text.get(pos), Some(b'p' | b'P')) {
        pos += 1;
        let negative = text.get(pos) == Some(&b'-');
        if matches!(text.get(pos), Some(b'+' | b'-')) {
            pos += 1;
        }
        let start = pos;
        while let Some(&b) = text.get(pos).filter(|b| b.is_ascii_digit()) {
            // Input is bounded to 1 MiB. This saturation is beyond every
            // exponent that its fractional digits could bring back in range.
            exponent = (exponent * 10 + i64::from(b - b'0')).min(1_000_000_000);
            pos += 1;
        }
        if pos == start {
            return None;
        }
        if negative {
            exponent = -exponent;
        }
    }
    if pos != text.len() {
        return None;
    }
    let (Some((first_index, first_digit)), Some((last_index, last_digit))) = (first, last) else {
        return Some(0.0);
    };
    let high = exponent - 4 * fraction
        + 4 * (digits - first_index - 1)
        + i64::from(7 - first_digit.leading_zeros());
    let low = exponent - 4 * fraction
        + 4 * (digits - last_index - 1)
        + i64::from(last_digit.trailing_zeros());
    let prefix = |bits: u32| {
        if bits == 0 {
            return 0;
        }
        let size = 64 - head.leading_zeros();
        if size > bits {
            head >> (size - bits)
        } else {
            head << (bits - size)
        }
    };
    if high > 1023 {
        return None;
    }
    if high == 1023 && prefix(54) == (1u64 << 54) - 1 {
        return None;
    }
    if high < -1022 {
        let accepted = if rounded_subnormals {
            // At half the smallest subnormal, ties-to-even produces zero.
            high > -1075 || high == -1075 && low < high
        } else {
            // A tie just below the normal boundary rounds to the minimum normal.
            (high == -1023 && prefix(53) == (1u64 << 53) - 1) || low >= -1074
        };
        if !accepted {
            return None;
        }
    }
    let precision = u32::try_from((high + 1075).clamp(0, 53)).ok()?;
    let mut significand = prefix(precision);
    let halfway = prefix(precision + 1) & 1 != 0;
    let sticky = low < high - i64::from(precision);
    if halfway && (sticky || significand & 1 != 0) {
        significand += 1;
    }
    let bits = if high < -1022 {
        if rounded_subnormals && significand == 1u64 << 52 {
            // The Windows native reader has an exponent-carry quirk when a
            // hexadecimal subnormal rounds to the minimum normal. Its result
            // depends on how many leading hexadecimal bits it buffered. Keep
            // that import behavior without calling the platform C runtime.
            let first_bits = 8 - first_digit.leading_zeros();
            let buffered_digits =
                (last_index - first_index + 1).min(if first_bits == 1 { 15 } else { 14 });
            let buffered_bits = i64::from(first_bits) + 4 * (buffered_digits - 1);
            u64::try_from(buffered_bits - 53).ok()? << 52
        } else {
            significand
        }
    } else {
        // A rounded carry advances the exponent; the overflow cases above
        // have already been rejected using the full guard/sticky information.
        let carry = significand == 1u64 << 53;
        if carry {
            significand >>= 1;
        }
        let exponent = u64::try_from(high + i64::from(carry) + 1023).ok()?;
        (exponent << 52) | (significand & ((1u64 << 52) - 1))
    };
    Some(f64::from_bits(bits))
}

#[cfg(test)]
mod tests {
    use super::{Reader, parse_for, valid_for};

    #[test]
    fn linux_coordinate_syntax_and_underflow_match_native_reader() {
        for (text, bits) in [
            ("1e-9999", 0),
            ("-1e-9999", 1u64 << 63),
            ("1e-308", 2_024_022_533_073_106),
            ("5e-324", 1),
        ] {
            assert_eq!(parse_for(text, Reader::Linux).map(f64::to_bits), Some(bits));
        }
        for text in ["0x0", "0x1p0", "-0x1p-1074", "1e309"] {
            assert!(!valid_for(text, Reader::Linux), "{text}");
        }
    }

    #[test]
    fn windows_hexadecimal_boundary_matches_observed_native_bits() {
        for (text, bits) in [
            ("0x0.fffffffffffff8p-1022", 13_510_798_882_111_488),
            ("0x1.fffffffffffff7p-1023", 18_014_398_509_481_984),
            ("0x1.fffffffffffff8p-1023", 18_014_398_509_481_984),
            ("0x1.ffffffffffffffffp-1023", 18_014_398_509_481_984),
        ] {
            assert_eq!(
                parse_for(text, Reader::Windows).map(f64::to_bits),
                Some(bits)
            );
            assert_eq!(
                parse_for(text, Reader::Mac).map(f64::to_bits),
                Some(f64::MIN_POSITIVE.to_bits())
            );
        }
    }

    #[test]
    fn native_underflow_rules_distinguish_windows_and_mac() {
        for input in [
            "1e-308",
            "5e-324",
            "0x1.1p-1075",
            "0x1.8p-1074",
            "0x1.123456789abcdefp-1023",
        ] {
            assert!(valid_for(input, Reader::Windows), "Windows: {input}");
            assert!(!valid_for(input, Reader::Mac), "macOS: {input}");
        }
        for input in [
            "0x1p-1075",
            "0x1p-1076",
            "1e-9999",
            "1e309",
            "0x1p1024",
            "0x1.fffffffffffff8p1023",
            "bad",
        ] {
            assert!(!valid_for(input, Reader::Windows), "Windows: {input}");
            assert!(!valid_for(input, Reader::Mac), "macOS: {input}");
        }
        for input in [
            "0x1p-1074",
            "0x0.fffffffffffff8p-1022",
            "0x1p-1022",
            "0x1.fffffffffffffp1023",
            "0e-9999",
        ] {
            assert!(valid_for(input, Reader::Windows), "Windows: {input}");
            assert!(valid_for(input, Reader::Mac), "macOS: {input}");
        }
    }
}
