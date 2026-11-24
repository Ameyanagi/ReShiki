//! Closed surfaces of a signed scalar field, in model coordinates (Å).

// Lattice strides, tetrahedron corners and triangle indices all refer to the
// corresponding bounded arrays. A collapse retains these indices until compaction.
#![allow(clippy::indexing_slicing)]

use std::collections::HashMap;

pub const MAX_SIDE: usize = 97;
pub const MAX_SAMPLES: usize = 1_500_000;
pub const MAX_RAW: usize = 3_000_000;

/// A closed triangle mesh with outward normals and one phase sign per face.
#[derive(Clone, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub positive: Vec<bool>,
    pub truncated: bool,
}

/// A regular sample lattice in model space.
#[derive(Clone, Copy)]
pub struct Grid {
    pub origin: [f32; 3],
    pub step: f32,
    pub size: [usize; 3],
}

impl Grid {
    /// Cover the box with cubic voxels.
    pub fn around(lo: [f32; 3], hi: [f32; 3]) -> Grid {
        Self::fit(lo, hi, MAX_SIDE)
    }

    fn fit(origin: [f32; 3], hi: [f32; 3], side: usize) -> Grid {
        let extent = std::array::from_fn::<_, 3, _>(|i| f64::from(hi[i]) - f64::from(origin[i]));
        let longest = extent.into_iter().fold(0., f64::max);
        let mut step = if longest > 0. {
            (longest / (side - 1) as f64) as f32
        } else {
            1.
        };
        if f64::from(step) * ((side - 1) as f64) < longest {
            step = step.next_up();
        }
        loop {
            let size = extent.map(|d| (1 + (d / f64::from(step)).ceil() as usize).min(side));
            if size.iter().product::<usize>() <= MAX_SAMPLES {
                return Grid { origin, step, size };
            }
            step *= 1.01;
        }
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn corner(i: usize) -> [usize; 3] {
    [i & 1, (i >> 1) & 1, i >> 2]
}

// Each mask lists oriented triangles whose vertices are pairs of cube corners.
type Crossings = Vec<[[usize; 2]; 3]>;

static CUBE_CASES: std::sync::LazyLock<[Crossings; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|mask| {
        let mut triangles = Vec::new();
        for [a, b] in [[1, 3], [3, 2], [2, 6], [6, 4], [4, 5], [5, 1]] {
            let (inside, outside): (Vec<_>, Vec<_>) =
                [0, a, b, 7].into_iter().partition(|i| mask & (1 << i) != 0);
            let edges = match (inside.as_slice(), outside.as_slice()) {
                ([i], [a, b, c]) | ([a, b, c], [i]) => vec![[*i, *a], [*i, *b], [*i, *c]],
                ([i, j], [a, b]) => vec![[*i, *a], [*i, *b], [*j, *b], [*j, *a]],
                _ => continue,
            };
            for j in 1..edges.len() - 1 {
                let mut triangle = [edges[0], edges[j], edges[j + 1]];
                // Winding depends only on the mask. Midpoints avoid near-level ties.
                let [p, q, r] = triangle.map(|[u, v]| {
                    std::array::from_fn(|i| (corner(u)[i] + corner(v)[i]) as f64 * 0.5)
                });
                if dot(
                    cross(sub(q, p), sub(r, p)),
                    sub(corner(inside[0]).map(|x| x as f64), p),
                ) > 0.
                {
                    triangle.swap(1, 2);
                }
                triangles.push(triangle);
            }
        }
        triangles
    })
});

