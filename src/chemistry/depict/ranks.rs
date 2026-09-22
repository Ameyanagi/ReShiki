//! Literal depiction rank properties. RDKit reads these lazily in
//! DepictUtils.cpp::rankAtomsByRank; RDValue.h::from_rdvalue trims only the
//! right-hand whitespace before unsigned conversion. BSD-3-Clause, see
//! licenses/rdkit/NOTICE. Copyright (C) 2015 Novartis Institutes for
//! BioMedical Research Inc. and RDKit contributors.

/// A retained native property. Invalid text is an observable value until an
/// algorithm actually requests its numeric value; it is not a parse failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Property {
    #[default]
    Absent,
    Value(u32),
    Invalid,
}
impl Property {
    /// Convert a native string property without raising a premature error.
    /// The native C locale trims trailing ASCII space characters, accepts one
    /// sign, and wraps a negative magnitude only after its u32 range check.
    pub fn from_bytes(mut value: &[u8]) -> Self {
        while let Some((last, rest)) = value.split_last() {
            if !matches!(last, b' ' | b'\t' | b'\r' | b'\n' | 11 | 12) {
                break;
            }
            value = rest;
        }
        let negative = value.first() == Some(&b'-');
        if let Some((b'+' | b'-', rest)) = value.split_first() {
            value = rest;
        }
        if value.is_empty() {
            return Self::Invalid;
        }
        let mut number = 0u32;
        for &digit in value {
            if !digit.is_ascii_digit() {
                return Self::Invalid;
            }
            let Some(next) = number
                .checked_mul(10)
                .and_then(|v| v.checked_add(u32::from(digit - b'0')))
            else {
                return Self::Invalid;
            };
            number = next;
        }
        Self::Value(if negative {
            number.wrapping_neg()
        } else {
            number
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtomProperties {
    pub cip: Property,
    pub chiral: Property,
}
impl AtomProperties {
    pub fn from_pairs(properties: &[(Vec<u8>, Vec<u8>)]) -> Self {
        let mut ranks = Self::default();
        for (name, value) in properties {
            match name.as_slice() {
                b"_CIPRank" => ranks.cip = Property::from_bytes(value),
                b"_chiralAtomRank" => ranks.chiral = Property::from_bytes(value),
                _ => {}
            }
        }
        ranks
    }
}

/// Borrow either the established numeric API or retained string properties.
#[derive(Clone, Copy)]
pub(crate) enum Input<'a> {
    Numeric(&'a [Option<u32>]),
    Properties(&'a [AtomProperties]),
}
impl Input<'_> {
    pub(super) fn len(self) -> usize {
        match self {
            Self::Numeric(v) => v.len(),
            Self::Properties(v) => v.len(),
        }
    }
    pub(super) fn get(self, id: usize) -> Option<AtomProperties> {
        match self {
            Self::Numeric(v) => v.get(id).map(|value| AtomProperties {
                cip: Property::Absent,
                chiral: value.map_or(Property::Absent, Property::Value),
            }),
            Self::Properties(v) => v.get(id).copied(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn string_unsigned_conversion_preserves_native_right_trim_and_wrapping() {
        for (text, expected) in [
            ("0", 0),
            ("+1", 1),
            ("-1", u32::MAX),
            ("4294967295", u32::MAX),
            ("-4294967295", 1),
            ("01", 1),
            ("1 \t\r\n\x0b\x0c", 1),
            ("-0", 0),
        ] {
            assert_eq!(
                Property::from_bytes(text.as_bytes()),
                Property::Value(expected),
                "{text:?}"
            );
        }
        for text in [
            "",
            " ",
            " 1",
            "1.0",
            "1e0",
            "4294967296",
            "-4294967296",
            "++1",
            "1\0",
            "1\u{a0}",
        ] {
            assert_eq!(
                Property::from_bytes(text.as_bytes()),
                Property::Invalid,
                "{text:?}"
            );
        }
    }
}
