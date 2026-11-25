use super::mesh::{
    Grid, MAX_RAW, MAX_SAMPLES, MAX_SIDE, Mesh, REFINE_FALLBACKS, isosurface, isosurface_with_step,
    refine, simplify,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

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

fn volume(mesh: &Mesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|tri| {
            let [a, b, c] = tri.map(|v| mesh.positions[v as usize].map(f64::from));
            dot(a, cross(b, c)) / 6.
        })
        .sum()
}

fn edge_length_cv(mesh: &Mesh) -> f64 {
    let mut edges = HashSet::new();
    let mut lengths = Vec::new();
    for triangle in &mesh.triangles {
        for i in 0..3 {
            let (a, b) = (triangle[i], triangle[(i + 1) % 3]);
            if edges.insert((a.min(b), a.max(b))) {
                let d = sub(
                    mesh.positions[a as usize].map(f64::from),
                    mesh.positions[b as usize].map(f64::from),
                );
                lengths.push(dot(d, d).sqrt());
            }
        }
    }
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    let variance = lengths.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / lengths.len() as f64;
    variance.sqrt() / mean
}

fn summed_face_normals(mesh: &Mesh) -> Vec<[f64; 3]> {
    let mut sums = vec![[0.; 3]; mesh.positions.len()];
    for tri in &mesh.triangles {
        let [a, b, c] = tri.map(|v| mesh.positions[v as usize].map(f64::from));
        let face = cross(sub(b, a), sub(c, a));
        for &v in tri {
            for (sum, component) in sums[v as usize].iter_mut().zip(face) {
                *sum += component;
            }
        }
    }
    sums
}

fn check_gradient_normals(
    mesh: &Mesh,
    original: &Mesh,
    field: &impl Fn([f32; 3]) -> f32,
    step: f32,
) {
    let mut signs = vec![None; mesh.positions.len()];
    for (&tri, &positive) in mesh.triangles.iter().zip(&mesh.positive) {
        for v in tri {
            if let Some(previous) = signs[v as usize].replace(positive) {
                assert_eq!(previous, positive, "opposite phases share a vertex");
            }
        }
    }
    // A much smaller stencil than production's gives an independent check of
    // the field gradient and the opposite conventions at the two levels.
    let h = step * 1e-3;
    let face_normals = summed_face_normals(mesh);
    let mut wider_stencils = 0;
    let mut face_fallbacks = 0;
    for (i, (&p, &normal)) in mesh.positions.iter().zip(&mesh.normals).enumerate() {
        let displacement = sub(p.map(f64::from), original.positions[i].map(f64::from));
        assert!(
            dot(displacement, displacement) <= (0.5 * f64::from(step)).powi(2),
            "vertex {i} moved by more than half a grid step"
        );
        let gradient = |h| {
            std::array::from_fn(|axis| {
                let (mut lo, mut hi) = (p, p);
                lo[axis] -= h;
                hi[axis] += h;
                f64::from(field(hi)) - f64::from(field(lo))
            })
            .map(|g| if signs[i].unwrap() { -g } else { g })
        };
        let mut outward = gradient(h);
        let mut length = dot(outward, outward).sqrt();
        if !length.is_finite() || length == 0. {
            assert_eq!(p, original.positions[i], "moved vertex {i} has no gradient");
            // At a clamp boundary only the wider stencil may reach the field.
            outward = gradient(step * 1e-2);
            length = dot(outward, outward).sqrt();
            wider_stencils += 1;
        }
        if !length.is_finite() || length == 0. {
            // A restored QEM vertex can lie in a flat part of the field.
            let sum = face_normals[i];
            let face_length = dot(sum, sum).sqrt();
            assert!(face_length.is_finite() && face_length > 0.);
            let expected = sum.map(|x| x / face_length);
            assert!(
                normal
                    .iter()
                    .zip(expected)
                    .all(|(&n, e)| (f64::from(n) - e).abs() < 1e-5),
                "restored vertex {i} normal {normal:?} differs from face normal {expected:?}"
            );
            face_fallbacks += 1;
            continue;
        }
        assert!(
            dot(outward, normal.map(f64::from)) > 0.95 * length,
            "normal {i} points away from its phase's outward gradient"
        );
    }
    if wider_stencils > 0 {
        eprintln!(
            "restored normals: {wider_stencils} require a wider field stencil, {face_fallbacks} use area-weighted faces"
        );
    }
}

