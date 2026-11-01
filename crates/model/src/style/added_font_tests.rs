//! Fonts added at runtime, as the browser build does with a page's fonts.

#[test]
fn missing_named_families_use_an_installed_face_for_label_metrics() {
    let mut db = resvg::usvg::fontdb::Database::new();
    for bytes in [
        include_bytes!("../../fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../fonts/LiberationSans-Bold.ttf").as_slice(),
        include_bytes!("../../fonts/LiberationSans-Italic.ttf").as_slice(),
        include_bytes!("../../fonts/LiberationSans-BoldItalic.ttf").as_slice(),
    ] {
        db.load_font_data(bytes.to_vec());
    }
    let names = super::family_names(&db);
    let mut fonts = super::Fonts {
        db: std::sync::Arc::new(db),
        names,
        added: vec![],
    };
    for bold in [false, true] {
        for italic in [false, true] {
            let style = crate::typography::TextStyle {
                family: "Arial".into(),
                bold,
                italic,
                ..Default::default()
            };
            for c in ['C', 'N'] {
                let (family, advance) = super::glyph_metrics_in(c, &style, &fonts);
                assert_eq!(family, "Liberation Sans");
                assert!(advance > 0.);
            }
            assert_eq!(
                super::glyph_metrics_in('\u{10ffff}', &style, &fonts),
                ("Arial", 0.6)
            );
        }
    }
    // Keep an installed alias in SVG instead of substituting the canonical name.
    let db = std::sync::Arc::make_mut(&mut fonts.db);
    let faces: Vec<_> = db.faces().cloned().collect();
    for mut face in faces {
        db.remove_face(face.id);
        face.families.push((
            "Arial".into(),
            resvg::usvg::fontdb::Language::English_UnitedStates,
        ));
        db.push_face_info(face);
    }
    fonts.names = super::family_names(db);
    assert_eq!(
        super::glyph_metrics_in('N', &crate::typography::TextStyle::default(), &fonts).0,
        "Arial"
    );
}

#[test]
fn last_resort_checks_glyph_weight_and_style_in_database_order() {
    use resvg::usvg::fontdb::{Database, Language, Style, Weight};
    let mut db = Database::new();
    let mut ids = vec![];
    for (bytes, name) in [
        (
            include_bytes!("../../fonts/LiberationSans-Regular.ttf").as_slice(),
            ".Hidden",
        ),
        (
            include_bytes!("../../fonts/LiberationSans-BoldItalic.ttf").as_slice(),
            "Bold Italic",
        ),
        (
            include_bytes!("../../fonts/LiberationSans-Bold.ttf").as_slice(),
            "Bold",
        ),
        (
            include_bytes!("../../fonts/LiberationSans-Italic.ttf").as_slice(),
            "Italic",
        ),
        (
            include_bytes!("../../../../tests/fixtures/font-export-103/static-400.subset.ttf")
                .as_slice(),
            "No C",
        ),
        (
            include_bytes!("../../fonts/LiberationSans-Regular.ttf").as_slice(),
            "Z First",
        ),
    ] {
        db.load_font_data(bytes.to_vec());
        let mut face = db.faces().last().unwrap().clone();
        db.remove_face(face.id);
        face.families = vec![(name.into(), Language::English_UnitedStates)];
        ids.push(db.push_face_info(face));
    }
    // Skips the hidden face, the other styles and weights, and the face without C.
    assert_eq!(
        super::resolve_font_in(&db, "Arial", Weight(400), Style::Normal, 'C', &[]),
        Some(ids[5])
    );
    // A sans face wins over a serif one listed before it.
    let mut db = Database::new();
    let mut ids = vec![];
    for name in ["Liberation Serif", "Liberation Sans"] {
        db.load_font_data(include_bytes!("../../fonts/LiberationSans-Regular.ttf").to_vec());
        let mut face = db.faces().last().unwrap().clone();
        db.remove_face(face.id);
        face.families = vec![(name.into(), Language::English_UnitedStates)];
        ids.push(db.push_face_info(face));
    }
    assert_eq!(
        super::resolve_font_in(&db, "Arial", Weight(400), Style::Normal, 'C', &[]),
        Some(ids[1])
    );
    // A lone variable face whose default weight is 100 still serves 400 and 700.
    let mut db = Database::new();
    db.load_font_data(
        include_bytes!(
            "../../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf"
        )
        .to_vec(),
    );
    let id = db.faces().next().map(|face| face.id);
    for weight in [400, 700] {
        assert_eq!(
            super::resolve_font_in(&db, "Arial", Weight(weight), Style::Normal, 'N', &[]),
            id
        );
    }
}

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
    let order: Vec<_> = super::fallback_families("Arial", &fonts.added).collect();
    assert_eq!(order.first(), Some(&"Arial"));
    assert!(order.contains(&family));
    assert!(order.ends_with(&fonts.added));
    drop(fonts);
    // Export shares the measuring database; another renderer loads its own.
    // Both get the added font.
    let mut db = resvg::usvg::fontdb::Database::new();
    load_fonts(&mut db);
    for db in [&db, &*super::fontdb()] {
        assert!(
            db.faces()
                .any(|face| face.families.iter().any(|(name, _)| name == family))
        );
    }
}
