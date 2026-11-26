//! Conformer search settings and the desktop and Python pipeline.

use crate::Failure;
use reshiki_qcdata::{MAX_HESSIAN_ATOMS, XtbMethod};

mod export;
pub use export::{RankedConformer, ranked_trajectory, xyz};
mod rows;
pub use rows::{RowStatistics, populations, trajectory_rows};
#[cfg(not(target_arch = "wasm32"))]
mod pipeline;
#[cfg(not(target_arch = "wasm32"))]
pub use pipeline::{Found, Search, generate_model, search_conformers, starting_force_field};

/// Starts ETKDG may make; the engine's limit.
pub const MAX_STARTS: u32 = 300;
/// xTB re-ranking (and so the whole search) works up to this many atoms.
pub const MAX_ATOMS: usize = MAX_HESSIAN_ATOMS;
/// GFN2-xTB may minimize every start only up to this many atoms.
pub const MAX_GFN2_START_ATOMS: usize = 120;
/// Hartree in thermochemical kcal/mol (CODATA 2022).
pub const KCAL_PER_HARTREE: f64 = 627.509_474_063;
/// Representatives xTB re-ranks at most.
pub const MAX_REPRESENTATIVES: usize = 60;
/// xTB jobs a search may run concurrently.
pub const MAX_CONCURRENT_JOBS: u32 = 8;

/// What relaxes the ETKDG starts before clustering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Minimizer {
    #[default]
    Mmff94s,
    Mmff94,
    Uff,
    GfnFf,
    Gfn2,
}

impl Minimizer {
    pub const ALL: [Self; 5] = [
        Self::Mmff94s,
        Self::Mmff94,
        Self::Uff,
        Self::GfnFf,
        Self::Gfn2,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mmff94s => "MMFF94s",
            Self::Mmff94 => "MMFF94",
            Self::Uff => "UFF",
            Self::GfnFf => "GFN-FF",
            Self::Gfn2 => "GFN2-xTB",
        }
    }

    /// The xTB method that minimizes every start, for GFN-FF and GFN2-xTB.
    pub fn xtb(self) -> Option<XtbMethod> {
        match self {
            Self::GfnFf => Some(XtbMethod::GfnFf),
            Self::Gfn2 => Some(XtbMethod::Gfn2Xtb),
            _ => None,
        }
    }

    /// An xTB minimizer sets the method for the whole search.
    pub fn effective_method(self, reranker: XtbMethod) -> XtbMethod {
        self.xtb().unwrap_or(reranker)
    }
}

impl std::fmt::Display for Minimizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Clustering threshold, as the card names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Diversity {
    Fine,
    #[default]
    Normal,
    Coarse,
}

impl Diversity {
    pub const ALL: [Self; 3] = [Self::Fine, Self::Normal, Self::Coarse];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fine => "fine",
            Self::Normal => "normal",
            Self::Coarse => "coarse",
        }
    }

    /// Heavy-atom RMSD cutoff in Å, as the engine uses it.
    pub fn angstrom(self) -> f64 {
        match self {
            Self::Fine => 0.5,
            Self::Normal => 0.75,
            Self::Coarse => 1.25,
        }
    }
}

/// A search as it runs: the card's fields, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub starts: u32,
    pub minimizer: Minimizer,
    pub diversity: Diversity,
    pub window_kcal: f64,
    pub representatives: usize,
    pub three: bool,
}

impl Config {
    pub fn new(
        starts: u32,
        minimizer: Minimizer,
        diversity: Diversity,
        window_kcal: f64,
        representatives: usize,
        per_cluster: u8,
    ) -> Result<Self, String> {
        if !(1..=MAX_STARTS).contains(&starts) {
            return Err(format!(
                "Starting structures must be a whole number from 1 to {MAX_STARTS}"
            ));
        }
        if !(0.1..=100.).contains(&window_kcal) {
            return Err("Energy window must be from 0.1 to 100 kcal/mol".into());
        }
        if !(1..=MAX_REPRESENTATIVES).contains(&representatives) {
            return Err(format!(
                "Representatives must be a whole number from 1 to {MAX_REPRESENTATIVES}"
            ));
        }
        if !matches!(per_cluster, 1 | 3) {
            return Err("Conformers per cluster must be 1 or 3".into());
        }
        let rerank = minimizer.xtb().is_none();
        Ok(Self {
            starts,
            minimizer,
            diversity,
            window_kcal,
            // Without re-ranking every cluster keeps its lowest member.
            representatives: if rerank {
                representatives
            } else {
                MAX_REPRESENTATIVES
            },
            three: rerank && per_cluster == 3,
        })
    }

    pub fn check_atoms(&self, count: usize) -> Result<(), Failure> {
        if count > MAX_ATOMS {
            return Err(Failure::TooMany {
                count,
                limit: MAX_ATOMS,
            });
        }
        if self.minimizer == Minimizer::Gfn2 && count > MAX_GFN2_START_ATOMS {
            return Err(Failure::Other(format!(
                "GFN2-xTB minimizes every start up to {MAX_GFN2_START_ATOMS} atoms; choose GFN-FF or MMFF94s"
            )));
        }
        Ok(())
    }

    /// The method line's search part, kept as the result's method name:
    /// "MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank".
    pub fn describe(&self, method: XtbMethod) -> String {
        let mut text = format!(
            "{} ETKDG {} starts · Butina {} Å",
            self.minimizer.label(),
            self.starts,
            self.diversity.angstrom()
        );
        if self.minimizer.xtb().is_none() {
            text.push_str(&format!(" · {} re-rank", method.label()));
        }
        text
    }
}

/// What the running search is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// The geometry worker embeds (and, with a force field, minimizes) starts.
    #[default]
    Starts,
    /// Valid starts so far of those asked for.
    Sampled {
        done: usize,
        total: usize,
    },
    /// xTB minimizes every start (GFN-FF, GFN2-xTB).
    Minimizing {
        done: usize,
        total: usize,
    },
    Clustering,
    /// xTB optimizes the representatives.
    Reranking {
        done: usize,
        total: usize,
    },
}
