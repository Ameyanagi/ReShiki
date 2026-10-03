use reshiki::{
    canvas_theme::{self, CanvasTheme},
    document::{Document, Point},
    document_styles::Preset,
    exchange, export,
    palette::{Color, Hue, Palette, Row},
    scene,
};

fn sample() -> Document {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(carbon, oxygen, 2, "plain");
    doc
}

#[test]
fn attached_hydrogens_follow_h_palette_in_figures_clipboard_and_chemdraw() {
    use reshiki::canvas_theme::ColorTheme;
    let mut doc = Document::default();
    let c = doc.add_atom("C", Point::default());
    let n = doc.add_atom("N", Point::new(42., 0.));
    doc.add_bond(c, n, 1, "plain");
    doc.atom_mut(n).unwrap().label_h = 2;
    for theme in [
        ColorTheme::Presentation,
        ColorTheme::Pastel,
        ColorTheme::Jmol,
    ] {
        theme.apply(&mut doc);
        for mode in CanvasTheme::ALL {
            doc.canvas_theme = mode;
            let h_color = theme.element_color("H", mode);
            let n_color = theme.element_color("N", mode);
            let verify = |drawing: &Document| {
                let colors: Vec<_> = scene::primitives(drawing)
                    .into_iter()
                    .filter_map(|p| match p {
                        scene::Primitive::Text { text, color, .. } => {
                            Some((text, drawing.canvas_theme.color(color)))
                        }
                        _ => None,
                    })
                    .collect();
                for symbol in ["H", "2"] {
                    assert!(
                        colors.iter().any(|(s, rgb)| s == symbol && *rgb == h_color),
                        "{theme}/{mode}/{symbol}: {colors:?}"
                    );
                }
                assert!(colors.iter().any(|(s, rgb)| s == "N" && *rgb == n_color));
            };
            verify(&doc);
            verify(&canvas_theme::for_paste(doc.clone(), mode.toggled()));
            let xml = exchange::drawing::write(&doc, Default::default()).unwrap();
            for xml in [
                xml.clone(),
                exchange::from_cdx(&exchange::to_cdx(&xml).unwrap()).unwrap(),
            ] {
                let imported = reshiki::chemistry::cdxml::import_cdxml(&xml)
                    .unwrap()
                    .document;
                verify(&imported);
                assert_eq!(
                    imported
                        .atoms
                        .iter()
                        .find(|a| a.element == "N")
                        .unwrap()
                        .label_h,
                    2
                );
            }
        }
    }
    doc.atom_mut(n).unwrap().display.color_override = true;
    doc.atom_mut(n).unwrap().text_style = Some(reshiki::typography::TextStyle {
        color: Color::Custom([130, 30, 100]),
        ..Default::default()
    });
    let atom = doc.atom(n).unwrap();
    assert_eq!(
        canvas_theme::hydrogen_color(&doc, atom),
        canvas_theme::atom_color(&doc, atom)
    );
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
fn foreign_pastes_across_canvas_modes_keep_visible_colors_and_editable_atoms() {
    for source in CanvasTheme::ALL {
        let mut doc = sample();
        doc.canvas_theme = source;
        doc.bonds[0].color = Color::Palette(Hue::Red, Row::Strong);
        let shown = Palette::of(&doc).rgb(doc.bonds[0].color);
        for target in CanvasTheme::ALL {
            let part = canvas_theme::for_paste(doc.clone(), target);
            part.validate().unwrap();
            assert_eq!(part.canvas_theme, target);
            assert_eq!(part.atoms.len(), doc.atoms.len());
            assert_eq!(part.bonds[0].order, 2);
            assert_eq!(part.drawing_style, doc.drawing_style);
            // Pasted colors are custom, so they look the same on either canvas.
            assert_eq!(part.bonds[0].color, Color::Custom(shown));
            // Ink stays Ink on the same canvas, so it follows later canvas changes.
            let ink = part.atoms[1].text_style.as_ref().unwrap().color;
            let expected = if source == target {
                Color::Ink
            } else {
                Color::Custom(source.color([0; 3]))
            };
            assert_eq!(ink, expected);
            assert!(part.atoms[1].display.color_override);
            assert_eq!(
                part.graphics, doc.graphics,
                "Copy adds no background object"
            );
        }
    }
}

/// A colored ring with a palette bond, a custom bond, a palette ring fill and
/// automatic atom colors, on the light Publication canvas.
fn palette_sample() -> Document {
    let mut doc = reshiki::rings::Preset::Regular.document(42., false);
    doc.atoms[0].element = "N".into();
    doc.bonds[0].color = Color::Palette(Hue::Red, Row::Strong);
    doc.bonds[1].color = Color::Custom([12, 34, 56]);
    let ids = doc.all_ids();
    reshiki::ring_fills::apply(&mut doc, &ids, Some(Color::Palette(Hue::Blue, Row::Tint)));
    doc
}

/// Colors of the copy appended after the first `from` atoms and bonds (one ring),
/// with the atom ink the drawing shows.
fn appended_colors(doc: &Document, from: usize) -> (Vec<Color>, Vec<Color>, Vec<[u8; 3]>) {
    (
        doc.bonds[from..].iter().map(|b| b.color).collect(),
        doc.ring_fills[1..].iter().map(|f| f.color).collect(),
        doc.atoms[from..]
            .iter()
            .map(|a| canvas_theme::atom_color(doc, a))
            .collect(),
    )
}

#[test]
fn native_pastes_keep_palette_colors_like_duplicates() {
    use reshiki::editing;
    let source = palette_sample();
    let part = editing::selection(&source, &source.all_ids());
    let offset = Point::new(24., 24.);
    let mut duplicated = source.clone();
    assert!(!editing::append(&mut duplicated, &part, offset).is_empty());
    let mut pasted = source.clone();
    let native = canvas_theme::for_native_paste(part.clone(), source.canvas_theme);
    assert!(!editing::append(&mut pasted, &native, offset).is_empty());
    let count = source.bonds.len();
    assert_eq!(
        appended_colors(&pasted, count),
        appended_colors(&duplicated, count)
    );
    assert_eq!(
        pasted.bonds[count].color,
        Color::Palette(Hue::Red, Row::Strong)
    );
    assert_eq!(
        pasted.ring_fills[1].color,
        Color::Palette(Hue::Blue, Row::Tint)
    );
    assert!(
        pasted.atoms[count..]
            .iter()
            .all(|a| !a.display.color_override)
    );
}

#[test]
fn native_pastes_follow_the_target_theme_canvas_and_hues() {
    use reshiki::{canvas_theme::ColorTheme, editing, palette::Hues};
    let source = palette_sample();
    let part = editing::selection(&source, &source.all_ids());
    let mut target = Document {
        canvas_theme: CanvasTheme::Dark,
        ..Default::default()
    };
    ColorTheme::Presentation.apply(&mut target);
    let mut hues = Hues::default();
    hues.set(Hue::Red, 5);
    reshiki::palette::set_hues(&mut target, hues);
    let native = canvas_theme::for_native_paste(part, target.canvas_theme);
    let ids = editing::append(&mut target, &native, Point::default());
    assert_eq!(ids.len(), source.all_ids().len());
    target.validate().unwrap();
    let palette = Palette::of(&target);
    let red = target.bonds[0].color;
    assert_eq!(red, Color::Palette(Hue::Red, Row::Strong));
    assert_eq!(
        palette.rgb(red),
        Palette::new(
            ColorTheme::Presentation.tones(CanvasTheme::Dark),
            hues,
            CanvasTheme::Dark
        )
        .swatch(Hue::Red, Row::Strong)
    );
    assert_ne!(palette.rgb(red), Palette::of(&source).rgb(red));
    assert_eq!(target.bonds[1].color, Color::Custom([12, 34, 56]));
    assert_eq!(palette.rgb(target.bonds[1].color), [12, 34, 56]);
    assert_eq!(target.bonds[2].color, Color::Ink);
    assert_eq!(palette.rgb(Color::Ink), [255; 3]);
    assert_eq!(
        target.ring_fills[0].color,
        Color::Palette(Hue::Blue, Row::Tint)
    );
    // Automatic atom colors come from the target's theme and canvas.
    let mut plain = target.clone();
    plain.ring_fills.clear();
    let nitrogen = plain.atoms.iter().find(|a| a.element == "N").unwrap();
    assert!(!nitrogen.display.color_override);
    let shown = CanvasTheme::Dark.color(canvas_theme::atom_color(&plain, nitrogen));
    assert_eq!(
        shown,
        canvas_theme::element_color(&plain, "N", CanvasTheme::Dark)
    );
    assert_ne!(shown, canvas_theme::atom_color(&source, &source.atoms[0]));
    let foreign = canvas_theme::for_paste(
        editing::selection(&source, &source.all_ids()),
        CanvasTheme::Dark,
    );
    assert!(foreign.atoms.iter().all(|a| a.display.color_override));
    assert!(!reshiki::palette::any_color(&foreign, |c| matches!(
        c,
        Color::Palette(..)
    )));
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
    font.color = Color::Palette(Hue::Blue, Row::Strong);
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
                assert!(
                    contrast >= reshiki::color_contrast::TEXT_TARGET,
                    "{theme}/{mode}/{element}: {contrast}"
                );
            }
        }
    }
}

