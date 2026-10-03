use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    document::{Document, Point},
    graphics::PathCommand,
    highlights,
    palette::{Color, Hue, Row},
    scene::{self, Primitive},
};

fn native() -> Document {
    reshiki::chemistry::cdxml::import_cdxml(include_str!(
        "fixtures/structure-highlights/native-prime.cdxml"
    ))
    .unwrap()
    .document
}

fn numbers(text: &str) -> Vec<f32> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| word.parse().unwrap())
        .collect()
}

// The native yellow capsule uses only Move, Cubic and Close. Reading its path
// here keeps its geometry an independent oracle, not a Rust-generated golden.
fn native_capsule(source: &str) -> Vec<PathCommand> {
    let svg = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let path = svg
        .descendants()
        .find(|n| n.attribute("fill") == Some("#ffc600"))
        .unwrap();
    let mut tokens = path
        .attribute("d")
        .unwrap()
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|word| !word.is_empty());
    let mut result = Vec::new();
    while let Some(command) = tokens.next() {
        let mut point = || {
            Point::new(
                tokens.next().unwrap().parse::<f32>().unwrap() * 0.05,
                tokens.next().unwrap().parse::<f32>().unwrap() * 0.05,
            )
        };
        result.push(match command {
            "M" => PathCommand::Move(point()),
            "C" => PathCommand::Cubic(point(), point(), point()),
            "Z" => PathCommand::Close,
            _ => panic!("Unexpected native capsule command {command}"),
        });
    }
    result
}

fn capsule_metrics(
    commands: &[PathCommand],
    a: Point,
    b: Point,
    points_per_world: f32,
) -> [f32; 3] {
    let length = a.distance(b);
    let ux = (b.x - a.x) / length;
    let uy = (b.y - a.y) / length;
    reshiki::graphics::flattened(commands)
        .into_iter()
        .flatten()
        .fold(
            [f32::INFINITY, f32::NEG_INFINITY, 0.],
            |[lo, hi, radius], point| {
                let x = point.x - a.x;
                let y = point.y - a.y;
                let along = (x * ux + y * uy) * points_per_world;
                let across = (y * ux - x * uy).abs() * points_per_world;
                [lo.min(along), hi.max(along), radius.max(across)]
            },
        )
}

#[test]
fn capsule_geometry_matches_the_actual_chemdraw_prime_svg() {
    let doc = native();
    let yellow = Color::Custom([255, 198, 0]);
    let bond = doc
        .bonds
        .iter()
        .find(|b| b.highlight == Some(yellow))
        .unwrap();
    let a = doc.atom(bond.a).unwrap().position;
    let b = doc.atom(bond.b).unwrap().position;
    let drawing = scene::primitives(&doc);
    let commands = drawing
        .iter()
        .find_map(|p| match p {
            Primitive::Path {
                commands,
                style,
                filled: true,
            } if style.fill == Some(yellow) => Some(commands),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        commands
            .iter()
            .filter(|p| matches!(p, PathCommand::Move(_)))
            .count(),
        1,
        "equal atom and bond paint uses one capsule, without doubled antialiased edges"
    );
    let actual = capsule_metrics(commands, a, b, doc.drawing_style.points_per_world());
    let expected = capsule_metrics(
        &native_capsule(include_str!(
            "fixtures/structure-highlights/native-prime.svg"
        )),
        Point::new(130., 60.),
        Point::new(115.08, 76.58),
        1.,
    );
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.002,
            "native {expected} pt vs ReShiki {actual} pt"
        );
    }
}

