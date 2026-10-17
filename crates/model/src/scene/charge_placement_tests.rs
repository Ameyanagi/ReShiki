use super::*;
use crate::{
    atom_labels::HydrogenPosition,
    scientific::{AtomMark, MarkKind},
    typography::TextStyle,
};

fn chelate(two_contacts: bool) -> Document {
    serde_json::from_str(if two_contacts {
        include_str!(
            "../../../../tests/fixtures/coordination/windows-native-20261010/co-en-windows-chelate-after.rsk"
        )
    } else {
        include_str!("../../../../tests/fixtures/coordination/join-shortcut/co-en3-cmd-j-native.rsk")
    })
    .unwrap()
}

fn charge_run(doc: &Document, atom: u64, text: &str) -> Point {
    atom_label_runs(doc.atom(atom).unwrap(), doc)
        .into_iter()
        .find_map(|run| match run {
            Primitive::Text {
                position,
                text: content,
                ..
            } if content == text => Some(position),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing {text} on atom {atom}"))
}

/// Inspect actual shaft/head primitives, their donor direction, and the ink
/// they approach. Counting graph contacts alone would miss the original bug.
fn assert_contacts_painted(doc: &Document, expected: usize) {
    let ink: Vec<_> = doc
        .atoms
        .iter()
        .flat_map(|a| atom_label_ink_boxes(a, doc))
        .collect();
    let rendered = primitives(doc);
    let heads: Vec<_> = rendered
        .iter()
        .filter_map(|p| match p {
            Primitive::Polygon(points) if points.len() == 3 => Some(points),
            _ => None,
        })
        .collect();
    assert_eq!(heads.len(), expected, "Actual directed arrowhead count");
    for bond in doc.bonds.iter().filter(|b| b.order == 5) {
        let donor = doc.atom(bond.a).unwrap().position;
        let metal = doc.atom(bond.b).unwrap().position;
        let length = donor.distance(metal);
        let ux = (metal.x - donor.x) / length;
        let uy = (metal.y - donor.y) / length;
        let along = |p: Point| (p.x - donor.x) * ux + (p.y - donor.y) * uy;
        let across = |p: Point| (p.x - donor.x) * uy - (p.y - donor.y) * ux;
        let head = heads
            .iter()
            .find(|h| {
                let base = Point::new((h[1].x + h[2].x) * 0.5, (h[1].y + h[2].y) * 0.5);
                across(h[0]).abs() < 0.002
                    && along(h[0]) > along(base) + 9.9
                    && (0. ..length).contains(&along(h[0]))
            })
            .unwrap_or_else(|| panic!("Missing donor {} → metal {} head", bond.a, bond.b));
        let (start, end, width) = rendered
            .iter()
            .find_map(|p| match p {
                Primitive::Line(start, end, width)
                    if end.distance(head[0]) < 0.002 && across(*start).abs() < 0.002 =>
                {
                    Some((*start, *end, *width))
                }
                _ => None,
            })
            .expect("Arrowhead must have its actual incident shaft");
        assert!(
            along(start) < along(end),
            "Shaft still points donor → metal"
        );
        for step in 0..=100 {
            let t = step as f32 / 100.;
            let p = start.offset((end.x - start.x) * t, (end.y - start.y) * t);
            for &(lo, hi) in &ink {
                assert!(
                    p.x + width / 2. <= lo.x
                        || p.x - width / 2. >= hi.x
                        || p.y + width / 2. <= lo.y
                        || p.y - width / 2. >= hi.y,
                    "{} → {} shaft crosses glyph ink at {p:?}",
                    bond.a,
                    bond.b
                );
            }
        }
    }
}

fn assert_native_contact_fixture(two: bool) {
    let doc = chelate(two);
    let native = serde_json::to_vec(&doc).unwrap();
    assert_contacts_painted(&doc, if two { 2 } else { 6 });
    assert_eq!(doc.atom(1).unwrap().charge, 3);
    charge_run(&doc, 1, "3+");
    let svg = svg(&doc);
    let tree = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        tree.descendants()
            .filter(|n| n.has_tag_name("polygon"))
            .count(),
        if two { 2 } else { 6 }
    );
    assert!(svg.contains(">3+</text>"), "Figure export retains +3");
    assert_eq!(serde_json::to_vec(&doc).unwrap(), native);
}

#[test]
fn actual_two_contact_chelate_keeps_charge_and_all_directed_arrows() {
    assert_native_contact_fixture(true);
}

#[test]
#[ignore = "TODO #246: Linux renders only five native Co(en)3 arrows"]
fn native_six_contact_chelate_keeps_charge_and_all_directed_arrows() {
    assert_native_contact_fixture(false);
}

fn assert_contact_font_and_transform_cases(two: bool) {
    for size in [8., 10., 18.] {
        for bold in [false, true] {
            for reflected in [false, true] {
                for degrees in (0..360).step_by(30) {
                    let mut doc = chelate(two);
                    for atom in &mut doc.atoms {
                        if reflected {
                            atom.position.x = -atom.position.x;
                        }
                    }
                    let ids = doc.all_ids();
                    // Preserve the physical label/bond ratio for large
                    // fonts; truly label-covered short bonds stay hidden.
                    crate::editing::transform_about(
                        &mut doc,
                        &ids,
                        Point::default(),
                        (size / 10_f32).max(1.),
                        degrees as f32,
                    );
                    for atom in &mut doc.atoms {
                        atom.text_style = Some(TextStyle {
                            size_pt: size,
                            bold,
                            ..Default::default()
                        });
                    }
                    assert_contacts_painted(&doc, if two { 2 } else { 6 });
                    charge_run(&doc, 1, "3+");
                }
            }
        }
    }
}

#[test]
fn two_contact_charge_placement_survives_rotation_reflection_and_font_styles() {
    assert_contact_font_and_transform_cases(true);
}

#[test]
#[ignore = "TODO #246: Linux renders only five native Co(en)3 arrows"]
fn native_six_contact_charge_placement_survives_rotation_reflection_and_font_styles() {
    assert_contact_font_and_transform_cases(false);
}

#[test]
fn clear_automatic_charges_keep_the_exact_existing_layout() {
    for charge in [-3, -1, 1, 3] {
        let mut doc = Document::default();
        let n = doc.add_atom("N", Point::default());
        let c = doc.add_atom("C", Point::new(-90., 0.));
        doc.add_bond(n, c, 1, "plain");
        let atom = doc.atom_mut(n).unwrap();
        atom.charge = charge;
        atom.explicit_h = 2;
        atom.no_implicit = true;
        atom.isotope = 15;
        atom.display.hydrogen_position = HydrogenPosition::Right;
        atom.radical_electrons = 1;
        assert_eq!(
            format!("{:?}", atom_label_runs(doc.atom(n).unwrap(), &doc)),
            format!(
                "{:?}",
                atom_label_runs_with_charge_layout(
                    doc.atom(n).unwrap(),
                    &doc,
                    ChargeLayout::Unplaced
                )
            ),
            "Clear {charge} charge/H/isotope/radical layout changed"
        );
    }
}

#[test]
fn moved_charge_and_automatic_radical_remain_one_superscript_cluster() {
    let mut doc = chelate(true);
    doc.atom_mut(1).unwrap().radical_electrons = 1;
    let old =
        atom_label_runs_with_charge_layout(doc.atom(1).unwrap(), &doc, ChargeLayout::Unplaced);
    let old_position = |label: &str| {
        old.iter()
            .find_map(|run| match run {
                Primitive::Text { position, text, .. } if text == label => Some(*position),
                _ => None,
            })
            .unwrap()
    };
    let charge = charge_run(&doc, 1, "3+");
    let radical = charge_run(&doc, 1, "•");
    let old_charge = old_position("3+");
    let old_radical = old_position("•");
    assert_ne!(charge, old_charge, "Obstructing charge should move");
    assert!((charge.x - old_charge.x - radical.x + old_radical.x).abs() < 0.002);
    assert!((charge.y - old_charge.y - radical.y + old_radical.y).abs() < 0.002);
    assert_contacts_painted(&doc, 2);
}

#[test]
fn hidden_and_explicit_charge_radical_marks_keep_existing_placements() {
    for kind in [
        MarkKind::Charge,
        MarkKind::CircledCharge,
        MarkKind::RadicalIon,
    ] {
        let mut doc = chelate(true);
        let atom = doc.atom_mut(1).unwrap();
        atom.radical_electrons = 1;
        atom.marks.push(AtomMark {
            id: None,
            kind,
            offset: Point::new(-42., -28.),
            angle: 25.,
            size_pt: Some(9.),
        });
        let native = serde_json::to_vec(&doc).unwrap();
        assert_eq!(
            format!("{:?}", atom_label_runs(doc.atom(1).unwrap(), &doc)),
            format!(
                "{:?}",
                atom_label_runs_with_charge_layout(
                    doc.atom(1).unwrap(),
                    &doc,
                    ChargeLayout::Unplaced
                )
            )
        );
        primitives(&doc);
        assert_eq!(serde_json::to_vec(&doc).unwrap(), native);
    }
    let mut doc = chelate(true);
    doc.atom_mut(1).unwrap().display.hide_charge = true;
    assert_contacts_painted(&doc, 2);
    assert_eq!(doc.atom(1).unwrap().charge, 3);
    assert!(
        atom_label_runs(doc.atom(1).unwrap(), &doc)
            .iter()
            .all(|p| { !matches!(p, Primitive::Text { text, .. } if text == "3+") })
    );
}

#[test]
fn explicit_radical_keeps_its_user_offset_when_automatic_charge_moves() {
    let mut doc = chelate(true);
    let atom = doc.atom_mut(1).unwrap();
    atom.radical_electrons = 1;
    atom.marks.push(AtomMark {
        id: None,
        kind: MarkKind::Radical,
        offset: Point::new(-42., -28.),
        angle: 25.,
        size_pt: Some(9.),
    });
    let native = serde_json::to_vec(&doc).unwrap();
    let atom = doc.atom(1).unwrap();
    let expected = crate::scientific::styled_mark_parts(atom, &doc.drawing_style);
    let old_charge = atom_label_runs_with_charge_layout(atom, &doc, ChargeLayout::Unplaced)
        .into_iter()
        .find_map(|run| match run {
            Primitive::Text { position, text, .. } if text == "3+" => Some(position),
            _ => None,
        })
        .unwrap();
    assert_ne!(charge_run(&doc, 1, "3+"), old_charge);
    let rendered = primitives(&doc);
    for part in expected {
        assert!(rendered.iter().any(|p| {
            matches!(p, Primitive::Path { commands, style, filled }
                if *commands == part.commands && *style == part.style && *filled == part.filled)
        }));
    }
    assert!(
        atom_label_runs(atom, &doc)
            .iter()
            .all(|p| { !matches!(p, Primitive::Text { text, .. } if text == "•") })
    );
    assert_eq!(serde_json::to_vec(&doc).unwrap(), native);
    assert_contacts_painted(&doc, 2);
}

#[test]
fn core_covered_neighbors_still_reserve_their_visible_manual_marks() {
    let mut doc = chelate(true);
    let hydrogens: Vec<_> = doc.atoms.iter().map(|a| (a.id, a.label_h)).collect();
    let near = doc.add_atom("N", Point::new(1., 0.));
    doc.add_bond(1, near, 1, "plain");
    // Adding the test-only covered bond invalidates cached H counts; retain
    // the exact original chelate label geometry for this rendering check.
    for (id, label_h) in hydrogens {
        doc.atom_mut(id).unwrap().label_h = label_h;
    }
    let preferred = charge_run(&doc, 1, "3+");
    let charge_ink: Vec<_> = atom_label_runs(doc.atom(1).unwrap(), &doc)
        .into_iter()
        .filter(|p| matches!(p, Primitive::Text { text, .. } if text == "3+"))
        .collect();
    let (lo, hi) = label_ink_boxes(&charge_ink)[0];
    let blocked = Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    let neighbor = doc.atom_mut(near).unwrap();
    neighbor.radical_electrons = 1;
    neighbor.marks.push(AtomMark {
        id: None,
        kind: MarkKind::Radical,
        offset: Point::new(
            blocked.x - neighbor.position.x,
            blocked.y - neighbor.position.y,
        ),
        angle: 0.,
        size_pt: Some(4.),
    });
    let native = serde_json::to_vec(&doc).unwrap();
    let parts = crate::scientific::styled_mark_parts(doc.atom(near).unwrap(), &doc.drawing_style);
    assert_eq!(parts.len(), 1, "The obstacle is an actual painted radical");
    assert_ne!(charge_run(&doc, 1, "3+"), preferred);
    let charge_ink: Vec<_> = atom_label_runs(doc.atom(1).unwrap(), &doc)
        .into_iter()
        .filter(|p| matches!(p, Primitive::Text { text, .. } if text == "3+"))
        .collect();
    let (mark_lo, mark_hi) = parts[0]
        .commands
        .iter()
        .flat_map(crate::graphics::PathCommand::iter_points)
        .map(|p| (p, p))
        .reduce(|(lo, hi), (a, b)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        })
        .unwrap();
    let pad = parts[0].style.width() * 0.5;
    for (lo, hi) in label_ink_boxes(&charge_ink) {
        assert!(
            hi.x <= mark_lo.x - pad
                || lo.x >= mark_hi.x + pad
                || hi.y <= mark_lo.y - pad
                || lo.y >= mark_hi.y + pad,
            "Charge still covers the fixed manual radical"
        );
    }
    let rendered = primitives(&doc);
    assert!(rendered.iter().any(|p| {
        matches!(p, Primitive::Path { commands, .. } if *commands == parts[0].commands)
    }));
    assert_contacts_painted(&doc, 2);
    assert_eq!(serde_json::to_vec(&doc).unwrap(), native);
}

