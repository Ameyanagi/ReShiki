//! Proper rigid fits (Horn, J. Opt. Soc. Am. A 4, 629, 1987 [Horn1987Quaternion]) and atom assignment
//! (Kuhn, Naval Research Logistics Quarterly 2, 83, 1955 [Kuhn1955Hungarian], and
//! Munkres, Journal of the Society for Industrial and Applied Mathematics 5, 32,
//! 1957 [Munkres1957Assignment]).

use std::array::from_fn;

use crate::conformers::horn_rotation;

use super::{TsError, validate_geometry};

/// Complete the atom map by minimum total squared displacement, per element.
///
/// `mapped[i] = Some(j)` fixes reactant atom `i` to product atom `j`. Only these
/// fixed pairs determine the equal-weight proper rigid fit before assignment.
/// With zero fixed pairs, assignment uses the supplied coordinates. One fixed
/// pair determines a translation. A collinear set permits free axial rotation.
/// The returned permutation maps each reactant index to a product index.
pub fn least_motion_map(
    numbers_reactant: &[u8],
    reactant: &[f64],
    numbers_product: &[u8],
    product: &[f64],
    mapped: &[Option<usize>],
) -> Result<Vec<usize>, TsError> {
    validate_geometry(numbers_reactant, reactant)?;
    validate_geometry(numbers_product, product)?;
    let n = numbers_reactant.len();
    if numbers_product.len() != n || mapped.len() != n {
        return Err(TsError::Invalid("atom-map lengths differ".into()));
    }
    let counts = |numbers: &[u8]| {
        let mut counts = [0_usize; 119];
        for &number in numbers {
            counts[usize::from(number)] += 1;
        }
        counts
    };
    if counts(numbers_reactant) != counts(numbers_product) {
        return Err(TsError::Invalid("endpoint element counts differ".into()));
    }

    let mut permutation = vec![usize::MAX; n];
    let mut used = vec![false; n];
    let mut reference = Vec::new();
    let mut mobile = Vec::new();
    for (i, &j) in mapped.iter().enumerate() {
        if let Some(j) = j {
            if j >= n || used[j] || numbers_reactant[i] != numbers_product[j] {
                return Err(TsError::Invalid(
                    "fixed atom map must be an element-preserving injection".into(),
                ));
            }
            permutation[i] = j;
            used[j] = true;
            reference.extend_from_slice(&reactant[3 * i..3 * i + 3]);
            mobile.extend_from_slice(&product[3 * j..3 * j + 3]);
        }
    }
    let fit = Fit::new(&reference, &mobile, &vec![1.0; reference.len() / 3]);
    let aligned = fit.apply(product);
    for number in 1..=118 {
        let rows: Vec<usize> = (0..n)
            .filter(|&i| numbers_reactant[i] == number && mapped[i].is_none())
            .collect();
        let columns: Vec<usize> = (0..n)
            .filter(|&j| numbers_product[j] == number && !used[j])
            .collect();
        let costs: Vec<Vec<f64>> = rows
            .iter()
            .map(|&i| {
                columns
                    .iter()
                    .map(|&j| {
                        (0..3)
                            .map(|k| (reactant[3 * i + k] - aligned[3 * j + k]).powi(2))
                            .sum()
                    })
                    .collect()
            })
            .collect();
        if costs.iter().flatten().any(|cost| !cost.is_finite()) {
            return Err(TsError::Invalid("atom-map distances overflow".into()));
        }
        for (i, j) in assignment(&costs).into_iter().enumerate() {
            permutation[rows[i]] = columns[j];
        }
    }
    Ok(permutation)
}

/// Fit all atoms with mass weights, retaining their input order. Callers have
/// validated matching Cartesian lengths and finite, positive masses.
pub(super) fn superpose(reference: &[f64], mobile: &[f64], masses: &[f64]) -> Vec<f64> {
    Fit::new(reference, mobile, masses).apply(mobile)
}

pub(super) struct Fit {
    rotation: [[f64; 3]; 3],
    reference_anchor: [f64; 3],
    reference_mean: [f64; 3],
    mobile_anchor: [f64; 3],
    mobile_mean: [f64; 3],
}