fn check_field(
    field: impl Fn([f32; 3]) -> f32,
    grid: Grid,
    iso: f32,
    components: &[(bool, isize)],
    radius: Option<f32>,
) -> (Mesh, Mesh) {
    let (mesh, step) = isosurface_with_step(&field, grid, iso);
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
    let fallbacks_before = REFINE_FALLBACKS.load(Ordering::Relaxed);
    let refined = refine(&reduced, &field, iso, step);
    assert_eq!(REFINE_FALLBACKS.load(Ordering::Relaxed), fallbacks_before);
    assert_eq!(refined.triangles, reduced.triangles);
    assert_eq!(refined.positive, reduced.positive);
    assert_eq!(refined.truncated, reduced.truncated);
    assert_eq!(refined.positions.len(), reduced.positions.len());
    for m in [&mesh, &reduced, &refined] {
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
    check_gradient_normals(&refined, &reduced, &field, step);
    if let Some(radius) = radius {
        let radius_errors: Vec<_> = refined
            .positions
            .iter()
            .map(|p| {
                let r = p.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>().sqrt();
                (r - f64::from(radius)).abs()
            })
            .collect();
        let max_radius_error = radius_errors.iter().copied().fold(0., f64::max);
        // Restored vertices retain the simplified mesh's existing level error.
        let max_projected_radius_error = radius_errors
            .iter()
            .zip(refined.positions.iter().zip(&reduced.positions))
            .filter_map(|(&error, (p, original))| (p != original).then_some(error))
            .fold(0., f64::max);
        let exact = 4. * std::f64::consts::PI * f64::from(radius).powi(3) / 3.;
        let before_volume = (volume(&reduced) / exact - 1.).abs();
        let after_volume = (volume(&refined) / exact - 1.).abs();
        let before_cv = edge_length_cv(&reduced);
        let after_cv = edge_length_cv(&refined);
        eprintln!(
            "refined sphere radius={radius} max_radius_error={max_radius_error:.9} max_projected_radius_error={max_projected_radius_error:.9} step={step} volume_error={before_volume:.6}->{after_volume:.6} edge_cv={before_cv:.6}->{after_cv:.6}"
        );
        assert!(max_projected_radius_error < 1e-3 * f64::from(step));
        assert!(
            after_volume < before_volume,
            "volume error {before_volume:.6} -> {after_volume:.6}"
        );
        assert!(
            after_cv < before_cv,
            "edge-length CV {before_cv:.6} -> {after_cv:.6}"
        );
    }
    (mesh, refined)
}

// These unsigned fixtures stop at zero so the negative level has no second shell.
fn sphere(p: [f32; 3]) -> f32 {
    (1. - p.iter().map(|x| x * x).sum::<f32>().sqrt()).max(0.)
}

#[test]
fn sphere_topology() {
    let mut worst_volume_error = 0_f64;
    for sign in [1., -1.] {
        let (_, refined) = check_field(
            |p| sign * sphere(p),
            grid([-1.23; 3], 0.12, [22; 3]),
            0.13,
            &[(sign > 0., 2)],
            Some(0.87),
        );
        let exact = 4. * std::f64::consts::PI * 0.87_f64.powi(3) / 3.;
        worst_volume_error = worst_volume_error.max((volume(&refined) / exact - 1.).abs());
    }
    // Vertices on the sphere: an inscribed polyhedron this coarse is at least
    // 0.54 % short of the ball's volume (Fejes Toth), so 1 % is the honest bound.
    assert!(
        worst_volume_error < 0.01,
        "refined sphere volume error {worst_volume_error:.6}"
    );
}

#[test]
fn opposite_lobe_topology() {
    let p = |v: [f32; 3]| v[2] * (-v.iter().map(|x| x * x).sum::<f32>().sqrt()).exp();
    check_field(
        p,
        grid([-1.77, -1.77, -2.81], 0.24, [16, 16, 25]),
        0.27,
        &[(false, 2), (true, 2)],
        None,
    );
}

#[test]
fn torus_topology() {
    let field =
        |[x, y, z]: [f32; 3]| (0.43f32.powi(2) - ((1.05 - x.hypot(y)).powi(2) + z * z)).max(0.);
    check_field(
        field,
        grid([-1.62, -1.62, -0.65], 0.13, [26, 26, 11]),
        0.035,
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
        field,
        grid([-1.75, -0.97, -0.97], step, [29, 17, 17]),
        1. - radius,
        &[(true, 2), (true, 2)],
        None,
    );
}

#[test]
fn grows_only_where_the_surface_leaves_the_box() {
    let initial = grid([-0.64, -1.07, -1.07], 0.12, [16, 19, 19]);
    let (mesh, _) = check_field(sphere, initial, 0.14, &[(true, 2)], Some(0.86));
    assert!(mesh.positions.iter().any(|p| p[0] < -0.83));
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

fn unit_octahedron() -> Mesh {
    let positions = vec![
        [1., 0., 0.],
        [0., 1., 0.],
        [-1., 0., 0.],
        [0., -1., 0.],
        [0., 0., 1.],
        [0., 0., -1.],
    ];
    Mesh {
        normals: positions.clone(),
        positions,
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
        positive: vec![true; 8],
        truncated: false,
    }
}

#[test]
fn refinement_restores_vertices_when_capped_projection_misses_the_level() {
    let step = 0.1;
    let mut mesh = unit_octahedron();
    for sign in [1., -1.] {
        mesh.positive.fill(sign > 0.);
        let field = |p: [f32; 3]| sign * (2. - p.iter().map(|x| x * x).sum::<f32>());
        let refined = refine(&mesh, field, 1., step);
        let volume_ratio = volume(&refined) / volume(&mesh);
        let distances: Vec<_> = refined
            .positions
            .iter()
            .map(|p| {
                // The analytic gradient makes this independent of refinement's stencil.
                let radius2 = p.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
                (1. - radius2).abs() / (2. * radius2.sqrt())
            })
            .collect();
        eprintln!(
            "capped projection sign={sign} distances={distances:?} volume_ratio={volume_ratio:.9}"
        );
        for (i, (&p, &distance)) in refined.positions.iter().zip(&distances).enumerate() {
            assert!(
                p == mesh.positions[i]
                    || (distance.is_finite() && distance <= 0.05 * f64::from(step)),
                "vertex {i} moved off its level by {distance}"
            );
        }
        assert!(volume_ratio >= 0.99, "volume ratio {volume_ratio:.9}");
    }
}

#[test]
fn refinement_restores_vertices_when_a_flat_field_hides_large_displacement() {
    let step = 0.1;
    let mut mesh = unit_octahedron();
    for sign in [1., -1.] {
        mesh.positive.fill(sign > 0.);
        let field = |p: [f32; 3]| {
            let s = p.iter().map(|x| x * x).sum::<f32>();
            sign * (1. + (1. - s) * (1.001 - (200. * (s - 0.12)).tanh()))
        };
        let refined = refine(&mesh, field, 1., step);
        let volume_ratio = volume(&refined) / volume(&mesh);
        let max_displacement = refined
            .positions
            .iter()
            .zip(&mesh.positions)
            .map(|(&p, &original)| {
                let d = sub(p.map(f64::from), original.map(f64::from));
                dot(d, d).sqrt()
            })
            .fold(0., f64::max);
        eprintln!(
            "flat field sign={sign} volume_ratio={volume_ratio:.9} max_displacement={max_displacement:.9}"
        );
        assert!(volume_ratio >= 0.99, "volume ratio {volume_ratio:.9}");
        assert!(
            max_displacement <= 0.5 * f64::from(step),
            "vertex displacement {max_displacement:.9} exceeds half a grid step"
        );
    }
}

#[test]
fn refinement_rollback_rechecks_collisions_and_keeps_unrelated_moves() {
    use super::mesh::guard_refinement;

    let tetrahedron = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, 0.5],
    ];
    let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
    let mut mesh = Mesh::default();
    for (component, positive) in [true, true, false, true].into_iter().enumerate() {
        mesh.positions
            .extend(tetrahedron.map(|[x, y, z]| [x + 4. * component as f32, y, z]));
        mesh.triangles
            .extend(faces.map(|t| t.map(|v| v + 4 * component as u32)));
        mesh.positive.extend([positive; 4]);
    }
    mesh.normals = vec![[0., 0., 1.]; mesh.positions.len()];
    assert_eq!(
        topology(&mesh),
        [(false, 2), (true, 2), (true, 2), (true, 2)]
    );
    check_embedded(&mesh);

    let mut candidate = mesh.positions.clone();
    // A overlaps stationary B. C occupies A's old position, so restoring A
    // creates a second collision across phases that requires another pass.
    candidate[..4].copy_from_slice(&mesh.positions[4..8]);
    candidate[8..12].copy_from_slice(&mesh.positions[..4]);
    for p in &mut candidate[12..] {
        p[1] += 0.25;
    }
    let control = candidate[12..].to_vec();
    assert!(guard_refinement(&mesh, &mut candidate));
    assert_eq!(candidate[..4], mesh.positions[..4], "A was not restored");
    assert_eq!(candidate[4..8], mesh.positions[4..8], "B moved");
    assert_eq!(
        candidate[8..12],
        mesh.positions[8..12],
        "C was not restored"
    );
    assert_eq!(
        candidate[12..],
        control,
        "unrelated legal move was discarded"
    );
    mesh.positions = candidate;
    check_embedded(&mesh);
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
fn refinement_with_nonfinite_fields_uses_face_normals() {
    let step = 0.25;
    let mut mesh = simplify(
        &isosurface(sphere, grid([-1.25; 3], step, [11; 3]), 0.2),
        500,
    );
    // Poison the cached normals so keeping the input normals cannot mask a
    // failed gradient or a failure to recompute normals after moving vertices.
    mesh.normals.fill([f32::NAN; 3]);
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let fallbacks_before = REFINE_FALLBACKS.load(Ordering::Relaxed);
        let refined = refine(&mesh, |_| invalid, 0.2, step);
        assert_eq!(REFINE_FALLBACKS.load(Ordering::Relaxed), fallbacks_before);
        assert_eq!(refined.positions, mesh.positions);
        assert_eq!(refined.triangles, mesh.triangles);
        assert_eq!(refined.positive, mesh.positive);
        assert!(refined.positions.iter().flatten().all(|x| x.is_finite()));
        assert_eq!(topology(&refined), [(true, 2)]);
        check_embedded(&refined);
        for (normal, sum) in refined.normals.iter().zip(summed_face_normals(&refined)) {
            let length = dot(sum, sum).sqrt();
            let expected = sum.map(|x| x / length);
            assert!(
                normal
                    .iter()
                    .zip(expected)
                    .all(|(&n, e)| (f64::from(n) - e).abs() < 1e-5),
                "normal {normal:?} differs from its area-weighted face normal {expected:?}"
            );
        }
    }
}

