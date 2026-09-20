use reshiki::{
    document::{Document, Point},
    engine::{ChemistryEngine, PythonEngine, Request},
    templates::{LIBRARY, place},
};

fn template(name: &str) -> &'static Document {
    &LIBRARY.iter().find(|t| t.name == name).unwrap().document
}

#[tokio::test]
async fn every_thumbnail_is_the_structure_that_gets_placed() {
    let engine = PythonEngine::default();
    for item in LIBRARY.iter() {
        let empty = Document::default();
        let (doc, ids) = place(&empty, &item.document, Point::new(150.0, 90.0), None, 5.0).unwrap();
        doc.validate().unwrap();
        assert_eq!(ids.len(), item.document.atoms.len());
        assert!(empty.atoms.is_empty());
        let expected = engine
            .execute(Request::import_smiles(&item.smiles))
            .await
            .unwrap()
            .analysis
            .unwrap();
        let actual = engine
            .execute(Request::molecule("analyze", doc))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(actual.smiles, expected.smiles, "{}", item.name);
    }
}

#[tokio::test]
async fn fused_templates_reuse_atoms_match_scale_and_keep_chemical_identity() {
    let engine = PythonEngine::default();
    for (name, order, expected) in [
        ("Cyclopentane", 1, "C1CCCC1"),
        ("Cyclopentane", 2, "C1=CCCC1"),
        ("Cyclohexane", 2, "C1=CCCCC1"),
        ("Pyridine", 1, "c1ccncc1"),
        ("Furan", 1, "c1ccoc1"),
        ("Benzene", 2, "c1ccccc1"),
    ] {
        for angle in [0.0_f32, 0.7, 1.9] {
            let mut original = Document::default();
            let a = original.add_atom("C", Point::new(20.0, 30.0));
            let b = original.add_atom(
                "C",
                Point::new(20.0 + 70.0 * angle.cos(), 30.0 + 70.0 * angle.sin()),
            );
            original.add_bond(a, b, order, "plain");
            let p = original.atom(a).unwrap().position;
            let q = original.atom(b).unwrap().position;
            let (placed, ids) = place(
                &original,
                template(name),
                Point::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0),
                None,
                5.0,
            )
            .unwrap();
            placed.validate().unwrap();
            assert_eq!(placed.atoms.len(), template(name).atoms.len());
            assert_eq!(ids.len(), placed.atoms.len());
            assert_eq!(placed.atom(a).unwrap().position, p);
            assert_eq!(placed.atom(b).unwrap().position, q);
            for bond in &placed.bonds {
                let length = placed
                    .atom(bond.a)
                    .unwrap()
                    .position
                    .distance(placed.atom(bond.b).unwrap().position);
                assert!((length - 70.0).abs() < 0.01);
            }
            let result = engine
                .execute(Request::molecule("analyze", placed))
                .await
                .unwrap()
                .analysis
                .unwrap();
            assert_eq!(result.smiles, expected, "{name}");
        }
    }
}

#[tokio::test]
async fn atom_attachment_and_fused_aromatics_are_connected() {
    let engine = PythonEngine::default();
    let mut chain = Document::default();
    let a = chain.add_atom("C", Point::default());
    let b = chain.add_atom("C", Point::new(42.0, 0.0));
    chain.add_bond(a, b, 1, "plain");
    let (methyl, _) = place(
        &chain,
        template("Cyclohexane"),
        Point::new(42.0, 0.0),
        None,
        5.0,
    )
    .unwrap();
    assert_eq!(
        engine
            .execute(Request::molecule("analyze", methyl))
            .await
            .unwrap()
            .analysis
            .unwrap()
            .smiles,
        "CC1CCCCC1"
    );
    let benzene = template("Benzene");
    let bond = benzene.bonds.iter().find(|b| b.order == 2).unwrap();
    let a = benzene.atom(bond.a).unwrap().position;
    let b = benzene.atom(bond.b).unwrap().position;
    let (naphthalene, _) = place(
        benzene,
        benzene,
        Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        None,
        5.0,
    )
    .unwrap();
    let analysis = engine
        .execute(Request::molecule("analyze", naphthalene))
        .await
        .unwrap()
        .analysis
        .unwrap();
    assert_eq!(analysis.formula, "C10H8");
    assert_eq!(analysis.smiles, "c1ccc2ccccc2c1");
}

#[test]
fn dragging_selects_attachment_side_and_invalid_targets_are_unchanged() {
    let mut original = Document::default();
    let a = original.add_atom("C", Point::new(-30.0, 0.0));
    let b = original.add_atom("C", Point::new(30.0, 0.0));
    original.add_bond(a, b, 1, "plain");
    let before = original.clone();
    for sign in [-1.0, 1.0] {
        let (doc, _) = place(
            &original,
            template("Cyclopentane"),
            Point::default(),
            Some(Point::new(0.0, 100.0 * sign)),
            5.0,
        )
        .unwrap();
        assert!(
            doc.atoms
                .iter()
                .filter(|x| x.id != a && x.id != b)
                .all(|x| x.position.y * sign > 1.0)
        );
    }
    assert_eq!(original, before);
    original.add_bond(a, b, 3, "plain");
    let before = original.clone();
    assert!(place(&original, template("Benzene"), Point::default(), None, 5.0).is_err());
    assert_eq!(original, before);
    // A saturated oxygen cannot acquire two ring bonds or silently become carbon.
    original.atom_mut(a).unwrap().element = "O".into();
    assert!(
        place(
            &original,
            template("Cyclohexane"),
            Point::new(-30.0, 0.0),
            None,
            5.0
        )
        .is_err()
    );
}