/// Extract both levels, growing the box to contain the field when possible.
pub fn isosurface(field: impl Fn([f32; 3]) -> f32, mut grid: Grid, iso: f32) -> Mesh {
    assert!(
        iso.is_finite() && iso > 0.,
        "isovalue must be finite and positive"
    );
    if grid.size.contains(&0) {
        return Mesh::default();
    }
    if grid.size.iter().any(|&n| n > MAX_SIDE) {
        grid = Grid::around(
            grid.origin,
            std::array::from_fn(|i| grid.origin[i] + (grid.size[i] - 1) as f32 * grid.step),
        );
    }
    let cases = &*CUBE_CASES;
    let mut growths = 0;
    loop {
        let [nx, ny, nz] = grid.size.map(|n| n + 2);
        let strides = [1, nx, nx * ny];
        let mut values = vec![0f32; nx * ny * nz];
        let mut peak = 0f32;
        let mut grow_lo = [false; 3];
        let mut grow_hi = [false; 3];
        for z in 1..nz - 1 {
            for y in 1..ny - 1 {
                for x in 1..nx - 1 {
                    let ijk = [x - 1, y - 1, z - 1];
                    let value = field(std::array::from_fn(|i| {
                        (grid.origin[i] as f64 + ijk[i] as f64 * grid.step as f64) as f32
                    }));
                    peak = peak.max(value.abs());
                    if value.abs() >= iso {
                        for i in 0..3 {
                            grow_lo[i] |= ijk[i] == 0;
                            grow_hi[i] |= ijk[i] + 1 == grid.size[i];
                        }
                    }
                    values[x + nx * (y + ny * z)] =
                        if (value.abs() - iso).abs() < 1e-6 * iso || value.abs() == iso {
                            (iso * (1. - 1e-6)).min(iso.next_down()).copysign(value)
                        } else {
                            value
                        };
                }
            }
        }
        let truncated = grow_lo.into_iter().chain(grow_hi).any(|b| b);
        if truncated && growths < 3 {
            let extent = grid.size.map(|n| (n - 1) as f32 * grid.step);
            let lo = std::array::from_fn(|i| {
                grid.origin[i] - if grow_lo[i] { extent[i] * 0.25 } else { 0. }
            });
            let hi = std::array::from_fn(|i| {
                grid.origin[i] + extent[i] * if grow_hi[i] { 1.25 } else { 1. }
            });
            grid = Grid::fit(lo, hi, grid.size.into_iter().fold(2, usize::max));
            growths += 1;
            continue;
        }

        // Count crossings before allocating a potentially very large welded mesh.
        let offsets: [usize; 8] =
            std::array::from_fn(|i| corner(i).iter().zip(strides).map(|(u, s)| u * s).sum());
        let mut active = Vec::new();
        let mut count = 0;
        'count: for z in 0..nz - 1 {
            for y in 0..ny - 1 {
                for x in 0..nx - 1 {
                    let base = x + nx * (y + ny * z);
                    let mut masks = [0u8; 2];
                    for (i, offset) in offsets.iter().enumerate() {
                        let v = values[base + offset];
                        masks[0] |= u8::from(v > iso) << i;
                        masks[1] |= u8::from(v < -iso) << i;
                    }
                    for (phase, mask) in masks.into_iter().enumerate() {
                        if mask != 0 && mask != 255 {
                            count += cases[mask as usize].len();
                            if count > MAX_RAW {
                                break 'count;
                            }
                            active.push((base as u32, mask, phase == 0));
                        }
                    }
                }
            }
        }
        if count > MAX_RAW {
            grid.step *= 2.;
            grid.size = grid.size.map(|n| (n - 1).div_ceil(2) + 1);
            continue;
        }

        // A crossing extremely close to a grid node can round back onto it.
        // Move that sample away from its level, keeping its classification and
        // one shared linear field for every tetrahedron meeting at the node.
        // If the two level residuals have magnitudes a and b, their crossing
        // fraction is a/(a+b). For at least one coordinate ULP of clearance
        // (fraction u), enforce a >= b*u/(1-u) at either end of a cut edge.
        let axis_ulp: [f64; 3] = std::array::from_fn(|i| {
            let end = grid.origin[i] + grid.step * grid.size[i] as f32;
            let reach = (grid.origin[i] - grid.step).abs().max(end.abs());
            f64::from(reach.next_up() - reach)
        });
        let ratios: [f64; 8] = std::array::from_fn(|j| {
            let ulp = (0..3)
                .filter(|&d| corner(j)[d] != 0)
                .map(|d| axis_ulp[d])
                .fold(0., f64::max);
            let fraction = ulp / f64::from(grid.step);
            fraction / (1. - fraction)
        });
        let near_level =
            ratios.iter().copied().fold(0., f64::max) * (f64::from(peak) + f64::from(iso));
        let mut near = Vec::new();
        for z in 1..nz - 1 {
            for y in 1..ny - 1 {
                for x in 1..nx - 1 {
                    let i = x + nx * (y + ny * z);
                    if f64::from((values[i].abs() - iso).abs()) < near_level {
                        near.push(i);
                    }
                }
            }
        }
        loop {
            let mut changed = false;
            for &i in &near {
                let v = values[i];
                let inside = v.abs() > iso;
                let mut margin = 0.;
                // The seven increasing grid directions and their reverses are
                // exactly the edges used by the tetrahedra at an interior node.
                for (&d, &ratio) in offsets[1..].iter().zip(&ratios[1..]) {
                    for w in [values[i - d], values[i + d]] {
                        let residual =
                            f64::from(if v.is_sign_negative() { -w } else { w }) - f64::from(iso);
                        if (residual > 0.) != inside {
                            margin = f64::max(margin, ratio * residual.abs());
                        }
                    }
                }
                if (f64::from(v.abs()) - f64::from(iso)).abs() < margin {
                    let exact = if inside {
                        f64::from(iso) + margin
                    } else {
                        (f64::from(iso) - margin).max(0.)
                    };
                    let mut magnitude = exact as f32;
                    if inside && f64::from(magnitude) < exact {
                        magnitude = magnitude.next_up();
                    }
                    if !inside && f64::from(magnitude) > exact {
                        magnitude = magnitude.next_down();
                    }
                    let adjusted = magnitude.copysign(v);
                    changed |= adjusted != v;
                    values[i] = adjusted;
                }
            }
            if !changed {
                break;
            }
        }

        let mut mesh = Mesh {
            triangles: Vec::with_capacity(count),
            positive: Vec::with_capacity(count),
            truncated,
            ..Mesh::default()
        };
        let mut welded = HashMap::new();
        let h = grid.step * 0.5;
        let mut needs_face_normals = false;
        for (base, mask, positive) in active {
            let base = base as usize;
            let cell = [base % nx, base / nx % ny, base / (nx * ny)];
            let level = if positive { iso } else { -iso } as f64;
            for edges in &cases[mask as usize] {
                let triangle = edges.map(|[a, b]| {
                    let [u, v] = [base + offsets[a], base + offsets[b]];
                    let key =
                        ((u.min(v) as u64) << 32) | ((u.max(v) as u64) << 1) | positive as u64;
                    *welded.entry(key).or_insert_with(|| {
                        let t = (level - values[u] as f64) / (values[v] as f64 - values[u] as f64);
                        let p = std::array::from_fn(|i| {
                            let local = corner(a)[i] as f64
                                + t * (corner(b)[i] as f64 - corner(a)[i] as f64);
                            (grid.origin[i] as f64
                                + (cell[i] as f64 - 1. + local) * grid.step as f64)
                                as f32
                        });
                        let id = mesh.positions.len() as u32;
                        mesh.positions.push(p);
                        let n = std::array::from_fn(|i| {
                            let (mut lo, mut hi) = (p, p);
                            hi[i] += h;
                            lo[i] -= h;
                            let diff = (f64::from(field(hi)) - f64::from(field(lo)))
                                / f64::from(grid.step);
                            if positive { -diff } else { diff }
                        });
                        mesh.normals.push(if dot(n, n) < 1e-18 {
                            needs_face_normals = true;
                            [0.; 3]
                        } else {
                            unit_normal(n).map(|v| v as f32)
                        });
                        id
                    })
                });
                mesh.triangles.push(triangle);
                mesh.positive.push(positive);
            }
        }
        if needs_face_normals {
            let mut summed = vec![[0.; 3]; mesh.positions.len()];
            for &tri in &mesh.triangles {
                let [a, b, c] = tri.map(|v| mesh.positions[v as usize].map(f64::from));
                let n = cross(sub(b, a), sub(c, a));
                for v in tri {
                    let sum = &mut summed[v as usize];
                    for i in 0..3 {
                        sum[i] += n[i];
                    }
                }
            }
            for (normal, sum) in mesh.normals.iter_mut().zip(summed) {
                if *normal == [0.; 3] {
                    *normal = unit_normal(sum).map(|v| v as f32);
                }
            }
        }
        return mesh;
    }
}

