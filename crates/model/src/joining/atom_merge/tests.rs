use super::*;
use crate::{
    document::{Annotation, Arrow, AtomStereo},
    reactions::{self, Role},
};

fn sites(n: usize, element: &str, arms: bool) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let mut selected = vec![];
    for index in 0..n {
        let angle = index as f32 * std::f32::consts::TAU / n as f32;
        let id = doc.add_atom(element, Point::new(12. * angle.cos(), 12. * angle.sin()));
        selected.push(id);
        if arms {
            let end = doc.add_atom("C", Point::new(54. * angle.cos(), 54. * angle.sin()));
            doc.add_bond(id, end, 1, "plain");
        }
    }
    (doc, selected)
}

fn rejects(doc: &Document, selected: &[u64], diagnostic: &str) {
    let before = doc.clone();
    let error = merge_atoms(doc, selected).unwrap_err();
    assert!(error.contains(diagnostic), "{error}");
    assert_eq!(*doc, before);
}

#[test]
fn asymmetric_three_atoms_use_bounds_center_and_first_style_with_fixed_neighbors() {
    let doc = Document::from_json(include_bytes!(
        "../../../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    let before = doc.clone();
    let (joined, selected) = merge_atoms(&doc, &[1, 3, 5]).unwrap();
    assert_eq!(selected, [1]);
    assert_eq!((joined.atoms.len(), joined.bonds.len()), (4, 3));
    let atom = joined.atom(1).unwrap();
    assert_eq!(atom.element, "N");
    assert_eq!(atom.position, Point::new(0., 2.));
    // The arithmetic mean of this asymmetric selection is (0, 0).
    assert_ne!(atom.position, Point::default());
    assert_eq!(atom.text_style, doc.atom(1).unwrap().text_style);
    for id in [2, 4, 6] {
        assert_eq!(joined.atom(id).unwrap(), doc.atom(id).unwrap());
    }
    assert_eq!(doc, before);
    joined.validate().unwrap();
    let mut saved = joined.clone();
    saved.version = crate::document::VERSION;
    assert_eq!(
        Document::from_json(&serde_json::to_vec(&saved).unwrap()).unwrap(),
        saved
    );
}

#[test]
fn four_and_many_atoms_merge_in_selection_order_including_different_labels() {
    for count in [3, 4, 12, 300] {
        let (mut doc, mut selected) = sites(count, "C", false);
        doc.atom_mut(selected[0]).unwrap().element = "N".into();
        doc.atom_mut(*selected.last().unwrap()).unwrap().element = "O".into();
        doc.atom_mut(*selected.last().unwrap()).unwrap().text_style =
            Some(crate::typography::TextStyle {
                color: crate::palette::Color::Custom([20, 70, 140]),
                ..Default::default()
            });
        for reverse in [false, true] {
            if reverse {
                selected.reverse();
            }
            let (joined, ids) = merge_atoms(&doc, &selected).unwrap();
            assert_eq!(joined.atoms.len(), 1);
            let expected = doc.atom(selected[0]).unwrap();
            assert_eq!(ids, [expected.id]);
            assert_eq!(joined.atoms[0].element, expected.element);
            assert_eq!(joined.atoms[0].text_style, expected.text_style);
            let expected_center = if count == 3 {
                Point::new(3., 0.)
            } else {
                Point::default()
            };
            assert!(joined.atoms[0].position.distance(expected_center) < 0.0001);
        }
    }
}

#[test]
fn supplied_position_is_used_and_nonfinite_positions_reject_atomically() {
    let (mut doc, selected) = sites(3, "N", true);
    for (id, depth) in selected.iter().zip([2., 5., -1.]) {
        doc.atom_mut(*id).unwrap().depth = depth;
    }
    let before = doc.clone();
    let position = Point::new(42., -23.);
    let (joined, ids) = merge_atoms_at(&doc, &selected, position).unwrap();
    assert_eq!(ids, [selected[0]]);
    assert_eq!(joined.atom(selected[0]).unwrap().position, position);
    assert_eq!(joined.atom(selected[0]).unwrap().depth, 2.);
    for atom in doc.atoms.iter().filter(|a| !selected.contains(&a.id)) {
        assert_eq!(joined.atom(atom.id).unwrap(), atom);
    }
    for invalid in [
        Point::new(f32::NAN, 0.),
        Point::new(0., f32::NAN),
        Point::new(f32::INFINITY, 0.),
        Point::new(0., f32::NEG_INFINITY),
    ] {
        let error = merge_atoms_at(&doc, &selected, invalid).unwrap_err();
        assert!(error.contains("finite"), "{error}");
        assert_eq!(doc, before);
    }
}

#[test]
fn native_valence_accepts_three_and_four_arms_and_metal_centers_and_rejects_five_carbons() {
    for (n, element) in [(3, "N"), (4, "C"), (6, "Rh"), (5, "P")] {
        let (doc, selected) = sites(n, element, true);
        let (joined, ids) = merge_atoms(&doc, &selected).unwrap();
        assert_eq!((joined.atoms.len(), joined.bonds.len()), (n + 1, n));
        assert_eq!(ids, [selected[0]]);
        joined.validate().unwrap();
    }
    let (doc, selected) = sites(5, "C", true);
    rejects(&doc, &selected, "valence");
}

#[test]
fn self_loops_disappear_and_identical_duplicate_bonds_coalesce() {
    let (mut doc, selected) = sites(3, "C", false);
    let external = doc.add_atom("C", Point::new(70., 0.));
    for id in &selected {
        doc.add_bond(*id, external, 1, "plain");
    }
    doc.add_bond(selected[0], selected[1], 1, "plain");
    let (joined, _) = merge_atoms(&doc, &selected).unwrap();
    assert_eq!((joined.atoms.len(), joined.bonds.len()), (2, 1));
    assert_eq!(
        (joined.bonds[0].a, joined.bonds[0].b),
        (selected[0], external)
    );
    doc.bonds[1].order = 2;
    rejects(&doc, &selected, "conflicting order");
    doc.bonds[1].order = 1;
    doc.bonds[1].color = crate::palette::Color::Custom([1, 2, 3]);
    rejects(&doc, &selected, "appearance");
}

#[test]
fn reversed_dative_duplicates_are_rejected_without_losing_direction() {
    let (mut doc, selected) = sites(3, "Rh", false);
    let external = doc.add_atom("N", Point::new(70., 0.));
    doc.add_bond(external, selected[0], 5, "plain");
    doc.add_bond(selected[1], external, 5, "plain");
    rejects(&doc, &selected, "direction");
}

#[test]
fn chemical_metadata_conflicts_are_specific_and_atomic() {
    let (original, selected) = sites(3, "C", false);
    for (index, diagnostic) in [
        "charges",
        "isotopes",
        "radicals",
        "fixed hydrogens",
        "fixed hydrogens",
        "aromatic states",
        "atom maps",
    ]
    .iter()
    .enumerate()
    {
        let mut doc = original.clone();
        let atom = doc.atom_mut(selected[1]).unwrap();
        match index {
            0 => atom.charge = 1,
            1 => atom.isotope = 13,
            2 => atom.radical_electrons = 1,
            3 => atom.explicit_h = 1,
            4 => atom.no_implicit = true,
            5 => atom.aromatic = true,
            6 => atom.map_num = 7,
            _ => unreachable!(),
        }
        rejects(&doc, &selected, diagnostic);
    }
    let mut mapped = original.clone();
    for id in &selected {
        mapped.atom_mut(*id).unwrap().map_num = 9;
    }
    let (joined, _) = merge_atoms(&mapped, &selected).unwrap();
    assert_eq!(joined.atoms[0].map_num, 9);
    let mut marked = original.clone();
    marked
        .atom_mut(selected[1])
        .unwrap()
        .marks
        .push(crate::scientific::AtomMark {
            kind: crate::scientific::MarkKind::LonePair,
            offset: Point::new(10., 0.),
            angle: 0.,
            size_pt: None,
        });
    rejects(&marked, &selected, "atom marks");
}

fn stereo_fixture() -> (Document, u64, Vec<u64>) {
    let (mut doc, selected) = sites(3, "C", false);
    let center = doc.add_atom("C", Point::new(100., 0.));
    let mut neighbors = vec![selected[1]];
    for (element, position) in [
        ("F", Point::new(100., 42.)),
        ("Cl", Point::new(142., 0.)),
        ("Br", Point::new(100., -42.)),
    ] {
        neighbors.push(doc.add_atom(element, position));
    }
    for id in &neighbors {
        doc.add_bond(center, *id, 1, "plain");
    }
    doc.atom_mut(center).unwrap().stereo = Some(AtomStereo {
        winding: "ccw".into(),
        neighbors,
    });
    (doc, center, selected)
}

#[test]
fn remote_stereo_references_remap_without_changing_winding_or_neighbor_order() {
    let (doc, center, selected) = stereo_fixture();
    let (joined, _) = merge_atoms(&doc, &selected).unwrap();
    let mut expected = doc.atom(center).unwrap().stereo.clone().unwrap();
    expected.neighbors[0] = selected[0];
    assert_eq!(
        joined.atom(center).unwrap().stereo.as_ref(),
        Some(&expected)
    );
    assert_eq!(
        joined.atom(center).unwrap().position,
        doc.atom(center).unwrap().position
    );
    joined.validate().unwrap();
}

#[test]
fn merged_stereocenters_collapsed_neighbors_and_incident_stereo_bonds_are_rejected() {
    let (mut doc, center, selected) = stereo_fixture();
    rejects(&doc, &[center, selected[0], selected[2]], "stereocenter");
    let other_neighbor = doc.atom(center).unwrap().stereo.as_ref().unwrap().neighbors[1];
    // Match labels to isolate the topology conflict from atom metadata.
    doc.atom_mut(other_neighbor).unwrap().element = "C".into();
    rejects(
        &doc,
        &[selected[0], selected[1], other_neighbor],
        "distinct stereocenter",
    );
    doc.bonds[0].display = "hash".into();
    rejects(&doc, &selected, "stereochemical bond");
}

#[test]
fn groups_reaction_roles_coefficients_and_captions_survive_remapping() {
    let (mut doc, selected) = sites(3, "C", false);
    doc.add_bond(selected[0], selected[1], 1, "plain");
    doc.add_bond(selected[1], selected[2], 1, "plain");
    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(50., 80.),
        text: "Reactant".into(),
        format: Default::default(),
    });
    doc.group_selection(&[selected[0], caption]).unwrap();
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(100., 0.),
        Point::new(200., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut doc, arrow, &selected, Role::Reactant).unwrap();
    doc.reactions[0].reactants[0].coefficient = 7;
    doc.reactions[0].annotations = vec![caption];
    let reversed = [selected[2], selected[1], selected[0]];
    let (joined, _) = merge_atoms(&doc, &reversed).unwrap();
    assert_eq!(joined.groups[0].members, [selected[2], caption]);
    assert_eq!(joined.reactions[0].reactants[0].atoms, [selected[2]]);
    assert_eq!(joined.reactions[0].reactants[0].coefficient, 7);
    assert_eq!(joined.reactions[0].annotations, [caption]);
    assert_eq!(joined.arrows, doc.arrows);
    assert_eq!(joined.annotations, doc.annotations);
}

