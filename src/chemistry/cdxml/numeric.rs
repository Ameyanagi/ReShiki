//! Pinned native scalar parsing and diagnostic quoting shared by CDXML readers.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid literal for int() with base 10: {0}")]
    Integer(String),
    #[error(
        "Exceeds the limit (4300 digits) for integer string conversion: value has {0} digits; use sys.set_int_max_str_digits() to increase the limit"
    )]
    IntegerDigits(usize),
    #[error("could not convert string to float: {0}")]
    Float(String),
}
type Result<T> = std::result::Result<T, Error>;

// Rust 1.95 uses Unicode 17; the pinned Python 3.12 uses Unicode 15. These
// observed newly printable intervals retain the original repr of invalid input.
fn nonprinting(ch: char) -> bool {
    if ch.is_ascii() {
        return ch.is_ascii_control();
    }
    if format!("x{ch}").escape_debug().nth(1) == Some('\\') {
        return true;
    }
    const LATER: &[(u32, u32)] = &[
        (0x88f, 0x88f),
        (0x897, 0x897),
        (0xc5c, 0xc5c),
        (0xcdc, 0xcdc),
        (0x1acf, 0x1add),
        (0x1ae0, 0x1aeb),
        (0x1b4e, 0x1b4f),
        (0x1b7f, 0x1b7f),
        (0x1c89, 0x1c8a),
        (0x20c1, 0x20c1),
        (0x2427, 0x2429),
        (0x2b96, 0x2b96),
        (0x2ffc, 0x2fff),
        (0x31e4, 0x31e5),
        (0x31ef, 0x31ef),
        (0xa7cb, 0xa7cf),
        (0xa7d2, 0xa7d2),
        (0xa7d4, 0xa7d4),
        (0xa7da, 0xa7dc),
        (0xa7f1, 0xa7f1),
        (0xfbc3, 0xfbd2),
        (0xfd90, 0xfd91),
        (0xfdc8, 0xfdce),
        (0x105c0, 0x105f3),
        (0x10940, 0x10959),
        (0x10d40, 0x10d65),
        (0x10d69, 0x10d85),
        (0x10d8e, 0x10d8f),
        (0x10ec2, 0x10ec7),
        (0x10ed0, 0x10ed8),
        (0x10efa, 0x10efc),
        (0x11380, 0x11389),
        (0x1138b, 0x1138b),
        (0x1138e, 0x1138e),
        (0x11390, 0x113b5),
        (0x113b7, 0x113c0),
        (0x113c2, 0x113c2),
        (0x113c5, 0x113c5),
        (0x113c7, 0x113ca),
        (0x113cc, 0x113d5),
        (0x113d7, 0x113d8),
        (0x113e1, 0x113e2),
        (0x116d0, 0x116e3),
        (0x11b60, 0x11b67),
        (0x11bc0, 0x11be1),
        (0x11bf0, 0x11bf9),
        (0x11db0, 0x11ddb),
        (0x11de0, 0x11de9),
        (0x11f5a, 0x11f5a),
        (0x13460, 0x143fa),
        (0x16100, 0x16139),
        (0x16d40, 0x16d79),
        (0x16ea0, 0x16eb8),
        (0x16ebb, 0x16ed3),
        (0x16ff2, 0x16ff6),
        (0x187f8, 0x187ff),
        (0x18cff, 0x18cff),
        (0x18d09, 0x18d1e),
        (0x18d80, 0x18df2),
        (0x1cc00, 0x1ccfc),
        (0x1cd00, 0x1ceb3),
        (0x1ceba, 0x1ced0),
        (0x1cee0, 0x1cef0),
        (0x1e5d0, 0x1e5fa),
        (0x1e5ff, 0x1e5ff),
        (0x1e6c0, 0x1e6de),
        (0x1e6e0, 0x1e6f5),
        (0x1e6fe, 0x1e6ff),
        (0x1f6d8, 0x1f6d8),
        (0x1f777, 0x1f77a),
        (0x1f8b2, 0x1f8bb),
        (0x1f8c0, 0x1f8c1),
        (0x1f8d0, 0x1f8d8),
        (0x1fa54, 0x1fa57),
        (0x1fa89, 0x1fa8a),
        (0x1fa8e, 0x1fa8f),
        (0x1fabe, 0x1fabe),
        (0x1fac6, 0x1fac6),
        (0x1fac8, 0x1fac8),
        (0x1facd, 0x1facd),
        (0x1fadc, 0x1fadc),
        (0x1fadf, 0x1fadf),
        (0x1fae9, 0x1faea),
        (0x1faef, 0x1faef),
        (0x1fbcb, 0x1fbef),
        (0x1fbfa, 0x1fbfa),
        (0x2b73a, 0x2b73f),
        (0x2cea2, 0x2cead),
        (0x2ebf0, 0x2ee5d),
        (0x323b0, 0x33479),
    ];
    let value = u32::from(ch);
    LATER
        .partition_point(|(start, _)| *start <= value)
        .checked_sub(1)
        .and_then(|i| LATER.get(i))
        .is_some_and(|(_, end)| value <= *end)
}

