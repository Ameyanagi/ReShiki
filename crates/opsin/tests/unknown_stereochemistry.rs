use opsin::{ParseOptions, Parser, Status, WarningKind};

fn canonical(smiles: &str) -> String {
    let imported = nazuchem_kernel::smiles::read(smiles, &Default::default()).unwrap();
    nazuchem_kernel::smiles::write::write(&imported.prepared.state, Default::default())
        .unwrap()
        .text
}

fn parsed(parser: &Parser, name: &str) -> opsin::Structure {
    let result = parser.parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    result.structure.unwrap()
}

#[test]
fn coupled_pseudoasymmetric_xi_has_no_configured_centres() {
    let parser = Parser::new().unwrap();
    for (name, smiles) in [
        ("(1ξ,4ξ)-cyclohexane-1,4-diol", "OC1CCC(O)CC1"),
        ("(1xi,4xi)-cyclohexane-1,4-diol", "OC1CCC(O)CC1"),
        ("(4ξ,1ξ)-cyclohexane-1,4-diol", "OC1CCC(O)CC1"),
        ("(1ξ,4ξ)-cyclohexane-1,4-diamine", "NC1CCC(N)CC1"),
        ("(2xi)-butan-2-ol", "CC(CC)O"),
    ] {
        let structure = parsed(&parser, name);
        assert_eq!(
            canonical(&structure.semantic_cxsmiles().unwrap()),
            canonical(smiles),
            "{name}"
        );
        assert!(
            structure
                .graph
                .atoms
                .iter()
                .all(|atom| atom.parity.is_none()),
            "{name}"
        );
    }
}

#[test]
fn coupled_xi_rejects_nonstereogenic_wrong_case_and_mixed_descriptors() {
    let parser = Parser::new().unwrap();
    for name in [
        "(1ξ)-cyclohexan-1-ol",
        "(1xi)-cyclohexan-1-ol",
        "(1xi,2xi)-ethane-1,2-diol",
        "(1Ξ,4Ξ)-cyclohexane-1,4-diol",
        "(2ξ)-2-aminopropanoic acid",
        "(1s,4ξ)-cyclohexane-1,4-diol",
        "(1ξ,4s)-cyclohexane-1,4-diol",
        "(1s,4xi)-cyclohexane-1,4-diol",
        "(1xi,4s)-cyclohexane-1,4-diol",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
        assert!(result.structure.is_none(), "{name}");
    }
}

#[test]
fn unlocanted_xi_keeps_the_ordinary_stereocentre_search() {
    // Coupled ring centres are reachable only through a locanted lowercase ξ.
    let parser = Parser::new().unwrap();
    let structure = parsed(&parser, "(Ξ)-4-(2-hydroxypropyl)cyclohexan-1-ol");
    assert_eq!(
        canonical(&structure.semantic_cxsmiles().unwrap()),
        canonical("CC(O)CC1CCC(O)CC1")
    );
    assert!(
        structure
            .graph
            .atoms
            .iter()
            .all(|atom| atom.parity.is_none())
    );
    let result = parser.parse(
        "(ξ)-4-(2-hydroxypropyl)cyclohexan-1-ol",
        &ParseOptions::strict(),
    );
    assert_eq!(result.status, Status::Failure, "{}", result.message);
}

#[test]
fn coupled_lowercase_rs_controls_keep_their_distinct_configurations() {
    let parser = Parser::new().unwrap();
    let mut identities = Vec::new();
    for (name, label) in [
        ("(1s,4s)-cyclohexane-1,4-diol", "s"),
        ("(1r,4r)-cyclohexane-1,4-diol", "r"),
    ] {
        let structure = parsed(&parser, name);
        for locant in ["1", "4"] {
            let atom = structure
                .graph
                .atom_by_locant(structure.fragment, locant)
                .unwrap();
            let parity = structure.graph.atom(atom).parity.as_ref().unwrap();
            assert_eq!(
                parity.stereo_group.kind,
                opsin::graph::StereoGroupType::Absolute,
                "{name}"
            );
        }
        let smiles = structure.semantic_cxsmiles().unwrap();
        let native = nazuchem_kernel::smiles::read(&smiles, &Default::default()).unwrap();
        let labels = nazuchem_kernel::stereo::cip::label::assign(
            &native.prepared.state,
            &Default::default(),
        )
        .unwrap();
        let labels: Vec<_> = labels
            .state
            .properties
            .atoms
            .iter()
            .filter_map(|atom| atom.cip_code.as_deref())
            .collect();
        assert_eq!(labels, [label, label], "{name}");
        identities.push(canonical(&smiles));
    }
    assert_ne!(identities[0], identities[1]);
}

