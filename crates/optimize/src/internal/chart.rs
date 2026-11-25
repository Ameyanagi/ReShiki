//! Primitive values, their local differences and free-column Wilson rows.

use super::{fragment::Fragment, topology::Topology};

pub(super) struct Chart {
    pub(super) topology: Topology,
    pub(super) fragments: Vec<Fragment>,
    ncart: usize,
    nprim: usize,
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
        })
    }

    pub(super) fn valid_positions(&self, x: &[f64]) -> bool {
        x.len() == self.ncart && x.iter().all(|value| value.is_finite())
    }

    pub(super) fn values(&self, x: &[f64]) -> Option<Vec<f64>> {
        if !self.valid_positions(x) {
            return None;
        }
        let mut values = Vec::with_capacity(self.nprim);
        for (coordinate, _) in &self.topology.locals {
            values.push(coordinate.value(x)?);
        }
        for fragment in &self.fragments {
            values.extend(fragment.values(x)?);
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
