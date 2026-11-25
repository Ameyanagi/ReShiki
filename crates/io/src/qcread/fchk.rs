//! Gaussian formatted checkpoints, read from the documented record layout.
//!
//! The Gaussian 09 User's Reference, "Structure of the Formatted Checkpoint File",
//! specifies the field widths, atomic units, normalized primitive contractions, and
//! Gaussian component order. Printed G09 and G16 Mo4OCl4-sp overlaps verify the
//! pure d order and signs, and the cclib G09 fchk fixture verifies signed grid
//! values. The reader retains s/p/SP and enables pure d for Gaussian 09 or 16.
//! Higher pure shells and Cartesian d and above have unverified conventions.
//! This adapter is original code under MIT OR Apache-2.0.
use std::collections::{BTreeMap, BTreeSet};

use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, AtomicCharges, BasisSet, Beta, CartesianNorm, ChargeScheme,
    MAX_BASIS_FUNCTIONS, MAX_COEFFICIENTS, MAX_HESSIAN_ATOMS, OrbitalSet, Orbitals, Shell, Status,
    Task, Vibrations, mo_from_molden,
};

use super::{Error, Format, Job, JobKind, Origin, QcFile, QcProgram, Step, frame};

#[derive(Debug)]
enum Value {
    Integers(Vec<i64>),
    Reals(Vec<f64>),
    Characters(String),
    ShortCharacters(String),
    Logicals(Vec<bool>),
}

impl Value {
    fn len(&self) -> usize {
        match self {
            Self::Integers(v) => v.len(),
            Self::Reals(v) => v.len(),
            Self::Characters(v) => v.len().div_ceil(12),
            Self::ShortCharacters(v) => v.len().div_ceil(8),
            Self::Logicals(v) => v.len(),
        }
    }
}

#[derive(Debug)]
struct Field {
    line: usize,
    value: Value,
}

struct Fields(BTreeMap<String, Field>);

impl Fields {
    fn get(&self, name: &str) -> Option<&Field> {
        self.0.get(name)
    }

    fn integers(&self, name: &str) -> Result<Option<&[i64]>, Error> {
        self.get(name)
            .map(|field| match &field.value {
                Value::Integers(values) => Ok(values.as_slice()),
                _ => Err(Error::at(
                    field.line,
                    format!("{name} must be integer data"),
                )),
            })
            .transpose()
    }

    fn reals(&self, name: &str) -> Result<Option<&[f64]>, Error> {
        self.get(name)
            .map(|field| match &field.value {
                Value::Reals(values) => Ok(values.as_slice()),
                _ => Err(Error::at(field.line, format!("{name} must be real data"))),
            })
            .transpose()
    }

    fn integer(&self, name: &str) -> Result<Option<i64>, Error> {
        let Some(values) = self.integers(name)? else {
            return Ok(None);
        };
        if let [value] = values {
            Ok(Some(*value))
        } else {
            Err(self.invalid(name, "must be a scalar"))
        }
    }

    fn count(&self, name: &str) -> Result<usize, Error> {
        let value = self
            .integer(name)?
            .ok_or_else(|| Error::Invalid(format!("The fchk file has no {name} record")))?;
        usize::try_from(value).map_err(|_| self.invalid(name, "must be non-negative"))
    }

    fn real(&self, name: &str) -> Result<Option<f64>, Error> {
        let Some(values) = self.reals(name)? else {
            return Ok(None);
        };
        if let [value] = values {
            Ok(Some(*value))
        } else {
            Err(self.invalid(name, "must be a scalar"))
        }
    }

    fn characters(&self, name: &str) -> Result<Option<&str>, Error> {
        self.get(name)
            .map(|field| match &field.value {
                Value::Characters(value) | Value::ShortCharacters(value) => Ok(value.trim()),
                _ => Err(Error::at(
                    field.line,
                    format!("{name} must be character data"),
                )),
            })
            .transpose()
    }

    fn real_array(&self, name: &str, count: usize) -> Result<Option<&[f64]>, Error> {
        let values = self.reals(name)?;
        if values.is_some_and(|v| v.len() != count) {
            return Err(self.invalid(name, format!("must contain {count} values")));
        }
        Ok(values)
    }

    fn invalid(&self, name: &str, message: impl std::fmt::Display) -> Error {
        let message = format!("{name} {message}");
        match self.get(name) {
            Some(field) => Error::at(field.line, message),
            None => Error::Invalid(message),
        }
    }
}

fn integer(text: &str, line: usize) -> Result<i64, Error> {
    text.trim()
        .parse()
        .map_err(|_| Error::at(line, format!("Invalid fchk integer: {}", text.trim())))
}

