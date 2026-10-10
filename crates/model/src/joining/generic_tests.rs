//! Cross-feature outcomes: unresolved chemistry cannot be pruned by a join.
use super::*;
use crate::{
    arrow_anchors::{self, Pick, Target},
    arrows::{ArrowStyle, Preset},
    generic::{
        Alternative, AlternativeSite, Crossing, JOIN_NOTICE, LinkNode, RepeatCount, RepeatPattern,
        RepeatUnit,
    },
    graphics::{BracketSides, Graphic, GraphicKind},
};

#[derive(Clone, Copy, Debug)]
enum Definition {
    Element,
    Group,
    Unresolved,
    Link,
    Finite,
    Indefinite,
    Variable,
}
const DEFINITIONS: [Definition; 7] = [
    Definition::Element,
    Definition::Group,
    Definition::Unresolved,
    Definition::Link,
    Definition::Finite,
    Definition::Indefinite,
    Definition::Variable,
];

fn chain(doc: &mut Document, x: f32, y: f32) -> [u64; 3] {
    let a = doc.add_atom("C", Point::new(x, y));
    let b = doc.add_atom("C", Point::new(x + 42., y));
    let c = doc.add_atom("C", Point::new(x + 84., y));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    [a, b, c]
}

fn define(doc: &mut Document, site: u64, kind: Definition) {
    match kind {
        Definition::Element | Definition::Group | Definition::Unresolved => {
            doc.atom_mut(site).unwrap().element = "*".into();
            let choice = match kind {
                Definition::Element => Alternative::Element {
                    symbol: "N".into(),
                    isotope: 15,
                },
                Definition::Group => Alternative::Group { label: "Ph".into() },
                _ => Alternative::Unresolved { label: "R1".into() },
            };
            doc.generic.alternatives.push(AlternativeSite {
                atom: site,
                choices: vec![choice],
                excluded: false,
            });
        }
        Definition::Link => doc.generic.links.push(LinkNode {
            atom: site,
            min: 1,
            max: 3,
        }),
        Definition::Finite | Definition::Indefinite => {
            let bracket = doc.next_id();
            let point = doc.atom(site).unwrap().position;
            doc.graphics.push(Graphic::dragged(
                bracket,
                GraphicKind::Brackets,
                point.offset(-15., -30.),
                point.offset(15., 30.),
                Default::default(),
                BracketSides::Both,
                false,
            ));
            let crossings: Vec<_> = doc
                .bonds
                .iter()
                .filter_map(|bond| {
                    if bond.a == site {
                        Some(Crossing {
                            inside: site,
                            outside: bond.b,
                        })
                    } else if bond.b == site {
                        Some(Crossing {
                            inside: site,
                            outside: bond.a,
                        })
                    } else {
                        None
                    }
                })
                .collect();
            let extra_bracket = doc.next_id();
            doc.graphics.push(Graphic::dragged(
                extra_bracket,
                GraphicKind::Brackets,
                point.offset(-18., -33.),
                point.offset(18., 33.),
                Default::default(),
                BracketSides::Both,
                false,
            ));
            let crossing_brackets = vec![extra_bracket; crossings.len()];
            let count = if matches!(kind, Definition::Finite) {
                RepeatCount::Finite { min: 1, max: 3 }
            } else {
                RepeatCount::Indefinite { label: "n".into() }
            };
            doc.generic.repeats.push(RepeatUnit {
                bracket: Some(bracket),
                bracket_objects: vec![extra_bracket],
                members: vec![site],
                crossings,
                crossing_brackets,
                head_count: None,
                usage: "SRU".into(),
                pattern: RepeatPattern::HeadToTail,
                count,
                label: "n".into(),
                flip: "Unspecified".into(),
            });
        }
        Definition::Variable => {
            let neighbor = doc
                .bonds
                .iter()
                .rev()
                .find_map(|bond| {
                    if bond.a == site {
                        Some(bond.b)
                    } else if bond.b == site {
                        Some(bond.a)
                    } else {
                        None
                    }
                })
                .unwrap();
            let _ =
                crate::attachments::add(doc, &[site, neighbor], crate::attachments::Kind::Variable)
                    .unwrap();
        }
    }
    doc.validate().unwrap();
}