#[tokio::test]
async fn explicit_atom_connection_keeps_both_rings_and_exact_source_atom() {
    use reshiki::templates::{Anchor, Connection, place_with_mode};
    let engine = PythonEngine::default();
    let benzene = template("Benzene");
    let furan = template("Furan");
    let target = benzene.atoms.first().unwrap();
    let mut isomers = std::collections::BTreeSet::new();
    for source in furan.atoms.iter().filter(|a| a.element == "C") {
        let (result, ids) = place_with_mode(
            benzene,
            furan,
            target.position,
            None,
            5.,
            Anchor::Atom(source.id),
            Connection::Connect,
        )
        .unwrap();
        assert_eq!(result.atoms.len(), 11);
        assert_eq!(result.bonds.len(), 12);
        for atom in &benzene.atoms {
            assert_eq!(result.atom(atom.id).unwrap().position, atom.position);
        }
        let mapped = furan
            .all_ids()
            .iter()
            .zip(&ids)
            .find(|(id, _)| **id == source.id)
            .unwrap()
            .1;
        assert!(result.bonds.iter().any(|b| b.order == 1
            && ((b.a == target.id && b.b == *mapped) || (b.b == target.id && b.a == *mapped))));
        let analysis = engine
            .execute(Request::molecule("analyze", result))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(analysis.formula, "C10H8O");
        isomers.insert(analysis.inchikey);
    }
    assert_eq!(
        isomers.len(),
        2,
        "The chosen carbon distinguishes 2- and 3-phenylfuran"
    );
    let oxygen = furan.atoms.iter().find(|a| a.element == "O").unwrap();
    assert!(
        place_with_mode(
            benzene,
            furan,
            target.position,
            None,
            5.,
            Anchor::Atom(oxygen.id),
            Connection::Connect
        )
        .is_err()
    );
}

#[tokio::test]
async fn chosen_aromatic_edges_fuse_regardless_of_kekule_phase() {
    use reshiki::templates::{Anchor, Connection, place_with_mode};
    let engine = PythonEngine::default();
    let benzene = template("Benzene");
    for name in ["Furan", "Benzene"] {
        let part = template(name);
        for source in &part.bonds {
            if [source.a, source.b]
                .iter()
                .any(|id| part.atom(*id).unwrap().element != "C")
            {
                continue;
            }
            let mut identities = std::collections::BTreeSet::new();
            for target in &benzene.bonds {
                let a = benzene.atom(target.a).unwrap().position;
                let b = benzene.atom(target.b).unwrap().position;
                let point = Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                let (result, _) = place_with_mode(
                    benzene,
                    part,
                    point,
                    None,
                    5.,
                    Anchor::Bond(source.a, source.b),
                    Connection::FuseBond,
                )
                .unwrap();
                assert_eq!(
                    result.atoms.len(),
                    benzene.atoms.len() + part.atoms.len() - 2
                );
                assert_eq!(
                    result.bonds.len(),
                    benzene.bonds.len() + part.bonds.len() - 1
                );
                for atom in &benzene.atoms {
                    assert_eq!(result.atom(atom.id).unwrap().position, atom.position);
                }
                let analysis = engine
                    .execute(Request::molecule("analyze", result))
                    .await
                    .unwrap()
                    .analysis
                    .unwrap();
                assert_eq!(
                    analysis.formula,
                    if name == "Furan" { "C8H6O" } else { "C10H8" }
                );
                identities.insert(analysis.inchikey);
            }
            assert_eq!(
                identities.len(),
                1,
                "Destination single/double phase must not alter product identity"
            );
        }
    }
}

