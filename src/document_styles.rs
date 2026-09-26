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
            Self::Jacs => include_str!("../presets/drawing/acs-guidance.reshiki-style"),
            Self::Nature => include_str!("../presets/drawing/nature-publisher.reshiki-style"),
            Self::Rsc => include_str!("../presets/drawing/rsc-guidance.reshiki-style"),
            Self::Angewandte => {
                include_str!("../presets/drawing/angewandte-publisher.reshiki-style")
            }
            Self::Synthesis => include_str!("../presets/drawing/synthesis-guidance.reshiki-style"),
            Self::Presentation => include_str!("../presets/drawing/presentation.reshiki-style"),
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
mod tests {
    use super::*;
    use crate::document::{Annotation, Point};

    #[test]
    fn journal_presets_use_publisher_settings() {
        let mut ids = std::collections::HashSet::new();
        for preset in Preset::ALL {
            assert!(ids.insert(preset.id()));
            let style: DrawingStyle = serde_json::from_str(preset.json()).unwrap();
            style.validate().unwrap();
            assert_eq!(style.name, preset.to_string());
        }
        assert_eq!(Preset::Jacs.style(), DrawingStyle::default());
        assert_eq!(Preset::Nature.style().font_family, "Helvetica");
        assert!((Preset::Angewandte.style().bold_width_pt - 2.6015625).abs() < 0.000001);
        assert_eq!(Preset::Rsc.style().line_width_pt, 0.5);
    }

    fn stationery() -> Vec<u8> {
        // Deliberately different from ReShiki defaults, with a nonstandard font ID.
        crate::exchange::to_cdx(
            r#"<CDXML BondLength="17" BondSpacing="18" LineWidth="0.75"
            BoldWidth="2.6015625" MarginWidth="2" HashSpacing="2.6015625"
            LabelFont="393" LabelSize="12"><fonttable><font id="393" name="Arial" charset="cp1252"/>
            </fonttable></CDXML>"#,
        )
        .unwrap()
    }

    #[test]
    fn reads_current_and_legacy_stationery_without_importing_artwork() {
        let directory = tempfile::tempdir().unwrap();
        let binary = stationery();
        assert_eq!(&binary[22..24], &[0, 0x80]);
        let padded = [&binary[..22], &[0u8; 6], &binary[22..]].concat();
        let legacy = [&binary[..22], &[0u8; 6], &binary[28..]].concat();
        for bytes in [binary, padded, legacy] {
            let path = directory.path().join("Publisher.CDS");
            std::fs::write(&path, &bytes).unwrap();
            let style = load(&path).unwrap();
            assert_eq!(style.name, "Publisher");
            assert_eq!(style.font_family, "Arial");
            assert_eq!(style.bond_length_pt, 17.);
            assert_eq!(style.bold_width_pt, 2.6015625);
            assert_eq!(style.font_size_pt, 12.);
        }
        let mut artwork = stationery();
        // Unknown object containing a large opaque property; valid framing, no drawing import.
        let end = artwork.len() - 4;
        let mut object = vec![0xfe, 0x8f, 7, 0, 0, 0, 0xfe, 0x7f, 0xff, 0xff];
        object.extend(70_000u32.to_le_bytes());
        object.extend(vec![0; 70_000]);
        object.extend([0, 0]);
        artwork.splice(end..end, object);
        let path = directory.path().join("Artwork.cds");
        std::fs::write(&path, &artwork).unwrap();
        assert_eq!(load(&path).unwrap().font_size_pt, 12.);
        assert!(
            crate::exchange::from_cdx(&artwork).is_err(),
            "Drawing imports remain strict"
        );
        for cut in [0, 12, 27, 35, artwork.len() - 3] {
            std::fs::write(&path, &artwork[..cut]).unwrap();
            assert!(load(&path).is_err(), "Truncated at {cut}");
        }
    }

    #[test]
    fn stationery_requires_explicit_valid_settings() {
        let xml = crate::exchange::from_cdx(&stationery()).unwrap();
        let style = from_chemdraw_xml(&xml, "Reference").unwrap();
        assert_eq!(style.line_width_pt, 0.75);
        for invalid in [
            xml.replace("BondLength=\"17\"", "BondLength=\"NaN\""),
            xml.replace("LabelFont=\"393\"", "LabelFont=\"99\""),
            xml.replace("HashSpacing=\"2.6015625\"", ""),
            xml.replace("LineWidth=\"0.75\"", "LineWidth=\"9\""),
        ] {
            assert!(from_chemdraw_xml(&invalid, "Reference").is_err());
        }
    }

