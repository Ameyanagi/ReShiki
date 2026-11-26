use super::*;
use crate::canvas_theme::CanvasTheme;
use crate::document::CALCULATIONS;
use crate::models3d::{
    Labels, balls_and_sticks, frame, primitives,
    surface::{Figure, OrbitalRef, Snapshot, snapshot},
};
use reshiki_qcdata::{CalcResult, Frame, Spin, Task};
use serde_json::json;

static REGISTRY_TEST: Mutex<()> = Mutex::new(());

fn lock_registry() -> std::sync::MutexGuard<'static, ()> {
    let guard = REGISTRY_TEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    REGISTERED.lock().unwrap().clear();
    guard
}

fn energy_result(frame: &Frame) -> CalcResult {
    serde_json::from_value(json!({
        "frame": frame.hash(),
        "task": "energy",
        "settings": {"imported": {}},
        "provenance": {
            "program": "test",
            "source": {"imported": {"format": "test", "file_name": "test", "job": 0}}
        },
        "status": "converged",
        "energy_hartree": -1.0
    }))
    .unwrap()
}

fn place(model: &mut Model3d, result: &CalcResult) {
    model.surface = Some(Figure {
        orbital: OrbitalRef {
            result: science::result_key(result),
            spin: Spin::Alpha,
            index: 0,
        },
        iso: 0.05,
        look: Look::new(Fill::Solid, false, 100).unwrap(),
        snapshot: None,
    });
}

fn groups(doc: &Document) -> Option<Vec<SurfaceGroup>> {
    primitives(doc)
        .into_iter()
        .find_map(|primitive| match primitive {
            Primitive::Surface { groups } => Some(groups),
            _ => None,
        })
}

fn two_lobes() -> Meshes {
    let field = |[x, y, z]: [f32; 3]| {
        let lobe = |c: f32| (0.64 - (x - c).powi(2) - y * y - z * z).max(0.);
        lobe(1.4) - lobe(-1.4)
    };
    let grid = Grid {
        origin: [-3.; 3],
        step: 0.25,
        size: [25; 3],
    };
    let mesh = mesh::isosurface(field, grid, 0.05);
    Arc::new([mesh.clone(), mesh])
}

