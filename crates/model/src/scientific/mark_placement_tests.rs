use super::*;
use crate::{document::Document, typography::TextStyle};

fn tool(kind: SymbolKind) -> Drawing {
    Drawing {
        kind: GraphicKind::Symbol(kind),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
        snap_orbitals: true,
    }
}
fn mark_bounds(doc: &Document, id: u64, index: usize) -> (Point, Point) {
    let mut atom = doc.atom(id).unwrap().clone();
    atom.marks = vec![atom.marks[index].clone()];
    mark_placement::bounds(&styled_mark_parts(&atom, &doc.drawing_style))
}
fn rectangle_gap((a, b): (Point, Point), (c, d): (Point, Point)) -> f32 {
    let dx = (c.x - b.x).max(a.x - d.x).max(0.);
    let dy = (c.y - b.y).max(a.y - d.y).max(0.);
    dx.hypot(dy)
}
fn click(doc: &mut Document, id: u64, kind: SymbolKind) {
    let at = doc.atom(id).unwrap().position;
    tool(kind).place(doc, at, at, false, 10.).unwrap();
}

#[test]
fn first_click_pair_is_closer_with_positive_visible_clearance_for_actual_fonts_and_sizes() {
    for family in ["Arial", "Times New Roman", "Courier New"] {
        for size_pt in [8., 10., 18., 30.] {
            for kind in [SymbolKind::LonePair, SymbolKind::LonePairBar] {
                let mut doc = Document::default();
                let id = doc.add_atom("O", p(0., 0.));
                let atom = doc.atom_mut(id).unwrap();
                atom.label_h = 1;
                atom.text_style = Some(TextStyle {
                    family: family.into(),
                    size_pt,
                    ..Default::default()
                });
                let chemistry = doc.atom(id).unwrap().clone();
                let labels = crate::scene::atom_label_ink_boxes(&chemistry, &doc);
                assert!(labels.len() >= 2, "OH must have visible ink");
                click(&mut doc, id, kind);
                let atom = doc.atom(id).unwrap();
                assert!(chemistry.marks.is_empty());
                let allocated_id = chemistry.mark_serial.checked_add(1).unwrap();
                assert_eq!(atom.marks.len(), 1);
                assert_eq!(atom.marks[0].id, Some(allocated_id));
                assert_eq!(atom.mark_serial, allocated_id);
                let radius = atom.marks[0].offset.distance(p(0., 0.));
                assert!(
                    radius < DEFAULT.world(size_pt),
                    "{family} {size_pt}: {radius}"
                );
                let bounds = mark_bounds(&doc, id, 0);
                for &label in &labels {
                    assert!(rectangle_gap(bounds, label) > DEFAULT.world(size_pt * 0.06));
                }
                let mut actual = atom.clone();
                actual.marks.clear();
                actual.mark_serial = chemistry.mark_serial;
                assert_eq!(actual, chemistry, "annotation does not change chemistry");
                assert_eq!(
                    Document::from_json(&doc.file_json().unwrap()).unwrap(),
                    doc.current()
                );
            }
        }
    }
}

#[test]
fn repeated_clicks_avoid_hydrogens_isotopes_charges_bonds_and_existing_marks() {
    for (element, hydrogens, charge, isotope) in [
        ("O", 1, 0, 18),
        ("N", 2, 0, 15),
        ("O", 2, 1, 18),
        ("N", 3, 1, 15),
    ] {
        for bond_style in ["plain", "wedge", "hash"] {
            let mut doc = Document::default();
            let c = doc.add_atom("C", p(0., -42.));
            let id = doc.add_atom(element, p(0., 0.));
            doc.add_bond(c, id, 1, bond_style);
            let atom = doc.atom_mut(id).unwrap();
            atom.label_h = hydrogens;
            atom.charge = charge;
            atom.isotope = isotope;
            attach(atom, SymbolKind::LonePair, p(-15., 0.)).unwrap();
            let old_mark = atom.marks[0].clone();
            let bonds = doc.bonds.clone();
            for i in 1..7 {
                let old_marks = doc.atom(id).unwrap().marks.clone();
                click(&mut doc, id, SymbolKind::LonePair);
                assert_eq!(&doc.atom(id).unwrap().marks[..i], &old_marks);
                let bounds = mark_bounds(&doc, id, i);
                for other in 0..i {
                    assert!(rectangle_gap(bounds, mark_bounds(&doc, id, other)) > 0.2);
                }
                for atom in &doc.atoms {
                    for label in crate::scene::atom_label_ink_boxes(atom, &doc) {
                        assert!(rectangle_gap(bounds, label) > 0.2);
                    }
                }
                // The top bond passes through x=0. A mark on its route must
                // be above its remote endpoint rather than on the bond ink.
                if bounds.0.x <= 0. && bounds.1.x >= 0. {
                    assert!(bounds.0.y > 0. || bounds.1.y < -42.);
                }
            }
            let atom = doc.atom(id).unwrap();
            assert_eq!(atom.marks[0], old_mark);
            assert_eq!(atom.charge, charge);
            assert_eq!(atom.label_h, hydrogens);
            assert_eq!(doc.bonds, bonds);
            doc.validate().unwrap();
        }
    }
}

