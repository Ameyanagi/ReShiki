use super::*;
#[test]
fn depth_emphasis_preserves_graph_and_does_not_create_stereo_wedges() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 5, false, 5.);
    let edges: Vec<_> = doc.bonds.iter().map(|b| (b.a, b.b, b.order)).collect();
    tilt(&mut doc, &ids, 60., true);
    depth_bonds(&mut doc, &ids);
    assert!(doc.bonds.iter().any(|b| b.display == "bold"));
    assert!(doc.bonds.iter().all(|b| b.projection));
    assert_eq!(
        edges,
        doc.bonds
            .iter()
            .map(|b| (b.a, b.b, b.order))
            .collect::<Vec<_>>()
    );
    let chemistry = crate::chemistry::document::prepare(&doc).unwrap();
    assert!(
        chemistry
            .state
            .directions
            .iter()
            .all(|d| *d == crate::chemistry::kekulize::Direction::None)
    );
    let displays: Vec<_> = doc.bonds.iter().map(|b| b.display.clone()).collect();
    crate::editing::transform(&mut doc, &ids, crate::editing::Transform::FlipHorizontal);
    assert_eq!(
        displays,
        doc.bonds
            .iter()
            .map(|b| b.display.clone())
            .collect::<Vec<_>>()
    );
}
#[test]
fn malformed_centroids_and_nonfinite_depth_are_rejected() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(40., 0.));
    let c = add_centroid(&mut doc, &[a, b]).unwrap();
    for members in [vec![a], vec![a, a], vec![a, 999], vec![a, c]] {
        doc.atom_mut(c).unwrap().centroid = members;
        assert!(doc.validate().is_err());
    }
    doc.atom_mut(c).unwrap().centroid = vec![a, b];
    doc.atom_mut(a).unwrap().depth = f32::NAN;
    assert!(doc.validate().is_err());
}

#[test]
fn tilt_inverse_restores_ring_and_ellipse_without_changing_bond_connections() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::new(100., 100.), 5, false, 5.);
    let g = crate::graphics::Graphic::dragged(
        doc.next_id(),
        crate::graphics::GraphicKind::Ellipse,
        Point::new(80., 80.),
        Point::new(120., 120.),
        Default::default(),
        Default::default(),
        false,
    );
    doc.graphics.push(g);
    let all = doc.all_ids();
    let original = doc.clone();
    for x in [true, false] {
        tilt(&mut doc, &all, 60., x);
        assert!(doc.atoms.iter().any(|a| a.depth.abs() > 1.));
        tilt(&mut doc, &all, -60., x);
    }
    for (a, b) in doc.atoms.iter().zip(&original.atoms) {
        assert!(a.position.distance(b.position) < 0.001);
        assert!(a.depth.abs() < 0.001);
    }
    assert_eq!(doc.bonds, original.bonds);
    assert!(doc.graphics[0].axis_y.distance(original.graphics[0].axis_y) < 0.001);
    assert_eq!(ids.len(), 5);
}
#[test]
fn centroid_tracks_members_and_survives_copy_save_and_deletion() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::new(100., 100.), 5, false, 5.);
    let c = add_centroid(&mut doc, &ids).unwrap();
    let fe = doc.add_atom("Fe", Point::new(100., 200.));
    doc.add_bond(c, fe, 5, "dashed");
    let p = doc.atom(c).unwrap().position;
    doc.translate(&ids, 30., 40.);
    assert!(doc.atom(c).unwrap().position.distance(p.offset(30., 40.)) < 0.001);
    assert!(crate::scene::svg(&doc).contains("Fe"));
    assert!(!crate::scene::svg(&doc).contains(">*<"));
    doc.validate().unwrap();
    let saved: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    let mut copy = Document::default();
    crate::editing::append(&mut copy, &saved, Point::new(10., 20.));
    copy.validate().unwrap();
    assert_eq!(copy.atoms.len(), saved.atoms.len());
    assert_eq!(copy.bonds.len(), saved.bonds.len());
    doc.delete(&[ids[0]]);
    assert!(doc.atom(c).is_none());
    assert!(doc.bonds.iter().all(|b| b.a != c && b.b != c));
    doc.validate().unwrap();
}
