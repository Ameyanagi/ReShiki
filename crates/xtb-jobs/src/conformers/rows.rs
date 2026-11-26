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

/// Statistics in trajectory order, including RMSD using the first saved
/// conformer's topology. Drawing bonds keep their atom mapping; each model
/// hydrogen is attached to its nearest linked atom in that saved conformer.
pub fn trajectory_rows(
    doc: &Document,
    atoms: &[Option<u64>],
    path: &Trajectory,
) -> Vec<RowStatistics> {
    let mut rows = statistics(path);
    measure_rmsds(doc, atoms, path, &mut rows);
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
    // Keep the browser's existing display: it has no conformer fitting.
    #[cfg(target_arch = "wasm32")]
    let _ = (doc, atoms, path, rows);
}

pub(super) fn statistics(path: &Trajectory) -> Vec<RowStatistics> {
    let minimum = path
        .energies_hartree
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
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
            delta_kcal: (energy - minimum) * KCAL_PER_HARTREE,
            start_kcal: path
                .minimizer_energies_hartree
                .get(index)
                .map(|energy| (energy - start_minimum) * KCAL_PER_HARTREE),
            percent: fraction * 100.0,
            rmsd: None,
            cluster: path.cluster_sizes.get(index).copied(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use reshiki_model::document::Point;
    use reshiki_optimize::conformers::{Ranked, trajectory};
    #[cfg(not(target_arch = "wasm32"))]
    use reshiki_optimize::conformers::{Settings, Topology, rmsd};

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
        }
    }

    #[test]
    fn rows_and_exports_share_retained_ensemble_statistics() {
        let positions = [[0., 0., 0.], [1., 0., 0.], [1., 2., 0.], [-0.8, 0., 0.]];
        let path = ensemble(&[positions, positions]);
        let rows = trajectory_rows(&Document::default(), &[None; 4], &path);
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
        let rows = trajectory_rows(&Document::default(), &[None], &path);
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
        let rows = trajectory_rows(&doc, &atoms, &path);
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
    fn rows_keep_statistics_when_optional_saved_data_is_absent() {
        let positions = [[0.; 3]; 4];
        let mut path = ensemble(&[positions, positions]);
        path.positions_angstrom.clear();
        path.minimizer_energies_hartree.clear();
        path.cluster_sizes.clear();
        let rows = trajectory_rows(&Document::default(), &[None; 4], &path);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| {
            row.start_kcal.is_none() && row.cluster.is_none() && row.rmsd.is_none()
        }));
    }
}
