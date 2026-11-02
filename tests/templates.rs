use reshiki::{
    document::{Document, Point},
    engine::{ChemistryEngine, LocalEngine, Request},
    templates::{LIBRARY, place},
};

fn template(name: &str) -> &'static Document {
    &LIBRARY.iter().find(|t| t.name == name).unwrap().document
}

#[tokio::test]
async fn every_thumbnail_is_the_structure_that_gets_placed()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    let catalog: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../assets/template-catalog.json"))?;
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
        if let Some(facts) = catalog
            .iter()
            .find(|f| f.get("name").and_then(|v| v.as_str()) == Some(item.name.as_str()))
        {
            assert_eq!(
                actual.formula,
                facts
                    .get("formula")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing formula")?,
                "{}",
                item.name
            );
            assert_eq!(
                actual.inchikey,
                facts
                    .get("depiction_inchikey")
                    .or_else(|| facts.get("inchikey"))
                    .and_then(|v| v.as_str())
                    .ok_or("Missing InChIKey")?,
                "{}",
                item.name
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn fused_templates_reuse_atoms_match_scale_and_keep_chemical_identity() {
    let engine = LocalEngine::default();
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
    let engine = LocalEngine::default();
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
    let engine = LocalEngine::default();
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
    let engine = LocalEngine::default();
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
    let engine = LocalEngine::default();
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

fn numbered_template(name: &str) -> &'static reshiki::templates::Template {
    LIBRARY.iter().find(|t| t.name == name).unwrap()
}

fn locant_atom<'a>(
    item: &'a reshiki::templates::Template,
    locant: &str,
) -> &'a reshiki::document::Atom {
    let id = item
        .locants
        .iter()
        .find_map(|(id, number)| (number == locant).then_some(*id))
        .unwrap_or_else(|| panic!("{} has no locant {locant}", item.name));
    item.document.atom(id).unwrap()
}

fn locants_are_bonded(item: &reshiki::templates::Template, a: &str, b: &str) -> bool {
    let a = locant_atom(item, a).id;
    let b = locant_atom(item, b).id;
    item.document
        .bonds
        .iter()
        .any(|edge| (edge.a == a && edge.b == b) || (edge.a == b && edge.b == a))
}

#[test]
fn ring_locants_cover_exactly_ring_atoms_without_duplicates() {
    use std::collections::HashSet;

    for item in LIBRARY.iter().filter(|t| {
        matches!(
            t.group.as_str(),
            "Rings"
                | "Cycloalkanes"
                | "Unsaturated rings"
                | "Heterocycles"
                | "Bicyclics and cages"
                | "Nucleobases"
        ) || (t.group == "Macrocycles" && !t.name.starts_with("Porphine"))
            || t.name == "1,10-Phenanthroline"
    }) {
        assert!(!item.locants.is_empty(), "{} needs ring locants", item.name);
        let labels: HashSet<_> = item.locants.values().collect();
        assert_eq!(labels.len(), item.locants.len(), "{}", item.name);
        for (id, label) in &item.locants {
            assert!(item.document.atom(*id).is_some(), "{}: {label}", item.name);
            assert!(!label.is_empty(), "{}", item.name);
        }
        // An edge belongs to a cycle exactly when its endpoints remain
        // connected after removing that edge. This checks the stored graph,
        // independently of the catalog's SMILES atom order or its locants.
        let mut ring_atoms = HashSet::new();
        for (edge_index, edge) in item.document.bonds.iter().enumerate() {
            let mut visited = HashSet::from([edge.a]);
            let mut pending = vec![edge.a];
            while let Some(id) = pending.pop() {
                for other in item
                    .document
                    .bonds
                    .iter()
                    .enumerate()
                    .filter_map(|(index, other)| {
                        if index == edge_index {
                            return None;
                        }
                        if other.a == id {
                            Some(other.b)
                        } else if other.b == id {
                            Some(other.a)
                        } else {
                            None
                        }
                    })
                {
                    if visited.insert(other) {
                        pending.push(other);
                    }
                }
            }
            if visited.contains(&edge.b) {
                ring_atoms.extend([edge.a, edge.b]);
            }
        }
        assert_eq!(
            item.locants.keys().copied().collect::<HashSet<_>>(),
            ring_atoms,
            "{}: every ring atom, and no exocyclic atom, must have a locant",
            item.name
        );
    }
}

