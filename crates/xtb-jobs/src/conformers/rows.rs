//! Statistics of the retained, saved ensemble, in trajectory order.

use reshiki_model::document::Document;
use reshiki_optimize::conformers::boltzmann_weights;
use reshiki_qcdata::Trajectory;

use super::KCAL_PER_HARTREE;

/// One saved conformer's statistics, shared by tables and exports.
#[derive(Clone, Debug, PartialEq)]
pub struct RowStatistics {
    pub delta_kcal: f64,
    pub start_kcal: Option<f64>,
    pub percent: f64,
    /// Symmetry-aware heavy-atom RMSD to the first saved conformer, in Å.
    pub rmsd: Option<f64>,
    pub cluster: Option<u32>,
}

/// Boltzmann fractions of the kept ensemble at 298.15 K, in input order.
pub fn populations(energies_hartree: &[f64]) -> Vec<f64> {
    boltzmann_weights(energies_hartree, 298.15).unwrap_or_default()
}

/// Statistics in trajectory order. For older results without recorded RMSDs,
/// infer the topology from the drawing bonds and hydrogen neighbors in the
/// first saved conformer.
pub fn trajectory_rows(
    doc: &Document,
    atoms: &[Option<u64>],
    path: &Trajectory,
    reference_energy: f64,
) -> Vec<RowStatistics> {
    let mut rows = statistics(path, reference_energy);
    if path.rmsd_to_best_angstrom.is_empty() {
        measure_rmsds(doc, atoms, path, &mut rows);
    }
    rows
}

fn measure_rmsds(
    doc: &Document,
    atoms: &[Option<u64>],
    path: &Trajectory,
    rows: &mut [RowStatistics],
) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(first) = path.frame(0) {
        use reshiki_model::science;
        use reshiki_optimize::conformers::{Settings, Topology, rmsd};

        let bonds = science::frame_bonds(doc, atoms, &first.positions_angstrom)
            .into_iter()
            .map(|(a, b, _)| [a, b])
            .collect();
        if let Ok(topology) = Topology::new(first.numbers.clone(), bonds) {
            let autos = topology.automorphisms(Settings::default().automorphism_cap);
            for (index, row) in rows.iter_mut().enumerate() {
                row.rmsd = path.frame(index).and_then(|frame| {
                    rmsd(
                        &topology,
                        &autos,
                        &first.positions_angstrom,
                        &frame.positions_angstrom,
                    )
                    .ok()
                });
            }
        }
    }
    // The browser has no conformer fitting.
    #[cfg(target_arch = "wasm32")]
    let _ = (doc, atoms, path, rows);
}