// Python uses repr in scalar-conversion and missing-key errors. Preserve the
// file spelling rather than exposing a Rust parsing or indexing diagnostic.
pub(super) fn quoted(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for ch in value.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch == quote => {
                result.push('\\');
                result.push(ch);
            }
            ch if nonprinting(ch) => {
                let n = u32::from(ch);
                if n < 256 {
                    result.push_str(&format!("\\x{n:02x}"));
                } else if n < 65536 {
                    result.push_str(&format!("\\u{n:04x}"));
                } else {
                    result.push_str(&format!("\\U{n:08x}"));
                }
            }
            ch => result.push(ch),
        }
    }
    result.push(quote);
    result
}

pub(super) fn decimal_digits(text: &str) -> String {
    // Python 3.12's Unicode 15 decimal blocks, observed independently from
    // unicodedata.decimal. XML numeric attributes may use any of these digits.
    const ZEROES: [u32; 68] = [
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60,
        0x16ac0, 0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0,
        0x1e950, 0x1fbf0,
    ];
    text.chars()
        .map(|ch| {
            if ch.is_ascii() {
                return ch;
            }
            let n = u32::from(ch);
            ZEROES
                .iter()
                .find_map(|zero| n.checked_sub(*zero).filter(|d| *d < 10))
                .and_then(|digit| char::from_u32(u32::from('0') + digit))
                .unwrap_or(ch)
        })
        .collect()
}

// Keep overflow separate from lexical errors: oversized elements never match,
// oversized colors are invalid, and oversized layers exceed the document type.
pub(super) fn integer(text: &str) -> Result<Option<i128>> {
    let decimal = decimal_digits(text);
    let value = decimal.trim();
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    let mut previous_digit = false;
    let mut count = 0;
    for ch in digits.chars() {
        if ch.is_ascii_digit() {
            previous_digit = true;
            count += 1;
        } else if ch == '_' && previous_digit {
            previous_digit = false;
        } else {
            return Err(Error::Integer(quoted(text).chars().take(200).collect()));
        }
    }
    if !previous_digit {
        return Err(Error::Integer(quoted(text).chars().take(200).collect()));
    }
    if count > 4300 {
        return Err(Error::IntegerDigits(count));
    }
    Ok(value.replace('_', "").parse().ok())
}

pub(super) fn float(text: &str) -> Result<f64> {
    // Python permits underscores only between decimal digits.
    let decimal = decimal_digits(text);
    let value = decimal.trim();
    let mut previous = None;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_'
            && (!previous.is_some_and(|c: char| c.is_ascii_digit())
                || !chars.peek().is_some_and(|c| c.is_ascii_digit()))
        {
            return Err(Error::Float(quoted(text)));
        }
        previous = Some(ch);
    }
    value
        .replace('_', "")
        .parse()
        .map_err(|_| Error::Float(quoted(text)))
}
