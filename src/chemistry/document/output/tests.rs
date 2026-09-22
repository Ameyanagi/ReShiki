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

#[test]
fn output_keeps_presentation_and_can_be_undone_as_one_edit() -> anyhow::Result<()> {
    let mut original = Document::default();
    let a = original.add_atom("C", Point::new(-14., 10.));
    let b = original.add_atom("O", Point::new(14., 10.));
    original.add_bond(a, b, 1, "plain");
    original.bonds.first_mut().context("Missing bond")?.color = [40, 70, 180];
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
