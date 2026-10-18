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
    // Independent winding/ray oracle for the emitted orbital fills.
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

fn text_is(primitive: &Primitive, label: &str) -> bool {
    match primitive {
        Primitive::Text { text, .. } => text == label,
        Primitive::Opacity { primitive, .. } => text_is(primitive, label),
        _ => false,
    }
}

/// The orbital's complete lobes, the halo above them, then the label.
fn assert_haloed_label(scene: &[Primitive], lobes: &[Part], label: &str) {
    let lobe_indices: Vec<_> = lobes
        .iter()
        .map(|lobe| {
            scene
                .iter()
                .position(|primitive| {
                    matches!(primitive, Primitive::Path { commands, filled, .. }
                        if *commands == lobe.commands && *filled == lobe.filled)
                })
                .expect("Every lobe is drawn complete, without a label cut")
        })
        .collect();
    let halos: Vec<_> = scene
        .iter()
        .enumerate()
        .filter(|(_, primitive)| matches!(primitive, Primitive::Halo { .. }))
        .collect();
    assert_eq!(halos.len(), 1, "One halo covers every haloed label");
    let (halo, Primitive::Halo { radius, primitives }) = halos[0] else {
        unreachable!()
    };
    assert_eq!(*radius, label_halo_radius());
    assert!(primitives.iter().any(|p| text_is(p, label)));
    assert!(lobe_indices.iter().all(|lobe| *lobe < halo));
    assert!(
        !scene[..halo].iter().any(|p| text_is(p, label)),
        "The haloed label is not also drawn beneath the orbital"
    );
    assert!(scene[halo..].iter().any(|p| text_is(p, label)));
}

#[test]
fn all_orbital_shapes_phases_layers_and_colors_keep_complete_lobes_under_haloed_labels() {
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
                        assert_eq!(
                            orbital_label_halos(&doc),
                            OrbitalHalos {
                                atoms: vec![(n, (layer >= 0).then_some(doc.graphics[0].id))],
                                front: if layer >= 0 {
                                    vec![doc.graphics[0].id]
                                } else {
                                    vec![]
                                },
                            }
                        );
                        let canonical = crate::canvas_theme::canonical_document(&doc);
                        let lobes = canonical.graphics[0].parts();
                        assert_eq!(lobes.iter().any(|part| part.filled), *phase != Phase::Open);
                        assert_haloed_label(&crate::scene::primitives(&doc), &lobes, "N");
                        assert_eq!(
                            doc.file_json().unwrap(),
                            before,
                            "Rendering must not change atom positions, chemical data, orbital axes or layer"
                        );
                        let [r, g, b] = theme.background();
                        let paper = format!("stroke=\"rgb({r},{g},{b})\"");
                        let figure = crate::scene::svg_with_background(&doc);
                        assert!(figure.contains(&paper), "Figures paint a paper halo");
                        assert!(!figure.contains("<mask"));
                        let svg = crate::scene::svg(&doc);
                        assert!(
                            !svg.contains(&paper),
                            "Transparent SVG must not paint a paper-colored halo"
                        );
                        let mask = svg.find("mask=\"url(#reshiki-label-halo-1)\"").unwrap();
                        assert_eq!(svg.matches("<rect").count(), 1, "Only the mask's extent");
                        assert!(svg.rfind(">N</text>").unwrap() > mask);
                    }
                }
            }
        }
    }
}

#[test]
fn distant_labels_keep_their_place_without_a_halo() {
    let mut doc = Document::default();
    let n = doc.add_atom("N", p(200., 0.));
    doc.atom_mut(n).unwrap().no_implicit = true;
    tool(true)
        .place(&mut doc, p(0., 0.), p(0., -42.), false, 10.)
        .unwrap();
    assert_eq!(orbital_label_halos(&doc), OrbitalHalos::default());
    let scene = crate::scene::primitives(&doc);
    assert!(
        !scene
            .iter()
            .any(|primitive| matches!(primitive, Primitive::Halo { .. }))
    );
    assert!(scene.iter().any(|p| text_is(p, "N")));
    assert!(!crate::scene::svg(&doc).contains("<mask"));
}