fn real(text: &str, line: usize) -> Result<f64, Error> {
    let text = text.trim();
    let value = if text.contains(['D', 'd']) {
        text.replace(['D', 'd'], "E").parse::<f64>()
    } else {
        text.parse::<f64>()
    }
    .map_err(|_| Error::at(line, format!("Invalid fchk real number: {text}")))?;
    if !value.is_finite() {
        return Err(Error::at(line, "A fchk real number is not finite"));
    }
    Ok(value)
}

fn logical(text: &str, line: usize) -> Result<bool, Error> {
    match text.trim() {
        "T" => Ok(true),
        "F" => Ok(false),
        _ => Err(Error::at(line, "A fchk logical value must be T or F")),
    }
}

/// Parse each record once. Never reserve memory from an untrusted declared count.
fn fields<'a>(
    lines: impl Iterator<Item = (usize, &'a str)>,
    bytes: usize,
) -> Result<Fields, Error> {
    let mut lines = lines;
    let mut records = BTreeMap::new();
    while let Some((line_number, line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.is_ascii() {
            return Err(Error::at(line_number, "Invalid fchk record header"));
        }
        let (header, tail) = line
            .split_at_checked(44)
            .ok_or_else(|| Error::at(line_number, "Invalid fchk record header"))?;
        // The checked 44-byte ASCII header contains an A40 label and a four-byte type field.
        let (name, kind) = header.split_at(40);
        let name = name.trim_end();
        let kind = kind.trim();
        let tail = tail.trim();
        let mut value = match kind {
            "I" => Value::Integers(Vec::new()),
            "R" => Value::Reals(Vec::new()),
            "C" => Value::Characters(String::new()),
            "H" => Value::ShortCharacters(String::new()),
            "L" => Value::Logicals(Vec::new()),
            _ => {
                return Err(Error::Unsupported(format!(
                    "Line {line_number}: fchk record type {kind} is not supported"
                )));
            }
        };
        if let Some(size) = tail.strip_prefix("N=") {
            let count = size.trim().parse::<usize>().map_err(|_| {
                Error::at(line_number, format!("Invalid fchk array size for {name}"))
            })?;
            let width = match kind {
                "I" | "C" => 12,
                "R" => 16,
                "H" => 8,
                _ => 1,
            };
            if count.checked_mul(width).is_none_or(|n| n > bytes) {
                return Err(Error::at(
                    line_number,
                    format!("The declared {name} array is larger than the file"),
                ));
            }
            while value.len() < count {
                let (number, data) = lines.next().ok_or_else(|| {
                    Error::at(line_number, format!("The {name} array is truncated"))
                })?;
                if !data.is_ascii() {
                    return Err(Error::at(number, "Fchk array data must be ASCII"));
                }
                let per_line = match kind {
                    "I" => 6,
                    "R" | "C" => 5,
                    "H" => 9,
                    _ => 72,
                };
                let expected = per_line.min(count - value.len());
                // Fortran character fields include spaces, including entirely blank fields.
                // Numeric fields also have fixed widths (6I12 and 5E16.8).
                let (mut columns, extra) = data
                    .split_at_checked(expected * width)
                    .ok_or_else(|| Error::at(number, format!("Invalid {name} array row width")))?;
                if !extra.trim().is_empty() {
                    return Err(Error::at(number, format!("Invalid {name} array row width")));
                }
                // ASCII columns have exactly expected*width bytes, so each split is a
                // complete fixed-width field at a character boundary.
                while !columns.is_empty() {
                    let (item, rest) = columns.split_at(width);
                    columns = rest;
                    match &mut value {
                        Value::Integers(v) => v.push(integer(item, number)?),
                        Value::Reals(v) => v.push(real(item, number)?),
                        Value::Characters(v) | Value::ShortCharacters(v) => v.push_str(item),
                        Value::Logicals(v) => v.push(logical(item, number)?),
                    }
                }
            }
        } else {
            match &mut value {
                Value::Integers(v) => v.push(integer(tail, line_number)?),
                Value::Reals(v) => v.push(real(tail, line_number)?),
                Value::Characters(v) | Value::ShortCharacters(v) => v.push_str(tail),
                Value::Logicals(v) => v.push(logical(tail, line_number)?),
            }
        }
        // Gaussian repeats some metadata (for example Force Field); the last record wins.
        records.insert(
            name.to_string(),
            Field {
                line: line_number,
                value,
            },
        );
    }
    Ok(Fields(records))
}

/// Read one checkpoint geometry and the calculated properties attached to it.
pub(super) fn read(text: &str, origin: &Origin<'_>) -> Result<QcFile, Error> {
    let mut lines = text.lines().enumerate().map(|(i, line)| (i + 1, line));
    let (_, title) = lines
        .next()
        .ok_or_else(|| Error::at(1, "The fchk file has no title"))?;
    let (_, description) = lines
        .next()
        .ok_or_else(|| Error::at(2, "The fchk file has no calculation description"))?;
    if !description.is_ascii() {
        return Err(Error::at(
            2,
            "The fchk calculation description is too short",
        ));
    }
    let (description, basis_name) = description
        .split_at_checked(40)
        .ok_or_else(|| Error::at(2, "The fchk calculation description is too short"))?;
    // The checked 40-byte ASCII prefix is A10 calculation followed by A30 method.
    let (calculation, method) = description.split_at(10);
    let calculation = calculation.trim().to_ascii_uppercase();
    let method = nonempty(method);
    let basis_name = nonempty(basis_name);
    let fields = fields(lines, text.len())?;
    let numbers = fields
        .integers("Atomic numbers")?
        .ok_or_else(|| Error::Invalid("The fchk file has no Atomic numbers record".into()))?;
    let n_atoms = fields.count("Number of atoms")?;
    if numbers.len() != n_atoms {
        return Err(fields.invalid("Atomic numbers", "must match Number of atoms"));
    }
    let mut atoms = numbers
        .iter()
        .map(|&number| {
            u8::try_from(number)
                .ok()
                .filter(|&z| (1..=118).contains(&z))
                .map(|z| (z, 0))
                .ok_or_else(|| fields.invalid("Atomic numbers", "contains an invalid element"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(masses) = fields.real_array("Real atomic weights", n_atoms)? {
        let mut common_masses = BTreeMap::new();
        for ((z, isotope), &mass) in atoms.iter_mut().zip(masses) {
            if mass <= 0.0 || mass.round() > f64::from(u16::MAX) {
                return Err(
                    fields.invalid("Real atomic weights", "contains an invalid atomic mass")
                );
            }
            // Gaussian supplies the most common isotope's exact mass by default.
            // Preserve its unlabeled identity unless the explicit mass clearly differs.
            let common = match common_masses.get(z) {
                Some(&mass) => mass,
                None => {
                    let common_isotope = crate::chemistry::ELEMENTS
                        .get(usize::from(*z))
                        .ok_or_else(|| {
                            fields.invalid("Atomic numbers", "contains an invalid element")
                        })?
                        .common_isotope;
                    let mass = crate::chemistry::properties(&[crate::chemistry::AtomFacts {
                        atomic_number: *z,
                        isotope: common_isotope,
                        charge: 0,
                        hydrogens: 0,
                        radical_electrons: 0,
                    }])
                    .map_err(Error::Invalid)?
                    .mass;
                    common_masses.insert(*z, mass);
                    mass
                }
            };
            if (mass - common).abs() > 0.5 {
                *isotope = mass.round() as u16;
            }
        }
    }
    let axes = n_atoms
        .checked_mul(3)
        .ok_or_else(|| fields.invalid("Number of atoms", "is too large"))?;
    let coordinates = fields
        .real_array("Current cartesian coordinates", axes)?
        .ok_or_else(|| {
            Error::Invalid("The fchk file has no Current cartesian coordinates record".into())
        })?;
    let charge = i32::try_from(
        fields
            .integer("Charge")?
            .ok_or_else(|| Error::Invalid("The fchk file has no Charge record".into()))?,
    )
    .map_err(|_| fields.invalid("Charge", "is out of range"))?;
    let multiplicity = u32::try_from(
        fields
            .integer("Multiplicity")?
            .ok_or_else(|| Error::Invalid("The fchk file has no Multiplicity record".into()))?,
    )
    .map_err(|_| fields.invalid("Multiplicity", "is out of range"))?;
    let frame = frame(
        &atoms,
        coordinates
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[x, y, z]| {
                [
                    x * ANGSTROM_PER_BOHR,
                    y * ANGSTROM_PER_BOHR,
                    z * ANGSTROM_PER_BOHR,
                ]
            })
            .collect(),
        charge,
        multiplicity,
    )?;
    let version = fields.characters("Gaussian Version")?.and_then(nonempty);
    let program = QcProgram {
        name: "Gaussian".into(),
        version: version.clone(),
    };
    let (kind, task) = match calculation.as_str() {
        "SP" => (JobKind::SinglePoint, Task::Energy),
        "FOPT" | "POPT" => (JobKind::Optimization, Task::Optimize),
        "FREQ" => (JobKind::Frequencies, Task::Hessian),
        "FORCE" => (JobKind::Other, Task::Gradient),
        "SCAN" => (JobKind::Scan, Task::Energy),
        _ => (JobKind::Other, Task::Energy),
    };
    let energy = fields.real("Total Energy")?;
    let mut job = Job {
        index: 0,
        kind,
        method,
        basis: basis_name,
        keywords: fields
            .characters("Route")?
            .and_then(nonempty)
            .into_iter()
            .collect(),
        completion: None,
        steps: Vec::new(),
        result: None,
    };
    let status = match fields.integer("Job Status")? {
        Some(3) => Status::Failed {
            reason: "Gaussian job status 3 (error)".into(),
        },
        Some(-1) => Status::NotConverged {
            reason: "job in progress".into(),
        },
        Some(0) => Status::NotConverged {
            reason: "Gaussian Job Status is 0".into(),
        },
        _ => Status::Converged,
    };
    let mut result = origin.result(&frame, task, status, &program, &job);
    let mut warnings = Vec::new();
    result.energy_hartree = energy;
    result.gradient_hartree_per_bohr = fields
        .real_array("Cartesian Gradient", axes)?
        .unwrap_or_default()
        .to_vec();
    if let Some(values) = fields.real_array("Mulliken Charges", n_atoms)? {
        result.charges.push(AtomicCharges {
            scheme: ChargeScheme::Mulliken,
            values: values.to_vec(),
        });
    }
    result.dipole_e_bohr = fields
        .reals("Dipole Moment")?
        .map(|xyz| {
            xyz.try_into()
                .map_err(|_| fields.invalid("Dipole Moment", "must contain 3 values"))
        })
        .transpose()?;
    result.orbitals = orbitals(
        &fields,
        &frame,
        coordinates,
        job.method.as_deref(),
        &job.keywords,
        &mut warnings,
    )?;
    if n_atoms <= MAX_HESSIAN_ATOMS {
        // Optimization checkpoints may carry an approximate update Hessian. Only a
        // frequency calculation establishes this record as a calculated Hessian.
        if kind == JobKind::Frequencies {
            let packed_len = axes * (axes + 1) / 2;
            if let Some(packed) = fields.real_array("Cartesian Force Constants", packed_len)? {
                result.hessian_hartree_per_bohr2 = expand_hessian(packed, axes);
            }
        }
        result.vibrations = vibrations(&fields, axes, n_atoms)?;
    } else if kind != JobKind::Frequencies && fields.get("Number of Normal Modes").is_some() {
        warnings.push(format!(
            "Job 1: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms"
        ));
    }
    job.steps.push(Step {
        frame,
        energy_hartree: energy,
        label: nonempty(title),
    });
    job.result = Some(result);
    Ok(QcFile {
        format: Format::Fchk,
        program: Some(program.name),
        program_version: version,
        jobs: vec![job],
        warnings,
    })
}

#[allow(
    clippy::indexing_slicing,
    reason = "The caller checks packed.len() == axes*(axes+1)/2; row and column are below axes, and full is allocated to axes*axes"
)]
fn expand_hessian(packed: &[f64], axes: usize) -> Vec<f64> {
    let mut full = vec![0.0; axes * axes];
    let mut index = 0;
    for row in 0..axes {
        for column in 0..=row {
            full[row * axes + column] = packed[index];
            full[column * axes + row] = packed[index];
            index += 1;
        }
    }
    full
}

fn nonempty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Retain s, p, SP and Gaussian 09 or 16 pure d shells. An unverified shell
/// convention leaves the spectrum intact while omitting all orbital shapes.
fn basis(
    fields: &Fields,
    coordinates: &[f64],
    n_ao: usize,
    stored_coefficients: Option<usize>,
    warnings: &mut Vec<String>,
) -> Result<Option<BasisSet>, Error> {
    let types = fields.integers("Shell types")?;
    let gaussian = match fields.characters("Gaussian Version")? {
        Some(version) => version
            .split_whitespace()
            .next()
            .and_then(|word| word.rsplit('-').next())
            .and_then(|id| {
                id.strip_prefix("G09Rev")
                    .or_else(|| id.strip_prefix("G16Rev"))
            })
            .is_some_and(|revision| {
                matches!(
                    revision.as_bytes(),
                    [b'A'..=b'Z', b'.', b'0'..=b'9', b'0'..=b'9']
                )
            }),
        None => fields.get("ILSW").is_some() && fields.get("IOpCl").is_some(),
    };
    let unverified: BTreeSet<_> = types
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|&kind| kind.unsigned_abs() >= 2 && !(kind == -2 && gaussian))
        .map(|kind| (kind.unsigned_abs(), kind > 0))
        .collect();
    let non_gaussian_d = unverified.contains(&(2, false));
    let mut names = unverified
        .into_iter()
        .map(|(l, cartesian)| {
            let representation = if cartesian { "Cartesian" } else { "pure" };
            format!("{representation} {}", angular_name(l))
        })
        .collect::<Vec<_>>();
    let verified = names.is_empty();
    if let Some(last) = names.pop() {
        let qualifier = if gaussian { "Gaussian " } else { "" };
        let location = if non_gaussian_d {
            " in an fchk file whose Gaussian 09 or 16 origin could not be verified"
        } else {
            ""
        };
        let functions = if names.is_empty() {
            last
        } else {
            format!("{} and {last}", names.join(", "))
        };
        warnings.push(format!(
            "Orbital shapes left out: {qualifier}{functions} functions{location} have no verified convention here; orbital energies are kept"
        ));
    }
    let Some(count) = stored_coefficients else {
        return Ok(None);
    };
    if n_ao > MAX_BASIS_FUNCTIONS {
        warnings.push(format!(
            "Orbital shapes left out: the basis has {n_ao} functions (limit {MAX_BASIS_FUNCTIONS}); orbital energies are kept"
        ));
        return Ok(None);
    }
    if count > MAX_COEFFICIENTS {
        warnings.push(format!(
            "Orbital shapes left out: each spin has {count} coefficients (limit {MAX_COEFFICIENTS}); orbital energies are kept"
        ));
        return Ok(None);
    }
    let types =
        types.ok_or_else(|| Error::Invalid("The fchk file has no Shell types record".into()))?;
    let primitives = fields
        .integers("Number of primitives per shell")?
        .ok_or_else(|| {
            Error::Invalid("The fchk file has no Number of primitives per shell record".into())
        })?;
    let atoms = fields
        .integers("Shell to atom map")?
        .ok_or_else(|| Error::Invalid("The fchk file has no Shell to atom map record".into()))?;
    if primitives.len() != types.len() || atoms.len() != types.len() {
        return Err(fields.invalid("Shell types", "and shell metadata have different lengths"));
    }
    if let Some(count) = fields.integer("Number of contracted shells")?
        && usize::try_from(count).ok() != Some(types.len())
    {
        return Err(fields.invalid("Shell types", "must match Number of contracted shells"));
    }
    let exponents = fields
        .reals("Primitive exponents")?
        .ok_or_else(|| Error::Invalid("The fchk file has no Primitive exponents record".into()))?;
    if exponents.iter().any(|&exponent| exponent <= 0.0) {
        return Err(fields.invalid("Primitive exponents", "must be positive"));
    }
    let coefficients = fields
        .real_array("Contraction coefficients", exponents.len())?
        .ok_or_else(|| {
            Error::Invalid("The fchk file has no Contraction coefficients record".into())
        })?;
    let p_coefficients = fields.real_array("P(S=P) Contraction coefficients", exponents.len())?;
    if let Some(count) = fields.integer("Number of primitive shells")?
        && usize::try_from(count).ok() != Some(exponents.len())
    {
        return Err(fields.invalid(
            "Primitive exponents",
            "must match Number of primitive shells",
        ));
    }
    let shell_coordinates = fields.real_array("Coordinates of each shell", types.len() * 3)?;
    let atom_centres = coordinates.as_chunks::<3>().0;
    let shell_centres = shell_coordinates.map(|xyz| xyz.as_chunks::<3>().0);
    let mut shells = Vec::new();
    let mut primitive_start = 0usize;
    let mut source_ao = 0usize;
    let mut displaced_centres = false;
    for (index, ((&kind, &n_primitives), &atom)) in
        types.iter().zip(primitives).zip(atoms).enumerate()
    {
        let atom = usize::try_from(atom)
            .ok()
            .and_then(|a| a.checked_sub(1))
            .filter(|&a| a < atom_centres.len())
            .ok_or_else(|| fields.invalid("Shell to atom map", "contains an invalid atom index"))?;
        let count = usize::try_from(n_primitives)
            .ok()
            .filter(|&count| count > 0)
            .ok_or_else(|| fields.invalid("Number of primitives per shell", "must be positive"))?;
        let end = primitive_start
            .checked_add(count)
            .filter(|&end| end <= exponents.len())
            .ok_or_else(|| {
                fields.invalid(
                    "Number of primitives per shell",
                    "exceeds the primitive arrays",
                )
            })?;
        let l = usize::try_from(kind.unsigned_abs())
            .map_err(|_| fields.invalid("Shell types", "contains excessive angular momentum"))?;
        let functions = match kind {
            -1 => Some(4),
            ..=-2 => l.checked_mul(2).and_then(|v| v.checked_add(1)),
            _ => l
                .checked_add(1)
                .and_then(|a| l.checked_add(2).and_then(|b| a.checked_mul(b)))
                .map(|v| v / 2),
        };
        let functions = functions
            .filter(|&count| count <= n_ao.saturating_sub(source_ao))
            .ok_or_else(|| fields.invalid("Shell types", "exceeds Number of basis functions"))?;
        if kind == -1 && p_coefficients.is_none() {
            return Err(Error::Invalid(
                "SP shells require P(S=P) Contraction coefficients".into(),
            ));
        }
        if verified {
            #[allow(
                clippy::indexing_slicing,
                reason = "primitive_start <= end <= exponents.len() is checked above, and both contraction arrays are checked to have that same length"
            )]
            let mut push_shell = |l: u8, contractions: &[f64]| {
                shells.push(Shell {
                    atom: atom as u32,
                    l,
                    pure: kind == -2,
                    exponents_per_bohr2: exponents[primitive_start..end].to_vec(),
                    coefficients: contractions[primitive_start..end].to_vec(),
                });
            };
            if let Some(p) = p_coefficients.filter(|_| kind == -1) {
                push_shell(0, coefficients);
                push_shell(1, p);
            } else {
                push_shell(l as u8, coefficients);
            }
        }
        #[allow(
            clippy::indexing_slicing,
            reason = "index enumerates the shell count checked against shell_centres; atom is checked below n_atoms, and the caller validates 3*n_atoms coordinates"
        )]
        let displaced = shell_centres.is_some_and(|centres| {
            centres[index]
                .iter()
                .zip(&atom_centres[atom])
                .any(|(&a, &b)| (a - b).abs() > 1e-7)
        });
        displaced_centres |= displaced;
        primitive_start = end;
        source_ao += functions;
    }
    if primitive_start != exponents.len() {
        return Err(fields.invalid(
            "Number of primitives per shell",
            "does not use all primitives",
        ));
    }
    if source_ao != n_ao {
        return Err(fields.invalid("Shell types", "does not match Number of basis functions"));
    }
    if displaced_centres {
        warnings.push(
            "Orbital shapes left out: shell centres differ from their mapped atoms; orbital energies are kept".into(),
        );
    }
    if !verified || displaced_centres {
        return Ok(None);
    }
    Ok(Some(BasisSet { shells }))
}

