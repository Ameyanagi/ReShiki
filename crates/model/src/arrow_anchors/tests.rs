use super::*;
use crate::{
    document::History,
    editing,
    scientific::{self, SymbolKind},
    transaction,
};
#[test]
fn target_atoms_lists_each_referenced_atom() {
    for (target, expected) in [
        (Target::Atom { atom: 7 }, vec![7]),
        (
            Target::Bond {
                a: 3,
                b: 9,
                fraction: 0.25,
            },
            vec![3, 9],
        ),
        (Target::LonePair { atom: 7, mark: 11 }, vec![7]),
    ] {
        assert_eq!(target.atoms().collect::<Vec<_>>(), expected);
    }
}
#[test]
fn arrow_is_attached_with_either_anchor() {
    let (mut doc, _, _, _) = fixture();
    let mut arrow = doc.arrows.pop().unwrap();
    let start = arrow.start_anchor.take();
    let end = arrow.end_anchor.take();
    assert!(!arrow.is_attached());
    arrow.start_anchor = start.clone();
    assert!(arrow.is_attached());
    arrow.start_anchor = None;
    arrow.end_anchor = end;
    assert!(arrow.is_attached());
    arrow.start_anchor = start;
    assert!(arrow.is_attached());
}
fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
}
fn fixture() -> (Document, u64, u64, u64) {
    let mut d = Document::default();
    let a = d.add_atom("O", Point::new(0., 0.));
    let b = d.add_atom("C", Point::new(100., 0.));
    d.add_bond(a, b, 1, "plain");
    scientific::attach(
        d.atom_mut(a).unwrap(),
        SymbolKind::LonePair,
        Point::new(0., -25.),
    )
    .unwrap();
    let source = pick(&d, Point::new(0., -25.), 2.).unwrap();
    let id = create(
        &mut d,
        &source,
        &Pick::Atom(b),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    (d, a, b, id)
}
#[test]
fn mechanism_attachment_92_target_moves_only_one_endpoint_and_adjacent_control() {
    let (mut d, a, b, _) = fixture();
    let before = d.arrows[0].clone();
    d.translate(&[a], 12., -8.);
    let arrow = &d.arrows[0];
    near(arrow.start, before.start.offset(12., -8.));
    near(
        arrow.cubic.unwrap()[0],
        before.cubic.unwrap()[0].offset(12., -8.),
    );
    assert_eq!(
        (arrow.end, arrow.cubic.unwrap()[1]),
        (before.end, before.cubic.unwrap()[1])
    );
    let middle = arrow.clone();
    d.translate(&[b], -7., 18.);
    let arrow = &d.arrows[0];
    near(arrow.end, middle.end.offset(-7., 18.));
    near(
        arrow.cubic.unwrap()[1],
        middle.cubic.unwrap()[1].offset(-7., 18.),
    );
    assert_eq!(
        (arrow.start, arrow.cubic.unwrap()[0]),
        (middle.start, middle.cubic.unwrap()[0])
    );
    let once = d.clone();
    reconcile(&mut d);
    reconcile(&mut d);
    assert_eq!(d, once);
}
#[test]
fn mechanism_attachment_92_derived_target_resolves_after_final_member_transform() {
    let mut d = Document::default();
    let a = d.add_atom("C", Point::new(0., 0.));
    let b = d.add_atom("C", Point::new(40., 0.));
    let destination = d.add_atom("O", Point::new(100., -50.));
    let centroid = crate::projection::add_centroid(&mut d, &[a, b]).unwrap();
    create(
        &mut d,
        &Pick::Atom(centroid),
        &Pick::Atom(destination),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    let before = d.arrows[0].clone();
    editing::scale_axes_about(&mut d, &[a, b], Point::default(), 2., 1.);
    assert_eq!(d.atom(centroid).unwrap().position, Point::new(40., 0.));
    near(d.arrows[0].start, before.start.offset(20., 0.));
    near(
        d.arrows[0].cubic.unwrap()[0],
        before.cubic.unwrap()[0].offset(20., 0.),
    );
    assert_eq!(
        (d.arrows[0].end, d.arrows[0].cubic.unwrap()[1]),
        (before.end, before.cubic.unwrap()[1])
    );
    let once = d.clone();
    reconcile(&mut d);
    assert_eq!(d, once);
}
#[test]
fn mechanism_attachment_92_all_selected_transforms_once_and_clearance_preserves_tangent() {
    let (d, a, b, id) = fixture();
    let ids = [a, b, id];
    for (scale, degrees) in [(1.0_f32, 0.0_f32), (1., 90.), (2., 32.), (0.15, 0.)] {
        let mut next = d.clone();
        let before = d.arrows[0].clone();
        let (s, c) = degrees.to_radians().sin_cos();
        let vector =
            |p: Point| Point::new((p.x * c - p.y * s) * scale, (p.x * s + p.y * c) * scale);
        editing::transform_about(&mut next, &ids, Point::default(), scale, degrees);
        for (i, (old, end)) in [
            (before.start, next.arrows[0].start),
            (before.end, next.arrows[0].end),
        ]
        .into_iter()
        .enumerate()
        {
            let affine = vector(old);
            let correction = delta(end, affine);
            near(
                next.arrows[0].cubic.unwrap()[i],
                vector(before.cubic.unwrap()[i]).offset(correction.x, correction.y),
            );
            if scale >= 1. && degrees == 0. {
                near(end, affine);
            }
        }
        let once = next.clone();
        reconcile(&mut next);
        assert_eq!(next, once);
    }
    let mut next = d.clone();
    next.translate(&ids, 13., -19.);
    near(next.arrows[0].start, d.arrows[0].start.offset(13., -19.));
    near(next.arrows[0].end, d.arrows[0].end.offset(13., -19.));
    let mut next = d.clone();
    editing::transform(&mut next, &ids, editing::Transform::FlipHorizontal);
    let once = next.clone();
    reconcile(&mut next);
    assert_eq!(next, once);
    let mut next = d.clone();
    editing::scale_axes_about(&mut next, &ids, Point::default(), 1.8, 0.6);
    let once = next.clone();
    reconcile(&mut next);
    assert_eq!(next, once);
}
#[test]
fn mechanism_attachment_92_arrow_only_detaches_and_endpoint_edit_is_independent() {
    let (mut d, _, _, id) = fixture();
    let before = d.arrows[0].clone();
    d.translate(&[id], 40., 20.);
    assert!(d.arrows[0].start_anchor.is_none() && d.arrows[0].end_anchor.is_none());
    near(d.arrows[0].start, before.start.offset(40., 20.));
    let (mut d, _, _, _) = fixture();
    let end = d.arrows[0].end_anchor.clone();
    let controls = d.arrows[0].cubic.unwrap();
    d.arrows[0].edit_handle(3, controls[0].offset(4., -13.));
    assert!(d.arrows[0].start_anchor.is_some());
    assert_eq!(d.arrows[0].end_anchor, end);
    d.arrows[0].edit_handle(0, Point::new(-10., -80.));
    assert!(d.arrows[0].start_anchor.is_none());
    assert_eq!(d.arrows[0].end_anchor, end);
    let start = d.arrows[0].start;
    d.arrows[0].reverse();
    assert_eq!(d.arrows[0].end, start);
    assert_eq!(d.arrows[0].start_anchor, end);
    assert!(d.arrows[0].end_anchor.is_none());
}
#[test]
fn mechanism_attachment_92_fragment_remap_and_external_links_detach() {
    let (d, a, b, id) = fixture();
    let mut pasted = Document::default();
    pasted.add_atom("N", Point::new(-100., 0.));
    let all = editing::selection(&d, &[a, b, id]);
    let ids = editing::append(&mut pasted, &all, Point::new(20., 30.));
    assert_eq!(ids.len(), 3);
    let arrow = &pasted.arrows[0];
    assert!(matches!(arrow.end_anchor.as_ref().unwrap().target,Target::Atom{atom} if atom==ids[1]));
    assert!(
        matches!(arrow.start_anchor.as_ref().unwrap().target,Target::LonePair{atom,..} if atom==ids[0])
    );
    near(arrow.start, d.arrows[0].start.offset(20., 30.));
    pasted.validate().unwrap();
    let only = editing::selection(&d, &[id]);
    assert_eq!(only.arrows[0].start, d.arrows[0].start);
    assert!(only.arrows[0].start_anchor.is_none() && only.arrows[0].end_anchor.is_none());
    only.validate().unwrap();
    let mut larger_ink = Document::default();
    larger_ink.drawing_style.font_size_pt = 48.;
    editing::append(&mut larger_ink, &all, Point::new(20., 30.));
    let arrow = &larger_ink.arrows[0];
    let center = arrow
        .start_anchor
        .as_ref()
        .unwrap()
        .target
        .center(&larger_ink)
        .unwrap();
    let dot_radius = DEFAULT.world(48. * 0.75) * 0.1;
    let required = dot_radius
        + arrow.start_anchor.as_ref().unwrap().gap
        + DEFAULT.world(arrow.appearance().width_pt) * 0.5;
    assert!(
        center.y - arrow.start.y >= required,
        "paste resolves the destination's actual larger dot ink before export"
    );
    let once = larger_ink.clone();
    reconcile(&mut larger_ink);
    assert_eq!(larger_ink, once);
}
#[test]
fn mechanism_attachment_92_deleted_targets_detach_cached_geometry_and_undo_restores() {
    let (d, a, b, _) = fixture();
    for change in 0..3 {
        let mut next = d.clone();
        let before = next.clone();
        let mut history = History::default();
        match change {
            0 => next.delete(&[a]),
            1 => next.atom_mut(a).unwrap().marks.clear(),
            _ => next.atom_mut(a).unwrap().marks[0].kind = MarkKind::Radical,
        };
        transaction::apply(&mut next, &mut history, before, false).unwrap();
        assert!(next.arrows[0].start_anchor.is_none());
        assert_eq!(next.arrows[0].start, d.arrows[0].start);
        assert_eq!(
            next.arrows[0].cubic.unwrap()[0],
            d.arrows[0].cubic.unwrap()[0]
        );
        assert!(next.arrows[0].end_anchor.is_some());
        assert!(history.undo(&mut next));
        assert_eq!(next, d);
        assert!(history.redo(&mut next));
    }
    let mut next = d.clone();
    next.delete(&[b]);
    assert!(next.arrows[0].end_anchor.is_none());
    let mut next = d.clone();
    let old = next.atom(a).unwrap().marks[0].id;
    next.atom_mut(a).unwrap().marks.clear();
    scientific::attach(
        next.atom_mut(a).unwrap(),
        SymbolKind::LonePair,
        Point::new(0., -25.),
    )
    .unwrap();
    reconcile(&mut next);
    assert_ne!(next.atom(a).unwrap().marks[0].id, old);
    assert!(next.arrows[0].start_anchor.is_none());
}
#[test]
fn mechanism_attachment_92_bond_pair_fraction_survives_reordering_and_remaps() {
    let mut d = Document::default();
    let a = d.add_atom("C", Point::new(0., 0.));
    let b = d.add_atom("C", Point::new(100., 0.));
    let c = d.add_atom("O", Point::new(50., -80.));
    d.add_bond(b, a, 1, "bold");
    let source = pick(&d, Point::new(30., 0.), 2.).unwrap();
    assert!(
        matches!(source,Pick::Bond{a:x,b:y,fraction} if x==a&&y==b&&(fraction-0.3).abs()<0.001)
    );
    create(
        &mut d,
        &source,
        &Pick::Atom(c),
        Preset::Fishhook,
        ArrowStyle::preset(Preset::Fishhook),
    )
    .unwrap();
    d.bonds.reverse();
    let bond = &mut d.bonds[0];
    std::mem::swap(&mut bond.a, &mut bond.b);
    let old = d.clone();
    reconcile(&mut d);
    assert_eq!(d, old);
    let mut reverse_atom_order = d.clone();
    reverse_atom_order.atoms.reverse();
    let all = editing::selection(&reverse_atom_order, &reverse_atom_order.all_ids());
    let mut pasted = Document::default();
    editing::append(&mut pasted, &all, Point::new(8., 9.));
    pasted.validate().unwrap();
    let target = &pasted.arrows[0].start_anchor.as_ref().unwrap().target;
    assert!(matches!(target, Target::Bond { a: 2, b: 3, fraction }
        if (*fraction - 0.7).abs() < 0.001));
    near(target.center(&pasted).unwrap(), Point::new(38., 9.));
    let last = d.arrows[0].start;
    d.bonds.clear();
    reconcile(&mut d);
    assert!(d.arrows[0].start_anchor.is_none());
    assert_eq!(d.arrows[0].start, last);
}
#[test]
fn mechanism_attachment_92_legacy_marks_native21_and_validation() {
    let mut d = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/mechanism-attachments-92/before.rsk"
    ))
    .unwrap();
    let original = d.clone();
    assert!(
        d.atoms
            .iter()
            .flat_map(|a| &a.marks)
            .all(|m| m.id.is_none())
    );
    let source = pick(&d, Point::new(0., -29.166668), 2.).unwrap();
    let target = Pick::Atom(1);
    create(
        &mut d,
        &source,
        &target,
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    assert!(!transaction::chemistry_changed(&original, &d));
    let atom = d.atom(2).unwrap();
    assert_eq!(atom.position, original.atom(2).unwrap().position);
    assert!(atom.marks[0].id.is_some());
    assert_eq!(d.bonds, original.bonds);
    let bytes = d.file_json().unwrap();
    let saved = Document::from_native_file(&bytes).unwrap();
    assert_eq!(saved, d.current());
    assert_eq!(saved.version, crate::document::DISTANCE_BONDS);
    let mut duplicate = saved.clone();
    let mark = duplicate.atom(2).unwrap().marks[0].clone();
    duplicate.atom_mut(2).unwrap().marks.push(mark);
    assert!(duplicate.validate().is_err());
    let mut missing_mark = saved.clone();
    missing_mark.arrows[0].start_anchor.as_mut().unwrap().target =
        Target::LonePair { atom: 2, mark: 999 };
    assert!(
        Document::from_native_file(&missing_mark.file_json().unwrap())
            .unwrap_err()
            .contains("Invalid mechanism-arrow attachment")
    );
    let mut replaced = original.clone();
    replaced.atom_mut(2).unwrap().marks.clear();
    scientific::attach(
        replaced.atom_mut(2).unwrap(),
        SymbolKind::LonePair,
        Point::new(0., -29.166668),
    )
    .unwrap();
    let before = replaced.clone();
    assert!(
        create(
            &mut replaced,
            &source,
            &target,
            Preset::Curved,
            ArrowStyle::default()
        )
        .is_err()
    );
    assert_eq!(
        replaced, before,
        "a stale pending mark pick cannot attach to its replacement"
    );
    let mut stale = saved.clone();
    stale.arrows[0].start = Point::new(999., 999.);
    let loaded = Document::from_native_file(&stale.file_json().unwrap()).unwrap();
    assert_eq!(loaded.arrows[0].start, saved.arrows[0].start);
    let mut same = original.clone();
    let before = same.clone();
    assert!(
        create(
            &mut same,
            &source,
            &source,
            Preset::Curved,
            ArrowStyle::default()
        )
        .is_err()
    );
    assert_eq!(same, before);
}

#[test]
fn mechanism_attachment_92_actual_before_desktop_arrows_remain_free_and_exact() {
    let bytes = include_bytes!(
        "../../../../tests/fixtures/mechanism-attachments-92/two-free-arrows-before.rsk"
    );
    let d = Document::from_native_file(bytes).unwrap();
    assert_eq!(d.arrows.len(), 2);
    assert!(
        d.arrows
            .iter()
            .all(|a| a.start_anchor.is_none() && a.end_anchor.is_none() && a.cubic.is_none())
    );
    let paths: Vec<_> = d.arrows.iter().map(Arrow::paths).collect();
    let reopened = Document::from_native_file(&d.file_json().unwrap()).unwrap();
    for (arrow, path) in reopened.arrows.iter().zip(paths) {
        let parts = |paths: Vec<crate::arrows::ArrowPath>| {
            paths
                .into_iter()
                .map(|p| (p.commands, p.style, p.filled))
                .collect::<Vec<_>>()
        };
        assert_eq!(parts(arrow.paths()), parts(path));
    }
}

#[test]
fn mechanism_attachment_92_ink_shrink_never_pulls_fixed_end_inward_and_growth_carries_control() {
    let (mut d, a, b, _) = fixture();
    // A wide H label is a real visible target; derived-cache shrinkage or a
    // smaller font may retain extra clearance, never move the fixed endpoint.
    d.atom_mut(b).unwrap().element = "O".into();
    d.atom_mut(b).unwrap().label_h = 2;
    d.atom_mut(b).unwrap().text_style = Some(crate::typography::TextStyle {
        size_pt: 24.,
        ..Default::default()
    });
    reconcile(&mut d);
    let large = d.arrows[0].clone();
    d.atom_mut(b).unwrap().label_h = 0;
    d.atom_mut(b).unwrap().text_style.as_mut().unwrap().size_pt = 6.;
    reconcile(&mut d);
    assert_eq!(d.arrows[0], large);
    d.translate(&[a], 8., 3.);
    assert_eq!(d.arrows[0].end, large.end);
    assert_eq!(d.arrows[0].cubic.unwrap()[1], large.cubic.unwrap()[1]);
    let before = d.arrows[0].clone();
    d.atom_mut(b).unwrap().text_style.as_mut().unwrap().size_pt = 48.;
    reconcile(&mut d);
    let next = &d.arrows[0];
    let displacement = delta(next.end, before.end);
    near(
        next.cubic.unwrap()[1],
        before.cubic.unwrap()[1].offset(displacement.x, displacement.y),
    );
    assert_eq!(
        (next.start, next.cubic.unwrap()[0]),
        (before.start, before.cubic.unwrap()[0])
    );
    let once = d.clone();
    for _ in 0..20 {
        reconcile(&mut d);
    }
    assert_eq!(d, once);
}

#[test]
fn mechanism_attachment_92_bond_clearance_uses_final_normal_after_anisotropic_stretch() {
    let mut d = Document::default();
    let a = d.add_atom("C", Point::new(0., 0.));
    let b = d.add_atom("C", Point::new(100., 100.));
    let c = d.add_atom("O", Point::new(80., -70.));
    d.add_bond(a, b, 1, "bold");
    let source = pick(&d, Point::new(30., 30.), 2.).unwrap();
    let id = create(
        &mut d,
        &source,
        &Pick::Atom(c),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    let before = d.arrows[0].clone();
    editing::scale_axes_about(&mut d, &[a, b, c, id], Point::default(), 20., 0.02);
    let arrow = &d.arrows[0];
    let anchor = arrow.start_anchor.as_ref().unwrap();
    // Independently measure the physical bold stroke, rather than reuse the
    // resolver's clearance predicate. The diagonal becomes (2000, 2).
    let center = Point::new(600., 0.6);
    let length = 2000.0_f32.hypot(2.);
    let normal = Point::new(2. / length, -2000. / length);
    let required = d.drawing_style.world(d.drawing_style.bold_width_pt) * 0.5
        + anchor.gap
        + d.drawing_style.world(arrow.appearance().width_pt) * 0.5;
    let physical_distance =
        (arrow.start.x - center.x) * normal.x + (arrow.start.y - center.y) * normal.y;
    assert!(
        physical_distance >= required,
        "{physical_distance} < {required}"
    );
    let affine = |p: Point| Point::new(p.x * 20., p.y * 0.02);
    let correction = delta(arrow.start, affine(before.start));
    near(
        arrow.cubic.unwrap()[0],
        affine(before.cubic.unwrap()[0]).offset(correction.x, correction.y),
    );
    let once = d.clone();
    reconcile(&mut d);
    assert_eq!(d, once);
}

fn opacity_cage(alpha: f32) -> Document {
    let mut doc = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
    doc
}
#[test]
fn composed_attachments_reject_hidden_cage_atom_lone_pair_and_projected_bond() {
    let mut doc = opacity_cage(0.);
    let atom = 52; // Rear interior vertex of the pinned C60, not a depth-only rule.
    let p = doc.atom(atom).unwrap().position;
    scientific::attach(
        doc.atom_mut(atom).unwrap(),
        SymbolKind::LonePair,
        Point::new(-3., -3.),
    )
    .unwrap();
    let mark = doc.atom(atom).unwrap().marks.last().unwrap().clone();
    let hidden = [
        Pick::Atom(atom),
        Pick::LonePair {
            atom,
            mark: mark.clone(),
        },
        Pick::Bond {
            a: 52,
            b: 57,
            fraction: 0.5,
        },
    ];
    let ink = crate::scene::attachment_ink::AttachmentInk::new(&doc);
    assert_eq!(crate::rear_opacity::Paint::new(&doc).atom(atom), 0.);
    let bond_index = doc
        .bonds
        .iter()
        .position(|b| b.a == 52 && b.b == 57)
        .unwrap();
    assert_eq!(
        doc.bonds[bond_index].order, 2,
        "Exercise projected secondary rail rendering"
    );
    assert!(!ink.bond(bond_index, 0.5));
    assert_ne!(pick(&doc, p, 0.2), Some(hidden[0].clone()));
    assert_ne!(
        pick(&doc, p.offset(mark.offset.x, mark.offset.y), 0.2),
        Some(hidden[1].clone())
    );
    let center = hidden[2].center(&doc).unwrap();
    assert!(!matches!(
        pick(&doc, center, 0.2),
        Some(Pick::Bond { a: 52, b: 57, .. })
    ));
    let destination = doc.add_atom("O", Point::new(250., 150.));
    for cached in hidden {
        let before = doc.clone();
        let error = create(
            &mut doc,
            &cached,
            &Pick::Atom(destination),
            Preset::Curved,
            ArrowStyle::preset(Preset::Curved),
        )
        .unwrap_err();
        assert!(error.contains("hidden"));
        assert_eq!(
            doc, before,
            "A failed cached target must allocate no arrow/mark ID"
        );
    }
    let ids: Vec<_> = (1..=60).collect();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    assert!(
        matches!(
            pick(&doc, p.offset(mark.offset.x, mark.offset.y), 0.2),
            Some(Pick::LonePair { atom: 52, .. })
        ),
        "Positive-alpha rear ink is still a new attachment target"
    );
}
#[test]
fn composed_attachments_keep_exposed_negative_z_rim_targets_at_zero_opacity() {
    let mut doc = opacity_cage(0.);
    // These neighboring hull vertices are both behind Z=0, but fully exposed.
    for id in [38, 48] {
        let a = doc.atom(id).unwrap();
        assert!(a.depth < 0.);
        assert_eq!(pick(&doc, a.position, 0.2), Some(Pick::Atom(id)));
    }
    let source = Pick::Bond {
        a: 38,
        b: 48,
        fraction: 0.5,
    };
    let p = source.center(&doc).unwrap();
    assert!(matches!(
        pick(&doc, p, 0.2),
        Some(Pick::Bond { a: 38, b: 48, .. })
    ));
    let destination = doc.add_atom("O", Point::new(-250., 150.));
    create(
        &mut doc,
        &source,
        &Pick::Atom(destination),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    assert_eq!(doc.arrows.len(), 1);
    assert_eq!(
        doc.arrows[0].start_anchor.as_ref().unwrap().target,
        Target::Bond {
            a: 38,
            b: 48,
            fraction: 0.5
        }
    );
}
#[test]
fn composed_attachments_existing_hidden_lone_pair_link_survives_public_reopen() {
    let mut doc = opacity_cage(1.);
    scientific::attach(
        doc.atom_mut(52).unwrap(),
        SymbolKind::LonePair,
        Point::new(-3., -3.),
    )
    .unwrap();
    let source = Pick::LonePair {
        atom: 52,
        mark: doc.atom(52).unwrap().marks.last().unwrap().clone(),
    };
    let destination = doc.add_atom("O", Point::new(250., 150.));
    create(
        &mut doc,
        &source,
        &Pick::Atom(destination),
        Preset::Fishhook,
        ArrowStyle::preset(Preset::Fishhook),
    )
    .unwrap();
    let arrow = doc.arrows[0].clone();
    let serial = doc.atom(52).unwrap().mark_serial;
    let atoms = doc.atoms.clone();
    let bonds = doc.bonds.clone();
    let ids: Vec<_> = (1..=60).collect();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    assert_eq!(crate::rear_opacity::Paint::new(&doc).atom(52), 0.);
    reconcile(&mut doc);
    reconcile(&mut doc);
    assert_eq!(doc.arrows[0], arrow);
    assert_eq!(doc.atoms, atoms);
    assert_eq!(doc.bonds, bonds);
    assert_eq!(doc.atom(52).unwrap().mark_serial, serial);
    let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened, doc.current());
    assert_eq!(reopened.arrows[0].start_anchor, arrow.start_anchor);
    assert_eq!(reopened.arrows[0].cubic, arrow.cubic);
}

/// Manual observations of cold per-query visibility caches, not a latency gate.
#[test]
#[ignore = "manual cold attachment-query observations; no performance threshold"]
fn manual_cold_cage_attachment_queries_report_surviving_ink() {
    use std::{hint::black_box, time::Instant};

    const SAMPLES: usize = 5;
    const RADIUS: f32 = 0.2;
    let baseline = opacity_cage(0.25);
    assert_eq!(baseline.version, 22, "Keep the authentic fixture version");
    assert_eq!((baseline.atoms.len(), baseline.bonds.len()), (60, 90));
    let ids = baseline.all_ids();
    let rear_atom = baseline.atom(52).unwrap().position;
    let rim_atom = baseline.atom(38).unwrap().position;
    assert!(baseline.atom(38).unwrap().depth < 0.);
    assert!(baseline.atom(48).unwrap().depth < 0.);
    let rear_bond = Pick::Bond {
        a: 52,
        b: 57,
        fraction: 0.5,
    }
    .center(&baseline)
    .unwrap();
    let rim_bond = Pick::Bond {
        a: 38,
        b: 48,
        fraction: 0.5,
    }
    .center(&baseline)
    .unwrap();
    let (_, hi) = baseline.bounds();
    let queries = [
        ("rear_atom", rear_atom),
        ("rear_projected_bond", rear_bond),
        ("exposed_negative_z_rim_atom", rim_atom),
        ("exposed_negative_z_rim_bond", rim_bond),
        ("remote_empty", hi.offset(1000., 1000.)),
    ];
    let fixture_bytes =
        include_bytes!("../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk").len();

    for alpha in [0., 0.25, 1.] {
        let mut doc = baseline.clone();
        crate::depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        assert_eq!(
            doc.atoms, baseline.atoms,
            "Compare the same chemical graph and XYZ"
        );
        assert_eq!(
            doc.bonds, baseline.bonds,
            "Retain every original bond and style"
        );
        let unchanged = doc.clone();
        let native_bytes = doc.file_json().unwrap().len();
        let mut rows = Vec::new();
        for (query, point) in queries {
            let mut observations = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                // The public picker creates a new AttachmentInk each call;
                // no prepared Paint, scene or visibility cache is handed in.
                let started = Instant::now();
                let hit = black_box(pick(black_box(&doc), black_box(point), black_box(RADIUS)));
                let elapsed_ns = started.elapsed().as_nanos();
                match query {
                    "rear_atom" if alpha == 0. => {
                        assert_ne!(hit, Some(Pick::Atom(52)), "Hidden ink is not a new target");
                    }
                    "rear_atom" => assert_eq!(hit, Some(Pick::Atom(52))),
                    "rear_projected_bond" if alpha == 0. => {
                        assert!(!matches!(
                            hit.as_ref(),
                            Some(Pick::Bond { a: 52, b: 57, .. })
                        ));
                    }
                    "rear_projected_bond" => assert!(matches!(hit.as_ref(),
                        Some(Pick::Bond { a: 52, b: 57, fraction }) if (*fraction - 0.5).abs() < 0.0001)),
                    "exposed_negative_z_rim_atom" => assert_eq!(hit, Some(Pick::Atom(38))),
                    "exposed_negative_z_rim_bond" => assert!(matches!(hit.as_ref(),
                        Some(Pick::Bond { a: 38, b: 48, fraction }) if (*fraction - 0.5).abs() < 0.0001)),
                    "remote_empty" => assert_eq!(hit, None),
                    _ => panic!("Unexpected authored query"),
                }
                observations.push((elapsed_ns, hit));
            }
            let first_hit = observations.first().unwrap().1.clone();
            assert!(
                observations.iter().all(|(_, hit)| *hit == first_hit),
                "Picking the same immutable document is deterministic"
            );
            let mut durations: Vec<_> = observations.iter().map(|(ns, _)| *ns).collect();
            durations.sort_unstable();
            rows.push((query, first_hit, durations));
        }
        // Size and classification observations run after the timed calls.
        // This does not measure private cache bytes, heap/RSS or pixels.
        let primitives = crate::scene::primitives(&doc).len();
        let paint = crate::rear_opacity::Paint::new(&doc);
        assert_eq!(paint.atom(52), alpha);
        assert_eq!(paint.atom(38), 1., "An exposed negative-Z rim stays solid");
        assert_eq!(
            doc, unchanged,
            "Cold picking and rendering must not mutate native data"
        );
        for (query, hit, durations) in rows {
            eprintln!(
                "cold_attachment_query alpha={alpha} query={query} samples={SAMPLES} \
                 atoms={} bonds={} fixture_bytes={fixture_bytes} native_bytes={native_bytes} \
                 scene_primitives={primitives} min_ns={} median_ns={} max_ns={} hit={hit:?}",
                doc.atoms.len(),
                doc.bonds.len(),
                durations.first().unwrap(),
                durations.get(SAMPLES / 2).unwrap(),
                durations.last().unwrap(),
            );
        }
    }
}
/// Smallest angle between the arrival tangent at `end` and a bond leaving `atom`.
fn head_bond_clearance(d: &Document, arrow: &Arrow, atom: u64) -> f32 {
    let center = d.atom(atom).unwrap().position;
    let head = unit(delta(arrow.cubic.unwrap()[1], arrow.end));
    d.bonds
        .iter()
        .filter(|b| b.a == atom || b.b == atom)
        .map(|b| {
            let other = d.atom(if b.a == atom { b.b } else { b.a }).unwrap();
            dot(head, unit(delta(other.position, center)))
                .clamp(-1., 1.)
                .acos()
                .to_degrees()
        })
        .fold(180., f32::min)
}
fn lone_pair_arrow_to(d: &mut Document, destination: u64) -> u64 {
    lone_pair_arrow_from(d, -120., destination)
}
/// Arrow from a lone pair 25 units right of an O at `(x, 0)`.
fn lone_pair_arrow_from(d: &mut Document, x: f32, destination: u64) -> u64 {
    let o = d.add_atom("O", Point::new(x, 0.));
    scientific::attach(
        d.atom_mut(o).unwrap(),
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .unwrap();
    let source = pick(d, Point::new(x + 25., 0.), 2.).unwrap();
    assert!(matches!(source, Pick::LonePair { .. }));
    create(
        d,
        &source,
        &Pick::Atom(destination),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap()
}
#[test]
fn mechanism_attachment_312_lone_pair_to_carbonyl_head_clears_methyl_bond() {
    // Methoxide lone pair left of acetone; the default arrival direction runs
    // exactly along one C-CH3 bond, which put the arrowhead on that bond.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    let along = 0.7f32.atan2(-1.);
    for (k, symbol, order) in [(0., "C", 1), (1., "C", 1), (2., "O", 2)] {
        let angle = along + k * 2. * std::f32::consts::FRAC_PI_3;
        let n = d.add_atom(symbol, Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let clearance = head_bond_clearance(&d, arrow, c);
    assert!(
        clearance > 55.,
        "arrowhead within {clearance} deg of a bond"
    );
    // The head still approaches from the source side.
    assert!(arrow.cubic.unwrap()[1].x < arrow.end.x);
}
#[test]
fn mechanism_attachment_312_pi_bond_to_own_oxygen_head_clears_the_bond() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    let o = d.add_atom("O", Point::new(0., -42.));
    d.add_bond(c, o, 2, "plain");
    let (a, b) = if c < o { (c, o) } else { (o, c) };
    let id = create(
        &mut d,
        &Pick::Bond {
            a,
            b,
            fraction: 0.5,
        },
        &Pick::Atom(o),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let clearance = head_bond_clearance(&d, arrow, o);
    assert!(
        clearance > 55.,
        "arrowhead within {clearance} deg of the C=O bond"
    );
    assert!(rendered_head_ink_gap(&d, arrow) > 0.);
}
#[test]
fn mechanism_attachment_699_acetyl_chloride_pi_to_oxygen_has_a_bond_sized_bow() {
    for side in [-1., 1.] {
        for fraction in [0.3, 0.32, 0.5, 0.7] {
            let mut d = Document::default();
            let c = d.add_atom("C", Point::new(0., 0.));
            let o = d.add_atom("O", Point::new(0., -42.));
            d.add_bond(c, o, 2, "plain");
            for (symbol, x, y) in [("C", -36., 21.), ("Cl", 39., 15.)] {
                let n = d.add_atom(symbol, Point::new(x * side, y));
                d.add_bond(c, n, 1, "plain");
            }
            let id = create(
                &mut d,
                &Pick::Bond {
                    a: c,
                    b: o,
                    fraction,
                },
                &Pick::Atom(o),
                Preset::Curved,
                ArrowStyle::preset(Preset::Curved),
            )
            .unwrap();
            let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
            let points: Vec<_> = (0..=256).map(|i| arrow.point(i as f32 / 256.)).collect();
            // The methyl side has more free angle than the chlorine side.
            assert!(points.iter().all(|p| p.x * side < 0.));
            let min = points.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let max = points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            assert!(
                max - min >= 42. * 0.6,
                "bow extent {} at {fraction}",
                max - min
            );
            assert!(ink_gap(&d, &points) > DEFAULT.world(arrow.appearance().width_pt) * 0.5);
            assert!(rendered_head_ink_gap(&d, arrow) > 0.);
        }
    }
}
#[test]
fn mechanism_attachment_701_bromine_homolysis_fishhooks_have_separate_clear_bows() {
    for reverse in [false, true] {
        let mut d = Document::default();
        let left = d.add_atom("Br", Point::new(0., 0.));
        let right = d.add_atom("Br", Point::new(42., 0.));
        d.add_bond(left, right, 1, "plain");
        for atom in if reverse {
            [right, left]
        } else {
            [left, right]
        } {
            create(
                &mut d,
                &Pick::Bond {
                    a: left,
                    b: right,
                    fraction: 0.5,
                },
                &Pick::Atom(atom),
                Preset::Fishhook,
                ArrowStyle::preset(Preset::Fishhook),
            )
            .unwrap();
        }
        let pen = DEFAULT.world(d.arrows[0].appearance().width_pt) * 0.5;
        let labels: Vec<_> = d
            .atoms
            .iter()
            .flat_map(|a| crate::scene::atom_label_ink_boxes(a, &d))
            .collect();
        for arrow in &d.arrows {
            let points: Vec<_> = (0..=256).map(|i| arrow.point(i as f32 / 256.)).collect();
            assert!(ink_gap(&d, &points) > pen, "shaft touches Br-Br ink");
            assert!(rendered_head_ink_gap(&d, arrow) > 0.);
            for path in arrow.paths() {
                for contour in crate::graphics::flattened(&path.commands) {
                    for p in contour {
                        for (lo, hi) in &labels {
                            let nearest = Point::new(p.x.clamp(lo.x, hi.x), p.y.clamp(lo.y, hi.y));
                            assert!(p.distance(nearest) > pen, "arrow touches a Br label");
                        }
                    }
                }
            }
        }
        let segments: Vec<Vec<_>> = d
            .arrows
            .iter()
            .map(|arrow| {
                arrow
                    .paths()
                    .iter()
                    .flat_map(|path| {
                        crate::graphics::flattened(&path.commands)
                            .into_iter()
                            .flat_map(|contour| {
                                contour.windows(2).map(|w| (w[0], w[1])).collect::<Vec<_>>()
                            })
                    })
                    .collect()
            })
            .collect();
        assert!(
            segments[0].iter().all(|(a, b)| segments[1]
                .iter()
                .all(|(c, d)| segment_gap(*a, *b, *c, *d) > 2. * pen)),
            "fishhook ink overlaps"
        );
    }
}
#[test]
fn mechanism_attachment_312_bond_avoidance_keeps_approach_on_source_side() {
    // The widest gap lies behind the target; the arrival must not swing there.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [80f32, 190., 310.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let arrival = unit(delta(arrow.cubic.unwrap()[1], arrow.end));
    assert!(
        arrival.x < 0.,
        "arrival {arrival:?} turned behind the target"
    );
    assert!(head_bond_clearance(&d, arrow, c) > 50.);
    assert!(rendered_head_ink_gap(&d, arrow) > 0.);
}
#[test]
fn mechanism_attachment_312_bond_avoidance_stays_in_the_default_opening() {
    // The default arrives between bonds at 120 and 180 degrees; a wider gap
    // past the 120-degree bond must not pull the shaft across that bond.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [120f32, 180.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let arrival = unit(delta(arrow.cubic.unwrap()[1], arrow.end));
    let degrees = arrival.y.atan2(arrival.x).to_degrees();
    assert!(
        (120.0..=180.).contains(&degrees),
        "arrival {degrees} deg left the opening between the bonds"
    );
    assert!(head_bond_clearance(&d, arrow, c) > 25.);
}
#[test]
fn mechanism_attachment_312_clear_default_arrival_is_unchanged() {
    // A destination whose bonds are already clear keeps the default arrival.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    let n = d.add_atom("C", Point::new(42., 0.));
    d.add_bond(c, n, 1, "plain");
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    near(
        unit(delta(arrow.cubic.unwrap()[1], arrow.end)),
        unit(Point::new(-1., 0.7)),
    );
}
/// Smallest gap between the filled arrowhead outlines `arrow` renders and
/// rendered bond ink.
fn rendered_head_ink_gap(d: &Document, arrow: &Arrow) -> f32 {
    let mut samples = Vec::new();
    for path in arrow.paths().into_iter().filter(|p| p.filled) {
        for outline in crate::graphics::flattened(&path.commands) {
            for edge in outline.windows(2) {
                samples.extend((0..=128).map(|k| edge[0].lerp(edge[1], k as f32 / 128.)));
            }
        }
    }
    assert!(!samples.is_empty(), "arrow renders no filled head");
    ink_gap(d, &samples)
}
/// Smallest gap between `samples` and the bond ink rendered without arrows.
fn ink_gap(d: &Document, samples: &[Point]) -> f32 {
    use crate::{graphics::PathCommand, scene::Primitive};
    let mut bare = d.clone();
    bare.arrows.clear();
    let segment = |p: Point, a: Point, b: Point| {
        let v = delta(b, a);
        let t = (dot(delta(p, a), v) / dot(v, v).max(0.00001)).clamp(0., 1.);
        p.distance(a.offset(v.x * t, v.y * t))
    };
    // Stroked lines as (a, b, half width); filled bond shapes as polygons.
    let mut lines = Vec::new();
    let mut polygons: Vec<Vec<Point>> = Vec::new();
    for primitive in crate::scene::primitives(&bare) {
        match primitive {
            Primitive::Line(a, b, width) => lines.push((a, b, width / 2.)),
            Primitive::Polygon(points) => polygons.push(points),
            Primitive::Path {
                commands,
                filled: true,
                ..
            } => {
                for command in commands {
                    match command {
                        PathCommand::Move(p) => polygons.push(vec![p]),
                        PathCommand::Line(p) | PathCommand::Cubic(_, _, p) => {
                            polygons.last_mut().unwrap().push(p);
                        }
                        PathCommand::Close => {}
                    }
                }
            }
            Primitive::Path {
                commands,
                style,
                filled: false,
            } => {
                let half = DEFAULT.world(style.width_pt) / 2.;
                for contour in crate::graphics::flattened(&commands) {
                    lines.extend(contour.windows(2).map(|w| (w[0], w[1], half)));
                }
            }
            _ => {}
        }
    }
    let polygon_gap = |p: Point, points: &[Point]| {
        let edges = || points.iter().zip(points.iter().cycle().skip(1));
        let inside = edges()
            .filter(|(a, b)| {
                (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x)
            })
            .count()
            % 2
            == 1;
        let edge = edges()
            .map(|(a, b)| segment(p, *a, *b))
            .fold(f32::MAX, f32::min);
        if inside { -edge } else { edge }
    };
    let mut worst = (f32::MAX, Point::default());
    for p in samples {
        for g in lines
            .iter()
            .map(|(a, b, w)| segment(*p, *a, *b) - w)
            .chain(polygons.iter().map(|q| polygon_gap(*p, q)))
        {
            if g < worst.0 {
                worst = (g, *p);
            }
        }
    }
    worst.0
}
#[test]
fn mechanism_attachment_312_lone_pair_to_acetone_head_clears_rendered_bonds() {
    // Acetone drawn with C=O up; the default arrival from a nucleophile on
    // the left ran along the lower-left methyl bond.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, symbol, order) in [(270f32, "O", 2), (150., "C", 1), (30., "C", 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom(symbol, Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_671_level_methoxide_to_acetone_uses_the_source_opening() {
    for degrees in -10..=10 {
        let mut d = Document::default();
        let c = d.add_atom("C", Point::new(0., 0.));
        for (degrees, symbol, order) in [(270f32, "O", 2), (150., "C", 1), (30., "C", 1)] {
            let angle = degrees.to_radians();
            let n = d.add_atom(symbol, Point::new(42. * angle.cos(), 42. * angle.sin()));
            d.add_bond(c, n, order, "plain");
        }
        let angle = (degrees as f32).to_radians();
        let o = d.add_atom("O", Point::new(-126., -106. * angle.tan()));
        d.atom_mut(o).unwrap().charge = -1;
        let methyl = d.add_atom("C", d.atom(o).unwrap().position.offset(-42., 0.));
        d.add_bond(methyl, o, 1, "plain");
        scientific::attach(
            d.atom_mut(o).unwrap(),
            SymbolKind::LonePair,
            Point::new(20., 0.),
        )
        .unwrap();
        let source = Pick::LonePair {
            atom: o,
            mark: d.atom(o).unwrap().marks[0].clone(),
        };
        let id = create(
            &mut d,
            &source,
            &Pick::Atom(c),
            Preset::Curved,
            ArrowStyle::preset(Preset::Curved),
        )
        .unwrap();
        let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
        assert!(
            rendered_head_ink_gap(&d, arrow) > 0.,
            "head touches bond at {degrees} deg"
        );
        let shaft: Vec<_> = (0..=512).map(|i| arrow.point(i as f32 / 512.)).collect();
        assert!(
            ink_gap(&d, &shaft) > DEFAULT.world(arrow.appearance().width_pt) * 0.5,
            "shaft touches bond at {degrees} deg"
        );
    }
}
#[test]
fn mechanism_attachment_312_head_clears_bold_bond_width() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, display) in [(130f32, "plain"), (190., "bold")] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, display);
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_aromatic_dashed_rail() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order) in [(25f32, 4), (180., 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_search_never_steps_off_a_sampled_bond() {
    // A search step lands exactly on the 195-degree bond; it must stop there
    // rather than cross into the next opening.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    let preferred = 0.7f32.atan2(-1.).to_degrees();
    for degrees in [preferred - 25., preferred + 50.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let arrival = unit(delta(arrow.cubic.unwrap()[1], arrow.end));
    let degrees = arrival.y.atan2(arrival.x).to_degrees().rem_euclid(360.);
    assert!(
        (preferred - 25.0..=preferred + 50.).contains(&degrees),
        "arrival {degrees} deg left the opening between the bonds"
    );
}
#[test]
fn mechanism_attachment_312_short_arrow_scores_its_shortened_head() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order) in [(50f32, 2), (175., 1), (300., 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    d.bonds[0].double_position = crate::bonds::DoublePosition::Center;
    let id = lone_pair_arrow_from(&mut d, -55., c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_projected_dative_uses_its_drawn_width() {
    let mut d = Document::default();
    d.drawing_style.bold_width_pt = 2.6;
    let c = d.add_atom("C", Point::new(0., 0.));
    let angle = 100f32.to_radians();
    let donor = d.add_atom("N", Point::new(42. * angle.cos(), 42. * angle.sin()));
    let side = d.add_atom("C", Point::new(-42., 0.));
    d.add_bond(c, donor, 5, "wedge_end");
    d.bonds[0].projection = true;
    d.add_bond(c, side, 1, "plain");
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_crossed_wavy_double_bond() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order, display) in [(70f32, 2, "wavy"), (160., 1, "plain"), (280., 1, "plain")] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, display);
    }
    d.bonds[0].double_position = crate::bonds::DoublePosition::Left;
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_long_thin_head_is_checked_along_its_length() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order) in [(269f32, 3), (126., 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    let o = d.add_atom("O", Point::new(-240., 0.));
    scientific::attach(
        d.atom_mut(o).unwrap(),
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .unwrap();
    let source = pick(&d, Point::new(-215., 0.), 2.).unwrap();
    let mut style = ArrowStyle::preset(Preset::Curved);
    (style.head_length_pt, style.head_width_pt, style.width_pt) = (24., 0.25, 0.6);
    let id = create(&mut d, &source, &Pick::Atom(c), Preset::Curved, style).unwrap();
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_bold_secondary_rail() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order) in [(43f32, 2), (159., 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    d.bonds[0].double_position = crate::bonds::DoublePosition::Center;
    d.bonds[0].secondary_display = Some("bold".into());
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_aromatic_rail_ignores_saved_position() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order) in [(30f32, 4), (180., 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    d.bonds[0].double_position = crate::bonds::DoublePosition::Left;
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_short_side_double_bond_does_not_panic() {
    // The set-back inner rail of a side double bond has no length here.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    let n = d.add_atom("C", Point::new(4., 0.));
    d.add_bond(c, n, 2, "plain");
    d.bonds[0].double_position = crate::bonds::DoublePosition::Right;
    lone_pair_arrow_to(&mut d, c);
}
#[test]
fn mechanism_attachment_312_head_clears_double_bond_rails() {
    // A centered C=O beside the opening: angles from bond centre lines alone
    // would put the head on a rail.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, order) in [(70f32, 2), (160., 1), (280., 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_mitered_bold_junction() {
    // The outer miter of a narrow bold V points straight at the default
    // arrival, past both strokes.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [295f32, 355.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "bold");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_mitered_side_double_backbone() {
    // A side double bond's backbone is mitered like a single bond.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [295f32, 355.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 2, "bold");
    }
    for bond in &mut d.bonds {
        bond.double_position = crate::bonds::DoublePosition::Right;
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_hatch_stroke() {
    // Hatches span the bold width and are then stroked, so their caps
    // reach past it.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, display) in [(154f32, "hashed"), (200., "plain")] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, display);
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_head_clears_joined_hollow_wedge_outline() {
    // Two hollow wedges meeting at their wide ends share a mitered corner
    // that reaches past either wedge's own width.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [295f32, 355.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(n, c, 1, "hollow_wedge");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_tiny_head_inside_a_miter_fill_is_detected() {
    // A head small enough to sit inside the filled miter touches none of its
    // edges or strokes.
    let mut d = Document::default();
    d.drawing_style.bold_width_pt = 2.742857;
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [320f32, 350.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "bold");
    }
    let o = d.add_atom("O", Point::new(-120., 0.));
    scientific::attach(
        d.atom_mut(o).unwrap(),
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .unwrap();
    let source = pick(&d, Point::new(-95., 0.), 2.).unwrap();
    let mut style = ArrowStyle::preset(Preset::Curved);
    (style.width_pt, style.head_length_pt, style.head_width_pt) = (0.1, 0.5, 0.25);
    let id = create(&mut d, &source, &Pick::Atom(c), Preset::Curved, style).unwrap();
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_search_finds_a_clear_direction_between_coarse_steps() {
    // The only clear arrival (about 142.5 degrees) lies between 5-degree steps.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [128.5f32, 156.5] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "arrowhead overlaps bond ink by {}", -gap);
}
#[test]
fn mechanism_attachment_312_bond_ink_covers_plain_dative_head() {
    // The filled head drawn at the acceptor is much wider than the dative
    // line, and starts past the acceptor's label when it has one.
    for acceptor in ["C", "Pd"] {
        let mut d = Document::default();
        let c = d.add_atom(acceptor, Point::new(0., 0.));
        let donor = d.add_atom("N", Point::new(-30., 30.));
        d.add_bond(donor, c, 5, "plain");
        let (_, lines, _) = bond_ink(&d, c, Point::new(0., 0.));
        let corners: Vec<Point> = crate::scene::primitives(&d)
            .into_iter()
            .filter_map(|p| match p {
                crate::scene::Primitive::Polygon(points) if points.len() == 3 => Some(points),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(corners.len(), 3, "{acceptor}: one drawn dative head");
        for p in corners {
            let covered = lines.iter().any(|(a, b, half)| {
                let v = delta(*b, *a);
                let t = (dot(delta(p, *a), v) / dot(v, v)).clamp(0., 1.);
                p.distance(a.offset(v.x * t, v.y * t)) <= half + 0.01
            });
            assert!(covered, "{acceptor}: head corner {p:?} outside bond ink");
        }
    }
}
#[test]
fn mechanism_attachment_312_fishhook_barb_clears_rendered_bonds() {
    // A thick shaft widens the solid half head's barb past its configured
    // half-width; the search must keep the drawn barb off the bonds.
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for degrees in [110f32, 180.] {
        let angle = degrees.to_radians();
        let n = d.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, 1, "plain");
    }
    let o = d.add_atom("O", Point::new(-120., 0.));
    scientific::attach(
        d.atom_mut(o).unwrap(),
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .unwrap();
    let source = pick(&d, Point::new(-95., 0.), 2.).unwrap();
    let mut style = ArrowStyle::preset(Preset::Fishhook);
    (style.width_pt, style.head_width_pt, style.head_length_pt) = (4., 0.25, 0.5);
    let id = create(&mut d, &source, &Pick::Atom(c), Preset::Fishhook, style).unwrap();
    let arrow = d.arrows.iter().find(|a| a.id == id).unwrap();
    let gap = rendered_head_ink_gap(&d, arrow);
    assert!(gap > 0., "fishhook barb overlaps bond ink by {}", -gap);
}
#[test]
fn double_bond_strokes_are_arrow_targets_not_only_the_gap() {
    // Issue #330: acetone-style terminal C=O at 225% zoom (8 px pick radius).
    let mut d = Document::default();
    let length = d.drawing_style.bond_length_world;
    let c = d.add_atom("C", Point::new(0., 0.));
    let o = d.add_atom("O", Point::new(length, 0.));
    d.add_bond(c, o, 2, "plain");
    let radius = 8. / 2.25;
    let half = length * d.drawing_style.bond_spacing_ratio / 2.;
    let mid = length / 2.;
    for y in [0., -half, half] {
        assert!(
            matches!(
                pick(&d, Point::new(mid, y), radius),
                Some(Pick::Bond { .. })
            ),
            "stroke at offset {y} was not a target"
        );
    }
    assert_eq!(pick(&d, Point::new(mid, length * 0.5), radius), None);

    // A one-sided double bond is hit on its rail, not on its empty side.
    let spacing = half * 2.;
    let mut sided = d.clone();
    sided.bonds[0].double_position = crate::bonds::DoublePosition::Left;
    let hit = |y: f32| pick(&sided, Point::new(mid, y), radius).is_some();
    assert!(hit(0.) && hit(-spacing / 2.) && hit(-spacing));
    assert!(!hit(spacing));

    // A bold secondary rail is targetable across its whole width.
    let mut bold = d.clone();
    bold.bonds[0].secondary_display = Some("bold".into());
    assert_eq!(pick(&d, Point::new(mid, half + 5.), radius), None);
    assert!(pick(&bold, Point::new(mid, half + 5.), radius).is_some());

    // The widened band never steals a click on a nearby single bond's ink,
    // including on the thin side of a bold-secondary double bond.
    let s = bold.add_atom("C", Point::new(0., -6.));
    let t = bold.add_atom("C", Point::new(length, -6.));
    bold.add_bond(s, t, 1, "plain");
    bold.bonds.swap(0, 1);
    assert!(matches!(
        pick(&bold, Point::new(mid, -6.), radius),
        Some(Pick::Bond { a, b, .. }) if a == s && b == t
    ));
    let s = d.add_atom("C", Point::new(0., -6.));
    let t = d.add_atom("C", Point::new(length, -6.));
    d.add_bond(s, t, 1, "plain");
    d.bonds.swap(0, 1);
    assert!(matches!(
        pick(&d, Point::new(mid, -6.), radius),
        Some(Pick::Bond { a, b, .. }) if a == s && b == t
    ));

    // A single bond inside a later quadruple bond's rail gap keeps its clicks.
    let mut q = Document::default();
    let s = q.add_atom("C", Point::new(0., spacing));
    let t = q.add_atom("C", Point::new(length, spacing));
    q.add_bond(s, t, 1, "plain");
    let qa = q.add_atom("C", Point::new(0., 0.));
    let qb = q.add_atom("C", Point::new(length, 0.));
    q.add_bond(qa, qb, 6, "plain");
    assert!(matches!(
        pick(&q, Point::new(mid, spacing), radius),
        Some(Pick::Bond { a, b, .. }) if a == s && b == t
    ));
}
#[test]
fn nearby_bonds_rank_by_their_drawn_ink() {
    // Each case draws a plain single bond at `y` first and the shaped bond
    // along y = 0 later; a false zero distance would let the later bond win.
    let single_wins = |order: u8, display: &str, y: f32, at: Point| {
        let mut d = Document::default();
        let length = d.drawing_style.bond_length_world;
        let s = d.add_atom("C", Point::new(0., y));
        let t = d.add_atom("C", Point::new(length, y));
        d.add_bond(s, t, 1, "plain");
        let a = d.add_atom("C", Point::new(0., 0.));
        let b = d.add_atom("C", Point::new(length, 0.));
        d.add_bond(a, b, order, display);
        d.bonds[1].double_position = crate::bonds::DoublePosition::Left;
        match pick(&d, at, 2.) {
            Some(Pick::Bond { a, .. }) => a == s,
            other => panic!("expected a bond at {at:?}, got {other:?}"),
        }
    };
    let style = Document::default().drawing_style;
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    // With no adjoining bonds the offset rail reaches the bare end, so the
    // later double bond wins the tie with the overlapping single bond.
    assert!(!single_wins(2, "plain", -spacing, Point::new(3., -spacing)));
    // Beside the narrow end of a wedge.
    assert!(single_wins(1, "wedge", 2.6, Point::new(10., 2.6)));
    // Beside a wave's trough the single bond wins; on its crest the wave does.
    assert!(single_wins(1, "wavy", 2.9, Point::new(18.375, 2.9)));
    assert!(!single_wins(1, "wavy", 3.5, Point::new(23.625, 2.1875)));
    // Hashed bars keep a constant bold width.
    assert!(!single_wins(1, "hashed", 4., Point::new(7.291667, 2.4)));

    // A double bond under a resolved ring arc draws only its backbone.
    let mut d = Document::default();
    let length = d.drawing_style.bond_length_world;
    let s = d.add_atom("C", Point::new(0., 3.));
    let t = d.add_atom("C", Point::new(length, 3.));
    d.add_bond(s, t, 1, "plain");
    let h = length * 3f32.sqrt() / 2.;
    let ring: Vec<_> = [
        (0., 0.),
        (length, 0.),
        (length * 1.5, -h),
        (length, -h * 2.),
        (0., -h * 2.),
        (-length / 2., -h),
    ]
    .into_iter()
    .map(|(x, y)| d.add_atom("C", Point::new(x, y)))
    .collect();
    for (i, &a) in ring.iter().enumerate() {
        d.add_bond(
            a,
            ring[(i + 1) % 6],
            if i % 2 == 0 { 2 } else { 1 },
            "plain",
        );
    }
    assert_eq!(
        crate::ring_arcs::toggle(&mut d, &[ring[5], ring[0], ring[1], ring[2]]),
        Ok(true)
    );
    // Centred rails would leave the pointer nearer the single bond.
    d.bonds[1].double_position = crate::bonds::DoublePosition::Center;
    let ring_bond = Some(Pick::Bond {
        a: ring[0],
        b: ring[1],
        fraction: 0.5,
    });
    assert_eq!(pick(&d, Point::new(length / 2., 0.), 8. / 2.25), ring_bond);
    // A ring_arc flag left on a bond with no resolved arc keeps its rails.
    let spacing_half = spacing / 2.;
    let mut open = Document::default();
    let a = open.add_atom("C", Point::new(0., 0.));
    let b = open.add_atom("C", Point::new(length, 0.));
    open.add_bond(a, b, 2, "plain");
    open.bonds[0].ring_arc = true;
    assert!(pick(&open, Point::new(length / 2., spacing_half), 8. / 2.25).is_some());

    // A bold quadruple bond's outer ink stays a target with tight spacing.
    let mut quad = Document::default();
    quad.drawing_style.bond_spacing_ratio = 0.05;
    let a = quad.add_atom("C", Point::new(0., 0.));
    let b = quad.add_atom("C", Point::new(length, 0.));
    quad.add_bond(a, b, 6, "bold");
    let style = &quad.drawing_style;
    let edge = style.bond_length_world * style.bond_spacing_ratio * 1.5
        + DEFAULT.world(style.bold_width_pt) / 2.;
    assert!(pick(&quad, Point::new(length / 2., edge - 0.1), 8. / 2.25).is_some());

    // A crossed C=O crosses at the middle of its label-trimmed segment.
    let mut d = Document::default();
    let length = d.drawing_style.bond_length_world;
    let s = d.add_atom("C", Point::new(0., 1.2));
    let t = d.add_atom("C", Point::new(length, 1.2));
    d.add_bond(s, t, 1, "plain");
    let c = d.add_atom("C", Point::new(0., 0.));
    let o = d.add_atom("O", Point::new(length, 0.));
    d.add_bond(c, o, 2, "wavy");
    let (start, end) = crate::scene::bond_segment(&d, &d.bonds[1], None).unwrap();
    let cross = Point::new((start.x + end.x) / 2., (start.y + end.y) / 2.);
    assert!(matches!(
        pick(&d, cross, 2.),
        Some(Pick::Bond { a, b, .. }) if a == c && b == o
    ));
}
/// The #312 methoxide -> acetone arrow, C=O up, before any aim.
fn acetone_arrow() -> (Document, u64, u64) {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    for (degrees, symbol, order) in [(270f32, "O", 2), (150., "C", 1), (30., "C", 1)] {
        let angle = degrees.to_radians();
        let n = d.add_atom(symbol, Point::new(42. * angle.cos(), 42. * angle.sin()));
        d.add_bond(c, n, order, "plain");
    }
    let id = lone_pair_arrow_to(&mut d, c);
    (d, c, id)
}
fn arrow(d: &Document, id: u64) -> &Arrow {
    d.arrows.iter().find(|a| a.id == id).unwrap()
}
fn arrival(arrow: &Arrow) -> Point {
    unit(delta(arrow.cubic.unwrap()[1], arrow.end))
}
/// `d` after only [`Arrow::edit_handle`], as before #379.
fn bare_edit(d: &Document, id: u64, index: usize, p: Point) -> Document {
    let mut d = d.clone();
    d.arrows
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap()
        .edit_handle(index, p);
    d
}
/// The end tangent square dragged so the arrival points `degrees` from the end.
fn aim(d: &Document, id: u64, degrees: f32) -> Point {
    let (s, c) = degrees.to_radians().sin_cos();
    arrow(d, id).end.offset(20. * c, 20. * s)
}
#[test]
fn mechanism_attachment_379_aimed_end_head_clears_the_carbonyl() {
    let (d, c, id) = acetone_arrow();
    // The default now uses the upper-left opening. Aim down-left toward its
    // methyl bond to exercise an edit that still puts the head on bond ink.
    let p = aim(&d, id, 120.);
    let plain = bare_edit(&d, id, 4, p);
    assert!(
        rendered_head_ink_gap(&plain, arrow(&plain, id)) < 0.,
        "the bare tangent edit leaves the head on bond ink"
    );
    let mut aimed = d.clone();
    edit_handle(&mut aimed, id, 4, p);
    let a = arrow(&aimed, id);
    let gap = rendered_head_ink_gap(&aimed, a);
    assert!(gap > 0., "aimed arrowhead overlaps bond ink by {}", -gap);
    // The aim is kept, and the attachment and start are untouched.
    near(arrival(a), arrival(arrow(&plain, id)));
    assert_eq!(
        a.end_anchor.as_ref().unwrap().target,
        Target::Atom { atom: c }
    );
    assert_eq!(a.start_anchor, arrow(&d, id).start_anchor);
    assert_eq!((a.start, a.cubic.unwrap()[0]), {
        let b = arrow(&d, id);
        (b.start, b.cubic.unwrap()[0])
    });
    aimed.validate().unwrap();
    let once = aimed.clone();
    reconcile(&mut aimed);
    assert_eq!(aimed, once, "an aimed end is stable under reconciliation");
}
#[test]
fn mechanism_attachment_379_clear_aim_leaves_the_arrow_unchanged() {
    let (d, _, id) = acetone_arrow();
    let mut aimed = d.clone();
    let p = arrow(&d, id).cubic.unwrap()[1];
    edit_handle(&mut aimed, id, 4, p);
    let (a, b) = (arrow(&aimed, id), arrow(&d, id));
    assert_eq!((a.start, a.end, a.cubic), (b.start, b.end, b.cubic));
}
#[test]
fn mechanism_attachment_506_aim_along_a_bond_keeps_the_aim_and_clears_ink() {
    let (d, c, id) = acetone_arrow();
    // Straight down the C=O bond needs sideways clearance; the aim never turns.
    let mut aimed = d.clone();
    edit_handle(&mut aimed, id, 4, aim(&d, id, 270.));
    let a = arrow(&aimed, id);
    near(arrival(a), Point::new(0., -1.));
    let anchor = a.end_anchor.as_ref().unwrap();
    assert_eq!(anchor.target, Target::Atom { atom: c });
    assert!((anchor.gap - end_gap()).abs() < 0.0001);
    assert!(rendered_head_ink_gap(&aimed, a) > 0.);
    assert!(a.end.x.abs() > 0.1);
    let once = aimed.clone();
    reconcile(&mut aimed);
    assert_eq!(aimed, once);
}
#[test]
fn mechanism_attachment_379_repeated_aims_do_not_pile_up_gap() {
    let (mut d, _, id) = acetone_arrow();
    let center = Point::new(0., 0.);
    let mut reach = Vec::new();
    // Re-aim along the same rays, down the C=O bond and toward it, again
    // and again: the end must settle, not back away a cap per edit.
    for degrees in [270f32, 270., 235., 235., 270., 270.] {
        let p = aim(&d, id, degrees);
        edit_handle(&mut d, id, 4, p);
        let anchor = arrow(&d, id).end_anchor.as_ref().unwrap();
        assert!(anchor.gap <= 2. * end_gap() + 0.0001);
        reach.push(arrow(&d, id).end.distance(center));
    }
    assert!((reach[0] - reach[1]).abs() < 0.001, "{reach:?}");
    assert!((reach[2] - reach[3]).abs() < 0.001, "{reach:?}");
    assert!((reach[4] - reach[5]).abs() < 0.001, "{reach:?}");
    assert!((reach[0] - reach[4]).abs() < 0.001, "{reach:?}");
}
#[test]
fn mechanism_attachment_379_aimed_bond_end_keeps_target_aim_and_stability() {
    let mut d = Document::default();
    let c = d.add_atom("C", Point::new(0., 0.));
    let o = d.add_atom("O", Point::new(0., -42.));
    d.add_bond(c, o, 2, "plain");
    let source = d.add_atom("O", Point::new(-120., -21.));
    scientific::attach(
        d.atom_mut(source).unwrap(),
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .unwrap();
    let source = pick(&d, Point::new(-95., -21.), 2.).unwrap();
    let (a, b) = (c.min(o), c.max(o));
    let target = Pick::Bond {
        a,
        b,
        fraction: 0.5,
    };
    let style = ArrowStyle::preset(Preset::Curved);
    let id = create(&mut d, &source, &target, Preset::Curved, style).unwrap();
    let before = arrow(&d, id).end_anchor.clone().unwrap().target;
    for degrees in [150f32, 200., 260.] {
        let p = aim(&d, id, degrees);
        let mut aimed = d.clone();
        edit_handle(&mut aimed, id, 4, p);
        let (s, c) = degrees.to_radians().sin_cos();
        near(arrival(arrow(&aimed, id)), Point::new(c, s));
        assert_eq!(
            arrow(&aimed, id).end_anchor.as_ref().unwrap().target,
            before
        );
        let once = aimed.clone();
        reconcile(&mut aimed);
        assert_eq!(aimed, once);
    }
}
#[test]
fn mechanism_attachment_379_other_handles_do_not_reaim() {
    let (d, _, id) = acetone_arrow();
    let p = arrow(&d, id).cubic.unwrap()[0].offset(4., -13.);
    let mut edited = d.clone();
    edit_handle(&mut edited, id, 3, p);
    assert_eq!(edited, bare_edit(&d, id, 3, p));
}
#[test]
fn mechanism_attachment_379_an_aimed_end_depends_only_on_its_aim() {
    let (mut d, _, id) = acetone_arrow();
    let mut first = None;
    // Swinging the aim around and back must not walk the end away: the same
    // aim always lands on the same end.
    for degrees in [235f32, 300., 180., 235., 330., 150., 200., 260., 235.] {
        let p = aim(&d, id, degrees);
        edit_handle(&mut d, id, 4, p);
        let a = arrow(&d, id);
        if degrees == 235. {
            let end = (a.end, a.end_anchor.clone());
            let previous = first.get_or_insert(end.clone());
            near(previous.0, end.0);
            let (previous, current) = (previous.1.as_ref().unwrap(), end.1.as_ref().unwrap());
            assert_eq!(previous.target, current.target);
            near(previous.offset, current.offset);
            near(previous.direction, current.direction);
            assert!((previous.gap - current.gap).abs() < 0.0001);
        }
    }
}
#[test]
fn mechanism_attachment_379_aim_after_detaching_the_start_still_clears_bonds() {
    let (mut d, _, id) = acetone_arrow();
    // Dragging the Start handle detaches the start but keeps the end.
    d.arrows
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap()
        .start_anchor = None;
    let p = aim(&d, id, 235.);
    edit_handle(&mut d, id, 4, p);
    let a = arrow(&d, id);
    assert!(rendered_head_ink_gap(&d, a) > 0.);
}

#[test]
fn mechanism_attachment_506_carbonyl_aim_sweep_has_positive_rendered_clearance() {
    let (d, c, id) = acetone_arrow();
    for degrees in (235..=305).step_by(5) {
        let mut aimed = d.clone();
        edit_handle(&mut aimed, id, 4, aim(&d, id, degrees as f32));
        let a = arrow(&aimed, id);
        let gap = rendered_head_ink_gap(&aimed, a);
        assert!(gap > 0., "{degrees}°: rendered head overlaps by {}", -gap);
        assert_eq!(
            a.end_anchor.as_ref().unwrap().target,
            Target::Atom { atom: c }
        );
        let (s, c) = (degrees as f32).to_radians().sin_cos();
        near(arrival(a), Point::new(c, s));
        let once = aimed.clone();
        reconcile(&mut aimed);
        assert_eq!(aimed, once, "{degrees}° changes on reconciliation");
    }
}