fn link(doc: &mut Document, source: u64, target: u64) -> u64 {
    arrow_anchors::create(
        doc,
        &Pick::Atom(source),
        &Pick::Atom(target),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap()
}

fn unchanged<T: std::fmt::Debug>(doc: &Document, before: &Document, result: Result<T, String>) {
    assert_eq!(result.unwrap_err(), JOIN_NOTICE);
    assert_eq!(doc, before);
}

#[test]
fn atom_merge_rejects_selected_or_remote_generic_definitions_atomically() {
    for kind in DEFINITIONS {
        for selected_site in [false, true] {
            let mut doc = Document::default();
            let [first, remote, _] = chain(&mut doc, 0., 0.);
            let second = doc.add_atom("C", Point::new(0., 100.));
            let third = doc.add_atom("C", Point::new(0., 200.));
            let site = if selected_site { first } else { remote };
            if selected_site && matches!(kind, Definition::Link) {
                let extra = doc.add_atom("C", Point::new(-42., 0.));
                doc.add_bond(first, extra, 1, "plain");
            }
            define(&mut doc, site, kind);
            let _ = link(&mut doc, first, second);
            doc.validate().unwrap();
            let before = doc.clone();
            let selected = [first, second, third];
            unchanged(&doc, &before, merge_atoms(&doc, &selected));
            unchanged(
                &doc,
                &before,
                merge_atoms_at(&doc, &selected, Point::new(40., 70.)),
            );
        }
    }
}

fn chelate() -> (Document, u64, u64, u64, u64) {
    let mut doc = Document::default();
    let metal = doc.add_atom("Co", Point::default());
    doc.atom_mut(metal).unwrap().charge = 3;
    let donor = doc.add_atom("N", Point::new(150., -42.));
    let middle = doc.add_atom("C", Point::new(192., -42.));
    let tail = doc.add_atom("C", Point::new(192., 42.));
    let second = doc.add_atom("N", Point::new(150., 42.));
    for (a, b) in [(donor, middle), (middle, tail), (tail, second)] {
        doc.add_bond(a, b, 1, "plain");
    }
    for id in [donor, second] {
        doc.atom_mut(id).unwrap().label_h = 2;
    }
    (doc, metal, donor, second, middle)
}

#[test]
fn coordinate_atoms_rejects_generic_donor_and_metal_components_before_chemistry() {
    for kind in DEFINITIONS {
        for host_side in [false, true] {
            for existing_contact in [false, true] {
                let (mut doc, metal, donor, second, remote) = chelate();
                if existing_contact {
                    doc = templates::coordinate_atoms(&doc, donor, metal).unwrap();
                }
                let site = if host_side {
                    let [a, b, _] = chain(&mut doc, -180., 0.);
                    doc.add_bond(metal, a, 1, "plain");
                    b
                } else {
                    remote
                };
                define(&mut doc, site, kind);
                let _ = link(&mut doc, donor, metal);
                let before = doc.clone();
                unchanged(
                    &doc,
                    &before,
                    templates::coordinate_atoms(&doc, second, metal),
                );
                // An already existing contact must not bypass the unresolved-site guard.
                unchanged(
                    &doc,
                    &before,
                    templates::coordinate_atoms(&doc, donor, metal),
                );
            }
        }
    }
}

#[test]
fn prepared_coordinate_rechecks_original_fragment_and_host_generic_dependencies() {
    let (doc, metal, donor, _, remote) = chelate();
    for mode in [Connection::Connect, Connection::Coordinate] {
        let mut generic = doc.clone();
        define(&mut generic, remote, Definition::Link);
        let before = generic.clone();
        unchanged(
            &generic,
            &before,
            Prepared::with_mode(&generic, &[donor], mode),
        );
    }
    // A selected arrow can depend on a typed component that selection() would
    // otherwise detach. Unselected links do not join the two chemical components.
    for target_kind in ["atom", "bond", "pair"] {
        let mut doc = Document::default();
        let [host, generic_site, _] = chain(&mut doc, 0., 0.);
        let [source, _, _] = chain(&mut doc, 210., 0.);
        define(&mut doc, generic_site, Definition::Link);
        let pick = match target_kind {
            "bond" => Pick::Bond {
                a: host,
                b: generic_site,
                fraction: 0.5,
            },
            "pair" => {
                let mark = crate::scientific::AtomMark {
                    id: Some(1),
                    kind: crate::scientific::MarkKind::LonePair,
                    offset: Point::new(0., -20.),
                    angle: 0.,
                    size_pt: Some(6.),
                };
                let atom = doc.atom_mut(host).unwrap();
                atom.mark_serial = 1;
                atom.marks.push(mark.clone());
                Pick::LonePair { atom: host, mark }
            }
            _ => Pick::Atom(host),
        };
        let arrow = arrow_anchors::create(
            &mut doc,
            &Pick::Atom(source),
            &pick,
            Preset::Curved,
            ArrowStyle::preset(Preset::Curved),
        )
        .unwrap();
        doc.validate().unwrap();
        let before = doc.clone();
        assert!(
            Prepared::new(&doc, &[source]).is_ok(),
            "an unselected ordinary arrow does not merge chemical components"
        );
        unchanged(&doc, &before, Prepared::new(&doc, &[source, arrow]));
    }
    for kind in [Definition::Finite, Definition::Indefinite] {
        let mut doc = Document::default();
        let [_, site, _] = chain(&mut doc, 0., 0.);
        let [source, _, _] = chain(&mut doc, 210., 0.);
        define(&mut doc, site, kind);
        let bracket = doc
            .generic
            .repeats
            .first()
            .unwrap()
            .crossing_brackets
            .first()
            .copied()
            .unwrap();
        let before = doc.clone();
        unchanged(&doc, &before, Prepared::new(&doc, &[source, bracket]));
    }
    for changed in ["original", "fragment", "target"] {
        let mut prepared = Prepared::with_mode(&doc, &[donor], Connection::Coordinate).unwrap();
        match changed {
            "original" => define(&mut prepared.original, remote, Definition::Group),
            "fragment" => define(&mut prepared.fragment, remote, Definition::Finite),
            _ => {
                let [a, b, _] = chain(&mut prepared.original, -180., 0.);
                prepared.original.add_bond(metal, a, 1, "plain");
                define(&mut prepared.original, b, Definition::Unresolved);
            }
        }
        let before = (
            prepared.original.clone(),
            prepared.fragment.clone(),
            prepared.base.clone(),
            prepared.moving.clone(),
        );
        let result = prepared.place(
            Point::default(),
            None,
            5.,
            Anchor::Atom(donor),
            Connection::Coordinate,
        );
        assert_eq!(result.unwrap_err(), JOIN_NOTICE, "{changed}");
        assert_eq!(
            (
                prepared.original,
                prepared.fragment,
                prepared.base,
                prepared.moving
            ),
            before
        );
    }
}

#[test]
fn atom_and_bond_attachment_reject_generic_source_and_target_components_before_pose() {
    for kind in DEFINITIONS {
        for source_side in [false, true] {
            for mode in [
                Connection::Connect,
                Connection::ShareAtom,
                Connection::FuseBond,
            ] {
                let mut doc = Document::default();
                let [host, remote, host_end] = chain(&mut doc, 0., 0.);
                let [source, source_remote, source_end] = chain(&mut doc, 210., 0.);
                define(
                    &mut doc,
                    if source_side { source_remote } else { remote },
                    kind,
                );
                let before = doc.clone();
                let source_ids = [source, source_remote, source_end];
                let part = editing::selection(&doc, &source_ids);
                let base = if source_side {
                    editing::selection(&doc, &[host, remote, host_end])
                } else {
                    doc.clone()
                };
                let (point, anchor) = if mode == Connection::FuseBond {
                    (Point::new(21., 0.), Anchor::Bond(source, source_remote))
                } else {
                    (Point::default(), Anchor::Atom(source))
                };
                assert_eq!(
                    templates::place_with_mode(&base, &part, point, None, 5., anchor, mode)
                        .unwrap_err(),
                    JOIN_NOTICE,
                    "{kind:?} {mode:?}"
                );
                assert_eq!(
                    templates::place_anchored(&base, &part, point, None, 5., anchor).unwrap_err(),
                    JOIN_NOTICE
                );
                if source_side {
                    unchanged(&doc, &before, Prepared::new(&doc, &[source]));
                    let (free, ids) = templates::place_anchored(
                        &base,
                        &part,
                        Point::new(600., 300.),
                        None,
                        5.,
                        Anchor::Atom(source),
                    )
                    .unwrap();
                    free.validate().unwrap();
                    assert!(!ids.is_empty());
                    assert!(
                        !free.generic.is_empty()
                            || free.atoms.iter().any(|a| a.attachment.is_some())
                    );
                } else {
                    let prepared = Prepared::new(&doc, &[source]).unwrap();
                    assert_eq!(
                        prepared.place(point, None, 5., anchor, mode).unwrap_err(),
                        JOIN_NOTICE
                    );
                }
                assert_eq!(doc, before);
            }
        }
    }
}

#[test]
fn ordinary_joins_preserve_disconnected_generic_components_and_arrow_references() {
    for mode in [
        Connection::Connect,
        Connection::ShareAtom,
        Connection::FuseBond,
    ] {
        let mut doc = Document::default();
        let [host, _, _] = chain(&mut doc, 0., 0.);
        let [source, source_end, tail] = chain(&mut doc, 210., 0.);
        let [ga, gb, gc] = chain(&mut doc, 600., 300.);
        define(&mut doc, gb, Definition::Finite);
        let atom = doc.atom_mut(ga).unwrap();
        atom.depth = 17.;
        atom.mark_serial = 2;
        atom.marks.push(crate::scientific::AtomMark {
            id: Some(2),
            kind: crate::scientific::MarkKind::LonePair,
            offset: Point::new(0., -18.),
            angle: 0.,
            size_pt: Some(6.),
        });
        let unrelated: Vec<_> = [ga, gb, gc]
            .into_iter()
            .map(|id| doc.atom(id).unwrap().clone())
            .collect();
        let arrow = link(&mut doc, source, tail);
        let prepared = Prepared::new(&doc, &[source]).unwrap();
        let (point, anchor) = if mode == Connection::FuseBond {
            (Point::new(21., 0.), Anchor::Bond(source, source_end))
        } else {
            (Point::default(), Anchor::Atom(source))
        };
        let (after, ids) = prepared.place(point, None, 5., anchor, mode).unwrap();
        assert_eq!(after.generic, doc.generic);
        assert_eq!(after.graphics, doc.graphics);
        assert_eq!(
            [ga, gb, gc]
                .into_iter()
                .map(|id| after.atom(id).unwrap().clone())
                .collect::<Vec<_>>(),
            unrelated
        );
        let mapping: HashMap<_, _> = prepared.fragment.all_ids().into_iter().zip(ids).collect();
        let expected = *mapping.get(&source).unwrap();
        let after_arrow = after.arrows.iter().find(|a| a.id == arrow).unwrap();
        assert!(
            matches!(after_arrow.start_anchor.as_ref().unwrap().target, Target::Atom { atom } if atom == expected)
        );
        assert!(
            matches!(after_arrow.end_anchor.as_ref().unwrap().target, Target::Atom { atom } if atom == tail)
        );
        after.validate().unwrap();
        assert_eq!(after.atom(host).unwrap().position, Point::default());
    }
    let (mut doc, metal, donor, _, _) = chelate();
    let [_, gb, _] = chain(&mut doc, 600., 300.);
    define(&mut doc, gb, Definition::Link);
    let after = templates::coordinate_atoms(&doc, donor, metal).unwrap();
    assert_eq!(after.atoms, doc.atoms);
    assert_eq!(after.generic, doc.generic);
    assert_eq!(after.bonds.len(), doc.bonds.len() + 1);
    let mut doc = Document::default();
    let selected: Vec<_> = (0..3)
        .map(|n| doc.add_atom("C", Point::new(0., n as f32 * 60.)))
        .collect();
    let [a, b, c] = chain(&mut doc, 600., 300.);
    define(&mut doc, b, Definition::Indefinite);
    let before = doc.clone();
    let (after, ids) = merge_atoms(&doc, &selected).unwrap();
    assert_eq!(ids, vec![*selected.first().unwrap()]);
    assert_eq!(after.generic, before.generic);
    assert_eq!(after.graphics, before.graphics);
    for id in [a, b, c] {
        assert_eq!(after.atom(id), before.atom(id));
    }
    after.validate().unwrap();
}