/// Reduce a closed surface with area-weighted Garland–Heckbert quadrics.
/// Geometry and topology checks may leave the result above the requested budget.
pub fn simplify(mesh: &Mesh, target: usize) -> Mesh {
    if mesh.triangles.len() <= target {
        return qem_compact(
            &mesh.positions,
            &mesh.normals,
            mesh.triangles
                .iter()
                .copied()
                .zip(mesh.positive.iter().copied()),
            mesh.truncated,
        );
    }
    let vertex_count = mesh.positions.len();
    let mut positions = mesh.positions.clone();
    let mut normals = mesh.normals.clone();
    let mut triangles = mesh.triangles.clone();
    let mut alive = vec![true; triangles.len()];
    let mut degree = vec![0usize; vertex_count];
    for triangle in &triangles {
        for &v in triangle {
            degree[v as usize] += 1;
        }
    }
    let mut incident: Vec<_> = degree.into_iter().map(Vec::with_capacity).collect();
    let mut quadrics = vec![[0.0; 10]; vertex_count];
    let mut face_normals = Vec::with_capacity(triangles.len());
    let mut bounds = Vec::with_capacity(triangles.len());
    let mut area = 0.0;
    for (id, &t) in triangles.iter().enumerate() {
        let p = t.map(|v| positions[v as usize].map(f64::from));
        let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        face_normals.push(n);
        bounds.push(qem_bounds(p));
        let length = dot(n, n).sqrt();
        let weight = length * 0.5;
        area += weight;
        if length > 0.0 {
            let n = n.map(|v| v / length);
            let plane = [n[0], n[1], n[2], -dot(n, p[0])];
            // Upper triangle of the symmetric homogeneous plane quadric.
            let [x, y, z, w] = plane;
            let q = [
                x * x,
                x * y,
                x * z,
                x * w,
                y * y,
                y * z,
                y * w,
                z * z,
                z * w,
                w * w,
            ]
            .map(|v| v * weight);
            for &v in &t {
                qem_add_assign(&mut quadrics[v as usize], q);
            }
        }
        for &v in &t {
            incident[v as usize].push(id);
        }
    }
    let mut grid = QemGrid::new(&bounds, &alive, area);
    let mut versions = vec![0u32; vertex_count];
    let mut heap = qem_heap(&positions, &quadrics, &versions, &incident, &triangles);
    let mut seen = vec![0u32; triangles.len()];
    let mut query_stamp = 0u32;
    let mut candidates = Vec::new();
    let mut omitted = vec![false; triangles.len()];
    let mut adjacent_a = Vec::new();
    let mut adjacent_b = Vec::new();
    let mut changes = Vec::new();
    let mut remaining = triangles.len();
    let mut last_rebuild = remaining;
    let mut grid_rebuild = remaining;

    while remaining > target {
        let Some(edge) = heap.pop() else {
            // An earlier refusal may become legal after nearby geometry changes.
            if remaining == last_rebuild {
                break;
            }
            heap = qem_heap(&positions, &quadrics, &versions, &incident, &triangles);
            last_rebuild = remaining;
            continue;
        };
        let (a, b) = (edge.a as usize, edge.b as usize);
        if edge.va != versions[a]
            || edge.vb != versions[b]
            || incident[a].is_empty()
            || incident[b].is_empty()
        {
            continue;
        }
        qem_neighbours(a, &incident, &triangles, &mut adjacent_a);
        qem_neighbours(b, &incident, &triangles, &mut adjacent_b);
        let mut opposite = [0usize; 2];
        let mut edge_faces = [0usize; 2];
        let mut edge_uses = 0usize;
        for &id in &incident[a] {
            let t = triangles[id];
            if t.contains(&(b as u32)) {
                if edge_uses < 2 {
                    for v in t {
                        if v as usize != a && v as usize != b {
                            opposite[edge_uses] = v as usize;
                            edge_faces[edge_uses] = id;
                        }
                    }
                }
                edge_uses += 1;
            }
        }
        if edge_uses != 2 || opposite[0] == opposite[1] {
            continue;
        }
        // Both vertices and edges must satisfy the link condition. A shared
        // opposite edge is the boundary of a tetrahedron, which must stay closed.
        let common = adjacent_a
            .iter()
            .filter(|v| adjacent_b.binary_search(v).is_ok())
            .count();
        if common != 2 {
            continue;
        }
        if [a, b].into_iter().all(|v| {
            incident[v].iter().any(|&id| {
                triangles[id].contains(&(opposite[0] as u32))
                    && triangles[id].contains(&(opposite[1] as u32))
            })
        }) {
            continue;
        }
        changes.clear();
        let mut legal = true;
        for &id in incident[a].iter().chain(&incident[b]) {
            let old = triangles[id];
            if edge_faces.contains(&id) {
                continue;
            }
            let new = old.map(|v| if v as usize == b { a as u32 } else { v });
            let points = new.map(|v| {
                if v as usize == a {
                    edge.point.map(f64::from)
                } else {
                    positions[v as usize].map(f64::from)
                }
            });
            let previous = if old.contains(&(a as u32)) { a } else { b };
            let moved = edge.point != positions[previous];
            let normal = if moved {
                cross(sub(points[1], points[0]), sub(points[2], points[0]))
            } else {
                face_normals[id]
            };
            if moved && !qem_shape_ok(face_normals[id], normal, points) {
                legal = false;
                break;
            }
            let bound = qem_bounds(points);
            changes.push((id, new, bound, points, moved, normal));
        }
        if legal {
            for &id in incident[a].iter().chain(&incident[b]) {
                omitted[id] = true;
            }
            'faces: for &(_, new, bound, points, moved, normal) in &changes {
                if !moved {
                    continue;
                }
                grid.query(
                    bound,
                    &omitted,
                    &mut seen,
                    &mut query_stamp,
                    &mut candidates,
                );
                for &other in &candidates {
                    if (0..3)
                        .any(|k| bound.1[k] < bounds[other].0[k] || bounds[other].1[k] < bound.0[k])
                        || new.iter().any(|v| triangles[other].contains(v))
                    {
                        continue;
                    }
                    if qem_triangles_intersect(
                        points,
                        normal,
                        triangles[other].map(|v| positions[v as usize].map(f64::from)),
                        face_normals[other],
                    ) {
                        legal = false;
                        break 'faces;
                    }
                }
            }
            for &id in incident[a].iter().chain(&incident[b]) {
                omitted[id] = false;
            }
        }
        if !legal {
            // A legal endpoint can remove a narrow face when the optimum would
            // make a new needle. Queue each remaining position at its own cost.
            let mut q = quadrics[a];
            qem_add_assign(&mut q, quadrics[b]);
            let alternatives = qem_fallbacks(a, b, &positions, &q);
            for alternative in (edge.alternative + 1)..=3 {
                let point = alternatives[usize::from(alternative) - 1];
                if point != edge.point {
                    heap.push(QemCollapse {
                        point,
                        cost: qem_error(&q, point.map(f64::from)),
                        alternative,
                        ..edge
                    });
                    break;
                }
            }
            continue;
        }

        let direction = sub(positions[b].map(f64::from), positions[a].map(f64::from));
        let length2 = dot(direction, direction);
        let t = if length2 > 0.0 {
            (dot(
                sub(edge.point.map(f64::from), positions[a].map(f64::from)),
                direction,
            ) / length2)
                .clamp(0.0, 1.0)
        } else {
            0.5
        };
        let normal = unit_normal(std::array::from_fn(|i| {
            f64::from(normals[a][i]) * (1.0 - t) + f64::from(normals[b][i]) * t
        }));
        for id in edge_faces {
            grid.remove(id);
            for v in triangles[id] {
                let faces = &mut incident[v as usize];
                if let Some(index) = faces.iter().position(|&face| face == id) {
                    faces.swap_remove(index);
                }
            }
            alive[id] = false;
        }
        positions[a] = edge.point;
        normals[a] = if dot(normal, normal) > 0.0 {
            normal.map(|v| v as f32)
        } else {
            normals[a]
        };
        let removed_quadric = quadrics[b];
        qem_add_assign(&mut quadrics[a], removed_quadric);
        versions[a] += 1;
        versions[b] += 1;
        for &(id, triangle, bound, _, _, normal) in &changes {
            face_normals[id] = normal;
            if triangles[id] != triangle {
                triangles[id] = triangle;
                incident[a].push(id);
            }
            if bounds[id] != bound {
                grid.update(id, bound);
                bounds[id] = bound;
            }
        }
        incident[b].clear();
        remaining -= 2;
        qem_neighbours(a, &incident, &triangles, &mut adjacent_a);
        for &b in &adjacent_a {
            heap.push(qem_candidate(a, b, &positions, &quadrics, &versions));
        }
        if remaining < grid_rebuild / 4 {
            grid = QemGrid::new(&bounds, &alive, area);
            grid_rebuild = remaining;
        }
    }

    qem_compact(
        &positions,
        &normals,
        triangles
            .into_iter()
            .zip(mesh.positive.iter().copied())
            .enumerate()
            .filter_map(|(id, face)| alive[id].then_some(face)),
        mesh.truncated,
    )
}

