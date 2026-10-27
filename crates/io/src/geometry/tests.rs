use super::*;
use crate::document::{Annotation, Point};

fn ethanol() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(100., 100.));
    doc.atom_mut(a).unwrap().id = 41;
    let a = doc.add_atom("C", Point::new(142., 100.));
    doc.atom_mut(a).unwrap().id = 3;
    let a = doc.add_atom("O", Point::new(184., 72.));
    doc.atom_mut(a).unwrap().id = 89;
    doc.add_bond(41, 3, 1, "plain");
    doc.add_bond(3, 89, 1, "plain");
    doc
}
fn conformer(prepared: &Prepared) -> Conformer {
    Conformer {
        positions: prepared.drawing_positions.clone(),
        original_atom_count: prepared.ids.len(),
        hydrogen_parents: vec![],
    }
}

fn carbon_chain(atoms: usize) -> Document {
    let mut doc = Document::default();
    let ids: Vec<_> = (0..atoms)
        .map(|n| doc.add_atom("C", Point::new(n as f32 * 42., 0.)))
        .collect();
    for edge in ids.windows(2) {
        doc.add_bond(edge[0], edge[1], 1, "plain");
    }
    doc
}

#[test]
fn machine_capacity_changes_admission_without_changing_the_document() {
    use reshiki_process_heap::policy::{Capabilities, MIB};
    let limits = |memory| {
        let budget = Capabilities {
            memory_headroom_bytes: memory,
            lookup_operations_per_second: None,
        }
        .resolve();
        Limits {
            timeout: budget.geometry_timeout(),
            heap_bytes: budget.heap_bytes(),
        }
    };
    let low = limits(Some(256 * MIB as u64));
    let fallback = limits(None);
    let high = limits(Some(8 * 1024 * MIB as u64));
    assert_eq!(low.capacity().atoms, 128);
    assert_eq!(fallback.capacity().atoms, 512);
    assert_eq!(high.capacity().atoms, 640);
    for (limit, n, accepted) in [
        (low, 128, true),
        (low, 129, false),
        (fallback, 511, true),
        (fallback, 512, true),
        (fallback, 513, false),
        (high, 513, true),
        (high, 640, true),
        (high, 641, false),
    ] {
        let source = carbon_chain(n);
        let original = source.clone();
        let result = Prepared::with_limits(&source, &[], limit);
        assert_eq!(result.is_ok(), accepted, "{n} atoms, {limit:?}");
        if !accepted {
            assert!(matches!(
                result,
                Err(Error::Capacity {
                    kind: "original atoms",
                    ..
                })
            ));
        }
        assert_eq!(source, original);
    }
}

#[test]
fn generation_offers_retained_depth_in_physical_units_without_mutating_the_source() {
    let mut source = ethanol();
    source.atom_mut(89).unwrap().depth = 17.;
    let original = source.clone();
    let prepared = Prepared::new(&source, &[3]).unwrap();
    let request = prepared
        .native_request(
            ForceField::Mmff94s,
            reshiki_geometry::Operation::Generate,
            None,
            &[],
            500,
        )
        .unwrap();
    assert_eq!(request.coordinates.len(), 3);
    for (coordinates, id) in request.coordinates.iter().zip(prepared.ids()) {
        let atom = source.atom(*id).unwrap();
        assert_eq!(
            *coordinates,
            [
                f64::from(atom.position.x) / prepared.units,
                -f64::from(atom.position.y) / prepared.units,
                f64::from(atom.depth) / prepared.units,
            ]
        );
    }
    assert!(request.fixed_atoms.is_empty());
    assert_eq!(source, original);

    // A flat drawing translated in depth still needs a generated conformer.
    for atom in &mut source.atoms {
        atom.depth = 17.;
    }
    let prepared = Prepared::new(&source, &[]).unwrap();
    let request = prepared
        .native_request(
            ForceField::Mmff94s,
            reshiki_geometry::Operation::Generate,
            None,
            &[],
            500,
        )
        .unwrap();
    assert!(request.coordinates.is_empty());
}
fn identity(doc: &Document) -> String {
    let molecule = molecular::prepare(doc).unwrap();
    crate::chemistry::smiles::write::write(&molecule.state, Default::default())
        .unwrap()
        .text
}

