use super::*;

fn ring() -> (Document, Model3d) {
    let mut doc = Document::default();
    let points = [
        [-2., -0.6],
        [-0.4, -1.1],
        [2., -0.4],
        [1.4, 0.9],
        [-1.3, 1.2],
    ];
    let spin = turn([1., 0., 0., 0.], 130., -65., false);
    let atoms: Vec<_> = points
        .into_iter()
        .map(|[x, y]| {
            let id = doc.add_atom("C", Point::new(x * 42., y * 42.));
            Placed {
                id,
                xyz: rotate(spin, [x, -y, 0.]),
            }
        })
        .collect();
    for i in 0..atoms.len() {
        doc.add_bond(atoms[i].id, atoms[(i + 1) % atoms.len()].id, 1, "plain");
    }
    let model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
    (doc, model)
}

fn correlation(model: &Model3d, doc: &Document) -> [f32; 2] {
    let center = heavy_center(&model.points(doc).unwrap());
    let n = model.atoms.len() as f32;
    let drawing = model
        .geometry_sources()
        .into_iter()
        .map(|id| doc.atom(id.unwrap()).unwrap().position)
        .fold(Point::default(), |sum, p| sum.offset(p.x / n, p.y / n));
    let q = model.initial_view(doc);
    model
        .atoms
        .iter()
        .zip(model.geometry_sources())
        .fold([0.; 2], |[x, y], (placed, source)| {
            let p = rotate(q, sub(placed.xyz, center));
            let at = doc.atom(source.unwrap()).unwrap().position;
            [x + (at.x - drawing.x) * p[0], y - (at.y - drawing.y) * p[1]]
        })
}

fn proper(q: [f32; 4]) {
    let [x, y, z] = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]].map(|p| rotate(q, p));
    let det = x[0] * (y[1] * z[2] - y[2] * z[1]) - x[1] * (y[0] * z[2] - y[2] * z[0])
        + x[2] * (y[0] * z[1] - y[1] * z[0]);
    assert!(
        (det - 1.).abs() < 1e-5,
        "The opening view must be a proper rotation: {det}"
    );
    assert!((q.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 1e-5);
}

#[test]
fn initial_face_follows_planar_drawing_in_each_mirror_layout() {
    let (original, model) = ring();
    for sx in [-1., 1.] {
        for sy in [-1., 1.] {
            let mut doc = original.clone();
            for atom in &mut doc.atoms {
                atom.position =
                    Point::new(sx * atom.position.x + 870., sy * atom.position.y - 320.);
            }
            let chemistry = (doc.atoms.clone(), doc.bonds.clone());
            let q = model.initial_view(&doc);
            proper(q);
            let scores = correlation(&model, &doc);
            assert!(scores.into_iter().all(|s| s > 0.), "{sx},{sy}: {scores:?}");
            assert_eq!((doc.atoms, doc.bonds), chemistry);
            assert_eq!(
                model.atoms,
                ring().1.atoms,
                "Choosing a face cannot change the conformer"
            );
        }
    }
}

#[test]
fn initial_face_keeps_tetrahedral_winding_chemical_stereo_and_saved_views() {
    use crate::document::AtomStereo;
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let f = doc.add_atom("F", Point::new(-42., -25.));
    let cl = doc.add_atom("Cl", Point::new(42., -25.));
    let br = doc.add_atom("Br", Point::new(0., 42.));
    for id in [f, cl, br] {
        doc.add_bond(c, id, 1, "plain");
    }
    let atom = doc.atom_mut(c).unwrap();
    atom.explicit_h = 1;
    atom.no_implicit = true;
    atom.stereo = Some(AtomStereo {
        neighbors: vec![f, cl, br],
        winding: "cw".into(),
    });
    let atoms = vec![
        Placed {
            id: c,
            xyz: [0.; 3],
        },
        Placed {
            id: f,
            xyz: [1., 1., 1.],
        },
        Placed {
            id: cl,
            xyz: [-1., -1., 1.],
        },
        Placed {
            id: br,
            xyz: [-1., 1., -1.],
        },
    ];
    let model = Model3d::new(
        &doc,
        atoms,
        vec![Hydrogen {
            parent: c,
            xyz: [1., -1., -1.],
        }],
        ForceField::Mmff94s,
    );
    let chemistry = (doc.atoms.clone(), doc.bonds.clone());
    let volume = |points: [[f32; 3]; 3]| {
        let [a, b, c] = points;
        a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0])
    };
    let vectors = [model.atoms[1].xyz, model.atoms[2].xyz, model.atoms[3].xyz];
    for flip in [1., -1.] {
        let mut mirrored = doc.clone();
        for atom in &mut mirrored.atoms {
            atom.position.x *= flip;
        }
        let q = model.initial_view(&mirrored);
        proper(q);
        assert!((volume(vectors.map(|p| rotate(q, p))) - volume(vectors)).abs() < 1e-4);
    }
    doc.models.push(model);
    doc.require_version(crate::document::MODELS_3D);
    doc.models[0].rotation = turn(doc.models[0].rotation, 37., -21., false);
    let stored = doc.models[0].rotation;
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(
        read.models[0].rotation, stored,
        "Loading must retain a saved custom view"
    );
    assert_eq!((read.atoms, read.bonds), chemistry);
}

#[test]
fn tied_drawing_and_crystal_cell_keep_their_original_orientation() {
    let (mut doc, model) = ring();
    for atom in &mut doc.atoms {
        atom.position = Point::new(100., 200.);
    }
    let center = heavy_center(&model.points(&doc).unwrap());
    let heavy: Vec<_> = model.atoms.iter().map(|p| sub(p.xyz, center)).collect();
    assert_eq!(model.initial_view(&doc), initial_rotation(&heavy));
    let crystal = model.with_cell(
        &doc,
        Some(Cell {
            vectors: [[4., 0., 0.], [0., 5., 0.], [0., 0., 6.]],
            crossing: vec![],
        }),
        vec![],
    );
    let opening = crystal.initial_view(&doc);
    for (i, atom) in doc.atoms.iter_mut().enumerate() {
        atom.position = Point::new(i as f32 * -200., i as f32 * 140.);
    }
    assert_eq!(
        crystal.initial_view(&doc),
        opening,
        "A crystal's cell orientation is independent of its drawing"
    );
}
