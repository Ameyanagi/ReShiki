// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/CipSequenceRules.java | b91b610af5ab07560fedb20730d7aef46bb2bca0
// Source: OPSIN | opsin-core/src/test/java/uk/ac/cam/ch/wwmm/opsin/StereochemistryTest.java | b91b610af5ab07560fedb20730d7aef46bb2bca0
// Source: ReShiki | crates/opsin/tests/cip_ordering.rs | 0e6e9009ca2e8bfebe149c260fc5a2f484f7dfb9
// Copyright 2017 Daniel Lowe
// Changes: Rust graph/tree construction and regression harness around source-derived cases/assertions; local regressions retained.

//! Ordering expectations transcribed from OPSIN 2.9.0
//! `StereochemistryTest.testCIPpriority1..15` and `testCipUnassignable`,
//! commit b91b610af5ab07560fedb20730d7aef46bb2bca0 (MIT).
//! Upstream atom IDs start at one; this crate's arena indices start at zero.

use nazuchem_n2s::cip::{CipOrderingError, CipSequenceRules};
use nazuchem_n2s::graph::{AtomId, Element, Graph};
use nazuchem_n2s::smiles::build_fragment;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy)]
enum Expected {
    Id(usize),
    Element(Element),
}
use Expected::{Element as El, Id};

fn graph(smiles: &str, explicit_hydrogens: bool) -> Graph {
    let mut graph = Graph::default();
    let fragment = build_fragment(&mut graph, smiles, "", "").unwrap();
    if explicit_hydrogens {
        graph.make_hydrogens_explicit(fragment).unwrap();
    }
    graph
}

fn check(graph: &Graph, centre: usize, expected: &[Expected]) {
    let ordered = CipSequenceRules::new(graph, AtomId(centre - 1))
        .get_neighbouring_atoms_in_cip_order()
        .unwrap();
    assert_eq!(ordered.len(), expected.len());
    for (&actual, expected) in ordered.iter().zip(expected) {
        match expected {
            Id(id) => assert_eq!(actual.0 + 1, *id),
            El(element) => assert_eq!(graph.atom(actual).element, *element),
        }
    }
}

#[test]
fn upstream_atomic_numbers_and_isotopes() {
    check(
        &graph("C(Br)(F)([H])Cl", false),
        1,
        &[
            El(Element::H),
            El(Element::F),
            El(Element::Cl),
            El(Element::Br),
        ],
    );
    check(
        &graph("C(Cl)([2H])([3H])[H]", false),
        1,
        &[Id(5), Id(3), Id(4), Id(2)],
    );
}

#[test]
fn upstream_ring_paths_and_multiple_bonds() {
    let cases: &[(&str, usize, bool, &[Expected])] = &[
        (
            "C([H])(C1CC1)(C1CCC1)O",
            1,
            false,
            &[El(Element::H), Id(3), Id(6), El(Element::O)],
        ),
        (
            "[C](N)(C1=CC(O)=CC=C1)([H])C2=CC=C(O)C=C2",
            1,
            false,
            &[El(Element::H), Id(11), Id(3), El(Element::N)],
        ),
        (
            "[C](N)(C1CC(O)CCC1)([H])C2CCC(O)CC2",
            1,
            false,
            &[El(Element::H), Id(11), Id(3), El(Element::N)],
        ),
        (
            "C1([H])(C(=O)O[H])C([H])([H])SC([H])([H])N([H])1",
            1,
            false,
            &[El(Element::H), Id(3), Id(7), El(Element::N)],
        ),
        (
            "C1([H])(O)C([H])(C([H])([H])[H])OC([H])([H])C([H])([H])C1([H])(O[H])",
            1,
            false,
            &[El(Element::H), Id(17), Id(4), El(Element::O)],
        ),
        (
            "[H]OC2([H])(C([H])([H])C([H])([H])C3([H])(C4([H])(C([H])([H])C([H])([H])C1=C([H])C([H])([H])C([H])([H])C([H])([H])C1([H])C4([H])(C([H])([H])C([H])([H])C23(C([H])([H])[H])))))",
            35,
            false,
            &[El(Element::H), Id(37), Id(13), Id(33)],
        ),
        (
            "C1(C=C)CC1C2=CC=CC=C2",
            1,
            true,
            &[El(Element::H), Id(4), Id(2), Id(5)],
        ),
        (
            "C(O[H])([H])(C1([H])C([H])(F)C([H])(Cl)C([H])([H])C([H])(I)C1([H])([H]))C1([H])C([H])(F)C([H])(Br)C([H])([H])C([H])(Cl)C1([H])([H])",
            1,
            true,
            &[El(Element::H), Id(5), Id(22), El(Element::O)],
        ),
        (
            "C1(C)(CCC(=O)N1)CCC(=O)NC(C)C",
            1,
            true,
            &[Id(2), Id(3), Id(8), El(Element::N)],
        ),
        (
            "C(O)(C#CC)C1=CC=CC=C1",
            1,
            true,
            &[El(Element::H), Id(6), Id(3), Id(2)],
        ),
        (
            "C([H])(O)(C(C(F)CCl)CCBr)C(C(F)CF)CCI",
            1,
            true,
            &[Id(2), Id(12), Id(4), Id(3)],
        ),
    ];
    for (smiles, centre, explicit_hydrogens, expected) in cases {
        check(&graph(smiles, *explicit_hydrogens), *centre, expected);
    }
}

