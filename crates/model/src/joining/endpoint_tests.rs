use super::*;
use crate::{
    arrow_anchors::{self, Anchor as ArrowAnchor, Target},
    arrows::{ArrowStyle, Preset},
    document::{Arrow, History},
    scientific::{AtomMark, MarkKind},
    transaction,
};

fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
}

fn linked(doc: &mut Document, start: Target, end: Target) -> u64 {
    let offset = Point::new(0., -80.);
    let position = |target: &Target| {
        let p = target.center(doc).unwrap();
        p.offset(offset.x, offset.y)
    };
    let id = doc.next_id();
    let mut arrow = Arrow::new(
        id,
        position(&start),
        position(&end),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    );
    arrow.cubic = Some([arrow.start.offset(20., -25.), arrow.end.offset(-20., -25.)]);
    let anchor = |target| ArrowAnchor {
        target,
        offset,
        direction: Point::new(0., -1.),
        gap: 1.,
    };
    arrow.start_anchor = Some(anchor(start));
    arrow.end_anchor = Some(anchor(end));
    doc.arrows.push(arrow);
    arrow_anchors::reconcile(doc);
    doc.validate().unwrap();
    id
}

fn arrow(doc: &Document, id: u64) -> &Arrow {
    doc.arrows.iter().find(|a| a.id == id).unwrap()
}

fn atom_target(anchor: &Option<ArrowAnchor>, expected: u64) {
    assert!(matches!(anchor.as_ref().unwrap().target, Target::Atom { atom } if atom == expected));
}

fn one_frame_and_reopen(before: &Document, after: &Document) {
    let mut doc = after.clone();
    let mut history = History::default();
    assert!(
        transaction::apply(&mut doc, &mut history, before.clone(), false)
            .unwrap()
            .recorded
    );
    assert_eq!(doc, *after, "final reconciliation is idempotent");
    assert_eq!(history.undo_frames().len(), 1);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, *before);
    assert!(!history.can_undo());
    assert!(history.redo(&mut doc));
    assert_eq!(doc, *after);
    assert!(!history.can_redo());
    let mut saved = doc.clone();
    saved.version = crate::document::VERSION;
    assert_eq!(
        Document::from_json(&serde_json::to_vec(&saved).unwrap()).unwrap(),
        saved
    );
}

fn lone_pair(offset: Point, size: f32) -> AtomMark {
    AtomMark {
        id: Some(7),
        kind: MarkKind::LonePair,
        offset,
        angle: 0.,
        size_pt: Some(size),
    }
}

fn merge_fixture() -> (Document, Vec<u64>, [u64; 5]) {
    let mut doc = Document::default();
    let removed = doc.add_atom("N", Point::new(-40., -20.));
    let terminal = doc.add_atom("C", Point::new(-90., -20.));
    let survivor = doc.add_atom("N", Point::new(0., 60.));
    let other = doc.add_atom("N", Point::new(70., 15.));
    let terminal2 = doc.add_atom("C", Point::new(120., 15.));
    let terminal3 = doc.add_atom("C", Point::new(0., 110.));
    for (a, b) in [
        (removed, terminal),
        (other, terminal2),
        (survivor, terminal3),
        (removed, other),
    ] {
        doc.add_bond(a, b, 1, "plain");
    }
    for id in [removed, survivor, other] {
        doc.atom_mut(id)
            .unwrap()
            .marks
            .push(lone_pair(Point::new(0., -20.), 6.));
    }
    let atom = linked(
        &mut doc,
        Target::Atom { atom: removed },
        Target::Atom { atom: terminal },
    );
    let bond = linked(
        &mut doc,
        Target::Bond {
            a: removed,
            b: terminal,
            fraction: 0.25,
        },
        Target::Atom { atom: terminal2 },
    );
    let collapsed = linked(
        &mut doc,
        Target::Bond {
            a: removed,
            b: other,
            fraction: 0.4,
        },
        Target::Atom { atom: terminal3 },
    );
    let pair = linked(
        &mut doc,
        Target::LonePair {
            atom: removed,
            mark: 7,
        },
        Target::Atom { atom: terminal3 },
    );
    let untouched = linked(
        &mut doc,
        Target::Atom { atom: terminal },
        Target::Atom { atom: terminal2 },
    );
    (
        doc,
        vec![survivor, removed, other],
        [atom, bond, collapsed, pair, untouched],
    )
}

