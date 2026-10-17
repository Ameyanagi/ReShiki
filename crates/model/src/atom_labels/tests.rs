use super::*;

/// A mapped atom at the origin with plain bonds to carbons at `neighbors`.
fn mapped(element: &str, neighbors: &[Point]) -> (Document, Indicator) {
    let mut doc = Document::default();
    doc.atom_labels.maps = true;
    let atom = doc.add_atom(element, Point::new(0., 0.));
    for &p in neighbors {
        let c = doc.add_atom("C", p);
        doc.add_bond(atom, c, 1, "plain");
    }
    doc.atom_mut(atom).unwrap().map_num = 3;
    let map = indicators(&doc)
        .into_iter()
        .find(|i| i.owner == Owner::Mapping(atom))
        .unwrap();
    (doc, map)
}

/// Issue #314: a map number beside its element label reads as part of it
/// (O 3 → O₃), whichever way the bond points after Align rotates a product.
#[test]
fn map_numbers_stack_on_their_element_label() {
    for element in ["O", "I"] {
        for step in 0..48 {
            let angle = (step as f32 * 7.5).to_radians();
            let (doc, map) = mapped(element, &[Point::new(42. * angle.cos(), 42. * angle.sin())]);
            let (lo, hi) = crate::scene::atom_label_ink_bounds(&doc.atoms[0], &doc).unwrap();
            let (left, right) = (map.origin.x, map.origin.x + map.width);
            let (top, bottom) = (map.origin.y, map.origin.y + map.height);
            assert!(
                (bottom <= lo.y || top >= hi.y) && left < hi.x && right > lo.x,
                "{element} bond at {}°: map {:?} not stacked on label {lo:?}..{hi:?}",
                step as f32 * 7.5,
                map.center,
            );
        }
    }
}

#[test]
fn stacking_never_moves_a_map_number_nearer_another_atom() {
    let carbons = [Point::new(0., -42.), Point::new(0., 42.)];
    let (_, map) = mapped("O", &carbons);
    let distance = |p: Point| (p.x - map.center.x).hypot(p.y - map.center.y);
    let own = distance(Point::new(0., 0.));
    assert!(
        carbons.iter().all(|&c| distance(c) > own),
        "map {:?} reads as a carbon's",
        map.center
    );
}

#[test]
fn a_stacked_label_map_does_not_push_a_carbon_map_beside_the_label() {
    // Fully mapped O=C–C–C–C–C: map 6 must not end up beside the O.
    let mut doc = Document::default();
    doc.atom_labels.maps = true;
    let points = [
        (0., 0.),
        (0., -42.),
        (36.373_07, -63.),
        (72.746_13, -42.),
        (72.746_13, 0.),
        (36.373_07, 21.),
    ];
    let ids: Vec<_> = points
        .iter()
        .zip(["O", "C", "C", "C", "C", "C"])
        .map(|(&(x, y), e)| doc.add_atom(e, Point::new(x, y)))
        .collect();
    for (n, pair) in ids.windows(2).enumerate() {
        doc.add_bond(pair[0], pair[1], if n == 0 { 2 } else { 1 }, "plain");
    }
    for (n, id) in ids.iter().enumerate() {
        doc.atom_mut(*id).unwrap().map_num = n as u32 + 1;
    }
    for map in indicators(&doc) {
        let Owner::Mapping(owner) = map.owner else {
            continue;
        };
        let distance = |p: Point| (p.x - map.center.x).hypot(p.y - map.center.y);
        let own = distance(doc.atom(owner).unwrap().position);
        assert!(
            doc.atoms.iter().all(|a| distance(a.position) >= own),
            "map {} at {:?} is nearer another atom",
            map.text,
            map.center
        );
    }
}
