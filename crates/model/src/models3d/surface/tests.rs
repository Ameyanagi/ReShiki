use super::mesh::{Grid, MAX_RAW, MAX_SAMPLES, MAX_SIDE, Mesh, isosurface, simplify};
use std::collections::{HashMap, HashSet};

mod view;

fn grid(origin: [f32; 3], step: f32, size: [usize; 3]) -> Grid {
    Grid { origin, step, size }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// Independent separating-axis test, including in-plane axes for coplanar faces.
fn intersect(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> bool {
    let edges = |t: [[f64; 3]; 3]| [sub(t[1], t[0]), sub(t[2], t[1]), sub(t[0], t[2])];
    let ea = edges(a);
    let eb = edges(b);
    let na = cross(ea[0], ea[1]);
    let nb = cross(eb[0], eb[1]);
    let axes = [na, nb]
        .into_iter()
        .chain(ea.into_iter().map(|e| cross(na, e)))
        .chain(eb.into_iter().map(|e| cross(nb, e)))
        .chain(ea.into_iter().flat_map(|e| eb.map(|f| cross(e, f))));
    for axis in axes {
        let range = |t: [[f64; 3]; 3]| {
            let d = t.map(|p| dot(axis, sub(p, a[0])));
            (d[0].min(d[1]).min(d[2]), d[0].max(d[1]).max(d[2]))
        };
        let (al, ah) = range(a);
        let (bl, bh) = range(b);
        if ah < bl || bh < al {
            return false;
        }
    }
    true
}

fn check_embedded(mesh: &Mesh) {
    let faces = mesh
        .triangles
        .iter()
        .map(|tri| tri.map(|v| mesh.positions[v as usize].map(f64::from)))
        .collect::<Vec<_>>();
    let mut bounds: Vec<_> = faces
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let lo: [f64; 3] = std::array::from_fn(|k| f[0][k].min(f[1][k]).min(f[2][k]));
            let hi: [f64; 3] = std::array::from_fn(|k| f[0][k].max(f[1][k]).max(f[2][k]));
            (i, lo, hi)
        })
        .collect();
    bounds.sort_unstable_by(|a, b| a.1[0].total_cmp(&b.1[0]));
    for (idx, &(i, lo, hi)) in bounds.iter().enumerate() {
        for &(j, bl, bh) in &bounds[idx + 1..] {
            if bl[0] > hi[0] {
                break;
            }
            if (1..3).any(|d| bh[d] < lo[d] || hi[d] < bl[d]) {
                continue;
            }
            let a = mesh.triangles[i];
            let b = mesh.triangles[j];
            let shared = a.iter().filter(|v| b.contains(v)).count();
            let forbidden = match shared {
                0 => intersect(faces[i], faces[j]),
                1 => {
                    // Any intersection extending from the common vertex reaches
                    // the opposite edge of at least one of the two triangles.
                    let ia = a.iter().position(|v| b.contains(v)).unwrap();
                    let ib = b.iter().position(|v| a.contains(v)).unwrap();
                    let opposite = |t: [[f64; 3]; 3], common: usize| {
                        [
                            t[(common + 1) % 3],
                            t[(common + 2) % 3],
                            t[(common + 2) % 3],
                        ]
                    };
                    intersect(opposite(faces[i], ia), faces[j])
                        || intersect(opposite(faces[j], ib), faces[i])
                }
                2 => {
                    let ia = a.iter().position(|v| !b.contains(v)).unwrap();
                    let ib = b.iter().position(|v| !a.contains(v)).unwrap();
                    let origin = faces[i][(ia + 1) % 3];
                    let edge = sub(faces[i][(ia + 2) % 3], origin);
                    let ua = sub(faces[i][ia], origin);
                    let ub = sub(faces[j][ib], origin);
                    let na = cross(edge, ua);
                    // Shared-edge faces can overlap only when coplanar, with
                    // their third vertices on the same side of that edge.
                    dot(na, ub).abs() <= 1e-12 * (dot(na, na) * dot(ub, ub)).sqrt()
                        && dot(na, cross(edge, ub)) > 0.
                }
                _ => true,
            };
            assert!(
                !forbidden,
                "triangles {i} and {j} intersect beyond their shared vertices"
            );
        }
    }
}

