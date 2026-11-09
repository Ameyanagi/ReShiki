const VARIABLE: &[u8] =
    include_bytes!("../../../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf");

#[test]
fn bundled_faces_supply_arial_in_an_empty_database_for_each_label_style() {
    use resvg::usvg::fontdb::{Database, Family, Query, Style, Weight};

    let mut db = Database::new();
    super::load_bundled(&mut db);
    assert_eq!(db.faces().count(), 4);
    for weight in [400, 700] {
        for style in [Style::Normal, Style::Italic] {
            let id = db
                .query(&Query {
                    families: &[Family::Name("Arial")],
                    weight: Weight(weight),
                    style,
                    ..Default::default()
                })
                .unwrap();
            let info = db.face(id).unwrap();
            assert_eq!(info.weight, Weight(weight));
            assert_eq!(info.style, style);
            assert!(
                info.families
                    .iter()
                    .any(|(name, _)| name == "Liberation Sans")
            );
            db.with_face_data(id, |bytes, index| {
                let face = super::face(bytes, index, weight).unwrap();
                for c in "CHO".chars() {
                    let glyph = face.glyph_index(c).unwrap();
                    assert!(face.glyph_hor_advance(glyph).unwrap() > 0);
                    assert!(face.glyph_bounding_box(glyph).is_some());
                }
            })
            .unwrap();
        }
    }
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