pub(super) fn statistics(path: &Trajectory, reference_energy: f64) -> Vec<RowStatistics> {
    let start_minimum = path
        .minimizer_energies_hartree
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    path.energies_hartree
        .iter()
        .zip(populations(&path.energies_hartree))
        .enumerate()
        .map(|(index, (energy, fraction))| RowStatistics {
            delta_kcal: (energy - reference_energy) * KCAL_PER_HARTREE,
            start_kcal: path
                .minimizer_energies_hartree
                .get(index)
                .map(|energy| (energy - start_minimum) * KCAL_PER_HARTREE),
            percent: fraction * 100.0,
            rmsd: path.rmsd_to_best_angstrom.get(index).copied(),
            cluster: path.cluster_sizes.get(index).copied(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use reshiki_model::document::Point;
    #[cfg(not(target_arch = "wasm32"))]
    use reshiki_optimize::conformers::{Candidate, Settings, Topology, rank, rmsd};
    use reshiki_optimize::conformers::{Ranked, trajectory};

    fn ensemble(positions: &[[[f64; 3]; 4]]) -> Trajectory {
        Trajectory {
            numbers: vec![6, 6, 6, 1],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 2,
            positions_angstrom: positions.iter().flatten().flatten().copied().collect(),
            energies_hartree: vec![-5., -4.999],
            labels: vec![],
            minimizer_energies_hartree: vec![0.01, 0.009],
            cluster_sizes: vec![12, 5],
            rmsd_to_best_angstrom: vec![],
        }
    }

    #[test]
    fn rows_and_exports_share_retained_ensemble_statistics() {
        let positions = [[0., 0., 0.], [1., 0., 0.], [1., 2., 0.], [-0.8, 0., 0.]];
        let path = ensemble(&[positions, positions]);
        let rows = trajectory_rows(&Document::default(), &[None; 4], &path, -5.);
        let (_, exported) = super::super::ranked_trajectory(Some(&path)).unwrap();
        for (row, exported) in rows.iter().zip(exported) {
            assert_eq!(row.delta_kcal, exported.statistics.delta_kcal);
            assert_eq!(row.start_kcal, exported.statistics.start_kcal);
            assert_eq!(row.percent, exported.statistics.percent);
            assert_eq!(row.cluster, exported.statistics.cluster);
        }
        assert_eq!(rows[0].delta_kcal, 0.);
        assert!((rows[1].delta_kcal - 0.001 * KCAL_PER_HARTREE).abs() < 1e-9);
        assert!((rows[0].start_kcal.unwrap() - 0.001 * KCAL_PER_HARTREE).abs() < 1e-9);
        assert_eq!(rows[1].start_kcal, Some(0.));
        assert_eq!((rows[0].cluster, rows[1].cluster), (Some(12), Some(5)));
        assert!((rows.iter().map(|row| row.percent).sum::<f64>() - 100.).abs() < 1e-12);
        assert_eq!(populations(&[]), Vec::<f64>::new());
    }

    #[test]
    fn populations_are_renormalized_after_only_saved_conformers_are_kept() {
        let ranked = vec![
            Ranked {
                positions: vec![[0.; 3]],
                energy_hartree: -5.,
                relative_kcal: 0.,
                start_energy_hartree: 0.,
                cluster_size: 1,
                boltzmann: 1. / 33.,
                rmsd_to_best: 0.,
            };
            33
        ];
        let path = trajectory(&ranked, &[8], 0, 1).unwrap();
        assert_eq!(path.steps(), 32);
        let rows = trajectory_rows(&Document::default(), &[None], &path, -5.);
        assert!(rows.iter().all(|row| row.percent == 100. / 32.));
        assert_eq!(rows.iter().map(|row| row.percent).sum::<f64>(), 100.);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn rmsd_uses_hydrogen_topology_of_first_saved_frame() {
        let mut doc = Document::default();
        let ids: Vec<_> = (0..3)
            .map(|_| doc.add_atom("C", Point::default()))
            .collect();
        doc.add_bond(ids[0], ids[1], 1, "plain");
        doc.add_bond(ids[1], ids[2], 1, "plain");
        let atoms = [Some(ids[0]), Some(ids[1]), Some(ids[2]), None];
        // The launch model's H was attached to the central carbon. After
        // optimization, the saved best frame puts it nearest an end carbon.
        let best = [[0., 0., 0.], [1., 0., 0.], [1., 2., 0.], [-0.8, 0., 0.]];
        let mobile = [best[2], best[1], best[0], best[3]];
        let path = ensemble(&[best, mobile]);
        let rows = trajectory_rows(&doc, &atoms, &path, -5.);
        let topology = Topology::new(path.numbers.clone(), vec![[0, 1], [1, 2], [0, 3]]).unwrap();
        let expected = rmsd(
            &topology,
            &topology.automorphisms(Settings::default().automorphism_cap),
            &best,
            &mobile,
        )
        .unwrap();
        assert!(expected > 0.1);
        assert_eq!(rows[1].rmsd, Some(expected));
        let launch_topology =
            Topology::new(path.numbers.clone(), vec![[0, 1], [1, 2], [1, 3]]).unwrap();
        let launch_rmsd = rmsd(
            &launch_topology,
            &launch_topology.automorphisms(Settings::default().automorphism_cap),
            &best,
            &mobile,
        )
        .unwrap();
        assert!(launch_rmsd < 1e-12);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn ranked_rmsd_distinguishes_swapped_ends_after_hydrogen_moves() {
        let mut doc = Document::default();
        let ids: Vec<_> = (0..3)
            .map(|_| doc.add_atom("C", Point::default()))
            .collect();
        doc.add_bond(ids[0], ids[1], 1, "plain");
        doc.add_bond(ids[1], ids[2], 1, "plain");
        let atoms = [Some(ids[0]), Some(ids[1]), Some(ids[2]), None];
        let numbers = vec![6, 6, 6, 1];
        let topology = Topology::new(numbers.clone(), vec![[0, 1], [1, 2], [0, 3]]).unwrap();
        // H was attached to an end carbon at launch and is now nearest the middle carbon.
        let best = [[0., 0., 0.], [1., 0., 0.], [1., 2., 0.], [1., -0.8, 0.]];
        let swapped = [best[2], best[1], best[0], best[3]];
        let candidates = [best, swapped]
            .into_iter()
            .zip([-5., -4.999])
            .enumerate()
            .map(|(cluster, (positions, energy_hartree))| Candidate {
                positions: positions.to_vec(),
                energy_hartree,
                start_energy_hartree: energy_hartree,
                cluster,
                cluster_size: 1,
            })
            .collect();
        let ranked = rank(&topology, candidates, &Settings::default()).unwrap();
        assert_eq!(ranked.len(), 2);
        let expected = ranked[1].rmsd_to_best;
        assert!((expected - 0.568_818_709).abs() < 1e-8, "{expected}");

        let mut path = trajectory(&ranked, &numbers, 0, 2).unwrap();
        let rows = trajectory_rows(&doc, &atoms, &path, -5.);
        assert_eq!(rows[0].rmsd, Some(0.));
        assert_eq!(rows[1].rmsd, Some(expected));
        let (_, sorted) = super::super::ranked_trajectory(Some(&path)).unwrap();
        assert_eq!(sorted[1].statistics.rmsd, Some(expected));

        path.rmsd_to_best_angstrom.clear();
        let legacy = trajectory_rows(&doc, &atoms, &path, -5.);
        assert!(legacy[1].rmsd.unwrap() < 1e-12);
    }

    #[test]
    fn rows_keep_statistics_when_optional_saved_data_is_absent() {
        let positions = [[0.; 3]; 4];
        let mut path = ensemble(&[positions, positions]);
        path.positions_angstrom.clear();
        path.minimizer_energies_hartree.clear();
        path.cluster_sizes.clear();
        let rows = trajectory_rows(&Document::default(), &[None; 4], &path, -5.);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| {
            row.start_kcal.is_none() && row.cluster.is_none() && row.rmsd.is_none()
        }));
    }
}