// Walk directed edge uses, every vertex link, and each component of the dual graph.
fn topology(mesh: &Mesh) -> Vec<(bool, isize)> {
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert_eq!(mesh.positive.len(), mesh.triangles.len());
    let mut directed = HashMap::new();
    let mut links = vec![HashMap::new(); mesh.positions.len()];
    for (i, &tri) in mesh.triangles.iter().enumerate() {
        let [a, b, c] = tri.map(|v| mesh.positions[v as usize].map(f64::from));
        let n = cross(sub(b, a), sub(c, a));
        assert!(dot(n, n) > 0., "zero area face {i}: {tri:?}");
        for k in 0..3 {
            let [u, v, w] = [tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]];
            assert!(
                directed.insert((u, v), i).is_none(),
                "repeated directed edge {u}-{v}"
            );
            assert!(links[u as usize].insert(v, w).is_none());
        }
    }
    for (n, link) in mesh.normals.iter().zip(links) {
        assert!(
            (n.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 1e-4,
            "non-unit normal {n:?}"
        );
        let first = *link.keys().next().expect("unused vertex");
        let mut cur = first;
        let mut visited = HashSet::new();
        while visited.insert(cur) {
            cur = link[&cur];
        }
        assert_eq!(cur, first);
        assert_eq!(visited.len(), link.len(), "disconnected vertex link");
    }
    let mut components = Vec::new();
    let mut seen = vec![false; mesh.triangles.len()];
    for seed in 0..seen.len() {
        if seen[seed] {
            continue;
        }
        let mut stack = vec![seed];
        seen[seed] = true;
        let mut verts = HashSet::new();
        let mut count = 0;
        let mut volume6 = 0.;
        while let Some(i) = stack.pop() {
            let tri = mesh.triangles[i];
            let [a, b, c] = tri.map(|v| mesh.positions[v as usize].map(f64::from));
            volume6 += dot(a, cross(b, c));
            count += 1;
            assert_eq!(mesh.positive[i], mesh.positive[seed]);
            for k in 0..3 {
                verts.insert(tri[k]);
                let other = directed[&(tri[(k + 1) % 3], tri[k])];
                if !seen[other] {
                    seen[other] = true;
                    stack.push(other);
                }
            }
        }
        assert!(volume6 > 0., "inward-oriented component");
        components.push((
            mesh.positive[seed],
            verts.len() as isize - count * 3 / 2 + count,
        ));
    }
    components.sort_unstable();
    components
}

fn check_field(mesh: Mesh, components: &[(bool, isize)], radius: Option<f32>) {
    assert!(!mesh.truncated);
    let reduced = simplify(&mesh, 2000);
    assert!(
        reduced.triangles.len() <= 2000,
        "{} triangles left",
        reduced.triangles.len()
    );
    eprintln!(
        "topology: {} -> {} triangles",
        mesh.triangles.len(),
        reduced.triangles.len()
    );
    for m in [&mesh, &reduced] {
        assert_eq!(topology(m), components);
        check_embedded(m);
        if let Some(radius) = radius {
            let mut total = 0.;
            for (p, n) in m.positions.iter().zip(&m.normals) {
                let r = p.iter().map(|x| x * x).sum::<f32>().sqrt();
                total += r as f64;
                assert!(p.iter().zip(n).map(|(x, y)| x * y).sum::<f32>() > 0.95 * r);
            }
            assert!((total / m.positions.len() as f64 / radius as f64 - 1.).abs() < 0.02);
        }
    }
}

// These unsigned fixtures stop at zero so the negative level has no second shell.
fn sphere(p: [f32; 3]) -> f32 {
    (1. - p.iter().map(|x| x * x).sum::<f32>().sqrt()).max(0.)
}

#[test]
fn sphere_topology() {
    for sign in [1., -1.] {
        check_field(
            isosurface(|p| sign * sphere(p), grid([-1.23; 3], 0.12, [22; 3]), 0.13),
            &[(sign > 0., 2)],
            Some(0.87),
        );
    }
}

