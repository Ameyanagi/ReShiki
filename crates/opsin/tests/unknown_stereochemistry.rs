use opsin::{ParseOptions, Parser, Status, WarningKind};

#[test]
fn xi_case_survives_tokenization_in_both_directions() {
    let parser = Parser::new().unwrap();
    for name in [
        "(2Ξ,3ξ)-3-chlorobutan-2-ol",
        "(R,Ξ)-3-chlorobutan-2-ol",
        "(Ξ)-cyclooctene",
        "(ξ)-cholestane",
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
        (
            "(2R,3ξ,4S)-2,3,4-trihydroxypentanedioic acid",
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
fn wrong_xi_case_fails_or_warns_and_preserves_other_stereochemistry() {
    let parser = Parser::new().unwrap();
    let warn = ParseOptions {
        warn_rather_than_fail_on_uninterpretable_stereochemistry: true,
        ..ParseOptions::strict()
    };
    for (name, expected) in [
        ("(2ξ)-but-2-ene", "but-2-ene"),
        ("(2ξ,3R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
        // C20 is a side-chain R/S centre, not a steroid α/β position.
        ("(20ξ)-cholestane", "cholestane"),
        (
            "(2R,3Ξ,4S)-2,3,4-trihydroxypentanedioic acid",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
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
        "(5alpha,5xi)-cholestane",
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
fn ascii_xi_keeps_its_historical_meaning_on_true_centres() {
    let parser = Parser::new().unwrap();
    for allow_radicals in [false, true] {
        let options = ParseOptions {
            allow_radicals,
            ..ParseOptions::strict()
        };
        let result = parser.parse("(3xi)-threonine", &options);
        assert_eq!(result.status, Status::Success, "{}", result.message);
        assert_eq!(
            result.structure.unwrap().semantic_cxsmiles().unwrap(),
            "N[C@@H](C(O)C)C(=O)O"
        );
        let result = parser.parse("(3ξ)-threonine", &options);
        assert_eq!(result.status, Status::Failure, "{}", result.message);
        assert!(
            result.message.contains("Lowercase xi"),
            "{}",
            result.message
        );
    }
}
