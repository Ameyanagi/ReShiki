//! A molecular orbital in the model frame (Å), evaluated from normalized Gaussian AOs.

use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, BasisSet, MAX_L, Shell, axial_norm_ratio, canonical_cartesian_order,
    contraction_overlap, primitive_norm,
};

use std::thread;

use super::{draw::DETAIL, mesh, mesh::Grid};

/// One molecular orbital with its contractions and angular coefficients prepared for evaluation.
pub struct OrbitalField {
    shells: Vec<FieldShell>,
}

struct FieldShell {
    centre: [f64; 3],
    radius2: f64,
    l: u8,
    angular_bound: f64,
    primitives: Vec<(f64, f64)>,
    angular: Angular,
}

enum Angular {
    S(f64),
    P([f64; 3]),
    Cartesian(Vec<([usize; 3], f64)>),
    // Coefficients of the real and imaginary parts of (x + iy)^m, m = 0..l.
    Pure(Vec<[f64; 2]>),
}

impl OrbitalField {
    /// One MO in canonical shell order. Each shell's atom indexes `centres` in Å.
    pub fn new(basis: &BasisSet, row: &[f64], centres: &[[f64; 3]]) -> Result<Self, String> {
        if row.len() != basis.shells.iter().map(Shell::n_functions).sum::<usize>() {
            return Err("orbital row must have one coefficient per basis function".into());
        }
        if row.iter().any(|c| !c.is_finite()) {
            return Err("orbital row contains a non-finite coefficient".into());
        }

        let mut shells = Vec::new();
        let mut remaining = row;
        for shell in &basis.shells {
            shell.validate(centres.len()).map_err(|e| e.to_string())?;
            let centre = centres
                .get(shell.atom as usize)
                .ok_or("shell atom index is out of range")?
                .map(|x| x / ANGSTROM_PER_BOHR);
            if centre.iter().any(|x| !x.is_finite()) {
                return Err("orbital centre must be finite".into());
            }
            let (coefficients, tail) = remaining.split_at(shell.n_functions());
            remaining = tail;
            if coefficients.iter().all(|&c| c == 0.) {
                continue;
            }

            let norm = contraction_overlap(shell).sqrt();
            if !norm.is_finite() || norm <= 0. {
                return Err("orbital contraction normalization must be positive and finite".into());
            }
            let mut primitives = Vec::new();
            for (&a, &d) in shell
                .exponents_per_bohr2
                .iter()
                .zip(&shell.coefficients)
                .filter(|(_, d)| **d != 0.)
            {
                let normalization = primitive_norm(a, shell.l);
                let weight = (d / norm) * normalization;
                if !normalization.is_finite() || !weight.is_finite() {
                    return Err("orbital primitive normalization and weight must be finite".into());
                }
                primitives.push((a, weight));
            }
            let angular = Angular::new(shell, coefficients);
            // Each normalized real harmonic on the unit sphere has magnitude ≤ 1
            // by the addition theorem. Cartesian monomials also have magnitude ≤ 1.
            let angular_bound: f64 = match &angular {
                Angular::Cartesian(terms) => terms.iter().map(|(_, c)| c.abs()).sum(),
                _ => coefficients.iter().map(|c| c.abs()).sum(),
            };
            if !angular_bound.is_finite() {
                return Err("orbital angular coefficient bound must be finite".into());
            }
            let diffuse = primitives
                .iter()
                .map(|&(a, _)| a)
                .fold(f64::INFINITY, f64::min);
            // Start where even the most diffuse Gaussian has decayed by 1e-10.
            // Beyond this radius every r^l exp(-αr²) is decreasing. Include its
            // polynomial and MO weight so a high-l tail cannot be cut prematurely.
            let mut prepared = FieldShell {
                centre,
                radius2: 10. * std::f64::consts::LN_10 / diffuse,
                l: shell.l,
                angular_bound,
                primitives,
                angular,
            };
            while prepared.tail_bound(prepared.radius2) > 1e-10 {
                prepared.radius2 *= 1.25;
            }
            shells.push(prepared);
        }
        Ok(Self { shells })
    }

    /// Signed ψ in atomic units (bohr^(-3/2)) at a model-space point in Å.
    pub fn value(&self, p: [impl Into<f64>; 3]) -> f32 {
        let [x, y, z] = p.map(|x| x.into() / ANGSTROM_PER_BOHR);
        let mut value = 0.;
        for shell in &self.shells {
            let [cx, cy, cz] = shell.centre;
            let (x, y, z) = (x - cx, y - cy, z - cz);
            let r2 = x * x + (y * y + z * z);
            if r2 <= shell.radius2 {
                let radial: f64 = shell
                    .primitives
                    .iter()
                    .map(|&(a, w)| w * (-a * r2).exp())
                    .sum();
                value += radial * shell.angular.at(x, y, z, r2);
            }
        }
        value as f32
    }

