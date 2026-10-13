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

fn flagged_double_bond(doc: &mut Document, kind: &str) {
    let bond = doc.bonds.first_mut().unwrap();
    bond.order = 2;
    match kind {
        "captured" => bond.stereo = Some("any".into()),
        "authority" => bond.stereo_authoritative = true,
        "controls" => bond.stereo_atoms = vec![3, 4],
        "marker" => bond.display = "wavy".into(),
        _ => unreachable!(),
    }
    assert!(bond.specifies_stereo());
}

fn cyclic_double_bond(size: usize, kind: &str) -> Document {
    let mut doc = chain(&vec!["C"; size]);
    doc.add_bond(size as u64, 1, 1, "plain");
    flagged_double_bond(&mut doc, kind);
    doc
}

fn depiction_accepts(doc: &Document, atom: u64) -> bool {
    crate::generic::depiction::Plan::new(
        doc,
        &std::collections::HashSet::from([atom]),
        &mut crate::generic::Work::new(usize::MAX),
    )
    .is_ok()
}

#[test]
fn terminal_double_bond_import_flags_do_not_block_direct_or_neighbor_guards() {
    for kind in ["captured", "authority", "controls", "marker"] {
        let mut doc = chain(&["O", "C", "C", "C"]);
        flagged_double_bond(&mut doc, kind);
        for affected in [1, 2, 3] {
            assert!(
                super::reject_affected_stereo(&doc, &[affected]).is_ok(),
                "terminal {kind}, affected atom {affected}"
            );
            assert!(depiction_accepts(&doc, affected), "terminal {kind}");
        }
    }
}

#[test]
fn explicit_hydrogen_of_any_isotope_keeps_a_double_bond_end_guarded() {
    // Only implicit hydrogens leave an end without E/Z. An explicit isotope
    // label or a lone N-H defines it ([2H]/C=C/C, [H]/N=C/C); an explicit
    // protium on =CH2 is refused conservatively.
    for (end, isotope) in [("C", 0), ("C", 1), ("C", 2), ("C", 3), ("C", 99), ("N", 0)] {
        let mut doc = chain(&[end, "C", "C"]);
        flagged_double_bond(&mut doc, "authority");
        assert!(super::reject_affected_stereo(&doc, &[1]).is_ok(), "{end}");
        assert!(depiction_accepts(&doc, 3), "{end}");
        let hydrogen = doc.add_atom("H", Point::new(0., 42.));
        doc.atom_mut(hydrogen).unwrap().isotope = isotope;
        doc.add_bond(1, hydrogen, 1, "plain");
        // Adding a bond clears the neighboring stereo flags.
        flagged_double_bond(&mut doc, "authority");
        for affected in [1, 2, 3, hydrogen] {
            assert!(
                super::reject_affected_stereo(&doc, &[affected]).is_err(),
                "{end}, isotope {isotope}, affected atom {affected}"
            );
        }
        assert!(!depiction_accepts(&doc, 3), "{end}, isotope {isotope}");
    }
}

#[test]
fn dative_and_hydrogen_bond_cycles_do_not_exempt_double_bond_stereo() {
    // N1/C=C/N->[Zn]<-1 and an N-H...N hydrogen bond close no ring in
    // chemistry ring perception, so the C=C keeps its E/Z.
    use crate::bonds::DATIVE;
    for (closure, order) in [("Zn", DATIVE), ("H", 0)] {
        let mut doc = chain(&["N", "C", "C", "N"]);
        doc.bonds.swap(0, 1);
        let closing = doc.add_atom(closure, Point::new(63., 42.));
        if order == DATIVE {
            doc.add_bond(4, closing, DATIVE, "plain");
            doc.add_bond(1, closing, DATIVE, "plain");
        } else {
            doc.add_bond(4, closing, 1, "plain");
            doc.add_bond(closing, 1, 0, "plain");
        }
        flagged_double_bond(&mut doc, "authority");
        for affected in [2, 3] {
            assert!(
                super::reject_affected_stereo(&doc, &[affected]).is_err(),
                "{closure}, affected atom {affected}"
            );
        }
        assert!(!depiction_accepts(&doc, 2), "{closure}");
        // The same cycle through ordinary bonds is a five-member ring.
        doc.bonds
            .iter_mut()
            .filter(|bond| bond.order == order)
            .for_each(|bond| bond.order = 1);
        assert!(
            super::reject_affected_stereo(&doc, &[2]).is_ok(),
            "{closure}"
        );
        assert!(depiction_accepts(&doc, 2), "{closure}");
    }
}

