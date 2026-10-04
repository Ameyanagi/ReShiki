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

fn reaction_join_drawing() -> Document {
    use reshiki::{
        document::Arrow,
        reactions::{self, Role},
    };
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(180., 0.));
    let d = doc.add_atom("C", Point::new(222., 0.));
    doc.add_bond(c, d, 1, "plain");
    doc.arrows.push(Arrow::new(
        5,
        Point::new(80., 60.),
        Point::new(140., 60.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut doc, 5, &[c], Role::Reactant).unwrap();
    doc.reactions[0].reactants[0].coefficient = 7;
    doc
}

fn reaction_join_target(mode: Connection) -> (Point, Anchor) {
    if mode == Connection::FuseBond {
        (Point::new(21., 0.), Anchor::Bond(3, 4))
    } else {
        (Point::default(), Anchor::Atom(3))
    }
}

#[test]
fn joining_preserves_reaction_membership_and_coefficient() {
    use reshiki::reactions::{self, Role};
    for role in Role::ALL {
        for mode in [
            Connection::Connect,
            Connection::ShareAtom,
            Connection::FuseBond,
        ] {
            let mut doc = reaction_join_drawing();
            if role != Role::Reactant {
                reactions::assign(&mut doc, 5, &[3], role).unwrap();
                doc.reactions[0].participants_mut(role)[0].coefficient = 7;
            }
            let original = doc.clone();
            let prepared = Prepared::new(&doc, &[3]).unwrap();
            let (point, anchor) = reaction_join_target(mode);
            let (joined, _) = prepared.place(point, None, 5., anchor, mode).unwrap();
            joined.validate().unwrap();
            assert_eq!(doc, original);
            assert_eq!(joined.reactions.len(), 1);
            assert_eq!(joined.reactions[0].arrow, 5);
            let participants = joined.reactions[0].participants(role);
            assert_eq!(participants.len(), 1, "{role:?} {mode:?}");
            assert_eq!(participants[0].coefficient, 7);
            let mut expected: Vec<_> = joined.atoms.iter().map(|atom| atom.id).collect();
            expected.sort_unstable();
            assert_eq!(participants[0].atoms, expected);
        }
    }
}

#[test]
fn joining_rejects_merging_distinct_reaction_participants_atomically() {
    use reshiki::reactions::{self, Role};
    for target_role in Role::ALL {
        for mode in [
            Connection::Connect,
            Connection::ShareAtom,
            Connection::FuseBond,
        ] {
            let mut doc = reaction_join_drawing();
            reactions::assign(&mut doc, 5, &[1], target_role).unwrap();
            let original = doc.clone();
            let prepared = Prepared::new(&doc, &[3]).unwrap();
            let (point, anchor) = reaction_join_target(mode);
            let error = prepared.place(point, None, 5., anchor, mode).unwrap_err();
            assert_eq!(
                error,
                "This joins separate reaction participants. Clear their reaction roles before joining them."
            );
            assert_eq!(doc, original);
            assert_eq!(prepared.original, original);
        }
    }
}

#[test]
fn joining_restores_moved_reaction_arrow_and_caption_references() {
    for mode in [
        Connection::Connect,
        Connection::ShareAtom,
        Connection::FuseBond,
    ] {
        let mut doc = reaction_join_drawing();
        doc.annotations.push(Annotation {
            id: 6,
            position: Point::new(180., 80.),
            text: "Conditions".into(),
            format: Default::default(),
        });
        doc.reactions[0].annotations.push(6);
        let original = doc.clone();
        let prepared = Prepared::new(&doc, &[3, 5, 6]).unwrap();
        let (point, anchor) = reaction_join_target(mode);
        let (joined, selected) = prepared.place(point, None, 5., anchor, mode).unwrap();
        joined.validate().unwrap();
        assert_eq!(joined.reactions[0].arrow, 5);
        assert_eq!(joined.reactions[0].annotations, [6]);
        assert_eq!(joined.reactions[0].reactants[0].coefficient, 7);
        let mut expected: Vec<_> = joined.atoms.iter().map(|atom| atom.id).collect();
        expected.sort_unstable();
        assert_eq!(joined.reactions[0].reactants[0].atoms, expected, "{mode:?}");
        assert!(selected.contains(&5) && selected.contains(&6));
        assert_eq!(joined.annotations[0].id, 6);
        assert_eq!(joined.annotations[0].text, "Conditions");
        assert_eq!(doc, original);
    }
}

#[test]
fn joining_preserves_salt_components_and_rejects_participant_conflicts() {
    use reshiki::reactions::{self, Role};
    for mode in [
        Connection::Connect,
        Connection::ShareAtom,
        Connection::FuseBond,
    ] {
        for conflict in [false, true] {
            let mut doc = reaction_join_drawing();
            let counterion = doc.add_atom("Na", Point::new(400., 120.));
            doc.atom_mut(counterion).unwrap().charge = 1;
            doc.reactions[0].reactants[0].atoms.push(counterion);
            if conflict {
                reactions::assign(&mut doc, 5, &[1], Role::Product).unwrap();
            }
            doc.validate().unwrap();
            let original = doc.clone();
            let prepared = Prepared::new(&doc, &[3]).unwrap();
            assert!(prepared.fragment.atom(counterion).is_none());
            assert!(prepared.base.atom(counterion).is_some());
            let (point, anchor) = reaction_join_target(mode);
            let result = prepared.place(point, None, 5., anchor, mode);
            if conflict {
                assert_eq!(
                    result.unwrap_err(),
                    "This joins separate reaction participants. Clear their reaction roles before joining them."
                );
            } else {
                let (joined, _) = result.unwrap();
                joined.validate().unwrap();
                assert_eq!(joined.reactions.len(), 1);
                assert_eq!(joined.reactions[0].reactants.len(), 1);
                let participant = &joined.reactions[0].reactants[0];
                assert_eq!(participant.coefficient, 7);
                let mut expected: Vec<_> = joined.atoms.iter().map(|atom| atom.id).collect();
                expected.sort_unstable();
                assert_eq!(participant.atoms, expected, "{mode:?}");
                assert_eq!(reactions::molecules(&joined, &participant.atoms).len(), 2);
                assert_eq!(joined.atom(counterion), original.atom(counterion));
            }
            assert_eq!(doc, original);
            assert_eq!(prepared.original, original);
        }
    }
}

#[test]
fn joining_allows_overlap_between_different_reaction_arrows() {
    use reshiki::{
        document::Arrow,
        reactions::{self, Role},
    };
    for mode in [
        Connection::Connect,
        Connection::ShareAtom,
        Connection::FuseBond,
    ] {
        let mut doc = reaction_join_drawing();
        let destination_arrow = doc.next_id();
        doc.arrows.push(Arrow::new(
            destination_arrow,
            Point::new(80., -60.),
            Point::new(140., -60.),
            Default::default(),
            Default::default(),
        ));
        reactions::assign(&mut doc, destination_arrow, &[1], Role::Product).unwrap();
        doc.reactions[1].products[0].coefficient = 11;
        let mut captions = vec![];
        for (position, text) in [
            (Point::new(180., 80.), "Source conditions"),
            (Point::new(180., 100.), "Source yield"),
            (Point::new(-100., 80.), "Destination conditions"),
        ] {
            let id = doc.next_id();
            doc.annotations.push(Annotation {
                id,
                position,
                text: text.into(),
                format: Default::default(),
            });
            captions.push(id);
        }
        doc.reactions[0].annotations = vec![captions[1], captions[0]];
        doc.reactions[1].annotations = vec![captions[2]];
        doc.validate().unwrap();
        let original = doc.clone();
        let prepared = Prepared::new(&doc, &[3, 5, captions[0], captions[1]]).unwrap();
        let (point, anchor) = reaction_join_target(mode);
        let (joined, selected) = prepared.place(point, None, 5., anchor, mode).unwrap();
        joined.validate().unwrap();
        let mut atoms: Vec<_> = joined.atoms.iter().map(|atom| atom.id).collect();
        atoms.sort_unstable();
        let mut expected = original.reactions.clone();
        expected[0].reactants[0].atoms = atoms.clone();
        expected[1].products[0].atoms = atoms;
        assert_eq!(joined.reactions, expected, "{mode:?}");
        assert!(selected.contains(&5));
        for caption in &original.annotations {
            let actual = joined
                .annotations
                .iter()
                .find(|a| a.id == caption.id)
                .unwrap();
            assert_eq!(actual.text, caption.text);
            assert_eq!(actual.format, caption.format);
            if caption.id == captions[2] {
                assert_eq!(actual, caption);
            } else {
                assert!(selected.contains(&caption.id));
            }
        }
        assert_eq!(doc, original);
        assert_eq!(prepared.original, original);
    }
}