    // Axes and slabs are indexed by the grid's own dimensions.
    #[allow(clippy::indexing_slicing)]
    fn sample(&self, grid: Grid) -> impl Iterator<Item = f32> {
        let [xs, ys, zs] = [0, 1, 2].map(|axis| {
            (0..grid.size[axis])
                .map(|i| f64::from(grid.coordinate(axis, i)) / ANGSTROM_PER_BOHR)
                .collect::<Vec<_>>()
        });
        let plane = xs.len() * ys.len();
        let mut values = vec![0.; plane * zs.len()];
        let fill = |slab: &[f64], values: &mut [f64]| {
            for shell in &self.shells {
                shell.add_to_slab(&xs, &ys, slab, values);
            }
        };
        let workers = if cfg!(target_arch = "wasm32") {
            1
        } else {
            thread::available_parallelism()
                .map_or(1, usize::from)
                .min(zs.len())
        };
        if workers > 1 {
            thread::scope(|scope| {
                let mut remaining = values.as_mut_slice();
                let mut first = 0;
                for i in 1..workers {
                    let end = zs.len() * i / workers;
                    let (out, tail) = remaining.split_at_mut((end - first) * plane);
                    let slab = &zs[first..end];
                    scope.spawn(move || fill(slab, out));
                    first = end;
                    remaining = tail;
                }
                fill(&zs[first..], remaining);
            });
        } else {
            fill(&zs, &mut values);
        }
        values.into_iter().map(|v| v as f32)
    }
}

impl FieldShell {
    fn tail_bound(&self, radius2: f64) -> f64 {
        let power = f64::from(self.l) * 0.5;
        // A diffuse primitive can peak outside the box even if it is small at its edge.
        let radial: f64 = self
            .primitives
            .iter()
            .map(|&(a, w)| {
                let r2 = radius2.max(power / a);
                w.abs() * (-a * r2).exp() * r2.powf(power)
            })
            .sum();
        self.angular_bound * radial
    }

