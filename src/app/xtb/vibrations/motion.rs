//! Geometric descriptions of normal modes, from internal-coordinate motion.
#![allow(clippy::indexing_slicing)] // The bond graph and mode chunks share the checked frame size.

use reshiki::qcdata::{BondOrder, Frame};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

const STEP: f64 = 1e-4;
const MIN_LENGTH: f64 = 1e-10;
const MAX_BEND: f64 = 175. * PI / 180.;
// Fits the stretches and bends of a 200-atom alkane across its full mode set.
const MAX_COORDINATE_EVALUATIONS: usize = 500_000;
// Finite differences and ring checks have an up-front cost even for a short mode list.
const MIN_BUDGETED_MODES: usize = 128;
const BEND_FORCE: f64 = 0.7;
const OUT_OF_PLANE_FORCE: f64 = 0.3;
const TORSION_FORCE: f64 = 0.1;

/// One description per complete Cartesian mode (3N values, in frame order).
/// An empty bond-order list uses distance-based connectivity with a conservative
/// terminal-carbonyl estimate. Each description names the coordinate with the
/// largest E = k Σ(Bᵢ · u)² for unit Cartesian u, summing the two transverse
/// components for a linear bend.
///
/// Cartesian positions and stretch coordinates are in Å; angular coordinates
/// are in radians, so their unnormalized Wilson rows are in rad/Å. The heuristic
/// force constants are in mdyn/Å for stretches and mdyn Å/rad² for angles,
/// making every score a curvature in mdyn/Å along the unit Cartesian direction.
/// These local scores describe motion; they are not a potential-energy analysis
/// of the full molecular force field.
pub(super) fn describe(frame: &Frame, bond_orders: &[BondOrder], modes: &[f64]) -> Vec<String> {
    let width = frame.numbers.len() * 3;
    if width == 0 {
        return Vec::new();
    }
    let mode_count = modes.len() / width;
    if mode_count == 0 {
        return Vec::new();
    }
    if frame.positions_angstrom.len() != frame.numbers.len()
        || frame
            .positions_angstrom
            .iter()
            .flatten()
            .any(|x| !x.is_finite())
    {
        return vec![String::new(); mode_count];
    }
    let coordinates = coordinates(frame, bond_orders, mode_count);
    if coordinates.is_empty() {
        return vec![String::new(); mode_count];
    }
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
                let score = coordinate.score(&unit);
                if score > largest {
                    largest = score;
                    label = &coordinate.label;
                }
            }
            label.to_owned()
        })
        .collect()
}

type WilsonRow = Vec<(usize, [f64; 3])>;

struct Coordinate {
    label: String,
    /// Sparse Wilson B row, retaining dq/dx rather than normalizing its length.
    row: WilsonRow,
    transverse: Option<WilsonRow>,
    force: f64,
}

impl Coordinate {
    fn new(
        label: String,
        row: WilsonRow,
        transverse: Option<WilsonRow>,
        force: f64,
    ) -> Option<Self> {
        let norm = row
            .iter()
            .chain(transverse.iter().flatten())
            .map(|(_, b)| dot(*b, *b))
            .sum::<f64>()
            .sqrt();
        if !norm.is_finite() || norm <= MIN_LENGTH {
            return None;
        }
        Some(Self {
            label,
            row,
            transverse,
            force,
        })
    }

    fn score(&self, unit_mode: &[f64]) -> f64 {
        let squared_projection = |row: &WilsonRow| {
            row.iter()
                .map(|(atom, [x, y, z])| {
                    let offset = 3 * atom;
                    x * unit_mode[offset] + y * unit_mode[offset + 1] + z * unit_mode[offset + 2]
                })
                .sum::<f64>()
                .powi(2)
        };
        self.force
            * (squared_projection(&self.row)
                + self.transverse.as_ref().map_or(0., squared_projection))
    }
}