#[test]
fn native_highlight_padding_follows_bond_spacing_independently_of_label_size() {
    for (source, svg) in [
        (
            include_str!("fixtures/structure-highlights/native-prime.cdxml"),
            include_str!("fixtures/structure-highlights/native-prime.svg"),
        ),
        (
            include_str!("fixtures/structure-highlights/authored-acs-font10.cdxml"),
            include_str!("fixtures/structure-highlights/native-prime-acs-font10.svg"),
        ),
        (
            include_str!("fixtures/structure-highlights/authored-acs-font20.cdxml"),
            include_str!("fixtures/structure-highlights/native-prime-acs-font20.svg"),
        ),
    ] {
        let doc = reshiki::chemistry::cdxml::import_cdxml(source)
            .unwrap()
            .document;
        let yellow = Color::Custom([255, 198, 0]);
        let cyan = Color::Custom([129, 230, 255]);
        let bond = doc
            .bonds
            .iter()
            .find(|b| b.highlight == Some(yellow))
            .unwrap();
        let a = doc.atom(bond.a).unwrap().position;
        let b = doc.atom(bond.b).unwrap().position;
        let drawing = scene::primitives(&doc);
        let painted = |color| {
            drawing
                .iter()
                .find_map(|p| match p {
                    Primitive::Path {
                        commands,
                        style,
                        filled: true,
                    } if style.fill == Some(color) => Some(commands),
                    _ => None,
                })
                .unwrap()
        };
        let actual = capsule_metrics(painted(yellow), a, b, doc.drawing_style.points_per_world());
        let expected = capsule_metrics(
            &native_capsule(svg),
            Point::new(130., 60.),
            Point::new(115.08, 76.58),
            1.,
        );
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.002,
                "native {expected}pt vs {actual}pt at label size {}pt",
                doc.drawing_style.font_size_pt
            );
        }
        let parsed = roxmltree::Document::parse_with_options(
            svg,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let native_label = parsed
            .descendants()
            .find(|n| n.attribute("fill") == Some("#81e6ff"))
            .unwrap();
        let mut tokens = native_label
            .attribute("d")
            .unwrap()
            .split(|c: char| c == ',' || c.is_whitespace());
        assert!(tokens.any(|t| t == "A"));
        let expected_rx = tokens.next().unwrap().parse::<f32>().unwrap() * 0.05;
        let expected_ry = tokens.next().unwrap().parse::<f32>().unwrap() * 0.05;
        let [
            PathCommand::Move(_),
            PathCommand::Line(top),
            PathCommand::Cubic(_, _, side),
            ..,
        ] = painted(cyan).as_slice()
        else {
            panic!("Expected an elliptical text capsule");
        };
        let rx = (side.x - top.x) * doc.drawing_style.points_per_world();
        let ry = (side.y - top.y) * doc.drawing_style.points_per_world();
        assert!((rx * 2. - ry).abs() < 0.002);
        // Exact native text dimensions apply when its Arial face is installed.
        // Fallback fonts retain their own ink metrics and the same bond padding.
        if reshiki::style::glyph_metrics('O', &doc.drawing_style.text_style()).0 == "Arial" {
            assert!(
                (rx - expected_rx).abs() < 0.002,
                "native label rx {expected_rx}pt vs {rx}pt"
            );
            assert!(
                (ry - expected_ry).abs() < 0.002,
                "native label ry {expected_ry}pt vs {ry}pt"
            );
        }
    }
}

#[test]
fn highlight_extents_are_inside_selection_and_figure_bounds_at_all_font_sizes() {
    for font_size in [4., 10., 24., 144.] {
        let mut doc = Document::default();
        doc.drawing_style.font_size_pt = font_size;
        let a = doc.add_atom("N", Point::default());
        doc.atom_mut(a).unwrap().label_h = 2;
        doc.atom_mut(a).unwrap().isotope = 15;
        doc.atom_mut(a).unwrap().charge = 1;
        let paint = Color::Custom([190, 230, 240]);
        highlights::apply(&mut doc, &[a], Some(paint));
        let (lo, hi) = scene::selection_bounds(&doc, &[a]).unwrap();
        let svg = scene::svg(&doc);
        let parsed = roxmltree::Document::parse(&svg).unwrap();
        let view = numbers(parsed.root_element().attribute("viewBox").unwrap());
        let mut painted_points = 0;
        for primitive in scene::primitives(&doc) {
            if let Primitive::Path {
                commands,
                style,
                filled: true,
            } = primitive
                && style.fill == Some(paint)
            {
                for point in reshiki::graphics::flattened(&commands)
                    .into_iter()
                    .flatten()
                {
                    painted_points += 1;
                    assert!(point.x >= lo.x - 0.001 && point.x <= hi.x + 0.001);
                    assert!(point.y >= lo.y - 0.001 && point.y <= hi.y + 0.001);
                    assert!(point.x > view[0] && point.x < view[0] + view[2]);
                    assert!(point.y > view[1] && point.y < view[1] + view[3]);
                }
            }
        }
        assert!(painted_points > 100);
        assert!(svg.contains("<text"), "halos keep text as text");
    }
}