// Run with: cargo test -p reshiki-model --release refine_timing -- --ignored --nocapture
#[test]
#[ignore = "release-mode post-simplification refinement timing evidence"]
fn refine_timing() {
    use std::time::Instant;

    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "run this ignored benchmark with --release"
    );
    // Identical 12-lobe benzene field and extraction grid to view/budget.rs.
    let grid = grid([-5.5; 3], 11. / 96., [97; 3]);
    let centers: Vec<[f32; 3]> = (0..6)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 6.;
            [1.4 * angle.cos(), 1.4 * angle.sin(), 0.]
        })
        .collect();
    let field = |p: [f32; 3]| {
        centers
            .iter()
            .enumerate()
            .map(|(i, center)| {
                let d = std::array::from_fn::<_, 3, _>(|j| p[j] - center[j]);
                let sign = if i % 2 == 0 { 1. } else { -1. };
                sign * d[2] * (-1.6 * d.iter().map(|v| v * v).sum::<f32>().sqrt()).exp()
            })
            .sum()
    };
    let raw = isosurface(field, grid, 0.035);
    assert!(!raw.truncated);
    assert!(raw.triangles.len() > 20_000);
    let components: Vec<_> = [(false, 2); 6].into_iter().chain([(true, 2); 6]).collect();
    assert_eq!(topology(&raw), components);
    let mut timing_failures = Vec::new();
    for target in [20_000, 4_000] {
        let mesh = simplify(&raw, target);
        assert!(mesh.triangles.len() <= target);
        assert!(mesh.triangles.len() >= target * 9 / 10);
        let fallbacks_before = REFINE_FALLBACKS.load(Ordering::Relaxed);
        // Extraction, simplification and all verification are outside the timer.
        let start = Instant::now();
        let refined =
            std::hint::black_box(refine(std::hint::black_box(&mesh), field, 0.035, grid.step));
        let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.;
        let fallbacks = REFINE_FALLBACKS.load(Ordering::Relaxed) - fallbacks_before;
        eprintln!(
            "refine scene=12-lobe samples={} raw={} target={target} vertices={} triangles={} refine_ms={elapsed_ms:.2} fallbacks={fallbacks} edge_cv={:.6}->{:.6}",
            97_usize.pow(3),
            raw.triangles.len(),
            refined.positions.len(),
            refined.triangles.len(),
            edge_length_cv(&mesh),
            edge_length_cv(&refined),
        );
        assert_eq!(
            fallbacks, 0,
            "timing must exercise refinement without fallback"
        );
        assert_eq!(refined.triangles, mesh.triangles);
        assert_eq!(refined.positive, mesh.positive);
        assert_eq!(topology(&refined), components);
        check_embedded(&refined);
        check_gradient_normals(&refined, &mesh, &field, grid.step);
        if target == 20_000 && elapsed_ms > 150. {
            timing_failures.push(format!(
                "target={target}: refinement took {elapsed_ms:.2}ms, budget 150ms"
            ));
        }
    }
    assert!(timing_failures.is_empty(), "{}", timing_failures.join("\n"));
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