#[test]
fn placed_orbital_uses_live_meshes_or_a_matching_current_snapshot() {
    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    doc.models[0].labels = Labels::All;
    let atom = doc.atoms[0].id;
    let launch = science::launch(&doc, atom, 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    let without = format!("{:?}", primitives(&doc));
    place(&mut doc.models[0], &result);
    assert_eq!(doc.file_version(), CALCULATIONS);
    let attachment = science::attach(&mut doc, &launch, vec![], result).unwrap();
    assert_eq!(attachment.freshness, Freshness::Current);
    let key = key(&doc, &doc.models[0]).unwrap();
    assert_eq!(format!("{:?}", primitives(&doc)), without);

    let meshes = two_lobes();
    let registered_meshes = Arc::clone(&meshes);
    std::thread::spawn(move || register(key, registered_meshes))
        .join()
        .unwrap();
    assert!(Arc::ptr_eq(&registered(key).unwrap(), &meshes));
    let live = groups(&doc).unwrap();
    let fills: Vec<_> = live.iter().flat_map(|g| &g.fills).collect();
    assert_eq!(fills.len(), 2);
    assert!(fills[0].color[2] > fills[0].color[0]);
    assert!(fills[1].color[0] > fills[1].color[2]);
    let live_frame = frame(&doc, &doc.models[0]).unwrap();
    let live_molecule = balls_and_sticks(&doc, &doc.models[0]).unwrap();
    let saved = doc.current();
    let figure = saved.models[0].surface.as_ref().unwrap();
    let snapshot = figure.snapshot.as_ref().unwrap();
    assert!(snapshot.shapes.len() <= snapshot::MAX_BYTES);
    assert!(snapshot.reach > doc.models[0].reach(&doc));
    assert_eq!(snapshot.view, view_signature(&doc, &doc.models[0]));
    assert!(doc.models[0].surface.as_ref().unwrap().snapshot.is_none());
    let bytes = doc.file_json().unwrap();

    let mut stale = saved.clone();
    stale.atom_mut(atom).unwrap().isotope = 13;
    assert_eq!(
        stale.science.attachments[0].freshness(&stale),
        Freshness::Stale
    );
    assert!(groups(&stale).is_none());

    REGISTERED.lock().unwrap().clear();
    assert!(groups(&stale).is_none());
    let mut reopened = Document::from_json(&bytes).unwrap();
    assert_eq!(frame(&reopened, &reopened.models[0]), Some(live_frame));
    assert_eq!(
        balls_and_sticks(&reopened, &reopened.models[0]),
        Some(live_molecule)
    );
    let fallback = groups(&reopened).unwrap();
    assert_eq!(fallback[0].fills.len(), live[0].fills.len());
    assert!(matches!(
        primitives(&reopened).last(),
        Some(Primitive::Surface { .. })
    ));
    let mut removed = reopened.clone();
    removed.science.results.clear();
    assert!(groups(&removed).is_none());
    let part = crate::editing::selection(&reopened, &reopened.all_ids());
    assert_eq!(part.science.results.len(), 1);
    let mut duplicated = reopened.clone();
    let inserted = crate::editing::append(&mut duplicated, &part, Point::new(120., 0.));
    assert!(!inserted.is_empty());
    let surface_count = |doc: &Document| {
        primitives(doc)
            .iter()
            .filter(|primitive| matches!(primitive, Primitive::Surface { .. }))
            .count()
    };
    assert_eq!(surface_count(&duplicated), 2);
    duplicated
        .atom_mut(duplicated.models[1].linked_atoms[0])
        .unwrap()
        .isotope = 13;
    assert_eq!(surface_count(&duplicated), 1);
    let anchor = |groups: &[SurfaceGroup]| match groups[0].fills[0].region[0] {
        crate::graphics::PathCommand::Move(point) => point,
        _ => panic!("surface contour starts with a move"),
    };
    let before = anchor(&fallback);
    reopened.models[0].offset = Point::new(12., -8.);
    assert_eq!(
        view_signature(&reopened, &reopened.models[0]),
        snapshot.view
    );
    let after = anchor(&groups(&reopened).unwrap());
    assert!(after.distance(before.offset(12., -8.)) < 0.001);
    reopened.canvas_theme = CanvasTheme::Dark;
    assert!(groups(&reopened).is_none());
    reopened.canvas_theme = CanvasTheme::Light;
    let half_turn = std::f32::consts::FRAC_1_SQRT_2;
    reopened.models[0].rotation = [half_turn, half_turn, 0., 0.];
    assert!(groups(&reopened).is_none());
}

#[test]
fn placed_orbital_only_draws_on_its_result_geometry() {
    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    let launch = science::launch(&doc, doc.atoms[0].id, 0, 1).unwrap();
    let mut final_frame = (*launch.frame).clone();
    final_frame.positions_angstrom[0][2] += 0.2;
    let mut result = energy_result(&final_frame);
    result.task = Task::Optimize;
    result.input_frame = Some(launch.frame.hash());
    let final_hash = result.frame;
    place(&mut doc.models[0], &result);
    let attached = science::attach(&mut doc, &launch, vec![final_frame], result).unwrap();
    science::apply_geometry(&mut doc, attached.attachment, &final_hash).unwrap();
    let mesh = two_lobes();
    register(key(&doc, &doc.models[0]).unwrap(), Arc::clone(&mesh));
    assert!(figure_ready(&doc, &doc.models[0]));
    assert!(groups(&doc).is_some());
    doc = doc.current();
    let saved = doc.models[0]
        .surface
        .as_ref()
        .unwrap()
        .snapshot
        .clone()
        .unwrap();
    science::move_model(&mut doc, &launch.atoms, &launch.frame).unwrap();
    assert_eq!(
        doc.science
            .attachment(attached.attachment)
            .unwrap()
            .freshness(&doc),
        Freshness::Current,
    );
    assert!(!figure_ready(&doc, &doc.models[0]));
    register(key(&doc, &doc.models[0]).unwrap(), mesh);
    assert!(groups(&doc).is_none());

    REGISTERED.lock().unwrap().clear();
    doc.models[0]
        .surface
        .as_mut()
        .unwrap()
        .snapshot
        .as_mut()
        .unwrap()
        .view = view_signature(&doc, &doc.models[0]);
    assert!(groups(&doc).is_none());
    science::apply_geometry(&mut doc, attached.attachment, &final_hash).unwrap();
    doc.models[0].surface.as_mut().unwrap().snapshot = Some(saved);
    assert!(groups(&doc).is_some());

    doc.models[0].atoms[0].xyz[2] += 0.0005;
    assert!(figure_ready(&doc, &doc.models[0]));
    doc.models[0].atoms[0].xyz[2] += 0.001;
    assert!(!figure_ready(&doc, &doc.models[0]));
}

#[test]
fn snapshot_reach_does_not_depend_on_decoding() {
    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    let launch = science::launch(&doc, doc.atoms[0].id, 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    place(&mut doc.models[0], &result);
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    let before = balls_and_sticks(&doc, &doc.models[0]).unwrap();
    let signature = view_signature(&doc, &doc.models[0]);
    let saved = Snapshot {
        view: signature.clone(),
        reach: 10.,
        shapes: "invalid base64!".into(),
    };
    doc.models[0].surface.as_mut().unwrap().snapshot = Some(saved);
    assert_eq!(view_signature(&doc, &doc.models[0]), signature);
    let after = balls_and_sticks(&doc, &doc.models[0]).unwrap();
    assert!(after.0[0].1 < before.0[0].1);
    assert!(groups(&doc).is_none());
}

#[test]
fn view_signature_preserves_existing_snapshot_signatures() {
    let mut doc = tests::ethylene();
    let launch = science::launch(&doc, doc.atoms[0].id, 0, 1).unwrap();
    place(&mut doc.models[0], &energy_result(&launch.frame));
    let model = &mut doc.models[0];
    model.offset = Point::new(12., -8.);
    model.labels = Labels::All;
    model.notes.push("view signature".into());
    model.surface.as_mut().unwrap().snapshot = Some(Snapshot {
        view: String::new(),
        reach: 3.,
        shapes: "A".repeat(snapshot::MAX_BYTES),
    });
    let model = &doc.models[0];
    let mut legacy = model.clone();
    legacy.offset = Point::default();
    legacy.surface.as_mut().unwrap().snapshot = None;
    let points = legacy.points(&doc).map(|points| {
        points
            .into_iter()
            .map(|(element, xyz)| (element, xyz.map(f32::to_bits)))
            .collect::<Vec<_>>()
    });
    let bytes = serde_json::to_vec(&(&legacy, doc.canvas_theme, points)).unwrap();
    assert_eq!(
        view_signature(&doc, model),
        format!("{:x}", Sha256::digest(bytes))
    );
}

#[test]
fn an_occluded_orbital_snapshot_preserves_reach_without_shapes() {
    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    doc.models[0].style = Style::SpaceFilling;
    doc.models[0].rotation = [1., 0., 0., 0.];
    doc.models[0].perspective = 0.;
    let unscaled = balls_and_sticks(&doc, &doc.models[0]).unwrap();
    let launch = science::launch(&doc, doc.atoms[0].id, 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    place(&mut doc.models[0], &result);
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    let mesh = mesh::isosurface(
        |[x, y, z]| (0.21 - (x + 0.665).powi(2) - y * y - (z + 4.).powi(2)).max(0.),
        Grid {
            origin: [-2., -1., -5.],
            step: 0.125,
            size: [25, 17, 17],
        },
        0.05,
    );
    assert!(!mesh.triangles.is_empty());
    register(
        key(&doc, &doc.models[0]).unwrap(),
        Arc::new([mesh.clone(), mesh]),
    );
    assert!(groups(&doc).is_none());
    let live = balls_and_sticks(&doc, &doc.models[0]).unwrap();
    assert!(live.0[0].1 < unscaled.0[0].1);
    let bytes = doc.file_json().unwrap();
    REGISTERED.lock().unwrap().clear();
    let reopened = Document::from_json(&bytes).unwrap();
    let model = &reopened.models[0];
    let saved = model.surface.as_ref().unwrap().snapshot.as_ref().unwrap();
    assert!(saved.reach > 4.);
    assert!(matches!(
        snapshot::decode(&saved.shapes, Point::default()).unwrap(),
        Primitive::Surface { groups } if groups.is_empty()
    ));
    assert_eq!(balls_and_sticks(&reopened, model), Some(live));
    assert!(groups(&reopened).is_none());
}

#[test]
fn mesh_registry_keeps_twenty_four_recent_figures() {
    let _guard = lock_registry();
    let meshes = Arc::new([Mesh::default(), Mesh::default()]);
    for other_key in 0..24 {
        register(other_key, Arc::clone(&meshes));
    }
    for other_key in 0..24 {
        assert!(registered(other_key).is_some(), "{other_key}");
    }
    assert!(registered(0).is_some());
    register(24, meshes);
    assert!(registered(0).is_some());
    assert!(registered(1).is_none());
    REGISTERED.lock().unwrap().clear();
}

#[test]
fn panels_draw_small_models_with_captions_and_keep_their_snapshots() {
    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    let atom = doc.atoms[0].id;
    let launch = science::launch(&doc, atom, 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    place(&mut doc.models[0], &result);
    let figure = doc.models[0].surface.take().unwrap();
    doc.models[0].panels = ["HOMO (−11.33 eV)", "LUMO (−7.66 eV)"]
        .into_iter()
        .enumerate()
        .map(|(k, caption)| crate::models3d::surface::Panel {
            figure: Figure {
                orbital: OrbitalRef {
                    index: k as u32,
                    ..figure.orbital.clone()
                },
                ..figure.clone()
            },
            caption: caption.into(),
        })
        .collect();
    assert_eq!(doc.file_version(), CALCULATIONS);
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    let model = &doc.models[0];
    let (lo, hi) = frame(&doc, model).unwrap();
    let panels = crate::models3d::panels(&doc, model);
    assert_eq!(panels.len(), 2);
    let frames: Vec<_> = panels
        .iter()
        .map(|(small, _)| frame(&doc, small).unwrap())
        .collect();
    // Under the model's frame, side by side, smaller, not overlapping.
    assert!(frames.iter().all(|(plo, _)| plo.y > hi.y));
    assert!(frames[0].1.x <= frames[1].0.x);
    assert!(frames[0].1.x - frames[0].0.x < hi.x - lo.x);
    let pins = doc.orbital_pins();
    assert_eq!(
        pins.values().next().unwrap(),
        &[(Spin::Alpha, 0), (Spin::Alpha, 1)]
    );

    // A registered mesh draws its panel, and saving keeps its snapshot.
    let key = key(&doc, &panels[0].0).unwrap();
    register(key, two_lobes());
    assert!(groups(&doc).is_some());
    let saved = doc.current();
    assert!(saved.models[0].panels[0].figure.snapshot.is_some());
    assert!(saved.models[0].panels[1].figure.snapshot.is_none());
    REGISTERED.lock().unwrap().clear();
    assert!(
        groups(&saved).is_some(),
        "the snapshot draws without the mesh"
    );
    let texts = primitives(&saved)
        .into_iter()
        .filter(|p| matches!(p, Primitive::Text { text, .. } if text.ends_with("eV)")))
        .count();
    assert_eq!(texts, 2);
}