#[test]
fn upstream_bridged_polycycle_five_centres() {
    let graph = graph("C17C=CC23C45OC6C19.O74.O2C3.C5.C6(C)C.C9", true);
    check(&graph, 1, &[El(Element::H), Id(2), Id(8), El(Element::O)]);
    check(&graph, 4, &[Id(3), Id(11), Id(5), El(Element::O)]);
    check(&graph, 5, &[Id(12), Id(4), Id(6), Id(9)]);
    check(&graph, 7, &[El(Element::H), Id(13), Id(8), El(Element::O)]);
    check(&graph, 8, &[El(Element::H), Id(16), Id(7), Id(1)]);
}

#[test]
fn upstream_fused_ring_priority8_frozen_graph() {
    // StereochemistryTest.testCIPpriority8 uses a name-built graph. Freeze that
    // graph so this CIP test does not depend on the rest of the naming port.
    // Name: (6aR)-6-phenyl-6,6a-dihydroisoindolo[2,1-a]quinazoline-5,11-dione
    // Oracle: OPSIN 2.9.0 core JAR, SHA256
    // 627ee5da4af551f9c4d1d766f545eb7cf519a344776e0bb677247a47abac0252.
    // Reproduction: parseChemicalName(name).getStructure(); enumerate
    // getAtomList() and getBondSet(), mapping atoms to zero-based list indices;
    // order new CipSequenceRules(fragment.getAtomByLocant("6a")).
    // getNeighbouringAtomsInCipOrder(). The expected atoms below correspond to
    // H, C at 6b, N at 6, and N at 12, exactly as the upstream assertions.
    use Element::{C, H, N, O};
    let elements = [
        C, C, C, C, C, C, H, H, H, H, H, C, C, C, C, C, C, N, C, C, C, C, C, C, C, C, N, C, O, O,
        H, H, H, H, H, H, H, H, H,
    ];
    let bonds = [
        (0, 1, 2),
        (1, 2, 1),
        (2, 3, 2),
        (3, 4, 1),
        (4, 5, 2),
        (0, 5, 1),
        (1, 6, 1),
        (2, 7, 1),
        (3, 8, 1),
        (4, 9, 1),
        (5, 10, 1),
        (0, 17, 1),
        (26, 18, 1),
        (18, 17, 1),
        (17, 16, 1),
        (16, 15, 1),
        (15, 14, 1),
        (14, 13, 2),
        (13, 12, 1),
        (12, 11, 2),
        (11, 27, 1),
        (26, 27, 1),
        (15, 27, 2),
        (25, 24, 1),
        (24, 23, 1),
        (23, 22, 2),
        (22, 21, 1),
        (21, 20, 2),
        (20, 19, 1),
        (24, 19, 2),
        (26, 25, 1),
        (18, 19, 1),
        (16, 28, 2),
        (25, 29, 2),
        (11, 30, 1),
        (12, 31, 1),
        (13, 32, 1),
        (14, 33, 1),
        (18, 34, 1),
        (20, 35, 1),
        (21, 36, 1),
        (22, 37, 1),
        (23, 38, 1),
    ];
    let mut graph = Graph::default();
    let fragment = graph.add_fragment("");
    for element in elements {
        graph.add_atom(fragment, element);
    }
    for (from, to, order) in bonds {
        graph.add_bond(AtomId(from), AtomId(to), order).unwrap();
    }
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(18))
            .get_neighbouring_atoms_in_cip_order()
            .unwrap(),
        vec![AtomId(34), AtomId(19), AtomId(17), AtomId(26)]
    );
}

