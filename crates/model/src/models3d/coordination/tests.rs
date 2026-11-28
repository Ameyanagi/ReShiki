use super::*;
use crate::models3d::{ForceField, Placed};

fn system_with_ligands(bridging: bool, names: [&str; 2]) -> (Document, Model3d) {
    let mut doc = Document::default();
    let metals = [
        doc.add_atom("Ru", Point::new(-67., 0.)),
        doc.add_atom("Ru", Point::new(67., 0.)),
    ];
    let mut xyz = HashMap::new();
    xyz.insert(metals[0], [0., 0., -1.2]);
    xyz.insert(metals[1], [0., 0., 1.2]);
    let mut anchors = Vec::new();
    for ((side, metal), name) in [-1_f32, 1.].into_iter().zip(metals).zip(names) {
        let anchor = doc.add_atom("C", Point::new(side * 153., 0.));
        doc.add_bond(anchor, metal, 1, "plain");
        doc = crate::ligands::replace(&doc, anchor, name).unwrap();
        let ring = doc.atom(anchor).unwrap().centroid.clone();
        for (i, &id) in ring.iter().enumerate() {
            let angle = i as f32 * std::f32::consts::TAU / 5.;
            xyz.insert(id, [1.22 * angle.cos(), 1.22 * angle.sin(), side * 3.]);
            if name == "Cp*" {
                let methyl = doc
                    .bonds
                    .iter()
                    .find_map(|b| {
                        let other = if b.a == id {
                            b.b
                        } else if b.b == id {
                            b.a
                        } else {
                            return None;
                        };
                        (!ring.contains(&other) && other != anchor).then_some(other)
                    })
                    .unwrap();
                xyz.insert(methyl, [2.7 * angle.cos(), 2.7 * angle.sin(), side * 3.]);
            }
        }
        anchors.push(anchor);
    }
    doc.abbreviations.clear();
    for (i, p) in if bridging {
        vec![
            [0.9, 0.9, 0.],
            [-0.9, 0.9, 0.],
            [-0.9, -0.9, 0.],
            [0.9, -0.9, 0.],
        ]
    } else {
        vec![
            [1.5, 0., -0.6],
            [0., 1.2, 0.],
            [-1.5, 0., 0.6],
            [0., -1.2, 0.],
        ]
    }
    .into_iter()
    .enumerate()
    {
        let id = doc.add_atom("H", Point::new(i as f32, 0.));
        xyz.insert(id, p);
        for (index, metal) in metals.iter().enumerate() {
            if bridging || i % 2 == 1 || index == i / 2 {
                doc.add_bond(*metal, id, 1, "plain");
            }
        }
    }
    let placed = doc
        .atoms
        .iter()
        .filter_map(|atom| {
            Some(Placed {
                id: atom.id,
                xyz: *xyz.get(&atom.id)?,
            })
        })
        .collect();
    let mut model =
        Model3d::new(&doc, placed, vec![], ForceField::default()).with_cell(&doc, None, vec![]);
    model.linked_atoms.extend(anchors);
    doc.models.push(model.clone());
    crate::models3d::require_version(&mut doc);
    doc.validate().unwrap();
    (doc, model)
}

fn system(bridging: bool) -> (Document, Model3d) {
    system_with_ligands(bridging, ["Cp*"; 2])
}

fn check_uniform_projection(doc: &Document, model: &Model3d) {
    let scale = doc.drawing_style.bond_length_world / 1.54;
    let physical: HashMap<_, _> = super::super::viewed(doc, model).into_iter().collect();
    let diagram = recognize(doc, model).unwrap();
    for (i, a) in model.atoms.iter().enumerate() {
        for b in model.atoms.iter().skip(i + 1) {
            let (p, q) = (doc.atom(a.id).unwrap(), doc.atom(b.id).unwrap());
            let expected_delta = sub(physical[&a.id], physical[&b.id]);
            let expected = dot(expected_delta, expected_delta) * scale * scale;
            let projected = p.position.distance(q.position).powi(2) + (p.depth - q.depth).powi(2);
            assert!(
                (projected - expected).abs() < expected * 2e-5 + 0.002,
                "all coordinates must share the standard C--C factor: {} {} {projected} {expected}",
                a.id,
                b.id
            );
        }
    }
    for ligand in &diagram.ligands {
        for (&carbon, &methyl) in ligand.ring.iter().zip(&ligand.methyls) {
            let delta = sub(physical[&carbon], physical[&methyl]);
            let target = dot(delta, delta).sqrt() * scale;
            let actual = doc
                .atom(carbon)
                .unwrap()
                .position
                .distance(doc.atom(methyl).unwrap().position);
            assert!(
                actual <= target + 0.001,
                "orthographic shortening must be retained"
            );
        }
    }
}

