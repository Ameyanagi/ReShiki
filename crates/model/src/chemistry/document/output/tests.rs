use super::*;
use crate::{
    chemistry::document::prepare,
    document::{Annotation, History},
};
use anyhow::Context;

fn labels(drawing: &Drawing) -> Labels {
    Labels {
        rdkit_version: RDKIT_VERSION.into(),
        atoms: vec![None; drawing.molecule().ids.len()],
        bonds: drawing
            .molecule()
            .state
            .metadata
            .bonds
            .iter()
            .map(|b| BondLabel {
                code: None,
                stereo: b.stereo,
                stereo_atoms: b.stereo_atoms.clone(),
            })
            .collect(),
    }
}

fn generated(text: &str, options: DrawingOptions) -> anyhow::Result<Document> {
    let input = crate::chemistry::smiles::read(text)?;
    let state = input.prepared.state;
    let positions = crate::chemistry::depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )?
    .positions;
    let molecule = Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        state,
        positions,
    };
    let draft = for_generated(&molecule, &input.prepared.dummy_labels, options)?;
    let labels = draft.labels()?;
    Ok(draft.finish(labels)?)
}

#[test]
fn drawing_reconstruction_keeps_singlet_beside_the_unchanged_chemistry_graph() -> anyhow::Result<()>
{
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    for position in [Point::new(-36., 21.), Point::new(36., 21.)] {
        let chlorine = doc.add_atom("Cl", position);
        doc.add_bond(carbon, chlorine, 1, "plain");
    }
    let atom = doc.atom_mut(carbon).unwrap();
    atom.radical_electrons = 2;
    atom.singlet = true;
    let molecule = prepare(&doc)?;
    assert_eq!(molecule.state.graph.atoms[0].radical_electrons, 2);
    let draft = for_drawing(&molecule, &doc)?;
    let reconstructed = draft.clone().finish(labels(&draft))?;
    assert!(reconstructed.atom(carbon).unwrap().singlet);
    assert_eq!(prepare(&reconstructed)?.state.graph, molecule.state.graph);
    Ok(())
}

