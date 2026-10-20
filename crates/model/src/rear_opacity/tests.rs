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
fn exposed_chain_is_opaque_at_every_depth_and_flat_output_is_unchanged() {
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
        [1., 1., 1., 1.]
    );
    let bond = &doc.bonds[1];
    assert_eq!(paint.bond(bond, -100.), 1.);
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
    assert_eq!(depth::rear_opacity(&partial, &ids[..2]), Some(0.4));
    assert_eq!(Paint::new(&partial).atom(ids[0]), 1.);
    let mut target = chain();
    let original_target = target.clone();
    let pasted = editing::append(&mut target, &partial, Point::new(400., 0.));
    assert_eq!(depth::rear_opacity(&target, &pasted), Some(0.4));
    assert_eq!(Paint::new(&target).atom(pasted[0]), 1.);
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
    // RGB remains frozen; exposed chain ink stays solid after a view change.
    assert_eq!(Paint::new(&frozen).atom(ids[0]), 1.);
    assert_eq!(crate::scene::svg(&frozen), svg);
    let materialized = depth::materialize(&doc);
    assert_eq!(Paint::new(materialized.as_ref()).atom(ids[0]), 1.);
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
fn exposed_bond_crossing_the_midplane_keeps_full_stroke_and_end_caps() {
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
    assert!(
        matches!(clipped.as_slice(),[Primitive::Line(a,b,w)] if *a==Point::new(-10.,0.)&&*b==Point::new(110.,0.)&&*w==2.)
    );
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
        matches!(clipped.as_slice(),[Primitive::Line(a,b,w)] if *a==Point::new(-10.,0.)&&*b==Point::new(110.,0.)&&*w==2.)
    );
}

#[test]
fn rear_owned_labels_marks_numbers_highlights_and_front_owned_labels_follow_alpha() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    let paint = Paint::new(&doc);
    let hidden = ids
        .iter()
        .copied()
        .find(|id| paint.atom(*id) == 0.25)
        .unwrap();
    let exposed = ids
        .iter()
        .copied()
        .find(|id| paint.atom(*id) == 1.)
        .unwrap();
    for (id, text) in [(hidden, "hidden"), (exposed, "exposed")] {
        let a = doc.atom_mut(id).unwrap();
        a.element = "O".into();
        a.label_h = 1;
        a.display.number = Some(crate::atom_labels::Number {
            text: text.into(),
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
    let primitives = crate::scene::primitives(&doc);
    assert!(primitives.iter().any(|p|matches!(p,Primitive::Opacity{alpha,primitive} if *alpha==0.25 && matches!(primitive.as_ref(),Primitive::Text{text,..} if text=="O"))));
    assert!(primitives.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Text{text,..} if text=="hidden"))));
    assert!(
        primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..} if text=="exposed"))
    );
    assert!(primitives.iter().any(|p|matches!(p,Primitive::Opacity{primitive,..} if matches!(primitive.as_ref(),Primitive::Path{style,filled:true,..} if style.fill==Some(crate::palette::Color::Custom([255,200,50]))))));
    let original = doc.clone();
    depth::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let svg = crate::scene::svg(&doc);
    assert!(!svg.contains("hidden"));
    assert!(svg.contains("exposed"));
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
}

#[test]
fn isolated_tilted_aromatic_fill_and_arcs_do_not_acquire_an_occluding_surface() {
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
    assert_eq!(
        crate::scene::svg(&doc),
        svg,
        "Exposed aromatic circle and ring fill remain opaque"
    );
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
    for bond in &mut doc.bonds {
        bond.ring_arc = true;
    }
    let mut opaque_arcs = doc.clone();
    opaque_arcs.depth_appearance.clear();
    assert_eq!(crate::scene::svg(&doc), crate::scene::svg(&opaque_arcs));
    doc.bonds = original.bonds.clone();
    depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
    assert_eq!(crate::scene::svg(&doc), svg);
}

fn coordinated_chains() -> (Document, Vec<u64>, Vec<u64>) {
    let mut doc = chain();
    let first = doc.all_ids();
    let mut other = chain();
    for atom in &mut other.atoms {
        atom.depth += 130.;
    }
    let second = editing::append(&mut doc, &other, Point::new(400., 0.));
    doc.add_bond(first[3], second[0], 5, "plain");
    (doc, first, second)
}

