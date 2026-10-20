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
    let (lo, _) = frame(&moved, &moved.models[0]).unwrap();
    moved.translate(&atoms, 10., 5.);
    commit(&mut moved, before);
    assert_eq!(moved.models, doc.models);
    let (after, _) = frame(&moved, &moved.models[0]).unwrap();
    assert!((after.x - lo.x - 10.).abs() < 1e-3 && (after.y - lo.y - 5.).abs() < 1e-3);
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
fn a_kept_drawn_layout_is_pasted_with_its_model_and_restores() {
    let (mut doc, atoms, _) = shown(Display::Both);
    let original = doc.clone();
    doc.models[0].drawn = Some(Drawn::of(&doc, &doc.models[0]));
    for atom in &mut doc.atoms {
        atom.position = atom.position.offset(5., -3.);
        atom.depth = 2.;
    }
    doc.validate().unwrap();

    let part = editing::selection(&doc, &atoms);
    let mut pasted = doc.clone();
    let ids = editing::append(&mut pasted, &part, Point::new(0., 120.));
    let drawn = pasted.models[1].drawn.as_ref().unwrap();
    assert!(drawn.atoms.iter().all(|a| ids.contains(&a.0)));
    assert!(
        drawn
            .bonds
            .iter()
            .all(|b| ids.contains(&b.0) && ids.contains(&b.1))
    );
    pasted.validate().unwrap();

    assert!(restore_drawing(&mut doc, 0));
    assert_eq!(doc.atoms[..3], original.atoms[..3]);
    assert_eq!(doc.models[0].drawn, None);
    assert!(!restore_drawing(&mut doc, 0));
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
    let (flo, fhi) = frame(&doc, &doc.models[0]).unwrap();
    let center = Point::new((flo.x + fhi.x) / 2., (flo.y + fhi.y) / 2.);
    assert_eq!(doc.nearest(doc.atom(atoms[0]).unwrap().position, 5.), None);
    assert_eq!(editing::nearest_bond(&doc, Point::new(18., -10.), 5.), None);
    assert_eq!(frame_hit(&doc, center), Some(atoms.clone()));
    assert_eq!(frame_hit(&doc, fhi.offset(1., 0.)), None);
    let (lo, hi) = scene::selection_bounds(&doc, &atoms).unwrap();
    assert!(lo.x <= flo.x && hi.x >= fhi.x);
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
    assert!(shows_3d(&both, &atoms));
    assert!(!shows_3d(&drawing().0, &atoms));
    let (lo, _) = frame(&both, &both.models[0]).unwrap();
    let bond = both.drawing_style.bond_length_world;
    assert!((lo.x - (72. + bond)).abs() < 1e-3, "{lo:?}");
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

/// Give every atom of `doc`, one molecule, a model shown as `display`.
fn model_all(doc: &mut Document, display: Display) {
    let atoms = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| Placed {
            id: a.id,
            xyz: [i as f32, (i % 2) as f32, 0.1 * i as f32],
        })
        .collect();
    let model = Model3d::new(doc, atoms, vec![], ForceField::Mmff94s);
    doc.models.push(model);
    doc.require_version(MODELS_3D);
    set_display(doc, 0, display);
}

fn moved(doc: &Document, moves: &[(u64, Point)]) -> Document {
    let mut edited = doc.clone();
    let before = edited.clone();
    for (id, p) in moves {
        edited.atom_mut(*id).unwrap().position = *p;
    }
    commit(&mut edited, before);
    edited
}

#[test]
fn moving_atoms_that_flip_drawn_stereo_drops_the_model() {
    // (E)-1,2-difluoroethene.
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", Point::new(0., 0.));
    let c2 = doc.add_atom("C", Point::new(36., 0.));
    let f1 = doc.add_atom("F", Point::new(-18., -30.));
    let f2 = doc.add_atom("F", Point::new(54., 30.));
    doc.add_bond(c1, c2, 2, "plain");
    doc.add_bond(c1, f1, 1, "plain");
    doc.add_bond(c2, f2, 1, "plain");
    model_all(&mut doc, Display::Both);
    assert_eq!(moved(&doc, &[(f2, Point::new(60., 34.))]).models.len(), 1);
    assert!(
        moved(&doc, &[(f2, Point::new(54., -30.))])
            .models
            .is_empty()
    );

    // A wedged centre whose plain neighbours swap places is inverted.
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let f = doc.add_atom("F", Point::new(0., -36.));
    let cl = doc.add_atom("Cl", Point::new(31., 18.));
    let br = doc.add_atom("Br", Point::new(-31., 18.));
    doc.add_bond(c, f, 1, "wedge");
    doc.add_bond(c, cl, 1, "plain");
    doc.add_bond(c, br, 1, "plain");
    model_all(&mut doc, Display::Both);
    assert_eq!(moved(&doc, &[(cl, Point::new(35., 20.))]).models.len(), 1);
    let swapped = moved(
        &doc,
        &[(cl, Point::new(-31., 18.)), (br, Point::new(31., 18.))],
    );
    assert!(swapped.models.is_empty());
}

