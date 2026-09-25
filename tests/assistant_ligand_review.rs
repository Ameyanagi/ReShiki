use anyhow::Context;
use reshiki::{
    assistant::{
        review::{self, Edit},
        sketch::Sketch,
    },
    document::Document,
};

// A retained visual draft with unassigned metal/halide charges, not a
// chemically validated rhodium complex. Tests preserve its chemical data.
fn sketch() -> anyhow::Result<Sketch> {
    Ok(serde_json::from_str(include_str!(
        "fixtures/assistant-cp-star-dimer.json"
    ))?)
}
fn draft() -> anyhow::Result<Document> {
    sketch()?
        .render(&Default::default())
        .map_err(anyhow::Error::msg)
}
fn edit(target: &str, angles: [f32; 3], show_charge: bool) -> Edit {
    Edit::TiltLigand {
        target: target.into(),
        x_degrees: angles[0],
        y_degrees: angles[1],
        rotation_degrees: angles[2],
        depth_bonds: true,
        show_charge,
    }
}

#[test]
fn review_accepts_tapered_projection_edges_and_preserves_the_chemical_graph() -> anyhow::Result<()>
{
    let mut source = Document::default();
    let metal = source.add_atom("Fe", reshiki::document::Point::default());
    let mut doc = reshiki::hotkeys::atom_edit(&source, metal, "j", 42.)
        .context("Cp shortcut")?
        .map_err(anyhow::Error::msg)?
        .0;
    assert!(
        doc.bonds
            .iter()
            .any(|b| b.projection && b.display == "wedge")
    );
    let original = doc.clone();
    for angles in [[17., -8., 12.], [-17., 8., -12.]] {
        let target = review::targets(&doc)
            .into_iter()
            .find(|t| t.kind == "ligand")
            .context("Cp review target")?;
        doc = review::apply(&doc, &[edit(&target.name, angles, false)], true)
            .map_err(anyhow::Error::msg)?;
        assert_eq!(doc.atom(metal), original.atom(metal));
        for (a, b) in doc.bonds.iter().zip(&original.bonds) {
            assert_eq!(
                (a.a, a.b, a.order, &a.stereo),
                (b.a, b.b, b.order, &b.stereo)
            );
        }
    }
    assert_eq!(
        reshiki::attachments::composition(&doc).map_err(anyhow::Error::msg)?,
        reshiki::attachments::composition(&original).map_err(anyhow::Error::msg)?
    );
    Ok(())
}

#[test]
fn defined_ligand_phase_and_depth_preserve_graph_and_leave_methyl_bonds_thin() -> anyhow::Result<()>
{
    let mut s = sketch()?;
    assert!(
        s.ligands
            .iter()
            .all(|l| l.show_charge && l.phase_degrees == 0.),
        "Older drafts keep their display defaults"
    );
    let before = s.render(&Default::default()).map_err(anyhow::Error::msg)?;
    s.ligands
        .first_mut()
        .context("Missing ligand")?
        .phase_degrees = 36.;
    let after = s.render(&Default::default()).map_err(anyhow::Error::msg)?;
    assert_eq!(
        before
            .bonds
            .iter()
            .map(|b| (b.a, b.b, b.order))
            .collect::<Vec<_>>(),
        after
            .bonds
            .iter()
            .map(|b| (b.a, b.b, b.order))
            .collect::<Vec<_>>()
    );
    assert!(
        after
            .atoms
            .iter()
            .zip(&before.atoms)
            .any(|(a, b)| a.position.distance(b.position) > 1.)
    );
    assert!(
        after
            .bonds
            .iter()
            .filter(|b| b.projection)
            .all(|b| b.order == 4)
    );
    assert_eq!(
        after
            .bonds
            .iter()
            .filter(|b| b.order == 1
                && before.atom(b.a).is_some_and(|a| a.element == "C")
                && before.atom(b.b).is_some_and(|a| a.element == "C"))
            .count(),
        10
    );
    for b in &after.bonds {
        let a = after.atom(b.a).context("Missing endpoint")?;
        let z = after.atom(b.b).context("Missing endpoint")?;
        if a.element == "C" && z.element == "C" && b.order == 1 {
            assert_eq!(b.display, "plain");
            assert!(!b.projection);
        }
    }
    Ok(())
}

