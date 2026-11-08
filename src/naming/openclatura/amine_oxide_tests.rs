// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming/openclatura/amine_oxide_tests.rs | 327d32fed63a5b3df15b7b8dc5ab84a7c36e0865
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

use super::name_smiles;
use crate::naming::{NameReason, NameStatus, canonical_smiles};

type Case = (&'static str, &'static str, &'static str, NameStatus);

// P-62.5 / P-74.2.1.2: retain the oxide nitrogen's identity while naming the
// neutral amine or imine, then cite the functional class at that nitrogen.
const FUNCTIONAL_CLASS_CASES: &[Case] = &[
    (
        "C1",
        "[O-][n+]1ccccc1",
        "pyridine 1-oxide",
        NameStatus::Preferred,
    ),
    (
        "C2",
        "C[N+]1([O-])CCCCC1",
        "1-methylpiperidine 1-oxide",
        NameStatus::Preferred,
    ),
    (
        "C3",
        "C[N+](C)(C)[O-]",
        "N,N-dimethylmethanamine N-oxide",
        NameStatus::Preferred,
    ),
    (
        "C4",
        "CC[N+](CC)([O-])CC",
        "N,N-diethylethanamine N-oxide",
        NameStatus::Preferred,
    ),
    (
        "C5",
        "[O-][n+]1ccc(C)cc1",
        "4-methylpyridine 1-oxide",
        NameStatus::Preferred,
    ),
    (
        "C6",
        "[O-][N+]1(C)CCCC1",
        "1-methylpyrrolidine 1-oxide",
        NameStatus::Preferred,
    ),
    (
        "C7",
        "C1CC[N+]2([O-])CCC1CC2",
        "1-azabicyclo[3.2.2]nonane 1-oxide",
        NameStatus::Preferred,
    ),
    (
        "C8",
        "C=[N+](C)[O-]",
        "N-methylmethanimine N-oxide",
        NameStatus::Preferred,
    ),
    (
        "C8-Cl",
        "C=[N+](Cl)[O-]",
        "N-chloromethanimine N-oxide",
        NameStatus::Preferred,
    ),
    (
        "C9",
        "C[N+]1([O-])CCOCC1",
        "4-methylmorpholine 4-oxide",
        NameStatus::Unverified(NameReason::AmineChalcogenide),
    ),
    (
        "C10",
        "[O-][n+]1ccncc1",
        "pyrazine 1-oxide",
        NameStatus::Unverified(NameReason::AmineChalcogenide),
    ),
    (
        "C11",
        "[S-][n+]1ccccc1",
        "pyridine 1-sulfide",
        NameStatus::Preferred,
    ),
];

const CHALCOGEN_ANALOGUES: &[Case] = &[
    (
        "selenide",
        "[Se-][n+]1ccccc1",
        "pyridine 1-selenide",
        NameStatus::Preferred,
    ),
    (
        "telluride",
        "[Te-][n+]1ccccc1",
        "pyridine 1-telluride",
        NameStatus::Preferred,
    ),
    (
        "amine sulfide",
        "CC[N+](C)(C)[S-]",
        "N,N-dimethylethanamine N-sulfide",
        NameStatus::Preferred,
    ),
];

fn name(smiles: &str) -> Result<super::Name, String> {
    // Match the CLI boundary for the audit matrix; the atom-order test below
    // also exercises the naming pipeline directly without canonicalizing first.
    name_smiles(&canonical_smiles(smiles)?, &Default::default())
}

fn assert_case((row, smiles, expected, status): Case) {
    let actual = name(smiles).unwrap_or_else(|error| panic!("{row}: {smiles}: {error}"));
    assert_eq!(
        (actual.text.as_str(), actual.status),
        (expected, status),
        "{row}: {smiles}"
    );
}

#[test]
fn amine_oxide_functional_class_matrix() {
    for &case in FUNCTIONAL_CLASS_CASES {
        assert_case(case);
    }
}

#[test]
fn amine_chalcogen_analogues_use_locanted_functional_class_names() {
    for &case in CHALCOGEN_ANALOGUES {
        assert_case(case);
    }
}

#[test]
fn amine_oxide_zwitterionic_fallbacks_are_not_preferred() {
    // A senior suffix elsewhere, an oxide in a substituent, N-bound hydrogen,
    // an adjacent ring nitrogen, and multiple sites remain coverage limits.
    // Their existing zwitterionic text is known not to be the PIN (P-62.5).
    for (row, smiles, expected) in [
        (
            "C12",
            "[O-][n+]1ccccc1C(=O)O",
            "1-oxidopyridin-1-ium-2-carboxylic acid",
        ),
        (
            "C13",
            "CC(=O)N1CC[N+](C)([O-])CC1",
            "1-(4-methyl-4-oxidopiperazin-4-ium-1-yl)ethan-1-one",
        ),
        ("C14", "[O-][n+]1ccc(N)cc1", "1-oxidopyridin-1-ium-4-amine"),
        ("N-H", "[O-][NH2+]CC", "N-oxidoethanaminium"),
        (
            "secondary N-H",
            "C[NH+](C)[O-]",
            "N-methyl-N-oxidomethanaminium",
        ),
        ("N-N ring", "[O-][n+]1ncccc1", "1-oxidopyridazin-1-ium"),
        (
            "two sites",
            "C[N+]1([O-])CC[N+](C)([O-])CC1",
            "1,4-dimethyl-1,4-dioxidopiperazine-1,4-diium",
        ),
        (
            "ammonio prefix",
            "C[N+](C)([O-])CC(=O)[O-]",
            "[dimethyl(oxido)ammonio]acetate",
        ),
        (
            "methylidene ammonio prefix",
            "C=[N+]([O-])CC(=O)[O-]",
            "[methylidene(oxido)ammonio]acetate",
        ),
        (
            "ammonio prefix in substituent",
            "C[N+]([O-])(C)Cc1ccc(C(=O)[O-])cc1",
            "4-{[dimethyl(oxido)ammonio]methyl}benzoate",
        ),
    ] {
        let actual = name(smiles).unwrap_or_else(|error| panic!("{row}: {smiles}: {error}"));
        assert_eq!(actual.text, expected, "{row}: {smiles}");
        assert_eq!(
            actual.status,
            NameStatus::NotPreferred(NameReason::AmineOxideZwitterion),
            "{row}: {smiles}"
        );
    }
}

#[test]
fn amine_oxide_functional_class_keeps_neutral_assessment() {
    for case in [
        (
            "linear phane",
            "C[N+](C)([O-])c1ccc(Oc2ccc(Oc3ccc(Oc4ccccc4)cc3)cc2)cc1",
            "N,N-dimethyl-4-[4-(4-phenoxyphenoxy)phenoxy]aniline N-oxide",
            NameStatus::NotPreferred(NameReason::LinearPhane),
        ),
        (
            "unaudited parent",
            "C[N+]1([O-])CCOCCOCCOCCOCCOCC1",
            "16-methyl-1,4,7,10,13-pentaoxa-16-azacyclooctadecane 16-oxide",
            NameStatus::Unverified(NameReason::UnauditedParent),
        ),
    ] {
        assert_case(case);
    }
}

#[test]
fn amine_oxide_keeps_source_stereo_alternatives() {
    for (smiles, primary, expected, reason) in [
        (
            "C[N+](C)([O-])CC[C@H]1C[C@@H]2CC[C@H]1C2",
            "2-[(1S,2R,4R)-bicyclo[2.2.1]heptan-2-yl]-N,N-dimethyl-N-oxidoethan-1-aminium",
            "2-(bicyclo[2.2.1]heptan-2-exo-yl)-N,N-dimethyl-N-oxidoethan-1-aminium",
            NameReason::EndoExoStereo,
        ),
        (
            "C[N+](C)([O-])[C@H]1C[C@@H](C)C1",
            "(1s,3s)-N,N,3-trimethyl-N-oxidocyclobutan-1-aminium",
            "cis-N,N,3-trimethyl-N-oxidocyclobutan-1-aminium",
            NameReason::RelativeRingStereo,
        ),
    ] {
        let mol = super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        assert!(super::amine_oxides::name(&mol, &atoms).unwrap().is_none());
        assert!(!mol.has_endo_exo_alternative());
        assert!(!mol.has_relative_ring_alternative());
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedPath)
        );

        let actual = name(smiles).unwrap();
        assert_eq!(actual.text, primary, "{smiles}");
        assert_eq!(
            actual.status,
            NameStatus::NotPreferred(NameReason::AmineOxideZwitterion)
        );
        assert_eq!(actual.alternatives.len(), 1);
        let alternative = &actual.alternatives[0];
        assert_eq!(alternative.name, expected, "{smiles}");
        assert_eq!(alternative.status, NameStatus::NotPreferred(reason));
        for (text, status) in [
            (&actual.text, actual.status),
            (&alternative.name, alternative.status),
        ] {
            let request = crate::naming::local::verification_request(text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::verification_result(text, &reply).unwrap();
            crate::naming::reverse::verify_alternative_identity(smiles, &parsed.smiles, status)
                .unwrap_or_else(|error| panic!("{text}: {error}"));
        }
    }
}

