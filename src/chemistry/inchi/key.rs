//! InChIKey generation adapted from InChI 1.07.3 `ikey_dll.c`,
//! `ikey_base26.c` and `util.c::extract_inchi_substring`.
//! Copyright (c) 2024 IUPAC and InChI Trust. MIT; see licenses/inchi/.
//!
//! This reproduces the reference key function's limited string validation;
//! producing a key does not establish that an InChI describes a valid molecule.

use sha2::{Digest, Sha256};

/// Bound the input scan and hashing work, including any ignored suffix.
pub const MAX_INPUT_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid InChI prefix")]
    InvalidPrefix,
    #[error("Invalid input InChI string")]
    InvalidInchi,
    #[error("Invalid standard InChI string")]
    InvalidStandardInchi,
    #[error("InChIKey input or allocation exceeds the supported limit")]
    Limit,
}

impl Error {
    /// Corresponding `GetINCHIKeyFromINCHI` status, when this is a native error.
    pub fn native_code(self) -> Option<i32> {
        match self {
            Self::InvalidPrefix => Some(3),
            Self::InvalidInchi => Some(20),
            Self::InvalidStandardInchi => Some(21),
            Self::Limit => None,
        }
    }
}

/// Generate the reference 27-character key from an InChI string.
///
/// Like the native API, this stops at NUL and trims at the first character
/// outside the InChI alphabet, rather than requiring the complete input to be
/// valid. Standard, nonstandard and experimental version-1 prefixes are accepted.
/// Initial character validation uses the ASCII/C-locale rule. Some native
/// Windows locales classify a UTF-8 leading byte as a letter, then discard the
/// entire non-ASCII body and hash an empty layer; such malformed inputs are
/// rejected here. Generated chemical identifiers are ASCII and unaffected.
/// The input is unchanged. Work is linear and bounded by [`MAX_INPUT_BYTES`].
pub fn from_inchi(input: &str) -> Result<String, Error> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(Error::Limit);
    }
    let bytes = input.as_bytes();
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    let source = bytes.get(..end).ok_or(Error::InvalidPrefix)?;
    if source.len() < 9 || !source.starts_with(b"InChI=1") {
        return Err(Error::InvalidPrefix);
    }
    let (slash, standard) = match source.get(7) {
        Some(b'S') => (8, b'S'),
        Some(b'B') => (8, b'B'),
        _ => (7, b'N'),
    };
    if source.get(slash) != Some(&b'/') {
        return Err(Error::InvalidPrefix);
    }
    let start = slash + 1;
    if !source
        .get(start)
        .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'/' | b'?'))
    {
        return Err(Error::InvalidInchi);
    }
    let end = source
        .iter()
        .position(|&c| !in_alphabet(c))
        .unwrap_or(source.len());
    let source = source.get(..end).ok_or(Error::InvalidInchi)?;

    // The native scan ignores a trailing slash and remembers the last /p
    // encountered before the first minor layer. It does not enforce layer order.
    let mut minor_start = None;
    let mut proton_start = None;
    for (offset, pair) in source
        .get(start..)
        .ok_or(Error::InvalidInchi)?
        .windows(2)
        .enumerate()
    {
        let [first, next] = pair else {
            return Err(Error::InvalidInchi);
        };
        if *first != b'/' {
            continue;
        }
        let position = start + offset;
        match next {
            b'c' | b'h' | b'q' => continue,
            b'p' => {
                proton_start = Some(position);
                continue;
            }
            b'f' | b'r' if standard == b'S' => return Err(Error::InvalidStandardInchi),
            _ => minor_start = Some(position),
        }
        break;
    }
    let major_end = proton_start.or(minor_start).unwrap_or(source.len());
    let major = source.get(start..major_end).ok_or(Error::InvalidInchi)?;
    let minor = source
        .get(minor_start.unwrap_or(source.len())..)
        .ok_or(Error::InvalidInchi)?;
    let proton = match proton_start {
        None => b'N',
        Some(position) => {
            // Without a minor layer, C copies the terminating NUL as well.
            let native_end = minor_start.unwrap_or(source.len() + 1);
            if native_end - position < 3 {
                return Err(Error::InvalidInchi);
            }
            proton_flag(
                source
                    .get(position + 2..minor_start.unwrap_or(source.len()))
                    .ok_or(Error::InvalidInchi)?,
            )?
        }
    };

    let major_hash: [u8; 32] = Sha256::digest(major).into();
    let mut hasher = Sha256::new();
    hasher.update(minor);
    if !minor.is_empty() && minor.len() < 255 {
        hasher.update(minor);
    }
    let minor_hash: [u8; 32] = hasher.finalize().into();
    let mut key = String::new();
    key.try_reserve_exact(27).map_err(|_| Error::Limit)?;
    for start in [0, 14, 28, 42] {
        triplet(&mut key, bits(&major_hash, start, 14)?)?;
    }
    doublet(&mut key, bits(&major_hash, 56, 9)?)?;
    key.push('-');
    for start in [0, 14] {
        triplet(&mut key, bits(&minor_hash, start, 14)?)?;
    }
    doublet(&mut key, bits(&minor_hash, 28, 9)?)?;
    key.push(char::from(standard));
    key.push('A');
    key.push('-');
    key.push(char::from(proton));
    Ok(key)
}

