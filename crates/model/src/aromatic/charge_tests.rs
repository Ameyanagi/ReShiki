use super::*;
use crate::{chemistry::document::aromatic_display, editing, scene, scene::Primitive};

fn ion(size: u8, charge: i32) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let ids = editing::ring(&mut doc, Point::default(), size, false, 5.);
    doc.atom_mut(ids[0]).unwrap().charge = charge;
    for (i, bond) in doc.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 1 { 2 } else { 1 };
    }
    (doc, ids)
}

fn signs(doc: &Document) -> Vec<Primitive> {
    scene::primitives(doc)
        .into_iter()
        .filter(|part| matches!(part, Primitive::Text { text, .. } if ["+", "−", "2+", "2−"].contains(&text.as_str())))
        .collect()
}

fn ink_center(sign: &Primitive) -> Point {
    let boxes = scene::label_ink_boxes(std::slice::from_ref(sign));
    let lo = boxes
        .iter()
        .map(|(lo, _)| *lo)
        .reduce(|a, b| Point::new(a.x.min(b.x), a.y.min(b.y)))
        .unwrap();
    let hi = boxes
        .iter()
        .map(|(_, hi)| *hi)
        .reduce(|a, b| Point::new(a.x.max(b.x), a.y.max(b.y)))
        .unwrap();
    lo.lerp(hi, 0.5)
}

#[test]
fn aromatic_ion_circle_flow_centres_charge_and_restores_atom_label() -> anyhow::Result<()> {
    for (size, charge, sign) in [(5, -1, "−"), (7, 1, "+")] {
        let (original, ids) = ion(size, charge);
        assert!(charge_circle(&original, &ids).is_none());
        let mut doc = aromatic_display(&original, &ids)?.finish()?.document;
        assert!(doc.bonds.iter().all(|bond| bond.order == 4));
        assert_eq!(
            doc.atoms.iter().map(|a| a.label_h).sum::<u32>(),
            u32::from(size)
        );
        let center = circles(&doc)[0].center;
        assert!(ink_center(&signs(&doc)[0]).distance(center) > 10.);
        assert!(toggle_center_charge(&mut doc, &ids).unwrap());
        assert_eq!(doc.atom(ids[0]).unwrap().charge, charge);
        assert!(!doc.atom(ids[0]).unwrap().display.hide_charge);
        assert!(scene::atom_label_ink_bounds(doc.atom(ids[0]).unwrap(), &doc).is_none());
        let drawn = signs(&doc);
        assert_eq!(drawn.len(), 1);
        assert!(matches!(&drawn[0], Primitive::Text { text, .. } if text == sign));
        assert!(ink_center(&drawn[0]).distance(center) < 0.001);

        for atom in &mut doc.atoms {
            atom.position = atom.position.offset(90., -35.);
        }
        assert!(ink_center(&signs(&doc)[0]).distance(center.offset(90., -35.)) < 0.001);
        let without_circle = aromatic_display(&doc, &ids)?.finish()?.document;
        assert!(circles(&without_circle).is_empty());
        assert!(
            scene::atom_label_ink_bounds(without_circle.atom(ids[0]).unwrap(), &without_circle)
                .is_some()
        );
        assert!(ink_center(&signs(&without_circle)[0]).distance(center.offset(90., -35.)) > 10.);
        assert_eq!(without_circle.atom(ids[0]).unwrap().charge, charge);
        assert!(!toggle_center_charge(&mut doc, &ids).unwrap());
    }
    Ok(())
}

