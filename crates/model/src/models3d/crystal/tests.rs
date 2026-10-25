use super::*;
use crate::{
    chemistry::document as molecular,
    document::{CRYSTALS, DISTANCE_BONDS, Document, Point},
    editing,
    models3d::{ForceField, Model3d, Placed},
    transaction,
};

/// A legacy v29 cell: three explicit carbons and two inferred edges, with
/// the provenance supplied only by its live cell model.
fn legacy_cell() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let ids = [0., 42., 84.].map(|x| doc.add_atom("C", Point::new(x, 0.)));
    for atom in &mut doc.atoms {
        atom.no_implicit = true;
    }
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], 1, "plain");
    let placed = ids
        .iter()
        .enumerate()
        .map(|(i, id)| Placed {
            id: *id,
            xyz: [i as f32 * 1.5, 0., 0.],
        })
        .collect();
    let model = Model3d::new(&doc, placed, vec![], ForceField::default()).with_cell(
        &doc,
        Some(Cell {
            vectors: [[5., 0., 0.], [0., 5., 0.], [0., 0., 5.]],
            crossing: vec![],
        }),
        vec![],
    );
    doc.models.push(model);
    doc.require_version(CRYSTALS);
    doc.validate().unwrap();
    (doc, ids)
}

#[test]
fn legacy_native_cells_acquire_provenance_before_the_model_can_be_lost() {
    let (legacy, _) = legacy_cell();
    let bytes = serde_json::to_vec(&legacy).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value["bonds"][0].get("distance_derived").is_none());
    let opened = Document::from_json(&bytes).unwrap();
    assert_eq!(opened.version, DISTANCE_BONDS);
    assert!(opened.bonds.iter().all(|b| b.distance_derived));
    assert_eq!(opened.models, legacy.models);
    assert!(matches!(
        molecular::prepare(&opened),
        Err(molecular::Error::DistanceBonds)
    ));
    // Drawing interchange can still read the structurally validated edges.
    assert_eq!(molecular::drawing_graph(&opened).unwrap().bonds.len(), 2);
}

#[test]
fn topology_edit_and_native_round_trip_keep_distance_provenance() {
    let (before, _) = legacy_cell();
    let mut doc = before.clone();
    doc.bonds[0].order = 2;
    let _ = transaction::reconcile(&mut doc, before).unwrap();
    assert!(!present(&doc));
    assert!(doc.bonds.iter().all(|b| b.distance_derived));
    assert!(touches(&doc, &[]));
    assert_eq!(analysis_notice(&doc), DISTANCE_ANALYSIS_NOTICE);
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.bonds, doc.bonds);
    assert!(matches!(
        molecular::prepare(&reopened),
        Err(molecular::Error::DistanceBonds)
    ));
    let mut too_old = reopened;
    too_old.version = CRYSTALS;
    assert!(too_old.validate().unwrap_err().contains("version 30"));
}

#[test]
fn deleting_or_copying_part_of_a_legacy_cell_marks_surviving_edges() {
    let (before, [a, b, c]) = legacy_cell();
    let mut deleted = before.clone();
    deleted.delete(&[c]);
    assert!(!present(&deleted));
    assert!(deleted.bonds[0].distance_derived);
    let subset = editing::selection(&before, &[a, b]);
    assert!(!present(&subset));
    assert!(subset.bonds[0].distance_derived);
    assert!(touches(&subset, &[a]));
    let mut pasted = Document::default();
    let copied = editing::append(&mut pasted, &subset, Point::new(100., 0.));
    assert_eq!(copied.len(), 2);
    assert_eq!(pasted.version, DISTANCE_BONDS);
    assert!(pasted.bonds[0].distance_derived);
    pasted.validate().unwrap();
    assert!(matches!(
        molecular::prepare(&pasted),
        Err(molecular::Error::DistanceBonds)
    ));
}

#[test]
fn a_pruned_model_still_recovers_legacy_provenance_from_the_snapshot() {
    let (before, ids) = legacy_cell();
    let mut doc = before.clone();
    // Mimic a caller that removed the model before final reconciliation.
    doc.models.clear();
    doc.atoms.retain(|a| a.id != ids[2]);
    doc.bonds.retain(|b| b.a != ids[2] && b.b != ids[2]);
    let _ = transaction::reconcile(&mut doc, before).unwrap();
    assert!(doc.bonds[0].distance_derived);
    assert!(touches(&doc, &[ids[0]]));
}

#[test]
fn provenance_blocks_the_whole_component_but_not_an_ordinary_neighbor() {
    let (mut doc, [a, b, c]) = legacy_cell();
    mark_distance_bonds(&mut doc);
    doc.models.clear();
    let extension = doc.add_atom("C", Point::new(-42., 0.));
    doc.add_bond(extension, a, 1, "plain");
    let methane = doc.add_atom("C", Point::new(200., 0.));
    assert!(touches(&doc, &[extension]));
    assert!(!touches(&doc, &[methane]));
    assert_eq!(analysis_atoms(&doc), HashSet::from([a, b, c, extension]));
    let ordinary = editing::analysis_document(&doc, &[methane]);
    assert!(molecular::prepare(&ordinary).is_ok());
    // A source guard is required for a selection whose only distance edge is
    // cut away. It must not be turned into an inferred hydrogen cap either.
    let part = editing::analysis_document(&doc, &[a]);
    assert_eq!(part.atom(a).unwrap().explicit_h, 0);
    assert!(part.atom(a).unwrap().no_implicit);
    assert!(touches(&doc, &[a]));
}

