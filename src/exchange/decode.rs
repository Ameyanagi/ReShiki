use super::{values::*, *};
use std::collections::HashSet;

/// Read only document-level style properties from stationery. Unlike a drawing
/// import, template artwork may contain arbitrary objects: traverse their binary
/// framing, but do not interpret or import them. Older CDS files omit the root
/// object header after their 28-byte file header.
pub(crate) fn style_from_cdx(data: &[u8]) -> Result<String> {
    if data.len() > LIMIT || !data.starts_with(b"VjCD0100\x04\x03\x02\x01") {
        return Err("Invalid or oversized ChemDraw stationery".into());
    }
    let mut r = Reader::new(data);
    if data.get(22..24) == Some(&[0, 0x80]) {
        r.take(28)?;
    } else if data.get(28..30) == Some(&[0, 0x80]) {
        r.take(34)?;
    } else {
        r.take(28)?;
    }
    let schema = Schema::new();
    let mut tree = Tree {
        name: "CDXML",
        id: 0,
        raw: Vec::new(),
        children: Vec::new(),
    };
    let (mut depth, mut objects, mut properties) = (0usize, 0usize, 0usize);
    let mut seen = HashSet::new();
    loop {
        let tag = r.u16()?;
        if tag == 0 {
            if depth == 0 {
                break;
            }
            depth -= 1;
        } else if tag >= 0x8000 {
            r.u32()?;
            depth += 1;
            objects += 1;
            if depth > 64 || objects > MAX_OBJECTS {
                return Err("Stationery object limit exceeded".into());
            }
        } else {
            properties += 1;
            if properties > MAX_PROPERTIES {
                return Err("Stationery property limit exceeded".into());
            }
            let size = r.u16()?;
            let size = if size == u16::MAX {
                r.u32()? as usize
            } else {
                usize::from(size)
            };
            let value = r.take(size)?;
            let selected = matches!(tag, 0x100 | 0x80a)
                || schema.codes.get(&tag).is_some_and(|p| {
                    matches!(
                        p.name,
                        "BondLength"
                            | "BondSpacing"
                            | "LineWidth"
                            | "BoldWidth"
                            | "MarginWidth"
                            | "HashSpacing"
                            | "LabelFont"
                            | "LabelSize"
                    )
                });
            if depth == 0 && selected {
                if !seen.insert(tag) {
                    return Err("Duplicate stationery style property".into());
                }
                tree.raw.push((tag, value));
            }
        }
    }
    if !matches!(r.remaining, [] | [0, 0]) {
        return Err("Unexpected trailing stationery data".into());
    }
    let root = convert(&tree, &schema, &mut HashMap::new())?;
    let mut xml = String::new();
    root.write(&mut xml)?;
    Ok(xml)
}

