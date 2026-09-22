use super::{Error, Fonts, Result, bounded, numeric};
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const POINTS_PER_WORLD: f64 = 14.4 / 42.0;

/// Exact native values, before the bridge's final f64-to-f32 document decoding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeDrawingStyle {
    pub name: String,
    pub font_family: String,
    pub bond_length_world: f64,
    pub bond_length_pt: f64,
    pub font_size_pt: f64,
    pub line_width_pt: f64,
    pub bold_width_pt: f64,
    pub margin_width_pt: f64,
    pub hash_spacing_pt: f64,
    pub bond_spacing_ratio: f64,
    pub png_dpi: u32,
}
impl NativeDrawingStyle {
    pub fn defaults() -> Result<Self> {
        static DEFAULT: OnceLock<std::result::Result<NativeDrawingStyle, String>> = OnceLock::new();
        DEFAULT
            .get_or_init(|| {
                serde_json::from_str(include_str!("../../../../engine/drawing_style.json"))
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .cloned()
            .map_err(|error| Error::Defaults(error.clone()))
    }
    fn checked(&self) -> Result<()> {
        for (field, value, minimum, maximum) in [
            ("bond_length_pt", self.bond_length_pt, 5.0, 100.0),
            ("font_size_pt", self.font_size_pt, 4.0, 144.0),
            ("line_width_pt", self.line_width_pt, 0.1, 6.0),
            ("bold_width_pt", self.bold_width_pt, 0.1, 12.0),
            ("margin_width_pt", self.margin_width_pt, 0.0, 12.0),
            ("hash_spacing_pt", self.hash_spacing_pt, 0.3, 12.0),
            ("bond_spacing_ratio", self.bond_spacing_ratio, 0.05, 0.4),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err(Error::StyleRange {
                    field,
                    minimum,
                    maximum,
                });
            }
        }
        for (field, value, limit) in [
            ("name", &self.name, 120),
            ("font_family", &self.font_family, 256),
        ] {
            if value.trim().is_empty() || value.chars().count() > limit {
                return Err(Error::StyleName(field));
            }
        }
        if !self.bond_length_world.is_finite() {
            return Err(Error::StyleCoordinates);
        }
        if (self.bond_length_world - self.bond_length_pt / POINTS_PER_WORLD).abs() > 0.001 {
            return Err(Error::StyleScale);
        }
        if self.bold_width_pt < self.line_width_pt || self.png_dpi != 1200 {
            return Err(Error::StyleBold);
        }
        Ok(())
    }
    /// Apply the real document boundary after preserving native style identity.
    pub fn into_document(self) -> Result<crate::style::DrawingStyle> {
        self.checked()?;
        let result = crate::style::DrawingStyle {
            name: self.name,
            font_family: self.font_family,
            bond_length_world: self.bond_length_world as f32,
            bond_length_pt: self.bond_length_pt as f32,
            font_size_pt: self.font_size_pt as f32,
            line_width_pt: self.line_width_pt as f32,
            bold_width_pt: self.bold_width_pt as f32,
            margin_width_pt: self.margin_width_pt as f32,
            hash_spacing_pt: self.hash_spacing_pt as f32,
            bond_spacing_ratio: self.bond_spacing_ratio as f32,
            png_dpi: self.png_dpi,
        };
        result.validate().map_err(Error::Document)?;
        Ok(result)
    }
}

pub fn drawing_style(root: Node<'_, '_>) -> Result<NativeDrawingStyle> {
    bounded(root.document())?;
    let fonts = Fonts::from_root(root);
    let defaults = NativeDrawingStyle::defaults()?;
    let mut style = defaults.clone();
    for (attribute, value) in [
        ("BondLength", &mut style.bond_length_pt),
        ("LabelSize", &mut style.font_size_pt),
        ("LineWidth", &mut style.line_width_pt),
        ("BoldWidth", &mut style.bold_width_pt),
        ("MarginWidth", &mut style.margin_width_pt),
        ("HashSpacing", &mut style.hash_spacing_pt),
    ] {
        if let Some(text) = root.attribute(attribute) {
            *value = numeric::float(text)?;
        } else if attribute == "BondLength" {
            *value = 30.0;
        }
    }
    style.font_family = fonts.get(root.attribute("LabelFont"));
    style.bond_spacing_ratio =
        numeric::float(root.attribute("BondSpacing").unwrap_or("18"))? / 100.0;
    style.bond_length_world = style.bond_length_pt / POINTS_PER_WORLD;
    if style != defaults {
        style.name = "Imported style".into();
    }
    style.checked()?;
    Ok(style)
}
