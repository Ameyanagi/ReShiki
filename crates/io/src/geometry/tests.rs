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
fn geometry_reports_enhanced_stereo_without_dropping_sample_semantics() {
    let mut source = ethanol();
    source.atoms[0].stereo_group = Some(crate::stereo_groups::StereoGroup {
        kind: crate::stereo_groups::Kind::And,
        number: 1,
    });
    let before = source.clone();
    assert!(matches!(
        Prepared::new(&source, &[]),
        Err(Error::Unsupported("enhanced stereo groups"))
    ));
    assert_eq!(source, before);
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

#[test]
fn explicit_generation_seeds_preserve_field_and_reject_values_outside_sampling_range() {
    let prepared = Prepared::new(&ethanol(), &[]).unwrap();
    let default = prepared
        .native_request(
            ForceField::Mmff94s,
            reshiki_geometry::Operation::Generate,
            None,
            &[],
            500,
        )
        .unwrap();
    assert_eq!(default.seed, DEFAULT_SEED);
    assert_eq!(
        serde_json::to_value(
            prepared
                .generation_request(ForceField::Mmff94s, DEFAULT_SEED)
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(default).unwrap()
    );
    for field in [ForceField::Mmff94, ForceField::Mmff94s, ForceField::Uff] {
        for seed in [0, DEFAULT_SEED + 1, GENERATION_SEED_MAX] {
            let request = prepared.generation_request(field, seed).unwrap();
            assert_eq!(request.seed, seed);
            assert_eq!(request.field, field.native());
            assert_eq!(request.conformers, 8);
            assert!(request.coordinates.is_empty());
        }
        for seed in [-1, GENERATION_SEED_MAX + 1, i32::MAX] {
            assert!(matches!(
                prepared.generation_request(field, seed),
                Err(Error::Invalid(_))
            ));
        }
    }
}

#[test]
fn model_generation_updates_stale_uff_with_its_saved_seed() {
    let mut source = ethanol();
    let ids: Vec<_> = source.atoms.iter().map(|atom| atom.id).collect();
    let prepared = Prepared::new(&source, &ids).unwrap();
    let mut result = optimized(&prepared, vec![]);
    result.force_field = ForceField::Uff;
    let mut model = prepared.model(&result).unwrap();
    model.seed = 91_337;
    source.models.push(model);
    crate::models3d::require_version(&mut source);
    assert!(ModelGeneration::for_calculation(&source, &ids).is_none());

    source.atom_mut(89).unwrap().element = "N".into();
    assert!(source.models[0].is_stale(&source));
    let generation = ModelGeneration::for_calculation(&source, &ids).unwrap();
    assert_eq!(
        generation,
        ModelGeneration {
            target: Some(0),
            force_field: ForceField::Uff,
            seed: 91_337,
        }
    );
    assert_eq!(
        ModelGeneration::for_model(&source, 0, false),
        Some(generation)
    );
    let request = Prepared::new(&source, &ids)
        .unwrap()
        .without_retained_depth()
        .generation_request(generation.force_field, generation.seed)
        .unwrap();
    assert_eq!(request.field, reshiki_geometry::ForceField::UFF);
    assert_eq!(request.seed, 91_337);
}

#[test]
fn model_generation_keeps_new_models_and_regeneration_on_the_existing_seed_policy() {
    let mut source = ethanol();
    let ids: Vec<_> = source.atoms.iter().map(|atom| atom.id).collect();
    assert_eq!(
        ModelGeneration::for_calculation(&source, &ids),
        Some(ModelGeneration::default())
    );
    let prepared = Prepared::new(&source, &ids).unwrap();
    let mut model = prepared.model(&optimized(&prepared, vec![])).unwrap();
    model.seed = GENERATION_SEED_MAX - 2;
    source.models.push(model);
    let generation = ModelGeneration::for_model(&source, 0, true).unwrap();
    assert_eq!(generation.seed, 1_000_001);

    source.models[0].seed = 0;
    source.atom_mut(89).unwrap().element = "N".into();
    assert_eq!(
        ModelGeneration::for_calculation(&source, &ids)
            .unwrap()
            .seed,
        DEFAULT_SEED
    );
    source.models[0].file_geometry = true;
    assert!(ModelGeneration::for_calculation(&source, &ids).is_none());
    assert!(ModelGeneration::for_model(&source, 0, true).is_none());
}

#[test]
fn ensemble_request_encodes_fresh_generation_and_validates_limits() {
    let mut source = carbon_chain(4);
    source.atoms[0].depth = 17.;
    let prepared = Prepared::new(&source, &[]).unwrap();
    let field = ForceField::Mmff94s;
    let generation = prepared.generation_request(field, 42).unwrap();
    assert!(!generation.coordinates.is_empty());
    assert_eq!(generation.max_iterations, 500);
    for (mode, name) in [
        (Ensemble::Minimized, "Minimized"),
        (Ensemble::Embedded, "Embedded"),
    ] {
        let request = prepared.ensemble_request(field, 42, 16, mode).unwrap();
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["ensemble"], name);
        assert_eq!(request.operation, reshiki_geometry::Operation::Generate);
        assert_eq!(request.field, field.native());
        assert_eq!(request.seed, 42);
        assert_eq!(request.conformers, 16);
        assert_eq!(request.max_iterations, 2000);
        assert!(request.coordinates.is_empty());
    }
    for count in [0, reshiki_geometry::MAX_CONFORMERS + 1] {
        assert!(matches!(
            prepared.ensemble_request(field, 42, count, Ensemble::Minimized),
            Err(Error::Invalid(_))
        ));
    }
    let relaxed = Prepared::relaxed(&source, &[]).unwrap();
    assert!(matches!(
        relaxed.ensemble_request(field, 42, 16, Ensemble::Minimized),
        Err(Error::Invalid(_))
    ));
    let large = Prepared::with_limits(
        &carbon_chain(201),
        &[],
        Limits {
            heap_bytes: 256 * 1024 * 1024,
            timeout: std::time::Duration::from_secs(60),
        },
    )
    .unwrap();
    assert!(
        large
            .ensemble_request(field, 42, 31, Ensemble::Embedded)
            .is_ok()
    );
    assert!(matches!(
        large.ensemble_request(field, 42, 32, Ensemble::Embedded),
        Err(Error::Invalid(_))
    ));
}

fn undefined_tetrahedron(display: &str, reversed: bool) -> (Document, u64, HashMap<u64, Point3>) {
    let mut doc = Document::default();
    let centre = doc.add_atom("C", Point::default());
    doc.atom_mut(centre).unwrap().id = 41;
    let centre = 41;
    let mut positions = HashMap::from([(centre, Point3::default())]);
    for (index, (element, xyz)) in [
        ("F", [1., 1., 1.]),
        ("Cl", [1., -1., -1.]),
        ("Br", [-1., 1., -1.]),
        ("H", [-1., -1., 1.]),
    ]
    .into_iter()
    .enumerate()
    {
        let atom = doc.add_atom(element, Point::new(index as f32 * 42., 42.));
        let id = [3, 89, 17, 66][index];
        doc.atom_mut(atom).unwrap().id = id;
        let display = if index == 0 { display } else { "plain" };
        let (a, b) = if reversed { (id, centre) } else { (centre, id) };
        doc.add_bond(a, b, 1, display);
        positions.insert(
            id,
            Point3 {
                x: xyz[0],
                y: xyz[1],
                z: xyz[2],
            },
        );
    }
    // Native neighbour order is bond insertion order, not atom or drawing ID order.
    doc.bonds.swap(0, 1);
    (doc, centre, positions)
}

#[test]
fn undefined_stereo_pinning_uses_model_ids_and_native_neighbor_order() {
    for display in ["plain", "wavy"] {
        for reversed in [false, true] {
            let (source, centre, positions) = undefined_tetrahedron(display, reversed);
            let original = source.clone();
            let prepared = Prepared::new(&source, &[centre]).unwrap();
            let graph = &prepared.molecule.state.graph;
            let mut metadata = prepared.molecule.state.metadata.clone();
            for bond in &mut metadata.bonds {
                bond.unknown_stereo = false;
            }
            let expected = crate::chemistry::stereo::from_3d(
                graph,
                &metadata,
                &vec![crate::chemistry::kekulize::Direction::None; graph.bonds.len()],
                Some(&crate::chemistry::stereo::wedging::Conformer {
                    positions: prepared.ids.iter().map(|id| positions[id]).collect(),
                    is_3d: true,
                }),
                &crate::chemistry::stereo::SpatialAnnotations {
                    non_explicit: vec![None; prepared.ids.len()],
                    done: None,
                },
                crate::chemistry::stereo::SpatialOptions {
                    replace_existing: true,
                    allow_nontetrahedral: false,
                },
            )
            .unwrap()
            .metadata
            .atoms[prepared.index(centre).unwrap()]
            .chiral_tag;
            assert!(matches!(expected, 1 | 2));
            for invert in [false, true] {
                let mut pinned = prepared.clone();
                let materialized = pinned.materialized.clone();
                assert_eq!(
                    pinned.pin_undefined_stereo(&positions, invert).unwrap(),
                    StereoPinning {
                        pinned: vec![centre],
                        unpinned: vec![],
                    }
                );
                let request = pinned
                    .ensemble_request(ForceField::Uff, DEFAULT_SEED, 4, Ensemble::Embedded)
                    .unwrap();
                assert_eq!(
                    request.atoms[pinned.index(centre).unwrap()].chiral_tag,
                    u32::from(if invert { expected ^ 3 } else { expected })
                );
                assert_eq!(pinned.source, original);
                assert_eq!(pinned.materialized, materialized);
            }
            assert_eq!(source, original);
        }
    }
}

#[test]
fn undefined_stereo_pinning_preserves_defined_centers_and_is_transactional() {
    let (mut source, centre, mut positions) = undefined_tetrahedron("plain", false);
    let mut prepared = Prepared::new(&source, &[]).unwrap();
    let before = serde_json::to_value(&prepared.molecule.state.metadata).unwrap();
    let mut missing = positions.clone();
    missing.remove(&3);
    assert!(prepared.pin_undefined_stereo(&missing, false).is_err());
    for position in positions.values_mut() {
        position.z = 0.;
    }
    assert_eq!(
        prepared.pin_undefined_stereo(&positions, false).unwrap(),
        StereoPinning {
            pinned: vec![],
            unpinned: vec![centre],
        }
    );
    assert_eq!(
        serde_json::to_value(&prepared.molecule.state.metadata).unwrap(),
        before
    );

    source.atom_mut(centre).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors: vec![3, 89, 17, 66],
    });
    let mut defined = Prepared::new(&source, &[]).unwrap();
    let before = serde_json::to_value(&defined.molecule.state.metadata).unwrap();
    assert_eq!(
        defined.pin_undefined_stereo(&positions, true).unwrap(),
        StereoPinning::default()
    );
    assert_eq!(
        serde_json::to_value(&defined.molecule.state.metadata).unwrap(),
        before
    );
}

