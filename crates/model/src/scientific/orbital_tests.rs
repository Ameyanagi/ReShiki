use super::*;
use crate::{canvas_theme::CanvasTheme, document::Document, palette::Color, scene::Primitive};

fn tool(snap: bool) -> Drawing {
    Drawing {
        kind: GraphicKind::Orbital(OrbitalKind::P),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: false,
        snap_orbitals: snap,
    }
}

#[test]
fn orbital_node_snapping_is_independent_and_respects_the_supplied_zoom_radius() {
    let mut doc = Document::default();
    let a = doc.add_atom("N", p(0., 0.));
    let b = doc.add_atom("O", p(16., 0.));
    let before = doc.clone();
    for zoom in [0.5, 1., 2.] {
        for (start, expected) in [(p(3. / zoom, 0.), a), (p(16. - 2. / zoom, 0.), b)] {
            let radius = 10. / zoom;
            assert_eq!(
                tool(true).orbital_target(&doc, start, radius),
                Some(expected)
            );
            assert_eq!(tool(false).orbital_target(&doc, start, radius), None);
            for snap in [false, true] {
                let drawing = tool(snap);
                let mut preview = doc.clone();
                let mut committed = doc.clone();
                let end = start.offset(7., -42.);
                drawing
                    .place(&mut preview, start, end, true, radius)
                    .unwrap();
                drawing
                    .place(&mut committed, start, end, true, radius)
                    .unwrap();
                assert_eq!(preview, committed);
                assert_eq!(committed.atoms, before.atoms);
                assert_eq!(committed.bonds, before.bonds);
                let orbital = committed.graphics.last().unwrap();
                assert_eq!(
                    orbital.origin,
                    if snap {
                        doc.atom(expected).unwrap().position
                    } else {
                        start
                    }
                );
                assert_eq!(orbital.layer, -1);
                assert_eq!(
                    Document::from_json(&committed.file_json().unwrap()).unwrap(),
                    committed.current()
                );
            }
        }
        assert_eq!(
            tool(true).orbital_target(&doc, p(-11. / zoom, 0.), 10. / zoom),
            None
        );
    }
}

fn filled_at(commands: &[PathCommand], point: Point) -> bool {
    // Independent winding/ray oracle for the emitted scene, not the clipping
    // implementation. No orbital fill may cover any visible label ink box.
    crate::graphics::flattened(commands).iter().any(|path| {
        let mut inside = false;
        for (a, b) in path
            .iter()
            .zip(path.iter().cycle().skip(1))
            .take(path.len())
        {
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
        }
        inside
    })
}
fn distance_to_edge(point: Point, a: Point, b: Point) -> f32 {
    let delta = p(b.x - a.x, b.y - a.y);
    let length = delta.x * delta.x + delta.y * delta.y;
    let t = if length > 0. {
        ((point.x - a.x) * delta.x + (point.y - a.y) * delta.y) / length
    } else {
        0.
    }
    .clamp(0., 1.);
    point.distance(a.offset(delta.x * t, delta.y * t))
}

#[test]
fn all_orbital_shapes_phases_layers_and_colors_clear_label_ink_without_opaque_halos() {
    for kind in OrbitalKind::ALL {
        for phase in Phase::ALL {
            for layer in [-1, 1] {
                for theme in [CanvasTheme::Light, CanvasTheme::Dark] {
                    for color in [Color::Ink, Color::Custom([40, 105, 165])] {
                        let mut doc = Document {
                            canvas_theme: theme,
                            ..Default::default()
                        };
                        let c = doc.add_atom("C", p(-36.373, 21.));
                        let n = doc.add_atom("N", p(0., 0.));
                        doc.add_bond(c, n, 1, "plain");
                        let atom = doc.atom_mut(n).unwrap();
                        atom.charge = 1;
                        atom.label_h = 3;
                        atom.isotope = 15;
                        let mut drawing = tool(true);
                        drawing.kind = GraphicKind::Orbital(*kind);
                        drawing.phase = *phase;
                        drawing.style.stroke = color;
                        drawing
                            .place(&mut doc, p(0., 0.), p(0., -42.), false, 10.)
                            .unwrap();
                        doc.graphics[0].layer = layer;
                        let before = doc.file_json().unwrap();
                        let labels = crate::scene::atom_label_ink_boxes(doc.atom(n).unwrap(), &doc);
                        assert!(
                            labels.len() >= 3,
                            "Fixture must include hydrogen, isotope and charge ink"
                        );
                        let canonical = crate::canvas_theme::canonical_document(&doc);
                        let graphic = &canonical.graphics[0];
                        let parts = orbital_parts_with_label_clearance(graphic, &canonical);
                        assert!(!parts.is_empty());
                        assert_eq!(parts.iter().any(|part| part.filled), *phase != Phase::Open);
                        for (lo, hi) in labels {
                            for x in 0..=4 {
                                for y in 0..=4 {
                                    let point = lo.offset(
                                        (hi.x - lo.x) * x as f32 / 4.,
                                        (hi.y - lo.y) * y as f32 / 4.,
                                    );
                                    for part in &parts {
                                        assert!(
                                            !part.filled || !filled_at(&part.commands, point),
                                            "{kind:?}/{phase:?}/{layer}/{theme:?} fill covers {point:?}"
                                        );
                                        if part.style.width_pt > 0. {
                                            for path in crate::graphics::flattened(&part.commands) {
                                                for edge in path.windows(2) {
                                                    assert!(
                                                        distance_to_edge(point, edge[0], edge[1])
                                                            > part.style.width() * 0.5
                                                                + DEFAULT.world(0.6),
                                                        "Stroke violates visible gap"
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let scene = crate::scene::primitives(&doc);
                        assert!(scene.iter().any(
                            |primitive| matches!(primitive,Primitive::Text {text,..} if text=="N")
                        ));
                        let raw_colors: Vec<_> = graphic
                            .parts()
                            .iter()
                            .filter_map(|part| part.style.fill)
                            .collect();
                        assert!(
                            parts
                                .iter()
                                .filter_map(|part| part.style.fill)
                                .all(|fill| raw_colors.contains(&fill)),
                            "Clearance must not invent paper-colored fill"
                        );
                        assert_eq!(
                            doc.file_json().unwrap(),
                            before,
                            "Rendering must not change atom positions, chemical data, orbital axes or layer"
                        );
                        let svg = crate::scene::svg(&doc);
                        assert!(svg.contains("<path") && svg.contains("<text"));
                        assert!(
                            !svg.contains("<rect"),
                            "Transparent SVG must not paint a background halo"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn distant_labels_leave_the_original_orbital_vectors_exact() {
    let mut doc = Document::default();
    let n = doc.add_atom("N", p(200., 0.));
    doc.atom_mut(n).unwrap().no_implicit = true;
    tool(true)
        .place(&mut doc, p(0., 0.), p(0., -42.), false, 10.)
        .unwrap();
    let graphic = &doc.graphics[0];
    let raw = graphic.parts();
    let shown = orbital_parts_with_label_clearance(graphic, &doc);
    assert_eq!(raw.len(), shown.len());
    for (raw, shown) in raw.iter().zip(&shown) {
        assert_eq!(raw.commands, shown.commands);
        assert_eq!(raw.style, shown.style);
        assert_eq!(raw.filled, shown.filled);
    }
}
