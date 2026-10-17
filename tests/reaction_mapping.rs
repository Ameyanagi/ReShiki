use reshiki::{
    atom_labels::{self, Number, Owner},
    document::{Document, History, Point},
    editing,
    engine::{ChemistryEngine, LocalEngine, Request},
    reaction_mapping::{self as mapping, Budget},
    reactions::Role,
};
async fn import(smiles: &str) -> Document {
    LocalEngine::default()
        .execute(Request::import("rsmi", smiles))
        .await
        .unwrap()
        .document
        .unwrap()
}
async fn output(doc: Document, format: &str) -> String {
    let mut request = Request::molecule("export", doc);
    request.format = Some(format.into());
    LocalEngine::default()
        .execute(request)
        .await
        .unwrap()
        .output
        .unwrap()
}
fn side(doc: &Document, role: Role) -> Vec<u64> {
    doc.reactions[0]
        .participants(role)
        .iter()
        .flat_map(|p| p.atoms.iter().copied())
        .collect()
}
#[tokio::test]
async fn oxidation_maps_changes_in_hydrogens_and_bond_order_without_changing_chemistry() {
    let mut doc = import("[CH3][CH2][OH]>>[CH3][CH]=O").await;
    let id = side(&doc, Role::Reactant)[0];
    doc.atom_mut(id).unwrap().display.number = Some(Number {
        text: "Cα".into(),
        offset: Some(Point::new(0., -35.)),
        style: atom_labels::number_style(),
    });
    let original = doc.clone();
    let expected = output(doc.clone(), "rsmi").await;
    let proposal = mapping::propose(&doc, doc.reactions[0].arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(!proposal.multiple_best);
    assert_eq!(proposal.pairs.len(), 3);
    let mapped = proposal.apply(&doc, false).unwrap();
    assert_eq!(doc, original);
    assert_eq!(
        mapped.atom(id).unwrap().display.number,
        original.atom(id).unwrap().display.number
    );
    assert!(
        atom_labels::indicators(&mapped)
            .iter()
            .any(|i| i.owner == Owner::Mapping(id))
    );
    assert!(
        atom_labels::indicators(&mapped)
            .iter()
            .any(|i| i.owner == Owner::Number(id) && i.text == "Cα")
    );
    let mut without_maps = mapped.clone();
    for a in &mut without_maps.atoms {
        a.map_num = 0;
    }
    assert_eq!(output(without_maps, "rsmi").await, expected);
    let mut history = History::default();
    let mut current = mapped.clone();
    history.commit(original.clone(), &current);
    history.undo(&mut current);
    assert_eq!(current, original);
    history.redo(&mut current);
    assert_eq!(current, mapped);
    let reopened: Document = serde_json::from_slice(&serde_json::to_vec(&mapped).unwrap()).unwrap();
    assert_eq!(reopened, mapped);
    let mut copied = Document::default();
    copied.atom_labels.maps = false;
    let copied_ids = editing::append(&mut copied, &mapped, Point::new(0., 180.));
    assert!(!copied_ids.is_empty());
    assert_eq!(
        atom_labels::indicators(&copied)
            .iter()
            .filter(|i| matches!(i.owner, Owner::Mapping(_)))
            .count(),
        6
    );
    for format in ["rxn", "rsmi"] {
        let encoded = output(mapped.clone(), format).await;
        let decoded = LocalEngine::default()
            .execute(Request::import(format, &encoded))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(
            output(decoded, "rsmi").await,
            output(mapped.clone(), "rsmi").await
        );
    }
}
#[tokio::test]
async fn substitution_minimizes_changed_atoms_and_missing_coproducts_remain_unmatched() {
    let doc = import("CCBr.[OH-]>>CCO.[Br-]").await;
    let proposal = mapping::propose(&doc, doc.reactions[0].arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(!proposal.multiple_best);
    assert_eq!(proposal.pairs.len(), 4);
    let reactants = side(&doc, Role::Reactant);
    let products = side(&doc, Role::Product);
    assert!(proposal.pairs.contains(&(reactants[0], products[0])));
    assert!(proposal.pairs.contains(&(reactants[1], products[1])));
    let missing = import("CCBr>>CCO").await;
    let proposal =
        mapping::propose(&missing, missing.reactions[0].arrow, Budget::default()).unwrap();
    assert_eq!(proposal.pairs.len(), 2);
    assert_eq!(proposal.unmatched_reactants, 1);
    assert_eq!(proposal.unmatched_products, 1);
    let mapped = proposal.apply(&missing, true).unwrap();
    assert_eq!(mapped.atoms.iter().filter(|a| a.map_num == 0).count(), 2);
}
#[tokio::test]
async fn partial_anchors_isotopes_explicit_hydrogen_and_agents_are_respected() {
    let doc = import("[13CH3:17][CH2][OH].[H:29][Cl]>[Na+:91]>[13CH3][CH]=O.[H][Cl:30]").await;
    let proposal = mapping::propose(&doc, doc.reactions[0].arrow, Budget::default()).unwrap();
    let mapped = proposal.apply(&doc, true).unwrap();
    for (before, after) in doc.atoms.iter().zip(&mapped.atoms) {
        if before.map_num != 0 {
            assert_eq!(before.map_num, after.map_num);
        }
    }
    let agent = side(&doc, Role::Agent)[0];
    assert_eq!(mapped.atom(agent).unwrap().map_num, 91);
    assert!(
        !proposal
            .pairs
            .iter()
            .any(|(a, b)| *a == agent || *b == agent)
    );
    for (a, b) in &proposal.pairs {
        assert_eq!(doc.atom(*a).unwrap().element, doc.atom(*b).unwrap().element);
        assert_eq!(doc.atom(*a).unwrap().isotope, doc.atom(*b).unwrap().isotope);
    }
    assert!(
        proposal
            .pairs
            .iter()
            .any(|(a, _)| doc.atom(*a).unwrap().element == "H")
    );
    assert!(
        proposal
            .pairs
            .iter()
            .any(|(a, _)| doc.atom(*a).unwrap().isotope == 13)
    );
}
#[tokio::test]
async fn symmetry_budget_and_stale_proposals_need_review_and_never_mutate_the_source() {
    let doc = import("CC(O)C>>CC(=O)C").await;
    let before = doc.clone();
    let proposal = mapping::propose(&doc, doc.reactions[0].arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(proposal.multiple_best);
    assert!(proposal.apply(&doc, false).is_err());
    assert!(proposal.apply(&doc, true).is_ok());
    assert_eq!(doc, before);
    assert!(mapping::propose(&doc, doc.reactions[0].arrow, Budget { states: 0, work: 0 }).is_err());
    let limited = mapping::propose(
        &doc,
        doc.reactions[0].arrow,
        Budget {
            states: 0,
            work: 20_000_000,
        },
    )
    .unwrap();
    assert!(!limited.complete);
    assert_eq!(limited.pairs.len(), 4);
    assert!(limited.apply(&doc, false).is_err());
    assert_eq!(doc, before);
    let mut moved = doc.clone();
    moved.atoms[0].position.x += 1.;
    assert!(proposal.apply(&moved, true).is_err());
    let mut refreshed = doc.clone();
    refreshed.atoms[0].label_h = 99;
    assert!(proposal.current(&refreshed));
}
#[tokio::test]
async fn duplicate_map_classes_are_preserved_but_not_silently_remapped() {
    let mut doc = import("[CH3:1][CH2:2][OH:3]>>[CH3:1][CH:2]=[O:3]").await;
    let reactants = side(&doc, Role::Reactant);
    let before = doc.clone();
    assert!(mapping::set_map(&doc, reactants[1], 1).is_err());
    assert_eq!(doc, before);
    assert!(mapping::set_map(&doc, reactants[0], mapping::MAX_MAP + 1).is_err());
    doc.atom_mut(reactants[1]).unwrap().map_num = 1;
    doc.validate().unwrap();
    assert!(
        mapping::propose(&doc, doc.reactions[0].arrow, Budget::default())
            .unwrap_err()
            .contains("twice")
    );
    let json = serde_json::to_vec(&doc).unwrap();
    let restored: Document = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored, doc);
    assert_eq!(
        atom_labels::indicators(&doc)
            .iter()
            .filter(|i| matches!(i.owner, Owner::Mapping(_)) && i.text == "1")
            .count(),
        3
    );
}
#[tokio::test]
async fn proper_rigid_alignment_preserves_all_bonds_and_stereochemical_identity() {
    let mut doc = import("[CH3:1][C@H:2]([OH:3])[Cl:4]>>[CH3:1][C@@H:2]([OH:3])[Cl:4]").await;
    let products = side(&doc, Role::Product);
    editing::transform(&mut doc, &products, editing::Transform::Rotate(77.));
    let original = doc.clone();
    let expected = output(doc.clone(), "rsmi").await;
    let aligned = mapping::align(&doc, doc.reactions[0].arrow).unwrap();
    assert_eq!(aligned.bonds, doc.bonds);
    for (a, b) in doc.atoms.iter().zip(&aligned.atoms) {
        assert_eq!(a.stereo, b.stereo);
        assert_eq!(a.depth, b.depth);
    }
    for a in &products {
        for b in &products {
            let before = doc
                .atom(*a)
                .unwrap()
                .position
                .distance(doc.atom(*b).unwrap().position);
            let after = aligned
                .atom(*a)
                .unwrap()
                .position
                .distance(aligned.atom(*b).unwrap().position);
            assert!((before - after).abs() < 0.001);
        }
    }
    assert_eq!(output(aligned.clone(), "rsmi").await, expected);
    assert_eq!(doc, original);
    let deltas: Vec<_> = products
        .iter()
        .map(|id| {
            let p = aligned.atom(*id).unwrap();
            let r = aligned
                .atoms
                .iter()
                .find(|a| side(&aligned, Role::Reactant).contains(&a.id) && a.map_num == p.map_num)
                .unwrap();
            Point::new(p.position.x - r.position.x, p.position.y - r.position.y)
        })
        .collect();
    for delta in deltas.windows(2) {
        assert!(delta[0].distance(delta[1]) < 0.001);
    }
}
/// Align, then require the last `+` to sit clear of both products, with one
/// product wholly before it and the other wholly after it along the arrow.
fn assert_product_plus_separates(doc: &Document) {
    let plus = doc
        .annotations
        .iter()
        .filter(|a| a.text == "+")
        .map(|a| a.id)
        .max_by(|a, b| {
            let x = |id| {
                let a = doc.annotations.iter().find(|a| a.id == id).unwrap();
                let arrow = &doc.arrows[0];
                let (dx, dy) = (arrow.end.x - arrow.start.x, arrow.end.y - arrow.start.y);
                a.position.x * dx + a.position.y * dy
            };
            x(*a).total_cmp(&x(*b))
        })
        .unwrap();
    let aligned = mapping::align(doc, doc.reactions[0].arrow).unwrap();
    let arrow = &aligned.arrows[0];
    let length = arrow.start.distance(arrow.end);
    let axis = Point::new(
        (arrow.end.x - arrow.start.x) / length,
        (arrow.end.y - arrow.start.y) / length,
    );
    let along = |p: Point| p.x * axis.x + p.y * axis.y;
    let plus = aligned.annotations.iter().find(|a| a.id == plus).unwrap();
    let (w, h) = plus.size();
    let centre = plus.position.offset(w / 2., h / 2.);
    let sides: Vec<_> = aligned.reactions[0]
        .products
        .iter()
        .map(|part| {
            let points: Vec<_> = part
                .atoms
                .iter()
                .map(|id| aligned.atom(*id).unwrap().position)
                .collect();
            for p in &points {
                assert!(p.distance(centre) > 20., "+ at {centre:?} crowds {p:?}");
            }
            let before = points.iter().all(|p| along(*p) < along(centre));
            let after = points.iter().all(|p| along(*p) > along(centre));
            assert!(before || after, "+ at {centre:?} is inside a product");
            before
        })
        .collect();
    assert_ne!(
        sides[0], sides[1],
        "+ at {centre:?} is not between the products"
    );
    // Aligning an aligned scheme changes nothing.
    let again = mapping::align(&aligned, aligned.reactions[0].arrow).unwrap();
    for (a, b) in aligned.atoms.iter().zip(&again.atoms) {
        assert!(
            a.position.distance(b.position) < 0.01,
            "atom {} drifted",
            a.id
        );
    }
    for (a, b) in aligned.annotations.iter().zip(&again.annotations) {
        assert!(
            a.position.distance(b.position) < 0.01,
            "text {} drifted",
            a.id
        );
    }
}
#[tokio::test]
async fn alignment_keeps_the_product_plus_between_its_participants() {
    // Issue #335: HCl moved left of the amide but the `+` stayed on the right.
    let mut doc = import(
        "[CH3:1][C:2](=[O:3])[Cl:4].[CH3:6][C@H:7]([NH2:5])[C:8](=[O:9])[O:10][CH3:11]>>[CH3:1][C:2](=[O:3])[NH:5][C@@H:7]([CH3:6])[C:8](=[O:9])[O:10][CH3:11].[ClH:4]",
    )
    .await;
    let amide = doc.reactions[0].products[0].atoms.clone();
    editing::transform(&mut doc, &amide, editing::Transform::Rotate(60.));
    assert_product_plus_separates(&doc);
    // A vertical scheme reads along its arrow, not along x.
    let all = doc.reactions[0].ids();
    editing::transform(&mut doc, &all, editing::Transform::Rotate(90.));
    assert_product_plus_separates(&doc);
}
#[tokio::test]
#[ignore = "Writes disposable before/after drawings for native GUI verification"]
async fn write_reaction_mapping_gui_fixtures() {
    let directory = std::path::Path::new("tests/fixtures/reaction-mapping");
    std::fs::create_dir_all(directory).unwrap();
    for (name, smiles) in [
        ("mapped-import", "[CH3:1][OH:2]>O>[CH2:1]=[O:2]"),
        ("oxidation", "[CH3][CH2][OH]>>[CH3][CH]=O"),
        ("substitution", "CCBr.[OH-]>>CCO.[Br-]"),
        ("symmetry", "CC(O)C>>CC(=O)C"),
    ] {
        let mut doc = import(smiles).await;
        let id = side(&doc, Role::Reactant)[0];
        doc.atom_mut(id).unwrap().display.number = Some(Number {
            text: "Cα".into(),
            offset: None,
            style: atom_labels::number_style(),
        });
        std::fs::write(
            directory.join(format!("{name}-before.rsk")),
            serde_json::to_vec_pretty(&doc).unwrap(),
        )
        .unwrap();
        let proposal = mapping::propose(&doc, doc.reactions[0].arrow, Budget::default()).unwrap();
        let mapped = proposal.apply(&doc, true).unwrap();
        let aligned = mapping::align(&mapped, mapped.reactions[0].arrow).unwrap();
        std::fs::write(
            directory.join(format!("{name}-after.rsk")),
            serde_json::to_vec_pretty(&aligned).unwrap(),
        )
        .unwrap();
    }
}

fn multistep_shared_intermediate() -> (Document, u64, u64, u64) {
    use reshiki::{
        document::Arrow,
        reactions::{Participant, Reaction},
    };
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let x = doc.add_atom("C", Point::new(200., 0.));
    let y = doc.add_atom("C", Point::new(200., 90.));
    let z = doc.add_atom("C", Point::new(400., 0.));
    doc.atom_mut(y).unwrap().map_num = 1;
    doc.atom_mut(z).unwrap().map_num = 1;
    let first = doc.next_id();
    doc.arrows.push(Arrow::new(
        first,
        Point::new(60., 0.),
        Point::new(130., 0.),
        Default::default(),
        Default::default(),
    ));
    let second = doc.next_id();
    doc.arrows.push(Arrow::new(
        second,
        Point::new(260., 0.),
        Point::new(330., 0.),
        Default::default(),
        Default::default(),
    ));
    let participant = |id| Participant {
        atoms: vec![id],
        coefficient: 1,
    };
    let mut r1 = Reaction::new(first);
    r1.reactants.push(participant(a));
    r1.products.push(participant(x));
    let mut r2 = Reaction::new(second);
    r2.reactants.extend([participant(x), participant(y)]);
    r2.products.push(participant(z));
    doc.reactions = vec![r1, r2];
    doc.validate().unwrap();
    (doc, first, a, x)
}
#[test]
fn automatic_and_manual_pairing_reserve_fresh_maps_across_shared_reaction_steps() {
    let (doc, arrow, a, x) = multistep_shared_intermediate();
    let before = doc.clone();
    let proposal = mapping::propose(&doc, arrow, Budget::default()).unwrap();
    let mapped = proposal.apply(&doc, false).unwrap();
    assert_eq!(mapped.atom(a).unwrap().map_num, 2);
    assert_eq!(mapped.atom(x).unwrap().map_num, 2);
    assert_eq!(mapped.atom(3).unwrap().map_num, 1);
    assert_eq!(mapped.atom(4).unwrap().map_num, 1);
    let paired = mapping::pair(&doc, arrow, &[a, x]).unwrap();
    assert_eq!(paired.atom(x).unwrap().map_num, 2);
    assert_eq!(doc, before);
}
#[test]
fn inherited_maps_that_conflict_in_another_reaction_are_rejected_without_mutation() {
    let (mut doc, arrow, a, x) = multistep_shared_intermediate();
    doc.atom_mut(a).unwrap().map_num = 1;
    let before = doc.clone();
    let history = History::default();
    let error = mapping::propose(&doc, arrow, Budget::default()).unwrap_err();
    assert!(error.contains("twice"));
    assert!(mapping::pair(&doc, arrow, &[a, x]).is_err());
    assert_eq!(doc, before);
    assert!(!history.can_undo());
}
#[test]
fn old_documents_show_derived_maps_and_manual_numbers_remain_independent() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(10., 20.));
    doc.atom_mut(a).unwrap().map_num = 17;
    doc.atom_mut(a).unwrap().display.number = Some(Number {
        text: "Cα".into(),
        offset: Some(Point::new(0., -40.)),
        style: atom_labels::number_style(),
    });
    let mut legacy = serde_json::to_value(&doc).unwrap();
    legacy["atom_labels"]
        .as_object_mut()
        .unwrap()
        .remove("maps");
    let mut reopened: Document = serde_json::from_value(legacy).unwrap();
    assert!(reopened.atom_labels.maps);
    let indicators = atom_labels::indicators(&reopened);
    let custom = indicators
        .iter()
        .find(|i| i.owner == Owner::Number(a))
        .unwrap();
    let map = indicators
        .iter()
        .find(|i| i.owner == Owner::Mapping(a))
        .unwrap();
    assert_eq!(custom.text, "Cα");
    assert_eq!(map.text, "17");
    assert!(
        (custom.center.x - map.center.x).abs() > (custom.width + map.width) / 2.
            || (custom.center.y - map.center.y).abs() > (custom.height + map.height) / 2.
    );
    Owner::Mapping(a).set_offset(&mut reopened, Some(Point::new(30., 0.)));
    editing::transform_about(&mut reopened, &[a], Point::new(10., 20.), 1., 90.);
    let center = atom_labels::indicators(&reopened)
        .into_iter()
        .find(|i| i.owner == Owner::Mapping(a))
        .unwrap()
        .center;
    assert!(center.distance(Point::new(10., 50.)) < 0.001);
    reopened.atom_mut(a).unwrap().map_num = 19;
    assert!(
        atom_labels::indicators(&reopened)
            .iter()
            .any(|i| i.owner == Owner::Mapping(a) && i.text == "19")
    );
    reopened.atom_labels.maps = false;
    assert!(
        atom_labels::indicators(&reopened)
            .iter()
            .all(|i| i.owner != Owner::Mapping(a))
    );
    assert_eq!(reopened.atom(a).unwrap().map_num, 19);
}
#[tokio::test]
async fn symmetric_ring_anchors_resolve_only_the_constraints_they_supply() {
    let mut doc = import("C1CCCCC1>>C1=CCCCC1").await;
    let arrow = doc.reactions[0].arrow;
    let reactants = side(&doc, Role::Reactant);
    let products = side(&doc, Role::Product);
    let proposal = mapping::propose(&doc, arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(proposal.multiple_best);
    doc = mapping::pair(&doc, arrow, &[reactants[0], products[0]]).unwrap();
    let proposal = mapping::propose(&doc, arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(proposal.multiple_best);
    doc = mapping::pair(&doc, arrow, &[reactants[1], products[1]]).unwrap();
    let proposal = mapping::propose(&doc, arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(!proposal.multiple_best);
}

#[test]
fn newly_mapped_shared_participants_with_nonunit_coefficients_are_rejected_atomically() {
    let (mut doc, arrow, a, x) = multistep_shared_intermediate();
    for atom in &mut doc.atoms {
        atom.map_num = 0;
    }
    doc.reactions[1].reactants[0].coefficient = 2;
    doc.validate().unwrap();
    let before = doc.clone();
    assert!(
        mapping::propose(&doc, arrow, Budget::default())
            .unwrap_err()
            .contains("coefficients")
    );
    assert!(
        mapping::pair(&doc, arrow, &[a, x])
            .unwrap_err()
            .contains("coefficients")
    );
    assert!(
        mapping::set_map(&doc, x, 7)
            .unwrap_err()
            .contains("coefficients")
    );
    assert_eq!(doc, before);
}

#[tokio::test]
async fn aromatic_kekule_mapping_uses_adjacent_anchors_without_repainting_the_graph() {
    use std::collections::BTreeSet;

    let mut doc = import("[cH:41]1[cH:42]cccc1>>[CH:41]1=[CH:42]C=CC=C1").await;
    let reactants = side(&doc, Role::Reactant);
    let products = side(&doc, Role::Product);
    assert_eq!(doc.atoms.len(), 12);
    assert_eq!(doc.bonds.len(), 12);
    assert_eq!(reactants.len(), 6);
    assert_eq!(products.len(), 6);
    for ids in [&reactants, &products] {
        // Independently pin each input's six-cycle in SMILES traversal order.
        let expected_edges: BTreeSet<_> = ids
            .iter()
            .zip(ids.iter().cycle().skip(1))
            .map(|(&a, &b)| (a.min(b), a.max(b)))
            .collect();
        let bonds: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
            .collect();
        assert_eq!(bonds.len(), 6);
        assert_eq!(
            bonds
                .iter()
                .map(|b| (b.a.min(b.b), b.a.max(b.b)))
                .collect::<BTreeSet<_>>(),
            expected_edges
        );
        assert_eq!(bonds.iter().filter(|b| b.order == 1).count(), 3);
        assert_eq!(bonds.iter().filter(|b| b.order == 2).count(), 3);
        for (index, &id) in ids.iter().enumerate() {
            let atom = doc.atom(id).unwrap();
            assert_eq!(atom.element, "C");
            assert_eq!(atom.charge, 0);
            assert_eq!(atom.isotope, 0);
            assert_eq!(atom.map_num, [41, 42, 0, 0, 0, 0][index]);
        }
    }

    // Import depicts both rings as Kekulé bonds. Use the public identity-checked
    // display edit so this fixture really contains different stored bond orders.
    doc = reshiki::chemistry::document::aromatic_display(&doc, &reactants)
        .unwrap()
        .finish()
        .unwrap()
        .document;
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|b| reactants.contains(&b.a) && b.order == 4)
            .count(),
        6
    );
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|b| products.contains(&b.a) && matches!(b.order, 1 | 2))
            .count(),
        6
    );
    doc.atom_labels.maps = true;
    let visible = reactants[0];
    let hidden = reactants[1];
    let display = &mut doc.atom_mut(visible).unwrap().display.mapping;
    display.show = Some(true);
    display.offset = Some(Point::new(-11., 23.));
    display.style.bold = true;
    display.style.size_pt = 11.;
    display.style.color = reshiki::palette::Color::Custom([31, 78, 121]);
    doc.atom_mut(hidden).unwrap().display.mapping.show = Some(false);
    let original = doc.clone();

    // Ordered adjacent anchors fix a six-cycle uniquely: a graph automorphism
    // fixing both vertices individually cannot reflect or rotate the ring.
    let mut expected_pairs: Vec<_> = reactants.iter().copied().zip(products).collect();
    expected_pairs.sort_unstable();
    let proposal = mapping::propose(&doc, doc.reactions[0].arrow, Budget::default()).unwrap();
    assert!(proposal.complete);
    assert!(!proposal.multiple_best);
    assert_eq!(proposal.unmatched_reactants, 0);
    assert_eq!(proposal.unmatched_products, 0);
    assert_eq!(proposal.pairs, expected_pairs);
    // Correct pairing alone is insufficient: aromatic perception must make all
    // six corresponding chemical bonds unchanged despite the stored 4 vs 1/2.
    assert_eq!(proposal.score.heavy_adjacency, 6);
    assert_eq!(proposal.score.adjacency, 6);
    assert_eq!(proposal.score.unchanged_bonds, 6);
    assert_eq!(proposal.score.changed_atoms, 0);
    assert_eq!(proposal.score.property_delta, 0);
    let mapped = proposal.apply(&doc, false).unwrap();
    assert_eq!(doc, original);
    for &(a, b) in &expected_pairs {
        let left = mapped.atom(a).unwrap().map_num;
        assert_ne!(left, 0);
        assert_eq!(left, mapped.atom(b).unwrap().map_num);
    }
    assert_eq!(
        mapped
            .atoms
            .iter()
            .map(|a| a.map_num)
            .collect::<BTreeSet<_>>()
            .len(),
        6
    );
    // Only fresh map assignments may change; original anchors, IDs, all atom
    // chemistry/coordinates, bond orders/styles and other document data survive.
    let mut restored_maps = mapped.clone();
    for (after, before) in restored_maps.atoms.iter_mut().zip(&original.atoms) {
        if before.map_num != 0 {
            assert_eq!(after.map_num, before.map_num);
        }
        after.map_num = before.map_num;
    }
    assert_eq!(mapped.bonds, original.bonds);
    assert_eq!(restored_maps, original);
    let indicators = atom_labels::indicators(&mapped);
    let label = indicators
        .iter()
        .find(|i| i.owner == Owner::Mapping(visible))
        .unwrap();
    assert_eq!(label.text, "41");
    assert_eq!(
        label.center,
        original.atom(visible).unwrap().position.offset(-11., 23.)
    );
    assert_eq!(
        label.style,
        original.atom(visible).unwrap().display.mapping.style
    );
    assert!(!indicators.iter().any(|i| i.owner == Owner::Mapping(hidden)));

    // Use the edit-finalization API used by the app, not a raw history-only copy.
    let mut history = History::default();
    let mut current = mapped.clone();
    assert!(
        reshiki::transaction::apply(&mut current, &mut history, original.clone(), false)
            .unwrap()
            .recorded
    );
    assert_eq!(current, mapped);
    assert!(history.undo(&mut current));
    assert_eq!(current, original);
    assert!(!history.can_undo());
    assert!(history.redo(&mut current));
    assert_eq!(current, mapped);
    assert!(!history.can_redo());
}
