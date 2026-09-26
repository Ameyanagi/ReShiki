use reshiki::{
    canvas_theme::{self, CanvasTheme, ColorTheme},
    color_contrast::{TEXT_TARGET, contrast},
    document::Document,
    document_styles::{self, Preset},
    theme_files::{self, ColorValue, ThemeFile},
};

#[test]
fn builtin_themes_export_both_modes_without_style_or_artwork() {
    let dir = tempfile::tempdir().unwrap();
    for base in ColorTheme::ALL {
        let original = Document {
            color_theme: base,
            drawing_style: Preset::Nature.style(),
            ..Default::default()
        };
        let path = dir.path().join("sample.reshiki-theme");
        theme_files::save(&path, &ThemeFile::capture(&original)).unwrap();
        let file = theme_files::load(&path).unwrap();
        for mode in CanvasTheme::ALL {
            for &element in reshiki::editing::ELEMENTS {
                assert_eq!(
                    file.element_color(element, mode),
                    base.element_color(element, mode)
                );
                assert_eq!(
                    file.element_swatch(element, mode),
                    base.element_swatch(element, mode)
                );
            }
        }
        let mut doc = Document {
            drawing_style: Preset::Angewandte.style(),
            ..Default::default()
        };
        let style = doc.drawing_style.clone();
        file.apply(&mut doc).unwrap();
        assert_eq!(doc.drawing_style, style);
        doc.validate().unwrap();
        let json = serde_json::to_vec(&doc).unwrap();
        let reopened: Document = serde_json::from_slice(&json).unwrap();
        assert_eq!(reopened, doc);
        assert!(doc.version >= 16);
        ColorTheme::Publication.apply(&mut doc);
        assert!(doc.custom_theme.is_none());
    }
}

#[test]
fn curated_themes_cover_all_elements_and_ring_roles_in_both_modes() {
    for file in theme_files::bundled().unwrap() {
        for mode in CanvasTheme::ALL {
            let mut doc = reshiki::rings::Preset::Regular.document(42., false);
            doc.canvas_theme = mode;
            file.clone().apply(&mut doc).unwrap();
            for &element in reshiki::editing::ELEMENTS {
                doc.atoms[0].element = element.into();
                let ids = doc.all_ids();
                for (_, key) in reshiki::ring_fills::PALETTE {
                    reshiki::ring_fills::apply(&mut doc, &ids, Some(key));
                    let ink = mode.color(canvas_theme::atom_color(&doc, &doc.atoms[0]));
                    for bg in [
                        mode.background(),
                        canvas_theme::fill_color(&doc, &doc.ring_fills[0]),
                    ] {
                        assert!(
                            contrast(ink, bg) >= TEXT_TARGET,
                            "{}/{mode}/{element}",
                            file.name
                        );
                    }
                    let resolved = canvas_theme::resolved_document(&doc).into_owned();
                    assert!(resolved.custom_theme.is_none());
                    assert_eq!(
                        mode.color(canvas_theme::atom_color(&resolved, &resolved.atoms[0])),
                        ink
                    );
                    let pasted = canvas_theme::for_paste(doc.clone(), mode.toggled());
                    assert_eq!(
                        mode.toggled()
                            .color(canvas_theme::atom_color(&pasted, &pasted.atoms[0])),
                        ink
                    );
                }
            }
        }
    }
}

#[test]
fn imports_validate_format_limits_and_oklch_before_application() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("theme.reshiki-theme");
    let base = ThemeFile::capture(&Document::default());
    let good = serde_json::to_value(&base).unwrap();
    let mut cases = Vec::new();
    for (key, value) in [
        ("version", serde_json::json!(2)),
        ("id", serde_json::json!("../escape")),
        ("name", serde_json::json!("")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut case = good.clone();
        case[key] = value;
        cases.push(case);
    }
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("dark");
    cases.push(missing);
    for value in [
        serde_json::json!([256, 0, 0]),
        serde_json::json!({"oklch":[1.2,0.1,20]}),
        serde_json::json!({"oklch":[0.5,-1,20]}),
        serde_json::json!({"oklch":[0.5,0.1,400]}),
    ] {
        let mut case = good.clone();
        case["light"]["elements"]["O"] = value;
        cases.push(case);
    }
    let mut unknown = good.clone();
    unknown["light"]["elements"]["Xx"] = serde_json::json!([10, 20, 30]);
    cases.push(unknown);
    let mut bad_fill = good.clone();
    bad_fill["dark"]["ring_fills"]["Sky"] = serde_json::json!([255, 255, 255]);
    cases.push(bad_fill);
    for invalid in cases {
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(theme_files::load(&path).is_err(), "accepted {invalid}");
    }
    std::fs::write(&path, vec![b' '; theme_files::LIMIT + 1]).unwrap();
    assert!(theme_files::load(&path).unwrap_err().contains("256 KB"));
    let mut theme = base;
    theme.light.elements.insert(
        "O".into(),
        ColorValue::Oklch {
            oklch: [0.55, 0.4, 28.],
        },
    );
    theme_files::save(&path, &theme).unwrap();
    assert_eq!(theme_files::load(&path).unwrap(), theme);
    assert!(contrast(theme.element_color("O", CanvasTheme::Light), [255; 3]) >= TEXT_TARGET);
}

