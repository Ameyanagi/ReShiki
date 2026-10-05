//! Transactional document formatting, independent of chemistry and the interface.
use crate::{document::Document, editing, style::DrawingStyle, typography::TextStyle};

pub fn load(path: &std::path::Path) -> Result<DrawingStyle, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let chemdraw = path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["cds", "cdx", "cdxml"]
            .iter()
            .any(|extension| s.eq_ignore_ascii_case(extension))
    });
    let limit = if chemdraw {
        crate::exchange::LIMIT
    } else {
        64 * 1024
    };
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(if chemdraw {
            "Stationery exceeds 16 MB."
        } else {
            "Style file exceeds 64 KB."
        }
        .into());
    }
    if chemdraw {
        let xml = if bytes.starts_with(b"VjCD") {
            crate::exchange::style_from_cdx(&bytes)?
        } else {
            String::from_utf8(bytes).map_err(|_| "Invalid ChemDraw stationery encoding")?
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported ChemDraw style");
        return from_chemdraw_xml(&xml, name);
    }
    let style: DrawingStyle =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid drawing style: {e}"))?;
    style.validate()?;
    Ok(style)
}

/// Export dimension/font settings without carrying canvas colors or artwork.
/// CDS uses ChemDraw's binary document framing; native JSON retains ReShiki's
/// world-coordinate scale and PNG preference as well as physical dimensions.
pub fn save(path: &std::path::Path, style: &DrawingStyle) -> Result<(), String> {
    style.validate()?;
    let bytes = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cds"))
    {
        let doc = Document {
            drawing_style: style.clone(),
            ..Default::default()
        };
        let xml =
            crate::exchange::drawing::write(&doc, Default::default()).map_err(|e| e.to_string())?;
        crate::exchange::to_cds(&xml)?
    } else {
        serde_json::to_vec_pretty(style).map_err(|e| e.to_string())?
    };
    crate::storage::write_atomic(path, &bytes)
}

/// Stationery supplies label typography and bond dimensions. Page layout,
/// artwork, colors and independent caption styles are outside DrawingStyle.
fn from_chemdraw_xml(xml: &str, name: &str) -> Result<DrawingStyle, String> {
    let tree = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 100_000,
        },
    )
    .map_err(|e| format!("Invalid ChemDraw style: {e}"))?;
    let root = tree.root_element();
    if root.tag_name().name() != "CDXML" {
        return Err("Expected ChemDraw CDXML stationery".into());
    }
    // Do not fill absent ChemDraw values with unrelated ReShiki defaults.
    let attribute = |key| {
        root.attribute(key)
            .ok_or_else(|| format!("Stationery is missing {key}"))
    };
    let number = |key| -> Result<f32, String> {
        attribute(key)?
            .parse()
            .map_err(|_| format!("Invalid stationery {key}"))
    };
    let font_id = attribute("LabelFont")?;
    let font = root
        .children()
        .filter(|n| n.has_tag_name("fonttable"))
        .flat_map(|n| n.children())
        .find(|n| n.has_tag_name("font") && n.attribute("id") == Some(font_id))
        .and_then(|n| n.attribute("name"))
        .ok_or("Stationery label font is missing from its font table")?;
    let mut style = DrawingStyle {
        name: name.chars().take(120).collect(),
        font_family: font.into(),
        font_size_pt: number("LabelSize")?,
        line_width_pt: number("LineWidth")?,
        bold_width_pt: number("BoldWidth")?,
        margin_width_pt: number("MarginWidth")?,
        hash_spacing_pt: number("HashSpacing")?,
        bond_spacing_ratio: number("BondSpacing")? / 100.,
        ..Default::default()
    };
    style.set_bond_length(number("BondLength")?);
    style.validate()?;
    Ok(style)
}

