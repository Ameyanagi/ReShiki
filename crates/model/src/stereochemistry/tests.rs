use super::*;
use crate::document::AtomStereo;
use crate::{
    chemistry::{document::prepare, stereo::cip::label},
    haworth::{Anomer, HaworthOptions, Sugar, sugar_document, sugar_document_with_options},
};

fn identity(doc: &Document) -> anyhow::Result<String> {
    let molecule = prepare(doc)?;
    Ok(crate::chemistry::smiles::write::write(&molecule.state, Default::default())?.text)
}

fn inchikey(doc: &Document) -> anyhow::Result<String> {
    use crate::chemistry::inchi::{generator, kernel, key};
    let molecule = prepare(doc)?;
    let input = kernel::Molecule::prepare(&molecule.state, None)?;
    let generated = generator::generate_in_process(&input)?;
    anyhow::ensure!(generated.status.is_success(), "{}", generated.message);
    Ok(key::from_inchi(&generated.inchi)?)
}

fn configuration(doc: &Document, id: u64) -> anyhow::Result<Option<String>> {
    let molecule = prepare(doc)?;
    let index = molecule
        .ids
        .iter()
        .position(|&candidate| candidate == id)
        .unwrap();
    let labeled = label::assign(&molecule.state, &Default::default())?;
    Ok(labeled.state.properties.atoms[index].cip_code.clone())
}

#[test]
fn haworth_templates_invert_only_the_selected_centre_with_and_without_ring_hydrogens()
-> anyhow::Result<()> {
    for template in crate::haworth::templates().map_err(anyhow::Error::msg)? {
        let original = template.document;
        let centers: Vec<_> = original
            .atoms
            .iter()
            .filter(|atom| atom.stereo.is_some())
            .map(|atom| atom.id)
            .collect();
        let configurations: HashMap<_, _> = centers
            .iter()
            .map(|&id| Ok((id, configuration(&original, id)?)))
            .collect::<anyhow::Result<_>>()?;
        for &center in &centers {
            let mut doc = original.clone();
            invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
            let before = configurations[&center].as_deref();
            let expected = match before {
                Some("R") => "S",
                Some("S") => "R",
                other => anyhow::bail!("{}: unexpected centre {center}: {other:?}", template.name),
            };
            assert_eq!(
                configuration(&doc, center)?.as_deref(),
                Some(expected),
                "{} at {center}",
                template.name
            );
            for &other in centers.iter().filter(|&&other| other != center) {
                assert_eq!(
                    configuration(&doc, other)?,
                    configurations[&other],
                    "{}: inversion at {center} changed centre {other}",
                    template.name
                );
            }
            assert_eq!(
                interchange::export_bonds(&doc).map_err(anyhow::Error::msg)?,
                interchange::export_bonds(&original).map_err(anyhow::Error::msg)?,
                "{} at {center}",
                template.name
            );
        }
    }
    Ok(())
}