#[test]
fn exact_dragged_and_saved_offsets_are_preserved_after_label_and_style_changes() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(10., 20.));
    let start = p(10., 20.);
    let end = p(18., 5.);
    tool(SymbolKind::LonePair)
        .place(&mut doc, start, end, true, 10.)
        .unwrap();
    assert_eq!(doc.atom(id).unwrap().marks[0].offset, p(8., -15.));
    let saved_mark = doc.atom(id).unwrap().marks[0].clone();
    doc.atom_mut(id).unwrap().label_h = 2;
    doc.drawing_style.font_size_pt = 18.;
    let _ = crate::scene::primitives(&doc);
    assert_eq!(doc.atom(id).unwrap().marks[0], saved_mark);
    click(&mut doc, id, SymbolKind::LonePairBar);
    assert_eq!(doc.atom(id).unwrap().marks[0], saved_mark);
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(
        reopened.atom(id).unwrap().marks,
        doc.atom(id).unwrap().marks
    );
}

#[test]
fn charge_and_radical_click_defaults_keep_their_existing_placement() {
    for kind in [SymbolKind::Plus, SymbolKind::Radical] {
        let mut doc = Document::default();
        let id = doc.add_atom("N", p(0., 0.));
        click(&mut doc, id, kind);
        assert!(
            doc.atom(id).unwrap().marks[0]
                .offset
                .distance(p(0., -DEFAULT.font_size()))
                < 0.001
        );
        assert_eq!(doc.atom(id).unwrap().marks[0].angle, 0.);
    }
}

#[test]
fn a_charge_drag_works_out_its_label_hydrogens_once_and_reuses_them() {
    // Methylamine's N: a plus leaves it NH3+, so the first preview fills the memo.
    let mut doc = Document::default();
    let c = doc.add_atom("C", p(-42., 0.));
    let n = doc.add_atom("N", p(0., 0.));
    doc.add_bond(c, n, 1, "plain");
    let hydrogens = OnceCell::new();
    let drag = |doc: &Document, end: Point, hydrogens: &OnceCell<Option<u32>>| {
        let mut preview = doc.clone();
        tool(SymbolKind::Plus)
            .place_with(&mut preview, p(0., 0.), end, false, 10., hydrogens)
            .unwrap();
        preview.atom(n).unwrap().label_h
    };
    assert_eq!(drag(&doc, p(10., -20.), &hydrogens), 3);
    assert_eq!(hydrogens.get(), Some(&Some(3)));
    // Later frames take the memo instead of recomputing (a stale value shows it).
    let stale = OnceCell::from(Some(7));
    assert_eq!(drag(&doc, p(-5., 20.), &stale), 7);
    // `place` itself still computes afresh.
    let mut committed = doc.clone();
    tool(SymbolKind::Plus)
        .place(&mut committed, p(0., 0.), p(-5., 20.), false, 10.)
        .unwrap();
    assert_eq!(committed.atom(n).unwrap().label_h, 3);
}

