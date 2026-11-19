use super::*;

/// Atoms at `points` joined by `(a, b, order, display)` bonds.
fn drawing(points: &[(f32, f32)], bonds: &[(usize, usize, u8, &str)]) -> Document {
    let mut doc = Document::default();
    let ids: Vec<_> = points
        .iter()
        .map(|&(x, y)| doc.add_atom("C", Point::new(x, y)))
        .collect();
    for &(a, b, order, display) in bonds {
        doc.add_bond(ids[a], ids[b], order, display);
    }
    doc
}

/// A regular ring of `size` bond-length sides about (`x`, `y`).
fn ring_points(size: usize, x: f32, y: f32, turn: f32) -> Vec<(f32, f32)> {
    let side = Document::default().drawing_style.bond_length_world;
    let radius = side / (2. * (std::f32::consts::PI / size as f32).sin());
    (0..size)
        .map(|i| {
            let angle = turn + std::f32::consts::TAU * i as f32 / size as f32;
            (x + radius * angle.cos(), y + radius * angle.sin())
        })
        .collect()
}

fn ring(size: usize, orders: impl Fn(usize) -> u8) -> Document {
    let bonds: Vec<_> = (0..size)
        .map(|i| (i, (i + 1) % size, orders(i), "plain"))
        .collect();
    drawing(&ring_points(size, 0., 0., 0.5), &bonds)
}

fn lines(primitives: &[Primitive]) -> Vec<(Vec<Point>, bool, f32, crate::palette::Color)> {
    primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Polyline {
                points,
                closed,
                width,
                color,
            } => Some((points.clone(), *closed, *width, *color)),
            _ => None,
        })
        .collect()
}

/// Filled shapes, one per closed subpath.
fn fills(primitives: &[Primitive]) -> Vec<Vec<Point>> {
    let mut shapes = vec![];
    for p in primitives {
        match p {
            Primitive::Polygon(points) => shapes.push(points.clone()),
            Primitive::Path {
                commands,
                filled: true,
                ..
            } => {
                for command in commands {
                    match command {
                        crate::graphics::PathCommand::Move(p) => shapes.push(vec![*p]),
                        crate::graphics::PathCommand::Line(p) => {
                            shapes.last_mut().unwrap().push(*p)
                        }
                        _ => {}
                    }
                }
            }
            Primitive::Opacity { primitive, .. } => {
                shapes.extend(fills(std::slice::from_ref(primitive.as_ref())))
            }
            _ => {}
        }
    }
    shapes
}

fn area(points: &[Point]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - a.y * b.x)
        .sum::<f32>()
        .abs()
        / 2.
}

#[test]
fn rings_export_as_one_closed_mitred_line() {
    for (doc, rails) in [
        (ring(6, |_| 1), 0),
        (ring(6, |i| 1 + (i % 2 == 0) as u8), 3),
    ] {
        let width = doc.drawing_style.line_width();
        assert!(
            !fills(&primitives(&doc)).is_empty(),
            "The canvas still fills the joined outline"
        );
        assert!(lines(&primitives(&doc)).is_empty());
        let vector = vector_primitives(&doc);
        let lines = lines(&vector);
        assert_eq!(lines.len(), 1, "{vector:?}");
        let (points, closed, line_width, _) = &lines[0];
        assert!(closed);
        assert_eq!(points.len(), 6);
        assert_eq!(*line_width, width);
        assert!(fills(&vector).is_empty(), "{vector:?}");
        let inner = vector
            .iter()
            .filter(|p| matches!(p, Primitive::Line(..)))
            .count();
        assert_eq!(inner, rails, "Inner double rails stay lines");
    }
}

#[test]
fn chains_join_through_carbons_and_end_flat_at_labels() {
    let mut doc = drawing(
        &[(0., 0.), (26., 15.), (52., 0.), (78., 15.)],
        &[(0, 1, 1, "plain"), (1, 2, 1, "plain"), (2, 3, 1, "plain")],
    );
    doc.atoms[3].element = "O".into();
    let oxygen = doc.atoms[3].position;
    let vector = vector_primitives(&doc);
    let lines = lines(&vector);
    assert_eq!(lines.len(), 1, "{vector:?}");
    let (points, closed, ..) = &lines[0];
    assert!(!closed);
    assert_eq!(points.len(), 4);
    assert_eq!(points[0], doc.atoms[0].position);
    let end = points[3];
    assert!(end != oxygen && end.distance(oxygen) < 26., "Trimmed at O");
    assert!(fills(&vector).is_empty(), "{vector:?}");
}

