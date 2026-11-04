use super::*;

#[tokio::test]
async fn whole_drawing_and_all_ion_selection_generate_the_same_salt() {
    let mut doc = Document::default();
    let sodium = doc.add_atom("Na", crate::document::Point::default());
    let chloride = doc.add_atom("Cl", crate::document::Point::new(80., 0.));
    doc.atom_mut(sodium).unwrap().charge = 1;
    doc.atom_mut(chloride).unwrap().charge = -1;
    let whole = document_identity(&doc).unwrap();
    let selected = selected_identity(&doc, &[sodium, chloride]).unwrap();
    assert_eq!(whole.smiles, selected.smiles);
    let record = generate_name(&selected, Cancel::default()).await.unwrap();
    assert_eq!(record.title, "sodium chloride");
    assert_eq!(record.name_status, Some(NameStatus::Preferred));
    doc.atom_mut(chloride).unwrap().charge = 0;
    assert!(
        document_identity(&doc)
            .unwrap_err()
            .contains("charge-balanced salt")
    );
    doc.atom_mut(chloride).unwrap().charge = 1;
    assert!(document_identity(&doc).is_err());
}

#[test]
fn parent_locants_follow_canonical_atom_order_and_require_the_same_graph() {
    let mut document = Document::default();
    let terminal = document.add_atom("C", crate::document::Point::new(0., 0.));
    let middle = document.add_atom("C", crate::document::Point::new(28., 0.));
    let oxygen = document.add_atom("O", crate::document::Point::new(42., 24.));
    document.add_bond(terminal, middle, 1, "plain");
    document.add_bond(middle, oxygen, 1, "plain");
    document.atoms.rotate_left(1);
    let locants = Locants {
        smiles: "OCC".into(),
        atoms: vec![None, Some("1".into()), Some("2".into())],
    };
    assert_eq!(
        locants.for_document(&document),
        HashMap::from([(terminal, "2".into()), (middle, "1".into())])
    );
    let mut invalid = locants.clone();
    invalid.atoms.pop();
    assert!(invalid.for_document(&document).is_empty());
    document.atom_mut(oxygen).unwrap().element = "N".into();
    assert!(locants.for_document(&document).is_empty());
}

#[tokio::test]
async fn parent_locants_ignore_drawn_fusion_hydrogens_and_stereo() {
    // The default depiction draws the fusion hydrogens of cis-decalin as atoms.
    let response = crate::engine::LocalEngine::in_process()
        .request(crate::engine::Request::import_smiles(
            "C1CC[C@H]2CCCC[C@@H]2C1",
        ))
        .await
        .unwrap();
    let document = response.document.unwrap();
    assert!(document.atoms.iter().any(|atom| atom.element == "H"));
    let locants = Locants {
        smiles: "C1CCC2CCCCC2C1".into(),
        atoms: ["2", "3", "4", "4a", "5", "6", "7", "8", "8a", "1"]
            .map(|locant| Some(locant.into()))
            .into(),
    };
    let mapped = locants.for_document(&document);
    assert_eq!(mapped.len(), 10);
    assert!(
        mapped
            .keys()
            .all(|id| document.atom(*id).unwrap().element == "C")
    );
}