#[test]
fn molecular_ink_remains_after_highlight_paths_and_figure_formats_include_the_paint() {
    for canvas in CanvasTheme::ALL {
        let mut doc = native();
        doc.canvas_theme = canvas;
        ColorTheme::Presentation.apply(&mut doc);
        let drawing = scene::primitives(&doc);
        assert!(matches!(
            drawing.first(),
            Some(Primitive::Path { filled: true, .. })
        ));
        let text_index = drawing
            .iter()
            .position(|p| matches!(p, Primitive::Text { .. }))
            .unwrap();
        let last_highlight = drawing
            .iter()
            .rposition(|p| {
                matches!(p,
                    Primitive::Path { style, filled: true, .. }
                        if style.fill.is_some_and(|color| {
                            let rgb = canvas.color(color.rgb());
                            rgb == [255, 198, 0] || rgb == [129, 230, 255]
                        })
                )
            })
            .unwrap();
        assert!(last_highlight < text_index);
        let svg = scene::svg(&doc);
        assert!(svg.contains("rgb(255,198,0)"));
        assert!(svg.contains("rgb(129,230,255)"));
        let pdf = reshiki::export::clipboard_drawing(&doc, "pdf").unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let png = reshiki::export::clipboard_drawing(&doc, "png").unwrap();
        let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
        for (x, y) in [
            (0, 0),
            (pixels.width() - 1, 0),
            (0, pixels.height() - 1),
            (pixels.width() - 1, pixels.height() - 1),
        ] {
            assert_eq!(
                pixels.get_pixel(x, y).0[3],
                0,
                "transparent surround retains halo clearance"
            );
        }
        assert!(pixels.pixels().any(|p| p.0 == [255, 198, 0, 255]));
        assert!(pixels.pixels().any(|p| p.0 == [129, 230, 255, 255]));
    }
}

#[test]
fn automatic_label_ink_uses_its_halo_background_and_manual_ink_keeps_precedence() {
    for canvas in CanvasTheme::ALL {
        for rgb in [[0; 3], [255; 3]] {
            let mut doc = Document {
                canvas_theme: canvas,
                ..Document::default()
            };
            let a = doc.add_atom("O", Point::default());
            highlights::apply(&mut doc, &[a], Some(Color::Custom(rgb)));
            let atom = doc.atom(a).unwrap();
            let ink = canvas.color(reshiki::canvas_theme::atom_color(&doc, atom));
            assert!(reshiki::color_contrast::contrast(ink, rgb) >= 4.5);
            doc.atom_mut(a).unwrap().display.color_override = true;
            doc.atom_mut(a).unwrap().text_style = Some(reshiki::typography::TextStyle {
                color: Color::Palette(Hue::Red, Row::Strong),
                ..Default::default()
            });
            let atom = doc.atom(a).unwrap();
            assert_eq!(
                reshiki::canvas_theme::atom_color(&doc, atom),
                reshiki::palette::Palette::of(&doc)
                    .canonical(atom.text_style.as_ref().unwrap().color)
            );
        }
    }
}

#[test]
fn automatic_bond_ink_stays_legible_on_exact_highlights_without_mutating_its_color() {
    for canvas in CanvasTheme::ALL {
        let mut doc = native();
        doc.canvas_theme = canvas;
        let original = doc.clone();
        let resolved = reshiki::canvas_theme::resolved_document(&doc).into_owned();
        for bond in resolved.bonds.iter().filter(|b| b.highlight.is_some()) {
            let palette = reshiki::palette::Palette::of(&resolved);
            assert!(
                reshiki::color_contrast::contrast(
                    palette.rgb(bond.color),
                    palette.rgb(bond.highlight.unwrap())
                ) >= 4.5
            );
        }
        assert_eq!(doc, original);
        assert_eq!(
            reshiki::canvas_theme::resolved_document(&resolved).as_ref(),
            &resolved
        );
    }
}

