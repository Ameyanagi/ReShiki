//! Preview a caption bond against the ink that remains visible during the drag.
use crate::canvas::{MoleculeCanvas, World};
use reshiki::{bonds::BondPreset, document::Document};

type Endpoint = (World, Option<u64>, Option<u64>);

impl MoleculeCanvas<'_> {
    /// Only a two-carbon bond is built; the source document stays borrowed.
    /// Clip against existing ink so neighbor-dependent atom labels and caption
    /// scripts keep exactly the appearance shown by the cached source scene.
    pub(super) fn text_bond_preview(
        &self,
        start: Endpoint,
        end: Endpoint,
        preset: BondPreset,
    ) -> Option<Document> {
        let style = &self.doc.drawing_style;
        let (order, display, secondary) = preset.parts();
        let spacing = style.bond_length_world * style.bond_spacing_ratio;
        let rails = match order {
            2 | 7 if display == "bold" => spacing,
            2 | 7 => spacing / 2.,
            3 => spacing,
            6 => spacing * 1.5,
            _ => 0.,
        };
        let half_width = if matches!(
            display,
            "bold" | "wedge" | "hash" | "hashed" | "hollow_wedge"
        ) || secondary == Some("bold")
        {
            style.world(style.bold_width_pt) / 2. + style.line_width() / 2.
        } else if order == 5 && display == "plain" {
            3.5 + style.line_width() / 2.
        } else if display == "wavy" && order != 2 {
            style.line_width() * 1.75
        } else {
            style.line_width() / 2.
        };
        // Expand by the full transverse extent so every rail, wedge and wave
        // stays clear of retained text, including on diagonal drags.
        let gap = style.world(style.margin_width_pt) + rails + half_width;
        let a = self.preview_endpoint(start, end.0, gap)?;
        let b = self.preview_endpoint(end, start.0, gap)?;
        if (b.x - a.x) * (end.0.x - start.0.x) + (b.y - a.y) * (end.0.y - start.0.y) <= 0. {
            return None;
        }
        let mut drawing = Document {
            drawing_style: self.doc.drawing_style.clone(),
            ..Default::default()
        };
        let a = drawing.add_atom("C", a);
        let b = drawing.add_atom("C", b);
        preset.place(&mut drawing, a, b);
        Some(drawing)
    }

    fn preview_endpoint(&self, endpoint: Endpoint, toward: World, gap: f32) -> Option<World> {
        let (position, atom, caption) = endpoint;
        let ink = if let Some(caption) = caption {
            if reshiki::atom_text::text_atom_refusal(self.doc, caption).is_some() {
                return None;
            }
            let annotation = self.doc.annotations.iter().find(|a| a.id == caption)?;
            reshiki::atom_text::text_ink_bounds(annotation)
        } else {
            atom.and_then(|id| self.doc.atom(id))
                .and_then(|atom| reshiki::scene::atom_label_ink_bounds(atom, self.doc))
        };
        Some(ink.map_or(position, |bounds| trim_ink(position, toward, bounds, gap)))
    }
}

