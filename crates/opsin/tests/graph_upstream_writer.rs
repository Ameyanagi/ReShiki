// Frozen examples transcribed from OPSIN 2.9.0 SMILESWriterTest.java.
// Upstream commit b91b610af5ab07560fedb20730d7aef46bb2bca0, MIT.
// No Java executable or external oracle is used by these tests.
use opsin::{
    ParseOptions, Parser, Status,
    graph::{AtomId, Graph},
    smiles::{build_fragment, write_semantic_cxsmiles},
};

#[test]
fn unspecified_imine_stays_unspecified_beside_configured_alkenes() {
    let parser = Parser::new().unwrap();
    for (name, expected) in [
        (
            "ethyl (Z)-2-[[(E)-but-2-enylidene]amino]-3-phenylprop-2-enoate",
            r"C(\C=C\C)=NC(\C(=O)OCC)=C/C1=CC=CC=C1",
        ),
        (
            "ethyl (2Z)-2-{[(1E,2E)-but-2-en-1-ylidene]amino}-3-phenylprop-2-enoate",
            r"C(\C=C\C)=N/C(/C(=O)OCC)=C\C1=CC=CC=C1",
        ),
        (
            "ethyl (2Z)-2-(ethylideneamino)-3-phenylprop-2-enoate",
            r"C(C)=N\C(\C(=O)OCC)=C/C1=CC=CC=C1",
        ),
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        assert_eq!(
            result.structure.unwrap().semantic_cxsmiles().unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn writer_does_not_mark_unspecified_conjugated_double_bonds() {
    for (input, expected) in [
        (r"C/C=C/C=NC(=C/C)/C", r"C\C=C\C=NC(=C/C)/C"),
        (r"C/C=C/C=CC(=C/C)/C", r"C\C=C\C=CC(=C/C)/C"),
    ] {
        let mut graph = Graph::default();
        let fragment = build_fragment(&mut graph, input, "", "none").unwrap();
        graph.make_hydrogens_explicit(fragment).unwrap();
        let actual = write_semantic_cxsmiles(&graph, fragment).unwrap();
        assert_eq!(actual, expected, "{input}");
        let mut roundtrip = Graph::default();
        build_fragment(&mut roundtrip, &actual, "", "none").unwrap();
        assert_eq!(
            roundtrip
                .bonds
                .iter()
                .filter(|bond| bond.stereo.is_some())
                .count(),
            2,
            "{input}"
        );
    }
}

#[test]
fn writer_can_drop_primary_references_and_multiple_redundant_marks() {
    for (input, (from, to)) in [
        // The N-C reference mark must be replaced by the other mark on C-C.
        (r"C/C=C/C=N/C(=C/C)C", (3, 4)),
        // Both ends of the central alkene have two marked single bonds.
        (
            r"C(/C(/F)=C/F)(\C(\Cl)=C/Cl)=C(/C(/Br)=C/Br)\C(\I)=C/I",
            (0, 9),
        ),
    ] {
        let mut graph = Graph::default();
        let fragment = build_fragment(&mut graph, input, "", "none").unwrap();
        graph.make_hydrogens_explicit(fragment).unwrap();
        let central = graph.bond_between(AtomId(from), AtomId(to)).unwrap();
        graph.bond_mut(central).stereo = None;
        let actual = write_semantic_cxsmiles(&graph, fragment).unwrap();
        let mut roundtrip = Graph::default();
        build_fragment(&mut roundtrip, &actual, "", "none").unwrap();
        // These acyclic fixtures retain heavy-atom order. A different outer
        // reference flips Cis/Trans, but must preserve the same configuration.
        for bond in graph.bonds.iter().filter(|bond| bond.order == 2) {
            let written = roundtrip.bond_between(bond.from, bond.to).unwrap();
            let written = &roundtrip.bond(written).stereo;
            if let Some(expected) = &bond.stereo {
                let written = written.as_ref().expect("specified bond retains stereo");
                assert_eq!(written.atom_refs[1..3], expected.atom_refs[1..3]);
                let flipped = (written.atom_refs[0] != expected.atom_refs[0])
                    ^ (written.atom_refs[3] != expected.atom_refs[3]);
                assert_eq!(written.value == expected.value, !flipped, "{actual}");
            } else {
                assert!(written.is_none(), "{actual}");
            }
        }
    }
}

#[test]
fn writer_keeps_required_marks_when_none_is_redundant() {
    // Every end atom has one heavy neighbour, so leaving C=N unspecified would
    // need an explicit [H] to carry a mark; the writer keeps today's output.
    let mut graph = Graph::default();
    let fragment = build_fragment(&mut graph, "C/C=C/C=N/C=C/C", "", "none").unwrap();
    graph.make_hydrogens_explicit(fragment).unwrap();
    let central = graph.bond_between(AtomId(3), AtomId(4)).unwrap();
    graph.bond_mut(central).stereo = None;
    assert_eq!(
        write_semantic_cxsmiles(&graph, fragment).unwrap(),
        r"C\C=C\C=N\C=C\C"
    );
}

#[test]
fn upstream_writer_examples() {
    let fixtures: &[(&str, &str, bool, &str)] = &[
        ("testRoundTrip1", "C", true, "C"),
        ("testRoundTrip2", "C#N", true, "C#N"),
        ("testRoundTrip4", "O=C=O", true, "O=C=O"),
        ("testRoundTrip5", "CCN(CC)CC", true, "CCN(CC)CC"),
        ("testRoundTrip6", "CC(=O)O", true, "CC(=O)O"),
        ("testRoundTrip7", "C1CCCCC1", true, "C1CCCCC1"),
        ("testRoundTrip8", "C1=CC=CC=C1", true, "C1=CC=CC=C1"),
        (
            "testRoundTrip9",
            "NC(Cl)(Br)C(=O)O",
            true,
            "NC(Cl)(Br)C(=O)O",
        ),
        (
            "testRoundTrip10",
            "[NH4+].[Cl-].F.[He-2]",
            true,
            "[NH4+].[Cl-].F.[He-2]",
        ),
        ("testRoundTrip12", "CCO.N=O.C#N", true, "CCO.N=O.C#N"),
        ("testOrganic1", "[S]", false, "[S]"),
        ("testOrganic2", "[S][H]", false, "[SH]"),
        ("testOrganic3", "[S]([H])[H]", false, "S"),
        ("testOrganic4", "[S]([H])([H])[H]", false, "[SH3]"),
        ("testOrganic5", "[S]([H])([H])([H])[H]", false, "[SH4]"),
        ("testOrganic6", "S(F)(F)(F)F", false, "S(F)(F)(F)F"),
        (
            "testOrganic7",
            "S([H])(F)(F)(F)(F)F",
            false,
            "S(F)(F)(F)(F)F",
        ),
        (
            "testOrganic8",
            "S([H])([H])(F)(F)(F)F",
            false,
            "[SH2](F)(F)(F)F",
        ),
        (
            "testOrganic9",
            "S(F)(F)(F)(F)(F)(F)F",
            false,
            "S(F)(F)(F)(F)(F)(F)F",
        ),
        ("testOrganic10", "[I]([H])([H])[H]", false, "[IH3]"),
        ("testCharged1", "[CH3+]", true, "[CH3+]"),
        ("testCharged2", "[Mg+2]", true, "[Mg+2]"),
        ("testCharged3", "[BH4-]", true, "[BH4-]"),
        ("testCharged4", "[O-2]", true, "[O-2]"),
        ("testIsotope", "[15NH3]", true, "[15NH3]"),
        ("testRGroup1", "[R]CC[R]", true, "*CC*"),
        ("testRGroup2", "[H][R]", true, "*[H]"),
        (
            "testRingOpeningsGreaterThan10",
            "C12=C3C4=C5C6=C1C7=C8C9=C1C%10=C%11C(=C29)C3=C2C3=C4C4=C5C5=C9C6=C7C6=C7C8=C1C1=C8C%10=C%10C%11=C2C2=C3C3=C4C4=C5C5=C%11C%12=C(C6=C95)C7=C1C1=C%12C5=C%11C4=C3C3=C5C(=C81)C%10=C23",
            true,
            "C12=C3C4=C5C6=C1C1=C7C8=C9C%10=C%11C(=C28)C3=C3C2=C4C4=C5C5=C8C6=C1C1=C6C7=C9C9=C7C%10=C%10C%11=C3C3=C2C2=C4C4=C5C5=C%11C%12=C(C1=C85)C6=C9C9=C%12C%12=C%11C4=C2C2=C%12C(=C79)C%10=C32",
        ),
        (
            "testHydrogenNotBondedToAnyNonHydrogen1",
            "[H-].[H+]",
            false,
            "[H-].[H+]",
        ),
        (
            "testHydrogenNotBondedToAnyNonHydrogen2",
            "[H][H]",
            false,
            "[H][H]",
        ),
        (
            "testHydrogenNotBondedToAnyNonHydrogen3",
            "[2H][H]",
            false,
            "[2H][H]",
        ),
        (
            "testHydrogenNotBondedToAnyNonHydrogen4",
            "[H]B1[H]B([H])[H]1",
            false,
            "B1[H]B[H]1",
        ),
        (
            "testTetrahedralChirality1",
            "N[C@@H](F)C",
            true,
            "N[C@@H](F)C",
        ),
        (
            "testTetrahedralChirality2",
            "N[C@H](F)C",
            true,
            "N[C@H](F)C",
        ),
        (
            "testTetrahedralChirality3",
            "C2.N1.F3.[C@@H]231",
            true,
            "C[C@H](F)N",
        ),
        (
            "testTetrahedralChirality4",
            "[C@@H]231.C2.N1.F3",
            true,
            "[C@H](C)(N)F",
        ),
        (
            "testTetrahedralChirality5",
            "[C@@H](Cl)1[C@H](C)(F).Br1",
            true,
            "[C@H](Cl)([C@H](C)F)Br",
        ),
        (
            "testTetrahedralChirality6",
            "I[C@@](Cl)(Br)F",
            true,
            "I[C@@](Cl)(Br)F",
        ),
        (
            "testTetrahedralChirality7",
            "C[S@](N)=O",
            true,
            "C[S@](N)=O",
        ),
    ];
    for &(name, input, explicit, expected) in fixtures {
        let mut graph = Graph::default();
        let fragment = build_fragment(&mut graph, input, "", "none")
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        if explicit {
            graph
                .make_hydrogens_explicit(fragment)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }
        let actual = write_semantic_cxsmiles(&graph, fragment)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let actual = actual.split(" |").next().unwrap(); // Plain upstream fixtures omit semantic labels on R.
        assert_eq!(actual, expected, "{name}, {input}");
    }
}

#[test]
fn upstream_double_bond_writer_examples() {
    let fixtures: &[(&str, &str, &[&str])] = &[
        (
            "testDoubleBondSupport1",
            "C/C=C/C",
            &["C/C=C/C", "C\\C=C\\C"],
        ),
        (
            "testDoubleBondSupport2",
            "C/C=C\\C",
            &["C/C=C\\C", "C\\C=C/C"],
        ),
        (
            "testDoubleBondSupport3",
            "C/C=C\\C=C/C",
            &["C/C=C\\C=C/C", "C\\C=C/C=C\\C"],
        ),
        (
            "testDoubleBondSupport5",
            "C/C=N\\O",
            &["C/C=N\\O", "C\\C=N/O"],
        ),
        (
            "testDoubleBondSupport6",
            "O=C(/C=C(C(O)=O)\\C=C/C(O)=O)O",
            &[
                "O=C(/C=C(/C(O)=O)\\C=C/C(O)=O)O",
                "O=C(\\C=C(\\C(O)=O)/C=C\\C(O)=O)O",
            ],
        ),
        (
            "testDoubleBondSupport8",
            "[H]/N=C(\\N)/O",
            &["[H]/N=C(\\N)/O", "[H]\\N=C(/N)\\O"],
        ),
        (
            "testDoubleBondSupport9",
            "CN/1CCC2=C(\\C(=C/C(/C(=C1)/C(=O)[O-])=C\\OC)\\C1=CC=C(C=C1)C)C=C(C(=C2)OC)OC",
            &["CN/1CCC2=C(\\C(=C/C(/C(=C1)/C(=O)[O-])=C\\OC)\\C1=CC=C(C=C1)C)C=C(C(=C2)OC)OC"],
        ),
    ];
    for &(name, input, expected) in fixtures {
        let mut graph = Graph::default();
        let fragment = build_fragment(&mut graph, input, "", "none")
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        graph
            .make_hydrogens_explicit(fragment)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let actual = write_semantic_cxsmiles(&graph, fragment)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            expected.contains(&actual.as_str()),
            "{name}: {input}: got {actual}, expected {expected:?}"
        );
    }
}
