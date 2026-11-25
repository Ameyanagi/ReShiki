use super::*;
use crate::canvas_theme::CanvasTheme;
use crate::document::CALCULATIONS;
use crate::models3d::{
    Labels, balls_and_sticks, frame, primitives,
    surface::{Figure, OrbitalRef},
};
use reshiki_qcdata::{CalcResult, Spin};
use serde_json::json;

#[test]
fn placed_orbital_uses_live_meshes_or_a_matching_current_snapshot() {
    let mut doc = tests::ethylene();
    doc.models[0].labels = Labels::All;
    let atom = doc.atoms[0].id;
    let launch = science::launch(&doc, atom, 0, 1).unwrap();
    let result: CalcResult = serde_json::from_value(json!({
        "frame": launch.frame.hash(),
        "task": "energy",
        "settings": {"imported": {}},
        "provenance": {
            "program": "test",
            "source": {"imported": {"format": "test", "file_name": "test", "job": 0}}
        },
        "status": "converged",
        "energy_hartree": -1.0
    }))
    .unwrap();
    let without = format!("{:?}", primitives(&doc));
    let result_key = science::result_key(&result);
    doc.models[0].surface = Some(Figure {
        orbital: OrbitalRef {
            result: result_key,
            spin: Spin::Alpha,
            index: 0,
        },
        iso: 0.05,
        look: Look::new(Fill::Solid, false, 100).unwrap(),
        snapshot: None,
    });
    assert_eq!(doc.file_version(), CALCULATIONS);
    let attachment = science::attach(&mut doc, &launch, vec![], result).unwrap();
    assert_eq!(attachment.freshness, Freshness::Current);
    let key = key(&doc, &doc.models[0]).unwrap();
    let groups = |doc: &Document| {
        primitives(doc)
            .into_iter()
            .find_map(|primitive| match primitive {
                Primitive::Surface { groups } => Some(groups),
                _ => None,
            })
    };
    assert_eq!(format!("{:?}", primitives(&doc)), without);

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
    let meshes = Arc::new([mesh.clone(), mesh]);
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

    for other_key in 0..8 {
        register(other_key, Arc::clone(&meshes));
    }
    assert!(registered(0).is_some());
    register(8, meshes);
    assert!(registered(0).is_some());
    assert!(registered(1).is_none());
    REGISTERED.lock().unwrap().clear();
}
