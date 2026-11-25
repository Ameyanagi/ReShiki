//! Exact chart checks for bounded, ordinary organic coordination.

use super::{
    BOND_FACTOR, COS_FIVE_DEGREES, MIN_DISTANCE, Neighbor, Vector, covalent_radius, direction, dot,
    improper_mask, length, point, subtract,
};

pub(super) struct ChartCheck {
    neighbors: Vec<Vec<usize>>,
    improper_masks: Vec<u8>,
}

impl ChartCheck {
    pub(super) fn new(
        numbers: &[u8],
        x: &[f64],
        hbonds: bool,
        graph: &[Vec<Neighbor>],
    ) -> Option<Self> {
        if hbonds || !organic(numbers) || graph.iter().any(|adjacent| adjacent.len() > 4) {
            return None;
        }
        let mut check = Self {
            neighbors: graph
                .iter()
                .map(|adjacent| adjacent.iter().map(|edge| edge.atom).collect())
                .collect(),
            improper_masks: Vec::new(),
        };
        let mut masks = Vec::new();
        check.local_selection(x, |mask| {
            masks.push(mask);
            true
        })?;
        check.improper_masks = masks;
        Some(check)
    }

    /// Some is an exact answer; None requests the complete generator.
    pub(super) fn same_chart_at(
        &self,
        numbers: &[u8],
        x: &[f64],
        hbonds: bool,
        covalent: &[(usize, usize)],
    ) -> Option<bool> {
        if hbonds
            || numbers.len() != self.neighbors.len()
            || numbers.len().checked_mul(3) != Some(x.len())
            || !organic(numbers)
            || x.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        // Reuse the generator's distance and inclusive cutoff, including its
        // coincident-atom and overflow rejection. No neighbor/path allocations.
        let mut edge = 0;
        for a in 0..numbers.len() {
            for b in a + 1..numbers.len() {
                let r = length(subtract(point(x, a), point(x, b)));
                if !r.is_finite() || r < MIN_DISTANCE {
                    return Some(false);
                }
                let bonded =
                    r <= BOND_FACTOR * (covalent_radius(numbers[a]) + covalent_radius(numbers[b]));
                if bonded != (covalent.get(edge) == Some(&(a, b))) {
                    return Some(false);
                }
                edge += usize::from(bonded);
            }
        }
        let mut expected = self.improper_masks.iter();
        self.local_selection(x, |mask| expected.next() == Some(&mask))
    }

    fn local_selection(&self, x: &[f64], mut retain: impl FnMut(u8) -> bool) -> Option<bool> {
        for (center, adjacent) in self.neighbors.iter().enumerate() {
            let vectors: [Vector; 4] = std::array::from_fn(|index| {
                adjacent
                    .get(index)
                    .map_or([0.; 3], |&atom| direction(x, center, atom))
            });
            for i in 0..adjacent.len() {
                for j in i + 1..adjacent.len() {
                    // Away from either collinear direction, every bend exists
                    // and all ordinary torsion terminal planes are eligible.
                    // No degree-two center can extend a torsion's central path.
                    if dot(vectors[i], vectors[j]).abs() >= COS_FIVE_DEGREES {
                        return None;
                    }
                    for k in j + 1..adjacent.len() {
                        if !retain(improper_mask([vectors[i], vectors[j], vectors[k]])) {
                            return Some(false);
                        }
                    }
                }
            }
        }
        Some(true)
    }
}

fn organic(numbers: &[u8]) -> bool {
    // For these elements, the covalent distance bounds also keep every Lindh
    // rho and three-bond product finite and positive. Unusual elements retain
    // the generator's full force-constant validation.
    numbers
        .iter()
        .all(|number| matches!(number, 1 | 6 | 7 | 8 | 9 | 15 | 16 | 17 | 35 | 53))
}

#[cfg(test)]
mod tests {
    use super::super::{Local, Topology};
    use super::*;

    fn oracle(topology: &Topology, numbers: &[u8], x: &[f64], hbonds: bool) {
        let generated = Topology::generate(numbers, x, hbonds)
            .is_ok_and(|candidate| topology.same_chart(&candidate));
        assert_eq!(
            topology.same_chart_at(numbers, x, hbonds),
            generated,
            "{x:?}"
        );
    }