#[test]
fn joins_between_separate_reaction_participants_keep_the_existing_diagnostic() {
    let (mut doc, selected) = sites(3, "C", false);
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(100., 0.),
        Point::new(200., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut doc, arrow, &[selected[0]], Role::Reactant).unwrap();
    reactions::assign(&mut doc, arrow, &[selected[1]], Role::Product).unwrap();
    rejects(&doc, &selected, "separate reaction participants");
}

#[test]
fn centroid_and_semantic_attachment_targets_remap_and_collapse_is_rejected() {
    for attachment in [false, true] {
        let (mut doc, selected) = sites(3, "C", false);
        let other = doc.add_atom("C", Point::new(84., 0.));
        let point = if attachment {
            crate::attachments::add(
                &mut doc,
                &[selected[1], other],
                crate::attachments::Kind::MultiCenter,
            )
            .unwrap()
        } else {
            crate::projection::add_centroid(&mut doc, &[selected[1], other]).unwrap()
        };
        let (joined, _) = merge_atoms(&doc, &selected).unwrap();
        assert_eq!(joined.atom(point).unwrap().centroid, [selected[0], other]);
        joined.validate().unwrap();
        rejects(
            &doc,
            &[selected[0], selected[1], point],
            "centroid or attachment",
        );
        doc.atom_mut(point).unwrap().centroid = selected.clone();
        rejects(&doc, &selected, "target set");
    }
}

