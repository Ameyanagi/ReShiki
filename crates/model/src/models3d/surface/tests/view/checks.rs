use super::*;

fn planar_squares(squares: &[(f32, f32, bool)]) -> Mesh {
    let mut mesh = Mesh::default();
    for &(x, z, positive) in squares {
        let base = mesh.positions.len() as u32;
        mesh.positions
            .extend([[x, 0., z], [x + 2., 0., z], [x + 2., 2., z], [x, 2., z]]);
        mesh.triangles
            .extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
        mesh.positive.extend([positive; 2]);
        mesh.normals.extend([[0., 0., 1.]; 4]);
    }
    mesh
}

fn depth_contains(result: &Visible, depth: usize, positive: bool, point: [f64; 2]) -> bool {
    result.layers.iter().any(|layer| {
        layer.depth == depth && layer.positive == positive && contains(&layer.region, point)
    })
}

fn depth_area(result: &Visible, depth: usize, positive: bool) -> f64 {
    result
        .layers
        .iter()
        .find(|layer| layer.depth == depth && layer.positive == positive)
        .map_or(0., |layer| {
            layer
                .region
                .iter()
                .map(|contour| {
                    contour
                        .iter()
                        .zip(contour.iter().cycle().skip(1))
                        .take(contour.len())
                        .map(|(a, b)| {
                            f64::from(a[0]) * f64::from(b[1]) - f64::from(a[1]) * f64::from(b[0])
                        })
                        .sum::<f64>()
                })
                .sum::<f64>()
                .abs()
                * 0.5
        })
}

fn check_depth_output(result: &Visible, tones: &[usize]) {
    assert_eq!(
        result.stats.fragment_limit_hits, 0,
        "small exact fixture hit the fragment guard"
    );
    assert_eq!(result.rims.len(), tones.len());
    assert_eq!(result.lines.len(), tones.len());
    for layer in &result.layers {
        assert!(layer.depth < tones.len());
        assert!(layer.tone < tones[layer.depth]);
        assert!(!layer.region.is_empty());
    }
    for pair in result.layers.windows(2) {
        let key = |layer: &Layer| (layer.depth, !layer.positive, layer.tone);
        assert!(
            key(&pair[0]) < key(&pair[1]),
            "layers must be sorted by depth, sign and tone"
        );
    }
}

fn run_length(runs: &[Vec<[f32; 2]>]) -> f64 {
    runs.iter()
        .flat_map(|run| run.windows(2))
        .map(|edge| f64::from(edge[1][0] - edge[0][0]).hypot(f64::from(edge[1][1] - edge[0][1])))
        .sum()
}

