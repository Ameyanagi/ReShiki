use super::*;
use crate::{document::History, editing, scene, transaction};

fn drawing() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let ids = [("C", 0.), ("N", 40.), ("O", 80.)]
        .map(|(element, x)| doc.add_atom(element, Point::new(x, 0.)));
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], 1, "plain");
    let atoms = ids
        .iter()
        .enumerate()
        .map(|(i, id)| Placed {
            id: *id,
            xyz: [i as f32 * 1.4 - 1.4, 0., i as f32 * 0.4],
        })
        .collect();
    let mut model = Model3d::new(
        &doc,
        atoms,
        vec![Hydrogen {
            parent: ids[1],
            xyz: [0., 1., 0.4],
        }],
        ForceField::Mmff94s,
    );
    model.display = Display::Both;
    model.rotation = [1., 0., 0., 0.];
    doc.models.push(model);
    require_version(&mut doc);
    (doc, ids)
}

fn commit(doc: &mut Document, before: Document) {
    transaction::apply(doc, &mut History::default(), before, false).unwrap();
}

fn text(parts: &[Primitive]) -> Vec<(&str, Point, [u8; 3])> {
    parts
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text {
                text,
                position,
                color,
                ..
            } => Some((text.as_str(), *position, *color)),
            _ => None,
        })
        .collect()
}

#[test]
fn fingerprint_is_small_persistent_and_ignores_presentation_and_vector_order() {
    let (doc, ids) = drawing();
    let baseline = doc.models[0].fingerprint.as_ref().unwrap();
    assert_eq!(baseline.len(), 16);
    assert!(!doc.models[0].is_stale(&doc));
    let mut presented = doc.clone();
    presented.atoms.reverse();
    presented.bonds.reverse();
    presented.translate(&ids, 25., -10.);
    for atom in &mut presented.atoms {
        atom.depth += 10.;
        atom.map_num = 42;
        atom.display.hydrogens = Some(false);
    }
    presented.bonds[0].color = crate::palette::Color::Custom([10, 80, 200]);
    presented.bonds[0].highlight = Some(crate::palette::Color::Custom([255, 230, 40]));
    presented.bonds[0].projection = true;
    presented.models[0].style = Style::SpaceFilling;
    presented.models[0].look = Look::Glossy;
    presented.models[0].labels = Labels::All;
    presented.models[0].rotation = turn(presented.models[0].rotation, 50., 20., false);
    assert!(!presented.models[0].is_stale(&presented));
    let read = Document::from_json(&presented.file_json().unwrap()).unwrap();
    assert_eq!(read.models[0].fingerprint.as_ref().unwrap(), baseline);
    assert!(!read.models[0].is_stale(&read));
    // Copy order differs from the original IDs after vector reordering.
    let part = editing::selection(&presented, &ids);
    let mut pasted = Document::default();
    assert_eq!(
        editing::append(&mut pasted, &part, Point::default()).len(),
        3
    );
    assert!(!pasted.models[0].is_stale(&pasted));
}

