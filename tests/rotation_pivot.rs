use reshiki::{
    document::{Annotation, Arrow, Document, History, Point},
    editing::{self, Transform},
    graphics::{ArcGeometry, Graphic, GraphicKind, PathCommand},
};

#[path = "rotation_pivot/edge_cases.rs"]
mod edge_cases;

fn rotated(p: Point, pivot: Point, degrees: f64) -> Point {
    let (s, c) = degrees.to_radians().sin_cos();
    let x = f64::from(p.x) - f64::from(pivot.x);
    let y = f64::from(p.y) - f64::from(pivot.y);
    Point::new(
        (f64::from(pivot.x) + x * c - y * s) as f32,
        (f64::from(pivot.y) + x * s + y * c) as f32,
    )
}

fn pyrrole(degrees: f64, offset: Point) -> Document {
    let mut doc: Document =
        serde_json::from_str(include_str!("fixtures/pyrrole-rotation-drift.rsk")).unwrap();
    for atom in &mut doc.atoms {
        atom.position =
            rotated(atom.position, Point::default(), degrees).offset(offset.x, offset.y);
    }
    doc
}

fn atom_mean(doc: &Document) -> Point {
    let n = doc.atoms.len() as f64;
    Point::new(
        (doc.atoms
            .iter()
            .map(|a| f64::from(a.position.x))
            .sum::<f64>()
            / n) as f32,
        (doc.atoms
            .iter()
            .map(|a| f64::from(a.position.y))
            .sum::<f64>()
            / n) as f32,
    )
}

fn tolerance(points: impl IntoIterator<Item = Point>) -> f32 {
    let scale = points
        .into_iter()
        .fold(1_f32, |m, p| m.max(p.x.abs()).max(p.y.abs()));
    (8. * f32::EPSILON * scale).max(0.0001)
}

fn assert_atoms(actual: &Document, expected: &Document) {
    let epsilon = tolerance(expected.atoms.iter().map(|a| a.position));
    assert_eq!(actual.atoms.len(), expected.atoms.len());
    for (a, b) in actual.atoms.iter().zip(&expected.atoms) {
        assert_eq!(a.id, b.id);
        assert!(
            a.position.distance(b.position) <= epsilon,
            "atom {}: {:?} != {:?}, error {} > {epsilon}",
            a.id,
            a.position,
            b.position,
            a.position.distance(b.position)
        );
    }
    assert_eq!(actual.bonds, expected.bonds);
    for bond in &expected.bonds {
        let length = |doc: &Document| {
            doc.atom(bond.a)
                .unwrap()
                .position
                .distance(doc.atom(bond.b).unwrap().position)
        };
        assert!((length(actual) - length(expected)).abs() <= (2. * epsilon).max(0.0002));
    }
}

#[test]
fn pyrrole_inverse_and_incremental_rotations_keep_absolute_coordinates() {
    for orientation in [0., 17., 90., 143.5] {
        for offset in [
            Point::default(),
            Point::new(250., -130.),
            Point::new(1000., -750.),
            Point::new(10000., -7000.),
        ] {
            let original = pyrrole(orientation, offset);
            let ids = original.all_ids();
            for step in [1., 15.] {
                let mut doc = original.clone();
                for _ in 0..3 {
                    editing::transform(&mut doc, &ids, Transform::Rotate(step));
                    editing::transform(&mut doc, &ids, Transform::Rotate(-step));
                    assert_atoms(&doc, &original);
                }
            }
            for steps in [6, 12] {
                let mut expected = original.clone();
                let pivot = atom_mean(&original);
                for atom in &mut expected.atoms {
                    atom.position = rotated(atom.position, pivot, f64::from(steps * 15));
                }
                let mut incremental = original.clone();
                for _ in 0..steps {
                    editing::transform(&mut incremental, &ids, Transform::Rotate(15.));
                }
                assert_atoms(&incremental, &expected);
                let mut single = original.clone();
                editing::transform(&mut single, &ids, Transform::Rotate((steps * 15) as f32));
                assert_atoms(&incremental, &single);
            }
        }
    }
}