/// Publisher drawing recommendations. Canvas colors and interface appearance are independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Jacs,
    Nature,
    Rsc,
    Angewandte,
    Synthesis,
    // Retained for existing style files and the public API; not a journal choice.
    Presentation,
}
impl Preset {
    pub const ALL: [Self; 5] = [
        Self::Jacs,
        Self::Nature,
        Self::Rsc,
        Self::Angewandte,
        Self::Synthesis,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Jacs => "acs-guidance",
            Self::Nature => "nature-publisher",
            Self::Rsc => "rsc-guidance",
            Self::Angewandte => "angewandte-publisher",
            Self::Synthesis => "synthesis-guidance",
            Self::Presentation => "presentation",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Jacs => "ACS published values; existing ReShiki default.",
            Self::Nature => "Official Nature stylesheet: Helvetica, 6 pt labels.",
            Self::Rsc => {
                "RSC written pt values; label margin retained from its download. The downloaded template differs."
            }
            Self::Angewandte => "Official Angewandte download (legacy ChemDraw 4.5 stationery).",
            Self::Synthesis => {
                "Final-size dimensions from the 2026 instructions. Arial retained from bundled stationery; 6 pt labels."
            }
            Self::Presentation => "Presentation",
        }
    }
    pub fn source_url(self) -> Option<&'static str> {
        Some(match self {
            Self::Jacs => {
                "https://pubsapp.acs.org/paragonplus/submission/general/graphics_prep.html"
            }
            Self::Nature => "https://www.nature.com/nature/for-authors/formatting-guide",
            Self::Rsc => {
                "https://www.rsc.org/publishing/publish-with-us/publish-a-journal-article/analytical-methods"
            }
            Self::Angewandte => {
                "https://onlinelibrary.wiley.com/page/journal/15213773/homepage/notice-to-authors"
            }
            Self::Synthesis => {
                "https://www.thieme.de/statics/dokumente/thieme/final/de/dokumente/zw_synthesis/Instr_SS_26_feb.pdf"
            }
            Self::Presentation => return None,
        })
    }
    fn json(self) -> &'static str {
        match self {
            Self::Jacs => include_str!("../../../presets/drawing/acs-guidance.reshiki-style"),
            Self::Nature => include_str!("../../../presets/drawing/nature-publisher.reshiki-style"),
            Self::Rsc => include_str!("../../../presets/drawing/rsc-guidance.reshiki-style"),
            Self::Angewandte => {
                include_str!("../../../presets/drawing/angewandte-publisher.reshiki-style")
            }
            Self::Synthesis => {
                include_str!("../../../presets/drawing/synthesis-guidance.reshiki-style")
            }
            Self::Presentation => {
                include_str!("../../../presets/drawing/presentation.reshiki-style")
            }
        }
    }
    pub fn style(self) -> DrawingStyle {
        serde_json::from_str(self.json()).unwrap_or_default()
    }
}
impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Jacs => "JACS / ACS",
            Self::Nature => "Nature",
            Self::Rsc => "RSC",
            Self::Angewandte => "Angewandte",
            Self::Synthesis => "SYNLETT / SYNTHESIS",
            Self::Presentation => "Presentation",
        })
    }
}

fn matching_text(text: &mut TextStyle, old: &DrawingStyle, new: &DrawingStyle) {
    if text.family == old.font_family {
        text.family = new.font_family.clone();
    }
    if (text.size_pt - old.font_size_pt).abs() < 0.001 {
        text.size_pt = new.font_size_pt;
    }
}

/// Return a validated snapshot. Cancelling or invalid input never mutates a drawing.
/// Objects with different explicit sizes/fonts/line widths retain those overrides.
pub fn apply(
    original: &Document,
    style: DrawingStyle,
    update_matching: bool,
    scale_layout: bool,
) -> Result<Document, String> {
    style.validate()?;
    original.validate()?;
    let mut doc = original.clone();
    let old = &original.drawing_style;
    if scale_layout {
        let ids = doc.all_ids();
        let pivot = editing::center(&doc, &ids);
        editing::transform_about(
            &mut doc,
            &ids,
            pivot,
            style.bond_length_pt / old.bond_length_pt,
            0.,
        );
    }
    if update_matching {
        for atom in &mut doc.atoms {
            if let Some(text) = &mut atom.text_style {
                matching_text(text, old, &style);
            }
        }
        for group in &mut doc.abbreviations {
            if let Some(text) = &mut group.label_style {
                matching_text(text, old, &style);
            }
        }
        for label in &mut doc.annotations {
            matching_text(&mut label.format.style, old, &style);
            for span in &mut label.format.spans {
                matching_text(&mut span.style, old, &style);
            }
        }
        for arrow in &mut doc.arrows {
            let mut appearance = arrow.appearance();
            if (appearance.width_pt - old.line_width_pt).abs() < 0.001 {
                appearance.width_pt = style.line_width_pt;
                if arrow.appearance() != appearance {
                    arrow.style = Some(appearance);
                }
            }
        }
        for graphic in &mut doc.graphics {
            if (graphic.style.width_pt - old.line_width_pt).abs() < 0.001 {
                graphic.style.width_pt = style.line_width_pt;
            }
        }
    }
    doc.version = doc.version.max(15);
    doc.drawing_style = style;
    doc.validate()?;
    Ok(doc)
}

#[cfg(test)]
mod tests;
