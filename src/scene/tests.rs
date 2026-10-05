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
    let doc: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
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
fn expanded_azide_and_magnesium_bromide_keep_visible_bonds() -> Result<(), String> {
    for key in ["M", "Z"] {
        for angle in (0..360).step_by(15) {
            let mut doc = Document::default();
            let c = doc.add_atom("C", Point::new(-42., 0.));
            let end = doc.add_atom("C", Point::default());
            doc.add_bond(c, end, 1, "plain");
            doc = crate::hotkeys::atom_edit(&doc, end, key, 42.)
                .ok_or("key")??
                .0;
            let members = doc.abbreviation(end).ok_or("group")?.members.clone();
            doc.expand_abbreviations(&members);
            let ids = doc.all_ids();
            crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., angle as f32);
            let drawing = primitives(&doc);
            let strokes: usize = drawing
                .iter()
                .map(|p| match p {
                    Primitive::Line(_, _, _) | Primitive::Polygon(_) => 1,
                    Primitive::Path { commands, .. } => commands
                        .iter()
                        .filter(|c| matches!(c, crate::graphics::PathCommand::Move(_)))
                        .count(),
                    _ => 0,
                })
                .sum();
            let expected = if key == "M" { 2 } else { 5 };
            assert_eq!(strokes, expected, "{key}, {angle} degrees");
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
