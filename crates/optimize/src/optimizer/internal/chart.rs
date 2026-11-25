//! Primitive values, their local differences and free-column Wilson rows.

use super::{fragment::Fragment, topology::Topology};

pub(super) struct Chart {
    pub(super) topology: Topology,
    pub(super) fragments: Vec<Fragment>,
    ncart: usize,
    nprim: usize,
    /// All DLC weights of one primitive are adjacent for sparse projection.
    basis_columns: Vec<f64>,
}

impl Chart {
    pub(super) fn new(numbers: &[u8], x: &[f64], hbonds: bool) -> Result<Self, String> {
        let topology = Topology::generate(numbers, x, hbonds)?;
        let fragments: Vec<_> = topology
            .fragments
            .iter()
            .map(|atoms| Fragment::new(atoms, x).ok_or("undefined fragment reference frame"))
            .collect::<Result<_, _>>()?;
        let nprim = topology.locals.len() + fragments.iter().map(Fragment::len).sum::<usize>();
        Ok(Self {
            topology,
            fragments,
            ncart: x.len(),
            nprim,
            basis_columns: Vec::new(),
        })
    }

    pub(super) fn valid_positions(&self, x: &[f64]) -> bool {
        x.len() == self.ncart && x.iter().all(|value| value.is_finite())
    }

    pub(super) fn values(&self, x: &[f64]) -> Option<Vec<f64>> {
        self.collect_values(x, Fragment::values)
    }

    #[cfg(test)]
    pub(super) fn raw_values(&self, x: &[f64]) -> Option<Vec<f64>> {
        self.collect_values(x, Fragment::raw_values)
    }

    fn collect_values(
        &self,
        x: &[f64],
        fragment_values: impl Fn(&Fragment, &[f64]) -> Option<Vec<f64>>,
    ) -> Option<Vec<f64>> {
        if !self.valid_positions(x) {
            return None;
        }
        let mut values = Vec::with_capacity(self.nprim);
        for (coordinate, _) in &self.topology.locals {
            values.push(coordinate.value(x)?);
        }
        for fragment in &self.fragments {
            values.extend(fragment_values(fragment, x)?);
        }
        values.iter().all(|v| v.is_finite()).then_some(values)
    }

    pub(super) fn differences(&self, new: &[f64], old: &[f64]) -> Vec<f64> {
        let mut delta: Vec<_> = new.iter().zip(old).map(|(a, b)| a - b).collect();
        for (i, (p, _)) in self.topology.locals.iter().enumerate() {
            delta[i] = p.difference(new[i], old[i]);
        }
        delta
    }

    pub(super) fn rows(&self, x: &[f64], free: &[usize]) -> Option<Vec<f64>> {
        if !self.valid_positions(x) {
            return None;
        }
        profile_phase!("Wilson rows");
        let mut rows = Vec::with_capacity(self.nprim * free.len());
        let mut full = vec![0.; self.ncart];
        for (coordinate, _) in &self.topology.locals {
            coordinate.write_row(x, &mut full)?;
            rows.extend(free.iter().map(|&i| full[i]));
        }
        for fragment in &self.fragments {
            for row in fragment.rows(x)? {
                rows.extend(free.iter().map(|&i| row[i]));
            }
        }
        rows.iter().all(|v| v.is_finite()).then_some(rows)
    }

    pub(super) fn set_basis(&mut self, basis: &[f64], width: usize) -> Option<()> {
        if width == 0 || basis.len() != self.nprim.checked_mul(width)? {
            return None;
        }
        self.basis_columns = vec![0.; basis.len()];
        for (mode, row) in basis.chunks_exact(self.nprim).enumerate() {
            for (primitive, &weight) in row.iter().enumerate() {
                self.basis_columns[primitive * width + mode] = weight;
            }
        }
        Some(())
    }