#[test]
fn rgb_first_partial_alpha_keeps_other_molecule_and_live_rgb_basis_native_and_history() {
    let (mut doc, first, second) = coordinated_chains();
    let all = doc.all_ids();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    assert_eq!(doc.depth_appearance.len(), 1, "legacy coordinate RGB group");
    let original = doc.clone();
    let original_svg = crate::scene::svg(&doc);
    let rgb = depth::Paint::new(&doc);
    // A single selected atom still targets its complete covalent molecule.
    depth::set_rear_opacity(&mut doc, &[first[0]], 0.25).unwrap();
    assert_eq!(depth::rear_opacity(&doc, &first), Some(0.25));
    assert_eq!(depth::rear_opacity(&doc, &second), Some(1.));
    assert_eq!(Paint::new(&doc).atom(first[0]), 1.);
    assert_eq!(Paint::new(&doc).atom(first[3]), 1.);
    assert!(second.iter().all(|id| Paint::new(&doc).atom(*id) == 1.));
    assert_eq!(
        doc.depth_appearance[0].atoms,
        original.depth_appearance[0].atoms
    );
    assert_eq!(
        doc.depth_appearance[0].weights,
        original.depth_appearance[0].weights
    );
    assert!(doc.depth_appearance[0].automatic);
    for id in &all {
        assert_eq!(depth::Paint::new(&doc).amount(*id), rgb.amount(*id));
    }
    let mut nonpaint = doc.clone();
    nonpaint.version = original.version;
    nonpaint.depth_appearance = original.depth_appearance.clone();
    assert_eq!(nonpaint, original, "all graph, XYZ and other native fields");
    assert!(!transaction::chemistry_changed(&original, &doc));
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.depth_appearance, doc.depth_appearance);
    assert_eq!(depth::rear_opacity(&reopened, &second), Some(1.));
    let mut history = crate::document::History::default();
    let edited = doc.clone();
    assert!(history.commit(original.clone(), &doc));
    assert!(history.undo(&mut doc));
    assert_eq!(doc, original);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, edited);
    depth::set_rear_opacity(&mut doc, &first, 1.).unwrap();
    assert_eq!(
        crate::scene::svg(&doc),
        original_svg,
        "RGB basis unchanged after restoring opaque alpha"
    );
}

#[test]
fn alpha_first_and_rgb_first_toggles_keep_mixed_settings_and_legacy_rgb_normalization() {
    for rgb_first in [false, true] {
        let (mut doc, first, second) = coordinated_chains();
        let all = doc.all_ids();
        if rgb_first {
            depth::enable(&mut doc, &all, 0.6).unwrap();
        }
        depth::set_rear_opacity(&mut doc, &first, 0.25).unwrap();
        depth::set_rear_opacity(&mut doc, &second, 0.5).unwrap();
        for _ in 0..2 {
            depth::enable(&mut doc, &all, 0.6).unwrap();
            assert_eq!(
                doc.depth_appearance.len(),
                1,
                "keep legacy RGB connected grouping"
            );
            assert_eq!(depth::rear_opacity(&doc, &first), Some(0.25));
            assert_eq!(depth::rear_opacity(&doc, &second), Some(0.5));
            let mut rgb_only = doc.clone();
            rgb_only.depth_appearance.clear();
            depth::enable(&mut rgb_only, &all, 0.6).unwrap();
            for id in &all {
                assert_eq!(
                    depth::Paint::new(&doc).amount(*id),
                    depth::Paint::new(&rgb_only).amount(*id)
                );
            }
            let rear = Paint::new(&doc);
            depth::freeze(&mut doc, &all);
            crate::projection::tilt(&mut doc, &all, 20., true);
            for id in &all {
                assert_eq!(
                    Paint::new(&doc).atom(*id),
                    rear.atom(*id),
                    "exposed chain visibility after changing the view"
                );
            }
        }
        // The next RGB enable is live after tilt, without alpha merging.
        depth::enable(&mut doc, &all, 0.6).unwrap();
        assert_eq!(depth::rear_opacity(&doc, &first), Some(0.25));
        assert_eq!(depth::rear_opacity(&doc, &second), Some(0.5));
        for members in [&first, &second] {
            let isolated = editing::selection(&doc, members);
            for id in members {
                assert_eq!(Paint::new(&doc).atom(*id), Paint::new(&isolated).atom(*id));
            }
        }
        doc.validate().unwrap();
    }
}

