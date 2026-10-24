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
