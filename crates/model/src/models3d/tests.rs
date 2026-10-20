use super::*;
use crate::document::{History, MODELS_3D, VERSION};
use crate::{editing, scene, transaction};

/// Ethanol (C–C–O) and a separate methane, with a fixed conformer for ethanol.
fn drawing() -> (Document, Vec<u64>, u64) {
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", Point::new(0., 0.));
    let c2 = doc.add_atom("C", Point::new(36., -21.));
    let o = doc.add_atom("O", Point::new(72., 0.));
    doc.add_bond(c1, c2, 1, "plain");
    doc.add_bond(c2, o, 1, "plain");
    let methane = doc.add_atom("C", Point::new(200., 0.));
    let atoms = vec![
        Placed {
            id: c1,
            xyz: [-1.2, -0.2, 0.1],
        },
        Placed {
            id: c2,
            xyz: [0.1, 0.5, -0.1],
        },
        Placed {
            id: o,
            xyz: [1.2, -0.3, 0.2],
        },
    ];
    let hydrogens = vec![
        Hydrogen {
            parent: c1,
            xyz: [-2.0, 0.4, 0.5],
        },
        Hydrogen {
            parent: o,
            xyz: [1.9, 0.3, 0.1],
        },
    ];
    let model = Model3d::new(&doc, atoms, hydrogens, ForceField::Mmff94s);
    doc.models.push(model);
    doc.require_version(MODELS_3D);
    (doc, vec![c1, c2, o], methane)
}

fn shown(display: Display) -> (Document, Vec<u64>, u64) {
    let (mut doc, atoms, methane) = drawing();
    assert!(set_display(&mut doc, 0, display));
    (doc, atoms, methane)
}

fn commit(doc: &mut Document, before: Document) {
    transaction::apply(doc, &mut History::default(), before, false).unwrap();
}

#[test]
fn native_files_round_trip_models_and_omit_the_key_without_one() {
    let (doc, ..) = shown(Display::Both);
    let bytes = doc.file_json().unwrap();
    let read = Document::from_json(&bytes).unwrap();
    assert_eq!(read.version, VERSION);
    assert_eq!(read.models, doc.models);
    assert_eq!(read.file_json().unwrap(), bytes);

    let mut plain = doc.clone();
    plain.models.clear();
    let json: serde_json::Value = serde_json::from_slice(&plain.file_json().unwrap()).unwrap();
    assert!(json.get("models").is_none());
    assert_eq!(json["version"], VERSION);
}

#[test]
fn validation_rejects_malformed_models_and_older_versions() {
    let (doc, atoms, methane) = drawing();
    doc.validate().unwrap();
    let broken: [&dyn Fn(&mut Document); 7] = [
        &|d| d.version = MODELS_3D - 1,
        &|d| d.models[0].rotation = [0.5, 0., 0., 0.],
        &|d| d.models[0].atoms[0].xyz[1] = f32::NAN,
        &|d| d.models[0].hydrogens[0].parent = methane,
        &|d| {
            let first = d.models[0].atoms[0];
            d.models[0].atoms.push(first);
        },
        &|d| d.models[0].size = 0.,
        &|d| d.models[0].depth_fade = 2.,
    ];
    for (i, edit) in broken.iter().enumerate() {
        let mut bad = doc.clone();
        edit(&mut bad);
        assert!(bad.validate().is_err(), "case {i}");
    }
    let mut twice = doc.clone();
    twice.models.push(twice.models[0].clone());
    assert!(twice.validate().is_err(), "an atom in two models");
    // Raw serde paths keep the version check.
    let mut old = serde_json::to_value(&doc).unwrap();
    old["version"] = (MODELS_3D - 1).into();
    assert!(Document::from_json(&serde_json::to_vec(&old).unwrap()).is_err());
    assert_eq!(atoms.len(), doc.models[0].atoms.len());
}