#[test]
fn depth_paint_remaps_without_stale_ids_and_conflicts_are_rejected() {
    let (mut doc, selected) = sites(3, "C", false);
    doc.depth_appearance.push(crate::depth_appearance::Scope {
        atoms: selected.clone(),
        automatic: false,
        strength: 0.6,
        weights: selected.iter().map(|id| (*id, 0.2)).collect(),
        overrides: BTreeMap::new(),
    });
    let (joined, _) = merge_atoms(&doc, &selected).unwrap();
    assert_eq!(joined.depth_appearance[0].atoms, [selected[0]]);
    assert_eq!(
        joined.depth_appearance[0].weights,
        BTreeMap::from([(selected[0], 0.2)])
    );
    doc.depth_appearance[0].weights.insert(selected[1], 0.5);
    rejects(&doc, &selected, "depth paint");
}

#[test]
fn painted_ring_members_remap_and_collapsed_ring_paint_is_rejected() {
    let (mut doc, selected) = sites(3, "C", false);
    let ring_a = doc.add_atom("C", Point::new(100., 0.));
    let ring_b = doc.add_atom("C", Point::new(121., 36.));
    let ring_c = doc.add_atom("C", Point::new(79., 36.));
    for (a, b) in [(ring_a, ring_b), (ring_b, ring_c), (ring_c, ring_a)] {
        doc.add_bond(a, b, 1, "plain");
    }
    doc.ring_fills.push(crate::ring_fills::RingFill {
        atoms: vec![ring_a, ring_b, ring_c],
        color: crate::palette::Color::Custom([80, 100, 150]),
    });
    let (joined, _) = merge_atoms(&doc, &[selected[0], ring_a, selected[1]]).unwrap();
    assert_eq!(joined.ring_fills[0].atoms, [selected[0], ring_b, ring_c]);
    assert_eq!(joined.ring_fills[0].color, doc.ring_fills[0].color);
    rejects(&doc, &[selected[0], ring_a, ring_b], "painted ring");
}

