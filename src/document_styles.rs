//! Transactional document formatting, independent of chemistry and the interface.
use crate::{document::Document, editing, style::DrawingStyle, typography::TextStyle};

pub fn load(path: &std::path::Path) -> Result<DrawingStyle, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 {
        return Err("Style file exceeds 64 KB.".into());
    }
    let style: DrawingStyle =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid drawing style: {e}"))?;
    style.validate()?;
    Ok(style)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Jacs,
    Nature,
    Rsc,
    Presentation,
}
impl Preset {
    pub const ALL: [Self; 4] = [Self::Jacs, Self::Nature, Self::Rsc, Self::Presentation];
    pub fn style(self) -> DrawingStyle {
        let mut style = DrawingStyle::default();
        match self {
            Self::Jacs => {}
            Self::Nature => {
                // https://www.nature.com/documents/nr-chemical-structures-guide.pdf
                let points = |cm: f32| (cm * 72. / 2.54 * 1000.).round() / 1000.;
                style.name = "Nature".into();
                style.set_bond_length(10.8);
                style.font_size_pt = 6.;
                style.line_width_pt = points(0.021);
                style.bold_width_pt = points(0.055);
                style.margin_width_pt = points(0.042);
                style.hash_spacing_pt = points(0.06);
            }
            Self::Rsc => {
                style.name = "RSC".into();
                style.set_bond_length(12.2);
                style.font_size_pt = 7.;
                style.line_width_pt = 0.45;
                style.bold_width_pt = 1.6;
                style.margin_width_pt = 1.25;
                style.hash_spacing_pt = 1.76;
                style.bond_spacing_ratio = 0.2;
            }
            Self::Presentation => {
                style.name = "Presentation".into();
                style.set_bond_length(24.);
                style.font_size_pt = 16.;
                style.line_width_pt = 1.;
                style.bold_width_pt = 3.;
                style.margin_width_pt = 2.;
                style.hash_spacing_pt = 3.;
            }
        }
        style
    }
}
impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Jacs => "JACS / ACS",
            Self::Nature => "Nature",
            Self::Rsc => "RSC",
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
    doc.version = 15;
    doc.drawing_style = style;
    doc.validate()?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Annotation, Point};

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
