//! Geometry-only Rust solver. Calculations never replace the editor graph.
//! Coordinates are in angstroms, energies in kcal/mol, and gradients in
//! kcal/mol/angstrom. Call this synchronous API only in the disposable worker.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Mutex};

/// Version of the independent force-field reference, not a runtime dependency.
pub const RDKIT_VERSION: &str = "2026.03.6";
pub const COSMOLKIT_VERSION: &str = "0.3.0";
pub const MAX_ATOMS: usize = 512;
pub const MAX_COORDINATES: usize = 4096;
pub const MAX_BONDS: usize = 2048;
pub const MAX_CONFORMERS: u32 = 32;
pub const MAX_ITERATIONS: u32 = 10_000;
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
mod tests {
    use super::*;

    fn ethanol(field: ForceField) -> Request {
        Request {
            atoms: [6, 6, 8]
                .into_iter()
                .map(|atomic_number| AtomInput {
                    atomic_number,
                    isotope: 0,
                    charge: 0,
                    explicit_h: 0,
                    no_implicit: false,
                    aromatic: false,
                    radical: 0,
                    chiral_tag: 0,
                })
                .collect(),
            bonds: [(0, 1), (1, 2)]
                .into_iter()
                .map(|(a, b)| BondInput {
                    a,
                    b,
                    order: 1,
                    aromatic: false,
                    stereo: 0,
                    stereo_atoms: None,
                })
                .collect(),
            field,
            operation: Operation::Generate,
            coordinates: Vec::new(),
            fixed_atoms: Vec::new(),
            conformers: 2,
            seed: 61453,
            max_iterations: 500,
        }
    }

    fn molecule(numbers: &[u32], edges: &[(usize, usize, u32)], field: ForceField) -> Request {
        let mut request = ethanol(field);
        request.atoms = numbers
            .iter()
            .map(|&atomic_number| AtomInput {
                atomic_number,
                isotope: 0,
                charge: 0,
                explicit_h: 0,
                no_implicit: false,
                aromatic: false,
                radical: 0,
                chiral_tag: 0,
            })
            .collect();
        request.bonds = edges
            .iter()
            .map(|&(a, b, order)| BondInput {
                a,
                b,
                order,
                aromatic: order == 12,
                stereo: 0,
                stereo_atoms: None,
            })
            .collect();
        request.max_iterations = 1000;
        request
    }

    fn evaluate(request: &Request, coordinates: Vec<[f64; 3]>) -> Response {
        let mut evaluation = request.clone();
        evaluation.operation = Operation::Evaluate;
        evaluation.coordinates = coordinates;
        evaluation.fixed_atoms.clear();
        solve(&evaluation).unwrap()
    }

    fn close(actual: f64, expected: f64, absolute: f64, relative: f64) {
        assert!(
            (actual - expected).abs() <= absolute + relative * expected.abs(),
            "actual={actual}, expected={expected}"
        );
    }