#[test]
fn stable_order_full_component_and_target_only_patch_keep_unrelated_drawing_exact() {
    let mut source = ethanol();
    let unrelated = source.add_atom("*", Point::new(-100., 500.));
    source.annotations.push(Annotation {
        id: source.next_id(),
        position: Point::new(10., -90.),
        text: "source caption".into(),
        format: Default::default(),
    });
    let prepared = Prepared::new(&source, &[3]).unwrap();
    assert_eq!(prepared.ids(), [41, 3, 89]);
    assert_eq!(prepared.index(89), Some(2));
    let original = source.clone();
    let mut conf = conformer(&prepared);
    conf.positions.push(Point3 {
        x: 0.,
        y: 0.,
        z: 1.,
    });
    conf.hydrogen_parents.push(0);
    let mut frame = prepared.view_frame(&conf).unwrap();
    frame.rotate(43., -61.);
    let projected = prepared.document(&conf, &frame).unwrap();
    assert_eq!(source, original);
    assert_eq!(projected.atoms.len(), source.atoms.len());
    assert_eq!(projected.bonds, source.bonds);
    assert_eq!(projected.annotations, source.annotations);
    assert_eq!(projected.atom(unrelated), source.atom(unrelated));
    let request = prepared
        .native_request(
            ForceField::Mmff94,
            reshiki_geometry::Operation::Relax,
            Some(&conf),
            &[Pin {
                atom: 1,
                position: Point3 {
                    x: 1.,
                    y: 2.,
                    z: 3.,
                },
            }],
            20,
        )
        .unwrap();
    assert_eq!(request.bonds[0].a, 0);
    assert_eq!(request.bonds[0].b, 1);
    assert_eq!(request.bonds[1].a, 1);
    assert_eq!(request.bonds[1].b, 2);
    assert_eq!(request.fixed_atoms, [1]);
    assert_eq!(request.coordinates[1], [1., 2., 3.]);
    assert_eq!(
        request
            .atoms
            .iter()
            .map(|a| a.atomic_number)
            .collect::<Vec<_>>(),
        [6, 6, 8]
    );
}

#[test]
fn energy_evaluation_applies_pin_coordinates_without_fixed_degrees_of_freedom() {
    let source = ethanol();
    let prepared = Prepared::new(&source, &[3]).unwrap();
    let conf = conformer(&prepared);
    let before = conf.positions.clone();
    let pins = [Pin {
        atom: 1,
        position: Point3 {
            x: 1.2,
            y: -0.7,
            z: 0.5,
        },
    }];
    assert_ne!(conf.positions[1], pins[0].position);
    for field in [ForceField::Mmff94, ForceField::Mmff94s, ForceField::Uff] {
        let relax = prepared
            .native_request(
                field,
                reshiki_geometry::Operation::Relax,
                Some(&conf),
                &pins,
                20,
            )
            .unwrap();
        let evaluate = prepared
            .native_request(
                field,
                reshiki_geometry::Operation::Evaluate,
                Some(&conf),
                &pins,
                1,
            )
            .expect("Evaluation applies pin coordinates without relaxation constraints");
        assert_eq!(relax.fixed_atoms, [1]);
        assert!(evaluate.fixed_atoms.is_empty());
        assert_eq!(evaluate.coordinates, relax.coordinates);
        assert_eq!(evaluate.coordinates[1], [1.2, -0.7, 0.5]);
        for (index, position) in before.iter().enumerate().filter(|(index, _)| *index != 1) {
            assert_eq!(
                evaluate.coordinates[index],
                [position.x, position.y, position.z]
            );
        }
        evaluate.validate().unwrap();
        // Cross the same serialization boundary used by Client::evaluate.
        let frame = wire::encode(
            &wire::Request {
                heap_bytes: Limits::default().heap_bytes,
                capacity: None,
                operation: evaluate,
            },
            wire::MAX_REQUEST_BYTES,
        )
        .unwrap();
        let decoded: wire::Request = wire::decode(&frame, wire::MAX_REQUEST_BYTES).unwrap();
        decoded.operation.validate().unwrap();
        assert!(decoded.operation.fixed_atoms.is_empty());
        assert_eq!(decoded.operation.coordinates, relax.coordinates);
    }
    assert_eq!(conf.positions, before);
    assert_eq!(prepared.source(), &source);
}

