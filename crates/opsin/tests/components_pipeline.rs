//! Full ordered ComponentGenerator fixtures derived from the pinned upstream
//! transformations, with no external executable or backend.
use opsin::component_generator::{ComponentGenerationContext, process_components};
use opsin::parse_tree::{Arena, NodeId, ParseTree};
use opsin::xml_declarations::*;

fn token(a: &mut Arena, p: NodeId, name: &str, value: &str, attrs: &[(&str, &str)]) -> NodeId {
    let n = a.token(name, value);
    for &(k, v) in attrs {
        a[n].add_attribute(k, v)
    }
    a.add_child(p, n);
    n
}
fn tree() -> (ParseTree, NodeId) {
    let mut a = Arena::default();
    let molecule = a.grouping(MOLECULE_EL);
    let rule = a.grouping(WORDRULE_EL);
    a[rule].add_attribute(WORDRULE_ATR, "simple");
    let word = a.grouping(WORD_EL);
    let root = a.grouping(ROOT_EL);
    a.add_child(molecule, rule);
    a.add_child(rule, word);
    a.add_child(word, root);
    (
        ParseTree {
            arena: a,
            root: molecule,
        },
        root,
    )
}
fn run(t: &mut ParseTree) {
    process_components(t, &mut ComponentGenerationContext::default()).unwrap();
    for n in t.arena.descendants(t.root) {
        for &child in &t.arena[n].children {
            assert_eq!(t.arena[child].parent, Some(n));
        }
        let mut seen = std::collections::HashSet::new();
        for &child in &t.arena[n].children {
            assert!(seen.insert(child));
        }
    }
}
fn group(t: &ParseTree) -> NodeId {
    let g = t.arena.descendants_named(t.root, GROUP_EL);
    assert_eq!(g.len(), 1, "{}", t.to_xml());
    g[0]
}