#[test]
fn undefined_stereo_pinning_keeps_the_readable_centre_when_another_is_planar() {
    let (mut source, centre, mut positions) = undefined_tetrahedron("plain", false);
    let planar = 66;
    source.atom_mut(planar).unwrap().element = "C".into();
    positions.get_mut(&planar).unwrap().z = 0.;
    for (element, [x, y]) in [("F", [-2., -1.]), ("Cl", [-1., -2.]), ("Br", [-2., -2.])] {
        let atom = source.add_atom(element, Point::new(x as f32 * 42., y as f32 * 42.));
        source.add_bond(planar, atom, 1, "plain");
        positions.insert(atom, Point3 { x, y, z: 0. });
    }
    let mut tags = Vec::new();
    for invert in [false, true] {
        let mut prepared = Prepared::new(&source, &[]).unwrap();
        assert_eq!(
            prepared.pin_undefined_stereo(&positions, invert).unwrap(),
            StereoPinning {
                pinned: vec![centre],
                unpinned: vec![planar],
            }
        );
        let request = prepared
            .ensemble_request(ForceField::Uff, DEFAULT_SEED, 4, Ensemble::Embedded)
            .unwrap();
        tags.push(request.atoms[prepared.index(centre).unwrap()].chiral_tag);
        assert_eq!(request.atoms[prepared.index(planar).unwrap()].chiral_tag, 0);
    }
    assert!(matches!(tags.as_slice(), [1, 2] | [2, 1]), "{tags:?}");
}

