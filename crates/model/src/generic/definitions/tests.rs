use crate::document::{Document, Point};
use crate::generic::test_support::{
    InvalidCase, alternative_document, element, link_document, repeat_document,
};
use crate::generic::{Alternative, AlternativeSite, Crossing, LinkNode, RepeatCount};
use crate::graphics::GraphicKind;

type MutationCase = (&'static str, fn(&mut Document));

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