#[test]
fn a_hydrogen_above_or_beside_the_owner_does_not_pull_a_new_pair_away_from_the_heteroatom() {
    use crate::atom_labels::HydrogenPosition;
    for position in [
        HydrogenPosition::Above,
        HydrogenPosition::Below,
        HydrogenPosition::Left,
        HydrogenPosition::Right,
    ] {
        for kind in [SymbolKind::LonePair, SymbolKind::LonePairBar] {
            let mut doc = Document::default();
            let id = doc.add_atom("O", p(0., 0.));
            let atom = doc.atom_mut(id).unwrap();
            atom.label_h = 1;
            atom.display.hydrogen_position = position;
            click(&mut doc, id, kind);
            assert!(
                doc.atom(id).unwrap().marks[0].offset.distance(p(0., 0.))
                    < doc.drawing_style.font_size()
            );
            let bounds = mark_bounds(&doc, id, 0);
            for label in crate::scene::atom_label_ink_boxes(doc.atom(id).unwrap(), &doc) {
                assert!(rectangle_gap(bounds, label) > DEFAULT.world(0.6));
            }
        }
    }
}

#[test]
fn first_click_pairs_avoid_view_occluded_bond_ink_at_positive_rear_opacity() {
    use crate::scene::Primitive;

    for kind in [SymbolKind::LonePair, SymbolKind::LonePairBar] {
        let mut doc = Document::default();
        let owner = doc.add_atom("O", p(0., 0.));
        doc.atom_mut(owner).unwrap().depth = 40.;
        let mut clear = doc.clone();
        click(&mut clear, owner, kind);
        let preferred = clear.atom(owner).unwrap().marks[0].clone();
        let preferred_bounds = mark_bounds(&clear, owner, 0);
        assert_eq!(preferred.offset.x, 0.);
        assert!(
            preferred.offset.y < 0.,
            "The first preference must be above O"
        );

        // A closed tetrahedral cage supplies a real front face, with its
        // outline well clear of the O label and its preferred top pair.
        let cage: Vec<_> = [
            (-60., -50., 20.),
            (60., -50., 20.),
            (0., 70., 20.),
            (0., 20., -60.),
        ]
        .into_iter()
        .map(|(x, y, z)| {
            let id = doc.add_atom("C", p(x, y));
            doc.atom_mut(id).unwrap().depth = z;
            id
        })
        .collect();
        for (i, &a) in cage.iter().enumerate() {
            for &b in cage.iter().skip(i + 1) {
                doc.add_bond(a, b, 1, "plain");
            }
        }
        let rear_start = p(-18., preferred.offset.y);
        let rear_end = p(18., preferred.offset.y);
        let rear: Vec<_> = [rear_start, rear_end]
            .into_iter()
            .map(|position| {
                let id = doc.add_atom("C", position);
                doc.atom_mut(id).unwrap().depth = -20.;
                id
            })
            .collect();
        doc.add_bond(rear[0], rear[1], 1, "plain");
        let half_width = doc.drawing_style.line_width() * 0.5;
        let rear_bounds = (
            rear_start.offset(-half_width, -half_width),
            rear_end.offset(half_width, half_width),
        );
        assert_eq!(rectangle_gap(preferred_bounds, rear_bounds), 0.);

        let mut positive_placement = None;
        for alpha in [0., 0.25, 1.] {
            let mut drawing = doc.clone();
            crate::depth_appearance::set_rear_opacity(&mut drawing, &rear, alpha).unwrap();
            let parts = crate::scene::primitives(&drawing);
            if alpha == 0.25 {
                assert!(
                    parts.iter().any(|part| matches!(
                        part,
                        Primitive::Opacity { alpha: 0.25, primitive }
                            if matches!(primitive.as_ref(), Primitive::Line(a, b, _)
                                if *a == rear_start && *b == rear_end)
                    )),
                    "The relevant rear bond must emit actual 25% opacity ink"
                );
            }
            click(&mut drawing, owner, kind);
            let mark = &drawing.atom(owner).unwrap().marks[0];
            let placement = (mark.offset, mark.angle);
            if alpha == 0. {
                assert_eq!(placement, (preferred.offset, preferred.angle));
                assert_eq!(
                    rectangle_gap(mark_bounds(&drawing, owner, 0), rear_bounds),
                    0.
                );
            } else {
                assert_ne!(placement, (preferred.offset, preferred.angle));
                assert!(rectangle_gap(mark_bounds(&drawing, owner, 0), rear_bounds) > 0.2);
                if let Some(previous) = positive_placement {
                    assert_eq!(placement, previous);
                }
                positive_placement = Some(placement);
            }
            drawing.validate().unwrap();
        }
    }
}

