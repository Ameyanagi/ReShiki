use super::*;
use crate::{depth_appearance as depth, editing, transaction};

fn chain() -> Document {
    let mut doc = Document::default();
    for (i, z) in [-30., -10., 10., 30.].into_iter().enumerate() {
        let id = doc.add_atom("C", Point::new(i as f32 * 42., 0.));
        doc.atom_mut(id).unwrap().depth = z;
        if i > 0 {
            doc.add_bond(id - 1, id, 1, "plain");
        }
    }
    doc
}

#[test]
fn alpha_is_component_local_positive_z_front_bounded_and_flat_unchanged() {
    let mut doc = chain();
    let ids = doc.all_ids();
    let original = doc.clone();
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
    assert!(!transaction::chemistry_changed(&original, &doc));
    let paint = Paint::new(&doc);
    assert_eq!(
        ids.iter().map(|id| paint.atom(*id)).collect::<Vec<_>>(),
        [0.25, 0.25, 1., 1.]
    );
    let bond = &doc.bonds[1];
    assert_eq!(paint.bond(bond, -100.), 0.25);
    assert_eq!(paint.bond(bond, 100.), 1.);
    assert_eq!(paint.bond(bond, 0.5), 1.);
    for atom in &mut doc.atoms {
        atom.depth += 1000.;
    }
    let separate = doc.add_atom("O", Point::new(500., 0.));
    doc.atom_mut(separate).unwrap().depth = -10000.;
    depth::set_rear_opacity(&mut doc, &[separate], 0.).unwrap();
    let shifted = Paint::new(&doc);
    for id in &ids {
        assert_eq!(paint.atom(*id), shifted.atom(*id));
    }
    assert_eq!(shifted.atom(separate), 1.);
    for atom in &mut doc.atoms {
        atom.depth = 8.;
    }
    let flat = Paint::new(&doc);
    assert!(doc.all_ids().iter().all(|id| flat.atom(*id) == 1.));
    let mut clear = doc.clone();
    clear.depth_appearance.clear();
    assert_eq!(crate::scene::svg(&doc), crate::scene::svg(&clear));
}

#[test]
fn opacity_persists_freezes_copies_and_is_one_display_only_history_edit() {
    let mut doc = chain();
    let ids = doc.all_ids();
    let before = doc.clone();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    let fade = doc.depth_appearance[0].strength;
    depth::set_rear_opacity(&mut doc, &ids, 0.4).unwrap();
    assert_eq!(doc.depth_appearance[0].strength, fade);
    let edited = doc.clone();
    let mut history = crate::document::History::default();
    assert!(history.commit(before.clone(), &doc));
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, edited);
    assert!(!history.can_redo());
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.depth_appearance, doc.depth_appearance);
    assert_eq!(reopened.atoms, doc.atoms);
    assert_eq!(reopened.bonds, doc.bonds);
    let partial = editing::selection(&doc, &[ids[0], ids[1]]);
    assert!(!partial.depth_appearance[0].automatic);
    assert_eq!(Paint::new(&partial).atom(ids[0]), 0.4);
    let mut target = chain();
    let original_target = target.clone();
    let pasted = editing::append(&mut target, &partial, Point::new(400., 0.));
    assert_eq!(Paint::new(&target).atom(pasted[0]), 0.4);
    assert!(target.version >= 22);
    for atom in &original_target.atoms {
        assert_eq!(target.atom(atom.id), Some(atom));
    }
    let mut frozen = doc.clone();
    depth::freeze(&mut frozen, &ids);
    let svg = crate::scene::svg(&frozen);
    for atom in &mut frozen.atoms {
        atom.depth = -atom.depth;
    }
    // Classification remains frozen even if retained coordinates change.
    assert_eq!(Paint::new(&frozen).atom(ids[0]), 0.4);
    assert_eq!(crate::scene::svg(&frozen), svg);
    let materialized = depth::materialize(&doc);
    assert_eq!(Paint::new(materialized.as_ref()).atom(ids[0]), 0.4);
    assert_eq!(
        depth::materialize(materialized.as_ref()).as_ref(),
        materialized.as_ref()
    );
}

