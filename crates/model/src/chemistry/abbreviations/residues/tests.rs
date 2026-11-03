use super::*;
use crate::chemistry::{abbreviations, depict};

fn drawing(text: &str) -> Document {
    let input = smiles::read(text).unwrap();
    let state = input.prepared.state;
    let positions = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )
    .unwrap()
    .positions;
    let molecule = document::Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        state,
        positions,
    };
    let draft =
        document::for_generated(&molecule, &input.prepared.dummy_labels, Default::default())
            .unwrap();
    let labels = draft.labels().unwrap();
    draft.finish(labels).unwrap()
}

fn found(doc: &Document, label: &str, selection: &[u64]) -> Document {
    abbreviations::find_with_policy(
        doc,
        &document::prepare(doc).unwrap(),
        selection,
        Some(label),
        AttachmentPolicy::SharedAnchor,
    )
    .unwrap()
}

fn protected(definition: &ResidueDefinition) -> String {
    definition
        .smiles
        .replace("[*:1]", "CC(C)(C)OC(=O)")
        .replace("[*:2]", "OC")
}

#[test]
fn standard_table_has_mapped_dummies_and_all_d_definitions_mirror() {
    assert_eq!(RESIDUES.len(), 20);
    assert_eq!(residue_labels().count(), 39);
    assert!(residue("D-Gly").is_none());
    assert!(residue("Orn").is_none());
    assert!(residue("L-Ala").is_none());
    for entry in RESIDUES {
        let l = residue(entry.symbol).unwrap();
        let prepared = smiles::prepare(&l.smiles).unwrap();
        let dummy_maps: Vec<_> = prepared
            .state
            .graph
            .atoms
            .iter()
            .zip(&prepared.state.metadata.atoms)
            .filter(|(atom, _)| atom.atomic_number == 0)
            .map(|(_, meta)| meta.map_number)
            .collect();
        assert_eq!(dummy_maps, [1, 2]);
        if entry.symbol != "Gly" {
            let d = residue(&format!("D-{}", entry.symbol)).unwrap();
            assert_ne!(
                definition_identity(&l).unwrap(),
                definition_identity(&d).unwrap()
            );
            let d_state = smiles::prepare(&d.smiles).unwrap().state;
            let chiral: Vec<_> = prepared
                .state
                .metadata
                .atoms
                .iter()
                .map(|a| a.chiral_tag)
                .collect();
            let mirrored: Vec<_> = d_state
                .metadata
                .atoms
                .iter()
                .map(|a| a.chiral_tag)
                .collect();
            for (l, d) in chiral.into_iter().zip(mirrored) {
                assert_eq!(
                    d,
                    match l {
                        1 => 2,
                        2 => 1,
                        other => other,
                    },
                    "{}",
                    entry.symbol
                );
            }
        }
    }
}

#[test]
fn every_protected_l_and_d_residue_finds_with_n_anchor_and_carbonyl_exit() {
    for label in residue_labels() {
        let doc = drawing(&protected(&residue(&label).unwrap()));
        let result = found(&doc, &label, &[]);
        assert_eq!(result.abbreviations.len(), 1, "{label}");
        let group = &result.abbreviations[0];
        assert_eq!(group.label, label);
        assert_eq!(result.atom(group.anchor).unwrap().element, "N", "{label}");
        assert_eq!(
            result.atom(group.exit.unwrap()).unwrap().element,
            "C",
            "{label}"
        );
        assert_eq!(result.atoms, doc.atoms);
        assert_eq!(result.bonds, doc.bonds);
        assert_eq!(
            canonical(&document::prepare(&result).unwrap().state).unwrap(),
            canonical(&document::prepare(&doc).unwrap().state).unwrap()
        );
        assert!(residue_matches_selection(&doc, &group.members, &label).unwrap());
        if label != "Gly" {
            let other = label
                .strip_prefix("D-")
                .map(str::to_string)
                .unwrap_or_else(|| format!("D-{label}"));
            assert!(
                found(&doc, &other, &[]).abbreviations.is_empty(),
                "{label} mistaken for {other}"
            );
            assert!(!residue_matches_selection(&doc, &group.members, &other).unwrap());
        }
    }
}