fn qem_compact(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    triangles: impl Iterator<Item = ([u32; 3], bool)>,
    truncated: bool,
) -> Mesh {
    let mut result = Mesh {
        truncated,
        ..Mesh::default()
    };
    let mut remap = vec![u32::MAX; positions.len()];
    for (triangle, positive) in triangles {
        let triangle = triangle.map(|v| {
            let v = v as usize;
            if remap[v] == u32::MAX {
                remap[v] = result.positions.len() as u32;
                result.positions.push(positions[v]);
                result.normals.push(normals[v]);
            }
            remap[v]
        });
        result.triangles.push(triangle);
        result.positive.push(positive);
    }
    result
}

type QemBounds = ([f64; 3], [f64; 3]);

fn unit_normal(n: [f64; 3]) -> [f64; 3] {
    let length = dot(n, n).sqrt();
    if length > 0.0 && length.is_finite() {
        n.map(|v| v / length)
    } else {
        [0.0; 3]
    }
}

fn qem_add_assign(a: &mut [f64; 10], b: [f64; 10]) {
    for (a, b) in a.iter_mut().zip(b) {
        *a += b;
    }
}

fn qem_error(q: &[f64; 10], p: [f64; 3]) -> f64 {
    let [x, y, z] = p;
    (q[0] * x * x
        + 2.0 * q[1] * x * y
        + 2.0 * q[2] * x * z
        + 2.0 * q[3] * x
        + q[4] * y * y
        + 2.0 * q[5] * y * z
        + 2.0 * q[6] * y
        + q[7] * z * z
        + 2.0 * q[8] * z
        + q[9])
        .max(0.0)
}