#[test]
fn deleting_an_atom_drops_the_model_but_other_edits_keep_it() {
    let (doc, atoms, methane) = shown(Display::ThreeD);
    let mut other = doc.clone();
    other.delete(&[methane]);
    assert_eq!(other.models, doc.models);
    let mut cut = doc.clone();
    cut.delete(&[atoms[2]]);
    assert!(cut.models.is_empty());
    cut.validate().unwrap();

    // Moving the molecule carries the frame with it.
    let mut moved = doc.clone();
    let before = moved.clone();
    let (center, _) = frame(&moved, &moved.models[0]).unwrap();
    moved.translate(&atoms, 10., 5.);
    commit(&mut moved, before);
    assert_eq!(moved.models, doc.models);
    let (after, _) = frame(&moved, &moved.models[0]).unwrap();
    assert_eq!(after, center.offset(10., 5.));
}

#[test]
fn chemical_edits_and_joined_components_drop_the_model() {
    let (doc, atoms, methane) = shown(Display::Both);
    let mut element = doc.clone();
    let before = element.clone();
    element.atom_mut(atoms[2]).unwrap().element = "N".into();
    commit(&mut element, before);
    assert!(element.models.is_empty());

    let mut joined = doc.clone();
    let before = joined.clone();
    joined.add_bond(atoms[2], methane, 1, "plain");
    commit(&mut joined, before);
    assert!(joined.models.is_empty());

    let mut order = doc.clone();
    let before = order.clone();
    order.bonds[0].order = 2;
    commit(&mut order, before);
    assert!(order.models.is_empty());

    // Presentation is not chemistry.
    let mut color = doc.clone();
    let before = color.clone();
    color.bonds[0].color = crate::palette::Color::Custom([200, 0, 0]);
    commit(&mut color, before);
    assert_eq!(color.models, doc.models);
}

#[test]
fn copy_and_paste_remap_the_model_with_its_molecule() {
    let (doc, atoms, methane) = shown(Display::ThreeD);
    let part = editing::selection(&doc, &atoms);
    assert_eq!(part.models, doc.models);
    assert!(editing::selection(&doc, &atoms[..2]).models.is_empty());
    assert!(editing::selection(&doc, &[methane]).models.is_empty());

    let mut pasted = doc.clone();
    let ids = editing::append(&mut pasted, &part, Point::new(0., 120.));
    assert_eq!(pasted.models.len(), 2);
    let copy = &pasted.models[1];
    assert!(copy.atoms.iter().all(|a| ids.contains(&a.id)));
    assert!(copy.hydrogens.iter().all(|h| copy.contains(h.parent)));
    assert_eq!(copy.rotation, doc.models[0].rotation);
    pasted.validate().unwrap();

    // A drawing that never had a model gains one at the model version.
    let mut fresh = Document::default();
    editing::append(&mut fresh, &part, Point::default());
    assert_eq!(fresh.version, MODELS_3D);
    fresh.validate().unwrap();
}

