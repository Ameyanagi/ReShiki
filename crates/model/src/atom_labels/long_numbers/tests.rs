use super::*;
use crate::atom_labels::{Number, Owner, indicators, is_shift_label, shift_label_style};

/// `fixture` with `#N · ppm` labels, as scripts write predicted shifts.
fn labelled(fixture: &[u8], shifts: &[(u64, f64)], decimals: usize) -> Document {
    let mut doc = Document::from_json(fixture).unwrap();
    let style = shift_label_style(&doc);
    for &(id, ppm) in shifts {
        doc.atom_mut(id).unwrap().display.number = Some(Number {
            text: format!("#{id} · {ppm:.decimals$} ppm"),
            offset: None,
            style: style.clone(),
        });
    }
    doc
}

/// Morphine ¹H as predicted, the issue's own example.
fn morphine() -> Document {
    let shifts = [
        (1, 2.535),
        (3, 3.29),
        (4, 2.11),
        (8, 6.795),
        (9, 6.796),
        (14, 4.9575),
        (15, 3.775),
        (17, 6.26),
        (18, 6.26),
        (19, 3.36),
        (20, 3.57),
        (21, 3.005),
    ];
    let fixture = include_bytes!("../../../../../tests/fixtures/nmr/morphine.rsk");
    labelled(fixture, &shifts, 2)
}

/// 4-Anisaldehyde ¹³C as predicted.
fn anisaldehyde() -> Document {
    let shifts = [
        (1, 55.3),
        (3, 159.6),
        (4, 114.2),
        (5, 127.484),
        (6, 134.448),
        (7, 190.85),
        (9, 127.484),
        (10, 114.2),
    ];
    let fixture = include_bytes!("../../../../../tests/fixtures/nmr/anisaldehyde.rsk");
    labelled(fixture, &shifts, 1)
}

fn overlap(a: (Point, Point), b: (Point, Point)) -> bool {
    a.0.x < b.1.x && a.1.x > b.0.x && a.0.y < b.1.y && a.1.y > b.0.y
}

/// Every shift label's text box touches no bond, atom label, atom or other
/// label, and its leader no other label; a label without a leader is nearer
/// its own atom than any other.
fn assert_clear(doc: &Document, labels: usize) {
    let placed: Vec<_> = indicators(doc)
        .into_iter()
        .filter(|i| matches!(i.owner, Owner::Number(_)))
        .collect();
    assert_eq!(placed.len(), labels);
    let boxes: Vec<_> = placed
        .iter()
        .map(|i| (i.origin, i.origin.offset(i.width, i.height)))
        .collect();
    for (index, (label, &(lo, hi))) in placed.iter().zip(&boxes).enumerate() {
        let Owner::Number(id) = label.owner else {
            unreachable!()
        };
        let text = &label.text;
        for b in &doc.bonds {
            let (a, c) = (
                doc.atom(b.a).unwrap().position,
                doc.atom(b.b).unwrap().position,
            );
            assert!(
                !segment_hits_rect(a, c, lo, hi),
                "{text} touches bond {}-{}",
                b.a,
                b.b
            );
        }
        for atom in &doc.atoms {
            let ink = crate::scene::atom_label_bounds(atom, doc)
                .unwrap_or((atom.position, atom.position));
            assert!(!overlap((lo, hi), ink), "{text} touches atom {}", atom.id);
            assert!(
                atom.position.distance(nearest(atom.position, lo, hi)) > 0.,
                "{text} covers atom {}",
                atom.id
            );
        }
        for (other, &bounds) in boxes.iter().enumerate().filter(|(j, _)| *j != index) {
            assert!(
                !overlap((lo, hi), bounds),
                "{text} touches {}",
                placed[other].text
            );
            if let Some((a, b, _)) = placed[other].leader {
                assert!(!segment_hits_rect(a, b, lo, hi), "{text} crosses a leader");
            }
        }
        // Without a leader, the box is within 1.5 bond lengths of its atom
        // and nearer to it than to any other atom.
        let own = doc.atom(id).unwrap().position;
        let gap = |p: Point| p.distance(nearest(p, lo, hi));
        let attributed = label.leader.is_some()
            || (gap(own) <= 1.5 * doc.drawing_style.bond_length_world
                && doc
                    .atoms
                    .iter()
                    .all(|a| a.id == id || gap(a.position) > gap(own)));
        assert!(
            attributed,
            "{text} is not clearly its atom's and has no leader"
        );
        // A leader crosses no other label's text, nor any atom label but its
        // own atom's.
        if let Some((a, b, _)) = label.leader {
            for (other, &(olo, ohi)) in boxes.iter().enumerate().filter(|(j, _)| *j != index) {
                assert!(
                    !segment_hits_rect(a, b, olo, ohi),
                    "{text}'s leader crosses {}",
                    placed[other].text
                );
            }
            for atom in doc.atoms.iter().filter(|atom| atom.id != id) {
                if let Some((olo, ohi)) = crate::scene::atom_label_bounds(atom, doc) {
                    assert!(
                        !segment_hits_rect(a, b, olo, ohi),
                        "{text}'s leader crosses atom {}'s label",
                        atom.id
                    );
                }
            }
        }
        // A leader crosses a bond only well clear of its ends, where it
        // would read as a mark on the bond.
        if let Some((a, b, _)) = label.leader {
            for bond in doc.bonds.iter().filter(|bond| bond.a != id && bond.b != id) {
                let (c, d) = (
                    doc.atom(bond.a).unwrap().position,
                    doc.atom(bond.b).unwrap().position,
                );
                if let Some(t) = crossing(a, b, c, d) {
                    assert!(
                        t.min(1. - t) * a.distance(b) >= 0.5 * doc.drawing_style.bond_length_world,
                        "{text}'s leader crosses bond {}-{} near its end",
                        bond.a,
                        bond.b
                    );
                }
            }
        }
        if let Some((a, _, width)) = label.leader {
            assert!(a.distance(own) < 3. * doc.drawing_style.bond_length_world);
            assert!(width < doc.drawing_style.line_width());
        }
    }
}