#[test]
fn native_reopen_between_inverse_rotations_does_not_move_pyrrole() {
    let original = pyrrole(17., Point::new(250., -130.));
    let ids = original.all_ids();
    let mut doc = original.clone();
    editing::transform(&mut doc, &ids, Transform::Rotate(15.));
    let mut reopened: Document =
        serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(reopened, doc);
    editing::transform(&mut reopened, &ids, Transform::Rotate(-15.));
    assert_atoms(&reopened, &original);
}

fn mixed() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-40., 0.));
    let b = doc.add_atom("N", Point::new(20., 40.));
    doc.add_bond(a, b, 1, "plain");
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: Point::new(100., 20.),
        text: "Upright caption".into(),
        format: Default::default(),
    });
    doc.arrows.push(Arrow::new(
        doc.next_id(),
        Point::new(140., -40.),
        Point::new(220., 0.),
        reshiki::arrows::Preset::Fishhook,
        Default::default(),
    ));
    doc.graphics.push(Graphic::dragged(
        doc.next_id(),
        GraphicKind::Rectangle,
        Point::new(260., -30.),
        Point::new(320., 10.),
        Default::default(),
        Default::default(),
        false,
    ));
    doc.group_selection(&doc.all_ids()).unwrap();
    doc
}

fn geometry(doc: &Document, ids: &[u64]) -> Vec<Point> {
    doc.atoms
        .iter()
        .filter(|a| ids.contains(&a.id))
        .map(|a| a.position)
        .chain(
            doc.annotations
                .iter()
                .filter(|a| ids.contains(&a.id))
                .map(|a| a.position),
        )
        .chain(
            doc.arrows
                .iter()
                .filter(|a| ids.contains(&a.id))
                .flat_map(|a| {
                    [Some(a.start), Some(a.end), a.control_point()]
                        .into_iter()
                        .flatten()
                }),
        )
        .chain(
            doc.graphics
                .iter()
                .filter(|g| ids.contains(&g.id))
                .flat_map(|g| g.commands().into_iter().flat_map(|c| c.points())),
        )
        .collect()
}

fn assert_points(actual: Vec<Point>, expected: Vec<Point>) {
    let epsilon = tolerance(expected.iter().copied());
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(
            a.distance(b) <= epsilon,
            "{a:?} != {b:?}: error {} > {epsilon}",
            a.distance(b)
        );
    }
}

#[test]
fn mixed_selection_uses_geometry_sites_and_preserves_history_and_styles() {
    let mut original = mixed();
    let ids = original.all_ids();
    // Two atom sites, caption insertion point, arrow midpoint, rectangle center.
    let pivot = Point::new(110., 6.);
    assert_eq!(editing::rotation_center(&original, &ids), Some(pivot));
    let fixed = original.add_atom("O", Point::new(400., 130.));
    let mut changed_style = original.clone();
    changed_style.annotations[0].format.style.size_pt = 28.;
    changed_style.arrows[0].style.as_mut().unwrap().width_pt = 3.;
    changed_style.graphics[0].style.width_pt = 4.;
    assert_eq!(editing::rotation_center(&changed_style, &ids), Some(pivot));
    let mut reordered = ids.clone();
    reordered.reverse();
    reordered.extend([ids[0], u64::MAX]);
    assert_eq!(editing::rotation_center(&original, &reordered), Some(pivot));

    let mut doc = original.clone();
    let mut history = History::default();
    for _ in 0..6 {
        let before = doc.clone();
        editing::transform(&mut doc, &ids, Transform::Rotate(15.));
        assert!(history.commit(before, &doc));
    }
    assert_points(
        geometry(&doc, &ids),
        geometry(&original, &ids)
            .into_iter()
            .map(|p| rotated(p, pivot, 90.))
            .collect(),
    );
    assert_eq!(doc.atom(fixed), original.atom(fixed));
    assert_eq!(doc.bonds, original.bonds);
    assert_eq!(doc.groups, original.groups);
    assert_eq!(doc.annotations[0].format, original.annotations[0].format);
    assert_eq!(doc.arrows[0].style, original.arrows[0].style);
    assert_eq!(doc.graphics[0].style, original.graphics[0].style);
    assert_eq!(doc.drawing_style, original.drawing_style);
    let final_doc = doc.clone();
    for _ in 0..6 {
        assert!(history.undo(&mut doc));
    }
    assert_eq!(doc, original);
    assert!(!history.can_undo());
    for _ in 0..6 {
        assert!(history.redo(&mut doc));
    }
    assert_eq!(doc, final_doc);
    let wire = serde_json::to_vec(&doc).unwrap();
    doc = serde_json::from_slice(&wire).unwrap();
    assert_eq!(doc, final_doc);
    editing::transform(&mut doc, &ids, Transform::Rotate(-90.));
    assert_points(geometry(&doc, &ids), geometry(&original, &ids));
}