#[test]
fn energy_evaluation_still_rejects_invalid_duplicate_and_absent_pins() {
    let prepared = Prepared::new(&ethanol(), &[3]).unwrap();
    let conf = conformer(&prepared);
    let valid = Pin {
        atom: 1,
        position: Point3::default(),
    };
    for pins in [
        vec![valid.clone(), valid.clone()],
        vec![Pin {
            atom: conf.positions.len(),
            position: Point3::default(),
        }],
        vec![Pin {
            atom: 1,
            position: Point3 {
                x: f64::NAN,
                y: 0.,
                z: 0.,
            },
        }],
        vec![Pin {
            atom: 1,
            position: Point3 {
                x: 1_000_001.,
                y: 0.,
                z: 0.,
            },
        }],
    ] {
        for operation in [
            reshiki_geometry::Operation::Relax,
            reshiki_geometry::Operation::Evaluate,
        ] {
            assert!(matches!(
                prepared.native_request(ForceField::Mmff94s, operation, Some(&conf), &pins, 20),
                Err(Error::Coordinates(_))
            ));
        }
    }
}

#[test]
fn physical_scale_tracks_style_and_retained_y_depth() {
    for length in [14.4, 28.8] {
        let mut source = ethanol();
        source.drawing_style.set_bond_length(length);
        source.atoms[1].position = source.atoms[0]
            .position
            .offset(source.drawing_style.bond_length_world, 0.);
        source.atoms[2].depth = 18.;
        let prepared = Prepared::new(&source, &[]).unwrap();
        assert!(
            (prepared.drawing_positions[1].x - prepared.drawing_positions[0].x - 1.5).abs() < 1e-6
        );
        assert!(prepared.drawing_positions[2].y > prepared.drawing_positions[0].y);
        let conf = conformer(&prepared);
        let frame = prepared.view_frame(&conf).unwrap();
        let projected = prepared.document(&conf, &frame).unwrap();
        for (a, b) in projected.atoms.iter().zip(&source.atoms) {
            assert!(a.position.distance(b.position) < 0.001);
            assert!((a.depth - b.depth).abs() < 0.001);
        }
    }
}

