use super::*;

impl Writer<'_> {
    /// Partial charges are caption text, never ChemDraw formal-charge symbols.
    /// Text cannot stretch or mirror, so `x` alone sets its size and angle.
    pub(super) fn partial_charge_text(
        &mut self,
        text: &str,
        center: P,
        x: P,
        y: P,
        color: crate::palette::Color,
        z: usize,
    ) -> Result<Key> {
        let size = x.x.hypot(x.y);
        let width = real(crate::style::text_width(text, 0.85));
        let baseline = center.add(
            -x.x * width * 0.5 + y.x * 0.3,
            -x.y * width * 0.5 + y.y * 0.3,
        );
        let id = self.id()?;
        self.text(
            self.page,
            text,
            &TextFormat {
                style: TextStyle {
                    size_pt: (size * self.scale * 0.85) as f32,
                    color,
                    ..Default::default()
                },
                ..Default::default()
            },
            [
                ("id", id),
                ("Z", z.to_string()),
                ("InterpretChemically", "no".into()),
                ("p", self.position(baseline)),
                ("CaptionJustification", "Left".into()),
                (
                    "RotationAngle",
                    format!(
                        "{:.0}",
                        (x.y.atan2(x.x).to_degrees() * 65536.).round_ties_even()
                    ),
                ),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
        scientific::{SymbolKind, attach},
    };

    #[test]
    fn attached_and_free_partial_charges_export_as_uninterpreted_cdxml_and_cdx_text() {
        let mut doc = Document::default();
        let id = doc.add_atom("O", Point::default());
        attach(
            doc.atom_mut(id).unwrap(),
            SymbolKind::PartialNegative,
            Point::new(20., -20.),
        )
        .unwrap();
        doc.graphics.push(Graphic::dragged(
            doc.next_id(),
            GraphicKind::Symbol(SymbolKind::PartialPositive),
            Point::new(80., 0.),
            Point::new(80., 0.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        ));
        let before = doc.clone();
        let xml = write(&doc, Options::default()).unwrap();
        let cdx = crate::exchange::to_cdx(&xml).unwrap();
        let decoded = crate::exchange::from_cdx(&cdx).unwrap();
        for xml in [&xml, &decoded] {
            let tree = roxmltree::Document::parse(xml).unwrap();
            let captions: Vec<_> = tree
                .descendants()
                .filter(|n| n.has_tag_name("t") && n.attribute("InterpretChemically") == Some("no"))
                .collect();
            assert_eq!(captions.len(), 2);
            let text: Vec<_> = captions
                .iter()
                .map(|n| {
                    n.children()
                        .find(|n| n.has_tag_name("s"))
                        .unwrap()
                        .text()
                        .unwrap()
                })
                .collect();
            assert_eq!(text, ["δ−", "δ+"]);
            assert!(
                !tree
                    .descendants()
                    .any(|n| n.has_tag_name("curve") || n.has_tag_name("represent"))
            );
            assert!(
                tree.descendants().filter(|n| n.has_tag_name("n")).all(|n| {
                    n.attribute("Charge").is_none() && n.attribute("Radical").is_none()
                })
            );
        }
        assert_eq!(doc, before);
    }
}
