//! Cluster minimized starts by molecular symmetry and rank the reoptimized representatives.

mod alignment;
mod clustering;
mod ranking;
mod topology;

#[cfg(test)]
mod tests;

pub use alignment::{rmsd, superpose};
pub use clustering::{Clustering, Start, cluster};
pub use ranking::{Candidate, Ranked, boltzmann_weights, rank, trajectory};
pub use topology::{Automorphisms, Topology};

/// Maximum number of minimized starts in a conformer search.
pub const MAX_STARTS: u32 = 300;

/// Heavy-atom RMSD cutoff for Butina clustering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diversity {
    Fine,
    Normal,
    Coarse,
}

impl Diversity {
    /// Inclusive neighbor cutoff in ångströms.
    pub fn threshold_angstrom(self) -> f64 {
        match self {
            Self::Fine => 0.5,
            Self::Normal => 0.75,
            Self::Coarse => 1.25,
        }
    }
}

/// Maximum number of representatives taken from each cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerCluster {
    One,
    Three,
}

/// Conformer selection, final deduplication and population settings.
#[derive(Clone, Debug)]
pub struct Settings {
    pub diversity: Diversity,
    pub per_cluster: PerCluster,
    /// Maximum energy above the lowest minimized start, in kcal/mol.
    pub window_kcal: f64,
    /// Total budget for reoptimization, from 1 to 60.
    pub max_representatives: usize,
    pub final_dedupe_angstrom: f64,
    pub temperature_k: f64,
    /// Enumeration cap before adding missing inverses. At most twice this many
    /// mappings are retained to keep truncated RMSD comparisons symmetric.
    pub automorphism_cap: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            diversity: Diversity::Normal,
            per_cluster: PerCluster::One,
            window_kcal: 15.0,
            max_representatives: 20,
            final_dedupe_angstrom: 0.25,
            temperature_k: 298.15,
            automorphism_cap: 128,
        }
    }
}

impl Settings {
    fn validate(&self) -> Result<(), Error> {
        if !self.window_kcal.is_finite() || self.window_kcal < 0.0 {
            return Err(Error::Invalid(
                "window_kcal must be finite and nonnegative".into(),
            ));
        }
        if !(1..=60).contains(&self.max_representatives) {
            return Err(Error::Invalid(
                "max_representatives must be in 1..=60".into(),
            ));
        }
        if !self.final_dedupe_angstrom.is_finite() || self.final_dedupe_angstrom <= 0.0 {
            return Err(Error::Invalid(
                "final_dedupe_angstrom must be finite and positive".into(),
            ));
        }
        if !self.temperature_k.is_finite() || self.temperature_k <= 0.0 {
            return Err(Error::Invalid(
                "temperature_k must be finite and positive".into(),
            ));
        }
        if self.automorphism_cap == 0 {
            return Err(Error::Invalid("automorphism_cap must be positive".into()));
        }
        Ok(())
    }
}

/// Invalid input to a conformer operation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
}

fn validate_positions(topology: &Topology, positions: &[[f64; 3]]) -> Result<(), Error> {
    if positions.len() != topology.atom_count() {
        return Err(Error::Invalid(
            "coordinate count differs from topology".into(),
        ));
    }
    if positions.iter().flatten().any(|x| !x.is_finite()) {
        return Err(Error::Invalid("coordinates must be finite".into()));
    }
    Ok(())
}