#[test]
fn initial_view_shows_the_prototype_face_as_a_proper_rotation() {
    let points = [
        [1.4, 0.2, -0.3],
        [-0.9, 1.1, 0.4],
        [0.3, -1.6, 0.2],
        [-0.8, 0.3, -0.3],
    ];
    let q = initial_rotation(&points);
    let norm: f32 = q.iter().map(|v| v * v).sum();
    assert!((norm - 1.).abs() < 1e-5);
    // The prototype's rows were [a0, −a1, a0×a1], a reflection; the third
    // row is now a0×(−a1), so the face shown before the tilt is the same.
    let axes = principal_axes(&points);
    let y = axes[1].map(|v| -v);
    for p in points {
        let r = rotate(q, p);
        let expected = prototype_tilt([dot3(axes[0], p), dot3(y, p), dot3(cross3(axes[0], y), p)]);
        assert!(r.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-4));
    }
    // Linear and one-atom molecules use fixed axes, deterministically.
    for degenerate in [&[[0., 0., 0.]][..], &[[-1., 0., 0.], [1., 0., 0.]], &[]] {
        let q = initial_rotation(degenerate);
        assert!(q.iter().all(|v| v.is_finite()));
        assert_eq!(q, initial_rotation(degenerate));
        assert!((q.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 1e-5);
    }
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
/// The prototype's power iteration (template.html `initialView`).
fn principal_axes(points: &[[f32; 3]]) -> [[f32; 3]; 2] {
    let mut c = [[0f32; 3]; 3];
    for p in points {
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] += p[i] * p[j];
            }
        }
    }
    let mut axes: Vec<[f32; 3]> = vec![];
    for start in [[1., 0.3, 0.2], [0.2, 1., 0.3]] {
        let mut v: [f32; 3] = start;
        for _ in 0..60 {
            v = c.map(|row| dot3(row, v));
            for a in &axes {
                let d = dot3(v, *a);
                v = [v[0] - d * a[0], v[1] - d * a[1], v[2] - d * a[2]];
            }
            let n = length(v);
            v = v.map(|x| x / n);
        }
        axes.push(v);
    }
    [axes[0], axes[1]]
}
fn prototype_tilt(p: [f32; 3]) -> [f32; 3] {
    let (sy, cy) = 0.3f32.sin_cos();
    let p = [cy * p[0] + sy * p[2], p[1], -sy * p[0] + cy * p[2]];
    let (sx, cx) = (-0.35f32).sin_cos();
    [p[0], cx * p[1] - sx * p[2], sx * p[1] + cx * p[2]]
}

#[test]
fn ball_and_stick_primitives_are_back_to_front_in_exact_colors() {
    let (doc, ..) = shown(Display::ThreeD);
    let parts = primitives(&doc);
    let spheres = parts
        .iter()
        .filter(|p| matches!(p, Primitive::Sphere { .. }))
        .count();
    let quads = parts
        .iter()
        .filter(|p| matches!(p, Primitive::ShadedQuad { .. }))
        .count();
    // Three atoms and two hydrogens; four bonds, each in two halves.
    assert_eq!((spheres, quads), (5, 8));
    assert_eq!(parts.len(), 13);
    // Oxygen keeps the prototype's flat-look stops, faded toward the paper.
    let model = &doc.models[0];
    let c = heavy_center(&model.points(&doc).unwrap());
    let z = rotate(model.rotation, sub(model.atoms[2].xyz, c))[2];
    let reach = reach(&model.points(&doc).unwrap(), c);
    let base = mix(
        [224., 72., 61.],
        WHITE,
        0.35 * ((reach - z) / (2. * reach)).clamp(0., 1.),
    );
    assert!(parts.iter().any(|p| matches!(p,
        Primitive::Sphere { stops, outline: Some(o), .. }
            if stops[1] == (0.6, rgb(base))
                && stops[0] == (0., rgb(mix(base, WHITE, 0.45)))
                && o.color == [30, 34, 38] && o.alpha == 0.85)));
}

#[test]
fn spheres_are_sorted_by_depth() {
    let (mut doc, atoms, _) = shown(Display::ThreeD);
    doc.models[0].rotation = [1., 0., 0., 0.];
    doc.models[0].hydrogens.clear();
    doc.models[0].atoms[0].xyz = [0., 0., 1.];
    doc.models[0].atoms[1].xyz = [1., 0., -1.];
    doc.models[0].atoms[2].xyz = [2., 0., 0.];
    let order: Vec<_> = primitives(&doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Sphere { center, .. } => Some(center.x),
            _ => None,
        })
        .collect();
    // Back (z = −1) to front (z = 1): x of atoms 1, 2, 0.
    assert!(
        order[0] > order[2] && order[1] > order[0],
        "{order:?} {atoms:?}"
    );
}

