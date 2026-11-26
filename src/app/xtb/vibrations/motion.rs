//! Geometric descriptions of normal modes, from internal-coordinate motion.
#![allow(clippy::indexing_slicing)] // The bond graph and mode chunks share the checked frame size.

use reshiki::qcdata::{BondOrder, Frame};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

const STEP: f64 = 1e-4;
const MIN_LENGTH: f64 = 1e-10;
const MAX_BEND: f64 = 175. * PI / 180.;

/// One description per complete Cartesian mode (3N values, in frame order).
/// An empty bond-order list uses distance-based connectivity. Each description
/// names the coordinate with the largest |B · u| / |B|, after normalizing u.
pub(super) fn describe(frame: &Frame, bond_orders: &[BondOrder], modes: &[f64]) -> Vec<String> {
    let width = frame.numbers.len() * 3;
    if width == 0 {
        return Vec::new();
    }
    if frame.positions_angstrom.len() != frame.numbers.len()
        || frame
            .positions_angstrom
            .iter()
            .flatten()
            .any(|x| !x.is_finite())
    {
        return vec![String::new(); modes.len() / width];
    }
    let coordinates = coordinates(frame, bond_orders);
    modes
        .chunks_exact(width)
        .map(|mode| {
            // Scaling first also handles very small or large imported vectors.
            let scale = mode.iter().map(|x| x.abs()).fold(0., f64::max);
            if scale == 0. || mode.iter().any(|x| !x.is_finite()) {
                return String::new();
            }
            let norm = mode.iter().map(|x| (x / scale).powi(2)).sum::<f64>().sqrt();
            let unit: Vec<_> = mode.iter().map(|x| x / scale / norm).collect();
            let mut label = "";
            let mut largest = 0.;
            for coordinate in &coordinates {
                let score = coordinate
                    .row
                    .iter()
                    .map(|(atom, row)| {
                        row.iter()
                            .enumerate()
                            .map(|(axis, b)| b * unit[3 * atom + axis])
                            .sum::<f64>()
                    })
                    .sum::<f64>()
                    .abs();
                if score > largest {
                    largest = score;
                    label = &coordinate.label;
                }
            }
            label.to_owned()
        })
        .collect()
}

struct Coordinate {
    label: String,
    /// Sparse Wilson B row, normalized once before comparing modes.
    row: Vec<(usize, [f64; 3])>,
}

impl Coordinate {
    fn new(label: String, mut row: Vec<(usize, [f64; 3])>) -> Option<Self> {
        let norm = row.iter().map(|(_, b)| dot(*b, *b)).sum::<f64>().sqrt();
        if !norm.is_finite() || norm <= MIN_LENGTH {
            return None;
        }
        for (_, b) in &mut row {
            *b = b.map(|x| x / norm);
        }
        Some(Self { label, row })
    }
}