pub(super) fn qem_optimum(q: &[f64; 10]) -> Option<[f64; 3]> {
    let [a, b, c, d, e, f] = [q[0], q[1], q[2], q[4], q[5], q[7]];
    let aa = d * f - e * e;
    let ab = c * e - b * f;
    let ac = b * e - c * d;
    let dd = a * f - c * c;
    let de = b * c - a * e;
    let ff = a * d - b * b;
    let determinant = a * aa + b * ab + c * ac;
    let matrix_norm = (a.abs() + b.abs() + c.abs())
        .max(b.abs() + d.abs() + e.abs())
        .max(c.abs() + e.abs() + f.abs());
    let adjugate_norm = (aa.abs() + ab.abs() + ac.abs())
        .max(ab.abs() + dd.abs() + de.abs())
        .max(ac.abs() + de.abs() + ff.abs());
    // Reject a solve whose reciprocal one-norm condition is below 1e-10.
    if determinant <= 1e-10 * matrix_norm * adjugate_norm || !determinant.is_finite() {
        return None;
    }
    let point = [
        -(aa * q[3] + ab * q[6] + ac * q[8]) / determinant,
        -(ab * q[3] + dd * q[6] + de * q[8]) / determinant,
        -(ac * q[3] + de * q[6] + ff * q[8]) / determinant,
    ];
    point.iter().all(|v| v.is_finite()).then_some(point)
}