    /// UᵀB without expanding each local derivative over every Cartesian column.
    /// Local primitive rows contain at most twelve nonzero components; only
    /// fragment translations and rotations require dense row projection.
    pub(super) fn projected_rows(&self, x: &[f64], free: &[usize]) -> Option<Vec<f64>> {
        if !self.valid_positions(x)
            || free.is_empty()
            || self.basis_columns.len() != free.len().checked_mul(self.nprim)?
        {
            return None;
        }
        profile_phase!("Wilson rows");
        let mut columns = vec![usize::MAX; self.ncart];
        for (column, &index) in free.iter().enumerate() {
            *columns.get_mut(index)? = column;
        }
        let width = free.len();
        let mut column_major = vec![0.; width * width];
        for (primitive, (coordinate, _)) in self.topology.locals.iter().enumerate() {
            let weights = &self.basis_columns[primitive * width..(primitive + 1) * width];
            for &(atom, derivatives) in coordinate.sparse_row(x)?.entries() {
                for (axis, derivative) in derivatives.into_iter().enumerate() {
                    let column = columns[3 * atom + axis];
                    if column != usize::MAX && derivative != 0. {
                        add_scaled(
                            &mut column_major[column * width..(column + 1) * width],
                            weights,
                            derivative,
                        );
                    }
                }
            }
        }
        let mut primitive = self.topology.locals.len();
        for fragment in &self.fragments {
            for row in fragment.rows(x)? {
                let weights = &self.basis_columns[primitive * width..(primitive + 1) * width];
                for (column, &index) in free.iter().enumerate() {
                    let derivative = row[index];
                    if derivative != 0. {
                        add_scaled(
                            &mut column_major[column * width..(column + 1) * width],
                            weights,
                            derivative,
                        );
                    }
                }
                primitive += 1;
            }
        }
        let mut projected = vec![0.; column_major.len()];
        for (column, row) in column_major.chunks_exact(width).enumerate() {
            for (mode, &value) in row.iter().enumerate() {
                projected[mode * width + column] = value;
            }
        }
        projected.iter().all(|v| v.is_finite()).then_some(projected)
    }

    pub(super) fn lindh_hessian(&self, basis: &[f64], curvature_floor: f64) -> Vec<f64> {
        // A weak configurable prior on each centroid and each radius-scaled
        // rotation makes rigid fragment motions usable from the first RFO step.
        let mut force: Vec<_> = self.topology.locals.iter().map(|(_, k)| *k).collect();
        force.resize(self.nprim, curvature_floor);
        let dim = basis.len() / self.nprim;
        let mut h = vec![0.; dim * dim];
        for (i, u) in basis.chunks_exact(self.nprim).enumerate() {
            let ku: Vec<_> = force.iter().zip(u).map(|(k, x)| k * x).collect();
            for (j, v) in basis.chunks_exact(self.nprim).take(i + 1).enumerate() {
                let element = crate::linalg::dot(&ku, v);
                h[i * dim + j] = element;
                h[j * dim + i] = element;
            }
        }
        h
    }
}

fn add_scaled(target: &mut [f64], weights: &[f64], derivative: f64) {
    for (entry, &weight) in target.iter_mut().zip(weights) {
        *entry += derivative * weight;
    }
}

#[cfg(test)]
mod tests {
    use super::super::metric::{Tangent, transpose_apply};
    use super::Chart;

    #[test]
    fn sparse_dlc_projection_matches_dense_rows_with_permuted_fixed_components() {
        let numbers = [8, 1, 1, 8, 1, 1];
        let mut x = vec![
            0., 0., 0., 1.81, 0., 0., -0.45, 1.75, 0., 5.50, 0.60, 0.30, 6.96, -0.43, 0.56, 5.92,
            2.25, -0.32,
        ];
        let free: Vec<_> = (3..x.len()).rev().filter(|&i| i != 8).collect();
        let mut chart = Chart::new(&numbers, &x, true).unwrap();
        let initial = chart.rows(&x, &free).unwrap();
        let (basis, _) = Tangent::delocalize(&initial, free.len()).unwrap();
        chart.set_basis(&basis, free.len()).unwrap();
        for &index in &free {
            x[index] += 0.007 * (index as f64).sin();
        }
        let dense = chart.rows(&x, &free).unwrap();
        let expected: Vec<_> = basis
            .chunks_exact(dense.len() / free.len())
            .flat_map(|mode| transpose_apply(&dense, free.len(), mode))
            .collect();
        let actual = chart.projected_rows(&x, &free).unwrap();
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-14, "{a} != {e}");
        }
    }
}