impl Fit {
    pub(super) fn new(reference: &[f64], mobile: &[f64], masses: &[f64]) -> Self {
        if masses.is_empty() {
            return Self {
                rotation: from_fn(|i| from_fn(|j| f64::from(i == j))),
                reference_anchor: [0.0; 3],
                reference_mean: [0.0; 3],
                mobile_anchor: [0.0; 3],
                mobile_mean: [0.0; 3],
            };
        }
        // Relative coordinates retain precision when a molecule is far from
        // the origin. Normalizing weights leaves the optimal rotation intact.
        let mass_scale = masses.iter().copied().fold(0.0_f64, f64::max);
        let weights: Vec<f64> = masses.iter().map(|m| m / mass_scale).collect();
        let total: f64 = weights.iter().sum();
        let center = |x: &[f64]| {
            let anchor = from_fn(|k| x[k]);
            let mean = from_fn(|k| {
                weights
                    .iter()
                    .enumerate()
                    .map(|(i, w)| w * (x[3 * i + k] - anchor[k]))
                    .sum::<f64>()
                    / total
            });
            (anchor, mean)
        };
        let (reference_anchor, reference_mean) = center(reference);
        let (mobile_anchor, mobile_mean) = center(mobile);
        let mut covariance = [[0.0; 3]; 3];
        for (atom, &weight) in weights.iter().enumerate() {
            for (i, row) in covariance.iter_mut().enumerate() {
                let m = (mobile[3 * atom + i] - mobile_anchor[i]) - mobile_mean[i];
                for (j, value) in row.iter_mut().enumerate() {
                    let r = (reference[3 * atom + j] - reference_anchor[j]) - reference_mean[j];
                    *value += weight * m * r;
                }
            }
        }
        Self {
            rotation: horn_rotation(covariance),
            reference_anchor,
            reference_mean,
            mobile_anchor,
            mobile_mean,
        }
    }

    pub(super) fn apply(&self, positions: &[f64]) -> Vec<f64> {
        positions
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| {
                let offset: [f64; 3] =
                    from_fn(|k| (p[k] - self.mobile_anchor[k]) - self.mobile_mean[k]);
                from_fn::<_, 3, _>(|i| {
                    let rotated: f64 = self.rotation[i]
                        .iter()
                        .zip(offset)
                        .map(|(r, x)| r * x)
                        .sum();
                    (rotated + self.reference_mean[i]) + self.reference_anchor[i]
                })
            })
            .collect()
    }

    /// Rotate each Cartesian 3×3 atom-pair block as R H_ab Rᵀ into the fitted
    /// frame. Translating the positions has no effect on the force constants.
    pub(super) fn rotate_hessian(&self, h: &[f64], n: usize) -> Vec<f64> {
        let mut rotated = vec![0.; h.len()];
        for i in 0..n {
            for j in 0..=i {
                let mut value = 0.;
                for (a, &ria) in self.rotation[i % 3].iter().enumerate() {
                    let row = (i / 3 * 3 + a) * n + j / 3 * 3;
                    for (b, &rjb) in self.rotation[j % 3].iter().enumerate() {
                        value += ria * h[row + b] * rjb;
                    }
                }
                rotated[i * n + j] = value;
                rotated[j * n + i] = value;
            }
        }
        rotated
    }
}

