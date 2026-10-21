//! Fonts added at runtime, as the browser build does with a page's fonts.

#[test]
fn added_fonts_join_measurement_and_export() {
    use super::{add_font, added_fonts, font_families, font_generation, load_fonts};
    assert!(add_font(b"not a font".to_vec()).is_err());
    let before = font_generation();
    let bytes =
        include_bytes!("../../../../tests/fixtures/font-export-103/cff-static-400.subset.otf");
    assert_eq!(add_font(bytes.to_vec()), Ok(1));
    assert_eq!(font_generation(), before + 1);
    assert_eq!(added_fonts(before).len(), 1);
    let family = "ReShiki Font Export Fixture";
    assert!(font_families().contains(&family));
    // Labels whose font lacks a glyph look it up there last.
    let fonts = super::fonts();
    let order: Vec<_> = super::fallback_families("Arial", &fonts).collect();
    assert_eq!(order.first(), Some(&"Arial"));
    assert!(order.contains(&family));
    assert!(order.ends_with(&fonts.added));
    drop(fonts);
    // Export loads its own database; it gets the added font too.
    let mut db = resvg::usvg::fontdb::Database::new();
    load_fonts(&mut db);
    assert!(
        db.faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family))
    );
}