#[test]
fn upstream_symmetric_ring_is_unassignable() {
    let graph = graph("NC1(O)CCC(CCC2CCCCC2)CC1", false);
    let error = CipSequenceRules::new(&graph, AtomId(1))
        .get_neighbouring_atoms_in_cip_order()
        .unwrap_err();
    assert_eq!(error, CipOrderingError::UnresolvedTie);
    assert_eq!(
        error.to_string(),
        "Failed to assign CIP stereochemistry, this indicates a bug in OPSIN or a limitation in OPSIN's implementation of the sequence rules"
    );
}

#[test]
fn ignored_bond_neighbour_is_removed_before_ordering() {
    let graph = graph("FC(Cl)=C(Br)I", false);
    let rules = CipSequenceRules::new(&graph, AtomId(1));
    assert_eq!(
        rules
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(AtomId(3))
            .unwrap(),
        vec![AtomId(0), AtomId(2)]
    );
    assert!(matches!(
        rules.get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(AtomId(4)),
        Err(CipOrderingError::InvalidGraph(_))
    ));
}

#[test]
fn complete_constitutional_comparison_precedes_isotope_comparison() {
    // The more remote O outranks N before the nearer carbon isotope is used.
    let graph = graph("C([H])(F)([13C]N)CO", false);
    check(&graph, 1, &[Id(2), Id(4), Id(6), Id(3)]);
}

#[test]
fn specified_isotope_precedes_unspecified_even_for_a_lower_mass() {
    // OPSIN compares isotope presence, not an implicit natural isotope mass.
    let graph = graph("C([H])(F)([11C])C", false);
    check(&graph, 1, &[Id(2), Id(5), Id(4), Id(3)]);
}

#[test]
fn protium_ties_unlabelled_hydrogen_locally_and_in_remote_ligands() {
    for (smiles, centre, first, second) in [("C([1H])([H])(F)Cl", 0, 1, 2), ("OC(C[1H])C", 1, 2, 4)]
    {
        let graph = graph(smiles, true);
        assert_eq!(
            CipSequenceRules::new(&graph, AtomId(centre))
                .compare_ligands(AtomId(first), AtomId(second)),
            Err(CipOrderingError::UnresolvedTie),
            "{smiles}"
        );
    }
    check(
        &graph("C(Cl)([2H])([3H])[1H]", false),
        1,
        &[Id(5), Id(3), Id(4), Id(2)],
    );
}

#[test]
fn multiple_bond_to_chiral_root_does_not_duplicate_the_root() {
    // P-91.1.4.2.4: the terminal =O has no extra S ghost. Consequently the
    // physical H neighbour distinguishes OH from terminal O in this graph.
    let graph = graph("S(=O)(O[H])(C)CC", false);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(0))
            .compare_ligands(AtomId(1), AtomId(2))
            .unwrap(),
        Ordering::Less
    );
}

#[test]
fn equivalent_ligands_fail_and_empty_or_single_ligands_need_no_comparison() {
    let symmetric = graph("C([H])([H])([H])[H]", false);
    assert_eq!(
        CipSequenceRules::new(&symmetric, AtomId(0)).get_neighbouring_atoms_in_cip_order(),
        Err(CipOrderingError::UnresolvedTie)
    );
    let isolated = graph("[He]", false);
    assert_eq!(
        CipSequenceRules::new(&isolated, AtomId(0))
            .get_neighbouring_atoms_in_cip_order()
            .unwrap(),
        Vec::<AtomId>::new()
    );
    let single = graph("[H][H]", false);
    assert_eq!(
        CipSequenceRules::new(&single, AtomId(0))
            .get_neighbouring_atoms_in_cip_order()
            .unwrap(),
        vec![AtomId(1)]
    );
}

#[test]
fn stereo_beyond_constitution_and_isotopes_does_not_break_ties() {
    let graph = graph("C([H])(O)([C@]([H])(F)Cl)[C@@]([H])(F)Cl", false);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(0)).get_neighbouring_atoms_in_cip_order(),
        Err(CipOrderingError::UnresolvedTie)
    );
}

#[test]
fn invalid_arena_references_are_errors() {
    let graph = Graph::default();
    assert!(matches!(
        CipSequenceRules::new(&graph, AtomId(0)).get_neighbouring_atoms_in_cip_order(),
        Err(CipOrderingError::InvalidGraph(_))
    ));
}