#[test]
fn mixed_rear_settings_freeze_copy_remap_prune_and_validate_independently_of_rgb() {
    let (mut doc, first, second) = coordinated_chains();
    let all = doc.all_ids();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    depth::freeze(&mut doc, &all);
    let frozen_rgb = doc.depth_appearance[0].weights.clone();
    depth::set_rear_opacity(&mut doc, &first, 0.25).unwrap();
    assert_eq!(doc.depth_appearance[0].weights, frozen_rgb);
    assert!(!doc.depth_appearance[0].automatic);
    let partial = editing::selection(&doc, &first[..2]);
    assert_eq!(depth::rear_opacity(&partial, &first[..2]), Some(0.25));
    assert!(
        first[..2]
            .iter()
            .all(|id| Paint::new(&partial).atom(*id) == 1.)
    );
    for id in &first[..2] {
        assert_eq!(
            depth::Paint::new(&partial).amount(*id),
            depth::Paint::new(&doc).amount(*id)
        );
    }
    let mut target = chain();
    let pasted = editing::append(&mut target, &partial, Point::new(500., 0.));
    assert_eq!(depth::rear_opacity(&target, &pasted), Some(0.25));
    assert!(pasted.iter().all(|id| Paint::new(&target).atom(*id) == 1.));
    target.delete(&pasted[..1]);
    target.validate().unwrap();
    assert_eq!(Paint::new(&target).atom(pasted[1]), 1.);
    depth::clear(&mut doc, &first);
    assert_eq!(depth::rear_opacity(&doc, &first), Some(1.));
    assert_eq!(depth::rear_opacity(&doc, &second), Some(1.));
    doc.validate().unwrap();
    for map in ["rear_overrides", "rear_weights"] {
        let mut invalid = serde_json::to_value(&partial).unwrap();
        invalid["depth_appearance"][0][map]["999999"] = serde_json::json!(0.5);
        let invalid: Document = serde_json::from_value(invalid).unwrap();
        assert!(invalid.validate().is_err(), "foreign {map} owner");
    }
    let mut invalid = partial.clone();
    invalid.depth_appearance[0]
        .rear_overrides
        .insert(first[0], f32::NAN);
    assert!(invalid.validate().is_err());
}

#[test]
fn earlier_frozen_native_weights_and_alpha_settings_are_retained_but_not_visibility_caches() {
    let (mut doc, first, second) = coordinated_chains();
    let all = doc.all_ids();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    depth::freeze(&mut doc, &all);
    doc.depth_appearance[0].rear_opacity = 0.4;
    doc.version = 22;
    let weights = doc.depth_appearance[0].weights.clone();
    assert!(doc.depth_appearance[0].rear_weights.is_empty());
    for atom in &mut doc.atoms {
        atom.depth = -atom.depth;
    }
    let mut reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.depth_appearance[0].weights, weights);
    assert_eq!(depth::rear_opacity(&reopened, &all), Some(0.4));
    assert!(all.iter().all(|id| Paint::new(&reopened).atom(*id) == 1.));
    depth::set_rear_opacity(&mut reopened, &first, 0.25).unwrap();
    assert_eq!(depth::rear_opacity(&reopened, &first), Some(0.25));
    assert_eq!(depth::rear_opacity(&reopened, &second), Some(0.4));
    assert_eq!(reopened.depth_appearance[0].weights, weights);
    for id in &second {
        assert_eq!(Paint::new(&reopened).atom(*id), Paint::new(&doc).atom(*id));
    }
}

#[test]
fn quadruple_and_partial_covalent_bonds_are_one_alpha_molecule() {
    for order in [6, 7] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.atom_mut(a).unwrap().depth = -20.;
        doc.atom_mut(b).unwrap().depth = 20.;
        doc.add_bond(a, b, order, "plain");
        depth::set_rear_opacity(&mut doc, &[a], 0.25).unwrap();
        assert_eq!(depth::rear_opacity(&doc, &[a, b]), Some(0.25));
        assert_eq!(Paint::new(&doc).atom(a), 1.);
        assert_eq!(Paint::new(&doc).atom(b), 1.);
        assert_eq!(doc.depth_appearance.len(), 1);
    }
}

