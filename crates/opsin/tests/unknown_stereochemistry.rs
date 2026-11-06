use opsin::graph::StereoGroupType;
use opsin::{ParseOptions, Parser, Status, WarningKind};
use reshiki_chemistry::smiles::stereoisomers;

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
        let other_unknown = structure
            .graph
            .atom_by_locant(structure.fragment, "3")
            .unwrap();
        assert!(structure.graph.atom(unknown).parity.is_none(), "{name}");
        assert!(
            structure.graph.atom(other_unknown).parity.is_none(),
            "{name}"
        );
        assert_eq!(
            structure.semantic_cxsmiles().unwrap(),
            "ClC(C(C)O)C",
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
fn ignored_xi_preserves_the_complete_global_stereo_group() {
    let parser = Parser::new().unwrap();
    let warn = ParseOptions {
        warn_rather_than_fail_on_uninterpretable_stereochemistry: true,
        ..ParseOptions::strict()
    };
    for (qualifier, kind) in [
        ("rac", StereoGroupType::Racemic),
        ("rel", StereoGroupType::Relative),
    ] {
        for prefix in [qualifier.to_string(), format!("({qualifier})")] {
            let name = format!("{prefix}-(5alpha,20ξ)-cholestane");
            let strict = parser.parse(&name, &ParseOptions::strict());
            assert_eq!(strict.status, Status::Failure, "{name}: {}", strict.message);
            assert!(strict.structure.is_none(), "{name}");
            assert!(
                strict.message.contains("Lowercase xi"),
                "{name}: {}",
                strict.message
            );

            let result = parser.parse(&name, &warn);
            assert_eq!(result.status, Status::Warning, "{name}: {}", result.message);
            assert_eq!(result.warnings.len(), 1, "{name}");
            assert_eq!(
                result.warnings[0].kind,
                WarningKind::StereochemistryIgnored,
                "{name}"
            );
            let structure = result.structure.unwrap();
            let groups: Vec<_> = structure
                .graph
                .atoms
                .iter()
                .filter(|atom| atom.active)
                .filter_map(|atom| atom.parity.as_ref().map(|parity| parity.stereo_group))
                .collect();
            assert_eq!(groups.len(), 8, "{name}");
            assert_eq!(groups[0].kind, kind, "{name}");
            assert!(
                groups.iter().all(|group| *group == groups[0]),
                "{name}: {groups:?}"
            );
            let c20 = structure
                .graph
                .atom_by_locant(structure.fragment, "20")
                .unwrap();
            assert_eq!(
                structure
                    .graph
                    .atom(c20)
                    .parity
                    .as_ref()
                    .unwrap()
                    .stereo_group,
                groups[0],
                "{name}"
            );

            let expected = parser.parse(
                &format!("{prefix}-(5alpha)-cholestane"),
                &ParseOptions::strict(),
            );
            assert_eq!(expected.status, Status::Success, "{}", expected.message);
            let expected = expected.structure.unwrap().semantic_cxsmiles().unwrap();
            let written = structure.semantic_cxsmiles().unwrap();
            assert_eq!(written, expected, "{name}");
            let isomers = stereoisomers(&written).unwrap();
            assert_eq!(isomers.len(), 2, "{name}");
            assert_eq!(
                isomers,
                stereoisomers(
                    "CC(C)CCC[C@@H](C)[C@H]1CC[C@H]2[C@@H]3CC[C@H]4CCCC[C@]4(C)[C@H]3CC[C@]12C |r|"
                )
                .unwrap(),
                "{name}"
            );
        }
    }
}

#[test]
fn xi_removal_compacts_surviving_salt_stereo_groups() {
    let parser = Parser::new().unwrap();
    for (prefix, kind) in [
        ("rel", StereoGroupType::Relative),
        ("rac", StereoGroupType::Racemic),
    ] {
        let name = format!("{prefix}-(2R)-alaninium ({prefix})-(2Xi)-alaninate");
        let result = parser.parse(&name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        let structure = result.structure.unwrap();
        let written = structure.semantic_cxsmiles().unwrap();
        let centres: Vec<_> = structure
            .graph
            .atoms
            .iter()
            .filter(|atom| atom.active && atom.locants.iter().any(|locant| locant == "2"))
            .collect();
        assert_eq!(centres.len(), 2, "{name}");
        for centre in centres {
            let cation = structure
                .graph
                .neighbours(centre.id)
                .iter()
                .any(|&atom| structure.graph.atom(atom).charge == 1);
            if cation {
                let group = centre.parity.as_ref().unwrap().stereo_group;
                assert_eq!(group.kind, kind, "{name}");
                assert_eq!(group.number, 1, "{name}");
            } else {
                assert!(centre.parity.is_none(), "{name}: alaninate C2");
            }
        }
        assert_eq!(
            structure
                .graph
                .atoms
                .iter()
                .filter(|atom| atom.active && atom.parity.is_some())
                .count(),
            1,
            "{name}"
        );
        let expected = "C[C@H]([NH3+])C(=O)O.CC(N)C(=O)[O-] |o1:1|";
        assert_eq!(
            stereoisomers(&written).unwrap(),
            stereoisomers(expected).unwrap(),
            "{name}: {written}"
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
