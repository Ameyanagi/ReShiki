use reshiki::{
    atom_text::{self, Mode},
    attachments,
    document::{Document, Point},
    exchange,
};

fn complex(label: &str) -> anyhow::Result<Document> {
    let mut doc = Document::default();
    let fe = doc.add_atom("Fe", Point::new(100., 100.));
    doc.atom_mut(fe)
        .ok_or_else(|| anyhow::anyhow!("Fe"))?
        .charge = 2;
    for position in [Point::new(20., 30.), Point::new(180., 170.)] {
        let endpoint = doc.add_atom("C", position);
        doc.add_bond(fe, endpoint, 1, "plain");
        doc = atom_text::apply(&doc, endpoint, label, Mode::Auto).map_err(anyhow::Error::msg)?;
    }
    Ok(doc)
}

#[test]
fn cp_groups_count_real_carbons_hydrogens_and_charge_through_interchange() -> anyhow::Result<()> {
    for (label, carbons, formula) in [("Cp", 10, "C10H10Fe"), ("Cp*", 20, "C20H30Fe")] {
        let doc = complex(label)?;
        assert_eq!(
            doc.atoms.iter().filter(|a| a.element == "C").count(),
            carbons
        );
        assert_eq!(doc.abbreviations.len(), 2);
        assert_eq!(
            attachments::composition(&doc)
                .map_err(anyhow::Error::msg)?
                .formula,
            formula
        );
        assert_eq!(
            attachments::editor_markers(&doc).count(),
            0,
            "Collapsed labels hide their handles"
        );
        let xml = exchange::drawing::write(&doc, Default::default())?;
        for encoded in [
            xml.clone(),
            exchange::from_cdx(&exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?,
        ] {
            let mut restored = reshiki::chemistry::cdxml::import_cdxml(&encoded)?.document;
            restored.validate().map_err(anyhow::Error::msg)?;
            assert!(
                restored.abbreviations.is_empty(),
                "ChemDraw interchange expands haptic labels to preserve their atoms"
            );
            assert_eq!(
                restored.atoms.iter().filter(|a| a.element == "C").count(),
                carbons
            );
            assert_eq!(
                attachments::composition(&restored)
                    .map_err(anyhow::Error::msg)?
                    .formula,
                formula
            );
            assert_eq!(
                restored
                    .atoms
                    .iter()
                    .filter(|a| a.attachment.is_some() && a.centroid.len() == 5)
                    .count(),
                2
            );
            let before = serde_json::to_value(&restored.atoms)?;
            assert_eq!(restored.expand_abbreviations(&restored.all_ids()), 0);
            assert_eq!(before, serde_json::to_value(&restored.atoms)?);
            assert_eq!(attachments::editor_markers(&restored).count(), 2);
            assert_eq!(
                attachments::composition(&restored)
                    .map_err(anyhow::Error::msg)?
                    .formula,
                formula
            );
        }
    }
    Ok(())
}

#[test]
fn ligand_replacement_is_atomic_and_does_not_change_the_metal() -> anyhow::Result<()> {
    let doc = complex("Cp")?;
    let anchor = doc
        .abbreviations
        .first()
        .ok_or_else(|| anyhow::anyhow!("group"))?
        .anchor;
    let changed = atom_text::apply(&doc, anchor, "Cp*", Mode::Auto).map_err(anyhow::Error::msg)?;
    assert_eq!(
        attachments::composition(&changed)
            .map_err(anyhow::Error::msg)?
            .formula,
        "C15H20Fe"
    );
    assert_eq!(
        changed
            .atoms
            .iter()
            .find(|a| a.element == "Fe")
            .map(|a| a.charge),
        Some(2)
    );
    let mut invalid = Document::default();
    let center = invalid.add_atom("C", Point::new(0., 0.));
    for x in [-42., 42.] {
        let end = invalid.add_atom("C", Point::new(x, 0.));
        invalid.add_bond(center, end, 1, "plain");
    }
    let before = serde_json::to_value(&invalid)?;
    assert!(atom_text::apply(&invalid, center, "Cp*", Mode::Auto).is_err());
    assert_eq!(before, serde_json::to_value(&invalid)?);
    let text = atom_text::apply(&invalid, center, "Cp*", Mode::Text).map_err(anyhow::Error::msg)?;
    assert!(text.abbreviations.is_empty());
    assert_eq!(
        text.atom(center)
            .and_then(|a| a.display.variable.as_deref()),
        Some("Cp*")
    );
    Ok(())
}

#[test]
fn boc_formula_is_the_same_when_collapsed_expanded_or_copied() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    let c = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(n, c, 1, "plain");
    let doc = atom_text::apply(&doc, c, "Boc", Mode::Auto).map_err(anyhow::Error::msg)?;
    let count = |d: &Document| -> anyhow::Result<String> {
        let molecule = reshiki::chemistry::document::prepare(d)?;
        Ok(reshiki::chemistry::properties(
            &molecule
                .state
                .graph
                .atom_facts()
                .map_err(anyhow::Error::msg)?,
        )
        .map_err(anyhow::Error::msg)?
        .formula)
    };
    assert_eq!(count(&doc)?, "C5H11NO2");
    let xml = exchange::drawing::write(&doc, Default::default())?;
    let mut restored = reshiki::chemistry::cdxml::import_cdxml(&xml)?.document;
    assert_eq!(count(&restored)?, "C5H11NO2");
    restored.expand_abbreviations(&restored.all_ids());
    assert_eq!(count(&restored)?, "C5H11NO2");
    Ok(())
}

#[test]
fn chemdraw_resaved_haptic_ligands_keep_composition_and_lost_definitions_fail() -> anyhow::Result<()>
{
    let doc = reshiki::chemistry::cdxml::import_cdxml(include_str!(
        "fixtures/chemdraw-attachments/Cp-star-expanded.cdxml"
    ))?
    .document;
    assert_eq!(
        attachments::composition(&doc)
            .map_err(anyhow::Error::msg)?
            .formula,
        "C20H30Fe"
    );
    assert_eq!(doc.atoms.iter().filter(|a| a.element == "C").count(), 20);
    assert_eq!(
        doc.atoms.iter().filter(|a| a.centroid.len() == 5).count(),
        2
    );
    let cp = reshiki::chemistry::cdxml::import_cdxml(include_str!(
        "fixtures/chemdraw-attachments/Cp-expanded.cdxml"
    ))?
    .document;
    assert_eq!(
        attachments::composition(&cp)
            .map_err(anyhow::Error::msg)?
            .formula,
        "C10H10Fe"
    );
    // Real ChemDraw 26 output from the unsupported nested/haptic label case.
    // Never guess missing atom definitions from its caption.
    assert!(
        reshiki::chemistry::cdxml::import_cdxml(include_str!(
            "fixtures/chemdraw-attachments/unsupported-haptic-abbreviation.cdxml"
        ))
        .is_err()
    );
    Ok(())
}
