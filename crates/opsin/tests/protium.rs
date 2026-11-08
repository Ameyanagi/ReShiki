use opsin::{
    ParseOptions, Parser, Status, WarningKind,
    graph::{Element, Graph},
    smiles::{build_fragment, write_semantic_cxsmiles},
    stereo_analyser::StereoCentre,
    stereochemistry_handler::{
        apply_stereo_chemistry_to_stereo_centre, check_equivalency_of_atom_refs_and_parity,
    },
};

fn canonical(text: &str) -> String {
    let imported = nazuchem_kernel::smiles::read(text, &Default::default()).unwrap();
    nazuchem_kernel::smiles::write::write(&imported.prepared.state, Default::default())
        .unwrap()
        .text
}

#[test]
fn isotope_names_preserve_every_explicit_hydrogen_label() {
    let parser = Parser::new().unwrap();
    for (name, expected, mass, count) in [
        ("(1-1H1)penta-1,2,3-triene", "[1H]C=C=C=CC", 1, 1),
        ("(1,1-1H2)penta-1,2,3-triene", "[1H]C([1H])=C=C=CC", 1, 2),
        ("(2-1H1)ethan-1-ol", "[1H]CCO", 1, 1),
        ("(O-1H1)methanol", "[1H]OC", 1, 1),
        ("(1-2H1)penta-1,2,3-triene", "[2H]C=C=C=CC", 2, 1),
        ("(2-2H1)ethan-1-ol", "[2H]CCO", 2, 1),
        ("(2-3H1)ethan-1-ol", "[3H]CCO", 3, 1),
        ("(1H4)methane", "[1H]C([1H])([1H])[1H]", 1, 4),
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        let structure = result.structure.unwrap();
        let written = structure.semantic_cxsmiles().unwrap();
        assert_eq!(
            canonical(&written),
            canonical(expected),
            "{name}: {written}"
        );
        let mut roundtrip = Graph::default();
        build_fragment(&mut roundtrip, &written, "", "none").unwrap();
        for graph in [&structure.graph, &roundtrip] {
            assert_eq!(
                graph
                    .atoms
                    .iter()
                    .filter(|atom| atom.active
                        && atom.element == Element::H
                        && atom.isotope == Some(mass))
                    .count(),
                count,
                "{name}: {written}"
            );
            assert!(
                graph.atoms.iter().all(|atom| atom.parity.is_none()),
                "{name}"
            );
            assert!(
                graph.bonds.iter().all(|bond| bond.stereo.is_none()),
                "{name}"
            );
        }
    }
}

#[test]
fn protium_and_hydrogen_cannot_define_an_ethanol_stereocentre() {
    let parser = Parser::new().unwrap();
    for name in ["(1R)-(1-1H1)ethan-1-ol", "(1S)-(1-1H1)ethan-1-ol"] {
        for warn in [false, true] {
            let options = ParseOptions {
                warn_rather_than_fail_on_uninterpretable_stereochemistry: warn,
                ..ParseOptions::strict()
            };
            let result = parser.parse(name, &options);
            assert_eq!(
                result.status,
                if warn {
                    Status::Warning
                } else {
                    Status::Failure
                },
                "{name}: {}",
                result.message
            );
            assert!(
                result
                    .message
                    .contains("Failed to assign CIP stereochemistry")
            );
            if warn {
                assert_eq!(result.warnings.len(), 1);
                assert_eq!(result.warnings[0].kind, WarningKind::StereochemistryIgnored);
                let structure = result.structure.unwrap();
                assert!(
                    structure
                        .graph
                        .atoms
                        .iter()
                        .all(|atom| atom.parity.is_none()),
                    "{name}: {}",
                    structure.semantic_cxsmiles().unwrap()
                );
                assert_eq!(
                    canonical(&structure.semantic_cxsmiles().unwrap()),
                    canonical("[1H]C(C)O"),
                    "{name}"
                );
            } else {
                assert!(result.structure.is_none(), "{name}");
            }
        }
    }
}

fn assert_configuration(graph: &Graph, descriptor: &str) {
    let configured: Vec<_> = graph
        .atoms
        .iter()
        .filter(|atom| atom.parity.is_some())
        .collect();
    assert_eq!(configured.len(), 1);
    let atom = configured[0];
    let actual = atom.parity.as_ref().unwrap();
    let mut expected = graph.clone();
    apply_stereo_chemistry_to_stereo_centre(
        &mut expected,
        StereoCentre {
            atom: atom.id,
            true_stereo_centre: true,
        },
        descriptor,
    )
    .unwrap();
    let expected = expected.atom(atom.id).parity.as_ref().unwrap();
    assert!(check_equivalency_of_atom_refs_and_parity(
        &actual.atom_refs,
        actual.parity,
        &expected.atom_refs,
        expected.parity,
    ));
}

#[test]
fn genuine_protium_stereocentres_preserve_configuration_when_serialized() {
    for (input, descriptor) in [
        ("[1H][C@](F)(Cl)Br", "S"),
        ("[1H][C@@](F)(Cl)Br", "R"),
        ("[1H][C@]([2H])(F)Cl", "S"),
        ("[1H][C@@]([2H])(F)Cl", "R"),
    ] {
        let mut graph = Graph::default();
        let fragment = build_fragment(&mut graph, input, "", "none").unwrap();
        graph.make_hydrogens_explicit(fragment).unwrap();
        assert_configuration(&graph, descriptor);
        let written = write_semantic_cxsmiles(&graph, fragment).unwrap();
        assert!(written.contains("[1H]"), "{input}: {written}");
        assert_eq!(canonical(&written), canonical(input), "{input}");
        let mut roundtrip = Graph::default();
        let fragment = build_fragment(&mut roundtrip, &written, "", "none").unwrap();
        roundtrip.make_hydrogens_explicit(fragment).unwrap();
        assert_configuration(&roundtrip, descriptor);
    }
}

#[test]
fn genuine_protium_alkenes_preserve_configuration_when_serialized() {
    for input in ["[1H]/C(F)=C(Cl)/Br", r"[1H]/C(F)=C(Cl)\Br"] {
        let mut graph = Graph::default();
        let fragment = build_fragment(&mut graph, input, "", "none").unwrap();
        let written = write_semantic_cxsmiles(&graph, fragment).unwrap();
        assert!(written.contains("[1H]"), "{input}: {written}");
        assert_eq!(canonical(&written), canonical(input), "{input}");
        let mut roundtrip = Graph::default();
        let fragment = build_fragment(&mut roundtrip, &written, "", "none").unwrap();
        assert_eq!(
            canonical(&write_semantic_cxsmiles(&roundtrip, fragment).unwrap()),
            canonical(input),
            "{input}"
        );
    }
}
