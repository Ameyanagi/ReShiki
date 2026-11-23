//! Liquid fill edges and surface strokes share safe geometry.
use super::*;

fn rectangle(left: f64, top: f64, right: f64, bottom: f64) -> Vec<Xy> {
    vec![[left, top], [right, top], [right, bottom], [left, bottom]]
}

fn end(segment: Seg) -> Xy {
    match segment {
        Seg::Line(p) | Seg::Cubic(_, _, p) => p,
        _ => panic!("not an edge: {segment:?}"),
    }
}

fn edges(path: &[Seg]) -> Vec<(Xy, Seg)> {
    let (mut at, mut start) = ([0.; 2], [0.; 2]);
    let mut out = Vec::new();
    for &segment in path {
        match segment {
            Seg::Move(p) => (at, start) = (p, p),
            Seg::Close => {
                if at != start {
                    out.push((at, Seg::Line(start)));
                }
                at = start;
            }
            _ => {
                out.push((at, segment));
                at = end(segment);
            }
        }
    }
    out
}

/// A stroke must be an actual fill edge, including both controls and direction.
fn assert_shared(paths: &[(Ink, Vec<Seg>)], count: usize) {
    let mut strokes = 0;
    let mut saw_stroke = false;
    for (ink, path) in paths {
        if *ink == Ink::Fill {
            assert!(!saw_stroke, "a fill followed a surface stroke");
            continue;
        }
        saw_stroke = true;
        strokes += 1;
        let [Seg::Move(a), segment] = path.as_slice() else {
            panic!("unexpected surface: {path:?}");
        };
        let b = end(*segment);
        let reverse = match *segment {
            Seg::Cubic(c1, c2, _) => Seg::Cubic(c2, c1, *a),
            _ => Seg::Line(*a),
        };
        assert!(
            paths
                .iter()
                .filter(|(ink, _)| *ink == Ink::Fill)
                .any(|(_, fill)| edges(fill).iter().any(|&(from, edge)| {
                    (from == *a && edge == *segment) || (from == b && edge == reverse)
                }))
        );
    }
    assert_eq!(strokes, count);
}

#[test]
fn curved_surfaces_are_the_fill_boundary_at_the_original_wall_contacts() {
    let region = vec![rectangle(0., 0., 40., 30.)];
    for meniscus in [Meniscus::Concave, Meniscus::Convex] {
        for scale in [0.5, 1., 10.] {
            let y = level(&region, 0.5);
            let cut = surfaces(&below(&region, y), y);
            let paths = liquid(&region, 0.5, meniscus, scale);
            assert_shared(&paths, 1);
            let (_, stroke) = paths.last().unwrap();
            let [Seg::Move(a), Seg::Cubic(c1, c2, b)] = stroke.as_slice() else {
                panic!("safe surface should curve: {stroke:?}");
            };
            assert_eq!((a[0], b[0]), cut[0]);
            assert!((a[1] - y).abs() < 1e-6 && (b[1] - y).abs() < 1e-6);
            let width = b[0] - a[0];
            assert!((c1[0] - (a[0] + width / 3.)).abs() < 1e-12);
            assert!((c2[0] - (a[0] + 2. * width / 3.)).abs() < 1e-12);
            let m = (1.6_f64 * scale).min(width / 8.)
                * if meniscus == Meniscus::Convex {
                    -1.
                } else {
                    1.
                };
            let center = (a[1] + 3. * c1[1] + 3. * c2[1] + b[1]) / 8.;
            assert!((center - y - 0.2 * m).abs() < 1e-6);
            let old = below(&region, y);
            let wall_edges: Vec<_> = super::super::geometry::edges(&old[0][0], true)
                .filter(|(a, b)| (a[1] - y).abs() >= 1e-6 || (b[1] - y).abs() >= 1e-6)
                .collect();
            let fill_edges = edges(&paths[0].1);
            for (a, b) in wall_edges {
                assert!(fill_edges.contains(&(a, Seg::Line(b))));
            }
        }
    }
}

#[test]
fn unsafe_curves_share_a_straight_edge_near_walls_and_holes() {
    let thin = vec![rectangle(0., 0., 20., 0.02)];
    for meniscus in [Meniscus::Concave, Meniscus::Convex] {
        let paths = liquid(&thin, 0.5, meniscus, 1.);
        assert_shared(&paths, 1);
        assert!(
            paths
                .iter()
                .all(|(_, path)| !path.iter().any(|s| matches!(s, Seg::Cubic(..))))
        );
    }
    // All four controls avoid this hole, but the interior of their hull does not.
    let region = vec![
        rectangle(0., 0., 20., 20.),
        rectangle(9., 10.15, 11., 10.25),
    ];
    assert_eq!(
        meniscus_edge([0., 10.], [20., 10.], 10., Meniscus::Concave, 1., &region),
        Seg::Line([20., 10.])
    );
    let paths = liquid(&region, share_below(&region, 10.), Meniscus::Concave, 1.);
    assert_shared(&paths, 1);
    assert!(matches!(
        paths.last().unwrap().1.as_slice(),
        [Seg::Move(_), Seg::Line(_)]
    ));
}

#[test]
fn disconnected_pools_preserve_holes_and_the_widest_surface_handle() {
    let region = vec![
        rectangle(0., 0., 20., 20.),
        rectangle(40., 0., 70., 30.),
        rectangle(48., 20., 54., 24.),
    ];
    let fill = share_below(&region, 10.);
    let y = level(&region, fill);
    let pools = below(&region, y);
    let paths = liquid(&region, fill, Meniscus::Concave, 1.);
    assert_shared(&paths, 2);
    assert_eq!(paths.iter().filter(|(ink, _)| *ink == Ink::Fill).count(), 2);
    assert_eq!(
        paths
            .iter()
            .filter(|(_, path)| path.iter().any(|s| matches!(s, Seg::Cubic(..))))
            .count(),
        4
    );
    for (pool, (_, path)) in pools.iter().zip(paths.iter()) {
        let holes: Vec<_> = pool.iter().skip(1).flat_map(|c| polygon(c, true)).collect();
        assert!(path.ends_with(&holes));
    }
    let at = surface(&region, fill).unwrap();
    assert!((at[0] - 55.).abs() < 1e-6 && (at[1] - 10.).abs() < 1e-6);
    assert!((share_below(&region, at[1]) - fill).abs() < 1e-6);
}

#[test]
fn empty_full_and_flat_paths_keep_their_polygon_fast_paths() {
    let region = vec![rectangle(0., 0., 20., 20.), rectangle(5., 5., 10., 10.)];
    for meniscus in [Meniscus::Concave, Meniscus::Convex, Meniscus::Flat] {
        assert!(liquid(&region, 0., meniscus, 1.).is_empty());
        assert!(liquid(&[], 0.5, meniscus, 1.).is_empty());
        let expected: Vec<_> = below(&region, -1.)
            .iter()
            .map(|pool| {
                (
                    Ink::Fill,
                    pool.iter()
                        .flat_map(|c| polygon(c, true))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(liquid(&region, 1., meniscus, 1.), expected);
    }
    let y = level(&region, 0.5);
    let expected: Vec<_> = below(&region, y)
        .iter()
        .map(|pool| {
            (
                Ink::Fill,
                pool.iter()
                    .flat_map(|c| polygon(c, true))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(liquid(&region, 0.5, Meniscus::Flat, 1.), expected);
}
