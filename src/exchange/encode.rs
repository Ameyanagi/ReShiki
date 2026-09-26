use super::{values::*, *};
use roxmltree::Node;
use std::collections::HashSet;

/// Convert checked drawing XML into its binary representation without RDKit.
pub fn to_cdx(xml: &str) -> Result<Vec<u8>> {
    encode(xml, false)
}
/// ChemDraw stationery also requires packed document font defaults; individual
/// LabelFont/CaptionFont properties alone are ignored when opening a CDS file.
pub fn to_cds(xml: &str) -> Result<Vec<u8>> {
    encode(xml, true)
}
fn encode(xml: &str, stationery: bool) -> Result<Vec<u8>> {
    if xml.len() > LIMIT {
        return Err("Drawing exceeds the 16 MB structure limit".into());
    }
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            nodes_limit: 400_000,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Invalid drawing XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "CDXML" || root.tag_name().namespace().is_some() {
        return Err("Expected a CDXML drawing".into());
    }
    let mut next_id = 1u64;
    for el in root.descendants().filter(|n| n.is_element()) {
        if let Some(id) = el.attribute("id") {
            next_id = next_id.max(u64::from(integer::<u32>(id)?) + 1);
        }
    }
    let mut encoder = Encoder {
        schema: Schema::new(),
        stationery,
        next_id,
        count: 0,
        properties: 0,
        ids: HashSet::new(),
        out: HEADER.to_vec(),
    };
    encoder.object(root, 0)?;
    append(&mut encoder.out, &[0, 0])?;
    Ok(encoder.out)
}
struct Encoder {
    stationery: bool,
    schema: Schema,
    next_id: u64,
    count: usize,
    properties: usize,
    ids: HashSet<u32>,
    out: Vec<u8>,
}
impl Encoder {
    fn property(&mut self, code: u16, data: &[u8]) -> Result<()> {
        self.properties += 1;
        if self.properties > MAX_PROPERTIES {
            return Err("Drawing property limit exceeded".into());
        }
        property(&mut self.out, code, data)
    }
    fn object(&mut self, el: Node<'_, '_>, depth: usize) -> Result<()> {
        self.count += 1;
        if depth > 64 || self.count > MAX_OBJECTS {
            return Err("Drawing object limit exceeded".into());
        }
        let name = el.tag_name().name();
        let code = OBJECTS
            .iter()
            .find(|(_, n)| *n == name)
            .ok_or_else(|| format!("Unsupported binary drawing object: {name}"))?
            .0;
        if el.tag_name().namespace().is_some() {
            return Err("Unsupported drawing namespace".into());
        }
        let mut id = integer::<u32>(el.attribute("id").unwrap_or("0"))?;
        if id == 0 && depth != 0 {
            id = u32::try_from(self.next_id).map_err(|_| "Drawing object identifiers exhausted")?;
            self.next_id += 1;
        }
        if id != 0 && !self.ids.insert(id) {
            return Err("Duplicate drawing object identifier".into());
        }
        append(&mut self.out, &code.to_le_bytes())?;
        append(&mut self.out, &id.to_le_bytes())?;
        for attr in el.attributes() {
            if attr.name() == "id" {
                continue;
            }
            if attr.namespace().is_some() {
                return Err("Unsupported drawing attribute namespace".into());
            }
            let p =
                self.schema.names.get(attr.name()).ok_or_else(|| {
                    format!("Unsupported binary drawing property: {}", attr.name())
                })?;
            if p.kind == "CDXBooleanImplied" && attr.value() == "no" {
                continue;
            }
            let code = p.code;
            let data = encode_value(p, attr.value(), name)?;
            self.property(code, &data)?;
        }
        if self.stationery && name == "CDXML" {
            for (prefix, code) in [("Label", 0x080a), ("Caption", 0x080b)] {
                let mut data = Vec::new();
                for (suffix, default, scale) in [
                    ("Font", "3", 1.),
                    ("Face", "0", 1.),
                    ("Size", "10", 20.),
                    ("Color", "3", 1.),
                ] {
                    let key = format!("{prefix}{suffix}");
                    let value = finite(el.attribute(key.as_str()).unwrap_or(default))?;
                    data.extend(pack_number("UINT16", (value * scale).round())?);
                }
                self.property(code, &data)?;
            }
        }
        if name == "t" {
            self.property(0x700, &encode_text(el)?)?;
        }
        for child in el.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "s" => {}
                "fonttable" => self.property(0x100, &encode_fonts(child)?)?,
                "colortable" => self.property(0x300, &encode_colors(child)?)?,
                "represent" => {
                    let attr = child
                        .attribute("attribute")
                        .and_then(|n| self.schema.names.get(n))
                        .ok_or("Unsupported represented property")?;
                    let mut data = integer::<u32>(
                        child
                            .attribute("object")
                            .ok_or("Missing represented object")?,
                    )?
                    .to_le_bytes()
                    .to_vec();
                    data.extend_from_slice(&attr.code.to_le_bytes());
                    self.property(0xE, &data)?;
                }
                _ => self.object(child, depth + 1)?,
            }
        }
        append(&mut self.out, &[0, 0])
    }
}
fn encode_value(p: &Property, value: &str, element: &str) -> Result<Vec<u8>> {
    if numeric(p.kind) {
        return encode_number(p, value);
    }
    match p.kind {
        "CDXBooleanImplied" if value == "yes" => Ok(Vec::new()),
        "CDXBoolean" | "CDXBooleanImplied" => match value {
            "yes" => Ok(vec![1]),
            "no" => Ok(vec![0]),
            _ => Err("Invalid drawing boolean".into()),
        },
        "CDXPoint2D" | "CDXPoint3D" | "CDXRectangle" => encode_coordinates(value, p.kind),
        "CDXString" => {
            let mut data = vec![0, 0];
            for c in value.chars() {
                data.push(
                    u8::try_from(u32::from(c))
                        .map_err(|_| "Drawing metadata requires Latin-1 text")?,
                );
            }
            Ok(data)
        }
        "CDXObjectIDArray" | "CDXObjectIDArrayWithCounts" => {
            let mut data = if p.kind == "CDXObjectIDArrayWithCounts" {
                word(value.split_whitespace().count())?.to_vec()
            } else {
                Vec::new()
            };
            for v in value.split_whitespace() {
                append(&mut data, &integer::<u32>(v)?.to_le_bytes())?;
            }
            Ok(data)
        }
        "Unformatted" if element == "embeddedobject" => decode_hex(value),
        "CDXCurvePoints" => {
            let points = value.split_whitespace().collect::<Vec<_>>();
            if points.len() % 2 != 0 {
                return Err("Invalid curve points".into());
            }
            let mut data = word(points.len() / 2)?.to_vec();
            for pair in points.chunks_exact(2) {
                append(
                    &mut data,
                    &encode_coordinates(&pair.join(" "), "CDXPoint2D")?,
                )?;
            }
            Ok(data)
        }
        "INT16ListWithCounts" => {
            let numbers = value.split_whitespace().collect::<Vec<_>>();
            let mut data = word(numbers.len())?.to_vec();
            for n in numbers {
                append(&mut data, &integer::<u16>(n)?.to_le_bytes())?;
            }
            Ok(data)
        }
        _ => Err(format!("Unsupported binary property type: {}", p.name)),
    }
}
fn decode_hex(value: &str) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut chars = value.chars();
    while let Some(high) = chars.find(|c| !c.is_ascii_whitespace()) {
        let low = chars
            .next()
            .ok_or("Invalid embedded picture hexadecimal data")?;
        let (h, l) = (
            high.to_digit(16)
                .ok_or("Invalid embedded picture hexadecimal data")?,
            low.to_digit(16)
                .ok_or("Invalid embedded picture hexadecimal data")?,
        );
        append(&mut data, &[(h * 16 + l) as u8])?;
    }
    Ok(data)
}
fn encode_text(el: Node<'_, '_>) -> Result<Vec<u8>> {
    let spans = el
        .children()
        .filter(|n| n.has_tag_name("s"))
        .collect::<Vec<_>>();
    let mut data = word(spans.len())?.to_vec();
    let mut text = Vec::new();
    for span in spans {
        append(&mut data, &word(text.len())?)?;
        for (key, default) in [("font", "3"), ("face", "0")] {
            append(
                &mut data,
                &integer::<u16>(span.attribute(key).unwrap_or(default))?.to_le_bytes(),
            )?;
        }
        append(
            &mut data,
            &pack_number(
                "UINT16",
                finite(span.attribute("size").unwrap_or("10"))? * 20.,
            )?,
        )?;
        append(
            &mut data,
            &integer::<u16>(span.attribute("color").unwrap_or("3"))?.to_le_bytes(),
        )?;
        append(
            &mut text,
            span.text().unwrap_or("").replace('\n', "\r").as_bytes(),
        )?;
        if text.len() > 65535 {
            return Err("A binary text object must be shorter than 65536 UTF-8 bytes".into());
        }
    }
    append(&mut data, &text)?;
    Ok(data)
}
fn encode_fonts(el: Node<'_, '_>) -> Result<Vec<u8>> {
    let fonts = el.children().filter(|n| n.is_element()).collect::<Vec<_>>();
    let mut data = vec![0, 0];
    append(&mut data, &word(fonts.len())?)?;
    for font in fonts {
        let name = font.attribute("name").unwrap_or("Arial");
        append(
            &mut data,
            &integer::<u16>(font.attribute("id").ok_or("Missing font identifier")?)?.to_le_bytes(),
        )?;
        append(&mut data, &65001u16.to_le_bytes())?;
        append(&mut data, &word(name.len())?)?;
        append(&mut data, name.as_bytes())?;
    }
    Ok(data)
}
fn encode_colors(el: Node<'_, '_>) -> Result<Vec<u8>> {
    let colors = el.children().filter(|n| n.is_element()).collect::<Vec<_>>();
    let mut data = word(colors.len())?.to_vec();
    for color in colors {
        for axis in ["r", "g", "b"] {
            let n = finite(color.attribute(axis).unwrap_or("0"))?;
            if !(0.0..=1.0).contains(&n) {
                return Err("Invalid drawing color".into());
            }
            append(&mut data, &pack_number("UINT16", n * 65535.)?)?;
        }
    }
    Ok(data)
}
