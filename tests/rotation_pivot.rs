use reshiki::{
    document::{Document, Point},
    editing::{self, Transform},
};

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