fn degrees_apart(a: f32, b: f32) -> f32 {
    let turn = (a - b).rem_euclid(360.);
    turn.min(360. - turn)
}

/// Mark `index`'s ink points with their stroke half widths, and the owner's
/// label glyphs as (core box, radius): the rounded rectangles
/// `text_ink_shapes` fits to their outlines.
#[allow(clippy::type_complexity)]
fn mark_and_label(
    doc: &Document,
    id: u64,
    index: usize,
) -> (Vec<(Point, f32)>, Vec<((Point, Point), f32)>) {
    let mut atom = doc.atom(id).unwrap().clone();
    let shapes = crate::scene::atom_label_ink_shapes(&atom, doc)
        .into_iter()
        .map(|((lo, hi), radius)| {
            (
                (lo.offset(radius, radius), hi.offset(-radius, -radius)),
                radius,
            )
        })
        .collect();
    atom.marks = vec![atom.marks[index].clone()];
    let ink = styled_mark_parts(&atom, &doc.drawing_style)
        .iter()
        .flat_map(|part| {
            let pad = part.style.width() * 0.5;
            crate::graphics::flattened(&part.commands)
                .into_iter()
                .flatten()
                .map(move |q| (q, pad))
        })
        .collect();
    (ink, shapes)
}

/// Unit directions all the way round, for support-function distances.
fn normals() -> impl Iterator<Item = (f32, f32)> {
    (0..1440).map(|i| (i as f32 * 0.25).to_radians().sin_cos())
}

/// How far the label's outline (the convex hull of its rounded glyphs, so OH
/// reads as one rounded shape) reaches along (c, s).
fn label_reach(shapes: &[((Point, Point), f32)], (s, c): (f32, f32)) -> f32 {
    shapes
        .iter()
        .map(|&((lo, hi), radius)| (lo.x * c).max(hi.x * c) + (lo.y * s).max(hi.y * s) + radius)
        .fold(f32::NEG_INFINITY, f32::max)
}

/// The gap from mark `index`'s nearest ink to its owner's label outline.
fn ink_gap(doc: &Document, id: u64, index: usize) -> f32 {
    let (ink, shapes) = mark_and_label(doc, id, index);
    let reach: Vec<_> = normals().map(|n| (n, label_reach(&shapes, n))).collect();
    ink.iter()
        .map(|&(q, pad)| {
            reach
                .iter()
                .map(|&((s, c), h)| q.x * c + q.y * s - h)
                .fold(f32::NEG_INFINITY, f32::max)
                - pad
        })
        .fold(f32::INFINITY, f32::min)
}

/// The gap from the outline of mark `index` (the convex hull of its ink, so a
/// lone pair counts as one mark) to its owner's label outline.
fn outline_gap(doc: &Document, id: u64, index: usize) -> f32 {
    let (ink, shapes) = mark_and_label(doc, id, index);
    let pad = ink.iter().map(|&(_, pad)| pad).fold(0., f32::max);
    normals()
        .map(|(s, c)| {
            let near = ink
                .iter()
                .map(|&(q, _)| q.x * c + q.y * s)
                .fold(f32::INFINITY, f32::min);
            near - label_reach(&shapes, (s, c))
        })
        .fold(f32::NEG_INFINITY, f32::max)
        - pad
}

