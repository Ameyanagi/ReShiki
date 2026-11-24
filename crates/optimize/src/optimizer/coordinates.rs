//! Free Cartesian coordinates and the instantaneous translation and rotation subspace.

use crate::linalg::{dot, norm};

pub(super) struct Coordinates {
    pub indices: Vec<usize>,
    anchored: bool,
}

impl Coordinates {
    pub fn new(numbers: &[u8], positions: &[f64], fixed: &[usize]) -> Result<Self, String> {
        if numbers.len().checked_mul(3) != Some(positions.len()) {
            return Err("expected three Cartesian coordinates per atom".into());
        }
        if numbers.iter().any(|z| !(1..=118).contains(z)) {
            return Err("atomic numbers must be in 1..=118".into());
        }
        if !positions.iter().all(|x| x.is_finite()) {
            return Err("non-finite input position".into());
        }
        let mut mask = vec![false; numbers.len()];
        for &i in fixed {
            let entry = mask.get_mut(i).ok_or("fixed atom index out of range")?;
            if *entry {
                return Err("duplicate fixed atom index".into());
            }
            *entry = true;
        }
        let indices: Vec<_> = (0..positions.len()).filter(|&i| !mask[i / 3]).collect();
        if indices.is_empty() {
            return Err("at least one atom must be free".into());
        }
        Ok(Self {
            indices,
            anchored: !fixed.is_empty(),
        })
    }

    pub fn rigid(&self, positions: &[f64]) -> Vec<Vec<f64>> {
        if self.anchored {
            return Vec::new();
        }
        let n = positions.len();
        let natoms = (n / 3) as f64;
        let mut basis = Vec::with_capacity(6);
        for axis in 0..3 {
            let mut t = vec![0.; n];
            t[axis..]
                .iter_mut()
                .step_by(3)
                .for_each(|v| *v = 1. / natoms.sqrt());
            basis.push(t);
        }
        // Shift the origin before scaling to preserve small interatomic distances.
        let mut relative: Vec<_> = positions
            .iter()
            .enumerate()
            .map(|(i, x)| x - positions[i % 3])
            .collect();
        if relative.iter().any(|x| !x.is_finite()) {
            for (i, x) in relative.iter_mut().enumerate() {
                *x = positions[i] * 0.5 - positions[i % 3] * 0.5;
            }
        }
        let span = relative.iter().fold(0.0_f64, |s, x| s.max(x.abs()));
        if span == 0. {
            return basis;
        }
        let mut center = [0.; 3];
        for (i, x) in relative.iter_mut().enumerate() {
            *x /= span;
            center[i % 3] += *x / natoms;
        }
        let mut rotations = [
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        ];
        for xyz in relative.as_chunks::<3>().0 {
            let (x, y, z) = (xyz[0] - center[0], xyz[1] - center[1], xyz[2] - center[2]);
            rotations[0].extend([0., -z, y]);
            rotations[1].extend([z, 0., -x]);
            rotations[2].extend([-y, x, 0.]);
        }
        // Pivot the first axis by size. A second orthogonalization pass keeps the
        // surviving rotations perpendicular to translations even close to linearity.
        let mut rotations = rotations.map(|v| (norm(&v), v));
        rotations.sort_by(|a, b| b.0.total_cmp(&a.0));
        let tolerance = 1e-12 * rotations[0].0;
        for (_, mut axis) in rotations {
            project(&mut axis, &basis);
            project(&mut axis, &basis);
            let magnitude = norm(&axis);
            if magnitude > tolerance {
                axis.iter_mut().for_each(|v| *v /= magnitude);
                basis.push(axis);
            }
        }
        basis
    }
}

pub(super) fn project(vector: &mut [f64], orthonormal: &[Vec<f64>]) {
    for axis in orthonormal {
        let weight = dot(vector, axis);
        for (x, u) in vector.iter_mut().zip(axis) {
            *x -= weight * u;
        }
    }
}

pub(super) fn metrics(v: &[f64]) -> (f64, f64) {
    let max = v.iter().fold(0.0_f64, |s, x| s.max(x.abs()));
    (max, norm(v) / (v.len() as f64).sqrt())
}