/// Decode the supported binary drawing subset. Unknown drawing features fail
/// explicitly rather than silently yielding a chemically different structure.
pub fn from_cdx(data: &[u8]) -> Result<String> {
    if data.len() > LIMIT {
        return Err("Drawing exceeds the 16 MB structure limit".into());
    }
    if !data.starts_with(b"VjCD0100\x04\x03\x02\x01") {
        return Err("Invalid binary drawing header".into());
    }
    let start = if data.get(22..24) == Some(&[0, 0x80]) {
        22
    } else if data.get(28..30) == Some(&[0, 0x80]) {
        28
    } else {
        return Err("Binary drawing has no document object".into());
    };
    let mut r = Reader::new(data);
    r.take(start)?;
    let mut parser = Parser {
        r,
        objects: 0,
        properties: 0,
        ids: HashSet::new(),
    };
    let code = parser.r.u16()?;
    let tree = parser.object(code, 0)?;
    if tree.name != "CDXML" {
        return Err("Expected a binary document".into());
    }
    if !matches!(parser.r.remaining, [] | [0, 0]) {
        return Err("Unexpected trailing binary drawing data".into());
    }
    let mut fonts = HashMap::new();
    let root = convert(&tree, &Schema::new(), &mut fonts)?;
    let mut xml = String::new();
    root.write(&mut xml)?;
    Ok(xml)
}
struct Tree<'a> {
    name: &'static str,
    id: u32,
    raw: Vec<(u16, &'a [u8])>,
    children: Vec<Tree<'a>>,
}
struct Parser<'a> {
    r: Reader<'a>,
    objects: usize,
    properties: usize,
    ids: HashSet<u32>,
}
impl<'a> Parser<'a> {
    fn object(&mut self, code: u16, depth: usize) -> Result<Tree<'a>> {
        self.objects += 1;
        if depth > 64 || self.objects > MAX_OBJECTS {
            return Err("Drawing object limit exceeded".into());
        }
        let name = OBJECTS
            .iter()
            .find(|(c, _)| *c == code)
            .ok_or_else(|| format!("Unsupported binary drawing object 0x{code:04x}"))?
            .1;
        let id = self.r.u32()?;
        if id != 0 && !self.ids.insert(id) {
            return Err("Duplicate binary drawing object identifier".into());
        }
        let mut tree = Tree {
            name,
            id,
            raw: Vec::new(),
            children: Vec::new(),
        };
        loop {
            let tag = self.r.u16()?;
            if tag == 0 {
                break;
            }
            if tag >= 0x8000 {
                tree.children.push(self.object(tag, depth + 1)?);
                continue;
            }
            self.properties += 1;
            if self.properties > MAX_PROPERTIES {
                return Err("Drawing property limit exceeded".into());
            }
            let size = self.r.u16()?;
            let size = if size == u16::MAX {
                usize::try_from(self.r.u32()?).map_err(|_| "Drawing property is too large")?
            } else {
                usize::from(size)
            };
            tree.raw.push((tag, self.r.take(size)?));
        }
        Ok(tree)
    }
}
#[derive(Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
    text: String,
}
impl Element {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    fn set(&mut self, key: &str, value: impl ToString) {
        if let Some((_, v)) = self.attrs.iter_mut().find(|(k, _)| k == key) {
            *v = value.to_string();
        } else {
            self.attrs.push((key.into(), value.to_string()));
        }
    }
    fn write(&self, out: &mut String) -> Result<()> {
        put(out, "<")?;
        put(out, &self.name)?;
        for (key, value) in &self.attrs {
            put(out, " ")?;
            put(out, key)?;
            put(out, "=\"")?;
            escape(out, value, true)?;
            put(out, "\"")?;
        }
        if self.children.is_empty() && self.text.is_empty() {
            return put(out, " />");
        }
        put(out, ">")?;
        escape(out, &self.text, false)?;
        for child in &self.children {
            child.write(out)?;
        }
        put(out, "</")?;
        put(out, &self.name)?;
        put(out, ">")
    }
}
fn put(out: &mut String, text: &str) -> Result<()> {
    // Hex images and XML escaping can expand a bounded binary input.
    if text.len() > (128 * 1024 * 1024usize).saturating_sub(out.len()) {
        return Err("Decoded drawing XML is too large".into());
    }
    out.push_str(text);
    Ok(())
}
fn escape(out: &mut String, text: &str, attribute: bool) -> Result<()> {
    for ch in text.chars() {
        match ch {
            '&' => put(out, "&amp;")?,
            '<' => put(out, "&lt;")?,
            '>' => put(out, "&gt;")?,
            '"' if attribute => put(out, "&quot;")?,
            '\n' if attribute => put(out, "&#10;")?,
            '\r' if attribute => put(out, "&#13;")?,
            '\t' if attribute => put(out, "&#9;")?,
            ch if ch < ' ' && !matches!(ch, '\n' | '\r' | '\t')
                || matches!(ch, '\u{fffe}' | '\u{ffff}') =>
            {
                return Err("Invalid XML character in drawing text".into());
            }
            ch => put(out, ch.encode_utf8(&mut [0; 4]))?,
        }
    }
    Ok(())
}
fn convert(tree: &Tree<'_>, schema: &Schema, fonts: &mut HashMap<u16, u16>) -> Result<Element> {
    let mut el = Element::new(tree.name);
    if tree.id != 0 {
        el.set("id", tree.id);
    }
    // Tables must be read before text, even when serialized after it.
    for (tag, data) in &tree.raw {
        if *tag == 0x100 {
            let mut r = Reader::new(data);
            r.u16()?;
            let mut table = Element::new("fonttable");
            for _ in 0..r.u16()? {
                let id = r.u16()?;
                let charset = r.u16()?;
                let size = r.u16()?;
                let encoding = charset_name(charset)?;
                let name = decode_string(
                    r.take(usize::from(size))?,
                    if charset == 65001 { 65001 } else { 10000 },
                )?;
                fonts.insert(id, charset);
                let mut font = Element::new("font");
                font.set("id", id);
                font.set("name", name);
                font.set("charset", encoding);
                table.children.push(font);
            }
            r.done()?;
            el.children.push(table);
        }
    }
    let (mut text, mut utf8_text) = (None, None);
    for (tag, data) in &tree.raw {
        if *tag == 0x100 {
            continue;
        }
        let Some(p) = schema.codes.get(tag) else {
            if matches!(tree.name, "CDXML" | "page")
                || tree.name == "n" && matches!(tag, 0x448 | 0x44d)
            {
                continue;
            }
            return Err(format!("Unsupported binary drawing property 0x{tag:04x}"));
        };
        if *tag == 0x700 {
            text = Some(*data);
            continue;
        }
        if *tag == 0x709 {
            utf8_text = Some(*data);
            continue;
        }
        if numeric(p.kind) {
            el.set(p.name, decode_number(p, data)?);
            continue;
        }
        let mut r = Reader::new(data);
        match p.kind {
            "CDXBoolean" | "CDXBooleanImplied" => el.set(
                p.name,
                match *data {
                    [] | [1] => "yes",
                    [0] => "no",
                    _ => return Err("Invalid drawing boolean".into()),
                },
            ),
            "CDXPoint2D" | "CDXPoint3D" | "CDXRectangle" => {
                el.set(p.name, decode_coordinates(data, p.kind)?)
            }
            "CDXString" => el.set(
                p.name,
                decode_text(data, fonts, false)?
                    .into_iter()
                    .map(|e| e.text)
                    .collect::<String>(),
            ),
            "CDXColorTable" => {
                let mut table = Element::new("colortable");
                for _ in 0..r.u16()? {
                    let mut color = Element::new("color");
                    for axis in ["r", "g", "b"] {
                        color.set(axis, r.u16()? as f64 / 65535.);
                    }
                    table.children.push(color);
                }
                r.done()?;
                el.children.push(table);
            }
            "CDXFontStyle" => {
                let prefix = if *tag == 0x80a { "Label" } else { "Caption" };
                for suffix in ["Font", "Face", "Size", "Color"] {
                    let n = r.u16()? as f64 / if suffix == "Size" { 20. } else { 1. };
                    el.set(&format!("{prefix}{suffix}"), n);
                }
                r.done()?;
            }
            "CDXCurvePoints" => {
                let mut values = Vec::new();
                for _ in 0..r.u16()? {
                    values.push(decode_coordinates(r.take(8)?, "CDXPoint2D")?);
                }
                r.done()?;
                el.set(p.name, values.join(" "));
            }
            "CDXObjectIDArray" | "CDXObjectIDArrayWithCounts" | "INT16ListWithCounts" => {
                let count = if p.kind == "CDXObjectIDArray" {
                    data.len() / 4
                } else {
                    usize::from(r.u16()?)
                };
                let mut values = Vec::new();
                for _ in 0..count {
                    values.push(if p.kind != "INT16ListWithCounts" {
                        r.u32()?.to_string()
                    } else {
                        r.u16()?.to_string()
                    });
                }
                r.done()?;
                el.set(p.name, values.join(" "));
            }
            "CDXRepresentsProperty" => {
                let id = r.u32()?;
                let attr = r.u16()?;
                r.done()?;
                let p = schema
                    .codes
                    .get(&attr)
                    .ok_or("Unsupported represented property")?;
                let mut child = Element::new("represent");
                child.set("object", id);
                child.set("attribute", p.name);
                el.children.push(child);
            }
            "varies" if p.name == "Value" => {
                if !data.is_empty() {
                    return Err("Unsupported binary object-tag value".into());
                }
            }
            "Unformatted" if tree.name == "embeddedobject" => el.set(
                p.name,
                data.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            ),
            "Unformatted" | "CDXDate" if matches!(tree.name, "CDXML" | "page") => {}
            _ => return Err(format!("Unsupported binary drawing property: {}", p.name)),
        }
    }
    if let Some(data) = utf8_text.or(text) {
        el.children
            .extend(decode_text(data, fonts, utf8_text.is_some())?);
    }
    for child in &tree.children {
        el.children.push(convert(child, schema, fonts)?);
    }
    Ok(el)
}
struct Run {
    start: usize,
    font: Option<u16>,
    face: u16,
    size: u16,
    color: u16,
}
fn decode_text(data: &[u8], fonts: &HashMap<u16, u16>, utf8: bool) -> Result<Vec<Element>> {
    let mut r = Reader::new(data);
    let mut runs = Vec::new();
    for _ in 0..r.u16()? {
        runs.push(Run {
            start: usize::from(r.u16()?),
            font: Some(r.u16()?),
            face: r.u16()?,
            size: r.u16()?,
            color: r.u16()?,
        });
    }
    if runs.is_empty() {
        runs.push(Run {
            start: 0,
            font: None,
            face: 0,
            size: 0,
            color: 0,
        });
    }
    if runs.first().is_some_and(|r| r.start != 0) {
        return Err("Invalid binary text style ranges".into());
    }
    let mut result = Vec::new();
    let mut iter = runs.iter().peekable();
    while let Some(run) = iter.next() {
        let end = iter.peek().map_or(r.remaining.len(), |next| next.start);
        let bytes = r
            .remaining
            .get(run.start..end)
            .ok_or("Invalid binary text style ranges")?;
        let charset = if utf8 {
            65001
        } else {
            run.font.and_then(|id| fonts.get(&id)).copied().unwrap_or(0)
        };
        let mut span = Element::new("s");
        span.text = decode_string(bytes, charset)?.replace('\r', "\n");
        if let Some(font) = run.font {
            span.set("font", font);
            span.set("face", run.face);
            span.set("size", run.size as f64 / 20.);
            span.set("color", run.color);
        }
        result.push(span);
    }
    Ok(result)
}
fn charset_name(charset: u16) -> Result<&'static str> {
    match charset {
        65001 => Ok("utf-8"),
        10000 => Ok("mac_roman"),
        1252 => Ok("cp1252"),
        0 => Ok("latin-1"),
        932 => Ok("cp932"),
        936 => Ok("gbk"),
        949 => Ok("cp949"),
        950 => Ok("big5"),
        1251 => Ok("cp1251"),
        _ => Err(format!("Unsupported drawing text charset {charset}")),
    }
}
fn decode_string(bytes: &[u8], charset: u16) -> Result<String> {
    if charset == 0 {
        return Ok(bytes.iter().map(|b| char::from(*b)).collect());
    }
    if charset == 65001 {
        return std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| "Invalid binary drawing text encoding".into());
    }
    super::charsets::decode(bytes, charset)
}
