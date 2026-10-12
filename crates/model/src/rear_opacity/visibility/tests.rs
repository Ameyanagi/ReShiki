use super::*;
#[test]
fn opaque_ring_query_estimate_includes_managed_foreground_without_backbone_crossing() {
    let mut doc = Document::default();
    crate::editing::ring(&mut doc, Point::default(), 6, true, 0.);
    let circle = crate::aromatic::circles(&doc).remove(0);
    let foreground: Vec<_> = [-1.1, 1.1]
        .into_iter()
        .map(|x| {
            let id = doc.add_atom("C", circle.center.offset(circle.radius * x, 0.));
            doc.atom_mut(id).unwrap().depth = 100.;
            id
        })
        .collect();
    doc.add_bond(foreground[0], foreground[1], 1, "plain");
    crate::depth_appearance::set_rear_opacity(&mut doc, &foreground, 0.25).unwrap();
    let rear = rear_ids(&doc);
    assert!(
        circle.atoms.iter().all(|id| !rear.contains(id)),
        "The ring is fully opaque"
    );
    assert_eq!(
        crossing_queries(&doc, &rear),
        0,
        "Short foreground does not cross any graph backbone"
    );
    let source = circle.graphic().parts().remove(0);
    let original = Primitive::Path {
        commands: source.commands,
        style: source.style,
        filled: source.filled,
    };
    let (paint, _) = crate::crossings::ring_stroke(
        &doc,
        &circle,
        original.clone(),
        &crate::rear_opacity::Paint::new(&doc),
    );
    let (
        Primitive::Path {
            commands: painted, ..
        },
        Primitive::Path {
            commands: source, ..
        },
    ) = (paint, original)
    else {
        panic!("Expected an actual cubic circle stroke");
    };
    assert_ne!(
        painted, source,
        "The foreground must actually cross the aromatic circle"
    );
    let estimate = estimated_queries(&doc, &rear);
    assert!(
        estimate > foreground.len() + 4,
        "Managed foreground ring queries were ignored"
    );
}
#[test]
fn query_estimate_retains_pre_knockout_owned_arc_geometry() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 6, false, 0.);
    crate::ring_arcs::toggle(&mut doc, &ids[..3]).unwrap();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    let original = crate::ring_arcs::render(&doc);
    let raw_cost = original.primitives.iter().map(curve_queries).sum::<usize>();
    assert!(raw_cost > 20, "The fixture must exercise a real curved arc");
    doc.drawing_style.bold_width_pt = 100.;
    let a = doc.add_atom("C", Point::new(0., -200.));
    let b = doc.add_atom("C", Point::new(0., 200.));
    for id in [a, b] {
        doc.atom_mut(id).unwrap().depth = 100.;
    }
    doc.add_bond(a, b, 1, "bold");
    doc.bonds.last_mut().unwrap().z_order = 1;
    let mut opaque = doc.clone();
    opaque.depth_appearance.clear();
    assert_eq!(
        crate::ring_arcs::render(&opaque)
            .primitives
            .iter()
            .map(curve_queries)
            .sum::<usize>(),
        0,
        "The opaque knockout must remove the original arc"
    );
    let estimate = estimated_queries(&doc, &rear_ids(&doc));
    assert!(
        estimate >= raw_cost * 2,
        "Owned curve quota was inferred from already-knocked-out paint: pre-knockout curved segments={raw_cost}, retained query estimate={estimate}"
    );
}
#[test]
fn exact_triangle_intervals_find_hidden_middle_and_keep_depth_ties_visible() {
    let triangle = Triangle {
        vertices: [[0., -1., 2.], [1., 1., 2.], [-1., 1., 2.]],
        face: vec![],
        shell: 0,
        layer: 0,
    };
    let range = triangle
        .hidden(Point::new(-2., 0.), Point::new(2., 0.), 0., 0., 0.001)
        .unwrap();
    assert_eq!(range, (0.375, 0.625));
    assert!(
        triangle
            .hidden(Point::new(-2., 0.), Point::new(2., 0.), 2., 2., 0.001)
            .is_none()
    );
    assert!(
        triangle
            .hidden(Point::new(-2., 0.), Point::new(2., 0.), 3., 3., 0.001)
            .is_none()
    );
}
#[test]
fn triangle_validation_rejects_crossings_and_overlap_but_allows_shared_edges() {
    let a = [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]];
    assert!(improper_intersection(
        a,
        [[0.5, 0.5, -1.], [0.5, 0.5, 1.], [1.5, 0.5, 0.]],
        1e-6
    ));
    assert!(improper_intersection(
        a,
        [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        1e-6
    ));
    assert!(!improper_intersection(
        a,
        [[2., 0., 0.], [0., 0., 0.], [1., -1., 0.]],
        1e-6
    ));
    assert!(!improper_intersection(
        a,
        [[0., 0., 0.], [-1., 0., 1.], [0., -1., 1.]],
        1e-6
    ));
}
fn rectangle(x: f32, y: f32, w: f32, h: f32) -> Vec<Point> {
    vec![
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x + w, y + h),
        Point::new(x, y + h),
    ]
}
#[test]
fn ink_sweep_preserves_holes_tangencies_vertex_crossings_and_many_disjoint_contours() {
    let outer = rectangle(0., 0., 10., 10.);
    let mut hole = rectangle(3., 3., 4., 4.);
    hole.reverse();
    let ink = Ink {
        owners: vec![],
        layer: 0,
        contours: vec![outer, hole],
        field: Field::Constant(10.),
    };
    assert_eq!(
        ink.intervals(Point::new(-5., 5.), Point::new(15., 5.), 0., 0., 0, 0.001),
        vec![(0.25, 0.4), (0.6, 0.75)]
    );
    assert_eq!(
        ink.intervals(Point::new(5., 5.), Point::new(15., 5.), 0., 0., 0, 0.001),
        vec![(0.2, 0.5)]
    );
    assert_eq!(
        ink.intervals(Point::new(1., 1.), Point::new(2., 2.), 0., 0., 0, 0.001),
        vec![(0., 1.)]
    );
    assert!(
        ink.intervals(Point::new(-1., 1.), Point::new(1., -1.), 0., 0., 0, 0.001)
            .is_empty()
    );
    assert_eq!(
        ink.intervals(Point::new(-1., -1.), Point::new(11., 11.), 0., 0., 0, 0.001),
        vec![(1. / 12., 1. / 3.), (2. / 3., 11. / 12.)]
    );
    assert_eq!(
        ink.intervals(Point::new(1., 1.), Point::new(1., 1.), 0., 0., 0, 0.001),
        vec![(0., 1.)]
    );
    let many = Ink {
        owners: vec![],
        layer: 0,
        contours: (0..1000)
            .map(|i| rectangle(i as f32 * 2., -1., 1., 2.))
            .collect(),
        field: Field::Constant(10.),
    };
    let intervals = many.intervals(Point::new(-1., 0.), Point::new(2000., 0.), 0., 0., 0, 0.001);
    assert_eq!(intervals.len(), 1000);
    assert_eq!(intervals[0], (1. / 2001., 2. / 2001.));
    assert_eq!(intervals[999], (1999. / 2001., 2000. / 2001.));
}
