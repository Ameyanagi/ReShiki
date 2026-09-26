use reshiki::{
    canvas_theme::{self, CanvasTheme},
    document::{Document, Point},
    document_styles::Preset,
    exchange, export, scene,
};

fn sample() -> Document {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(carbon, oxygen, 2, "plain");
    doc
}

#[test]
fn theme_survives_native_storage_selection_and_printing_without_changing_style() {
    let mut doc = sample();
    let original = doc.clone();
    doc.canvas_theme = CanvasTheme::Dark;
    let saved = serde_json::to_vec(&doc).unwrap();
    let restored: Document = serde_json::from_slice(&saved).unwrap();
    assert_eq!(doc, restored);
    let selected = reshiki::editing::selection(&doc, &doc.all_ids());
    assert_eq!(selected.canvas_theme, CanvasTheme::Dark);
    let printed =
        reshiki::printing::snapshot(&doc, &[], reshiki::printing::Scope::Document).unwrap();
    assert_eq!(printed.canvas_theme, CanvasTheme::Dark);
    doc.canvas_theme = CanvasTheme::Light;
    assert_eq!(doc, original);
    assert!(
        !String::from_utf8(serde_json::to_vec(&doc).unwrap())
            .unwrap()
            .contains("canvas_theme")
    );
}

#[test]
fn editable_exchange_contains_white_bonds_and_a_black_background() {
    let mut doc = sample();
    doc.canvas_theme = CanvasTheme::Dark;
    let xml = exchange::drawing::write(&doc, Default::default()).unwrap();
    let cdx = exchange::to_cdx(&xml).unwrap();
    for text in [xml, exchange::from_cdx(&cdx).unwrap()] {
        let parsed = roxmltree::Document::parse(&text).unwrap();
        let colors: Vec<_> = parsed
            .descendants()
            .filter(|n| n.has_tag_name("color"))
            .collect();
        let rgb = |index: &str| {
            let node = colors[index.parse::<usize>().unwrap() - 2];
            ["r", "g", "b"].map(|attr| node.attribute(attr).unwrap().parse::<f32>().unwrap())
        };
        assert_eq!(
            rgb(parsed.root_element().attribute("bgcolor").unwrap()),
            [0.; 3]
        );
        let background = parsed
            .descendants()
            .find(|n| n.has_tag_name("graphic") && n.attribute("RectangleType") == Some("Filled"))
            .unwrap();
        assert_eq!(background.attribute("Z"), Some("1"));
        let page = parsed
            .descendants()
            .find(|n| n.has_tag_name("page"))
            .unwrap();
        assert_eq!(page.first_element_child(), Some(background));
        assert_eq!(rgb("2"), [1.; 3], "ChemDraw's reserved white entry");
        assert_eq!(rgb("3"), [0.; 3], "ChemDraw's reserved black entry");
        let mut ranks = std::collections::HashSet::new();
        for object in page
            .descendants()
            .filter(|n| n.is_element() && *n != background)
        {
            if let Some(z) = object.attribute("Z") {
                let z: usize = z.parse().unwrap();
                assert!(
                    z > 1 && ranks.insert(z),
                    "Every foreground object needs a distinct positive stacking rank"
                );
            }
        }
        assert_eq!(rgb(background.attribute("color").unwrap()), [0.; 3]);
        assert_eq!(
            parsed.descendants().filter(|n| n.has_tag_name("n")).count(),
            2
        );
        let bond = parsed.descendants().find(|n| n.has_tag_name("b")).unwrap();
        assert_eq!(rgb(bond.attribute("color").unwrap()), [1.; 3]);
        let label = parsed
            .descendants()
            .find(|n| n.has_tag_name("s") && n.text() == Some("O"))
            .unwrap();
        assert_eq!(rgb(label.attribute("color").unwrap()), [1.; 3]);
        assert_eq!(
            parsed
                .root_element()
                .attribute("LabelSize")
                .unwrap()
                .parse::<f32>()
                .unwrap(),
            10.
        );
    }
}

