use crate::document::{Document, Point};
use crate::generic::test_support::{InvalidCase, bracket, chain, element, link_document};
use crate::generic::{
    Alternative, AlternativeSite, Crossing, LinkNode, RepeatCount, RepeatPattern,
};
use crate::graphics::GraphicKind;

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
