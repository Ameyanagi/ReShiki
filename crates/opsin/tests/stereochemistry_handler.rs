//! Graph-stage tests derived from OPSIN StereochemistryHandler and the
//! StereochemistryTest fixtures. No external executable is used.
use opsin::build_state::BuildState;
use opsin::graph::{
    AtomId, AtomParity, BondStereoValue, FragmentId, StereoGroupType, StereoReference,
};
use opsin::parse_tree::{Arena, NodeId};
use opsin::stereo_analyser::analyse;
use opsin::stereochemistry_handler::{
    StereochemistryError, StereochemistryHandler, apply_stereo_chemistry_to_stereo_centre,
    check_equivalency_of_atom_refs_and_parity, cis_trans_unambiguous_on_bond,
    swaps_required_to_sort,
};
use opsin::xml_declarations::*;
use opsin::{ParseOptions, WarningKind};

fn token(a: &mut Arena, p: NodeId, name: &str, value: &str, attrs: &[(&str, &str)]) -> NodeId {
    let n = a.token(name, value);
    for &(k, v) in attrs {
        a[n].add_attribute(k, v)
    }
    a.add_child(p, n);
    n
}
fn fixture(smiles: &str) -> (BuildState, Arena, NodeId, NodeId, FragmentId) {
    let mut state = BuildState::new(ParseOptions::strict());
    let mut a = Arena::default();
    let molecule = a.grouping(MOLECULE_EL);
    let rule = a.grouping(WORDRULE_EL);
    let word = a.grouping(WORD_EL);
    a[word].add_attribute(TYPE_ATR, "full");
    let root = a.grouping(ROOT_EL);
    a.add_child(molecule, rule);
    a.add_child(rule, word);
    a.add_child(word, root);
    let group = token(
        &mut a,
        root,
        GROUP_EL,
        "group",
        &[(TYPE_ATR, SIMPLEGROUP_TYPE_VAL)],
    );
    let f = state
        .fragment_manager
        .build_token_smiles(smiles, &mut a, group, NUMERIC_LABELS_VAL)
        .unwrap();
    a[group].fragment = Some(f);
    state.fragment_manager.make_hydrogens_explicit().unwrap();
    opsin::cycle_detector::assign_cycle_membership(state.graph_mut(), f);
    (state, a, root, group, f)
}
fn apply(
    state: &mut BuildState,
    a: &mut Arena,
    f: FragmentId,
    elements: &[NodeId],
) -> Result<(), StereochemistryError> {
    let analysis = analyse(state.graph(), f).unwrap();
    StereochemistryHandler::new(state, a, &analysis.centres, &analysis.bonds)
        .apply_stereochemical_elements(elements)
}
fn refs(atoms: [usize; 4]) -> [Option<StereoReference>; 4] {
    atoms.map(|a| Some(StereoReference::Atom(AtomId(a))))
}