#[test]
fn merge_remaps_surviving_targets_reverses_fraction_and_detaches_collapsed_bond() {
    let (before, selected, [atom, bond, collapsed, pair, untouched]) = merge_fixture();
    let (after, ids) = merge_atoms_at(&before, &selected, Point::new(15., 20.)).unwrap();
    assert_eq!(ids, [3]);
    assert_eq!((after.atoms.len(), after.bonds.len()), (4, 3));
    for id in [2, 5, 6] {
        assert_eq!(after.atom(id), before.atom(id));
    }
    atom_target(&arrow(&after, atom).start_anchor, 3);
    atom_target(&arrow(&after, atom).end_anchor, 2);
    near(
        arrow(&after, atom).start,
        arrow(&before, atom).start.offset(55., 40.),
    );
    assert!(
        matches!(arrow(&after, bond).start_anchor.as_ref().unwrap().target,
        Target::Bond { a: 2, b: 3, fraction } if (fraction - 0.75).abs() < 0.00001)
    );
    near(
        arrow(&after, bond)
            .start_anchor
            .as_ref()
            .unwrap()
            .target
            .center(&after)
            .unwrap(),
        Point::new(-11.25, 10.),
    );
    assert!(arrow(&after, collapsed).start_anchor.is_none());
    assert_eq!(
        arrow(&after, collapsed).start,
        arrow(&before, collapsed).start
    );
    assert_eq!(
        arrow(&after, collapsed).cubic.unwrap()[0],
        arrow(&before, collapsed).cubic.unwrap()[0]
    );
    assert!(matches!(
        arrow(&after, pair).start_anchor.as_ref().unwrap().target,
        Target::LonePair { atom: 3, mark: 7 }
    ));
    assert_eq!(arrow(&after, untouched), arrow(&before, untouched));
    after.validate().unwrap();
    one_frame_and_reopen(&before, &after);
}

#[test]
fn actual_delete_after_merge_detaches_only_affected_ends_at_cached_curve_and_undo_restores() {
    let (before, selected, [atom, _, _, _, untouched]) = merge_fixture();
    let (joined, _) = merge_atoms(&before, &selected).unwrap();
    let mut deleted = joined.clone();
    deleted.delete(&[3]);
    assert!(arrow(&deleted, atom).start_anchor.is_none());
    atom_target(&arrow(&deleted, atom).end_anchor, 2);
    assert_eq!(arrow(&deleted, atom).start, arrow(&joined, atom).start);
    assert_eq!(
        arrow(&deleted, atom).cubic.unwrap()[0],
        arrow(&joined, atom).cubic.unwrap()[0]
    );
    assert_eq!(arrow(&deleted, untouched), arrow(&joined, untouched));
    deleted.validate().unwrap();
    one_frame_and_reopen(&joined, &deleted);
}