fn angular_name(l: u64) -> String {
    match l {
        2 => "d".into(),
        3 => "f".into(),
        4 => "g".into(),
        5 => "h".into(),
        _ => format!("l={l}"),
    }
}

fn orbitals(
    fields: &Fields,
    frame: &reshiki_qcdata::Frame,
    coordinates: &[f64],
    method: Option<&str>,
    keywords: &[String],
    warnings: &mut Vec<String>,
) -> Result<Option<Orbitals>, Error> {
    if natural_orbital_route(keywords) {
        warnings.push(
            "Orbitals left out: natural-orbital records store occupations instead of orbital energies"
                .into(),
        );
        return Ok(None);
    }
    let Some(alpha_energies) = fields.reals("Alpha Orbital Energies")? else {
        if fields.get("Beta Orbital Energies").is_some()
            || fields.get("Alpha MO coefficients").is_some()
            || fields.get("Beta MO coefficients").is_some()
        {
            return Err(Error::Invalid(
                "Orbital data requires Alpha Orbital Energies".into(),
            ));
        }
        return Ok(None);
    };
    let n_ao = fields.count("Number of basis functions")?;
    let n_mo = fields
        .integer("Number of independent functions")?
        .map(|n| {
            usize::try_from(n).map_err(|_| {
                fields.invalid("Number of independent functions", "must be non-negative")
            })
        })
        .transpose()?
        .unwrap_or(alpha_energies.len());
    if alpha_energies.len() != n_mo || n_mo == 0 || n_mo > n_ao {
        return Err(fields.invalid(
            "Alpha Orbital Energies",
            "must match Number of independent functions within the basis",
        ));
    }
    let beta_energies = fields.reals("Beta Orbital Energies")?;
    if beta_energies.is_some_and(|v| v.len() != n_mo) {
        return Err(fields.invalid(
            "Beta Orbital Energies",
            "must match Number of independent functions",
        ));
    }
    let n_alpha = fields.count("Number of alpha electrons")?;
    let n_beta = fields.count("Number of beta electrons")?;
    let irohf = fields.integer("IROHF")?;
    let unrestricted = method.is_some_and(|m| m.starts_with(['U', 'u']))
        || (irohf == Some(0) && n_alpha != n_beta);
    if n_alpha > n_mo || n_beta > n_mo {
        return Err(Error::Invalid(
            "The fchk electron counts exceed the orbital spectrum".into(),
        ));
    }
    let electrons = n_alpha
        .checked_add(n_beta)
        .ok_or_else(|| Error::Invalid("The fchk electron count is too large".into()))?;
    if fields
        .integer("Number of electrons")?
        .is_some_and(|count| usize::try_from(count).ok() != Some(electrons))
        || n_alpha.abs_diff(n_beta) != frame.multiplicity as usize - 1
    {
        return Err(Error::Invalid(
            "The fchk electron counts disagree with Number of electrons or Multiplicity".into(),
        ));
    }
    let mut core = Vec::new();
    if let Some(nuclear) = fields.real_array("Nuclear charges", frame.numbers.len())? {
        for (&z, &charge) in frame.numbers.iter().zip(nuclear) {
            let rounded = charge.round();
            if (charge - rounded).abs() > 1e-7 || rounded < 0.0 || rounded > f64::from(z) {
                return Err(fields.invalid(
                    "Nuclear charges",
                    "must be integer effective charges between zero and Z",
                ));
            }
            core.push(u32::from(z) - rounded as u32);
        }
    }
    if frame.electrons() - core.iter().map(|&n| i64::from(n)).sum::<i64>() != electrons as i64 {
        return Err(Error::Invalid(
            "The fchk electron count disagrees with the atomic numbers, nuclear charges and charge"
                .into(),
        ));
    }
    if core.iter().all(|&n| n == 0) {
        core.clear();
    }
    let coefficient_count = n_ao
        .checked_mul(n_mo)
        .ok_or_else(|| Error::Invalid("The fchk coefficient dimensions are too large".into()))?;
    let alpha_coefficients = fields.real_array("Alpha MO coefficients", coefficient_count)?;
    let beta_coefficients = fields.real_array("Beta MO coefficients", coefficient_count)?;
    if beta_coefficients.is_some() && beta_energies.is_none() {
        return Err(Error::Invalid(
            "Beta MO coefficients require Beta Orbital Energies".into(),
        ));
    }
    if unrestricted && beta_energies.is_none() {
        warnings.push(
            "Orbitals left out: unrestricted Gaussian orbital records have no beta spectrum".into(),
        );
        return Ok(None);
    }
    let stored_coefficients = alpha_coefficients.or(beta_coefficients).map(<[f64]>::len);
    let basis = basis(fields, coordinates, n_ao, stored_coefficients, warnings)?;
    let shared_open = beta_energies.is_none()
        && (irohf == Some(1) || method.is_some_and(|m| m.to_ascii_uppercase().starts_with("RO")));
    let restricted = beta_energies.is_none() && !shared_open;
    if restricted && n_alpha != n_beta {
        return Err(Error::Invalid(
            "Restricted fchk orbitals require equal alpha and beta electron counts".into(),
        ));
    }
    let ao_order = basis
        .as_ref()
        .map(|basis| {
            // With unit Cartesian norms and Molden's pure d signs, the supported
            // s, p, SP and pure d shells only permute these exact integer indices.
            let indices = (0..n_ao).map(|i| i as f64).collect::<Vec<_>>();
            mo_from_molden(basis, &indices, CartesianNorm::Unit)
                .map(|order| order.into_iter().map(|i| i as usize).collect::<Vec<_>>())
        })
        .transpose()?;
    // Preserve source occupations and row identities when canonicalizing the spectrum.
    // Stable sorting also keeps the source order within a degenerate set.
    let permutation = |energies: &[f64]| {
        let mut levels = energies.iter().copied().enumerate().collect::<Vec<_>>();
        levels.sort_by(|&(_, a), &(_, b)| {
            if a == b {
                std::cmp::Ordering::Equal
            } else {
                a.total_cmp(&b)
            }
        });
        levels
    };
    let alpha_order = permutation(alpha_energies);
    let set = |occupied: usize, coefficients: Option<&[f64]>, levels: &[(usize, f64)]| {
        let (rows, coefficients) = match (ao_order.as_deref(), coefficients) {
            (Some(ao_order), Some(values)) => (
                (0..n_mo as u32).collect(),
                canonical_coefficients(ao_order, values, levels),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        OrbitalSet {
            energies_hartree: levels.iter().map(|&(_, energy)| energy).collect(),
            occupations: levels
                .iter()
                .map(|&(i, _)| {
                    if i < occupied {
                        if restricted { 2.0 } else { 1.0 }
                    } else {
                        0.0
                    }
                })
                .collect(),
            symmetries: Vec::new(),
            rows,
            coefficients,
        }
    };
    let alpha = set(n_alpha, alpha_coefficients, &alpha_order);
    let beta = match beta_energies {
        Some(energies) => {
            Beta::Unrestricted(set(n_beta, beta_coefficients, &permutation(energies)))
        }
        None if shared_open => Beta::SameOrbitals {
            occupations: alpha_order
                .iter()
                .map(|&(i, _)| if i < n_beta { 1.0 } else { 0.0 })
                .collect(),
        },
        None => Beta::Restricted,
    };
    Ok(Some(Orbitals {
        basis,
        core_electrons: core,
        alpha,
        beta,
    }))
}

fn natural_orbital_route(keywords: &[String]) -> bool {
    keywords.iter().any(|route| {
        let lower = route.to_ascii_lowercase();
        let separator = |c: char| !c.is_ascii_alphanumeric();
        let natural = |token| matches!(token, "naturalorbitals" | "natorb");
        // Population analysis alone leaves the canonical checkpoint orbitals intact.
        if lower.split(separator).any(|token| {
            matches!(
                token,
                "saveno"
                    | "savenatural"
                    | "savenaturalorbitals"
                    | "savenaturaltransitionorbitals"
                    | "savento"
                    | "savenbo"
                    | "savenbos"
                    | "savenlmo"
                    | "savenlmos"
                    | "savemixed"
            )
        }) {
            return true;
        }
        let options = lower
            .match_indices("guess")
            .filter_map(|(start, _)| {
                if lower
                    .get(..start)
                    .and_then(|prefix| prefix.chars().next_back())
                    .is_some_and(|c| c.is_ascii_alphanumeric())
                {
                    return None;
                }
                let rest = lower.get(start + 5..).unwrap_or_default().trim_start();
                let options = if let Some(options) = rest.strip_prefix('=') {
                    options.trim_start()
                } else if rest.starts_with('(') {
                    rest
                } else {
                    return None;
                };
                if let Some(list) = options.strip_prefix('(') {
                    list.split_once(')').map(|(list, _)| list)
                } else {
                    options.split(separator).next()
                }
            })
            .flat_map(|options| options.split(separator));
        let mut guess_only = false;
        for option in options {
            if natural(option) {
                return true;
            }
            guess_only |= option == "only";
        }
        guess_only && lower.split(separator).any(natural)
    })
}

#[allow(
    clippy::indexing_slicing,
    reason = "The caller validates the matrix dimensions and builds the AO and MO permutations"
)]
fn canonical_coefficients(ao_order: &[usize], values: &[f64], levels: &[(usize, f64)]) -> Vec<f64> {
    let n_ao = ao_order.len();
    let identity = ao_order.iter().copied().eq(0..n_ao);
    let mut converted = Vec::with_capacity(values.len());
    for &(mo, _) in levels {
        let row = &values[mo * n_ao..(mo + 1) * n_ao];
        if identity {
            converted.extend_from_slice(row);
        } else {
            converted.extend(ao_order.iter().map(|&ao| row[ao]));
        }
    }
    converted
}