#[test]
fn retained_and_hantzsch_widman_heteroatom_locants_are_chemical_positions() {
    for (name, locants, element) in [
        ("Pyridine", "1", "N"),
        ("Pyrazole", "1 2", "N"),
        ("Imidazole", "1 3", "N"),
        ("Oxazole", "1", "O"),
        ("Oxazole", "3", "N"),
        ("Isoxazole", "2", "N"),
        ("Thiazole", "1", "S"),
        ("Morpholine", "1", "O"),
        ("Morpholine", "4", "N"),
        ("Quinoline", "1", "N"),
        ("Isoquinoline", "2", "N"),
        ("Indole", "1", "N"),
        ("9H-Purine", "1 3 7 9", "N"),
        ("7H-Purine", "1 3 7 9", "N"),
        ("Adenine (9H)", "1 3 7 9", "N"),
        ("Guanine (9H)", "1 3 7 9", "N"),
        ("Cytosine", "1 3", "N"),
        ("Thymine", "1 3", "N"),
        ("Uracil", "1 3", "N"),
        ("Carbazole (9H)", "9", "N"),
        // Acridine retains its special N-10 numbering in the 2013 Blue Book,
        // P-25.2.1 Table 2.8; it is not the systematic fusion position 5.
        ("Acridine", "10", "N"),
        ("Quinazoline", "1 3", "N"),
        ("Quinoxaline", "1 4", "N"),
        ("Benzimidazole (1H)", "1 3", "N"),
        ("Indazole (1H)", "1 2", "N"),
        ("Chromene (2H)", "1", "O"),
        ("Dibenzofuran", "5", "O"),
        ("Dibenzothiophene", "5", "S"),
        // FR-2.2.4 numbers phenanthroline systematically, unlike phenanthrene.
        ("1,10-Phenanthroline", "1 10", "N"),
        ("12-Crown-4", "1 4 7 10", "O"),
        ("18-Crown-6", "1 4 7 10 13 16", "O"),
        ("Cyclen", "1 4 7 10", "N"),
        ("Cyclam", "1 4 8 11", "N"),
    ] {
        let item = numbered_template(name);
        for locant in locants.split_whitespace() {
            assert_eq!(
                locant_atom(item, locant).element,
                element,
                "{name}: {locant}"
            );
        }
    }
}

