//! Source-derived regression cases from OPSIN 2.9.0
//! ComponentGeneration_ProcesslocantsTest, StereochemistryTest,
//! AmbiguitiesAndIrregularitiesTest and MiscTest (MIT).
use opsin::WarningKind;
use opsin::component_generator::{
    ComponentGenerationContext, ComponentGenerator, normalise_binary_brackets,
};
use opsin::parse_tree::{Arena, NodeId};
use opsin::xml_declarations::*;

fn token(a: &mut Arena, p: NodeId, name: &str, value: &str, attrs: &[(&str, &str)]) -> NodeId {
    let n = a.token(name, value);
    for &(k, v) in attrs {
        a[n].add_attribute(k, v)
    }
    a.add_child(p, n);
    n
}
fn locant(text: &str) -> (Arena, NodeId, NodeId) {
    let mut a = Arena::default();
    let sub = a.grouping(SUBSTITUENT_EL);
    let loc = token(&mut a, sub, LOCANT_EL, text, &[]);
    token(&mut a, sub, GROUP_EL, "", &[]);
    let mut context = ComponentGenerationContext::default();
    ComponentGenerator::new(&mut a, &mut context)
        .process_locants(sub)
        .unwrap();
    (a, sub, loc)
}
#[test]
fn upstream_locant_normalisation_cases() {
    for (input, expected) in [
        ("1", "1"),
        ("1-", "1"),
        ("N-", "N"),
        ("N1-", "N1"),
        ("1(10)-", "1(10)"),
        ("alpha", "alpha"),
        ("AlPhA-", "alpha"),
        ("NAlPhA-", "Nalpha"),
        ("2-N-", "N2"),
        ("N^(2)", "N2"),
        ("N^2", "N2"),
        ("N(2)", "N2"),
        ("N~12~", "N12"),
        ("N(alpha)", "Nalpha"),
        ("N^alpha", "Nalpha"),
        ("N*12*", "N12"),
        ("2,3-", "2,3"),
        (
            "2,N5,GaMMa,3-N,N^3,N(2),N~10~,4(5H),3-N(S),1(6)-",
            "2,N5,gamma,N3,N3,N2,N10,4,N3,1(6)",
        ),
        ("2''a", "2a''"),
        ("1A", "1a"),
    ] {
        let (a, _, loc) = locant(input);
        assert_eq!(a.value(loc), expected, "{input}");
    }
}
#[test]
fn upstream_added_hydrogen_and_stereochemical_locants() {
    for (input, expected, hydrogens) in [
        ("3(5'H)", "3", vec!["5'"]),
        ("1,2(2H,7H)", "1,2", vec!["2", "7"]),
    ] {
        let (a, sub, loc) = locant(input);
        assert_eq!(a.value(loc), expected);
        assert_eq!(
            a[loc].attribute(TYPE_ATR),
            Some(ADDEDHYDROGENLOCANT_TYPE_VAL)
        );
        let h = a.children_named(sub, ADDEDHYDROGEN_EL);
        assert_eq!(
            h.iter()
                .map(|&n| a[n].attribute(LOCANT_ATR).unwrap())
                .collect::<Vec<_>>(),
            hydrogens
        );
    }
    for (input, expected, stereo) in [
        ("5(R)", "5", "(5R)"),
        ("5-(S)", "5", "(5S)"),
        ("N(3)-(S)", "N3", "(N3S)"),
        ("5(RS)", "5", "(5RS)"),
        ("5(R,S)", "5", "(5RS)"),
        ("5(R/S)", "5", "(5RS)"),
    ] {
        let (a, sub, loc) = locant(input);
        assert_eq!(a.value(loc), expected);
        let s = a.children_named(sub, STEREOCHEMISTRY_EL);
        assert_eq!(s.len(), 1);
        assert_eq!(a.value(s[0]), stereo);
        assert_eq!(
            a[s[0]].attribute(TYPE_ATR),
            Some(STEREOCHEMISTRYBRACKET_TYPE_VAL)
        );
    }
}
#[test]
fn upstream_carbohydrate_locant_conversion() {
    for (numeric, symbol, expected) in [
        (Some("2,4,6"), "O", "O2,O4,O6"),
        (None, "O", "O,O',O''"),
        (Some("2,4,6"), "2", "2,4,6"),
    ] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        if let Some(n) = numeric {
            token(&mut a, sub, LOCANT_EL, n, &[]);
        }
        let m = token(&mut a, sub, MULTIPLIER_EL, "tri", &[(VALUE_ATR, "3")]);
        let symbol_loc = token(&mut a, sub, LOCANT_EL, symbol, &[]);
        token(&mut a, sub, GROUP_EL, "", &[]);
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_locants(sub)
            .unwrap();
        assert_eq!(a.value(a.previous_sibling(m).unwrap()), expected);
        assert_eq!(a[symbol_loc].parent.is_some(), symbol == "2");
    }
}
#[test]
fn upstream_stereochemistry_bracket_cases() {
    // input, (locant, normalized value, type, optional enhanced-stereo group)
    type StereoDescriptor<'a> = (Option<&'a str>, &'a str, &'a str, Option<&'a str>);
    let cases: Vec<(&str, Vec<StereoDescriptor<'_>>)> = vec![
        ("(S)", vec![(None, "S", R_OR_S_TYPE_VAL, Some("Abs"))]),
        (
            "(R,R)",
            vec![
                (None, "R", R_OR_S_TYPE_VAL, Some("Abs")),
                (None, "R", R_OR_S_TYPE_VAL, Some("Abs")),
            ],
        ),
        ("(1R)", vec![(Some("1"), "R", R_OR_S_TYPE_VAL, Some("Abs"))]),
        (
            "(alphaR,3S,7'S)",
            vec![
                (Some("alpha"), "R", R_OR_S_TYPE_VAL, Some("Abs")),
                (Some("3"), "S", R_OR_S_TYPE_VAL, Some("Abs")),
                (Some("7'"), "S", R_OR_S_TYPE_VAL, Some("Abs")),
            ],
        ),
        ("(E)", vec![(None, "E", E_OR_Z_TYPE_VAL, None)]),
        ("(5Z)", vec![(Some("5"), "Z", E_OR_Z_TYPE_VAL, None)]),
        (
            "(NZ,2E,R)",
            vec![
                (Some("N"), "Z", E_OR_Z_TYPE_VAL, None),
                (Some("2"), "E", E_OR_Z_TYPE_VAL, None),
                (None, "R", R_OR_S_TYPE_VAL, Some("Abs")),
            ],
        ),
        (
            "(NZ,2E-R)",
            vec![
                (Some("N"), "Z", E_OR_Z_TYPE_VAL, None),
                (Some("2"), "E", E_OR_Z_TYPE_VAL, None),
                (None, "R", R_OR_S_TYPE_VAL, Some("Abs")),
            ],
        ),
        (
            "(3cis,5trans)",
            vec![
                (Some("3"), "cis", CISORTRANS_TYPE_VAL, None),
                (Some("5"), "trans", CISORTRANS_TYPE_VAL, None),
            ],
        ),
        (
            "(5S-trans)",
            vec![
                (Some("5"), "S", R_OR_S_TYPE_VAL, Some("Abs")),
                (None, "trans", CISORTRANS_TYPE_VAL, None),
            ],
        ),
        (
            "(exo)",
            vec![(None, "exo", ENDO_EXO_SYN_ANTI_TYPE_VAL, None)],
        ),
        (
            "(3-endo,5S)",
            vec![
                (Some("3"), "endo", ENDO_EXO_SYN_ANTI_TYPE_VAL, None),
                (Some("5"), "S", R_OR_S_TYPE_VAL, Some("Abs")),
            ],
        ),
        ("(M)", vec![(None, "M", AXIAL_TYPE_VAL, None)]),
        ("(Ra)", vec![(None, "Ra", AXIAL_TYPE_VAL, None)]),
        (
            "(1a,2b,3bEtA,4alpha,5xi)",
            vec![
                (Some("1"), "alpha", ALPHA_OR_BETA_TYPE_VAL, None),
                (Some("2"), "beta", ALPHA_OR_BETA_TYPE_VAL, None),
                (Some("3"), "beta", ALPHA_OR_BETA_TYPE_VAL, None),
                (Some("4"), "alpha", ALPHA_OR_BETA_TYPE_VAL, None),
                (Some("5"), "xi", ALPHA_OR_BETA_TYPE_VAL, None),
            ],
        ),
        (
            "rel-(1R,3S,4S,7R)",
            vec![
                (Some("1"), "R", R_OR_S_TYPE_VAL, Some("Rel")),
                (Some("3"), "S", R_OR_S_TYPE_VAL, Some("Rel")),
                (Some("4"), "S", R_OR_S_TYPE_VAL, Some("Rel")),
                (Some("7"), "R", R_OR_S_TYPE_VAL, Some("Rel")),
            ],
        ),
        (
            "(1R*,3S*,4S*,7R*)",
            vec![
                (Some("1"), "R", R_OR_S_TYPE_VAL, Some("Rel")),
                (Some("3"), "S", R_OR_S_TYPE_VAL, Some("Rel")),
                (Some("4"), "S", R_OR_S_TYPE_VAL, Some("Rel")),
                (Some("7"), "R", R_OR_S_TYPE_VAL, Some("Rel")),
            ],
        ),
        (
            "rac-(2R)",
            vec![(Some("2"), "R", R_OR_S_TYPE_VAL, Some("Rac"))],
        ),
        ("(RS)", vec![(None, "R", R_OR_S_TYPE_VAL, Some("Rac"))]),
        ("(rs)", vec![(None, "R", R_OR_S_TYPE_VAL, Some("Rac"))]),
        ("(SR)", vec![(None, "S", R_OR_S_TYPE_VAL, Some("Rac"))]),
        ("(R/S)-", vec![(None, "R", R_OR_S_TYPE_VAL, Some("Rac"))]),
        (
            "(2RS,4SR)",
            vec![
                (Some("2"), "R", R_OR_S_TYPE_VAL, Some("Rac")),
                (Some("4"), "S", R_OR_S_TYPE_VAL, Some("Rac")),
            ],
        ),
        ("(EZ)", vec![(None, "EZ", E_OR_Z_TYPE_VAL, None)]),
        ("(2EZ)", vec![(Some("2"), "EZ", E_OR_Z_TYPE_VAL, None)]),
    ];
    for (input, expected) in cases {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        token(
            &mut a,
            sub,
            STEREOCHEMISTRY_EL,
            input,
            &[(TYPE_ATR, STEREOCHEMISTRYBRACKET_TYPE_VAL)],
        );
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_stereochemistry(sub)
            .unwrap();
        let children = &a[sub].children;
        assert_eq!(children.len(), expected.len(), "{input}");
        for (&n, (loc, val, kind, group)) in children.iter().zip(expected) {
            assert_eq!(
                (
                    a[n].attribute(LOCANT_ATR),
                    a[n].attribute(VALUE_ATR),
                    a[n].attribute(TYPE_ATR),
                    a[n].attribute(STEREOGROUP_ATR)
                ),
                (loc, Some(val), Some(kind), group),
                "{input}"
            )
        }
    }
}
#[test]
fn upstream_exclusive_racemic_and_relative_terms() {
    for (input, kind) in [
        ("rel-", REL_TYPE_VAL),
        ("rac-", RAC_TYPE_VAL),
        ("racem-", RAC_TYPE_VAL),
        ("racemic-", RAC_TYPE_VAL),
        ("(RAC)", RAC_TYPE_VAL),
    ] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        token(
            &mut a,
            sub,
            STEREOCHEMISTRY_EL,
            input,
            &[(TYPE_ATR, STEREOCHEMISTRYBRACKET_TYPE_VAL)],
        );
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_stereochemistry(sub)
            .unwrap();
        assert_eq!(a[sub].children.len(), 1);
        assert_eq!(a[a[sub].children[0]].attribute(TYPE_ATR), Some(kind));
    }
}
#[test]
fn unbracketed_ez_duplicates_ene_or_ylidene_locants() {
    for ylidene in [false, true] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        token(&mut a, sub, LOCANT_EL, "2", &[]);
        token(
            &mut a,
            sub,
            STEREOCHEMISTRY_EL,
            "E",
            &[(TYPE_ATR, E_OR_Z_TYPE_VAL)],
        );
        if !ylidene {
            token(&mut a, sub, LOCANT_EL, "4", &[]);
            token(
                &mut a,
                sub,
                STEREOCHEMISTRY_EL,
                "Z",
                &[(TYPE_ATR, E_OR_Z_TYPE_VAL)],
            );
            token(&mut a, sub, MULTIPLIER_EL, "di", &[(VALUE_ATR, "2")]);
        }
        token(
            &mut a,
            sub,
            if ylidene { SUFFIX_EL } else { UNSATURATOR_EL },
            if ylidene { "ylidene" } else { "en" },
            &[(VALUE_ATR, if ylidene { "ylidene" } else { "2" })],
        );
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_stereochemistry(sub)
            .unwrap();
        let loc = a.children_named(sub, LOCANT_EL);
        assert_eq!(loc.len(), 1);
        assert_eq!(a.value(loc[0]), if ylidene { "2" } else { "2,4" });
    }
}
#[test]
fn binary_stereo_brackets_and_warning_policy() {
    for (input, expected) in [
        ("(R)-and(S)-", "(RS)"),
        ("(R)-or(S)-", "(R*)"),
        ("(R,R)-or(S,R)-", "(R*,R)"),
    ] {
        assert_eq!(normalise_binary_brackets(input).unwrap(), expected)
    }
    let mut a = Arena::default();
    let sub = a.grouping(SUBSTITUENT_EL);
    token(
        &mut a,
        sub,
        STEREOCHEMISTRY_EL,
        "(1R)-or(2S)",
        &[(TYPE_ATR, STEREOCHEMISTRYBRACKET_TYPE_VAL)],
    );
    let mut strict = ComponentGenerationContext::default();
    assert!(
        ComponentGenerator::new(&mut a, &mut strict)
            .process_stereochemistry(sub)
            .is_err()
    );
    let mut a = Arena::default();
    let sub = a.grouping(SUBSTITUENT_EL);
    token(
        &mut a,
        sub,
        STEREOCHEMISTRY_EL,
        "(1R)-or(2S)",
        &[(TYPE_ATR, STEREOCHEMISTRYBRACKET_TYPE_VAL)],
    );
    let mut context = ComponentGenerationContext::default();
    context
        .options
        .warn_rather_than_fail_on_uninterpretable_stereochemistry = true;
    ComponentGenerator::new(&mut a, &mut context)
        .process_stereochemistry(sub)
        .unwrap();
    assert_eq!(context.warnings.len(), 1);
    assert_eq!(
        context.warnings[0].kind,
        WarningKind::StereochemistryIgnored
    );
    assert!(a[sub].children.is_empty());
}
#[test]
fn upstream_alkane_ambiguity_cases() {
    for (count, chain, locants, reject) in [
        (2, 10, None, false),
        (6, 6, None, false),
        (4, 10, None, true),
        (4, 10, Some("1,2,3,4"), false),
        (4, 10, Some("1"), true),
    ] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        if let Some(loc) = locants {
            token(&mut a, sub, LOCANT_EL, loc, &[]);
        }
        token(
            &mut a,
            sub,
            MULTIPLIER_EL,
            "tetra",
            &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, &count.to_string())],
        );
        token(
            &mut a,
            sub,
            ALKANESTEMCOMPONENT,
            "dec",
            &[(VALUE_ATR, &chain.to_string())],
        );
        assert_eq!(
            ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
                .resolve_ambiguities(sub)
                .is_err(),
            reject
        );
    }
}
#[test]
fn upstream_tetraphene_ambiguity_cases() {
    for (before, after, e, reject) in [
        (None, None, false, true),
        (None, None, true, false),
        (None, Some("2"), false, false),
        (Some("2"), None, false, false),
    ] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        if let Some(loc) = before {
            token(&mut a, sub, LOCANT_EL, loc, &[]);
        }
        token(
            &mut a,
            sub,
            MULTIPLIER_EL,
            "tetra",
            &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, "4")],
        );
        let phen = token(&mut a, sub, HYDROCARBONFUSEDRINGSYSTEM_EL, "phen", &[]);
        if e {
            a[phen].add_attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR, "e")
        }
        if let Some(loc) = after {
            token(&mut a, sub, LOCANT_EL, loc, &[]);
        }
        token(&mut a, sub, SUFFIX_EL, "yl", &[]);
        assert_eq!(
            ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
                .resolve_ambiguities(sub)
                .is_err(),
            reject
        );
    }
}