#[test]
fn chain_components_and_traditional_modifiers() {
    for (modifier, len, suffix, smiles, labels) in [
        ("tert", 4, true, "C(C)(C)C", NONE_LABELS_VAL),
        ("iso", 4, true, "CC(C)C", "1/2//"),
        ("sec", 4, true, "C(C)CC", NONE_LABELS_VAL),
        ("neo", 5, true, "CC(C)(C)C", NONE_LABELS_VAL),
        ("iso", 8, false, "C(C)(C)CC(C)(C)C", NONE_LABELS_VAL),
    ] {
        let (mut t, r) = tree();
        token(
            &mut t.arena,
            r,
            ALKANESTEMMODIFIER_EL,
            modifier,
            &[(VALUE_ATR, modifier)],
        );
        token(
            &mut t.arena,
            r,
            ALKANESTEMCOMPONENT,
            "but",
            &[(VALUE_ATR, &len.to_string())],
        );
        if suffix {
            token(
                &mut t.arena,
                r,
                SUFFIX_EL,
                "yl",
                &[(TYPE_ATR, INLINE_TYPE_VAL), (VALUE_ATR, "yl")],
            );
        }
        run(&mut t);
        let g = group(&t);
        assert_eq!(t.arena[g].attribute(VALUE_ATR), Some(smiles));
        assert_eq!(t.arena[g].attribute(LABELS_ATR), Some(labels));
        assert!(t.arena[g].attribute(USABLEASJOINER_ATR).is_none());
    }
    let (mut t, r) = tree();
    token(
        &mut t.arena,
        r,
        ALKANESTEMCOMPONENT,
        "do",
        &[(VALUE_ATR, "2")],
    );
    token(
        &mut t.arena,
        r,
        ALKANESTEMCOMPONENT,
        "dec",
        &[(VALUE_ATR, "10")],
    );
    run(&mut t);
    let g = group(&t);
    assert_eq!(t.arena.value(g), "dodec");
    assert_eq!(t.arena[g].attribute(VALUE_ATR), Some("CCCCCCCCCCCC"));
}
#[test]
fn invalid_alkane_modifier_meanings_are_rejected() {
    for (modifier, len) in [
        ("tert", 3),
        ("tert", 9),
        ("iso", 2),
        ("iso", 3),
        ("sec", 4),
        ("neo", 4),
    ] {
        let (mut t, r) = tree();
        token(
            &mut t.arena,
            r,
            ALKANESTEMMODIFIER_EL,
            modifier,
            &[(VALUE_ATR, modifier)],
        );
        token(
            &mut t.arena,
            r,
            ALKANESTEMCOMPONENT,
            "alk",
            &[(VALUE_ATR, &len.to_string())],
        );
        assert!(
            process_components(&mut t, &mut ComponentGenerationContext::default()).is_err(),
            "{modifier} {len}"
        )
    }
}
#[test]
fn normal_methyl_and_ethyl_mean_nitrogen_locants() {
    for len in [1, 2] {
        let (mut t, r) = tree();
        token(&mut t.arena, r, ALKANESTEMMODIFIER_EL, "n-", &[]);
        token(
            &mut t.arena,
            r,
            ALKANESTEMCOMPONENT,
            "alk",
            &[(VALUE_ATR, &len.to_string())],
        );
        run(&mut t);
        let loc = t.arena.children_named(r, LOCANT_EL);
        assert_eq!(loc.len(), 1);
        assert_eq!(t.arena.value(loc[0]), "N");
    }
}
#[test]
fn cyclo_spiro_and_von_baeyer_descriptors() {
    for (kind, text, multiplier, len, expected) in [
        (CYCLO_EL, "cyclo", None, 6, "C1CCCCC1"),
        (SPIRO_EL, "spiro[2.2]", None, 5, "C0CC10(CC1)"),
        (
            SPIRO_EL,
            "spiro[2.1.2.1]",
            Some((BASIC_TYPE_VAL, "2")),
            8,
            "C0CC10(CC2(CC2)C1)",
        ),
        (
            VONBAEYER_EL,
            "cyclo[2.2.1]",
            Some((VONBAEYER_TYPE_VAL, "2")),
            7,
            "C1(CCC2CC1)C2",
        ),
        (
            VONBAEYER_EL,
            "cyclo[2.2.1.0^1,4]",
            Some((VONBAEYER_TYPE_VAL, "3")),
            7,
            "C13(CCC23CC1)C2",
        ),
    ] {
        let (mut t, r) = tree();
        if let Some((kind, count)) = multiplier {
            token(
                &mut t.arena,
                r,
                MULTIPLIER_EL,
                "",
                &[(TYPE_ATR, kind), (VALUE_ATR, count)],
            );
        }
        token(&mut t.arena, r, kind, text, &[]);
        token(
            &mut t.arena,
            r,
            ALKANESTEMCOMPONENT,
            "alk",
            &[(VALUE_ATR, &len.to_string())],
        );
        run(&mut t);
        let g = group(&t);
        assert_eq!(t.arena[g].attribute(VALUE_ATR), Some(expected), "{text}");
        assert_eq!(t.arena[g].attribute(TYPE_ATR), Some(RING_TYPE_VAL));
        assert!(t.arena[g].attribute(USABLEASJOINER_ATR).is_none());
        if kind == CYCLO_EL {
            assert_eq!(
                t.arena[g].attribute(LABELS_ATR),
                Some("1/2,ortho/3,meta/4,para/5/6")
            )
        }
    }
}
#[test]
fn incompatible_ring_counts_are_rejected() {
    for (kind, text, multiplier, len) in [
        (CYCLO_EL, "cyclo", None, 2),
        (SPIRO_EL, "spiro[2.2]", None, 6),
        (VONBAEYER_EL, "cyclo[2.2.1]", Some("2"), 6),
        (VONBAEYER_EL, "cyclo[2.2.1]", Some("3"), 7),
    ] {
        let (mut t, r) = tree();
        if let Some(count) = multiplier {
            token(
                &mut t.arena,
                r,
                MULTIPLIER_EL,
                "",
                &[(TYPE_ATR, VONBAEYER_TYPE_VAL), (VALUE_ATR, count)],
            );
        }
        token(&mut t.arena, r, kind, text, &[]);
        token(
            &mut t.arena,
            r,
            ALKANESTEMCOMPONENT,
            "alk",
            &[(VALUE_ATR, &len.to_string())],
        );
        assert!(
            process_components(&mut t, &mut ComponentGenerationContext::default()).is_err(),
            "{text}"
        )
    }
}
#[test]
fn heteroatom_chains_and_alternating_hydrides() {
    let (mut t, r) = tree();
    token(
        &mut t.arena,
        r,
        MULTIPLIER_EL,
        "tetra",
        &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, "4")],
    );
    token(
        &mut t.arena,
        r,
        GROUP_EL,
        "phosph",
        &[
            (TYPE_ATR, CHAIN_TYPE_VAL),
            (SUBTYPE_ATR, HETEROSTEM_SUBTYPE_VAL),
            (VALUE_ATR, "P"),
        ],
    );
    run(&mut t);
    assert_eq!(t.arena[group(&t)].attribute(VALUE_ATR), Some("PPPP"));
    for cyclic in [false, true] {
        let (mut t, r) = tree();
        if cyclic {
            token(&mut t.arena, r, CYCLO_EL, "cyclo", &[]);
        }
        token(
            &mut t.arena,
            r,
            MULTIPLIER_EL,
            "tri",
            &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, "3")],
        );
        token(
            &mut t.arena,
            r,
            HETEROATOM_EL,
            "sil",
            &[(VALUE_ATR, "[SiH2]")],
        );
        token(&mut t.arena, r, HETEROATOM_EL, "ox", &[(VALUE_ATR, "O")]);
        token(&mut t.arena, r, UNSATURATOR_EL, "an", &[(VALUE_ATR, "1")]);
        run(&mut t);
        assert_eq!(
            t.arena[group(&t)].attribute(VALUE_ATR),
            Some(if cyclic {
                "O1[SiH?]O[SiH?]O[SiH?]1"
            } else {
                "[SiH?]O[SiH?]O[SiH?]"
            })
        );
    }
}
#[test]
fn indicated_hydrogen_group_is_split_and_case_fixed() {
    let (mut t, r) = tree();
    token(&mut t.arena, r, INDICATEDHYDROGEN_EL, "(1H,3AH)-", &[]);
    token(
        &mut t.arena,
        r,
        GROUP_EL,
        "ring",
        &[(TYPE_ATR, RING_TYPE_VAL)],
    );
    run(&mut t);
    let h = t.arena.children_named(r, INDICATEDHYDROGEN_EL);
    assert_eq!(h.len(), 2);
    assert_eq!(t.arena[h[0]].attribute(LOCANT_ATR), Some("1"));
    assert_eq!(t.arena[h[1]].attribute(LOCANT_ATR), Some("3a"));
}
#[test]
fn suffix_prefix_and_structurally_multiplied_infixes() {
    for bracket in [false, true] {
        let (mut t, r) = tree();
        token(
            &mut t.arena,
            r,
            GROUP_EL,
            "group",
            &[(TYPE_ATR, SIMPLEGROUP_TYPE_VAL)],
        );
        if bracket {
            token(&mut t.arena, r, STRUCTURALOPENBRACKET_EL, "(", &[]);
        } else {
            token(
                &mut t.arena,
                r,
                SUFFIXPREFIX_EL,
                "sulfono",
                &[(VALUE_ATR, "S(=O)(=O)")],
            );
        }
        token(
            &mut t.arena,
            r,
            MULTIPLIER_EL,
            "di",
            &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, "2")],
        );
        token(&mut t.arena, r, INFIX_EL, "thio", &[(VALUE_ATR, "O:S")]);
        let suffix = token(
            &mut t.arena,
            r,
            SUFFIX_EL,
            "ate",
            &[(TYPE_ATR, INLINE_TYPE_VAL), (VALUE_ATR, "ate")],
        );
        if bracket {
            token(&mut t.arena, r, STRUCTURALCLOSEBRACKET_EL, ")", &[]);
        }
        run(&mut t);
        assert_eq!(t.arena[suffix].attribute(INFIX_ATR), Some("O:S;O:S"));
        if !bracket {
            assert_eq!(
                t.arena[suffix].attribute(SUFFIXPREFIX_ATR),
                Some("S(=O)(=O)")
            )
        }
        assert!(t.arena.children_named(r, INFIX_EL).is_empty());
        assert!(t.arena.children_named(r, MULTIPLIER_EL).is_empty());
    }
}
#[test]
fn lambda_values_are_assigned_to_multiplied_heteroatoms() {
    let (mut t, r) = tree();
    token(
        &mut t.arena,
        r,
        LAMBDACONVENTION_EL,
        "1lambda^5,3lambda^3",
        &[],
    );
    token(
        &mut t.arena,
        r,
        MULTIPLIER_EL,
        "di",
        &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, "2")],
    );
    token(
        &mut t.arena,
        r,
        HETEROATOM_EL,
        "phosph",
        &[(VALUE_ATR, "P")],
    );
    token(
        &mut t.arena,
        r,
        GROUP_EL,
        "ring",
        &[
            (TYPE_ATR, RING_TYPE_VAL),
            (SUBTYPE_ATR, HANTZSCHWIDMAN_SUBTYPE_VAL),
        ],
    );
    run(&mut t);
    let h = t.arena.children_named(r, HETEROATOM_EL);
    assert_eq!(h.len(), 2);
    let mut assignments = h
        .iter()
        .map(|&id| {
            (
                t.arena[id].attribute(LOCANT_ATR).unwrap(),
                t.arena[id].attribute(LAMBDA_ATR).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assignments.sort();
    assert_eq!(assignments, vec![("1", "5"), ("3", "3")]);
    assert!(t.arena.children_named(r, LAMBDACONVENTION_EL).is_empty());
}
#[test]
fn brackets_preserve_prefixes_and_grouping_scopes() {
    let (mut t, r) = tree();
    let word = t.arena[r].parent.unwrap();
    let sub = t.arena.grouping(SUBSTITUENT_EL);
    t.arena.insert_child(word, sub, 0);
    token(&mut t.arena, sub, LOCANT_EL, "1", &[]);
    token(&mut t.arena, sub, OPENBRACKET_EL, "(", &[]);
    let left = token(
        &mut t.arena,
        sub,
        GROUP_EL,
        "left",
        &[(TYPE_ATR, SIMPLEGROUP_TYPE_VAL)],
    );
    let right = token(
        &mut t.arena,
        r,
        GROUP_EL,
        "right",
        &[(TYPE_ATR, SIMPLEGROUP_TYPE_VAL)],
    );
    token(&mut t.arena, r, CLOSEBRACKET_EL, ")", &[]);
    run(&mut t);
    let b = t.arena.descendants_named(t.root, BRACKET_EL);
    assert_eq!(b.len(), 1);
    assert_eq!(t.arena[word].children, vec![b[0]]);
    assert_eq!(t.arena[b[0]].children.len(), 3);
    assert_eq!(t.arena.value(t.arena[b[0]].children[0]), "1");
    assert_eq!(t.arena[left].parent, Some(sub));
    assert_eq!(t.arena[right].parent, Some(r));
    assert!(t.arena.descendants_named(t.root, OPENBRACKET_EL).is_empty());
    assert!(
        t.arena
            .descendants_named(t.root, CLOSEBRACKET_EL)
            .is_empty()
    );
}
#[test]
fn quinone_and_ylene_irregularities() {
    for (suffix, expected) in [("quinone", "one"), ("ylene", "yl")] {
        let (mut t, r) = tree();
        token(
            &mut t.arena,
            r,
            ALKANESTEMCOMPONENT,
            "hex",
            &[(VALUE_ATR, "6")],
        );
        let s = token(
            &mut t.arena,
            r,
            SUFFIX_EL,
            suffix,
            &[
                (TYPE_ATR, INLINE_TYPE_VAL),
                (VALUE_ATR, expected),
                (ADDITIONALVALUE_ATR, "legacy"),
            ],
        );
        run(&mut t);
        assert_eq!(t.arena.value(s), expected);
        assert!(t.arena[s].attribute(ADDITIONALVALUE_ATR).is_none());
        let m = t.arena.previous_sibling(s).unwrap();
        assert_eq!(t.arena[m].name, MULTIPLIER_EL);
        assert_eq!(t.arena[m].attribute(VALUE_ATR), Some("2"));
    }
}
#[test]
fn upstream_salt_component_rejection_and_numerical_multiplier() {
    let (mut t, r) = tree();
    token(
        &mut t.arena,
        r,
        GROUP_EL,
        "hydrate",
        &[
            (TYPE_ATR, SIMPLEGROUP_TYPE_VAL),
            (SUBTYPE_ATR, SALTCOMPONENT_SUBTYPE_VAL),
        ],
    );
    assert!(process_components(&mut t, &mut ComponentGenerationContext::default()).is_err());
    let (mut t, r) = tree();
    let extra = t.arena.grouping(WORDRULE_EL);
    t.arena.insert_child(t.root, extra, 0);
    let g = token(
        &mut t.arena,
        r,
        GROUP_EL,
        "2hcl",
        &[
            (TYPE_ATR, SIMPLEGROUP_TYPE_VAL),
            (SUBTYPE_ATR, SALTCOMPONENT_SUBTYPE_VAL),
        ],
    );
    run(&mut t);
    assert_eq!(t.arena.value(g), "hcl");
    let m = t.arena.previous_sibling(g).unwrap();
    assert_eq!(t.arena[m].name, MULTIPLIER_EL);
    assert_eq!(t.arena[m].attribute(VALUE_ATR), Some("2"));
}
#[test]
fn elementary_diatomics_are_bonded() {
    for (atom, expected) in [
        ("[H]", "[H][H]"),
        ("[N]", "N#N"),
        ("[O]", "O=O"),
        ("[F]", "FF"),
        ("[Cl]", "ClCl"),
        ("[Br]", "BrBr"),
        ("[I]", "II"),
    ] {
        let (mut t, r) = tree();
        token(
            &mut t.arena,
            r,
            MULTIPLIER_EL,
            "di",
            &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, "2")],
        );
        token(
            &mut t.arena,
            r,
            GROUP_EL,
            "atom",
            &[(TYPE_ATR, ELEMENTARYATOM_TYPE_VAL), (VALUE_ATR, atom)],
        );
        run(&mut t);
        assert_eq!(t.arena[group(&t)].attribute(VALUE_ATR), Some(expected));
        assert!(t.arena.children_named(r, MULTIPLIER_EL).is_empty());
    }
}