#[test]
fn amine_oxide_neutral_candidates_restore_source_status() {
    // A detached neutral result has its own assessment. Neither a successful
    // neutral proof nor a declined parent choice may certify the source graph.
    for (row, smiles, expected, status, accepted) in [
        (
            "C1",
            "[O-][n+]1ccccc1",
            "pyridine 1-oxide",
            NameStatus::Preferred,
            true,
        ),
        (
            "C12",
            "[O-][n+]1ccccc1C(=O)O",
            "1-oxidopyridin-1-ium-2-carboxylic acid",
            NameStatus::NotPreferred(NameReason::AmineOxideZwitterion),
            false,
        ),
        (
            "C8-Cl",
            "C=[N+](Cl)[O-]",
            "N-chloromethanimine N-oxide",
            NameStatus::Preferred,
            true,
        ),
        (
            "C13",
            "CC(=O)N1CC[N+](C)([O-])CC1",
            "1-(4-methyl-4-oxidopiperazin-4-ium-1-yl)ethan-1-one",
            NameStatus::NotPreferred(NameReason::AmineOxideZwitterion),
            false,
        ),
    ] {
        let canonical = canonical_smiles(smiles).unwrap();
        let mol = super::graph_io::read_smiles(&canonical, &Default::default()).unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let initial = NameStatus::Unverified(NameReason::UnauditedPath);
        assert_eq!(mol.name_status(), initial, "{row}: before candidate");
        let candidate = super::amine_oxides::name(&mol, &atoms).unwrap();
        assert_eq!(candidate.is_some(), accepted, "{row}: candidate result");
        if let Some(candidate) = candidate {
            assert_eq!(
                (candidate.text.as_str(), candidate.status),
                (expected, status),
                "{row}: detached candidate"
            );
        }
        assert_eq!(mol.name_status(), initial, "{row}: after candidate");

        let ordinary = name_smiles(&canonical, &Default::default()).unwrap();
        assert_eq!(
            (ordinary.text.as_str(), ordinary.status),
            (expected, status),
            "{row}: complete naming pipeline"
        );
    }
}