#[derive(Clone, Copy)]
struct QemCollapse {
    cost: f64,
    point: [f32; 3],
    a: u32,
    b: u32,
    va: u32,
    vb: u32,
    alternative: u8,
}

impl PartialEq for QemCollapse {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for QemCollapse {}

impl PartialOrd for QemCollapse {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QemCollapse {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.a.cmp(&self.a))
            .then_with(|| other.b.cmp(&self.b))
            .then_with(|| other.va.cmp(&self.va))
            .then_with(|| other.vb.cmp(&self.vb))
            .then_with(|| other.alternative.cmp(&self.alternative))
            .then_with(|| other.point[0].total_cmp(&self.point[0]))
            .then_with(|| other.point[1].total_cmp(&self.point[1]))
            .then_with(|| other.point[2].total_cmp(&self.point[2]))
    }
}

fn qem_candidate(
    a: usize,
    b: usize,
    positions: &[[f32; 3]],
    quadrics: &[[f64; 10]],
    versions: &[u32],
) -> QemCollapse {
    let (a, b) = if a < b { (a, b) } else { (b, a) };
    let mut q = quadrics[a];
    qem_add_assign(&mut q, quadrics[b]);
    let (point, alternative) = if let Some(point) = qem_optimum(&q) {
        (point.map(|v| v as f32), 0)
    } else {
        (qem_fallbacks(a, b, positions, &q)[0], 1)
    };
    // Placements stay in f32 so validation uses exactly the returned geometry.
    QemCollapse {
        cost: qem_error(&q, point.map(f64::from)),
        point,
        a: a as u32,
        b: b as u32,
        va: versions[a],
        vb: versions[b],
        alternative,
    }
}

fn qem_fallbacks(a: usize, b: usize, positions: &[[f32; 3]], q: &[f64; 10]) -> [[f32; 3]; 3] {
    let midpoint = std::array::from_fn(|i| {
        ((f64::from(positions[a][i]) + f64::from(positions[b][i])) * 0.5) as f32
    });
    let mut points = [positions[a], positions[b], midpoint];
    points.sort_unstable_by(|&a, &b| {
        qem_error(q, a.map(f64::from)).total_cmp(&qem_error(q, b.map(f64::from)))
    });
    points
}

fn qem_neighbours(
    v: usize,
    incident: &[Vec<usize>],
    triangles: &[[u32; 3]],
    neighbours: &mut Vec<usize>,
) {
    neighbours.clear();
    for &face in &incident[v] {
        neighbours.extend(
            triangles[face]
                .iter()
                .map(|&v| v as usize)
                .filter(|&other| other != v),
        );
    }
    neighbours.sort_unstable();
    neighbours.dedup();
}

