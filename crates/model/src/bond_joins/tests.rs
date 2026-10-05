use super::*;

#[test]
fn ring_corners_remain_opaque_across_sizes_widths_orientations_and_tilts() {
    for members in 3..=8 {
        for tilt in [0., 45., 75.] {
            for line_width in [0.6, 1.5] {
                for mode in ["plain", "bold", "wedge", "mixed"] {
                    let mut doc = Document::default();
                    doc.drawing_style.line_width_pt = line_width;
                    doc.drawing_style.bold_width_pt = line_width * 3.;
                    let ids = crate::editing::ring(&mut doc, Point::default(), members, false, 5.);
                    for (i, bond) in doc.bonds.iter_mut().enumerate() {
                        bond.display = if mode == "mixed" {
                            ["plain", "bold", "wedge"][i % 3]
                        } else {
                            mode
                        }
                        .into();
                        bond.color = crate::palette::Color::Custom([180, 68, 32]);
                        if i % 2 == 0 {
                            bond.reverse();
                        }
                    }
                    crate::projection::tilt(&mut doc, &ids, tilt, true);
                    crate::editing::transform(
                        &mut doc,
                        &ids,
                        crate::editing::Transform::Rotate(17.),
                    );
                    let svg = crate::scene::svg(&doc);
                    let xml = roxmltree::Document::parse(&svg).unwrap();
                    let bounds: Vec<f32> = xml
                        .root_element()
                        .attribute("viewBox")
                        .unwrap()
                        .split_whitespace()
                        .map(|s| s.parse().unwrap())
                        .collect();
                    let tree = resvg::usvg::Tree::from_str(
                        &svg,
                        &resvg::usvg::Options {
                            dpi: 72.,
                            ..Default::default()
                        },
                    )
                    .unwrap();
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
                    for atom in &doc.atoms {
                        let x = ((atom.position.x - bounds[0]) * tree.size().width() * scale
                            / bounds[2])
                            .floor() as u32;
                        let y = ((atom.position.y - bounds[1]) * tree.size().height() * scale
                            / bounds[3])
                            .floor() as u32;
                        assert_eq!(
                            pixmap.pixel(x, y).unwrap().alpha(),
                            255,
                            "gap at atom {}: {members}-ring, tilt {tilt}, width {line_width}, {mode}",
                            atom.id
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn reported_three_way_junctions_have_no_white_pixels_at_the_shared_atoms() {
    let mut doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/bond-join-regression.rsk"
    ))
    .unwrap();
    for color in [
        crate::palette::Color::Ink,
        crate::palette::Color::Custom([180, 68, 32]),
    ] {
        for b in &mut doc.bonds {
            b.color = color;
        }
        let svg = crate::scene::svg(&doc);
        let xml = roxmltree::Document::parse(&svg).unwrap();
        let bounds: Vec<f32> = xml
            .root_element()
            .attribute("viewBox")
            .unwrap()
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let tree = resvg::usvg::Tree::from_str(
            &svg,
            &resvg::usvg::Options {
                dpi: 72.,
                ..Default::default()
            },
        )
        .unwrap();
        for scale in [3., 8., 16.] {
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
            let sx = tree.size().width() * scale / bounds[2];
            let sy = tree.size().height() * scale / bounds[3];
            for id in [1, 2, 3] {
                let p = doc.atom(id).unwrap().position;
                let x = ((p.x - bounds[0]) * sx).floor() as u32;
                let y = ((p.y - bounds[1]) * sy).floor() as u32;
                assert_eq!(
                    pixmap.pixel(x, y).unwrap().alpha(),
                    255,
                    "white gap at atom {id}, scale {scale}"
                );
            }
        }
    }
}

#[test]
fn tapered_bonds_share_wide_end_corners_and_have_normal_width_tips() {
    for style in ["wedge", "hollow_wedge"] {
        let mut d = Document::default();
        let a = d.add_atom("C", Point::new(-30., -40.));
        let joint = d.add_atom("C", Point::default());
        let b = d.add_atom("C", Point::new(30., -40.));
        d.add_bond(a, joint, 1, style);
        d.add_bond(b, joint, 1, style);
        let first = cap(
            &d,
            &d.bonds[0],
            joint,
            Point::default(),
            d.atom(a).unwrap().position,
        );
        let second = cap(
            &d,
            &d.bonds[1],
            joint,
            Point::default(),
            d.atom(b).unwrap().position,
        );
        assert!(first[0].distance(second[1]) < 0.001);
        assert!(first[1].distance(second[0]) < 0.001);
        let tip = cap(
            &d,
            &d.bonds[0],
            a,
            d.atom(a).unwrap().position,
            Point::default(),
        );
        assert!((tip[0].distance(tip[1]) - d.drawing_style.line_width()).abs() < 0.001);
        assert!(
            polygon(
                &d,
                &d.bonds[0],
                d.atom(a).unwrap().position,
                Point::default()
            )
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())
        );
    }
}

#[test]
fn rasterized_joins_have_ink_on_both_sides_of_the_shared_vertex() {
    for angle in [0., 30., 60., 80.] {
        let mut doc = Document::default();
        let ids = crate::editing::ring(&mut doc, Point::new(100., 100.), 5, false, 5.);
        for b in &mut doc.bonds {
            b.display = "bold".into();
        }
        crate::projection::tilt(&mut doc, &ids, angle, true);
        let svg = crate::scene::svg(&doc);
        let xml = roxmltree::Document::parse(&svg).unwrap();
        let bounds: Vec<f32> = xml
            .root_element()
            .attribute("viewBox")
            .unwrap()
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let options = resvg::usvg::Options {
            dpi: 72.,
            ..Default::default()
        };
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
        let sx = tree.size().width() * scale / bounds[2];
        let sy = tree.size().height() * scale / bounds[3];
        for b in &doc.bonds {
            let a = doc.atom(b.a).unwrap().position;
            let z = doc.atom(b.b).unwrap().position;
            for t in [0.02, 0.98] {
                let p = Point::new(a.x + (z.x - a.x) * t, a.y + (z.y - a.y) * t);
                let x = ((p.x - bounds[0]) * sx).round() as u32;
                let y = ((p.y - bounds[1]) * sy).round() as u32;
                if pixmap.pixel(x, y).unwrap().alpha() <= 200 {
                    let path = std::env::temp_dir().join("reshiki-bond-join-failure.png");
                    pixmap.save_png(&path).unwrap();
                    panic!(
                        "white seam at tilt {angle}: {p:?}, pixel {x},{y}, alpha {} bounds {bounds:?}, image {}",
                        pixmap.pixel(x, y).unwrap().alpha(),
                        path.display()
                    );
                }
            }
        }
    }
}

#[test]
fn thick_thin_and_thick_thick_share_corners_at_many_tilts() {
    for tilt in [0., 30., 60., 80.] {
        for displays in [["bold", "bold"], ["bold", "plain"]] {
            let mut d = Document::default();
            let ids = crate::editing::ring(&mut d, Point::default(), 5, false, 5.);
            d.bonds[0].display = displays[0].into();
            d.bonds[1].display = displays[1].into();
            crate::projection::tilt(&mut d, &ids, tilt, true);
            let a = &d.bonds[0];
            let b = &d.bonds[1];
            let joint = [a.a, a.b]
                .into_iter()
                .find(|id| *id == b.a || *id == b.b)
                .unwrap();
            let p = d.atom(joint).unwrap().position;
            let cap_a = cap(
                &d,
                a,
                joint,
                p,
                d.atom(if a.a == joint { a.b } else { a.a })
                    .unwrap()
                    .position,
            );
            let cap_b = cap(
                &d,
                b,
                joint,
                p,
                d.atom(if b.a == joint { b.b } else { b.a })
                    .unwrap()
                    .position,
            );
            assert!(
                cap_a[0].distance(cap_b[1]) < 0.001,
                "tilt {tilt} {displays:?}"
            );
            assert!(cap_a[1].distance(cap_b[0]) < 0.001);
            for p2 in cap_a {
                assert!(p2.distance(p) <= 4. * half_width(&d, a).max(half_width(&d, b)) + 0.001);
            }
            d.validate().unwrap();
        }
    }
}
#[test]
fn labels_and_unconnected_crossings_are_not_joined() {
    let mut d = Document::default();
    let a = d.add_atom("N", Point::default());
    let b = d.add_atom("C", Point::new(40., 0.));
    let c = d.add_atom("C", Point::new(0., 40.));
    d.add_bond(a, b, 1, "bold");
    d.add_bond(a, c, 1, "plain");
    let clipped = Point::new(12., 0.);
    let ends = cap(&d, &d.bonds[0], a, clipped, d.atom(b).unwrap().position);
    assert_eq!(ends[0].x, 12.);
    assert_eq!(ends[1].x, 12.);
    let x = d.add_atom("C", Point::new(20., -20.));
    let y = d.add_atom("C", Point::new(20., 20.));
    d.add_bond(x, y, 1, "plain");
    assert!(!needed(&d, &d.bonds[2]));
}