#[test]
fn review_tilts_only_one_ligand_and_preserves_3d_lengths_contacts_and_chemistry()
-> anyhow::Result<()> {
    let mut before = draft()?;
    let targets: Vec<_> = review::targets(&before)
        .into_iter()
        .filter(|t| t.kind == "ligand")
        .collect();
    assert_eq!(targets.len(), 2);
    let target = targets.first().context("Missing ligand target")?;
    // Reproduce the old incorrectly bold methyl bonds, so an existing draft
    // can be repaired as well as newly generated proposals.
    for b in &mut before.bonds {
        if target.ids.contains(&b.a) && target.ids.contains(&b.b) && b.order == 1 {
            b.projection = true;
            b.display = "bold".into();
        }
    }
    let after = review::apply(&before, &[edit(&target.name, [17., -8., 12.], false)], true)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(before.atoms.len(), after.atoms.len());
    assert_eq!(before.bonds.len(), after.bonds.len());
    for (a, b) in after.atoms.iter().zip(&before.atoms) {
        if !target.ids.contains(&a.id) {
            assert_eq!(a, b);
        }
        let mut normalized = a.clone();
        normalized.position = b.position;
        normalized.depth = b.depth;
        normalized.display.hide_charge = b.display.hide_charge;
        assert_eq!(&normalized, b);
    }
    for (a, b) in after.bonds.iter().zip(&before.bonds) {
        if !target.ids.contains(&a.a) || !target.ids.contains(&a.b) {
            assert_eq!(a, b);
        }
        let mut normalized = a.clone();
        normalized.projection = b.projection;
        normalized.display = b.display.clone();
        assert_eq!(&normalized, b);
        if target.ids.contains(&a.a) && target.ids.contains(&a.b) {
            let length = |doc: &Document| -> anyhow::Result<f32> {
                let first = doc.atom(a.a).context("Missing endpoint")?;
                let last = doc.atom(a.b).context("Missing endpoint")?;
                Ok(first
                    .position
                    .distance(last.position)
                    .hypot(first.depth - last.depth))
            };
            assert!((length(&before)? - length(&after)?).abs() < 0.001);
            if a.order == 1 {
                assert_eq!(a.display, "plain");
                assert!(!a.projection);
            }
        }
    }
    assert_ne!(reshiki::scene::svg(&before), reshiki::scene::svg(&after));
    assert_eq!(
        reshiki::attachments::composition(&before)
            .map_err(anyhow::Error::msg)?
            .formula,
        reshiki::attachments::composition(&after)
            .map_err(anyhow::Error::msg)?
            .formula
    );
    assert_eq!(before.graphics, after.graphics);
    Ok(())
}

#[tokio::test]
async fn hiding_charge_labels_keeps_formula_and_native_data_and_can_be_reversed()
-> anyhow::Result<()> {
    let before = draft()?;
    let targets: Vec<_> = review::targets(&before)
        .into_iter()
        .filter(|t| t.kind == "ligand")
        .collect();
    let edits: Vec<_> = targets
        .iter()
        .map(|t| edit(&t.name, [0.; 3], false))
        .collect();
    let hidden = review::apply(&before, &edits, true).map_err(anyhow::Error::msg)?;
    assert!(reshiki::scene::svg(&before).contains('−'));
    assert!(!reshiki::scene::svg(&hidden).contains('−'));
    assert_eq!(hidden.atoms.iter().map(|a| a.charge).sum::<i32>(), -2);
    let restored: Document = serde_json::from_slice(&serde_json::to_vec(&hidden)?)?;
    assert_eq!(restored, hidden);
    for format in ["svg", "png", "pdf"] {
        assert!(
            !reshiki::export::drawing(&hidden, format)
                .map_err(anyhow::Error::msg)?
                .is_empty()
        );
    }
    assert!(
        !reshiki::export::clipboard_png(&hidden)
            .map_err(anyhow::Error::msg)?
            .is_empty()
    );
    let xml = reshiki::exchange::drawing::write(&hidden, Default::default())?;
    let cdx = reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
    let xml = reshiki::exchange::from_cdx(&cdx).map_err(anyhow::Error::msg)?;
    let response = reshiki::engine::LocalEngine::default()
        .request(reshiki::engine::Request::import("cdxml", &xml))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(response.analysis.is_none());
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w.contains("Coordination assignments need review"))
    );
    let imported = response.document.context("Imported ligand drawing")?;
    assert_eq!(
        imported
            .bonds
            .iter()
            .filter(|b| b.order == 1 && b.display == "hash")
            .count(),
        2
    );
    assert_eq!(
        imported
            .bonds
            .iter()
            .filter(|b| b.order == 1 && b.display == "wedge")
            .count(),
        2
    );
    assert_eq!(imported.atoms.iter().map(|a| a.charge).sum::<i32>(), -2);
    assert_eq!(
        imported
            .atoms
            .iter()
            .filter(|a| a.charge == -1 && a.display.hide_charge)
            .count(),
        2
    );
    assert_eq!(imported.bonds.iter().filter(|b| b.order == 4).count(), 10);
    assert!(!reshiki::scene::svg(&imported).contains('−'));
    let edits: Vec<_> = targets
        .iter()
        .map(|t| edit(&t.name, [0.; 3], true))
        .collect();
    let shown = review::apply(&hidden, &edits, true).map_err(anyhow::Error::msg)?;
    assert_eq!(shown, before);
    Ok(())
}