#[test]
fn mapping_one_alkene_substituent_does_not_make_generated_chemistry_stale() {
    let mut doc = Document::default();
    let left = doc.add_atom("C", Point::new(0., 0.));
    let right = doc.add_atom("C", Point::new(40., 0.));
    let methyl = doc.add_atom("C", Point::new(-25., -30.));
    let ethyl = doc.add_atom("C", Point::new(-25., 30.));
    let terminal = doc.add_atom("C", Point::new(-65., 30.));
    let f = doc.add_atom("F", Point::new(65., -30.));
    let cl = doc.add_atom("Cl", Point::new(65., 30.));
    doc.add_bond(left, right, 2, "plain");
    for (a, b) in [
        (left, methyl),
        (left, ethyl),
        (ethyl, terminal),
        (right, f),
        (right, cl),
    ] {
        doc.add_bond(a, b, 1, "plain");
    }
    let stereo = crate::chemistry::document::prepare(&doc)
        .unwrap()
        .state
        .metadata
        .bonds[0]
        .stereo;
    assert!(matches!(stereo, 2 | 3));
    let atoms = doc
        .atoms
        .iter()
        .map(|a| Placed {
            id: a.id,
            xyz: [a.position.x / 28., a.position.y / 28., 0.],
        })
        .collect();
    let model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
    doc.models.push(model);
    require_version(&mut doc);
    assert!(!doc.models[0].is_stale(&doc));

    // Mapping just methyl changes the general legacy perception priorities,
    // unlike mapping every carbon or testing an achiral source drawing.
    doc.atom_mut(methyl).unwrap().map_num = 42;
    assert_ne!(
        crate::chemistry::document::prepare(&doc)
            .unwrap()
            .state
            .metadata
            .bonds[0]
            .stereo,
        stereo
    );
    assert!(!doc.models[0].is_stale(&doc));
    assert_eq!(doc.atom(methyl).unwrap().map_num, 42);

    // A real E/Z change must still invalidate the generated geometry.
    let position = doc.atom(f).unwrap().position;
    doc.atom_mut(f).unwrap().position = doc.atom(cl).unwrap().position;
    doc.atom_mut(cl).unwrap().position = position;
    assert!(doc.models[0].is_stale(&doc));
}

#[test]
fn conformer_dependent_chemistry_changes_the_fingerprint() {
    let (before, ids) = drawing();
    for edit in 0..9 {
        let mut doc = before.clone();
        match edit {
            0 => doc.atom_mut(ids[2]).unwrap().element = "S".into(),
            1 => doc.atom_mut(ids[1]).unwrap().charge = 1,
            2 => doc.atom_mut(ids[0]).unwrap().isotope = 13,
            3 => doc.atom_mut(ids[1]).unwrap().radical_electrons = 1,
            4 => doc.atom_mut(ids[2]).unwrap().explicit_h = 1,
            5 => doc.atom_mut(ids[2]).unwrap().no_implicit = true,
            6 => doc.atom_mut(ids[0]).unwrap().aromatic = true,
            7 => doc.bonds[0].order = 2,
            8 => {
                let element = doc.atoms[0].element.clone();
                doc.atoms[0].element = doc.atoms[2].element.clone();
                doc.atoms[2].element = element;
            }
            _ => unreachable!(),
        }
        assert!(doc.models[0].is_stale(&doc), "chemical edit {edit}");
    }
}

#[test]
fn ordinary_and_aromatic_endpoint_reversals_keep_the_fingerprint() {
    for order in [1, 4] {
        let mut doc = Document::default();
        let ids = std::array::from_fn::<_, 6, _>(|index| {
            let angle = index as f32 * std::f32::consts::TAU / 6.;
            doc.add_atom("C", Point::new(40. * angle.cos(), 40. * angle.sin()))
        });
        for index in 0..ids.len() {
            doc.add_bond(ids[index], ids[(index + 1) % ids.len()], order, "plain");
        }
        crate::chemistry::document::prepare(&doc).unwrap();
        let baseline = fingerprint(&doc, &ids);
        for bond in &mut doc.bonds {
            bond.reverse();
        }
        assert_eq!(fingerprint(&doc, &ids), baseline, "bond order {order}");
        if order == 4 {
            crate::bonds::BondPreset::Wedge.place(&mut doc, ids[0], ids[1]);
            assert!(doc.bonds[0].projection);
            assert_eq!(fingerprint(&doc, &ids), baseline);
        }
    }
}

