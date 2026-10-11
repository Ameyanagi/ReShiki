use super::*;
use crate::{
    atom_text::{self, Mode},
    document::Point,
};

fn chain(elements: &[&str]) -> Document {
    let mut doc = Document::default();
    let mut last = None;
    for (i, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(i as f32 * 42., 0.));
        if let Some(previous) = last {
            doc.add_bond(previous, id, 1, "plain");
        }
        last = Some(id);
    }
    doc
}
#[test]
fn two_mixed_anonymous_sites_are_independent_real_fragments() {
    let source = chain(&["*", "C", "C", "*"]);
    let source = atom_text::apply(&source, 1, "[OMe,OPh]", Mode::Auto).unwrap();
    let source = atom_text::apply(&source, 4, "[Cl,Br]", Mode::Auto).unwrap();
    let before = source.clone();
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(source, before);
    assert_eq!(products.len(), 4);
    let mut counts: Vec<_> = products.iter().map(|d| d.atoms.len()).collect();
    counts.sort();
    assert_eq!(counts, vec![5, 5, 10, 10]);
    for product in products {
        assert!(!present(&product));
        assert_eq!(product.atom(1).unwrap().element, "O");
        let neighbors: Vec<_> = product
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == 1 {
                    Some(b.b)
                } else if b.b == 1 {
                    Some(b.a)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&2));
        let group_root = *neighbors.iter().find(|id| **id != 2).unwrap();
        assert_eq!(product.atom(group_root).unwrap().element, "C");
        assert_eq!(
            bonds_at(&product, group_root).len(),
            if product.atoms.len() == 5 { 1 } else { 3 }
        );
        assert!(crate::chemistry::document::prepare(&product).is_ok());
    }
}
#[test]
fn finite_link_nodes_expand_chain_and_ring() {
    for ring in [false, true] {
        let mut source = chain(&["C", "C", "C", "C"]);
        if ring {
            source.add_bond(1, 4, 1, "plain");
        }
        let source = atom_text::apply(&source, 2, "[CH2]1-3", Mode::Auto).unwrap();
        let products = expand(&source, Limits::default()).unwrap();
        assert_eq!(
            products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
            vec![4, 5, 6]
        );
        assert_eq!(
            products.iter().map(|d| d.bonds.len()).collect::<Vec<_>>(),
            if ring { vec![4, 5, 6] } else { vec![3, 4, 5] }
        );
    }
}
#[test]
fn variable_attachment_chooses_one_real_target() {
    let mut source = chain(&["C", "C", "C"]);
    let point =
        crate::attachments::add(&mut source, &[1, 2, 3], crate::attachments::Kind::Variable)
            .unwrap();
    let cl = source.add_atom("Cl", Point::new(42., 42.));
    source.add_bond(point, cl, 1, "plain");
    let products = expand(&source, Limits::default()).unwrap();
    // Propane's terminal targets 1 and 3 both produce 1-chloropropane.
    // Keep the first terminal representative, followed by 2-chloropropane.
    assert_eq!(products.len(), 2);
    for (product, target) in products.iter().zip([1, 2]) {
        assert!(product.atom(point).is_none());
        assert!(
            product
                .bonds
                .iter()
                .any(|b| pair(b.a, b.b) == pair(target, cl))
        );
    }
}
#[test]
fn finite_polymer_keeps_both_end_groups() {
    let mut source = chain(&["O", "C", "C", "N"]);
    set_repeat_unit(
        &mut source,
        RepeatUnit {
            bracket: None,
            bracket_objects: vec![],
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
            usage: "SRU".into(),
            crossing_brackets: vec![],
            head_count: Some(1),
            pattern: RepeatPattern::HeadToTail,
            count: RepeatCount::Finite { min: 1, max: 3 },
            label: "1-3".into(),
            flip: "NoFlip".into(),
        },
    )
    .unwrap();
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(
        products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
        vec![4, 6, 8]
    );
    for p in products {
        assert_eq!(p.atoms.iter().filter(|a| a.element == "O").count(), 1);
        assert_eq!(p.atoms.iter().filter(|a| a.element == "N").count(), 1);
        assert_eq!(p.bonds.len(), p.atoms.len() - 1);
    }
}
#[test]
fn malformed_or_indefinite_choices_are_atomic_and_bounded() {
    let source = chain(&["C", "C", "C"]);
    for label in ["[CH2]0-3", "[CH2]3-1", "[Cl,,Br]", "[OMe,?]"] {
        assert!(atom_text::apply(&source, 2, label, Mode::Auto).is_err());
    }
    let source = atom_text::apply(&source, 1, "[Cl,Br]", Mode::Auto).unwrap();
    assert!(
        expand(
            &source,
            Limits {
                max_outputs: 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    assert!(crate::chemistry::document::prepare(&source).is_err());
    let bytes = source.file_json().unwrap();
    assert_eq!(Document::from_json(&bytes).unwrap().generic, source.generic);
}

#[test]
fn copy_remaps_typed_sites_and_native_reopen_keeps_finite_and_indefinite_units() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let bracket = source.next_id();
    source.graphics.push(crate::graphics::Graphic::dragged(
        bracket,
        crate::graphics::GraphicKind::Brackets,
        Point::new(20., -30.),
        Point::new(104., 30.),
        Default::default(),
        crate::graphics::BracketSides::Both,
        false,
    ));
    source = define_bracket(&source, bracket, "n", RepeatPattern::HeadToTail).unwrap();
    let selected = crate::editing::selection(&source, &[bracket]);
    assert_eq!(selected.generic.repeats.len(), 1);
    let mut target = chain(&["F"]);
    let inserted = crate::editing::append(&mut target, &selected, Point::new(0., 100.));
    assert_eq!(inserted.len(), 5);
    let unit = target.generic.repeats.first().unwrap();
    assert_eq!(unit.members, vec![3, 4]);
    assert_eq!(
        unit.crossings,
        vec![
            Crossing {
                inside: 3,
                outside: 2
            },
            Crossing {
                inside: 4,
                outside: 5
            }
        ]
    );
    assert_eq!(unit.bracket, Some(6));
    assert_eq!(
        Document::from_native_file(&target.file_json().unwrap())
            .unwrap()
            .generic,
        target.generic
    );
    assert!(
        expand(&target, Limits::default())
            .unwrap_err()
            .contains("indefinite")
    );
    let candidate = define_bracket(&target, 6, "1-3", RepeatPattern::HeadToTail).unwrap();
    assert_eq!(expand(&candidate, Limits::default()).unwrap().len(), 3);
}
#[test]
fn invalid_valence_rejects_the_entire_enumeration_without_changing_source() {
    let source = chain(&["C", "C", "C"]);
    let source = atom_text::apply(&source, 2, "[C,Cl]", Mode::Auto).unwrap();
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("invalid chemistry")
    );
    assert_eq!(source, before);
}
#[test]
fn isotope_choices_materialize_and_multiple_variable_sites_are_independent() {
    let source = chain(&["C", "*"]);
    let source = atom_text::apply(&source, 2, "[H,D,T]", Mode::Auto).unwrap();
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(
        products
            .iter()
            .map(|d| d.atom(2).unwrap().isotope)
            .collect::<Vec<_>>(),
        vec![0, 2, 3]
    );
    let mut source = chain(&["C", "C", "C", "C"]);
    for (members, element, x) in [([1, 2], "F", 0.), ([3, 4], "Cl", 126.)] {
        let point =
            crate::attachments::add(&mut source, &members, crate::attachments::Kind::Variable)
                .unwrap();
        let atom = source.add_atom(element, Point::new(x, 42.));
        source.add_bond(point, atom, 1, "plain");
    }
    assert_eq!(expand(&source, Limits::default()).unwrap().len(), 4);
}
#[test]
fn unsupported_polymer_topology_is_retained_and_explicitly_rejected() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let unit = RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
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
        usage: "SRU".into(),
        crossing_brackets: vec![],
        head_count: Some(1),
        pattern: RepeatPattern::HeadToHead,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "NoFlip".into(),
    };
    set_repeat_unit(&mut source, unit).unwrap();
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("repeat pattern")
    );
    assert_eq!(source, before);
    assert_eq!(
        Document::from_json(&source.file_json().unwrap())
            .unwrap()
            .generic,
        source.generic
    );
}
#[test]
fn atom_and_work_limits_are_checked_before_large_replication() {
    let source = chain(&["C", "C", "C"]);
    let source = atom_text::apply(&source, 2, "[CH2]1000-1000", Mode::Auto).unwrap();
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 10,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom limit")
    );
    assert!(
        expand(
            &source,
            Limits {
                max_work: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(products[0].atoms.len(), 1002);
}

fn unit(members: Vec<u64>, head: Crossing, tail: Crossing, min: u32, max: u32) -> RepeatUnit {
    RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members,
        crossings: vec![head, tail],
        usage: "SRU".into(),
        crossing_brackets: vec![],
        head_count: Some(1),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min, max },
        label: if min == max {
            min.to_string()
        } else {
            format!("{min}-{max}")
        },
        flip: "NoFlip".into(),
    }
}