#[test]
fn upstream_alpha_beta_locants_depend_on_the_following_group() {
    for (natural_before, natural_after, mixed, expected_locants) in [
        (false, true, false, None),
        (false, false, false, Some("3,5")),
        (true, false, false, Some("3,5")),
        (false, false, true, Some("3,4,10,12")),
    ] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        if natural_before {
            token(
                &mut a,
                sub,
                GROUP_EL,
                "natural",
                &[
                    (SUBTYPE_ATR, BIOCHEMICAL_SUBTYPE_VAL),
                    (ALPHABETACLOCKWISEATOMORDERING_ATR, ""),
                ],
            );
        }
        token(
            &mut a,
            sub,
            STEREOCHEMISTRY_EL,
            if mixed {
                "3beta,4,10,12alpha"
            } else {
                "3beta,5alpha"
            },
            &[(TYPE_ATR, ALPHA_OR_BETA_TYPE_VAL)],
        );
        if natural_after {
            token(
                &mut a,
                sub,
                GROUP_EL,
                "natural",
                &[
                    (SUBTYPE_ATR, BIOCHEMICAL_SUBTYPE_VAL),
                    (ALPHABETACLOCKWISEATOMORDERING_ATR, ""),
                ],
            );
        }
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_stereochemistry(sub)
            .unwrap();
        let stereo = a.children_named(sub, STEREOCHEMISTRY_EL);
        assert_eq!(stereo.len(), 2);
        assert_eq!(a[stereo[0]].attribute(LOCANT_ATR), Some("3"));
        assert_eq!(a[stereo[0]].attribute(VALUE_ATR), Some("beta"));
        assert_eq!(
            a[stereo[1]].attribute(LOCANT_ATR),
            Some(if mixed { "12" } else { "5" })
        );
        assert_eq!(a[stereo[1]].attribute(VALUE_ATR), Some("alpha"));
        let locants = a.children_named(sub, LOCANT_EL);
        assert_eq!(
            locants.first().map(|&id| a.value(id)),
            expected_locants.map(str::to_string)
        );
    }
}