#[test]
fn bond_styles_and_label_display_are_presentation() {
    let (doc, atoms, _) = shown(Display::ThreeD);
    let edits: [&dyn Fn(&mut Document); 3] = [
        &|d| d.bonds[0].display = "bold".into(),
        &|d| d.bonds[1].display = "hashed".into(),
        &|d| d.atom_mut(atoms[2]).unwrap().display.hydrogens = Some(false),
    ];
    for edit in edits {
        let mut edited = doc.clone();
        let before = edited.clone();
        edit(&mut edited);
        commit(&mut edited, before);
        assert_eq!(edited.models, doc.models);
    }
}

#[test]
fn loading_drops_models_that_are_not_one_whole_molecule() {
    let (doc, atoms, methane) = shown(Display::ThreeD);
    let mut partial = doc.clone();
    partial.models[0].atoms.pop();
    partial.models[0].hydrogens.retain(|h| h.parent != atoms[2]);
    let mut spanning = doc.clone();
    spanning.models[0].atoms.push(Placed {
        id: methane,
        xyz: [3., 0., 0.],
    });
    for file in [partial, spanning] {
        let read = Document::from_json(&file.file_json().unwrap()).unwrap();
        assert!(read.models.is_empty());
    }
}

#[test]
fn hidden_ink_is_not_measured_or_targeted() {
    let (mut doc, atoms, _) = shown(Display::ThreeD);
    let bounds = scene::selection_bounds(&doc, &atoms);
    doc.atom_mut(atoms[0]).unwrap().display.number = Some(crate::atom_labels::Number {
        text: "1".into(),
        offset: Some(Point::new(0., 500.)),
        style: crate::atom_labels::number_style(),
    });
    doc.bonds[0].highlight = Some(crate::palette::Color::Custom([255, 220, 0]));
    assert_eq!(scene::selection_bounds(&doc, &atoms), bounds);
    for id in &atoms {
        let p = doc.atom(*id).unwrap().position;
        assert!(crate::arrow_anchors::pick(&doc, p, 5.).is_none());
    }

    // A ring shown in 3D has no 2D interior.
    let mut ring = Document::default();
    let ids = [(0., 0.), (36., 0.), (18., 31.)].map(|(x, y)| ring.add_atom("C", Point::new(x, y)));
    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
        ring.add_bond(ids[a], ids[b], 1, "plain");
    }
    let inside = Point::new(18., 10.);
    assert!(editing::ring_at(&ring, inside).is_some());
    model_all(&mut ring, Display::ThreeD);
    assert_eq!(editing::ring_at(&ring, inside), None);
}

#[test]
fn sphere_svg_writes_the_prototype_gradient() {
    let (doc, ..) = shown(Display::ThreeD);
    let svg = scene::svg(&doc);
    let radial = svg
        .split("<radialGradient ")
        .nth(1)
        .and_then(|s| s.split("</radialGradient>").next())
        .unwrap();
    assert!(radial.contains("gradientUnits=\"userSpaceOnUse\""));
    assert!(radial.contains(" fr=\""));
    assert_eq!(radial.matches("<stop ").count(), 3);
    assert!(radial.contains("offset=\"0.6\""));
    // Each bond half outlines only its two long sides.
    let quad = svg.split("<linearGradient ").nth(1).unwrap();
    let outline = quad.split("<path d=\"").nth(1).unwrap();
    assert_eq!(outline.split('"').next().unwrap().matches('M').count(), 2);
}

#[test]
fn chemistry_and_reaction_roles_keep_a_drawing_with_a_model_valid() {
    let (mut doc, atoms, methane) = shown(Display::Both);
    let molecule = crate::chemistry::document::prepare(&doc).unwrap();
    let drawing = crate::chemistry::document::for_drawing(&molecule, &doc).unwrap();
    let labels = drawing.labels().unwrap();
    let output = drawing.finish(labels).unwrap();
    assert_eq!(output.models, doc.models);
    assert!(output.version >= MODELS_3D);

    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(100., 40.),
        Point::new(180., 40.),
        Default::default(),
        Default::default(),
    ));
    let role = crate::reactions::Role::Reactant;
    crate::reactions::assign(&mut doc, arrow, &atoms[..1], role).unwrap();
    crate::reactions::assign(&mut doc, arrow, &[methane], crate::reactions::Role::Product).unwrap();
    assert_eq!(doc.version, VERSION);
    assert_eq!(doc.models.len(), 1);
}