#[test]
fn authoritative_alkene_stereo_is_rejected_atomically() {
    let mut source = chain(&["Cl", "C", "C", "N"]);
    source.bonds[1].order = 2;
    source.bonds[1].stereo_authoritative = true;
    source.bonds[1].stereo = Some("Z".into());
    source.bonds[1].stereo_atoms = vec![1, 4];
    set_repeat_unit(
        &mut source,
        unit(
            vec![2, 3],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
            2,
            2,
        ),
    )
    .unwrap();
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("stereochemistry")
    );
    assert_eq!(source, before);
}

#[test]
fn link_chiral_tail_and_element_choice_neighbor_stereo_are_rejected_atomically() {
    let mut source = chain(&["C", "C", "C", "F"]);
    let cl = source.add_atom("Cl", Point::new(84., 42.));
    let br = source.add_atom("Br", Point::new(84., -42.));
    source.add_bond(3, cl, 1, "plain");
    source.add_bond(3, br, 1, "plain");
    source.generic.links.push(LinkNode {
        atom: 2,
        min: 2,
        max: 2,
    });
    source.atom_mut(3).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![2, 4, cl, br],
    });
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("stereochemistry")
    );
    assert_eq!(source, before);
    source.generic.links.clear();
    source.atom_mut(2).unwrap().element = "*".into();
    source.atom_mut(2).unwrap().no_implicit = true;
    source.generic.alternatives.push(AlternativeSite {
        atom: 2,
        choices: vec![Alternative::Element {
            symbol: "C".into(),
            isotope: 0,
        }],
        excluded: false,
    });
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("stereochemistry")
    );
    assert_eq!(source, before);
}

