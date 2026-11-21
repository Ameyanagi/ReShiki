use super::*;

fn graphic(axis_x: Point, axis_y: Point, borders: Vec<Border>) -> Graphic {
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(1., 1.),
        Default::default(),
        Default::default(),
        false,
    );
    g.kind = GraphicKind::Table;
    g.origin = Point::new(20., 30.);
    g.axis_x = axis_x;
    g.axis_y = axis_y;
    g.table = Some(Box::new(Table {
        cells: vec![Cell {
            id: 1,
            lo: Point::default(),
            hi: Point::new(1., 1.),
            members: vec![],
            borders,
        }],
    }));
    g
}
fn border(side: Side, width_pt: f32, pattern: LinePattern) -> Border {
    Border {
        side,
        width_pt,
        pattern,
        color: Color::Custom([200, 30, 60]),
    }
}
fn points(part: &crate::scientific::Part) -> Vec<Point> {
    part.commands
        .iter()
        .flat_map(PathCommand::iter_points)
        .collect()
}
fn frames() -> [(Point, Point); 4] {
    [
        (Point::new(120., 0.), Point::new(0., 90.)),
        (Point::new(0., 120.), Point::new(-90., 0.)),
        (Point::new(-120., 0.), Point::new(0., 90.)),
        (Point::new(120., 30.), Point::new(-20., 90.)),
    ]
}

#[test]
fn solid_single_edges_end_flat_at_the_grid_and_preserve_physical_width() {
    for (x, y) in frames() {
        for width in [0.1, 0.4, 5., 12.] {
            for side in [Side::Left, Side::Top, Side::Bottom, Side::Right] {
                let g = graphic(x, y, vec![border(side, width, LinePattern::Solid)]);
                let before = g.clone();
                let cell = &g.table.as_ref().unwrap().cells[0];
                let parts = cell.border_parts(&g);
                assert_eq!(parts.len(), 1);
                let part = &parts[0];
                assert!(part.filled && part.style.width_pt == 0.);
                assert_eq!(part.style.fill, Some(Color::Custom([200, 30, 60])));
                assert!(matches!(part.commands.last(), Some(PathCommand::Close)));
                let p = Frame::of(&g).unwrap();
                let (a, b) = match side {
                    Side::Left => (p.point(Point::default()), p.point(Point::new(0., 1.))),
                    Side::Top => (p.point(Point::default()), p.point(Point::new(1., 0.))),
                    Side::Bottom => (p.point(Point::new(0., 1.)), p.point(Point::new(1., 1.))),
                    Side::Right => (p.point(Point::new(1., 0.)), p.point(Point::new(1., 1.))),
                };
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let length = dx.hypot(dy);
                let half = crate::style::DEFAULT.world(width) * 0.5;
                for q in points(part) {
                    let along = ((q.x - a.x) * dx + (q.y - a.y) * dy) / length;
                    let across = (-(q.x - a.x) * dy + (q.y - a.y) * dx) / length;
                    assert!(along.abs() < 0.0001 || (along - length).abs() < 0.0001);
                    assert!((across.abs() - half).abs() < 0.0001);
                }
                assert_eq!(g, before);
            }
        }
    }
    assert!(
        GraphicStyle::default().round_caps(),
        "Ordinary solid graphics keep their caps"
    );
}

#[test]
fn closed_solid_borders_share_miter_vertices_and_stay_inside_shared_bounds() {
    for (x, y) in frames() {
        let g = graphic(
            x,
            y,
            vec![
                border(Side::Left, 0.8, LinePattern::Solid),
                border(Side::Top, 1.2, LinePattern::Solid),
                border(Side::Bottom, 0.8, LinePattern::Solid),
                border(Side::Right, 5., LinePattern::Solid),
            ],
        );
        let parts = g.table.as_ref().unwrap().cells[0].border_parts(&g);
        assert_eq!(parts.len(), 4);
        // Native order is left, right, top, bottom. Each adjacent pair shares
        // the outer intersection, so a closed grid has no missing corner wedge.
        for (a, b) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            assert!(
                points(&parts[a])
                    .iter()
                    .any(|p| points(&parts[b]).iter().any(|q| p.distance(*q) < 0.0001))
            );
        }
        let (lo, hi) = bounds(&g).unwrap();
        for q in parts.iter().flat_map(points) {
            assert!(q.is_finite() && q.x >= lo.x && q.x <= hi.x && q.y >= lo.y && q.y <= hi.y);
        }
    }
}

#[test]
fn dashed_and_dotted_borders_keep_the_existing_stroke_path_and_pattern() {
    for pattern in [LinePattern::Dashed, LinePattern::Dotted] {
        let g = graphic(
            Point::new(120., 0.),
            Point::new(0., 90.),
            vec![border(Side::Top, 5., pattern)],
        );
        let parts = g.table.as_ref().unwrap().cells[0].border_parts(&g);
        assert_eq!(parts.len(), 1);
        assert!(!parts[0].filled);
        assert_eq!(parts[0].style.width_pt, 5.);
        assert_eq!(parts[0].style.pattern, pattern);
        assert_eq!(
            parts[0].commands,
            [
                PathCommand::Move(g.origin),
                PathCommand::Line(g.origin.offset(120., 0.))
            ]
        );
        assert_eq!(parts[0].style.round_caps(), pattern == LinePattern::Dotted);
        assert!(!parts[0].style.dashes().is_empty());
    }
}

#[test]
fn faint_flat_bands_keep_subpixel_coverage_without_changing_their_width() {
    for width in [0.1, 0.4, 5.] {
        let g = graphic(
            Point::new(1500., 0.),
            Point::new(0., 90.),
            vec![border(Side::Bottom, width, LinePattern::Solid)],
        );
        let part = g.table.as_ref().unwrap().cells[0]
            .border_parts(&g)
            .remove(0);
        let primitive = crate::scene::Primitive::Path {
            commands: part.commands,
            style: part.style,
            filled: part.filled,
        };
        let svg = crate::scene::svg_part(&[primitive], Point::default(), Point::new(1600., 200.));
        let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
        for zoom in [0.05, 0.2, 1., 2.] {
            let mut pix =
                resvg::tiny_skia::Pixmap::new((1600. * zoom) as u32, (200. * zoom) as u32).unwrap();
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(zoom, zoom),
                &mut pix.as_mut(),
            );
            let area = pix
                .pixels()
                .iter()
                .map(|p| f64::from(p.alpha()) / 255.)
                .sum::<f64>();
            let expected = f64::from(1500. * crate::style::DEFAULT.world(width) * zoom * zoom);
            assert!(
                area > 0. && (area - expected).abs() <= expected * 0.35,
                "{width}pt at {zoom}: {area} vs {expected}"
            );
        }
    }
}