#[test]
fn endpoint_normalization_preserves_chemical_stereo_and_dative_direction() {
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let f = doc.add_atom("F", Point::new(0., -36.));
    let cl = doc.add_atom("Cl", Point::new(31., 18.));
    let br = doc.add_atom("Br", Point::new(-31., 18.));
    doc.add_bond(c, f, 1, "wedge");
    doc.add_bond(c, cl, 1, "plain");
    doc.add_bond(c, br, 1, "plain");
    let ids = [c, f, cl, br];
    let prepared = crate::chemistry::document::prepare(&doc).unwrap();
    assert_ne!(prepared.state.metadata.atoms[0].chiral_tag, 0);
    let baseline = fingerprint(&doc, &ids);
    doc.bonds[1].reverse();
    doc.bonds[2].reverse();
    assert_eq!(fingerprint(&doc, &ids), baseline);
    doc.bonds[0].display = "hash".into();
    assert_ne!(fingerprint(&doc, &ids), baseline);

    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::default());
    let cu = doc.add_atom("Cu", Point::new(40., 0.));
    doc.add_bond(n, cu, crate::bonds::DATIVE, "plain");
    let baseline = fingerprint(&doc, &[n, cu]);
    doc.bonds[0].reverse();
    assert_ne!(fingerprint(&doc, &[n, cu]), baseline);
}

#[test]
fn deleted_geometry_and_original_layout_cannot_alias_a_reused_id_after_paste() {
    for paste in [false, true] {
        let (mut doc, ids) = drawing();
        let drawn = Drawn::of(&doc, &doc.models[0]);
        let model = doc.models[0].clone();
        lay_out_flat(&mut doc, &model);
        doc.models[0].layout = Layout::View;
        doc.models[0].drawn = Some(drawn.kept(&doc, &doc.models[0]));
        let before = doc.clone();
        doc.delete(&[ids[2]]);
        commit(&mut doc, before);
        assert_eq!(
            doc.models[0].geometry_sources(),
            [Some(ids[0]), Some(ids[1]), None]
        );
        assert_eq!(doc.models[0].drawn.as_ref().unwrap().atoms.len(), 2);
        if paste {
            let part = editing::selection(&doc, &ids[..2]);
            let mut pasted = Document::default();
            assert_eq!(
                editing::append(&mut pasted, &part, Point::new(80., 40.)).len(),
                2
            );
            assert_eq!(pasted.models[0].atoms, doc.models[0].atoms);
            assert_eq!(pasted.models[0].geometry_sources()[2], None);
            doc = pasted;
        }
        let before = doc.clone();
        let added = doc.add_atom("F", Point::new(240., 80.));
        assert_eq!(added, ids[2], "the deleted highest drawing ID is reused");
        doc.atom_mut(added).unwrap().depth = 9.;
        doc.add_bond(ids[0], added, 1, "plain");
        commit(&mut doc, before);
        let added_atom = doc.atom(added).unwrap().clone();
        assert!(!doc.models[0].contains(added));
        assert_eq!(doc.models[0].geometry_sources()[2], None);
        assert_eq!(doc.models[0].generated.as_ref().unwrap().elements[2], "O");
        assert!(restore_drawing(&mut doc, 0));
        assert_eq!(doc.atom(added).unwrap(), &added_atom, "paste: {paste}");
        doc.validate().unwrap();
    }
}

