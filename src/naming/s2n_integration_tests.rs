// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: ReShiki | src/naming/openclatura/mod.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: ReShiki | src/naming/openclatura/radical_ions.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: ReShiki | src/naming/openclatura/phane/tests.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: ReShiki | src/naming/s2n_integration_tests.rs | Phase A step 4 (local extraction)
// Source: openclatura | tests/integration/test_describer.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: OPSIN | opsin-inchi/src/test/resources/uk/ac/cam/ch/wwmm/opsin/phanes.txt | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Copyright (c) 2026 lamalab-org
// Copyright 2017 Daniel Lowe
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Keep document and InChI integration checks in ReShiki after extracting the naming engine.
// Local contributions: MIT OR Apache-2.0; see NOTICE and
// PROVENANCE-nazuchem.toml at the nazuchem revision pinned in Cargo.toml:
// https://github.com/Ameyanagi/nazuchem/blob/main/PROVENANCE-nazuchem.toml.

//! Application integrations for the extracted structure-to-name library.

use super::verify_identity;
use nazuchem_s2n::name_smiles;

// Provenance: fn fixture_smiles | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::fixture_smiles
fn fixture_smiles(name: &str) -> Result<String, String> {
    let inchi = include_str!("../../tests/fixtures/naming/opsin/phanes.txt")
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .find(|(fixture, _)| *fixture == name)
        .ok_or("Missing upstream phane fixture")?
        .1;
    let imported = reshiki_chemistry::inchi::generator::read_in_process(inchi)
        .map_err(|error| error.to_string())?;
    let state = imported.state.ok_or(imported.message)?;
    reshiki_chemistry::smiles::write::write(&state, Default::default())
        .map(|output| output.text)
        .map_err(|error| error.to_string())
}

// Provenance: fn phane_name | original
// Source: ReShiki | src/naming/s2n_integration_tests.rs::phane_name
fn phane_name(smiles: &str) -> Result<Option<String>, String> {
    let named = name_smiles(smiles, &Default::default())?;
    if named.text.contains("phane") {
        return Ok(Some(named.text));
    }
    Ok(named.alternatives.into_iter().find_map(|alternative| {
        alternative
            .name
            .contains("phane")
            .then_some(alternative.name)
    }))
}

// Provenance: fn round_trip | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::round_trip
fn round_trip(smiles: &str, name: &str) -> Result<(), String> {
    let parsed = nazuchem_n2s::Parser::new()
        .map_err(|e| e.to_string())?
        .parse(name, &nazuchem_n2s::ParseOptions::strict());
    assert_eq!(
        parsed.status,
        nazuchem_n2s::Status::Success,
        "{name}: {}",
        parsed.message
    );
    assert!(parsed.warnings.is_empty(), "{name}: {:?}", parsed.warnings);
    let actual = parsed
        .structure
        .ok_or("Missing phane graph")?
        .semantic_cxsmiles()
        .map_err(|e| e.to_string())?;
    verify_identity(smiles, &actual)
}

#[test]
// Provenance: fn drawn_localized_benzene_radical_cation_recovers_benzene | adapted
// Source: ReShiki | src/naming/openclatura/radical_ions.rs::tests::drawn_localized_benzene_radical_cation_recovers_benzene
// Source: openclatura | tests/integration/test_describer.py::test_describe_handles_benzene_without_functional_group
// Test provenance: upstream inputs/expected names with local assertions and extensions;
// exact matches are conservatively adapted; independent ancestry is not established.
fn drawn_localized_benzene_radical_cation_recovers_benzene() {
    use crate::document::{Document, Point};
    let mut document = Document::default();
    let atoms: Vec<_> = (0..6)
        .map(|index| {
            let angle = std::f32::consts::TAU * index as f32 / 6.0;
            document.add_atom("C", Point::new(angle.cos() * 28.0, angle.sin() * 28.0))
        })
        .collect();
    // Cation and radical on adjacent atoms, their shared pi bond broken.
    for (index, order) in [1, 1, 2, 1, 2, 1].into_iter().enumerate() {
        document.add_bond(atoms[index], atoms[(index + 1) % 6], order, "plain");
    }
    document.atoms[0].charge = 1;
    document.atoms[1].radical_electrons = 1;
    let identity = crate::naming::document_identity(&document).unwrap();
    let (neutral, suffix) =
        nazuchem_s2n::recover_radical_ion(&identity.smiles, &Default::default())
            .unwrap()
            .unwrap();
    assert_eq!(
        neutral,
        crate::naming::canonical_smiles("c1ccccc1").unwrap()
    );
    assert_eq!(suffix, "radical cation");
    // The same-atom form over-fills a Kekulé carbon and is refused by
    // preparation, before naming.
    document.bonds[0].order = 2;
    document.atoms[1].radical_electrons = 0;
    document.atoms[0].radical_electrons = 1;
    assert!(crate::naming::document_identity(&document).is_err());
}

