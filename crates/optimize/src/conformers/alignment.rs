//! Heavy-atom RMSD under a proper rotation using Horn's quaternion matrix.
//!
//! Horn, J. Opt. Soc. Am. A 4, 629 (1987) \[Horn1987Quaternion\]. Full references:
//! docs/references/xtb.md.

use std::array::from_fn;

use super::{Automorphisms, Error, Topology, validate_positions};

type Point = [f64; 3];
type Matrix3 = [Point; 3];

/// Minimum RMSD over the topology's automorphisms, in the units of the input.
/// Hydrogen-only molecules use all atoms in input order.
pub fn rmsd(
    topology: &Topology,
    autos: &Automorphisms,
    a: &[Point],
    b: &[Point],
) -> Result<f64, Error> {
    let (reference, mobile) = centered_pair(topology, autos, a, b)?;
    Ok(reference.distance(&mobile, autos))
}

/// Rotate and translate every mobile atom into the reference frame, preserving
/// atom order even when a permutation gives the best heavy-atom RMSD.
pub fn superpose(
    topology: &Topology,
    autos: &Automorphisms,
    reference: &[Point],
    mobile: &[Point],
) -> Result<(Vec<Point>, f64), Error> {
    let (a, b) = centered_pair(topology, autos, reference, mobile)?;
    Ok(a.overlay(&b, autos, mobile))
}

fn centered_pair(
    topology: &Topology,
    autos: &Automorphisms,
    a: &[Point],
    b: &[Point],
) -> Result<(Centered, Centered), Error> {
    validate_positions(topology, a)?;
    validate_positions(topology, b)?;
    if let Some(first) = autos.perms().first()
        && first.len() != topology.heavy_atoms().len()
    {
        return Err(Error::Invalid("incompatible automorphism size".into()));
    }
    Ok((Centered::new(topology, a), Centered::new(topology, b)))
}

/// Fitting coordinates cached across pairwise comparisons.
pub(super) struct Centered {
    points: Vec<Point>,
    anchor: Point,
    mean: Point,
}

impl Centered {
    pub(super) fn new(topology: &Topology, positions: &[Point]) -> Self {
        let heavy = topology.heavy_atoms();
        let mut points = if heavy.is_empty() {
            positions.to_vec()
        } else {
            heavy.iter().map(|&i| positions[i]).collect()
        };
        // Average offsets from a local origin to preserve small displacements under translation.
        let anchor = points[0];
        let mean =
            from_fn(|k| points.iter().map(|p| p[k] - anchor[k]).sum::<f64>() / points.len() as f64);
        for p in &mut points {
            *p = from_fn(|k| (p[k] - anchor[k]) - mean[k]);
        }
        Self {
            points,
            anchor,
            mean,
        }
    }

    pub(super) fn distance(&self, mobile: &Self, autos: &Automorphisms) -> f64 {
        self.best_fit(mobile, autos).1
    }

    pub(super) fn overlay(
        &self,
        mobile: &Self,
        autos: &Automorphisms,
        positions: &[Point],
    ) -> (Vec<Point>, f64) {
        let (rotation, distance) = self.best_fit(mobile, autos);
        let aligned = positions
            .iter()
            .map(|p| {
                let offset = from_fn(|k| (p[k] - mobile.anchor[k]) - mobile.mean[k]);
                let rotated = rotation.map(|row| dot(row, offset));
                from_fn(|k| (rotated[k] + self.mean[k]) + self.anchor[k])
            })
            .collect();
        (aligned, distance)
    }