#[test]
fn prepared_connect_share_and_fuse_keep_retained_and_moved_links_under_original_ids() {
    for mode in [
        Connection::Connect,
        Connection::ShareAtom,
        Connection::FuseBond,
    ] {
        for moving_arrows in [false, true] {
            let mut doc = Document::default();
            let host = doc.add_atom("C", Point::default());
            let host_end = doc.add_atom("C", Point::new(42., 0.));
            let source = doc.add_atom("C", Point::new(180., 0.));
            let source_end = doc.add_atom("C", Point::new(222., 0.));
            doc.add_bond(host, host_end, 1, "plain");
            doc.add_bond(source, source_end, 1, "plain");
            let retained = linked(
                &mut doc,
                Target::Atom { atom: source },
                Target::Atom { atom: host_end },
            );
            let moving = linked(
                &mut doc,
                Target::Atom { atom: source },
                Target::Atom { atom: source_end },
            );
            let external = linked(
                &mut doc,
                Target::Atom { atom: source },
                Target::Atom { atom: host },
            );
            let bond = linked(
                &mut doc,
                Target::Bond {
                    a: source,
                    b: source_end,
                    fraction: 0.2,
                },
                Target::Atom { atom: source_end },
            );
            let mut selection = vec![source];
            if moving_arrows {
                selection.extend([moving, bond, external]);
            }
            let prepared = Prepared::new(&doc, &selection).unwrap();
            let (point, anchor) = if mode == Connection::FuseBond {
                (Point::new(21., 0.), Anchor::Bond(source, source_end))
            } else {
                (Point::default(), Anchor::Atom(source))
            };
            let (joined, ids) = prepared.place(point, None, 5., anchor, mode).unwrap();
            let mapping: HashMap<_, _> = prepared.fragment.all_ids().into_iter().zip(ids).collect();
            let a = mapping[&source];
            let b = mapping[&source_end];
            assert_eq!(
                joined
                    .all_ids()
                    .iter()
                    .filter(|id| !doc.all_ids().contains(id))
                    .count(),
                0
            );
            assert_eq!(joined.arrows.len(), 4);
            atom_target(&arrow(&joined, retained).start_anchor, a);
            atom_target(&arrow(&joined, retained).end_anchor, host_end);
            atom_target(&arrow(&joined, moving).start_anchor, a);
            atom_target(&arrow(&joined, moving).end_anchor, b);
            atom_target(&arrow(&joined, external).start_anchor, a);
            if moving_arrows {
                assert!(arrow(&prepared.fragment, external).end_anchor.is_none());
                assert!(
                    arrow(&joined, external).end_anchor.is_none(),
                    "a transformed arrow keeps the existing external-target detachment policy"
                );
            } else {
                atom_target(&arrow(&joined, external).end_anchor, host);
            }
            let expected_fraction = if a < b { 0.2 } else { 0.8 };
            assert!(
                matches!(arrow(&joined, bond).start_anchor.as_ref().unwrap().target,
                Target::Bond { a: x, b: y, fraction } if x == a.min(b) && y == a.max(b)
                    && (fraction - expected_fraction).abs() < 0.00001)
            );
            assert_eq!(
                (
                    joined.atom(host).unwrap().position,
                    joined.atom(host_end).unwrap().position
                ),
                (
                    doc.atom(host).unwrap().position,
                    doc.atom(host_end).unwrap().position
                )
            );
            joined.validate().unwrap();
            one_frame_and_reopen(&doc, &joined);
        }
    }
}

fn pair_fixture() -> (Document, u64, u64, u64, u64) {
    let mut doc = Document::default();
    let host = doc.add_atom("O", Point::default());
    let host_end = doc.add_atom("C", Point::new(42., 0.));
    let source = doc.add_atom("O", Point::new(180., 0.));
    let source_end = doc.add_atom("C", Point::new(222., 0.));
    doc.add_bond(host, host_end, 1, "plain");
    doc.add_bond(source, source_end, 1, "plain");
    doc.atom_mut(host)
        .unwrap()
        .marks
        .push(lone_pair(Point::new(20., 0.), 12.));
    doc.atom_mut(source)
        .unwrap()
        .marks
        .push(lone_pair(Point::new(0., -20.), 6.));
    let retained = linked(
        &mut doc,
        Target::LonePair {
            atom: source,
            mark: 7,
        },
        Target::Atom { atom: host_end },
    );
    let moving = linked(
        &mut doc,
        Target::LonePair {
            atom: source,
            mark: 7,
        },
        Target::Atom { atom: source_end },
    );
    (doc, host, source, retained, moving)
}

#[test]
fn connect_keeps_marked_atom_identity_and_marked_sharing_rejects_atomically() {
    let (doc, host, source, retained, moving) = pair_fixture();
    let prepared = Prepared::new(&doc, &[source, moving]).unwrap();
    let (joined, _) = prepared
        .place(
            Point::default(),
            None,
            5.,
            Anchor::Atom(source),
            Connection::Connect,
        )
        .unwrap();
    assert_eq!(
        joined.atom(host).unwrap().marks,
        doc.atom(host).unwrap().marks
    );
    for id in [retained, moving] {
        assert!(
            matches!(arrow(&joined, id).start_anchor.as_ref().unwrap().target,
            Target::LonePair { atom, mark: 7 } if atom == source)
        );
    }
    joined.validate().unwrap();
    one_frame_and_reopen(&doc, &joined);
    let before = prepared.original.clone();
    assert!(
        prepared
            .place(
                Point::default(),
                None,
                5.,
                Anchor::Atom(source),
                Connection::ShareAtom
            )
            .is_err()
    );
    assert_eq!(prepared.original, before);
    assert_eq!(
        doc, before,
        "sharing never aliases local mark 7 to another host"
    );
}

