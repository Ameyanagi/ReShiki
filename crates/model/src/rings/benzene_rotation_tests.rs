use super::*;
fn benzene() -> Drawing {
    Drawing {
        preset: Preset::Benzene,
        length: 42.,
        alternate: false,
        connect: false,
    }
}
fn fuse(doc: &Document, order: u8) -> (Document, Vec<u64>) {
    let bond = doc.bonds.iter().find(|b| b.order == order).unwrap();
    let (a, b) = (doc.atom(bond.a).unwrap(), doc.atom(bond.b).unwrap());
    let mid = Point::new(
        (a.position.x + b.position.x) / 2.,
        (a.position.y + b.position.y) / 2.,
    );
    benzene().place(doc, mid, None, 5.).unwrap()
}
fn doubles(doc: &Document, id: u64) -> usize {
    doc.bonds
        .iter()
        .filter(|b| (b.a == id || b.b == id) && b.order == 2)
        .count()
}
fn ring_orders(doc: &Document, ring: &[u64]) -> Vec<u8> {
    (0..6)
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % 6]);
            doc.bonds
                .iter()
                .find(|x| (x.a == a && x.b == b) || (x.a == b && x.b == a))
                .unwrap()
                .order
        })
        .collect()
}
fn assert_cycle_orders(actual: &[u8], expected: &[u8]) {
    // Template selection retains source-ID order; the shared edge need not
    // be first. Check the cyclic bond pattern independently of that order.
    assert!(
        (0..actual.len()).any(|start| {
            (0..actual.len()).all(|i| actual[(start + i) % actual.len()] == expected[i])
                || (0..actual.len())
                    .all(|i| actual[(start + actual.len() - i) % actual.len()] == expected[i])
        }),
        "{actual:?} does not match cyclic pattern {expected:?}"
    );
}

#[test]
fn free_benzene_is_unchanged() {
    let (doc, ring) = benzene()
        .place(&Document::default(), Point::default(), None, 5.)
        .unwrap();
    assert_eq!(ring.len(), 6);
    assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 3);
    assert!(ring.iter().all(|id| doubles(&doc, *id) == 1));
}

#[test]
fn kekule_single_bond_fuses_into_naphthalene() {
    let base = Preset::Benzene.document(42., false);
    let (doc, ring) = fuse(&base, 1);
    assert_eq!(doc.atoms.len(), 10);
    assert_eq!(doc.bonds.len(), 11);
    assert!(doc.atoms.iter().all(|a| doubles(&doc, a.id) == 1));
    assert_cycle_orders(&ring_orders(&doc, &ring), &[1, 1, 2, 1, 2, 1]);
}

#[test]
fn kekule_double_bond_fuses_into_naphthalene() {
    let base = Preset::Benzene.document(42., false);
    let (doc, ring) = fuse(&base, 2);
    assert_eq!(doc.atoms.len(), 10);
    assert!(doc.atoms.iter().all(|a| doubles(&doc, a.id) == 1));
    assert_cycle_orders(&ring_orders(&doc, &ring), &[2, 1, 2, 1, 2, 1]);
}

#[test]
fn saturated_single_bond_is_upgraded() {
    let base = Preset::Regular.document(42., false);
    let (doc, ring) = fuse(&base, 1);
    assert_eq!(ring_orders(&doc, &ring), vec![2, 1, 2, 1, 2, 1]);
    assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 3);
}

#[test]
fn crowded_single_bond_lowers_new_doubles() {
    let mut base = Document::default();
    let a = base.add_atom("C", Point::new(0., 0.));
    let b = base.add_atom("C", Point::new(42., 0.));
    let c = base.add_atom("C", Point::new(-21., -36.4));
    let d = base.add_atom("C", Point::new(63., -36.4));
    base.add_bond(a, b, 1, "plain");
    base.add_bond(a, c, 2, "plain");
    base.add_bond(b, d, 2, "plain");
    let (doc, ring) = fuse(&base, 1);
    assert_eq!(ring_orders(&doc, &ring)[0], 1);
    assert!(doc.atoms.iter().all(|x| doubles(&doc, x.id) <= 1));
}

fn shifted() -> Drawing {
    Drawing {
        alternate: true,
        ..benzene()
    }
}
fn fuse_with(drawing: Drawing, doc: &Document, order: u8) -> (Document, Vec<u64>) {
    let bond = doc.bonds.iter().find(|b| b.order == order).unwrap();
    let (a, b) = (doc.atom(bond.a).unwrap(), doc.atom(bond.b).unwrap());
    let mid = Point::new(
        (a.position.x + b.position.x) / 2.,
        (a.position.y + b.position.y) / 2.,
    );
    drawing.place(doc, mid, None, 5.).unwrap()
}

#[test]
fn shift_keeps_kekule_fusion_intact() {
    let base = Preset::Benzene.document(42., false);
    for order in [1, 2] {
        let (doc, _) = fuse_with(shifted(), &base, order);
        assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 5);
        assert!(doc.atoms.iter().all(|a| doubles(&doc, a.id) == 1));
    }
}

#[test]
fn shift_still_alternates_where_both_patterns_fit() {
    let base = Preset::Regular.document(42., false);
    let (doc, ring) = fuse_with(shifted(), &base, 1);
    assert_eq!(ring_orders(&doc, &ring), vec![1, 2, 1, 2, 1, 2]);
    let (doc, ring) = shifted()
        .place(&Document::default(), Point::default(), None, 5.)
        .unwrap();
    let (plain, plain_ring) = benzene()
        .place(&Document::default(), Point::default(), None, 5.)
        .unwrap();
    assert_ne!(ring_orders(&doc, &ring), ring_orders(&plain, &plain_ring));
}

/// A single bond a-b plus a carbon sitting on a vertex of the ring fused onto it,
/// carrying `substituents` single bonds that point away from the ring.
fn crowded_vertex(substituents: usize) -> Document {
    let mut base = Document::default();
    let a = base.add_atom("C", Point::new(0., 0.));
    let b = base.add_atom("C", Point::new(42., 0.));
    base.add_bond(a, b, 1, "plain");
    let x = base.add_atom("C", Point::new(0., 72.746));
    for (dx, dy) in [(-36.4, 21.), (36.4, 21.), (0., 42.)]
        .into_iter()
        .take(substituents)
    {
        let s = base.add_atom("C", Point::new(dx, 72.746 + dy));
        base.add_bond(x, s, 1, "plain");
    }
    base
}
/// Drags from the a-b bond toward the crowded carbon, forcing the ring onto its side.
fn fuse_toward_vertex(base: &Document) -> Result<(Document, Vec<u64>), &'static str> {
    benzene().place(base, Point::new(21., 0.), Some(Point::new(21., 60.)), 5.)
}

#[test]
fn nearby_atom_with_room_is_merged() {
    let base = crowded_vertex(1);
    let (doc, ring) = fuse_toward_vertex(&base).unwrap();
    assert_eq!(doc.atoms.len(), base.atoms.len() + 3);
    assert!(ring.contains(&3));
}

#[test]
fn nearby_saturated_atom_is_rejected() {
    assert!(fuse_toward_vertex(&crowded_vertex(3)).is_err());
}
