// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: Nazuchem/ReShiki | crates/model/src/molecule_names/identity_parity.rs | Phase A step 3 working tree (uncommitted)
// Source: ReShiki | crates/model/src/molecule_names.rs | 381ac4164154afecf04bb4a89d8eef32eeb52190
// Source: ReShiki | crates/model/src/molecule_names/axis_markers.rs | 2df788dc456793c687b934afd6d4b317e3c73ef8
// Source: ReShiki | crates/model/src/chemistry/document/atrop_tests.rs | feed38dc791ee59fb0b8c4b392a3f72c100a1be7
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs | 0d10886c93f6f7aa673f4179739143986d392a01
// Changes: new cross-kernel assertions reuse original ReShiki radical, axis and pseudoasymmetric cases.

//! Keep the app's chemistry copy and the standalone kernel identity in agreement.
use super::*;
use chemistry::stereo::perception::State;
use nazuchem_kernel::{budget::Budget, identity as kernel};

// Serialization is test-only: production naming uses kernel State directly.
fn kernel_state(state: &State) -> nazuchem_kernel::stereo::perception::State {
    serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap()
}

fn assert_identity_parity(state: &State) -> Result<String, IdentityRejection> {
    let expected = canonical_identity(state);
    assert_eq!(
        expected.clone().map_err(|error| format!("{error:?}")),
        kernel::canonical_identity(&kernel_state(state), &Budget::default())
            .map_err(|error| format!("{error:?}")),
    );
    expected
}

fn assert_decoding_parity(state: &State) -> State {
    let expected = apply_axis_markers(state).map(|state| serde_json::to_value(state).unwrap());
    let actual = kernel::apply_axis_markers(&kernel_state(state), &Budget::default())
        .map(|state| serde_json::to_value(state).unwrap());
    assert_eq!(
        expected.clone().map_err(|error| format!("{error:?}")),
        actual.map_err(|error| format!("{error:?}")),
    );
    apply_axis_markers(state).unwrap()
}

#[test]
fn identities_match_for_maps_isotopes_charges_radicals_and_specified_stereo() {
    for input in [
        "CCO",
        "[CH3:17][CH2:42][OH:99]",
        "[13CH3]O[2H]",
        "[Na+].[Cl-]",
        "F[C@H](Cl)[C@@H](Br)I",
        "F/C=C/C=C\\Cl",
        "[CH3]",
        "C[CH]C",
        "[CH2]",
        "[CH2]CC[CH2]",
        "[CH]1CCCCC1",
        "[c]1ccccc1",
        "[NH2]",
        "C[NH]",
        "[OH]",
        "C[O]",
        "[SH]",
        "C[S]",
        "C[C]=O",
    ] {
        let state = chemistry::smiles::prepare(input).unwrap().state;
        let identity = assert_identity_parity(&state).unwrap();
        // Prepare independently with both chemistry implementations as well.
        let prepared = nazuchem_kernel::smiles::prepare(input, &Budget::default()).unwrap();
        assert_eq!(
            kernel::canonical_identity(&prepared.state, &Budget::default()).unwrap(),
            identity,
            "{input}",
        );
    }
}

#[test]
fn unknown_units_clear_without_erasing_neighboring_configuration() {
    let mut atom = chemistry::smiles::prepare("F[C@H](Cl)[C@H](Br)I")
        .unwrap()
        .state;
    atom.properties.atoms[1].unknown = true;
    let expected = chemistry::smiles::prepare("FC(Cl)[C@H](Br)I")
        .unwrap()
        .state;
    assert_eq!(
        assert_identity_parity(&atom).unwrap(),
        canonical_identity(&expected).unwrap(),
    );
    let mut wavy = chemistry::smiles::prepare("C[C@H](F)Cl").unwrap().state;
    wavy.directions[1] = chemistry::kekulize::Direction::Unknown;
    assert_eq!(wavy.graph.bonds[1].a, 1);
    assert_eq!(
        assert_identity_parity(&wavy).unwrap(),
        canonical_identity(&chemistry::smiles::prepare("CC(F)Cl").unwrap().state).unwrap(),
    );
    let mut bond = chemistry::smiles::prepare("F/C=C/C=C/Cl").unwrap().state;
    let alkene = bond
        .graph
        .bonds
        .iter()
        .position(|bond| bond.order == 2)
        .unwrap();
    bond.metadata.bonds[alkene].unknown_stereo = true;
    bond.metadata.bonds[alkene].stereo = 1;
    let expected = chemistry::smiles::prepare("FC=C/C=C/Cl").unwrap().state;
    assert_eq!(
        assert_identity_parity(&bond).unwrap(),
        canonical_identity(&expected).unwrap(),
    );
}