#[test]
fn ring_perimeters_follow_retained_numbering_and_fusion_locants() {
    for (names, perimeter, fusion_edges) in [
        ("Cyclopropane", "1 2 3", ""),
        ("Cyclobutane", "1 2 3 4", ""),
        (
            "Cyclopentane|Cyclopentadiene|Pyrrole|Furan|Thiophene|Pyrazole|Imidazole|Oxazole|Isoxazole|Thiazole|Isothiazole|Pyrrolidine|Tetrahydrofuran",
            "1 2 3 4 5",
            "",
        ),
        (
            "Benzene|Cyclohexane|Cyclohexene|1,3-Cyclohexadiene|Pyridine|Pyridazine|Pyrimidine|Pyrazine|Piperidine|Piperazine|Morpholine|Tetrahydropyran|1,4-Dioxane|Cytosine|Thymine|Uracil",
            "1 2 3 4 5 6",
            "",
        ),
        ("Cycloheptane", "1 2 3 4 5 6 7", ""),
        ("Cyclooctane", "1 2 3 4 5 6 7 8", ""),
        ("12-Crown-4|Cyclen", "1 2 3 4 5 6 7 8 9 10 11 12", ""),
        ("Cyclam", "1 2 3 4 5 6 7 8 9 10 11 12 13 14", ""),
        ("Norbornane", "1 2 3 4 5 6", "1 7 7 4"),
        ("Bicyclo[1.1.1]pentane", "1 2 3 4", "1 5 5 3"),
        ("Adamantane", "1 2 3 4 5 6 7 8", "1 9 9 5 3 10 10 7"),
        ("Cubane", "1 2 3 4 5 6 7 8", "1 6 2 5 3 8 4 7"),
        (
            "Naphthalene|Decalin|Quinoline|Isoquinoline|Quinazoline|Quinoxaline|Chromene (2H)",
            "1 2 3 4 4a 5 6 7 8 8a",
            "4a 8a",
        ),
        (
            "Indole|Benzofuran|Benzothiophene|Indene (1H)|Benzimidazole (1H)|Indazole (1H)",
            "1 2 3 3a 4 5 6 7 7a",
            "3a 7a",
        ),
        ("Azulene", "1 2 3 3a 4 5 6 7 8 8a", "3a 8a"),
        (
            "Anthracene|Acridine",
            "1 2 3 4 4a 10 10a 5 6 7 8 8a 9 9a",
            "4a 9a 8a 10a",
        ),
        (
            "Phenanthrene",
            "1 2 3 4 4a 4b 5 6 7 8 8a 9 10 10a",
            "4a 10a 4b 8a",
        ),
        (
            "Fluorene (9H)|Carbazole (9H)",
            "1 2 3 4 4a 4b 5 6 7 8 8a 9 9a",
            "4a 9a 4b 8a",
        ),
        (
            "Dibenzofuran|Dibenzothiophene",
            "1 2 3 4 4a 5 5a 6 7 8 9 9a 9b",
            "4a 9b 5a 9a",
        ),
        (
            "1,10-Phenanthroline",
            "1 2 3 4 4a 5 6 6a 7 8 9 10 10a 10b",
            "4a 10b 6a 10a",
        ),
        (
            "Pyrene",
            "1 2 3 3a 4 5 5a 6 7 8 8a 9 10 10a",
            "3a 3a¹ 10a 3a¹ 3a¹ 5a¹ 5a 5a¹ 8a 5a¹",
        ),
        (
            "9H-Purine|7H-Purine|Adenine (9H)|Guanine (9H)",
            "1 2 3 4 9 8 7 5 6",
            "4 5",
        ),
    ] {
        for name in names.split('|') {
            let item = numbered_template(name);
            let perimeter: Vec<_> = perimeter.split_whitespace().collect();
            for (&a, &b) in perimeter
                .iter()
                .zip(perimeter.iter().cycle().skip(1))
                .take(perimeter.len())
            {
                assert!(locants_are_bonded(item, a, b), "{name}: {a}–{b}");
            }
            let fusion_edges: Vec<_> = fusion_edges.split_whitespace().collect();
            for edge in fusion_edges.as_chunks::<2>().0 {
                assert!(
                    locants_are_bonded(item, edge[0], edge[1]),
                    "{name}: fusion {}–{}",
                    edge[0],
                    edge[1]
                );
            }
        }
    }
}

fn nitrogen_hydrogens(item: &reshiki::templates::Template, locant: &str) -> u32 {
    let atom = locant_atom(item, locant);
    assert_eq!(atom.element, "N");
    assert_eq!(atom.charge, 0);
    let valence: u32 = item
        .document
        .bonds
        .iter()
        .filter(|b| b.a == atom.id || b.b == atom.id)
        .map(|b| {
            assert!((1..=3).contains(&b.order));
            b.order as u32
        })
        .sum();
    atom.explicit_h
        + if atom.no_implicit {
            0
        } else {
            3 - valence - atom.explicit_h
        }
}

