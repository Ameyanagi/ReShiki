//! Bounds for the Calculate card's SCF overrides.

use std::ops::RangeInclusive;

pub const ACCURACY: (RangeInclusive<f64>, &str) =
    (1e-4..=1e3, "Accuracy must be a number from 0.0001 to 1000");
pub const TEMPERATURE: (RangeInclusive<f64>, &str) = (
    0.0..=1e5,
    "Electronic temperature must be from 0 to 100000 K",
);
pub const ITERATIONS: (RangeInclusive<u32>, &str) = (
    1..=10_000,
    "Max SCF iterations must be a whole number from 1 to 10000",
);

pub fn check<T: PartialOrd>(
    value: Option<T>,
    (range, error): (RangeInclusive<T>, &'static str),
) -> Result<Option<T>, &'static str> {
    if value.as_ref().is_none_or(|value| range.contains(value)) {
        Ok(value)
    } else {
        Err(error)
    }
}
