use moruno::{
    crossings,
    document::{Document, Point},
    scene::{self, Primitive},
};
fn crossed() -> Document {
    let mut doc = Document::default();
    for p in [
        Point::new(-30., 0.),
        Point::new(30., 0.),
        Point::new(0., -30.),
        Point::new(0., 30.),
    ] {
        doc.add_atom("C", p);
    }
    doc.add_bond(1, 2, 1, "plain");
    doc.add_bond(3, 4, 1, "plain");
    doc
}
#[test]
fn crossing_trims_lower_geometry_and_front_back_reverse_it() {
    let mut doc = crossed();
    let gaps = crossings::gaps(&doc);
    assert_eq!(gaps[0].len(), 1);
    assert!(gaps[1].is_empty());
    let primitives = scene::primitives(&doc);
    let lines: Vec<_> = primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Line(a, b, _) => Some((a, b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines
            .iter()
            .filter(|(a, b)| a.y == 0. && b.y == 0.)
            .all(|(a, b)| a.x * b.x > 0.)
    );
    doc.bonds[0].z_order = 1;
    let reversed = crossings::gaps(&doc);
    assert!(reversed[0].is_empty());
    assert_eq!(reversed[1].len(), 1);
    assert!(!scene::svg(&doc).contains("rgb(255,255,255)"));
    doc.bonds[0].display = "wedge".into();
    doc.bonds[0].z_order = -1;
    assert!(
        scene::primitives(&doc)
            .iter()
            .filter(|p| matches!(p, Primitive::Polygon(_)))
            .count()
            >= 2
    );
    let serialized = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back.bonds[0].z_order, -1);
}
#[test]
fn connected_or_parallel_bonds_never_get_crossing_gaps() {
    let mut doc = crossed();
    doc.bonds[1].a = 1;
    assert!(crossings::gaps(&doc).iter().all(Vec::is_empty));
    doc = crossed();
    doc.atoms[2].position = Point::new(-30., 5.);
    doc.atoms[3].position = Point::new(30., 5.);
    assert!(crossings::gaps(&doc).iter().all(Vec::is_empty));
}