#[test]
fn pasting_across_canvas_modes_keeps_visible_colors_and_editable_atoms() {
    for source in CanvasTheme::ALL {
        let mut doc = sample();
        doc.canvas_theme = source;
        doc.bonds[0].color = [180, 50, 55];
        for target in CanvasTheme::ALL {
            let part = canvas_theme::for_paste(doc.clone(), target);
            part.validate().unwrap();
            assert_eq!(part.canvas_theme, target);
            assert_eq!(part.atoms.len(), doc.atoms.len());
            assert_eq!(part.bonds[0].order, 2);
            assert_eq!(part.drawing_style, doc.drawing_style);
            assert_eq!(
                target.color(part.bonds[0].color),
                source.color(doc.bonds[0].color)
            );
            let ink = part.atoms[1].text_style.as_ref().unwrap().color;
            assert_eq!(target.color(ink), source.color([0; 3]));
            assert!(part.atoms[1].display.color_override);
            assert_eq!(
                part.graphics, doc.graphics,
                "Copy adds no background object"
            );
        }
    }
}

#[test]
fn oxygen_glyph_is_centered_on_the_bond_axis_in_published_figures() {
    let mut styles: Vec<_> = Preset::ALL.into_iter().map(Preset::style).collect();
    let mut missing_font = styles[0].clone();
    missing_font.name = "Unavailable font".into();
    missing_font.font_family = "ReShiki deliberately unavailable test font".into();
    styles.push(missing_font);
    for drawing_style in styles {
        for bold in [false, true] {
            let mut doc = sample();
            doc.drawing_style = drawing_style.clone();
            let mut font = doc.drawing_style.text_style();
            font.bold = bold;
            doc.atoms[1].text_style = Some(font.clone());
            let svg = scene::svg(&doc);
            let xml = roxmltree::Document::parse(&svg).unwrap();
            let label = xml
                .descendants()
                .find(|node| node.has_tag_name("text") && node.text() == Some("O"))
                .unwrap();
            assert_eq!(
                label.attribute("font-family"),
                Some(reshiki::style::glyph_metrics('O', &font).0)
            );
            assert_eq!(doc.atoms[1].text_style.as_ref(), Some(&font));
            assert_eq!(doc.drawing_style, drawing_style);
            let view: Vec<f32> = xml
                .root_element()
                .attribute("viewBox")
                .unwrap()
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let scale = reshiki::style::DEFAULT.points_per_world() * 1200. / 72.;
            let oxygen = doc.atoms[1].position;
            let right_half = ((oxygen.x - view[0]) * scale).ceil() as u32;
            let expected_y = (oxygen.y - view[1]) * scale;
            let png = export::drawing(&doc, "png").unwrap();
            let raster = image::load_from_memory(&png).unwrap().into_rgba8();
            let ys: Vec<_> = raster
                .enumerate_pixels()
                .filter(|(x, _, p)| *x >= right_half && p[0] < 64)
                .map(|(_, y, _)| y)
                .collect();
            let top = *ys.iter().min().expect("oxygen ink");
            let bottom = *ys.iter().max().unwrap();
            let center = (top + bottom + 1) as f32 / 2.;
            assert!(
                (center - expected_y).abs() < 0.8,
                "{}, bold={bold}: O center {center}, bond axis {expected_y}",
                drawing_style.name
            );
        }
    }
}