#[test]
fn undefined_stereo_pinning_skips_equivalent_ligands() {
    let (mut source, centre, positions) = undefined_tetrahedron("plain", false);
    source.atom_mut(3).unwrap().element = "Cl".into();
    source.atom_mut(3).unwrap().map_num = 7;
    source.atom_mut(89).unwrap().map_num = 9;
    let mut prepared = Prepared::new(&source, &[]).unwrap();
    assert_eq!(
        prepared.pin_undefined_stereo(&positions, false).unwrap(),
        StereoPinning::default()
    );
    assert_eq!(
        prepared.molecule.state.metadata.atoms[prepared.index(centre).unwrap()].chiral_tag,
        0
    );
}

#[tokio::test]
async fn undefined_stereo_pinning_retains_coupled_ring_metadata() {
    use crate::engine::{LocalEngine, Request};

    let doc = LocalEngine::in_process()
        .request(Request::import_smiles("C[C@H]1CC[C@H](C)CC1"))
        .await
        .unwrap()
        .document
        .unwrap();
    let mut prepared = Prepared::new(&doc, &[]).unwrap();
    prepared.molecule.state = crate::chemistry::stereo::perception::perceive(
        &prepared.molecule.state,
        crate::chemistry::stereo::perception::Options {
            clean: true,
            force: true,
            flag_possible: true,
        },
    )
    .unwrap();
    assert!(
        prepared
            .molecule
            .state
            .metadata
            .atoms
            .iter()
            .any(|atom| atom.ring_stereo)
    );
    // These ring centres are already defined, so no model coordinates are
    // needed. Their paired ring-membership annotations must survive inference.
    assert_eq!(
        prepared
            .pin_undefined_stereo(&HashMap::new(), false)
            .unwrap(),
        StereoPinning::default()
    );
}