#[test]
fn cyclopentadienide_ranks_below_neutral_cyclopentadienyl() {
    // RDKit 2026.09.1 rdCIPLabeler.AssignCIPLabels gives S for all three.
    // P-92.1.4.4: sphere II compares neutral C (6) with the Cp− duplicate (24/5).
    for smiles in [
        "O[C@H](C1=CC=CC1)C1=C[CH-]C=C1",
        "O[C@H](C1=CCC=C1)C1=C[CH-]C=C1",
        "O[C@H](C1=CC=CC1)[C-]1C=CC=C1",
    ] {
        check(
            &graph(smiles, true),
            2,
            &[El(Element::H), Id(8), Id(3), El(Element::O)],
        );
    }
}

#[test]
fn cyclopentadienide_resonance_forms_have_equal_priority() {
    let graph = graph("C(F)(C1=C[CH-]C=C1)[C-]1C=CC=C1", true);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(0)).compare_ligands(AtomId(2), AtomId(7)),
        Err(CipOrderingError::UnresolvedTie)
    );
}

#[test]
fn sodium_counterion_does_not_disable_cyclopentadienide_averaging() {
    // The disconnected Na+ does not change P01's ligand priorities.
    check(
        &graph("[Na+].O[C@H](C1=CC=CC1)C1=C[CH-]C=C1", true),
        3,
        &[El(Element::H), Id(9), Id(4), El(Element::O)],
    );
}

#[test]
fn fluoropyrrolide_preserves_drawn_ordering() {
    // X01: the hetero anions use origin/main's drawn-bond comparison.
    // The first attachment has [N, C, C], above the second's [N, C].
    check(
        &graph("O[C@H](C1=CC(F)=C[N-]1)[C-]1C=CC=N1", true),
        2,
        &[El(Element::H), Id(9), Id(3), El(Element::O)],
    );
}

#[test]
fn pyrrolide_resonance_forms_preserve_drawn_ordering() {
    // X03: as on origin/main, [N, C, C] outranks [N, C]. Hetero anions do
    // not receive averaged duplicates that would make these ligands tie.
    let graph = graph("FC(C1=CC=C[N-]1)[C-]1C=CC=N1", true);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(1)).compare_ligands(AtomId(2), AtomId(7)),
        Ok(Ordering::Greater)
    );
}

#[test]
fn fused_indenide_preserves_drawn_kekule_ordering() {
    let mut graph = Graph::default();
    let fragment = build_fragment(&mut graph, "C[C@H](O)[C-]1C=Cc2ccccc21", "", "").unwrap();
    nazuchem_n2s::fragment_tools::convert_spare_valencies_to_double_bonds(&mut graph, fragment)
        .unwrap();
    graph.make_hydrogens_explicit(fragment).unwrap();
    check(&graph, 2, &[El(Element::H), Id(1), Id(4), El(Element::O)]);
}

#[test]
fn neutral_azaquinone_preserves_exocyclic_oxygen_duplicates() {
    // The decisive carbonyl branches are [O, O, N] versus [O, O, C].
    // A ring C/N average must not replace the exocyclic C=O oxygen duplicate.
    check(
        &graph("O[C@H](C1=CC(=O)C=NC1=O)C1=CC(=O)C=CC1=O", true),
        2,
        &[El(Element::H), Id(11), Id(3), El(Element::O)],
    );
}

#[test]
fn cationic_pyrylium_preserves_drawn_multiple_bond_duplicates() {
    // The drawn C=[O+] supplies [O, O, C], above the other ligand's [O, N, H].
    check(
        &graph("O[C@H](C1=[O+]C=CC=C1)C(N)OC", true),
        2,
        &[El(Element::H), Id(9), Id(3), El(Element::O)],
    );
}

#[test]
fn ammonium_diene_outranks_neutral_cyclopentadienyl() {
    // W21: the saturated N+ retains its drawn neighbours, as on origin/main.
    check(
        &graph("O[C@H](C1=CC=CC1)C1=C[N+](C)(C)C=C1", true),
        2,
        &[El(Element::H), Id(3), Id(8), El(Element::O)],
    );
}

#[test]
fn ineligible_pyrylium_next_to_cyclopentadienide_preserves_drawn_ordering() {
    // The O+ charge in the same component disables all averaging.
    // Drawn neighbours at the attachment atoms give [O, C, C] > Cp− [C, C].
    check(
        &graph("O[C@H]([C-]1C=CC=C1)C1=CC=CC=[O+]1", true),
        2,
        &[El(Element::H), Id(3), Id(8), El(Element::O)],
    );
}