#[test]
fn upstream_atom_parity_permutation_examples_and_placeholder_ids() {
    let a = refs([0, 1, 2, 3]);
    let b = refs([2, 3, 0, 1]);
    assert_eq!(swaps_required_to_sort(&b), 4);
    assert!(check_equivalency_of_atom_refs_and_parity(&a, 1, &b, 1));
    assert!(!check_equivalency_of_atom_refs_and_parity(&a, 1, &b, -1));
    let b = refs([1, 3, 0, 2]);
    assert_eq!(swaps_required_to_sort(&b), 3);
    assert!(!check_equivalency_of_atom_refs_and_parity(&a, 1, &b, 1));
    assert!(check_equivalency_of_atom_refs_and_parity(&a, 1, &b, -1));
    // Both source dummy H atoms have id 0; they remain distinct references but
    // sorting compares their equal IDs without an artificial extra swap.
    let dummy = [
        Some(StereoReference::DeoxyHydrogen),
        Some(StereoReference::ImplicitHydrogen),
        Some(StereoReference::Atom(AtomId(1))),
        Some(StereoReference::Atom(AtomId(0))),
    ];
    assert_eq!(swaps_required_to_sort(&dummy), 1);
}
#[test]
fn cip_r_and_s_use_the_source_reference_order() {
    for (value, parity) in [("R", -1), ("S", 1)] {
        let (mut state, mut a, r, _, f) = fixture("C(Br)(F)([H])Cl");
        let id = token(
            &mut a,
            r,
            STEREOCHEMISTRY_EL,
            value,
            &[
                (TYPE_ATR, R_OR_S_TYPE_VAL),
                (VALUE_ATR, value),
                (LOCANT_ATR, "1"),
                (STEREOGROUP_ATR, "Abs"),
            ],
        );
        apply(&mut state, &mut a, f, &[id]).unwrap();
        let atom = state.graph().fragment(f).atoms[0];
        let p = state.graph().atom(atom).parity.as_ref().unwrap();
        assert_eq!(p.atom_refs, refs([1, 3, 2, 4]));
        assert_eq!(p.parity, parity);
        assert_eq!(p.stereo_group.kind, StereoGroupType::Absolute);
        assert!(a[id].parent.is_none());
    }
}
#[test]
fn lone_pair_is_the_centre_atom_reference() {
    let (mut state, mut a, r, _, f) = fixture("[S](=O)(C)CC");
    let analysis = analyse(state.graph(), f).unwrap();
    assert_eq!(analysis.centres.len(), 1);
    let centre = analysis.centres[0];
    let id = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "R",
        &[(TYPE_ATR, R_OR_S_TYPE_VAL), (VALUE_ATR, "R")],
    );
    apply(&mut state, &mut a, f, &[id]).unwrap();
    let p = state.graph().atom(centre.atom).parity.as_ref().unwrap();
    assert_eq!(p.atom_refs[1], Some(StereoReference::Atom(centre.atom)));
}
#[test]
fn locanted_stereo_is_assigned_before_unlocanted_terms() {
    let (mut state, mut a, r, _, f) = fixture("C(Br)(F)(Cl)C(Br)(F)Cl");
    let unloc = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "R",
        &[(TYPE_ATR, R_OR_S_TYPE_VAL), (VALUE_ATR, "R")],
    );
    let loc = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "S",
        &[
            (TYPE_ATR, R_OR_S_TYPE_VAL),
            (VALUE_ATR, "S"),
            (LOCANT_ATR, "1"),
        ],
    );
    apply(&mut state, &mut a, f, &[unloc, loc]).unwrap();
    let first = state.graph().atom_by_locant(f, "1").unwrap();
    let second = state.graph().atom_by_locant(f, "5").unwrap();
    assert_eq!(state.graph().atom(first).parity.as_ref().unwrap().parity, 1);
    assert_eq!(
        state.graph().atom(second).parity.as_ref().unwrap().parity,
        -1
    );
}
#[test]
fn upstream_e_z_and_cis_trans_bond_cases() {
    for (kind, value, expected) in [
        (E_OR_Z_TYPE_VAL, "E", Some(BondStereoValue::Trans)),
        (E_OR_Z_TYPE_VAL, "Z", Some(BondStereoValue::Cis)),
        (E_OR_Z_TYPE_VAL, "EZ", None),
        (CISORTRANS_TYPE_VAL, "cis", Some(BondStereoValue::Cis)),
        (CISORTRANS_TYPE_VAL, "trans", Some(BondStereoValue::Trans)),
    ] {
        let (mut state, mut a, r, _, f) = fixture("CC=CC");
        let id = token(
            &mut a,
            r,
            STEREOCHEMISTRY_EL,
            value,
            &[(TYPE_ATR, kind), (VALUE_ATR, value), (LOCANT_ATR, "2")],
        );
        apply(&mut state, &mut a, f, &[id]).unwrap();
        let bond = state.graph().bond_between(AtomId(1), AtomId(2)).unwrap();
        assert_eq!(
            state.graph().bond(bond).stereo.as_ref().map(|s| s.value),
            expected
        );
        if let Some(stereo) = &state.graph().bond(bond).stereo {
            assert_eq!(
                stereo.atom_refs,
                [AtomId(0), AtomId(1), AtomId(2), AtomId(3)]
            )
        }
    }
    for (smiles, expected) in [
        ("[H]C([H])([H])C([H])=C([H])C([H])([H])[H]", true),
        ("[H]C([H])([H])C(Cl)=C([H])C([H])([H])[H]", false),
    ] {
        let (state, _, _, _, f) = fixture(smiles);
        let bond = *state
            .graph()
            .fragment(f)
            .bonds
            .iter()
            .find(|&&b| state.graph().bond(b).order == 2)
            .unwrap();
        assert_eq!(cis_trans_unambiguous_on_bond(state.graph(), bond), expected)
    }
}
#[test]
fn unsupported_descriptors_follow_exact_warning_categories() {
    for kind in [
        ENDO_EXO_SYN_ANTI_TYPE_VAL,
        RELATIVECISTRANS_TYPE_VAL,
        AXIAL_TYPE_VAL,
    ] {
        for warn in [false, true] {
            let (mut state, mut a, r, _, f) = fixture("C(Br)(F)([H])Cl");
            state
                .options
                .warn_rather_than_fail_on_uninterpretable_stereochemistry = warn;
            let id = token(
                &mut a,
                r,
                STEREOCHEMISTRY_EL,
                "unsupported",
                &[(TYPE_ATR, kind)],
            );
            let result = apply(&mut state, &mut a, f, &[id]);
            if warn {
                result.unwrap();
                assert_eq!(state.warnings.len(), 1);
                assert_eq!(state.warnings[0].kind, WarningKind::StereochemistryIgnored);
                assert_eq!(
                    state.warnings[0].message,
                    format!("{kind} stereochemistry is not currently interpretable by OPSIN")
                );
                assert!(a[id].parent.is_some())
            } else {
                assert!(matches!(
                    result,
                    Err(StereochemistryError::Uninterpretable(_))
                ));
                assert!(state.warnings.is_empty())
            }
        }
    }
    let (mut state, mut a, r, _, f) = fixture("C(Br)(F)([H])Cl");
    state
        .options
        .warn_rather_than_fail_on_uninterpretable_stereochemistry = true;
    let id = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "r",
        &[(TYPE_ATR, R_OR_S_TYPE_VAL), (VALUE_ATR, "r")],
    );
    assert!(matches!(
        apply(&mut state, &mut a, f, &[id]),
        Err(StereochemistryError::StructureBuilding(_))
    ));
    assert!(state.warnings.is_empty());
}
#[test]
fn optical_rotation_always_warns_and_detaches() {
    let (mut state, mut a, r, _, f) = fixture("C(Br)(F)([H])Cl");
    let id = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "(+)",
        &[(TYPE_ATR, OPTICALROTATION_TYPE_VAL)],
    );
    apply(&mut state, &mut a, f, &[id]).unwrap();
    assert_eq!(state.warnings[0].kind, WarningKind::StereochemistryIgnored);
    assert!(state.warnings[0].message.ends_with("(+)") && a[id].parent.is_none());
}
#[test]
fn global_racemic_and_relative_flags_and_multiple_undefined_centres() {
    for (kind, expected) in [
        (RAC_TYPE_VAL, StereoGroupType::Racemic),
        (REL_TYPE_VAL, StereoGroupType::Relative),
    ] {
        let (mut state, mut a, r, _, f) = fixture("C(Br)(F)([H])Cl");
        let id = token(&mut a, r, STEREOCHEMISTRY_EL, kind, &[(TYPE_ATR, kind)]);
        apply(&mut state, &mut a, f, &[id]).unwrap();
        let p = state.graph().atom(AtomId(0)).parity.as_ref().unwrap();
        assert_eq!(p.parity, -1);
        assert_eq!(p.stereo_group.kind, expected);
        assert_eq!(p.stereo_group.number, 1);
        assert!(state.warnings.is_empty());
    }
    let (mut state, mut a, r, _, f) = fixture("C(Br)(F)(Cl)C(Br)(F)Cl");
    let id = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "rac",
        &[(TYPE_ATR, RAC_TYPE_VAL)],
    );
    apply(&mut state, &mut a, f, &[id]).unwrap();
    assert_eq!(
        state.warnings[0].message,
        "More than one undefined stereocenter for rac- or rel- mixture"
    );
    assert!(
        state
            .graph()
            .fragment(f)
            .atoms
            .iter()
            .all(|&a| state.graph().atom(a).parity.is_none())
    );
}
#[test]
fn d_l_and_dl_amino_acid_reference_conventions() {
    for (value, parity, racemic) in [
        ("l", -1, false),
        ("ls", -1, false),
        ("d", 1, false),
        ("ds", 1, false),
        ("dl", 1, true),
    ] {
        let (mut state, mut a, r, _, f) = fixture("C([H])(N)(C(=O)O)C");
        let id = token(
            &mut a,
            r,
            STEREOCHEMISTRY_EL,
            value,
            &[(TYPE_ATR, DLSTEREOCHEMISTRY_TYPE_VAL), (VALUE_ATR, value)],
        );
        apply(&mut state, &mut a, f, &[id]).unwrap();
        let p = state.graph().atom(AtomId(0)).parity.as_ref().unwrap();
        assert_eq!(p.atom_refs, refs([3, 6, 2, 1]));
        assert_eq!(p.parity, parity);
        assert_eq!(p.stereo_group.kind == StereoGroupType::Racemic, racemic);
        if racemic {
            assert_eq!(p.stereo_group.number, 2);
            assert_eq!(state.racemic_group_count, 2)
        }
    }
}
#[test]
fn ring_cis_trans_preserves_an_existing_absolute_centre() {
    for value in ["cis", "trans"] {
        let (mut state, mut a, r, _, f) = fixture("C1(C)CC(C)CCC1");
        let analysis = analyse(state.graph(), f).unwrap();
        assert_eq!(analysis.centres.len(), 2);
        let first = analysis.centres[0];
        apply_stereo_chemistry_to_stereo_centre(state.graph_mut(), first, "R").unwrap();
        let before = state
            .graph()
            .atom(first.atom)
            .parity
            .as_ref()
            .unwrap()
            .clone();
        let id = token(
            &mut a,
            r,
            STEREOCHEMISTRY_EL,
            value,
            &[(TYPE_ATR, CISORTRANS_TYPE_VAL), (VALUE_ATR, value)],
        );
        apply(&mut state, &mut a, f, &[id]).unwrap();
        let after = state.graph().atom(first.atom).parity.as_ref().unwrap();
        assert!(check_equivalency_of_atom_refs_and_parity(
            &before.atom_refs,
            before.parity,
            &after.atom_refs,
            after.parity
        ));
        assert!(
            analysis
                .centres
                .iter()
                .all(|c| state.graph().atom(c.atom).parity.is_some())
        );
    }
}
#[test]
fn ring_cis_trans_without_hydrogen_emits_the_upstream_ambiguity() {
    let (mut state, mut a, r, _, f) = fixture("C1(F)(Cl)CC(Br)(I)CCC1");
    let id = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "trans",
        &[(TYPE_ATR, CISORTRANS_TYPE_VAL), (VALUE_ATR, "trans")],
    );
    apply(&mut state, &mut a, f, &[id]).unwrap();
    assert_eq!(state.warnings.len(), 1);
    assert_eq!(state.warnings[0].kind, WarningKind::AppearsAmbiguous);
    assert!(
        state.warnings[0]
            .message
            .starts_with("Ring cis/trans applied to stereocenter where no hydrogen was present.")
    );
}
#[test]
fn alpha_beta_contradictions_are_structural_errors_and_xi_clears_parity() {
    let (mut state, mut a, r, g, f) = fixture("C1(F)C(O)CCC1");
    a[g].add_attribute(ALPHABETACLOCKWISEATOMORDERING_ATR, "1/3/5/6/7");
    state
        .options
        .warn_rather_than_fail_on_uninterpretable_stereochemistry = true;
    let alpha = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "1alpha",
        &[
            (TYPE_ATR, ALPHA_OR_BETA_TYPE_VAL),
            (VALUE_ATR, "alpha"),
            (LOCANT_ATR, "1"),
        ],
    );
    let beta = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "1beta",
        &[
            (TYPE_ATR, ALPHA_OR_BETA_TYPE_VAL),
            (VALUE_ATR, "beta"),
            (LOCANT_ATR, "1"),
        ],
    );
    assert!(
        matches!(apply(&mut state,&mut a,f,&[alpha,beta]),Err(StereochemistryError::StructureBuilding(message)) if message=="contradictory alpha/beta stereochemistry at position 1")
    );
    assert!(state.warnings.is_empty());
    let xi = token(
        &mut a,
        r,
        STEREOCHEMISTRY_EL,
        "1xi",
        &[
            (TYPE_ATR, ALPHA_OR_BETA_TYPE_VAL),
            (VALUE_ATR, "xi"),
            (LOCANT_ATR, "1"),
        ],
    );
    apply(&mut state, &mut a, f, &[xi]).unwrap();
    assert!(state.graph().atom(AtomId(0)).parity.is_none());
}
#[test]
fn carbohydrate_prefixes_flip_or_clear_source_parities() {
    let (mut state, mut a, r, g, f) = fixture("O=C[C@H](O)[C@H](O)[C@H](O)CO");
    a[g].set_attribute(SUBTYPE_ATR, SYSTEMATICCARBOHYDRATESTEMALDOSE_SUBTYPE_VAL);
    let analysis = analyse(state.graph(), f).unwrap();
    assert_eq!(analysis.centres.len(), 3);
    let original = analysis
        .centres
        .iter()
        .map(|c| state.graph().atom(c.atom).parity.as_ref().unwrap().parity)
        .collect::<Vec<_>>();
    let id = a.token(STEREOCHEMISTRY_EL, "prefix");
    a[id].add_attribute(TYPE_ATR, CARBOHYDRATECONFIGURATIONPREFIX_TYPE_VAL);
    a[id].add_attribute(VALUE_ATR, "r/l/?");
    a.insert_before(g, id);
    apply(&mut state, &mut a, f, &[id]).unwrap();
    assert_eq!(
        state
            .graph()
            .atom(analysis.centres[0].atom)
            .parity
            .as_ref()
            .unwrap()
            .parity,
        original[0]
    );
    assert_eq!(
        state
            .graph()
            .atom(analysis.centres[1].atom)
            .parity
            .as_ref()
            .unwrap()
            .parity,
        -original[1]
    );
    assert!(
        state
            .graph()
            .atom(analysis.centres[2].atom)
            .parity
            .is_none()
    );
    assert_eq!(a[id].parent, Some(r));
}
#[test]
fn redundant_predefined_atom_and_bond_stereo_are_removed() {
    let (mut state, mut a, _, _, f) = fixture("C(C)(F)([H])C");
    let atom = state.graph().fragment(f).atoms[0];
    state.graph_mut().atom_mut(atom).parity = Some(AtomParity::new(refs([1, 2, 3, 4]), 1));
    let analysis = analyse(state.graph(), f).unwrap();
    assert!(analysis.centres.is_empty());
    StereochemistryHandler::new(&mut state, &mut a, &analysis.centres, &analysis.bonds)
        .remove_redundant_stereo_centres(&[atom], &[]);
    assert!(state.graph().atom(atom).parity.is_none());
}
