use super::*;

#[test]
fn abbreviation_containing_returns_the_first_member_match_or_none() {
    let first = Abbreviation {
        label_style: None,
        label_color_override: false,
        highlight: None,
        label: "first".into(),
        reverse_label: String::new(),
        anchor: 1,
        members: vec![1, 2],
        alignment: Default::default(),
    };
    let second = Abbreviation {
        label: "second".into(),
        anchor: 2,
        members: vec![2, 3],
        ..first.clone()
    };
    let doc = Document {
        abbreviations: vec![first.clone(), second.clone()],
        ..Default::default()
    };

    assert_eq!(doc.abbreviation_containing(1), Some(&first));
    assert_eq!(doc.abbreviation_containing(2), Some(&first));
    assert_eq!(doc.abbreviation_containing(3), Some(&second));
    assert_eq!(doc.abbreviation_containing(99), None);
}

#[test]
fn reversed_charged_labels_anchor_on_their_atom() {
    assert_eq!(anchor_range("Me3N+", true), 3..4);
    assert_eq!(anchor_range("F3B-", true), 2..3);
    assert_eq!(anchor_range("NMe3+", false), 0..1);
    assert_eq!(anchor_range("TBSO", true), 3..4);
    // Isothiocyanate and thiocyanate reverse to distinct spellings that end
    // in their attachment atom.
    assert_eq!(anchor_range("S=C=N", true), 4..5);
    let thiocyanate = "N≡CS";
    assert_eq!(&thiocyanate[anchor_range(thiocyanate, true)], "S");
}

fn shortcut_examples() -> Document {
    Document::from_json(include_bytes!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap()
}

#[test]
fn bond_visible_matches_atom_visibility_and_shared_groups() {
    let doc = shortcut_examples();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    for &a in &ids {
        for &b in &ids {
            let expected = doc.atom_visible(a)
                && doc.atom_visible(b)
                && !doc
                    .abbreviations
                    .iter()
                    .any(|g| g.members.contains(&a) && g.members.contains(&b));
            assert_eq!(doc.bond_visible(a, b), expected, "{a}-{b}");
        }
    }
}

#[test]
fn reconcile_keeps_moved_groups_and_drops_only_chemically_edited_ones() {
    let before = shortcut_examples();
    let groups = before.abbreviations.clone();
    assert!(groups.len() > 2);

    let mut moved = before.clone();
    for atom in &mut moved.atoms {
        atom.position = atom.position.offset(5., -3.);
    }
    moved.reconcile_abbreviations(&before);
    assert_eq!(moved.abbreviations, groups);

    // A bond order change inside one group reveals that group only.
    let inside = |g: &Abbreviation, b: &crate::document::Bond| {
        g.members.contains(&b.a) && g.members.contains(&b.b)
    };
    let edited = groups
        .iter()
        .position(|g| before.bonds.iter().any(|b| inside(g, b)))
        .unwrap();
    let mut doc = before.clone();
    let bond = doc
        .bonds
        .iter_mut()
        .find(|b| inside(&groups[edited], b))
        .unwrap();
    bond.order = if bond.order == 1 { 2 } else { 1 };
    doc.reconcile_abbreviations(&before);
    let mut expected = groups.clone();
    expected.remove(edited);
    assert_eq!(doc.abbreviations, expected);

    // An element change on a member reveals its group only.
    let mut doc = before.clone();
    let last = groups.len() - 1;
    let member = groups[last].members[0];
    doc.atom_mut(member).unwrap().element = "Si".into();
    doc.reconcile_abbreviations(&before);
    assert_eq!(doc.abbreviations, groups[..last]);
}

/// Import only the chemical graph, keeping tests independent of depiction.
/// The first SMILES atom is outside the contracted fragment.
fn contract_fragment(smiles: &str) -> (Document, Vec<u64>) {
    let prepared = crate::chemistry::smiles::prepare(smiles).unwrap();
    let mut doc = Document::default();
    let ids: Vec<_> = prepared
        .state
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| {
            let symbol = crate::chemistry::ELEMENTS[usize::from(atom.atomic_number)].symbol;
            let id = doc.add_atom(symbol, Point::new(index as f32 * 42., 0.));
            let drawing = doc.atom_mut(id).unwrap();
            drawing.charge = i32::from(atom.charge);
            drawing.explicit_h = u32::from(atom.explicit_hydrogens);
            drawing.aromatic = atom.aromatic;
            id
        })
        .collect();
    for bond in &prepared.state.graph.bonds {
        doc.add_bond(ids[bond.a], ids[bond.b], bond.order, "plain");
    }
    (doc, ids[1..].to_vec())
}

