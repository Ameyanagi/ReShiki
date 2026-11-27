use super::*;
use crate::canvas_theme::CanvasTheme;
use crate::document::CALCULATIONS;
use crate::graphics::{Graphic, GraphicStyle, MoDiagram, MoPicture};
use crate::models3d::{
    Labels, balls_and_sticks, frame, primitives,
    surface::{Figure, OrbitalRef, Snapshot, snapshot},
};
use reshiki_qcdata::{
    BasisSet, Beta, CalcResult, Frame, OrbitalSet, Orbitals, Shell, Source, Spin, Task,
};
use serde_json::json;

static REGISTRY_TEST: Mutex<()> = Mutex::new(());

pub(super) fn lock_registry() -> std::sync::MutexGuard<'static, ()> {
    let guard = REGISTRY_TEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    REGISTERED.lock().unwrap().clear();
    guard
}

pub(super) fn energy_result(frame: &Frame) -> CalcResult {
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

pub(super) fn place(model: &mut Model3d, result: &CalcResult) {
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

fn mo_diagram_document() -> Document {
    let mut doc = tests::ethylene();
    // The diagram supplies its own small model even when the molecule is 2D.
    doc.models[0].display = crate::models3d::Display::TwoD;
    let atom = doc.atoms[0].id;
    let launch = science::launch(&doc, atom, 0, 1).unwrap();
    let mut result = energy_result(&launch.frame);
    result.settings = serde_json::from_value(json!({"xtb": {"method": "gfn2_xtb"}})).unwrap();
    result.provenance.source = Source::Calculated {
        engine_revision: "test".into(),
    };
    result.orbitals = Some(Orbitals {
        basis: Some(BasisSet {
            shells: vec![Shell {
                atom: 0,
                l: 0,
                pure: false,
                exponents_per_bohr2: vec![1.],
                coefficients: vec![1.],
            }],
        }),
        core_electrons: vec![],
        alpha: OrbitalSet {
            energies_hartree: vec![-0.4, -0.2, 0.1],
            occupations: vec![2., 0., 0.],
            symmetries: vec![],
            rows: vec![0, 1, 2],
            coefficients: vec![0.25, 0.5, 0.75],
        },
        beta: Beta::Restricted,
    });
    result.validate(&launch.frame).unwrap();
    let columns = crate::orbital_levels::columns(result.orbitals.as_ref().unwrap());
    place(&mut doc.models[0], &result);
    let figure = doc.models[0].surface.take().unwrap();
    let mut mo = MoDiagram::new(atom, science::result_key(&result), columns);
    mo.pictures = (0..2)
        .map(|index| MoPicture {
            column: 0,
            figure: Figure {
                orbital: OrbitalRef {
                    index,
                    ..figure.orbital.clone()
                },
                ..figure.clone()
            },
        })
        .collect();
    let (width, height) = mo.natural_size(&doc.drawing_style);
    doc.graphics.push(Graphic::mo_diagram(
        doc.next_id(),
        Point::new(180., -120.),
        Point::new(width, height),
        mo,
        GraphicStyle::default(),
    ));
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    doc
}

fn mo_surfaces(doc: &Document) -> usize {
    crate::scene::primitives(doc)
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Surface { .. }))
        .count()
}

fn register_mo_picture(doc: &Document, index: usize) {
    let small = doc.graphics[0].mo_picture_models(doc).nth(index).unwrap();
    register(key(doc, &small).unwrap(), two_lobes());
}

#[test]
fn mo_diagram_pictures_draw_with_an_unsupported_source_style() {
    let _guard = lock_registry();
    let mut doc = mo_diagram_document();
    for style in [Style::Wireframe, Style::Ellipsoids] {
        doc.models[0].style = style;
        for (index, picture) in doc.graphics[0].mo_picture_models(&doc).enumerate() {
            assert_eq!(picture.style, Style::BallStick);
            assert!(figure_ready(&doc, &picture));
            register_mo_picture(&doc, index);
        }
        assert_eq!(mo_surfaces(&doc), 2, "{style:?}");
    }
}

