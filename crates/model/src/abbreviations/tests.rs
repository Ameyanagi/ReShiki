use super::*;

#[test]
fn abbreviation_containing_returns_the_first_member_match_or_none() {
    let first = Abbreviation {
        label_style: None,
        label_color_override: false,
        highlight: None,
        label: "first".into(),
        reverse_label: String::new(),
        anchor: 1,
        exit: None,
        members: vec![1, 2],
        alignment: Default::default(),
    };
    let second = Abbreviation {
        label: "second".into(),
        anchor: 2,
        members: vec![2, 3],
        ..first.clone()
    };
    let doc = Document {
        abbreviations: vec![first.clone(), second.clone()],
        ..Default::default()
    };

    assert_eq!(doc.abbreviation_containing(1), Some(&first));
    assert_eq!(doc.abbreviation_containing(2), Some(&first));
    assert_eq!(doc.abbreviation_containing(3), Some(&second));
    assert_eq!(doc.abbreviation_containing(99), None);
}

#[test]
fn reversed_charged_labels_anchor_on_their_atom() {
    assert_eq!(anchor_range("Me3N+", true), 3..4);
    assert_eq!(anchor_range("F3B-", true), 2..3);
    assert_eq!(anchor_range("NMe3+", false), 0..1);
    assert_eq!(anchor_range("TBSO", true), 3..4);
    // Isothiocyanate and thiocyanate reverse to distinct spellings that end
    // in their attachment atom.
    assert_eq!(anchor_range("S=C=N", true), 4..5);
    let thiocyanate = "N≡CS";
    assert_eq!(&thiocyanate[anchor_range(thiocyanate, true)], "S");
}

#[test]
fn iupac_peptide_aliases_anchor_on_their_oxygen() {
    for (text, left) in [
        ("OBzl", false),
        ("BzlO", true),
        ("OBut", false),
        ("ButO", true),
    ] {
        assert_eq!(&text[anchor_range(text, left)], "O", "{text}");
    }
}