#[test]
fn styles_round_trip_as_native_json_and_chemdraw_stationery() {
    let dir = tempfile::tempdir().unwrap();
    for preset in Preset::ALL {
        let style = preset.style();
        let native = dir.path().join("style.reshiki-style");
        document_styles::save(&native, &style).unwrap();
        assert_eq!(document_styles::load(&native).unwrap(), style);
        let cds = dir.path().join("style.cds");
        document_styles::save(&cds, &style).unwrap();
        assert!(std::fs::read(&cds).unwrap().starts_with(b"VjCD"));
        let imported = document_styles::load(&cds).unwrap();
        assert_eq!(imported.font_family, style.font_family);
        for (actual, expected) in [
            (imported.bond_length_pt, style.bond_length_pt),
            (imported.font_size_pt, style.font_size_pt),
            (imported.line_width_pt, style.line_width_pt),
            (imported.bold_width_pt, style.bold_width_pt),
            (imported.margin_width_pt, style.margin_width_pt),
            (imported.hash_spacing_pt, style.hash_spacing_pt),
            (imported.bond_spacing_ratio, style.bond_spacing_ratio),
        ] {
            assert!(
                (actual - expected).abs() < 0.0011,
                "{preset}: {actual} vs {expected}"
            );
        }
    }
}

#[test]
fn custom_theme_reaches_figures_editable_exchange_and_transparent_clipboard() {
    let mut theme = theme_files::bundled().unwrap().remove(0);
    theme
        .light
        .ring_fills
        .insert("Sky".into(), ColorValue::Rgb([240, 226, 247]));
    theme
        .dark
        .ring_fills
        .insert("Sky".into(), ColorValue::Rgb([86, 70, 105]));
    for mode in CanvasTheme::ALL {
        let mut doc = reshiki::rings::Preset::Regular.document(42., false);
        doc.canvas_theme = mode;
        doc.atoms[0].element = "O".into();
        theme.clone().apply(&mut doc).unwrap();
        let ids = doc.all_ids();
        reshiki::ring_fills::apply(&mut doc, &ids, Some(reshiki::ring_fills::PALETTE[0].1));
        let ink = mode.color(canvas_theme::atom_color(&doc, &doc.atoms[0]));
        let fill = canvas_theme::fill_color(&doc, &doc.ring_fills[0]);
        let svg = reshiki::scene::svg(&doc);
        let tree = roxmltree::Document::parse(&svg).unwrap();
        let label = tree
            .descendants()
            .find(|n| n.has_tag_name("text") && n.text() == Some("O"))
            .unwrap();
        assert_eq!(
            label.attribute("fill"),
            Some(format!("rgb({},{},{})", ink[0], ink[1], ink[2]).as_str())
        );
        let png = reshiki::export::clipboard_drawing(&doc, "png").unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
        for rgb in [ink, fill] {
            assert!(image.pixels().any(|p| p.0 == [rgb[0], rgb[1], rgb[2], 255]));
        }
        let xml = reshiki::exchange::drawing::write(&doc, Default::default()).unwrap();
        let decoded =
            reshiki::exchange::from_cdx(&reshiki::exchange::to_cdx(&xml).unwrap()).unwrap();
        let tree = roxmltree::Document::parse(&decoded).unwrap();
        let colors: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("color"))
            .collect();
        let label = tree
            .descendants()
            .find(|n| n.has_tag_name("s") && n.text() == Some("O"))
            .unwrap();
        let color = colors[label.attribute("color").unwrap().parse::<usize>().unwrap() - 2];
        let rgb = ["r", "g", "b"]
            .map(|c| (color.attribute(c).unwrap().parse::<f64>().unwrap() * 255.).round() as u8);
        assert_eq!(rgb, ink);
        let snapshot =
            reshiki::printing::snapshot(&doc, &[], reshiki::printing::Scope::Document).unwrap();
        assert_eq!(snapshot.custom_theme, doc.custom_theme);
        assert_eq!(snapshot.version, 16);
        let export = ThemeFile::capture(&doc);
        assert!(matches!(
            export.light.elements["O"],
            ColorValue::Oklch { .. }
        ));
    }
}