#[test]
fn ambiguous_disconnected_query_and_coordination_fail_without_mutation() {
    let mut source = ethanol();
    let other = source.add_atom("C", Point::new(400., 400.));
    assert!(matches!(
        Prepared::new(&source, &[]),
        Err(Error::Selection(_))
    ));
    assert!(matches!(
        Prepared::new(&source, &[3, other]),
        Err(Error::Selection(_))
    ));
    let before = source.clone();
    source.atom_mut(other).unwrap().element = "*".into();
    assert!(matches!(
        Prepared::new(&source, &[other]),
        Err(Error::Unsupported(_))
    ));
    source = before;
    source.add_bond(89, other, 5, "plain");
    let before = source.clone();
    assert!(matches!(
        Prepared::new(&source, &[3]),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(source, before);
}

#[test]
fn wedge_only_tetrahedral_winding_survives_edge_on_and_full_view_rotations() {
    let mut source = Document::default();
    let c = source.add_atom("C", Point::new(100., 100.));
    for (element, p, display) in [
        ("F", Point::new(142., 100.), "wedge"),
        ("Cl", Point::new(80., 64.), "plain"),
        ("Br", Point::new(80., 136.), "plain"),
        ("H", Point::new(100., 142.), "plain"),
    ] {
        let a = source.add_atom(element, p);
        source.add_bond(c, a, 1, display);
    }
    assert!(source.atom(c).unwrap().stereo.is_none());
    let expected = identity(&source);
    let prepared = Prepared::new(&source, &[c]).unwrap();
    let conf = Conformer {
        positions: vec![
            Point3::default(),
            Point3 {
                x: 1.,
                y: 1.,
                z: 1.,
            },
            Point3 {
                x: 1.,
                y: -1.,
                z: -1.,
            },
            Point3 {
                x: -1.,
                y: 1.,
                z: -1.,
            },
            Point3 {
                x: -1.,
                y: -1.,
                z: 1.,
            },
        ],
        original_atom_count: 5,
        hydrogen_parents: vec![],
    };
    let mut frame = prepared.view_frame(&conf).unwrap();
    for angle in [90., 90., 90., 90., 37., -37.] {
        frame.rotate(angle, 0.);
        frame.roll(30.);
        let projected = prepared.document(&conf, &frame).unwrap();
        assert!(projected.atom(c).unwrap().stereo.is_some());
        assert_eq!(identity(&projected), expected);
        assert_eq!(projected.bonds, source.bonds);
    }
}

#[test]
fn defined_unknown_and_unspecified_alkenes_survive_degenerate_projection_and_native_save() {
    for mode in [0, 1, 2] {
        let mut source = Document::default();
        let ids = [
            source.add_atom("F", Point::new(0., 0.)),
            source.add_atom("C", Point::new(42., 42.)),
            source.add_atom("C", Point::new(84., 42.)),
            source.add_atom("F", Point::new(126., 0.)),
        ];
        source.add_bond(ids[0], ids[1], 1, "plain");
        source.add_bond(ids[1], ids[2], 2, if mode == 1 { "wavy" } else { "plain" });
        source.add_bond(ids[2], ids[3], 1, "plain");
        if mode == 0 {
            source.bonds[1].stereo_authoritative = true;
        }
        let prepared = Prepared::new(&source, &[ids[1], ids[2]]).unwrap();
        let expected = prepared.molecule.state.metadata.bonds[1].stereo;
        assert_eq!(expected == 0, mode == 0);
        assert_eq!(expected == 1, mode == 1);
        let conf = conformer(&prepared);
        let mut frame = prepared.view_frame(&conf).unwrap();
        for angle in [90., 90., 90., 90.] {
            frame.rotate(angle, 0.);
            let mut projected = prepared.document(&conf, &frame).unwrap();
            // Exact end-on geometry, rather than relying on float sin(pi/2).
            for atom in &mut projected.atoms {
                atom.position.y = 100.;
            }
            let state = molecular::prepare(&projected).unwrap().state;
            assert_eq!(state.metadata.bonds[1].stereo, expected);
            assert_eq!(
                state.metadata.bonds[1].stereo_atoms,
                prepared.molecule.state.metadata.bonds[1].stereo_atoms
            );
            assert!(projected.bonds[1].stereo_authoritative);
            if mode == 1 {
                assert_eq!(projected.bonds[1].stereo.as_deref(), Some("any"));
            }
            // Protected absence and explicit unknown are chemical semantics,
            // independent of whether a renderer paints a plain or wavy rail.
            projected.bonds[1].display = if mode == 0 { "wavy" } else { "plain" }.into();
            let saved = Document::from_json(&projected.file_json().unwrap()).unwrap();
            assert_eq!(
                molecular::prepare(&saved).unwrap().state.metadata.bonds[1].stereo,
                expected
            );
        }
    }
}

#[test]
fn authoritative_unspecified_bond_rejects_neighbor_direction_leak() {
    let mut doc = Document::default();
    let mut ids = Vec::new();
    for (i, (x, y)) in [
        (0., 0.),
        (42., 42.),
        (84., 42.),
        (126., 0.),
        (168., 0.),
        (210., 42.),
    ]
    .into_iter()
    .enumerate()
    {
        ids.push(doc.add_atom(if i == 0 || i == 5 { "F" } else { "C" }, Point::new(x, y)));
    }
    for (i, pair) in ids.windows(2).enumerate() {
        doc.add_bond(
            pair[0],
            pair[1],
            if i == 1 || i == 3 { 2 } else { 1 },
            "plain",
        );
    }
    doc.bonds[1].stereo_authoritative = true;
    let prepared = molecular::prepare(&doc).unwrap();
    assert_eq!(prepared.state.metadata.bonds[1].stereo, 0);
    assert!(prepared.state.metadata.bonds[3].stereo > 1);
    assert!(prepared.state.metadata.bonds[1].stereo_atoms.is_empty());
}

#[test]
fn a_model_keeps_drawing_ids_and_parents_its_force_field_hydrogens() {
    let mut doc = ethanol();
    let methane = doc.add_atom("C", Point::new(400., 100.));
    // Another molecule's model must not stop this one being prepared.
    let other = Prepared::new(&doc, &[methane]).unwrap();
    let mut placed = other.model(&optimized(&other, vec![0])).unwrap();
    placed.display = crate::models3d::Display::ThreeD;
    doc.models.push(placed);
    doc.require_version(crate::document::MODELS_3D);
    doc.atom_mut(41).unwrap().depth = 30.;

    let prepared = Prepared::new(&doc, &[3]).unwrap();
    let model = prepared.model(&optimized(&prepared, vec![0, 2])).unwrap();
    let ids = prepared.ids();
    assert_eq!(model.atoms.iter().map(|a| a.id).collect::<Vec<_>>(), ids);
    assert_eq!(
        model.hydrogens.iter().map(|h| h.parent).collect::<Vec<_>>(),
        [ids[0], ids[2]]
    );
    assert_eq!(model.atoms[1].xyz, [1.5, 0., 0.]);
    assert_eq!(model.force_field, crate::models3d::ForceField::Mmff94s);
    // The drawing keeps its 2D atoms; only the model holds the conformer.
    assert_eq!(prepared.source().atoms, doc.atoms);
    // Generation for a model embeds afresh instead of reusing drawn depth.
    let request = |prepared: &Prepared| {
        prepared
            .native_request(
                ForceField::default(),
                reshiki_geometry::Operation::Generate,
                None,
                &[],
                1,
            )
            .unwrap()
            .coordinates
    };
    assert!(!request(&prepared).is_empty());
    assert!(request(&prepared.clone().without_retained_depth()).is_empty());
}

/// Original atoms along x, then one hydrogen per entry of `parents`.
fn optimized(prepared: &Prepared, parents: Vec<usize>) -> Optimized {
    let n = prepared.ids().len();
    Optimized {
        conformer: Conformer {
            positions: (0..n + parents.len())
                .map(|i| Point3 {
                    x: i as f64 * 1.5,
                    y: 0.,
                    z: 0.,
                })
                .collect(),
            original_atom_count: n,
            hydrogen_parents: parents,
        },
        initial_energy: 1.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 1,
        force_field: ForceField::Mmff94s,
        diagnostics: vec![],
    }
}

#[test]
fn flattening_a_model_projects_its_view_at_the_drawing_centre_and_scale() {
    let doc = ethanol();
    let prepared = Prepared::new(&doc, &[3]).unwrap();
    let mut model = prepared.model(&optimized(&prepared, vec![])).unwrap();
    let units = doc.drawing_style.bond_length_world / 1.5;
    let center = |doc: &Document| {
        let n = doc.atoms.len() as f32;
        doc.atoms.iter().fold(Point::default(), |c, a| {
            c.offset(a.position.x / n, a.position.y / n)
        })
    };
    let quarter = std::f32::consts::FRAC_1_SQRT_2;
    // A quarter turn about the view axis stands the chain upright…
    model.rotation = [quarter, 0., 0., quarter];
    let flat = prepared.flattened(&model).unwrap();
    let c = center(&doc);
    for (i, id) in prepared.ids().iter().enumerate() {
        let atom = flat.atom(*id).unwrap();
        let along = (i as f32 - 1.) * 1.5 * units;
        assert!((atom.position.x - c.x).abs() < 1e-3);
        assert!((atom.position.y - (c.y - along)).abs() < 1e-3);
        assert!(atom.depth.abs() < 1e-3);
    }
    assert_eq!(flat.bonds, doc.bonds);
    // …and one about the vertical axis turns it into depth.
    model.rotation = [quarter, 0., quarter, 0.];
    let flat = prepared.flattened(&model).unwrap();
    for (i, id) in prepared.ids().iter().enumerate() {
        let depth = flat.atom(*id).unwrap().depth;
        assert!((depth + (i as f32 - 1.) * 1.5 * units).abs() < 1e-3);
    }
}

/// #409: links to what the prepared molecule leaves out (its name caption, an
/// arrow's condition text, another molecule's depth fade) never stop a model.
#[test]
fn links_outside_the_molecule_do_not_stop_a_model() {
    use crate::arrow_labels::{ArrowLabel, Side};
    use crate::arrows::{ArrowStyle, Preset};
    use crate::document::Arrow;
    fn text(doc: &mut Document, text: &str, at: Point) -> u64 {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            id,
            position: at,
            text: text.into(),
            format: Default::default(),
        });
        id
    }
    let named: fn(&mut Document) = |doc| {
        let annotation = text(doc, "ethanol", Point::new(120., 140.));
        doc.molecule_names
            .push(crate::molecule_names::MoleculeName {
                annotation,
                atoms: vec![41, 3, 89],
                smiles: "CCO".into(),
                text: "ethanol".into(),
            });
    };
    let labelled: fn(&mut Document) = |doc| {
        let arrow = doc.next_id();
        doc.arrows.push(Arrow::new(
            arrow,
            Point::new(300., 100.),
            Point::new(420., 100.),
            Preset::Forward,
            ArrowStyle::preset(Preset::Forward),
        ));
        let annotation = text(doc, "heat", Point::new(350., 80.));
        doc.arrow_labels.push(ArrowLabel {
            arrow,
            annotation,
            side: Side::Above,
            gap: None,
        });
    };
    let faded: fn(&mut Document) = |doc| {
        let methane = doc.add_atom("C", Point::new(400., 100.));
        let scope = crate::depth_appearance::Scope::alpha_only(doc, vec![methane], 0.5);
        doc.depth_appearance.push(scope);
    };
    let linked: fn(&mut Document) = |doc| {
        let chain: Vec<_> = (0..3)
            .map(|n| doc.add_atom("C", Point::new(400. + n as f32 * 42., 100.)))
            .collect();
        doc.add_bond(chain[0], chain[1], 1, "plain");
        doc.add_bond(chain[1], chain[2], 1, "plain");
        doc.generic.links.push(crate::generic::LinkNode {
            atom: chain[1],
            min: 1,
            max: 3,
        });
    };
    for link in [named, labelled, faded, linked] {
        let mut doc = ethanol();
        link(&mut doc);
        doc.validate().unwrap();
        let prepared = Prepared::new(&doc, &[3]).unwrap();
        let mut model = prepared.model(&optimized(&prepared, vec![])).unwrap();
        model.display = crate::models3d::Display::Both;
        let links = (
            doc.molecule_names.clone(),
            doc.arrow_labels.clone(),
            doc.depth_appearance.clone(),
            doc.generic.clone(),
        );
        doc.models.push(model);
        doc.require_version(crate::document::MODELS_3D);
        doc.validate().unwrap();
        assert_eq!(
            (
                doc.molecule_names,
                doc.arrow_labels,
                doc.depth_appearance,
                doc.generic
            ),
            links
        );
    }
    // A link node on the molecule itself is still refused.
    let mut doc = ethanol();
    doc.generic.links.push(crate::generic::LinkNode {
        atom: 3,
        min: 1,
        max: 3,
    });
    doc.validate().unwrap();
    assert!(matches!(
        Prepared::new(&doc, &[3]),
        Err(Error::Unsupported(_))
    ));
}