#[test]
fn opposite_lobe_topology() {
    let p = |v: [f32; 3]| v[2] * (-v.iter().map(|x| x * x).sum::<f32>().sqrt()).exp();
    check_field(
        isosurface(p, grid([-1.77, -1.77, -2.81], 0.24, [16, 16, 25]), 0.27),
        &[(false, 2), (true, 2)],
        None,
    );
}

#[test]
fn torus_topology() {
    let field =
        |[x, y, z]: [f32; 3]| (0.43f32.powi(2) - ((1.05 - x.hypot(y)).powi(2) + z * z)).max(0.);
    check_field(
        isosurface(
            field,
            grid([-1.62, -1.62, -0.65], 0.13, [26, 26, 11]),
            0.035,
        ),
        &[(true, 0)],
        None,
    );
}

#[test]
fn narrow_gap_survives_simplification() {
    let step = 0.125;
    let radius = 0.74;
    let center = radius + 0.05 * step;
    let field =
        |[x, y, z]: [f32; 3]| (1. - ((x.abs() - center).powi(2) + y * y + z * z).sqrt()).max(0.);
    check_field(
        isosurface(
            field,
            grid([-1.75, -0.97, -0.97], step, [29, 17, 17]),
            1. - radius,
        ),
        &[(true, 2), (true, 2)],
        None,
    );
}

#[test]
fn grows_only_where_the_surface_leaves_the_box() {
    let initial = grid([-0.64, -1.07, -1.07], 0.12, [16, 19, 19]);
    let mesh = isosurface(sphere, initial, 0.14);
    assert!(mesh.positions.iter().any(|p| p[0] < -0.83));
    check_field(mesh, &[(true, 2)], Some(0.86));
}

#[test]
fn zero_padding_closes_a_truncated_field() {
    let mesh = isosurface(|_| 1., grid([-1.; 3], 0.2, [11; 3]), 0.5);
    assert!(mesh.truncated);
    assert_eq!(topology(&mesh), [(true, 2)]);
    let reduced = simplify(&mesh, 2000);
    assert!(reduced.truncated);
    assert_eq!(topology(&reduced), [(true, 2)]);
}

#[test]
fn grid_bounds_and_drawing_budget() {
    let lo = [-2., -1., -0.5];
    let hi = [3., 0.2, 4.];
    let g = Grid::around(lo, hi);
    assert_eq!(g.origin, lo);
    assert_eq!(*g.size.iter().max().unwrap(), MAX_SIDE);
    assert!(g.size.iter().product::<usize>() <= MAX_SAMPLES);
    for (i, limit) in hi.into_iter().enumerate() {
        assert!(g.origin[i] + (g.size[i] - 1) as f32 * g.step >= limit - 1e-6);
    }
    let mesh = isosurface(sphere, grid([-1.033; 3], 0.047, [45; 3]), 0.101);
    assert!(mesh.triangles.len() > 20_000);
    let reduced = simplify(&mesh, 20_000);
    assert!(reduced.triangles.len() <= 20_000);
    assert_eq!(topology(&reduced), [(true, 2)]);
}

#[test]
fn collapse_into_an_unmoved_high_degree_vertex() {
    // Split one wide sector of a flat fan. Collapsing the split into the
    // centre moves one surviving face, and leaves the rest of the fan in place.
    const SECTORS: u32 = 20_000;
    let centre = [0.; 3];
    let split = [0.25, 0.25, 0.];
    let mut positions = vec![centre, split, [0., 0., -0.01], [1., 0., 0.]];
    positions.extend((0..SECTORS - 1).map(|i| {
        let quarter_turn = std::f64::consts::FRAC_PI_2;
        let angle = quarter_turn + 3. * quarter_turn * f64::from(i) / f64::from(SECTORS - 1);
        let (sin, cos) = angle.sin_cos();
        let radius = if i == 0 { 1. } else { 0.02 };
        [(radius * cos) as f32, (radius * sin) as f32, 0.]
    }));
    let mut triangles = vec![[0, 3, 1], [0, 1, 4], [1, 3, 4]];
    for i in 0..SECTORS {
        let a = i + 3;
        let b = (i + 1) % SECTORS + 3;
        if i > 0 {
            triangles.push([0, a, b]);
        }
        triangles.push([2, b, a]);
    }
    let mesh = Mesh {
        normals: vec![[0., 0., 1.]; positions.len()],
        positive: vec![true; triangles.len()],
        positions,
        triangles,
        truncated: false,
    };
    let target = mesh.triangles.len() - 2;
    let reduced = simplify(&mesh, target);
    assert_eq!(reduced.triangles.len(), target);
    assert!(reduced.positions.contains(&centre));
    assert!(!reduced.positions.contains(&split));
}