fn coordinates(frame: &Frame, orders: &[BondOrder]) -> Vec<Coordinate> {
    let numbers = &frame.numbers;
    let positions = &frame.positions_angstrom;
    let edges = edges(frame, orders);
    let mut neighbors = vec![Vec::new(); numbers.len()];
    for &(a, b) in edges.keys() {
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    let mut coordinates = Vec::new();
    for (&(a, b), &order) in &edges {
        let Some(direction) = unit(sub(positions[a], positions[b])) else {
            continue;
        };
        let ring =
            numbers[a] != 1 && numbers[b] != 1 && connected(&neighbors, a, b, None, Some((a, b)));
        let label = if ring {
            "ring stretch".into()
        } else {
            let (a, b) = pair(numbers[a], numbers[b]);
            let bond = if order >= 2.5 {
                "≡"
            } else if order >= 1.5 {
                "="
            } else {
                "–"
            };
            format!("{a}{bond}{b} stretch")
        };
        if let Some(coordinate) =
            Coordinate::new(label, vec![(a, direction), (b, direction.map(|x| -x))])
        {
            coordinates.push(coordinate);
        }
    }
    for (b, ends) in neighbors.iter().enumerate() {
        for (i, &a) in ends.iter().enumerate() {
            for &c in &ends[i + 1..] {
                let points = [positions[a], positions[b], positions[c]];
                if !usable_bend(&points) {
                    continue;
                }
                // A path avoiding the centre puts all three on the same cycle,
                // including fused rings without combining two spiro rings.
                let label = if connected(&neighbors, a, c, Some(b), None) {
                    "ring deformation".into()
                } else {
                    let (a, c) = pair(numbers[a], numbers[c]);
                    format!("{a}–{}–{c} bend", super::state::symbol(numbers[b]))
                };
                if let Some(coordinate) = finite_difference([a, b, c], points, angle, false)
                    .and_then(|row| Coordinate::new(label, row))
                {
                    coordinates.push(coordinate);
                }
            }
        }
    }
    for &(b, c) in edges.keys() {
        for &a in &neighbors[b] {
            if a == c {
                continue;
            }
            for &d in &neighbors[c] {
                if d == b || d == a {
                    continue;
                }
                let points = [positions[a], positions[b], positions[c], positions[d]];
                if !usable_bend(&[points[0], points[1], points[2]])
                    || !usable_bend(&[points[1], points[2], points[3]])
                {
                    continue;
                }
                let (b_name, c_name) = pair(numbers[b], numbers[c]);
                let label = format!("{b_name}–{c_name} torsion");
                if let Some(coordinate) = finite_difference([a, b, c, d], points, torsion, true)
                    .and_then(|row| Coordinate::new(label, row))
                {
                    coordinates.push(coordinate);
                }
            }
        }
    }
    coordinates
}

fn edges(frame: &Frame, orders: &[BondOrder]) -> BTreeMap<(usize, usize), f64> {
    let mut edges = BTreeMap::new();
    let count = frame.numbers.len();
    if !orders.is_empty() {
        for order in orders {
            let (a, b) = (order.a as usize, order.b as usize);
            if a != b && a < count && b < count && order.order.is_finite() && order.order >= 0.5 {
                let pair = (a.min(b), a.max(b));
                let value = edges.entry(pair).or_insert(0_f64);
                *value = value.max(order.order);
            }
        }
    } else {
        for a in 0..count {
            for b in a + 1..count {
                let distance = length(sub(
                    frame.positions_angstrom[a],
                    frame.positions_angstrom[b],
                ));
                let cutoff = 1.25 * (radius(frame.numbers[a]) + radius(frame.numbers[b]));
                if distance > MIN_LENGTH && distance <= cutoff {
                    edges.insert((a, b), 1.);
                }
            }
        }
    }
    edges
}

/// Connectivity with an atom or bond removed, for cycle membership.
fn connected(
    neighbors: &[Vec<usize>],
    start: usize,
    end: usize,
    omit_atom: Option<usize>,
    omit_bond: Option<(usize, usize)>,
) -> bool {
    let mut visited = vec![false; neighbors.len()];
    visited[start] = true;
    let mut pending = vec![start];
    while let Some(a) = pending.pop() {
        for &b in &neighbors[a] {
            if Some(b) == omit_atom
                || omit_bond.is_some_and(|(i, j)| (a == i && b == j) || (a == j && b == i))
                || visited[b]
            {
                continue;
            }
            if b == end {
                return true;
            }
            visited[b] = true;
            pending.push(b);
        }
    }
    false
}

fn pair(a: u8, b: u8) -> (&'static str, &'static str) {
    let a = super::state::symbol(a);
    let b = super::state::symbol(b);
    let key = |symbol: &'static str| {
        (
            match symbol {
                "C" => 0,
                "H" => 2,
                _ => 1,
            },
            symbol,
        )
    };
    if key(a) <= key(b) { (a, b) } else { (b, a) }
}

/// Covalent radii in Å for common organic elements. The complete optimizer
/// table is private to its crate; other elements use a conservative 1.5 Å.
fn radius(number: u8) -> f64 {
    match number {
        1 => 0.31,
        5 => 0.84,
        6 => 0.76,
        7 => 0.71,
        8 => 0.66,
        9 => 0.57,
        14 => 1.11,
        15 => 1.07,
        16 => 1.05,
        17 => 1.02,
        35 => 1.20,
        53 => 1.39,
        _ => 1.5,
    }
}

