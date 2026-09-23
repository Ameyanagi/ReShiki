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
fn group_label_alignment_survives_native_cdxml_and_cdx_without_changing_atoms() -> anyhow::Result<()>
{
    use reshiki::abbreviations::LabelAlignment;
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::default());
    let anchor = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(n, anchor, 1, "plain");
    doc = atom_text::apply(&doc, anchor, "Boc", Mode::Group).map_err(anyhow::Error::msg)?;
    let before = doc.clone();
    let mut label_positions = Vec::new();
    for alignment in LabelAlignment::ALL {
        doc.abbreviations
            .first_mut()
            .ok_or_else(|| anyhow::anyhow!("group"))?
            .alignment = alignment;
        let native: Document = serde_json::from_str(&serde_json::to_string(&doc)?)?;
        assert_eq!(native, doc);
        assert_eq!(doc.atoms, before.atoms);
        assert_eq!(doc.bonds, before.bonds);
        let position = reshiki::scene::primitives(&doc)
            .into_iter()
            .find_map(|p| match p {
                reshiki::scene::Primitive::Text { position, text, .. } if text == "Boc" => {
                    Some(position)
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("Boc label"))?;
        label_positions.push(position);
        let xml = exchange::drawing::write(&doc, Default::default())?;
        for encoded in [
            xml.clone(),
            exchange::from_cdx(&exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?,
        ] {
            let restored = reshiki::chemistry::cdxml::import_cdxml(&encoded)?.document;
            assert_eq!(
                restored.abbreviations.first().map(|g| g.alignment),
                Some(alignment)
            );
            assert_eq!(restored.atoms.len(), doc.atoms.len());
            assert_eq!(restored.bonds.len(), doc.bonds.len());
        }
    }
    label_positions.dedup();
    assert_eq!(
        label_positions.len(),
        4,
        "Auto and left coincide here; centered, right, and above differ"
    );
    Ok(())
}

#[test]
fn chemdraw_flush_right_group_fixture_retains_its_override() -> anyhow::Result<()> {
    let doc = reshiki::chemistry::cdxml::import_cdxml(include_str!(
        "fixtures/chemdraw-attachments/group-alignment-right.cdxml"
    ))?
    .document;
    let group = doc
        .abbreviations
        .first()
        .ok_or_else(|| anyhow::anyhow!("Boc"))?;
    assert_eq!(group.label, "Boc");
    assert_eq!(
        group.alignment,
        reshiki::abbreviations::LabelAlignment::Right
    );
    let xml = exchange::drawing::write(&doc, Default::default())?;
    let roundtrip = reshiki::chemistry::cdxml::import_cdxml(&xml)?.document;
    assert_eq!(
        roundtrip.abbreviations.first().map(|g| g.alignment),
        Some(group.alignment)
    );
    Ok(())
}

#[tokio::test]
async fn cleanup_preserves_collapsed_expanded_and_variable_attachments() -> anyhow::Result<()> {
    use reshiki::{
        cleanup::{Options, Scope},
        engine::{Request, native_cleanup, native_response},
    };
    for label in ["Cp", "Cp*"] {
        for expanded in [false, true] {
            for variable in [false, true] {
                let mut doc = complex(label)?;
                if expanded {
                    doc.expand_abbreviations(&doc.all_ids());
                }
                if variable {
                    for atom in &mut doc.atoms {
                        if atom.attachment.is_some() {
                            atom.attachment = Some(attachments::Kind::Variable);
                        }
                    }
                }
                let member = doc
                    .atoms
                    .iter()
                    .find(|a| a.element == "C")
                    .ok_or_else(|| anyhow::anyhow!("Missing ligand carbon"))?
                    .id;
                for scope in [
                    Scope::Drawing,
                    Scope::SelectedAtoms,
                    Scope::SelectedMolecules,
                ] {
                    let mut request = Request::molecule("clean", doc.clone());
                    request.cleanup = Some(Options {
                        scope,
                        keep_orientation: false,
                    });
                    request.selected_ids = Some(vec![member]);
                    // No identifier helper should be discovered or started.
                    let response = native_cleanup::execute(
                        request,
                        Some(native_response::Config::new(std::path::PathBuf::from(
                            "relative-unused-helper",
                        ))),
                    )
                    .await?;
                    assert_eq!(response.document.as_ref(), Some(&doc));
                    assert!(response.analysis.is_none());
                    assert!(
                        response
                            .warnings
                            .iter()
                            .any(|w| w.contains("kept unchanged"))
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn cleanup_can_move_an_independent_molecule_without_moving_the_complex() -> anyhow::Result<()> {
    use reshiki::{
        chemistry::cleanup,
        cleanup::{Options, Scope},
    };
    let complex = complex("Cp*")?;
    let mut doc = complex.clone();
    let a = doc.add_atom("C", Point::new(400., 0.));
    let b = doc.add_atom("C", Point::new(440., 20.));
    let c = doc.add_atom("O", Point::new(470., 50.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    for scope in [
        Scope::Drawing,
        Scope::SelectedAtoms,
        Scope::SelectedMolecules,
    ] {
        let prepared = cleanup::prepare(
            &doc,
            Options {
                scope,
                keep_orientation: false,
            },
            &[a, b, c],
        )?;
        assert_eq!(prepared.requests().len(), 1);
        let cleaned = prepared.finish_with::<cleanup::Error>(|request| {
            assert_eq!(request.molecule.ids.len(), 3);
            let mut positions = request.molecule.positions.clone();
            let middle = positions.get_mut(1).ok_or(cleanup::Error::Layout)?;
            middle.y += 0.25;
            Ok(positions)
        })?;
        for atom in &complex.atoms {
            assert_eq!(cleaned.document.atom(atom.id), Some(atom));
        }
        assert_eq!(cleaned.document.abbreviations, complex.abbreviations);
        assert_ne!(
            cleaned.document.atom(b).map(|a| a.position),
            doc.atom(b).map(|a| a.position)
        );
        assert_eq!(
            cleaned.analysis_policy,
            cleanup::AnalysisPolicy::RetainedAttachments
        );
        assert!(cleaned.molecule.is_none());
    }
    Ok(())
}

#[test]
fn cleanup_attachment_handling_does_not_hide_invalid_input_or_solver_failure() -> anyhow::Result<()>
{
    use reshiki::{chemistry::cleanup, cleanup::Options};
    let mut doc = complex("Cp*")?;
    let id = doc.add_atom("C", Point::new(400., 0.));
    let before = doc.clone();
    let prepared = cleanup::prepare(&doc, Options::default(), &[])?;
    let failed = prepared.finish_with::<cleanup::Error>(|_| Err(cleanup::Error::Layout));
    assert!(matches!(failed, Err(cleanup::Error::Layout)));
    assert_eq!(doc, before);
    doc.atom_mut(id)
        .ok_or_else(|| anyhow::anyhow!("Missing atom"))?
        .element = "Xx".into();
    let prepared = cleanup::prepare(&doc, Options::default(), &[])?;
    assert!(prepared.finish(Vec::new()).is_err());
    let mut invalid = complex("Cp")?;
    let point = invalid
        .atoms
        .iter_mut()
        .find(|a| a.attachment.is_some())
        .ok_or_else(|| anyhow::anyhow!("Missing attachment"))?;
    point.centroid.push(u64::MAX);
    assert!(matches!(
        cleanup::prepare(&invalid, Options::default(), &[]),
        Err(cleanup::Error::Document(_))
    ));
    Ok(())
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
