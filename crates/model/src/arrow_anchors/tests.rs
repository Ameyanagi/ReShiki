use super::*;
use crate::{
    document::History,
    editing,
    scientific::{self, SymbolKind},
    transaction,
};
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
    assert_eq!(saved.version, 21);
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