    #[test]
    fn reads_legacy_packed_label_style_and_xml_files() {
        let binary = stationery();
        let mut legacy = [&binary[..22], &[0u8; 6]].concat();
        let mut pos = 28;
        while binary[pos..pos + 2] != [0, 0] {
            let tag = u16::from_le_bytes([binary[pos], binary[pos + 1]]);
            let size = u16::from_le_bytes([binary[pos + 2], binary[pos + 3]]) as usize;
            let end = pos + 4 + size;
            if !matches!(tag, 0x081a | 0x081c) {
                legacy.extend_from_slice(&binary[pos..end]);
            }
            pos = end;
        }
        legacy.extend([0x0a, 0x08, 8, 0]);
        for word in [393u16, 96, 240, 2] {
            legacy.extend(word.to_le_bytes());
        }
        legacy.extend([0, 0, 0, 0]);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.cds");
        std::fs::write(&path, legacy).unwrap();
        let style = load(&path).unwrap();
        assert_eq!(style.font_family, "Arial");
        assert_eq!(style.font_size_pt, 12.);
        let path = path.with_extension("cdxml");
        std::fs::write(&path, crate::exchange::from_cdx(&binary).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), style);
    }

    #[test]
    fn exported_percentages_do_not_round_down_in_chemdraw() {
        for (ratio, expected) in [
            (0.12, "12.0"),
            (0.179, "17.9"),
            (0.18, "18.0"),
            (0.2, "20.0"),
        ] {
            let mut doc = Document::default();
            doc.drawing_style.name = "Custom spacing".into();
            doc.drawing_style.bond_spacing_ratio = ratio;
            let xml = crate::exchange::drawing::write(&doc, Default::default()).unwrap();
            let tree = roxmltree::Document::parse(&xml).unwrap();
            assert_eq!(tree.root_element().attribute("BondSpacing"), Some(expected));
        }
    }

    #[test]
    fn reusable_style_files_are_validated_and_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Presentation.reshiki-style");
        let style = Preset::Presentation.style();
        crate::storage::write_atomic(&path, &serde_json::to_vec_pretty(&style).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), style);
        std::fs::write(&path, vec![b' '; 65537]).unwrap();
        assert!(load(&path).unwrap_err().contains("64 KB"));
        let mut invalid = style;
        invalid.bond_length_world = 42.;
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load(&path).unwrap_err().contains("coordinate units"));
    }

    #[test]
    fn style_roundtrip_preserves_coordinates_and_explicit_overrides()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut doc = Document::default();
        let a = doc.add_atom("N", Point::new(20., 30.));
        // Leave room for presentation-size labels without rescaling geometry.
        let b = doc.add_atom("O", Point::new(104., 30.));
        doc.add_bond(a, b, 1, "plain");
        doc.atom_mut(b).ok_or("Oxygen")?.text_style = Some(TextStyle {
            size_pt: 12.,
            color: [80, 0, 0],
            ..Default::default()
        });
        doc.annotations.push(Annotation {
            id: 3,
            position: Point::new(40., 90.),
            text: "Scheme 1".into(),
            format: Default::default(),
        });
        let styled = apply(&doc, Preset::Presentation.style(), true, false)?;
        assert_eq!(
            styled.atom(a).ok_or("Nitrogen")?.position,
            doc.atom(a).ok_or("Nitrogen")?.position
        );
        assert_eq!(
            styled.atom(b).ok_or("Oxygen")?.text_style,
            doc.atom(b).ok_or("Oxygen")?.text_style
        );
        assert_eq!(
            styled
                .annotations
                .first()
                .ok_or("Caption")?
                .format
                .style
                .size_pt,
            16.
        );
        assert_eq!(styled.bonds, doc.bonds);
        let reopened: Document = serde_json::from_str(&serde_json::to_string(&styled)?)?;
        assert_eq!(styled, reopened);
        let primitive = crate::scene::primitives(&styled);
        assert!(primitive.iter().any(|p| matches!(p, crate::scene::Primitive::Line(_, _, width) if (*width - styled.drawing_style.world(1.)).abs() < 0.001)));
        assert!(primitive.iter().any(|p| matches!(p, crate::scene::Primitive::Text{text, style, ..} if text == "N" && style.size_pt == 16.)));
        assert!(primitive.iter().any(|p| matches!(p, crate::scene::Primitive::Text{text, style, ..} if text == "O" && style.size_pt == 12.)));
        let mut crowded = styled;
        crowded.atom_mut(b).ok_or("Oxygen")?.position.x = 40.;
        assert!(
            !crate::scene::primitives(&crowded)
                .iter()
                .any(|p| matches!(p, crate::scene::Primitive::Line(..))),
            "A bond completely covered by enlarged labels must not cross their text"
        );
        Ok(())
    }

    #[test]
    fn optional_layout_scaling_is_explicit_and_atomic() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let scaled = apply(&doc, Preset::Presentation.style(), false, true).unwrap();
        assert!((scaled.atoms[0].position.distance(scaled.atoms[1].position) - 70.).abs() < 0.001);
        assert_eq!(
            editing::center(&doc, &[a, b]),
            editing::center(&scaled, &[a, b])
        );
        let mut invalid = Preset::Presentation.style();
        invalid.line_width_pt = f32::NAN;
        assert!(apply(&doc, invalid, true, true).is_err());
        assert_eq!(doc.atoms[0].position, Point::new(0., 0.));
        let mut history = crate::document::History::default();
        assert!(history.commit(doc.clone(), &scaled));
        let mut current = scaled.clone();
        assert!(history.undo(&mut current));
        assert_eq!(current, doc);
        assert!(history.redo(&mut current));
        assert_eq!(current, scaled);
    }
}