fn shortcut_examples() -> Document {
    Document::from_json(include_bytes!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap()
}

#[test]
fn bond_visible_matches_atom_visibility_and_shared_groups() {
    let doc = shortcut_examples();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    for &a in &ids {
        for &b in &ids {
            let expected = doc.atom_visible(a)
                && doc.atom_visible(b)
                && !doc
                    .abbreviations
                    .iter()
                    .any(|g| g.members.contains(&a) && g.members.contains(&b));
            assert_eq!(doc.bond_visible(a, b), expected, "{a}-{b}");
        }
    }
}

#[test]
fn legacy_deserialization_preserves_the_original_abbreviation_fields_and_version() {
    let bytes =
        include_bytes!("../../../../tests/fixtures/qa-label-feedback/before-acetyl-repeat.rsk");
    let source: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let doc: Document = serde_json::from_slice(bytes).unwrap();
    assert_eq!(doc.version, 15);
    assert_eq!(source["version"], doc.version);
    assert!(doc.abbreviations.iter().all(|group| group.exit.is_none()));
    let saved = serde_json::to_value(&doc).unwrap();
    assert_eq!(saved["version"], source["version"]);
    assert_eq!(saved["abbreviations"], source["abbreviations"]);
    assert!(
        saved["abbreviations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|group| group.get("exit").is_none())
    );
    assert_eq!(doc.file_version(), crate::document::DISTANCE_BONDS);
}

/// The real exit is below the visible label-to-right bond, so stale endpoint
/// geometry cannot accidentally pass the rendering and picking checks.
fn bridge() -> (Document, [u64; 5]) {
    let mut doc = Document::default();
    let ids = [(-60., 0.), (0., 0.), (20., 25.), (40., 20.), (80., 0.)]
        .map(|(x, y)| doc.add_atom("C", Point::new(x, y)));
    // Deliberately insert the right boundary first: ordering follows geometry.
    for [a, b] in [
        [ids[3], ids[4]],
        [ids[2], ids[3]],
        [ids[1], ids[2]],
        [ids[0], ids[1]],
    ] {
        doc.add_bond(a, b, 1, "plain");
    }
    (doc, ids)
}

#[test]
fn custom_two_sided_contraction_validates_boundaries_and_versions() {
    let (mut doc, [left, anchor, middle, exit, right]) = bridge();
    doc.contract(&[anchor, middle, exit], "Bridge", "").unwrap();
    let group = &doc.abbreviations[0];
    assert_eq!((group.anchor, group.exit), (anchor, Some(exit)));
    assert_eq!(doc.version, crate::document::RESIDUE_GROUPS);
    assert_eq!(doc.file_version(), crate::document::RESIDUE_GROUPS);
    assert!(doc.atom_visible(anchor));
    assert!(!doc.atom_visible(exit));
    assert!(doc.bond_visible(left, anchor));
    assert!(doc.bond_visible(exit, right));
    assert!(!doc.bond_visible(anchor, middle));
    assert!(!doc.bond_visible(middle, exit));
    doc.validate().unwrap();
    let saved = doc.file_json().unwrap();
    assert_eq!(Document::from_json(&saved).unwrap(), doc.current());

    for invalid in [Some(anchor), Some(right), None] {
        let mut bad = doc.clone();
        bad.abbreviations[0].exit = invalid;
        assert!(bad.validate().is_err(), "{invalid:?}");
    }
    let mut old = doc.clone();
    old.version = crate::document::MODEL_POLISH;
    assert!(old.validate().unwrap_err().contains("version 34"));
    let mut disconnected = doc.clone();
    disconnected.bonds.remove(1);
    assert!(disconnected.validate().unwrap_err().contains("connected"));
    let mut third_boundary = doc.clone();
    let outside = third_boundary.add_atom("C", Point::new(20., 80.));
    third_boundary.add_bond(middle, outside, 1, "plain");
    assert!(third_boundary.validate().is_err());
    third_boundary.abbreviations.clear();
    assert_eq!(
        third_boundary.contract(&[anchor, middle, exit], "Bridge", ""),
        Err("A group connects through at most two atoms".into())
    );
}

#[test]
fn two_sided_group_moves_copies_deletes_and_expands_as_one_fragment() {
    let (mut doc, [left, anchor, middle, exit, right]) = bridge();
    let expanded = doc.clone();
    doc.contract(&[anchor, middle, exit], "Bridge", "").unwrap();
    let contracted = doc.clone();
    doc.translate(&[exit], 8., -12.);
    for id in [anchor, middle, exit] {
        assert_eq!(
            doc.atom(id).unwrap().position,
            expanded.atom(id).unwrap().position.offset(8., -12.)
        );
    }
    for id in [left, right] {
        assert_eq!(doc.atom(id), expanded.atom(id));
    }
    doc.reconcile_abbreviations(&contracted);
    assert_eq!(doc.abbreviations, contracted.abbreviations);
    let moved = doc.clone();
    let mut history = crate::document::History::default();
    assert!(history.commit(contracted.clone(), &doc));
    assert!(history.undo(&mut doc));
    assert_eq!(doc, contracted);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, moved);

    let part = crate::editing::selection(&doc, &[anchor]);
    assert_eq!(part.atoms.len(), 3);
    assert_eq!(part.abbreviations, doc.abbreviations);
    let ids = crate::editing::append(&mut doc, &part, Point::new(150., 0.));
    assert_eq!(ids.len(), 3);
    let copied = &doc.abbreviations[1];
    assert_eq!((copied.anchor, copied.exit), (ids[0], Some(ids[2])));
    doc.validate().unwrap();
    let mut deleted = doc.clone();
    deleted.delete(&[exit]);
    assert!(
        [anchor, middle, exit]
            .into_iter()
            .all(|id| deleted.atom(id).is_none())
    );
    assert_eq!(deleted.abbreviations.len(), 1);
    assert_eq!(doc.expand_abbreviations(&[exit]), 1);
    assert_eq!(doc.file_version(), crate::document::RESIDUE_GROUPS);
    assert_eq!(doc.expand_abbreviations(&ids), 1);
    assert_eq!(doc.file_version(), crate::document::DISTANCE_BONDS);
    assert!(
        [anchor, middle, exit]
            .into_iter()
            .all(|id| doc.atom_visible(id))
    );
}

#[test]
fn two_sided_bonds_render_and_pick_at_both_label_edges() {
    let (mut doc, [left, anchor, middle, exit, right]) = bridge();
    doc.contract(&[anchor, middle, exit], "Ala", "Back")
        .unwrap();
    let (lo, hi) = crate::scene::atom_label_bounds(doc.atom(anchor).unwrap(), &doc).unwrap();
    let (ink_lo, ink_hi) =
        crate::scene::atom_label_ink_bounds(doc.atom(anchor).unwrap(), &doc).unwrap();
    assert!(((lo.x + hi.x) / 2.).abs() < 0.001);
    let primitives = crate::scene::primitives(&doc);
    assert!(primitives.iter().any(
        |primitive| matches!(primitive, crate::scene::Primitive::Text { text, .. } if text == "Ala")
    ));
    let paints_bond = |point: Point| {
        let contains = |polygon: &[Point]| {
            crate::polygon::closed_edges(polygon)
                .filter(|(a, b)| {
                    (a.y > point.y) != (b.y > point.y)
                        && point.x < a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y)
                })
                .count()
                % 2
                == 1
        };
        primitives.iter().any(|primitive| match primitive {
            crate::scene::Primitive::Line(a, b, width) => {
                crate::graphics::segment_distance(point, *a, *b) < width / 2.
            }
            crate::scene::Primitive::Polygon(points) => contains(points),
            crate::scene::Primitive::Path {
                commands,
                filled: true,
                ..
            } => crate::graphics::flattened(commands)
                .iter()
                .any(|points| contains(points)),
            _ => false,
        })
    };
    let drawn = doc.abbreviation_render_copy();
    let segments: Vec<_> = drawn
        .bonds
        .iter()
        .filter_map(|bond| crate::scene::bond_segment(&drawn, bond, None))
        .collect();
    assert_eq!(segments.len(), 2);
    for (a, b) in &segments {
        assert!(a.x.max(b.x) < ink_lo.x || a.x.min(b.x) > ink_hi.x);
    }
    assert_eq!((drawn.bonds[0].a, drawn.bonds[0].b), (anchor, right));
    assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (exit, right));
    for (index, point, endpoints) in [
        (3, Point::new(-45., 0.), (left, anchor)),
        (0, Point::new(65., 0.), (exit, right)),
    ] {
        assert_eq!(crate::editing::nearest_bond(&doc, point, 0.25), Some(index));
        let pick = crate::arrow_anchors::pick(&doc, point, 0.25).unwrap();
        assert!(
            matches!(pick, crate::arrow_anchors::Pick::Bond { a, b, .. } if (a,b) == endpoints)
        );
        assert!(pick.center(&doc).unwrap().distance(point) < 0.001);
        let (a, b) = crate::scene::bond_segment(&drawn, &drawn.bonds[index], None).unwrap();
        assert!(a.y.abs() < 0.001 && b.y.abs() < 0.001);
        assert!(paints_bond(point));
    }
    assert!(!paints_bond(Point::default()));
    assert_eq!(
        crate::editing::nearest_bond(&doc, Point::default(), 0.25),
        None
    );
    assert_eq!(
        crate::editing::nearest_bond(&doc, Point::new(60., 10.), 0.25),
        None
    );
    assert_eq!(doc.nearest(Point::new(lo.x + 1., 0.), 0.25), Some(anchor));
    assert!(crate::scene::bond_segment(&drawn, &drawn.bonds[1], None).is_none());
    let right_span = crate::scene::bond_segment(&drawn, &drawn.bonds[0], None).unwrap();
    assert!(right_span.0.x > hi.x - 1.);
    let left_span = crate::scene::bond_segment(&drawn, &drawn.bonds[3], None).unwrap();
    assert!(left_span.1.x < lo.x + 1.);
}

