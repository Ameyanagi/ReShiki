use super::*;
use crate::{
    chemistry::{document::prepare, smiles, stereo::cip::label},
    document::History,
    editing::{self, Transform},
};

fn identity(doc: &Document) -> anyhow::Result<String> {
    let molecule = prepare(doc)?;
    // SMILES import normalizes the template's explicit H into the same graph
    // representation as each implicit-H reference structure.
    canonical(&smiles::write::write(&molecule.state, Default::default())?.text)
}

fn canonical(text: &str) -> anyhow::Result<String> {
    let parsed = smiles::read(text)?;
    Ok(smiles::write::write(&parsed.prepared.state, Default::default())?.text)
}

fn labels(doc: &Document) -> anyhow::Result<Vec<String>> {
    let molecule = prepare(doc)?;
    let assigned = label::assign(&molecule.state, &Default::default())?;
    let codes: HashMap<_, _> = molecule
        .ids
        .iter()
        .copied()
        .zip(
            assigned
                .state
                .properties
                .atoms
                .into_iter()
                .map(|a| a.cip_code),
        )
        .collect();
    Ok(recognize(doc)
        .crosses
        .into_iter()
        .map(|c| codes[&c.center].clone().unwrap())
        .collect())
}

#[test]
fn templates_match_named_molecular_identities_and_cip() -> anyhow::Result<()> {
    for (template, expected_cip) in templates().map_err(anyhow::Error::msg)?.into_iter().zip([
        vec!["R"],
        vec!["S"],
        vec!["R", "S", "R", "R"],
        vec!["R", "R", "R"],
        vec!["S"],
        vec!["S"],
        vec!["R"],
    ]) {
        assert_eq!(
            identity(&template.document)?,
            canonical(&template.smiles)?,
            "{}",
            template.name
        );
        assert_eq!(
            labels(&template.document)?,
            expected_cip,
            "{}",
            template.name
        );
        template.document.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

#[test]
fn recognition_requires_explicit_plain_single_four_bond_axis_crosses() -> anyhow::Result<()> {
    let doc = document(Compound::DGlyceraldehyde, 42.).map_err(anyhow::Error::msg)?;
    let cross = recognize(&doc).crosses[0];
    assert_eq!(
        cross.reading(),
        doc.atom(cross.center).unwrap().stereo.clone().unwrap()
    );
    for change in 0..7 {
        let mut edited = doc.clone();
        match change {
            0 => edited.bonds[cross.bonds[0]].projection = false,
            1 => edited.bonds[cross.bonds[0]].display = "wedge".into(),
            2 => edited.bonds[cross.bonds[0]].order = 2,
            3 => edited.atom_mut(cross.up).unwrap().position.x += 20.,
            4 => {
                edited.atom_mut(cross.left).unwrap().position =
                    edited.atom(cross.right).unwrap().position
            }
            5 => {
                edited.bonds.remove(cross.bonds[0]);
            }
            6 => {
                let other = edited.add_atom("F", Point::new(20., 20.));
                edited.add_bond(cross.center, other, 1, "plain");
            }
            _ => unreachable!(),
        }
        assert!(recognize(&edited).crosses.is_empty(), "case {change}");
    }
    let mut ordinary = doc.clone();
    for bond in &mut ordinary.bonds {
        bond.projection = false;
    }
    assert!(recognize(&ordinary).crosses.is_empty());
    assert!(marked_centers(&ordinary).is_empty());
    Ok(())
}

#[test]
fn reading_obeys_neighbor_order_parity() {
    let original = reading(1, 2, 3, 4);
    assert!(equivalent(&original, &reading(3, 4, 1, 2)));
    assert!(!equivalent(&original, &reading(4, 1, 2, 3)));
    assert!(!equivalent(&original, &reading(1, 4, 3, 2)));
    let mut reordered = original.clone();
    reordered.neighbors.swap(0, 1);
    reordered.winding = if original.winding == "cw" {
        "ccw"
    } else {
        "cw"
    }
    .into();
    assert!(equivalent(&original, &reordered));
    assert!(!equivalent(&original, &reading(1, 2, 3, 5)));
}

#[test]
fn reconcile_repairs_only_different_configurations() -> anyhow::Result<()> {
    let mut doc = document(Compound::DGlyceraldehyde, 42.).map_err(anyhow::Error::msg)?;
    let cross = recognize(&doc).crosses[0];
    let template = doc.clone();
    // An edit that cleared the cached stereo keeps the Fischer identity.
    doc.atom_mut(cross.center).unwrap().stereo = None;
    reconcile(&mut doc, &template);
    assert_eq!(
        doc.atom(cross.center).unwrap().stereo,
        Some(cross.reading())
    );
    // A cross that was not a Fischer object before the edit gains nothing.
    let mut plain = doc.clone();
    plain.atom_mut(cross.center).unwrap().stereo = None;
    let snapshot = plain.clone();
    reconcile(&mut plain, &snapshot);
    assert_eq!(plain, snapshot);
    let atom = doc.atom_mut(cross.center).unwrap();
    atom.stereo.as_mut().unwrap().neighbors.rotate_left(2);
    atom.cip_label = Some("R".into());
    let before = doc.clone();
    reconcile(&mut doc, &before);
    assert_eq!(
        doc, before,
        "equivalent neighbor orders should not be rewritten"
    );
    doc.atom_mut(cross.center)
        .unwrap()
        .stereo
        .as_mut()
        .unwrap()
        .winding = "cw".into();
    reconcile(&mut doc, &before);
    assert_eq!(
        doc.atom(cross.center).unwrap().stereo,
        Some(cross.reading())
    );
    assert_eq!(doc.atom(cross.center).unwrap().cip_label, None);
    Ok(())
}

#[test]
fn axis_tolerance_accepts_hand_placement_without_reading_an_off_axis_cross() -> anyhow::Result<()> {
    let original = document(Compound::DGlyceraldehyde, 42.).map_err(anyhow::Error::msg)?;
    assert_eq!(recognize(&rotated(&original, 9.)?).crosses.len(), 1);
    assert!(recognize(&rotated(&original, 11.)?).crosses.is_empty());
    Ok(())
}

#[test]
fn swapping_horizontal_groups_and_mirror_flips_invert_configuration() -> anyhow::Result<()> {
    let original = document(Compound::DGlyceraldehyde, 42.).map_err(anyhow::Error::msg)?;
    let cross = recognize(&original).crosses[0];
    let mut swapped = original.clone();
    let left = swapped.atom(cross.left).unwrap().position;
    let right = swapped.atom(cross.right).unwrap().position;
    swapped.atom_mut(cross.left).unwrap().position = right;
    swapped.atom_mut(cross.right).unwrap().position = left;
    crate::transaction::apply(
        &mut swapped,
        &mut History::default(),
        original.clone(),
        false,
    )
    .map_err(|e| anyhow::anyhow!(e.message().to_owned()))?;
    assert_eq!(labels(&swapped)?, ["S"]);
    for transform in [Transform::FlipHorizontal, Transform::FlipVertical] {
        let mut flipped = original.clone();
        let ids = flipped.all_ids();
        editing::transform(&mut flipped, &ids, transform);
        crate::transaction::apply(
            &mut flipped,
            &mut History::default(),
            original.clone(),
            false,
        )
        .map_err(|e| anyhow::anyhow!(e.message().to_owned()))?;
        assert_eq!(labels(&flipped)?, ["S"]);
    }
    Ok(())
}

fn rotated(doc: &Document, degrees: f32) -> anyhow::Result<Document> {
    let mut after = doc.clone();
    let ids = after.all_ids();
    editing::transform(&mut after, &ids, Transform::Rotate(degrees));
    crate::transaction::apply(&mut after, &mut History::default(), doc.clone(), false)
        .map_err(|e| anyhow::anyhow!(e.message().to_owned()))?;
    Ok(after)
}

#[test]
fn normal_rotation_transaction_rereads_quarter_turn_and_preserves_half_turn() -> anyhow::Result<()>
{
    let original = document(Compound::DGlyceraldehyde, 42.).map_err(anyhow::Error::msg)?;
    assert_eq!(labels(&original)?, ["R"]);
    let quarter = rotated(&original, 90.)?;
    assert_eq!(labels(&quarter)?, ["S"]);
    assert_eq!(identity(&quarter)?, canonical("O=C[C@@H](O)CO")?);
    let half = rotated(&original, 180.)?;
    assert_eq!(labels(&half)?, ["R"]);
    assert_eq!(identity(&half)?, identity(&original)?);
    let tilted = rotated(&original, 37.)?;
    assert!(recognize(&tilted).crosses.is_empty());
    assert_eq!(marked_centers(&tilted), marked_centers(&original));
    assert_eq!(identity(&tilted)?, identity(&original)?);
    assert_eq!(
        tilted.atom(1).unwrap().stereo,
        original.atom(1).unwrap().stereo
    );
    let back_on_axis = rotated(&tilted, 53.)?;
    assert_eq!(labels(&back_on_axis)?, ["S"]);
    Ok(())
}

#[test]
fn pinned_rotation_uses_the_same_transaction_reading() -> anyhow::Result<()> {
    let original = document(Compound::DGlyceraldehyde, 42.).map_err(anyhow::Error::msg)?;
    let (mut turned, _) = editing::reference::rotate(
        &original,
        &original.all_ids(),
        Point::new(15., 20.),
        90.,
        false,
    )
    .map_err(anyhow::Error::msg)?;
    crate::transaction::apply(&mut turned, &mut History::default(), original, false)
        .map_err(|e| anyhow::anyhow!(e.message().to_owned()))?;
    assert_eq!(labels(&turned)?, ["S"]);
    Ok(())
}

#[test]
fn native_round_trip_retains_projection_graph_labels_and_stereo() -> anyhow::Result<()> {
    for template in templates().map_err(anyhow::Error::msg)? {
        let saved = template.document.file_json().map_err(anyhow::Error::msg)?;
        let restored = Document::from_native_file(&saved).map_err(anyhow::Error::msg)?;
        assert_eq!(restored, template.document.current());
        assert_eq!(identity(&restored)?, canonical(&template.smiles)?);
        let recognized = recognize(&restored);
        assert!(!recognized.crosses.is_empty());
        for cross in recognized.crosses {
            assert!(equivalent(
                &cross.reading(),
                restored
                    .atom(cross.center)
                    .unwrap()
                    .stereo
                    .as_ref()
                    .unwrap()
            ));
            for id in [cross.left, cross.right] {
                let atom = restored.atom(id).unwrap();
                if atom.element == "O" || atom.element == "N" {
                    assert_eq!(
                        atom.display.hydrogen_position,
                        if id == cross.left {
                            crate::atom_labels::HydrogenPosition::Left
                        } else {
                            crate::atom_labels::HydrogenPosition::Right
                        }
                    );
                }
            }
        }
        assert!(!restored.abbreviations.is_empty());
        assert_eq!(
            restored.atoms.iter().filter(|a| a.element == "H").count(),
            recognized.stereo.len()
        );
    }
    Ok(())
}

#[test]
fn haworth_documents_are_not_fischer_and_reconcile_does_not_touch_their_stereo()
-> anyhow::Result<()> {
    for template in crate::haworth::templates().map_err(anyhow::Error::msg)? {
        assert!(
            recognize(&template.document).crosses.is_empty(),
            "{}",
            template.name
        );
        assert!(marked_centers(&template.document).is_empty());
        let mut after = template.document.clone();
        let ids = after.all_ids();
        editing::transform(&mut after, &ids, Transform::Rotate(90.));
        reconcile(&mut after, &template.document);
        for atom in &after.atoms {
            assert_eq!(atom.stereo, template.document.atom(atom.id).unwrap().stereo);
        }
    }
    Ok(())
}

#[test]
fn builtin_catalog_includes_fischer_entries() -> anyhow::Result<()> {
    assert!(crate::templates::builtin_error().is_none());
    let fischer: Vec<_> = crate::templates::LIBRARY
        .iter()
        .filter(|t| t.group == "Fischer projections")
        .collect();
    assert_eq!(fischer.len(), 7);
    for template in fischer {
        assert!(template.id.starts_with("builtin:Fischer projections/"));
        assert!(template.keywords.iter().any(|k| k == "Fischer"));
        assert!(template.keywords.iter().any(|k| k == "projection"));
    }
    for length in [f32::NAN, f32::INFINITY, -1., 0.] {
        assert!(document(Compound::DGlucose, length).is_err());
    }
    for length in [0.5, 10_001.] {
        assert!(document(Compound::DGlucose, length).is_ok());
    }
    Ok(())
}

#[test]
fn emphasized_front_bonds_on_a_flat_cross_never_make_a_fischer_centre() {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for (element, x, y) in [
        ("F", 0., -42.),
        ("Cl", 42., 0.),
        ("Br", 0., 42.),
        ("I", -42., 0.),
    ] {
        let atom = doc.add_atom(element, Point::new(x, y));
        doc.add_bond(center, atom, 1, "plain");
    }
    let before = doc.clone();
    crate::projection::depth_bonds(&mut doc, &before.all_ids());
    assert!(
        doc.bonds
            .iter()
            .all(|b| b.projection && b.display == "plain")
    );
    assert!(crate::transaction::reconcile(&mut doc, before).is_ok());
    assert!(doc.atom(center).unwrap().stereo.is_none());
    assert!(marked_centers(&doc).is_empty());
}
