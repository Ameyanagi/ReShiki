use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use glam::{DMat3, DVec3};

const TOL: f64 = 1e-3; // ångströms

// The second-moment tensor and the inertia tensor have the same eigenvectors.
// Jacobi rotations diagonalize the former while accumulating its principal axes.
fn principal_axes(positions: &[DVec3], masses: &[f64]) -> [DVec3; 3] {
    let mut tensor = DMat3::ZERO;
    for (&p, &mass) in positions.iter().zip(masses) {
        tensor += DMat3::from_cols(p * p.x, p * p.y, p * p.z) * mass;
    }
    let trace = tensor.x_axis.x + tensor.y_axis.y + tensor.z_axis.z;
    let mut axes = DMat3::IDENTITY;
    for _ in 0..12 {
        let (p, q, axis) = [(1, 2, DVec3::X), (2, 0, DVec3::Y), (0, 1, DVec3::Z)]
            .into_iter()
            .max_by(|&(i, j, _), &(k, l, _)| {
                tensor.col(i)[j].abs().total_cmp(&tensor.col(k)[l].abs())
            })
            .unwrap();
        let off = tensor.col(p)[q];
        if off.abs() <= f64::EPSILON * trace {
            break;
        }
        let angle = 0.5 * (2.0 * off).atan2(tensor.col(p)[p] - tensor.col(q)[q]);
        let turn = DMat3::from_axis_angle(axis, angle);
        tensor = turn.transpose() * tensor * turn;
        axes *= turn;
    }
    [axes.x_axis, axes.y_axis, axes.z_axis]
}

// Usually each atom has just one image. If tolerance balls overlap, find an
// augmenting path so a valid permutation does not depend on atom ordering.
fn assign_image<I: Iterator<Item = usize>>(
    atom: usize,
    images: &impl Fn(usize) -> I,
    owner: &mut [Option<usize>],
    visited: &mut [usize],
    attempt: usize,
) -> bool {
    for j in images(atom) {
        if visited[j] == attempt {
            continue;
        }
        visited[j] = attempt;
        if owner[j].is_none_or(|prev| assign_image(prev, images, owner, visited, attempt)) {
            owner[j] = Some(atom);
            return true;
        }
    }
    false
}

pub(super) fn count(labels: &[u32], positions: &[DVec3], masses: &[f64]) -> u32 {
    let radii: Vec<_> = positions.iter().map(|p| p.length()).collect();
    // Rotations preserve radius and label. Sort each possible image set by x
    // so a trial rotation needs to search only a 2*tolerance-wide slice.
    let mut by_x: Vec<_> = (0..positions.len()).collect();
    by_x.sort_unstable_by(|&i, &j| positions[i].x.total_cmp(&positions[j].x));
    let shells: Vec<Vec<usize>> = (0..positions.len())
        .map(|i| {
            by_x.iter()
                .copied()
                .filter(|&j| labels[i] == labels[j] && (radii[i] - radii[j]).abs() <= TOL)
                .collect()
        })
        .collect();
    let images = |i: usize, dest: DVec3| {
        let shell = &shells[i];
        let start = shell.partition_point(|&j| positions[j].x < dest.x - TOL);
        shell[start..]
            .iter()
            .copied()
            .take_while(move |&j| positions[j].x <= dest.x + TOL)
            .filter(move |&j| positions[j].distance_squared(dest) <= TOL * TOL)
    };
    let mut owner = vec![None; positions.len()];
    let mut visited = vec![usize::MAX; positions.len()];
    let mut permutes_atoms = |rotation: DMat3| {
        owner.fill(None);
        visited.fill(usize::MAX);
        let targets = |i: usize| images(i, rotation * positions[i]);
        (0..positions.len()).all(|i| assign_image(i, &targets, &mut owner, &mut visited, i))
    };

    let a = (0..positions.len())
        .max_by(|&i, &j| radii[i].total_cmp(&radii[j]))
        .unwrap();
    // The images of two non-collinear vectors uniquely identify a proper
    // rotation (their cross product fixes the third direction). This also
    // coalesces matrices estimated from different atoms in a noisy geometry.
    let perpendicular = |i: usize| positions[a].cross(positions[i]).length_squared();
    let b = (0..positions.len())
        .max_by(|&i, &j| perpendicular(i).total_cmp(&perpendicular(j)))
        .unwrap();
    let axes = principal_axes(positions, masses);
    let radial_axes = || axes.into_iter().chain(positions.iter().copied());
    // A half-turn fixes the midpoint of an atom and its image. If both
    // reference atoms go to their antipodes, the axis is normal to both.
    let midpoints = [a, b]
        .into_iter()
        .flat_map(|i| shells[i].iter().map(move |&j| positions[i] + positions[j]));
    let half_turns = radial_axes()
        .chain(midpoints)
        .chain([positions[a].cross(positions[b])]);
    let line = axes
        .into_iter()
        .max_by(|u, v| {
            u.dot(positions[a])
                .abs()
                .total_cmp(&v.dot(positions[a]).abs())
        })
        .unwrap();
    if positions
        .iter()
        .all(|p| p.cross(line).length_squared() <= TOL * TOL)
    {
        // Only an end-for-end exchange contributes for a linear molecule.
        // Test actual perpendicular half-turns on the original, possibly noisy sites.
        let reversible = half_turns
            .filter_map(|v| line.cross(v).cross(line).try_normalize())
            .any(|axis| permutes_atoms(DMat3::from_axis_angle(axis, PI)));
        return 1 + u32::from(reversible);
    }
    let mut rotations = HashMap::from([((a, b), DMat3::IDENTITY)]);
    let mut test_rotation = |rotation: DMat3| {
        let image = |i: usize| {
            let dest = rotation * positions[i];
            images(i, dest).min_by(|&j, &k| {
                positions[j]
                    .distance_squared(dest)
                    .total_cmp(&positions[k].distance_squared(dest))
            })
        };
        let Some(key) = image(a).zip(image(b)) else {
            return false;
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = rotations.entry(key) {
            if !permutes_atoms(rotation) {
                return false;
            }
            entry.insert(rotation);
        }
        true
    };
    let mut test_axis = |direction: DVec3, order: u32| {
        let Some(axis) = direction.try_normalize() else {
            return;
        };
        let generator = DMat3::from_axis_angle(axis, TAU / f64::from(order));
        if test_rotation(generator) {
            let mut power = generator;
            for _ in 2..order {
                power *= generator;
                test_rotation(power);
            }
        }
    };

    for axis in half_turns {
        test_axis(axis, 2);
    }
    for axis in radial_axes() {
        for order in 3..=8 {
            test_axis(axis, order);
        }
    }

    // Under Cn (n >= 3) the outermost atom is fixed or traces a regular
    // polygon. Its two neighbours in that orbit are equidistant from it.
    // Only triples through this atom are needed, even for a spherical top.
    let chords = [3, 4, 5, 6, 7, 8].map(|n| (n, 2.0 * (PI / f64::from(n)).cos()));
    let shell = &shells[a];
    for (i, &j) in shell.iter().enumerate() {
        let edge = positions[j] - positions[a];
        let side = edge.length();
        for &k in &shell[..i] {
            let other = positions[k] - positions[a];
            if (side - other.length()).abs() <= 2.0 * TOL {
                let normal = edge.cross(other);
                // The chord between the two neighbours spans two polygon sides.
                let chord = positions[j].distance(positions[k]);
                for (order, ratio) in chords {
                    if (chord - ratio * side).abs() <= 4.0 * TOL {
                        test_axis(normal, order);
                    }
                }
            }
        }
    }
    rotations.len() as u32
}