#[tokio::test]
async fn circle_benzene_fuses_at_the_chosen_furan_edge_and_undo_restores_the_circle() {
    use reshiki::{
        document::History,
        editing,
        templates::{Anchor, Connection, place_with_mode},
    };
    let engine = PythonEngine::default();
    let mut original = Document::default();
    editing::ring(&mut original, Point::new(100., 100.), 6, true, 5.);
    assert_eq!(reshiki::aromatic::circles(&original).len(), 1);
    let furan = template("Furan");
    let source = Anchor::Bond(2, 3);
    for target in &original.bonds {
        let a = original.atom(target.a).unwrap().position;
        let b = original.atom(target.b).unwrap().position;
        let point = Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let direction = Some(point.offset(point.x - 100., point.y - 100.));
        let (mut fused, ids) = place_with_mode(
            &original,
            furan,
            point,
            direction,
            5.,
            source,
            Connection::FuseBond,
        )
        .unwrap();
        assert_eq!(fused.atoms.len(), 9);
        assert_eq!(fused.bonds.len(), 10);
        assert!(fused.bonds.iter().all(|b| matches!(b.order, 1 | 2)));
        let mapping: std::collections::HashMap<_, _> =
            furan.all_ids().into_iter().zip(ids).collect();
        assert_eq!(
            std::collections::BTreeSet::from([mapping[&2], mapping[&3]]),
            std::collections::BTreeSet::from([target.a, target.b]),
            "The selected source edge must be the shared edge"
        );
        for atom in &original.atoms {
            assert_eq!(fused.atom(atom.id).unwrap().position, atom.position);
        }
        let analysis = engine
            .execute(Request::molecule("analyze", fused.clone()))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(analysis.formula, "C8H6O");
        assert_eq!(analysis.smiles, "c1ccc2occc2c1");
        let expected = fused.clone();
        let mut history = History::default();
        assert!(history.commit(original.clone(), &fused));
        assert!(history.undo(&mut fused));
        assert_eq!(fused, original);
        assert!(!history.can_undo());
        assert!(history.redo(&mut fused));
        assert_eq!(fused, expected);
    }
    assert!(original.bonds.iter().all(|b| b.order == 4));
}

#[test]
fn unsupported_circle_fusion_targets_and_anchors_remain_unchanged() {
    use reshiki::{
        editing,
        templates::{Anchor, Connection, place_with_mode},
    };
    let mut ring = Document::default();
    editing::ring(&mut ring, Point::default(), 6, true, 5.);
    let target = ring.bonds.first().unwrap();
    let a = ring.atom(target.a).unwrap().position;
    let b = ring.atom(target.b).unwrap().position;
    let point = Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let mut partial = ring.clone();
    partial.bonds.last_mut().unwrap().order = 1;
    let mut charged = ring.clone();
    charged.atom_mut(target.a).unwrap().charge = 1;
    let mut full_valence = ring.clone();
    let methyl = full_valence.add_atom("C", a.offset(80., 0.));
    full_valence.add_bond(target.a, methyl, 1, "plain");
    let mut larger_aromatic_system = ring.clone();
    let outside = larger_aromatic_system.add_atom("C", a.offset(80., 0.));
    larger_aromatic_system.add_bond(target.a, outside, 4, "plain");
    for (doc, anchor) in [
        (partial, Anchor::Bond(2, 3)),
        (charged, Anchor::Bond(2, 3)),
        (full_valence, Anchor::Bond(2, 3)),
        (larger_aromatic_system, Anchor::Bond(2, 3)),
        (ring, Anchor::Bond(3, 4)), // Oxygen cannot replace the chosen carbon.
    ] {
        let before = doc.clone();
        assert!(
            place_with_mode(
                &doc,
                template("Furan"),
                point,
                None,
                5.,
                anchor,
                Connection::FuseBond,
            )
            .is_err()
        );
        assert_eq!(doc, before);
    }
}

#[test]
fn connected_phenyl_ring_has_120_degree_angles_at_every_source_vertex() {
    use reshiki::{
        editing,
        templates::{Anchor, Connection, place_with_mode},
    };
    let benzene = template("Benzene");
    for rotation in [0., 13.7, 41.25, 99.1] {
        let mut furan = template("Furan").clone();
        let ids = furan.all_ids();
        editing::transform_about(&mut furan, &ids, Point::default(), 1., rotation);
        for target in furan.atoms.iter().filter(|a| a.element == "C") {
            for source in &benzene.atoms {
                for direction in [None, Some(target.position.offset(117., -83.))] {
                    let (joined, ids) = place_with_mode(
                        &furan,
                        benzene,
                        target.position,
                        direction,
                        5.,
                        Anchor::Atom(source.id),
                        Connection::Connect,
                    )
                    .unwrap();
                    let mapping: std::collections::HashMap<_, _> =
                        benzene.all_ids().into_iter().zip(ids).collect();
                    let anchor = joined.atom(mapping[&source.id]).unwrap().position;
                    let link =
                        Point::new(target.position.x - anchor.x, target.position.y - anchor.y);
                    for bond in benzene
                        .bonds
                        .iter()
                        .filter(|b| b.a == source.id || b.b == source.id)
                    {
                        let other = if bond.a == source.id { bond.b } else { bond.a };
                        let p = joined.atom(mapping[&other]).unwrap().position;
                        let v = Point::new(p.x - anchor.x, p.y - anchor.y);
                        let cosine = (v.x * link.x + v.y * link.y)
                            / (p.distance(anchor) * target.position.distance(anchor));
                        assert!(
                            (cosine + 0.5).abs() < 0.0001,
                            "The connecting bond must form 120° with both phenyl edges, got {}°",
                            cosine.acos().to_degrees()
                        );
                    }
                    for atom in &furan.atoms {
                        assert_eq!(joined.atom(atom.id).unwrap().position, atom.position);
                    }
                }
            }
        }
    }
}
