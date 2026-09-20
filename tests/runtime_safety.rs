//! Regression cases for user-controlled values and stale editing references.
use reshiki::{
    document::{Document, Point},
    editing, scene,
    templates::{self, Anchor},
};

#[test]
fn stale_and_malformed_templates_fail_without_mutation() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let before = doc.clone();
    let mut part = doc.clone();
    assert!(
        templates::place_anchored(
            &doc,
            &part,
            Point::default(),
            None,
            8.,
            Anchor::Atom(u64::MAX)
        )
        .is_err()
    );
    part.atoms.retain(|atom| atom.id != b);
    assert!(templates::place(&doc, &part, Point::default(), None, 8.).is_err());
    assert!(editing::append(&mut doc, &part, Point::default()).is_empty());
    assert_eq!(doc, before);
    // Rendering an untrusted intermediate graph also skips dangling edges.
    let _ = scene::primitives(&part);
    let _ = editing::nearest_bond(&part, Point::default(), 8.);
}

#[test]
fn exhausted_ids_reject_copy_atomically() {
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    doc.atoms[0].id = u64::MAX - 1;
    let before = doc.clone();
    let mut part = Document::default();
    part.add_atom("O", Point::default());
    assert!(editing::append(&mut doc, &part, Point::default()).is_empty());
    assert_eq!(doc, before);
    assert!(templates::place(&doc, &part, Point::new(100., 0.), None, 8.).is_err());
    doc.atoms[0].id = u64::MAX;
    assert_eq!(doc.next_id(), u64::MAX);
    assert!(doc.validate().is_err());
}

#[test]
fn extreme_charge_and_stale_point_edits_do_not_panic() {
    let mut doc = Document::default();
    let id = doc.add_atom("N", Point::default());
    for charge in [i32::MIN, i32::MAX] {
        doc.atom_mut(id).unwrap().charge = charge;
        assert!(!scene::svg(&doc).is_empty());
        reshiki::scientific::attach(
            doc.atom_mut(id).unwrap(),
            reshiki::scientific::SymbolKind::Plus,
            Point::default(),
        )
        .unwrap();
    }
    reshiki::atom_labels::Owner::Number(u64::MAX).set_offset(&mut doc, None);
    reshiki::atom_labels::Owner::BondStereo(id, u64::MAX).set_offset(&mut doc, None);
    assert!(reshiki::atom_labels::sequence("a", usize::MAX).is_err());
    assert!(reshiki::atom_labels::sequence("18446744073709551615", 2).is_err());
}

#[test]
fn invalid_geometry_and_unicode_ranges_return_safely() {
    let doc = Document::default();
    let part = reshiki::rings::Preset::ChairUp.document(42., false);
    for radius in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(templates::place(&doc, &part, Point::default(), None, radius).is_err());
    }
    for length in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(
            reshiki::rings::Drawing {
                preset: reshiki::rings::Preset::ChairUp,
                length,
                alternate: false,
                connect: true
            }
            .place(&doc, Point::default(), None, 8.)
            .is_err()
        );
    }
    for range in [
        usize::MAX..usize::MAX,
        1..usize::MAX,
        std::ops::Range { start: 7, end: 0 },
        1..2,
    ] {
        let mut format = reshiki::typography::TextFormat::default();
        format.edited("αβγ", "δγ", range);
        format.validate("δγ").unwrap();
    }
}

#[test]
fn bundled_data_parses_and_retains_jacs_defaults() {
    assert!(templates::builtin_error().is_none());
    let style: reshiki::style::DrawingStyle =
        serde_json::from_str(include_str!("../engine/drawing_style.json")).unwrap();
    assert_eq!(style.font_family, "Arial");
    assert_eq!(style.font_size_pt, 10.);
    assert_eq!(style.bond_length_pt, 14.4);
    assert_eq!(style.line_width_pt, 0.6);
    assert_eq!(style.png_dpi, 1200);
}
