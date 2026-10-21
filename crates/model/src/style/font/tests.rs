const VARIABLE: &[u8] =
    include_bytes!("../../../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf");

#[test]
fn metrics_follow_requested_weight_instead_of_the_variable_default() {
    for (bytes, weight) in [
        (
            include_bytes!("../../../../../tests/fixtures/font-export-103/static-400.subset.ttf")
                .as_slice(),
            400,
        ),
        (
            include_bytes!("../../../../../tests/fixtures/font-export-103/static-700.subset.ttf")
                .as_slice(),
            700,
        ),
    ] {
        let variable = super::face(VARIABLE, 0, weight).unwrap();
        let reference = super::face(bytes, 0, weight).unwrap();
        assert_eq!(variable.ascender(), reference.ascender());
        for c in "HNO".chars() {
            let vg = variable.glyph_index(c).unwrap();
            let sg = reference.glyph_index(c).unwrap();
            assert_eq!(
                variable.glyph_hor_advance(vg),
                reference.glyph_hor_advance(sg)
            );
            let actual = variable.glyph_bounding_box(vg).unwrap();
            let expected = reference.glyph_bounding_box(sg).unwrap();
            for (a, e) in [
                (actual.x_min, expected.x_min),
                (actual.y_min, expected.y_min),
                (actual.x_max, expected.x_max),
                (actual.y_max, expected.y_max),
            ] {
                // The variable outline has fractional points; static glyf
                // coordinates are rounded to integer font units.
                assert!((i32::from(a) - i32::from(e)).abs() <= 1);
            }
        }
    }
}

#[test]
fn added_fonts_join_measurement_and_export() {
    use crate::style::{add_font, added_fonts, font_families, font_generation, load_fonts};
    assert!(add_font(b"not a font".to_vec()).is_err());
    let before = font_generation();
    let bytes =
        include_bytes!("../../../../../tests/fixtures/font-export-103/cff-static-400.subset.otf");
    assert_eq!(add_font(bytes.to_vec()), Ok(1));
    assert_eq!(font_generation(), before + 1);
    assert_eq!(added_fonts(before).len(), 1);
    let family = "ReShiki Font Export Fixture";
    assert!(font_families().contains(&family));
    // Export loads its own database; it gets the added font too.
    let mut db = resvg::usvg::fontdb::Database::new();
    load_fonts(&mut db);
    assert!(
        db.faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family))
    );
}