#[test]
fn defaults_legacy_roundtrip_and_invalid_values_are_checked_atomically() {
    let mut doc = chain();
    let ids = doc.all_ids();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    let value = serde_json::to_value(&doc).unwrap();
    assert!(value["depth_appearance"][0].get("rear_opacity").is_none());
    let legacy: Document = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.depth_appearance[0].rear_opacity, 1.);
    for alpha in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        let before = doc.clone();
        assert!(depth::set_rear_opacity(&mut doc, &ids, alpha).is_err());
        assert_eq!(doc, before);
        let mut invalid = doc.clone();
        invalid.depth_appearance[0].rear_opacity = alpha;
        assert!(invalid.validate().is_err());
    }
    let opaque = crate::scene::svg(&doc);
    depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
    assert_eq!(crate::scene::svg(&doc), opaque);
}

#[test]
fn crossing_cut_depends_on_visible_over_ink_including_explicit_layer_override() {
    let mut doc = Document::default();
    let ids: Vec<_> = [
        (-42., 0., -10.),
        (42., 0., -10.),
        (0., -42., 10.),
        (0., 42., 10.),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[2], ids[3], 1, "plain");
    // A connection makes one component's front/rear range without crossing.
    doc.add_bond(ids[0], ids[2], 1, "plain");
    doc.bonds[0].z_order = 1;
    assert!(!crate::crossings::gaps(&doc)[1].is_empty());
    for alpha in [0., 0.5] {
        depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        assert!(
            crate::crossings::gaps(&doc)[1].is_empty(),
            "Rear ink cut foreground at alpha {alpha}"
        );
    }
    depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
    assert!(!crate::crossings::gaps(&doc)[1].is_empty());
}

#[test]
fn mixed_bond_clips_visible_ink_at_midplane_without_moving_either_endpoint() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(100., 0.));
    doc.atom_mut(a).unwrap().depth = -20.;
    doc.atom_mut(b).unwrap().depth = 20.;
    doc.add_bond(a, b, 2, "plain");
    let original = doc.clone();
    depth::set_rear_opacity(&mut doc, &[a, b], 0.).unwrap();
    let paint = Paint::new(&doc);
    let clipped = paint.bond_parts(
        &doc,
        &doc.bonds[0],
        vec![Primitive::Line(
            Point::new(-10., 0.),
            Point::new(110., 0.),
            2.,
        )],
    );
    let vertices: Vec<_> = clipped.iter().flat_map(points).collect();
    assert!(vertices.iter().all(|p| p.x >= 49.999));
    assert!(vertices.iter().any(|p| p.x > 110.9)); // existing round endpoint retained
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
    depth::set_rear_opacity(&mut doc, &[a, b], 0.5).unwrap();
    let clipped = Paint::new(&doc).bond_parts(
        &doc,
        &doc.bonds[0],
        vec![Primitive::Line(
            Point::new(-10., 0.),
            Point::new(110., 0.),
            2.,
        )],
    );
    assert!(
        clipped
            .iter()
            .any(|p| matches!(p,Primitive::Opacity{alpha,..} if *alpha==0.5))
    );
}