    #[test]
    fn rust_results_preserve_hydrogen_mapping_and_gradient_for_every_field() {
        for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
            let mut request = ethanol(field);
            let generated = solve(&request).unwrap();
            assert_eq!(generated.original_count, 3);
            assert_eq!(generated.coordinates.len(), 9);
            assert_eq!(generated.hydrogen_parents, [0, 0, 0, 1, 1, 2]);
            assert!(generated.energy <= generated.initial_energy + 1e-6);
            request.operation = Operation::Evaluate;
            request.coordinates = generated.coordinates;
            request.coordinates[0][0] += 0.15;
            let evaluated = solve(&request).unwrap();
            let gradient = evaluated.gradient.unwrap();
            assert_eq!(gradient.len(), request.coordinates.len());
            let step = 1e-5;
            for (index, force) in gradient.iter().enumerate() {
                for (axis, &analytic) in force.iter().enumerate() {
                    let mut plus = request.coordinates.clone();
                    let mut minus = plus.clone();
                    plus[index][axis] += step;
                    minus[index][axis] -= step;
                    let numerical = (evaluate(&request, plus).energy
                        - evaluate(&request, minus).energy)
                        / (2.0 * step);
                    close(analytic, numerical, 2e-5, 2e-6);
                }
            }

            let transformed = request
                .coordinates
                .iter()
                .map(|&[x, y, z]| [-y + 6.5, x - 2.0, z + 1.25])
                .collect();
            let rotated = evaluate(&request, transformed);
            close(rotated.energy, evaluated.energy, 1e-7, 1e-9);
            let rotated_gradient = rotated.gradient.unwrap();
            for (original, rotated) in gradient.iter().zip(rotated_gradient) {
                let expected = [-original[1], original[0], original[2]];
                for axis in 0..3 {
                    close(rotated[axis], expected[axis], 1e-6, 1e-8);
                }
            }
        }
    }

    #[test]
    fn rust_relax_keeps_original_and_added_hydrogen_fixed() {
        for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
            let mut request = ethanol(field);
            request.coordinates = solve(&request).unwrap().coordinates;
            request.operation = Operation::Relax;
            request.max_iterations = 20;
            request.fixed_atoms = vec![0, 3];
            request.coordinates[0][0] += 0.04;
            let result = solve(&request).unwrap();
            for &i in &request.fixed_atoms {
                assert_eq!(result.coordinates[i], request.coordinates[i]);
            }
            assert!(result.energy <= result.initial_energy + 1e-6);
        }
    }

    #[test]
    fn seeded_sampling_is_repeatable_and_does_not_substitute_force_fields() {
        for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
            let request = ethanol(field);
            let first = solve(&request).unwrap();
            let repeated = solve(&request).unwrap();
            assert_eq!(first.field, field);
            close(first.energy, repeated.energy, 1e-8, 0.);
            for (first, repeated) in first.coordinates.iter().zip(repeated.coordinates) {
                for axis in 0..3 {
                    close(first[axis], repeated[axis], 1e-8, 0.);
                }
            }
        }
        let mut borane = molecule(&[5], &[], ForceField::UFF);
        let supported = solve(&borane).unwrap();
        assert_eq!(supported.field, ForceField::UFF);
        assert_eq!(supported.hydrogen_parents, [0, 0, 0]);
        for field in [ForceField::MMFF94, ForceField::MMFF94s] {
            borane.field = field;
            assert!(solve(&borane).unwrap_err().contains("MMFF parameters"));
        }
    }

    #[test]
    fn generation_seed_contract_and_invalid_geometry_fallback_are_bounded() {
        let mut request = ethanol(ForceField::UFF);
        request.coordinates = vec![[0., 0., 0.]; 2];
        assert!(request.validate().unwrap_err().contains("original-atom"));
        request.coordinates = vec![[0., 0., 0.]; 3];
        request.fixed_atoms = vec![0];
        assert!(request.validate().unwrap_err().contains("fixed atoms"));
        request.fixed_atoms.clear();
        for invalid in [
            vec![[0., 0., 0.]; 3],
            vec![[f64::NAN, 0., 0.]; 3],
            vec![[0., 0., 0.], [1.5, 0., 1.5], [2., 1., 3.]],
        ] {
            request.coordinates = invalid;
            let generated = solve(&request).unwrap();
            assert_eq!(generated.hydrogen_parents, [0, 0, 0, 1, 1, 2]);
            assert_eq!(generated.coordinates.len(), 9);
            assert!(
                generated.diagnostics[0].starts_with("existing original-atom coordinates ignored:")
            );
            assert!(generated.diagnostics[0].len() < 600);
            assert!(
                generated
                    .diagnostics
                    .iter()
                    .any(|line| line.contains("Rust ETKDGv3"))
            );
        }
    }

    #[test]
    fn existing_3d_seed_adds_temporary_hydrogens_and_preserves_tetrahedral_stereo() {
        let mut request = molecule(
            &[6, 9, 17, 35],
            &[(0, 1, 1), (0, 2, 1), (0, 3, 1)],
            ForceField::MMFF94s,
        );
        request.atoms[0].chiral_tag = 1;
        let embedded = solve(&request).unwrap();
        request.coordinates = embedded.coordinates[..request.atoms.len()].to_vec();
        let seeded = solve(&request).unwrap();
        assert_eq!(seeded.hydrogen_parents, [0]);
        assert_eq!(seeded.coordinates.len(), 5);
        assert!(seeded.diagnostics[0].starts_with("initialization=existing-3d;"));
        assert!(seeded.energy <= seeded.initial_energy + 1e-6);
        evaluate(&request, seeded.coordinates.clone());
        let repeated = solve(&request).unwrap();
        close(seeded.energy, repeated.energy, 1e-8, 0.);
        for (first, second) in seeded.coordinates.iter().zip(repeated.coordinates) {
            for axis in 0..3 {
                close(first[axis], second[axis], 1e-8, 0.);
            }
        }
        for point in &mut request.coordinates {
            point[2] = -point[2];
        }
        let corrected = solve(&request).unwrap();
        assert!(
            corrected.diagnostics[0].starts_with("existing original-atom coordinates ignored:")
        );
        assert!(corrected.diagnostics[0].contains("tetrahedral stereo"));
        evaluate(&request, corrected.coordinates);
        assert_eq!(request.atoms[0].chiral_tag, 1);
    }

    #[test]
    fn aromatic_hydrogens_and_amide_variant_parameters_are_preserved() {
        let mut benzene = molecule(
            &[6; 6],
            &[
                (0, 1, 12),
                (1, 2, 12),
                (2, 3, 12),
                (3, 4, 12),
                (4, 5, 12),
                (5, 0, 12),
            ],
            ForceField::MMFF94,
        );
        for atom in &mut benzene.atoms {
            atom.aromatic = true;
        }
        for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
            benzene.field = field;
            let result = solve(&benzene).unwrap();
            assert_eq!(result.coordinates.len(), 12);
            assert_eq!(result.hydrogen_parents, [0, 1, 2, 3, 4, 5]);
            assert!(result.energy <= result.initial_energy + 1e-6);
        }

        let mut amide = molecule(
            &[6, 6, 8, 7],
            &[(0, 1, 1), (1, 2, 2), (1, 3, 1)],
            ForceField::MMFF94,
        );
        let mut displaced = solve(&amide).unwrap().coordinates;
        // Displace nitrogen normal to the carbonyl plane, independent of the
        // embedded conformer's orientation, to exercise the improper terms.
        let a: [f64; 3] = std::array::from_fn(|i| displaced[0][i] - displaced[1][i]);
        let b: [f64; 3] = std::array::from_fn(|i| displaced[2][i] - displaced[1][i]);
        let normal = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = normal.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(length > 1e-8);
        for (coordinate, direction) in displaced[3].iter_mut().zip(normal) {
            *coordinate += 0.5 * direction / length;
        }
        let mmff94 = evaluate(&amide, displaced.clone()).energy;
        amide.field = ForceField::MMFF94s;
        let mmff94s = evaluate(&amide, displaced).energy;
        assert!((mmff94 - mmff94s).abs() > 1e-3);
    }

    #[test]
    fn tetrahedral_stereo_with_implicit_and_original_isotope_hydrogen_is_checked() {
        for tag in [1, 2] {
            let mut request = molecule(
                &[6, 9, 17, 35],
                &[(0, 1, 1), (0, 2, 1), (0, 3, 1)],
                ForceField::MMFF94s,
            );
            request.atoms[0].chiral_tag = tag;
            let generated = solve(&request).unwrap();
            assert_eq!(generated.hydrogen_parents, [0]);
            evaluate(&request, generated.coordinates.clone());
            request.operation = Operation::Evaluate;
            request.coordinates = generated.coordinates;
            for point in &mut request.coordinates {
                point[2] = -point[2];
            }
            assert!(solve(&request).unwrap_err().contains("tetrahedral stereo"));
            for point in &mut request.coordinates {
                point[2] = 0.;
            }
            assert!(solve(&request).is_err());
        }
        // Explicit deuterium is an original atom, not a disposable calculation H.
        let mut original_hydrogen = molecule(
            &[1, 6, 9, 17, 35],
            &[(0, 1, 1), (1, 2, 1), (1, 3, 1), (1, 4, 1)],
            ForceField::MMFF94,
        );
        original_hydrogen.atoms[0].isotope = 2;
        original_hydrogen.atoms[0].no_implicit = true;
        original_hydrogen.atoms[1].chiral_tag = 1;
        let result = solve(&original_hydrogen).unwrap();
        assert_eq!(result.original_count, 5);
        assert_eq!(result.coordinates.len(), 5);
        assert!(result.hydrogen_parents.is_empty());
        evaluate(&original_hydrogen, result.coordinates);
    }

    #[test]
    fn e_z_and_cis_trans_references_reject_inversion_and_degeneracy() {
        for stereo in [2, 3, 4, 5] {
            let mut request = molecule(
                &[6, 6, 6, 6],
                &[(0, 1, 1), (1, 2, 2), (2, 3, 1)],
                ForceField::MMFF94,
            );
            request.bonds[1].stereo = stereo;
            request.bonds[1].stereo_atoms = Some([0, 3]);
            let generated = solve(&request).unwrap();
            evaluate(&request, generated.coordinates.clone());
            request.operation = Operation::Evaluate;
            request.coordinates = generated.coordinates;
            // Rotate one designated substituent by 180 degrees around the bond.
            let axis: [f64; 3] =
                std::array::from_fn(|i| request.coordinates[2][i] - request.coordinates[1][i]);
            let reference: [f64; 3] =
                std::array::from_fn(|i| request.coordinates[3][i] - request.coordinates[2][i]);
            let axis2: f64 = axis.iter().map(|x| x * x).sum();
            let projection: f64 =
                axis.iter().zip(reference).map(|(a, b)| a * b).sum::<f64>() / axis2;
            request.coordinates[3] = std::array::from_fn(|i| {
                request.coordinates[2][i] + 2. * projection * axis[i] - reference[i]
            });
            assert!(solve(&request).is_err());
            request.coordinates[0] = request.coordinates[1];
            assert!(solve(&request).is_err());
        }
    }

    #[test]
    fn evaluation_rejects_collapsed_stretched_and_overlapping_geometry() {
        let mut request = ethanol(ForceField::MMFF94);
        let generated = solve(&request).unwrap();
        request.operation = Operation::Evaluate;
        request.coordinates = generated.coordinates.clone();
        request.coordinates[0][0] += 10.;
        assert!(solve(&request).is_err());
        request.coordinates = generated.coordinates.clone();
        request.coordinates[0] = request.coordinates[1];
        assert!(solve(&request).is_err());
        request.coordinates = generated.coordinates;
        request.coordinates[3] = request.coordinates[8];
        assert!(solve(&request).is_err());
        request.fixed_atoms.push(0);
        assert!(solve(&request).unwrap_err().contains("fixed atoms"));
    }

    #[test]
    fn invalid_requests_and_domain_errors_leave_solver_usable() {
        let mut request = ethanol(ForceField::UFF);
        request.bonds[0].a = usize::MAX;
        assert!(solve(&request).unwrap_err().contains("bond"));
        let mut request = ethanol(ForceField::MMFF94);
        request.atoms[0].atomic_number = 30;
        assert!(!solve(&request).unwrap_err().is_empty());
        let mut request = ethanol(ForceField::UFF);
        request.atoms[0].radical = 1;
        assert!(solve(&request).unwrap_err().contains("radicals"));
        let mut request = ethanol(ForceField::UFF);
        request.bonds[0].order = 17;
        assert!(solve(&request).unwrap_err().contains("coordination"));
        let mut request = ethanol(ForceField::UFF);
        request.bonds.pop();
        assert!(solve(&request).unwrap_err().contains("connected"));
        // A domain failure must not poison subsequent solver operations.
        assert!(solve(&ethanol(ForceField::MMFF94)).is_ok());
    }
}