#[test]
fn directly_adjacent_units_reject_in_both_definition_orders() {
    let mut source = chain(&["O", "C", "C", "N"]);
    source.generic.repeats = vec![
        unit(
            vec![2],
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 2,
                outside: 3,
            },
            2,
            2,
        ),
        unit(
            vec![3],
            Crossing {
                inside: 3,
                outside: 2,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
            2,
            2,
        ),
    ];
    for _ in 0..2 {
        let before = source.clone();
        assert!(
            expand(&source, Limits::default())
                .unwrap_err()
                .contains("Directly adjacent")
        );
        assert_eq!(source, before);
        source.generic.repeats.reverse();
    }
    source.generic.repeats.remove(1);
    source.generic.links.push(LinkNode {
        atom: 3,
        min: 1,
        max: 2,
    });
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("Directly adjacent")
    );
}

#[test]
fn mapped_repeat_and_link_atoms_are_retained_and_rejected() {
    let mut source = chain(&["O", "C", "C", "N"]);
    source.atom_mut(2).unwrap().map_num = 7;
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        2,
        2,
    ));
    let before = source.clone();
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("Mapped")
    );
    assert_eq!(source, before);
    source.generic.repeats.clear();
    source.generic.links.push(LinkNode {
        atom: 2,
        min: 2,
        max: 2,
    });
    assert!(
        expand(&source, Limits::default())
            .unwrap_err()
            .contains("Mapped")
    );
}

#[test]
fn ring_repeat_closes_on_the_existing_external_component() {
    let mut source = chain(&["C", "C", "C", "C"]);
    source.add_bond(1, 4, 1, "plain");
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1,
        3,
    ));
    let products = expand(&source, Limits::default()).unwrap();
    assert_eq!(
        products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
        vec![4, 6, 8]
    );
    for p in products {
        assert_eq!(p.atoms.len(), p.bonds.len());
        assert!(p.atoms.iter().all(|a| bonds_at(&p, a.id).len() == 2));
        assert_eq!(
            p.atom(1).unwrap().position,
            source.atom(1).unwrap().position
        );
        assert_eq!(
            p.atom(4).unwrap().position,
            source.atom(4).unwrap().position
        );
    }
}