/// Issue #318: a p orbital on formaldehyde's O (and C) kept a box-shaped cut
/// around the O label. Lobes now stay complete; the label sits above them in
/// a halo that a transparent export knocks out of the orbital ink.
#[test]
fn formaldehyde_oxygen_p_orbital_keeps_complete_lobes_and_a_readable_label() {
    let mut doc = Document::default();
    let c = doc.add_atom("C", p(0., 0.));
    let o = doc.add_atom("O", p(42., 0.));
    doc.add_bond(c, o, 2, "plain");
    for atom in [o, c] {
        let at = doc.atom(atom).unwrap().position;
        tool(true)
            .place(&mut doc, at, at.offset(0., -42.), false, 10.)
            .unwrap();
    }
    assert_eq!(
        orbital_label_halos(&doc),
        OrbitalHalos {
            atoms: vec![(o, None)],
            front: vec![]
        }
    );
    let scene = crate::scene::primitives(&doc);
    let lobes: Vec<_> = doc.graphics.iter().flat_map(|g| g.parts()).collect();
    assert_haloed_label(&scene, &lobes, "O");

    let (lo, hi) = crate::scene::bounds(&scene);
    let ink = crate::scene::atom_label_ink_boxes(doc.atom(o).unwrap(), &doc);
    let (ink_lo, ink_hi) = ink[0];
    let center = ink_lo.lerp(ink_hi, 0.5);
    let radius = label_halo_radius();
    let o_fills: Vec<_> = doc.graphics[0]
        .parts()
        .into_iter()
        .filter(|part| part.filled)
        .collect();
    let in_lobe = |point: Point| o_fills.iter().any(|part| filled_at(&part.commands, point));
    // Halfway into the halo above and below the glyph, inside a filled lobe.
    let halo_points: Vec<_> = [
        p(center.x, ink_lo.y - radius * 0.5),
        p(center.x, ink_hi.y + radius * 0.5),
    ]
    .into_iter()
    .filter(|point| in_lobe(*point))
    .collect();
    assert!(
        !halo_points.is_empty(),
        "Fixture: a filled lobe meets the label"
    );
    let lobe_point = [-30., 30.]
        .into_iter()
        .map(|dy| doc.atom(o).unwrap().position.offset(0., dy))
        .find(|point| in_lobe(*point))
        .unwrap();

    let render = |svg: String| {
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
        let scale = 8.;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(
            (tree.size().width() * scale).ceil() as u32,
            (tree.size().height() * scale).ceil() as u32,
        )
        .unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let per_world = tree.size().width() / (hi.x - lo.x) * scale;
        move |point: Point| {
            let x = ((point.x - lo.x) * per_world) as u32;
            let y = ((point.y - lo.y) * per_world) as u32;
            pixmap.pixel(x, y).unwrap().demultiply()
        }
    };
    let transparent = render(crate::scene::svg(&doc));
    assert_eq!(
        transparent(lo.offset(0.5, 0.5)).alpha(),
        0,
        "Surround stays transparent"
    );
    let lobe = transparent(lobe_point);
    assert_eq!(lobe.alpha(), 255, "The lobe is drawn complete");
    assert!(lobe.red() < 64);
    for point in &halo_points {
        assert!(
            transparent(*point).alpha() < 16,
            "The halo knocks the lobe out to transparency at {point:?}"
        );
    }
    assert!(
        transparent(center).alpha() < 16,
        "The middle of the O stays clear of lobe ink"
    );
    let figure = render(crate::scene::svg_with_background(&doc));
    for point in &halo_points {
        let paper = figure(*point);
        assert!(paper.alpha() == 255 && paper.red() > 240 && paper.blue() > 240);
    }
    assert!(figure(lobe_point).red() < 64);
}

