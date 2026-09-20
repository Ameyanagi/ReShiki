use reshiki::{
    chains::{self, BondDrawing, ChainDrawing},
    document::{Annotation, Document, Point},
    engine::{ChemistryEngine, PythonEngine, Request},
};

fn chain(atoms: usize) -> ChainDrawing {
    ChainDrawing {
        atoms: Some(atoms),
        angle: 120.,
    }
}
fn points(atoms: usize) -> Vec<Point> {
    chains::straight(
        Point::default(),
        Point::new(250., 0.),
        false,
        BondDrawing::default(),
        chain(atoms),
        false,
    )
}

#[test]
fn fixed_and_free_constraints_apply_independently_and_chain_atoms_are_exact() {
    let start = Point::new(3., 7.);
    let cursor = Point::new(96., 48.);
    let fixed = BondDrawing::default();
    let endpoint = fixed.endpoint(start, cursor);
    assert!((start.distance(endpoint) - 42.).abs() < 0.001);
    assert!((chains::direction(start, endpoint).to_degrees() - 30.).abs() < 0.001);
    let free_length = BondDrawing {
        fixed_length: false,
        ..fixed
    }
    .endpoint(start, cursor);
    assert!((start.distance(free_length) - start.distance(cursor)).abs() < 0.001);
    let free_angle = BondDrawing {
        fixed_angles: false,
        ..fixed
    }
    .endpoint(start, cursor);
    assert!(
        (chains::direction(start, free_angle) - chains::direction(start, cursor)).abs() < 0.001
    );
    assert!(
        fixed
            .unconstrained(true)
            .endpoint(start, cursor)
            .distance(cursor)
            < 0.001
    );
    for count in [1, 2, 6, 13, 512] {
        let path = points(count);
        assert_eq!(path.len(), count);
        assert!(
            path.windows(2)
                .all(|p| (p[0].distance(p[1]) - 42.).abs() < 0.01)
        );
        for p in path.windows(3) {
            let angle = (chains::direction(p[1], p[0]) - chains::direction(p[1], p[2])).abs();
            assert!((angle.to_degrees() - 120.).abs() < 0.02);
        }
    }
    for count in [2, 6, 7] {
        let path = chains::straight(
            start,
            cursor,
            false,
            fixed.unconstrained(true),
            chain(count),
            false,
        );
        assert!(path.last().unwrap().distance(cursor) < 0.001);
    }
}

#[test]
fn snaking_turns_retraces_and_handles_fast_pointer_moves() {
    let bond = BondDrawing::default();
    let chain = ChainDrawing::default();
    let mut path = vec![Point::default()];
    chains::snake(&mut path, Point::new(260., 0.), false, bond, chain, false);
    assert!(path.len() > 5);
    let straight = path.len();
    let corner = *path.last().unwrap();
    chains::snake(
        &mut path,
        corner.offset(0., -210.),
        false,
        bond,
        chain,
        false,
    );
    assert!(path.len() > straight);
    assert!(path.last().unwrap().y < -140.);
    assert!(
        path.windows(2)
            .all(|p| (p[0].distance(p[1]) - 42.).abs() < 0.001)
    );
    let retrace = path[straight - 2];
    chains::snake(&mut path, retrace, false, bond, chain, false);
    assert_eq!(path.len(), straight - 1);
    assert_eq!(*path.last().unwrap(), retrace);
    let before = path.clone();
    chains::snake(&mut path, retrace, false, bond, chain, false);
    assert_eq!(path, before);
    let mut free = vec![Point::default()];
    let cursor = Point::new(55., 21.);
    chains::snake(
        &mut free,
        cursor,
        false,
        bond.unconstrained(true),
        chain,
        false,
    );
    assert!(free.last().unwrap().distance(cursor) < 0.001);
}

#[tokio::test]
async fn chains_attach_to_existing_atoms_and_survive_molecular_and_drawing_exchange() {
    let engine = PythonEngine::default();
    let mut doc = Document::default();
    let path = points(7);
    let n = doc.add_atom("N", path[0]);
    let o = doc.add_atom("O", *path.last().unwrap());
    let (joined, ids) = chains::place(&doc, &path, Some(n), Some(o), 8.).unwrap();
    assert_eq!((joined.atoms.len(), joined.bonds.len()), (7, 6));
    assert_eq!((ids[0], ids[6]), (n, o));
    let result = engine
        .execute(Request::molecule("analyze", joined.clone()))
        .await
        .unwrap();
    assert_eq!(result.analysis.unwrap().formula, "C5H13NO");
    for format in ["mol", "cdxml"] {
        let mut req = Request::molecule("export", joined.clone());
        req.format = Some(format.into());
        let output = engine.execute(req).await.unwrap().output.unwrap();
        let result = engine
            .execute(Request::import(format, &output))
            .await
            .unwrap();
        assert_eq!(result.analysis.unwrap().formula, "C5H13NO");
    }
    let saved = serde_json::to_string(&joined).unwrap();
    assert_eq!(serde_json::from_str::<Document>(&saved).unwrap(), joined);
    let before = doc.clone();
    let blocked = vec![path[0], *path.last().unwrap(), path[2]];
    assert!(chains::place(&doc, &blocked, Some(n), None, 8.).is_err());
    assert_eq!(doc, before);
}

#[tokio::test]
async fn extending_and_joining_grouped_molecules_keeps_captions_and_valid_cdxml() {
    let mut doc = Document::default();
    let path = points(7);
    let a = doc.add_atom("N", path[0]);
    let b = doc.add_atom("O", *path.last().unwrap());
    for (atom, text) in [(a, "Amine"), (b, "Alcohol")] {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            id,
            position: doc.atom(atom).unwrap().position.offset(0., 60.),
            text: text.into(),
            format: Default::default(),
        });
        doc.group_selection(&[atom, id]).unwrap();
    }
    let (joined, _) = chains::place(&doc, &path, Some(a), Some(b), 8.).unwrap();
    joined.validate().unwrap();
    assert_eq!(joined.groups.len(), 1);
    assert_eq!(joined.groups[0].members.len(), 9);
    let engine = PythonEngine::default();
    let mut req = Request::molecule("export", joined);
    req.format = Some("cdxml".into());
    let xml = engine.execute(req).await.unwrap().output.unwrap();
    let reopened = engine
        .execute(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!(reopened.groups.len(), 1);
    assert_eq!(reopened.groups[0].members.len(), 9);
}
