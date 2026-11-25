//! Local internal-coordinate values and analytic Wilson B rows.
//!
//! Stretch, angle and signed dihedral derivatives follow V. Bakken and
//! T. Helgaker, J. Chem. Phys. 117, 9160–9174 (2002), Sec. II B,
//! doi:10.1063/1.1515483, Eqs. (17), (25) and (34). The signed out-of-plane
//! angle follows by differentiating the unit bond and plane normal (Wilson,
//! Decius and Cross, *Molecular Vibrations*, 1955, Ch. 4). Lengths use the
//! Cartesian input units and angles are in radians.

use std::f64::consts::{PI, TAU};

use super::geometry::{Vector, cross, dot, length, point, subtract};

// Numerical singularity threshold for evaluating an existing coordinate.
const MIN_SINE: f64 = 1e-12;

#[derive(Clone, Debug)]
pub(super) enum Local {
    Stretch(usize, usize),
    /// A–B–C, with B the central atom.
    Bend(usize, usize, usize),
    /// One component e·(u + v), with u = unit(A–B), v = unit(C–B).
    /// The stored transverse axis e is kept fixed for this coordinate chart.
    LinearBend(usize, usize, usize, Vector),
    /// Signed A–B–C–D torsion. Zero puts the outer bonds on the same side of B–C.
    Torsion(usize, usize, usize, usize),
    /// Signed angle of A–B out of the B–C–D plane. B is central and the
    /// positive plane normal is (C–B) × (D–B).
    OutOfPlane(usize, usize, usize, usize),
}

impl Local {
    pub(super) fn value(&self, x: &[f64]) -> Option<f64> {
        let value = match *self {
            Self::Stretch(a, b) => bond(x, a, b)?.1,
            Self::Bend(a, b, c) => {
                let (u, _) = bond(x, a, b)?;
                let (v, _) = bond(x, c, b)?;
                length(cross(u, v)).atan2(dot(u, v))
            }
            Self::LinearBend(a, b, c, axis) => {
                let (u, _) = bond(x, a, b)?;
                let (v, _) = bond(x, c, b)?;
                let (e, _) = unit(axis)?;
                dot(e, add(u, v))
            }
            Self::Torsion(a, b, c, d) => {
                let (u, _) = bond(x, a, b)?;
                let (v, _) = bond(x, d, c)?;
                let (w, _) = bond(x, c, b)?;
                let (p, ps) = unit(cross(u, w))?;
                let (q, qs) = unit(cross(v, w))?;
                if ps <= MIN_SINE || qs <= MIN_SINE {
                    return None;
                }
                dot(w, cross(p, q)).atan2(dot(p, q))
            }
            Self::OutOfPlane(a, b, c, d) => {
                let (u, _) = bond(x, a, b)?;
                let (v, _) = bond(x, c, b)?;
                let (w, _) = bond(x, d, b)?;
                let (normal, sine) = unit(cross(v, w))?;
                if sine <= MIN_SINE {
                    return None;
                }
                dot(u, normal).atan2(length(cross(u, normal)))
            }
        };
        value.is_finite().then_some(value)
    }