#[test]
fn ring_highlights_use_the_tint_row_and_copy_the_visible_color() {
    use reshiki::{canvas_theme::ColorTheme, ring_fills};
    for mode in CanvasTheme::ALL {
        for hue in Hue::ALL {
            let color = Color::Palette(hue, Row::Tint);
            let mut doc = reshiki::rings::Preset::Regular.document(42., false);
            doc.canvas_theme = mode;
            doc.color_theme = ColorTheme::Presentation;
            let ids = doc.all_ids();
            ring_fills::apply(&mut doc, &ids, Some(color));
            let expected = Palette::new(
                ColorTheme::Presentation.tones(mode),
                Default::default(),
                mode,
            )
            .swatch(hue, Row::Tint);
            let actual = scene::primitives(&doc)
                .into_iter()
                .find_map(|p| match p {
                    scene::Primitive::Path {
                        filled: true,
                        style,
                        ..
                    } => style.fill.map(|c| mode.color(c.rgb())),
                    _ => None,
                })
                .unwrap();
            assert_eq!(actual, expected, "{hue:?}/{mode}");
            let resolved = canvas_theme::resolved_document(&doc).into_owned();
            assert_eq!(resolved.ring_fills[0].color, Color::Custom(expected));
            assert_eq!(
                canvas_theme::resolved_document(&resolved).as_ref(),
                &resolved,
                "colors resolve once"
            );
            for target in CanvasTheme::ALL {
                let pasted = canvas_theme::for_paste(doc.clone(), target);
                assert_eq!(pasted.ring_fills[0].color, Color::Custom(expected));
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
                doc.ring_fills[0].color, color,
                "saved palette references remain stable"
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
    for hue in Hue::ALL {
        let color = Color::Palette(hue, Row::Tint);
        let mut doc = reshiki::rings::Preset::Regular.document(42., false);
        doc.canvas_theme = CanvasTheme::Dark;
        doc.color_theme = ColorTheme::Presentation;
        doc.atoms[0].element = "N".into();
        let ids = doc.all_ids();
        ring_fills::apply(&mut doc, &ids, Some(color));
        let original = doc.clone();
        let fill = Palette::of(&doc).rgb(color);
        let ink = CanvasTheme::Dark.color(canvas_theme::atom_color(&doc, &doc.atoms[0]));
        assert!((luminance(ink) + 0.05) / (luminance(fill) + 0.05) >= 4.5);
        assert!(
            1.05 / (luminance(fill) + 0.05) >= 5.,
            "white bonds remain distinct"
        );
        let resolved = canvas_theme::resolved_document(&doc).into_owned();
        assert_eq!(
            Palette::of(&resolved).rgb(resolved.atoms[0].text_style.as_ref().unwrap().color),
            ink
        );
        let pasted = canvas_theme::for_paste(doc.clone(), CanvasTheme::Light);
        assert_eq!(canvas_theme::atom_color(&pasted, &pasted.atoms[0]), ink);
        assert_eq!(doc, original);
        doc.canvas_theme = CanvasTheme::Light;
        assert!(
            reshiki::color_contrast::contrast(
                canvas_theme::atom_color(&doc, &doc.atoms[0]),
                Palette::of(&doc).rgb(color)
            ) >= reshiki::color_contrast::TEXT_TARGET
        );
        doc.canvas_theme = CanvasTheme::Dark;
        doc.atoms[0].display.color_override = true;
        assert_eq!(canvas_theme::atom_color(&doc, &doc.atoms[0]), [0; 3]);
    }
}

#[test]
fn all_elements_meet_text_target_over_every_builtin_fill_and_overlaps() {
    use reshiki::{
        color_contrast::{TEXT_TARGET, contrast},
        ring_fills::RingFill,
    };
    let mut measured = 0;
    let mut minimum = 21_f64;
    for theme in canvas_theme::ColorTheme::ALL {
        for mode in CanvasTheme::ALL {
            for element in reshiki::editing::ELEMENTS {
                let mut doc = Document {
                    color_theme: theme,
                    canvas_theme: mode,
                    ..Default::default()
                };
                let id = doc.add_atom(element, Point::default());
                let palette = Palette::of(&doc);
                for hue in Hue::ALL {
                    doc.ring_fills.push(RingFill {
                        atoms: vec![id],
                        color: Color::Palette(hue, Row::Tint),
                    });
                    let ink = mode.color(canvas_theme::atom_color(&doc, &doc.atoms[0]));
                    for background in std::iter::once(mode.background())
                        .chain(doc.ring_fills.iter().map(|f| palette.rgb(f.color)))
                    {
                        let ratio = contrast(ink, background);
                        minimum = minimum.min(ratio);
                        measured += 1;
                        assert!(
                            ratio >= TEXT_TARGET,
                            "{theme}/{mode}/{element}: {ink:?} on {background:?} = {ratio}"
                        );
                    }
                    assert!(canvas_theme::label_contrast_issues(&doc).is_empty());
                    let resolved = canvas_theme::resolved_document(&doc).into_owned();
                    assert_eq!(
                        mode.color(canvas_theme::atom_color(&resolved, &resolved.atoms[0])),
                        ink
                    );
                }
            }
        }
    }
    eprintln!("Canvas contrast: {measured} paper/fill/overlap pairs, minimum {minimum:.4}:1");
}

#[test]
fn custom_contrast_conflicts_are_reported_without_recoloring_user_ink_or_fills() {
    use reshiki::{canvas_theme::ColorTheme, ring_fills::RingFill};
    let mut doc = sample();
    doc.color_theme = ColorTheme::Presentation;
    let id = doc.atoms[1].id;
    doc.ring_fills = vec![
        RingFill {
            atoms: vec![id],
            color: Color::Custom([0; 3]),
        },
        RingFill {
            atoms: vec![id],
            color: Color::Custom([120; 3]),
        },
    ];
    let before = doc.clone();
    assert!(canvas_theme::label_contrast_issues(&doc).contains(&id));
    assert_eq!(doc, before);
    doc.atoms[1].text_style = Some(doc.drawing_style.text_style());
    doc.atoms[1].text_style.as_mut().unwrap().color = Color::Custom([255, 255, 0]);
    doc.atoms[1].display.color_override = true;
    assert_eq!(canvas_theme::atom_color(&doc, &doc.atoms[1]), [255, 255, 0]);
    assert!(canvas_theme::label_contrast_issues(&doc).contains(&id));
    let resolved = canvas_theme::resolved_document(&doc).into_owned();
    assert_eq!(resolved.ring_fills, doc.ring_fills);
    assert_eq!(resolved.atoms[1].text_style, doc.atoms[1].text_style);
}

#[test]
fn old_light_canvas_documents_keep_their_chemdraw_output() {
    // Written by the previous release: legacy byte colors, black bonds and
    // custom label, bond, caption and arrow colors on the light canvas.
    let doc: Document =
        serde_json::from_str(include_str!("fixtures/palette/legacy-light.rsk")).unwrap();
    assert_eq!(doc.bonds[0].color, Color::Ink);
    assert_eq!(doc.bonds[1].color, Color::Custom([10, 120, 200]));
    // Preserve the historical fixture. The writer now places decimals above
    // channel boundaries so native ChemDraw truncation keeps 150, 20 and 100
    // rather than 149, 19 and 99. Only these three spellings may change; every
    // other byte must match. All 256 channel boundaries are tested separately.
    let expected = include_str!("fixtures/palette/legacy-light.cdxml")
        .replace(
            r#"<color r="0.58823529" g="0.15686275" b="0.07843137" />"#,
            r#"<color r="0.58823530" g="0.15686275" b="0.07843138" />"#,
        )
        .replace(
            r#"<color r="0.78431373" g="0.39215686" b="0.00000000" />"#,
            r#"<color r="0.78431373" g="0.39215687" b="0.00000000" />"#,
        );
    assert_eq!(
        exchange::drawing::write(&doc, Default::default()).unwrap(),
        expected
    );
    // Saving writes the current form, which reads back to the same drawing.
    let saved: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(saved, doc);
}

#[test]
fn old_dark_canvas_documents_keep_their_appearance_once() {
    // Written and rendered by the previous release on the dark canvas, which
    // showed custom bond, label, caption, arrow, rectangle and ring-fill colors
    // lightness-flipped.
    let bytes = include_bytes!("fixtures/palette/legacy-dark.rsk");
    let colors = |svg: &str| -> Vec<String> {
        svg.split("rgb(")
            .skip(1)
            .filter_map(|s| s.split(')').next().map(str::to_owned))
            .collect()
    };
    let expected = colors(include_str!("fixtures/palette/legacy-dark.svg"));
    let doc = Document::from_json(bytes).unwrap();
    assert_eq!(colors(&scene::svg_with_background(&doc)), expected);
    assert_eq!(doc.bonds[1].color, Color::Custom([55, 165, 245]));
    assert_eq!(doc.bonds[0].color, Color::Ink);
    // Saving, copying or a recovery draft marks the current version, so reading
    // it back does not flip the colors again.
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened, doc.current());
    assert_eq!(colors(&scene::svg_with_background(&reopened)), expected);
    // Only old drawings on the dark canvas convert.
    let plain: Document = serde_json::from_slice(bytes).unwrap();
    let mut current = serde_json::to_value(&plain).unwrap();
    current["version"] = 17.into();
    let current = serde_json::to_vec(&current).unwrap();
    assert_eq!(Document::from_json(&current).unwrap().bonds, plain.bonds);
    let light = include_bytes!("fixtures/palette/legacy-light.rsk");
    assert_eq!(
        Document::from_json(light).unwrap(),
        serde_json::from_slice::<Document>(light).unwrap()
    );
}

#[test]
fn legacy_swatches_become_palette_colors_and_follow_the_theme() {
    let mut doc: Document = serde_json::from_value(serde_json::json!({
        "version": 15,
        "atoms": [
            {"id": 1, "element": "C", "position": {"x": 0.0, "y": 0.0}},
            {"id": 2, "element": "C", "position": {"x": 42.0, "y": 0.0}}
        ],
        "bonds": [{"a": 1, "b": 2, "order": 1, "color": [32, 80, 145]}]
    }))
    .unwrap();
    assert_eq!(doc.bonds[0].color, Color::Palette(Hue::Blue, Row::Strong));
    let blue = |doc: &Document| {
        scene::primitives(doc)
            .into_iter()
            .find_map(|p| match p {
                scene::Primitive::Path { style, .. } => {
                    Some(doc.canvas_theme.color(style.stroke.rgb()))
                }
                _ => None,
            })
            .unwrap()
    };
    let light = blue(&doc);
    assert_eq!(light, Palette::of(&doc).swatch(Hue::Blue, Row::Strong));
    doc.canvas_theme = CanvasTheme::Dark;
    let dark = blue(&doc);
    assert_eq!(dark, Palette::of(&doc).swatch(Hue::Blue, Row::Strong));
    assert_ne!(light, dark, "palette colors follow the canvas");
    // Custom colors stay exact on the dark canvas.
    doc.bonds[0].color = Color::Custom([32, 80, 145]);
    assert_eq!(blue(&doc), [32, 80, 145]);
}