fn qem_heap(
    positions: &[[f32; 3]],
    quadrics: &[[f64; 10]],
    versions: &[u32],
    incident: &[Vec<usize>],
    triangles: &[[u32; 3]],
) -> std::collections::BinaryHeap<QemCollapse> {
    let edge_count = incident.iter().map(Vec::len).sum::<usize>() / 2;
    let mut edges = Vec::with_capacity(edge_count);
    let mut neighbours = Vec::new();
    for a in 0..positions.len() {
        qem_neighbours(a, incident, triangles, &mut neighbours);
        for &b in &neighbours {
            if b > a {
                edges.push(qem_candidate(a, b, positions, quadrics, versions));
            }
        }
    }
    std::collections::BinaryHeap::from(edges)
}

pub(super) fn qem_shape_ok(old_normal: [f64; 3], new_normal: [f64; 3], new: [[f64; 3]; 3]) -> bool {
    let old_length2 = dot(old_normal, old_normal);
    let new_length2 = dot(new_normal, new_normal);
    if old_length2 == 0.0
        || new_length2 == 0.0
        || !new_length2.is_finite()
        || dot(old_normal, new_normal) < 0.2 * (old_length2 * new_length2).sqrt()
    {
        return false;
    }
    const COS_3_DEGREES_SQUARED: f64 = 0.9972609476841366;
    for corner in 0..3 {
        let a = sub(new[(corner + 1) % 3], new[corner]);
        let b = sub(new[(corner + 2) % 3], new[corner]);
        let product = dot(a, b);
        if product > 0.0 && product * product > COS_3_DEGREES_SQUARED * dot(a, a) * dot(b, b) {
            return false;
        }
    }
    true
}

fn qem_bounds(points: [[f64; 3]; 3]) -> QemBounds {
    (
        std::array::from_fn(|axis| points[0][axis].min(points[1][axis]).min(points[2][axis])),
        std::array::from_fn(|axis| points[0][axis].max(points[1][axis]).max(points[2][axis])),
    )
}

/// Separating axes include in-plane edge normals, so coplanar overlap counts.
pub(super) fn qem_triangles_intersect(
    a: [[f64; 3]; 3],
    normal_a: [f64; 3],
    b: [[f64; 3]; 3],
    normal_b: [f64; 3],
) -> bool {
    let origin = a[0];
    let a = a.map(|p| sub(p, origin));
    let b = b.map(|p| sub(p, origin));
    let extent = a
        .iter()
        .chain(&b)
        .flatten()
        .fold(0.0_f64, |largest, &v| largest.max(v.abs()));
    let separated = |axis: [f64; 3]| {
        let pa = a.map(|p| dot(p, axis));
        let pb = b.map(|p| dot(p, axis));
        let min_a = pa[0].min(pa[1]).min(pa[2]);
        let max_a = pa[0].max(pa[1]).max(pa[2]);
        let min_b = pb[0].min(pb[1]).min(pb[2]);
        let max_b = pb[0].max(pb[1]).max(pb[2]);
        let tolerance = 64.0 * f64::EPSILON * axis.iter().map(|v| v.abs()).sum::<f64>() * extent;
        max_a < min_b - tolerance || max_b < min_a - tolerance
    };
    // Cached face normals usually reject disjoint pairs before the edge axes.
    if separated(normal_a) || separated(normal_b) {
        return false;
    }
    let edges_a = [sub(a[1], a[0]), sub(a[2], a[1]), sub(a[0], a[2])];
    let edges_b = [sub(b[1], b[0]), sub(b[2], b[1]), sub(b[0], b[2])];
    for edge_a in edges_a {
        if separated(cross(normal_a, edge_a)) {
            return false;
        }
        for edge_b in edges_b {
            if separated(cross(edge_a, edge_b)) {
                return false;
            }
        }
    }
    for edge_b in edges_b {
        if separated(cross(normal_b, edge_b)) {
            return false;
        }
    }
    true
}

// Bounds map into a bounded, collision-free uniform spatial hash. Clamping
// outside coordinates to the border keeps every possible intersection candidate.
struct QemGrid {
    buckets: Vec<Vec<usize>>,
    ranges: Vec<([usize; 3], [usize; 3])>,
    size: [usize; 3],
    origin: [f64; 3],
    inverse_cell: f64,
}