#[test]
fn copying_a_partial_molecule_retains_settings_and_frozen_rgb_without_inventing_a_shell() {
    let mut doc = chain();
    let ids = doc.all_ids();
    depth::enable(&mut doc, &ids, 0.6).unwrap();
    depth::clear(&mut doc, &ids[..2]);
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    // The new alpha owner contains both selected atoms, but their covalent
    // molecule also includes the other RGB owner. Copy must not renormalize.
    let partial = editing::selection(&doc, &ids[..2]);
    assert_eq!(partial.depth_appearance.len(), 1);
    assert!(!partial.depth_appearance[0].automatic);
    assert_eq!(depth::rear_opacity(&partial, &ids[..2]), Some(0.25));
    assert_eq!(Paint::new(&partial).atom(ids[0]), 1.);
    assert_eq!(Paint::new(&partial).atom(ids[1]), 1.);
    partial.validate().unwrap();
}

#[test]
fn flat_alpha_target_inside_shared_rgb_owner_does_not_activate_unpainted_rear_ink() {
    let (mut doc, first, second) = coordinated_chains();
    for id in &first {
        doc.atom_mut(*id).unwrap().depth = 8.;
    }
    let all = doc.all_ids();
    depth::enable(&mut doc, &all, 0.6).unwrap();
    assert_eq!(doc.depth_appearance.len(), 1);
    let before = crate::scene::svg(&doc);
    depth::set_rear_opacity(&mut doc, &first, 0.25).unwrap();
    assert_eq!(depth::rear_opacity(&doc, &first), Some(0.25));
    assert_eq!(depth::rear_opacity(&doc, &second), Some(1.));
    assert_eq!(
        crate::scene::svg(&doc),
        before,
        "unselected nonflat weights cannot activate alpha"
    );
}

fn tilted_aromatic_ring() -> Document {
    let mut doc = Document::default();
    let ids = editing::ring(&mut doc, Point::default(), 6, true, 0.);
    crate::projection::tilt(&mut doc, &ids, 65., true);
    crate::projection::tilt(&mut doc, &ids, 20., false);
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    // A single exposed ring has no visibility masks. Cross its actual circle
    // with foreground ink so the shared-paint comparison exercises occlusion.
    let circle = crate::aromatic::circles(&doc).remove(0);
    let foreground: Vec<_> = [-1.1, 1.1]
        .into_iter()
        .map(|x| {
            let id = doc.add_atom("C", circle.center.offset(circle.radius * x, 0.));
            doc.atom_mut(id).unwrap().depth = 100.;
            id
        })
        .collect();
    doc.add_bond(foreground[0], foreground[1], 1, "plain");
    depth::set_rear_opacity(&mut doc, &foreground, 0.25).unwrap();
    doc
}

fn assert_ring_paint_parity(doc: &Document) -> (usize, usize) {
    let raw = Paint::new(doc);
    assert_eq!(!raw.has_visibility(), !present(doc));
    let painted = depth::Paint::new(doc).materialize(doc);
    let canonical = crate::canvas_theme::canonical_document(&painted);
    let canonical = canonical.as_ref();
    let old = Paint::new(canonical);
    let mut circle_parts = 0;
    for circle in crate::aromatic::circles(canonical) {
        for part in circle.graphic().parts() {
            circle_parts += 1;
            let stroke = Primitive::Path {
                commands: part.commands,
                style: part.style,
                filled: part.filled,
            };
            let old = crate::crossings::ring_stroke(canonical, &circle, stroke.clone(), &old);
            let new = crate::crossings::ring_stroke(canonical, &circle, stroke, &raw);
            assert_eq!(format!("{old:?}"), format!("{new:?}"));
        }
    }
    let old_arcs = crate::ring_arcs::render_with_opacity(canonical, || &old);
    let new_arcs = crate::ring_arcs::render_with_opacity(canonical, || &raw);
    assert_eq!(
        format!("{:?}", old_arcs.primitives),
        format!("{:?}", new_arcs.primitives)
    );
    assert_eq!(old_arcs.owners, new_arcs.owners);
    assert_eq!(
        format!("{:?}", old_arcs.crossings),
        format!("{:?}", new_arcs.crossings)
    );
    assert_eq!(old_arcs.bonds, new_arcs.bonds);
    assert_eq!(
        format!("{:?}", crate::scene::primitives(doc)),
        format!(
            "{:?}",
            crate::scene::primitives_with_opacity(doc, doc, &raw)
        )
    );
    (circle_parts, new_arcs.primitives.len())
}