    #[test]
    fn bounded_checks_match_full_generation_for_deformed_organic_geometries() {
        let fixtures = [
            // Bent, tetrahedral and trigonal-planar coordination.
            (vec![8, 1, 1], vec![0., 0., 0., 1.8, 0., 0., -0.5, 1.7, 0.]),
            (
                vec![6, 1, 1, 1, 1],
                vec![
                    0., 0., 0., 1.2, 1.2, 1.2, 1.2, -1.2, -1.2, -1.2, 1.2, -1.2, -1.2, -1.2, 1.2,
                ],
            ),
            (
                vec![6, 1, 1, 1],
                vec![0., 0., 0., 2., 0., 0., -1., 1.732, 0., -1., -1.732, 0.],
            ),
            // An ordinary central bond with four eligible terminal torsions.
            (
                vec![6, 6, 1, 1, 1, 1],
                vec![
                    -1.3, 0., 0., 1.3, 0., 0., -2.3, 1.7, 0., -2.3, -1.7, 0., 2.3, 1.7, 0., 2.3,
                    -1.7, 0.,
                ],
            ),
        ];
        for (numbers, x) in fixtures {
            let topology = Topology::generate(&numbers, &x, false).unwrap();
            let check = topology
                .chart_check
                .as_ref()
                .expect("ordinary organic chart");
            assert_eq!(
                check.same_chart_at(&numbers, &x, false, &topology.covalent),
                Some(true)
            );
            for sample in 0..50 {
                let scale = sample as f64 * 0.02;
                let displaced: Vec<_> = x
                    .iter()
                    .enumerate()
                    .map(|(i, value)| {
                        value
                            + scale * (0.73 * (i + 7 * sample) as f64).sin()
                            + [13., -17., 23.][i % 3]
                    })
                    .collect();
                oracle(&topology, &numbers, &displaced, false);
            }
        }
    }

    #[test]
    fn exact_bond_cutoffs_and_improper_changes_match_the_generator() {
        let numbers = [6, 6];
        let cutoff = BOND_FACTOR * (covalent_radius(6) + covalent_radius(6));
        for initial_distance in [0.9 * cutoff, 1.1 * cutoff] {
            let topology =
                Topology::generate(&numbers, &[0., 0., 0., initial_distance, 0., 0.], false)
                    .unwrap();
            for distance in [
                0.,
                MIN_DISTANCE * 0.5,
                0.9 * cutoff,
                cutoff,
                cutoff.next_up(),
                1.1 * cutoff,
            ] {
                oracle(&topology, &numbers, &[0., 0., 0., distance, 0., 0.], false);
            }
        }
        let numbers = [6, 1, 1, 1];
        let x = [0., 0., 0., 2., 0., 0., -1., 1.732, 0., -1., -1.732, 0.];
        let topology = Topology::generate(&numbers, &x, false).unwrap();
        for height in [0., 1e-6, 0.05, 0.1, 0.2, 0.5] {
            let mut displaced = x;
            displaced[5] = height;
            oracle(&topology, &numbers, &displaced, false);
        }
        let mut nonplanar = x;
        nonplanar[5] = 0.5;
        assert!(!topology.same_chart_at(&numbers, &nonplanar, false));
    }

    #[test]
    fn full_generation_preserves_linear_extended_torsion_and_hbond_checks() {
        let numbers = [1, 6, 6, 6, 6, 1];
        let x = [
            -1., 1.8, 0., 0., 0., 0., 2.5, 0., 0., 5., 0., 0., 7.5, 0., 0., 8.5, 1.8, 0.,
        ];
        let topology = Topology::generate(&numbers, &x, false).unwrap();
        assert!(topology.chart_check.is_none());
        assert!(
            topology
                .locals
                .iter()
                .any(|(local, _)| { matches!(local, Local::Torsion(_, 1, 4, _)) })
        );
        for offset in [0., 0.02, 0.2, 1.] {
            let mut bent = x;
            bent[7] = offset;
            oracle(&topology, &numbers, &bent, false);
        }

        let numbers = [8, 1, 8];
        let x = [0., 0., 0., 1.8, 0., 0., 5., 0., 0.];
        let topology = Topology::generate(&numbers, &x, true).unwrap();
        assert!(topology.chart_check.is_none());
        assert_eq!(topology.hydrogen_bonds.len(), 1);
        for distance in [4.5, 5., 6., 6.5] {
            let mut moved = x;
            moved[6] = distance;
            oracle(&topology, &numbers, &moved, true);
        }
    }
}
