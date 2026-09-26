use reshiki::{
    canvas_theme::{self, CanvasTheme, ColorTheme},
    color_contrast::{Oklch, TEXT_MIN, TEXT_TARGET, contrast},
    document::Document,
    theme_files::{self, ThemeFile},
    theme_generator::{Recipe, Reference, Tone},
};

#[test]
fn starting_recipes_reproduce_presentation_and_pastel_in_both_modes() {
    let template = ThemeFile::capture(&Document::default());
    for (recipe, base) in [
        (Recipe::PRESENTATION, ColorTheme::Presentation),
        (Recipe::PASTEL, ColorTheme::Pastel),
    ] {
        let theme = recipe.generate(&template).unwrap();
        for mode in CanvasTheme::ALL {
            assert_eq!(theme.palette(mode).elements.len(), 118);
            assert_eq!(theme.palette(mode).tile_seeds.len(), 109);
            for &element in reshiki::editing::ELEMENTS {
                assert_eq!(
                    theme.element_color(element, mode),
                    base.element_color(element, mode)
                );
                assert_eq!(
                    theme.element_swatch(element, mode),
                    base.element_swatch(element, mode)
                );
            }
        }
    }
    assert_eq!(
        Recipe::PRESENTATION.light,
        Tone {
            lightness: 0.50,
            chroma: 2.
        }
    );
    assert_eq!(
        Recipe::PRESENTATION.dark,
        Tone {
            lightness: 0.70,
            chroma: 1.
        }
    );
}

#[test]
fn presentation_default_contrast_is_readable_for_every_element() {
    let theme = Recipe::PRESENTATION
        .generate(&ThemeFile::capture(&Document::default()))
        .unwrap();
    for mode in CanvasTheme::ALL {
        let mut min = 21_f64;
        let mut max_colored = 1_f64;
        for &element in reshiki::editing::ELEMENTS {
            let color = theme.element_color(element, mode);
            let ratio = contrast(color, mode.background());
            assert!(ratio >= TEXT_TARGET, "{element}/{mode}: {ratio}");
            min = min.min(ratio);
            if color != [0; 3] && color != [255; 3] {
                max_colored = max_colored.max(ratio);
            }
        }
        eprintln!(
            "Presentation {mode}: label contrast {min:.4}–{max_colored:.4}:1 (neutral labels 21:1)"
        );
        for element in ["N", "O", "S", "Cl"] {
            let color = theme.element_color(element, mode);
            eprintln!(
                "{element}: {color:?} {:.4}:1",
                contrast(color, mode.background())
            );
        }
    }
}