fn check_horizontal_intervals(runs: &[Vec<[f32; 2]>], y: f32, expected: &[(f32, f32)]) {
    let mut intervals: Vec<_> = runs
        .iter()
        .flat_map(|run| run.windows(2))
        .map(|edge| {
            assert!(
                (edge[0][1] - y).abs() < 0.0001,
                "unexpected line edge {edge:?}"
            );
            assert!(
                (edge[1][1] - y).abs() < 0.0001,
                "unexpected line edge {edge:?}"
            );
            (edge[0][0].min(edge[1][0]), edge[0][0].max(edge[1][0]))
        })
        .filter(|&(start, end)| end - start > 0.00001)
        .collect();
    intervals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (start, end) in intervals {
        if let Some(last) = merged.last_mut()
            && start <= last.1 + 0.0001
        {
            assert!(
                start >= last.1 - 0.0001,
                "duplicate line interval at {start}..{end}"
            );
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    assert_eq!(
        merged.len(),
        expected.len(),
        "line intervals {merged:?}, expected {expected:?}"
    );
    for (actual, expected) in merged.iter().zip(expected) {
        assert!(
            (actual.0 - expected.0).abs() < 0.0001,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.1 - expected.1).abs() < 0.0001,
            "{actual:?} != {expected:?}"
        );
    }
    let expected_length: f64 = expected.iter().map(|&(a, b)| f64::from(b - a)).sum();
    assert!((run_length(runs) - expected_length).abs() < 0.0001);
}

#[test]
fn half_overlapping_squares_keep_exactly_the_requested_depth_groups() {
    let mesh = planar_squares(&[(0., 0., true), (1., 1., false)]);
    for tones in [&[1][..], &[1, 1][..]] {
        let result = visible(&Input {
            positions: &mesh.positions,
            triangles: &mesh.triangles,
            positive: &mesh.positive,
            front: &[true; 4],
            lightness: &[1.; 8],
            occluders: &[],
            tones,
            lines: &[],
            tolerance: 0.,
        });
        check_depth_output(&result, tones);
        assert_eq!(result.layers.len(), tones.len() + 1);
        for (depth, positive, expected) in [(0, false, 4.), (0, true, 2.), (1, true, 2.)] {
            if depth < tones.len() {
                assert!(
                    (depth_area(&result, depth, positive) - expected).abs() < 1e-6,
                    "square area at depth={depth}, positive={positive}, tones={tones:?}"
                );
            }
        }
        for ix in -2..14 {
            for iy in -2..10 {
                let point = [f64::from(ix) * 0.25 + 0.1, f64::from(iy) * 0.25 + 0.07];
                let in_y = point[1] > 0. && point[1] < 2.;
                let rear = in_y && point[0] > 0. && point[0] < 2.;
                let near = in_y && point[0] > 1. && point[0] < 3.;
                for depth in 0..tones.len() {
                    assert_eq!(
                        depth_contains(&result, depth, true, point),
                        rear && usize::from(near) == depth,
                        "rear square at {point:?}, depth={depth}, tones={tones:?}"
                    );
                    assert_eq!(
                        depth_contains(&result, depth, false, point),
                        near && depth == 0,
                        "near square at {point:?}, depth={depth}, tones={tones:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn three_stacked_sheets_use_each_groups_tones_and_can_omit_its_fill() {
    let mesh = planar_squares(&[(0., 0., true), (0., 1., false), (0., 2., true)]);
    let lightness: Vec<_> = [0.1, 0.5, 0.9]
        .into_iter()
        .flat_map(|light| [light; 4])
        .collect();
    for tones in [
        &[1, 1][..],
        &[1, 1, 1][..],
        &[2, 3, 4][..],
        &[0, 3, 0][..],
        &[0, 0, 0][..],
    ] {
        let result = visible(&Input {
            positions: &mesh.positions,
            triangles: &mesh.triangles,
            positive: &mesh.positive,
            front: &[true; 6],
            lightness: &lightness,
            occluders: &[],
            tones,
            lines: &[],
            tolerance: 0.,
        });
        check_depth_output(&result, tones);
        assert_eq!(
            result.layers.len(),
            tones.iter().filter(|&&n| n > 0).count()
        );
        for (depth, &count) in tones.iter().enumerate() {
            for positive in [true, false] {
                let expected = if count > 0 && positive == (depth != 1) {
                    4.
                } else {
                    0.
                };
                assert!((depth_area(&result, depth, positive) - expected).abs() < 1e-6);
            }
            for point in [[0.25, 0.5], [1.75, 0.5], [0.25, 1.5], [1.75, 1.5]] {
                for positive in [true, false] {
                    assert_eq!(
                        depth_contains(&result, depth, positive, point),
                        count > 0 && positive == (depth != 1),
                        "point={point:?}, depth={depth}, positive={positive}, tones={tones:?}"
                    );
                }
            }
            if count > 0 {
                let light = [0.9, 0.5, 0.1][depth];
                let layer = result
                    .layers
                    .iter()
                    .find(|layer| layer.depth == depth)
                    .unwrap();
                assert_eq!(
                    layer.tone,
                    ((light * count as f64).floor() as usize).min(count - 1)
                );
            }
        }
    }
}

#[test]
fn a_rear_rim_is_split_between_uncovered_and_once_covered_groups() {
    let result = visible(&Input {
        positions: &[
            [0., 0., 0.],
            [10., 0., 0.],
            [0., 10., 0.],
            [0., -1., -1.],
            [3., -1., 1.],
            [7., -1., 1.],
            [7., 3., 1.],
            [3., 3., 1.],
        ],
        triangles: &[[0, 1, 2], [1, 0, 3], [4, 5, 6], [4, 6, 7]],
        positive: &[true, true, false, false],
        front: &[true, false, true, true],
        lightness: &[1.; 8],
        occluders: &[],
        tones: &[1, 1],
        lines: &[],
        tolerance: 0.,
    });
    check_depth_output(&result, &[1, 1]);
    check_horizontal_intervals(&result.rims[0], 0., &[(0., 3.), (7., 10.)]);
    check_horizontal_intervals(&result.rims[1], 0., &[(3., 7.)]);
}

#[test]
fn a_front_triangle_line_is_cut_by_a_ball_and_grouped_under_a_near_sheet() {
    for tones in [&[1][..], &[1, 1][..], &[0, 0][..]] {
        let result = visible(&Input {
            positions: &[
                [0., 0., 0.],
                [12., 0., 0.],
                [0., 12., 0.],
                [6., 0., 2.],
                [8., 0., 2.],
                [8., 2., 2.],
                [6., 2., 2.],
            ],
            triangles: &[[0, 1, 2], [3, 4, 5], [3, 5, 6]],
            positive: &[true, false, false],
            front: &[true; 3],
            lightness: &[1.; 7],
            occluders: &[Occluder::Ball {
                center: [3.5, 1., 1.],
                radius: 0.5,
                depth: 0.,
            }],
            tones,
            lines: &[Line {
                triangle: 0,
                ends: [[0.5, 1., 0.], [10.5, 1., 0.]],
            }],
            tolerance: 0.,
        });
        check_depth_output(&result, tones);
        check_horizontal_intervals(&result.lines[0][0], 1., &[(0.5, 3.), (4., 6.), (8., 10.5)]);
        assert!(result.lines.iter().all(|groups| groups[1].is_empty()));
        if tones.len() == 2 {
            check_horizontal_intervals(&result.lines[1][0], 1., &[(6., 8.)]);
        }
    }
}

#[test]
fn a_closed_shapes_back_face_line_is_retained_only_with_two_groups() {
    let positions = [
        [0., 0., 1.],
        [2., 0., 1.],
        [2., 2., 1.],
        [0., 2., 1.],
        [0., 0., 0.],
        [2., 0., 0.],
        [2., 2., 0.],
        [0., 2., 0.],
    ];
    let triangles = [
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
    ];
    let front = std::array::from_fn::<_, 12, _>(|i| i < 2);
    for tones in [&[1][..], &[1, 1][..]] {
        let result = visible(&Input {
            positions: &positions,
            triangles: &triangles,
            positive: &[true; 12],
            front: &front,
            lightness: &[1.; 8],
            occluders: &[],
            tones,
            lines: &[
                Line {
                    triangle: 2,
                    ends: [[0.75, 0.25, 0.], [1.75, 0.25, 0.]],
                },
                Line {
                    triangle: 0,
                    ends: [[0.75, 0.25, 1.], [1.75, 0.25, 1.]],
                },
            ],
            tolerance: 0.,
        });
        check_depth_output(&result, tones);
        check_horizontal_intervals(&result.lines[0][0], 0.25, &[(0.75, 1.75)]);
        assert!(result.lines[0][1].is_empty());
        if tones.len() == 2 {
            check_horizontal_intervals(&result.lines[1][0], 0.25, &[(0.75, 1.75)]);
            assert!(result.lines[1][1].is_empty());
            assert!(!result.layers.iter().any(|layer| layer.depth == 1));
        }
    }
}

#[test]
fn touching_coplanar_triangles_do_not_create_a_once_covered_group() {
    let result = visible(&Input {
        positions: &[[0., 0., 0.], [2., 0., 0.], [0., 2., 0.], [2., 2., 0.]],
        triangles: &[[0, 1, 2], [1, 3, 2]],
        positive: &[true, false],
        front: &[true; 2],
        lightness: &[1.; 4],
        occluders: &[],
        tones: &[1, 1],
        // This lies on the shared edge and belongs to the later triangle.
        lines: &[Line {
            triangle: 1,
            ends: [[1.75, 0.25, 0.], [0.25, 1.75, 0.]],
        }],
        tolerance: 0.,
    });
    check_depth_output(&result, &[1, 1]);
    assert_eq!(result.stats.overlaps, 0);
    assert_eq!(result.layers.len(), 2);
    assert!(depth_contains(&result, 0, true, [0.25, 0.25]));
    assert!(depth_contains(&result, 0, false, [1.75, 1.75]));
    assert!(result.layers.iter().all(|layer| layer.depth == 0));
    assert!(result.rims[1].is_empty());
    assert!(result.lines[1].iter().all(Vec::is_empty));
    assert!(result.lines[0][0].is_empty());
    assert!((run_length(&result.lines[0][1]) - 1.5_f64 * 2_f64.sqrt()).abs() < 0.0001);
}

#[test]
fn a_line_on_a_clipping_fragment_boundary_is_emitted_once() {
    let result = visible(&Input {
        positions: &[[0., 0., 0.], [10., 0., 0.], [0., 10., 0.]],
        triangles: &[[0, 1, 2]],
        positive: &[true],
        front: &[true],
        lightness: &[1.; 3],
        occluders: &[Occluder::Stick {
            a: [3., 3., 1.],
            b: [6., 3., 1.],
            half: [1.; 2],
            depth: [0.; 2],
        }],
        tones: &[1, 1],
        // The first two surviving clip pieces share x=3 below y=2.
        lines: &[Line {
            triangle: 0,
            ends: [[3., 0.5, 0.], [3., 1.5, 0.]],
        }],
        tolerance: 0.,
    });
    check_depth_output(&result, &[1, 1]);
    assert!(
        result.stats.pieces > 1,
        "fixture must actually fragment the triangle"
    );
    assert!((run_length(&result.lines[0][0]) - 1.).abs() < 0.0001);
    assert_eq!(result.lines[0][0].len(), 1);
    assert!(
        result.lines[0][0][0]
            .iter()
            .all(|p| (p[0] - 3.).abs() < 0.0001)
    );
    assert!(result.lines[0][1].is_empty());
    assert!(result.lines[1].iter().all(Vec::is_empty));
}

#[test]
fn a_partly_covered_fragment_boundary_line_uses_its_deepest_count_once() {
    for tones in [&[1][..], &[1, 1][..]] {
        for reverse in [false, true] {
            let mut ends = [[4.5, 3., 0.], [5.5, 3., 0.]];
            if reverse {
                ends.reverse();
            }
            let result = visible(&Input {
                positions: &[
                    [0., 0., 0.],
                    [10., 0., 0.],
                    [0., 10., 0.],
                    [4.75, 3., 2.],
                    [5.25, 3., 2.],
                    [5.25, 3.25, 2.],
                    [4.75, 3.25, 2.],
                ],
                triangles: &[[0, 1, 2], [3, 4, 5], [3, 5, 6]],
                positive: &[true, false, false],
                front: &[true; 3],
                lightness: &[1.; 7],
                occluders: &[Occluder::Stick {
                    a: [3., 3., 1.],
                    b: [3., 6., 1.],
                    half: [1.; 2],
                    depth: [0.; 2],
                }],
                tones,
                // The clip pieces meet at y=3, x>4. The near square overlaps only
                // the upper piece, so the lower piece emits this line unsplit.
                lines: &[Line { triangle: 0, ends }],
                tolerance: 0.,
            });
            check_depth_output(&result, tones);
            check_horizontal_intervals(&result.lines[0][0], 3., &[(4.5, 4.75), (5.25, 5.5)]);
            if tones.len() == 2 {
                check_horizontal_intervals(&result.lines[1][0], 3., &[(4.75, 5.25)]);
            }
            assert!(result.lines.iter().all(|groups| groups[1].is_empty()));
        }
    }
}

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
        tones: &[1],
        lines: &[],
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
fn contour_thinning_skips_large_contours() {
    let contour: Vec<_> = (0..3000)
        .map(|i| {
            let (sin, cos) = (std::f64::consts::TAU * f64::from(i) / 3000.).sin_cos();
            [cos, sin]
        })
        .collect();
    let original: Vec<_> = contour.iter().map(|p| p.map(|x| x as f32)).collect();
    assert_eq!(simplify_closed(&contour, 0.01), original);
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
        tones: &[1],
        lines: &[],
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
        tones: &[1],
        lines: &[],
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
    let result = visible(&frame.input(&mesh, &[1]));
    let radius = 60.;
    let mut length = 0.;
    for run in &result.rims[0] {
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
            result.rims[0]
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
            let input = frame.input(&mesh, &[1]);
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
        tones: &[5],
        lines: &[],
        tolerance: 0.0001,
    };
    let result = visible(&input);
    check_depth_output(&result, &[5]);
    for x in [0.35, 1.35, 2.35, 3.35, 4.35, 5.35, 6.35, 7.35, 8.35] {
        let point = [x, 0.25];
        for band in 0..5 {
            let drawn = result
                .layers
                .iter()
                .filter(|layer| layer.tone == band && layer.positive)
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
        tones: &[10],
        lines: &[],
        tolerance: 0.0001,
    });
    check_depth_output(&result, &[10]);
    for positive in [true, false] {
        let layers: Vec<_> = result
            .layers
            .iter()
            .filter(|layer| layer.positive == positive)
            .collect();
        assert!(layers.len() >= 2, "must compare layers for sign {positive}");
        assert!(layers.iter().all(|layer| !layer.region.is_empty()));
        assert!(layers[0].tone > 0, "unoccupied ambient bands are omitted");
        for pair in layers.windows(2) {
            assert!(pair[0].tone < pair[1].tone);
            assert_ne!(
                pair[0].region, pair[1].region,
                "sign {positive} repeats regions in bands {} and {}",
                pair[0].tone, pair[1].tone
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
        tones: &[1],
        lines: &[],
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
        tones: &[1],
        lines: &[],
        tolerance: 0.0001,
    });
    let length: f32 = out.rims[0]
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
        tones: &[1],
        lines: &[],
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
        tones: &[1],
        lines: &[],
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
        tones: &[1],
        lines: &[],
        tolerance: 0.,
    });
    assert!(first_layer(&result, false).is_some_and(|layer| contains(&layer.region, [2., 2.])));
    assert!(
        result.rims[0].is_empty(),
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
        tones: &[1],
        lines: &[],
        tolerance: 0.,
    });
    assert!(first_layer(&result, true).is_some_and(|layer| contains(&layer.region, [1., 1.])));
    assert!(first_layer(&result, false).is_some_and(|layer| contains(&layer.region, [7., 1.])));
    let length: f32 = result.rims[0]
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
        tones: &[1],
        lines: &[],
        tolerance: 0.,
    });
    assert!(
        result.rims[0]
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