#[test]
fn jointly_covered_endpoint_cores_do_not_move_charge_or_force_an_arrow() {
    let mut doc = chelate(true);
    let donor = doc.atom_mut(2).unwrap();
    donor.position.x *= 0.5;
    donor.position.y *= 0.5;
    let metal = doc.atom(1).unwrap();
    let donor = doc.atom(2).unwrap();
    let length = metal.position.distance(donor.position);
    let ux = (donor.position.x - metal.position.x) / length;
    let uy = (donor.position.y - metal.position.y) / length;
    let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
    let core = |atom| {
        label_ink_boxes(&atom_label_runs_with_charge_layout(
            atom,
            &doc,
            ChargeLayout::Omitted,
        ))
    };
    let metal_end = label_end(metal.position, ux, uy, &core(metal), length, margin);
    let donor_end = label_end(donor.position, -ux, -uy, &core(donor), length, margin);
    let metal_trim = (metal_end.x - metal.position.x) * ux + (metal_end.y - metal.position.y) * uy;
    let donor_trim = (donor.position.x - donor_end.x) * ux + (donor.position.y - donor_end.y) * uy;
    assert!(metal_trim < length - 0.1 && donor_trim < length - 0.1);
    assert!(length - metal_trim - donor_trim <= 0.1);
    assert_eq!(
        format!("{:?}", atom_label_runs(metal, &doc)),
        format!(
            "{:?}",
            atom_label_runs_with_charge_layout(metal, &doc, ChargeLayout::Unplaced)
        ),
        "A jointly core-covered contact cannot justify moving the charge"
    );
    assert_eq!(
        primitives(&doc)
            .iter()
            .filter(|p| matches!(p, Primitive::Polygon(points) if points.len() == 3))
            .count(),
        1,
        "Only the other, adequately separated contact should paint"
    );
}

