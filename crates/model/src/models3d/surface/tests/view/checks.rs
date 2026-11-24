use super::*;

#[test]
fn contour_thinning_preserves_a_thin_occluder_hole() {
    let output = visible(&Input {
        positions: &[
            [0., 0., 0.],
            [100., 0., 0.],
            [100., 100., 0.],
            [0., 100., 0.],
        ],
        triangles: &[[0, 1, 2], [0, 2, 3]],
        positive: &[true; 2],
        front: &[true; 2],
        lightness: &[1.; 4],
        occluders: &[Occluder::Stick {
            a: [30., 50., 1.],
            b: [70., 50., 1.],
            half: [0.02; 2],
            depth: [0.; 2],
        }],
        bands: 1,
        tolerance: 0.1,
    });
    let region = &output.layers[0].region;
    assert!(contains(region, [50., 49.]));
    assert!(!contains(region, [50., 50.]));
}

#[test]
fn contour_thinning_rejects_a_self_intersection() {
    // The longer version loses less than 10% of its area even when it crosses.
    for height in [5., 24.] {
        let contour = [
            [-4., -2.],
            [-4., -4.],
            [5., -4.],
            [-1., -3.],
            [3., 1.],
            [2., height],
        ];
        assert_eq!(
            simplify_closed(&contour, 2.),
            contour.map(|p| p.map(|x| x as f32))
        );
    }
}

#[test]
fn contour_thinning_rejects_excessive_area_loss() {
    let contour = [[0., 0.], [10., 0.], [10., 5.], [5., 8.], [0., 5.]];
    assert_eq!(
        simplify_closed(&contour, 3.),
        contour.map(|p| p.map(|x| x as f32))
    );
}

#[test]
fn translated_depth_plane_recovers_its_anchor_depth() {
    let origin = 16_384.;
    let input = Input {
        positions: &[
            [origin, origin, 1e-6],
            [origin + 3., origin, 4.],
            [origin, origin + 5., 0.42],
        ],
        triangles: &[[0, 1, 2]],
        front: &[true],
        positive: &[true],
        lightness: &[1.; 3],
        occluders: &[],
        bands: 1,
        tolerance: 0.,
    };
    assert_eq!(front_depth(&input, [origin; 2]), Some(1e-6));
}

#[test]
fn translated_piece_planes_resolve_a_depth_gap() {
    let o = 16_384.;
    // Heights agree along the far edge. At the overlap centroid the second
    // face is nearer by 1e-12 / 3, which a global plane intercept loses.
    let positions: Vec<_> = [0., 1e-12]
        .into_iter()
        .flat_map(|z| [[o, o, z], [o + 3., o, 4.], [o, o + 5., 0.42]])
        .collect();
    let output = visible(&Input {
        positions: &positions,
        triangles: &[[0, 1, 2], [3, 4, 5]],
        front: &[true; 2],
        positive: &[false, true],
        lightness: &[1.; 6],
        bands: 1,
        occluders: &[],
        tolerance: 0.,
    });
    assert!(first_layer(&output, true).is_some_and(|layer| contains(&layer.region, [16_385.; 2])));
    assert!(first_layer(&output, false).is_none());
}

#[test]
fn spherical_rims_have_the_right_radius_and_cover_the_circumference() {
    let mesh = isosurface(
        |p| (-p.iter().map(|x| x * x).sum::<f32>()).exp(),
        Grid {
            origin: [-1.5; 3],
            step: 0.1,
            size: [31; 3],
        },
        (-1_f32).exp(),
    );
    let (frame, _) = prepare(&mesh, &[], rotations()[0], false);
    let result = visible(&frame.input(&mesh, 1));
    let radius = 60.;
    let mut length = 0.;
    for run in &result.rims {
        for p in run {
            let r = f64::from(p[0]).hypot(f64::from(p[1]));
            assert!(
                (r - radius).abs() < 0.02 * radius,
                "silhouette radius {r}, expected {radius}"
            );
        }
        for edge in run.windows(2) {
            length += f64::from(edge[1][0] - edge[0][0]).hypot(f64::from(edge[1][1] - edge[0][1]));
        }
    }
    let samples = 720;
    let covered = (0..samples)
        .filter(|&i| {
            let angle = f64::from(i) * std::f64::consts::TAU / f64::from(samples);
            let point = [radius * angle.cos(), radius * angle.sin()];
            result
                .rims
                .iter()
                .flat_map(|run| run.windows(2))
                .any(|edge| distance_to_segment(point, edge[0], edge[1]) < 0.02 * radius)
        })
        .count();
    assert!(
        covered * 10 > samples as usize * 9,
        "{covered}/{samples} outline directions covered"
    );
    let circumference = std::f64::consts::TAU * radius;
    assert!(
        length > 0.9 * circumference,
        "rim length {length}, circumference {circumference}"
    );
    assert!(
        length < 1.1 * circumference,
        "duplicated rim runs: {length}"
    );
}