#[test]
fn upstream_endo_exo_locants_can_retain_a_substituent_meaning() {
    for ring in [false, true] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        let loc = token(&mut a, sub, LOCANT_EL, "3", &[]);
        let stereo = token(
            &mut a,
            sub,
            STEREOCHEMISTRY_EL,
            "exo",
            &[(TYPE_ATR, ENDO_EXO_SYN_ANTI_TYPE_VAL), (VALUE_ATR, "exo")],
        );
        if ring {
            token(
                &mut a,
                sub,
                MULTIPLIER_EL,
                "bi",
                &[(TYPE_ATR, VONBAEYER_TYPE_VAL)],
            );
            token(&mut a, sub, VONBAEYER_EL, "", &[]);
        }
        token(
            &mut a,
            sub,
            GROUP_EL,
            "group",
            &[
                (TYPE_ATR, if ring { CHAIN_TYPE_VAL } else { SUBSTITUENT_EL }),
                (
                    SUBTYPE_ATR,
                    if ring {
                        ALKANESTEM_SUBTYPE_VAL
                    } else {
                        SIMPLESUBSTITUENT_SUBTYPE_VAL
                    },
                ),
            ],
        );
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_stereochemistry(sub)
            .unwrap();
        assert_eq!(a[stereo].attribute(LOCANT_ATR), Some("3"));
        assert_eq!(a[loc].parent.is_none(), ring);
    }
}

#[test]
fn relative_cis_trans_and_optical_racemates() {
    let mut a = Arena::default();
    let sub = a.grouping(SUBSTITUENT_EL);
    token(
        &mut a,
        sub,
        STEREOCHEMISTRY_EL,
        "c-4-",
        &[(TYPE_ATR, RELATIVECISTRANS_TYPE_VAL)],
    );
    ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
        .process_stereochemistry(sub)
        .unwrap();
    assert_eq!(a.value(a.children_named(sub, LOCANT_EL)[0]), "4");
    for value in ["(+/-)", "(+-)"] {
        let mut a = Arena::default();
        let sub = a.grouping(SUBSTITUENT_EL);
        token(
            &mut a,
            sub,
            STEREOCHEMISTRY_EL,
            value,
            &[(TYPE_ATR, OPTICALROTATION_TYPE_VAL)],
        );
        ComponentGenerator::new(&mut a, &mut ComponentGenerationContext::default())
            .process_stereochemistry(sub)
            .unwrap();
        assert_eq!(a[sub].children.len(), 2);
        assert_eq!(
            a[a[sub].children[0]].attribute(TYPE_ATR),
            Some(RAC_TYPE_VAL)
        );
    }
}