#[test]
fn purine_defaults_are_9h_and_the_named_7h_form_stays_7h() {
    for name in ["9H-Purine", "Adenine (9H)", "Guanine (9H)"] {
        let item = numbered_template(name);
        assert_eq!(nitrogen_hydrogens(item, "9"), 1, "{name}: N9");
        assert_eq!(nitrogen_hydrogens(item, "7"), 0, "{name}: N7");
    }
    let seven_h = numbered_template("7H-Purine");
    assert_eq!(nitrogen_hydrogens(seven_h, "7"), 1);
    assert_eq!(nitrogen_hydrogens(seven_h, "9"), 0);
}

#[tokio::test]
async fn adenine_can_connect_at_n9_without_changing_its_ring_tautomer() {
    use reshiki::templates::{Anchor, Connection, place_with_mode};

    let item = numbered_template("Adenine (9H)");
    let n9 = locant_atom(item, "9");
    let mut methyl = Document::default();
    let carbon = methyl.add_atom("C", Point::default());
    let (joined, ids) = place_with_mode(
        &methyl,
        &item.document,
        Point::default(),
        None,
        5.,
        Anchor::Atom(n9.id),
        Connection::Connect,
    )
    .expect("N9 of the 9H tautomer must have available valence for nucleoside-style attachment");
    let mapping: std::collections::HashMap<_, _> =
        item.document.all_ids().into_iter().zip(ids).collect();
    assert!(joined.bonds.iter().any(|b| b.order == 1
        && ((b.a == carbon && b.b == mapping[&n9.id])
            || (b.b == carbon && b.a == mapping[&n9.id]))));
    let engine = LocalEngine::default();
    let actual = engine
        .execute(Request::molecule("analyze", joined))
        .await
        .unwrap()
        .analysis
        .unwrap();
    let expected = engine
        .execute(Request::import_smiles("Cn1cnc2c(N)ncnc21"))
        .await
        .unwrap()
        .analysis
        .unwrap();
    assert_eq!(actual.formula, "C6H7N5");
    assert_eq!(actual.smiles, expected.smiles);
    assert_eq!(actual.inchikey, expected.inchikey);
}

fn locant_centroid(item: &reshiki::templates::Template, locants: &str) -> Point {
    let points: Vec<_> = locants
        .split_whitespace()
        .map(|locant| locant_atom(item, locant).position)
        .collect();
    Point::new(
        points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32,
        points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32,
    )
}

#[test]
fn new_fused_templates_store_the_requested_orientation() {
    let phenanthrene = numbered_template("Phenanthrene");
    let mut rings = [
        locant_centroid(phenanthrene, "4b 5 6 7 8 8a"),
        locant_centroid(phenanthrene, "4b 8a 9 10 10a 4a"),
        locant_centroid(phenanthrene, "10a 1 2 3 4 4a"),
    ];
    rings.sort_by(|a, b| a.x.total_cmp(&b.x));
    let [left, center, right] = rings;
    assert!((left.y - center.y).abs() < 0.01);
    assert!(left.x < center.x && center.x < right.x);
    assert!(right.y < center.y - 1.);

    for (name, lower) in [
        ("Benzimidazole (1H)", "3"),
        ("Indazole (1H)", "3"),
        ("Chromene (2H)", "4"),
    ] {
        let item = numbered_template(name);
        assert!(
            locant_atom(item, "1").position.y < locant_atom(item, lower).position.y,
            "{name}: heteroatom 1 must be above position {lower}"
        );
    }

    for (name, locant, direction) in [
        ("Fluorene (9H)", "9", -1.),
        ("Carbazole (9H)", "9", -1.),
        ("Dibenzofuran", "5", 1.),
        ("Dibenzothiophene", "5", 1.),
    ] {
        let item = numbered_template(name);
        let atom = locant_atom(item, locant);
        assert!(
            item.document
                .atoms
                .iter()
                .filter(|a| a.id != atom.id)
                .all(|a| (atom.position.y - a.position.y) * direction > 0.01),
            "{name}: locant {locant} must have the requested vertical orientation"
        );
    }
}