#[test]
fn remap_compares_full_transformed_mark_metadata_and_detaches_mismatched_local_number_at_cache() {
    let (doc, host, source, retained, moving) = pair_fixture();
    let source_end = 4;
    let mut posed = doc.clone();
    editing::transform_about(
        &mut posed,
        &[source, source_end],
        doc.atom(source).unwrap().position,
        1.5,
        60.,
    );
    let mark = posed.atom(source).unwrap().marks[0].clone();
    assert_ne!(mark.offset, doc.atom(source).unwrap().marks[0].offset);
    let mapping: HashMap<_, _> = posed
        .atoms
        .iter()
        .map(|a| (a.id, if a.id == source { host } else { a.id }))
        .collect();
    let mut merged = posed.clone();
    merged.atom_mut(host).unwrap().marks = vec![mark.clone()];
    merged.atoms.retain(|a| a.id != source);
    for b in &mut merged.bonds {
        if b.a == source {
            b.a = host;
        }
        if b.b == source {
            b.b = host;
        }
    }
    // This isolates the remapping contract. Public sharing deliberately rejects
    // positioned marks; >=3 atom merging allows identical selected metadata.
    let cached = merged.clone();
    arrow_anchors::remap_for_join(&mut merged, &posed.atoms, &mapping);
    for id in [retained, moving] {
        assert!(
            matches!(arrow(&merged, id).start_anchor.as_ref().unwrap().target,
            Target::LonePair { atom, mark: 7 } if atom == host)
        );
        assert_eq!(
            arrow(&merged, id).start,
            arrow(&cached, id).start,
            "mapping itself never resolves geometry"
        );
        assert_eq!(arrow(&merged, id).cubic, arrow(&cached, id).cubic);
    }
    arrow_anchors::reconcile(&mut merged);
    merged.validate().unwrap();
    let mut mismatched = cached.clone();
    arrow_anchors::remap_for_join(&mut mismatched, &doc.atoms, &mapping);
    for id in [retained, moving] {
        assert!(
            arrow(&mismatched, id).start_anchor.is_none(),
            "same mark number on another host is insufficient"
        );
        assert_eq!(arrow(&mismatched, id).start, arrow(&cached, id).start);
        assert_eq!(arrow(&mismatched, id).cubic, arrow(&cached, id).cubic);
    }
    arrow_anchors::reconcile(&mut mismatched);
    mismatched.validate().unwrap();
}

#[test]
fn direct_template_sharing_maps_surviving_lone_pair_and_atom_targets_and_rejects_marked_attachment()
{
    let mut host = Document::default();
    let a = host.add_atom("O", Point::default());
    let b = host.add_atom("C", Point::new(42., 0.));
    host.add_bond(a, b, 1, "plain");
    let mut part = Document::default();
    let source = part.add_atom("O", Point::new(180., 0.));
    let tail = part.add_atom("C", Point::new(222., 0.));
    part.add_bond(source, tail, 1, "plain");
    part.atom_mut(tail)
        .unwrap()
        .marks
        .push(lone_pair(Point::new(0., -20.), 6.));
    let id = linked(
        &mut part,
        Target::LonePair {
            atom: tail,
            mark: 7,
        },
        Target::Atom { atom: source },
    );
    let ordinary = linked(
        &mut part,
        Target::Atom { atom: source },
        Target::Atom { atom: tail },
    );
    let (joined, ids) = templates::place_anchored(
        &host,
        &part,
        Point::default(),
        None,
        5.,
        Anchor::Atom(source),
    )
    .unwrap();
    let mapping: HashMap<_, _> = part.all_ids().into_iter().zip(ids).collect();
    let pair = arrow(&joined, mapping[&id]);
    assert!(matches!(pair.start_anchor.as_ref().unwrap().target,
        Target::LonePair { atom, mark: 7 } if atom == mapping[&tail]));
    atom_target(&pair.end_anchor, a);
    atom_target(&arrow(&joined, mapping[&ordinary]).start_anchor, a);
    assert_eq!(joined.atom(a).unwrap().marks, host.atom(a).unwrap().marks);
    joined.validate().unwrap();
    one_frame_and_reopen(&host, &joined);
    part.atom_mut(source)
        .unwrap()
        .marks
        .push(lone_pair(Point::new(0., -20.), 6.));
    host.atom_mut(a)
        .unwrap()
        .marks
        .push(lone_pair(Point::new(20., 0.), 12.));
    let originals = (host.clone(), part.clone());
    assert!(
        templates::place_anchored(
            &host,
            &part,
            Point::default(),
            None,
            5.,
            Anchor::Atom(source)
        )
        .is_err()
    );
    assert_eq!((host, part), originals);
}