#[test]
fn generated_palettes_survive_extreme_controls_and_retain_readable_labels() {
    let template = ThemeFile::capture(&Document::default());
    for lightness in [0., 0.5, 1.] {
        for chroma in [0., 1., 1.5, 2.] {
            let tone = Tone { lightness, chroma };
            let recipe = Recipe {
                light: tone,
                dark: tone,
                reference: None,
            };
            let theme = recipe.generate(&template).unwrap();
            for mode in CanvasTheme::ALL {
                let mut doc = reshiki::rings::Preset::Regular.document(42., false);
                doc.canvas_theme = mode;
                theme.clone().apply(&mut doc).unwrap();
                for (_, key) in reshiki::ring_fills::PALETTE {
                    let ids = doc.all_ids();
                    reshiki::ring_fills::apply(&mut doc, &ids, Some(key));
                    for &element in reshiki::editing::ELEMENTS {
                        assert!(
                            contrast(theme.element_color(element, mode), mode.background())
                                >= TEXT_TARGET
                        );
                        doc.atoms[0].element = element.into();
                        let ink = mode.color(canvas_theme::atom_color(&doc, &doc.atoms[0]));
                        let bg = canvas_theme::fill_color(&doc, &doc.ring_fills[0]);
                        assert!(
                            contrast(ink, bg) >= TEXT_MIN,
                            "{lightness}/{chroma}/{mode}/{element}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn recipes_round_trip_but_explicit_palette_values_remain_authoritative() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("generated.reshiki-theme");
    let template = theme_files::bundled().unwrap().remove(0);
    let recipe = Recipe {
        light: Tone {
            lightness: 0.43,
            chroma: 1.5,
        },
        dark: Tone {
            lightness: 0.76,
            chroma: 2.,
        },
        reference: None,
    };
    let theme = recipe.generate(&template).unwrap();
    assert_eq!(theme.light.ring_fills, template.light.ring_fills);
    assert_eq!(theme.dark.ring_fills, template.dark.ring_fills);
    theme_files::save(&path, &theme).unwrap();
    let loaded = theme_files::load(&path).unwrap();
    assert_eq!(loaded.generator, Some(recipe.clone()));
    let mut doc = Document::default();
    loaded.apply(&mut doc).unwrap();
    let reopened: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(reopened, doc);
    assert_eq!(
        ThemeFile::capture(&reopened).generator,
        Some(recipe.clone())
    );
    // A file author may edit a generated color by hand; loading must not erase it.
    let mut edited = theme;
    edited
        .light
        .elements
        .insert("N".into(), theme_files::ColorValue::Rgb([23, 40, 80]));
    theme_files::save(&path, &edited).unwrap();
    assert_eq!(
        theme_files::load(&path)
            .unwrap()
            .element_color("N", CanvasTheme::Light),
        [23, 40, 80]
    );
    for value in [f64::NAN, f64::INFINITY, -0.1, 2.01] {
        let mut invalid = recipe.clone();
        invalid.dark.chroma = value;
        assert!(invalid.generate(&template).is_err());
        let mut invalid_file = edited.clone();
        invalid_file.generator = Some(invalid);
        assert!(invalid_file.validate().is_err());
    }
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.01] {
        let mut invalid = recipe.clone();
        invalid.light.lightness = value;
        assert!(invalid.generate(&template).is_err());
    }
}

#[test]
fn boosted_chroma_exceeds_the_reference_without_changing_target_lightness_or_hue() {
    let template = ThemeFile::capture(&Document::default());
    let source = Oklch::from_rgb([255, 181, 181]); // Jmol boron: muted enough to boost in sRGB.
    for factor in [1., 1.5, 2.] {
        let tone = Tone {
            lightness: 0.52,
            chroma: factor,
        };
        let theme = Recipe {
            light: tone,
            dark: tone,
            reference: None,
        }
        .generate(&template)
        .unwrap();
        for mode in CanvasTheme::ALL {
            let actual = Oklch::from_rgb(theme.element_swatch("B", mode).unwrap());
            assert!((actual.c - source.c * factor).abs() < 0.003);
            assert!((actual.l - tone.lightness).abs() < 0.003);
            assert!((actual.h - source.h).abs() < 0.025);
        }
    }
}

#[test]
fn reference_generation_preserves_distinct_label_and_tile_hues() {
    let mut source = theme_files::bundled().unwrap().remove(0);
    source.dark.tile_seeds.insert(
        "N".into(),
        theme_files::ColorValue::Oklch {
            oklch: [0.72, 0.10, 145.],
        },
    );
    let recipe = Recipe {
        reference: Some(Reference::capture(&source)),
        ..Recipe::PASTEL
    };
    let generated = recipe.generate(&source).unwrap();
    for mode in CanvasTheme::ALL {
        for element in ["N", "O", "Cl"] {
            let label = source
                .palette(mode)
                .elements
                .get(element)
                .map(|v| v.rgb())
                .unwrap_or_else(|| source.base.element_color(element, mode));
            let tile = source.element_swatch(element, mode).unwrap();
            let check_tone = |original, actual| {
                let original = Oklch::from_rgb(original);
                let actual = Oklch::from_rgb(actual);
                assert!(
                    (actual.h - original.h).abs() < 0.035,
                    "{mode}/{element}: hue changed"
                );
                assert!((actual.c - original.c * recipe.tone(mode).chroma).abs() < 0.003);
                assert!((actual.l - recipe.tone(mode).lightness).abs() < 0.003);
            };
            check_tone(label, generated.palette(mode).elements[element].rgb());
            check_tone(tile, generated.element_swatch(element, mode).unwrap());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reference.reshiki-theme");
    theme_files::save(&path, &generated).unwrap();
    let loaded = theme_files::load(&path).unwrap();
    assert_eq!(loaded.generator, Some(recipe.clone()));
    assert_eq!(
        loaded
            .generator
            .as_ref()
            .unwrap()
            .generate(&loaded)
            .unwrap(),
        generated
    );
    let mut legacy = serde_json::to_value(&recipe).unwrap();
    let reference = legacy["reference"].as_object_mut().unwrap();
    reference.remove("light_tile_seeds");
    reference.remove("dark_tile_seeds");
    let legacy: Recipe = serde_json::from_value(legacy).unwrap();
    let old = legacy.generate(&source).unwrap();
    for mode in CanvasTheme::ALL {
        assert_eq!(
            old.palette(mode).tile_seeds["N"],
            old.palette(mode).elements["N"]
        );
    }
    let mut invalid = recipe.clone();
    invalid
        .reference
        .as_mut()
        .unwrap()
        .light_tile_seeds
        .insert("Xx".into(), theme_files::ColorValue::Rgb([0; 3]));
    assert!(invalid.validate().is_err());
    let mut invalid = recipe;
    invalid.reference.as_mut().unwrap().dark_tile_seeds.insert(
        "N".into(),
        theme_files::ColorValue::Oklch {
            oklch: [0.5, -1., 10.],
        },
    );
    assert!(invalid.validate().is_err());
}

#[test]
fn references_use_each_modes_own_colors_and_survive_portable_roundtrip() {
    let mut source = ThemeFile::capture(&Document::default());
    source.id = "test-reference".into();
    source.name = "Test reference".into();
    source
        .light
        .elements
        .insert("B".into(), theme_files::ColorValue::Rgb([180, 100, 110]));
    source
        .dark
        .elements
        .insert("B".into(), theme_files::ColorValue::Rgb([80, 170, 130]));
    let recipe = Recipe {
        reference: Some(Reference::capture(&source)),
        ..Recipe::PASTEL
    };
    let generated = recipe.generate(&source).unwrap();
    for mode in CanvasTheme::ALL {
        let source_color = source.palette(mode).elements["B"].rgb();
        let original = Oklch::from_rgb(source_color);
        let actual = Oklch::from_rgb(generated.element_swatch("B", mode).unwrap());
        assert!((original.h - actual.h).abs() < 0.03);
        assert!((original.c * recipe.tone(mode).chroma - actual.c).abs() < 0.003);
        assert!((recipe.tone(mode).lightness - actual.l).abs() < 0.003);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reference.reshiki-theme");
    theme_files::save(&path, &generated).unwrap();
    let loaded = theme_files::load(&path).unwrap();
    assert_eq!(loaded.generator, Some(recipe.clone()));
    assert_eq!(
        loaded
            .generator
            .as_ref()
            .unwrap()
            .generate(&loaded)
            .unwrap(),
        loaded
    );
    let mut invalid = recipe;
    invalid.reference.as_mut().unwrap().dark.remove("B");
    assert!(invalid.validate().is_err());
    let old: Recipe = serde_json::from_str(
        r#"{"light":{"lightness":0.5,"chroma":2},"dark":{"lightness":0.7,"chroma":1}}"#,
    )
    .unwrap();
    assert_eq!(old, Recipe::PRESENTATION);
}
