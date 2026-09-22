//! Bounded decimal layers retain Python integers until the document boundary.
use super::{Error, Result};
use crate::chemistry::cdxml::numeric;
use serde::{Serialize, Serializer};
use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Serialized as a canonical decimal string so raw values beyond JSON/Rust
/// integer ranges remain lossless. `into_document` applies the separate i32 bound.
pub struct NativeLayer {
    negative: bool,
    // Most significant digit first; zero has one digit and no sign.
    digits: Vec<u8>,
}
impl Serialize for NativeLayer {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.decimal())
    }
}
impl Ord for NativeLayer {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (negative, _) => {
                let order = self
                    .digits
                    .len()
                    .cmp(&other.digits.len())
                    .then(self.digits.cmp(&other.digits));
                if negative { order.reverse() } else { order }
            }
        }
    }
}
impl PartialOrd for NativeLayer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl NativeLayer {
    pub fn decimal(&self) -> String {
        let mut out = String::with_capacity(self.digits.len() + 1);
        if self.negative {
            out.push('-');
        }
        out.extend(self.digits.iter().map(|d| char::from(b'0' + d)));
        out
    }
    pub fn into_document(&self) -> Result<i32> {
        self.decimal().parse().map_err(|_| Error::LayerBoundary)
    }
    pub(in crate::chemistry::cdxml) fn parse(text: &str) -> Result<Self> {
        numeric::integer(text)?;
        let normalized = numeric::decimal_digits(text);
        let value = normalized.trim();
        let negative = value.starts_with('-');
        let digits: Vec<u8> = value
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|b| b - b'0')
            .collect();
        Ok(Self::new(negative, digits))
    }
    fn new(negative: bool, digits: Vec<u8>) -> Self {
        let start = digits.iter().position(|d| *d != 0);
        if let Some(start) = start {
            Self {
                negative,
                digits: digits.get(start..).unwrap_or_default().to_vec(),
            }
        } else {
            Self {
                negative: false,
                digits: vec![0],
            }
        }
    }
    pub(super) fn length(&self) -> usize {
        self.digits.len()
    }
    /// Exact z-middle, with the chemical plane (zero) skipped.
    pub(super) fn relative(&self, middle: &Self) -> Self {
        let positive = self >= middle;
        let (large, small) = if positive {
            (self, middle)
        } else {
            (middle, self)
        };
        let different_signs = large.negative != small.negative;
        let (a, b) = if large.negative {
            (&small.digits, &large.digits)
        } else {
            (&large.digits, &small.digits)
        };
        let mut reversed = Vec::with_capacity(a.len().max(b.len()) + 1);
        let mut carry = i16::from(positive);
        let mut left = a.iter().rev();
        let mut right = b.iter().rev();
        loop {
            let (x, y) = (left.next(), right.next());
            if x.is_none() && y.is_none() {
                break;
            }
            let x = i16::from(x.copied().unwrap_or(0));
            let y = i16::from(y.copied().unwrap_or(0));
            let value = x + if different_signs { y } else { -y } + carry;
            reversed.push(value.rem_euclid(10) as u8);
            carry = value.div_euclid(10);
        }
        if carry > 0 {
            reversed.push(carry as u8);
        }
        reversed.reverse();
        Self::new(!positive, reversed)
    }
}