#[test]
fn contract_label_warnings_check_both_spellings_and_element_exceptions() {
    let (doc, members) = contract_fragment("NCC");
    let before = doc.clone();
    let warnings = doc.contract_label_warnings(&members, " Custom ", " Co ");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Co is an element symbol"));
    assert_eq!(doc.contract_label_warnings(&members, "Co", "Co"), warnings);
    assert!(
        doc.contract_label_warnings(&members, "CustomCo", "co")
            .is_empty()
    );
    for label in ["Ac", "Cm", "Nb", "Np", "Pr", "Ts"] {
        assert!(
            doc.contract_label_warnings(&members, label, "")
                .iter()
                .all(|warning| !warning.contains("element symbol")),
            "{label}"
        );
    }
    assert_eq!(doc, before);
    let mut contracted = doc;
    contracted.contract(&members, "Co", "").unwrap();
    assert_eq!(contracted.abbreviations[0].label, "Co");
}

#[test]
fn contract_label_warnings_compare_canonical_bundled_structures() {
    let (doc, members) = contract_fragment("NCC");
    let warnings = doc.contract_label_warnings(&members, "Boc", "");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Boc means tert-butoxycarbonyl"));

    let (mut doc, mut members) = contract_fragment("NC(OC(C)(C)C)=O");
    // Both SMILES traversal and native atom order differ from the catalog.
    doc.atoms.reverse();
    doc.bonds.reverse();
    assert!(doc.contract_label_warnings(&members, "Boc", "").is_empty());
    doc.atom_mut(members[0]).unwrap().map_num = 17;
    assert!(doc.contract_label_warnings(&members, "Boc", "").is_empty());
    let carbon = members[3];
    let hydrogen = doc.add_atom("H", Point::new(200., 42.));
    doc.add_bond(carbon, hydrogen, 1, "plain");
    members.push(hydrogen);
    assert!(doc.contract_label_warnings(&members, "Boc", "").is_empty());
    doc.atom_mut(carbon).unwrap().isotope = 13;
    assert_eq!(doc.contract_label_warnings(&members, "Boc", "").len(), 1);
}

#[test]
fn contract_label_warnings_use_bundled_reverse_spellings_and_ignore_user_overrides() {
    let errors = crate::chemistry::abbreviations::set_user_on_thread(
        r#"{"format":1,"groups":[{"label":"Boc","smiles":"*CC"}]}"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
    let (doc, members) = contract_fragment("NCC");
    assert_eq!(doc.contract_label_warnings(&members, "Boc", "").len(), 1);
    let (doc, members) = contract_fragment("NOC");
    assert!(
        doc.contract_label_warnings(&members, "Custom", "MeO")
            .is_empty()
    );
    let (doc, members) = contract_fragment("NCC");
    assert_eq!(
        doc.contract_label_warnings(&members, "Custom", "MeO").len(),
        1
    );
    let errors = crate::chemistry::abbreviations::set_user_on_thread(r#"{"format":1,"groups":[]}"#);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn contract_label_warnings_explain_bz_and_suggest_bn_for_benzyl() {
    let (doc, members) = contract_fragment("NCc1ccccc1");
    let warnings = doc.contract_label_warnings(&members, "Bz", "");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Bz means benzoyl"));
    assert!(warnings[0].contains("use Bn for benzyl"));
    assert!(doc.contract_label_warnings(&members, "Bn", "").is_empty());
    let (doc, members) = contract_fragment("NC(=O)c1ccccc1");
    assert!(doc.contract_label_warnings(&members, "Bz", "").is_empty());
}