#[test]
fn two_sided_bond_erasing_uses_drawn_exit_geometry() {
    let (mut doc, [_, anchor, middle, exit, _]) = bridge();
    doc.contract(&[anchor, middle, exit], "Bridge", "").unwrap();
    let before = doc.clone();
    let hidden = Point::new(60., 10.);
    crate::erasing::stroke(&mut doc, hidden, hidden, 0.25);
    assert_eq!(doc, before);
    let visible = Point::new(65., 0.);
    crate::erasing::stroke(&mut doc, visible, visible, 0.25);
    assert_eq!(doc.bonds, before.bonds[1..]);
}

#[test]
fn two_sided_bond_indicators_keep_their_chemical_owner() {
    let (mut doc, [_, anchor, middle, exit, right]) = bridge();
    doc.bonds[0].order = 2;
    doc.bonds[0].cip_label = Some("E".into());
    doc.bonds[0].indicator.show = Some(true);
    doc.contract(&[anchor, middle, exit], "Bridge", "").unwrap();
    let indicator = crate::atom_labels::indicators(&doc).remove(0);
    assert_eq!(
        indicator.owner,
        crate::atom_labels::Owner::BondStereo(exit, right)
    );
    let offset = Some(Point::new(4., 8.));
    indicator.owner.set_offset(&mut doc, offset);
    assert_eq!(doc.bonds[0].indicator.offset, offset);
}