fn in_alphabet(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'(' | b')' | b'*' | b'+' | b',' | b'-' | b'.' | b'/' | b';' | b'=' | b'?' | b'@'
        )
}

fn proton_flag(text: &[u8]) -> Result<u8, Error> {
    let (negative, digits) = match text.first() {
        Some(b'-') => (true, text.get(1..).ok_or(Error::InvalidInchi)?),
        Some(b'+') => (false, text.get(1..).ok_or(Error::InvalidInchi)?),
        _ => (false, text),
    };
    // strtol saturates at C LONG_MIN/MAX; the reference then narrows to int.
    // C long is 32 bits on Windows and 64 bits on the supported Unix hosts.
    let max = if size_of::<std::os::raw::c_long>() == 4 {
        i128::from(i32::MAX)
    } else {
        i128::from(i64::MAX)
    };
    let limit = max + i128::from(negative);
    let magnitude = digits
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0_i128, |value, c| {
            (value * 10 + i128::from(c - b'0')).min(limit)
        });
    let signed = i64::try_from(if negative { -magnitude } else { magnitude })
        .map_err(|_| Error::InvalidInchi)?;
    // Explicitly retain the low 32 bits, matching the supported C ABIs.
    let [a, b, c, d, ..] = signed.to_le_bytes();
    let count = i32::from_le_bytes([a, b, c, d]);
    match count {
        0 => Err(Error::InvalidStandardInchi),
        -12..=12 => u8::try_from(i32::from(b'N') + count).map_err(|_| Error::InvalidInchi),
        _ => Ok(b'A'),
    }
}

fn bits(hash: &[u8; 32], start: usize, width: usize) -> Result<u16, Error> {
    if width > 14 || start > 256 - width {
        return Err(Error::InvalidInchi);
    }
    let mut value = 0;
    for offset in 0..width {
        let bit = start + offset;
        let byte = hash.get(bit / 8).ok_or(Error::InvalidInchi)?;
        value |= u16::from((byte >> (bit % 8)) & 1) << offset;
    }
    Ok(value)
}

fn letter(value: u16) -> Result<char, Error> {
    if value >= 26 {
        return Err(Error::InvalidInchi);
    }
    Ok(char::from(
        b'A' + u8::try_from(value).map_err(|_| Error::InvalidInchi)?,
    ))
}

fn triplet(output: &mut String, value: u16) -> Result<(), Error> {
    if value >= 16384 {
        return Err(Error::InvalidInchi);
    }
    // Native t26 omits EAA..EZZ and TAA..TTV from lexical base-26 order.
    let value = value
        + if value >= 12168 {
            1192
        } else if value >= 2704 {
            676
        } else {
            0
        };
    output.push(letter(value / 676)?);
    doublet(output, value % 676)
}

fn doublet(output: &mut String, value: u16) -> Result<(), Error> {
    output.push(letter(value / 26)?);
    output.push(letter(value % 26)?);
    Ok(())
}