#[test]
fn subsets_abbreviations_and_derived_centroids_use_only_independent_visible_sites() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    assert_eq!(
        editing::rotation_center(&doc, &[c]),
        Some(Point::new(42., 0.))
    );
    assert_eq!(
        editing::rotation_center(&doc, &[a, c]),
        Some(Point::new(21., 0.))
    );
    let original = doc.clone();
    editing::transform(&mut doc, &[c], Transform::Rotate(15.));
    assert_eq!(
        doc.atom(a).unwrap().position,
        original.atom(a).unwrap().position
    );
    assert_eq!(
        doc.atom(b).unwrap().position,
        original.atom(b).unwrap().position
    );
    let mut reopened: Document =
        serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    editing::transform(&mut reopened, &[c], Transform::Rotate(-15.));
    assert_atoms(&reopened, &original);

    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-20., 0.));
    let b = doc.add_atom("C", Point::new(20., 20.));
    let c = doc.add_atom("N", Point::new(80., 40.));
    let centroid = reshiki::projection::add_centroid(&mut doc, &[a, b]).unwrap();
    assert_eq!(
        editing::rotation_center(&doc, &[a, c, centroid]),
        Some(Point::new(30., 20.))
    );
    assert_eq!(editing::rotation_center(&doc, &[centroid]), None);
    let original = doc.clone();
    editing::transform(&mut doc, &[centroid], Transform::Rotate(15.));
    assert_eq!(doc, original);
    for angle in [15., -15.] {
        editing::transform(&mut doc, &[a, c, centroid], Transform::Rotate(angle));
        assert_eq!(doc.atom(b), original.atom(b));
    }
    assert_atoms(&doc, &original);
    for kind in [
        reshiki::attachments::Kind::MultiCenter,
        reshiki::attachments::Kind::Variable,
    ] {
        let mut typed = original.clone();
        typed.atom_mut(centroid).unwrap().attachment = Some(kind);
        typed.atom_mut(centroid).unwrap().position = Point::new(100., -30.);
        assert_eq!(
            editing::rotation_center(&typed, &[centroid]),
            Some(Point::new(100., -30.))
        );
    }
}

