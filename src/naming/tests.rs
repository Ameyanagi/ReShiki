use super::*;

#[test]
fn graph_identity_preserves_stereo_isotopes_charges_and_tautomers() -> Result<(), String> {
    verify_identity("CCO", "OCC")?;
    verify_identity("[CH3:1][CH2:2][OH:3]", "CCO")?;
    for (expected, different) in [
        ("C[C@H](O)C(=O)O", "C[C@@H](O)C(=O)O"),
        ("C[C@H](O)C(=O)O", "CC(O)C(=O)O"),
        ("C/C=C/C", "C/C=C\\C"),
        ("C/C=C/C", "CC=CC"),
        ("[13CH3]CO", "CCO"),
        ("CC(=O)[O-]", "CC(=O)O"),
        ("CC=O", "C=CO"),
    ] {
        assert!(
            verify_identity(expected, different).is_err(),
            "{expected} became {different}"
        );
    }
    for text in [
        "*C",
        "[CH3]",
        "C~C",
        "CCO>>CC=O",
        "CCO |$;;OH$|",
        "FC=C(F)F |w:1|",
        "invalid",
    ] {
        assert!(
            canonical_smiles(text).is_err(),
            "Accepted unsupported input {text}"
        );
    }
    Ok(())
}

#[test]
fn oversized_smiles_are_rejected_before_graph_preparation() {
    assert!(bound_atom_tokens(&"C".repeat(513)).is_err());
    assert!(bound_atom_tokens(&"[13CH3]".repeat(513)).is_err());
    assert!(bound_atom_tokens(&"Cl".repeat(512)).is_ok());
    assert!(bound_atom_tokens(&"Cc".repeat(257)).is_err());
    assert!(bound_atom_tokens("C[CH3").is_err());
}

#[test]
fn unsupported_or_ignored_winding_is_rejected_before_cleanup() {
    for text in [
        "FC=[C@AL1]=CF",
        "F[Pt@SP1](Cl)(Br)I",
        "F[P@TB1](Cl)(Br)(I)N",
        "F[Co@OH1](Cl)(Br)(I)(N)O",
        "C[C@H](O)C",
        "[C@H3]CO",
    ] {
        assert!(canonical_smiles(text).is_err(), "Lost winding from {text}");
    }
}

#[test]
fn selections_cannot_silently_truncate_a_molecule() -> Result<(), String> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", crate::document::Point::new(0., 0.));
    let b = doc.add_atom("C", crate::document::Point::new(28., 0.));
    let o = doc.add_atom("O", crate::document::Point::new(42., 24.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, o, 1, "plain");
    assert!(selected_identity(&doc, &[a, b]).is_err());
    assert_eq!(selected_identity(&doc, &[a, b, o])?.smiles, "CCO");
    doc.contract(&[a, b], "Et", "")?;
    let anchor = doc.abbreviations[0].anchor;
    assert_eq!(
        selected_identity(&doc, &[anchor, o])?.smiles,
        "CCO",
        "A selected abbreviation retains its complete hidden graph"
    );
    assert_eq!(doc.atoms.len(), 3);
    let water = doc.add_atom("O", crate::document::Point::new(100., 0.));
    assert!(selected_identity(&doc, &[a, b, o, water]).is_err());
    doc.bonds[0].display = "wavy".into();
    assert!(selected_identity(&doc, &[a, b, o]).is_err());
    Ok(())
}