#[test]
fn changed_and_removed_atoms_leave_the_generated_elements_and_bonds_intact() {
    let (before, ids) = drawing();
    let geometry = format!("{:?}", primitives(&before));
    let origin = frame_origin(&before, &before.models[0]).unwrap();
    let mut doc = before.clone();
    doc.atom_mut(ids[2]).unwrap().element = "S".into();
    doc.bonds[0].order = 2;
    doc.delete(&[ids[2]]);
    commit(&mut doc, before.clone());
    assert!(doc.models[0].is_stale(&doc));
    assert_eq!(doc.models[0].atoms, before.models[0].atoms);
    assert_eq!(
        doc.models[0].generated.as_ref().unwrap().elements,
        before.models[0].generated.as_ref().unwrap().elements
    );
    assert_eq!(
        doc.models[0].generated.as_ref().unwrap().bonds,
        before.models[0].generated.as_ref().unwrap().bonds
    );
    assert_eq!(
        doc.models[0].geometry_sources(),
        [Some(ids[0]), Some(ids[1]), None]
    );
    assert_eq!(frame_origin(&doc, &doc.models[0]).unwrap(), origin);
    assert_eq!(format!("{:?}", primitives(&doc)), geometry);
    assert_eq!(doc.models[0].linked_ids(&doc), ids[..2]);
    assert!(!doc.models[0].contains(ids[2]));
    assert_eq!(selected_molecule(&doc, &[ids[0]]), Some(ids[..2].to_vec()));
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert!(read.models[0].is_stale(&read));
    assert_eq!(format!("{:?}", primitives(&read)), geometry);
    let part = editing::selection(&doc, &ids[..2]);
    assert_eq!(part.models.len(), 1);
    let mut pasted = Document::default();
    let mapped = editing::append(&mut pasted, &part, Point::default());
    assert_eq!(mapped.len(), 2);
    assert_eq!(pasted.models.len(), 1);
    assert_eq!(pasted.models[0].atoms.len(), 3);
    assert_eq!(pasted.models[0].generated, doc.models[0].generated);
    assert!(pasted.models[0].is_stale(&pasted));
    pasted.validate().unwrap();
}

#[test]
fn additions_and_splits_keep_the_source_link_undo_and_stale_geometry() {
    let (before, ids) = drawing();
    let mut doc = before.clone();
    let extra = doc.add_atom("F", Point::new(120., 0.));
    doc.add_bond(ids[2], extra, 1, "plain");
    let mut history = History::default();
    transaction::apply(&mut doc, &mut history, before.clone(), false).unwrap();
    assert!(!doc.models[0].contains(extra));
    assert_eq!(doc.models[0].linked_ids(&doc), ids);
    assert_eq!(
        selected_molecule(&doc, &[extra]),
        Some(vec![ids[0], ids[1], ids[2], extra])
    );
    assert!(doc.models[0].is_stale(&doc));
    assert_eq!(doc.models[0].atoms, before.models[0].atoms);
    let added = doc.clone();
    history.undo(&mut doc);
    assert_eq!(doc, before);
    history.redo(&mut doc);
    assert_eq!(doc, added);
    let before_split = doc.clone();
    doc.bonds.clear();
    commit(&mut doc, before_split);
    assert_eq!(doc.models[0].linked_ids(&doc), ids);
    assert!(doc.models[0].is_stale(&doc));
    assert_eq!(doc.models[0].chemistry_ids(&doc), ids);
    doc.delete(&[ids[0], ids[1], ids[2], extra]);
    assert!(doc.models.is_empty());
}

#[test]
fn a_split_keeps_every_source_fragment_and_deleting_one_retains_the_others() {
    let (mut before, ids) = drawing();
    before.models[0].display = Display::ThreeD;
    let geometry = before.models[0].atoms.clone();
    let mut doc = before.clone();
    doc.bonds.retain(|b| b.a != ids[0] && b.b != ids[0]);
    commit(&mut doc, before.clone());
    assert_eq!(doc.models[0].linked_ids(&doc), ids);
    assert_eq!(selected_molecule(&doc, &[ids[0]]), Some(ids.to_vec()));
    assert_eq!(selected_molecule(&doc, &[ids[1]]), Some(ids.to_vec()));
    assert!(!doc.atom_drawn(ids[1]) && !doc.atom_drawn(ids[2]));
    assert!(!doc.atom_drawn(ids[0]));
    assert_eq!(doc.models[0].atoms, geometry);
    assert!(doc.models[0].is_stale(&doc));
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(read.models, doc.models);

    let mut deleted = before.clone();
    deleted.delete(&[ids[0]]);
    commit(&mut deleted, before);
    assert_eq!(deleted.models[0].linked_ids(&deleted), ids[1..]);
    assert_eq!(deleted.models[0].chemistry_ids(&deleted), ids[1..]);
    assert_eq!(deleted.models[0].atoms, geometry);
}

