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