    fn best_fit(&self, mobile: &Self, autos: &Automorphisms) -> (Matrix3, f64) {
        let fit = |perm: &[u32]| {
            let pairs = self.points.iter().enumerate().map(|(i, &r)| {
                let j = if perm.is_empty() { i } else { perm[i] as usize };
                (mobile.points[j], r)
            });
            let mut covariance = [[0.0; 3]; 3];
            for (m, r) in pairs.clone() {
                for i in 0..3 {
                    for j in 0..3 {
                        covariance[i][j] += m[i] * r[j];
                    }
                }
            }
            let rotation = horn_rotation(covariance);
            let mut loss = 0.0;
            for (m, r) in pairs {
                let delta = from_fn(|k| dot(rotation[k], m) - r[k]);
                loss += dot(delta, delta);
            }
            (rotation, loss)
        };
        let (rotation, loss) = autos
            .perms()
            .iter()
            .map(|p| fit(p))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or_else(|| fit(&[]));
        (rotation, (loss / self.points.len() as f64).sqrt())
    }
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn horn_rotation([[a, b, c], [d, e, f], [g, h, i]]: Matrix3) -> Matrix3 {
    let [w, x, y, z] = largest_eigenvector([
        [a + e + i, f - h, g - c, b - d],
        [f - h, a - e - i, b + d, c + g],
        [g - c, b + d, e - a - i, f + h],
        [b - d, c + g, f + h, i - a - e],
    ]);
    let [xx, yy, zz] = [x, y, z].map(|v| 2.0 * v * v);
    let [xy, xz, yz] = [2.0 * x * y, 2.0 * x * z, 2.0 * y * z];
    let [wx, wy, wz] = [x, y, z].map(|v| 2.0 * w * v);
    [
        [1.0 - (yy + zz), xy - wz, xz + wy],
        [xy + wz, 1.0 - (xx + zz), yz - wx],
        [xz - wy, yz + wx, 1.0 - (xx + yy)],
    ]
}

// For (nearly) linear molecules, any vector in the degenerate leading eigenspace
// gives an optimal residual to rounding accuracy at the 0.125 Å clustering scale.
fn largest_eigenvector(mut a: [[f64; 4]; 4]) -> [f64; 4] {
    let mut vectors: [[f64; 4]; 4] = from_fn(|i| from_fn(|j| f64::from(i == j)));
    let tolerance = 4.0 * f64::EPSILON * a.iter().flatten().map(|x| x.abs()).fold(0.0, f64::max);
    // Cyclic Jacobi sweeps diagonalize a while accumulating the same plane rotations.
    for _ in 0..16 {
        let mut changed = false;
        for (p, q) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
            let off = a[p][q];
            if off.abs() <= tolerance {
                continue;
            }
            changed = true;
            let tau = (a[q][q] - a[p][p]) / (2.0 * off);
            let t = tau.signum() / (tau.abs() + (1.0 + tau * tau).sqrt());
            let c = 1.0 / (1.0 + t * t).sqrt();
            let s = t * c;
            let rotate = |x, y| (c * x - s * y, s * x + c * y);
            a[p][p] -= t * off;
            a[q][q] += t * off;
            a[p][q] = 0.0;
            a[q][p] = 0.0;
            for r in 0..4 {
                if r != p && r != q {
                    (a[r][p], a[r][q]) = rotate(a[r][p], a[r][q]);
                    a[p][r] = a[r][p];
                    a[q][r] = a[r][q];
                }
                (vectors[r][p], vectors[r][q]) = rotate(vectors[r][p], vectors[r][q]);
            }
        }
        if !changed {
            break;
        }
    }
    let mut largest = 0;
    for i in 1..4 {
        if a[i][i] > a[largest][largest] {
            largest = i;
        }
    }
    let q = vectors.map(|row| row[largest]);
    let norm = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    q.map(|x| x / norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topology(numbers: &[u8]) -> Topology {
        Topology::new(numbers.to_vec(), Vec::new()).unwrap()
    }

    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() < tolerance, "{a} differs from {b}");
    }

    fn random(state: &mut u64) -> f64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (*state >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
    }

    #[test]
    fn displaced_pair_has_analytic_rmsd() {
        let topology = topology(&[6, 6]);
        let a = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let b = [[0.0, -2.0, 0.0], [0.0, 2.0, 0.0]];
        close(
            rmsd(&topology, &topology.automorphisms(1), &a, &b).unwrap(),
            1.0,
            1.0e-12,
        );
    }

    #[test]
    fn rigid_superposition_rotates_every_atom_and_ignores_hydrogen_in_fit() {
        let topology = topology(&[6, 7, 8, 6, 1]);
        let a = [
            [0.1, 0.2, 0.3],
            [1.2, -0.4, 0.5],
            [-0.7, 0.8, 0.9],
            [0.2, 0.3, -1.1],
            [2.0, 1.0, -0.5],
        ];
        let mut b: Vec<_> = a
            .iter()
            .map(|&[x, y, z]| [z + 10.0, x - 8.0, y + 3.0])
            .collect();
        let (aligned, distance) = superpose(&topology, &topology.automorphisms(1), &a, &b).unwrap();
        close(distance, 0.0, 1.0e-12);
        for (expected, actual) in a.iter().zip(&aligned) {
            for k in 0..3 {
                close(expected[k], actual[k], 1.0e-12);
            }
        }
        b[4] = [1.0e4, -2.0e4, 3.0e4];
        close(
            rmsd(&topology, &topology.automorphisms(1), &a, &b).unwrap(),
            0.0,
            1.0e-12,
        );
    }

    #[test]
    fn seeded_rodrigues_rotations_and_translations_fit() {
        let topology = topology(&[6; 12]);
        let mut seed = 42;
        for _ in 0..32 {
            let a: Vec<[f64; 3]> = (0..12)
                .map(|_| std::array::from_fn(|_| 3.0 * random(&mut seed)))
                .collect();
            let mut axis = std::array::from_fn(|_| random(&mut seed));
            let length = dot(axis, axis).sqrt();
            for x in &mut axis {
                *x /= length;
            }
            let angle = random(&mut seed) * std::f64::consts::PI;
            let (sin, cos) = angle.sin_cos();
            let shift: [f64; 3] = std::array::from_fn(|_| 20.0 * random(&mut seed));
            // Construct an independent rotation with Rodrigues' formula.
            let b: Vec<[f64; 3]> = a
                .iter()
                .map(|&p| {
                    let cross = [
                        axis[1] * p[2] - axis[2] * p[1],
                        axis[2] * p[0] - axis[0] * p[2],
                        axis[0] * p[1] - axis[1] * p[0],
                    ];
                    std::array::from_fn(|k| {
                        p[k] * cos
                            + cross[k] * sin
                            + axis[k] * dot(axis, p) * (1.0 - cos)
                            + shift[k]
                    })
                })
                .collect();
            let (aligned, distance) =
                superpose(&topology, &topology.automorphisms(1), &a, &b).unwrap();
            close(distance, 0.0, 2.0e-14);
            for (expected, actual) in a.iter().zip(&aligned) {
                for k in 0..3 {
                    close(expected[k], actual[k], 5.0e-14);
                }
            }
        }
    }

    #[test]
    fn near_zero_distance_keeps_small_nonzero_displacements() {
        let topology = topology(&[6, 6]);
        let a = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let b = [[0.0, -1.000_000_002, 0.0], [0.0, 1.000_000_002, 0.0]];
        close(
            rmsd(&topology, &topology.automorphisms(1), &a, &b).unwrap(),
            2.0e-9,
            1.0e-15,
        );
    }

    #[test]
    fn reflection_does_not_replace_a_proper_rotation() {
        let topology = topology(&[6; 4]);
        let a = [
            [1.0, 1.0, 1.0],
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
        ];
        let b = a.map(|[x, y, z]| [-x, y, z]);
        let (aligned, distance) = superpose(&topology, &topology.automorphisms(1), &a, &b).unwrap();
        close(distance, 2.0, 1.0e-12);
        let actual = a
            .iter()
            .zip(&aligned)
            .map(|(&a, &b)| {
                let delta = std::array::from_fn(|k| a[k] - b[k]);
                dot(delta, delta)
            })
            .sum::<f64>();
        close((actual / 4.0).sqrt(), distance, 1.0e-12);
    }

    #[test]
    fn symmetry_permutation_fits_without_reordering_output() {
        let topology = topology(&[6; 4]);
        let a = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 3.0],
        ];
        let b = [a[1], a[0], a[2], a[3]];
        let autos = topology.automorphisms(24);
        let (aligned, distance) = superpose(&topology, &autos, &a, &b).unwrap();
        close(distance, 0.0, 1.0e-12);
        for (expected, actual) in b.iter().zip(&aligned) {
            for k in 0..3 {
                close(expected[k], actual[k], 1.0e-12);
            }
        }
    }

    #[test]
    fn degenerate_linear_planar_and_coincident_coordinates_fit() {
        let topology = topology(&[6; 3]);
        for a in [
            vec![[-2.0, 0.0, 0.0], [0.0, 0.0, 0.0], [3.0, 0.0, 0.0]],
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
            vec![[1.0, 2.0, 3.0]; 3],
        ] {
            let b: Vec<_> = a
                .iter()
                .map(|[x, y, z]| [y + 4.0, z - 3.0, x + 2.0])
                .collect();
            close(
                rmsd(&topology, &topology.automorphisms(1), &a, &b).unwrap(),
                0.0,
                1.0e-12,
            );
        }
    }

    #[test]
    fn one_atom_and_all_hydrogen_fallback() {
        let one = topology(&[1]);
        let autos = one.automorphisms(1);
        let (aligned, distance) =
            superpose(&one, &autos, &[[1.0, 2.0, 3.0]], &[[8.0, 9.0, 10.0]]).unwrap();
        assert_eq!(aligned, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(distance, 0.0);
        let two = topology(&[1, 1]);
        close(
            rmsd(
                &two,
                &two.automorphisms(1),
                &[[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
                &[[0.0, -2.0, 0.0], [0.0, 2.0, 0.0]],
            )
            .unwrap(),
            1.0,
            1.0e-12,
        );
    }

    #[test]
    fn alignment_rejects_invalid_coordinates_and_mismatched_mapping_sizes() {
        let two = topology(&[6, 8]);
        let autos = two.automorphisms(1);
        let points = [[0.0; 3]; 2];
        for invalid in [&points[..1], &[[0.0; 3], [0.0, f64::NAN, 0.0]]] {
            for (a, b) in [(invalid, &points[..]), (&points[..], invalid)] {
                assert!(rmsd(&two, &autos, a, b).is_err());
                assert!(superpose(&two, &autos, a, b).is_err());
            }
        }
        let other = topology(&[6, 7, 8]).automorphisms(1);
        assert!(rmsd(&two, &other, &points, &points).is_err());
        assert!(superpose(&two, &other, &points, &points).is_err());
    }
}