#[test]
fn fused_atoms_end_three_lines_without_fills() {
    let mut points = ring_points(6, 0., 0., 0.);
    let side = Document::default().drawing_style.bond_length_world;
    // The second ring shares the edge from vertex 0 to vertex 1.
    let shift = side * 3f32.sqrt();
    let twin = ring_points(6, shift * (30f32.to_radians()).cos(), shift * 0.5, 0.);
    let mut bonds: Vec<_> = (0..6).map(|i| (i, (i + 1) % 6, 1, "plain")).collect();
    let mut index = vec![];
    for p in twin {
        let near = points
            .iter()
            .position(|q| (q.0 - p.0).hypot(q.1 - p.1) < 0.01);
        index.push(near.unwrap_or_else(|| {
            points.push(p);
            points.len() - 1
        }));
    }
    for i in 0..6 {
        let (a, b) = (index[i], index[(i + 1) % 6]);
        if !bonds
            .iter()
            .any(|&(x, y, ..)| (x, y) == (a, b) || (x, y) == (b, a))
        {
            bonds.push((a, b, 1, "plain"));
        }
    }
    assert_eq!(bonds.len(), 11, "Two hexagons sharing one edge");
    let doc = drawing(&points, &bonds);
    let vector = vector_primitives(&doc);
    let mut runs: Vec<_> = lines(&vector)
        .into_iter()
        .map(|(points, closed, ..)| (points.len(), closed))
        .collect();
    runs.sort();
    assert_eq!(runs, [(2, false), (6, false), (6, false)]);
    assert!(fills(&vector).is_empty(), "{vector:?}");
}

#[test]
fn a_wedge_neighbour_keeps_the_lines_half_of_their_mitre_filled() {
    let doc = drawing(
        &[(0., 0.), (26., 15.), (52., 0.)],
        &[(0, 1, 1, "plain"), (1, 2, 1, "wedge")],
    );
    let width = doc.drawing_style.line_width();
    let vector = vector_primitives(&doc);
    assert_eq!(lines(&vector).len(), 1);
    let joint = doc.atoms[1].position;
    let (open, wedge): (Vec<_>, Vec<_>) = fills(&vector)
        .into_iter()
        .partition(|shape| shape.iter().all(|p| p.distance(joint) < width * 2.));
    assert_eq!(wedge.len(), 1, "The wedge stays filled");
    assert_eq!(open.len(), 1, "{open:?}");
    assert!(area(&open[0]) > 0. && area(&open[0]) < width * width);
}

#[test]
fn colors_split_lines_and_fill_only_the_open_mitre_halves() {
    let mut doc = ring(6, |_| 1);
    doc.bonds[0].color = crate::palette::Color::Custom([200, 0, 0]);
    let width = doc.drawing_style.line_width();
    let vector = vector_primitives(&doc);
    let mut runs: Vec<_> = lines(&vector)
        .into_iter()
        .map(|(points, closed, _, color)| (color, points.len(), closed))
        .collect();
    runs.sort_by_key(|run| run.1);
    assert_eq!(
        runs,
        [
            (crate::palette::Color::Custom([200, 0, 0]), 2, false),
            (crate::palette::Color::Ink, 6, false)
        ]
    );
    let open = fills(&vector);
    assert_eq!(open.len(), 4, "Each color fills its half of two corners");
    assert!(open.iter().all(|shape| area(shape) < width * width));
}

#[test]
fn ketone_rails_stop_on_the_ring_line_with_round_ends() {
    let mut points = ring_points(6, 0., 0., 0.);
    let side = Document::default().drawing_style.bond_length_world;
    let (x, y) = points[0];
    let length = x.hypot(y);
    points.push((x + x / length * side, y + y / length * side));
    let mut bonds: Vec<_> = (0..6).map(|i| (i, (i + 1) % 6, 1, "plain")).collect();
    bonds.push((0, 6, 2, "plain"));
    let mut doc = drawing(&points, &bonds);
    doc.atoms[6].element = "O".into();
    let vector = vector_primitives(&doc);
    assert_eq!(lines(&vector).len(), 1);
    assert!(fills(&vector).is_empty(), "{vector:?}");
    let rails: Vec<_> = vector
        .iter()
        .filter_map(|p| match p {
            Primitive::Line(a, b, _) => Some((*a, *b)),
            _ => None,
        })
        .collect();
    assert_eq!(rails.len(), 2);
}

#[test]
fn svg_writes_bond_lines_as_butt_mitred_strokes_of_the_line_width() {
    let doc = ring(6, |_| 1);
    let svg = svg_of(&doc, vector_primitives(&doc), None);
    let width = doc.drawing_style.line_width();
    assert!(!svg.contains("<polygon"), "{svg}");
    assert!(!svg.contains("fill=\"rgb"), "{svg}");
    assert!(
        svg.contains(&format!(
            "stroke-width=\"{width}\" stroke-linecap=\"butt\" stroke-linejoin=\"miter\" stroke-miterlimit=\"4\""
        )),
        "{svg}"
    );
    assert!(svg.contains("Z\""), "The ring path closes");
}

#[test]
fn stroked_bounds_match_the_filled_outlines() {
    for doc in [
        ring(6, |_| 1),
        ring(5, |_| 1),
        drawing(
            &[(0., 0.), (26., 15.), (52., 0.)],
            &[(0, 1, 1, "plain"), (1, 2, 1, "wedge")],
        ),
    ] {
        let (lo, hi) = bounds(&primitives(&doc));
        let (vlo, vhi) = bounds(&vector_primitives(&doc));
        for (a, b) in [(lo.x, vlo.x), (lo.y, vlo.y), (hi.x, vhi.x), (hi.y, vhi.y)] {
            assert!((a - b).abs() < 1e-3, "{lo:?} {hi:?} vs {vlo:?} {vhi:?}");
        }
    }
}
