use super::*;
use crate::document::Point;

fn alcohol() -> Document {
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let o = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(c, o, 1, "plain");
    doc
}

#[test]
fn independent_fragments_survive_errors_and_only_unchanged_chemistry_is_reused() {
    let mut source = alcohol();
    let invalid = source.add_atom("O", Point::new(100., 100.));
    source.atom_mut(invalid).unwrap().explicit_h = 5;
    let cached = Refresh::calculate(&source, &Refresh::default()).unwrap();
    assert!(cached.notice.is_some());
    cached.apply(&mut source);
    assert_eq!(source.atoms[1].label_h, 1);
    let repeated = Refresh::calculate(&source, &cached).unwrap();
    for id in [1, invalid] {
        assert!(Arc::ptr_eq(&cached.cache[&id], &repeated.cache[&id]));
    }
    source.atoms[1].element = "N".into();
    let changed = Refresh::calculate(&source, &cached).unwrap();
    assert!(!Arc::ptr_eq(&cached.cache[&1], &changed.cache[&1]));
    assert!(Arc::ptr_eq(
        &cached.cache[&invalid],
        &changed.cache[&invalid]
    ));
    changed.apply(&mut source);
    assert_eq!(source.atoms[1].label_h, 2);
    source.atoms[1].position.y += 5.;
    let moved = Refresh::calculate(&source, &changed).unwrap();
    assert!(!Arc::ptr_eq(&changed.cache[&1], &moved.cache[&1]));
    source.atoms[1].charge = 1;
    let charged = Refresh::calculate(&source, &moved).unwrap();
    charged.apply(&mut source);
    assert_eq!(source.atoms[1].label_h, 3);
    source.atoms[1].explicit_h = 1;
    source.atoms[1].no_implicit = true;
    let explicit = Refresh::calculate(&source, &charged).unwrap();
    assert!(!Arc::ptr_eq(&charged.cache[&1], &explicit.cache[&1]));
}

#[test]
fn gallery_attachment_does_not_block_ordinary_oxygen_and_display_is_preserved() {
    let mut source: Document = serde_json::from_str(include_str!(
        "../../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let ids = crate::editing::append(&mut source, &alcohol(), Point::new(-100., -100.));
    let before = source.clone();
    let result = Refresh::calculate(&source, &Refresh::default()).unwrap();
    assert!(result.notice.is_none(), "{:?}", result.notice);
    result.apply(&mut source);
    assert_eq!(source.atom(ids[1]).unwrap().label_h, 1);
    for (before, after) in before.atoms.iter().zip(&source.atoms) {
        let mut atom = before.clone();
        atom.label_h = after.label_h;
        atom.cip_label.clone_from(&after.cip_label);
        assert_eq!(&atom, after);
    }
    for (before, after) in before.bonds.iter().zip(&source.bonds) {
        let mut bond = before.clone();
        bond.cip_label.clone_from(&after.cip_label);
        assert_eq!(&bond, after);
    }
    assert_eq!(source.abbreviations, before.abbreviations);
}

#[test]
fn unsupported_components_keep_labels_without_hiding_invalid_drawings() {
    for kind in [
        Some(crate::attachments::Kind::MultiCenter),
        Some(crate::attachments::Kind::Variable),
        None,
    ] {
        let mut source = alcohol();
        source.atoms[1].label_h = 1;
        let point = crate::projection::add_centroid(&mut source, &[1, 2]).unwrap();
        source.atom_mut(point).unwrap().attachment = kind;
        let before = source.clone();
        let first = Refresh::calculate(&source, &Refresh::default()).unwrap();
        let cached = Refresh::calculate(&source, &first).unwrap();
        assert!(cached.notice.is_none());
        assert!(Arc::ptr_eq(&first.cache[&1], &cached.cache[&1]));
        cached.apply(&mut source);
        assert_eq!(source, before, "Keep labels and attachment semantics");
        assert!(
            chemistry::prepare(&source).is_err(),
            "Export stays restricted"
        );

        let invalid = source.add_atom("O", Point::new(100., 100.));
        source.atom_mut(invalid).unwrap().explicit_h = 5;
        let result = Refresh::calculate(&source, &cached).unwrap();
        assert!(result.notice.is_some(), "Independent valence errors remain");

        source.atom_mut(point).unwrap().centroid = vec![999, 1000];
        assert!(
            Refresh::calculate(&source, &result).is_err(),
            "Malformed attachment targets must still fail validation"
        );
    }
}

#[test]
fn cache_is_bounded_and_deleted_components_are_evicted() {
    let mut source = Document::default();
    for _ in 0..MAX_CACHED_COMPONENTS + 2 {
        source.add_atom("O", Point::default());
    }
    let cached = Refresh::calculate(&source, &Refresh::default()).unwrap();
    assert_eq!(cached.cache.len(), MAX_CACHED_COMPONENTS);
    let result = Refresh::calculate(&alcohol(), &cached).unwrap();
    assert_eq!(result.cache.len(), 1);
    let empty = Refresh::calculate(&Document::default(), &result).unwrap();
    assert!(empty.cache.is_empty());
}