#[test]
fn interior_locants_use_unicode_superscript_digits() {
    assert_eq!(locant_text("3a^12"), "3a¹²");
    assert_eq!(locant_text("4a"), "4a");
    assert_eq!(locant_text("3a¹"), "3a¹");
}

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
fn specified_cumulene_ez_is_rejected_before_cleanup() {
    for text in [
        "C/C=C=C=C/C",
        r"C/C=C=C=C\C",
        "C/C=C=C=C=C=C/C",
        r"C/C=C=C=C=C=C\C",
        "[2H]/C=C=C=C/C",
        r"[2H]/C=C=C=C\C",
        "[3H]/C=C=C=C/C",
        "[2H]/C=C=C=C=C=C/C",
        "[2H]/C([H])=C=C=C/C",
        "F/C=C=C=C/Cl",
        "C/C(Cl)=C=C=C(/C)F",
        "C(=C=C=C/C)/C",
        r"C/C=C/C(=C=C=C/F)/C=C\C",
        r"C/C=C/C(=C=C=C\F)/C=C\C",
        "[H]/N=C=C=C/C",
        r"[H]/N=C=C=C\C",
        "C/N=C=C=C/C",
        "C/C=C=C=C/C.CC",
        "C/1=C=C=C/CCCC1",
        // An aromatic terminal bond carries the direction.
        "C/C=C=C=c1/cccc[nH]1",
        "C/C=C=C=c1cccc[nH]/1",
    ] {
        assert_eq!(
            canonical_smiles(text).unwrap_err(),
            "Naming does not support cumulene E/Z configuration yet; no less specific name was substituted",
            "{text}"
        );
    }
    for text in [
        "CC=C=C=CC",
        "C/C=C=C=CC",
        "CC=C=C=C/C",
        "CC=C=CC",
        "C/C=C=C/C",
        "C/C=C=C=C=C/C",
        "C/C=C/C",
        r"C/C=C\C",
        "C/C=C/C=C/C",
        // An end with two identical ligands is not stereogenic (P-91.2.1.2.1).
        "C/C(C)=C=C=C/C",
        "C/C(C)=C=C=C(/C)C",
        "[2H]/C([2H])=C=C=C/C",
        "[CH3:1]/C([CH3:2])=C=C=C/C",
        "C/C=C/C(/C=C/C)=C=C=C/C=C/C",
    ] {
        assert!(canonical_smiles(text).is_ok(), "Rejected {text}");
    }
    // An explicit 1H end is the same ligand as H, so no cumulene E/Z is lost.
    for text in ["[1H]/C=C=C=C/C", r"[1H]/C=C=C=C\C", "[1H]/C([H])=C=C=C/C"] {
        assert!(
            !canonical_smiles(text).is_err_and(|error| error.contains("cumulene")),
            "{text}"
        );
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
    verify_identity("CCO.O", &structure_smiles(&doc)?)?;
    assert!(document_identity(&doc).is_err());
    doc.bonds[0].display = "wavy".into();
    assert_eq!(selected_identity(&doc, &[a, b, o])?.smiles, "CCO");
    Ok(())
}

#[test]
fn naming_notice_lists_only_undefined_stereogenic_units() -> Result<(), String> {
    let mut doc = Document::default();
    let ids = ["C", "C", "O", "C", "C", "C"]
        .into_iter()
        .enumerate()
        .map(|(index, element)| {
            doc.add_atom(element, crate::document::Point::new(index as f32 * 28., 0.))
        })
        .collect::<Vec<_>>();
    for (a, b, order) in [(0, 1, 1), (1, 2, 1), (1, 3, 1), (3, 4, 2), (4, 5, 1)] {
        doc.add_bond(ids[a], ids[b], order, "plain");
    }
    doc.bonds[3].stereo_authoritative = true;
    assert_eq!(
        document_identity(&doc)?.warnings,
        ["C2 and the C4=C5 bond have no configuration; the name leaves them unspecified"]
    );
    doc.atoms[1].display.number = Some(crate::atom_labels::Number {
        text: "alpha".into(),
        offset: None,
        style: crate::atom_labels::number_style(),
    });
    assert_eq!(
        document_identity(&doc)?.warnings,
        ["alpha and the C4=C5 bond have no configuration; the name leaves them unspecified"]
    );
    for case in ["plain", "R", "S", "E", "Z"] {
        assert!(
            document_identity(&identity_case_document(case))?
                .warnings
                .is_empty(),
            "A fully specified {case} molecule should have no missing-stereo notice"
        );
    }
    Ok(())
}

#[test]
fn naming_notice_uses_document_atom_order_for_a_selected_molecule() -> Result<(), String> {
    let mut doc = Document::default();
    doc.add_atom("O", crate::document::Point::new(-50., 0.));
    let ids = ["C", "C", "O", "C"]
        .into_iter()
        .enumerate()
        .map(|(index, element)| {
            doc.add_atom(element, crate::document::Point::new(index as f32 * 28., 0.))
        })
        .collect::<Vec<_>>();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], 1, "plain");
    doc.add_bond(ids[1], ids[3], 1, "plain");
    // The right branch has a different isotope, so this is a possible centre.
    doc.atom_mut(ids[3]).unwrap().isotope = 13;
    assert_eq!(
        selected_identity(&doc, &ids)?.warnings,
        ["C3 has no configuration; the name leaves it unspecified"]
    );
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
        ("wavy", Ok("CCO")),
        ("radical", Ok("[CH2]CO")),
        ("query", Err(UNSUPPORTED)),
        ("mapped", Ok("CCO")),
        ("unknown", Ok("CC=CC")),
    ] {
        let doc = identity_case_document(case);
        let actual = document_identity(&doc).map(|identity| identity.smiles);
        assert_eq!(
            actual.as_deref().map_err(String::as_str),
            expected,
            "{case}"
        );
        if let Ok(molecule) = chemistry::document::prepare(&doc) {
            // The state and document paths omit exactly the same unknown units.
            let state_expected = expected;
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
    const NONCLASSICAL: &str = "Naming supports tetrahedral, ordinary alkene and biaryl axis stereochemistry; other stereo classes cannot be discarded";
    let doc = identity_case_document("plain");
    let base = chemistry::document::prepare(&doc).unwrap().state;
    let mut state = base.clone();
    state.graph.atoms[0].radical_electrons = 3;
    state.properties.atoms[0].unknown = true;
    state.metadata.atoms[0].chiral_tag = 3;
    assert_eq!(canonical_state(&state).unwrap_err(), UNSUPPORTED);
    state.graph.atoms[0].radical_electrons = 0;
    assert_eq!(canonical_state(&state).unwrap_err(), NONCLASSICAL);
    state.properties.atoms[0].unknown = false;
    assert_eq!(canonical_state(&state).unwrap_err(), NONCLASSICAL);
    state.metadata.atoms[0].chiral_tag = 0;
    state.metadata.bonds[0].stereo = 6;
    assert_eq!(canonical_state(&state).unwrap_err(), NONCLASSICAL);
    state.metadata.bonds[0].unknown_stereo = true;
    assert_eq!(canonical_state(&state).unwrap_err(), NONCLASSICAL);

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

fn known_and_wavy_stereo_document() -> Document {
    let input = chemistry::smiles::read("C[C@@H](O)[C@H](Cl)C").unwrap();
    let state = input.prepared.state;
    let positions = chemistry::depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )
    .unwrap()
    .positions;
    let molecule = chemistry::document::Molecule {
        rdkit_version: chemistry::RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        state,
        positions,
    };
    let drawing = chemistry::document::for_generated(
        &molecule,
        &input.prepared.dummy_labels,
        Default::default(),
    )
    .unwrap();
    let labels = drawing.labels().unwrap();
    let mut doc = drawing.finish(labels).unwrap();
    for bond in &mut doc.bonds {
        if bond.a == 2 && bond.order == 1 {
            bond.display = "plain".into();
        }
        if [bond.a, bond.b].contains(&2) && [bond.a, bond.b].contains(&3) {
            bond.a = 2;
            bond.b = 3;
            bond.display = "wavy".into();
        }
    }
    doc
}

pub(super) fn undefined_and_known_stereo_document() -> Document {
    let mut doc = known_and_wavy_stereo_document();
    doc.atom_mut(2).unwrap().stereo = None;
    for bond in &mut doc.bonds {
        if bond.display == "wavy" {
            bond.display = "plain".into();
        }
    }
    doc
}

pub(super) fn undefined_alkene_document() -> Document {
    let mut doc = identity_case_document("unknown");
    doc.bonds[1].stereo = None;
    doc
}

pub(super) fn wavy_and_undefined_stereo_document() -> Document {
    let mut doc = known_and_wavy_stereo_document();
    doc.atom_mut(4).unwrap().stereo = None;
    for bond in &mut doc.bonds {
        if bond.a == 4 && bond.order == 1 {
            bond.display = "plain".into();
        }
    }
    doc
}

#[test]
fn unknown_document_identity_keeps_known_stereo_and_only_internal_maps() {
    let mut doc = known_and_wavy_stereo_document();
    for atom in &mut doc.atoms {
        atom.map_num = 42;
    }
    let identity = document_identity(&doc).unwrap();
    let expected = canonical_smiles("CC(O)[C@H](Cl)C").unwrap();
    assert_eq!(identity.smiles, expected);
    assert!(!identity.write_unknown_stereo);
    assert_eq!(identity.unknown_stereo_smiles, None);
    let marked = identity.naming_smiles.unwrap();
    assert_eq!(canonical_smiles(&marked).unwrap(), expected);
    let raw = chemistry::smiles::parse(&marked).unwrap();
    let maps: Vec<_> = raw
        .metadata
        .atoms
        .iter()
        .filter_map(|atom| (atom.map_number != 0).then_some(atom.map_number))
        .collect();
    assert_eq!(maps, [900_001]);
    let name = openclatura::name_smiles(&marked).unwrap();
    assert_eq!(name.text, "(3R)-3-chlorobutan-2-ol");
    assert_eq!(name.omitted_stereo, ["2"]);
    assert_eq!(name.alternatives[0].name, "(2Ξ,3R)-3-chlorobutan-2-ol");
}

#[test]
fn undefined_document_units_only_enter_the_explicit_unknown_input() {
    for (doc, expected, explicit) in [
        (
            undefined_and_known_stereo_document(),
            "CC(O)[C@H](Cl)C",
            "(2Ξ,3R)-3-chlorobutan-2-ol",
        ),
        (undefined_alkene_document(), "CC=CC", "(2Ξ)-but-2-ene"),
    ] {
        let mut identity = document_identity(&doc).unwrap();
        let expected = canonical_smiles(expected).unwrap();
        assert_eq!(identity.smiles, expected);
        assert_eq!(identity.naming_smiles, None);
        assert_eq!(identity.generation_smiles(&expected), expected);
        assert!(
            openclatura::name_smiles(identity.generation_smiles(&expected))
                .unwrap()
                .alternatives
                .iter()
                .all(|name| !name.name.contains(['Ξ', 'ξ']))
        );
        identity.write_unknown_stereo = true;
        let marked = identity.generation_smiles(&expected);
        assert_eq!(canonical_smiles(marked).unwrap(), expected);
        assert_eq!(
            openclatura::name_smiles(marked).unwrap().alternatives[0].name,
            explicit
        );
    }
}

#[tokio::test]
async fn imported_alkenes_keep_unspecified_units_in_names_and_checks() {
    for (smiles, expected, explicit, undefined) in [
        ("CC=CC", "but-2-ene", "(2Ξ)-but-2-ene", (2, 3)),
        (
            "C/C=C/C=CC",
            "(4E)-hexa-2,4-diene",
            "(2E,4Ξ)-hexa-2,4-diene",
            (4, 5),
        ),
    ] {
        let response = crate::engine::LocalEngine::in_process()
            .request(crate::engine::Request::import_smiles(smiles))
            .await
            .unwrap();
        let document = response.document.unwrap();
        assert_eq!(
            chemistry::stereo_lint::undefined(&document).bonds,
            [undefined],
            "{smiles}"
        );
        let mut identity = document_identity(&document).unwrap();
        assert_eq!(
            generate_name(&identity, Cancel::default())
                .await
                .unwrap()
                .title,
            expected
        );
        identity.write_unknown_stereo = true;
        assert_eq!(
            generate_name(&identity, Cancel::default())
                .await
                .unwrap()
                .title,
            explicit
        );
    }
}

#[tokio::test]
async fn inchi_import_preserves_unspecified_and_defined_alkenes() {
    for (inchi, expected, unspecified) in [
        ("InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3", "but-2-ene", true),
        (
            "InChI=1S/C4H8/c1-3-4-2/h3-4H,1-2H3/b4-3+",
            "(2E)-but-2-ene",
            false,
        ),
    ] {
        let response = crate::engine::LocalEngine::in_process()
            .request(crate::engine::Request::import("inchi", inchi))
            .await
            .unwrap();
        let document = response.document.unwrap();
        let bond = document.bonds.iter().find(|bond| bond.order == 2).unwrap();
        assert_eq!(
            bond.stereo_authoritative && bond.stereo.is_none(),
            unspecified
        );
        assert_eq!(
            chemistry::stereo_lint::undefined(&document)
                .bonds
                .is_empty(),
            !unspecified
        );
        let identity = document_identity(&document).unwrap();
        assert_eq!(
            generate_name(&identity, Cancel::default())
                .await
                .unwrap()
                .title,
            expected
        );
    }
}

#[tokio::test]
async fn mol_roundtrip_keeps_unspecified_and_z_alkene_names() {
    let engine = crate::engine::LocalEngine::in_process();
    for (smiles, expected) in [("CC=CC", "but-2-ene"), (r"C/C=C\C", "(2Z)-but-2-ene")] {
        let document = engine
            .request(crate::engine::Request::import_smiles(smiles))
            .await
            .unwrap()
            .document
            .unwrap();
        let mol = chemistry::molfile::write_document(&document).unwrap();
        let document = engine
            .request(crate::engine::Request::import("mol", &mol))
            .await
            .unwrap()
            .document
            .unwrap();
        let identity = document_identity(&document).unwrap();
        assert_eq!(
            generate_name(&identity, Cancel::default())
                .await
                .unwrap()
                .title,
            expected
        );
    }
}

#[tokio::test]
async fn plain_user_drawn_alkene_geometry_still_defines_configuration() {
    for (case, expected) in [("E", "(2E)-but-2-ene"), ("Z", "(2Z)-but-2-ene")] {
        let mut document = identity_case_document(case);
        let bond = &mut document.bonds[1];
        bond.stereo_authoritative = false;
        bond.stereo = None;
        bond.stereo_atoms.clear();
        let identity = document_identity(&document).unwrap();
        assert!(
            chemistry::stereo_lint::undefined(&document)
                .bonds
                .is_empty()
        );
        assert_eq!(
            generate_name(&identity, Cancel::default())
                .await
                .unwrap()
                .title,
            expected
        );
    }
}

#[test]
fn explicit_unknown_input_combines_wavy_and_undefined_units() {
    let doc = wavy_and_undefined_stereo_document();
    let mut identity = document_identity(&doc).unwrap();
    let expected = canonical_smiles("CC(O)C(Cl)C").unwrap();
    assert_eq!(identity.smiles, expected);
    let legacy = identity.generation_smiles(&expected).to_owned();
    let original = openclatura::name_smiles(&legacy).unwrap();
    assert_eq!(original.omitted_units, 1);
    assert_eq!(original.alternatives[0].name, "(2Ξ)-3-chlorobutan-2-ol");
    identity.write_unknown_stereo = true;
    let marked = identity.generation_smiles(&expected);
    assert_eq!(canonical_smiles(marked).unwrap(), expected);
    let explicit = openclatura::name_smiles(marked).unwrap();
    assert_eq!(explicit.omitted_units, 2);
    assert_eq!(explicit.alternatives[0].name, "(2Ξ,3Ξ)-3-chlorobutan-2-ol");
    identity.write_unknown_stereo = false;
    assert_eq!(identity.generation_smiles(&expected), legacy);
}

#[test]
fn crossed_and_either_alkenes_preserve_unknown_bond_markers() {
    for crossed in [false, true] {
        let mut doc = identity_case_document("unknown");
        if crossed {
            doc.bonds[1].stereo = None;
            doc.bonds[1].stereo_authoritative = false;
            doc.bonds[1].display = "wavy".into();
        }
        let identity = document_identity(&doc).unwrap();
        assert_eq!(identity.smiles, "CC=CC");
        let name = openclatura::name_smiles(identity.naming_smiles.as_deref().unwrap()).unwrap();
        assert_eq!(name.omitted_stereo, ["2=3"]);
        assert_eq!(name.alternatives[0].name, "(2Ξ)-but-2-ene");
    }
}

#[test]
fn unknown_bond_that_cannot_be_e_z_is_not_marked() {
    // 2-methylbut-2-ene drawn with an unknown (either) double bond.
    let mut doc = identity_case_document("unknown");
    let c2 = doc.bonds[1].a;
    let methyl = doc.add_atom("C", crate::document::Point::new(28., 28.));
    doc.add_bond(c2, methyl, 1, "plain");
    let identity = document_identity(&doc).unwrap();
    assert_eq!(identity.smiles, canonical_smiles("CC=C(C)C").unwrap());
    assert_eq!(identity.naming_smiles, None);
}