fn finite_difference<const N: usize>(
    atoms: [usize; N],
    positions: [[f64; 3]; N],
    value: fn(&[[f64; 3]; N]) -> Option<f64>,
    periodic: bool,
) -> Option<Vec<(usize, [f64; 3])>> {
    let mut row = Vec::with_capacity(N);
    for (atom, &index) in atoms.iter().enumerate() {
        let mut derivative = [0.; 3];
        for (axis, b) in derivative.iter_mut().enumerate() {
            let mut plus = positions;
            let mut minus = positions;
            plus[atom][axis] += STEP;
            minus[atom][axis] -= STEP;
            let delta = value(&plus)? - value(&minus)?;
            let delta = if periodic { wrapped(delta) } else { delta };
            *b = delta / (2. * STEP);
        }
        row.push((index, derivative));
    }
    Some(row)
}

fn wrapped(angle: f64) -> f64 {
    (angle + PI).rem_euclid(TAU) - PI
}

fn usable_bend(points: &[[f64; 3]; 3]) -> bool {
    angle(points).is_some_and(|angle| angle <= MAX_BEND && angle > MIN_LENGTH)
}

fn angle(points: &[[f64; 3]; 3]) -> Option<f64> {
    let a = unit(sub(points[0], points[1]))?;
    let b = unit(sub(points[2], points[1]))?;
    Some(dot(a, b).clamp(-1., 1.).acos())
}