#[test]
fn dragged_marks_orbit_every_snap_direction_hugging_the_label() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(0., 0.));
    doc.atom_mut(id).unwrap().label_h = 1;
    click(&mut doc, id, SymbolKind::LonePair);
    let clicked = doc.atom(id).unwrap().marks[0].clone();
    assert_eq!(clicked.offset.x, 0.);
    for kind in [SymbolKind::LonePair, SymbolKind::CirclePlus] {
        let mut doc = doc.clone();
        if kind == SymbolKind::CirclePlus {
            let atom = doc.atom_mut(id).unwrap();
            atom.marks.clear();
            attach(atom, kind, p(0., -10.)).unwrap();
        }
        // A charge clears the label hydrogens, leaving only O to orbit.
        let labels = crate::scene::atom_label_ink_boxes(doc.atom(id).unwrap(), &doc);
        assert_eq!(
            labels.len(),
            if kind == SymbolKind::LonePair { 2 } else { 1 }
        );
        for step in 0..24 {
            let snapped = step as f32 * MARK_ORBIT_STEP;
            // The pointer may be off the snap direction and at any distance.
            let pointer = (snapped + 6.).to_radians();
            let to = p(40. * pointer.cos(), 40. * pointer.sin());
            let atom = doc.atom(id).unwrap();
            let mark = orbit_mark(&doc, atom, &atom.marks[0], to, false);
            let direction = mark.offset.y.atan2(mark.offset.x).to_degrees();
            assert!(degrees_apart(direction, snapped) < 1e-3, "{direction}");
            if kind == SymbolKind::LonePair {
                assert!(degrees_apart(mark.angle, snapped + 90.) < 1e-3);
                if step == 18 {
                    // Straight up is where a click puts the pair.
                    assert!(mark.offset.distance(clicked.offset) < 1e-3);
                    assert!(degrees_apart(mark.angle, clicked.angle) < 1e-3);
                }
            } else {
                assert_eq!(mark.angle, 0.);
            }
            let mut placed = doc.clone();
            placed.atom_mut(id).unwrap().marks[0] = mark.clone();
            // The mark's ink clears the label (O and H) and hugs it.
            let nearest = ink_gap(&placed, id, 0);
            assert!(
                nearest > DEFAULT.world(10. * 0.07) * 0.99,
                "{kind} at {snapped}°: {nearest}"
            );
            assert!(
                nearest < DEFAULT.world(10. * 0.3),
                "{kind} at {snapped}°: {nearest}"
            );
        }
    }
}

#[test]
fn orbits_settle_and_keep_the_gap_from_label_corners() {
    let gap = DEFAULT.world(10. * 0.07) * 0.99;
    for (element, label_h) in [("O", Some(1)), ("N", None)] {
        for kind in [SymbolKind::LonePairBar, SymbolKind::LonePair] {
            let mut doc = Document::default();
            let id = doc.add_atom(element, p(0., 0.));
            let atom = doc.atom_mut(id).unwrap();
            match label_h {
                Some(h) => atom.label_h = h,
                None => atom.no_implicit = true,
            }
            click(&mut doc, id, kind);
            let atom = doc.atom(id).unwrap();
            // OH with a bar at 324° used to loop forever on a rounding re-entry.
            let directions = (0..120).map(|i| i as f32 * 3.);
            for (i, (mark, _)) in mark_placement::orbit(&doc, atom, &atom.marks[0], directions)
                .into_iter()
                .enumerate()
            {
                let mut placed = doc.clone();
                placed.atom_mut(id).unwrap().marks[0] = mark;
                // N's stems reach its box corners, so diagonals must clear them.
                let clear = ink_gap(&placed, id, 0);
                assert!(clear > gap, "{element} {kind} at {}°: {clear}", i * 3);
            }
        }
    }
}

#[test]
fn free_drags_keep_the_offset_and_turn_only_lone_pairs() {
    let mut doc = Document::default();
    let id = doc.add_atom("N", p(0., 0.));
    let atom = doc.atom_mut(id).unwrap();
    attach(atom, SymbolKind::LonePair, p(0., -10.)).unwrap();
    attach(atom, SymbolKind::Plus, p(10., -10.)).unwrap();
    let atom = doc.atom(id).unwrap();
    let to = p(-17., 9.);
    let pair = orbit_mark(&doc, atom, &atom.marks[0], to, true);
    assert_eq!(pair.offset, to);
    assert!(degrees_apart(pair.angle, 9_f32.atan2(-17.).to_degrees() + 90.) < 1e-3);
    let charge = orbit_mark(&doc, atom, &atom.marks[1], to, true);
    assert_eq!((charge.offset, charge.angle), (to, 0.));
}

