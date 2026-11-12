//! Fonts added at runtime, as the browser build does with a page's fonts.

#[test]
fn generic_sans_fallback_precedes_cjk_without_replacing_requested_families() {
    use resvg::usvg::fontdb::{Database, Language};

    let mut db = Database::new();
    for name in ["Noto Sans CJK JP", "Local Sans", "Requested Font"] {
        db.load_font_data(include_bytes!("../../fonts/LiberationSans-Regular.ttf").to_vec());
        let mut face = db.faces().last().unwrap().clone();
        db.remove_face(face.id);
        face.families = vec![(name.into(), Language::English_UnitedStates)];
        db.push_face_info(face);
    }
    db.set_sans_serif_family("Local Sans");
    super::font::load_bundled(&mut db);
    let fonts = super::Fonts {
        names: super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    let style = crate::typography::TextStyle {
        family: "Missing Font".into(),
        ..Default::default()
    };
    assert_eq!(super::font_name_in(&style.family, &fonts), "Local Sans");
    for c in "CHO".chars() {
        assert_eq!(super::glyph_metrics_in(c, &style, &fonts).0, "Local Sans");
    }
    for family in ["Noto Sans CJK JP", "Requested Font"] {
        let requested = crate::typography::TextStyle {
            family: family.into(),
            ..style.clone()
        };
        assert_eq!(super::glyph_metrics_in('O', &requested, &fonts).0, family);
        assert_eq!(super::font_name_in(family, &fonts), family);
    }
}

#[test]
fn missing_generic_family_uses_an_installed_sans_before_the_bundle() {
    use resvg::usvg::fontdb::{Database, Family, Language, Query};

    let mut db = Database::new();
    db.load_font_data(include_bytes!("../../fonts/LiberationSans-Regular.ttf").to_vec());
    let mut face = db.faces().next().unwrap().clone();
    db.remove_face(face.id);
    face.families = vec![("Local Sans".into(), Language::English_UnitedStates)];
    db.push_face_info(face);
    assert!(
        db.query(&Query {
            families: &[Family::SansSerif],
            ..Default::default()
        })
        .is_none()
    );
    super::font::load_bundled(&mut db);
    assert_eq!(db.family_name(&Family::SansSerif), "Local Sans");
    assert!(
        db.query(&Query {
            families: &[Family::Name("Arial")],
            ..Default::default()
        })
        .is_some()
    );
    let fonts = super::Fonts {
        names: super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    assert_eq!(
        super::glyph_metrics_in(
            'O',
            &crate::typography::TextStyle {
                family: "Missing Font".into(),
                ..Default::default()
            },
            &fonts
        )
        .0,
        "Local Sans"
    );
}

#[test]
fn bundled_sans_supplies_glyphs_missing_from_installed_faces() {
    use resvg::usvg::fontdb::Database;

    let mut db = Database::new();
    db.load_font_data(
        include_bytes!("../../../../tests/fixtures/font-export-103/static-400.subset.ttf").to_vec(),
    );
    let family = db.faces().next().unwrap().families[0].0.clone();
    db.set_sans_serif_family(&family);
    super::font::load_bundled(&mut db);
    let fonts = super::Fonts {
        names: super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    let style = crate::typography::TextStyle {
        family: family.clone(),
        ..Default::default()
    };
    assert_eq!(super::glyph_metrics_in('N', &style, &fonts).0, family);
    assert_eq!(super::glyph_metrics_in('C', &style, &fonts).0, "Arial");
}

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
    let order: Vec<_> = super::fallback_families("Arial", &fonts.db, &fonts.added).collect();
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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bundled_faces_supply_arial_and_helvetica_for_each_label_style() {
    use resvg::usvg::fontdb::{Database, Family, Query, Style, Weight};

    let mut db = Database::new();
    super::load_bundled_fonts(&mut db);
    assert_eq!(db.faces().count(), 4);
    assert_eq!(db.family_name(&Family::SansSerif), "Arial");
    let fonts = super::Fonts {
        names: super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    for (bold, italic, bytes) in [
        (
            false,
            false,
            include_bytes!("../../fonts/LiberationSans-Regular.ttf").as_slice(),
        ),
        (
            true,
            false,
            include_bytes!("../../fonts/LiberationSans-Bold.ttf").as_slice(),
        ),
        (
            false,
            true,
            include_bytes!("../../fonts/LiberationSans-Italic.ttf").as_slice(),
        ),
        (
            true,
            true,
            include_bytes!("../../fonts/LiberationSans-BoldItalic.ttf").as_slice(),
        ),
    ] {
        let weight = if bold { 700 } else { 400 };
        let style = if italic { Style::Italic } else { Style::Normal };
        let expected = super::font::face(bytes, 0, weight).unwrap();
        for family in ["Arial", "Helvetica"] {
            let db = &fonts.db;
            let id = db
                .query(&Query {
                    families: &[Family::Name(family)],
                    weight: Weight(weight),
                    style,
                    ..Default::default()
                })
                .unwrap();
            let info = db.face(id).unwrap();
            assert_eq!(info.weight, Weight(weight));
            assert_eq!(info.style, style);
            assert_eq!(
                info.families
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
                [
                    "Arial",
                    "Helvetica",
                    "Liberation Sans",
                    super::font::BUNDLED_FAMILY
                ]
            );
            let text_style = crate::typography::TextStyle {
                family: family.into(),
                bold,
                italic,
                ..Default::default()
            };
            for c in "CNO".chars() {
                let glyph = expected.glyph_index(c).unwrap();
                let advance = expected.glyph_hor_advance(glyph).unwrap();
                assert!(advance > 0);
                assert!(expected.glyph_bounding_box(glyph).is_some());
                assert_eq!(
                    super::glyph_metrics_in(c, &text_style, &fonts),
                    (
                        family,
                        f32::from(advance) / f32::from(expected.units_per_em())
                    )
                );
            }
        }
    }
}

#[test]
fn an_existing_arial_face_skips_the_bundle_and_keeps_its_metrics() {
    use resvg::usvg::fontdb::{Database, Language};

    let bytes = include_bytes!("../../../../tests/fixtures/font-export-103/static-400.subset.ttf");
    let mut db = Database::new();
    db.load_font_data(bytes.to_vec());
    let mut face = db.faces().next().unwrap().clone();
    db.remove_face(face.id);
    face.families
        .push(("Arial".into(), Language::English_UnitedStates));
    let id = db.push_face_info(face);
    let original_families = db.face(id).unwrap().families.clone();
    super::load_bundled_fonts(&mut db);
    assert_eq!(db.faces().count(), 1);
    assert_eq!(db.face(id).unwrap().families, original_families);
    let fonts = super::Fonts {
        names: super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    let expected = super::font::face(bytes, 0, 400).unwrap();
    for c in "NO".chars() {
        let advance = expected
            .glyph_hor_advance(expected.glyph_index(c).unwrap())
            .unwrap();
        assert_eq!(
            super::glyph_metrics_in(c, &crate::typography::TextStyle::default(), &fonts),
            (
                "Arial",
                f32::from(advance) / f32::from(expected.units_per_em())
            )
        );
    }
}

/// macOS and Windows ship Arial, so the bundled faces stay unloaded there.
#[cfg(any(target_os = "macos", windows))]
#[test]
fn installed_arial_leaves_the_bundled_faces_unloaded() {
    let mut db = resvg::usvg::fontdb::Database::new();
    super::load_fonts(&mut db);
    let has = |family: &str| {
        db.faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family))
    };
    assert!(has("Arial"));
    assert!(!has(super::font::BUNDLED_FAMILY));
}