#[test]
fn rear_owned_labels_marks_numbers_highlights_and_front_owned_labels_follow_alpha() {
    let mut doc = chain();
    let ids = doc.all_ids();
    for id in [ids[0], ids[3]] {
        let a = doc.atom_mut(id).unwrap();
        a.element = "O".into();
        a.label_h = 1;
        a.display.number = Some(crate::atom_labels::Number {
            text: format!("atom{id}"),
            offset: Some(Point::new(0., 18.)),
            style: crate::atom_labels::number_style(),
        });
        a.display.highlight = Some(crate::palette::Color::Custom([255, 200, 50]));
        crate::scientific::attach(
            a,
            crate::scientific::SymbolKind::LonePair,
            Point::new(0., -18.),
        )
        .unwrap();
    }
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    let primitives = crate::scene::primitives(&doc);
    assert!(primitives.iter().any(|p|matches!(p,Primitive::Opacity{alpha,primitive} if *alpha==0.25 && matches!(primitive.as_ref(),Primitive::Text{text,..} if text=="O"))));
    assert!(primitives.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Text{text,..} if text=="atom1"))));
    assert!(
        primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..} if text=="atom4"))
    );
    assert!(primitives.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Path{style,filled:true,..} if style.fill==Some(crate::palette::Color::Custom([255,200,50]))))));
    let original = doc.clone();
    depth::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let svg = crate::scene::svg(&doc);
    assert!(!svg.contains("atom1"));
    assert!(svg.contains("atom4"));
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
}

#[test]
fn tilted_aromatic_curve_fill_and_arcs_share_one_depth_plane_and_default_geometry() {
    let mut doc = Document::default();
    let ids = editing::ring(&mut doc, Point::default(), 6, true, 0.);
    crate::projection::tilt(&mut doc, &ids, 65., true);
    crate::projection::tilt(&mut doc, &ids, 20., false);
    crate::ring_fills::apply(
        &mut doc,
        &ids,
        Some(crate::palette::Color::Custom([200, 220, 255])),
    );
    let original = doc.clone();
    let svg = crate::scene::svg(&doc);
    depth::set_rear_opacity(&mut doc, &ids, 0.5).unwrap();
    let painted = crate::scene::primitives(&doc);
    assert!(painted.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Path{filled:true,style,..} if style.fill==Some(crate::palette::Color::Ink)))));
    assert!(painted.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Path{filled:true,style,..} if style.fill==Some(crate::palette::Color::Custom([200,220,255]))))));
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
    for bond in &mut doc.bonds {
        bond.ring_arc = true;
    }
    let arcs = crate::scene::primitives(&doc);
    assert!(arcs.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Path{filled:true,style,..} if style.fill==Some(crate::palette::Color::Ink)))));
    doc.bonds = original.bonds.clone();
    depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
    assert_eq!(crate::scene::svg(&doc), svg);
}

#[test]
fn new_coordinated_molecules_have_independent_alpha_and_rgb_toggle_keeps_ownership() {
    let mut doc = chain();
    let first = doc.all_ids();
    let other = chain();
    let second = editing::append(&mut doc, &other, Point::new(400., 0.));
    doc.add_bond(first[3], second[0], 5, "plain");
    let all = doc.all_ids();
    depth::set_rear_opacity(&mut doc, &all, 0.5).unwrap();
    assert_eq!(doc.depth_appearance.len(), 2);
    depth::set_rear_opacity(&mut doc, &first, 0.25).unwrap();
    assert_eq!(Paint::new(&doc).atom(first[0]), 0.25);
    assert_eq!(Paint::new(&doc).atom(second[0]), 0.5);
    let scopes: Vec<_> = doc
        .depth_appearance
        .iter()
        .map(|s| (s.atoms.clone(), s.rear_opacity))
        .collect();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    assert_eq!(
        doc.depth_appearance
            .iter()
            .map(|s| (s.atoms.clone(), s.rear_opacity))
            .collect::<Vec<_>>(),
        scopes
    );
    assert_eq!(Paint::new(&doc).atom(first[0]), 0.25);
    assert_eq!(Paint::new(&doc).atom(second[0]), 0.5);
    let mut legacy = chain();
    let other = chain();
    let second = editing::append(&mut legacy, &other, Point::new(400., 0.));
    legacy.add_bond(4, second[0], 5, "plain");
    let all = legacy.all_ids();
    depth::enable(&mut legacy, &all, 0.6).unwrap();
    assert_eq!(
        legacy.depth_appearance.len(),
        1,
        "Legacy RGB grouping migrated"
    );
}