#[test]
fn front_depth_matches_two_sided_ray_intersections_at_random_screen_points() {
    let mesh = p_mesh();
    for perspective in [false, true] {
        for (index, rotation) in rotations().into_iter().enumerate() {
            let (frame, oracle) = prepare(&mesh, &[], rotation, perspective);
            let input = frame.input(&mesh, 1);
            let mut random = Random(0x5eed_de97 + index as u64);
            let mut hits = 0;
            for _ in 0..600 {
                let point = [0, 1].map(|i| {
                    oracle.bounds[i] + random.unit() * (oracle.bounds[i + 2] - oracle.bounds[i])
                });
                let screen = point.map(|x| x as f32);
                let expected = oracle.surface(screen.map(f64::from)).0.map(|hit| hit.0);
                let actual = front_depth(&input, screen);
                match (actual, expected) {
                    (Some(actual), Some(expected)) => {
                        hits += 1;
                        assert!(
                            (f64::from(actual) - expected).abs() < 0.002,
                            "perspective={perspective}, rotation={index}, at={screen:?}, actual={actual}, expected={expected}"
                        );
                    }
                    (None, None) => {}
                    other => panic!("front_depth hit mismatch: {other:?}, at={screen:?}"),
                }
            }
            assert!(
                hits > 70,
                "front_depth test must actually hit both lobes: {hits}"
            );
        }
    }
}

#[test]
fn nested_shading_layers_follow_the_continuous_vertex_lightness_field() {
    let mesh = Mesh {
        positions: vec![[0., 0., 0.], [10., 0., 0.], [0., 10., 0.]],
        normals: vec![[0., 0., 1.]; 3],
        triangles: vec![[0, 1, 2]],
        positive: vec![true],
        truncated: false,
    };
    let input = Input {
        positions: &mesh.positions,
        triangles: &mesh.triangles,
        positive: &mesh.positive,
        front: &[true],
        lightness: &[0., 1., 0.],
        occluders: &[],
        bands: 5,
        tolerance: 0.0001,
    };
    let result = visible(&input);
    for x in [0.35, 1.35, 2.35, 3.35, 4.35, 5.35, 6.35, 7.35, 8.35] {
        let point = [x, 0.25];
        for band in 0..5 {
            let drawn = result
                .layers
                .iter()
                .filter(|layer| layer.band == band && layer.positive)
                .any(|layer| contains(&layer.region, point));
            assert_eq!(drawn, x / 10. >= band as f64 / 5., "x={x}, band={band}");
        }
    }
}

#[test]
fn empty_shading_bands_do_not_repeat_the_previous_region() {
    let lightness = [
        0.35, 0.35, 0.35, 0.85, 0.85, 0.85, 0.45, 0.45, 0.45, 0.95, 0.95, 0.95,
    ];
    assert!(lightness.iter().all(|&light| light >= 0.35));
    let result = visible(&Input {
        positions: &[
            [0., 0., 0.],
            [10., 0., 0.],
            [0., 10., 0.],
            [20., 0., 0.],
            [30., 0., 0.],
            [20., 10., 0.],
            [0., 20., 0.],
            [10., 20., 0.],
            [0., 30., 0.],
            [20., 20., 0.],
            [30., 20., 0.],
            [20., 30., 0.],
        ],
        triangles: &[[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]],
        positive: &[true, true, false, false],
        front: &[true; 4],
        lightness: &lightness,
        occluders: &[],
        bands: 10,
        tolerance: 0.0001,
    });
    for positive in [true, false] {
        let layers: Vec<_> = result
            .layers
            .iter()
            .filter(|layer| layer.positive == positive)
            .collect();
        assert!(layers.len() >= 2, "must compare layers for sign {positive}");
        assert!(layers.iter().all(|layer| !layer.region.is_empty()));
        assert!(layers[0].band > 0, "unoccupied ambient bands are omitted");
        for pair in layers.windows(2) {
            assert!(pair[0].band < pair[1].band);
            assert_ne!(
                pair[0].region, pair[1].region,
                "sign {positive} repeats regions in bands {} and {}",
                pair[0].band, pair[1].band
            );
        }
    }
}