#[test]
fn graphic_reference_sites_follow_frames_for_all_supported_kinds() {
    use reshiki::scientific::{OrbitalKind, SymbolKind};
    let mut cases: Vec<_> = GraphicKind::DRAWABLE
        .into_iter()
        .chain([
            GraphicKind::Path,
            GraphicKind::Picture,
            GraphicKind::Symbol(SymbolKind::CirclePlus),
            GraphicKind::Orbital(OrbitalKind::Lobe),
        ])
        .map(|kind| {
            let mut g = Graphic::dragged(
                1,
                kind,
                Point::default(),
                Point::new(80., 60.),
                Default::default(),
                Default::default(),
                false,
            );
            g.origin = Point::new(10., 20.);
            g.axis_x = Point::new(80., 60.);
            g.axis_y = Point::new(-30., 40.);
            let expected = match kind {
                GraphicKind::Symbol(_) | GraphicKind::Orbital(_) => Point::new(10., 20.),
                GraphicKind::Line | GraphicKind::Curve => Point::new(50., 50.),
                GraphicKind::Arc => Point::new(20., 90.),
                GraphicKind::Path => {
                    g.path = vec![
                        PathCommand::Move(Point::default()),
                        PathCommand::Cubic(
                            Point::new(0.25, -0.5),
                            Point::new(0.75, 1.),
                            Point::new(1., 0.5),
                        ),
                        PathCommand::Close,
                    ];
                    Point::new(42.5, 60.)
                }
                _ => Point::new(35., 70.),
            };
            if kind == GraphicKind::Picture {
                let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 255]));
                let mut bytes = std::io::Cursor::new(Vec::new());
                image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
                g.picture = Some(reshiki::pictures::Picture::import(bytes.get_ref()).unwrap());
            }
            (g, expected)
        })
        .collect();
    for sweep in [234.5, 360.] {
        let (mut g, _) = cases
            .iter()
            .find(|(g, _)| g.kind == GraphicKind::Arc)
            .unwrap()
            .clone();
        g.arc = Some(ArcGeometry {
            start_degrees: 17.,
            sweep_degrees: sweep,
        });
        cases.push((g, Point::new(35., 70.)));
    }
    let (mut edge_on, _) = cases
        .iter()
        .find(|(g, _)| g.kind == GraphicKind::Ellipse)
        .unwrap()
        .clone();
    edge_on.axis_y = Point::default();
    cases.push((edge_on, Point::new(50., 50.)));

    for (graphic, expected) in cases {
        let source = Document {
            graphics: vec![graphic],
            ..Document::default()
        };
        source.validate().unwrap();
        assert_eq!(editing::rotation_center(&source, &[1]), Some(expected));
        let mut doc = source.clone();
        for _ in 0..6 {
            editing::transform(&mut doc, &[1], Transform::Rotate(15.));
        }
        assert_points(
            geometry(&doc, &[1]),
            geometry(&source, &[1])
                .into_iter()
                .map(|p| rotated(p, expected, 90.))
                .collect(),
        );
        assert_points(
            vec![editing::rotation_center(&doc, &[1]).unwrap()],
            vec![expected],
        );
        let mut reopened: Document =
            serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(reopened, doc);
        editing::transform(&mut reopened, &[1], Transform::Rotate(-90.));
        assert_points(geometry(&reopened, &[1]), geometry(&source, &[1]));
    }
}

#[test]
fn rotation_center_does_not_change_reflection_or_explicit_pivot_semantics() {
    let original = pyrrole(17., Point::new(250., -130.));
    let ids = original.all_ids();
    let bounds_center = editing::center(&original, &ids);
    assert_ne!(
        Some(bounds_center),
        editing::rotation_center(&original, &ids)
    );
    for transform in [Transform::FlipHorizontal, Transform::FlipVertical] {
        let mut doc = original.clone();
        editing::transform(&mut doc, &ids, transform);
        for (atom, old) in doc.atoms.iter().zip(&original.atoms) {
            let expected = match transform {
                Transform::FlipHorizontal => {
                    Point::new(2. * bounds_center.x - old.position.x, old.position.y)
                }
                _ => Point::new(old.position.x, 2. * bounds_center.y - old.position.y),
            };
            assert_points(vec![atom.position], vec![expected]);
        }
        editing::transform(&mut doc, &ids, transform);
        assert_atoms(&doc, &original);
    }
    let mut doc = original.clone();
    let pivot = Point::new(-50., 70.);
    editing::transform_about(&mut doc, &ids, pivot, 1., 15.);
    assert_points(
        geometry(&doc, &ids),
        geometry(&original, &ids)
            .into_iter()
            .map(|p| rotated(p, pivot, 15.))
            .collect(),
    );
    assert_eq!(editing::rotation_center(&original, &[]), None);
    assert_eq!(editing::rotation_center(&original, &[u64::MAX]), None);
    let mut singleton = Document::default();
    let id = singleton.add_atom("C", Point::new(20., -40.));
    let before = singleton.clone();
    editing::transform(&mut singleton, &[id], Transform::Rotate(15.));
    assert_eq!(singleton, before);
}