#[test]
fn mo_diagram_pictures_draw_live_and_saved_after_reopening() {
    let _guard = lock_registry();
    let doc = mo_diagram_document();
    assert_eq!(doc.file_version(), CALCULATIONS);
    assert_eq!(mo_surfaces(&doc), 0);
    register_mo_picture(&doc, 0);
    assert_eq!(mo_surfaces(&doc), 1);
    let stroke_widths = |doc: &Document| {
        crate::scene::primitives(doc)
            .into_iter()
            .flat_map(|primitive| match primitive {
                Primitive::Surface { groups } => groups
                    .into_iter()
                    .flat_map(|g| g.strokes)
                    .map(|s| s.outline.width)
                    .collect(),
                _ => vec![],
            })
            .collect::<Vec<_>>()
    };
    let live_widths = stroke_widths(&doc);

    let saved = doc.current();
    let pictures = &saved.graphics[0].mo.as_ref().unwrap().pictures;
    assert!(pictures[0].figure.snapshot.is_some());
    assert!(pictures[1].figure.snapshot.is_none());
    assert!(
        doc.graphics[0].mo.as_ref().unwrap().pictures[0]
            .figure
            .snapshot
            .is_none()
    );
    let bytes = doc.file_json().unwrap();
    REGISTERED.lock().unwrap().clear();
    assert_eq!(mo_surfaces(&saved), 1, "the saved picture needs no mesh");
    assert!(!live_widths.is_empty());
    assert_eq!(stroke_widths(&saved), live_widths);

    let reopened = Document::from_json(&bytes).unwrap();
    assert_eq!(reopened.graphics[0].mo, saved.graphics[0].mo);
    assert_eq!(mo_surfaces(&reopened), 1);
    let mut resized = reopened.clone();
    resized.graphics[0].axis_x.x *= 0.7;
    resized.graphics[0].axis_y.y *= 0.7;
    assert_eq!(mo_surfaces(&resized), 1, "resizing keeps the saved surface");
    let origin = resized.graphics[0].origin;
    resized.graphics[0].map_positions(|p| p.turned(origin, 45.));
    assert_eq!(mo_surfaces(&resized), 1, "turning keeps the saved surface");
    let mut stale = reopened.clone();
    stale.atoms[0].isotope = 13;
    assert_eq!(mo_surfaces(&stale), 0);
    assert!(crate::scene::primitives(&stale).iter().any(|primitive| {
        matches!(primitive, Primitive::Text { text, .. } if text.starts_with("HOMO"))
    }));
    let mut missing = reopened;
    missing.science.results.clear();
    assert_eq!(mo_surfaces(&missing), 0);
}

#[test]
fn mo_diagram_pictures_survive_an_unrelated_3d_only_model() {
    let _guard = lock_registry();
    let mut doc = mo_diagram_document();
    register_mo_picture(&doc, 0);
    assert_eq!(mo_surfaces(&doc), 1);

    let atom = doc.add_atom("O", Point::new(-200., 0.));
    let mut other = Model3d::new(
        &doc,
        vec![crate::models3d::Placed {
            id: atom,
            xyz: [0.; 3],
        }],
        vec![],
        crate::models3d::ForceField::Mmff94s,
    );
    other.display = crate::models3d::Display::ThreeD;
    doc.models.push(other);
    assert!(crate::models3d::drawn(&doc).models.is_empty());

    for layer in [-1, 0] {
        doc.graphics[0].layer = layer;
        assert_eq!(mo_surfaces(&doc), 1);
        assert_eq!(
            crate::scene::vector_primitives(&doc)
                .iter()
                .filter(|primitive| matches!(primitive, Primitive::Surface { .. }))
                .count(),
            1
        );
    }
    let saved = doc.current();
    REGISTERED.lock().unwrap().clear();
    assert_eq!(mo_surfaces(&saved), 1);
}