#[test]
fn xi_case_survives_tokenization_in_both_directions() {
    let parser = Parser::new().unwrap();
    for name in [
        "(2Ξ,3ξ)-3-chlorobutan-2-ol",
        "(R,Ξ)-3-chlorobutan-2-ol",
        "(Ξ)-cyclooctene",
        "(ξ)-cholestane",
        "(2Xi,3xI)-3-chlorobutan-2-ol",
        "(2XI,3xi)-3-chlorobutan-2-ol",
    ] {
        let normalized = opsin::preprocess::preprocess(name).unwrap();
        for result in [
            parser.tokenize(&normalized, false).unwrap(),
            parser.tokenize_reverse(&normalized, false).unwrap(),
        ] {
            assert!(result.is_successful(), "{name}: {result:?}");
            for word in result.words {
                for alternative in word.alternatives {
                    assert_eq!(alternative.text(), word.word, "{name}");
                }
            }
        }
    }
}

#[test]
fn unknown_stereochemistry_preserves_other_defined_centres() {
    let parser = Parser::new().unwrap();
    for (name, expected) in [
        ("(2Ξ)-but-2-ene", "but-2-ene"),
        ("(2xi)-but-2-ene", "but-2-ene"),
        ("(2Ξ,3R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        ("(2xi,3R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        ("(2xi,R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        ("(2Ξ,R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        ("(2xi,2R,3R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        (
            "(2R,3ξ,4S)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
        ),
        (
            "(3ξ,2R,4S)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
        ),
        (
            "(2R,4S,ξ)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
        ),
        ("(Ξ)-cyclooctene", "cyclooctene"),
        ("(Ξ)-cyclononene", "cyclononene"),
        ("(Ξ)-bromochlorofluoromethane", "bromochlorofluoromethane"),
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        let expected = parser.parse(expected, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        assert_eq!(expected.status, Status::Success, "{}", expected.message);
        assert_eq!(
            result.structure.unwrap().semantic_cxsmiles().unwrap(),
            expected.structure.unwrap().semantic_cxsmiles().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn locanted_xi_reserves_its_centre_before_global_stereochemistry() {
    let parser = Parser::new().unwrap();
    for name in [
        "(rac)-(2xi)-3-chlorobutan-2-ol",
        "(rac)-(2Ξ)-3-chlorobutan-2-ol",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        let structure = result.structure.unwrap();
        let unknown = structure
            .graph
            .atom_by_locant(structure.fragment, "2")
            .unwrap();
        let racemic = structure
            .graph
            .atom_by_locant(structure.fragment, "3")
            .unwrap();
        assert!(structure.graph.atom(unknown).parity.is_none(), "{name}");
        assert_eq!(
            structure
                .graph
                .atom(racemic)
                .parity
                .as_ref()
                .unwrap()
                .stereo_group
                .kind,
            opsin::graph::StereoGroupType::Racemic,
            "{name}"
        );
    }
}

#[test]
fn uppercase_xi_accepts_diastereomorphic_ligands_and_preserves_the_partial_graph() {
    let parser = Parser::new().unwrap();
    for name in [
        "(2R,3R,4Ξ,5S,6R)-2,3,4,5,6-pentahydroxyheptanedioic acid",
        "(4Ξ,2R,3R,5S,6R)-2,3,4,5,6-pentahydroxyheptanedioic acid",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        let structure = result.structure.unwrap();
        assert_eq!(
            structure.semantic_cxsmiles().unwrap(),
            "O[C@@H](C(=O)O)[C@@H](C([C@@H]([C@H](C(=O)O)O)O)O)O",
            "{name}"
        );
        let centre = structure
            .graph
            .atom_by_locant(structure.fragment, "4")
            .unwrap();
        assert!(structure.graph.atom(centre).parity.is_none(), "{name}");
    }
}

#[test]
fn wrong_xi_case_fails_or_warns_and_preserves_other_stereochemistry() {
    let parser = Parser::new().unwrap();
    let warn = ParseOptions {
        warn_rather_than_fail_on_uninterpretable_stereochemistry: true,
        ..ParseOptions::strict()
    };
    for (name, expected) in [
        ("(2ξ)-but-2-ene", "but-2-ene"),
        ("(2ξ,3R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        (
            "(2R,3ξ)-2,3,4-trihydroxypentanedioic acid",
            "(2R)-2,3,4-trihydroxypentanedioic acid",
        ),
        // C20 is a side-chain R/S centre, not a steroid α/β position.
        ("(20ξ)-cholestane", "cholestane"),
        (
            "(2R,3Ξ,4S)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
        ),
        (
            "(3Ξ,2R,4S)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
        ),
        (
            "(2R,3ξ,4R)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4R)-2,3,4-trihydroxypentanedioic acid",
        ),
        (
            "(3Ξ,2R,4R)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4R)-2,3,4-trihydroxypentanedioic acid",
        ),
        // The undefined stereocentre in the unique hydroxyethyl ligand
        // cannot distinguish the identical R/R ligands at C3.
        (
            "(2R,3ξ,4R)-2,4-dihydroxy-3-(1-hydroxyethyl)pentanedioic acid",
            "(2R,4R)-2,4-dihydroxy-3-(1-hydroxyethyl)pentanedioic acid",
        ),
        (
            "(2R,3Ξ,4R)-2,4-dihydroxy-3-(1-hydroxyethyl)pentanedioic acid",
            "(2R,4R)-2,4-dihydroxy-3-(1-hydroxyethyl)pentanedioic acid",
        ),
        (
            "(2R,3R,4ξ,5S,6R)-2,3,4,5,6-pentahydroxyheptanedioic acid",
            "(2R,3R,5S,6R)-2,3,4,5,6-pentahydroxyheptanedioic acid",
        ),
        (
            "(4ξ,2R,3R,5S,6R)-2,3,4,5,6-pentahydroxyheptanedioic acid",
            "(2R,3R,5S,6R)-2,3,4,5,6-pentahydroxyheptanedioic acid",
        ),
    ] {
        let failed = parser.parse(name, &ParseOptions::strict());
        assert_eq!(failed.status, Status::Failure, "{name}: {}", failed.message);
        assert!(failed.structure.is_none(), "{name}");
        let warned = parser.parse(name, &warn);
        assert_eq!(warned.status, Status::Warning, "{name}: {}", warned.message);
        assert_eq!(warned.warnings.len(), 1, "{name}");
        assert_eq!(
            warned.warnings[0].kind,
            WarningKind::StereochemistryIgnored,
            "{name}"
        );
        let expected = parser.parse(expected, &ParseOptions::strict());
        assert_eq!(expected.status, Status::Success, "{}", expected.message);
        assert_eq!(
            warned.structure.unwrap().semantic_cxsmiles().unwrap(),
            expected.structure.unwrap().semantic_cxsmiles().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn either_xi_case_is_allowed_when_ligand_stereochemistry_is_undefined() {
    let parser = Parser::new().unwrap();
    for name in [
        "(3ξ)-2,3,4-trihydroxypentanedioic acid",
        "(3Ξ)-2,3,4-trihydroxypentanedioic acid",
        "(3Ξ,2R)-2,3,4-trihydroxypentanedioic acid",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        let structure = result.structure.unwrap();
        let centre = structure
            .graph
            .atom_by_locant(structure.fragment, "3")
            .unwrap();
        assert!(structure.graph.atom(centre).parity.is_none(), "{name}");
    }
}

#[test]
fn all_xi_targets_are_unspecified_before_case_validation() {
    let parser = Parser::new().unwrap();
    for name in [
        "(2R,3Ξ,4S,2Ξ,4Ξ)-2,3,4-trihydroxypentanedioic acid",
        "(2R,3ξ,4R,2Ξ,4Ξ)-2,3,4-trihydroxypentanedioic acid",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        assert!(
            result
                .structure
                .unwrap()
                .graph
                .atoms
                .iter()
                .all(|atom| atom.parity.is_none()),
            "{name}"
        );
    }
}

#[test]
fn xi_is_applied_after_lowercase_descriptor_resolution() {
    let parser = Parser::new().unwrap();
    let expected = parser.parse(
        "(1R,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
        &ParseOptions::strict(),
    );
    assert_eq!(expected.status, Status::Success, "{}", expected.message);
    let expected = expected.structure.unwrap().semantic_cxsmiles().unwrap();
    for name in [
        "(3ξ,1R,3s,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
        "(1R,3s,5S,3ξ)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
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
fn xi_does_not_allow_repeated_defined_descriptors() {
    let parser = Parser::new().unwrap();
    for name in [
        "(2xi,2R,2S)-3-chlorobutan-2-ol",
        "(2R,2xi,2S)-3-chlorobutan-2-ol",
        "(2xi,2R,2xi,2S)-3-chlorobutan-2-ol",
        "(3ξ,1R,3s,3s,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
    }
}

#[test]
fn unlocanted_absolute_stereochemistry_matches_its_locanted_structure() {
    let parser = Parser::new().unwrap();
    for (name, locanted) in [
        ("(E)-cyclooctene", "(1E)-cyclooctene"),
        ("(Z)-cyclooctene", "(1Z)-cyclooctene"),
        (
            "(R)-bromochlorofluoromethane",
            "(1R)-bromochlorofluoromethane",
        ),
        (
            "(S)-bromochlorofluoromethane",
            "(1S)-bromochlorofluoromethane",
        ),
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        let locanted = parser.parse(locanted, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        assert_eq!(locanted.status, Status::Success, "{}", locanted.message);
        let structure = result.structure.unwrap();
        assert!(
            structure
                .graph
                .fragment(structure.fragment)
                .atoms
                .iter()
                .any(|&a| structure.graph.atom(a).parity.is_some())
                || structure
                    .graph
                    .fragment(structure.fragment)
                    .bonds
                    .iter()
                    .any(|&b| structure.graph.bond(b).stereo.is_some()),
            "{name}"
        );
        assert_eq!(
            structure.semantic_cxsmiles().unwrap(),
            locanted.structure.unwrap().semantic_cxsmiles().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn steroid_xi_keeps_its_historical_alpha_beta_meaning() {
    let parser = Parser::new().unwrap();
    let parent = parser.parse("(5alpha)-cholestane", &ParseOptions::strict());
    assert_eq!(parent.status, Status::Success, "{}", parent.message);
    let parent = parent.structure.unwrap();
    let centre = parent.graph.atom_by_locant(parent.fragment, "5").unwrap();
    assert!(parent.graph.atom(centre).parity.is_some());
    let mut expected = parent;
    expected.graph.atom_mut(centre).parity = None;
    for name in [
        "(5xi)-cholestane",
        "(5ξ)-cholestane",
        "5xi-cholestane",
        "5ξ-cholestane",
        "5XIcholestane",
        "(5alpha,5xi)-cholestane",
        "(5xi,5alpha)-cholestane",
        "(5alpha,5ξ)-cholestane",
    ] {
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        assert_eq!(
            result.structure.unwrap().semantic_cxsmiles().unwrap(),
            expected.semantic_cxsmiles().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn repeated_xi_warning_targets_preserve_or_clear_inherited_stereochemistry() {
    let parser = Parser::new().unwrap();
    let options = ParseOptions {
        warn_rather_than_fail_on_uninterpretable_stereochemistry: true,
        ..ParseOptions::strict()
    };
    let parent = parser
        .parse("cholestane", &ParseOptions::strict())
        .structure
        .unwrap();
    let original = parent.semantic_cxsmiles().unwrap();
    let centre = parent.graph.atom_by_locant(parent.fragment, "20").unwrap();
    let mut cleared = parent;
    cleared.graph.atom_mut(centre).parity = None;
    let cleared = cleared.semantic_cxsmiles().unwrap();
    for (name, warning_count, expected) in [
        ("(20ξ,20xi)-cholestane", 1, &cleared),
        ("(20xi,20ξ)-cholestane", 1, &cleared),
        ("(20ξ,20ξ)-cholestane", 2, &original),
    ] {
        let result = parser.parse(name, &options);
        assert_eq!(result.status, Status::Warning, "{name}: {}", result.message);
        assert_eq!(result.warnings.len(), warning_count, "{name}");
        assert!(
            result
                .warnings
                .iter()
                .all(|warning| warning.kind == WarningKind::StereochemistryIgnored),
            "{name}"
        );
        assert_eq!(
            &result.structure.unwrap().semantic_cxsmiles().unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn rejected_xi_descriptors_preserve_inherited_stereochemistry() {
    let parser = Parser::new().unwrap();
    let options = ParseOptions {
        warn_rather_than_fail_on_uninterpretable_stereochemistry: true,
        ..ParseOptions::strict()
    };
    let expected = parser.parse("ribitol", &ParseOptions::strict());
    assert_eq!(expected.status, Status::Success, "{}", expected.message);
    let expected = expected.structure.unwrap().semantic_cxsmiles().unwrap();
    for name in ["(2ξ,3Ξ)-ribitol", "(3Ξ,2ξ)-ribitol"] {
        let failed = parser.parse(name, &ParseOptions::strict());
        assert_eq!(failed.status, Status::Failure, "{name}: {}", failed.message);
        let result = parser.parse(name, &options);
        assert_eq!(result.status, Status::Warning, "{name}: {}", result.message);
        assert_eq!(result.warnings.len(), 2, "{name}");
        assert!(
            result
                .warnings
                .iter()
                .all(|warning| warning.kind == WarningKind::StereochemistryIgnored),
            "{name}"
        );
        assert_eq!(
            result.structure.unwrap().semantic_cxsmiles().unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn ascii_xi_keeps_its_historical_meaning_on_true_centres() {
    let parser = Parser::new().unwrap();
    for allow_radicals in [false, true] {
        let options = ParseOptions {
            allow_radicals,
            ..ParseOptions::strict()
        };
        for spelling in ["xi", "Xi", "xI", "XI"] {
            for (name, expected) in [
                (format!("(3{spelling})-threonine"), "N[C@@H](C(O)C)C(=O)O"),
                (format!("(2{spelling})-but-2-ene"), "CC=CC"),
                (
                    format!("(2R,3{spelling},4S)-2,3,4-trihydroxypentanedioic acid"),
                    "O[C@@H](C(=O)O)C([C@@H](C(=O)O)O)O",
                ),
                (
                    format!("(2R,3{spelling},4R)-2,3,4-trihydroxypentanedioic acid"),
                    "O[C@@H](C(=O)O)C([C@H](C(=O)O)O)O",
                ),
            ] {
                let result = parser.parse(&name, &options);
                assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
                assert_eq!(
                    result.structure.unwrap().semantic_cxsmiles().unwrap(),
                    expected,
                    "{name}"
                );
            }
        }
        let result = parser.parse("(3ξ)-threonine", &options);
        assert_eq!(result.status, Status::Failure, "{}", result.message);
        assert!(
            result.message.contains("Lowercase xi"),
            "{}",
            result.message
        );
    }
}