#[test]
fn element_themes_match_scene_svg_png_and_editable_exchange_in_both_modes() {
    use canvas_theme::ColorTheme;
    for palette in ColorTheme::ALL {
        for mode in CanvasTheme::ALL {
            let mut doc = sample();
            doc.color_theme = palette;
            doc.canvas_theme = mode;
            let expected = palette.element_color("O", mode);
            let scene_ink = scene::primitives(&doc)
                .into_iter()
                .find_map(|p| match p {
                    scene::Primitive::Text { text, color, .. } if text == "O" => {
                        Some(mode.color(color))
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(scene_ink, expected);
            let svg = scene::svg(&doc);
            let svg_xml = roxmltree::Document::parse(&svg).unwrap();
            let label = svg_xml
                .descendants()
                .find(|n| n.has_tag_name("text") && n.text() == Some("O"))
                .unwrap();
            let [r, g, b] = expected;
            assert_eq!(
                label.attribute("fill"),
                Some(format!("rgb({r},{g},{b})").as_str())
            );
            let xml = exchange::drawing::write(&doc, Default::default()).unwrap();
            let binary = exchange::to_cdx(&xml).unwrap();
            let xml = exchange::from_cdx(&binary).unwrap();
            let xml = roxmltree::Document::parse(&xml).unwrap();
            let colors: Vec<_> = xml
                .descendants()
                .filter(|n| n.has_tag_name("color"))
                .collect();
            let label = xml
                .descendants()
                .find(|n| n.has_tag_name("s") && n.text() == Some("O"))
                .unwrap();
            let index = label.attribute("color").unwrap().parse::<usize>().unwrap() - 2;
            let actual = ["r", "g", "b"].map(|attr| {
                (colors[index]
                    .attribute(attr)
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
                    * 255.)
                    .round() as u8
            });
            assert_eq!(actual, expected);
            let png = export::clipboard_drawing(&doc, "png").unwrap();
            let image = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(
                image.get_pixel(0, 0).0[3],
                0,
                "transparent clipboard in {palette}/{mode}"
            );
            assert!(
                image.pixels().any(|p| p.0 == [r, g, b, 255]),
                "visible theme ink in {palette}/{mode}"
            );
            let opaque = image::load_from_memory(&export::drawing(&doc, "png").unwrap())
                .unwrap()
                .to_rgba8();
            let [r, g, b] = mode.background();
            assert_eq!(opaque.get_pixel(0, 0).0, [r, g, b, 255]);
            let saved: Document =
                serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
            assert_eq!(doc, saved);
            for target in CanvasTheme::ALL {
                let mut pasted = canvas_theme::for_paste(doc.clone(), target);
                // A destination palette must not change pasted visible ink.
                pasted.color_theme = ColorTheme::Pastel;
                assert_eq!(
                    target.color(canvas_theme::atom_color(&pasted, &pasted.atoms[1])),
                    expected
                );
                assert_eq!(pasted.atoms[1].position, doc.atoms[1].position);
            }
        }
    }
}

#[test]
fn theme_changes_reset_atom_overrides_without_changing_geometry_or_fonts() {
    use canvas_theme::ColorTheme;
    let mut doc = sample();
    doc.atoms[1].text_style = Some(doc.drawing_style.text_style());
    let font = doc.atoms[1].text_style.as_mut().unwrap();
    font.bold = true;
    font.size_pt = 14.;
    font.color = [32, 80, 145];
    let original = doc.clone();
    ColorTheme::Presentation.apply(&mut doc);
    assert_eq!(doc.drawing_style, original.drawing_style);
    assert_eq!(doc.bonds, original.bonds);
    assert!(doc.atoms[1].text_style.as_ref().unwrap().bold);
    assert_eq!(doc.atoms[1].text_style.as_ref().unwrap().size_pt, 14.);
    assert_eq!(
        canvas_theme::atom_color(&doc, &doc.atoms[1]),
        ColorTheme::Presentation.element_color("O", CanvasTheme::Light)
    );
    doc.atoms[1].display.color_override = true;
    assert_eq!(
        canvas_theme::atom_color(&doc, &doc.atoms[1]),
        [0; 3],
        "explicit neutral ink overrides a palette"
    );
    ColorTheme::Pastel.apply(&mut doc);
    assert!(!doc.atoms[1].display.color_override);
    ColorTheme::Publication.apply(&mut doc);
    assert_eq!(canvas_theme::atom_color(&doc, &doc.atoms[1]), [0; 3]);
    assert_eq!(doc.atoms[1].position, original.atoms[1].position);
}

#[test]
fn element_palettes_keep_legible_contrast_on_their_canvas() {
    use canvas_theme::ColorTheme;
    let luminance = |rgb: [u8; 3]| {
        let linear = rgb.map(|c| {
            let v = f64::from(c) / 255.;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    };
    for theme in ColorTheme::ALL {
        for mode in CanvasTheme::ALL {
            for element in reshiki::editing::ELEMENTS {
                let ink = luminance(theme.element_color(element, mode));
                let paper = luminance(mode.background());
                let contrast = (ink.max(paper) + 0.05) / (ink.min(paper) + 0.05);
                assert!(contrast >= 3., "{theme}/{mode}/{element}: {contrast}");
            }
        }
    }
}

#[test]
fn ring_highlights_have_their_own_palette_and_copy_the_visible_color() {
    use reshiki::{canvas_theme::ColorTheme, ring_fills};
    for mode in CanvasTheme::ALL {
        for (name, key) in ring_fills::PALETTE {
            let mut doc = reshiki::rings::Preset::Regular.document(42., false);
            doc.canvas_theme = mode;
            doc.color_theme = ColorTheme::Presentation;
            let ids = doc.all_ids();
            ring_fills::apply(&mut doc, &ids, Some(key));
            let expected = ring_fills::palette_color(key, mode);
            let actual = scene::primitives(&doc)
                .into_iter()
                .find_map(|p| match p {
                    scene::Primitive::Path {
                        filled: true,
                        style,
                        ..
                    } => style.fill.map(|c| mode.color(c)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(actual, expected, "{name}/{mode}");
            let resolved = canvas_theme::resolved_document(&doc).into_owned();
            assert_eq!(
                canvas_theme::resolved_document(&resolved).as_ref(),
                &resolved,
                "colors resolve once"
            );
            for target in CanvasTheme::ALL {
                let pasted = canvas_theme::for_paste(doc.clone(), target);
                assert_eq!(pasted.ring_fills[0].visible_color(target), expected);
                assert!(pasted.ring_fills[0].fixed_color);
            }
            let png = image::load_from_memory(&export::clipboard_drawing(&doc, "png").unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(png.get_pixel(0, 0).0[3], 0);
            let [r, g, b] = expected;
            assert!(png.pixels().any(|p| p.0 == [r, g, b, 255]));
            let saved: Document =
                serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
            assert_eq!(saved, doc);
            assert_eq!(
                doc.ring_fills[0].color, key,
                "saved palette keys remain stable"
            );
        }
    }
}

#[test]
fn dark_ring_highlights_keep_automatic_atom_labels_legible() {
    use reshiki::{canvas_theme::ColorTheme, ring_fills};
    let luminance = |rgb: [u8; 3]| -> f64 {
        rgb.into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(v, w)| {
                let v = f64::from(v) / 255.;
                w * if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            })
            .sum()
    };
    for (_, key) in ring_fills::PALETTE {
        let mut doc = reshiki::rings::Preset::Regular.document(42., false);
        doc.canvas_theme = CanvasTheme::Dark;
        doc.color_theme = ColorTheme::Presentation;
        doc.atoms[0].element = "N".into();
        let ids = doc.all_ids();
        ring_fills::apply(&mut doc, &ids, Some(key));
        let original = doc.clone();
        let fill = ring_fills::palette_color(key, CanvasTheme::Dark);
        let ink = CanvasTheme::Dark.color(canvas_theme::atom_color(&doc, &doc.atoms[0]));
        assert!((luminance(ink) + 0.05) / (luminance(fill) + 0.05) >= 4.5);
        assert!(
            1.05 / (luminance(fill) + 0.05) >= 5.,
            "white bonds remain distinct"
        );
        let resolved = canvas_theme::resolved_document(&doc).into_owned();
        assert_eq!(
            CanvasTheme::Dark.color(resolved.atoms[0].text_style.as_ref().unwrap().color),
            ink
        );
        let pasted = canvas_theme::for_paste(doc.clone(), CanvasTheme::Light);
        assert_eq!(canvas_theme::atom_color(&pasted, &pasted.atoms[0]), ink);
        assert_eq!(doc, original);
        doc.canvas_theme = CanvasTheme::Light;
        assert_eq!(
            canvas_theme::atom_color(&doc, &doc.atoms[0]),
            ColorTheme::Presentation.element_color("N", CanvasTheme::Light)
        );
        doc.canvas_theme = CanvasTheme::Dark;
        doc.atoms[0].display.color_override = true;
        assert_eq!(canvas_theme::atom_color(&doc, &doc.atoms[0]), [0; 3]);
    }
}