#[test]
fn symbol_tool_drags_orbit_unless_freed() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(0., 0.));
    let to = p(30., -14.);
    let mut orbited = doc.clone();
    tool(SymbolKind::LonePair)
        .place(&mut orbited, p(0., 0.), to, false, 10.)
        .unwrap();
    let mark = &orbited.atom(id).unwrap().marks[0];
    let direction = mark.offset.y.atan2(mark.offset.x).to_degrees();
    assert!(degrees_apart(direction, -30.) < 1e-3, "{direction}");
    assert!(degrees_apart(mark.angle, 60.) < 1e-3);
    tool(SymbolKind::LonePair)
        .place(&mut doc, p(0., 0.), to, true, 10.)
        .unwrap();
    assert_eq!(doc.atom(id).unwrap().marks[0].offset, to);
}

#[test]
fn rotating_and_flipping_turn_lone_pairs_while_charges_stay_upright() {
    use crate::editing::{Transform, transform, transform_about};
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(0., 0.));
    let atom = doc.atom_mut(id).unwrap();
    attach(atom, SymbolKind::LonePair, p(0., -10.)).unwrap();
    attach(atom, SymbolKind::CircleMinus, p(10., -10.)).unwrap();
    let angles = |doc: &Document| {
        let atom = doc.atom(id).unwrap();
        (atom.marks[0].angle, atom.marks[1].angle)
    };
    transform(&mut doc, &[id], Transform::Rotate(30.));
    assert!(degrees_apart(angles(&doc).0, 30.) < 1e-3);
    assert_eq!(angles(&doc).1, 0.);
    transform_about(&mut doc, &[id], p(5., 5.), 1., 15.);
    assert!(degrees_apart(angles(&doc).0, 45.) < 1e-3);
    transform(&mut doc, &[id], Transform::FlipHorizontal);
    assert!(degrees_apart(angles(&doc).0, 135.) < 1e-3);
    assert_eq!(angles(&doc).1, 0.);
}

#[test]
fn unequal_stretching_keeps_lone_pairs_across_their_atom_direction() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(0., 0.));
    let atom = doc.atom_mut(id).unwrap();
    attach(atom, SymbolKind::LonePair, p(10., 10.)).unwrap();
    atom.marks[0].angle = 135.;
    crate::editing::scale_axes_about(&mut doc, &[id], p(0., 0.), 2., 1.);
    let mark = &doc.atom(id).unwrap().marks[0];
    assert_eq!(mark.offset, p(20., 10.));
    let direction = 10_f32.atan2(20.).to_degrees();
    assert!(
        degrees_apart(mark.angle, direction + 90.) < 1e-3,
        "{}",
        mark.angle
    );
}

#[test]
fn hidden_charges_keep_their_offset_when_dragged() {
    let mut doc = Document::default();
    let id = doc.add_atom("N", p(0., 0.));
    let atom = doc.atom_mut(id).unwrap();
    attach(atom, SymbolKind::Plus, p(10., -10.)).unwrap();
    assert_ne!(atom.charge, 0);
    atom.display.hide_charge = true;
    let atom = doc.atom(id).unwrap();
    let mark = &atom.marks[0];
    assert!(!mark_drawn(atom, mark));
    for free in [false, true] {
        assert_eq!(orbit_mark(&doc, atom, mark, p(-30., 5.), free), *mark);
    }
    assert_eq!(mark_orbit_guide(&doc, atom, mark), (vec![], vec![]));
}

#[test]
fn round_glyphs_round_their_corners_and_stemmed_ones_keep_them() {
    for (element, round) in [("O", true), ("N", false), ("H", false)] {
        let mut doc = Document::default();
        let id = doc.add_atom(element, p(0., 0.));
        doc.atom_mut(id).unwrap().no_implicit = true;
        let shapes = crate::scene::atom_label_ink_shapes(doc.atom(id).unwrap(), &doc);
        let &((lo, hi), radius) = shapes.first().unwrap();
        let half = 0.5 * (hi.x - lo.x).min(hi.y - lo.y);
        assert_eq!(radius > half * 0.6, round, "{element}: {radius} of {half}");
    }
}