#[test]
fn uniform_physical_projection_preserves_geometry_topology_and_document_style() {
    for bridging in [true, false] {
        let (mut doc, model) = system(bridging);
        let a = doc.add_atom("C", Point::new(1000., 1000.));
        let b = doc.add_atom("O", Point::new(1042., 1000.));
        doc.add_bond(a, b, 2, "plain");
        let before = doc.clone();
        assert!(can_draw_coordination_diagram(&doc, &model));
        assert!(lay_out_coordination_diagram(&mut doc, &model));
        doc.validate().unwrap();
        assert_eq!(doc.models, before.models);
        assert_eq!(doc.drawing_style, before.drawing_style);
        assert_eq!(doc.atom(a), before.atom(a));
        assert_eq!(doc.atom(b), before.atom(b));
        assert_eq!(doc.bonds.last(), before.bonds.last());
        assert_eq!(
            doc.bonds
                .iter()
                .map(|b| (b.a, b.b, b.order))
                .collect::<Vec<_>>(),
            before
                .bonds
                .iter()
                .map(|b| (b.a, b.b, b.order))
                .collect::<Vec<_>>()
        );
        assert!(doc.depth_appearance.is_empty());
        check_uniform_projection(&doc, &model);
    }
}

#[test]
fn rotated_and_reordered_inputs_keep_their_physical_identity_and_hydride_roles() {
    for bridging in [true, false] {
        for reverse in [false, true] {
            let (mut doc, mut model) = system(bridging);
            let rotation = crate::models3d::turn([1., 0., 0., 0.], 73., -42., false);
            for atom in &mut model.atoms {
                atom.xyz = super::super::rotate(rotation, atom.xyz);
            }
            if reverse {
                doc.atoms.reverse();
                doc.bonds.reverse();
                model.atoms.reverse();
                model.linked_atoms.reverse();
            }
            doc.models[0] = model.clone();
            let before = doc.clone();
            assert!(lay_out_coordination_diagram(&mut doc, &model));
            assert_eq!(doc.models, before.models);
            assert_eq!(
                doc.bonds
                    .iter()
                    .map(|b| (b.a, b.b, b.order))
                    .collect::<Vec<_>>(),
                before
                    .bonds
                    .iter()
                    .map(|b| (b.a, b.b, b.order))
                    .collect::<Vec<_>>()
            );
            check_uniform_projection(&doc, &model);
            let diagram = recognize(&doc, &model).unwrap();
            assert_eq!(diagram.bridges.len(), if bridging { 4 } else { 2 });
            assert_eq!(
                diagram.terminals.iter().map(Vec::len).sum::<usize>(),
                if bridging { 0 } else { 2 }
            );
        }
    }
}

#[test]
fn unsupported_connectivity_is_rejected_without_any_edit() {
    let (doc, model) = system(true);
    for kind in 0..4 {
        let mut invalid = doc.clone();
        match kind {
            0 => {
                invalid
                    .atoms
                    .iter_mut()
                    .find(|a| a.attachment.is_some())
                    .unwrap()
                    .centroid
                    .pop();
            }
            1 => {
                let h = invalid.atoms.iter().find(|a| a.element == "H").unwrap().id;
                invalid.bonds.retain(|b| b.a != h && b.b != h);
            }
            2 => {
                invalid
                    .atoms
                    .iter_mut()
                    .find(|a| a.element == "C")
                    .unwrap()
                    .element = "N".into();
            }
            3 => {
                invalid
                    .bonds
                    .iter_mut()
                    .find(|b| {
                        invalid
                            .atoms
                            .iter()
                            .any(|a| a.element == "H" && (a.id == b.a || a.id == b.b))
                    })
                    .unwrap()
                    .order = 2;
            }
            _ => unreachable!(),
        }
        let before = invalid.clone();
        assert!(!can_draw_coordination_diagram(&invalid, &model));
        assert!(!lay_out_coordination_diagram(&mut invalid, &model));
        assert_eq!(invalid, before);
    }
}

#[test]
fn cp_and_cp_star_use_custom_theme_scale_without_modifying_the_physical_frame() {
    for names in [["Cp"; 2], ["Cp*"; 2], ["Cp", "Cp*"]] {
        for bridging in [true, false] {
            for points in [14.4, 19.2, 28.8] {
                let (mut doc, model) = system_with_ligands(bridging, names);
                doc.drawing_style.set_bond_length(points);
                let before = doc.models.clone();
                assert!(lay_out_coordination_diagram(&mut doc, &model));
                assert_eq!(doc.models, before);
                check_uniform_projection(&doc, &model);
                doc.validate().unwrap();
            }
        }
    }
}

#[test]
fn coincident_metals_are_rejected_without_partial_drawing_changes() {
    let (mut doc, mut model) = system(true);
    let diagram = recognize(&doc, &model).unwrap();
    for atom in &mut model.atoms {
        if diagram.metals.contains(&atom.id) {
            atom.xyz = [0.; 3];
        }
    }
    doc.models[0] = model.clone();
    let before = doc.clone();
    assert!(!lay_out_coordination_diagram(&mut doc, &model));
    assert_eq!(doc, before);
}
