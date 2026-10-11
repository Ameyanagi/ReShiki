//! Geometry-only Rust solver. Calculations never replace the editor graph.
//! Coordinates are in angstroms, energies in kcal/mol, and gradients in
//! kcal/mol/angstrom. Call this synchronous API only in the disposable worker.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Mutex};

/// Version of the independent force-field reference, not a runtime dependency.
pub const RDKIT_VERSION: &str = "2026.03.6";
pub const COSMOLKIT_VERSION: &str = "0.3.0";
/// Absolute structural safety ceiling, separate from machine-budget admission.
pub const MAX_ATOMS: usize = 640;
pub const MAX_COORDINATES: usize = 4096;
pub const MAX_BONDS: usize = 4096;
pub const MAX_CONFORMERS: u32 = 32;
pub const MAX_ITERATIONS: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    pub atoms: usize,
    pub bonds: usize,
    pub coordinates: usize,
}
impl Default for Capacity {
    fn default() -> Self {
        Self {
            atoms: 512,
            bonds: 2048,
            coordinates: 4096,
        }
    }
}
impl Capacity {
    /// Admission envelope retaining the historical 256 MiB / 512-atom anchor.
    /// Above that anchor, pair-table growth needs substantially more headroom:
    /// every additional original atom reserves 2 MiB, up to the validated ceiling.
    /// Eligibility does not guarantee optimization success or measured usage.
    pub fn for_heap(heap_bytes: usize) -> Self {
        let reference_heap = 256 * 1024 * 1024;
        let base = (heap_bytes / (512 * 1024)).min(512);
        let additional =
            (heap_bytes.saturating_sub(reference_heap) / (2 * 1024 * 1024)).min(MAX_ATOMS - 512);
        let atoms = base.saturating_add(additional).min(MAX_ATOMS);
        Self {
            atoms,
            bonds: atoms.saturating_mul(4).min(MAX_BONDS),
            coordinates: atoms.saturating_mul(8).min(MAX_COORDINATES),
        }
    }
    pub fn validate(self) -> Result<(), String> {
        if self.atoms == 0
            || self.atoms > MAX_ATOMS
            || self.bonds == 0
            || self.bonds > MAX_BONDS
            || self.coordinates < self.atoms
            || self.coordinates > MAX_COORDINATES
        {
            return Err("Geometry capacity exceeds the validated structural safety ceiling".into());
        }
        Ok(())
    }
}
// Keep deterministic seeded sampling isolated from concurrent solver calls.
static SOLVER_OPERATION: Mutex<()> = Mutex::new(());
mod cage;
mod solver;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)] // Preserve the scientific force-field names.
pub enum ForceField {
    MMFF94,
    #[serde(rename = "MMFF94s")]
    MMFF94s,
    UFF,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    Generate,
    Relax,
    Evaluate,
}

/// Tetrahedral tags use RDKit's bond insertion order: 0 unspecified, 1 CW, 2 CCW.
/// The caller must translate its graph's neighbor order before setting a tag.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomInput {
    pub atomic_number: u32,
    pub isotope: u32,
    pub charge: i32,
    pub explicit_h: u32,
    pub no_implicit: bool,
    pub aromatic: bool,
    pub radical: u32,
    pub chiral_tag: u32,
}

/// `order` and `stereo` use RDKit enum values. The solver supports 1 single,
/// 2 double, 3 triple, and 12 aromatic. The wire recognizes 17 dative so the
/// solver can report that coordination bonds are unsupported.
/// Stereo is 0 none, 1 unspecified, 2 Z, 3 E, 4 cis, or 5 trans. Its two
/// reference atom indices refer to neighbors of a and b, respectively.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BondInput {
    pub a: usize,
    pub b: usize,
    pub order: u32,
    pub aromatic: bool,
    pub stereo: u32,
    pub stereo_atoms: Option<[usize; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub atoms: Vec<AtomInput>,
    pub bonds: Vec<BondInput>,
    pub field: ForceField,
    pub operation: Operation,
    /// Generate accepts either no coordinates or an optional original-atom 3D
    /// starting geometry. Relax/Evaluate require original+added-H coordinates.
    pub coordinates: Vec<[f64; 3]>,
    pub fixed_atoms: Vec<usize>,
    pub conformers: u32,
    pub seed: i32,
    pub max_iterations: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub coordinates: Vec<[f64; 3]>,
    /// One original parent index for each hydrogen appended after original_count.
    pub hydrogen_parents: Vec<usize>,
    pub original_count: usize,
    pub initial_energy: f64,
    pub energy: f64,
    pub gradient: Option<Vec<[f64; 3]>>,
    pub converged: bool,
    pub field: ForceField,
    pub diagnostics: Vec<String>,
}