#[test]
fn raw_and_canonical_ring_paints_agree_for_c60_rear_opacity() {
    let doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    assert_ring_paint_parity(&doc);
}

#[test]
fn raw_and_canonical_ring_paints_agree_for_tilted_circles_and_arcs() {
    let original = tilted_aromatic_ring();
    let ring = crate::aromatic::circles(&original).remove(0);
    for theme in crate::canvas_theme::CanvasTheme::ALL {
        for rgb_fade in [false, true] {
            let mut doc = crate::canvas_theme::for_paste(original.clone(), theme);
            if rgb_fade {
                depth::enable(&mut doc, &ring.atoms, 0.6).unwrap();
                assert_ne!(depth::Paint::new(&doc).materialize(&doc).as_ref(), &doc);
            }
            let paint = Paint::new(&doc);
            assert_eq!(paint.owned_at(&doc, &ring.atoms, ring.center), 0.25);
            for ring_arc in [false, true] {
                for bond in &mut doc.bonds {
                    bond.ring_arc = ring_arc && ring.contains_bond(bond.a, bond.b);
                }
                let (circles, arcs) = assert_ring_paint_parity(&doc);
                assert!(circles > 0);
                assert_eq!(arcs > 0, ring_arc);
            }
        }
    }
}

#[test]
fn raw_and_canonical_ring_paints_agree_without_rear_opacity() {
    let mut doc = tilted_aromatic_ring();
    doc.depth_appearance.clear();
    let ring = crate::aromatic::circles(&doc).remove(0);
    for ring_arc in [false, true] {
        for bond in &mut doc.bonds {
            bond.ring_arc = ring_arc && ring.contains_bond(bond.a, bond.b);
        }
        let (circles, arcs) = assert_ring_paint_parity(&doc);
        assert!(circles > 0);
        assert_eq!(arcs > 0, ring_arc);
    }
}

/// An opaque aromatic ring R crossed by a 25% bond X in front of it, where an
/// opaque bond in front of X hides X at that crossing, plus 80 distant tilted
/// RGB-faded ring-arc rings. RGB materialization splits each of their arcs
/// into six pieces, so only the canonical document exceeds the visibility
/// preflight budget and falls back to opaque. Returns R's atoms.
fn visibility_budget_boundary_drawing(ring_arc: bool) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let ring = editing::ring(&mut doc, Point::default(), 6, true, 0.);
    let circle = crate::aromatic::circles(&doc).remove(0);
    let (center, radius) = (circle.center, circle.radius);
    let mut bond = |from: Point, to: Point, depth: f32| {
        let ids: Vec<_> = [from, to]
            .into_iter()
            .map(|p| {
                let id = doc.add_atom("C", p);
                doc.atom_mut(id).unwrap().depth = depth;
                id
            })
            .collect();
        doc.add_bond(ids[0], ids[1], 1, "plain");
        ids
    };
    // X crosses R's curve once, at a shallow angle; the occluder crosses
    // X and the curve radially at that same point.
    let x = bond(
        center.offset(0., 0.95 * radius),
        center.offset(1.5 * radius, 0.95 * radius),
        50.,
    );
    let (dx, dy) = (0.312 * radius, 0.95 * radius);
    bond(
        center.offset(0.5 * dx, 0.5 * dy),
        center.offset(1.5 * dx, 1.5 * dy),
        100.,
    );
    depth::set_rear_opacity(&mut doc, &x, 0.25).unwrap();
    for bond in &mut doc.bonds {
        bond.ring_arc = ring_arc && ring.contains(&bond.a) && ring.contains(&bond.b);
    }
    for i in 0..80 {
        let ids = editing::ring(
            &mut doc,
            Point::new(1000. + (i % 10) as f32 * 200., (i / 10) as f32 * 200.),
            6,
            false,
            0.,
        );
        crate::projection::tilt(&mut doc, &ids, 65., true);
        crate::projection::tilt(&mut doc, &ids, 20., false);
        for bond in &mut doc.bonds {
            bond.ring_arc |= ids.contains(&bond.a) && ids.contains(&bond.b);
        }
        depth::enable(&mut doc, &ids, 0.6).unwrap();
    }
    (doc, ring)
}

