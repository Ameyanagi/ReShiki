use super::super::parse;
use super::*;
use crate::document::Point;

/// CH3–O–C(=O)–C(CH3)3 with the methyl outside the selection.
fn pivalate() -> (Document, u64, Vec<u64>) {
    let mut doc = Document::default();
    let methyl = doc.add_atom("C", Point::new(0., 0.));
    let oxygen = doc.add_atom("O", Point::new(42., 0.));
    let carbonyl = doc.add_atom("C", Point::new(84., 0.));
    let carbonyl_oxygen = doc.add_atom("O", Point::new(84., -42.));
    let quaternary = doc.add_atom("C", Point::new(126., 0.));
    let mut members = vec![oxygen, carbonyl, carbonyl_oxygen, quaternary];
    doc.add_bond(methyl, oxygen, 1, "plain");
    doc.add_bond(oxygen, carbonyl, 1, "plain");
    doc.add_bond(carbonyl, carbonyl_oxygen, 2, "plain");
    doc.add_bond(carbonyl, quaternary, 1, "plain");
    for (x, y) in [(168., 0.), (126., 42.), (126., -42.)] {
        let carbon = doc.add_atom("C", Point::new(x, y));
        doc.add_bond(quaternary, carbon, 1, "plain");
        members.push(carbon);
    }
    (doc, methyl, members)
}

#[test]
fn a_fragment_with_one_outside_bond_becomes_an_entry() {
    let (doc, _, members) = pivalate();
    let defined = from_selection(&doc, &members, " OPv ", "PvO").unwrap();
    let entry = &defined.entry;
    assert_eq!(
        (entry.label.as_str(), entry.reverse.as_str()),
        ("OPv", "PvO")
    );
    assert!(entry.smiles.starts_with('*'), "{}", entry.smiles);
    assert_eq!(entry.formula.as_deref(), Some("C5H9O2"));
    assert_eq!(entry.category, "user");
    let mut expected = members.clone();
    expected.sort_unstable();
    assert_eq!(defined.members, expected);
    let preset = Preset::from_entry(entry.clone(), true).unwrap();
    assert_eq!(preset.atoms.len(), members.len() + 1);
}

#[test]
fn a_selected_star_atom_is_the_attachment() {
    let mut doc = Document::default();
    let star = doc.add_atom("*", Point::new(0., 0.));
    let carbon = doc.add_atom("C", Point::new(42., 0.));
    let fluorine = doc.add_atom("F", Point::new(84., 0.));
    doc.add_bond(star, carbon, 1, "plain");
    doc.add_bond(carbon, fluorine, 1, "plain");
    let defined = from_selection(&doc, &[star, carbon, fluorine], "CH2F2", "").unwrap();
    assert_eq!(defined.entry.formula.as_deref(), Some("CH2F"));
    assert_eq!(defined.members, [carbon, fluorine]);
}

#[test]
fn ambiguous_attachments_and_bad_labels_are_refused() {
    let (doc, methyl, members) = pivalate();
    // Without the carbonyl oxygen, the fragment has two outside bonds.
    assert!(from_selection(&doc, &members[..2], "X1", "").is_err());
    // The whole molecule has no attachment at all.
    let mut all = members.clone();
    all.push(methyl);
    assert!(from_selection(&doc, &all, "X1", "").is_err());
    assert!(from_selection(&doc, &[], "X1", "").is_err());
    let error = from_selection(&doc, &members, "two words", "").unwrap_err();
    assert!(error.contains("without spaces"), "{error}");
}

#[test]
fn a_wavy_double_bond_is_refused_rather_than_dropped() {
    let mut doc = Document::default();
    let outside = doc.add_atom("C", Point::new(0., 0.));
    let first = doc.add_atom("C", Point::new(42., 0.));
    let second = doc.add_atom("C", Point::new(63., 36.));
    let last = doc.add_atom("C", Point::new(105., 36.));
    doc.add_bond(outside, first, 1, "plain");
    doc.add_bond(first, second, 2, "wavy");
    doc.add_bond(second, last, 1, "plain");
    let error = from_selection(&doc, &[first, second, last], "Prop", "").unwrap_err();
    assert!(error.contains("Stereo"), "{error}");
    // A wavy single bond to a tert-butyl methyl is refused too.
    let (mut doc, _, members) = pivalate();
    let bond = doc
        .bonds
        .iter_mut()
        .find(|b| (b.a, b.b) == (members[3], members[4]))
        .unwrap();
    bond.display = "wavy".into();
    let error = from_selection(&doc, &members, "OPv", "").unwrap_err();
    assert!(error.contains("Stereo"), "{error}");
}

#[test]
fn saving_adds_or_replaces_by_label_and_keeps_other_entries() {
    let entry = |label: &str, smiles: &str| Entry {
        label: label.into(),
        reverse: String::new(),
        smiles: smiles.into(),
        formula: None,
        category: "user".into(),
        names: Vec::new(),
        aliases: Vec::new(),
        contract: true,
    };
    let first = upsert(None, entry("A1", "*C")).unwrap();
    let second = upsert(Some(&first), entry("B1", "*CC")).unwrap();
    let replaced = upsert(Some(&second), entry("A1", "*CCC")).unwrap();
    let (presets, errors) = parse(&replaced, true);
    assert!(errors.is_empty(), "{errors:?}");
    let pairs: Vec<_> = presets
        .iter()
        .map(|p| (p.label.as_str(), p.smiles.as_str()))
        .collect();
    assert_eq!(pairs, [("A1", "*CCC"), ("B1", "*CC")]);
    // One group per line, like the bundled catalog.
    assert_eq!(replaced.lines().count(), 7);
    assert!(upsert(Some("{ not json"), entry("C1", "*C")).is_err());
    // An entry skipped at startup is kept and does not block saving.
    let skipped = r#"{"format": 1, "groups": [{"label": "Typo", "smile": "*C"}]}"#;
    let saved = upsert(Some(skipped), entry("D1", "*CC")).unwrap();
    assert!(
        saved.contains(r#"{"label":"Typo","smile":"*C"}"#),
        "{saved}"
    );
    let (presets, errors) = parse(&saved, true);
    assert_eq!(presets.len(), 1);
    assert_eq!(errors.len(), 1, "{errors:?}");
}