#[test]
fn alpha_glucose_anomer_inverts_to_beta_and_native_round_trips() -> anyhow::Result<()> {
    let mut doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    let beta = sugar_document(Sugar::Glucose, Anomer::Beta, 42.).map_err(anyhow::Error::msg)?;
    let center = doc.atoms[0].id;
    let old = doc.clone();
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(identity(&doc)?, identity(&beta)?);
    let beta_key = inchikey(&beta)?;
    assert_eq!(inchikey(&doc)?, beta_key);
    assert_eq!(
        interchange::export_bonds(&doc)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    for atom in doc.atoms.iter().filter(|a| a.id != center) {
        assert_eq!(atom.stereo, old.atom(atom.id).unwrap().stereo);
    }
    let restored = Document::from_native_file(&doc.file_json().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(identity(&restored)?, identity(&beta)?);
    assert_eq!(inchikey(&restored)?, beta_key);
    assert_eq!(
        interchange::export_bonds(&restored)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    Ok(())
}

#[test]
fn glucose_c2_inverts_to_mannose_without_changing_the_anomer() -> anyhow::Result<()> {
    let mut doc = sugar_document(Sugar::Glucose, Anomer::Beta, 42.).map_err(anyhow::Error::msg)?;
    let mannose = sugar_document(Sugar::Mannose, Anomer::Beta, 42.).map_err(anyhow::Error::msg)?;
    let center = doc.atoms[1].id;
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(identity(&doc)?, identity(&mannose)?);
    let mannose_key = inchikey(&mannose)?;
    assert_eq!(inchikey(&doc)?, mannose_key);
    assert_eq!(
        interchange::export_bonds(&doc)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    let restored = Document::from_native_file(&doc.file_json().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(inchikey(&restored)?, mannose_key);
    assert_eq!(
        interchange::export_bonds(&restored)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    Ok(())
}

#[test]
fn explicit_ring_hydrogen_and_hydroxyl_both_change_face() -> anyhow::Result<()> {
    let mut doc = sugar_document_with_options(
        Sugar::Glucose,
        Anomer::Alpha,
        42.,
        HaworthOptions {
            ring_hydrogens: true,
        },
    )
    .map_err(anyhow::Error::msg)?;
    let center = doc.atoms[0].id;
    let position = doc.atom(center).unwrap().position;
    let old = doc.clone();
    let branches: Vec<_> = doc
        .bonds
        .iter()
        .filter(|bond| !bond.projection)
        .filter_map(|bond| other(bond, center))
        .collect();
    assert_eq!(branches.len(), 2);
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    for id in branches {
        assert_eq!(
            doc.atom(id).unwrap().position.x,
            old.atom(id).unwrap().position.x
        );
        assert_eq!(
            doc.atom(id).unwrap().position.y,
            2. * position.y - old.atom(id).unwrap().position.y
        );
    }
    let beta = sugar_document_with_options(
        Sugar::Glucose,
        Anomer::Beta,
        42.,
        HaworthOptions {
            ring_hydrogens: true,
        },
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(identity(&doc)?, identity(&beta)?);
    assert_eq!(
        interchange::export_bonds(&doc)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    Ok(())
}

#[test]
fn haworth_inversion_reflects_the_entire_condensed_subtree_after_affine_transform()
-> anyhow::Result<()> {
    let mut doc = sugar_document(Sugar::GlcNAc, Anomer::Beta, 42.).map_err(anyhow::Error::msg)?;
    let center = doc.atoms[1].id;
    let anchor = doc
        .bonds
        .iter()
        .filter(|b| !b.projection)
        .find_map(|bond| other(bond, center))
        .unwrap();
    let members = doc
        .abbreviations
        .iter()
        .find(|g| g.anchor == anchor)
        .unwrap()
        .members
        .clone();
    for atom in &mut doc.atoms {
        let Point { x, y } = atom.position;
        atom.position = Point::new(50. + x * 1.2 + y * 0.3, -70. + x * 0.5 + y * 0.9);
    }
    let old = doc.clone();
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    for id in members {
        assert_ne!(
            doc.atom(id).unwrap().position,
            old.atom(id).unwrap().position
        );
    }
    assert_ne!(identity(&doc)?, identity(&old)?);
    assert_eq!(
        interchange::export_bonds(&doc)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(identity(&doc)?, identity(&old)?);
    for atom in &doc.atoms {
        assert!(atom.position.distance(old.atom(atom.id).unwrap().position) < 0.001);
    }
    Ok(())
}

fn wedge_document(display: &str) -> (Document, u64) {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for (element, position, kind) in [
        ("F", Point::new(0., -42.), display),
        ("Cl", Point::new(-36., 21.), "plain"),
        ("Br", Point::new(36., 21.), "plain"),
    ] {
        let atom = doc.add_atom(element, position);
        doc.add_bond(center, atom, 1, kind);
    }
    (doc, center)
}

#[test]
fn drawn_wedge_r_inverts_to_s_by_swapping_marks_only() -> anyhow::Result<()> {
    let (mut doc, center) = wedge_document("wedge");
    assert_eq!(configuration(&doc, center)?.as_deref(), Some("R"));
    assert!(doc.atom(center).unwrap().stereo.is_none());
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(doc.bonds[0].display, "hash");
    assert!(doc.atom(center).unwrap().stereo.is_none());
    assert_eq!(configuration(&doc, center)?.as_deref(), Some("S"));
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(doc.bonds[0].display, "wedge");
    assert_eq!(configuration(&doc, center)?.as_deref(), Some("R"));
    Ok(())
}

#[test]
fn outline_wedges_and_long_hashes_keep_their_display_family() -> anyhow::Result<()> {
    for (before, after) in [("hollow_wedge", "hashed"), ("hashed", "hollow_wedge")] {
        let (mut doc, center) = wedge_document(before);
        let previous = configuration(&doc, center)?;
        invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
        assert_eq!(doc.bonds[0].display, after);
        assert_ne!(configuration(&doc, center)?, previous);
    }
    Ok(())
}

#[test]
fn drawing_configuration_uses_bond_neighbor_order_after_records_are_reordered() -> anyhow::Result<()>
{
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let (mut doc, center) = wedge_document("wedge");
        let bonds = doc.bonds.clone();
        doc.bonds = order
            .into_iter()
            .map(|index| bonds[index].clone())
            .collect();
        doc.atoms.rotate_left(2);
        assert_eq!(configuration(&doc, center)?.as_deref(), Some("R"));
        invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
        assert_eq!(configuration(&doc, center)?.as_deref(), Some("S"));
    }
    Ok(())
}

#[test]
fn stored_winding_without_drawing_marks_can_be_inverted() -> anyhow::Result<()> {
    let (mut doc, center) = wedge_document("plain");
    let neighbors = doc.bonds.iter().map(|b| b.b).collect();
    doc.atom_mut(center).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    let before = configuration(&doc, center)?;
    assert!(before.is_some());
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(doc.bonds[0].display, "plain");
    assert_eq!(
        doc.atom(center).unwrap().stereo.as_ref().unwrap().winding,
        "ccw"
    );
    assert_ne!(configuration(&doc, center)?, before);
    assert!(configuration(&doc, center)?.is_some());
    Ok(())
}

#[test]
fn invalid_or_undefined_atoms_fail_without_mutation() -> anyhow::Result<()> {
    let (mut doc, center) = wedge_document("plain");
    let before = doc.clone();
    assert!(invert_configuration(&mut doc, u64::MAX).is_err());
    assert_eq!(doc, before);
    assert!(invert_configuration(&mut doc, center).is_err());
    assert_eq!(doc, before);
    doc.bonds[0].display = "wavy".into();
    let before = doc.clone();
    assert!(invert_configuration(&mut doc, center).is_err());
    assert_eq!(doc, before);
    Ok(())
}

#[test]
fn fischer_l_alanine_inverts_to_d_and_stays_consistent_after_reconciliation() -> anyhow::Result<()>
{
    let mut doc = crate::fischer::document(crate::fischer::Compound::LAlanine, 42.)
        .map_err(anyhow::Error::msg)?;
    let cross = crate::fischer::recognize(&doc).crosses[0];
    let center = cross.center;
    assert_eq!(configuration(&doc, center)?.as_deref(), Some("S"));
    let left = doc.atom(cross.left).unwrap().position;
    let right = doc.atom(cross.right).unwrap().position;
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(doc.atom(cross.left).unwrap().position, right);
    assert_eq!(doc.atom(cross.right).unwrap().position, left);
    assert_eq!(configuration(&doc, center)?.as_deref(), Some("R"));
    let reading = crate::fischer::recognize_center(&doc, center)
        .unwrap()
        .reading();
    assert!(crate::fischer::equivalent(
        doc.atom(center).unwrap().stereo.as_ref().unwrap(),
        &reading
    ));
    let snapshot = doc.clone();
    crate::fischer::reconcile(&mut doc, &snapshot);
    assert_eq!(configuration(&doc, center)?.as_deref(), Some("R"));
    assert!(doc.bonds.iter().all(|b| b.display == "plain"));
    Ok(())
}

#[test]
fn fischer_inversion_reflects_complete_branches_and_rejects_reconnections_atomically()
-> anyhow::Result<()> {
    let mut doc = crate::fischer::document(crate::fischer::Compound::LAlanine, 42.)
        .map_err(anyhow::Error::msg)?;
    let cross = crate::fischer::recognize(&doc).crosses[0];
    let template = doc.clone();
    let old_root = doc.atom(cross.left).unwrap().position;
    let methyl = doc.add_atom("C", old_root.offset(-21., -21.));
    doc.add_bond(cross.left, methyl, 1, "plain");
    crate::fischer::reconcile(&mut doc, &template);
    let before = doc.clone();
    invert_configuration(&mut doc, cross.center).map_err(anyhow::Error::msg)?;
    assert_eq!(
        doc.atom(methyl).unwrap().position,
        Point::new(-before.atom(methyl).unwrap().position.x, old_root.y - 21.)
    );
    assert_eq!(configuration(&doc, cross.center)?.as_deref(), Some("R"));

    let mut cyclic = before;
    let snapshot = cyclic.clone();
    cyclic.add_bond(methyl, cross.down, 1, "plain");
    crate::fischer::reconcile(&mut cyclic, &snapshot);
    let before = cyclic.clone();
    assert!(invert_configuration(&mut cyclic, cross.center).is_err());
    assert_eq!(cyclic, before);
    Ok(())
}

#[test]
fn fischer_inversion_preserves_a_drawn_stereocentre_on_a_horizontal_branch() -> anyhow::Result<()> {
    let mut doc = crate::fischer::document(crate::fischer::Compound::LAlanine, 42.)
        .map_err(anyhow::Error::msg)?;
    let cross = crate::fischer::recognize(&doc).crosses[0];
    let template = doc.clone();
    let origin = doc.atom(cross.left).unwrap().position;
    let root = doc.atom_mut(cross.left).unwrap();
    root.element = "C".into();
    root.label_h = 0;
    for (element, offset, display) in [
        ("F", Point::new(-42., 0.), "wedge"),
        ("Cl", Point::new(0., -42.), "plain"),
        ("Br", Point::new(0., 42.), "plain"),
    ] {
        let atom = doc.add_atom(element, origin.offset(offset.x, offset.y));
        doc.add_bond(cross.left, atom, 1, display);
    }
    crate::fischer::reconcile(&mut doc, &template);
    let before = configuration(&doc, cross.left)?;
    assert!(before.is_some());
    invert_configuration(&mut doc, cross.center).map_err(anyhow::Error::msg)?;
    assert_eq!(configuration(&doc, cross.left)?, before);
    Ok(())
}

#[test]
fn reflected_haworth_subtree_preserves_its_other_drawn_stereocentre() -> anyhow::Result<()> {
    let mut doc = sugar_document(Sugar::Glucose, Anomer::Beta, 42.).map_err(anyhow::Error::msg)?;
    let center = doc.atoms[0].id;
    let stored = doc.atom(center).unwrap().stereo.clone();
    let oxygen = doc
        .bonds
        .iter()
        .filter(|b| !b.projection)
        .find_map(|bond| other(bond, center))
        .unwrap();
    let origin = doc.atom(oxygen).unwrap().position;
    let side_center = doc.add_atom("C", origin.offset(0., -42.));
    doc.add_bond(oxygen, side_center, 1, "plain");
    for (element, offset, display) in [
        ("F", Point::new(-36., -63.), "wedge"),
        ("Cl", Point::new(36., -63.), "plain"),
        ("Br", Point::new(0., -84.), "plain"),
    ] {
        let atom = doc.add_atom(element, origin.offset(offset.x, offset.y));
        doc.add_bond(side_center, atom, 1, display);
    }
    doc.atom_mut(center).unwrap().stereo = stored;
    let old_configuration = configuration(&doc, side_center)?;
    assert!(old_configuration.is_some());
    invert_configuration(&mut doc, center).map_err(anyhow::Error::msg)?;
    assert_eq!(configuration(&doc, side_center)?, old_configuration);
    assert!(
        doc.bonds
            .iter()
            .any(|b| b.a == side_center && b.display == "hash")
    );
    assert_eq!(
        interchange::export_bonds(&doc)
            .map_err(anyhow::Error::msg)?
            .len(),
        6
    );
    Ok(())
}

#[test]
fn haworth_inversion_rejects_a_fischer_subtree_without_changing_another_centre()
-> anyhow::Result<()> {
    let mut doc = sugar_document(Sugar::Glucose, Anomer::Beta, 42.).map_err(anyhow::Error::msg)?;
    let center = doc.atoms[0].id;
    let stored = doc.atom(center).unwrap().stereo.clone();
    let oxygen = doc
        .bonds
        .iter()
        .filter(|bond| !bond.projection)
        .find_map(|bond| other(bond, center))
        .unwrap();
    let origin = doc.atom(oxygen).unwrap().position.offset(0., -84.);
    let branch_center = doc.add_atom("C", origin);
    let mut neighbors = Vec::new();
    for (element, offset) in [
        ("C", Point::new(0., -42.)),
        ("F", Point::new(42., 0.)),
        ("Cl", Point::new(0., 42.)),
        ("Br", Point::new(-42., 0.)),
    ] {
        let atom = doc.add_atom(element, origin.offset(offset.x, offset.y));
        doc.add_bond(branch_center, atom, 1, "plain");
        doc.bonds.last_mut().unwrap().projection = true;
        neighbors.push(atom);
    }
    doc.add_bond(oxygen, neighbors[0], 1, "plain");
    let cross = crate::fischer::recognize_center(&doc, branch_center).unwrap();
    doc.atom_mut(branch_center).unwrap().stereo = Some(cross.reading());
    doc.atom_mut(center).unwrap().stereo = stored;
    let original = doc.clone();
    let error = invert_configuration(&mut doc, center).unwrap_err();
    assert!(error.contains("Fischer projection"), "{error}");
    assert_eq!(doc, original);
    Ok(())
}

#[test]
fn off_axis_fischer_centre_is_not_inverted() -> anyhow::Result<()> {
    let mut doc = crate::fischer::document(crate::fischer::Compound::DGlyceraldehyde, 42.)
        .map_err(anyhow::Error::msg)?;
    let center = crate::fischer::recognize(&doc).crosses[0].center;
    let ids = doc.all_ids();
    crate::editing::transform(&mut doc, &ids, crate::editing::Transform::Rotate(37.));
    let original = doc.clone();
    let error = invert_configuration(&mut doc, center).unwrap_err();
    assert!(error.contains("Align the Fischer projection"), "{error}");
    assert_eq!(doc, original);
    Ok(())
}