#[test]
fn checkerboard_retries_at_double_spacing() {
    let field = |p: [f32; 3]| {
        let ijk = p.map(|x| (x * 8.).round() as i32);
        if ijk.iter().any(|&i| !(1..=64).contains(&i)) {
            0.
        } else if ijk.iter().sum::<i32>() % 2 == 0 {
            1.
        } else {
            -1.
        }
    };
    // Each interior cube crosses both levels twice in each of its six tetrahedra.
    assert!(63usize.pow(3) * 24 > MAX_RAW);
    let mesh = isosurface(field, grid([0.; 3], 0.125, [67; 3]), 0.5);
    assert!(mesh.triangles.len() <= MAX_RAW);
    assert!(!mesh.triangles.is_empty());
    assert!(
        mesh.positive.iter().all(|&positive| positive),
        "coarse samples should all have even parity"
    );
}

#[test]
fn rejects_invalid_isovalues() {
    for iso in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(
            std::panic::catch_unwind(|| isosurface(sphere, grid([-1.; 3], 0.5, [5; 3]), iso))
                .is_err()
        );
    }
}

#[test]
fn opposite_levels_on_the_same_grid_edges_stay_separate() {
    // Two neighbouring extreme samples share a grid edge and several tetrahedra.
    let mesh = isosurface(
        |[x, y, z]| {
            if y == 0. && z == 0. && x == 0. {
                1.
            } else if y == 0. && z == 0. && x == 1. {
                -1.
            } else {
                0.
            }
        },
        grid([-2.; 3], 1., [6; 3]),
        0.25,
    );
    assert_eq!(topology(&mesh), [(false, 2), (true, 2)]);
    assert!(mesh.positions.contains(&[0.375, 0., 0.]));
    assert!(mesh.positions.contains(&[0.625, 0., 0.]));
    check_embedded(&mesh);
}

#[test]
fn tetrahedron_survives_an_unreachable_budget() {
    let mesh = Mesh {
        positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        normals: vec![[0., 0., 1.]; 4],
        positive: vec![false; 4],
        truncated: false,
    };
    let result = simplify(&mesh, 0);
    assert_eq!(result.triangles.len(), 4);
    assert_eq!(topology(&result), [(false, 2)]);
}

#[test]
fn collapses_preserve_embedding_between_incident_faces() {
    let mesh = Mesh {
        positions: vec![
            [0.922003, -0.444820, 1.067303],
            [-0.160357, 0.187812, -0.154576],
            [-0.144974, 0.029406, 0.047445],
            [0.303625, -1.859955, -0.714529],
            [0.302908, 0.236536, 0.607518],
            [0.028060, 0.062259, -0.205521],
        ],
        triangles: vec![
            [0, 1, 4],
            [1, 0, 5],
            [1, 2, 4],
            [2, 1, 5],
            [2, 3, 4],
            [3, 2, 5],
            [3, 0, 4],
            [0, 3, 5],
        ],
        normals: vec![[0., 0., 1.]; 6],
        positive: vec![true; 8],
        truncated: false,
    };
    assert_eq!(topology(&mesh), [(true, 2)]);
    check_embedded(&mesh);
    let reduced = simplify(&mesh, 6);
    assert_eq!(reduced.triangles.len(), 6);
    assert_eq!(topology(&reduced), [(true, 2)]);
    check_embedded(&reduced);
}