impl QemGrid {
    fn new(bounds: &[QemBounds], alive: &[bool], area: f64) -> Self {
        let mut origin = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        let mut count = 0usize;
        for (&bound, &alive) in bounds.iter().zip(alive) {
            if alive {
                count += 1;
                for axis in 0..3 {
                    origin[axis] = origin[axis].min(bound.0[axis]);
                    maximum[axis] = maximum[axis].max(bound.1[axis]);
                }
            }
        }
        let extent = (0..3).fold(0.0_f64, |v, axis| v.max(maximum[axis] - origin[axis]));
        let mut cell = (2.5 * (area / count as f64).sqrt())
            .max(extent * 1e-8)
            .max(1e-12);
        let size = loop {
            let size: [usize; 3] = std::array::from_fn(|axis| {
                ((maximum[axis] - origin[axis]) / cell).ceil() as usize + 1
            });
            let cells = size.into_iter().fold(1usize, usize::saturating_mul);
            if cells <= 2_000_000 {
                break size;
            }
            cell *= (cells as f64 / 2_000_000.).cbrt().max(1.1);
        };
        let mut grid = Self {
            buckets: vec![Vec::new(); size.into_iter().product()],
            ranges: vec![([0; 3], [0; 3]); bounds.len()],
            size,
            origin,
            inverse_cell: 1.0 / cell,
        };
        for (id, (&bound, &alive)) in bounds.iter().zip(alive).enumerate() {
            if alive {
                grid.ranges[id] = grid.range(bound);
                grid.insert(id);
            }
        }
        grid
    }

    fn range(&self, bound: QemBounds) -> ([usize; 3], [usize; 3]) {
        let cell = |p: [f64; 3]| {
            std::array::from_fn(|axis| {
                (((p[axis] - self.origin[axis]) * self.inverse_cell).max(0.) as usize)
                    .min(self.size[axis] - 1)
            })
        };
        (cell(bound.0), cell(bound.1))
    }

    fn insert(&mut self, id: usize) {
        let (low, high) = self.ranges[id];
        for z in low[2]..=high[2] {
            for y in low[1]..=high[1] {
                let row = (z * self.size[1] + y) * self.size[0];
                for x in low[0]..=high[0] {
                    self.buckets[row + x].push(id);
                }
            }
        }
    }

    fn remove(&mut self, id: usize) {
        let (low, high) = self.ranges[id];
        for z in low[2]..=high[2] {
            for y in low[1]..=high[1] {
                let row = (z * self.size[1] + y) * self.size[0];
                for x in low[0]..=high[0] {
                    let bucket = &mut self.buckets[row + x];
                    if let Some(index) = bucket.iter().position(|&other| other == id) {
                        bucket.swap_remove(index);
                    }
                }
            }
        }
    }

    fn update(&mut self, id: usize, bound: QemBounds) {
        let (low, high) = self.range(bound);
        let (old_low, old_high) = self.ranges[id];
        if (low, high) == (old_low, old_high) {
            return;
        }
        for z in old_low[2]..=old_high[2] {
            for y in old_low[1]..=old_high[1] {
                let row = (z * self.size[1] + y) * self.size[0];
                for x in old_low[0]..=old_high[0] {
                    if x < low[0]
                        || x > high[0]
                        || y < low[1]
                        || y > high[1]
                        || z < low[2]
                        || z > high[2]
                    {
                        let bucket = &mut self.buckets[row + x];
                        if let Some(index) = bucket.iter().position(|&other| other == id) {
                            bucket.swap_remove(index);
                        }
                    }
                }
            }
        }
        for z in low[2]..=high[2] {
            for y in low[1]..=high[1] {
                let row = (z * self.size[1] + y) * self.size[0];
                for x in low[0]..=high[0] {
                    if x < old_low[0]
                        || x > old_high[0]
                        || y < old_low[1]
                        || y > old_high[1]
                        || z < old_low[2]
                        || z > old_high[2]
                    {
                        self.buckets[row + x].push(id);
                    }
                }
            }
        }
        self.ranges[id] = (low, high);
    }

    fn query(
        &self,
        bound: QemBounds,
        omitted: &[bool],
        seen: &mut [u32],
        stamp: &mut u32,
        out: &mut Vec<usize>,
    ) {
        *stamp = stamp.wrapping_add(1);
        if *stamp == 0 {
            seen.fill(0);
            *stamp = 1;
        }
        out.clear();
        let (low, high) = self.range(bound);
        for z in low[2]..=high[2] {
            for y in low[1]..=high[1] {
                let row = (z * self.size[1] + y) * self.size[0];
                for x in low[0]..=high[0] {
                    for &id in &self.buckets[row + x] {
                        if seen[id] != *stamp {
                            seen[id] = *stamp;
                            if !omitted[id] {
                                out.push(id);
                            }
                        }
                    }
                }
            }
        }
    }
}
