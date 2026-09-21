use reshiki::{
    document::{Annotation, Document, Point},
    editing,
    engine::{ChemistryEngine, LocalEngine, Request},
    joining::Prepared,
    templates::{Anchor, Connection, LIBRARY},
};

async fn molecule(engine: &LocalEngine, smiles: &str) -> Document {
    engine
        .execute(Request::import_smiles(smiles))
        .await
        .unwrap()
        .document
        .unwrap()
}
async fn identity(engine: &LocalEngine, doc: Document) -> String {
    engine
        .execute(Request::molecule("analyze", doc))
        .await
        .unwrap()
        .analysis
        .unwrap()
        .smiles
}
fn template(name: &str) -> Document {
    LIBRARY
        .iter()
        .find(|t| t.name == name)
        .unwrap()
        .document
        .clone()
}
fn midpoint(doc: &Document, a: u64, b: u64) -> Point {
    let p = doc.atom(a).unwrap().position;
    let q = doc.atom(b).unwrap().position;
    Point::new((p.x + q.x) / 2., (p.y + q.y) / 2.)
}

#[tokio::test]
async fn connecting_and_sharing_existing_fragments_preserve_ids_and_unrelated_content() {
    let engine = LocalEngine::default();
    let host = molecule(&engine, "CO").await;
    let part = molecule(&engine, "CC").await;
    for (mode, expected, atom_count) in [
        (Connection::Connect, "CCCO", 4),
        (Connection::ShareAtom, "CCO", 3),
    ] {
        let mut doc = host.clone();
        let moving = editing::append(&mut doc, &part, Point::new(200., 100.));
        let note = doc.next_id();
        doc.annotations.push(Annotation {
            id: note,
            position: Point::new(-100., 100.),
            text: "Unrelated note".into(),
            format: Default::default(),
        });
        let original = doc.clone();
        // Selecting only one endpoint expands to the connected fragment.
        let prepared = Prepared::new(&doc, &[moving[0]]).unwrap();
        assert_eq!(prepared.fragment.atoms.len(), 2);
        let target = host.atoms.iter().find(|a| a.element == "C").unwrap();
        let (joined, selected) = prepared
            .place(target.position, None, 5., Anchor::Atom(moving[0]), mode)
            .unwrap();
        assert_eq!(joined.atoms.len(), atom_count);
        assert_eq!(joined.annotations, doc.annotations);
        for atom in &host.atoms {
            assert_eq!(joined.atom(atom.id).unwrap().position, atom.position);
        }
        assert_eq!(
            joined
                .all_ids()
                .iter()
                .filter(|id| !doc.all_ids().contains(id))
                .count(),
            0
        );
        assert!(selected.contains(&moving[1]));
        assert_eq!(identity(&engine, joined).await, expected);
        assert_eq!(doc, original);
    }
}

#[tokio::test]
async fn fused_aromatic_fragments_keep_their_ids_and_captions() {
    let engine = LocalEngine::default();
    let mut doc = template("Benzene");
    let host = doc.clone();
    let part = template("Furan");
    let mut moving = editing::append(&mut doc, &part, Point::new(230., 120.));
    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(230., 200.),
        text: "Fused product".into(),
        format: Default::default(),
    });
    moving.push(caption);
    doc.group_selection(&moving).unwrap();
    let prepared = Prepared::new(&doc, &moving).unwrap();
    let source = prepared
        .fragment
        .bonds
        .iter()
        .find(|b| {
            b.order == 1
                && prepared.fragment.atom(b.a).unwrap().element == "C"
                && prepared.fragment.atom(b.b).unwrap().element == "C"
        })
        .unwrap();
    let target = host.bonds.iter().find(|b| b.order == 1).unwrap();
    let (joined, ids) = prepared
        .place(
            midpoint(&host, target.a, target.b),
            None,
            4.,
            Anchor::Bond(source.a, source.b),
            Connection::FuseBond,
        )
        .unwrap();
    assert_eq!(joined.atoms.len(), 9);
    assert!(
        joined
            .annotations
            .iter()
            .any(|a| a.id == caption && a.text == "Fused product")
    );
    assert!(ids.contains(&target.a) && ids.contains(&target.b));
    assert!(
        joined
            .groups
            .iter()
            .any(|g| g.members.contains(&caption) && g.members.contains(&target.a))
    );
    assert_eq!(
        identity(&engine, joined).await,
        identity(&engine, molecule(&engine, "c1ccc2cocc2c1").await).await
    );
}

