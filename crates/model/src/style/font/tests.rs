const VARIABLE: &[u8] =
    include_bytes!("../../../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf");

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bundled_faces_supply_arial_and_helvetica_for_each_label_style() {
    use resvg::usvg::fontdb::{Database, Family, Query, Style, Weight};

    let mut db = Database::new();
    super::super::load_bundled_fonts(&mut db);
    assert_eq!(db.faces().count(), 4);
    assert_eq!(db.family_name(&Family::SansSerif), "Arial");
    let fonts = super::super::Fonts {
        names: super::super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    for (bold, italic, bytes) in [
        (
            false,
            false,
            include_bytes!("../../../fonts/LiberationSans-Regular.ttf").as_slice(),
        ),
        (
            true,
            false,
            include_bytes!("../../../fonts/LiberationSans-Bold.ttf").as_slice(),
        ),
        (
            false,
            true,
            include_bytes!("../../../fonts/LiberationSans-Italic.ttf").as_slice(),
        ),
        (
            true,
            true,
            include_bytes!("../../../fonts/LiberationSans-BoldItalic.ttf").as_slice(),
        ),
    ] {
        let weight = if bold { 700 } else { 400 };
        let style = if italic { Style::Italic } else { Style::Normal };
        let expected = super::face(bytes, 0, weight).unwrap();
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
                    super::BUNDLED_FAMILY
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
                    super::super::glyph_metrics_in(c, &text_style, &fonts),
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

    let bytes =
        include_bytes!("../../../../../tests/fixtures/font-export-103/static-400.subset.ttf");
    let mut db = Database::new();
    db.load_font_data(bytes.to_vec());
    let mut face = db.faces().next().unwrap().clone();
    db.remove_face(face.id);
    face.families
        .push(("Arial".into(), Language::English_UnitedStates));
    let id = db.push_face_info(face);
    let original_families = db.face(id).unwrap().families.clone();
    super::super::load_bundled_fonts(&mut db);
    assert_eq!(db.faces().count(), 1);
    assert_eq!(db.face(id).unwrap().families, original_families);
    let fonts = super::super::Fonts {
        names: super::super::family_names(&db),
        db: std::sync::Arc::new(db),
        added: vec![],
    };
    let expected = super::face(bytes, 0, 400).unwrap();
    for c in "NO".chars() {
        let advance = expected
            .glyph_hor_advance(expected.glyph_index(c).unwrap())
            .unwrap();
        assert_eq!(
            super::super::glyph_metrics_in(c, &crate::typography::TextStyle::default(), &fonts),
            (
                "Arial",
                f32::from(advance) / f32::from(expected.units_per_em())
            )
        );
    }
}

#[test]
fn an_existing_helvetica_face_is_not_aliased_to_the_bundle() {
    use resvg::usvg::fontdb::{Database, Family, Language, Query};

    let mut db = Database::new();
    db.load_font_data(
        include_bytes!("../../../../../tests/fixtures/font-export-103/static-400.subset.ttf")
            .to_vec(),
    );
    let mut face = db.faces().next().unwrap().clone();
    db.remove_face(face.id);
    face.families = vec![("Helvetica".into(), Language::English_UnitedStates)];
    let helvetica = db.push_face_info(face);
    super::super::load_bundled_fonts(&mut db);
    assert_eq!(db.faces().count(), 5);
    assert_eq!(
        db.query(&Query {
            families: &[Family::Name("Helvetica")],
            ..Default::default()
        }),
        Some(helvetica)
    );
    for face in db.faces().filter(|face| face.id != helvetica) {
        assert_eq!(
            face.families
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["Arial", "Liberation Sans", super::BUNDLED_FAMILY]
        );
    }
}

/// macOS and Windows ship Arial, so the bundled faces stay unloaded there.
#[cfg(any(target_os = "macos", windows))]
#[test]
fn installed_arial_leaves_the_bundled_faces_unloaded() {
    let mut db = resvg::usvg::fontdb::Database::new();
    super::super::load_fonts(&mut db);
    let has = |family: &str| {
        db.faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family))
    };
    assert!(has("Arial"));
    assert!(!has(super::BUNDLED_FAMILY));
}

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

/// Exports and label metrics read the default family's faces for every glyph;
/// those must come from memory without changing which face a query finds.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn resident_faces_keep_their_order_and_matches() {
    use resvg::usvg::fontdb::{Database, Family, Query, Source, Weight};
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/font-export-103"
    );
    let load = || {
        let mut db = Database::new();
        for file in [
            "static-400.subset.ttf",
            "cff-static-700.subset.otf",
            "static-100.subset.ttf",
        ] {
            db.load_font_file(format!("{dir}/{file}")).unwrap();
        }
        db
    };
    let summary = |db: &Database| {
        db.faces()
            .map(|face| (face.post_script_name.clone(), face.weight, face.index))
            .collect::<Vec<_>>()
    };
    let family = load().faces().next().unwrap().families[0].0.clone();
    let query = |db: &Database, weight| {
        let id = db.query(&Query {
            families: &[Family::Name(&family)],
            weight: Weight(weight),
            ..Query::default()
        });
        id.map(|id| db.face(id).unwrap().post_script_name.clone())
    };
    let before = load();
    let mut db = load();
    super::keep_resident(&mut db, &["No Such Family"]);
    assert!(
        db.faces()
            .all(|face| matches!(face.source, Source::File(_)))
    );
    super::keep_resident(&mut db, &[&family]);
    assert_eq!(summary(&db), summary(&before));
    for weight in [100, 400, 700] {
        assert_eq!(query(&db, weight), query(&before, weight));
    }
    let data = |db: &Database, name: &str| {
        let face = db
            .faces()
            .find(|face| face.post_script_name == name)
            .unwrap();
        db.with_face_data(face.id, |data, index| (data.to_vec(), index))
    };
    for face in db.faces() {
        let in_family = face.families.iter().any(|(name, _)| *name == family);
        let resident = matches!(face.source, Source::Binary(_));
        assert_eq!(resident, in_family, "{}", face.post_script_name);
        let name = &face.post_script_name;
        assert_eq!(data(&db, name), data(&before, name));
    }
}