/// Primal-dual shortest augmenting paths for a square assignment. Potentials
/// make reduced costs nonnegative. One augmentation matches each new row.
fn assignment(costs: &[Vec<f64>]) -> Vec<usize> {
    let n = costs.len();
    let mut row_potential = vec![0.0; n + 1];
    let mut column_potential = vec![0.0; n + 1];
    let mut matched_row = vec![0; n + 1];
    for row in 1..=n {
        matched_row[0] = row;
        let mut column = 0;
        let mut slack = vec![f64::INFINITY; n + 1];
        let mut predecessor = vec![0; n + 1];
        let mut visited = vec![false; n + 1];
        loop {
            visited[column] = true;
            let active_row = matched_row[column];
            let mut delta = f64::INFINITY;
            let mut next_column = 0;
            for j in 1..=n {
                if !visited[j] {
                    let reduced = costs[active_row - 1][j - 1]
                        - row_potential[active_row]
                        - column_potential[j];
                    if reduced < slack[j] {
                        slack[j] = reduced;
                        predecessor[j] = column;
                    }
                    if slack[j] < delta {
                        delta = slack[j];
                        next_column = j;
                    }
                }
            }
            for j in 0..=n {
                if visited[j] {
                    row_potential[matched_row[j]] += delta;
                    column_potential[j] -= delta;
                } else {
                    slack[j] -= delta;
                }
            }
            column = next_column;
            if matched_row[column] == 0 {
                break;
            }
        }
        while column != 0 {
            let previous = predecessor[column];
            matched_row[column] = matched_row[previous];
            column = previous;
        }
    }
    let mut result = vec![0; n];
    for column in 1..=n {
        result[matched_row[column] - 1] = column - 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hydrogen_assignment_uses_only_fixed_atoms_for_the_fit() {
        // Three fixed atoms determine the rigid frame. The remaining hydrogens
        // of this methane-like set are deliberately interchanged.
        let numbers = [6, 1, 1, 1, 1];
        let reference = [
            0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0,
        ];
        let order = [0, 2, 1, 4, 3];
        let product: Vec<f64> = order
            .iter()
            .flat_map(|&i| {
                let [x, y, z] = reference[3 * i..3 * i + 3].try_into().unwrap();
                [z + 10.0, x - 3.0, y + 7.0]
            })
            .collect();
        let map = least_motion_map(
            &numbers,
            &reference,
            &numbers,
            &product,
            &[Some(0), Some(2), Some(1), None, None],
        )
        .unwrap();
        assert_eq!(map, order);
        for i in 0..numbers.len() {
            assert_eq!(order[map[i]], i);
        }
    }

    #[test]
    fn fixed_atoms_are_respected_even_when_motion_is_larger() {
        let x = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 4.0, 0.0, 0.0];
        let map = least_motion_map(&[1, 1, 1], &x, &[1, 1, 1], &x, &[Some(2), None, None]).unwrap();
        assert_eq!(map[0], 2);
        let mut sorted = map;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2]);
    }

    #[test]
    fn malformed_maps_and_mismatched_elements_are_errors() {
        let x = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        for (a, b, mapped) in [
            ([6, 1], [6, 6], [None, None]),
            ([6, 1], [6, 1], [Some(0), Some(0)]),
            ([6, 1], [6, 1], [Some(2), None]),
            ([6, 1], [6, 1], [Some(1), None]),
        ] {
            assert!(matches!(
                least_motion_map(&a, &x, &b, &x, &mapped),
                Err(TsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn assignment_finds_the_global_minimum_and_breaks_ties_deterministically() {
        let costs = vec![
            vec![1.0, 2.0, 9.0],
            vec![1.1, 8.0, 9.0],
            vec![9.0, 1.0, 0.0],
        ];
        assert_eq!(assignment(&costs), [1, 0, 2]);
        assert_eq!(assignment(&vec![vec![0.0; 4]; 4]), [0, 1, 2, 3]);
        assert!(assignment(&[]).is_empty());
    }

    #[test]
    fn all_atom_fit_is_weighted_and_never_reflects() {
        let reference = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0];
        let mobile: Vec<f64> = reference
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[2] + 5.0, p[0] - 2.0, p[1] + 8.0])
            .collect();
        let masses = [12.0, 1.0, 16.0, 14.0];
        let aligned = superpose(&reference, &mobile, &masses);
        assert!(
            aligned
                .iter()
                .zip(reference)
                .all(|(a, b)| (a - b).abs() < 1e-12)
        );
        let reflected: Vec<f64> = reference
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [-p[0], p[1], p[2]])
            .collect();
        let aligned = superpose(&reference, &reflected, &masses);
        assert!(
            aligned
                .iter()
                .zip(reference)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                > 0.1
        );
        for k in 0..3 {
            let center_difference: f64 = masses
                .iter()
                .enumerate()
                .map(|(i, m)| m * (aligned[3 * i + k] - reference[3 * i + k]))
                .sum();
            assert!(center_difference.abs() < 1e-12);
        }
    }
}