#[test]
fn a_3d_molecule_leaves_the_2d_pass_and_a_2d_model_changes_nothing() {
    let (doc, atoms, _) = shown(Display::ThreeD);
    let mut without = doc.clone();
    without.models.clear();
    let flat = |doc: &Document| -> Vec<String> {
        scene::primitives(doc)
            .iter()
            .filter(|p| !matches!(p, Primitive::Sphere { .. } | Primitive::ShadedQuad { .. }))
            .map(|p| format!("{p:?}"))
            .collect()
    };
    let mut removed = without.clone();
    removed.delete(&atoms);
    assert_eq!(flat(&doc), flat(&removed));
    assert!(scene::svg(&doc).contains("<radialGradient"));

    let (two_d, ..) = drawing();
    assert_eq!(scene::svg(&two_d), scene::svg(&without));
    assert_eq!(
        format!("{:?}", scene::primitives(&two_d)),
        format!("{:?}", scene::primitives(&without))
    );
}

#[test]
fn hidden_atoms_are_not_picked_and_the_frame_selects_the_molecule() {
    let (doc, atoms, methane) = shown(Display::ThreeD);
    assert!(!doc.atom_drawn(atoms[0]) && doc.atom_drawn(methane));
    let (center, size) = frame(&doc, &doc.models[0]).unwrap();
    assert_eq!(doc.nearest(doc.atom(atoms[0]).unwrap().position, 5.), None);
    assert_eq!(editing::nearest_bond(&doc, Point::new(18., -10.), 5.), None);
    assert_eq!(frame_hit(&doc, center), Some(atoms.clone()));
    assert_eq!(frame_hit(&doc, center.offset(size, 0.)), None);
    let (lo, hi) = scene::selection_bounds(&doc, &atoms).unwrap();
    assert!(lo.x <= center.x - size / 2. && hi.x >= center.x + size / 2.);
    let lasso = [
        lo.offset(-1., -1.),
        Point::new(hi.x + 1., lo.y - 1.),
        hi.offset(1., 1.),
        Point::new(lo.x - 1., hi.y + 1.),
    ];
    let region = crate::selection_region::objects(&doc, &lasso);
    assert!(atoms.iter().all(|id| region.contains(id)));
    assert!(shows_3d(&doc, &atoms) && !shows_3d(&doc, &[methane]));

    // Beside the drawing, the frame sits right of the atoms.
    let (both, ..) = shown(Display::Both);
    let (center, size) = frame(&both, &both.models[0]).unwrap();
    assert!(center.x - size / 2. > 72.);
    assert!(both.atom_drawn(atoms[0]));
}

/// Paint the stepped discs and a reference two-point radial gradient over a
/// pixel grid; they agree within a few levels.
#[test]
fn stepped_discs_match_a_radial_gradient() {
    let center = Point::new(50., 50.);
    let radius = 40.;
    let focal = center.offset(-radius * 0.38, -radius * 0.42);
    let stops = [
        (0., [240, 200, 190]),
        (0.6, [224, 72, 61]),
        (1., [184, 59, 50]),
    ];
    let discs = sphere_discs(center, radius, focal, &stops, 32);
    let mut worst = 0;
    let mut total = 0u64;
    let mut count = 0u64;
    for y in 0..100 {
        for x in 0..100 {
            let p = Point::new(x as f32 + 0.5, y as f32 + 0.5);
            if p.distance(center) > radius - 1. {
                continue;
            }
            // The last disc painted over p, the smallest one containing it.
            let painted = discs
                .iter()
                .rev()
                .find(|(c, r, _)| p.distance(*c) <= *r)
                .map(|d| d.2)
                .unwrap();
            // The gradient's t: the largest circle of the family through p.
            let (mut lo, mut hi) = (0f32, 1f32);
            for _ in 0..40 {
                let t = (lo + hi) / 2.;
                let r = radius * FOCAL_RADIUS + (radius - radius * FOCAL_RADIUS) * t;
                if p.distance(focal.lerp(center, t)) <= r {
                    hi = t;
                } else {
                    lo = t;
                }
            }
            let expected = gradient_color(&stops, hi);
            for (a, b) in painted.iter().zip(expected) {
                let d = a.abs_diff(b);
                worst = worst.max(d);
                total += u64::from(d);
                count += 1;
            }
        }
    }
    assert!(worst <= 6, "worst channel difference {worst}");
    assert!(total as f64 / (count as f64) < 2., "mean difference");
}