#[test]
fn centre_charge_is_not_offered_for_neutral_partial_multiple_or_fused_rings() {
    let mut doc = Document::default();
    let ids = editing::ring(&mut doc, Point::default(), 6, true, 5.);
    assert!(charge_circle(&doc, &ids).is_none());
    doc.atom_mut(ids[0]).unwrap().charge = 1;
    assert!(charge_circle(&doc, &ids[..3]).is_none());
    assert!(charge_circle(&doc, &ids).is_some());
    let next = editing::ring(&mut doc, Point::new(300., 0.), 6, true, 5.);
    doc.atom_mut(next[0]).unwrap().charge = -1;
    let all = doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    assert!(charge_circle(&doc, &all).is_none());

    // A fused neighbour disqualifies the ring, with or without its own circle.
    for order in [4, 1] {
        let mut doc = Document::default();
        let ids = editing::ring(&mut doc, Point::default(), 6, true, 5.);
        doc.atom_mut(ids[0]).unwrap().charge = 1;
        let (a, b) = (ids[0], ids[1]);
        let center = circles(&doc)[0].center;
        let start = doc.atom(a).unwrap().position;
        let mut point = doc.atom(b).unwrap().position;
        let mut delta = Point::new(point.x - start.x, point.y - start.y);
        let cross = delta.x * (center.y - start.y) - delta.y * (center.x - start.x);
        let angle = -cross.signum() * std::f32::consts::PI / 3.;
        let mut previous = b;
        for _ in 0..4 {
            delta = Point::new(
                delta.x * angle.cos() - delta.y * angle.sin(),
                delta.x * angle.sin() + delta.y * angle.cos(),
            );
            point = point.offset(delta.x, delta.y);
            let next = doc.add_atom("C", point);
            doc.add_bond(previous, next, order, "plain");
            previous = next;
        }
        doc.add_bond(previous, a, order, "plain");
        assert!(charge_circle(&doc, &ids).is_none(), "order {order}");
        for atom in &mut doc.atoms {
            atom.display.center_charge_in_ring = true;
        }
        assert!(!charge_is_centered(&doc, doc.atom(a).unwrap()));
    }
}

#[test]
fn centered_ring_charge_sums_flagged_atoms_and_preserves_legacy_serialization() -> anyhow::Result<()>
{
    let (doc, ids) = ion(7, 1);
    let legacy = serde_json::to_string(&doc)?;
    assert!(!legacy.contains("center_charge_in_ring"));
    assert_eq!(
        serde_json::to_string(&serde_json::from_str::<Document>(&legacy)?)?,
        legacy
    );
    let mut doc = aromatic_display(&doc, &ids)?.finish()?.document;
    toggle_center_charge(&mut doc, &ids).unwrap();
    doc.atom_mut(ids[1]).unwrap().charge = 1;
    assert!(matches!(&signs(&doc)[0], Primitive::Text { text, .. } if text == "2+"));
    let stored = serde_json::to_string(&doc)?;
    assert!(stored.contains("\"center_charge_in_ring\":true"));
    assert_eq!(serde_json::from_str::<Document>(&stored)?, doc);
    Ok(())
}

#[test]
fn centering_an_attached_charge_removes_its_ink_and_eraser_target() -> anyhow::Result<()> {
    use crate::scientific::{AtomMark, MarkKind, document_mark_parts, styled_mark_parts};
    let (doc, ids) = ion(7, 1);
    let mut doc = aromatic_display(&doc, &ids)?.finish()?.document;
    let atom = doc.atom_mut(ids[0]).unwrap();
    atom.marks.push(AtomMark {
        id: None,
        kind: MarkKind::Charge,
        offset: Point::new(100., 100.),
        angle: 0.,
        size_pt: None,
    });
    let former_mark = atom.position.offset(100., 100.);
    assert!(!document_mark_parts(doc.atom(ids[0]).unwrap(), &doc).is_empty());
    toggle_center_charge(&mut doc, &ids).unwrap();
    let atom = doc.atom(ids[0]).unwrap();
    assert!(!styled_mark_parts(atom, &doc.drawing_style).is_empty());
    assert!(document_mark_parts(atom, &doc).is_empty());
    let before = doc.clone();
    crate::erasing::stroke(&mut doc, former_mark, former_mark, 1.);
    assert_eq!(doc, before);
    Ok(())
}