#[test]
fn review_rejects_invalid_or_ambiguous_ligand_edits_without_partial_changes() -> anyhow::Result<()>
{
    let before = draft()?;
    let target = review::targets(&before)
        .into_iter()
        .find(|t| t.kind == "ligand")
        .context("Missing ligand target")?;
    for angles in [
        [f32::NAN, 0., 0.],
        [86., 0., 0.],
        [0., -86., 0.],
        [0., 0., 361.],
    ] {
        assert!(review::apply(&before, &[edit(&target.name, angles, false)], true).is_err());
    }
    assert!(review::apply(&before, &[edit("molecule:0", [0.; 3], false)], true).is_err());
    assert!(
        review::apply(
            &before,
            &[
                edit(&target.name, [0.; 3], false),
                edit("ligand:9999", [0.; 3], false)
            ],
            true
        )
        .is_err()
    );
    assert!(before.atoms.iter().all(|a| !a.display.hide_charge));
    let mut stereo = before.clone();
    let bond = stereo
        .bonds
        .iter_mut()
        .find(|b| target.ids.contains(&b.a) && target.ids.contains(&b.b) && b.order == 1)
        .context("Missing ligand bond")?;
    bond.display = "wedge".into();
    bond.projection = false;
    assert!(
        !review::targets(&stereo)
            .iter()
            .any(|t| t.name == target.name)
    );
    assert!(review::apply(&stereo, &[edit(&target.name, [0.; 3], false)], true).is_err());
    let mut s = sketch()?;
    s.ligands
        .first_mut()
        .context("Missing ligand")?
        .phase_degrees = f32::INFINITY;
    assert!(s.validate().is_err());
    Ok(())
}

#[test]
fn front_contact_stays_continuous_without_changing_its_bond_style() -> anyhow::Result<()> {
    let mut source = sketch()?;
    source
        .ligands
        .first_mut()
        .context("Missing ligand")?
        .contact_in_front = Some(false);
    let before = source
        .render(&Default::default())
        .map_err(anyhow::Error::msg)?;
    let target = review::targets(&before)
        .into_iter()
        .find(|t| t.kind == "ligand")
        .context("Missing ligand")?;
    let anchor = target.ids.first().copied().context("Missing anchor")?;
    let index = before
        .bonds
        .iter()
        .position(|b| b.a == anchor || b.b == anchor)
        .context("Missing contact")?;
    assert!(!reshiki::crossings::gaps(&before)[index].is_empty());
    let front = review::apply(
        &before,
        &[Edit::ContactLayer {
            target: target.name.clone(),
            in_front: true,
        }],
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert!(reshiki::crossings::gaps(&front)[index].is_empty());
    assert_eq!(front.atoms, before.atoms);
    for (a, b) in front.bonds.iter().zip(&before.bonds) {
        let mut original = a.clone();
        original.z_order = b.z_order;
        assert_eq!(&original, b);
    }
    let back = review::apply(
        &front,
        &[Edit::ContactLayer {
            target: target.name,
            in_front: false,
        }],
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(back, before);
    let mut s = sketch()?;
    s.ligands
        .first_mut()
        .context("Missing ligand")?
        .contact_in_front = Some(true);
    let generated = s.render(&Default::default()).map_err(anyhow::Error::msg)?;
    assert!(reshiki::crossings::gaps(&generated)[index].is_empty());
    Ok(())
}

#[test]
fn hiding_explicit_charge_marks_retains_radical_marks() -> anyhow::Result<()> {
    use reshiki::scientific::{AtomMark, MarkKind, styled_mark_parts};
    let mut doc = Document::default();
    let id = doc.add_atom("N", Default::default());
    let style = doc.drawing_style.clone();
    let atom = doc.atom_mut(id).context("Missing atom")?;
    atom.charge = 1;
    atom.display.hide_charge = true;
    for kind in [MarkKind::Charge, MarkKind::CircledCharge] {
        atom.marks = vec![AtomMark {
            kind,
            offset: Default::default(),
            angle: 0.,
            size_pt: None,
        }];
        assert!(styled_mark_parts(atom, &style).is_empty());
    }
    for electrons in [1, 2] {
        atom.radical_electrons = electrons;
        atom.marks = vec![AtomMark {
            kind: MarkKind::RadicalIon,
            offset: Default::default(),
            angle: 0.,
            size_pt: None,
        }];
        assert_eq!(styled_mark_parts(atom, &style).len(), electrons as usize);
        assert_eq!(atom.charge, 1);
    }
    Ok(())
}