fn vibrations(fields: &Fields, axes: usize, n_atoms: usize) -> Result<Option<Vibrations>, Error> {
    let Some(properties) = fields.reals("Vib-E2")? else {
        return Ok(None);
    };
    let count = fields.count("Number of Normal Modes")?;
    if count > axes.saturating_sub(5) {
        return Err(fields.invalid("Number of Normal Modes", "is out of range"));
    }
    if count == 0 {
        return Ok(None);
    }
    // The G16 fixture/output pair proves four contiguous blocks: frequency, reduced
    // mass, force constant, and IR intensity. Later Raman/ROA blocks are not interpreted.
    if properties.len() < count * 4 || properties.len() % count != 0 {
        return Err(fields.invalid(
            "Vib-E2",
            "must contain complete frequency, reduced mass, force constant and IR blocks",
        ));
    }
    // The complete-block check above proves all four splits are within the array.
    let (frequencies, properties) = properties.split_at(count);
    let (reduced_masses, properties) = properties.split_at(count);
    let (_force_constants, properties) = properties.split_at(count);
    let (intensities, _) = properties.split_at(count);
    let mut modes = fields
        .real_array("Vib-Modes", count * axes)?
        .unwrap_or_default()
        .to_vec();
    for mode in modes.chunks_exact_mut(axes) {
        let scale = mode.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        if scale == 0.0 {
            return Err(fields.invalid("Vib-Modes", "contains a zero displacement vector"));
        }
        let norm = mode.iter().map(|x| (x / scale).powi(2)).sum::<f64>().sqrt();
        for component in mode {
            *component = (*component / scale) / norm;
        }
    }
    Ok(Some(Vibrations {
        frequencies_cm1: frequencies.to_vec(),
        reduced_masses_amu: reduced_masses.to_vec(),
        ir_intensities_km_mol: intensities.to_vec(),
        modes,
        masses_amu: fields
            .real_array("Vib-AtMass", n_atoms)?
            .unwrap_or_default()
            .to_vec(),
    }))
}

#[cfg(test)]
#[path = "tests/fchk.rs"]
mod tests;