#[test]
fn amine_oxides_preserve_other_charge_and_retained_name_families() {
    for case in [
        (
            "C15 nitro",
            "[O-][N+](=O)c1ccccc1",
            "nitrobenzene",
            NameStatus::Preferred,
        ),
        (
            "nitromethane",
            "O=[N+]([O-])C",
            "nitromethane",
            NameStatus::Preferred,
        ),
        (
            "nitrile oxide",
            "CC#[N+][O-]",
            "acetonitrile oxide",
            NameStatus::Preferred,
        ),
        (
            "formonitrile oxide",
            "C#[N+][O-]",
            "formonitrile oxide",
            NameStatus::Preferred,
        ),
        (
            "nitrile sulfide",
            "CC#[N+][S-]",
            "ethanenitrile sulfide",
            NameStatus::Preferred,
        ),
        (
            "C16 quinoline",
            "[O-][n+]1cccc2ccccc12",
            "quinoline 1-oxide",
            NameStatus::Preferred,
        ),
        (
            "isoquinoline",
            "[O-][n+]1ccc2ccccc2c1",
            "isoquinoline 2-oxide",
            NameStatus::Preferred,
        ),
        (
            "C17 betaine",
            "C[N+](C)(C)CC[O-]",
            "2-(trimethylammonio)ethan-1-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "C18 phosphol",
            "O=P1(C)C=CC=C1",
            "1-methyl-1H-1lambda^5-phosphol-1-one",
            NameStatus::Preferred,
        ),
        (
            "ammonium salt",
            "C[N+](C)(C)C.[Cl-]",
            "N,N,N-trimethylmethanaminium chloride",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
        (
            "azoxy",
            "CN=[N+](C)[O-]",
            "1,2-dimethyldiazene 1-oxide",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
    ] {
        assert_case(case);
    }
}

#[test]
fn amine_oxide_nitrogen_stereocentres_remain_rejected() {
    for smiles in ["CC[N@+](C)([O-])c1ccccc1", "CC[N@@+](C)([O-])c1ccccc1"] {
        let error = name(smiles).unwrap_err();
        assert!(
            error.contains("N and ring lone-pair stereocentres are not validated"),
            "{smiles}: {error}"
        );
    }
}

#[test]
fn amine_oxide_unknown_stereo_does_not_certify_zwitterionic_alternatives() {
    let smiles = "[O-][n+]1ccc([CH:900001](F)Cl)cc1";
    let actual = name_smiles(smiles, &Default::default()).unwrap();
    assert_eq!(actual.text, "4-(chlorofluoromethyl)-1-oxidopyridin-1-ium");
    assert_eq!(
        actual.status,
        NameStatus::NotPreferred(NameReason::UnknownConfiguration)
    );
    assert_eq!(actual.omitted_units, 1);
    assert_eq!(actual.alternatives.len(), 1);
    let explicit = &actual.alternatives[0];
    assert_eq!(
        explicit.name,
        "4-[(Ξ)-chlorofluoromethyl]-1-oxidopyridin-1-ium"
    );
    assert_eq!(
        explicit.status,
        NameStatus::NotPreferred(NameReason::AmineOxideZwitterion)
    );
    for text in [&actual.text, &explicit.name] {
        let request = crate::naming::local::parse_request(text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(text, &reply).unwrap();
        assert_eq!(
            canonical_smiles(&parsed.smiles).unwrap(),
            canonical_smiles(smiles).unwrap(),
            "{text}"
        );
    }
}

#[test]
fn amine_oxide_names_and_statuses_are_independent_of_atom_order() {
    for (smiles, permutations) in [
        (
            "[O-][n+]1ccccc1",
            &["c1cc[n+]([O-])cc1", "c1cccc[n+]1[O-]"][..],
        ),
        (
            "C[N+]1([O-])CCCCC1",
            &["[O-][N+]1(C)CCCCC1", "C1CC[N+](C)([O-])CC1"][..],
        ),
        (
            "C[N+](C)(C)[O-]",
            &["[O-][N+](C)(C)C", "C[N+]([O-])(C)C"][..],
        ),
        ("[O-][n+]1ccc(C)cc1", &["Cc1cc[n+]([O-])cc1"][..]),
        ("C1CC[N+]2([O-])CCC1CC2", &["[O-][N+]12CCC(CC1)CCC2"][..]),
        ("C=[N+](C)[O-]", &["[O-][N+](C)=C", "C[N+]([O-])=C"][..]),
        ("C=[N+](Cl)[O-]", &["[O-][N+](Cl)=C", "Cl[N+]([O-])=C"][..]),
        ("C[N+]1([O-])CCOCC1", &["O1CC[N+](C)([O-])CC1"][..]),
        ("[O-][n+]1ccccc1C(=O)O", &["O=C(O)c1cccc[n+]1[O-]"][..]),
        ("[O-][NH2+]CC", &["CC[NH2+][O-]"][..]),
        (
            "C[N+]1([O-])CC[N+](C)([O-])CC1",
            &["[O-][N+]1(C)CC[N+]([O-])(C)CC1"][..],
        ),
    ] {
        let identity = canonical_smiles(smiles).unwrap();
        let expected = name(smiles).unwrap();
        for &permutation in permutations {
            assert_eq!(
                canonical_smiles(permutation).unwrap(),
                identity,
                "{permutation}"
            );
            let actual = name_smiles(permutation, &Default::default()).unwrap();
            assert_eq!(
                (actual.text.as_str(), actual.status),
                (expected.text.as_str(), expected.status),
                "{smiles}: {permutation}"
            );
        }
    }
}

#[test]
fn amine_oxide_ambiguous_ring_locants_remain_unverified() {
    // Neutral pyrazine numbering can choose either equivalent nitrogen first.
    // Its oxide locant is outside this adapter's certification scope.
    let identity = canonical_smiles("[O-][n+]1ccncc1").unwrap();
    for smiles in ["[O-][n+]1ccncc1", "c1c[n+]([O-])ccn1"] {
        assert_eq!(canonical_smiles(smiles).unwrap(), identity, "{smiles}");
        let actual = name_smiles(smiles, &Default::default()).unwrap();
        assert_eq!(
            actual.status,
            NameStatus::Unverified(NameReason::AmineChalcogenide),
            "{smiles}: {}",
            actual.text
        );
        assert!(
            actual.text.starts_with("pyrazine ") && actual.text.ends_with("-oxide"),
            "{}",
            actual.text
        );
        let request = crate::naming::local::parse_request(&actual.text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&actual.text, &reply).unwrap();
        assert_eq!(
            canonical_smiles(&parsed.smiles).unwrap(),
            identity,
            "{smiles}: {}",
            actual.text
        );
    }
}

#[test]
fn amine_oxide_functional_class_names_round_trip() {
    for &(row, smiles, expected, _) in FUNCTIONAL_CLASS_CASES.iter().chain(CHALCOGEN_ANALOGUES) {
        let actual = name(smiles).unwrap();
        assert_eq!(actual.text, expected, "{row}: {smiles}");
        let request = crate::naming::local::parse_request(&actual.text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&actual.text, &reply)
            .unwrap_or_else(|error| panic!("{row}: {}: {error}", actual.text));
        assert_eq!(
            canonical_smiles(smiles).unwrap(),
            canonical_smiles(&parsed.smiles).unwrap(),
            "{row}: {}",
            actual.text
        );
    }
}

#[test]
fn amine_oxide_functional_class_names_preserve_remote_carbon_stereo() {
    let mut names = Vec::new();
    let mut identities = Vec::new();
    for smiles in ["C[N+](C)([O-])C[C@H](F)Cl", "C[N+](C)([O-])C[C@@H](F)Cl"] {
        let mol = super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
        let centre = mol
            .atoms
            .values()
            .find(|atom| atom.is_carbon() && atom.has_stereo())
            .unwrap();
        let cip = centre.stereo.as_deref().unwrap();
        assert!(matches!(cip, "R" | "S"), "{smiles}: {cip}");

        let actual = name(smiles).unwrap();
        assert!(actual.text.ends_with(" N-oxide"), "{}", actual.text);
        assert!(
            actual.text.contains(&format!("{cip})-")),
            "{smiles}: expected source descriptor {cip}, got {}",
            actual.text
        );
        let request = crate::naming::local::parse_request(&actual.text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&actual.text, &reply).unwrap();
        let identity = canonical_smiles(smiles).unwrap();
        assert_eq!(
            canonical_smiles(&parsed.smiles).unwrap(),
            identity,
            "{}",
            actual.text
        );
        names.push(actual.text);
        identities.push(identity);
    }
    assert_ne!(
        names[0], names[1],
        "Opposite carbon configurations need distinct names"
    );
    assert_ne!(identities[0], identities[1]);
}

#[test]
fn amine_oxide_keeps_source_carbon_stereo_when_neutral_priorities_reverse() {
    for (smiles, neutral, descriptor, neutral_descriptor) in [
        (
            "C[N+](C)([O-])[C@H](F)N1CCCCC1",
            "CN(C)[C@H](F)N1CCCCC1",
            "R",
            "S",
        ),
        (
            "C[N+](C)([O-])[C@@H](F)N1CCCCC1",
            "CN(C)[C@@H](F)N1CCCCC1",
            "S",
            "R",
        ),
    ] {
        // Removing oxygen reverses the priority of the two nitrogen branches.
        for (input, expected) in [(smiles, descriptor), (neutral, neutral_descriptor)] {
            let mol = super::graph_io::read_smiles(input, &Default::default()).unwrap();
            let centre = mol
                .atoms
                .values()
                .find(|atom| atom.is_carbon() && atom.has_stereo())
                .unwrap();
            assert_eq!(centre.stereo.as_deref(), Some(expected), "{input}");
        }
        let actual = name(smiles).unwrap();
        assert_eq!(
            actual.text,
            format!("(1{descriptor})-1-fluoro-N,N-dimethyl-1-(piperidin-1-yl)methanamine N-oxide")
        );
        assert_eq!(actual.status, NameStatus::Preferred);
        let request = crate::naming::local::parse_request(&actual.text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&actual.text, &reply).unwrap();
        assert_eq!(
            canonical_smiles(&parsed.smiles).unwrap(),
            canonical_smiles(smiles).unwrap(),
            "{}",
            actual.text
        );
    }
}

#[test]
fn amine_oxide_nitrones_keep_opposite_double_bond_stereo() {
    for (smiles, descriptor) in [("C/C=[N+](/C)[O-]", "Z"), ("C/C=[N+](\\C)[O-]", "E")] {
        let mol = super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
        let bond = mol.bonds.values().find(|bond| bond.order == 2).unwrap();
        assert_eq!(bond.stereo.as_deref(), Some(descriptor), "{smiles}");
        let actual = name(smiles).unwrap();
        assert_eq!(
            actual.text,
            format!("({descriptor})-N-methylethanimine N-oxide")
        );
        assert_eq!(actual.status, NameStatus::Preferred);
        let request = crate::naming::local::parse_request(&actual.text).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&actual.text, &reply).unwrap();
        assert_eq!(
            canonical_smiles(&parsed.smiles).unwrap(),
            canonical_smiles(smiles).unwrap(),
            "{}",
            actual.text
        );
    }
}