#[test]
fn depth_queries_use_the_mesh_plane_even_below_paint_area_or_behind_a_ball() {
    let edge = 1e-5;
    let input = Input {
        positions: &[[0., 0., 1.], [edge, 0., 3.], [0., edge, 5.]],
        triangles: &[[0, 1, 2]],
        positive: &[true],
        front: &[true],
        lightness: &[1.; 3],
        occluders: &[Occluder::Ball {
            center: [0., 0., 10.],
            radius: 1.,
            depth: 1.,
        }],
        bands: 1,
        tolerance: 0.,
    };
    assert_eq!(front_depth(&input, [edge * 0.25, edge * 0.25]), Some(2.5));
    assert_eq!(front_depth(&input, [edge, edge]), None);
}

#[test]
fn nearer_box_clips_both_silhouette_edges_of_a_face() {
    // Two 2-by-2 front faces overlap over a strip one unit wide. The whole
    // near perimeter is visible. Only four units of the far perimeter remain.
    let mut mesh = Mesh::default();
    let mut front = Vec::new();
    for (x, z) in [(0., 0.), (1., 2.)] {
        let base = mesh.positions.len() as u32;
        for depth in [z, z - 1.] {
            mesh.positions.extend([
                [x - 1., -1., depth],
                [x + 1., -1., depth],
                [x + 1., 1., depth],
                [x - 1., 1., depth],
            ]);
        }
        for (i, t) in [
            [0, 1, 2],
            [0, 2, 3],
            [4, 6, 5],
            [4, 7, 6],
            [0, 4, 5],
            [0, 5, 1],
            [1, 5, 6],
            [1, 6, 2],
            [2, 6, 7],
            [2, 7, 3],
            [3, 7, 4],
            [3, 4, 0],
        ]
        .into_iter()
        .enumerate()
        {
            mesh.triangles.push(t.map(|v| base + v));
            mesh.positive.push(base == 0);
            front.push(i < 2);
        }
    }
    let out = visible(&Input {
        positions: &mesh.positions,
        triangles: &mesh.triangles,
        positive: &mesh.positive,
        front: &front,
        lightness: &[0.5; 16],
        occluders: &[],
        bands: 1,
        tolerance: 0.0001,
    });
    let length: f32 = out
        .rims
        .iter()
        .flat_map(|r| r.windows(2))
        .map(|e| (e[1][0] - e[0][0]).hypot(e[1][1] - e[0][1]))
        .sum();
    assert!((length - 12.).abs() < 0.001, "visible rim length {length}");
}

#[test]
fn rounded_ball_cuts_still_overlap_the_nearer_surface() {
    let result = visible(&Input {
        positions: &[
            [61., 87., -1.],
            [133., 91., -1.],
            [89.7, 100., -1.],
            [50., 80., 20.],
            [150., 80., 20.],
            [100., 140., 20.],
        ],
        triangles: &[[0, 1, 2], [3, 4, 5]],
        positive: &[true, false],
        front: &[true; 2],
        lightness: &[1.; 6],
        occluders: &[Occluder::Ball {
            center: [100., 100., 0.],
            radius: 10.3,
            depth: 10.3,
        }],
        bands: 1,
        tolerance: 0.,
    });
    let point = [85., 93.];
    assert!(first_layer(&result, false).is_some_and(|layer| contains(&layer.region, point)));
    assert!(
        !first_layer(&result, true).is_some_and(|layer| contains(&layer.region, point)),
        "a duplicate clip vertex must not make the rear piece miss its overlap"
    );
}

#[test]
fn zero_length_polygon_edges_have_no_separating_axis() {
    let triangle = [[0., 0.], [3., 0.], [0., 3.]];
    let closed = [triangle[0], triangle[1], triangle[2], triangle[0]];
    let mut scratch = [Vec::new(), Vec::new()];
    assert_eq!(
        intersection_centroid(&closed, &triangle, &mut scratch),
        Some(([1., 1.], 4.5))
    );
}