    /// Write a full Cartesian row, clearing every component before evaluation.
    /// Undefined derivatives or malformed input return `None` with a zero row.
    pub(super) fn write_row(&self, x: &[f64], row: &mut [f64]) -> Option<()> {
        row.fill(0.0);
        if row.len() != x.len() {
            return None;
        }
        match *self {
            Self::Stretch(a, b) => {
                let (u, _) = bond(x, a, b)?;
                write_sparse(row, &[(a, u), (b, scale(u, -1.0))])
            }
            Self::Bend(a, b, c) => {
                let (u, ru) = bond(x, a, b)?;
                let (v, rv) = bond(x, c, b)?;
                let (normal, sine) = unit(cross(u, v))?;
                if sine <= MIN_SINE {
                    return None;
                }
                let first = divide(cross(u, normal), ru);
                let last = divide(cross(normal, v), rv);
                write_sparse(
                    row,
                    &[(a, first), (b, scale(add(first, last), -1.0)), (c, last)],
                )
            }
            Self::LinearBend(a, b, c, axis) => {
                let (u, ru) = bond(x, a, b)?;
                let (v, rv) = bond(x, c, b)?;
                let (e, _) = unit(axis)?;
                // d(unit r)/dr = (I - uu^T)/|r|. Unlike an ordinary
                // angle row these derivatives are smooth through linearity.
                let first = divide(subtract(e, scale(u, dot(e, u))), ru);
                let last = divide(subtract(e, scale(v, dot(e, v))), rv);
                write_sparse(
                    row,
                    &[(a, first), (b, scale(add(first, last), -1.0)), (c, last)],
                )
            }
            Self::Torsion(a, b, c, d) => {
                let (u, ru) = bond(x, a, b)?;
                let (v, rv) = bond(x, d, c)?;
                let (w, rw) = bond(x, c, b)?;
                let p = cross(u, w);
                let q = cross(v, w);
                let pp = dot(p, p);
                let qq = dot(q, q);
                if pp <= MIN_SINE * MIN_SINE || qq <= MIN_SINE * MIN_SINE {
                    return None;
                }
                let first = divide(divide(p, pp), ru);
                let last = scale(divide(divide(q, qq), rv), -1.0);
                let axis_change = divide(
                    subtract(
                        scale(divide(p, pp), dot(u, w)),
                        scale(divide(q, qq), dot(v, w)),
                    ),
                    rw,
                );
                let second = subtract(axis_change, first);
                let third = scale(add(axis_change, last), -1.0);
                write_sparse(row, &[(a, first), (b, second), (c, third), (d, last)])
            }
            Self::OutOfPlane(a, b, c, d) => {
                let (u, ru) = bond(x, a, b)?;
                let (v, rv) = bond(x, c, b)?;
                let (w, rw) = bond(x, d, b)?;
                let (normal, sine) = unit(cross(v, w))?;
                let cosine = length(cross(u, normal));
                if sine <= MIN_SINE || cosine <= MIN_SINE {
                    return None;
                }
                let projection = dot(u, normal);
                let first = divide(divide(subtract(normal, scale(u, projection)), cosine), ru);
                // For p=v×w, dq/dp=(u-(u·n)n)/(|p| cos q).
                // Its tangent projection is already perpendicular to p.
                let plane = divide(divide(subtract(u, scale(normal, projection)), cosine), sine);
                let third = divide(cross(w, plane), rv);
                let fourth = divide(cross(plane, v), rw);
                let second = scale(add(first, add(third, fourth)), -1.0);
                write_sparse(row, &[(a, first), (b, second), (c, third), (d, fourth)])
            }
        }
    }

    /// Difference on the local chart, wrapping proper torsions at ±π.
    pub(super) fn difference(&self, new: f64, old: f64) -> f64 {
        let difference = new - old;
        if matches!(self, Self::Torsion(..)) {
            let wrapped = difference % TAU;
            if wrapped > PI {
                wrapped - TAU
            } else if wrapped <= -PI {
                wrapped + TAU
            } else {
                wrapped
            }
        } else {
            difference
        }
    }
}

/// Choose two orthonormal transverse axes for a near-linear A–B–C angle.
///
/// The line direction is unit(u−v), which remains well conditioned near 180°.
/// Projecting the least-aligned Cartesian axis onto its perpendicular plane
/// avoids a small initial denominator. Freezing both axes avoids jumps at
/// Cartesian-axis ties. The two components equal bending angles to first
/// order and retain full transverse rank while the new line has a nonzero
/// projection on the original line.
pub(super) fn bend_axes(x: &[f64], a: usize, b: usize, c: usize) -> Option<[Vector; 2]> {
    let (u, _) = bond(x, a, b)?;
    let (v, _) = bond(x, c, b)?;
    let (line, _) = unit(subtract(u, v))?;
    let axis = (0..3).min_by(|&i, &j| line[i].abs().total_cmp(&line[j].abs()))?;
    let mut reference = [0.0; 3];
    reference[axis] = 1.0;
    let (first, _) = unit(subtract(reference, scale(line, dot(reference, line))))?;
    let (second, _) = unit(cross(line, first))?;
    Some([first, second])
}

/// Unit vector from B to A, and its length.
fn bond(x: &[f64], a: usize, b: usize) -> Option<(Vector, f64)> {
    unit(subtract(point(x, a)?, point(x, b)?))
}

fn unit(vector: Vector) -> Option<(Vector, f64)> {
    let norm = length(vector);
    (norm.is_finite() && norm > 0.0).then(|| (divide(vector, norm), norm))
}

fn write_sparse(row: &mut [f64], entries: &[(usize, Vector)]) -> Option<()> {
    if entries
        .iter()
        .any(|(_, vector)| vector.iter().any(|value| !value.is_finite()))
    {
        return None;
    }
    for &(atom, vector) in entries {
        for (target, value) in row[3 * atom..3 * atom + 3].iter_mut().zip(vector) {
            *target += value;
        }
    }
    if row.iter().any(|value| !value.is_finite()) {
        row.fill(0.0);
        return None;
    }
    Some(())
}

fn add(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|k| a[k] + b[k])
}

fn scale(a: Vector, value: f64) -> Vector {
    a.map(|component| component * value)
}

fn divide(a: Vector, value: f64) -> Vector {
    a.map(|component| component / value)
}
