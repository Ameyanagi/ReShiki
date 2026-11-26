//! Conformers in energy order, with thermal populations and XYZ labels.

use reshiki_io::qc_input::write_xyz_trajectory;
use reshiki_qcdata::{Frame, Trajectory};

use super::KCAL_PER_HARTREE;
const RT_KCAL: f64 = 0.001_987_204_258_640_832 * 298.15;
const NO_SAVED_POSITIONS: &str = "This result has no saved conformer positions.";

/// Boltzmann fractions in input order for finite energies at 298.15 K.
pub fn populations(energies_hartree: &[f64]) -> Vec<f64> {
    let minimum = energies_hartree
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let mut weights: Vec<_> = energies_hartree
        .iter()
        .map(|energy| (-(energy - minimum) * KCAL_PER_HARTREE / RT_KCAL).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    for weight in &mut weights {
        *weight /= total;
    }
    weights
}

/// Export every saved conformer as XYZ, lowest energy first.
pub fn xyz(path: Option<&Trajectory>, title: &str) -> Result<String, String> {
    let (path, ranked) = ranked_trajectory(path)?;
    let labeled = Trajectory {
        numbers: path.numbers.clone(),
        isotopes: path.isotopes.clone(),
        masses_amu: path.masses_amu.clone(),
        charge: path.charge,
        multiplicity: path.multiplicity,
        positions_angstrom: ranked
            .iter()
            .flat_map(|conformer| conformer.frame.positions_angstrom.iter().flatten().copied())
            .collect(),
        // The label supplies the entire comment, including the energy.
        energies_hartree: vec![],
        labels: ranked
            .iter()
            .enumerate()
            .map(|(index, conformer)| {
                let rank = index + 1;
                let RankedConformer {
                    step,
                    energy,
                    delta_kcal,
                    percent,
                    ..
                } = conformer;
                let cluster = path
                    .cluster_sizes
                    .get(*step)
                    .map_or_else(String::new, |size| format!(" · cluster {size}"));
                // charge= and mult= as extended XYZ reads them back; the
                // energy last, as ReShiki and xtb tools read it.
                format!(
                    "{title} conformer {rank} · ΔE {delta_kcal:.2} kcal/mol · {percent:.1} %{cluster} · charge={} mult={} · E = {energy:.10} Eh",
                    path.charge, path.multiplicity
                )
            })
            .collect(),
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    };
    write_xyz_trajectory(&labeled, title).map_err(|error| error.to_string())
}

pub struct RankedConformer {
    pub step: usize,
    pub frame: Frame,
    pub energy: f64,
    pub delta_kcal: f64,
    pub percent: f64,
}

pub fn ranked_trajectory(
    path: Option<&Trajectory>,
) -> Result<(&Trajectory, Vec<RankedConformer>), String> {
    let path = path
        .filter(|path| !path.positions_angstrom.is_empty())
        .ok_or(NO_SAVED_POSITIONS)?;
    path.validate()
        .map_err(|error| format!("The saved conformers could not be exported ({error})."))?;
    let energies = &path.energies_hartree;
    let minimum = energies
        .iter()
        .copied()
        .min_by(f64::total_cmp)
        .ok_or("This result has no saved conformer energies.")?;
    let mut ranked = energies
        .iter()
        .copied()
        .zip(populations(energies))
        .enumerate()
        .map(|(step, (energy, fraction))| {
            Ok(RankedConformer {
                step,
                frame: path.frame(step).ok_or(NO_SAVED_POSITIONS)?,
                energy,
                delta_kcal: (energy - minimum) * KCAL_PER_HARTREE,
                percent: fraction * 100.0,
            })
        })
        .collect::<Result<Vec<_>, &str>>()?;
    ranked.sort_by(|a, b| a.energy.total_cmp(&b.energy));
    Ok((path, ranked))
}