#[test]
fn ring_knockouts_keep_the_canonical_paint_at_the_visibility_budget_boundary() {
    for ring_arc in [false, true] {
        let (doc, mut ring) = visibility_budget_boundary_drawing(ring_arc);
        ring.sort_unstable();
        let is_ring = |ids: &[u64]| {
            let mut ids = ids.to_vec();
            ids.sort_unstable();
            ids == ring
        };
        let raw = Paint::new(&doc);
        let painted = depth::Paint::new(&doc).materialize(&doc);
        let canonical = crate::canvas_theme::canonical_document(&painted);
        let canonical = canonical.as_ref();
        let old = Paint::new(canonical);
        assert!(raw.has_visibility());
        assert!(!old.has_visibility());
        assert_eq!(raw.bond(&doc.bonds[6], 0.208), 0.25);
        let strokes = |paint: &Paint| {
            if ring_arc {
                let arcs = crate::ring_arcs::render_with_opacity(canonical, || paint);
                let owned = arcs.owners.iter().map(|ids| is_ring(ids));
                arcs.primitives
                    .iter()
                    .zip(owned)
                    .filter(|(_, owned)| *owned)
                    .map(|(stroke, _)| format!("{stroke:?}"))
                    .collect::<Vec<_>>()
            } else {
                let circle = crate::aromatic::circles(canonical)
                    .into_iter()
                    .find(|c| is_ring(&c.atoms))
                    .unwrap();
                circle
                    .graphic()
                    .parts()
                    .into_iter()
                    .map(|part| {
                        let stroke = Primitive::Path {
                            commands: part.commands,
                            style: part.style,
                            filled: part.filled,
                        };
                        let (stroke, _) =
                            crate::crossings::ring_stroke(canonical, &circle, stroke, paint);
                        format!("{stroke:?}")
                    })
                    .collect()
            }
        };
        // The faded X cuts R's curve only under the canonical Paint, which
        // ring knockouts used before the scene Paint was shared.
        let (expected, shared) = (strokes(&old), strokes(&raw));
        assert!(!expected.is_empty());
        assert_ne!(expected, shared);
        let scene = format!("{:?}", crate::scene::primitives(&doc));
        for (expected, shared) in expected.iter().zip(&shared) {
            assert!(scene.contains(expected.as_str()));
            assert!(!scene.contains(shared.as_str()));
        }
    }
}

#[test]
fn opaque_paint_avoids_maps_and_preserves_every_ink_query() {
    let doc = chain();
    let paint = Paint::new(&doc);
    assert!(paint.nodes.is_empty());
    assert!(paint.atoms.is_empty());
    assert!(paint.corners.is_empty());
    assert!(!paint.has_visibility());
    let bond = &doc.bonds[1];
    assert!(!paint.manages_bond(bond));
    assert_eq!(paint.atom(bond.a), 1.);
    assert_eq!(paint.bond(bond, 0.5), 1.);
    assert_eq!(
        paint.owned_at(&doc, &[bond.a, bond.b], Point::default()),
        1.
    );
    let parts = vec![Primitive::Line(
        Point::new(42., 0.),
        Point::new(84., 0.),
        2.,
    )];
    let rail = crate::scene::projected_bonds::RailDepth {
        points: (Point::new(42., 2.), Point::new(84., 2.)),
        field: DepthField::Constant(-20.),
        support: vec![bond.a, bond.b],
    };
    for result in [
        paint.atom_parts(bond.a, parts.clone()),
        paint.bond_parts(&doc, bond, parts.clone()),
        paint.rail_parts(&doc, bond, &rail, parts.clone()),
        paint.owned_parts(&doc, &[bond.a, bond.b], parts.clone()),
    ] {
        assert_eq!(format!("{result:?}"), format!("{parts:?}"));
    }
    assert!(paint.exposed_caps(&doc, bond, &parts[0]).is_empty());
}

#[test]
fn ring_arc_wrapper_builds_visibility_only_for_emitted_crossing_strokes() {
    let mut doc = tilted_aromatic_ring();
    let before = visibility::construction_count();
    assert!(crate::ring_arcs::render(&doc).primitives.is_empty());
    // A ring_arc flag on a bond outside every ring still emits no stroke.
    doc.bonds.last_mut().unwrap().ring_arc = true;
    assert!(crate::ring_arcs::render(&doc).primitives.is_empty());
    assert_eq!(visibility::construction_count(), before);
    let ring = crate::aromatic::circles(&doc).remove(0);
    for bond in &mut doc.bonds {
        bond.ring_arc = ring.contains_bond(bond.a, bond.b);
    }
    doc.bonds[0].color = crate::palette::Color::Custom([32, 80, 145]);
    assert!(crate::ring_arcs::uncut(&doc).primitives.len() > 1);
    assert_eq!(visibility::construction_count(), before);
    assert!(crate::ring_arcs::render(&doc).primitives.len() > 1);
    assert_eq!(visibility::construction_count() - before, 1);
}