#[test]
fn restoring_a_split_model_restores_every_surviving_source_fragment() {
    let (mut doc, ids) = drawing();
    let drawn = Drawn::of(&doc, &doc.models[0]);
    for atom in &mut doc.atoms {
        atom.position.y += 60.;
    }
    doc.models[0].drawn = Some(drawn.kept(&doc, &doc.models[0]));
    let before = doc.clone();
    doc.bonds
        .retain(|bond| bond.a != ids[0] && bond.b != ids[0]);
    commit(&mut doc, before);
    assert!(restore_drawing(&mut doc, 0));
    for (index, id) in ids.into_iter().enumerate() {
        assert_eq!(
            doc.atom(id).unwrap().position,
            Point::new(index as f32 * 40., 0.)
        );
    }
}

#[test]
fn restoring_original_coordinates_preserves_an_edited_double_bond_configuration() {
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", Point::new(0., 0.));
    let c2 = doc.add_atom("C", Point::new(36., 0.));
    let f1 = doc.add_atom("F", Point::new(-18., -30.));
    let f2 = doc.add_atom("F", Point::new(54., 30.));
    doc.add_bond(c1, c2, 2, "plain");
    doc.add_bond(c1, f1, 1, "plain");
    doc.add_bond(c2, f2, 1, "plain");
    let atoms = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| Placed {
            id: atom.id,
            xyz: [index as f32, (index % 2) as f32, 0.],
        })
        .collect();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    require_version(&mut doc);
    let original = Drawn::of(&doc, &doc.models[0]);
    doc.models[0].drawn = Some(original);
    doc.bonds[0].stereo = Some("z".into());
    doc.bonds[0].stereo_atoms = vec![f1, f2];
    doc.bonds[0].stereo_authoritative = true;
    assert!(doc.models[0].is_stale(&doc));
    let current = fingerprint(&doc, &doc.models[0].chemistry_ids(&doc));
    for regenerate in [false, true] {
        let mut restored = doc.clone();
        if regenerate {
            let old = &restored.models[0];
            let atoms = old
                .atoms
                .iter()
                .zip(old.geometry_sources())
                .map(|(atom, source)| Placed {
                    id: source.unwrap(),
                    xyz: atom.xyz,
                })
                .collect();
            let mut generated = Model3d::new(&restored, atoms, vec![], old.force_field);
            generated.drawn = old.drawn.clone();
            restored.models[0] = generated;
            assert!(!restored.models[0].is_stale(&restored));
        }
        assert!(restore_drawing(&mut restored, 0));
        assert!(
            matches!(restored.bonds[0].stereo.as_deref(), Some("z" | "cis")),
            "regenerated: {regenerate}"
        );
        assert!(restored.bonds[0].stereo_authoritative);
        assert_eq!(
            fingerprint(&restored, &restored.models[0].chemistry_ids(&restored)),
            current
        );
    }
}

