//! Native-precision drawing styles and rich text from bounded, read-only XML.
//! Conversion to editable document types is an explicit later step: equality,
//! paragraph calculations and palette rounding first use the original f64 data.
mod style;
mod text;

use super::numeric;
pub use super::numeric::Error as NumericError;
use roxmltree::{Document, Node};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
pub use style::{NativeDrawingStyle, drawing_style};
pub use text::{NativeFormat, NativeSpan, NativeText, NativeTextStyle, TextReader};

pub type Attributes = BTreeMap<String, String>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Numeric(#[from] NumericError),
    #[error("Invalid CDXML presentation XML: {0}")]
    Xml(String),
    #[error("CDXML presentation exceeds the size, nesting, or object limit")]
    Limit,
    #[error("Invalid drawing style {field}: expected {minimum}–{maximum}")]
    StyleRange {
        field: &'static str,
        minimum: f64,
        maximum: f64,
    },
    #[error("Invalid drawing style {0}")]
    StyleName(&'static str),
    #[error("Invalid drawing style coordinate units")]
    StyleCoordinates,
    #[error("Drawing style uses incompatible coordinate units")]
    StyleScale,
    #[error("Invalid drawing style bold width or image resolution")]
    StyleBold,
    #[error("cannot convert float NaN to integer")]
    ColorNaN,
    #[error("cannot convert float infinity to integer")]
    ColorInfinite,
    #[error("Outlined/shadowed CDXML text is not supported yet.")]
    Face,
    #[error("Unsupported CDXML text color or font size")]
    TextColorSize,
    #[error("CDXML text has no supported style runs")]
    Runs,
    #[error("This CDXML text alignment is not supported yet")]
    Alignment,
    #[error("This CDXML paragraph spacing or width is not supported yet")]
    Paragraph,
    #[error("Rotated CDXML text is not supported yet")]
    Rotation,
    #[error("CDXML color is outside the unsigned 8-bit document range")]
    ColorBoundary,
    #[error("{0}")]
    Document(String),
    #[error("Invalid native drawing style defaults: {0}")]
    Defaults(String),
}
type Result<T> = std::result::Result<T, Error>;

const BYTE_LIMIT: usize = 16 * 1024 * 1024;
const NODE_LIMIT: u32 = 400_000;
const OBJECT_LIMIT: usize = 100_000;
const PROPERTY_LIMIT: usize = 1_000_000;

fn named(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

fn bounded(document: &Document<'_>) -> Result<()> {
    if document.input_text().len() > BYTE_LIMIT
        || super::xml_guard::has_entity_declaration(document.input_text())
    {
        return Err(Error::Limit);
    }
    let (mut objects, mut properties) = (0usize, 0usize);
    for (index, node) in document.descendants().enumerate() {
        if index >= NODE_LIMIT as usize {
            return Err(Error::Limit);
        }
        if !node.is_element() {
            continue;
        }
        objects += 1;
        properties += node.attributes().len();
        if objects > OBJECT_LIMIT
            || properties > PROPERTY_LIMIT
            || node.ancestors().take(67).count() > 66
        {
            return Err(Error::Limit);
        }
    }
    Ok(())
}

/// Parse with the same explicit limits applied to externally supplied Nodes.
/// External DTD declarations may be present, but entity definitions are rejected.
pub fn parse(text: &str) -> Result<Document<'_>> {
    if text.len() > BYTE_LIMIT || super::xml_guard::has_entity_declaration(text) {
        return Err(Error::Limit);
    }
    let document = Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: NODE_LIMIT,
        },
    )
    .map_err(|error| Error::Xml(error.to_string()))?;
    bounded(&document)?;
    Ok(document)
}

/// Raw integer-valued channels, including negative and very large values. A
/// rounded finite binary float denotes an exact integer even beyond i128; using
/// f64 here preserves the native palette without eagerly imposing u8 bounds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct NativeColor(pub [f64; 3]);
impl NativeColor {
    pub fn into_document(self) -> Result<[u8; 3]> {
        if self
            .0
            .iter()
            .any(|n| !n.is_finite() || !(0.0..=255.0).contains(n) || n.fract() != 0.0)
        {
            return Err(Error::ColorBoundary);
        }
        Ok(self.0.map(|n| n as u8))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Palette {
    pub colors: Vec<NativeColor>,
}
impl Palette {
    fn from_root(root: Node<'_, '_>) -> Result<Self> {
        let mut colors = vec![NativeColor([0.0; 3]), NativeColor([255.0; 3])];
        for color in root
            .children()
            .filter(|n| named(*n, "colortable"))
            .flat_map(|table| table.children().filter(|n| named(*n, "color")))
        {
            let mut channels = [0.0; 3];
            for (axis, channel) in ["r", "g", "b"].into_iter().zip(&mut channels) {
                let value = numeric::float(color.attribute(axis).unwrap_or("0"))? * 255.0;
                if value.is_nan() {
                    return Err(Error::ColorNaN);
                }
                if value.is_infinite() {
                    return Err(Error::ColorInfinite);
                }
                *channel = value.round_ties_even();
            }
            colors.push(NativeColor(channels));
        }
        if colors.len() == 2 {
            colors.extend([NativeColor([255.0; 3]), NativeColor([0.0; 3])]);
        }
        Ok(Self { colors })
    }
}

/// Construct the native text palette without rejecting unused wide channels.
pub fn palette(root: Node<'_, '_>) -> Result<Palette> {
    bounded(root.document())?;
    Palette::from_root(root)
}

struct Fonts {
    named: HashMap<String, String>,
    missing_id: Option<String>,
}
impl Fonts {
    fn from_root(root: Node<'_, '_>) -> Self {
        let mut named_fonts = HashMap::new();
        let mut missing_id = None;
        for font in root
            .children()
            .filter(|n| named(*n, "fonttable"))
            .flat_map(|table| table.children().filter(|n| named(*n, "font")))
        {
            let name = font.attribute("name").unwrap_or("Arial").to_owned();
            if let Some(id) = font.attribute("id") {
                named_fonts.insert(id.to_owned(), name);
            } else {
                missing_id = Some(name);
            }
        }
        Self {
            named: named_fonts,
            missing_id,
        }
    }
    fn get(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.named.get(id))
            .or_else(|| id.is_none().then_some(self.missing_id.as_ref()).flatten())
            .map(String::as_str)
            .unwrap_or("Arial")
            .to_owned()
    }
}