#[test]
fn each_ball_covers_its_own_half_bonds_but_not_a_farther_atom() {
    let (mut doc, ..) = shown(Display::ThreeD);
    doc.models[0].rotation = [1., 0., 0., 0.];
    doc.models[0].hydrogens.clear();
    // C1 sits behind C2, whose half-bond to O points toward the viewer; O is
    // in front of both. C1 and its half are drawn first, behind everything.
    doc.models[0].atoms[0].xyz = [-1.2, 0., -1.5];
    doc.models[0].atoms[1].xyz = [0., 0., 0.];
    doc.models[0].atoms[2].xyz = [1.2, 0., 1.];
    let order: Vec<_> = primitives(&doc)
        .into_iter()
        .map(|p| match p {
            Primitive::Sphere { center, .. } => (true, center.x),
            Primitive::ShadedQuad { corners, .. } => (false, (corners[0].x + corners[3].x) / 2.),
            _ => unreachable!(),
        })
        .collect();
    let model = &doc.models[0];
    let points = model.points(&doc).unwrap();
    let view = View::new(&doc, model).unwrap();
    let x = |i: usize| view.project(view.rotated[i]).0.x;
    let sphere = |i| order.iter().position(|&(s, px)| s && px == x(i)).unwrap();
    let halves = |i| -> Vec<usize> {
        order
            .iter()
            .enumerate()
            .filter(|(_, (s, px))| !s && (*px - x(i)).abs() < 1e-3)
            .map(|(n, _)| n)
            .collect()
    };
    assert_eq!(points.len(), 3);
    // Every half-bond, including C2's half toward the nearer O, comes before
    // its own ball.
    for i in 0..3 {
        assert!(halves(i).iter().all(|&h| h < sphere(i)), "{i}: {order:?}");
    }
    // The halves of the front oxygen still pass in front of the far C1.
    assert!(halves(2).iter().all(|&h| h > sphere(0)), "{order:?}");
}

#[test]
fn the_frame_is_a_square_that_holds_the_model_in_every_rotation() {
    let (mut doc, atoms, _) = shown(Display::ThreeD);
    // A resized frame keeps its size while the model turns.
    resize(&mut doc, &atoms, Point::default(), 1.3);
    let (lo, hi) = frame(&doc, &doc.models[0]).unwrap();
    assert!(
        ((hi.x - lo.x) - (hi.y - lo.y)).abs() < 1e-3,
        "{lo:?} {hi:?}"
    );
    let mut rotation = doc.models[0].rotation;
    for step in 0..60 {
        rotation = turn(rotation, 13., -7. + step as f32 * 0.5, step % 5 == 0);
        doc.models[0].rotation = rotation;
        assert_eq!(frame(&doc, &doc.models[0]), Some((lo, hi)));
        for p in primitives(&doc) {
            if let Primitive::Sphere { center, radius, .. } = p {
                assert!(
                    center.x - radius >= lo.x
                        && center.y - radius >= lo.y
                        && center.x + radius <= hi.x
                        && center.y + radius <= hi.y,
                    "{step}: {center:?} {radius} outside {lo:?} {hi:?}"
                );
            }
        }
    }
    // Without perspective the square is as tight as the bounding sphere.
    doc.models[0].perspective = 0.;
    let (a, b) = frame(&doc, &doc.models[0]).unwrap();
    assert!(b.x - a.x < hi.x - lo.x);
    assert!((b.x - a.x - 0.88 * doc.models[0].size).abs() < 1e-2);
}

