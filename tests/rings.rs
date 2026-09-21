use reshiki::{
    document::{Document, Point},
    engine::{LocalEngine, Request},
    rings::{Drawing, Preset},
    templates,
};
fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}
fn drawing(preset: Preset) -> Drawing {
    Drawing {
        preset,
        length: 42.,
        alternate: false,
        connect: false,
    }
}

#[test]
fn chair_projections_close_with_equal_jacs_edges_and_mirror_geometry() {
    let first = Preset::ChairUp.document(42., false);
    let second = Preset::ChairDown.document(42., false);
    assert_eq!((first.atoms.len(), first.bonds.len()), (6, 6));
    for doc in [&first, &second] {
        doc.validate().unwrap();
        for b in &doc.bonds {
            assert!(
                (doc.atom(b.a)
                    .unwrap()
                    .position
                    .distance(doc.atom(b.b).unwrap().position)
                    - 42.)
                    .abs()
                    < 0.001
            );
        }
        // Two inward corners distinguish the chair from a convex regular ring.
        let turns: Vec<_> = (0..6)
            .map(|i| {
                let a = doc.atoms[i].position;
                let b = doc.atoms[(i + 1) % 6].position;
                let c = doc.atoms[(i + 2) % 6].position;
                (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
            })
            .collect();
        assert_eq!(
            turns
                .iter()
                .filter(|x| **x > 0.)
                .count()
                .min(turns.iter().filter(|x| **x < 0.).count()),
            2
        );
    }
    for (a, b) in first.atoms.iter().zip(&second.atoms) {
        assert!((a.position.x - b.position.x).abs() < 0.001);
        assert!((a.position.y + b.position.y).abs() < 0.001);
    }
}

#[tokio::test]
async fn fusion_and_atom_sharing_keep_host_coordinates_and_molecular_identity() {
    let engine = LocalEngine::default();
    for preset in [Preset::ChairUp, Preset::ChairDown, Preset::Cyclopentadiene] {
        let mut host = Document::default();
        let a = host.add_atom("C", p(-42., 0.));
        let b = host.add_atom("C", p(42., 0.));
        host.add_bond(a, b, 1, "plain");
        for (anchor, expected) in [
            (
                p(0., 0.),
                if preset == Preset::Cyclopentadiene {
                    "C5H6"
                } else {
                    "C6H12"
                },
            ),
            (
                p(42., 0.),
                if preset == Preset::Cyclopentadiene {
                    "C6H8"
                } else {
                    "C7H14"
                },
            ),
        ] {
            let (doc, _) = drawing(preset)
                .place(&host, anchor, Some(p(0., 200.)), 5.)
                .unwrap();
            assert_eq!(doc.atom(a).unwrap().position, p(-42., 0.));
            assert_eq!(doc.atom(b).unwrap().position, p(42., 0.));
            for bond in &doc.bonds {
                assert!(
                    (doc.atom(bond.a)
                        .unwrap()
                        .position
                        .distance(doc.atom(bond.b).unwrap().position)
                        - 84.)
                        .abs()
                        < 0.02
                );
            }
            let result = engine
                .request(Request::molecule("analyze", doc))
                .await
                .unwrap();
            assert_eq!(result.analysis.unwrap().formula, expected);
        }
    }
}

#[tokio::test]
async fn connecting_by_a_bond_keeps_the_whole_ring_and_rejects_full_valence() {
    let engine = LocalEngine::default();
    let mut host = Document::default();
    let c = host.add_atom("C", p(0., 0.));
    let o = host.add_atom("O", p(42., 0.));
    host.add_bond(c, o, 1, "plain");
    let mut tool = drawing(Preset::ChairUp);
    tool.connect = true;
    let (doc, ids) = tool
        .place(&host, p(42., 0.), Some(p(140., 0.)), 5.)
        .unwrap();
    assert_eq!((doc.atoms.len(), doc.bonds.len()), (8, 8));
    assert!(ids.contains(&o));
    assert_eq!(
        doc.atom(o).unwrap().position,
        host.atom(o).unwrap().position
    );
    let result = engine
        .request(Request::molecule("analyze", doc.clone()))
        .await
        .unwrap();
    assert_eq!(result.analysis.unwrap().smiles, "COC1CCCCC1");
    assert!(tool.place(&doc, p(42., 0.), None, 5.).is_err());
    assert!(tool.place(&host, p(21., 0.), None, 5.).is_err());
    assert_eq!(host.atoms.len(), 2);
}

#[tokio::test]
async fn orientation_alternate_bonds_and_cdxml_preserve_the_drawing() {
    let engine = LocalEngine::default();
    let empty = Document::default();
    let (doc, _) = drawing(Preset::ChairDown)
        .place(&empty, p(120., 80.), Some(p(120., 160.)), 5.)
        .unwrap();
    let (lo, hi) = doc.bounds();
    assert!(hi.y - lo.y > hi.x - lo.x);
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some("cdxml".into());
    let xml = engine.request(request).await.unwrap().output.unwrap();
    let restored = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    for (a, b) in doc.atoms.iter().zip(&restored.atoms) {
        assert!(
            (a.position.x
                - doc.atoms[0].position.x
                - (b.position.x - restored.atoms[0].position.x))
                .abs()
                < 0.01
        );
        assert!(
            (a.position.y
                - doc.atoms[0].position.y
                - (b.position.y - restored.atoms[0].position.y))
                .abs()
                < 0.01
        );
    }
    for alternate in [false, true] {
        let doc = Preset::Cyclopentadiene.document(42., alternate);
        let doubles: Vec<_> = doc
            .bonds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.order == 2)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(doubles, if alternate { vec![1, 3] } else { vec![0, 2] });
        assert_eq!(
            engine
                .request(Request::molecule("analyze", doc))
                .await
                .unwrap()
                .analysis
                .unwrap()
                .formula,
            "C5H6"
        );
    }
}

#[test]
fn builtin_chairs_use_the_same_molecular_geometry_as_the_toolbar() {
    for preset in [Preset::ChairUp, Preset::ChairDown] {
        let t = templates::LIBRARY
            .iter()
            .find(|t| t.name == format!("Cyclohexane · {preset}"))
            .unwrap();
        assert_eq!(t.document, preset.document(42., false));
        let json = serde_json::to_string(&t.document).unwrap();
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), t.document);
    }
}