#[test]
fn shared_arrow_paint_keeps_picks_and_never_rebuilds_visibility() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    let hidden = doc.atom(52).unwrap().position;
    let rim = doc.atom(38).unwrap().position;
    let bond = doc
        .atom(52)
        .unwrap()
        .position
        .lerp(doc.atom(57).unwrap().position, 0.5);
    let points = [hidden, rim, bond, Point::new(50_000., 50_000.)];
    for alpha in [0., 0.25, 1.] {
        depth::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        let paint = std::rc::Rc::new(Paint::new(&doc));
        let before = visibility::construction_count();
        let picks: Vec<_> = points
            .iter()
            .map(|p| crate::arrow_anchors::pick_with_paint(&doc, *p, 2., paint.clone()))
            .collect();
        assert_eq!(visibility::construction_count(), before);
        for (point, pick) in points.iter().zip(picks) {
            assert_eq!(pick, crate::arrow_anchors::pick(&doc, *point, 2.));
        }
    }
}

fn dative_projection_end(display: &str) -> Document {
    let mut doc = Document::default();
    let ids: Vec<_> = [
        (0., 0., -10.),
        (80., 0., 10.),
        (-40., -30., -20.),
        (120., 30., 20.),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(ids[0], ids[1], crate::bonds::DATIVE, display);
    doc.bonds[0].projection = true;
    doc.add_bond(ids[2], ids[0], 1, "plain");
    doc.add_bond(ids[1], ids[3], 1, "plain");
    depth::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    doc
}

fn dative_occlusion_signature(doc: &Document) -> Vec<(Vec<u64>, i16, String, String)> {
    let bond = &doc.bonds[0];
    let mut owners = vec![bond.a, bond.b];
    owners.sort_unstable();
    crate::scene::occlusion_ink(doc)
        .into_iter()
        .filter_map(|mut source| {
            source.owners.sort_unstable();
            if source.owners != owners {
                return None;
            }
            let field = match source.field {
                DepthField::Constant(z) => format!("constant {z:?}"),
                DepthField::Bond { a, b, wa, wb } => format!("bond {a:?} {b:?} {wa:?} {wb:?}"),
                DepthField::Plane {
                    origin,
                    weight,
                    dx,
                    dy,
                } => format!("plane {origin:?} {weight:?} {dx:?} {dy:?}"),
            };
            let parts = format!("{:?}", source.parts);
            Some((source.owners, source.layer, field, parts))
        })
        .collect()
}

#[test]
fn dative_projection_end_occlusion_matches_drawn_geometry() {
    let mut mismatches = vec![];
    for display in ["wedge_end", "hash_end", "hollow_wedge_end"] {
        let mut doc = dative_projection_end(display);
        assert!(Paint::new(&doc).has_visibility(), "{display}");
        let mut twin = doc.clone();
        twin.bonds[0].display = display.trim_end_matches("_end").into();
        twin.bonds[0].reverse();
        assert!(Paint::new(&twin).has_visibility(), "{display}");
        assert_eq!(
            format!("{:?}", crate::scene::primitives(&doc)),
            format!("{:?}", crate::scene::primitives(&twin)),
            "render anchor: {display}"
        );
        let ink = dative_occlusion_signature(&doc);
        assert!(!ink.is_empty(), "{display}");
        if ink != dative_occlusion_signature(&twin) {
            mismatches.push(display);
        }
        let ids = doc.all_ids();
        depth::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
        assert!(!Paint::new(&doc).has_visibility(), "{display}");
        let mut clear = doc.clone();
        clear.depth_appearance.clear();
        assert_eq!(
            format!("{:?}", crate::scene::primitives(&doc)),
            format!("{:?}", crate::scene::primitives(&clear)),
            "opaque render: {display}"
        );
    }
    assert!(mismatches.is_empty(), "occlusion drift: {mismatches:?}");
}