#[test]
fn ineligible_pyridinium_next_to_cyclopentadienide_preserves_drawn_ordering() {
    // The NH+ charge in the same component likewise disables averaging.
    // Its attachment sphere [N, C, C] outranks Cp−'s drawn [C, C].
    check(
        &graph("O[C@H]([C-]1C=CC=C1)C1=CC=CC=[NH+]1", true),
        2,
        &[El(Element::H), Id(3), Id(8), El(Element::O)],
    );
}

#[test]
fn neutral_azaquinone_alkene_endpoint_preserves_oxygen_duplicates() {
    let graph = graph("FC=C(C1=CC(=O)C=NC1=O)C1=CC(=O)C=CC1=O", true);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(2))
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(AtomId(1))
            .unwrap(),
        vec![AtomId(11), AtomId(3)]
    );
}

#[test]
fn perchlorate_spectator_preserves_ligand_ordering() {
    let graph = graph("[NH3+][C@H](C)CC.Cl(=O)(=O)(=O)[O-]", true);
    // OPSIN's perchlorate representation must not impose a valence check on
    // this ordinary methyl [H, H, H] versus ethyl [C, H, H] comparison.
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(1)).compare_ligands(AtomId(2), AtomId(3)),
        Ok(Ordering::Less)
    );
}

#[test]
fn multiply_charged_ring_preserves_drawn_ordering() {
    let graph = graph("O[C@H](C1CC1)[C-]1C=C[NH+]=C1", true);
    // The extra positive charge excludes averaging: [C, C] < [C, C, H].
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(1)).compare_ligands(AtomId(5), AtomId(2)),
        Ok(Ordering::Less)
    );
}

#[test]
fn saturated_fused_anion_preserves_drawn_ordering_against_simple_cyclopentadienide() {
    // M01: fusion disables averaging throughout the component. Origin/main's
    // drawn-bond comparison places the fused ligand above the simple ring.
    check(
        &graph("O[C@H]([C-]1C=CC=C1)[C-]1C=CC2=C1CCC2", true),
        2,
        &[El(Element::H), Id(3), Id(8), El(Element::O)],
    );
}

#[test]
fn saturated_fused_anion_preserves_drawn_ordering_against_cyclopropyl() {
    let graph = graph("O[C@H](C1CC1)[C-]1C=CC2=C1CCC2", true);
    // Origin/main's drawn attachment neighbours give [C, C] < [C, C, H].
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(1)).compare_ligands(AtomId(5), AtomId(2)),
        Ok(Ordering::Less)
    );
}

#[test]
fn saturated_fused_anion_preserves_drawn_ordering_at_alkene_endpoint() {
    // M03: the same origin/main ordering applies at the alkene endpoint.
    let graph = graph("F/C=C(/[C-]1C=CC=C1)[C-]1C=CC2=C1CCC2", true);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(2))
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(AtomId(1))
            .unwrap(),
        vec![AtomId(3), AtomId(8)]
    );
}

#[test]
fn saturated_fused_anion_ranks_below_cyclopropyl_at_alkene_endpoint() {
    let graph = graph("F/C=C(/C1CC1)[C-]1C=CC2=C1CCC2", true);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(2))
            .get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(AtomId(1))
            .unwrap(),
        vec![AtomId(6), AtomId(3)]
    );
}

#[test]
fn ineligible_fused_indenide_disables_component_resonance_averaging() {
    let mut graph = Graph::default();
    let fragment =
        build_fragment(&mut graph, "O[C@H]([C-]1C=CC=C1)[C-]1C=Cc2ccccc21", "", "").unwrap();
    nazuchem_n2s::fragment_tools::convert_spare_valencies_to_double_bonds(&mut graph, fragment)
        .unwrap();
    graph.make_hydrogens_explicit(fragment).unwrap();
    // origin/main's drawn-bond ordering: both attachment atoms expose [C, C].
    // In the next sphere, the fused aromatic carbon has two actual C neighbours
    // and a Kekule duplicate, so its [C, C, C] beats simple Cp− [C, C, H].
    // Averaging only the eligible Cp− would instead make its extra 24/5 win.
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(1)).compare_ligands(AtomId(2), AtomId(7)),
        Ok(Ordering::Less)
    );
}

#[test]
fn stereocentre_inside_the_anion_ring_keeps_drawn_ordering() {
    let graph = graph("F[P@]1(Cl)=CC=C[CH-]1", true);
    assert_eq!(
        CipSequenceRules::new(&graph, AtomId(1)).compare_ligands(AtomId(3), AtomId(6)),
        Err(CipOrderingError::UnresolvedTie)
    );
}