#[test]
fn the_orbit_gap_to_the_label_ink_stays_even_all_the_way_round() {
    for (label_h, kind) in [
        (None, SymbolKind::CircleMinus),
        (None, SymbolKind::LonePair),
        (Some(("O", 1)), SymbolKind::LonePair),
        (Some(("O", 1)), SymbolKind::LonePairBar),
        (Some(("N", 2)), SymbolKind::LonePair),
        (Some(("N", 2)), SymbolKind::Radical),
    ] {
        let (element, hydrogens) = label_h.unwrap_or(("O", 0));
        let mut doc = Document::default();
        let id = doc.add_atom(element, p(0., 0.));
        let atom = doc.atom_mut(id).unwrap();
        atom.label_h = hydrogens;
        atom.no_implicit = hydrogens == 0;
        attach(atom, kind, p(0., -20.)).unwrap();
        let atom = doc.atom(id).unwrap();
        let gaps: Vec<_> = (0..24)
            .map(|step| {
                let snapped = (step as f32 * MARK_ORBIT_STEP).to_radians();
                let to = p(40. * snapped.cos(), 40. * snapped.sin());
                let mut placed = doc.clone();
                placed.atom_mut(id).unwrap().marks[0] =
                    orbit_mark(&doc, atom, &atom.marks[0], to, false);
                outline_gap(&placed, id, 0)
            })
            .collect();
        let (low, high) = gaps
            .iter()
            .fold((f32::INFINITY, 0_f32), |(l, h), &g| (l.min(g), h.max(g)));
        assert!(high <= low * 1.15, "{element}H{hydrogens} {kind}: {gaps:?}");
    }
}

/// Methanol's O (as OH) with one mark of `kind` above it; C sits at 180°.
fn methanol_o(kind: SymbolKind) -> (Document, u64) {
    let mut doc = Document::default();
    let c = doc.add_atom("C", p(-42., 0.));
    let o = doc.add_atom("O", p(0., 0.));
    doc.add_bond(c, o, 1, "plain");
    doc.atom_mut(o).unwrap().label_h = 1;
    attach(doc.atom_mut(o).unwrap(), kind, p(0., -20.)).unwrap();
    (doc, o)
}

#[test]
fn orbit_guides_are_smooth_and_carry_the_committed_snaps() {
    for (element, hydrogens) in [("O", 0), ("O", 1), ("N", 2)] {
        for kind in [SymbolKind::LonePair, SymbolKind::CircleMinus] {
            let mut doc = Document::default();
            let id = doc.add_atom(element, p(0., 0.));
            let atom = doc.atom_mut(id).unwrap();
            atom.label_h = hydrogens;
            atom.no_implicit = hydrogens == 0;
            attach(atom, kind, p(0., -20.)).unwrap();
            let atom = doc.atom(id).unwrap();
            let (runs, snaps) = mark_orbit_guide(&doc, atom, &atom.marks[0]);
            assert_eq!((runs.len(), snaps.len()), (1, 24));
            let run = &runs[0];
            // Each dot is the position a drag there commits, on the guide.
            for (step, snap) in snaps.iter().enumerate() {
                let direction = (step as f32 * MARK_ORBIT_STEP).to_radians();
                let to = p(40. * direction.cos(), 40. * direction.sin());
                let mark = orbit_mark(&doc, atom, &atom.marks[0], to, false);
                assert!(snap.distance(mark.offset) < 1e-4);
                assert!(run.iter().any(|q| q.distance(*snap) < 1e-4));
            }
            // No zig-zag: no sample strays from the line through its neighbors.
            let worst = run
                .windows(3)
                .map(|w| {
                    let (dx, dy) = (w[2].x - w[0].x, w[2].y - w[0].y);
                    ((w[1].x - w[0].x) * dy - (w[1].y - w[0].y) * dx).abs() / dx.hypot(dy)
                })
                .fold(0., f32::max);
            assert!(worst < 0.4, "{element}H{hydrogens} {kind}: strays {worst}");
        }
    }
}

