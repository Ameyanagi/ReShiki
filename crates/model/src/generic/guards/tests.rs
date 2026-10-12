use crate::arrow_anchors::{Anchor, Target};
use crate::document::{Arrow, Document, Point};
use crate::generic::test_support::{chain, element, repeat_document};
use crate::generic::{AlternativeSite, Crossing, LinkNode, pair};

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
            doc.bonds.retain(|b| pair(b.a, b.b) != (3, 4));
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
