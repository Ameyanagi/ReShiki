#[test]
fn highlighted_selection_bounds_build_join_geometry_once_at_any_selection_size() {
    use crate::{
        document::{Document, Point},
        palette::Color,
    };
    for count in [8, 128] {
        let mut doc = Document::default();
        let ids: Vec<_> = (0..count)
            .map(|i| doc.add_atom("C", Point::new(i as f32 * 36., (i % 2) as f32 * 21.)))
            .collect();
        for pair in ids.windows(2) {
            let [a, b] = pair else { continue };
            doc.add_bond(*a, *b, 1, "bold");
        }
        let before = crate::bond_joins::construction_count();
        assert!(super::selection_bounds(&doc, &ids).is_some());
        assert_eq!(crate::bond_joins::construction_count() - before, 0);
        for bond in &mut doc.bonds {
            bond.highlight = Some(Color::Custom([190, 230, 240]));
        }
        let before = crate::bond_joins::construction_count();
        assert!(super::selection_bounds(&doc, &[]).is_none());
        assert_eq!(crate::bond_joins::construction_count() - before, 0);
        let before = crate::bond_joins::construction_count();
        assert!(super::selection_bounds(&doc, &ids).is_some());
        assert_eq!(
            crate::bond_joins::construction_count() - before,
            1,
            "{count} selected atoms"
        );
    }
}

#[test]
fn primitives_build_join_geometry_once_plus_once_for_highlights() {
    use crate::{
        document::{Document, Point},
        palette::Color,
    };
    let mut doc = Document::default();
    let ids: Vec<_> = (0..8)
        .map(|i| doc.add_atom("C", Point::new(i as f32 * 36., (i % 2) as f32 * 21.)))
        .collect();
    for pair in ids.windows(2) {
        let [a, b] = pair else { continue };
        doc.add_bond(*a, *b, 1, "bold");
    }
    let before = crate::bond_joins::construction_count();
    assert!(!super::primitives(&doc).is_empty());
    assert_eq!(crate::bond_joins::construction_count() - before, 1);
    for bond in &mut doc.bonds {
        bond.highlight = Some(Color::Custom([190, 230, 240]));
    }
    let before = crate::bond_joins::construction_count();
    assert!(!super::primitives(&doc).is_empty());
    assert_eq!(crate::bond_joins::construction_count() - before, 2);
}

use super::*;

#[test]
fn overlapping_atom_label_ink_keeps_reverse_paint_order() {
    let mut doc = Document::default();
    let first = doc.add_atom("N", Point::default());
    let last = doc.add_atom("N", Point::default());
    let (lo, hi) = atom_label_ink_boxes(doc.atom(first).unwrap(), &doc)[0];
    let point = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert_eq!(atom_label_hit(&doc, point, 0.), Some(last));
    doc.atoms.reverse();
    assert_eq!(atom_label_hit(&doc, point, 0.), Some(first));
}

#[test]
fn batched_bounds_match_each_selection() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let groups = crate::editing::groups(&doc, &doc.all_ids());
    assert!(groups.len() > 100);
    for (group, bounds) in groups.iter().zip(selections_bounds(&doc, &groups)) {
        assert_eq!(bounds, selection_bounds(&doc, group), "{group:?}");
    }
    assert_eq!(selections_bounds(&doc, &[vec![]]), [None]);
}

#[test]
fn stacked_hydrogens_keep_charge_isotope_and_subscript_ink_separate() {
    for size in [8., 10., 18.] {
        for degrees in (0..360).step_by(15) {
            let mut doc = Document::default();
            let n = doc.add_atom("N", Point::default());
            for x in [-60., 60.] {
                let c = doc.add_atom("C", Point::new(x, 35.));
                doc.add_bond(n, c, 1, "plain");
            }
            let atom = doc.atom_mut(n).unwrap();
            atom.explicit_h = 2;
            atom.no_implicit = true;
            atom.charge = 1;
            atom.isotope = 15;
            atom.text_style = Some(crate::typography::TextStyle {
                size_pt: size,
                ..Default::default()
            });
            let ids = doc.all_ids();
            crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., degrees as f32);
            let boxes = label_ink_boxes(&atom_label(doc.atom(n).unwrap(), &doc));
            for (i, (a, b)) in boxes.iter().enumerate() {
                for (c, d) in &boxes[i + 1..] {
                    assert!(
                        b.x <= c.x || a.x >= d.x || b.y <= c.y || a.y >= d.y,
                        "Overlapping label glyphs at {degrees} degrees, {size} pt"
                    );
                }
            }
        }
    }
}