fn coordinates(frame: &Frame, orders: &[BondOrder], mode_count: usize) -> Vec<Coordinate> {
    let numbers = &frame.numbers;
    let positions = &frame.positions_angstrom;
    let mut edges = edges(frame, orders);
    let mut neighbors = vec![Vec::new(); numbers.len()];
    for &(a, b) in edges.keys() {
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    // Count candidates before geometry or ring checks. Torsion counts include
    // triangle closures, which are filtered out during generation.
    let base = edges
        .len()
        .saturating_add(neighbors.iter().filter(|ends| ends.len() == 3).count());
    let with_bends = neighbors
        .iter()
        .map(|ends| ends.len().saturating_mul(ends.len().saturating_sub(1)) / 2)
        .fold(base, usize::saturating_add);
    let all = edges
        .keys()
        .map(|&(b, c)| (neighbors[b].len() - 1).saturating_mul(neighbors[c].len() - 1))
        .fold(with_bends, usize::saturating_add);
    let limit = MAX_COORDINATE_EVALUATIONS / mode_count.max(MIN_BUDGETED_MODES);
    if base > limit {
        return Vec::new();
    }

    if orders.is_empty() {
        infer_carbonyls(frame, &neighbors, &mut edges);
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
        // X–H stretching is stiffer than a heavy-atom single bond. For other
        // stretches the (possibly fractional) bond order scales the stiffness.
        let force = if numbers[a] == 1 || numbers[b] == 1 {
            5.
        } else {
            4.5 * order.max(1.)
        };
        if let Some(coordinate) = Coordinate::new(
            label,
            vec![(a, direction), (b, direction.map(|x| -x))],
            None,
            force,
        ) {
            coordinates.push(coordinate);
        }
    }
    for (b, ends) in neighbors.iter().enumerate() {
        if with_bends <= limit {
            for (i, &a) in ends.iter().enumerate() {
                for &c in &ends[i + 1..] {
                    let points = [positions[a], positions[b], positions[c]];
                    let Some(bend) = angle(&points).filter(|&bend| bend > MIN_LENGTH) else {
                        continue;
                    };
                    let linear = bend > MAX_BEND;
                    // A path avoiding the centre puts all three on the same cycle,
                    // including fused rings without combining two spiro rings.
                    let label = if !linear && connected(&neighbors, a, c, Some(b), None) {
                        "ring deformation".into()
                    } else {
                        let (a, c) = pair(numbers[a], numbers[c]);
                        format!("{a}–{}–{c} bend", super::state::symbol(numbers[b]))
                    };
                    let coordinate = if linear {
                        // The two transverse components remain regular at 180°.
                        linear_bends([a, b, c], &points).and_then(|[row, transverse]| {
                            Coordinate::new(label, row, Some(transverse), BEND_FORCE)
                        })
                    } else {
                        finite_difference([a, b, c], points, angle, false)
                            .and_then(|row| Coordinate::new(label, row, None, BEND_FORCE))
                    };
                    if let Some(coordinate) = coordinate {
                        coordinates.push(coordinate);
                    }
                }
            }
        }
        if ends.len() == 3 {
            // One improper angle per trigonal centre. A terminal H makes the
            // familiar X–H wag the named coordinate when one is available.
            let mut ends = ends.clone();
            ends.sort_by_key(|&a| (!(numbers[a] == 1 && neighbors[a].len() == 1), a));
            let [a, c, d] = [ends[0], ends[1], ends[2]];
            let center = super::state::symbol(numbers[b]);
            let label = if numbers[a] == 1 && neighbors[a].len() == 1 {
                format!("{center}–H out-of-plane bend")
            } else {
                format!("{center} out-of-plane bend")
            };
            let points = [positions[a], positions[b], positions[c], positions[d]];
            if let Some(coordinate) = finite_difference([a, b, c, d], points, out_of_plane, false)
                .and_then(|row| Coordinate::new(label, row, None, OUT_OF_PLANE_FORCE))
            {
                coordinates.push(coordinate);
            }
        }
    }
    if all > limit {
        return coordinates;
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
                    .and_then(|row| Coordinate::new(label, row, None, TORSION_FORCE))
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

/// Many imported frequency files have no bond orders. Recognize an unambiguous
/// short terminal C=O without running a general valence/bond-order search: one
/// such O at a three-coordinate carbon, or two at a two-coordinate carbon (CO₂).
/// The 1.30 Å cutoff excludes ordinary C–O single bonds; terminal oxygen and
/// coordination checks also exclude alcohols, ethers and saturated carbon.
/// Two short terminal O atoms at a three-coordinate carbon are ambiguous (as
/// in a carboxylate), so neither is assigned an arbitrary localized double bond.
fn infer_carbonyls(
    frame: &Frame,
    neighbors: &[Vec<usize>],
    edges: &mut BTreeMap<(usize, usize), f64>,
) {
    for (carbon, ends) in neighbors.iter().enumerate() {
        if frame.numbers[carbon] != 6 || !matches!(ends.len(), 2 | 3) {
            continue;
        }
        let terminal_oxygens: Vec<_> = ends
            .iter()
            .copied()
            .filter(|&oxygen| {
                frame.numbers[oxygen] == 8
                    && neighbors[oxygen].len() == 1
                    && length(sub(
                        frame.positions_angstrom[carbon],
                        frame.positions_angstrom[oxygen],
                    )) <= 1.30
            })
            .collect();
        if (ends.len() == 3 && terminal_oxygens.len() == 1)
            || (ends.len() == 2 && terminal_oxygens.len() == 2)
        {
            for oxygen in terminal_oxygens {
                edges.insert((carbon.min(oxygen), carbon.max(oxygen)), 2.);
            }
        }
    }
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
) -> Option<WilsonRow> {
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

/// Centre motion perpendicular to the endpoint axis, relative to interpolated
/// endpoint motion. Scaling by 1/L_left + 1/L_right, with L the projected bond
/// lengths, gives two orthogonal rad/Å rows with no axial response.
fn linear_bends(atoms: [usize; 3], points: &[[f64; 3]; 3]) -> Option<[WilsonRow; 2]> {
    let axis = unit(sub(points[2], points[0]))?;
    let left_length = dot(sub(points[1], points[0]), axis);
    let right_length = dot(sub(points[2], points[1]), axis);
    if left_length <= MIN_LENGTH || right_length <= MIN_LENGTH {
        return None;
    }
    // Cross with the least parallel Cartesian axis to avoid a small divisor.
    let reference = if axis[0].abs() <= axis[1].abs() && axis[0].abs() <= axis[2].abs() {
        [1., 0., 0.]
    } else if axis[1].abs() <= axis[2].abs() {
        [0., 1., 0.]
    } else {
        [0., 0., 1.]
    };
    let first = unit(cross(axis, reference))?;
    Some([first, cross(axis, first)].map(|transverse| {
        let left = transverse.map(|x| -x / left_length);
        let right = transverse.map(|x| -x / right_length);
        let center = transverse.map(|x| x * (left_length.recip() + right_length.recip()));
        vec![(atoms[0], left), (atoms[1], center), (atoms[2], right)]
    }))
}

/// Signed angle between the first bond and the plane of the other two bonds.
fn out_of_plane(points: &[[f64; 3]; 4]) -> Option<f64> {
    let bond = unit(sub(points[0], points[1]))?;
    let normal = unit(cross(sub(points[2], points[1]), sub(points[3], points[1])))?;
    Some(dot(bond, normal).clamp(-1., 1.).asin())
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
    fn imported_formaldehyde_and_carbon_dioxide_modes() {
        let file = reshiki::qcread::read(
            "CH2O_CO2.log",
            include_bytes!("../../../../tests/fixtures/qc/pymatgen/CH2O_CO2.log"),
        )
        .unwrap();
        let cases = [
            (
                0,
                vec![
                    "C–H out-of-plane bend",
                    "O–C–H bend",
                    "H–C–H bend",
                    "C=O stretch",
                    "C–H stretch",
                    "C–H stretch",
                ],
            ),
            (
                2,
                vec!["O–C–O bend", "O–C–O bend", "C=O stretch", "C=O stretch"],
            ),
        ];
        for (job_index, expected) in cases {
            let job = &file.jobs[job_index];
            let frame = &job.steps[0].frame;
            let result = job.result.as_ref().unwrap();
            let vibrations = result.vibrations.as_ref().unwrap();
            assert!(result.bond_orders.is_empty());
            assert_eq!(
                describe(frame, &result.bond_orders, &vibrations.modes),
                expected
            );
        }
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
    fn carbonyl_fallback_preserves_single_bonds_and_reported_orders() {
        let carbonyl = |frame: &Frame, orders: &[BondOrder]| {
            coordinates(frame, orders, 1)
                .iter()
                .any(|c| c.label == "C=O stretch")
        };
        let base = frame(
            &[6, 8, 1, 1],
            &[
                [0.; 3],
                [1.23, 0., 0.],
                [-0.58, 0.94, 0.],
                [-0.58, -0.94, 0.],
            ],
        );
        assert!(carbonyl(&base, &[]));
        assert!(!carbonyl(&base, &[order(0, 1, 1.)]));

        let mut long_bond = base.clone();
        long_bond.positions_angstrom[1][0] = 1.43;
        assert!(!carbonyl(&long_bond, &[]));

        let mut saturated_carbon = base.clone();
        saturated_carbon.numbers.push(1);
        saturated_carbon.positions_angstrom.push([0., 0., 1.05]);
        assert!(!carbonyl(&saturated_carbon, &[]));

        let mut protonated_oxygen = base;
        protonated_oxygen.numbers.push(1);
        protonated_oxygen.positions_angstrom.push([2.15, 0., 0.]);
        assert!(!carbonyl(&protonated_oxygen, &[]));

        let carboxylate = frame(
            &[6, 8, 8, 6],
            &[
                [0.; 3],
                [1.26, 0., 0.],
                [-0.63, 1.091, 0.],
                [-0.75, -1.299, 0.],
            ],
        );
        assert!(!carbonyl(&carboxylate, &[]));
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
    fn linear_bends_have_two_transverse_components_and_skip_torsions() {
        let frame = frame(
            &[1, 6, 6, 1],
            &[[0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [3., 0., 0.]],
        );
        let orders = [order(0, 1, 1.), order(1, 2, 1.), order(2, 3, 1.)];
        let coordinates = coordinates(&frame, &orders, 1);
        assert_eq!(coordinates.len(), 5);
        assert_eq!(
            coordinates
                .iter()
                .filter(|c| c.label.ends_with("bend"))
                .count(),
            2
        );
        assert!(!coordinates.iter().any(|c| c.label.ends_with("torsion")));
        for direction in [[0., 1., 0.], [0., 0., 1.]] {
            let mode: Vec<_> = [[0.; 3], direction, [0.; 3], [0.; 3]]
                .into_iter()
                .flatten()
                .collect();
            assert!(describe(&frame, &orders, &mode)[0].ends_with("bend"));
        }
    }

    #[test]
    fn carbon_dioxide_bend_is_invariant_under_axial_rotation() {
        let frame = frame(&[8, 6, 8], &[[-1.16, 0., 0.], [0.; 3], [1.16, 0., 0.]]);
        let orders = [order(0, 1, 2.), order(1, 2, 2.)];
        let diagonal = std::f64::consts::FRAC_1_SQRT_2;
        // The axial component competes with the bend after a 45° rotation.
        let modes = [
            [[0.; 3], [0.4, 1., 0.], [0.; 3]],
            [[0.; 3], [0.4, diagonal, diagonal], [0.; 3]],
            [[0., -1.16, 0.], [0.; 3], [0., 1.16, 0.]], // Rigid rotation.
        ];
        let modes: Vec<_> = modes.into_iter().flatten().flatten().collect();
        assert_eq!(
            describe(&frame, &orders, &modes),
            ["O–C–O bend", "O–C–O bend", ""]
        );
    }

    #[test]
    fn near_linear_bends_keep_angular_scale_for_different_bond_lengths() {
        let points = [[-2., 0., 0.], [0.; 3], [2., 0., 0.]];
        let rows = linear_bends([0, 1, 2], &points).unwrap();
        assert!(dot(rows[0][0].1, rows[1][0].1).abs() < 1e-12);
        for row in &rows {
            assert!((length(row[0].1) - 0.5).abs() < 1e-12);
            assert!((length(row[1].1) - 1.).abs() < 1e-12);
            assert!((length(row[2].1) - 0.5).abs() < 1e-12);
        }
        let points = [[-0.8, 0., 0.], [0., 0.04, 0.], [1.7, 0., 0.]];
        assert!(angle(&points).unwrap() > MAX_BEND);
        let axis = unit(sub(points[2], points[0])).unwrap();
        let rows = linear_bends([0, 1, 2], &points).unwrap();
        for row in &rows {
            // Every atom's axial displacement is independent of either bend,
            // including unequal bond lengths at a slightly bent geometry.
            assert!(row.iter().all(|(_, b)| dot(*b, axis).abs() < 1e-12));
            assert!((length(row[1].1) - (1. / 0.8 + 1. / 1.7)).abs() < 1e-12);
        }
        let frame = frame(&[8, 6, 8], &points);
        let coordinates = coordinates(&frame, &[order(0, 1, 2.), order(1, 2, 2.)], 1);
        assert_eq!(
            coordinates
                .iter()
                .filter(|c| c.label.ends_with("bend"))
                .count(),
            1
        );
        assert!(coordinates.iter().all(|c| {
            c.row
                .iter()
                .chain(c.transverse.iter().flatten())
                .all(|(_, b)| b.iter().all(|x| x.is_finite()))
        }));
    }

    #[test]
    fn out_of_plane_bend_uses_one_terminal_hydrogen_per_trigonal_center() {
        // The heavy atom precedes the H in atom order: H still names the wag.
        let frame = frame(
            &[6, 8, 6, 1],
            &[[0.; 3], [1., 0., 0.], [-0.5, 1., 0.], [-0.5, -1., 0.]],
        );
        let orders = [order(0, 1, 2.), order(0, 2, 1.), order(0, 3, 1.)];
        let rows = coordinates(&frame, &orders, 1);
        let out_of_plane: Vec<_> = rows
            .iter()
            .filter(|c| c.label.contains("out-of-plane"))
            .collect();
        assert_eq!(out_of_plane.len(), 1);
        assert_eq!(out_of_plane[0].label, "C–H out-of-plane bend");
        assert_eq!(out_of_plane[0].row[0].0, 3);

        let mut frame = frame;
        frame.numbers[3] = 6;
        assert!(
            coordinates(&frame, &orders, 1)
                .iter()
                .any(|c| c.label == "C out-of-plane bend")
        );
    }

    #[test]
    fn dense_bond_graph_budgets_the_full_describe_path() {
        let positions: Vec<_> = (0..200)
            .map(|i| {
                let i = f64::from(i);
                [i.cos(), i.sin(), i]
            })
            .collect();
        let frame = frame(&[6; 200], &positions);
        let orders: Vec<_> = (0..200)
            .flat_map(|a| (a + 1..200).map(move |b| order(a, b, 1.)))
            .collect();
        let mut mode = [0.; 600];
        mode[0] = 1.;
        let modes = mode.repeat(594);

        // Even the stretches alone would require 19,900 × 594 evaluations.
        let start = std::time::Instant::now();
        assert_eq!(describe(&frame, &orders, &modes), vec![String::new(); 594]);
        assert_eq!(describe(&frame, &orders, &mode), [""]);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));

        // A smaller complete graph fits the budget only after dropping both
        // torsions and bends. Its stretches still name all the modes.
        let orders: Vec<_> = orders.into_iter().filter(|order| order.b < 24).collect();
        let rows = coordinates(&frame, &orders, 594);
        assert_eq!(rows.len(), orders.len());
        assert_eq!(describe(&frame, &orders, &modes), vec!["ring stretch"; 594]);

        let orders: Vec<_> = orders.into_iter().filter(|order| order.b < 12).collect();
        let rows = coordinates(&frame, &orders, 594);
        assert!(rows.iter().any(|c| c.label == "ring deformation"));
        assert!(!rows.iter().any(|c| c.label.ends_with("torsion")));
    }
}
