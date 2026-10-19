use super::*;
use crate::generic::drawing_test_support::{
    chain, legible, raw, same_except_positions, union_actual_ink_contained, union_source,
};
use crate::generic::probe;
use crate::{
    atom_text::{self, Mode},
    generic::{self, Limits},
};

#[test]
fn generated_union_keeps_fixed_maps_captions_cubic_arrows_and_rear_metadata() {
    let source = union_source();
    let before = source.clone();
    let products = generic::expand_drawings(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 3);
    for (i, product) in products.iter().enumerate() {
        same_except_positions(product.document.clone(), &raw(&source, i as u32 + 1));
        assert_eq!(product.document.atom(4), source.atom(4));
        assert_eq!(product.document.depth_appearance, source.depth_appearance);
        assert_eq!(product.document.molecule_names, source.molecule_names);
        assert_eq!(product.document.annotations, source.annotations);
        assert_eq!(product.document.arrows, source.arrows);
        union_actual_ink_contained(&product.document, product.bounds);
        legible(&crate::editing::analysis_document(
            &product.document,
            &product
                .document
                .atoms
                .iter()
                .filter(|a| a.id != 4)
                .map(|a| a.id)
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(source, before);
}

#[test]
fn prepared_union_publication_inherits_claims_without_identity_and_rejects_tampering() {
    let mut source = union_source();
    // Intentionally uninterpreted native claim: the contract transfers it, and
    // must not pretend to certify it by silently calling identifier generation.
    source.molecule_names[0].smiles = "inherited-native-claim".into();
    let before = source.clone();
    crate::molecule_names::IDENTITY_ENTRIES.with(|n| n.set(0));
    let input = generic::prepare_expansion_input(&source, &[], Limits::default()).unwrap();
    let prepared = generic::expand_prepared(input).unwrap();
    // Strict component selection preserves complete input exactly and avoids
    // ordinary delete/arrow-reconcile/name/automatic-weight paths.
    let selected =
        generic::prepare_expansion_input(&source, &[1, 2, 3], Limits::default()).unwrap();
    assert!(selected.source.molecule_names.is_empty());
    assert!(selected.source.arrows.is_empty());
    assert!(selected.source.depth_appearance.is_empty());
    assert_eq!(selected.source.generic, source.generic);
    assert_eq!(selected.source.atoms, source.atoms[..3]);
    assert!(
        generic::prepare_expansion_input(&source, &[2], Limits::default())
            .err()
            .unwrap()
            .contains("Select whole molecules")
    );
    let mut oversized = source.clone();
    oversized.annotations.push(crate::document::Annotation {
        id: oversized.next_id(),
        position: Point::default(),
        text: "x".repeat(Limits::default().max_work),
        format: Default::default(),
    });
    probe::reset();
    assert!(
        generic::prepare_expansion_input(&oversized, &[], Limits::default())
            .err()
            .unwrap()
            .contains("work limit")
    );
    assert!(probe::counts().iter().take(5).all(|count| *count == 0));
    let replay = prepared.clone();
    let bounds = prepared.bounds();
    assert!(!format!("{prepared:?}").contains("oxidane"));
    let insertion = crate::transaction::take_expansion(prepared).unwrap();
    let mut doc = Document::default();
    let empty = doc.clone();
    let mut history = crate::document::History::default();
    crate::transaction::install_expansion(&mut doc, empty, insertion)
        .unwrap()
        .commit(&mut doc, &mut history, false);
    assert_eq!(
        crate::molecule_names::IDENTITY_ENTRIES.with(std::cell::Cell::get),
        0
    );
    assert!(crate::transaction::take_expansion(replay).is_err());
    assert_eq!(doc.molecule_names.len(), 3);
    assert!(
        doc.molecule_names
            .iter()
            .all(|n| n.smiles == "inherited-native-claim" && n.text == "oxidane")
    );
    union_actual_ink_contained(&doc, bounds);
    assert_eq!(source, before);
    let tile = generic::expand_drawings(&source, Limits::default())
        .unwrap()
        .remove(0)
        .document;
    let mut target = tile.clone();
    target.atoms.clear();
    target.bonds.clear();
    target.annotations.clear();
    target.graphics.clear();
    target.arrows.clear();
    target.groups.clear();
    target.depth_appearance.clear();
    target.molecule_names.clear();
    let shift = Point::new(120., 80.);
    let (part, map, _) = crate::editing::preserved_append_part(&target, &tile, shift).unwrap();
    certify_translation(&tile, &part, &map, shift).unwrap();
    crate::molecule_names::certify_inherited(&tile, &part, &map, shift).unwrap();
    for case in 0..4 {
        let mut tampered = part.clone();
        match case {
            0 => {
                tampered
                    .atoms
                    .iter_mut()
                    .find(|a| a.id == map[&4])
                    .unwrap()
                    .charge = 1
            }
            1 => tampered.annotations[0].format.style.italic = false,
            2 => tampered.arrows[0].cubic.as_mut().unwrap()[0].x += 1.,
            _ => {
                let id = map[&4];
                tampered.depth_appearance[0].rear_weights.insert(id, 0.2);
            }
        }
        assert!(certify_translation(&tile, &tampered, &map, shift).is_err());
        if case <= 1 {
            assert!(
                crate::molecule_names::certify_inherited(&tile, &tampered, &map, shift).is_err()
            );
        }
    }
    // Independent negative construction: old+shift equality alone succeeds
    // at 1e30, while every 42-unit bond collapses. Local-vector proof rejects.
    let huge = Point::new(1e30, 0.);
    let (collapsed, map, _) = crate::editing::preserved_append_part(&target, &tile, huge).unwrap();
    assert!(
        certify_translation(&tile, &collapsed, &map, huge)
            .unwrap_err()
            .contains("local geometry")
    );
    // Real native loader performs endpoint repair. Demonstrate formerly unsafe
    // Atom AND Bond under-clearance links, then prove corrected packed geometry
    // survives that same loader and a refreshed-label reconciliation unchanged.
    for bond_target in [false, true] {
        let mut unsettled = union_source();
        let target = if bond_target {
            let tail = unsettled.add_atom("C", Point::new(-208., 80.));
            unsettled.add_bond(4, tail, 1, "plain");
            unsettled.molecule_names[0].atoms.push(tail);
            crate::arrow_anchors::Target::Bond {
                a: 4,
                b: tail,
                fraction: 0.3,
            }
        } else {
            crate::arrow_anchors::Target::Atom { atom: 4 }
        };
        let center = target.center(&unsettled).unwrap();
        let direction = if bond_target {
            Point::new(0., 1.)
        } else {
            Point::new(1., 0.)
        };
        let anchor = unsettled.arrows[0].start_anchor.as_mut().unwrap();
        anchor.target = target;
        anchor.direction = direction;
        anchor.offset = Point::new(0.1, 0.1);
        anchor.gap = 1000.;
        unsettled.arrows[0].start = center.offset(0.1, 0.1);
        unsettled.validate().unwrap();
        assert!(crate::arrow_anchors::certify_resolved(&unsettled).is_err());
        let before = unsettled.clone();
        let generated = generic::prepare_expansion(&unsettled, Limits::default()).unwrap();
        let bounds = generated.bounds();
        let mut doc = generated.take().unwrap();
        assert!(doc.arrows.iter().all(|a| {
            a.start_anchor
                .as_ref()
                .unwrap()
                .offset
                .distance(Point::default())
                > 999.
        }));
        crate::arrow_anchors::certify_resolved(&doc).unwrap();
        union_actual_ink_contained(&doc, bounds);
        let reloaded = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reloaded, doc.current());
        union_actual_ink_contained(&reloaded, bounds);
        let labels =
            crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default()).unwrap();
        labels.apply(&mut doc);
        let before_reconcile = doc.clone();
        crate::arrow_anchors::reconcile(&mut doc);
        assert_eq!(doc, before_reconcile);
        let reloaded = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        let mut expected = doc.current();
        crate::atom_labels::clear_computed(&mut expected);
        assert_eq!(reloaded, expected);
        union_actual_ink_contained(&reloaded, bounds);
        assert_eq!(unsettled, before);
    }
    // Saved H caches are drawing state, not chemical authority. They may be
    // larger than fresh valence H, and both immediate/reopened ink must fit.
    // A one-copy link retains the cache; a two-copy link changes chemistry and
    // correctly clears it. Element alternatives would clear both saved caches.
    let mut stale_cache =
        atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-2", Mode::Auto).unwrap();
    let oxygen = stale_cache.add_atom("O", Point::new(-200., 80.));
    let a = stale_cache.atom_mut(oxygen).unwrap();
    a.label_h = 123_456_789;
    a.display.hydrogens = Some(true);
    a.text_style = Some(crate::typography::TextStyle {
        size_pt: 144.,
        ..Default::default()
    });
    stale_cache.validate().unwrap();
    let original = stale_cache.clone();
    let prepared = generic::prepare_expansion(&stale_cache, Limits::default()).unwrap();
    assert_eq!(prepared.count(), 2);
    let bounds = prepared.bounds();
    let output = prepared.take().unwrap();
    assert_eq!(
        output
            .atoms
            .iter()
            .filter(|a| a.element == "O")
            .map(|a| a.label_h)
            .collect::<Vec<_>>(),
        vec![123_456_789, 0]
    );
    union_actual_ink_contained(&output, bounds);
    let reopened = Document::from_native_file(&output.file_json().unwrap()).unwrap();
    assert_eq!(reopened, output.current());
    union_actual_ink_contained(&reopened, bounds);
    assert_eq!(stale_cache, original);
    // A genuine post-install refresh can restore displayed CIP on an unrelated
    // specified component. Potential glyph envelopes must be prepaid and fit
    // even when the installed/native cache itself has no descriptor yet.
    let mut future_cip = chain(&["C", "*"]);
    future_cip = atom_text::apply(&future_cip, 2, "[Cl,Br]", Mode::Auto).unwrap();
    let center = future_cip.add_atom("C", Point::new(-240., 80.));
    let mut neighbors = Vec::new();
    for (element, x, y) in [
        ("F", -198., 80.),
        ("Cl", -240., 122.),
        ("Br", -282., 80.),
        ("I", -240., 38.),
    ] {
        let id = future_cip.add_atom(element, Point::new(x, y));
        future_cip.add_bond(center, id, 1, "plain");
        neighbors.push(id);
    }
    let center_atom = future_cip.atom_mut(center).unwrap();
    center_atom.stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    center_atom.display.stereo.show = Some(true);
    center_atom.display.stereo.offset = Some(Point::new(1000., 1000.));
    center_atom.display.stereo.style = crate::typography::TextStyle {
        family: "Times New Roman".into(),
        size_pt: 144.,
        italic: true,
        underline: true,
        ..Default::default()
    };
    // Cl and N outrank the implicit H at their respective alkene ends.
    // Both carriers lie on the same side of C=C: this is independently Z.
    let mut alkene = Vec::new();
    for (element, x, y) in [
        ("Cl", -360., -118.),
        ("C", -318., -160.),
        ("C", -276., -160.),
        ("N", -234., -118.),
    ] {
        alkene.push(future_cip.add_atom(element, Point::new(x, y)));
    }
    future_cip.add_bond(alkene[0], alkene[1], 1, "plain");
    future_cip.add_bond(alkene[1], alkene[2], 2, "plain");
    future_cip.add_bond(alkene[2], alkene[3], 1, "plain");
    let double = future_cip
        .bonds
        .iter_mut()
        .find(|b| b.a == alkene[1] && b.b == alkene[2])
        .unwrap();
    // Native stereo tags are lowercase; uppercase Z is a derived CIP label.
    double.stereo = Some("z".into());
    double.stereo_authoritative = true;
    double.stereo_atoms = vec![alkene[0], alkene[3]];
    double.indicator.show = Some(true);
    double.indicator.offset = Some(Point::new(-1000., -1000.));
    double.indicator.style = crate::typography::TextStyle {
        size_pt: 144.,
        italic: true,
        underline: true,
        ..Default::default()
    };
    future_cip.validate().unwrap();
    let original = future_cip.clone();
    let prepared = generic::prepare_expansion(&future_cip, Limits::default()).unwrap();
    let bounds = prepared.bounds();
    let mut output = prepared.take().unwrap();
    assert!(output.atoms.iter().all(|a| a.cip_label.is_none()));
    assert!(output.bonds.iter().all(|b| b.cip_label.is_none()));
    let fresh =
        crate::atom_labels::refresh::Refresh::calculate(&output, &Default::default()).unwrap();
    assert!(fresh.notice.is_none(), "{:?}", fresh.notice);
    fresh.apply(&mut output);
    assert!(output.atoms.iter().any(|a| a.cip_label.is_some()));
    assert_eq!(
        output
            .bonds
            .iter()
            .filter(|b| b.order == 2)
            .map(|b| b.cip_label.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Z"), Some("Z")]
    );
    union_actual_ink_contained(&output, bounds);
    let reopened = Document::from_native_file(&output.file_json().unwrap()).unwrap();
    union_actual_ink_contained(&reopened, bounds);
    assert_eq!(future_cip, original);
    let poisoned = generic::prepare_expansion(&source, Limits::default()).unwrap();
    let handle = poisoned.clone();
    assert!(
        std::panic::catch_unwind(move || {
            let _held = handle.data.document.lock().unwrap();
            panic!("intentional transport poison");
        })
        .is_err()
    );
    assert!(crate::transaction::take_expansion(poisoned).is_err());
    assert_eq!(doc.molecule_names.len(), 3);
    assert_eq!(source, before);
}
