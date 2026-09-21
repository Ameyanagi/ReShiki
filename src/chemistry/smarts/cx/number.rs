//! Validate the lexical_cast<double> coordinate spellings used by CX readers.
//! Hexadecimal input needs separate range checks; no FFI or locale state.
pub(super) fn valid(text: &str) -> bool {
    // UCRT accepts rounded nonzero subnormals. The Unix reference reports
    // decimal underflow and inexact hexadecimal subnormals as range errors.
    valid_for(text, cfg!(windows))
}

fn valid_for(text: &str, rounded_subnormals: bool) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
        return hexadecimal(
            unsigned.as_bytes().get(2..).unwrap_or_default(),
            rounded_subnormals,
        );
    }
    let Ok(value) = text.parse::<f64>() else {
        return false;
    };
    if value.is_nan() || value.is_infinite() {
        return unsigned.eq_ignore_ascii_case("nan")
            || unsigned.eq_ignore_ascii_case("inf")
            || unsigned.eq_ignore_ascii_case("infinity");
    }
    if value.is_subnormal() {
        return rounded_subnormals;
    }
    value != 0.0
        || !unsigned
            .bytes()
            .take_while(|b| !matches!(b, b'e' | b'E'))
            .any(|b| matches!(b, b'1'..=b'9'))
}

fn hexadecimal(text: &[u8], rounded_subnormals: bool) -> bool {
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
        return false;
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
            return false;
        }
        if negative {
            exponent = -exponent;
        }
    }
    if pos != text.len() {
        return false;
    }
    let (Some((first_index, first_digit)), Some((last_index, last_digit))) = (first, last) else {
        return true;
    };
    let high = exponent - 4 * fraction
        + 4 * (digits - first_index - 1)
        + i64::from(7 - first_digit.leading_zeros());
    let low = exponent - 4 * fraction
        + 4 * (digits - last_index - 1)
        + i64::from(last_digit.trailing_zeros());
    let prefix = |bits: u32| {
        let size = 64 - head.leading_zeros();
        if size > bits {
            head >> (size - bits)
        } else {
            head << (bits - size)
        }
    };
    if high > 1023 {
        return false;
    }
    if high == 1023 && prefix(54) == (1u64 << 54) - 1 {
        return false;
    }
    if high < -1022 {
        if rounded_subnormals {
            // At half the smallest subnormal, ties-to-even produces zero.
            return high > -1075 || high == -1075 && low < high;
        }
        // A tie just below the normal boundary rounds to the minimum normal.
        return (high == -1023 && prefix(53) == (1u64 << 53) - 1) || low >= -1074;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::valid_for;

    #[test]
    fn native_underflow_rules_distinguish_windows_and_unix() {
        for input in [
            "1e-308",
            "5e-324",
            "0x1.1p-1075",
            "0x1.8p-1074",
            "0x1.123456789abcdefp-1023",
        ] {
            assert!(valid_for(input, true), "Windows: {input}");
            assert!(!valid_for(input, false), "Unix: {input}");
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
            assert!(!valid_for(input, true), "Windows: {input}");
            assert!(!valid_for(input, false), "Unix: {input}");
        }
        for input in [
            "0x1p-1074",
            "0x0.fffffffffffff8p-1022",
            "0x1p-1022",
            "0x1.fffffffffffffp1023",
            "0e-9999",
        ] {
            assert!(valid_for(input, true), "Windows: {input}");
            assert!(valid_for(input, false), "Unix: {input}");
        }
    }
}