#[test]
// Provenance: fn pheno_fusion_components_preserve_inchi | original
// Source: ReShiki | src/naming/openclatura/mod.rs::tests::pheno_fusion_components_choose_preferred_parents_and_round_trip
fn pheno_fusion_components_preserve_inchi() {
    let inchi = |smiles: &str| {
        use reshiki_chemistry::inchi::{generator, kernel};
        let state = reshiki_chemistry::smiles::prepare(smiles).unwrap().state;
        let input = kernel::Molecule::prepare(&state, None).unwrap();
        let output = generator::generate_in_process(&input).unwrap();
        assert!(output.status.is_success(), "{smiles}: {}", output.message);
        output.inchi
    };
    for smiles in [
        "c1ccc2c(c1)Nc1cc3cnc4ccccc4c3cc1O2",
        "C1=CC=C2C(=C1)NC1=CC3=CN=C4C=CC=CC4=C3C=C1O2",
        "c1ccc2c(c1)Nc1cc3cnc4ccccc4c3cc1S2",
        "c1ccc2cc3c(cc2c1)Nc1cc2ccccc2cc1O3",
        "c1ccc2c(c1)Sc1ccc3ccccc3c1S2",
        "c1ccc2c(c1)Oc1ccc3ccccc3c1S2",
        "c1ccc2c(c1)Nc1ccc3ccccc3c1S2",
        "c1ccc2c(c1)Nc1c(ccc3ccccc13)S2",
        "c1ccc2c(c1)Nc1ccc3ccccc3c1O2",
        "c1ccc2c(c1)Oc1ccc3ccccc3c1N2",
        "c1ccc2c(c1)Nc1cc3ccccc3cc1S2",
        "c1ccc2cc3c(cc2c1)Sc1ccccc1S3",
        "c1cc2Nc3ccccc3Oc2c2ncccc12",
        "c1ccc2c(c1)Oc1ccc3ncccc3c1N2",
        "c1ccc2nc3cc4Oc5ccccc5Nc4cc3nc2c1",
        "c1ccc2c(c1)Oc1cc3Sc4ccccc4Sc3cc1S2",
        "c1ccc2c(c1)Nc1cc3Sc4ccccc4Nc3cc1O2",
    ] {
        let name = name_smiles(smiles, &Default::default()).unwrap();
        let request = crate::naming::local::parse_request(&name.text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&name.text, &reply).unwrap();
        assert_eq!(
            inchi(smiles),
            inchi(&parsed.smiles),
            "{smiles}: {}",
            name.text
        );
    }
}

#[test]
// Provenance: fn names_independent_upstream_inchi_graphs | mixed
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::names_independent_upstream_inchi_graphs
fn names_independent_upstream_inchi_graphs() -> Result<(), String> {
    let seven = "1(1,3),4(1,4)-dibenzenacycloheptaphane";
    verify_identity("C1CCc2cccc(c2)CCc2ccc1cc2", &fixture_smiles(seven)?)?;
    for expected in [
        seven,
        "1,3,5,7(1,3)-tetrabenzenacyclooctaphane",
        "5-oxa-2-thia-1,7(1,3)-dibenzenacyclododecaphane",
        "2,5-diaza-1,4(1,4)-dibenzenacyclohexaphane",
    ] {
        let smiles = fixture_smiles(expected)?;
        assert_eq!(phane_name(&smiles)?.as_deref(), Some(expected));
        round_trip(&smiles, expected)?;
    }
    Ok(())
}