    // Multiplying the x and yz factors of exp(-αr²) avoids an exponential per voxel.
    // Accumulate shells in canonical order and round only the complete MO to f32.
    #[allow(clippy::indexing_slicing)]
    fn add_to_slab(&self, xs: &[f64], ys: &[f64], zs: &[f64], values: &mut [f64]) {
        let [cx, cy, cz] = self.centre;
        let radius = self.radius2.sqrt();
        let start = xs.partition_point(|&x| x < cx - radius);
        let end = xs.partition_point(|&x| x <= cx + radius);
        if start == end || zs.iter().all(|&z| (z - cz).abs() > radius) {
            return;
        }
        let dx: Vec<_> = xs[start..end].iter().map(|&x| x - cx).collect();
        let factors: Vec<_> = self
            .primitives
            .iter()
            .map(|&(a, w)| {
                (
                    a,
                    dx.iter()
                        .map(|&x| w * (-a * (x * x)).exp())
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let mut radial = vec![0.; dx.len()];
        for (&z, plane) in zs.iter().zip(values.chunks_exact_mut(xs.len() * ys.len())) {
            let z = z - cz;
            if z * z > self.radius2 {
                continue;
            }
            for (&y, row) in ys.iter().zip(plane.chunks_exact_mut(xs.len())) {
                let y = y - cy;
                let yz2 = y * y + z * z;
                if yz2 > self.radius2 {
                    continue;
                }
                radial.fill(0.);
                for (a, fx) in &factors {
                    let fyz = (-a * yz2).exp();
                    for (sum, &x_weight) in radial.iter_mut().zip(fx) {
                        *sum += x_weight * fyz;
                    }
                }
                for ((value, &x), &r) in row[start..end].iter_mut().zip(&dx).zip(&radial) {
                    let r2 = x * x + yz2;
                    if r2 <= self.radius2 {
                        *value += r * self.angular.at(x, y, z, r2);
                    }
                }
            }
        }
    }
}

impl Angular {
    fn new(shell: &Shell, row: &[f64]) -> Self {
        match *row {
            [s] => Self::S(s),
            [x, y, z] => Self::P([x, y, z]),
            _ if shell.pure => {
                let l = shell.l;
                let (negative, positive) = row.split_at(usize::from(l));
                let sine = std::iter::once(0.).chain(negative.iter().rev().copied());
                Self::Pure(
                    (0..=l)
                        .zip(positive.iter().zip(sine))
                        .map(|(m, (&cos, sin))| {
                            // Relative to x^l, the angular squared norm for m > 0 is
                            // (l+m)! / (2(l-m)!). The zonal harmonic already has axial norm.
                            let ratio = if m == 0 {
                                1.
                            } else {
                                (l - m + 1..=l + m).map(f64::from).product::<f64>() * 0.5
                            };
                            let scale = ratio.sqrt().recip();
                            [cos * scale, sin * scale]
                        })
                        .collect(),
                )
            }
            _ => Self::Cartesian(
                canonical_cartesian_order(shell.l)
                    .into_iter()
                    .zip(row)
                    .filter(|(_, c)| **c != 0.)
                    .map(|(powers @ [a, b, c], mo)| {
                        (
                            powers.map(usize::from),
                            mo / axial_norm_ratio(a, b, c).sqrt(),
                        )
                    })
                    .collect(),
            ),
        }
    }

    // Cartesian powers are bounded by the validated shell's l <= MAX_L.
    #[allow(clippy::indexing_slicing)]
    fn at(&self, x: f64, y: f64, z: f64, r2: f64) -> f64 {
        match self {
            Self::S(s) => *s,
            Self::P([a, b, c]) => a * x + b * y + c * z,
            Self::Cartesian(terms) => {
                let powers = |v| {
                    let mut p = [1.; MAX_L as usize + 1];
                    for i in 1..p.len() {
                        p[i] = v * p[i - 1];
                    }
                    p
                };
                let (xp, yp, zp) = (powers(x), powers(y), powers(z));
                terms
                    .iter()
                    .map(|&([a, b, c], scale)| scale * xp[a] * yp[b] * zp[c])
                    .sum()
            }
            Self::Pure(terms) => {
                let l = terms.len() - 1;
                let (mut re, mut im) = (1., 0.);
                let mut diagonal = 1.;
                let mut sum = 0.;
                for (m, &[cos, sin]) in terms.iter().enumerate() {
                    // Homogeneous associated Legendre recurrence. The diagonal
                    // is (2m-1)!! with the azimuth carried by (x+iy)^m.
                    let (mut previous, mut current) = (0., diagonal);
                    for n in m + 1..=l {
                        let next = ((2 * n - 1) as f64 * z * current
                            - (n + m - 1) as f64 * r2 * previous)
                            / (n - m) as f64;
                        (previous, current) = (current, next);
                    }
                    sum += current * (cos * re + sin * im);
                    (re, im) = (re * x - im * y, im * x + re * y);
                    diagonal *= (2 * m + 1) as f64;
                }
                sum
            }
        }
    }
}

fn orbital_grid(field: &OrbitalField, points: &[[f32; 3]], iso: f32) -> Option<Grid> {
    let mut lo = *points.first()?;
    let mut hi = lo;
    for p in points {
        if p.iter().any(|x| !x.is_finite()) {
            return None;
        }
        for ((low, high), &x) in lo.iter_mut().zip(&mut hi).zip(p) {
            *low = low.min(x);
            *high = high.max(x);
        }
    }
    let max_radius2 = (15. / ANGSTROM_PER_BOHR).powi(2);
    let mut margin2 = (3. / ANGSTROM_PER_BOHR).powi(2);
    for shell in &field.shells {
        while margin2 < max_radius2 && shell.tail_bound(margin2) >= f64::from(iso) {
            margin2 = (margin2 * 1.25).min(max_radius2);
        }
    }
    let margin = (margin2.sqrt() * ANGSTROM_PER_BOHR) as f32;
    Some(Grid::around(lo.map(|x| x - margin), hi.map(|x| x + margin)))
}

/// Drawing and turning meshes at ±iso around the supplied points in Å.
/// Empty above the sampled peak, or `None` if the grown box still cuts a lobe.
pub fn orbital_meshes(
    field: &OrbitalField,
    points: &[[f32; 3]],
    iso: f32,
) -> Option<[mesh::Mesh; 2]> {
    if !iso.is_finite() || iso <= 0. {
        return None;
    }
    let value = |p| field.value(p);
    let (raw, step) =
        mesh::isosurface_sampled(&value, orbital_grid(field, points, iso)?, iso, |grid| {
            field.sample(grid)
        });
    if raw.truncated {
        return None;
    }
    let fine = mesh::simplify(&raw, DETAIL[0]);
    let coarse = mesh::simplify(&fine, DETAIL[1]);
    Some([fine, coarse].map(|m| mesh::refine(&m, value, iso, step)))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod timing;
