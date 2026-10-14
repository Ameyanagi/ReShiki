use super::*;

#[test]
fn typed_error_display_preserves_message_bytes() {
    for message in [
        "",
        "Enter a chemical name of at most 2048 bytes without control characters",
        "Local parser worker failed: fixture failure",
        "Local name parsing was cancelled",
        "Local name generation was cancelled",
        "OPSIN details · complete stereochemistry",
    ] {
        for error in [
            Error::Rejected(message.into()),
            Error::Worker(message.into()),
            Error::Cancelled(message.into()),
        ] {
            assert_eq!(error.to_string().as_bytes(), message.as_bytes());
        }
    }
}

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

// Keep these cases, documents and expected bytes in sync with the identity
// characterization table in crates/model/src/molecule_names/tests.rs.
fn identity_case_document(case: &str) -> Document {
    let mut doc = Document::from_json(include_bytes!(
        "../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap();
    match case {
        "plain" => {}
        "isotope" => {
            let id = doc
                .atoms
                .iter()
                .find(|atom| {
                    atom.element == "C"
                        && doc
                            .bonds
                            .iter()
                            .filter(|bond| bond.a == atom.id || bond.b == atom.id)
                            .count()
                            == 1
                })
                .unwrap()
                .id;
            doc.atom_mut(id).unwrap().isotope = 13;
        }
        "charge" => {
            doc.atoms[2].charge = -1;
            doc.atoms[2].no_implicit = true;
        }
        "mapped" => {
            for (index, atom) in doc.atoms.iter_mut().enumerate() {
                atom.map_num = index as u32 + 1;
            }
        }
        "R" | "S" => {
            doc = Document::from_json(include_bytes!(
                "../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
            ))
            .unwrap();
            if case == "S" {
                let stereo = doc
                    .atoms
                    .iter_mut()
                    .find_map(|atom| atom.stereo.as_mut())
                    .unwrap();
                stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
            }
        }
        "E" | "Z" | "unknown" => {
            doc = Document::default();
            let ids = (0..4)
                .map(|index| {
                    doc.add_atom(
                        "C",
                        crate::document::Point::new(
                            index as f32 * 28.,
                            match index {
                                0 => 20.,
                                3 if case == "E" => -20.,
                                3 => 20.,
                                _ => 0.,
                            },
                        ),
                    )
                })
                .collect::<Vec<_>>();
            doc.add_bond(ids[0], ids[1], 1, "plain");
            doc.add_bond(ids[1], ids[2], 2, "plain");
            doc.add_bond(ids[2], ids[3], 1, "plain");
            doc.bonds[1].stereo_authoritative = true;
            doc.bonds[1].stereo = Some(
                match case {
                    "E" => "e",
                    "Z" => "z",
                    _ => "any",
                }
                .into(),
            );
            if case != "unknown" {
                doc.bonds[1].stereo_atoms = vec![ids[0], ids[3]];
            }
        }
        "allene" => {
            // Native AtomStereo can retain only tetrahedral winding. An allene
            // tag must fail validation rather than become an unspecified graph.
            doc = Document::default();
            let ids = ["F", "C", "C", "C", "F"]
                .into_iter()
                .enumerate()
                .map(|(index, element)| {
                    doc.add_atom(element, crate::document::Point::new(index as f32 * 28., 0.))
                })
                .collect::<Vec<_>>();
            for (index, pair) in ids.windows(2).enumerate() {
                doc.add_bond(
                    pair[0],
                    pair[1],
                    if index == 1 || index == 2 { 2 } else { 1 },
                    "plain",
                );
            }
            doc.atoms[2].stereo = Some(crate::document::AtomStereo {
                winding: "AL1".into(),
                neighbors: vec![ids[1], ids[3]],
            });
        }
        "wavy" => doc.bonds[0].display = "wavy".into(),
        "radical" => {
            let id = doc
                .atoms
                .iter()
                .find(|atom| {
                    atom.element == "C"
                        && doc
                            .bonds
                            .iter()
                            .filter(|bond| bond.a == atom.id || bond.b == atom.id)
                            .count()
                            == 1
                })
                .unwrap()
                .id;
            let atom = doc.atom_mut(id).unwrap();
            atom.radical_electrons = 1;
            atom.explicit_h = 2;
            atom.no_implicit = true;
        }
        "query" => doc.atoms[0].element = "*".into(),
        _ => panic!("Unknown identity characterization case: {case}"),
    }
    doc
}

#[test]
fn identity_characterization_table() {
    const UNSUPPORTED: &str =
        "Naming does not support query atoms, radicals, exotic bonds or relative stereo groups";
    const UNKNOWN: &str = "Resolve unknown stereochemistry before generating a name; wavy stereo cannot be sent as unspecified stereo";
    for (case, expected) in [
        ("plain", Ok("CCO")),
        ("isotope", Ok("[13CH3]CO")),
        ("charge", Ok("CC[O-]")),
        ("E", Ok("C/C=C/C")),
        ("Z", Ok("C/C=C\\C")),
        ("R", Ok("C[C@@H](O)C(=O)O")),
        ("S", Ok("C[C@H](O)C(=O)O")),
        (
            "allene",
            Err("Invalid drawing: Invalid stereocenter neighbor mapping"),
        ),
        (
            "wavy",
            Err("Resolve wavy/unknown stereochemistry before generating a name"),
        ),
        ("radical", Err(UNSUPPORTED)),
        ("query", Err(UNSUPPORTED)),
        ("mapped", Ok("CCO")),
        ("unknown", Err(UNKNOWN)),
    ] {
        let doc = identity_case_document(case);
        let actual = document_identity(&doc).map(|identity| identity.smiles);
        assert_eq!(
            actual.as_deref().map_err(String::as_str),
            expected,
            "{case}"
        );
        if let Ok(molecule) = chemistry::document::prepare(&doc) {
            // Exercise the actual state path as well as document pre-checks.
            // Wavy bonds are rejected at the document boundary even when
            // preparation finds no stereocenter to mark unknown.
            let state_expected = if case == "wavy" { Ok("CCO") } else { expected };
            assert_eq!(
                canonical_state(&molecule.state)
                    .as_deref()
                    .map_err(String::as_str),
                state_expected,
                "state: {case}"
            );
        }
    }
    // The raw SMILES route sees the allene tag before import cleanup.
    assert_eq!(
        canonical_smiles("FC=[C@AL1]=CF").unwrap_err(),
        "Naming does not support allene or coordination stereo classes; no plain graph was substituted"
    );
}

#[test]
fn canonical_state_preserves_rejection_order_and_writer_errors() {
    const UNSUPPORTED: &str =
        "Naming does not support query atoms, radicals, exotic bonds or relative stereo groups";
    const UNKNOWN: &str = "Resolve unknown stereochemistry before generating a name; wavy stereo cannot be sent as unspecified stereo";
    const NONCLASSICAL: &str = "Naming supports tetrahedral and ordinary alkene stereochemistry only; other stereo classes cannot be discarded";
    let doc = identity_case_document("plain");
    let base = chemistry::document::prepare(&doc).unwrap().state;
    let mut state = base.clone();
    state.graph.atoms[0].radical_electrons = 1;
    state.properties.atoms[0].unknown = true;
    state.metadata.atoms[0].chiral_tag = 3;
    assert_eq!(canonical_state(&state).unwrap_err(), UNSUPPORTED);
    state.graph.atoms[0].radical_electrons = 0;
    assert_eq!(canonical_state(&state).unwrap_err(), UNKNOWN);
    state.properties.atoms[0].unknown = false;
    assert_eq!(canonical_state(&state).unwrap_err(), NONCLASSICAL);
    state.metadata.atoms[0].chiral_tag = 0;
    state.metadata.bonds[0].stereo = 6;
    assert_eq!(canonical_state(&state).unwrap_err(), NONCLASSICAL);
    state.metadata.bonds[0].unknown_stereo = true;
    assert_eq!(canonical_state(&state).unwrap_err(), UNKNOWN);

    let mut state = base;
    state.metadata.atoms.clear();
    let writer_error = chemistry::smiles::write::write(
        &state,
        chemistry::smiles::write::Options {
            ignore_maps: true,
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(canonical_state(&state).unwrap_err(), writer_error);
}