#[test]
fn implicit_carbon_charge_and_genuinely_covered_short_bonds_are_not_repositioned() {
    for element in ["C", "Co"] {
        let mut doc = Document::default();
        let center = doc.add_atom(element, Point::default());
        let neighbor = doc.add_atom("N", Point::new(3., -1.));
        doc.add_bond(neighbor, center, 5, "plain");
        doc.atom_mut(center).unwrap().charge = 3;
        assert_eq!(
            format!("{:?}", atom_label_runs(doc.atom(center).unwrap(), &doc)),
            format!(
                "{:?}",
                atom_label_runs_with_charge_layout(
                    doc.atom(center).unwrap(),
                    &doc,
                    ChargeLayout::Unplaced
                )
            )
        );
        assert!(
            primitives(&doc)
                .iter()
                .all(|p| { !matches!(p, Primitive::Polygon(points) if points.len() == 3) })
        );
    }
}

/// Issue #337: unjoined en donors around Co³⁺ keep their automatic H₂ clear
/// of the metal's 3+ and of each other; a manual side is still honored.
#[test]
fn automatic_hydrogens_avoid_nearby_unbonded_labels_and_charges() {
    let mut doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/coordination/co-en3-before.rsk"
    ))
    .unwrap();
    let overlaps = |doc: &Document, a: u64, b: u64| {
        let other = atom_label_ink_boxes(doc.atom(b).unwrap(), doc);
        atom_label_ink_boxes(doc.atom(a).unwrap(), doc)
            .iter()
            .any(|(lo, hi)| {
                other
                    .iter()
                    .any(|(a, b)| hi.x > a.x && lo.x < b.x && hi.y > a.y && lo.y < b.y)
            })
    };
    for a in &doc.atoms {
        for b in doc.atoms.iter().filter(|b| b.id > a.id) {
            assert!(!overlaps(&doc, a.id, b.id), "Labels {} and {}", a.id, b.id);
        }
    }
    // The upper-right NH₂ (atom 2) stacks its H₂ above rather than toward Co.
    let n = charge_run(&doc, 2, "N");
    let h = charge_run(&doc, 2, "H");
    assert!(h.y < n.y && (h.x - n.x).abs() < 0.1, "N {n:?}, H {h:?}");
    doc.atom_mut(2).unwrap().display.hydrogen_position = HydrogenPosition::Left;
    assert!(charge_run(&doc, 2, "H").x < n.x);
}