#[test]
fn custom_contract_uses_residue_attachments_with_reversed_termini() {
    let mut doc = drawing("CC(C)(C)OC(=O)N[C@@H](C)C(=O)OC");
    let residue = found(&doc, "Ala", &[]).abbreviations.remove(0);
    let exit = residue.exit.unwrap();
    for (attachment, x) in [(residue.anchor, 200.), (exit, -200.)] {
        let outside = doc
            .bonds
            .iter()
            .find_map(|bond| {
                let other = if bond.a == attachment {
                    bond.b
                } else if bond.b == attachment {
                    bond.a
                } else {
                    return None;
                };
                (!residue.members.contains(&other)).then_some(other)
            })
            .unwrap();
        doc.atom_mut(outside).unwrap().position.x = x;
    }
    for (label, reverse, expected) in [
        (" Ala ", "", (residue.anchor, Some(exit))),
        ("D-Ala", "", (exit, Some(residue.anchor))),
        ("Custom", "Ala", (exit, Some(residue.anchor))),
    ] {
        let mut contracted = doc.clone();
        contracted
            .contract(&residue.members, label, reverse)
            .unwrap();
        let group = &contracted.abbreviations[0];
        assert_eq!((group.anchor, group.exit), expected, "{label}");
        assert_eq!(group.label, label.trim());
        assert_eq!(contracted.atoms, doc.atoms);
        assert_eq!(contracted.bonds, doc.bonds);
    }
}

#[test]
fn residue_temporary_ids_ignore_unrelated_page_objects() {
    let mut doc = drawing("CC(C)(C)OC(=O)N[C@@H](C)C(=O)OC");
    let expected = found(&doc, "Ala", &[]).abbreviations;
    let other = doc.add_atom("Ne", crate::document::Point::new(0., 1000.));
    doc.atom_mut(other).unwrap().id = u64::MAX - 1;
    let result = found(&doc, "Ala", &[]);
    assert_eq!(result.abbreviations, expected);
    assert_eq!(result.atoms, doc.atoms);
    assert_eq!(result.bonds, doc.bonds);
}

#[test]
fn boc_ala_ome_requires_stereo_and_matches_all_centers_of_ile_and_thr() {
    let doc = drawing("CC(C)(C)OC(=O)N[C@@H](C)C(=O)OC");
    let result = found(&doc, "Ala", &[]);
    assert_eq!(result.abbreviations.len(), 1);
    assert!(found(&doc, "D-Ala", &[]).abbreviations.is_empty());
    let group = &result.abbreviations[0];
    assert_eq!(group.members.len(), 5);
    assert!(!residue_matches_selection(&doc, &group.members, "Gly").unwrap());
    let unspecified = drawing("CC(C)(C)OC(=O)NC(C)C(=O)OC");
    assert!(found(&unspecified, "Ala", &[]).abbreviations.is_empty());
    assert!(found(&unspecified, "D-Ala", &[]).abbreviations.is_empty());
    for (label, allo) in [
        ("Ile", "CC(C)(C)OC(=O)N[C@@H]([C@H](C)CC)C(=O)OC"),
        ("Thr", "CC(C)(C)OC(=O)N[C@@H]([C@@H](O)C)C(=O)OC"),
    ] {
        let doc = drawing(allo);
        assert!(
            found(&doc, label, &[]).abbreviations.is_empty(),
            "allo-{label}"
        );
        assert!(
            found(&doc, &format!("D-{label}"), &[])
                .abbreviations
                .is_empty(),
            "allo-{label}"
        );
    }
    // Independent inputs cover natural and fully mirrored side-chain centers,
    // and Pro's N-connected side-chain ring rather than deriving the inputs
    // from the definitions being matched.
    for (label, text) in [
        ("Ile", "CC(C)(C)OC(=O)N[C@@H]([C@@H](C)CC)C(=O)OC"),
        ("D-Ile", "CC(C)(C)OC(=O)N[C@H]([C@H](C)CC)C(=O)OC"),
        ("Thr", "CC(C)(C)OC(=O)N[C@@H]([C@H](O)C)C(=O)OC"),
        ("D-Thr", "CC(C)(C)OC(=O)N[C@H]([C@@H](O)C)C(=O)OC"),
        ("Pro", "CC(C)(C)OC(=O)N1[C@@H](CCC1)C(=O)OC"),
        ("D-Pro", "CC(C)(C)OC(=O)N1[C@H](CCC1)C(=O)OC"),
        ("Gly", "CC(C)(C)OC(=O)NCC(=O)OC"),
    ] {
        assert_eq!(
            found(&drawing(text), label, &[]).abbreviations.len(),
            1,
            "{label}"
        );
    }
}