#[test]
fn atom_halo_stays_above_different_incident_bond_colors_in_both_insertion_orders() {
    for canvas in CanvasTheme::ALL {
        for reverse in [false, true] {
            let mut doc = Document {
                canvas_theme: canvas,
                ..Document::default()
            };
            let center = doc.add_atom("C", Point::default());
            let left = doc.add_atom("C", Point::new(-42., 0.));
            let right = doc.add_atom("C", Point::new(42., 0.));
            let cyan = Color::Custom([129, 230, 255]);
            let yellow = Color::Custom([255, 198, 0]);
            doc.atom_mut(center).unwrap().display.highlight = Some(cyan);
            doc.add_bond(center, left, 1, "plain");
            doc.add_bond(center, right, 1, "plain");
            doc.bonds[0].highlight = Some(cyan);
            doc.bonds[1].highlight = Some(yellow);
            if reverse {
                doc.bonds.reverse();
            }
            let colors: Vec<_> = scene::primitives(&doc)
                .iter()
                .filter_map(|primitive| match primitive {
                    Primitive::Path {
                        style,
                        filled: true,
                        ..
                    } => style.fill.map(|color| canvas.color(color.rgb())),
                    _ => None,
                })
                .filter(|rgb| *rgb == [129, 230, 255] || *rgb == [255, 198, 0])
                .collect();
            assert_eq!(
                colors.len(),
                3,
                "two underlays plus the independent atom halo"
            );
            assert_eq!(colors.last(), Some(&[129, 230, 255]));
        }
    }
}

#[test]
fn atom_halo_stays_above_an_unrelated_crossing_bond_color() {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    let left = doc.add_atom("C", Point::new(-42., 0.));
    let above = doc.add_atom("C", Point::new(0., -42.));
    let below = doc.add_atom("C", Point::new(0., 42.));
    let cyan = Color::Custom([129, 230, 255]);
    let yellow = Color::Custom([255, 198, 0]);
    doc.atom_mut(center).unwrap().display.highlight = Some(cyan);
    doc.add_bond(center, left, 1, "plain");
    doc.add_bond(above, below, 1, "plain");
    doc.bonds[0].highlight = Some(cyan);
    doc.bonds[1].highlight = Some(yellow);
    let drawing = scene::primitives(&doc);
    let colors: Vec<_> = drawing
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Path {
                style,
                filled: true,
                ..
            } if style.fill == Some(cyan) || style.fill == Some(yellow) => style.fill,
            _ => None,
        })
        .collect();
    assert_eq!(colors, [cyan, yellow, cyan]);
}