#[test]
fn unsupported_semantics_and_unreconstructable_electrons_match() {
    let plain = chemistry::smiles::prepare("CCO").unwrap().state;
    let mut wildcard = plain.clone();
    wildcard.graph.atoms[0].atomic_number = 0;
    let mut electrons = plain.clone();
    electrons.graph.atoms[0].radical_electrons = 3;
    let mut exotic = plain.clone();
    exotic.graph.bonds[0].order = 5;
    let mut relative = plain.clone();
    relative
        .metadata
        .groups
        .push(chemistry::ranking::StereoGroup {
            kind: 1,
            atoms: vec![0],
            ..Default::default()
        });
    let mut unreconstructable = chemistry::smiles::prepare("[CH3].[O]").unwrap().state;
    unreconstructable.graph.atoms[1].radical_electrons = 0;
    let mut charged_radical = chemistry::smiles::prepare("c1ccccc1").unwrap().state;
    charged_radical.graph.atoms[0].charge = 1;
    charged_radical.graph.atoms[0].radical_electrons = 1;
    for state in [
        wildcard,
        electrons,
        exotic,
        relative,
        unreconstructable,
        charged_radical,
    ] {
        assert_eq!(
            assert_identity_parity(&state),
            Err(IdentityRejection::UnsupportedAtomsOrBonds),
        );
    }
    let mut atom = plain.clone();
    atom.metadata.atoms[0].chiral_tag = 3;
    atom.properties.atoms[0].unknown = true;
    assert_eq!(
        assert_identity_parity(&atom),
        Err(IdentityRejection::NonclassicalStereo)
    );
    for stereo in [6, 7, 8] {
        let mut bond = plain.clone();
        bond.metadata.bonds[0].stereo = stereo;
        bond.metadata.bonds[0].unknown_stereo = true;
        assert_eq!(
            assert_identity_parity(&bond),
            Err(IdentityRejection::NonclassicalStereo)
        );
    }
    let allene = chemistry::smiles::prepare("FC=[C@AL1]=CCl").unwrap().state;
    assert_eq!(
        assert_identity_parity(&allene),
        Err(IdentityRejection::NonclassicalStereo)
    );
}

#[test]
fn axis_identity_and_decoding_match_for_both_helicities_and_invalid_markers() {
    for display in ["hash", "wedge"] {
        let state = chemistry::document::prepare(&chemistry::document::atrop_tests::binol(display))
            .unwrap()
            .state;
        let identity = assert_identity_parity(&state).unwrap();
        let encoded = encode_axis_markers(&state).unwrap();
        assert_eq!(
            serde_json::to_value(&encoded).unwrap(),
            serde_json::to_value(
                kernel::encode_axis_markers(&kernel_state(&state), &Budget::default()).unwrap()
            )
            .unwrap(),
        );
        let restored = assert_decoding_parity(&encoded);
        assert_eq!(assert_identity_parity(&restored).unwrap(), identity);
        let parsed = chemistry::smiles::prepare(&identity).unwrap().state;
        assert_eq!(
            assert_identity_parity(&assert_decoding_parity(&parsed)).unwrap(),
            identity
        );

        let marked = encoded
            .metadata
            .atoms
            .iter()
            .position(|atom| atom.map_number == AXIS_M || atom.map_number == AXIS_P)
            .unwrap();
        let mut unpaired = encoded.clone();
        unpaired.metadata.atoms[marked].map_number = 0;
        let mut mismatched = encoded;
        mismatched.metadata.atoms[marked].map_number =
            if display == "hash" { AXIS_P } else { AXIS_M };
        for invalid in [unpaired, mismatched] {
            assert_eq!(
                kernel::apply_axis_markers(&kernel_state(&invalid), &Budget::default())
                    .unwrap_err(),
                kernel::IdentityRejection::NonclassicalStereo,
            );
            assert!(matches!(
                apply_axis_markers(&invalid),
                Err(IdentityRejection::NonclassicalStereo)
            ));
        }
    }
    for input in [
        "[CH3:900004][CH3:900004]",
        "[c:900004]1ccccc1-[c:900004]1ccccc1",
    ] {
        let state = chemistry::smiles::prepare(input).unwrap().state;
        assert!(matches!(
            apply_axis_markers(&state),
            Err(IdentityRejection::NonclassicalStereo)
        ));
        assert_eq!(
            kernel::apply_axis_markers(&kernel_state(&state), &Budget::default()).unwrap_err(),
            kernel::IdentityRejection::NonclassicalStereo,
        );
    }
}

#[test]
fn pseudoasymmetric_trials_match_and_ignore_atom_maps() {
    for (input, center, expected) in [
        ("[CH:17](O)(C[C@H](F)C)C[C@@H](F)C", 0, true),
        ("C[C@H](F)Cl", 1, false),
        ("CCO", 0, false),
        ("CCO", 3, false),
    ] {
        let mut state = chemistry::smiles::prepare(input).unwrap().state;
        for (index, atom) in state.metadata.atoms.iter_mut().enumerate() {
            atom.map_number = (index + 1) as i32;
            atom.map_present = true;
        }
        assert_eq!(
            chemistry::stereo_lint::is_pseudoasymmetric(&state, center),
            expected,
            "{input}"
        );
        assert_eq!(
            nazuchem_kernel::stereo::is_pseudoasymmetric(&kernel_state(&state), center),
            expected,
            "{input}",
        );
    }
}

#[test]
fn identity_and_axis_helpers_use_the_injected_budget() {
    let budget = Budget::new(0, usize::MAX, None, usize::MAX, |_| Some(Box::new(())));
    let radical = chemistry::smiles::prepare("[CH3]").unwrap().state;
    assert_eq!(
        kernel::canonical_identity(&kernel_state(&radical), &budget),
        Err(kernel::IdentityRejection::UnsupportedAtomsOrBonds),
    );
    let axis = chemistry::document::prepare(&chemistry::document::atrop_tests::binol("hash"))
        .unwrap()
        .state;
    assert_eq!(
        kernel::encode_axis_markers(&kernel_state(&axis), &budget).unwrap_err(),
        kernel::IdentityRejection::NonclassicalStereo,
    );
    let marked = encode_axis_markers(&axis).unwrap();
    assert_eq!(
        kernel::apply_axis_markers(&kernel_state(&marked), &budget).unwrap_err(),
        kernel::IdentityRejection::NonclassicalStereo,
    );
}