#[test]
fn two_sided_reverse_spelling_and_label_bounds_match_the_render_view() {
    let (mut doc, [_, anchor, middle, exit, _]) = bridge();
    doc.contract(&[anchor, middle, exit], "Forward", "Reverse")
        .unwrap();
    for alignment in LabelAlignment::ALL {
        doc.abbreviations[0].alignment = alignment;
        let drawn = doc.abbreviation_render_copy();
        let expected = if alignment == LabelAlignment::Right {
            "Reverse"
        } else {
            "Forward"
        };
        assert_eq!(doc.abbreviations[0].text(&doc), expected);
        assert_eq!(drawn.abbreviations[0].text(&drawn), expected);
        assert_eq!(
            crate::scene::atom_label_bounds(doc.atom(anchor).unwrap(), &doc),
            crate::scene::atom_label_bounds(drawn.atom(anchor).unwrap(), &drawn)
        );
    }
}

#[test]
fn label_bounds_and_hits_use_drawn_exit_geometry_for_outside_hydrogens() {
    let (mut doc, [_, anchor, middle, exit, right]) = bridge();
    doc.atom_mut(exit).unwrap().position.y = -20.;
    doc.atom_mut(right).unwrap().element = "N".into();
    let other = doc.add_atom("C", Point::new(120., 0.));
    doc.add_bond(right, other, 1, "plain");
    doc.contract(&[anchor, middle, exit], "Ala", "").unwrap();
    doc.atom_mut(right).unwrap().label_h = 1;
    let drawn = doc.abbreviation_render_copy();
    let boxes = crate::scene::atom_label_ink_boxes(drawn.atom(right).unwrap(), &drawn);
    assert!(boxes.len() >= 2);
    let ink = crate::scene::attachment_ink::AttachmentInk::new(&doc);
    for (lo, hi) in boxes {
        let point = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
        assert_eq!(crate::scene::atom_label_hit(&doc, point, 0.), Some(right));
        assert_eq!(ink.label_hit(point, 0.), Some(right));
    }
}

#[test]
fn reconcile_keeps_moved_groups_and_drops_only_chemically_edited_ones() {
    let before = shortcut_examples();
    let groups = before.abbreviations.clone();
    assert!(groups.len() > 2);

    let mut moved = before.clone();
    for atom in &mut moved.atoms {
        atom.position = atom.position.offset(5., -3.);
    }
    moved.reconcile_abbreviations(&before);
    assert_eq!(moved.abbreviations, groups);

    // A bond order change inside one group reveals that group only.
    let inside = |g: &Abbreviation, b: &crate::document::Bond| {
        g.members.contains(&b.a) && g.members.contains(&b.b)
    };
    let edited = groups
        .iter()
        .position(|g| before.bonds.iter().any(|b| inside(g, b)))
        .unwrap();
    let mut doc = before.clone();
    let bond = doc
        .bonds
        .iter_mut()
        .find(|b| inside(&groups[edited], b))
        .unwrap();
    bond.order = if bond.order == 1 { 2 } else { 1 };
    doc.reconcile_abbreviations(&before);
    let mut expected = groups.clone();
    expected.remove(edited);
    assert_eq!(doc.abbreviations, expected);

    // An element change on a member reveals its group only.
    let mut doc = before.clone();
    let last = groups.len() - 1;
    let member = groups[last].members[0];
    doc.atom_mut(member).unwrap().element = "Si".into();
    doc.reconcile_abbreviations(&before);
    assert_eq!(doc.abbreviations, groups[..last]);
}