#[test]
fn arbitrarily_close_depths_keep_the_nearest_of_three_sheets() {
    let positions: Vec<_> = [0., 7.5e-8, 1.5e-7]
        .into_iter()
        .flat_map(|z| [[0., 0., z], [10., 0., z], [0., 10., z]])
        .collect();
    let result = visible(&Input {
        positions: &positions,
        triangles: &[[0, 1, 2], [3, 4, 5], [6, 7, 8]],
        positive: &[true, false, true],
        front: &[true; 3],
        lightness: &[1.; 9],
        occluders: &[],
        bands: 1,
        tolerance: 0.,
    });
    assert!(
        first_layer(&result, true).is_some_and(|layer| contains(&layer.region, [2., 2.])),
        "depth comparisons must not form a tolerance-induced cycle"
    );
    assert!(first_layer(&result, false).is_none());
}

#[test]
fn tiny_depth_gaps_hide_the_rear_silhouette() {
    let result = visible(&Input {
        positions: &[
            [0., 0., 0.],
            [10., 0., 0.],
            [0., 10., 0.],
            [0., 0., 7.5e-8],
            [10., 0., 7.5e-8],
            [0., 10., 7.5e-8],
            [0., -1., -1.],
        ],
        triangles: &[[0, 1, 2], [3, 4, 5], [1, 0, 6]],
        positive: &[true, false, true],
        front: &[true, true, false],
        lightness: &[1.; 7],
        occluders: &[],
        bands: 1,
        tolerance: 0.,
    });
    assert!(first_layer(&result, false).is_some_and(|layer| contains(&layer.region, [2., 2.])));
    assert!(
        result.rims.is_empty(),
        "a strictly nearer sheet hides the rim"
    );
}

#[test]
fn exact_depth_ties_use_the_same_owner_for_fill_and_rims() {
    let result = visible(&Input {
        positions: &[
            [0., 0., 0.],
            [5., 0., 0.],
            [0., 5., 0.],
            [0., 0., 0.],
            [10., 0., 0.],
            [0., 10., 0.],
            [0., -1., -1.],
        ],
        triangles: &[[0, 1, 2], [3, 4, 5], [4, 3, 6]],
        positive: &[true, false, false],
        front: &[true, true, false],
        lightness: &[1.; 7],
        occluders: &[],
        bands: 1,
        tolerance: 0.,
    });
    assert!(first_layer(&result, true).is_some_and(|layer| contains(&layer.region, [1., 1.])));
    assert!(first_layer(&result, false).is_some_and(|layer| contains(&layer.region, [7., 1.])));
    let length: f32 = result
        .rims
        .iter()
        .flat_map(|run| run.windows(2))
        .map(|edge| (edge[1][0] - edge[0][0]).hypot(edge[1][1] - edge[0][1]))
        .sum();
    assert!((length - 5.).abs() < 0.001, "tied rear rim length {length}");
}

#[test]
fn covered_fill_does_not_discard_a_visible_silhouette_edge() {
    let epsilon = 5e-11;
    let result = visible(&Input {
        positions: &[
            [0., 0., 0.],
            [10., 0., 0.],
            [0., 10., 0.],
            [0., epsilon, 1.],
            [10., epsilon, 1.],
            [0., 10., 1.],
            [0., -1., -1.],
        ],
        triangles: &[[0, 1, 2], [3, 4, 5], [1, 0, 6]],
        positive: &[true, false, true],
        front: &[true, true, false],
        lightness: &[1.; 7],
        occluders: &[],
        bands: 1,
        tolerance: 0.,
    });
    assert!(
        result
            .rims
            .iter()
            .flat_map(|run| run.windows(2))
            .any(|edge| distance_to_segment([5., 0.], edge[0], edge[1]) < 1e-8),
        "area coverage does not prove that a boundary edge is hidden"
    );
}

#[test]
fn clipped_vertices_only_store_geometry_and_edge_metadata() {
    let bytes = std::mem::size_of::<Vertex>();
    let budget = std::mem::size_of::<[f32; 3]>() + 2 * std::mem::size_of::<usize>();
    assert!(
        bytes <= budget,
        "clipped vertex uses {bytes} bytes, budget {budget}"
    );
}