/// A replacement side must not put the hydrogens across an unrelated bond.
#[test]
fn automatic_hydrogen_replacement_side_avoids_nearby_bonds() {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    let c = doc.add_atom("C", Point::new(60., 0.));
    doc.add_bond(n, c, 1, "plain");
    doc.atom_mut(n).unwrap().label_h = 2;
    // A free O label blocks the usual left-hand H2.
    doc.add_atom("O", Point::new(-35., 0.));
    let (n_text, h) = (charge_run(&doc, n, "N"), charge_run(&doc, n, "H"));
    assert!(h.y < n_text.y && (h.x - n_text.x).abs() < 0.1);
    // An unrelated shaft above the N leaves only the lower side clear.
    let left = doc.add_atom("C", Point::new(-40., -25.));
    let right = doc.add_atom("C", Point::new(40., -25.));
    doc.add_bond(left, right, 1, "plain");
    // Editing bonds clears computed hydrogen counts.
    doc.atom_mut(n).unwrap().label_h = 2;
    let h = charge_run(&doc, n, "H");
    assert!(h.y > n_text.y && (h.x - n_text.x).abs() < 0.1);
}

/// A replacement side must not cover an explicit mark on the atom.
#[test]
fn automatic_hydrogen_replacement_side_avoids_atom_marks() {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    let c = doc.add_atom("C", Point::new(60., 0.));
    doc.add_bond(n, c, 1, "plain");
    doc.add_atom("O", Point::new(-35., 0.));
    let atom = doc.atom_mut(n).unwrap();
    atom.label_h = 2;
    atom.marks.push(AtomMark {
        id: None,
        kind: MarkKind::LonePairBar,
        offset: Point::new(0., -28.),
        angle: 0.,
        size_pt: None,
    });
    let (n_text, h) = (charge_run(&doc, n, "N"), charge_run(&doc, n, "H"));
    assert!(h.y > n_text.y && (h.x - n_text.x).abs() < 0.1);
}