#[test]
fn a_drag_turns_like_the_prototype_and_shift_rolls() {
    let id = [1., 0., 0., 0.];
    let v = [0.3, -0.7, 1.1];
    // The prototype: R = rotY(dx·0.01) · rotX(dy·0.01) · R; rolls rotZ(−dx·0.01).
    let (sy, cy) = 0.1f32.sin_cos();
    let (sx, cx) = 0.2f32.sin_cos();
    let x = [v[0], cx * v[1] - sx * v[2], sx * v[1] + cx * v[2]];
    let expected = [cy * x[0] + sy * x[2], x[1], -sy * x[0] + cy * x[2]];
    let turned = rotate(turn(id, 10., 20., false), v);
    assert!(
        (0..3).all(|i| (turned[i] - expected[i]).abs() < 1e-5),
        "{turned:?}"
    );
    let (sz, cz) = (-0.1f32).sin_cos();
    let rolled = rotate(turn(id, 10., 20., true), v);
    let expected = [cz * v[0] - sz * v[1], sz * v[0] + cz * v[1], v[2]];
    assert!((0..3).all(|i| (rolled[i] - expected[i]).abs() < 1e-5));
    // Turns compose onto the current view and stay unit quaternions.
    let mut q = id;
    for _ in 0..500 {
        q = turn(q, 7., -3., false);
    }
    assert!((q.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 1e-4);
    // The opening view is what the model was created with.
    let (doc, ..) = drawing();
    assert_eq!(doc.models[0].initial_view(&doc), doc.models[0].rotation);
}

#[test]
fn a_corner_resize_scales_the_frame_about_the_pivot_and_keeps_the_drawing() {
    for display in [Display::ThreeD, Display::Both] {
        let (mut doc, atoms, methane) = shown(display);
        let before = doc.clone();
        let (lo, hi) = frame(&doc, &doc.models[0]).unwrap();
        let pivot = Point::new(lo.x - 10., lo.y - 4.);
        assert!(only_shown(&doc, &atoms) && !only_shown(&doc, &[atoms[0], methane]));
        resize(&mut doc, &atoms, pivot, 1.5);
        let (a, b) = frame(&doc, &doc.models[0]).unwrap();
        let scaled = |p: f32, q: f32| q + (p - q) * 1.5;
        for (got, want) in [
            (a.x, scaled(lo.x, pivot.x)),
            (a.y, scaled(lo.y, pivot.y)),
            (b.x, scaled(hi.x, pivot.x)),
            (b.y, scaled(hi.y, pivot.y)),
        ] {
            assert!((got - want).abs() < 1e-2, "{display:?} {got} {want}");
        }
        assert_eq!(doc.atoms, before.atoms);
        assert_eq!(doc.models[0].rotation, before.models[0].rotation);
        doc.validate().unwrap();
    }
    // A model shown as 2D has no frame to resize; sizes stay in range.
    let (mut doc, atoms, _) = drawing();
    let before = doc.clone();
    resize(&mut doc, &atoms, Point::default(), 3.);
    assert_eq!(doc, before);
    let (mut doc, atoms, _) = shown(Display::ThreeD);
    for _ in 0..20 {
        resize(&mut doc, &atoms, Point::default(), 20.);
    }
    assert_eq!(doc.models[0].size, 1e5);
    doc.validate().unwrap();
}

#[test]
fn the_morph_starts_flat_on_the_drawing_and_ends_on_the_model() {
    let (mut doc, atoms, _) = shown(Display::ThreeD);
    let settled = format!("{:?}", primitives(&doc));
    doc.models[0].morph = Some(1.);
    assert_eq!(format!("{:?}", primitives(&doc)), settled);

    doc.models[0].morph = Some(0.);
    let spheres: Vec<_> = primitives(&doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Sphere { center, .. } => Some((center, 1.)),
            Primitive::Opacity { alpha, primitive } => match *primitive {
                Primitive::Sphere { center, .. } => Some((center, alpha)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(spheres.len(), 5);
    // Every ball starts on a drawn atom; hydrogens start transparent, so
    // they never cover the atom they grow out of.
    for (center, _) in &spheres {
        assert!(
            atoms
                .iter()
                .any(|id| doc.atom(*id).unwrap().position.distance(*center) < 1e-3),
            "{center:?} is off the drawing"
        );
    }
    assert_eq!(spheres.iter().filter(|(_, alpha)| *alpha == 0.).count(), 2);
    // Never saved.
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(read.models[0].morph, None);
}

#[test]
fn hidden_indicator_owners_and_group_labels_are_out_of_reach() {
    use crate::abbreviations::label_hit;
    use crate::atom_labels::Owner;
    let (mut doc, atoms, methane) = drawing();
    doc.contract(&[atoms[2]], "OH", "HO").unwrap();
    let o = doc.atom(atoms[2]).unwrap().position;
    assert_eq!(label_hit(&doc, o, 1.), Some(atoms[2]));
    assert!(Owner::BondStereo(atoms[0], atoms[1]).drawn(&doc));
    assert!(set_display(&mut doc, 0, Display::ThreeD));
    assert_eq!(label_hit(&doc, o, 1.), None);
    assert!(!Owner::Number(atoms[0]).drawn(&doc));
    assert!(!Owner::BondStereo(atoms[0], atoms[1]).drawn(&doc));
    assert!(Owner::Number(methane).drawn(&doc));
}
