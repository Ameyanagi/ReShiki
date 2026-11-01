use super::*;

impl Writer<'_> {
    /// Partial charges are upright caption text, never ChemDraw formal-charge symbols.
    pub(super) fn partial_charge_text(
        &mut self,
        text: &str,
        center: P,
        size: f64,
        color: crate::palette::Color,
        z: usize,
    ) -> Result<Key> {
        let width = real(crate::style::text_width(text, 0.85));
        let baseline = center.add(-size * width * 0.5, size * 0.3);
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
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chemistry::cdxml,
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

    #[test]
    fn transformed_partial_charges_roundtrip_as_upright_cdxml_and_cdx_captions() {
        let mut doc = Document::default();
        doc.add_atom("O", Point::default());
        let frames = [
            (Point::new(18., 24.), Point::new(-24., 18.)),
            (Point::new(-30., 0.), Point::new(0., 30.)),
            (Point::new(30., 0.), Point::new(0., 90.)),
            (Point::new(30., 0.), Point::new(0., -30.)),
        ];
        for (i, (x, y)) in frames.into_iter().enumerate() {
            let center = Point::new(80. + i as f32 * 60., 0.);
            let mut graphic = Graphic::dragged(
                doc.next_id(),
                GraphicKind::Symbol(SymbolKind::PartialPositive),
                center,
                center,
                GraphicStyle::default(),
                BracketSides::Both,
                false,
            );
            graphic.axis_x = x;
            graphic.axis_y = y;
            doc.graphics.push(graphic);
        }
        let xml = write(&doc, Options::default()).unwrap();
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        let scale = f64::from(doc.drawing_style.bond_length_pt)
            / f64::from(doc.drawing_style.bond_length_world);
        let width = real(crate::style::text_width("δ+", 0.85));
        for xml in [&xml, &decoded] {
            let tree = roxmltree::Document::parse(xml).unwrap();
            let captions: Vec<_> = tree
                .descendants()
                .filter(|node| {
                    node.has_tag_name("t") && node.attribute("InterpretChemically") == Some("no")
                })
                .collect();
            assert_eq!(captions.len(), frames.len());
            for (caption, graphic) in captions.iter().zip(&doc.graphics) {
                assert_eq!(caption.attribute("RotationAngle").unwrap_or("0"), "0");
                let position: Vec<f64> = caption
                    .attribute("p")
                    .unwrap()
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect();
                let expected = [
                    30. + (real(graphic.origin.x) - 30. * width * 0.5) * scale,
                    30. + 30. * 0.3 * scale,
                ];
                for (actual, expected) in position.iter().zip(expected) {
                    assert!((actual - expected).abs() < 0.001, "{position:?}");
                }
            }
            let imported = cdxml::import_cdxml(xml).unwrap().document;
            assert_eq!(imported.annotations.len(), frames.len());
            assert!(imported.annotations.iter().all(|text| text.text == "δ+"));
        }
    }
}
