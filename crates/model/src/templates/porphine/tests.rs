use super::*;

#[test]
fn symmetric_core_has_twenty_carbons_opposite_hydrogens_and_equal_bonds() {
    let item = template().unwrap();
    let doc = &item.document;
    assert_eq!(doc.atoms.len(), 24);
    assert_eq!(doc.bonds.len(), 28);
    assert_eq!(doc.atoms.iter().filter(|a| a.element == "C").count(), 20);
    for atom in doc.atoms.iter().filter(|a| a.element == "N") {
        assert_eq!(atom.label_h, u32::from(matches!(atom.id, 21 | 23)));
        let valence: u8 = doc
            .bonds
            .iter()
            .filter(|b| b.a == atom.id || b.b == atom.id)
            .map(|b| b.order)
            .sum();
        assert_eq!(valence, if atom.label_h == 1 { 2 } else { 3 });
    }
    for bond in &doc.bonds {
        let length = doc
            .atom(bond.a)
            .unwrap()
            .position
            .distance(doc.atom(bond.b).unwrap().position);
        assert!((length - 42.).abs() < 0.00003, "{bond:?}: {length}");
    }
    for id in 1..=5 {
        let p = doc.atom(id).unwrap().position;
        let q = doc.atom(id + 5).unwrap().position;
        assert_eq!(q, Point::new(-p.y, p.x));
    }
    for meso in [5, 10, 15, 20] {
        let center = doc.atom(meso).unwrap().position;
        let before = doc.atom(meso - 1).unwrap().position;
        let after = doc.atom(meso % 20 + 1).unwrap().position;
        let a = (before.x - center.x, before.y - center.y);
        let b = (after.x - center.x, after.y - center.y);
        let cosine = (a.0 * b.0 + a.1 * b.1) / (before.distance(center) * after.distance(center));
        assert!((cosine + 0.5).abs() < 0.000001, "Meso {meso}: {cosine}");
    }
    let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened, doc.current());
}