#[test]
fn first_coordination_bridge_stretches_but_second_contact_closes_a_cycle_with_links_and_charge_intact()
 {
    use crate::editing::reference::{self, Stretch};
    let mut original = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/coordination/co-en3-before.rsk"
    ))
    .unwrap();
    let id = linked(
        &mut original,
        Target::Atom { atom: 2 },
        Target::Atom { atom: 1 },
    );
    let first = templates::coordinate_atoms(&original, 2, 1).unwrap();
    assert_eq!(first.atoms, original.atoms);
    let stretch = Stretch::new(&first, 1, 2).unwrap();
    let stretched = stretch.apply(&first, stretch.length * 1.1).unwrap();
    assert_eq!(stretched.bonds, first.bonds);
    assert_eq!(stretched.atom(1), first.atom(1));
    assert_eq!(stretched.atom(1).unwrap().charge, 3);
    let displacement = Point::new(
        stretched.atom(2).unwrap().position.x - first.atom(2).unwrap().position.x,
        stretched.atom(2).unwrap().position.y - first.atom(2).unwrap().position.y,
    );
    near(
        arrow(&stretched, id).start,
        arrow(&first, id)
            .start
            .offset(displacement.x, displacement.y),
    );
    atom_target(&arrow(&stretched, id).start_anchor, 2);
    atom_target(&arrow(&stretched, id).end_anchor, 1);
    let chelate = templates::coordinate_atoms(&stretched, 5, 1).unwrap();
    assert_eq!(chelate.atoms, stretched.atoms);
    assert!(
        Stretch::new(&chelate, 1, 2)
            .unwrap_err()
            .contains("ring bond")
    );
    for (before, after) in [
        (&original, &first),
        (&first, &stretched),
        (&stretched, &chelate),
    ] {
        one_frame_and_reopen(before, after);
    }
    let mut ids = reference::scope(&chelate, &[1, 2], true).unwrap();
    ids.push(id);
    let (rotated, _) = reference::rotate(
        &chelate,
        &ids,
        chelate.atom(1).unwrap().position,
        17.25,
        false,
    )
    .unwrap();
    assert_eq!(rotated.bonds, chelate.bonds);
    for a in &chelate.atoms {
        let b = rotated.atom(a.id).unwrap();
        assert_eq!(
            (b.charge, b.depth, b.label_h),
            (a.charge, a.depth, a.label_h)
        );
    }
    let (copied, new_ids) = reference::rotate(
        &rotated,
        &ids,
        rotated.atom(1).unwrap().position,
        31.5,
        true,
    )
    .unwrap();
    let map: HashMap<_, _> = ids.into_iter().zip(new_ids).collect();
    assert_eq!(copied.atom(map[&1]).unwrap().charge, 3);
    atom_target(&arrow(&copied, map[&id]).start_anchor, map[&2]);
    atom_target(&arrow(&copied, map[&id]).end_anchor, map[&1]);
    let mut once = copied.clone();
    arrow_anchors::reconcile(&mut once);
    arrow_anchors::reconcile(&mut once);
    assert_eq!(once, copied);
    one_frame_and_reopen(&chelate, &rotated);
    one_frame_and_reopen(&rotated, &copied);
}