#[test]
fn fragment_catalog_work_and_exact_output_atom_boundaries_are_checked() {
    let source = atom_text::apply(&chain(&["*", "C"]), 1, "[OMe,OPh]", Mode::Auto).unwrap();
    assert!(
        expand(
            &source,
            Limits {
                max_work: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    let source = atom_text::apply(&chain(&["*", "C", "*"]), 1, "[Cl,Br]", Mode::Auto).unwrap();
    let source = atom_text::apply(&source, 3, "[F,I]", Mode::Auto).unwrap();
    assert_eq!(
        expand(
            &source,
            Limits {
                max_outputs: 4,
                max_total_atoms: 12,
                ..Limits::default()
            }
        )
        .unwrap()
        .len(),
        4
    );
    assert!(
        expand(
            &source,
            Limits {
                max_outputs: 3,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("output limit")
    );
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 11,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom limit")
    );
    let mut source = chain(&["O", "C", "C", "N"]);
    source.generic.repeats.push(unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1,
        3,
    ));
    assert_eq!(
        expand(
            &source,
            Limits {
                max_total_atoms: 18,
                ..Limits::default()
            }
        )
        .unwrap()
        .len(),
        3
    );
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 17,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom limit")
    );
}

#[test]
fn large_repeats_check_work_and_source_size_before_graph_construction() {
    let mut source = chain(&vec!["C"; 101]);
    source.generic.repeats.push(unit(
        (2..=100).collect(),
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 100,
            outside: 101,
        },
        1000,
        1000,
    ));
    let before = source.clone();
    assert!(
        expand(
            &source,
            Limits {
                max_work: 1_000_000,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    assert_eq!(source, before);
    // Source validation itself uses a prebuilt ID index, including a large
    // member set. Cheap bounds precede even that validation in expansion.
    source.generic.repeats[0].members = (1..=101).collect();
    source.generic.repeats[0].crossings.clear();
    assert!(
        expand(
            &source,
            Limits {
                max_total_atoms: 100,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("atom or bond limit")
    );
}

#[test]
fn entering_alternative_and_link_labels_preserves_neighbor_stereochemistry() {
    let mut chiral = chain(&["C", "C", "C", "C"]);
    let f = chiral.add_atom("F", Point::new(84., 42.));
    let cl = chiral.add_atom("Cl", Point::new(84., -42.));
    chiral.add_bond(3, f, 1, "plain");
    chiral.add_bond(3, cl, 1, "plain");
    chiral.atom_mut(3).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![2, 4, f, cl],
    });
    let mut alkene = chain(&["C", "C", "C", "C", "N"]);
    alkene.bonds[2].order = 2;
    alkene.bonds[2].stereo_authoritative = true;
    alkene.bonds[2].stereo = Some("Z".into());
    alkene.bonds[2].stereo_atoms = vec![2, 5];
    let mut marker = chiral.clone();
    marker.atom_mut(3).unwrap().stereo = None;
    marker
        .bonds
        .iter_mut()
        .find(|b| pair(b.a, b.b) == pair(3, f))
        .unwrap()
        .display = "wedge".into();
    for source in [chiral, alkene, marker] {
        source.validate().unwrap();
        let before = source.clone();
        for label in ["[C,N]", "[CH2]1-3"] {
            assert!(
                atom_text::apply(&source, 2, label, Mode::Auto)
                    .unwrap_err()
                    .contains("stereochemistry")
            );
            assert_eq!(source, before);
        }
    }
}

#[test]
fn native_current_version_preserves_generic_units_and_still_reads_version19() {
    let mut source = chain(&["O", "C", "C", "N"]);
    let mut repeat = unit(
        vec![2, 3],
        Crossing {
            inside: 2,
            outside: 1,
        },
        Crossing {
            inside: 3,
            outside: 4,
        },
        1,
        3,
    );
    repeat.count = RepeatCount::Indefinite { label: "n".into() };
    repeat.label = "n".into();
    source.generic.repeats.push(repeat);
    let bytes = source.file_json().unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
        crate::document::VERSION
    );
    let reopened = Document::from_native_file(&bytes).unwrap();
    assert_eq!(reopened.version, crate::document::VERSION);
    assert_eq!(reopened.generic, source.generic);
    let older = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/geometry/adamantane.rsk"
    ))
    .unwrap();
    assert_eq!(older.version, 19);
    assert!(older.generic.is_empty());
    assert_eq!(older.atoms.len(), 10);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&older.file_json().unwrap()).unwrap()["version"],
        crate::document::VERSION
    );
    // Clipboard and recovery copies use the same established current gate.
    assert_eq!(source.current().version, crate::document::VERSION);
}

#[test]
fn prepared_join_rejects_remote_generic_source_before_selection_or_deletion() {
    let mut source = chain(&["C", "C", "C"]);
    source.add_atom("C", Point::new(250., 0.));
    let source = atom_text::apply(&source, 3, "[C,N]", Mode::Auto).unwrap();
    let before = source.clone();
    let error = crate::joining::Prepared::new(&source, &[1]).unwrap_err();
    assert_eq!(error, JOIN_NOTICE);
    assert_eq!(source, before);
}