#[test]
fn undefined_stereo_pinning_keeps_every_sample_in_the_model_or_inverted_configuration() {
    use crate::chemistry::{kekulize::Direction, stereo};

    let (source, centre, positions) = undefined_tetrahedron("wavy", true);
    for invert in [false, true] {
        let mut prepared = Prepared::new(&source, &[]).unwrap();
        prepared.pin_undefined_stereo(&positions, invert).unwrap();
        let request = prepared
            .ensemble_request(ForceField::Uff, DEFAULT_SEED, 8, Ensemble::Minimized)
            .unwrap();
        let expected =
            prepared.molecule.state.metadata.atoms[prepared.index(centre).unwrap()].chiral_tag;
        let sampled = reshiki_geometry::solve(&request).unwrap();
        assert_eq!(sampled.ensemble.len(), 8, "{:?}", sampled.diagnostics);
        for member in sampled.ensemble {
            let state = &prepared.molecule.state;
            let conformer = stereo::wedging::Conformer {
                positions: member.coordinates[..prepared.ids.len()]
                    .iter()
                    .map(|&[x, y, z]| Point3 { x, y, z })
                    .collect(),
                is_3d: true,
            };
            let mut metadata = state.metadata.clone();
            for bond in &mut metadata.bonds {
                bond.unknown_stereo = false;
            }
            let spatial = stereo::from_3d(
                &state.graph,
                &metadata,
                &vec![Direction::None; state.graph.bonds.len()],
                Some(&conformer),
                &stereo::SpatialAnnotations {
                    non_explicit: vec![None; prepared.ids.len()],
                    done: None,
                },
                stereo::SpatialOptions {
                    replace_existing: true,
                    allow_nontetrahedral: false,
                },
            )
            .unwrap();
            assert_eq!(
                spatial.metadata.atoms[prepared.index(centre).unwrap()].chiral_tag,
                expected
            );
        }
    }
}