#[test]
fn existing_edge_edits_keep_provenance_but_a_fresh_drawing_is_not_tainted() {
    let (mut doc, [a, b, _]) = legacy_cell();
    mark_distance_bonds(&mut doc);
    doc.models.clear();
    doc.add_bond(a, b, 2, "wedge");
    assert!(doc.bond_between(a, b).unwrap().1.distance_derived);
    doc.bonds.clear();
    doc.add_bond(a, b, 1, "plain");
    assert!(!doc.bonds[0].distance_derived);
    assert!(!touches(&doc, &[]));
    assert!(!matches!(
        molecular::prepare(&doc),
        Err(molecular::Error::DistanceBonds)
    ));
}

#[test]
fn coalescing_duplicate_edges_keeps_provenance_in_either_order() {
    for marked_first in [false, true] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        let b = doc.add_atom("C", Point::new(0., 42.));
        let c = doc.add_atom("C", Point::new(42., 0.));
        let d = doc.add_atom("C", Point::new(0., 84.));
        doc.add_bond(a, c, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        doc.add_bond(d, c, 1, "plain");
        doc.bonds[usize::from(!marked_first)].distance_derived = true;
        doc.require_version(DISTANCE_BONDS);
        let (merged, _) = crate::joining::merge_atoms(&doc, &[a, b, d]).unwrap();
        assert_eq!(merged.bonds.len(), 1);
        assert!(merged.bonds[0].distance_derived);
        assert!(touches(&merged, &[a]));
    }
}

#[test]
fn label_refresh_checks_the_component_even_when_its_first_atom_is_unmarked() {
    let (mut doc, [a, _, _]) = legacy_cell();
    mark_distance_bonds(&mut doc);
    doc.models.clear();
    let extension = doc.add_atom("C", Point::new(-42., 0.));
    doc.add_bond(extension, a, 1, "plain");
    let first = doc.atoms.pop().unwrap();
    assert_eq!(first.id, extension);
    doc.atoms.insert(0, first);
    assert!(
        doc.bonds
            .iter()
            .filter(|b| b.a == extension || b.b == extension)
            .all(|b| !b.distance_derived)
    );
    let refresh =
        crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default()).unwrap();
    assert!(refresh.notice.is_none());
}

#[test]
fn malformed_distance_graph_is_rejected_by_structure_validation_first() {
    let (mut doc, _) = legacy_cell();
    mark_distance_bonds(&mut doc);
    doc.bonds[0].b = 999;
    assert!(matches!(
        molecular::prepare(&doc),
        Err(molecular::Error::Drawing(_))
    ));
    assert!(molecular::drawing_graph(&doc).is_err());
}

#[test]
fn a_new_user_drawn_edge_removing_an_existing_cell_keeps_its_own_origin() {
    let (before, [a, _, c]) = legacy_cell();
    let mut doc = before.clone();
    doc.add_bond(a, c, 1, "plain");
    let _ = transaction::reconcile(&mut doc, before).unwrap();
    assert!(!present(&doc));
    assert!(
        doc.bonds
            .iter()
            .filter(|b| !b.connects(a, c))
            .all(|b| b.distance_derived)
    );
    assert!(!doc.bond_between(a, c).unwrap().1.distance_derived);
}

#[test]
fn compound_live_cell_edits_keep_unvalidated_context_scoped_to_the_cell() {
    let (before, [a, _, c]) = legacy_cell();
    let mut doc = before.clone();
    mark_distance_bonds(&mut doc);
    let ordinary = [
        doc.add_atom("C", Point::new(200., 0.)),
        doc.add_atom("C", Point::new(242., 0.)),
    ];
    doc.add_bond(ordinary[0], ordinary[1], 1, "plain");
    let annotation = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        id: annotation,
        position: Point::new(100., 100.),
        text: "temporary".into(),
        format: Default::default(),
    });
    let before = doc.clone();
    doc.add_bond(a, c, 1, "plain");
    doc.delete(&[annotation]);
    let _ = transaction::reconcile(&mut doc, before).unwrap();
    assert!(!present(&doc));
    // A compound edit made before cell removal keeps conservative context
    // inside the imported graph, including its new unvalidated edge.
    assert!(doc.bond_between(a, c).unwrap().1.distance_derived);
    assert!(
        !doc.bond_between(ordinary[0], ordinary[1])
            .unwrap()
            .1
            .distance_derived
    );
    assert!(!touches(&doc, &ordinary));
    assert!(molecular::prepare(&editing::analysis_document(&doc, &ordinary)).is_ok());
}