#[test]
fn small_ring_double_bond_import_flags_do_not_block_direct_or_neighbor_guards() {
    for size in 3..=7 {
        for kind in ["captured", "authority", "controls", "marker"] {
            let doc = cyclic_double_bond(size, kind);
            for affected in [1, 2, 3] {
                assert!(
                    super::reject_affected_stereo(&doc, &[affected]).is_ok(),
                    "ring size {size}, {kind}, affected atom {affected}"
                );
                assert!(
                    depiction_accepts(&doc, affected),
                    "ring size {size}, {kind}"
                );
            }
        }
    }
}

#[test]
fn double_bond_uses_the_shortest_alternative_ring_path() {
    let mut doc = cyclic_double_bond(9, "authority");
    assert!(super::reject_affected_stereo(&doc, &[1]).is_err());
    doc.add_bond(3, 8, 1, "plain");
    flagged_double_bond(&mut doc, "authority");
    assert!(super::reject_affected_stereo(&doc, &[1]).is_ok());
    assert!(super::reject_affected_stereo(&doc, &[3]).is_ok());
    assert!(depiction_accepts(&doc, 5));
}

#[test]
fn acyclic_and_eight_member_or_larger_double_bond_stereo_remains_guarded() {
    for kind in ["captured", "authority", "controls", "marker"] {
        let mut acyclic = chain(&["C", "C", "C", "C"]);
        acyclic.bonds.swap(0, 1);
        flagged_double_bond(&mut acyclic, kind);
        let mut sources = vec![(acyclic, 1)];
        sources.extend([8, 9].map(|size| (cyclic_double_bond(size, kind), 3)));
        for (doc, neighbor) in sources {
            assert!(super::reject_affected_stereo(&doc, &[2]).is_err(), "{kind}");
            assert!(!depiction_accepts(&doc, 4), "{kind}");
            if matches!(kind, "authority" | "marker") {
                assert!(
                    super::reject_affected_stereo(&doc, &[neighbor]).is_err(),
                    "{kind}"
                );
            }
        }
    }
}

#[test]
fn small_ring_or_terminal_single_bond_stereo_remains_guarded() {
    for kind in ["captured", "authority", "controls", "marker"] {
        for mut doc in [chain(&["C", "C", "C"]), cyclic_double_bond(6, kind)] {
            flagged_double_bond(&mut doc, kind);
            doc.bonds.first_mut().unwrap().order = 1;
            assert!(super::reject_affected_stereo(&doc, &[1]).is_err(), "{kind}");
            assert!(!depiction_accepts(&doc, 3), "{kind}");
        }
    }
}

#[test]
fn prepared_terminal_and_small_ring_unknown_stereo_metadata_allows_generated_layout() {
    for mut doc in [
        chain(&["O", "C", "C", "C"]),
        cyclic_double_bond(6, "captured"),
    ] {
        flagged_double_bond(&mut doc, "captured");
        doc.bonds.first_mut().unwrap().stereo_authoritative = true;
        let before = doc.clone();
        let molecule = crate::chemistry::document::prepare(&doc).unwrap();
        let metadata = molecule.state.metadata.bonds.first().unwrap();
        assert!(metadata.stereo != 0 || metadata.unknown_stereo);
        let mut work = crate::generic::Work::new(crate::generic::Limits::default().max_work);
        crate::generic::depiction::Plan::new(
            &doc,
            &std::collections::HashSet::from([3]),
            &mut work,
        )
        .unwrap()
        .finish(&mut doc, &molecule, &mut work)
        .unwrap();
        crate::generic::drawing_test_support::same_except_positions(doc, &before);
    }
}

#[test]
fn prepared_real_alkene_stereo_still_rejects_generated_layout_atomically() {
    let mut doc = chain(&["C", "C", "C", "C"]);
    doc.bonds.get_mut(1).unwrap().order = 2;
    doc.atom_mut(1).unwrap().position.y = 42.;
    doc.atom_mut(4).unwrap().position.y = -42.;
    assert!(doc.bonds.iter().all(|bond| !bond.specifies_stereo()));
    let molecule = crate::chemistry::document::prepare(&doc).unwrap();
    assert!(molecule.state.metadata.bonds.iter().any(|m| m.stereo > 1));
    let before = doc.clone();
    let mut work = crate::generic::Work::new(crate::generic::Limits::default().max_work);
    let error = crate::generic::depiction::Plan::new(
        &doc,
        &std::collections::HashSet::from([1]),
        &mut work,
    )
    .unwrap()
    .finish(&mut doc, &molecule, &mut work)
    .unwrap_err();
    assert!(error.contains("component stereochemistry"));
    assert_eq!(doc, before);
}