fn valid_coordinate(p: &[f64; 3]) -> bool {
    p.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0)
}

impl Request {
    pub fn validate_capacity(&self, capacity: Capacity) -> Result<(), String> {
        capacity.validate()?;
        if self.atoms.len() > capacity.atoms
            || self.bonds.len() > capacity.bonds
            || self.coordinates.len() > capacity.coordinates
        {
            return Err("Geometry request exceeds its machine-budget capacity".into());
        }
        self.validate()
    }
    pub fn validate(&self) -> Result<(), String> {
        let n = self.atoms.len();
        if n == 0 || n > MAX_ATOMS || self.bonds.len() > MAX_BONDS {
            return Err("Geometry atom/bond limit exceeded".into());
        }
        if self.conformers == 0
            || self.conformers > MAX_CONFORMERS
            || self.max_iterations == 0
            || self.max_iterations > MAX_ITERATIONS
            || self.seed < 0
        {
            return Err("Invalid geometry sampling/minimization limits".into());
        }
        if self.atoms.iter().any(|a| {
            a.atomic_number == 0
                || a.atomic_number > 118
                || a.isotope > 500
                || !(-16..=16).contains(&a.charge)
                || a.explicit_h > 8
                || a.radical > 8
                || a.chiral_tag > 2
        }) {
            return Err("Invalid geometry atom input".into());
        }
        let mut edges = BTreeSet::new();
        for b in &self.bonds {
            if b.a >= n
                || b.b >= n
                || b.a == b.b
                || !matches!(b.order, 1 | 2 | 3 | 12 | 17)
                || b.stereo > 5
                || !edges.insert((b.a.min(b.b), b.a.max(b.b)))
            {
                return Err("Invalid geometry bond input".into());
            }
            if b.stereo > 1 && (b.order != 2 || b.stereo_atoms.is_none()) {
                return Err("Specified double-bond stereo requires two reference atoms".into());
            }
            if let Some([a, c]) = b.stereo_atoms
                && (a >= n || c >= n || a == c || a == b.a || a == b.b || c == b.a || c == b.b)
            {
                return Err("Invalid geometry stereo reference".into());
            }
        }
        for b in &self.bonds {
            if let Some([a, c]) = b.stereo_atoms
                && (!edges.contains(&(a.min(b.a), a.max(b.a)))
                    || !edges.contains(&(c.min(b.b), c.max(b.b))))
            {
                return Err("Geometry stereo references must be bonded neighbors".into());
            }
        }
        if self.coordinates.len() > MAX_COORDINATES
            || (self.operation != Operation::Generate
                && !self.coordinates.iter().all(valid_coordinate))
        {
            return Err("Invalid geometry coordinate input".into());
        }
        match self.operation {
            Operation::Generate if !self.fixed_atoms.is_empty() => {
                return Err("Geometry generation cannot receive fixed atoms".into());
            }
            Operation::Generate if !self.coordinates.is_empty() && self.coordinates.len() != n => {
                return Err("Geometry generation seed requires original-atom coordinates".into());
            }
            Operation::Relax | Operation::Evaluate if self.coordinates.len() < n => {
                return Err("Geometry operation requires original and added-H coordinates".into());
            }
            Operation::Evaluate if !self.fixed_atoms.is_empty() => {
                return Err("Energy evaluation does not accept fixed atoms".into());
            }
            _ => (),
        }
        let fixed: BTreeSet<_> = self.fixed_atoms.iter().copied().collect();
        if fixed.len() != self.fixed_atoms.len()
            || fixed.iter().any(|&i| i >= self.coordinates.len())
        {
            return Err("Invalid or duplicate fixed geometry atom".into());
        }
        Ok(())
    }
}

/// Execute one bounded operation using COSMolKit's Rust force fields and
/// distance geometry. Only owned data leaves the disposable worker.
pub fn solve(request: &Request) -> Result<Response, String> {
    request.validate()?;
    let _guard = SOLVER_OPERATION
        .lock()
        .map_err(|_| "Geometry solver lock poisoned")?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| solver::solve(request)))
        .map_err(|_| "Rust geometry solver panicked".to_owned())?
}

#[cfg(test)]
mod tests;