#[test]
fn mo_diagram_pictures_survive_an_unrelated_table_and_stay_clipped_in_a_cell() {
    use crate::graphics::{
        GraphicKind,
        table::{Cell, Table},
    };

    let _guard = lock_registry();
    let mut doc = mo_diagram_document();
    register_mo_picture(&doc, 0);
    assert_eq!(mo_surfaces(&doc), 1);

    let mut table = Graphic::dragged(
        doc.next_id(),
        GraphicKind::Table,
        Point::new(100., -200.),
        Point::new(1000., 500.),
        Default::default(),
        Default::default(),
        false,
    );
    table.table = Some(Box::new(Table {
        cells: vec![Cell {
            id: 1,
            lo: Point::default(),
            hi: Point::new(1., 1.),
            members: vec![],
            borders: vec![],
        }],
    }));
    doc.graphics.push(table);
    for layer in [-1, 0] {
        doc.graphics[0].layer = layer;
        assert_eq!(mo_surfaces(&doc), 1);
        assert_eq!(
            crate::scene::vector_primitives(&doc)
                .iter()
                .filter(|primitive| matches!(primitive, Primitive::Surface { .. }))
                .count(),
            1
        );
    }
    let mut saved = doc.current();
    REGISTERED.lock().unwrap().clear();
    assert_eq!(mo_surfaces(&saved), 1);

    let diagram = saved.graphics[0].id;
    saved.graphics[1].table.as_mut().unwrap().cells[0]
        .members
        .push(diagram);
    saved.validate().unwrap();
    for layer in [-1, 0] {
        saved.graphics[0].layer = layer;
        for drawing in [
            crate::scene::primitives(&saved),
            crate::scene::vector_primitives(&saved),
        ] {
            assert!(
                !drawing
                    .iter()
                    .any(|p| matches!(p, Primitive::Surface { .. }))
            );
            assert_eq!(
                drawing
                    .iter()
                    .filter_map(|primitive| match primitive {
                        Primitive::Clip { primitives, .. } => Some(primitives),
                        _ => None,
                    })
                    .flatten()
                    .filter(|primitive| matches!(primitive, Primitive::Surface { .. }))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn mo_diagram_current_refreshes_each_picture_in_its_own_slot() {
    let _guard = lock_registry();
    let mut doc = mo_diagram_document();
    let stale = &mut doc.graphics[0].mo.as_mut().unwrap().pictures[0].figure;
    stale.orbital.result = "stale".into();
    stale.snapshot = Some(Snapshot {
        view: "stale".into(),
        reach: 3.,
        shapes: "the first slot is retained".into(),
    });
    let original = stale.clone();
    register_mo_picture(&doc, 1);
    assert_eq!(mo_surfaces(&doc), 1);

    let saved = doc.current();
    let pictures = &saved.graphics[0].mo.as_ref().unwrap().pictures;
    assert_eq!(pictures[0].figure, original);
    assert_eq!(pictures[1].figure.orbital.index, 1);
    assert!(pictures[1].figure.snapshot.is_some());
    assert!(
        doc.graphics[0].mo.as_ref().unwrap().pictures[1]
            .figure
            .snapshot
            .is_none()
    );
    REGISTERED.lock().unwrap().clear();
    assert_eq!(mo_surfaces(&saved), 1);
}

#[test]
fn mo_diagram_pictures_pin_rows_and_carry_referenced_science() {
    let _guard = lock_registry();
    let doc = mo_diagram_document();
    let mo = doc.graphics[0].mo.as_ref().unwrap();
    assert_eq!(
        doc.orbital_pins().get(&mo.result),
        Some(&vec![(Spin::Alpha, 0), (Spin::Alpha, 1)])
    );
    let original = doc.science.results[0].result.orbitals.as_ref().unwrap();
    let saved = doc.current();
    let orbitals = saved.science.results[0].result.orbitals.as_ref().unwrap();
    assert_eq!(orbitals.basis, original.basis);
    assert_eq!(orbitals.alpha.rows, [0, 1]);
    assert_eq!(orbitals.alpha.coefficients, [0.25, 0.5]);
    assert_eq!(original.alpha.rows, [0, 1, 2]);

    let science = science::for_figures(&doc);
    assert_eq!(science.results.len(), 1);
    assert_eq!(science.attachments.len(), 1);
    let orbitals = science.results[0].result.orbitals.as_ref().unwrap();
    assert_eq!(orbitals.alpha.rows, [0, 1]);
    assert_eq!(orbitals.alpha.coefficients, [0.25, 0.5]);
    Document {
        science,
        ..doc.clone()
    }
    .validate()
    .unwrap();

    let mut levels_only = doc.clone();
    levels_only.graphics[0]
        .mo
        .as_mut()
        .unwrap()
        .pictures
        .clear();
    let science = science::for_figures(&levels_only);
    assert_eq!(
        science.results.len(),
        1,
        "the diagram references its result"
    );
    let orbitals = science.results[0].result.orbitals.as_ref().unwrap();
    assert!(orbitals.alpha.rows.is_empty());
    assert!(orbitals.basis.is_none());
    let mut without_science = levels_only;
    without_science.science = Default::default();
    assert_eq!(without_science.file_version(), CALCULATIONS);
}

#[test]
fn mo_diagram_copy_keeps_saved_pictures_with_a_remapped_molecule() {
    let _guard = lock_registry();
    let doc = mo_diagram_document();
    register_mo_picture(&doc, 0);
    let source = doc.current();
    REGISTERED.lock().unwrap().clear();
    assert_eq!(mo_surfaces(&source), 1);
    let part = crate::editing::selection(&source, &source.all_ids());
    assert_eq!(part.science.results.len(), 1);
    assert_eq!(
        part.science.results[0]
            .result
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .rows,
        [0, 1]
    );
    let mut pasted = source.clone();
    let inserted = crate::editing::append(&mut pasted, &part, Point::new(400., 0.));
    let copy = pasted.graphics[1].mo.as_ref().unwrap();
    let molecule = copy.molecule.unwrap();
    assert!(inserted.contains(&molecule));
    assert_ne!(
        copy.molecule,
        source.graphics[0].mo.as_ref().unwrap().molecule
    );
    assert!(pasted.models[1].contains(molecule));
    assert!(copy.linked);
    assert_eq!(mo_surfaces(&pasted), 2);
    pasted.validate().unwrap();
    let reopened = Document::from_json(&pasted.file_json().unwrap()).unwrap();
    assert_eq!(mo_surfaces(&reopened), 2);
}

#[test]
fn mo_diagram_copied_alone_becomes_static_without_pictures() {
    let _guard = lock_registry();
    let doc = mo_diagram_document();
    register_mo_picture(&doc, 0);
    let source = doc.current();
    REGISTERED.lock().unwrap().clear();
    let part = crate::editing::selection(&source, &[source.graphics[0].id]);
    assert!(part.models.is_empty());
    assert!(part.atoms.is_empty());
    let mut pasted = Document::default();
    assert!(!crate::editing::append(&mut pasted, &part, Point::new(20., 30.)).is_empty());
    let copy = pasted.graphics[0].mo.as_ref().unwrap();
    assert_eq!(copy.molecule, None);
    assert!(!copy.linked);
    assert_eq!(
        copy.columns,
        source.graphics[0].mo.as_ref().unwrap().columns
    );
    assert_eq!(copy.pictures.len(), 2);
    assert_eq!(mo_surfaces(&pasted), 0);
    assert!(crate::scene::primitives(&pasted).iter().any(|primitive| {
        matches!(primitive, Primitive::Text { text, .. } if text.starts_with("HOMO"))
    }));
    pasted.validate().unwrap();
}

#[test]
fn mo_diagram_generic_tiles_keep_signed_pictures() {
    use crate::{atom_text, generic};

    let _guard = lock_registry();
    let mut doc = mo_diagram_document();
    let site = doc.add_atom("*", Point::new(1000., 0.));
    doc = atom_text::apply(&doc, site, "[Cl,Br]", atom_text::Mode::Auto).unwrap();
    register_mo_picture(&doc, 0);
    let saved = doc.current();
    REGISTERED.lock().unwrap().clear();
    let expanded = generic::prepare_expansion(&saved, generic::Limits::default())
        .unwrap()
        .take()
        .unwrap();
    let molecules: std::collections::HashSet<_> = expanded
        .graphics
        .iter()
        .filter_map(|g| g.mo.as_ref()?.molecule)
        .collect();
    assert_eq!(molecules.len(), 2);
    assert_eq!(mo_surfaces(&expanded), 2);
}

#[test]
fn mo_diagram_keeps_pictures_when_moving_its_molecule_to_attach() {
    use crate::joining::Prepared;
    use crate::templates::{Anchor, Connection};

    let _guard = lock_registry();
    let mut doc = mo_diagram_document();
    let atom = doc.atoms[0].id;
    let diagram = doc.graphics[0].id;
    doc.translate(&doc.models[0].linked_atoms.clone(), 400., 200.);
    let source = doc.add_atom("C", Point::new(180., 0.));
    let source_end = doc.add_atom("O", Point::new(222., 0.));
    doc.add_bond(source, source_end, 1, "plain");
    let target = doc.add_atom("C", Point::default());
    let target_end = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(target, target_end, 1, "plain");
    register_mo_picture(&doc, 0);
    let saved = doc.current();
    REGISTERED.lock().unwrap().clear();
    for move_diagram in [false, true] {
        let mut moving = vec![source, atom];
        if move_diagram {
            moving.push(diagram);
        }
        let (joined, _) = Prepared::new(&saved, &moving)
            .unwrap()
            .place(
                Point::default(),
                None,
                5.,
                Anchor::Atom(source),
                Connection::Connect,
            )
            .unwrap();
        let mo = joined.graphics[0].mo.as_ref().unwrap();
        assert_eq!(mo.molecule, Some(atom));
        assert!(mo.linked);
        assert_eq!(mo_surfaces(&joined), 1);
    }
}

#[test]
fn generic_expansion_keeps_saved_orbital_pictures() {
    use crate::{atom_text, generic};

    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    let site = doc.add_atom("*", Point::new(200., 0.));
    doc = atom_text::apply(&doc, site, "[Cl,Br]", atom_text::Mode::Auto).unwrap();
    let atom = doc.atoms[0].id;
    let launch = science::launch(&doc, atom, 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    place(&mut doc.models[0], &result);
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    register(key(&doc, &doc.models[0]).unwrap(), two_lobes());
    let saved = doc.current();
    REGISTERED.lock().unwrap().clear();
    assert!(groups(&saved).is_some());

    let expanded = generic::prepare_expansion(&saved, generic::Limits::default())
        .unwrap()
        .take()
        .unwrap();
    assert_eq!(expanded.models.len(), 2);
    assert!(!expanded.models[1].contains(atom));
    assert_eq!(
        primitives(&expanded)
            .iter()
            .filter(|primitive| matches!(primitive, Primitive::Surface { .. }))
            .count(),
        2
    );
}

#[test]
fn move_and_attach_keeps_a_companions_saved_orbital_panel() {
    use crate::joining::Prepared;
    use crate::models3d::Display;
    use crate::models3d::surface::Panel;
    use crate::templates::{Anchor, Connection};

    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    let companion = doc.all_ids();
    doc.translate(&companion, 400., 200.);
    doc.models[0].display = Display::Both;
    let source = doc.add_atom("C", Point::new(180., 0.));
    let source_end = doc.add_atom("O", Point::new(222., 0.));
    doc.add_bond(source, source_end, 1, "plain");
    let target = doc.add_atom("C", Point::default());
    let target_end = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(target, target_end, 1, "plain");
    let launch = science::launch(&doc, companion[0], 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    place(&mut doc.models[0], &result);
    let figure = doc.models[0].surface.take().unwrap();
    let small = crate::models3d::panel_model(&doc.models[0], &figure);
    doc.models[0].panels.push(Panel {
        figure,
        caption: "HOMO".into(),
    });
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    register(key(&doc, &small).unwrap(), two_lobes());
    let saved = doc.current();
    REGISTERED.lock().unwrap().clear();
    assert!(groups(&saved).is_some());

    let moving = [source, source_end, companion[0], companion[1]];
    let (joined, _) = Prepared::new(&saved, &moving)
        .unwrap()
        .place(
            Point::default(),
            None,
            5.,
            Anchor::Atom(source),
            Connection::Connect,
        )
        .unwrap();
    assert_eq!(joined.models[0].linked_ids(&joined), companion);
    assert!(groups(&joined).is_some(), "the saved panel still draws");
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
fn saving_a_turning_orbital_uses_resting_quality_at_the_saved_angle() {
    let _guard = lock_registry();
    let mut doc = tests::ethylene();
    let launch = science::launch(&doc, doc.atoms[0].id, 0, 1).unwrap();
    let result = energy_result(&launch.frame);
    place(&mut doc.models[0], &result);
    science::attach(&mut doc, &launch, vec![], result).unwrap();
    let model = &mut doc.models[0];
    model.surface.as_mut().unwrap().look.opacity = 50;
    let half_turn = std::f32::consts::FRAC_1_SQRT_2;
    let rotation = [half_turn, half_turn, 0., 0.];
    model.rotation = rotation;
    register(key(&doc, &doc.models[0]).unwrap(), two_lobes());

    let resting = doc.current().models[0].surface.clone().unwrap();
    assert!(!resting.snapshot.as_ref().unwrap().shapes.is_empty());
    doc.models[0].turning = true;
    let saved = doc.current();
    assert_eq!(saved.models[0].surface.as_ref(), Some(&resting));
    assert_eq!(saved.models[0].rotation, rotation);
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.models[0].surface.as_ref(), Some(&resting));
    assert_eq!(reopened.models[0].rotation, rotation);
    assert!(doc.models[0].turning);
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
    // Captions keep at least one and a half of their size between them.
    let span = |caption: &Primitive| match caption {
        Primitive::Text {
            position,
            text,
            size,
            style,
            ..
        } => (
            position.x,
            position.x + crate::style::styled_text_width(text, *size, style),
            *size,
        ),
        _ => unreachable!("a caption is text"),
    };
    let ((_, end, size), (start, _, _)) = (span(&panels[0].1), span(&panels[1].1));
    assert!(start - end >= size * 1.5 - 1e-3, "{end} {start}");
    assert!(frames[0].1.x - frames[0].0.x < hi.x - lo.x);
    // A click on a panel picks the molecule, as one on the frame does.
    let (plo, phi) = frames[1];
    let centre = Point::new((plo.x + phi.x) / 2., (plo.y + phi.y) / 2.);
    assert_eq!(
        crate::models3d::frame_hit(&doc, centre),
        Some(model.linked_ids(&doc))
    );
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
    // The file reads back with both panels.
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.models[0].panels.len(), 2);
    assert_eq!(reopened.models[0].panels[1].caption, "LUMO (−7.66 eV)");
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

    // A pasted copy gets new IDs and still draws its saved panel.
    let surfaces = |doc: &Document| {
        primitives(doc)
            .iter()
            .filter(|p| matches!(p, Primitive::Surface { .. }))
            .count()
    };
    assert_eq!(surfaces(&saved), 1);
    let part = crate::editing::selection(&saved, &saved.all_ids());
    let mut pasted = saved.clone();
    assert!(!crate::editing::append(&mut pasted, &part, Point::new(200., 0.)).is_empty());
    assert_eq!(surfaces(&pasted), 2);

    // A stale first panel keeps its own figure when the second is refreshed.
    let mut mixed = doc.clone();
    mixed.models[0].panels[0].figure.orbital.result = "stale".into();
    let shown = crate::models3d::panels(&mixed, &mixed.models[0]);
    assert_eq!(shown.len(), 1);
    register(super::key(&mixed, &shown[0].0).unwrap(), two_lobes());
    let saved = mixed.current();
    let kept = &saved.models[0].panels;
    assert_eq!(kept[0].figure.orbital.result, "stale");
    assert_eq!(kept[1].figure.orbital.index, 1);
    assert!(kept[1].figure.snapshot.is_some());
    REGISTERED.lock().unwrap().clear();
}