#[test]
fn orbit_steps_that_would_touch_a_bond_are_skipped() {
    let style = &DEFAULT;
    let need = DEFAULT.world(10. * 0.07) * 0.99 + style.world(style.line_width_pt) * 0.5;
    for kind in [SymbolKind::LonePair, SymbolKind::CircleMinus] {
        let (doc, o) = methanol_o(kind);
        let atom = doc.atom(o).unwrap();
        let bond = crate::scene::bond_segment(&doc, &doc.bonds[0], None).unwrap();
        let (_, snaps) = mark_orbit_guide(&doc, atom, &atom.marks[0]);
        assert!(snaps.len() < 24, "{kind}: no step was blocked");
        let mut directions = Vec::new();
        for step in 0..24 {
            let pointer = (step as f32 * MARK_ORBIT_STEP + 4.).to_radians();
            let to = p(40. * pointer.cos(), 40. * pointer.sin());
            let mark = orbit_mark(&doc, atom, &atom.marks[0], to, false);
            let mut placed = doc.clone();
            placed.atom_mut(o).unwrap().marks[0] = mark.clone();
            let (ink, _) = mark_and_label(&placed, o, 0);
            let clear = ink
                .iter()
                .map(|&(q, pad)| {
                    // Sample the bond finely; its ends are trimmed at the label.
                    (0..=200)
                        .map(|i| {
                            let t = i as f32 / 200.;
                            q.distance(p(
                                bond.0.x + (bond.1.x - bond.0.x) * t,
                                bond.0.y + (bond.1.y - bond.0.y) * t,
                            ))
                        })
                        .fold(f32::INFINITY, f32::min)
                        - pad
                })
                .fold(f32::INFINITY, f32::min);
            let direction = mark.offset.y.atan2(mark.offset.x).to_degrees();
            assert!(clear >= need, "{kind} at {direction}°: {clear}");
            assert!(snaps.iter().any(|s| s.distance(mark.offset) < 1e-4));
            directions.push(direction.rem_euclid(360.).round() as i32);
        }
        // Toward C, the drag lands on the nearest clear step instead.
        assert!(!directions.contains(&180), "{kind}: {directions:?}");
        // A free drag still goes exactly where it is put.
        let free = orbit_mark(&doc, atom, &atom.marks[0], p(-30., 0.), true);
        assert_eq!(free.offset, p(-30., 0.));
    }
}

#[test]
fn dragged_charges_and_radicals_orbit_the_label_they_will_have() {
    // Methylamine's N reads NH2 with the hydrogens to the right, away from C.
    for (kind, hydrogens) in [(SymbolKind::Radical, 1), (SymbolKind::Plus, 3)] {
        let mut doc = Document::default();
        let c = doc.add_atom("C", p(-42., 0.));
        let n = doc.add_atom("N", p(0., 0.));
        doc.add_bond(c, n, 1, "plain");
        crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default())
            .unwrap()
            .apply(&mut doc);
        assert_eq!(doc.atom(n).unwrap().label_h, 2);
        // Attaching clears the hydrogens; the drag to the right must still
        // orbit the label the refresh brings back.
        tool(kind)
            .place(&mut doc, p(0., 0.), p(40., 0.), false, 10.)
            .unwrap();
        crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default())
            .unwrap()
            .apply(&mut doc);
        assert_eq!(doc.atom(n).unwrap().label_h, hydrogens);
        let gap = ink_gap(&doc, n, 0);
        assert!(gap > DEFAULT.world(10. * 0.07) * 0.99, "{kind}: {gap}");
        assert!(gap < DEFAULT.world(10. * 0.3), "{kind}: {gap}");
    }
}

#[test]
fn methylamine_circled_plus_retains_the_clear_upward_default() {
    let mut doc = Document::default();
    let c = doc.add_atom("C", p(-42., 0.));
    let n = doc.add_atom("N", p(0., 0.));
    doc.add_bond(c, n, 1, "plain");
    click(&mut doc, n, SymbolKind::CirclePlus);
    let atom = doc.atom(n).unwrap();
    assert_eq!(atom.label_h, 3);
    assert!(atom.marks[0].offset.distance(p(0., -DEFAULT.font_size())) < 0.001);
}