#[tokio::test]
async fn ritonavir_ensemble_embeds_and_converges_all_starts() {
    use crate::engine::{LocalEngine, Request};

    let smiles = "CC(C)c1nc(CN(C)C(=O)N[C@@H](C(C)C)C(=O)N[C@@H](Cc2ccccc2)C[C@H](O)[C@H](Cc2ccccc2)NC(=O)OCc2cncs2)cs1";
    let doc = LocalEngine::in_process()
        .request(Request::import_smiles(smiles))
        .await
        .unwrap()
        .document
        .unwrap();
    let prepared = Prepared::new(&doc, &[]).unwrap();
    let starts = 12;
    let request = prepared
        .ensemble_request(
            ForceField::Mmff94s,
            DEFAULT_SEED,
            starts,
            Ensemble::Minimized,
        )
        .unwrap();
    let result = reshiki_geometry::solve(&request).unwrap();
    let converged = result
        .ensemble
        .iter()
        .filter(|member| member.converged)
        .count();
    assert_eq!(
        (result.ensemble.len(), converged),
        (starts as usize, starts as usize),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn butane_ensemble_round_trips_through_the_worker_frame() {
    let prepared = Prepared::new(&carbon_chain(4), &[]).unwrap();
    let operation = prepared
        .ensemble_request(ForceField::Mmff94s, DEFAULT_SEED, 16, Ensemble::Minimized)
        .unwrap();
    let request = wire::encode(
        &wire::Request {
            heap_bytes: 64 * 1024 * 1024,
            capacity: None,
            operation,
        },
        wire::MAX_REQUEST_BYTES,
    )
    .unwrap();
    let bytes = worker::respond(&request).unwrap();
    let response: wire::Response = wire::decode(&bytes, wire::MAX_RESPONSE_BYTES).unwrap();
    assert_eq!(response.version, reshiki_geometry::RDKIT_VERSION);
    let response = response.result.unwrap();
    assert!(response.ensemble.len() > 1);
    assert!(
        response
            .diagnostics
            .iter()
            .any(|line| line.contains("iteration_limit=2000"))
    );
    assert_eq!(response.original_count, 4);
    assert_eq!(response.hydrogen_parents.len(), 10);
    assert!(response.hydrogen_parents.iter().all(|&parent| parent < 4));
    assert_eq!(response.coordinates.len(), 14);
    for member in &response.ensemble {
        assert_eq!(member.coordinates.len(), response.coordinates.len());
        assert!(member.energy.is_finite());
    }
    assert_eq!(
        response.energy,
        response
            .ensemble
            .iter()
            .map(|member| member.energy)
            .fold(f64::INFINITY, f64::min)
    );
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
        angle: 0.,
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
            (prepared.drawing_positions[1].x - prepared.drawing_positions[0].x - 1.54).abs() < 1e-6
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
fn custom_theme_physical_seed_optimize_frame_and_pin_inverse_share_the_same_units() {
    for points in [14.4, 18., 28.8] {
        let mut source = ethanol();
        source.drawing_style.set_bond_length(points);
        let length = source.drawing_style.bond_length_world;
        source.atoms[0].position = Point::new(173., -81.);
        source.atoms[1].position = source.atoms[0].position.offset(length, 0.);
        source.atoms[2].position = source.atoms[1].position.offset(length * 0.8, length * 0.6);
        source.atoms[0].depth = 9.;
        source.atoms[1].depth = 19.;
        source.atoms[2].depth = 14.;
        let before = source.clone();
        let prepared = Prepared::new(&source, &[]).unwrap();
        assert!(
            (prepared.drawing_positions[1].x - prepared.drawing_positions[0].x - 1.54).abs() < 1e-6
        );
        let conf = conformer(&prepared);
        let generation = prepared
            .generation_request(ForceField::Mmff94s, DEFAULT_SEED)
            .unwrap();
        assert_eq!(generation.coordinates.len(), 3);
        for (coordinates, id) in generation.coordinates.iter().zip(prepared.ids()) {
            let atom = source.atom(*id).unwrap();
            let physical = [atom.position.x, -atom.position.y, atom.depth];
            for (&coordinate, world) in coordinates.iter().zip(physical) {
                assert!((coordinate - f64::from(world) * 1.54 / f64::from(length)).abs() < 1e-5);
            }
        }
        // Optimization starts fitted to the authored frame, preserving its
        // translated XY and depth centre while using the physical scale.
        let mut frame = prepared.view_frame(&conf).unwrap();
        assert!((frame.units_per_angstrom - f64::from(length) / 1.54).abs() < 1e-5);
        let initial = prepared.document(&conf, &frame).unwrap();
        for (atom, authored) in initial.atoms.iter().zip(&source.atoms) {
            assert!(atom.position.distance(authored.position) < 1e-3);
            assert!((atom.depth - authored.depth).abs() < 1e-3);
        }
        frame.rotate(23., -41.);
        frame.roll(17.);
        let projected = prepared.document(&conf, &frame).unwrap();
        let pins: Vec<_> = prepared
            .ids()
            .iter()
            .enumerate()
            .map(|(atom, id)| {
                let drawn = projected.atom(*id).unwrap();
                Pin {
                    atom,
                    position: frame.unproject(drawn.position, drawn.depth),
                }
            })
            .collect();
        let relax = prepared
            .native_request(
                ForceField::Mmff94s,
                reshiki_geometry::Operation::Relax,
                Some(&conf),
                &pins,
                20,
            )
            .unwrap();
        for (coordinates, physical) in relax.coordinates.iter().zip(&conf.positions) {
            assert!((coordinates[0] - physical.x).abs() < 1e-6);
            assert!((coordinates[1] - physical.y).abs() < 1e-6);
            assert!((coordinates[2] - physical.z).abs() < 1e-6);
        }
        assert_eq!(projected.bonds, source.bonds);
        assert_eq!(source, before);
    }
}

#[test]
fn physical_model_flattening_preserves_explicit_camera_at_default_and_custom_theme_scale() {
    let quarter = std::f32::consts::FRAC_1_SQRT_2;
    for points in [14.4, 18., 28.8] {
        let mut source = carbon_chain(2);
        source.drawing_style.set_bond_length(points);
        let length = source.drawing_style.bond_length_world;
        source.atoms[0].position = Point::new(173., -81.);
        source.atoms[1].position = source.atoms[0].position.offset(length, 0.);
        let prepared = Prepared::new(&source, &[]).unwrap();
        let mut result = optimized(&prepared, vec![]);
        result.conformer.positions[1].x = 1.54;
        let mut model = prepared.model(&result).unwrap();
        let geometry = model.atoms.clone();
        let center = Point::new(173. + length / 2., -81.);
        for (rotation, expected) in [
            ([1., 0., 0., 0.], [length, 0., 0.]),
            ([quarter, 0., 0., quarter], [0., -length, 0.]),
            ([quarter, 0., quarter, 0.], [0., 0., -length]),
            (
                [3_f32.sqrt() / 2., 0., 0.5, 0.],
                [length / 2., 0., -length * 3_f32.sqrt() / 2.],
            ),
        ] {
            model.rotation = rotation;
            let flat = prepared.flattened(&model).unwrap();
            let a = flat.atom(prepared.ids()[0]).unwrap();
            let b = flat.atom(prepared.ids()[1]).unwrap();
            let vector = [
                b.position.x - a.position.x,
                b.position.y - a.position.y,
                b.depth - a.depth,
            ];
            for (actual, expected) in vector.into_iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 2e-4,
                    "{rotation:?}: {actual} != {expected}"
                );
            }
            let distance = vector.into_iter().map(|v| v * v).sum::<f32>().sqrt();
            assert!((distance - length).abs() < 2e-4);
            assert!((a.position.x + b.position.x - 2. * center.x).abs() < 2e-4);
            assert!((a.position.y + b.position.y - 2. * center.y).abs() < 2e-4);
            assert!((a.depth + b.depth).abs() < 2e-4);
            assert_eq!(flat.bonds, source.bonds);
            assert_eq!(model.atoms, geometry);
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
        Err(Error::Atom { atom, problem: AtomProblem::Query, .. }) if atom == other
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
fn atom_errors_identify_the_selected_drawing_atom_and_molecule_position() {
    let mut source = ethanol();
    source.atom_mut(89).unwrap().element = "Fe".into();
    source.add_atom("*", Point::new(400., 400.));
    let before = source.clone();
    let error = Prepared::new(&source, &[3]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Can't build a starting 3D structure for Fe3: MMFF has no parameters for it"
    );
    assert!(matches!(
        error,
        Error::Atom {
            atom: 89,
            ref symbol,
            ref label,
            problem: AtomProblem::NoMmffParameters,
        } if symbol == "Fe" && label == "Fe3"
    ));
    assert_eq!(source, before);
}

#[test]
fn atom_range_and_parameter_errors_are_typed() {
    // Strict chemistry refuses these first; the relaxed retry names the atom.
    for change in [
        |atom: &mut crate::document::Atom| atom.charge = 9,
        |atom: &mut crate::document::Atom| atom.charge = -9,
        |atom: &mut crate::document::Atom| atom.isotope = 501,
        |atom: &mut crate::document::Atom| atom.explicit_h = 9,
    ] {
        let mut source = ethanol();
        change(source.atom_mut(89).unwrap());
        assert!(matches!(
            Prepared::relaxed(&source, &[3]),
            Err(Error::Atom { atom: 89, label, problem: AtomProblem::OutOfRange, .. })
                if label == "O3"
        ));
    }
    // A metal radical is not capped, so a radical CeF4 is still 4-coordinate.
    let mut source = Document::default();
    let cerium = source.add_atom("Ce", Point::default());
    for i in 0..4 {
        let turn = std::f32::consts::FRAC_PI_2 * i as f32;
        let fluorine = source.add_atom("F", Point::new(42. * turn.cos(), 42. * turn.sin()));
        source.add_bond(cerium, fluorine, 1, "plain");
    }
    source.atom_mut(cerium).unwrap().radical_electrons = 1;
    let prepared = Prepared::starting(&source, &[], Mode::Relaxed).unwrap();
    assert!(prepared.radicals().is_empty());
    assert!(matches!(
        prepared.check_generation(),
        Err(Error::Atom { atom, problem: AtomProblem::WrongShape { bonds: 4 }, .. })
            if atom == cerium
    ));
    // Hydrogens written on the atom count as ligands: [CeH3] is 3-coordinate.
    let mut source = Document::default();
    let cerium = source.add_atom("Ce", Point::default());
    let atom = source.atom_mut(cerium).unwrap();
    atom.explicit_h = 3;
    atom.no_implicit = true;
    assert!(matches!(
        Prepared::relaxed(&source, &[]).unwrap().check_generation(),
        Err(Error::Atom { atom, problem: AtomProblem::WrongShape { bonds: 3 }, .. })
            if atom == cerium
    ));
    // No force field has Rf; only new geometry is refused, so a file's
    // coordinates can still be flattened.
    let mut source = Document::default();
    let atom = source.add_atom("Rf", Point::default());
    let prepared = Prepared::relaxed(&source, &[]).unwrap();
    for prepared in [prepared.clone(), prepared.rough()] {
        assert!(matches!(
            prepared.check_generation(),
            Err(Error::Atom { atom: id, label, problem: AtomProblem::NoParameters, .. })
                if id == atom && label == "Rf1"
        ));
    }
    assert_eq!(
        AtomProblem::WrongShape { bonds: 1 }.to_string(),
        "the force field gives it the wrong shape with 1 bond"
    );
    assert_eq!(
        AtomProblem::WrongShape { bonds: 3 }.to_string(),
        "the force field gives it the wrong shape with 3 bonds"
    );
}

#[test]
fn radical_saturation_is_detached_and_models_drop_only_the_last_caps_for_each_parent() {
    let mut source = ethanol();
    source.atom_mut(41).unwrap().radical_electrons = 2;
    source.atom_mut(41).unwrap().singlet = true;
    source.atom_mut(89).unwrap().radical_electrons = 1;
    let before = source.clone();
    for mode in [Mode::Strict, Mode::Relaxed] {
        let prepared = Prepared::starting(&source, &[3], mode).unwrap();
        assert_eq!(prepared.radicals().len(), 2);
        assert_eq!(prepared.radicals()[0].atom, 41);
        assert_eq!(prepared.radicals()[0].label, "C1");
        assert_eq!(prepared.radicals()[0].electrons, 2);
        assert_eq!(prepared.radicals()[1].atom, 89);
        assert_eq!(prepared.radicals()[1].label, "O3");
        assert_eq!(prepared.radicals()[1].electrons, 1);
        assert!(
            prepared
                .molecule()
                .state
                .graph
                .atoms
                .iter()
                .all(|atom| atom.radical_electrons == 0)
        );
        let request = prepared
            .generation_request(ForceField::Mmff94s, DEFAULT_SEED)
            .unwrap();
        assert!(request.atoms.iter().all(|atom| atom.radical == 0));
        let solved = optimized(&prepared, vec![0, 1, 0, 2, 1, 0, 2]);
        let model = prepared.model(&solved).unwrap();
        assert_eq!(
            model
                .hydrogens
                .iter()
                .map(|hydrogen| hydrogen.parent)
                .collect::<Vec<_>>(),
            [0, 1, 2, 1]
        );
        assert_eq!(
            model
                .hydrogens
                .iter()
                .map(|hydrogen| hydrogen.xyz[0])
                .collect::<Vec<_>>(),
            [4.5, 6., 9., 10.5]
        );
        let frame = prepared.view_frame(&solved.conformer).unwrap();
        let projected = prepared.document(&solved.conformer, &frame).unwrap();
        let flattened = prepared.flattened(&model).unwrap();
        for document in [
            prepared.source(),
            &prepared.materialized,
            &projected,
            &flattened,
        ] {
            assert_eq!(document.atom(41).unwrap().radical_electrons, 2);
            assert!(document.atom(41).unwrap().singlet);
            assert_eq!(document.atom(89).unwrap().radical_electrons, 1);
            assert_eq!(document.atoms.len(), source.atoms.len());
            assert_eq!(document.bonds, source.bonds);
        }
    }
    assert_eq!(source, before);
}

#[test]
fn a_radical_with_a_fixed_hydrogen_count_is_capped_from_that_count() {
    // An imported [CH2]CC-style centre: its hydrogens are fixed, not implicit.
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    let atom = source.atom_mut(id).unwrap();
    atom.radical_electrons = 1;
    atom.explicit_h = 3;
    atom.no_implicit = true;
    assert!(matches!(
        Prepared::new(&source, &[]),
        Err(Error::Unsupported("radicals"))
    ));
    let prepared = Prepared::starting(&source, &[], Mode::Strict).unwrap();
    assert_eq!(prepared.radicals()[0].electrons, 1);
    let request = prepared
        .generation_request(ForceField::Mmff94s, DEFAULT_SEED)
        .unwrap();
    assert_eq!(
        (request.atoms[0].explicit_h, request.atoms[0].radical),
        (4, 0)
    );
    let model = prepared
        .model(&optimized(&prepared, vec![0, 0, 0, 0]))
        .unwrap();
    assert_eq!(model.hydrogens.len(), 3);
    assert_eq!(prepared.source(), &source);
}

#[test]
fn relaxing_existing_coordinates_keeps_radicals_as_drawn() {
    let mut source = ethanol();
    source.atom_mut(89).unwrap().radical_electrons = 1;
    let prepared = Prepared::relaxed(&source, &[3]).unwrap();
    assert!(prepared.radicals().is_empty());
    let request = prepared
        .generation_request(ForceField::Uff, DEFAULT_SEED)
        .unwrap();
    assert_eq!(request.atoms.iter().map(|a| a.radical).sum::<u32>(), 1);
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
    crate::models3d::require_version(&mut doc);
    doc.atom_mut(41).unwrap().depth = 30.;

    let prepared = Prepared::new(&doc, &[3]).unwrap();
    let model = prepared.model(&optimized(&prepared, vec![0, 2])).unwrap();
    let ids = prepared.ids();
    assert_eq!(
        model.geometry_sources(),
        ids.iter().copied().map(Some).collect::<Vec<_>>()
    );
    assert_eq!(
        model.hydrogens.iter().map(|h| h.parent).collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(model.atoms[1].xyz, [1.5, 0., 0.]);
    assert_eq!(model.force_field, crate::models3d::ForceField::Mmff94s);
    assert!(model.fingerprint.is_some());
    assert!(!model.is_stale(&doc));
    let generated = model.generated.as_ref().unwrap();
    assert_eq!(generated.elements, ["C", "C", "O"]);
    assert_eq!(generated.bonds, [[0, 1], [1, 2]]);
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
        ensemble: Vec::new(),
        initial_energy: 1.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 1,
        force_field: ForceField::Mmff94s,
        diagnostics: vec![],
        approximations: vec![],
    }
}

#[test]
fn flattening_a_model_projects_its_view_at_the_drawing_centre_and_scale() {
    let doc = ethanol();
    let prepared = Prepared::new(&doc, &[3]).unwrap();
    let mut model = prepared.model(&optimized(&prepared, vec![])).unwrap();
    let units = doc.drawing_style.molecular_units_per_angstrom();
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

#[test]
fn flattening_a_stale_model_requires_update_after_addition_or_deletion() {
    let doc = ethanol();
    let prepared = Prepared::new(&doc, &[]).unwrap();
    let model = prepared.model(&optimized(&prepared, vec![])).unwrap();
    for delete in [false, true] {
        let mut edited = doc.clone();
        if delete {
            edited.delete(&[89]);
        } else {
            let added = edited.add_atom("C", Point::new(140., 20.));
            edited.add_bond(89, added, 1, "plain");
        }
        let current = Prepared::new(&edited, &[]).unwrap();
        assert!(matches!(
            current.flattened(&model),
            Err(Error::Selection("Update the model first"))
        ));
    }
}

/// #409: links to what the prepared molecule leaves out (its name or property
/// caption, an arrow's condition text, another molecule's depth fade) never
/// stop a model.
#[test]
fn links_outside_the_molecule_do_not_stop_a_model() {
    use crate::arrow_labels::{ArrowLabel, Side};
    use crate::arrows::{ArrowStyle, Preset};
    use crate::document::Arrow;
    fn text(doc: &mut Document, text: &str, at: Point) -> u64 {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            angle: 0.,
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
    let captioned: fn(&mut Document) = |doc| {
        let atoms = crate::molecule_names::component(doc, 3);
        crate::linked_properties::toggle(
            doc,
            &atoms,
            crate::linked_properties::Property::Weight,
            Default::default(),
            true,
        )
        .unwrap();
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
    for link in [named, captioned, labelled, faded, linked] {
        let mut doc = ethanol();
        link(&mut doc);
        doc.validate().unwrap();
        Prepared::relaxed(&doc, &[3]).unwrap();
        let prepared = Prepared::new(&doc, &[3]).unwrap();
        let mut model = prepared.model(&optimized(&prepared, vec![])).unwrap();
        model.display = crate::models3d::Display::Both;
        let links = (
            doc.molecule_names.clone(),
            doc.property_captions.clone(),
            doc.arrow_labels.clone(),
            doc.depth_appearance.clone(),
            doc.generic.clone(),
        );
        doc.models.push(model);
        crate::models3d::require_version(&mut doc);
        doc.validate().unwrap();
        assert_eq!(
            (
                doc.molecule_names,
                doc.property_captions,
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

mod relaxed;