#[test]
fn morphine_1h_shift_labels_are_clear_and_attributable() {
    assert_clear(&morphine(), 12);
}

#[test]
fn morphine_13c_shift_labels_are_clear_and_attributable() {
    let shifts = [(1, 41.8), (8, 127.4), (9, 114.3), (10, 149.0), (18, 126.8)];
    let fixture = include_bytes!("../../../../../tests/fixtures/nmr/morphine.rsk");
    assert_clear(&labelled(fixture, &shifts, 1), 5);
}

#[test]
fn anisaldehyde_13c_shift_labels_are_clear_and_attributable() {
    assert_clear(&anisaldehyde(), 8);
}

#[test]
fn the_layout_is_deterministic_and_only_for_numbers_with_a_space() {
    let doc = morphine();
    let first: Vec<_> = indicators(&doc)
        .iter()
        .map(|i| (i.center, i.leader))
        .collect();
    let again: Vec<_> = indicators(&doc)
        .iter()
        .map(|i| (i.center, i.leader))
        .collect();
    assert_eq!(first, again);
    for short in [
        "1",
        "12a",
        "Cα",
        "#19",
        "atom1",
        "L (S)",
        "attached caption text",
    ] {
        assert!(!is_shift_label(short), "{short}");
    }
    for long in ["#19 · 3.36 ppm", "3.36 ppm"] {
        assert!(is_shift_label(long), "{long}");
    }
}

#[test]
fn shift_labels_are_smaller_than_atom_labels() {
    let doc = anisaldehyde();
    let style = shift_label_style(&doc);
    assert!((style.size_pt - 0.65 * doc.drawing_style.font_size_pt).abs() < 1e-6);
    let label = indicators(&doc).remove(0);
    assert!(label.height < 0.7 * doc.drawing_style.font_size());
}

#[test]
fn a_dragged_long_label_far_from_its_atom_gets_a_leader() {
    let mut doc = anisaldehyde();
    let far = 2. * doc.drawing_style.bond_length_world;
    doc.atom_mut(7)
        .unwrap()
        .display
        .number
        .as_mut()
        .unwrap()
        .offset = Some(Point::new(far, far));
    let label = indicators(&doc)
        .into_iter()
        .find(|i| i.owner == Owner::Number(7))
        .unwrap();
    let anchor = doc.atom(7).unwrap().position;
    assert_eq!(label.center, anchor.offset(far, far));
    assert!(label.leader.is_some());
    assert!(matches!(
        label.primitives()[0],
        crate::scene::Primitive::Line(..)
    ));
}

#[test]
fn shift_labels_never_go_below_the_smallest_text_size() {
    let mut doc = anisaldehyde();
    doc.drawing_style.font_size_pt = 6.;
    let style = shift_label_style(&doc);
    assert_eq!(style.size_pt, 4.);
    style.validate().unwrap();
}