#[test]
fn merged_models_keep_disjoint_frame_selection_and_share_the_changed_target() {
    let (mut before, ids) = drawing();
    before.models[0].display = Display::ThreeD;
    let other =
        [("C", 220.), ("F", 260.)].map(|(element, x)| before.add_atom(element, Point::new(x, 0.)));
    before.add_bond(other[0], other[1], 1, "plain");
    let mut second = Model3d::new(
        &before,
        other
            .iter()
            .enumerate()
            .map(|(i, id)| Placed {
                id: *id,
                xyz: [i as f32 * 1.4, 0., 0.],
            })
            .collect(),
        vec![],
        ForceField::Mmff94s,
    );
    second.display = Display::ThreeD;
    second.offset = Point::new(180., 80.);
    before.models.push(second);
    let origins: Vec<_> = before
        .models
        .iter()
        .map(|m| frame_origin(&before, m).unwrap())
        .collect();
    let mut doc = before.clone();
    doc.add_bond(ids[2], other[0], 1, "plain");
    let added = doc.add_atom("H", Point::new(-40., 0.));
    doc.add_bond(ids[0], added, 1, "plain");
    commit(&mut doc, before.clone());
    assert_eq!(doc.models.len(), 2);
    let first_links = ids.to_vec();
    assert_eq!(doc.models[0].linked_ids(&doc), first_links);
    assert_eq!(doc.models[1].linked_ids(&doc), other);
    let all: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    for (index, model) in doc.models.iter().enumerate() {
        assert!(model.is_stale(&doc));
        assert_eq!(
            model
                .chemistry_ids(&doc)
                .into_iter()
                .collect::<HashSet<_>>(),
            all
        );
        assert_eq!(model.atoms, before.models[index].atoms);
        assert_eq!(frame_origin(&doc, model).unwrap(), origins[index]);
    }
    assert_eq!(frame_hit(&doc, origins[0]), Some(first_links.clone()));
    assert_eq!(frame_hit(&doc, origins[1]), Some(other.to_vec()));
    assert_eq!(selected_molecule(&doc, &[ids[0]]), Some(first_links));
    assert_eq!(selected_molecule(&doc, &[other[0]]), Some(other.to_vec()));
    doc.validate().unwrap();
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(read.models, doc.models);
    assert!(read.models.iter().all(|m| m.is_stale(&read)));
    // Reconciliation is stable across later transactions.
    let stable = doc.clone();
    reconcile(&mut doc, &stable);
    assert_eq!(doc, stable);
}