#[test]
fn collapses_preserve_embedding_against_spatial_neighbours() {
    // Checking affected faces against each other does not reject edge (1, 5):
    // a changed face crosses an untouched face outside the collapsing edge's fan.
    let mesh = Mesh {
        positions: vec![
            [12., -3., -4.],
            [7., 9., 0.],
            [9., 0., 3.],
            [-1., 0., -14.],
            [16., 2., 7.],
            [3., 14., 8.],
        ],
        triangles: vec![
            [0, 1, 4],
            [1, 0, 5],
            [1, 2, 4],
            [2, 1, 5],
            [2, 3, 4],
            [3, 2, 5],
            [3, 0, 4],
            [0, 3, 5],
        ],
        normals: vec![[0., 0., 1.]; 6],
        positive: vec![true; 8],
        truncated: false,
    };
    assert_eq!(topology(&mesh), [(true, 2)]);
    check_embedded(&mesh);
    let reduced = simplify(&mesh, 6);
    assert_eq!(reduced.triangles.len(), 6);
    assert_eq!(topology(&reduced), [(true, 2)]);
    check_embedded(&reduced);
}

#[test]
fn nonfinite_samples_are_outside_the_surface() {
    let g = grid([-1.25; 3], 0.25, [11; 3]);
    let reference = isosurface(|p| if p == [0.75, 0., 0.] { 0. } else { sphere(p) }, g, 0.2);
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let empty = isosurface(|_| invalid, g, 0.2);
        assert!(empty.triangles.is_empty());
        assert!(!empty.truncated);
        let mesh = isosurface(
            |p| {
                if p == [0.75, 0., 0.] {
                    invalid
                } else {
                    sphere(p)
                }
            },
            g,
            0.2,
        );
        assert!(mesh.positions.iter().flatten().all(|x| x.is_finite()));
        assert_eq!(mesh.positions, reference.positions);
        assert_eq!(mesh.triangles, reference.triangles);
        assert_eq!(topology(&mesh), [(true, 2)]);
        check_embedded(&mesh);
    }
}

#[test]
fn nonfinite_gradients_use_face_normals() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mesh = isosurface(
            |p| {
                if p.iter().all(|x| (x * 4.).fract() == 0.) {
                    sphere(p)
                } else {
                    invalid
                }
            },
            grid([-1.25; 3], 0.25, [11; 3]),
            0.2,
        );
        assert_eq!(topology(&mesh), [(true, 2)]);
        check_embedded(&mesh);
    }
}

#[test]
fn translated_level_ties_have_distinct_edge_crossings() {
    let offset = 12.;
    let mesh = isosurface(
        |p| sphere(p.map(|x| x - offset)),
        grid([offset - 1.5; 3], 0.0625, [49; 3]),
        0.25,
    );
    assert_eq!(topology(&mesh), [(true, 2)]);
    check_embedded(&mesh);
}

#[test]
fn below_budget_output_discards_unused_vertices() {
    let mut mesh = isosurface(sphere, grid([-1.2; 3], 0.21, [13; 3]), 0.16);
    let used = mesh.positions.len();
    mesh.positions.push([18.; 3]);
    mesh.normals.push([0., 0., 1.]);
    let reduced = simplify(&mesh, mesh.triangles.len());
    assert_eq!(reduced.positions.len(), used);
    assert_eq!(topology(&reduced), [(true, 2)]);
}

#[test]
fn quadrics_distinguish_unique_and_singular_minima() {
    use super::mesh::qem_optimum;
    // (x-2)² + 2(y+1)² + 4(z-3)², in packed upper-triangular form.
    assert_eq!(
        qem_optimum(&[1., 0., 0., -2., 2., 0., 2., 4., -12., 42.]),
        Some([2., -1., 3.])
    );
    assert_eq!(
        qem_optimum(&[1., 0., 0., -2., 2., 0., 2., 0., 0., 6.]),
        None
    );
}