#[test]
fn internal_label_bonds_clear_every_glyph_through_rotation() -> Result<(), String> {
    for label in ["NH", "CH2", "CCl2", "CF2", "NMe", "SiH2", "C(OH)2"] {
        for degrees in (0..360).step_by(15) {
            let mut doc = Document::default();
            let center = doc.add_atom("C", Point::default());
            for x in [-60., 60.] {
                let end = doc.add_atom("C", Point::new(x, 35.));
                doc.add_bond(center, end, 1, "plain");
            }
            doc = crate::atom_text::apply(&doc, center, label, crate::atom_text::Mode::Auto)?;
            let ids = doc.all_ids();
            crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., degrees as f32);
            let boxes = label_ink_boxes(&atom_label(doc.atom(center).ok_or("Atom")?, &doc));
            let mut edges = Vec::new();
            for primitive in primitives(&doc) {
                match primitive {
                    Primitive::Line(a, b, width) => edges.push((a, b, width)),
                    Primitive::Polygon(points) => {
                        for i in 0..points.len() {
                            edges.push((points[i], points[(i + 1) % points.len()], 0.));
                        }
                    }
                    Primitive::Path {
                        commands, style, ..
                    } => {
                        use crate::graphics::PathCommand as P;
                        let mut first = Point::default();
                        let mut last = first;
                        for command in commands {
                            match command {
                                P::Move(p) => {
                                    first = p;
                                    last = p;
                                }
                                P::Line(p) => {
                                    edges.push((last, p, style.width()));
                                    last = p;
                                }
                                P::Close => {
                                    edges.push((last, first, style.width()));
                                    last = first;
                                }
                                P::Cubic(..) => panic!("Unexpected curved bond"),
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert!(
                !edges.is_empty(),
                "The test must inspect actual rendered bonds"
            );
            for (from, to, width) in edges {
                for step in 0..=100 {
                    let f = step as f32 / 100.;
                    let point =
                        Point::new(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
                    for (lo, hi) in &boxes {
                        assert!(
                            point.x + width / 2. <= lo.x
                                || point.x - width / 2. >= hi.x
                                || point.y + width / 2. <= lo.y
                                || point.y - width / 2. >= hi.y,
                            "{label} at {degrees} crosses label ink"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn rotated_group_bonds_do_not_cross_label_ink() -> Result<(), String> {
    use crate::abbreviations::LabelAlignment;
    for label in ["C2H5", "OCH3", "Boc"] {
        for alignment in LabelAlignment::ALL {
            for order in [1, 2, 3] {
                for degrees in (0..360).step_by(15) {
                    let mut doc = Document::default();
                    let n = doc.add_atom("N", Point::new(-70., 0.));
                    let c = doc.add_atom("C", Point::default());
                    doc.add_bond(n, c, 1, "plain");
                    doc = crate::atom_text::apply(&doc, c, label, crate::atom_text::Mode::Auto)?;
                    // Imported group drawings may carry multiple bonds.
                    doc.bonds
                        .iter_mut()
                        .find(|b| b.a == n && b.b == c)
                        .ok_or("Bond")?
                        .order = order;
                    doc.abbreviations.first_mut().ok_or("Group")?.alignment = alignment;
                    let ids = doc.all_ids();
                    crate::editing::transform_about(
                        &mut doc,
                        &ids,
                        Point::default(),
                        1.,
                        degrees as f32,
                    );
                    let boxes: Vec<_> = [n, c]
                        .into_iter()
                        .filter_map(|id| doc.atom(id))
                        .flat_map(|a| label_ink_boxes(&atom_label(a, &doc)))
                        .collect();
                    for primitive in primitives(&doc) {
                        if let Primitive::Line(from, to, width) = primitive {
                            // Sample the complete stroked rail, not only its midpoint.
                            for step in 0..=100 {
                                let t = step as f32 / 100.;
                                let p = from.offset((to.x - from.x) * t, (to.y - from.y) * t);
                                for (lo, hi) in &boxes {
                                    assert!(
                                        p.x < lo.x - width / 2.
                                            || p.x > hi.x + width / 2.
                                            || p.y < lo.y - width / 2.
                                            || p.y > hi.y + width / 2.,
                                        "Bond crosses {label} at {degrees}°, {alignment:?}, order {order}: {p:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn group_labels_remain_visible_and_anchor_to_elements_at_every_angle() -> Result<(), String> {
    use crate::abbreviations::LabelAlignment;
    for label in ["C2H5", "C₂H₅", "OCH3", "Boc"] {
        let mut source = Document::default();
        let n = source.add_atom("N", Point::new(-70., 0.));
        let a = source.add_atom("C", Point::default());
        source.add_bond(n, a, 1, "plain");
        source = crate::atom_text::apply(&source, a, label, crate::atom_text::Mode::Auto)?;
        for alignment in LabelAlignment::ALL {
            for angle in (0..360).step_by(5) {
                let mut doc = source.clone();
                doc.abbreviations.first_mut().ok_or("Group")?.alignment = alignment;
                let ids = doc.all_ids();
                crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., angle as f32);
                let atom = doc.atom(a).ok_or("Anchor")?;
                let runs = atom_label(atom, &doc);
                let (lo, hi) = text_bounds(&runs).ok_or("Label disappeared")?;
                assert!(lo.x.is_finite() && lo.y.is_finite() && hi.x > lo.x && hi.y > lo.y);
                assert!((hi.x - lo.x) < 120., "Label bounds exploded at {angle}°");
                let mut text = String::new();
                for run in &runs {
                    if let Primitive::Text { text: value, .. } = run {
                        text.push_str(value);
                    }
                }
                assert_eq!(text, doc.abbreviation(a).ok_or("Group")?.text(&doc));
                if label == "C2H5"
                    && !matches!(alignment, LabelAlignment::Above | LabelAlignment::Center)
                {
                    // Digits are scripts; the attachment is centered on C,
                    // even for the reversed H5C2 spelling.
                    let (position, size, style) = runs
                        .iter()
                        .find_map(|p| match p {
                            Primitive::Text {
                                text,
                                position,
                                size,
                                style,
                                ..
                            } if text == "C" => Some((position, size, style)),
                            _ => None,
                        })
                        .ok_or("Carbon glyph")?;
                    let center =
                        position.x + crate::style::styled_text_width("C", *size, style) / 2.;
                    assert!(
                        (center - atom.position.x).abs() < 0.01,
                        "{alignment:?}, {angle}°"
                    );
                }
            }
        }
    }
    // Imported groups can contain a whitespace-only reverse spelling.
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(70., 0.));
    let a = doc.add_atom("C", Point::default());
    doc.add_bond(n, a, 1, "plain");
    doc = crate::atom_text::apply(&doc, a, "Boc", crate::atom_text::Mode::Auto)?;
    doc.abbreviations.first_mut().ok_or("Group")?.reverse_label = "  ".into();
    assert!(atom_label_bounds(doc.atom(a).ok_or("Anchor")?, &doc).is_some());
    assert_eq!(doc.abbreviation(a).ok_or("Group")?.text(&doc), "Boc");
    Ok(())
}
#[test]
fn group_name_hyphens_stay_on_the_baseline() -> Result<(), String> {
    use crate::typography::Script;
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(-70., 0.));
    let a = doc.add_atom("C", Point::default());
    doc.add_bond(n, a, 1, "plain");
    doc = crate::atom_text::apply(&doc, a, "2-Py", crate::atom_text::Mode::Auto)?;
    let runs = atom_label(doc.atom(a).ok_or("Anchor")?, &doc);
    let mut text = String::new();
    for run in &runs {
        if let Primitive::Text {
            text: value, style, ..
        } = run
        {
            assert_eq!(style.script, Script::Normal, "{value}");
            text.push_str(value);
        }
    }
    assert_eq!(text, "2-Py");
    Ok(())
}
#[test]
fn svg_preserves_annotations_and_escapes_xml() {
    let mut d = Document::default();
    d.annotations.push(crate::document::Annotation {
        id: 1,
        position: Point::default(),
        text: "A < B & C".into(),
        format: Default::default(),
    });
    let xml = svg(&d);
    assert!(xml.contains("A &lt; B &amp; C"));
    assert!(!xml.contains("NaN"));
}

#[test]
fn free_text_labels_render_formula_scripts_in_svg() -> Result<(), Box<dyn std::error::Error>> {
    for size_pt in [8., 10., 18.] {
        for (label, expected_normal, expected_subscript, expected_superscript) in [
            ("M2+", "M", "", "2+"),
            ("M2⁺", "M", "", "2+"),
            ("M2⁻", "M", "", "2−"),
            ("R1", "R", "1", ""),
            ("SO4 2-", "SO", "4", "2−"),
            ("SO42-", "SO", "4", "2−"),
            ("NMe4+", "NMe", "4", "+"),
            ("PPh4+", "PPh", "4", "+"),
        ] {
            let mut doc = Document::default();
            let id = doc.add_atom("C", Point::new(20., 30.));
            doc = crate::atom_text::apply(&doc, id, label, crate::atom_text::Mode::Text)?;
            doc.atom_mut(id).ok_or("Atom")?.text_style = Some(crate::typography::TextStyle {
                size_pt,
                ..Default::default()
            });
            let before = doc.clone();
            let svg = svg(&doc);
            let xml = roxmltree::Document::parse(&svg)?;
            let runs: Vec<_> = xml
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .map(|node| {
                    Ok::<_, Box<dyn std::error::Error>>((
                        node.text().ok_or("Text content")?,
                        node.attribute("font-size")
                            .ok_or("Font size")?
                            .parse::<f32>()?,
                        node.attribute("y").ok_or("Text origin")?.parse::<f32>()?,
                    ))
                })
                .collect::<Result<_, _>>()?;
            let size = STYLE.world(size_pt);
            let normal_y = runs
                .iter()
                .find(|(_, run_size, _)| (*run_size - size).abs() < 0.001)
                .ok_or("Normal text run")?
                .2;
            let mut normal = String::new();
            let mut subscript = String::new();
            let mut superscript = String::new();
            for (text, run_size, y) in runs {
                assert!(!text.chars().any(char::is_whitespace), "{label}: {svg}");
                if (run_size - size).abs() < 0.001 {
                    assert!((y - normal_y).abs() < 0.001, "{label}: {svg}");
                    normal.push_str(text);
                } else {
                    assert!((run_size - size * 0.7).abs() < 0.001, "{label}: {svg}");
                    if y > normal_y {
                        subscript.push_str(text);
                    } else {
                        assert!(y < normal_y, "{label}: {svg}");
                        superscript.push_str(text);
                    }
                }
            }
            assert_eq!(normal, expected_normal, "{label}: {svg}");
            assert_eq!(subscript, expected_subscript, "{label}: {svg}");
            assert_eq!(superscript, expected_superscript, "{label}: {svg}");
            assert_eq!(doc, before, "Rendering must preserve the stored label");
        }
    }
    Ok(())
}

#[test]
fn all_normal_free_labels_keep_their_existing_primitive() -> Result<(), String> {
    let mut source = Document::default();
    let id = source.add_atom("C", Point::new(20., 30.));
    let style = crate::typography::TextStyle {
        size_pt: 14.,
        bold: true,
        italic: true,
        color: crate::palette::Color::Custom([40, 80, 120]),
        ..Default::default()
    };
    source.atom_mut(id).ok_or("Atom")?.text_style = Some(style.clone());
    let size = STYLE.world(style.size_pt);
    for label in [
        "M",
        "L",
        "X",
        "Ar",
        "Boc",
        "Cp*",
        "custom ligand",
        "2 eq",
        "R₁",
        "Cl,Br",
    ] {
        let doc = crate::atom_text::apply(&source, id, label, crate::atom_text::Mode::Text)?;
        let atom = doc.atom(id).ok_or("Atom")?;
        let expected = Primitive::Text {
            position: atom.position.offset(
                -crate::style::styled_text_width(label, size, &style) / 2.,
                -crate::style::label_vertical_center(label, size, &style),
            ),
            text: label.into(),
            size,
            color: style.color.rgb(),
            style: style.clone(),
        };
        assert_eq!(
            format!("{:?}", atom_label_runs(atom, &doc)),
            format!("{:?}", vec![expected]),
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn manual_label_scripts_override_formula_inference_in_core_and_condensed_suffix()
-> Result<(), String> {
    use crate::typography::{Script, ScriptRun};
    let run = |start, end, script| ScriptRun { start, end, script };
    let mut source = Document::default();
    let id = source.add_atom("C", Point::new(20., 30.));
    for (label, scripts, normal, subscript, superscript) in [
        ("R1", vec![run(1, 2, Script::Superscript)], "R", "", "1"),
        ("R1", vec![run(0, 2, Script::Normal)], "R1", "", ""),
        ("CO2Et", vec![run(0, 5, Script::Normal)], "CO2Et", "", ""),
        ("CO2Et", vec![run(0, 1, Script::Subscript)], "O2Et", "C", ""),
        (
            "NH42+",
            vec![run(2, 3, Script::Subscript), run(3, 5, Script::Superscript)],
            "NH",
            "4",
            "2+",
        ),
        (
            "SO4 2⁻",
            vec![run(2, 3, Script::Subscript), run(3, 8, Script::Superscript)],
            "SO",
            "4",
            "2−",
        ),
    ] {
        let doc = crate::atom_text::apply_with_scripts(
            &source,
            id,
            label,
            &scripts,
            crate::atom_text::Mode::Text,
        )?;
        let before = doc.clone();
        let mut actual_normal = String::new();
        let mut actual_subscript = String::new();
        let mut actual_superscript = String::new();
        for primitive in atom_label_runs(doc.atom(id).ok_or("Atom")?, &doc) {
            let Primitive::Text {
                text, size, style, ..
            } = primitive
            else {
                return Err("Unexpected label primitive".into());
            };
            assert!(!style.formula, "{label}: {style:?}");
            assert_eq!(size, style.size(), "{label}");
            match style.script {
                Script::Normal => actual_normal.push_str(&text),
                Script::Subscript => actual_subscript.push_str(&text),
                Script::Superscript => actual_superscript.push_str(&text),
            }
        }
        assert_eq!(
            (
                actual_normal.as_str(),
                actual_subscript.as_str(),
                actual_superscript.as_str()
            ),
            (normal, subscript, superscript),
            "{label}"
        );
        assert_eq!(doc, before);
    }
    Ok(())
}

#[test]
fn a_fully_manual_superscript_label_is_raised_from_the_normal_baseline() -> Result<(), String> {
    use crate::typography::{Script, ScriptRun};
    let mut source = Document::default();
    let id = source.add_atom("C", Point::new(20., 30.));
    let normal = crate::atom_text::apply(&source, id, "M", crate::atom_text::Mode::Text)?;
    let scripted = crate::atom_text::apply_with_scripts(
        &source,
        id,
        "M",
        &[ScriptRun {
            start: 0,
            end: 1,
            script: Script::Superscript,
        }],
        crate::atom_text::Mode::Text,
    )?;
    let normal_runs = atom_label_runs(normal.atom(id).ok_or("Normal atom")?, &normal);
    let [
        Primitive::Text {
            position: normal_position,
            size: normal_size,
            ..
        },
    ] = normal_runs.as_slice()
    else {
        return Err("Normal run".into());
    };
    let normal_position = *normal_position;
    let normal_size = *normal_size;
    let runs = atom_label_runs(scripted.atom(id).ok_or("Scripted atom")?, &scripted);
    let [
        Primitive::Text {
            position,
            size,
            style,
            ..
        },
    ] = runs.as_slice()
    else {
        return Err("Scripted run".into());
    };
    assert_eq!(style.script, Script::Superscript);
    assert!(*size < normal_size);
    assert!(position.y < normal_position.y);
    Ok(())
}

#[test]
fn manual_superscript_word_spaces_are_literal_except_for_a_trailing_charge_separator()
-> Result<(), String> {
    use crate::typography::{Script, ScriptRun};
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    for (label, start, end, expected_normal, expected_superscript) in [
        ("custom ligand", 0, 13, "", "custom ligand"),
        ("X ‡", 1, 5, "X", " ‡"),
        ("X A B", 1, 5, "X", " A B"),
        ("SO4 2-", 3, 6, "SO4", "2−"),
        ("SO4 2-", 0, 6, "", "SO42−"),
    ] {
        let doc = crate::atom_text::apply_with_scripts(
            &source,
            id,
            label,
            &[ScriptRun {
                start,
                end,
                script: Script::Superscript,
            }],
            crate::atom_text::Mode::Text,
        )?;
        let mut normal = String::new();
        let mut superscript = String::new();
        for primitive in atom_label_runs(doc.atom(id).ok_or("Atom")?, &doc) {
            let Primitive::Text { text, style, .. } = primitive else {
                return Err("Unexpected label primitive".into());
            };
            match style.script {
                Script::Normal => normal.push_str(&text),
                Script::Superscript => superscript.push_str(&text),
                Script::Subscript => return Err("Unexpected inferred subscript".into()),
            }
        }
        assert_eq!(
            (normal.as_str(), superscript.as_str()),
            (expected_normal, expected_superscript),
            "{label}"
        );
        assert_eq!(
            doc.atom(id).ok_or("Atom")?.display.variable.as_deref(),
            Some(label)
        );
    }
    Ok(())
}

#[test]
fn generic_alternative_and_link_labels_keep_their_existing_runs() -> Result<(), String> {
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    let left = source.add_atom("C", Point::new(-60., 0.));
    let right = source.add_atom("C", Point::new(60., 0.));
    source.add_bond(left, id, 1, "plain");
    source.add_bond(id, right, 1, "plain");
    for label in ["C,N,R1", "M1-3"] {
        let doc = crate::atom_text::apply(&source, id, label, crate::atom_text::Mode::Auto)?;
        assert!(doc.generic.owns_label(id));
        let runs = atom_label_runs(doc.atom(id).ok_or("Atom")?, &doc);
        assert_eq!(runs.len(), 1, "{label}");
        assert!(matches!(
            runs.first(),
            Some(Primitive::Text { text, size, .. })
                if text == label && *size == STYLE.font_size()
        ));
    }
    Ok(())
}

#[test]
fn scripted_free_label_ink_remains_hittable_and_clears_bonds() -> Result<(), String> {
    for label in ["M2+", "R1", "SO4 2-"] {
        for degrees in (0..360).step_by(30) {
            let mut doc = Document::default();
            let id = doc.add_atom("C", Point::default());
            let end = doc.add_atom("C", Point::new(90., 0.));
            doc.add_bond(id, end, 1, "plain");
            doc = crate::atom_text::apply(&doc, id, label, crate::atom_text::Mode::Text)?;
            let ids = doc.all_ids();
            crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., degrees as f32);
            let atom = doc.atom(id).ok_or("Atom")?;
            let runs = atom_label(atom, &doc);
            let scripts: Vec<_> = runs
                .iter()
                .filter(
                    |run| matches!(run, Primitive::Text { size, .. } if *size < STYLE.font_size()),
                )
                .cloned()
                .collect();
            assert!(!scripts.is_empty(), "{label} must have positioned scripts");
            let script_ink = label_ink_boxes(&scripts);
            assert!(!script_ink.is_empty(), "{label} must have script ink");
            let bounds = atom_label_bounds(atom, &doc).ok_or("Label bounds")?;
            for (lo, hi) in script_ink {
                let center = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
                assert_eq!(atom_label_hit(&doc, center, 0.), Some(id), "{label}");
                assert!(
                    center.x >= bounds.0.x
                        && center.x <= bounds.1.x
                        && center.y >= bounds.0.y
                        && center.y <= bounds.1.y,
                    "{label} script is outside its label bounds"
                );
            }
            let ink = label_ink_boxes(&runs);
            let lines: Vec<_> = primitives(&doc)
                .into_iter()
                .filter_map(|primitive| match primitive {
                    Primitive::Line(from, to, width) => Some((from, to, width)),
                    _ => None,
                })
                .collect();
            assert!(!lines.is_empty(), "Inspect an actual rendered bond");
            for (from, to, width) in lines {
                for step in 0..=100 {
                    let t = step as f32 / 100.;
                    let point = from.offset((to.x - from.x) * t, (to.y - from.y) * t);
                    for (lo, hi) in &ink {
                        assert!(
                            point.x + width / 2. <= lo.x
                                || point.x - width / 2. >= hi.x
                                || point.y + width / 2. <= lo.y
                                || point.y - width / 2. >= hi.y,
                            "Bond crosses {label} at {degrees}°"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn publication_labels_clear_bonds_and_keep_scripts_separate() {
    let mut d = Document::default();
    let n = d.add_atom("N", Point::default());
    let c = d.add_atom("C", Point::new(42.0, 0.0));
    d.add_bond(n, c, 1, "plain");
    d.atom_mut(n).unwrap().label_h = 2;
    let drawing = primitives(&d);
    let bounds = text_bounds(&drawing).unwrap();
    // A terminal amine bonded to the right must put H2 to its left.
    assert!(
        drawing
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, position, .. }
            if text == "H" && position.x < -10.0))
    );
    assert!(
        drawing
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, size, .. }
            if text == "2" && *size < STYLE.font_size()))
    );
    assert!(
        drawing
            .iter()
            .filter_map(|p| match p {
                Primitive::Line(a, _, _) => Some(a.x > bounds.1.x),
                _ => None,
            })
            .all(|clear| clear)
    );
    assert!(
        drawing
            .iter()
            .all(|p| !matches!(p, Primitive::Text { color, .. } if *color != [0, 0, 0]))
    );
    d.atom_mut(n).unwrap().isotope = 15;
    let drawing = primitives(&d);
    let right = text_bounds(&drawing).unwrap().1.x;
    assert!(
        drawing
            .iter()
            .any(|p| matches!(p, Primitive::Line(a, _, _) if a.x > right))
    );
}

#[test]
fn ring_double_bonds_keep_a_continuous_outer_edge() {
    let mut d = Document::default();
    crate::editing::ring(&mut d, Point::default(), 6, false, 5.0);
    for (i, b) in d.bonds.iter_mut().enumerate() {
        if i % 2 == 0 {
            b.order = 2;
        }
    }
    let drawing = primitives(&d);
    let lines: Vec<_> = drawing
        .iter()
        .filter_map(|p| match p {
            Primitive::Line(a, b, _) => Some((*a, *b)),
            _ => None,
        })
        .collect();
    // The six outer edges now share filled joins; the three inset rails
    // remain independent strokes. Raster coverage is checked in bond_joins.
    assert_eq!(lines.len(), 3);
    assert!(
        drawing
            .iter()
            .any(|p| matches!(p, Primitive::Path { filled: true, .. }))
    );
    assert_eq!(
        lines
            .iter()
            .filter(
                |(a, b)| a.distance(Point::default()) < 41.0 && b.distance(Point::default()) < 41.0
            )
            .count(),
        3
    );
}

#[test]
fn sideways_alternative_lists_grow_away_from_their_bond() {
    use crate::{
        atom_text::{Mode, apply},
        document::{Document, Point},
    };
    // Issue #316: Clean up puts 4-chlorophenol's substituents left and right.
    // Centered lists covered the ring between them (x = -24..24).
    let mut doc = Document::default();
    let ids: Vec<_> = [(-48., 0.), (-24., 0.), (24., 0.), (48., 0.), (24., -48.)]
        .into_iter()
        .map(|(x, y)| doc.add_atom("C", Point::new(x, y)))
        .collect();
    for (a, b) in [(0, 1), (1, 2), (2, 3), (2, 4)] {
        doc.add_bond(ids[a], ids[b], 1, "plain");
    }
    let doc = apply(&doc, ids[0], "[Cl,Br,I]", Mode::Auto).unwrap();
    let doc = apply(&doc, ids[3], "[OMe,OPh]", Mode::Auto).unwrap();
    let doc = apply(&doc, ids[4], "[F,Cl]", Mode::Auto).unwrap();
    let bounds = |id| {
        let atom = doc.atom(id).unwrap();
        assert_eq!(atom.element, "*");
        super::atom_label_ink_bounds(atom, &doc).unwrap()
    };
    let (_, left) = bounds(ids[0]);
    assert!(left.x < -28., "{left:?}");
    let (right, _) = bounds(ids[3]);
    assert!(right.x > 28., "{right:?}");
    // A list on a vertical bond stays centered over its atom.
    let (lo, hi) = bounds(ids[4]);
    assert!(((lo.x + hi.x) / 2. - 24.).abs() < 1., "{lo:?} {hi:?}");
}