#[test]
fn remote_double_bond_stereo_references_remap_and_incident_stereo_is_rejected() {
    let (mut doc, selected) = sites(3, "C", false);
    let a = doc.add_atom("C", Point::new(100., 0.));
    let b = doc.add_atom("C", Point::new(142., 0.));
    let ligand = doc.add_atom("F", Point::new(142., 42.));
    doc.add_bond(a, selected[1], 1, "plain");
    doc.add_bond(b, ligand, 1, "plain");
    doc.add_bond(a, b, 2, "plain");
    let double = doc.bonds.last_mut().unwrap();
    double.stereo = Some("E".into());
    double.stereo_atoms = vec![selected[1], ligand];
    double.stereo_authoritative = true;
    let (joined, _) = merge_atoms(&doc, &selected).unwrap();
    let double = joined.bonds.last().unwrap();
    assert_eq!(double.stereo.as_deref(), Some("E"));
    assert_eq!(double.stereo_atoms, [selected[0], ligand]);
    assert!(double.stereo_authoritative);
    rejects(&doc, &[a, selected[0], selected[2]], "stereochemical bond");
}

#[test]
fn mixed_objects_duplicate_ids_small_selections_and_abbreviations_are_rejected() {
    let (mut doc, selected) = sites(3, "C", false);
    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::default(),
        text: "Note".into(),
        format: Default::default(),
    });
    rejects(&doc, &[selected[0], selected[1]], "at least three");
    rejects(
        &doc,
        &[selected[0], selected[1], selected[1]],
        "distinct atoms",
    );
    rejects(&doc, &[selected[0], selected[1], caption], "only atoms");
    doc.add_bond(selected[0], selected[1], 1, "plain");
    doc.contract(&[selected[0], selected[1]], "Et", "Et")
        .unwrap();
    rejects(&doc, &selected, "Expand abbreviations");
}