fn inside_paint(point: Point, contours: &[Vec<Point>]) -> bool {
    contours.iter().any(|contour| {
        let mut inside = false;
        for pair in contour.windows(2) {
            let [a, b] = pair else { continue };
            if reshiki::graphics::segment_distance(point, *a, *b) < 0.001 {
                return true;
            }
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
        inside
    })
}

fn paint_contours(drawing: &[Primitive], color: Color) -> Vec<Vec<Point>> {
    drawing
        .iter()
        .flat_map(|primitive| match primitive {
            Primitive::Path {
                commands,
                style,
                filled: true,
            } if style.fill == Some(color) => reshiki::graphics::flattened(commands),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn wavy_bond_ink_is_inside_its_highlight_with_small_fonts_and_thick_lines() {
    for (font_size, line_width) in [(4., 1.), (10., 2.)] {
        let mut doc = Document::default();
        doc.drawing_style.font_size_pt = font_size;
        doc.drawing_style.line_width_pt = line_width;
        doc.drawing_style.bond_spacing_ratio = 0.05;
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "wavy");
        let paint = Color::Custom([190, 230, 240]);
        doc.bonds[0].highlight = Some(paint);
        let drawing = scene::primitives(&doc);
        let contours = paint_contours(&drawing, paint);
        let mut sampled = 0;
        for primitive in &drawing {
            if let Primitive::Path {
                commands,
                style,
                filled: false,
            } = primitive
            {
                for point in reshiki::graphics::flattened(commands).into_iter().flatten() {
                    for angle in (0..8).map(|i| i as f32 * std::f32::consts::FRAC_PI_4) {
                        let edge = point.offset(
                            angle.cos() * style.width() / 2.,
                            angle.sin() * style.width() / 2.,
                        );
                        assert!(
                            inside_paint(edge, &contours),
                            "font {font_size}pt, line {line_width}pt: {edge:?}"
                        );
                        sampled += 1;
                    }
                }
            }
        }
        assert!(sampled > 100);
    }
}

#[test]
fn sharp_bold_and_wedge_junction_ink_stays_inside_its_highlight() {
    for display in ["bold", "wedge", "hollow_wedge"] {
        for angle in [10_f32, 30., 90., 150.] {
            let mut doc = Document::default();
            doc.drawing_style.font_size_pt = 10.;
            doc.drawing_style.line_width_pt = 0.6;
            doc.drawing_style.bold_width_pt = 2.;
            doc.drawing_style.bond_spacing_ratio = 0.12;
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(42., 0.));
            let c = doc.add_atom(
                "C",
                Point::new(
                    angle.to_radians().cos() * 42.,
                    angle.to_radians().sin() * 42.,
                ),
            );
            // Wide ends meet at the central atom.
            doc.add_bond(b, a, 1, display);
            doc.add_bond(c, a, 1, display);
            let paint = Color::Custom([190, 230, 240]);
            for bond in &mut doc.bonds {
                bond.highlight = Some(paint);
            }
            let drawing = scene::primitives(&doc);
            let contours = paint_contours(&drawing, paint);
            let mut sampled = 0;
            for primitive in &drawing {
                let (points, stroke) = match primitive {
                    Primitive::Path {
                        commands,
                        style,
                        filled,
                    } if style.fill != Some(paint) => (
                        reshiki::graphics::flattened(commands)
                            .into_iter()
                            .flatten()
                            .collect(),
                        if *filled { 0. } else { style.width() / 2. },
                    ),
                    Primitive::Polygon(points) => (points.clone(), 0.),
                    _ => continue,
                };
                for point in points {
                    for direction in (0..8).map(|i| i as f32 * std::f32::consts::FRAC_PI_4) {
                        let edge = point.offset(direction.cos() * stroke, direction.sin() * stroke);
                        assert!(
                            inside_paint(edge, &contours),
                            "{display}, {angle} degrees: {edge:?}"
                        );
                        sampled += 1;
                    }
                }
            }
            assert!(sampled > 0);
        }
    }
}

#[test]
#[ignore = "Opt-in native highlight comparison artifacts"]
fn write_native_highlight_comparison_artifacts() {
    let folder = std::path::PathBuf::from(std::env::var("RESHIKI_HIGHLIGHT_EVIDENCE_DIR").unwrap());
    std::fs::create_dir_all(&folder).unwrap();
    // Start with the actual native clipboard fixture, then make the visible
    // label and its hidden oxygen require opposite automatic foregrounds.
    // The resulting editable files below come from the production writer.
    let mut autoink = reshiki::chemistry::cdxml::import_cdxml(include_str!(
        "fixtures/structure-highlights/native-contracted.cdxml"
    ))
    .unwrap()
    .document;
    autoink.canvas_theme = CanvasTheme::Light;
    let anchor = autoink.abbreviations[0].anchor;
    autoink.abbreviations[0].highlight = Some(Color::Custom([0; 3]));
    autoink.abbreviations[0].label_style = None;
    autoink.abbreviations[0].label_color_override = false;
    let atom = autoink.atom_mut(anchor).unwrap();
    atom.display.highlight = Some(Color::Custom([255; 3]));
    atom.display.color_override = false;
    atom.display.hydrogen_color = None;
    atom.text_style = None;
    let ink = |doc: &Document| {
        doc.canvas_theme.color(reshiki::canvas_theme::atom_color(
            doc,
            doc.atom(anchor).unwrap(),
        ))
    };
    let mut expanded = autoink.clone();
    assert_eq!(expanded.expand_abbreviations(&[anchor]), 1);
    let outer_ink = ink(&autoink);
    let inner_ink = ink(&expanded);
    assert_ne!(outer_ink, inner_ink);
    assert!(reshiki::color_contrast::contrast(outer_ink, [0; 3]) >= 4.5);
    assert!(reshiki::color_contrast::contrast(inner_ink, [255; 3]) >= 4.5);
    let palette = reshiki::palette::Palette::of(&autoink);
    let expected = serde_json::json!({
        "scenario": "Automatic ink on a black contracted OMe halo and white internal oxygen halo",
        "source_fixture": "tests/fixtures/structure-highlights/native-contracted.cdxml",
        "generation": "ReShiki Document via the production editable and figure exporters",
        "foreground_mode": "automatic",
        "contracted_label": {
            "text": "OMe", "highlight_rgb": [0, 0, 0], "text_rgb": outer_ink,
            "abbreviation_count": 1
        },
        "expanded_anchor": {
            "element": "O", "highlight_rgb": [255, 255, 255], "text_rgb": inner_ink,
            "abbreviation_count": 0
        },
        "native_acceptance": "Open CDXML or CDX in Prime; Copy/New/Paste; Expand Label; save both states. Compare the label/oxygen foregrounds and all atom/bond highlights with this expectation.",
        "atoms": autoink.atoms.iter().map(|atom| serde_json::json!({
            "rsk_id": atom.id, "element": atom.element, "charge": atom.charge,
            "highlight_rgb": atom.display.highlight.map(|color| palette.rgb(color))
        })).collect::<Vec<_>>(),
        "bonds": autoink.bonds.iter().map(|bond| serde_json::json!({
            "rsk_atom_ids": [bond.a, bond.b], "order": bond.order,
            "highlight_rgb": bond.highlight.map(|color| palette.rgb(color))
        })).collect::<Vec<_>>()
    });
    std::fs::write(
        folder.join("reshiki-ome-auto-ink.expected.json"),
        serde_json::to_vec_pretty(&expected).unwrap(),
    )
    .unwrap();
    std::fs::write(
        folder.join("reshiki-ome-auto-ink-expanded.svg"),
        scene::svg(&expanded),
    )
    .unwrap();
    for (name, doc) in [
        ("reshiki-native-prime", native()),
        ("reshiki-ome-auto-ink", autoink),
        (
            "reshiki-native-independent-label-ink",
            reshiki::chemistry::cdxml::import_cdxml(include_str!(
                "fixtures/structure-highlights/native-independent-label-ink.cdxml"
            ))
            .unwrap()
            .document,
        ),
        (
            "reshiki-native-prime-acs-font10",
            reshiki::chemistry::cdxml::import_cdxml(include_str!(
                "fixtures/structure-highlights/authored-acs-font10.cdxml"
            ))
            .unwrap()
            .document,
        ),
        (
            "reshiki-native-prime-acs-font20",
            reshiki::chemistry::cdxml::import_cdxml(include_str!(
                "fixtures/structure-highlights/authored-acs-font20.cdxml"
            ))
            .unwrap()
            .document,
        ),
    ] {
        std::fs::write(folder.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
        for extension in ["svg", "pdf", "png"] {
            std::fs::write(
                folder.join(format!("{name}.{extension}")),
                reshiki::export::clipboard_drawing(&doc, extension).unwrap(),
            )
            .unwrap();
        }
        let xml = reshiki::exchange::drawing::write(&doc, Default::default()).unwrap();
        std::fs::write(folder.join(format!("{name}.cdxml")), &xml).unwrap();
        std::fs::write(
            folder.join(format!("{name}.cdx")),
            reshiki::exchange::to_cdx(&xml).unwrap(),
        )
        .unwrap();
    }
}
