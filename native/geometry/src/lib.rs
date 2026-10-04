//! Geometry-only RDKit bridge. Native chemistry never replaces the editor graph.
//! Coordinates are in angstroms, energies in kcal/mol, and gradients in
//! kcal/mol/angstrom. Call this synchronous API only in the disposable worker.
#![deny(unsafe_op_in_unsafe_fn)]

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::{CStr, c_char, c_void},
    sync::Mutex,
};

pub const RDKIT_VERSION: &str = "2026.03.6";
pub const MAX_ATOMS: usize = 512;
pub const MAX_COORDINATES: usize = 4096;
pub const MAX_BONDS: usize = 2048;
pub const MAX_CONFORMERS: u32 = 32;
pub const MAX_ITERATIONS: u32 = 10_000;
// Upstream's numeric eigen solver seeds the process-global RDGeneral RNG.
// No native objects escape this API; serialize calls to protect that state.
static NATIVE_OPERATION: Mutex<()> = Mutex::new(());

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

/// `order` and `stereo` are RDKit enum values. Supported orders are 1 single,
/// 2 double, 3 triple, 12 aromatic, and 17 dative (native domain checks apply).
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
    /// Empty for Generate; otherwise full original+added-H coordinates.
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
            || !self.coordinates.iter().all(valid_coordinate)
        {
            return Err("Invalid geometry coordinate input".into());
        }
        match self.operation {
            Operation::Generate if !self.coordinates.is_empty() || !self.fixed_atoms.is_empty() => {
                return Err("Geometry generation cannot receive coordinates or fixed atoms".into());
            }
            Operation::Relax | Operation::Evaluate if self.coordinates.len() < n => {
                return Err("Geometry operation requires original and added-H coordinates".into());
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

#[repr(C)]
struct NativeAtom {
    atomic_number: u32,
    isotope: u32,
    charge: i32,
    explicit_h: u32,
    no_implicit: u32,
    aromatic: u32,
    radical: u32,
    chiral_tag: u32,
}
#[repr(C)]
struct NativeBond {
    a: u32,
    b: u32,
    order: u32,
    aromatic: u32,
    stereo: u32,
    stereo_atoms: [u32; 2],
}
#[repr(C)]
struct NativeRequest {
    abi_version: u32,
    operation: u32,
    field: u32,
    conformers: u32,
    seed: i32,
    max_iterations: u32,
    atoms: *const NativeAtom,
    atom_count: usize,
    bonds: *const NativeBond,
    bond_count: usize,
    coordinates: *const f64,
    coordinate_count: usize,
    fixed_atoms: *const u32,
    fixed_atom_count: usize,
}
#[repr(C)]
struct NativeResponse {
    owner: *mut c_void,
    coordinates: *const f64,
    gradient: *const f64,
    hydrogen_parents: *const u32,
    atom_count: usize,
    original_count: usize,
    initial_energy: f64,
    energy: f64,
    converged: u32,
    field: u32,
    diagnostics: *const c_char,
    error: *const c_char,
}

unsafe extern "C" {
    fn rsh_geometry_solve(request: *const NativeRequest, output: *mut NativeResponse) -> i32;
    fn rsh_geometry_free(output: *mut NativeResponse);
}

struct OwnedResponse(NativeResponse);
impl Drop for OwnedResponse {
    fn drop(&mut self) {
        // SAFETY: the result is initialized once by the native bridge; this is
        // its sole owner and only the matching C++ bridge frees its allocations.
        unsafe { rsh_geometry_free(&mut self.0) };
    }
}

fn field_value(field: ForceField) -> u32 {
    match field {
        ForceField::MMFF94 => 0,
        ForceField::MMFF94s => 1,
        ForceField::UFF => 2,
    }
}

/// Execute one geometry operation. Native storage stays in C++; all returned
/// values are copied and checked before releasing its opaque owner.
pub fn solve(request: &Request) -> Result<Response, String> {
    request.validate()?;
    let _native_guard = NATIVE_OPERATION
        .lock()
        .map_err(|_| "Native geometry lock poisoned")?;
    let atoms: Vec<_> = request
        .atoms
        .iter()
        .map(|a| NativeAtom {
            atomic_number: a.atomic_number,
            isotope: a.isotope,
            charge: a.charge,
            explicit_h: a.explicit_h,
            no_implicit: u32::from(a.no_implicit),
            aromatic: u32::from(a.aromatic),
            radical: a.radical,
            chiral_tag: a.chiral_tag,
        })
        .collect();
    let bonds: Vec<_> = request
        .bonds
        .iter()
        .map(|b| NativeBond {
            a: b.a as u32,
            b: b.b as u32,
            order: b.order,
            aromatic: u32::from(b.aromatic),
            stereo: b.stereo,
            stereo_atoms: b
                .stereo_atoms
                .map(|p| p.map(|i| i as u32))
                .unwrap_or([u32::MAX; 2]),
        })
        .collect();
    let fixed: Vec<u32> = request.fixed_atoms.iter().map(|&i| i as u32).collect();
    let input = NativeRequest {
        abi_version: 1,
        operation: match request.operation {
            Operation::Generate => 0,
            Operation::Relax => 1,
            Operation::Evaluate => 2,
        },
        field: field_value(request.field),
        conformers: request.conformers,
        seed: request.seed,
        max_iterations: request.max_iterations,
        atoms: atoms.as_ptr(),
        atom_count: atoms.len(),
        bonds: bonds.as_ptr(),
        bond_count: bonds.len(),
        coordinates: request.coordinates.as_ptr().cast(),
        coordinate_count: request.coordinates.len(),
        fixed_atoms: fixed.as_ptr(),
        fixed_atom_count: fixed.len(),
    };
    // SAFETY: a zeroed native output is the documented initial state. All
    // fields are pointers or scalar numbers for which zero is a valid value.
    let mut output = OwnedResponse(unsafe { std::mem::zeroed() });
    // SAFETY: all input slices are validated and alive for the call; the native
    // entry point catches exceptions and retains its output until free above.
    let status = unsafe { rsh_geometry_solve(&input, &mut output.0) };
    let out = &output.0;
    if status != 0 {
        return Err(if out.error.is_null() {
            "Native geometry operation failed".into()
        } else {
            // SAFETY: the bridge guarantees an owned, NUL-terminated message.
            unsafe { CStr::from_ptr(out.error) }
                .to_string_lossy()
                .into_owned()
        });
    }
    if out.owner.is_null()
        || out.coordinates.is_null()
        || out.original_count != atoms.len()
        || out.atom_count < out.original_count
        || out.atom_count > MAX_COORDINATES
        || out.field != field_value(request.field)
        || (out.atom_count > out.original_count && out.hydrogen_parents.is_null())
        || out.converged > 1
        || !out.initial_energy.is_finite()
        || !out.energy.is_finite()
        || (request.operation != Operation::Generate && out.atom_count != request.coordinates.len())
        || (request.operation == Operation::Evaluate && out.gradient.is_null())
    {
        return Err("Invalid native geometry response".into());
    }
    // SAFETY: native result owns exactly atom_count coordinate triples, checked
    // above. f64 triples have identical layout and alignment on both sides.
    let coordinates =
        unsafe { std::slice::from_raw_parts(out.coordinates.cast::<[f64; 3]>(), out.atom_count) }
            .to_vec();
    if !coordinates.iter().all(valid_coordinate) {
        return Err("Nonfinite or out-of-range native geometry coordinates".into());
    }
    let hydrogen_count = out.atom_count - out.original_count;
    let hydrogen_parents = if hydrogen_count == 0 {
        Vec::new()
    } else {
        // SAFETY: bridge owns one parent index for each appended H, as checked
        // against the coordinate count above; this copy outlives native storage.
        unsafe { std::slice::from_raw_parts(out.hydrogen_parents, hydrogen_count) }
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>()
    };
    if hydrogen_parents.iter().any(|&i| i >= out.original_count) {
        return Err("Invalid native geometry hydrogen parent".into());
    }
    for &i in &request.fixed_atoms {
        if coordinates[i] != request.coordinates[i] {
            return Err("Native geometry moved a fixed atom".into());
        }
    }
    let gradient = if out.gradient.is_null() {
        None
    } else {
        // SAFETY: a nonnull gradient has the same checked count as coordinates.
        let values =
            unsafe { std::slice::from_raw_parts(out.gradient.cast::<[f64; 3]>(), out.atom_count) }
                .to_vec();
        if values.iter().flatten().any(|x| !x.is_finite()) {
            return Err("Nonfinite native geometry gradient".into());
        }
        Some(values)
    };
    let diagnostics = if out.diagnostics.is_null() {
        Vec::new()
    } else {
        // SAFETY: the bridge guarantees an owned, NUL-terminated message.
        unsafe { CStr::from_ptr(out.diagnostics) }
            .to_string_lossy()
            .lines()
            .map(str::to_owned)
            .collect()
    };
    Ok(Response {
        coordinates,
        hydrogen_parents,
        original_count: out.original_count,
        initial_energy: out.initial_energy,
        energy: out.energy,
        gradient,
        converged: out.converged != 0,
        field: request.field,
        diagnostics,
    })
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

    #[test]
    fn native_results_copy_hydrogen_mapping_and_gradient_for_every_field() {
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
            request.coordinates[0][0] += step;
            let plus = solve(&request).unwrap().energy;
            request.coordinates[0][0] -= 2.0 * step;
            let minus = solve(&request).unwrap().energy;
            let numerical = (plus - minus) / (2.0 * step);
            assert!(
                (numerical - gradient[0][0]).abs() < 1e-4,
                "{field:?}: numerical {numerical}, native {}",
                gradient[0][0]
            );
        }
    }

    #[test]
    fn native_relax_keeps_original_and_added_hydrogen_fixed() {
        let mut request = ethanol(ForceField::MMFF94);
        request.coordinates = solve(&request).unwrap().coordinates;
        request.operation = Operation::Relax;
        request.fixed_atoms = vec![0, 3];
        request.coordinates[0][0] += 0.04;
        let result = solve(&request).unwrap();
        for &i in &request.fixed_atoms {
            assert_eq!(result.coordinates[i], request.coordinates[i]);
        }
        assert!(result.energy <= result.initial_energy + 1e-6);
    }

    #[test]
    fn invalid_requests_fail_before_native_and_native_domain_errors_are_owned() {
        let mut request = ethanol(ForceField::UFF);
        request.bonds[0].a = usize::MAX;
        assert!(solve(&request).unwrap_err().contains("bond"));
        let mut request = ethanol(ForceField::MMFF94);
        request.atoms[0].atomic_number = 30;
        assert!(!solve(&request).unwrap_err().is_empty());
        // A failed native result has already been freed; a subsequent operation
        // must still succeed and return valid storage through the same ABI.
        assert!(solve(&ethanol(ForceField::MMFF94)).is_ok());
    }
}