#[test]
fn four_substituent_wedge_pair_preserves_configuration() -> anyhow::Result<()> {
    let one = generated("CC[C@](C)(Br)Cl", DrawingOptions::default())?;
    let pair = generated(
        "CC[C@](C)(Br)Cl",
        DrawingOptions {
            two_wedge_stereocentres: true,
            ..Default::default()
        },
    )?;
    let stereo = |doc: &Document| {
        doc.bonds
            .iter()
            .filter(|b| matches!(b.display.as_str(), "wedge" | "hash"))
            .map(|b| b.display.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(stereo(&one).len(), 1);
    let directions = stereo(&pair);
    assert_eq!(directions.len(), 2);
    assert!(directions.iter().any(|d| d == "wedge"));
    assert!(directions.iter().any(|d| d == "hash"));
    let identity = |doc: &Document| -> anyhow::Result<String> {
        let molecule = prepare(doc)?;
        Ok(crate::chemistry::smiles::write::write(&molecule.state, Default::default())?.text)
    };
    assert_eq!(identity(&one)?, identity(&pair)?);
    Ok(())
}

#[test]
fn generated_norcamphor_uses_both_bonds_to_the_one_atom_bridge() -> anyhow::Result<()> {
    let text = "O=C1C[C@H]2CC[C@@H]1C2";
    let drawing = generated(text, DrawingOptions::default())?;
    let wedges = drawing
        .bonds
        .iter()
        .filter(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
        .collect::<Vec<_>>();
    assert_eq!(wedges.len(), 2);
    assert!(
        wedges
            .iter()
            .all(|bond| [4, 7].contains(&bond.a) && bond.b == 8)
    );
    let state = crate::chemistry::smiles::prepare(text)?.state;
    let output = prepare(&drawing)?;
    assert_eq!(
        crate::chemistry::smiles::write::write(&state, Default::default())?.text,
        crate::chemistry::smiles::write::write(&output.state, Default::default())?.text,
    );
    Ok(())
}

#[test]
fn generated_fusion_hydrogens_are_optional_and_cleanup_does_not_add_them() -> anyhow::Result<()> {
    let text = "C1CC[C@H]2CCCC[C@@H]2C1";
    let shown = generated(text, DrawingOptions::default())?;
    let hidden = generated(
        text,
        DrawingOptions {
            ring_fusion_hydrogens: false,
            ..Default::default()
        },
    )?;
    let hydrogens = shown
        .atoms
        .iter()
        .filter(|a| a.element == "H")
        .map(|a| a.id)
        .collect::<Vec<_>>();
    assert_eq!(hydrogens.len(), 2);
    for h in hydrogens {
        assert!(
            shown
                .bonds
                .iter()
                .any(|b| b.b == h && matches!(b.display.as_str(), "wedge" | "hash"))
        );
    }
    assert_eq!(shown.atoms.len(), hidden.atoms.len() + 2);
    let molecule = prepare(&hidden)?;
    let draft = for_drawing(&molecule, &hidden)?;
    let labels = draft.labels()?;
    assert_eq!(draft.finish(labels)?.atoms.len(), hidden.atoms.len());
    Ok(())
}

#[test]
fn fusion_hydrogens_clear_atoms_labels_and_bonds_preserving_stereo() -> anyhow::Result<()> {
    let label_gap = (1.5 * WORLD_PER_DEPICT_UNIT * 0.25) as f32;
    for (text, expected) in [
        (
            "C[C@]12CC[C@H]3[C@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@H]34",
            vec!["wedge", "hash", "wedge", "hash"],
        ),
        (
            "C[C@]12CC[C@H]3[C@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@]34C",
            vec!["wedge", "hash", "wedge"],
        ),
        (
            "C[C@]12CC[C@H]3[C@H]([C@@H]1CC[C@@H]2O)CCC4=C3C=CC(=C4)O",
            vec!["hash", "wedge", "hash"],
        ),
        (
            "C[C@H](CCCC(C)C)[C@H]1CC[C@@H]2[C@@]1(CC[C@H]3[C@H]2CC=C4[C@@]3(CCC(C4)O)C)C",
            vec!["hash", "hash", "wedge"],
        ),
        ("C1CC[C@H]2CCCC[C@@H]2C1", vec!["hash", "wedge"]),
        ("C1CC[C@H]2CCC[C@H]2C1", vec!["hash", "hash"]),
    ] {
        let doc = generated(text, DrawingOptions::default())?;
        let hydrogens = doc
            .atoms
            .iter()
            .filter(|a| a.element == "H")
            .collect::<Vec<_>>();
        assert_eq!(hydrogens.len(), expected.len(), "{text}");
        for (hydrogen, display) in hydrogens.iter().zip(expected) {
            let bond = doc
                .bonds
                .iter()
                .find(|b| b.b == hydrogen.id)
                .context("Missing C-H bond")?;
            assert_eq!(bond.display, display, "{text}: H{}", hydrogen.id);
        }
        // Normalize graph H atoms to compare the same representation while
        // checking the configurations reconstructed from the drawn wedges.
        let canonical =
            |state: &crate::chemistry::stereo::perception::State| -> anyhow::Result<String> {
                let text = crate::chemistry::smiles::write::write(state, Default::default())?.text;
                let state = crate::chemistry::smiles::prepare(&text)?.state;
                Ok(crate::chemistry::smiles::write::write(&state, Default::default())?.text)
            };
        assert_eq!(
            canonical(&crate::chemistry::smiles::prepare(text)?.state)?,
            canonical(&prepare(&doc)?.state)?,
            "{text}"
        );
        for hydrogen in hydrogens {
            let (lo, hi) =
                crate::scene::atom_label_ink_bounds(hydrogen, &doc).context("Missing H label")?;
            for other in doc.atoms.iter().filter(|a| a.id != hydrogen.id) {
                let p = other.position;
                assert!(
                    p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y,
                    "{text}: H{} label covers atom {}",
                    hydrogen.id,
                    other.id
                );
                for (a, b) in crate::scene::atom_label_ink_boxes(other, &doc) {
                    let gap = (lo.x - b.x)
                        .max(a.x - hi.x)
                        .max(0.)
                        .hypot((lo.y - b.y).max(a.y - hi.y).max(0.));
                    assert!(
                        gap >= label_gap - 1e-4,
                        "{text}: H{} label gap to label {} is {gap}, expected at least {label_gap}",
                        hydrogen.id,
                        other.id
                    );
                }
            }
            for bond in doc
                .bonds
                .iter()
                .filter(|b| b.a != hydrogen.id && b.b != hydrogen.id)
            {
                let a = doc.atom(bond.a).context("Missing bond start")?.position;
                let b = doc.atom(bond.b).context("Missing bond end")?.position;
                assert!(
                    !crate::atom_labels::segment_hits_rect(a, b, lo, hi),
                    "{text}: H{} label overlaps bond {}-{}",
                    hydrogen.id,
                    bond.a,
                    bond.b
                );
            }
        }
        for (index, atom) in doc.atoms.iter().enumerate() {
            for other in &doc.atoms[index + 1..] {
                let distance =
                    (atom.position.x - other.position.x).hypot(atom.position.y - other.position.y);
                assert!(
                    distance >= 21. - 1e-4,
                    "{text}: atoms {} and {} at {distance}",
                    atom.id,
                    other.id
                );
            }
        }
    }
    Ok(())
}

#[test]
fn unknown_alkene_uses_wavy_substituent_or_crossed_fallback() -> anyhow::Result<()> {
    for (text, wavy_count) in [
        ("CC=CCC", 1),
        ("C1=CCCCC1", 0),
        ("C1CCCCC1C=CC2CCCCC2", 0),
        ("F[C@H](Cl)C=C[C@H](Br)I", 0),
    ] {
        let mut source = generated(text, DrawingOptions::default())?;
        let mut molecule = prepare(&source)?;
        let double = molecule
            .state
            .graph
            .bonds
            .iter()
            .position(|b| b.order == 2)
            .unwrap();
        molecule.state.metadata.bonds[double].stereo = 1;
        molecule.state.metadata.bonds[double].stereo_atoms.clear();
        let draft = for_import(&molecule, false, &vec![None; molecule.ids.len()])?;
        let labels = draft.labels()?;
        source = draft.finish(labels)?;
        assert!(
            source
                .bonds
                .iter()
                .any(|b| b.order == 2 && b.display == "wavy")
        );
        assert_eq!(
            depict_unknown_alkene(&molecule, double, &mut source.bonds, true, |_| true)?,
            wavy_count != 0
        );
        assert_eq!(
            source
                .bonds
                .iter()
                .filter(|b| b.order == 1 && b.display == "wavy")
                .count(),
            wavy_count
        );
        if wavy_count != 0 {
            let single = source
                .bonds
                .iter()
                .find(|b| b.order == 1 && b.display == "wavy")
                .unwrap();
            let double = source.bonds.iter().find(|b| b.order == 2).unwrap();
            assert_eq!(double.display, "plain");
            assert!(single.a == double.a || single.a == double.b);
            assert_eq!(
                prepare(&source)?
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| b.stereo == 1)
                    .count(),
                1
            );
        }
    }
    Ok(())
}

#[test]
fn output_keeps_presentation_and_can_be_undone_as_one_edit() -> anyhow::Result<()> {
    let mut original = Document::default();
    let a = original.add_atom("C", Point::new(-14., 10.));
    let b = original.add_atom("O", Point::new(14., 10.));
    original.add_bond(a, b, 1, "plain");
    original.bonds.first_mut().context("Missing bond")?.color =
        crate::palette::Color::Custom([40, 70, 180]);
    let atom = original.atoms.get_mut(1).context("Missing oxygen")?;
    atom.label_h = 99;
    atom.text_style = Some(crate::typography::TextStyle {
        family: "Hiragino Sans".into(),
        size_pt: 12.,
        ..Default::default()
    });
    original.annotations.push(Annotation {
        id: 55,
        position: Point::new(0., 42.),
        text: "試料 A".into(),
        format: Default::default(),
    });
    original.groups.push(crate::grouping::Group {
        id: 56,
        members: vec![a, b, 55],
        integral: false,
    });
    let molecule = prepare(&original)?;
    let draft = for_drawing(&molecule, &original)?;
    let labels = labels(&draft);
    let output = draft.finish(labels)?;
    assert_eq!(output.annotations, original.annotations);
    assert_eq!(output.groups, original.groups);
    assert_eq!(output.drawing_style, original.drawing_style);
    assert_eq!(output.bonds, original.bonds);
    assert_eq!(output.atoms.get(1).context("Missing oxygen")?.label_h, 1);
    assert_eq!(
        output.atoms.get(1).context("Missing oxygen")?.text_style,
        original.atoms.get(1).context("Missing oxygen")?.text_style
    );
    let mut history = History::default();
    assert!(history.commit(original.clone(), &output));
    let mut changed = output;
    assert!(history.undo(&mut changed));
    assert_eq!(changed, original);
    Ok(())
}

#[test]
fn bad_identity_or_label_metadata_never_yields_a_partial_document() -> anyhow::Result<()> {
    let mut original = Document::default();
    original.add_atom("O", Point::default());
    let molecule = prepare(&original)?;
    let mut invalid_molecule = molecule.clone();
    invalid_molecule.ids.clear();
    assert!(for_drawing(&invalid_molecule, &original).is_err());
    for kind in 0..4 {
        let draft = for_drawing(&molecule, &original)?;
        let mut data = labels(&draft);
        match kind {
            0 => data.rdkit_version = "wrong".into(),
            1 => data.atoms.clear(),
            2 => data.bonds.push(BondLabel {
                code: None,
                stereo: 0,
                stereo_atoms: Vec::new(),
            }),
            _ => *data.atoms.first_mut().context("Missing atom label")? = Some("invalid".into()),
        }
        assert!(draft.finish(data).is_err());
    }
    assert_eq!(original.atoms.first().context("Missing oxygen")?.label_h, 0);
    Ok(())
}

#[test]
fn bond_labels_follow_identity_when_base_order_changes() -> anyhow::Result<()> {
    let mut base = Document::default();
    let a = base.add_atom("F", Point::new(-14., 24.));
    let b = base.add_atom("C", Point::default());
    let c = base.add_atom("C", Point::new(28., 0.));
    let d = base.add_atom("Cl", Point::new(42., -24.));
    base.add_bond(a, b, 1, "plain");
    base.add_bond(b, c, 2, "plain");
    base.add_bond(c, d, 1, "plain");
    let molecule = prepare(&base)?;
    for kind in 0..3 {
        let draft = for_drawing(&molecule, &base)?;
        let mut data = labels(&draft);
        let bond = data.bonds.get_mut(1).context("Missing stereo bond")?;
        match kind {
            0 => bond.stereo = 6,
            1 => bond.stereo_atoms = vec![0],
            _ => bond.stereo_atoms = vec![3, 0],
        }
        assert!(draft.finish(data).is_err());
    }
    base.bonds.swap(0, 1);
    let draft = for_drawing(&molecule, &base)?;
    let mut data = labels(&draft);
    data.bonds
        .get_mut(1)
        .context("Missing double bond label")?
        .code = Some("E".into());
    let doc = draft.finish(data)?;
    assert_eq!(
        doc.bonds
            .first()
            .context("Missing double bond")?
            .cip_label
            .as_deref(),
        Some("E")
    );
    assert!(
        doc.bonds
            .get(1)
            .context("Missing single bond")?
            .cip_label
            .is_none()
    );
    Ok(())
}

#[test]
fn detached_scene_draft_requires_appearance_and_callback_success() -> anyhow::Result<()> {
    #[derive(Debug, thiserror::Error)]
    enum Failure {
        #[error(transparent)]
        Drawing(#[from] Error),
        #[error("restoration failed")]
        Restore,
    }
    let mut base = Document::default();
    let a = base.add_atom("O", Point::new(0., 0.));
    let h = base.add_atom("H", Point::new(42., 0.));
    let b = base.add_atom("O", Point::new(84., 0.));
    base.add_bond(a, h, 1, "plain");
    base.add_bond(h, b, 0, "dotted");
    let molecule = prepare(&base)?;
    let before = serde_json::to_value(&molecule)?;
    assert!(for_import(&molecule, false, &[None, None, None]).is_err());
    let draft = for_import_scene(&molecule, false)?;
    let data = labels(&draft);
    assert!(draft.finish(data).is_err());
    let draft = for_import_scene(&molecule, false)?;
    let data = labels(&draft);
    let failed: Result<Document, Failure> = draft.finish_with(data, |document| {
        for bond in &mut document.bonds {
            if bond.order == 0 {
                bond.display = "dotted".into();
            }
        }
        Err(Failure::Restore)
    });
    assert!(matches!(failed, Err(Failure::Restore)));
    let draft = for_import_scene(&molecule, false)?;
    let data = labels(&draft);
    let restored: Result<Document, Failure> = draft.finish_with(data, |document| {
        for bond in &mut document.bonds {
            if bond.order == 0 {
                bond.display = "dotted".into();
            }
        }
        Ok(())
    });
    restored?.validate().map_err(anyhow::Error::msg)?;
    assert_eq!(serde_json::to_value(&molecule)?, before);
    let mut invalid_base = base.clone();
    invalid_base.bonds.last_mut().context("bond")?.display = "plain".into();
    assert!(for_drawing(&molecule, &invalid_base).is_err());
    assert_eq!(
        base.bonds.last().context("original bond")?.display,
        "dotted"
    );
    Ok(())
}