#[test]
fn legacy_models_capture_the_baseline_before_the_first_chemical_edit() {
    let (mut before, ids) = drawing();
    let sources = before.models[0].geometry_sources();
    for hydrogen in &mut before.models[0].hydrogens {
        hydrogen.parent = sources[hydrogen.parent as usize].unwrap();
    }
    for (atom, source) in before.models[0].atoms.iter_mut().zip(sources) {
        atom.id = source.unwrap();
    }
    before.models[0].fingerprint = None;
    before.models[0].generated = None;
    before.version = crate::document::MODELS_3D;
    let mut legacy = serde_json::to_value(&before).unwrap();
    legacy["models"][0]
        .as_object_mut()
        .unwrap()
        .remove("linked_atoms");
    let before = Document::from_json(&serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(before.models[0].linked_atoms, ids);
    assert!(!before.models[0].is_file_derived());
    assert!(!before.models[0].is_stale(&before));
    assert!(before.models[0].generated.is_none());
    assert!(before.models[0].fingerprint.is_none());
    let mut doc = before.clone();
    doc.atom_mut(ids[2]).unwrap().element = "S".into();
    doc.delete(&[ids[0]]);
    commit(&mut doc, before);
    assert!(doc.models[0].is_stale(&doc));
    assert_eq!(
        doc.models[0].generated.as_ref().unwrap().elements,
        ["C", "N", "O"]
    );
    assert_eq!(doc.version, crate::document::MODEL_POLISH);
}

#[test]
fn file_models_are_exempt_with_a_cell_or_only_imported_sites() {
    let (before, ids) = drawing();
    for cell in [
        None,
        Some(Cell {
            vectors: [[4., 0., 0.], [0., 4., 0.], [0., 0., 4.]],
            crossing: vec![],
        }),
    ] {
        let mut doc = before.clone();
        doc.models[0] = doc.models[0].clone().with_cell(
            &doc,
            cell,
            vec![Site {
                id: ids[0],
                label: "C1".into(),
                adp: None,
                asymmetric: true,
            }],
        );
        assert!(doc.models[0].is_file_derived());
        assert!(doc.models[0].fingerprint.is_none());
        assert!(doc.models[0].generated.is_none());
        let snapshot = doc.clone();
        doc.atom_mut(ids[2]).unwrap().element = "S".into();
        doc.atom_mut(ids[1]).unwrap().charge = 1;
        commit(&mut doc, snapshot.clone());
        assert_eq!(doc.models, snapshot.models);
        assert!(!doc.models[0].is_stale(&doc));
    }
}

#[test]
fn label_modes_omit_hidden_hydrogens_and_labels_travel_with_their_atom_depth() {
    let (mut doc, _) = drawing();
    assert!(text(&primitives(&doc)).is_empty());
    doc.models[0].labels = Labels::Heteroatoms;
    let parts = primitives(&doc);
    assert_eq!(
        text(&parts).iter().map(|v| v.0).collect::<Vec<_>>(),
        ["N", "O"]
    );
    doc.models[0].labels = Labels::All;
    let parts = primitives(&doc);
    assert_eq!(text(&parts).len(), 4);
    let view = View::new(&doc, &doc.models[0]).unwrap();
    for part in &parts {
        if let Primitive::Text {
            text,
            position,
            size,
            style,
            ..
        } = part
        {
            let index = view
                .symbols
                .iter()
                .position(|symbol| *symbol == text)
                .unwrap();
            let center = view.project(view.rotated[index]).0;
            let width = crate::style::styled_text_width(text, *size, style);
            assert!((position.y + size * 0.6 - center.y).abs() < 1e-4);
            assert!((position.x + width / 2. - center.x).abs() < 1e-4);
        }
    }
    let carbon = parts
        .iter()
        .position(|p| matches!(p, Primitive::Text { text, .. } if text == "C"))
        .unwrap();
    let oxygen = parts
        .iter()
        .position(|p| matches!(p, Primitive::Sphere { center, .. } if center.x > 65.))
        .unwrap();
    assert!(carbon < oxygen, "near atoms can occlude farther labels");
    doc.models[0].show_hydrogens = Hydrogens::None;
    assert_eq!(
        text(&primitives(&doc))
            .iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        ["C", "N", "O"]
    );
    doc.models[0].display = Display::ThreeD;
    let svg = scene::svg(&doc);
    assert!(svg.contains(">N</text>") && svg.contains(">O</text>"));
    assert!(!svg.contains(">H</text>"));
}

#[test]
fn labels_fade_with_depth_and_wireframe_labels_contrast_with_each_canvas() {
    let (mut doc, _) = drawing();
    doc.models[0].labels = Labels::All;
    doc.models[0].style = Style::Wireframe;
    for theme in [
        crate::canvas_theme::CanvasTheme::Light,
        crate::canvas_theme::CanvasTheme::Dark,
    ] {
        doc.canvas_theme = theme;
        doc.models[0].depth_fade = 0.;
        let parts = primitives(&doc);
        let solid: Vec<_> = text(&parts).iter().map(|v| theme.color(v.2)).collect();
        assert!(solid.iter().all(|c| if theme.is_light() {
            c[0] < 50
        } else {
            c[0] > 200
        }));
        doc.models[0].depth_fade = 1.;
        let faded_parts = primitives(&doc);
        let faded: Vec<_> = text(&faded_parts)
            .iter()
            .map(|v| theme.color(v.2))
            .collect();
        assert_ne!(solid, faded);
    }
}

#[test]
fn optional_labels_and_seed_round_trip_and_legacy_defaults_stay_omitted() {
    let (mut doc, _) = drawing();
    let value = serde_json::to_value(&doc).unwrap();
    assert!(value["models"][0].get("labels").is_none());
    assert!(value["models"][0].get("seed").is_none());
    doc.models[0].labels = Labels::All;
    doc.models[0].seed = 123456;
    let read = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(read.models, doc.models);
    assert_eq!(read.version, crate::document::MODEL_POLISH);
    let mut old = serde_json::to_value(&doc).unwrap();
    old["version"] = crate::document::DISTANCE_BONDS.into();
    assert!(Document::from_json(&serde_json::to_vec(&old).unwrap()).is_err());
}