#[test]
fn prepared_join_rechecks_generic_target_and_remote_source_before_placement() {
    use crate::templates::{Anchor, Connection};
    let mut source = Document::default();
    source.add_atom("C", Point::default());
    let start = source.add_atom("C", Point::new(180., 0.));
    let end = source.add_atom("C", Point::new(222., 0.));
    source.add_bond(start, end, 1, "plain");
    let target_generic = atom_text::apply(&source, 1, "[C,N]", Mode::Auto).unwrap();
    let prepared = crate::joining::Prepared::new(&target_generic, &[start]).unwrap();
    let before = target_generic.clone();
    let base = prepared.base.clone();
    let fragment = prepared.fragment.clone();
    assert_eq!(
        prepared
            .place(
                Point::default(),
                None,
                5.,
                Anchor::Atom(start),
                Connection::ShareAtom
            )
            .unwrap_err(),
        JOIN_NOTICE
    );
    assert_eq!(target_generic, before);
    assert_eq!(prepared.base, base);
    assert_eq!(prepared.fragment, fragment);

    let mut prepared = crate::joining::Prepared::new(&source, &[start]).unwrap();
    prepared.fragment = atom_text::apply(&prepared.fragment, end, "[C,N]", Mode::Auto).unwrap();
    let base = prepared.base.clone();
    let fragment = prepared.fragment.clone();
    assert_eq!(
        prepared
            .place(
                Point::default(),
                None,
                5.,
                Anchor::Atom(start),
                Connection::Connect
            )
            .unwrap_err(),
        JOIN_NOTICE
    );
    assert_eq!(prepared.base, base);
    assert_eq!(prepared.fragment, fragment);
}

#[test]
fn direct_template_share_fuse_and_connect_reject_referenced_generic_ports() {
    use crate::templates::{self, Anchor, Connection};
    let host = atom_text::apply(&chain(&["C", "C"]), 1, "[C,N]", Mode::Auto).unwrap();
    let part = chain(&["C", "C"]);
    let host_before = host.clone();
    let part_before = part.clone();
    for (mode, point, anchor) in [
        (Connection::ShareAtom, Point::default(), Anchor::Atom(1)),
        (Connection::Connect, Point::default(), Anchor::Atom(1)),
        (
            Connection::FuseBond,
            Point::new(21., 0.),
            Anchor::Bond(1, 2),
        ),
    ] {
        assert_eq!(
            templates::place_with_mode(&host, &part, point, None, 2., anchor, mode).unwrap_err(),
            JOIN_NOTICE
        );
    }
    assert_eq!(
        templates::place_anchored(
            &host,
            &part,
            Point::new(21., 0.),
            None,
            2.,
            Anchor::Bond(1, 2)
        )
        .unwrap_err(),
        JOIN_NOTICE
    );
    assert_eq!(host, host_before);
    assert_eq!(part, part_before);

    let part = atom_text::apply(&chain(&["C", "C", "C"]), 3, "[C,N]", Mode::Auto).unwrap();
    let ordinary_host = chain(&["C", "C"]);
    let before = part.clone();
    assert_eq!(
        templates::place_with_mode(
            &ordinary_host,
            &part,
            Point::default(),
            None,
            2.,
            Anchor::Atom(1),
            Connection::ShareAtom
        )
        .unwrap_err(),
        JOIN_NOTICE
    );
    assert_eq!(part, before);
}

#[test]
fn plain_joins_and_free_generic_insertion_keep_unrelated_host_definitions() {
    use crate::templates::{self, Anchor, Connection};
    let mut host = chain(&["C", "C"]);
    let unrelated = host.add_atom("C", Point::new(300., 0.));
    let host = atom_text::apply(&host, unrelated, "[C,N]", Mode::Auto).unwrap();
    let part = chain(&["C", "C"]);
    let before = host.clone();
    let (joined, _) = templates::place_with_mode(
        &host,
        &part,
        Point::default(),
        None,
        2.,
        Anchor::Atom(1),
        Connection::Connect,
    )
    .unwrap();
    assert_eq!(joined.generic, host.generic);
    assert_eq!(joined.atom(unrelated), host.atom(unrelated));
    assert_eq!(joined.bonds.len(), host.bonds.len() + part.bonds.len() + 1);
    joined.validate().unwrap();
    assert_eq!(host, before);

    let generic_part = atom_text::apply(&part, 2, "[C,N]", Mode::Auto).unwrap();
    let (inserted, _) = templates::place_anchored(
        &host,
        &generic_part,
        Point::new(600., 0.),
        None,
        2.,
        Anchor::Atom(1),
    )
    .unwrap();
    assert_eq!(inserted.generic.alternatives.len(), 2);
    assert_eq!(
        inserted.generic.alternatives[0],
        host.generic.alternatives[0]
    );
    inserted.validate().unwrap();
    assert_eq!(host, before);
}
