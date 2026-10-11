use super::*;
use crate::graphics::{BracketSides, Graphic, GraphicKind};

type MutationCase = (&'static str, fn(&mut Document));
type InvalidCase = (&'static str, fn(&mut Document), &'static str);

fn chain(elements: &[&str]) -> Document {
    let mut doc = Document::default();
    let mut previous = None;
    for (index, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(index as f32 * 42., 0.));
        if let Some(previous) = previous {
            doc.add_bond(previous, id, 1, "plain");
        }
        previous = Some(id);
    }
    doc
}

fn element(symbol: &str, isotope: u32) -> Alternative {
    Alternative::Element {
        symbol: symbol.into(),
        isotope,
    }
}

fn bracket(doc: &mut Document) -> u64 {
    let id = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Brackets,
        Point::new(20., -30.),
        Point::new(104., 30.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    id
}

fn alternative_document() -> Document {
    let mut doc = chain(&["*", "C", "C"]);
    doc.generic.alternatives.push(AlternativeSite {
        atom: 1,
        choices: vec![element("Cl", 0), element("Br", 0)],
        excluded: false,
    });
    doc
}

fn link_document() -> Document {
    let mut doc = chain(&["C", "C", "C"]);
    doc.generic.links.push(LinkNode {
        atom: 2,
        min: 1,
        max: 3,
    });
    doc.atom_mut(2).unwrap().display.variable = Some("(CH2)1-3".into());
    doc
}

fn repeat_document() -> Document {
    let mut doc = chain(&["O", "C", "C", "N"]);
    let primary = bracket(&mut doc);
    let additional = bracket(&mut doc);
    doc.generic.repeats.push(RepeatUnit {
        bracket: Some(primary),
        bracket_objects: vec![additional],
        members: vec![2, 3],
        crossings: vec![
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
        ],
        crossing_brackets: vec![primary, additional],
        head_count: Some(1),
        usage: "SRU".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "Unspecified".into(),
    });
    doc
}

mod definitions {
    use super::*;

    fn check_invalid_cases(base: Document, cases: &[InvalidCase]) {
        assert_eq!(super::validate(&base), Ok(()));
        for (name, mutate, message) in cases {
            let mut doc = base.clone();
            mutate(&mut doc);
            assert_eq!(super::validate(&doc), Err((*message).into()), "{name}");
        }
    }

    #[test]
    fn alternative_validation_errors_and_precedence() {
        check_invalid_cases(
            alternative_document(),
            &[
                (
                    "definition budget precedes missing atoms",
                    |d| {
                        let mut site = d.generic.alternatives[0].clone();
                        site.atom = 999;
                        d.generic.alternatives = vec![site; 10_001];
                    },
                    "Too many generic definitions",
                ),
                (
                    "missing atom",
                    |d| d.generic.alternatives[0].atom = 999,
                    "Missing alternative atom",
                ),
                (
                    "duplicate site",
                    |d| {
                        d.generic
                            .alternatives
                            .push(d.generic.alternatives[0].clone())
                    },
                    "Invalid alternative atom definition",
                ),
                (
                    "ordinary element",
                    |d| d.atoms[0].element = "C".into(),
                    "Invalid alternative atom definition",
                ),
                (
                    "attachment",
                    |d| d.atoms[0].attachment = Some(crate::attachments::Kind::Variable),
                    "Invalid alternative atom definition",
                ),
                (
                    "centroid",
                    |d| d.atoms[0].centroid = vec![2],
                    "Invalid alternative atom definition",
                ),
                (
                    "no choices",
                    |d| d.generic.alternatives[0].choices.clear(),
                    "Invalid alternative atom definition",
                ),
                (
                    "119 choices",
                    |d| d.generic.alternatives[0].choices = vec![element("C", 0); 119],
                    "Invalid alternative atom definition",
                ),
                (
                    "unknown element",
                    |d| d.generic.alternatives[0].choices[0] = element("Qq", 0),
                    "Invalid alternative element",
                ),
                (
                    "isotope overflow",
                    |d| d.generic.alternatives[0].choices[0] = element("H", 65_536),
                    "Invalid alternative element",
                ),
                (
                    "empty group",
                    |d| {
                        d.generic.alternatives[0].choices[0] = Alternative::Group {
                            label: String::new(),
                        }
                    },
                    "Invalid alternative label",
                ),
                (
                    "control in nickname",
                    |d| {
                        d.generic.alternatives[0].choices[0] = Alternative::Unresolved {
                            label: "R\n".into(),
                        }
                    },
                    "Invalid alternative label",
                ),
                (
                    "long label",
                    |d| {
                        d.generic.alternatives[0].choices[0] = Alternative::Group {
                            label: "x".repeat(65),
                        }
                    },
                    "Invalid alternative label",
                ),
                (
                    "duplicate element",
                    |d| d.generic.alternatives[0].choices[1] = element("Cl", 0),
                    "Duplicate alternative choice",
                ),
                (
                    "group and nickname share a key",
                    |d| {
                        d.generic.alternatives[0].choices = vec![
                            Alternative::Group { label: "R".into() },
                            Alternative::Unresolved { label: "R".into() },
                        ]
                    },
                    "Duplicate alternative choice",
                ),
            ],
        );

        let mut doc = alternative_document();
        doc.generic.alternatives[0].excluded = true;
        doc.generic.alternatives[0].choices = vec![element("H", 65_535), element("H", 2)];
        assert_eq!(super::validate(&doc), Ok(()));
        // Current behavior: element keys and literal label keys share a namespace.
        doc.generic.alternatives[0].choices = vec![
            element("C", 0),
            Alternative::Group {
                label: "C:0".into(),
            },
        ];
        assert_eq!(
            super::validate(&doc),
            Err("Duplicate alternative choice".into())
        );
    }

    #[test]
    fn link_validation_errors_and_precedence() {
        check_invalid_cases(
            link_document(),
            &[
                (
                    "range precedes missing atom",
                    |d| {
                        d.generic.links[0].atom = 999;
                        d.generic.links[0].min = 0;
                    },
                    "Repeat counts must be an ordered range from 1 to 1000",
                ),
                (
                    "missing atom",
                    |d| d.generic.links[0].atom = 999,
                    "Missing link atom",
                ),
                (
                    "duplicate link",
                    |d| d.generic.links.push(d.generic.links[0].clone()),
                    "A link node requires exactly two connecting bonds",
                ),
                (
                    "alternative site overlap",
                    |d| {
                        d.atoms[1].element = "*".into();
                        d.generic.alternatives.push(AlternativeSite {
                            atom: 2,
                            choices: vec![element("C", 0)],
                            excluded: false,
                        });
                    },
                    "A link node requires exactly two connecting bonds",
                ),
                (
                    "attachment",
                    |d| d.atoms[1].attachment = Some(crate::attachments::Kind::Variable),
                    "A link node requires exactly two connecting bonds",
                ),
                (
                    "centroid",
                    |d| d.atoms[1].centroid = vec![1, 3],
                    "A link node requires exactly two connecting bonds",
                ),
                (
                    "one bond",
                    |d| {
                        d.bonds.pop();
                    },
                    "A link node requires exactly two connecting bonds",
                ),
                (
                    "three bonds",
                    |d| {
                        let id = d.add_atom("C", Point::new(42., 42.));
                        d.add_bond(2, id, 1, "plain");
                    },
                    "A link node requires exactly two connecting bonds",
                ),
            ],
        );
    }

    #[test]
    fn repeat_validation_errors_and_precedence() {
        check_invalid_cases(
            repeat_document(),
            &[
                (
                    "member budget precedes bracket lookup",
                    |d| {
                        let unit = &mut d.generic.repeats[0];
                        unit.crossings.resize(100_001, unit.crossings[0].clone());
                        unit.bracket = Some(999);
                    },
                    "Invalid repeating unit",
                ),
                (
                    "empty membership",
                    |d| d.generic.repeats[0].members.clear(),
                    "Invalid repeating unit",
                ),
                (
                    "duplicate member",
                    |d| d.generic.repeats[0].members.push(2),
                    "Invalid repeating unit",
                ),
                (
                    "missing member",
                    |d| d.generic.repeats[0].members[0] = 999,
                    "Invalid repeating unit",
                ),
                (
                    "empty usage",
                    |d| d.generic.repeats[0].usage.clear(),
                    "Invalid repeating unit",
                ),
                (
                    "control in label",
                    |d| d.generic.repeats[0].label = "n\t".into(),
                    "Invalid repeating unit",
                ),
                (
                    "missing primary bracket",
                    |d| d.generic.repeats[0].bracket = Some(999),
                    "Missing repeating-unit bracket",
                ),
                (
                    "primary graphic is not a bracket",
                    |d| d.graphics[0].kind = GraphicKind::Rectangle,
                    "Missing repeating-unit bracket",
                ),
                (
                    "missing additional bracket",
                    |d| d.generic.repeats[0].bracket_objects = vec![999],
                    "Missing additional repeating-unit bracket",
                ),
                (
                    "additional graphic is not a bracket",
                    |d| d.graphics[1].kind = GraphicKind::Line,
                    "Missing additional repeating-unit bracket",
                ),
                (
                    "association length",
                    |d| {
                        d.generic.repeats[0].crossing_brackets.pop();
                    },
                    "Invalid crossing-bond bracket association",
                ),
                (
                    "association outside bracket set",
                    |d| d.generic.repeats[0].crossing_brackets[0] = 999,
                    "Invalid crossing-bond bracket association",
                ),
                (
                    "zero heads",
                    |d| d.generic.repeats[0].head_count = Some(0),
                    "Invalid repeat head-bond count",
                ),
                (
                    "too many heads",
                    |d| d.generic.repeats[0].head_count = Some(3),
                    "Invalid repeat head-bond count",
                ),
                (
                    "invalid finite count",
                    |d| d.generic.repeats[0].count = RepeatCount::Finite { min: 2, max: 1 },
                    "Repeat counts must be an ordered range from 1 to 1000",
                ),
                (
                    "empty indefinite label",
                    |d| {
                        d.generic.repeats[0].count = RepeatCount::Indefinite {
                            label: String::new(),
                        }
                    },
                    "Invalid indefinite repeat label",
                ),
                (
                    "missing crossing",
                    |d| {
                        d.generic.repeats[0].crossings.pop();
                        d.generic.repeats[0].crossing_brackets.pop();
                    },
                    "Repeating-unit crossing bonds must match its atom membership",
                ),
                (
                    "duplicate crossing",
                    |d| {
                        let crossing = d.generic.repeats[0].crossings[0].clone();
                        d.generic.repeats[0].crossings.push(crossing);
                        d.generic.repeats[0].crossing_brackets.push(5);
                    },
                    "Repeating-unit crossing bonds must match its atom membership",
                ),
                (
                    "reversed crossing",
                    |d| {
                        let c = &mut d.generic.repeats[0].crossings[0];
                        std::mem::swap(&mut c.inside, &mut c.outside);
                    },
                    "Repeating-unit crossing bonds must match its atom membership",
                ),
                (
                    "crossing does not exist",
                    |d| d.generic.repeats[0].crossings[0].outside = 4,
                    "Repeating-unit crossing bonds must match its atom membership",
                ),
            ],
        );
        let mut doc = repeat_document();
        let unit = &mut doc.generic.repeats[0];
        unit.crossing_brackets.clear();
        unit.head_count = None;
        unit.count = RepeatCount::Indefinite {
            label: "é".repeat(33),
        };
        assert_eq!(super::validate(&doc), Ok(()));
        assert_eq!(super::validate(&Document::default()), Ok(()));
    }

    #[test]
    fn range_printability_and_unordered_pair_boundaries() {
        for (min, max, valid) in [
            (1, 1, true),
            (1, 1000, true),
            (1000, 1000, true),
            (0, 1, false),
            (2, 1, false),
            (1, 1001, false),
        ] {
            let expected = if valid {
                Ok(())
            } else {
                Err("Repeat counts must be an ordered range from 1 to 1000".into())
            };
            assert_eq!(super::check_range(min, max), expected, "{min}-{max}");
        }
        for (text, expected) in [
            (String::new(), false),
            (" ".into(), true),
            ("x".repeat(64), true),
            ("x".repeat(65), false),
            ("é".repeat(64), true),
            ("é".repeat(65), false),
            ("n\n".into(), false),
            ("n\0".into(), false),
        ] {
            assert_eq!(super::printable(&text), expected, "{text:?}");
        }
        for (a, b, expected) in [
            (1, 9, (1, 9)),
            (9, 1, (1, 9)),
            (5, 5, (5, 5)),
            (u64::MAX, 0, (0, u64::MAX)),
        ] {
            assert_eq!(super::pair(a, b), expected);
        }
    }

    #[test]
    fn pruning_removes_missing_sites_and_clears_only_removed_link_labels() {
        let mut doc = link_document();
        doc.generic.links.push(LinkNode {
            atom: 999,
            min: 1,
            max: 3,
        });
        doc.generic.alternatives = vec![
            AlternativeSite {
                atom: 1,
                choices: vec![],
                excluded: false,
            },
            AlternativeSite {
                atom: 999,
                choices: vec![],
                excluded: false,
            },
        ];
        doc.atoms[0].display.variable = Some("retained".into());
        doc.bonds.pop();
        super::prune(&mut doc);
        assert!(doc.generic.links.is_empty());
        assert_eq!(doc.atom(2).unwrap().display.variable, None);
        assert_eq!(
            doc.atom(1).unwrap().display.variable.as_deref(),
            Some("retained")
        );
        // Current behavior: pruning checks site existence, not full validation.
        assert_eq!(doc.generic.alternatives.len(), 1);
        assert_eq!(doc.generic.alternatives[0].atom, 1);

        let mut retained = link_document();
        let before = retained.clone();
        super::prune(&mut retained);
        assert_eq!(retained, before);
    }

    #[test]
    fn repeat_pruning_checks_existence_and_retained_crossing_bonds() {
        let cases: [MutationCase; 6] = [
            ("member", |d| d.atoms.retain(|a| a.id != 2)),
            ("outside atom", |d| d.atoms.retain(|a| a.id != 1)),
            ("primary bracket", |d| d.graphics.retain(|g| g.id != 5)),
            ("additional bracket", |d| d.graphics.retain(|g| g.id != 6)),
            ("crossing bond", |d| {
                d.bonds.retain(|b| super::pair(b.a, b.b) != (1, 2))
            }),
            ("missing inside endpoint", |d| {
                d.generic.repeats[0].crossings[0].inside = 999
            }),
        ];
        for (name, mutate) in cases {
            let mut doc = repeat_document();
            mutate(&mut doc);
            super::prune(&mut doc);
            assert!(doc.generic.repeats.is_empty(), "{name}");
        }
        let mut retained = repeat_document();
        for bond in &mut retained.bonds {
            std::mem::swap(&mut bond.a, &mut bond.b);
        }
        // Current behavior: existing graphics and bonds retain invalid metadata.
        retained.graphics[0].kind = GraphicKind::Rectangle;
        retained.generic.repeats[0].crossing_brackets = vec![999];
        retained.generic.repeats[0].head_count = Some(0);
        let before = retained.clone();
        super::prune(&mut retained);
        assert_eq!(retained, before);
    }

    #[test]
    fn selection_is_a_sorted_transitive_closure_of_repeat_dependencies() {
        let mut doc = repeat_document();
        let mut second = doc.generic.repeats[0].clone();
        second.bracket = Some(9);
        second.bracket_objects = vec![10];
        second.members = vec![7];
        second.crossings = vec![
            Crossing {
                inside: 7,
                outside: 4,
            },
            Crossing {
                inside: 7,
                outside: 8,
            },
        ];
        // Visit the second unit first: the seed must require another closure pass.
        doc.generic.repeats.insert(0, second);
        for ids in [&[1][..], &[2], &[4], &[5], &[6], &[7], &[8], &[9], &[10]] {
            assert_eq!(
                super::selection(&doc, ids),
                (1..=10).collect::<Vec<_>>(),
                "{ids:?}"
            );
        }
        assert_eq!(super::selection(&doc, &[]), Vec::<u64>::new());
        assert_eq!(super::selection(&doc, &[99, 98, 99]), vec![98, 99]);
        assert_eq!(
            super::selection(&doc, &[99, 1, 99]),
            (1..=10).chain([99]).collect::<Vec<_>>()
        );
        assert_eq!(super::selection(&alternative_document(), &[1, 1]), vec![1]);
        assert_eq!(super::selection(&link_document(), &[2]), vec![2]);
    }
}

mod authoring {
    use super::*;

    #[test]
    fn alternative_lists_keep_prefixes_isotopes_and_choice_kinds() {
        for (text, choices, excluded) in [
            (
                " [Cl, Br] ",
                vec![element("Cl", 0), element("Br", 0)],
                false,
            ),
            ("NOT C,N", vec![element("C", 0), element("N", 0)], true),
            ("[-C,N]", vec![element("C", 0), element("N", 0)], true),
            ("[NOT D,T]", vec![element("H", 2), element("H", 3)], true),
            ("[D,T]", vec![element("H", 2), element("H", 3)], false),
            (
                "[OMe,OPh]",
                vec![
                    Alternative::Group {
                        label: "OMe".into(),
                    },
                    Alternative::Group {
                        label: "OPh".into(),
                    },
                ],
                false,
            ),
            (
                "[R,X,R12]",
                vec![
                    Alternative::Unresolved { label: "R".into() },
                    Alternative::Unresolved { label: "X".into() },
                    Alternative::Unresolved {
                        label: "R12".into(),
                    },
                ],
                false,
            ),
        ] {
            assert_eq!(
                super::alternative(text),
                Ok(Some((choices, excluded))),
                "{text}"
            );
        }
        // Current behavior: comma-free input bypasses bracket mismatch checks.
        for text in ["", "Cl", "[Cl]", "[Cl", "Cl]", "NOT C"] {
            assert_eq!(super::alternative(text), Ok(None), "{text}");
        }
        // Current behavior: parsing retains duplicates; validate rejects them.
        assert_eq!(
            super::alternative("[C,C]"),
            Ok(Some((vec![element("C", 0); 2], false)))
        );
        let maximum = vec!["C"; 118].join(",");
        assert_eq!(super::alternative(&maximum).unwrap().unwrap().0.len(), 118);
    }

    #[test]
    fn alternative_errors_are_exact() {
        for (text, message) in [
            ("[Cl,Br", "Use matching brackets for an alternative list"),
            ("Cl,Br]", "Use matching brackets for an alternative list"),
            (
                "[Cl,,Br]",
                "Each alternative must be a printable element or defined group",
            ),
            (
                "[Cl,R\nX]",
                "Each alternative must be a printable element or defined group",
            ),
            ("[Cl,?]", "Alternative ? has no defined chemical fragment"),
            (
                "[-OMe,Cl]",
                "Alternative OMe has no defined chemical fragment",
            ),
            (
                "[NOT R,Cl]",
                "Alternative R has no defined chemical fragment",
            ),
            (
                "[Rfoo,Cl]",
                "Alternative Rfoo has no defined chemical fragment",
            ),
            (
                "[[Cl,Br]]",
                "Alternative [Cl has no defined chemical fragment",
            ),
        ] {
            assert_eq!(super::alternative(text), Err(message.into()), "{text:?}");
        }
        assert_eq!(
            super::alternative(&vec!["C"; 119].join(",")),
            Err("Too many alternatives".into())
        );
        assert_eq!(
            super::alternative(&format!("C,{}", "R".repeat(65))),
            Err("Each alternative must be a printable element or defined group".into())
        );
    }

    #[test]
    fn repeat_count_parsing_and_byte_length_quirk() {
        for (label, expected) in [
            (" 1 ", RepeatCount::Finite { min: 1, max: 1 }),
            ("1 - 3", RepeatCount::Finite { min: 1, max: 3 }),
            (
                "1000",
                RepeatCount::Finite {
                    min: 1000,
                    max: 1000,
                },
            ),
            (" n ", RepeatCount::Indefinite { label: "n".into() }),
            (
                "1-x",
                RepeatCount::Indefinite {
                    label: "1-x".into(),
                },
            ),
            (
                "1-2-3",
                RepeatCount::Indefinite {
                    label: "1-2-3".into(),
                },
            ),
            (
                "1–3",
                RepeatCount::Indefinite {
                    label: "1–3".into(),
                },
            ),
        ] {
            assert_eq!(RepeatCount::parse(label), Ok(expected), "{label:?}");
        }
        for (label, message) in [
            ("", "Enter a repeat count or label, such as 1-3 or n"),
            ("  ", "Enter a repeat count or label, such as 1-3 or n"),
            ("n\0", "Enter a repeat count or label, such as 1-3 or n"),
            ("0", "Repeat counts must be an ordered range from 1 to 1000"),
            (
                "3-1",
                "Repeat counts must be an ordered range from 1 to 1000",
            ),
            (
                "1-1001",
                "Repeat counts must be an ordered range from 1 to 1000",
            ),
            ("-1", "Invalid repeat count"),
            ("1-", "Invalid repeat count"),
            ("1-4294967296", "Invalid repeat count"),
        ] {
            assert_eq!(RepeatCount::parse(label), Err(message.into()), "{label:?}");
        }
        // Current behavior: an overflowing singleton number becomes a label.
        assert_eq!(
            RepeatCount::parse("4294967296"),
            Ok(RepeatCount::Indefinite {
                label: "4294967296".into()
            })
        );
        for (label, accepted) in [
            ("x".repeat(64), true),
            ("x".repeat(65), false),
            ("é".repeat(32), true),
            ("é".repeat(33), false),
        ] {
            let expected = if accepted {
                Ok(RepeatCount::Indefinite {
                    label: label.clone(),
                })
            } else {
                Err("Enter a repeat count or label, such as 1-3 or n".into())
            };
            assert_eq!(RepeatCount::parse(&label), expected);
        }
        // Current behavior: printable counts characters; parse counts UTF-8 bytes.
        let non_ascii = "é".repeat(33);
        assert!(super::printable(&non_ascii));
        assert_eq!(
            RepeatCount::parse(&non_ascii),
            Err("Enter a repeat count or label, such as 1-3 or n".into())
        );
    }

    #[test]
    fn link_labels_accept_delimited_hydrides_and_preserve_the_source() {
        let source = chain(&["C", "C", "C"]);
        for text in ["[CH2]1-3", "(CH2)1-3", "{CH2}1-3", "C1-3"] {
            let before = source.clone();
            let result = super::apply_label(&source, 2, text).unwrap().unwrap();
            assert_eq!(source, before);
            assert_eq!(
                result.generic.links,
                vec![LinkNode {
                    atom: 2,
                    min: 1,
                    max: 3
                }]
            );
            assert_eq!(result.atom(2).unwrap().element, "C");
            assert_eq!(
                result.atom(2).unwrap().explicit_h,
                if text == "C1-3" { 0 } else { 2 }
            );
            assert_eq!(
                result.atom(2).unwrap().display.variable.as_deref(),
                Some(text)
            );
        }
        // Current behavior: CH21-3 is split at the first digit, producing 21-3.
        for (text, message) in [
            (
                "CH21-3",
                "Repeat counts must be an ordered range from 1 to 1000",
            ),
            (
                "(CH2)0-3",
                "Repeat counts must be an ordered range from 1 to 1000",
            ),
            ("{CH2}n", "A link node requires a finite number range"),
            ("[CH2]1-", "Invalid repeat count"),
        ] {
            assert_eq!(
                super::apply_label(&source, 2, text),
                Err(message.into()),
                "{text}"
            );
        }
        for text in ["C", "CH2", "(CH2)", "{CH2}"] {
            assert_eq!(super::apply_label(&source, 2, text), Ok(None), "{text}");
        }
        assert_eq!(
            super::apply_label(&source, 1, "(CH2)1-3"),
            Err("A link node requires exactly two connecting bonds".into())
        );
        assert_eq!(
            super::apply_label(&source, 999, "[Cl,Br]"),
            Err("Missing alternative atom".into())
        );
    }

    #[test]
    fn alternative_authoring_resets_atom_fields_and_replaces_existing_site() {
        let mut source = link_document();
        let atom = source.atom_mut(2).unwrap();
        atom.charge = 1;
        atom.isotope = 13;
        atom.explicit_h = 2;
        atom.aromatic = true;
        atom.label_h = 1;
        let before = source.clone();
        let result = super::apply_label(&source, 2, "[D,OPh,R12]")
            .unwrap()
            .unwrap();
        assert_eq!(source, before);
        assert!(result.generic.links.is_empty());
        let atom = result.atom(2).unwrap();
        assert_eq!(atom.element, "*");
        assert!(atom.no_implicit);
        assert_eq!(
            (atom.charge, atom.isotope, atom.explicit_h, atom.label_h),
            (0, 0, 0, 0)
        );
        assert!(!atom.aromatic);
        assert_eq!(atom.display.variable.as_deref(), Some("[D,OPh,R12]"));
        assert_eq!(
            result.generic.alternatives,
            vec![AlternativeSite {
                atom: 2,
                choices: vec![
                    element("H", 2),
                    Alternative::Group {
                        label: "OPh".into()
                    },
                    Alternative::Unresolved {
                        label: "R12".into()
                    }
                ],
                excluded: false
            }]
        );
        assert_eq!(
            super::apply_label(&source, 2, "[C,C]"),
            Err("Duplicate alternative choice".into())
        );
        source.atom_mut(2).unwrap().centroid = vec![1, 3];
        assert_eq!(
            super::apply_label(&source, 2, "[Cl,Br]"),
            Err("Use an ordinary atom for alternatives".into())
        );
    }

    #[test]
    fn bracket_crossings_follow_axis_projection_and_stable_ties() {
        for (axis_x, origin, expected) in [
            (
                Point::new(0., 84.),
                Point::new(30., -20.),
                vec![
                    Crossing {
                        inside: 2,
                        outside: 5,
                    },
                    Crossing {
                        inside: 2,
                        outside: 4,
                    },
                    Crossing {
                        inside: 1,
                        outside: 3,
                    },
                ],
            ),
            (
                Point::new(0., -84.),
                Point::new(30., 64.),
                vec![
                    Crossing {
                        inside: 1,
                        outside: 3,
                    },
                    Crossing {
                        inside: 2,
                        outside: 5,
                    },
                    Crossing {
                        inside: 2,
                        outside: 4,
                    },
                ],
            ),
        ] {
            let mut source = Document::default();
            for position in [
                Point::new(0., 40.),
                Point::new(0., 0.),
                Point::new(-100., 110.),
                Point::new(100., -60.),
                Point::new(-100., -60.),
            ] {
                source.add_atom("C", position);
            }
            for (a, b) in [(1, 3), (2, 5), (4, 2), (1, 2)] {
                source.add_bond(a, b, 1, "plain");
            }
            let id = bracket(&mut source);
            let graphic = source.graphics.last_mut().unwrap();
            graphic.origin = origin;
            graphic.axis_x = axis_x;
            graphic.axis_y = Point::new(-60., 0.);
            let before = source.clone();
            let result =
                super::define_bracket(&source, id, " 1-3 ", RepeatPattern::HeadToHead).unwrap();
            assert_eq!(source, before);
            let unit = &result.generic.repeats[0];
            assert_eq!(unit.members, vec![1, 2]);
            assert_eq!(unit.crossings, expected);
            assert_eq!(unit.head_count, None);
            assert_eq!(unit.pattern, RepeatPattern::HeadToHead);
            assert_eq!(unit.label, "1-3");
            assert_eq!(unit.count, RepeatCount::Finite { min: 1, max: 3 });
        }
        let mut source = chain(&["O", "C", "C", "N"]);
        let id = bracket(&mut source);
        let result = super::define_bracket(&source, id, "n", RepeatPattern::EitherUnknown).unwrap();
        assert_eq!(result.generic.repeats[0].head_count, Some(1));
    }

    #[test]
    fn bracket_authoring_errors_are_exact_and_leave_the_source_unchanged() {
        let cases: [InvalidCase; 5] = [
            (
                "missing bracket",
                |d| d.graphics.clear(),
                "Select one bracket around the repeating unit",
            ),
            (
                "wrong graphic",
                |d| d.graphics[0].kind = GraphicKind::Rectangle,
                "Select one bracket around the repeating unit",
            ),
            (
                "zero area",
                |d| d.graphics[0].axis_x = Point::default(),
                "Draw a bracket with a nonzero enclosed area",
            ),
            (
                "empty frame",
                |d| d.graphics[0].origin = Point::new(1000., 1000.),
                "The bracket encloses no atoms",
            ),
            (
                "validation precedes bracket lookup",
                |d| {
                    d.graphics.clear();
                    d.generic.alternatives.push(AlternativeSite {
                        atom: 999,
                        choices: vec![element("C", 0)],
                        excluded: false,
                    });
                },
                "Missing alternative atom",
            ),
        ];
        for (name, mutate, message) in cases {
            let mut source = chain(&["O", "C", "C", "N"]);
            let id = bracket(&mut source);
            mutate(&mut source);
            let before = source.clone();
            assert_eq!(
                super::define_bracket(&source, id, "1-3", RepeatPattern::HeadToTail),
                Err(message.into()),
                "{name}"
            );
            assert_eq!(source, before);
        }
    }
}

mod guards {
    use super::*;
    use crate::arrow_anchors::{Anchor, Target};
    use crate::document::Arrow;

    fn referenced_document() -> Document {
        let mut doc = chain(&["*", "C", "C", "C", "C", "C", "C", "*", "C"]);
        doc.bonds.clear();
        doc.generic.alternatives.push(AlternativeSite {
            atom: 1,
            choices: vec![element("C", 0)],
            excluded: false,
        });
        doc.generic.links.push(LinkNode {
            atom: 2,
            min: 1,
            max: 3,
        });
        let mut unit = repeat_document().generic.repeats.remove(0);
        unit.bracket = Some(10);
        unit.bracket_objects = vec![11];
        unit.members = vec![3, 4];
        unit.crossings = vec![Crossing {
            inside: 3,
            outside: 5,
        }];
        unit.crossing_brackets = vec![12];
        doc.generic.repeats.push(unit);
        let variable = doc.atom_mut(8).unwrap();
        variable.attachment = Some(crate::attachments::Kind::Variable);
        variable.centroid = vec![7, 6];
        doc
    }

    #[test]
    fn intersection_checks_each_drawing_reference_without_connectivity() {
        let doc = referenced_document();
        for id in 1..=12 {
            assert_eq!(super::intersects(&doc, &[id]), id != 9, "id {id}");
        }
        for ids in [&[][..], &[9], &[99], &[9, 99]] {
            assert!(!super::intersects(&doc, ids), "{ids:?}");
        }
        assert!(super::intersects(&doc, &[99, 7]));
        assert!(!super::intersects(&Document::default(), &[1]));
    }

    #[test]
    fn atom_references_preserve_metadata_order_and_duplicate_atoms() {
        let doc = referenced_document();
        assert_eq!(
            super::atom_references(&doc).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 3, 5, 8, 7, 6]
        );
        // Bracket graphics do not seed chemical components.
        for ids in [&[10][..], &[11], &[12], &[9], &[]] {
            assert!(!super::component_intersects(&doc, ids), "{ids:?}");
        }
        assert!(super::component_intersects(&doc, &[5]));
        assert_eq!(super::atom_references(&Document::default()).count(), 0);
    }

    #[test]
    fn component_intersection_traverses_centroid_edges_without_bonds() {
        let mut doc = chain(&["C", "*", "*", "C"]);
        doc.bonds.clear();
        doc.atom_mut(2).unwrap().centroid = vec![1];
        let variable = doc.atom_mut(3).unwrap();
        variable.attachment = Some(crate::attachments::Kind::Variable);
        variable.centroid = vec![2];
        assert!(!super::intersects(&doc, &[1]));
        for (ids, expected) in [
            (&[1][..], true),
            (&[2], true),
            (&[3], true),
            (&[4], false),
            (&[], false),
        ] {
            assert_eq!(super::component_intersects(&doc, ids), expected, "{ids:?}");
        }
        // The only path from atom 1 is the reverse direction of a centroid edge.
        doc.atom_mut(2).unwrap().centroid.clear();
        assert!(!super::component_intersects(&doc, &[1]));
    }

    fn anchor(target: Target) -> Anchor {
        Anchor {
            target,
            offset: Point::default(),
            direction: Point::new(1., 0.),
            gap: 0.,
        }
    }

    #[test]
    fn only_selected_arrows_seed_atom_lone_pair_and_bond_anchor_components() {
        for target in [
            Target::Atom { atom: 1 },
            Target::LonePair { atom: 1, mark: 1 },
            Target::Bond {
                a: 1,
                b: 2,
                fraction: 0.5,
            },
        ] {
            for use_start in [true, false] {
                let mut doc = chain(&["O", "C", "*", "C", "C"]);
                doc.bonds.retain(|b| super::pair(b.a, b.b) != (3, 4));
                doc.generic.alternatives.push(AlternativeSite {
                    atom: 3,
                    choices: vec![element("Cl", 0)],
                    excluded: false,
                });
                doc.atom_mut(1)
                    .unwrap()
                    .marks
                    .push(crate::scientific::AtomMark {
                        id: Some(1),
                        kind: crate::scientific::MarkKind::LonePair,
                        offset: Point::new(0., -20.),
                        angle: 0.,
                        size_pt: None,
                    });
                let id = doc.next_id();
                let unresolved_anchor = anchor(target.clone());
                let ordinary_anchor = anchor(Target::Atom { atom: 4 });
                let (start_anchor, end_anchor) = if use_start {
                    (unresolved_anchor, ordinary_anchor)
                } else {
                    (ordinary_anchor, unresolved_anchor)
                };
                doc.arrows.push(Arrow {
                    id,
                    start: Point::new(0., -30.),
                    end: Point::new(168., -30.),
                    start_anchor: Some(start_anchor),
                    end_anchor: Some(end_anchor),
                    kind: "forward".into(),
                    control: None,
                    cubic: None,
                    style: None,
                });
                assert!(!super::intersects(&doc, &[id]));
                assert!(
                    super::component_intersects(&doc, &[id]),
                    "{target:?}, start={use_start}"
                );
                // An unselected linked arrow must not connect the ordinary component.
                assert!(
                    !super::component_intersects(&doc, &[4, 5]),
                    "{target:?}, start={use_start}"
                );
                assert!(super::component_intersects(&doc, &[4, id]));
                assert!(super::component_intersects(&doc, &[1]));
                doc.arrows[0].start_anchor = None;
                doc.arrows[0].end_anchor = None;
                assert!(!super::component_intersects(&doc, &[id]));
            }
        }
        assert!(!super::component_intersects(&chain(&["C", "C"]), &[1]));
    }

    #[test]
    fn selected_bond_anchor_seeds_both_endpoints_without_a_target_bond() {
        // Current behavior: guards accept incomplete drawings without validation.
        // Keep the target endpoints disconnected so either missing seed is visible.
        for (a, b) in [(1, 2), (2, 1)] {
            let mut doc = chain(&["C", "C", "*"]);
            doc.bonds.clear();
            doc.add_bond(1, 3, 1, "plain");
            doc.generic.alternatives.push(AlternativeSite {
                atom: 3,
                choices: vec![element("Cl", 0)],
                excluded: false,
            });
            let id = doc.next_id();
            doc.arrows.push(Arrow {
                id,
                start: Point::new(0., -30.),
                end: Point::new(84., -30.),
                start_anchor: Some(anchor(Target::Bond {
                    a,
                    b,
                    fraction: 0.5,
                })),
                end_anchor: None,
                kind: "forward".into(),
                control: None,
                cubic: None,
                style: None,
            });
            assert!(!super::component_intersects(&doc, &[2]));
            assert!(super::component_intersects(&doc, &[id]), "{a}, {b}");
            doc.arrows[0].start_anchor = None;
            assert!(!super::component_intersects(&doc, &[id]));
        }
    }
}
