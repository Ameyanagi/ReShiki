//! Drawing-style input units. Stored drawing dimensions remain points.
use super::DrawingStyle;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Unit {
    #[default]
    #[serde(rename = "pt")]
    Points,
    #[serde(rename = "mm")]
    Millimetres,
    #[serde(rename = "cm")]
    Centimetres,
}
impl<'de> Deserialize<'de> for Unit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A future preference must not discard the user's other UI settings.
        Ok(match String::deserialize(deserializer)?.as_str() {
            "mm" => Self::Millimetres,
            "cm" => Self::Centimetres,
            _ => Self::Points,
        })
    }
}
impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Points => "pt",
            Self::Millimetres => "mm",
            Self::Centimetres => "cm",
        })
    }
}
impl Unit {
    pub const ALL: [Self; 3] = [Self::Points, Self::Millimetres, Self::Centimetres];

    fn points(self, amount: f64) -> f64 {
        match self {
            Self::Points => amount,
            Self::Millimetres => amount * 720. / 254.,
            Self::Centimetres => amount * 7200. / 254.,
        }
    }
    fn display_value(self, points: f64) -> f64 {
        match self {
            Self::Points => points,
            Self::Millimetres => points * 254. / 720.,
            Self::Centimetres => points * 254. / 7200.,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    Incomplete(&'static str),
    Invalid(&'static str),
}
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete(message) | Self::Invalid(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for InputError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parsed {
    pub points: f32,
    pub unit: Unit,
}

/// Parse an entire edit, never a numeric prefix. Bare values use `default_unit`.
pub fn parse(input: &str, default_unit: Unit) -> Result<Parsed, InputError> {
    use InputError::{Incomplete, Invalid};
    let input = input.trim();
    if input.contains(',') {
        return Err(Invalid(
            "Use a decimal point, for example 0.5 mm; no grouping separators.",
        ));
    }
    let bytes = input.as_bytes();
    let mut end = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    if end == bytes.len() {
        return Err(Incomplete("Finish the number."));
    }
    let start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let whole_digits = end - start;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let decimal_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if decimal_start == end {
            return Err(Incomplete("Finish the decimal number, for example 0.5."));
        }
    } else if whole_digits == 0 {
        return Err(Invalid(
            "Enter a finite number, optionally followed by cm, mm or pt.",
        ));
    }
    let mantissa_end = end;
    if bytes.get(end).is_some_and(|b| matches!(b, b'e' | b'E')) {
        end += 1;
        if bytes.get(end).is_some_and(|b| matches!(b, b'+' | b'-')) {
            end += 1;
        }
        let exponent_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if exponent_start == end {
            return Err(Incomplete("Finish the exponent, for example 1e-2."));
        }
    }
    let suffix = input[end..].trim();
    let unit = if suffix.is_empty() {
        default_unit
    } else if suffix.eq_ignore_ascii_case("pt") {
        Unit::Points
    } else if suffix.eq_ignore_ascii_case("mm") {
        Unit::Millimetres
    } else if suffix.eq_ignore_ascii_case("cm") {
        Unit::Centimetres
    } else if ["p", "m", "c"]
        .iter()
        .any(|s| suffix.eq_ignore_ascii_case(s))
    {
        return Err(Incomplete("Finish the unit: cm, mm or pt."));
    } else {
        return Err(Invalid(
            "Use cm, mm or pt after one number; no grouped digits or other units.",
        ));
    };
    let amount: f64 = input[..end]
        .parse()
        .map_err(|_| Invalid("Enter a finite decimal number."))?;
    if amount.is_sign_negative() {
        return Err(Invalid("Dimensions cannot be negative."));
    }
    let converted = unit.points(amount);
    let points = converted as f32;
    if !amount.is_finite() || !converted.is_finite() || !points.is_finite() {
        return Err(Invalid("The dimension is too large to represent."));
    }
    let nonzero = input[..mantissa_end]
        .bytes()
        .any(|b| matches!(b, b'1'..=b'9'));
    if points == 0. && nonzero {
        return Err(Invalid("The dimension is too small to represent."));
    }
    Ok(Parsed { points, unit })
}

fn compact(text: String) -> String {
    let (mantissa, exponent) = text
        .split_once('e')
        .map_or((text.as_str(), None), |(m, e)| (m, Some(e)));
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    match exponent {
        Some(exponent) => format!("{mantissa}e{exponent}"),
        None => mantissa.into(),
    }
}

/// A readable edit string that round-trips to the same point value. Editors
/// still retain the exact canonical value instead of reparsing rendered text.
pub fn format(points: f32, unit: Unit) -> String {
    if points == 0. || !points.is_finite() {
        return points.to_string();
    }
    let amount = unit.display_value(f64::from(points));
    let exponent = amount.abs().log10().floor() as i32;
    for precision in 1..=9 {
        let text = if !(-4..9).contains(&exponent) {
            compact(format!("{amount:.digits$e}", digits = precision - 1))
        } else {
            let digits = (precision as i32 - 1 - exponent).max(0) as usize;
            compact(format!("{amount:.digits$}"))
        };
        if parse(&text, unit).is_ok_and(|value| value.points.to_bits() == points.to_bits()) {
            return text;
        }
    }
    compact(format!("{amount:.16e}"))
}

/// The five physical Drawing Style fields share their existing point limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Bond,
    Line,
    Bold,
    Margin,
    Hash,
}
impl Dimension {
    pub const ALL: [Self; 5] = [Self::Bond, Self::Line, Self::Bold, Self::Margin, Self::Hash];
    pub fn name(self) -> &'static str {
        match self {
            Self::Bond => "Bond length",
            Self::Line => "Line width",
            Self::Bold => "Bold width",
            Self::Margin => "Label margin",
            Self::Hash => "Hash spacing",
        }
    }
    pub fn range(self) -> (f32, f32) {
        match self {
            Self::Bond => (5., 100.),
            Self::Line => (0.1, 6.),
            Self::Bold => (0.1, 12.),
            Self::Margin => (0., 12.),
            Self::Hash => (0.3, 12.),
        }
    }
    pub fn get(self, style: &DrawingStyle) -> f32 {
        match self {
            Self::Bond => style.bond_length_pt,
            Self::Line => style.line_width_pt,
            Self::Bold => style.bold_width_pt,
            Self::Margin => style.margin_width_pt,
            Self::Hash => style.hash_spacing_pt,
        }
    }
    pub fn set(self, style: &mut DrawingStyle, value: f32) {
        match self {
            Self::Bond => style.set_bond_length(value),
            Self::Line => style.line_width_pt = value,
            Self::Bold => style.bold_width_pt = value,
            Self::Margin => style.margin_width_pt = value,
            Self::Hash => style.hash_spacing_pt = value,
        }
    }
    pub fn validate(self, value: f32, unit: Unit) -> Result<(), String> {
        let (min, max) = self.range();
        if value.is_finite() && (min..=max).contains(&value) {
            return Ok(());
        }
        let converted = if unit == Unit::Points {
            String::new()
        } else {
            format!(" ({}–{} {unit})", format(min, unit), format(max, unit))
        };
        Err(format!(
            "{} must be between {min} and {max} pt{converted}.",
            self.name()
        ))
    }
}

#[cfg(test)]
mod tests;