fn torsion(points: &[[f64; 3]; 4]) -> Option<f64> {
    let axis = unit(sub(points[2], points[1]))?;
    let left = sub(points[0], points[1]);
    let right = sub(points[3], points[2]);
    let left = unit(sub(left, axis.map(|x| x * dot(left, axis))))?;
    let right = unit(sub(right, axis.map(|x| x * dot(right, axis))))?;
    Some(dot(cross(axis, left), right).atan2(dot(left, right)))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let length = length(a);
    (length.is_finite() && length > MIN_LENGTH).then(|| a.map(|x| x / length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(numbers: &[u8], positions: &[[f64; 3]]) -> Frame {
        Frame {
            numbers: numbers.to_vec(),
            positions_angstrom: positions.to_vec(),
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    fn water() -> Frame {
        frame(
            &[8, 1, 1],
            &[[0., 0., 0.], [0.9572, 0., 0.], [-0.2399872, 0.9266272, 0.]],
        )
    }

    fn order(a: u32, b: u32, order: f64) -> BondOrder {
        BondOrder { a, b, order }
    }

    #[test]
    fn water_symmetric_stretch_and_bend() {
        let frame = water();
        let a = unit(frame.positions_angstrom[1]).unwrap();
        let b = unit(frame.positions_angstrom[2]).unwrap();
        let stretch: Vec<_> = [[0.; 3], a, b].into_iter().flatten().collect();
        let bend: Vec<_> = [[0.; 3], [0., 1., 0.], [b[1], -b[0], 0.]]
            .into_iter()
            .flatten()
            .collect();
        let modes = [stretch, bend].concat();
        assert_eq!(
            describe(&frame, &[order(0, 1, 0.9), order(0, 2, 0.9)], &modes),
            ["O–H stretch", "H–O–H bend"]
        );
    }

    #[test]
    fn formaldehyde_carbonyl_stretch() {
        let frame = frame(
            &[6, 8, 1, 1],
            &[
                [0., 0., 0.],
                [1.21, 0., 0.],
                [-0.58, 0.94, 0.],
                [-0.58, -0.94, 0.],
            ],
        );
        // Move the CH2 group as a unit opposite the oxygen: pure C=O motion.
        let mode = [-1., 0., 0., 1., 0., 0., -1., 0., 0., -1., 0., 0.];
        assert_eq!(
            describe(
                &frame,
                &[order(0, 1, 2.), order(0, 2, 0.9), order(0, 3, 0.9)],
                &mode
            ),
            ["C=O stretch"]
        );
    }

    #[test]
    fn benzene_fractional_orders_still_identify_ring_stretch() {
        let carbons: Vec<_> = (0..6)
            .map(|i| {
                let angle = TAU * f64::from(i) / 6.;
                [1.4 * angle.cos(), 1.4 * angle.sin(), 0.]
            })
            .collect();
        let hydrogens: Vec<_> = carbons.iter().map(|p| p.map(|x| x * 2.48 / 1.4)).collect();
        let positions = [carbons.clone(), hydrogens].concat();
        let frame = frame(&[6, 6, 6, 6, 6, 6, 1, 1, 1, 1, 1, 1], &positions);
        let orders: Vec<_> = (0..6)
            .flat_map(|i| [order(i, (i + 1) % 6, 1.4), order(i, i + 6, 0.9)])
            .collect();
        // Radial breathing moves each attached H with its C, preserving C–H.
        let mode: Vec<_> = [carbons.clone(), carbons]
            .concat()
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(describe(&frame, &orders, &mode), ["ring stretch"]);
    }

    #[test]
    fn missing_bond_orders_use_distances() {
        let frame = water();
        assert_eq!(
            edges(&frame, &[]).keys().copied().collect::<Vec<_>>(),
            [(0, 1), (0, 2)]
        );
        let a = unit(frame.positions_angstrom[1]).unwrap();
        let b = unit(frame.positions_angstrom[2]).unwrap();
        let mode: Vec<_> = [[0.; 3], a, b].into_iter().flatten().collect();
        assert_eq!(describe(&frame, &[], &mode), ["O–H stretch"]);
        // A present sparse list is authoritative, even if no order reaches 0.5.
        assert_eq!(describe(&frame, &[order(0, 1, 0.4)], &mode), [""]);
    }

    #[test]
    fn labels_are_scale_and_sign_invariant_and_unknown_modes_are_empty() {
        let frame = frame(&[8, 6], &[[0., 0., 0.], [1.2, 0., 0.]]);
        let mode = [1., 0., 0., -1., 0., 0.];
        for scale in [1e-200, -7., 1e200] {
            assert_eq!(
                describe(&frame, &[order(0, 1, 2.)], &mode.map(|x| x * scale)),
                ["C=O stretch"]
            );
        }
        assert_eq!(describe(&frame, &[], &[0.; 6]), [""]);
        assert_eq!(describe(&frame, &[], &[1., 2., 3., 1., 2., 3.]), [""]);
        assert_eq!(describe(&frame, &[], &[f64::NAN; 6]), [""]);
    }

    #[test]
    fn wrapped_torsion_derivative_at_pi_is_finite() {
        let points = [[0., 1., 0.], [0., 0., 0.], [1., 0., 0.], [1., -1., 0.]];
        assert!((torsion(&points).unwrap().abs() - PI).abs() < 1e-12);
        let row = finite_difference([0, 1, 2, 3], points, torsion, true).unwrap();
        let norm = row.iter().map(|(_, b)| dot(*b, *b)).sum::<f64>().sqrt();
        assert!((norm - 2.).abs() < 1e-6, "norm = {norm}");
        let frame = frame(&[1, 6, 8, 1], &points);
        let mode: Vec<_> = row.into_iter().flat_map(|(_, b)| b).collect();
        assert_eq!(
            describe(
                &frame,
                &[order(0, 1, 1.), order(1, 2, 1.), order(2, 3, 1.)],
                &mode
            ),
            ["C–O torsion"]
        );
    }

    #[test]
    fn linear_bends_and_torsions_are_skipped() {
        let frame = frame(
            &[1, 6, 6, 1],
            &[[0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [3., 0., 0.]],
        );
        let orders = [order(0, 1, 1.), order(1, 2, 1.), order(2, 3, 1.)];
        let coordinates = coordinates(&frame, &orders);
        assert_eq!(coordinates.len(), 3);
        assert!(coordinates.iter().all(|c| c.label.ends_with("stretch")));
    }
}