/// Leave a gap around visible ink, even when the anchor is off-center within it.
fn trim_ink(start: World, end: World, (lo, hi): (World, World), gap: f32) -> World {
    let axis = |from: f32, to: f32, lo: f32, hi: f32| {
        let delta = to - from;
        if delta.abs() < 0.001 {
            (from >= lo && from <= hi).then_some((0., 1.))
        } else {
            let a = (lo - from) / delta;
            let b = (hi - from) / delta;
            Some((a.min(b), a.max(b)))
        }
    };
    let Some((x0, x1)) = axis(start.x, end.x, lo.x - gap, hi.x + gap) else {
        return start;
    };
    let Some((y0, y1)) = axis(start.y, end.y, lo.y - gap, hi.y + gap) else {
        return start;
    };
    let enter = x0.max(y0).max(0.);
    let exit = x1.min(y1).min(1.);
    if enter > exit {
        return start;
    }
    start.offset((end.x - start.x) * exit, (end.y - start.y) * exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Tool, tests::chain_canvas};
    use reshiki::{atom_labels::Carbons, chains::ChainMode, document::Annotation};

    fn caption(doc: &mut Document, text: &str) -> u64 {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            id,
            position: World::new(80., 0.),
            text: text.into(),
            format: Default::default(),
        });
        id
    }

    #[test]
    fn text_bond_preview_is_bounded_and_does_not_validate_the_source() {
        let mut doc = Document::default();
        let source = doc.add_atom("C", World::new(-80., 0.));
        for i in 0..100 {
            doc.add_atom("C", World::new(i as f32 * 20., -80.));
        }
        let text = caption(&mut doc, "Pd2+");
        let end = reshiki::atom_text::text_atom_position(&doc, text).unwrap();
        doc.version = 0;
        assert!(doc.validate().is_err());
        let before = doc.clone();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Bond(2);
        let preview = canvas
            .text_bond_preview(
                (doc.atom(source).unwrap().position, Some(source), None),
                (end, None, Some(text)),
                BondPreset::Double,
            )
            .unwrap();
        assert_eq!((preview.atoms.len(), preview.bonds.len()), (2, 1));
        assert!(preview.annotations.is_empty());
        assert!(preview.atoms[1].position.distance(end) > 0.);
        assert_eq!(preview.bonds[0].order, 2);
        assert_eq!(doc, before);
    }

    #[test]
    fn text_bond_preview_uses_existing_carbon_visibility_and_caption_ink() {
        let mut doc = Document::default();
        doc.atom_labels.carbons = Carbons::Terminal;
        let start = World::new(-80., 0.);
        let source = doc.add_atom("C", start);
        for position in [start.offset(-42., 0.), start.offset(0., -42.)] {
            let neighbor = doc.add_atom("C", position);
            doc.add_bond(source, neighbor, 1, "plain");
        }
        let text = caption(&mut doc, "CH3");
        let end = reshiki::atom_text::text_atom_position(&doc, text).unwrap();
        let canvas = chain_canvas(&doc, ChainMode::Straight);
        let drawing = canvas
            .text_bond_preview(
                (start, Some(source), None),
                (end, None, Some(text)),
                BondPreset::Single,
            )
            .unwrap();
        assert_eq!(
            drawing.atoms[0].position, start,
            "An invisible internal carbon must not acquire synthetic ink"
        );

        let cursor = end.offset(100., 0.);
        let drawing = canvas
            .text_bond_preview(
                (end, None, Some(text)),
                (cursor, None, None),
                BondPreset::Single,
            )
            .unwrap();
        let (_, hi) = reshiki::atom_text::text_ink_bounds(&doc.annotations[0]).unwrap();
        assert!(
            drawing.atoms[0].position.x > hi.x,
            "The source caption's H3 stays visible to the right throughout the drag"
        );
    }

    #[test]
    fn diagonal_multirail_preview_respects_caption_ink_and_custom_margin() {
        for margin_pt in [0., 3.] {
            let mut doc = Document::default();
            doc.drawing_style.margin_width_pt = margin_pt;
            let text = caption(&mut doc, "CH3");
            let start = reshiki::atom_text::text_atom_position(&doc, text).unwrap();
            let (_, hi) = reshiki::atom_text::text_ink_bounds(&doc.annotations[0]).unwrap();
            let margin = doc.drawing_style.world(margin_pt);
            let canvas = chain_canvas(&doc, ChainMode::Straight);
            for preset in [
                BondPreset::Double,
                BondPreset::BoldDouble,
                BondPreset::Triple,
                BondPreset::Quadruple,
            ] {
                for slope in [0.25, 1., 4.] {
                    let end = start.offset(200., 200. * slope);
                    let preview = canvas
                        .text_bond_preview((start, None, Some(text)), (end, None, None), preset)
                        .unwrap();
                    let rails = reshiki::scene::primitives(&preview);
                    assert!(!rails.is_empty());
                    for rail in rails {
                        let (points, half_width) = match rail {
                            reshiki::scene::Primitive::Line(a, b, width) => {
                                (vec![a, b], width / 2.)
                            }
                            reshiki::scene::Primitive::Polygon(points) => (points, 0.),
                            reshiki::scene::Primitive::Path { commands, .. } => (
                                reshiki::graphics::flattened(&commands)
                                    .into_iter()
                                    .flatten()
                                    .collect(),
                                0.,
                            ),
                            _ => panic!("Expected bond rail geometry"),
                        };
                        for p in points {
                            assert!(
                                p.x >= hi.x + margin + half_width - 0.001
                                    || p.y >= hi.y + margin + half_width - 0.001,
                                "{preset:?} at slope {slope} crosses caption ink: {p:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn refused_text_bond_preview_has_no_bond_geometry() {
        let mut doc = Document::default();
        let start = World::new(-80., 0.);
        let source = doc.add_atom("C", start);
        let text = caption(&mut doc, &"x".repeat(40));
        let end = reshiki::atom_text::text_first_glyph_center(&doc.annotations[0]).unwrap();
        let canvas = chain_canvas(&doc, ChainMode::Straight);
        assert!(
            canvas
                .text_bond_preview(
                    (start, Some(source), None),
                    (end, None, Some(text)),
                    BondPreset::Single
                )
                .is_none()
        );
    }
}