/// Import only the chemical graph, keeping tests independent of depiction.
/// The first SMILES atom is outside the contracted fragment.
fn contract_fragment(smiles: &str) -> (Document, Vec<u64>) {
    let prepared = crate::chemistry::smiles::prepare(smiles).unwrap();
    let mut doc = Document::default();
    let ids: Vec<_> = prepared
        .state
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| {
            let symbol = crate::chemistry::ELEMENTS[usize::from(atom.atomic_number)].symbol;
            let id = doc.add_atom(symbol, Point::new(index as f32 * 42., 0.));
            let drawing = doc.atom_mut(id).unwrap();
            drawing.charge = i32::from(atom.charge);
            drawing.explicit_h = u32::from(atom.explicit_hydrogens);
            drawing.aromatic = atom.aromatic;
            id
        })
        .collect();
    for bond in &prepared.state.graph.bonds {
        doc.add_bond(ids[bond.a], ids[bond.b], bond.order, "plain");
    }
    (doc, ids[1..].to_vec())
}

#[test]
fn contract_label_warnings_check_both_spellings_and_element_exceptions() {
    let (doc, members) = contract_fragment("NCC");
    let before = doc.clone();
    let warnings = doc.contract_label_warnings(&members, " Custom ", " Co ");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Co is an element symbol"));
    assert_eq!(doc.contract_label_warnings(&members, "Co", "Co"), warnings);
    assert!(
        doc.contract_label_warnings(&members, "CustomCo", "co")
            .is_empty()
    );
    for label in ["Ac", "Cm", "Nb", "Np", "Pr", "Ts"] {
        assert!(
            doc.contract_label_warnings(&members, label, "")
                .iter()
                .all(|warning| !warning.contains("element symbol")),
            "{label}"
        );
    }
    assert_eq!(doc, before);
    let mut contracted = doc;
    contracted.contract(&members, "Co", "").unwrap();
    assert_eq!(contracted.abbreviations[0].label, "Co");
}

#[test]
fn contract_label_warnings_compare_canonical_bundled_structures() {
    let (doc, members) = contract_fragment("NCC");
    let warnings = doc.contract_label_warnings(&members, "Boc", "");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Boc means tert-butoxycarbonyl"));

    let (mut doc, mut members) = contract_fragment("NC(OC(C)(C)C)=O");
    // Both SMILES traversal and native atom order differ from the catalog.
    doc.atoms.reverse();
    doc.bonds.reverse();
    assert!(doc.contract_label_warnings(&members, "Boc", "").is_empty());
    doc.atom_mut(members[0]).unwrap().map_num = 17;
    assert!(doc.contract_label_warnings(&members, "Boc", "").is_empty());
    let carbon = members[3];
    let hydrogen = doc.add_atom("H", Point::new(200., 42.));
    doc.add_bond(carbon, hydrogen, 1, "plain");
    members.push(hydrogen);
    assert!(doc.contract_label_warnings(&members, "Boc", "").is_empty());
    doc.atom_mut(carbon).unwrap().isotope = 13;
    assert_eq!(doc.contract_label_warnings(&members, "Boc", "").len(), 1);
}

#[test]
fn contract_label_warnings_use_bundled_reverse_spellings_and_ignore_user_overrides() {
    let errors = crate::chemistry::abbreviations::set_user_on_thread(
        r#"{"format":1,"groups":[{"label":"Boc","smiles":"*CC"}]}"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
    let (doc, members) = contract_fragment("NCC");
    assert_eq!(doc.contract_label_warnings(&members, "Boc", "").len(), 1);
    let (doc, members) = contract_fragment("NOC");
    assert!(
        doc.contract_label_warnings(&members, "Custom", "MeO")
            .is_empty()
    );
    let (doc, members) = contract_fragment("NCC");
    assert_eq!(
        doc.contract_label_warnings(&members, "Custom", "MeO").len(),
        1
    );
    let errors = crate::chemistry::abbreviations::set_user_on_thread(r#"{"format":1,"groups":[]}"#);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn contract_label_warnings_explain_bz_and_suggest_bn_for_benzyl() {
    let (doc, members) = contract_fragment("NCc1ccccc1");
    let warnings = doc.contract_label_warnings(&members, "Bz", "");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Bz means benzoyl"));
    assert!(warnings[0].contains("use Bn for benzyl"));
    assert!(doc.contract_label_warnings(&members, "Bn", "").is_empty());
    let (doc, members) = contract_fragment("NC(=O)c1ccccc1");
    assert!(doc.contract_label_warnings(&members, "Bz", "").is_empty());
}