#[test]
fn multiple_residues_selection_and_existing_groups_do_not_overlap() {
    let doc = drawing("CC(C)(C)OC(=O)N[C@@H](C)C(=O)N[C@@H](C)C(=O)OC");
    let all = found(&doc, "Ala", &[]);
    assert_eq!(all.abbreviations.len(), 2);
    let members = all.abbreviations[0].members.clone();
    let selected = found(&doc, "Ala", &members);
    assert_eq!(selected.abbreviations.len(), 1);
    assert_eq!(selected.abbreviations[0], all.abbreviations[0]);
    assert!(
        found(&doc, "Ala", &members[..members.len() - 1])
            .abbreviations
            .is_empty()
    );
    assert_eq!(found(&selected, "Ala", &[]).abbreviations.len(), 2);
    assert_eq!(found(&all, "Ala", &[]).abbreviations.len(), 2);
}

#[test]
fn residues_need_two_single_boundaries_and_keep_marked_atoms_visible() {
    let free_amine = drawing("N[C@@H](C)C(=O)OC");
    assert!(found(&free_amine, "Ala", &[]).abbreviations.is_empty());
    let mut doc = drawing("CC(C)(C)OC(=O)N[C@@H](C)C(=O)OC");
    let full = found(&doc, "Ala", &[]);
    let member = full.abbreviations[0].members[0];
    doc.atom_mut(member).unwrap().map_num = 12;
    assert!(found(&doc, "Ala", &[]).abbreviations.is_empty());
    for label in ["Ala", "D-Ala", "Gly", "Pro"] {
        assert!(abbreviations::lookup(label).unwrap().is_none());
        assert!(
            abbreviations::replace_with_policy(&doc, &[], label, AttachmentPolicy::SharedAnchor)
                .unwrap_err()
                .to_string()
                .contains("two attachment points")
        );
        assert!(
            abbreviations::replace_bundled(&doc, &[], label)
                .unwrap_err()
                .to_string()
                .contains("two attachment points")
        );
    }
    let all = abbreviations::find_with_policy(
        &doc,
        &document::prepare(&doc).unwrap(),
        &[],
        None,
        AttachmentPolicy::SharedAnchor,
    )
    .unwrap();
    assert!(all.abbreviations.iter().all(|group| group.exit.is_none()));
}

#[test]
fn explicitly_drawn_amino_hydrogen_is_part_of_gly_residue() {
    let mut doc = drawing("CC(C)(C)OC(=O)NCC(=O)OC");
    let implicit = found(&doc, "Gly", &[]);
    let nitrogen = implicit.abbreviations[0].anchor;
    let position = doc.atom(nitrogen).unwrap().position;
    let hydrogen = doc.add_atom(
        "H",
        crate::document::Point::new(position.x, position.y - 28.),
    );
    doc.add_bond(nitrogen, hydrogen, 1, "plain");
    let result = found(&doc, "Gly", &[]);
    assert_eq!(result.abbreviations.len(), 1);
    assert!(result.abbreviations[0].members.contains(&hydrogen));
}

#[test]
fn residue_groups_validate_only_with_shared_anchor_and_valid_distinct_exit() {
    let doc = drawing("CC(C)(C)OC(=O)N[C@@H](C)C(=O)OC");
    let mut result = found(&doc, "Ala", &[]);
    assert!(validate_with_policy(&result, AttachmentPolicy::SharedAnchor).is_ok());
    assert!(matches!(
        validate_with_policy(&result, AttachmentPolicy::Terminal),
        Err(Error::Attachment)
    ));
    let exit = result.abbreviations[0].exit;
    result.abbreviations[0].exit = Some(result.abbreviations[0].anchor);
    assert!(matches!(
        validate_with_policy(&result, AttachmentPolicy::SharedAnchor),
        Err(Error::Members)
    ));
    result.abbreviations[0].exit = Some(u64::MAX);
    assert!(matches!(
        validate_with_policy(&result, AttachmentPolicy::SharedAnchor),
        Err(Error::Members)
    ));
    result.abbreviations[0].exit = None;
    assert!(matches!(
        validate_with_policy(&result, AttachmentPolicy::SharedAnchor),
        Err(Error::InternalAttachment)
    ));
    result.abbreviations[0].exit = exit;
}
