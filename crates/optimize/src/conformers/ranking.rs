//! Deduplicate optimized candidates, compute thermal populations, and save aligned geometries.

use reshiki_qcdata::{MAX_SAVED_CONFORMERS, Trajectory};

use super::{Error, Settings, Topology, alignment::Centered, validate_positions};
use crate::KCAL_PER_HARTREE;

/// A reoptimized representative with metadata from its minimized start.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Final coordinates in ångströms, in topology atom order.
    pub positions: Vec<[f64; 3]>,
    pub energy_hartree: f64,
    pub start_energy_hartree: f64,
    pub cluster_size: u32,
}

/// One distinct final geometry, aligned onto the lowest-energy candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub positions: Vec<[f64; 3]>,
    pub energy_hartree: f64,
    pub relative_kcal: f64,
    pub start_energy_hartree: f64,
    /// Sum of the cluster sizes represented by this geometry.
    pub cluster_size: u32,
    pub boltzmann: f64,
    /// Symmetry-aware heavy-atom RMSD in ångströms.
    pub rmsd_to_best: f64,
}

/// Keep the lowest final energy per duplicate group and align survivors onto the best.
pub fn rank(
    topology: &Topology,
    mut candidates: Vec<Candidate>,
    settings: &Settings,
) -> Result<Vec<Ranked>, Error> {
    settings.validate()?;
    for candidate in &candidates {
        validate_positions(topology, &candidate.positions)?;
        if !candidate.energy_hartree.is_finite() || !candidate.start_energy_hartree.is_finite() {
            return Err(Error::Invalid("candidate energies must be finite".into()));
        }
        if candidate.cluster_size == 0 {
            return Err(Error::Invalid("cluster_size must be positive".into()));
        }
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    candidates.sort_by(|a, b| a.energy_hartree.partial_cmp(&b.energy_hartree).unwrap());
    let autos = topology.automorphisms(settings.automorphism_cap);
    let mut kept: Vec<Candidate> = Vec::new();
    let mut centered: Vec<Centered> = Vec::new();
    for candidate in candidates {
        let mobile = Centered::new(topology, &candidate.positions);
        if let Some(i) = centered
            .iter()
            .position(|c| c.distance(&mobile, &autos) < settings.final_dedupe_angstrom)
        {
            kept[i].cluster_size = kept[i]
                .cluster_size
                .checked_add(candidate.cluster_size)
                .ok_or_else(|| Error::Invalid("merged cluster_size exceeds u32".into()))?;
        } else {
            kept.push(candidate);
            centered.push(mobile);
        }
    }
    let energies: Vec<_> = kept.iter().map(|c| c.energy_hartree).collect();
    let minimum = energies[0];
    let weights = normalized_weights(&energies, minimum, settings.temperature_k);
    Ok(kept
        .into_iter()
        .zip(weights)
        .enumerate()
        .map(|(i, (c, boltzmann))| {
            let (positions, rmsd_to_best) = if i == 0 {
                (c.positions, 0.0)
            } else {
                centered[0].overlay(&centered[i], &autos, &c.positions)
            };
            Ranked {
                positions,
                rmsd_to_best,
                boltzmann,
                energy_hartree: c.energy_hartree,
                relative_kcal: (c.energy_hartree - minimum) * KCAL_PER_HARTREE,
                start_energy_hartree: c.start_energy_hartree,
                cluster_size: c.cluster_size,
            }
        })
        .collect())
}

/// Normalized thermal populations in input order, with one state per energy.
///
/// Returns an error if an energy is nonfinite or the temperature is nonfinite
/// or nonpositive.
pub fn boltzmann_weights(energies_hartree: &[f64], temperature_k: f64) -> Result<Vec<f64>, Error> {
    if !temperature_k.is_finite() || temperature_k <= 0.0 {
        return Err(Error::Invalid(
            "temperature must be finite and positive".into(),
        ));
    }
    if energies_hartree.iter().any(|energy| !energy.is_finite()) {
        return Err(Error::Invalid("energies must be finite".into()));
    }
    let minimum = energies_hartree
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    Ok(normalized_weights(energies_hartree, minimum, temperature_k))
}

fn normalized_weights(energies: &[f64], minimum: f64, temperature_k: f64) -> Vec<f64> {
    // Molar gas constant in kcal/(mol K), converted to hartree per kelvin.
    const K_B: f64 = 0.001_987_204_258_640_83 / KCAL_PER_HARTREE;
    // Divide by T first so even the smallest positive T gives weight 1 for the minimum.
    let mut weights: Vec<_> = energies
        .iter()
        .map(|energy| (-(energy - minimum) / temperature_k / K_B).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    for weight in &mut weights {
        *weight /= sum;
    }
    weights
}

/// Save up to the first 32 ranked conformers as a validated qcdata trajectory.
pub fn trajectory(
    ranked: &[Ranked],
    numbers: &[u8],
    charge: i32,
    multiplicity: u32,
) -> Result<Trajectory, Error> {
    let mut saved = Trajectory {
        numbers: numbers.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge,
        multiplicity,
        positions_angstrom: vec![],
        energies_hartree: vec![],
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    };
    for (i, conformer) in ranked.iter().take(MAX_SAVED_CONFORMERS).enumerate() {
        if conformer.positions.len() != numbers.len() {
            return Err(Error::Invalid(format!(
                "atom count differs in ranked conformer {}",
                i + 1
            )));
        }
        saved
            .positions_angstrom
            .extend(conformer.positions.iter().flatten().copied());
        saved.energies_hartree.push(conformer.energy_hartree);
        saved
            .minimizer_energies_hartree
            .push(conformer.start_energy_hartree);
        saved.cluster_sizes.push(conformer.cluster_size);
        saved.labels.push((i + 1).to_string());
    }
    saved
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(saved)
}