#[test]
fn haloed_labels_sit_right_above_the_topmost_orbital_reaching_them() {
    let mut doc = Document::default();
    let n = doc.add_atom("N", p(0., 0.));
    doc.atom_mut(n).unwrap().no_implicit = true;
    tool(true)
        .place(&mut doc, p(0., 0.), p(0., -42.), false, 10.)
        .unwrap();
    doc.graphics[0].layer = 1;
    let orbital = doc.graphics[0].id;
    let mut cover = crate::graphics::Graphic::dragged(
        doc.next_id(),
        GraphicKind::Ellipse,
        p(-42., -42.),
        p(42., 42.),
        GraphicStyle {
            fill: Some(Color::Ink),
            ..Default::default()
        },
        crate::graphics::BracketSides::Both,
        false,
    );
    cover.layer = 2;
    let cover_parts = cover.parts();
    doc.graphics.push(cover);
    assert_eq!(orbital_label_halos(&doc).atoms, vec![(n, Some(orbital))]);
    let scene = crate::scene::primitives(&doc);
    assert_haloed_label(&scene, &doc.graphics[0].parts(), "N");
    let label = scene.iter().rposition(|p| text_is(p, "N")).unwrap();
    let ellipse = scene
        .iter()
        .position(|primitive| {
            matches!(primitive, Primitive::Path { commands, .. } if *commands == cover_parts[0].commands)
        })
        .unwrap();
    assert!(
        label < ellipse,
        "A graphic stacked above the orbital still covers the label, as hit testing expects"
    );
}

#[test]
fn each_haloed_label_stacks_by_the_orbitals_that_reach_it() {
    // Codex review: a distant front orbital must not lift an unrelated label
    // above a graphic that covers it.
    let mut doc = Document::default();
    let near = doc.add_atom("N", p(0., 0.));
    let far = doc.add_atom("N", p(300., 0.));
    for (atom, at, layer) in [(near, p(0., 0.), -1), (far, p(300., 0.), 2)] {
        doc.atom_mut(atom).unwrap().no_implicit = true;
        tool(true)
            .place(&mut doc, at, at.offset(0., -42.), false, 10.)
            .unwrap();
        doc.graphics.last_mut().unwrap().layer = layer;
    }
    let far_orbital = doc.graphics[1].id;
    let mut cover = crate::graphics::Graphic::dragged(
        doc.next_id(),
        GraphicKind::Rectangle,
        p(-20., -20.),
        p(20., 20.),
        GraphicStyle {
            fill: Some(Color::Ink),
            ..Default::default()
        },
        crate::graphics::BracketSides::Both,
        false,
    );
    cover.layer = 1;
    let cover_parts = cover.parts();
    doc.graphics.push(cover);
    assert_eq!(
        orbital_label_halos(&doc),
        OrbitalHalos {
            atoms: vec![(near, None), (far, Some(far_orbital))],
            front: vec![far_orbital],
        }
    );
    let scene = crate::scene::primitives(&doc);
    let halos: Vec<_> = scene
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p, Primitive::Halo { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(halos.len(), 2);
    let cover = scene
        .iter()
        .position(|primitive| {
            matches!(primitive, Primitive::Path { commands, .. } if *commands == cover_parts[0].commands)
        })
        .unwrap();
    assert!(
        halos[0] < cover,
        "The near label stays under its covering rectangle"
    );
    assert!(halos[1] > cover);
}

#[test]
fn labels_with_zero_rear_opacity_are_not_raised_over_orbitals() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let paint = crate::rear_opacity::Paint::new(&doc);
    let hidden = ids
        .iter()
        .copied()
        .find(|id| paint.atom(*id) == 0.)
        .unwrap();
    let exposed = ids
        .iter()
        .copied()
        .find(|id| paint.atom(*id) == 1.)
        .unwrap();
    for id in [hidden, exposed] {
        let atom = doc.atom_mut(id).unwrap();
        atom.element = "O".into();
        atom.no_implicit = true;
        let at = atom.position;
        tool(false)
            .place(&mut doc, at, at.offset(0., -42.), false, 10.)
            .unwrap();
    }
    let halos = orbital_label_halos(&doc);
    assert!(halos.contains(exposed));
    assert!(!halos.contains(hidden), "No label is drawn at zero opacity");
}
