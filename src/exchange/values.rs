use super::{Property, Reader, Result, finite};

pub(super) fn numeric(kind: &str) -> bool {
    matches!(
        kind,
        "INT8"
            | "UINT8"
            | "INT16"
            | "UINT16"
            | "INT32"
            | "UINT32"
            | "CDXObjectID"
            | "CDXCoordinate"
            | "FLOAT64"
    )
}
fn bitfield(name: &str) -> bool {
    matches!(
        name,
        "Order" | "LineType" | "RectangleType" | "OvalType" | "CurveType"
    )
}
fn line_height(name: &str) -> bool {
    matches!(name, "LineHeight" | "CaptionLineHeight" | "LabelLineHeight")
}
fn variants(p: &Property) -> &[(&str, i64)] {
    if line_height(p.name) {
        &[
            ("variable", 0),
            ("auto", 1),
            ("Variable", 0),
            ("Auto", 1),
            ("Automatic", 1),
        ]
    } else {
        p.variants
    }
}
fn scaled(p: &Property) -> bool {
    p.kind == "CDXCoordinate" || matches!(p.name, "ChainAngle" | "PositioningAngle")
}
fn kind(p: &Property) -> &str {
    if p.kind == "INT16" && bitfield(p.name) {
        "UINT16"
    } else {
        p.kind
    }
}

pub(super) fn encode_number(p: &Property, value: &str) -> Result<Vec<u8>> {
    let variants = variants(p);
    let mut n = if let Some((_, n)) = variants.iter().find(|(k, _)| *k == value) {
        *n as f64
    } else if bitfield(p.name) && !variants.is_empty() {
        if let Ok(n) = value.trim().parse::<i64>() {
            n as f64
        } else {
            let mut flags = 0;
            for part in value.split_whitespace() {
                flags |= variants
                    .iter()
                    .find(|(k, _)| *k == part)
                    .ok_or_else(|| format!("Unsupported drawing flag: {}", p.name))?
                    .1;
            }
            flags as f64
        }
    } else if !variants.is_empty() && !line_height(p.name) {
        return Err(format!("Unsupported drawing value: {}={value}", p.name));
    } else {
        finite(value)?
    };
    if scaled(p) {
        n *= 65536.;
    }
    if p.name == "BondSpacing" {
        n *= 10.;
    }
    pack_number(kind(p), n)
}

pub(super) fn pack_number(kind: &str, n: f64) -> Result<Vec<u8>> {
    if !n.is_finite() {
        return Err("Non-finite drawing value".into());
    }
    if kind == "FLOAT64" {
        return Ok(n.to_le_bytes().to_vec());
    }
    let n = n.round_ties_even();
    macro_rules! pack {
        ($t:ty) => {{
            if n < <$t>::MIN as f64 || n > <$t>::MAX as f64 {
                return Err("Drawing value exceeds the binary format range".into());
            }
            Ok((n as $t).to_le_bytes().to_vec())
        }};
    }
    match kind {
        "INT8" => pack!(i8),
        "UINT8" => pack!(u8),
        "INT16" => pack!(i16),
        "UINT16" => pack!(u16),
        "INT32" | "CDXCoordinate" => pack!(i32),
        "UINT32" | "CDXObjectID" => pack!(u32),
        _ => Err("Unsupported binary number type".into()),
    }
}

pub(super) fn decode_number(p: &Property, data: &[u8]) -> Result<String> {
    let mut r = Reader::new(data);
    let mut n = match kind(p) {
        "INT8" => i8::from_le_bytes(r.array()?) as f64,
        "UINT8" => u8::from_le_bytes(r.array()?) as f64,
        "INT16" => i16::from_le_bytes(r.array()?) as f64,
        "UINT16" => r.u16()? as f64,
        "INT32" | "CDXCoordinate" => i32::from_le_bytes(r.array()?) as f64,
        "UINT32" | "CDXObjectID" => r.u32()? as f64,
        "FLOAT64" => f64::from_le_bytes(r.array()?),
        _ => return Err("Unsupported binary number type".into()),
    };
    r.done()?;
    if !n.is_finite() {
        return Err("Non-finite binary number".into());
    }
    if p.name == "DoublePosition" && matches!(n as i64, 0..=2) {
        return Ok("auto".into());
    }
    let variants = variants(p);
    if !variants.is_empty() {
        if let Some((name, _)) = variants.iter().rev().find(|(_, v)| *v as f64 == n) {
            return Ok(if line_height(p.name) {
                name.to_lowercase()
            } else {
                (*name).to_owned()
            });
        }
        if bitfield(p.name) {
            if p.name == "CurveType" {
                return Ok((n as i64).to_string());
            }
            let n = n as i64;
            let flags = variants
                .iter()
                .filter(|(_, v)| *v != 0 && n & v == *v)
                .collect::<Vec<_>>();
            if flags.iter().fold(0, |bits, (_, v)| bits | v) == n {
                return Ok(flags.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(" "));
            }
        }
        if !line_height(p.name) {
            return Err(format!("Unsupported binary enumeration: {}", p.name));
        }
    }
    if scaled(p) {
        n /= 65536.;
    }
    if p.name == "BondSpacing" {
        n /= 10.;
    }
    general(n)
}

/// Match the reference's eight-significant-digit coordinate/number spelling.
fn general(n: f64) -> Result<String> {
    let scientific = format!("{n:.7e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .ok_or("Invalid formatted number")?;
    let exp = exponent
        .parse::<i32>()
        .map_err(|_| "Invalid numeric exponent")?;
    if !(-4..8).contains(&exp) {
        Ok(format!(
            "{}e{exp:+03}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        ))
    } else {
        let places = usize::try_from((7 - exp).max(0)).map_err(|_| "Invalid number precision")?;
        let text = format!("{n:.places$}");
        Ok(if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            text
        })
    }
}

pub(super) fn encode_coordinates(value: &str, kind: &str) -> Result<Vec<u8>> {
    let n = match kind {
        "CDXPoint2D" => 2,
        "CDXPoint3D" => 3,
        "CDXRectangle" => 4,
        _ => return Err("Invalid coordinate type".into()),
    };
    let mut values = value
        .split_whitespace()
        .map(finite)
        .collect::<Result<Vec<_>>>()?;
    if values.len() != n {
        return Err("Invalid drawing coordinates".into());
    }
    reorder(&mut values);
    let mut result = Vec::new();
    for v in values {
        result.extend(pack_number("INT32", v * 65536.)?);
    }
    Ok(result)
}
pub(super) fn decode_coordinates(data: &[u8], kind: &str) -> Result<String> {
    let n = match kind {
        "CDXPoint2D" => 2,
        "CDXPoint3D" => 3,
        "CDXRectangle" => 4,
        _ => return Err("Invalid coordinate type".into()),
    };
    let mut r = Reader::new(data);
    let mut values = Vec::new();
    for _ in 0..n {
        values.push(i32::from_le_bytes(r.array()?) as f64 / 65536.);
    }
    r.done()?;
    reorder(&mut values);
    Ok(values
        .into_iter()
        .map(general)
        .collect::<Result<Vec<_>>>()?
        .join(" "))
}
fn reorder(values: &mut [f64]) {
    // Pattern matching avoids unchecked indexing even for malformed input.
    match values {
        [x, y] => std::mem::swap(x, y),
        [x, y, z, w] => {
            std::mem::swap(x, y);
            std::mem::swap(z, w);
        }
        _ => {}
    }
}