#[test]
fn collapses_reject_needles_and_flipped_faces() {
    use super::mesh::qem_shape_ok;
    let up = [0., 0., 1.];
    for (angle, ok) in [(2.9_f64, false), (3.1, true)] {
        let (y, x) = angle.to_radians().sin_cos();
        let triangle = [[0.; 3], [1., 0., 0.], [x, y, 0.]];
        assert_eq!(qem_shape_ok(up, [0., 0., y], triangle), ok);
        assert!(!qem_shape_ok(up, [0., 0., -y], triangle));
    }
}

#[test]
fn triangle_contacts_include_coplanar_edges_and_piercings() {
    use super::mesh::qem_triangles_intersect;
    let a = [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]];
    let n = |t: [[f64; 3]; 3]| cross(sub(t[1], t[0]), sub(t[2], t[0]));
    for (b, hit) in [
        ([[1., 0., 0.], [2., 0., 0.], [2., -1., 0.]], true),
        ([[0.3, 0.3, -1.], [0.3, 0.3, 1.], [0.8, 0.3, 0.]], true),
        ([[1.5, 1.5, 0.], [2., 1.5, 0.], [1.5, 2., 0.]], false),
        (a.map(|[x, y, _]| [x, y, 1e-9]), false),
    ] {
        assert_eq!(
            qem_triangles_intersect(a, n(a), b, n(b), ([0, 1, 2], [3, 4, 5])),
            hit
        );
    }
}

#[test]
fn triangle_contacts_allow_only_the_topologically_shared_simplex() {
    use super::mesh::qem_triangles_intersect;
    let a = [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]];
    let n = |t: [[f64; 3]; 3]| cross(sub(t[1], t[0]), sub(t[2], t[0]));
    for (b, vertices, hit) in [
        (
            [[0., 0., 0.], [-1., 0., 0.], [0., -1., 0.]],
            [0, 3, 4],
            false,
        ),
        (
            [[0., 0., 0.], [-1., 0., 0.], [0., 0., 1.]],
            [0, 3, 4],
            false,
        ),
        (
            [[0., 0., 0.], [1., 0.5, -1.], [1., 0.5, 1.]],
            [0, 3, 4],
            true,
        ),
        (
            [[0., 0., 0.], [2., 0., 0.], [0., -2., 0.]],
            [0, 1, 3],
            false,
        ),
        ([[0., 0., 0.], [2., 0., 0.], [1., 1., 1.]], [0, 1, 3], false),
        ([[0., 0., 0.], [2., 0., 0.], [1., 1., 0.]], [0, 1, 3], true),
        ([[0., 0., 0.], [2., 0., 0.], [0., -2., 0.]], [0, 3, 4], true),
    ] {
        assert_eq!(
            qem_triangles_intersect(a, n(a), b, n(b), ([0, 1, 2], vertices)),
            hit,
            "{b:?}, {vertices:?}"
        );
    }
}

#[test]
fn near_coplanar_incident_faces_cannot_cross() {
    use super::mesh::qem_triangles_intersect;
    let a = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    let b = [[0., 0., 0.], [1., 1., 4e-14], [-1., -0.5, -4.04e-14]];
    let n = |t: [[f64; 3]; 3]| cross(sub(t[1], t[0]), sub(t[2], t[0]));
    assert!(qem_triangles_intersect(
        a,
        n(a),
        b,
        n(b),
        ([0, 1, 2], [0, 3, 4])
    ));
}

#[test]
fn convex_intersections_distinguish_area_from_shared_edges() {
    use super::visible::{intersection_centroid, segment_interval};
    let square = [[4., 2.], [6., 2.], [6., 4.], [4., 4.]];
    let mut scratch = [Vec::new(), Vec::new()];
    for (offset, expected) in [(1., Some(([5.5, 3.], 2.))), (2., None)] {
        let other = square.map(|[x, y]| [x + offset, y]);
        assert_eq!(
            intersection_centroid(&square, &other, &mut scratch),
            expected
        );
    }
    assert_eq!(
        segment_interval([3., 2.], [7., 2.], &square),
        Some((0.25, 0.75))
    );
    assert_eq!(segment_interval([3., 1.], [7., 1.], &square), None);
}