#[test]
fn single_atoms_and_edges_can_merge_without_duplicate_objects() {
    let mut doc = Document::default();
    let host = doc.add_atom("C", Point::default());
    let source = doc.add_atom("C", Point::new(100., 0.));
    let label = doc.next_id();
    doc.annotations.push(Annotation {
        id: label,
        position: Point::new(100., 40.),
        text: "Carbon".into(),
        format: Default::default(),
    });
    doc.group_selection(&[source, label]).unwrap();
    let prepared = Prepared::new(&doc, &[source, label]).unwrap();
    let (joined, ids) = prepared
        .place(
            Point::default(),
            None,
            5.,
            Anchor::Atom(source),
            Connection::ShareAtom,
        )
        .unwrap();
    assert_eq!(joined.atoms.len(), 1);
    assert_eq!(joined.atoms[0].id, host);
    assert!(ids.contains(&host) && ids.contains(&label));
    assert_eq!(joined.annotations[0].position, Point::new(0., 40.));
    assert_eq!(joined.groups[0].members, vec![host, label]);
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(100., 0.));
    let d = doc.add_atom("C", Point::new(142., 0.));
    doc.add_bond(c, d, 1, "plain");
    let prepared = Prepared::new(&doc, &[c, d]).unwrap();
    let (joined, ids) = prepared
        .place(
            Point::new(21., 0.),
            None,
            5.,
            Anchor::Bond(c, d),
            Connection::FuseBond,
        )
        .unwrap();
    assert_eq!((joined.atoms.len(), joined.bonds.len()), (2, 1));
    assert!(ids.contains(&a) && ids.contains(&b));
}

#[tokio::test]
async fn moving_a_fragment_preserves_remote_tetrahedral_stereochemistry() {
    let engine = LocalEngine::default();
    let mut doc = molecule(&engine, "O").await;
    let source = molecule(&engine, "CC[C@H](F)Cl").await;
    let ids = editing::append(&mut doc, &source, Point::new(200., 100.));
    let source_atom = source
        .atoms
        .iter()
        .find(|a| {
            a.element == "C"
                && source
                    .bonds
                    .iter()
                    .filter(|b| b.a == a.id || b.b == a.id)
                    .count()
                    == 1
        })
        .unwrap();
    let original_ids = source.all_ids();
    let anchor = ids[original_ids
        .iter()
        .position(|id| *id == source_atom.id)
        .unwrap()];
    let prepared = Prepared::new(&doc, &ids).unwrap();
    let (joined, _) = prepared
        .place(
            doc.atoms[0].position,
            Some(Point::new(0., -100.)),
            5.,
            Anchor::Atom(anchor),
            Connection::Connect,
        )
        .unwrap();
    let expected = molecule(&engine, "OCC[C@H](F)Cl").await;
    assert_eq!(
        identity(&engine, joined).await,
        identity(&engine, expected).await
    );
}

#[test]
fn invalid_or_incompatible_joins_never_change_the_document() {
    let mut doc = Document::default();
    let oxygen = doc.add_atom("O", Point::default());
    let carbon = doc.add_atom("C", Point::new(100., 0.));
    let before = doc.clone();
    let prepared = Prepared::new(&doc, &[carbon]).unwrap();
    assert!(
        prepared
            .place(
                Point::default(),
                None,
                5.,
                Anchor::Atom(carbon),
                Connection::ShareAtom
            )
            .is_err()
    );
    assert!(
        prepared
            .place(
                Point::new(500., 0.),
                None,
                5.,
                Anchor::Atom(carbon),
                Connection::Connect
            )
            .is_err()
    );
    assert!(
        prepared
            .place(
                Point::default(),
                None,
                5.,
                Anchor::Atom(u64::MAX),
                Connection::Connect
            )
            .is_err()
    );
    assert!(
        prepared
            .place(
                Point::default(),
                None,
                f32::NAN,
                Anchor::Atom(carbon),
                Connection::Connect
            )
            .is_err()
    );
    assert!(Prepared::new(&doc, &[u64::MAX]).is_err());
    assert!(Prepared::new(&doc, &[oxygen, carbon]).is_err());
    assert_eq!(doc, before);
}
