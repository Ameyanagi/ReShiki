//! Molden records, converted at the boundary to the canonical AO convention.
//!
//! The cclib Molden 5.7 carbon fixture confirms Cartesian `Unit` normalization
//! for d/f/g by MO orthonormality. The PySCF 2.15.0 fixtures confirm the Molden
//! pure d/f/g order and signs, Cartesian d/f/g order and `Unit` normalization,
//! and mixed per-l flags against signed grid values. By import policy, files
//! from Molden and unidentified producers use the specification convention for
//! pure d/f/g as verified on PySCF exports. The reader accepts this convention
//! for PySCF exports with a recorded major version of 2 or later.
//!
//! The ORCA formaldehyde TDDFT and water TZVP fixtures verify bare radial
//! coefficients for contracted s/p shells by implied AO overlap. Every d/f
//! shell in these fixtures has one primitive. The phase comparison assumes
//! ORCA's p order and signs are x, y, z. It confirms the Molden pure d order
//! and signs and the pure f order with inverted m = ±3 signs. The adapter
//! applies this convention to all `orca_2mkl` files. Pure g and Cartesian
//! d/f/g from `orca_2mkl` and d/f/g from other named producers remain unverified.

use std::collections::{BTreeMap, BTreeSet};

use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, BasisSet, Beta, CartesianNorm, MAX_BASIS_FUNCTIONS, MAX_COEFFICIENTS,
    MAX_FRAME_ATOMS, MAX_HESSIAN_ATOMS, MAX_PRIMITIVES, MAX_TRAJECTORY_ATOMS, OrbitalSet, Orbitals,
    Shell, Status, Task, Vibrations, mo_from_molden, primitive_norm,
};

use super::{
    Error, Format, Job, JobKind, Origin, QcFile, QcProgram, Step, element, frame,
    lowest_multiplicity,
};

#[derive(Clone, Copy)]
struct Line<'a> {
    number: usize,
    text: &'a str,
}

struct Section<'a> {
    name: String,
    qualifier: &'a str,
    line: usize,
    body: &'a str,
}

impl Section<'_> {
    fn lines(&self) -> impl Iterator<Item = Line<'_>> {
        self.body.lines().enumerate().map(|(index, text)| Line {
            number: self.line + 1 + index,
            text,
        })
    }

    fn data(&self) -> impl Iterator<Item = Line<'_>> {
        self.lines().filter(|line| is_data(line.text))
    }
}

#[derive(Clone)]
struct Geometry {
    atoms: Vec<(u8, u16)>,
    positions: Vec<[f64; 3]>,
    label: Option<String>,
}

struct Atoms {
    geometry: Geometry,
    indices: BTreeMap<usize, u32>,
    core: Vec<u32>,
}

struct GaussianBasis {
    // Keep the true AO count after dropping an oversized basis, to check sparse indices.
    basis: Option<BasisSet>,
    n_ao: usize,
}

struct Producer {
    name: String,
    verified_writer: bool,
}

#[derive(Default)]
struct Mo {
    energy: Option<f64>,
    occupation: Option<f64>,
    symmetry: Option<String>,
    beta: bool,
    fields: u8,
    coefficients: Vec<(usize, f64)>,
}

#[derive(Clone, Copy)]
enum MoHeader {
    Symmetry = 1,
    Energy = 2,
    Spin = 4,
    Occupation = 8,
}

struct Level {
    energy: f64,
    occupation: Option<f64>,
    symmetry: Option<String>,
    coefficients: Option<Vec<f64>>,
}

#[derive(Default)]
struct Channel {
    levels: Vec<Level>,
    // Include occupations whose missing energy prevents storing the level.
    occupations: Vec<Option<f64>>,
    rows: usize,
    shapes_dropped: bool,
}

#[derive(Default)]
struct MolecularOrbitals {
    alpha: Channel,
    beta: Channel,
    missing_energies: usize,
    missing_spins: usize,
}

struct InferredState {
    charge: i32,
    multiplicity: u32,
    unrestricted: bool,
    // Only integral, separate alpha/beta populations establish a multiplicity.
    known_spin: bool,
    max_unpaired: f64,
}

/// Read the Molden specification's Gaussian, geometry, and vibration records.
pub(super) fn read(text: &str, origin: &Origin<'_>) -> Result<QcFile, Error> {
    let sections = sections(text)?;
    if !sections.iter().any(|s| s.name == "MOLDEN FORMAT") {
        return Err(Error::Invalid("Missing [Molden Format] header".into()));
    }
    if sections.iter().any(|s| s.name == "STO") {
        return Err(Error::Unsupported(
            "Slater-type orbitals ([STO]) are not supported".into(),
        ));
    }
    let producer = producer(text, &sections);
    let orca = producer
        .as_ref()
        .is_some_and(|p| p.name == "ORCA" && p.verified_writer);
    let representations = read_representations(&sections)?;
    let mut warnings = Vec::new();
    let n_atoms = scalar(&sections, "N_ATOMS")?
        .map(|l| integer::<usize>(l.text, l.number, "atom count"))
        .transpose()?;
    if n_atoms.is_some_and(|n| n == 0 || n > MAX_FRAME_ATOMS) {
        return Err(Error::Unsupported(format!(
            "Molden atom counts must be between 1 and {MAX_FRAME_ATOMS}"
        )));
    }
    let explicit_charge = scalar(&sections, "CHARGE")?
        .map(|l| integer::<i32>(l.text, l.number, "charge"))
        .transpose()?;
    let explicit_multiplicity = scalar(&sections, "MULTIPLICITY")?
        .map(|l| integer::<u32>(l.text, l.number, "multiplicity"))
        .transpose()?;
    if explicit_multiplicity == Some(0) {
        return Err(Error::Invalid("Multiplicity must be at least 1".into()));
    }
    let title = unique(&sections, "TITLE")?.and_then(|s| {
        let title = s
            .data()
            .map(|l| l.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        (!title.is_empty()).then_some(title)
    });
    let mut atoms = unique(&sections, "ATOMS")?
        .map(|s| read_atoms(s, title.clone()))
        .transpose()?;
    if let Some(section) = unique(&sections, "PSEUDO")? {
        read_pseudo(
            section,
            atoms
                .as_mut()
                .ok_or_else(|| Error::at(section.line, "[Pseudo] requires [Atoms]"))?,
        )?;
    }
    let frequency_geometry = unique(&sections, "FR-COORD")?
        .map(|s| coordinate_geometry(s, ANGSTROM_PER_BOHR))
        .transpose()?;
    let geometries = unique(&sections, "GEOMETRIES")?
        .map(read_geometries)
        .transpose()?
        .unwrap_or_default();
    let main_geometry = atoms
        .as_ref()
        .map(|a| &a.geometry)
        .or(geometries.last())
        .or(frequency_geometry.as_ref());
    for geometry in main_geometry.into_iter().chain(geometries.iter()) {
        if n_atoms.is_some_and(|n| n != geometry.atoms.len()) {
            return Err(Error::Invalid(
                "[N_ATOMS] does not match the geometry atom count".into(),
            ));
        }
    }
    let binding = atoms
        .as_ref()
        .map(|a| &a.geometry)
        .or(geometries.last())
        .or(frequency_geometry.as_ref())
        .ok_or_else(|| Error::Invalid("The Molden file has no geometry".into()))?;
    let core = atoms
        .as_ref()
        .map(|a| a.core.clone())
        .filter(|core| core.iter().any(|&n| n != 0))
        .unwrap_or_default();
    let basis = unique(&sections, "GTO")?
        .map(|s| {
            let atoms = atoms
                .as_ref()
                .ok_or_else(|| Error::at(s.line, "[GTO] requires [Atoms]"))?;
            read_basis(s, atoms, &representations, orca)
        })
        .transpose()?;
    let shape_reason = basis
        .as_ref()
        .and_then(|b| b.basis.as_ref())
        .and_then(|b| unsupported_shapes(b, producer.as_ref()));
    if let Some(reason) = &shape_reason {
        warnings.push(reason.clone());
    }
    if basis.as_ref().is_some_and(|b| b.basis.is_none()) {
        warnings.push(format!("Orbital shapes left out (more than {MAX_BASIS_FUNCTIONS} basis functions); orbital energies are kept"));
    }
    let mut molecular = unique(&sections, "MO")?
        .map(|s| read_mos(s, basis.as_ref(), shape_reason.is_none(), orca))
        .transpose()?
        .unwrap_or_default();
    if !molecular.beta.occupations.is_empty() && molecular.missing_spins != 0 {
        warnings.push("Orbitals left out: a Beta MO is present but another MO has no Spin= header; spin channels are ambiguous".into());
        molecular = MolecularOrbitals::default();
    }
    if molecular.missing_energies > 0 {
        warnings.push(format!("Omitted {} molecular orbitals without recorded energies; their occupations are kept for complete-spectrum charge and spin checks", molecular.missing_energies));
    }
    if molecular.alpha.shapes_dropped || molecular.beta.shapes_dropped {
        warnings.push(format!("Orbital shapes left out (more than {MAX_COEFFICIENTS} coefficients per spin); orbital energies are kept"));
    }
    let inferred = infer_state(&molecular, basis.as_ref(), &binding.atoms, &core)?;
    let charge = explicit_charge
        .or(inferred.as_ref().map(|s| s.charge))
        .unwrap_or(0);
    if let Some(inferred) = &inferred {
        if explicit_charge.is_some_and(|charge| charge != inferred.charge) {
            warnings.push(format!("Explicit [Charge] disagrees with complete MO occupations (inferred {}); kept the explicit charge", inferred.charge));
        }
        if let Some(multiplicity) = explicit_multiplicity {
            let electrons = binding.atoms.iter().map(|a| i64::from(a.0)).sum::<i64>()
                - i64::from(inferred.charge);
            let unpaired = i64::from(multiplicity) - 1;
            let contradicts = electrons < unpaired
                || (electrons - unpaired) % 2 != 0
                || if inferred.known_spin {
                    multiplicity != inferred.multiplicity
                } else if inferred.unrestricted {
                    false
                } else {
                    f64::from(multiplicity - 1) > inferred.max_unpaired + 1e-6
                };
            if contradicts {
                warnings.push("Explicit [Multiplicity] disagrees with complete MO occupations; kept the explicit multiplicity".into());
            }
        } else if !inferred.unrestricted {
            // A closed shell (every occupation 0 or 2) is a singlet; only open shells are ambiguous.
            if inferred.max_unpaired > 1e-6 {
                warnings.push("Spin-summed restricted MO occupations determine only the lowest consistent multiplicity; assumed the lowest spin state".into());
            }
        } else if !inferred.known_spin {
            warnings.push("Fractional unrestricted MO occupations do not determine spin; assumed the lowest spin state".into());
        }
    }
    let multiplicity = explicit_multiplicity
        .or(inferred.as_ref().and_then(|s| {
            // An explicit charge can change electron parity. A spin inferred
            // for the MO electron count must still fit the chosen frame.
            let electrons =
                binding.atoms.iter().map(|a| i64::from(a.0)).sum::<i64>() - i64::from(charge);
            let unpaired = i64::from(s.multiplicity) - 1;
            (electrons >= unpaired && (electrons - unpaired) % 2 == 0).then_some(s.multiplicity)
        }))
        .unwrap_or_else(|| lowest_multiplicity(&binding.atoms, charge));
    if explicit_multiplicity.is_none()
        && inferred
            .as_ref()
            .is_some_and(|s| s.multiplicity != multiplicity)
    {
        warnings.push("Spin inferred from MO occupations disagrees with the explicit charge; assumed the lowest spin state".into());
    }
    if inferred.is_none() {
        match (explicit_charge.is_none(), explicit_multiplicity.is_none()) {
            (true, true) => warnings.push(
                "Charge and spin are not recorded; assumed neutral and the lowest spin state"
                    .into(),
            ),
            (true, false) => warnings.push("Charge is not recorded; assumed neutral".into()),
            (false, true) => {
                warnings.push("Spin is not recorded; assumed the lowest spin state".into())
            }
            (false, false) => {}
        }
    }
    let bound_frame = frame(
        &binding.atoms,
        binding.positions.clone(),
        charge,
        multiplicity,
    )?;
    let orbitals = make_orbitals(molecular, basis, core, multiplicity, &mut warnings);
    let vibrations = read_vibrations(
        &sections,
        binding,
        frequency_geometry.as_ref(),
        &mut warnings,
    )?;
    let energies = unique(&sections, "GEOCONV")?
        .map(read_geometry_energies)
        .transpose()?
        .unwrap_or_default();
    if !energies.is_empty() && energies.len() != geometries.len() {
        return Err(Error::Invalid(
            "[GEOCONV] energies do not match the number of [GEOMETRIES] records".into(),
        ));
    }
    let optimization = !geometries.is_empty();
    let mut steps = Vec::new();
    for (index, geometry) in geometries.iter().enumerate() {
        if geometry.atoms != binding.atoms {
            return Err(Error::Invalid(
                "[GEOMETRIES] atom order does not match the result geometry".into(),
            ));
        }
        steps.push(Step {
            frame: frame(
                &geometry.atoms,
                geometry.positions.clone(),
                charge,
                multiplicity,
            )?,
            energy_hartree: energies.get(index).copied(),
            label: geometry.label.clone(),
        });
    }
    // MO and frequency records describe [Atoms], rather than an arbitrary last path step.
    // Preserve its exact coordinates when XYZ path coordinates were printed more coarsely.
    let result_energy = if let Some(last) = geometries.last() {
        if same_geometry(last, binding, 1e-6) {
            if let Some(step) = steps.last_mut() {
                step.frame = bound_frame.clone();
            }
            energies.last().copied()
        } else {
            steps.push(Step {
                frame: bound_frame.clone(),
                energy_hartree: None,
                label: binding.label.clone(),
            });
            None
        }
    } else {
        steps.push(Step {
            frame: bound_frame.clone(),
            energy_hartree: None,
            label: binding.label.clone(),
        });
        None
    };
    let (kind, task) = if optimization {
        (JobKind::Optimization, Task::Optimize)
    } else if vibrations.is_some() {
        (JobKind::Frequencies, Task::Hessian)
    } else {
        (JobKind::SinglePoint, Task::Energy)
    };
    let mut job = Job {
        index: 0,
        kind,
        method: None,
        basis: None,
        keywords: Vec::new(),
        completion: None,
        steps,
        result: None,
    };
    let program = producer.map(|p| p.name);
    let qc_program = QcProgram {
        name: program.clone().unwrap_or_else(|| "Molden".into()),
        version: None,
    };
    let mut result = origin.result(&bound_frame, task, Status::Converged, &qc_program, &job);
    result.energy_hartree = result_energy;
    result.orbitals = orbitals;
    result.vibrations = vibrations;
    job.result = Some(result);
    Ok(QcFile {
        format: Format::Molden,
        program,
        program_version: None,
        jobs: vec![job],
        warnings,
    })
}

fn sections(text: &str) -> Result<Vec<Section<'_>>, Error> {
    let mut sections: Vec<Section<'_>> = Vec::new();
    let mut offset = 0;
    let mut body_start = 0;
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        let content = raw.trim_end_matches(['\r', '\n']);
        let line = Line {
            number: index + 1,
            text: content,
        };
        if let Some(header) = content.trim().strip_prefix('[') {
            if let Some(section) = sections.last_mut() {
                // Both offsets are sums of complete source lines and UTF-8 boundaries.
                section.body = text.split_at(offset).0.split_at(body_start).1;
            }
            let (name, qualifier) = header
                .split_once(']')
                .ok_or_else(|| Error::at(line.number, "Unclosed Molden section header"))?;
            sections.push(Section {
                name: name.trim().to_ascii_uppercase(),
                qualifier: qualifier.trim(),
                line: line.number,
                body: "",
            });
            body_start = offset + raw.len();
        } else if sections.is_empty() && is_data(content) {
            return Err(Error::at(line.number, "Expected [Molden Format] header"));
        }
        offset += raw.len();
    }
    if let Some(section) = sections.last_mut() {
        section.body = text.split_at(body_start).1;
    }
    Ok(sections)
}

fn is_data(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !text.starts_with(['#', '!', ';']) && !text.starts_with("//")
}

fn unique<'a>(sections: &'a [Section<'a>], name: &str) -> Result<Option<&'a Section<'a>>, Error> {
    let mut found = sections.iter().filter(|s| s.name == name);
    let first = found.next();
    if let Some(duplicate) = found.next() {
        return Err(Error::at(
            duplicate.line,
            format!("Repeated [{name}] section"),
        ));
    }
    Ok(first)
}

fn scalar<'a>(sections: &'a [Section<'a>], name: &str) -> Result<Option<Line<'a>>, Error> {
    let Some(section) = unique(sections, name)? else {
        return Ok(None);
    };
    let mut lines = section.data();
    let value = if is_data(section.qualifier) {
        Line {
            number: section.line,
            text: section.qualifier,
        }
    } else {
        lines
            .next()
            .ok_or_else(|| Error::at(section.line, format!("Empty [{name}] section")))?
    };
    if let Some(extra) = lines.next() {
        return Err(Error::at(
            extra.number,
            format!("[{name}] requires one value"),
        ));
    }
    Ok(Some(value))
}

fn integer<T: std::str::FromStr>(text: &str, line: usize, what: &str) -> Result<T, Error> {
    text.trim()
        .parse()
        .map_err(|_| Error::at(line, format!("Invalid {what}: {text}")))
}

fn real(text: &str, line: usize, what: &str) -> Result<f64, Error> {
    let normalized = if text.contains(['D', 'd']) {
        std::borrow::Cow::Owned(text.replace(['D', 'd'], "E"))
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    let value = normalized
        .parse::<f64>()
        .map_err(|_| Error::at(line, format!("Invalid {what}: {text}")))?;
    if !value.is_finite() {
        return Err(Error::at(line, format!("{what} must be finite")));
    }
    Ok(value)
}

fn symbol(text: &str, line: usize) -> Result<(u8, u16), Error> {
    let symbol: String = text.chars().filter(|c| !c.is_ascii_digit()).collect();
    element(&symbol).ok_or_else(|| Error::at(line, format!("Unknown element label {text}")))
}

fn coordinates(fields: &[&str], line: usize, scale: f64) -> Result<[f64; 3], Error> {
    if fields.len() != 3 {
        return Err(Error::at(line, "Expected three Cartesian coordinates"));
    }
    let mut xyz = [0.0; 3];
    for (coordinate, text) in xyz.iter_mut().zip(fields) {
        *coordinate = real(text, line, "coordinate")? * scale;
        if !coordinate.is_finite() {
            return Err(Error::at(line, "Coordinate is outside the supported range"));
        }
    }
    Ok(xyz)
}

fn check_atom_count(count: usize, line: usize) -> Result<(), Error> {
    if count == 0 {
        Err(Error::at(line, "A geometry must contain at least one atom"))
    } else if count > MAX_FRAME_ATOMS {
        Err(Error::Unsupported(format!(
            "The geometry has more than {MAX_FRAME_ATOMS} atoms"
        )))
    } else {
        Ok(())
    }
}

fn read_atoms(section: &Section<'_>, label: Option<String>) -> Result<Atoms, Error> {
    let units = section.qualifier.trim_matches(['(', ')']).trim();
    let scale = if units.eq_ignore_ascii_case("AU") {
        ANGSTROM_PER_BOHR
    } else if units.eq_ignore_ascii_case("ANGS") {
        1.0
    } else {
        return Err(Error::at(section.line, "[Atoms] must specify Angs or AU"));
    };
    let mut geometry = Geometry {
        atoms: Vec::new(),
        positions: Vec::new(),
        label,
    };
    let mut indices = BTreeMap::new();
    let mut core = Vec::new();
    for line in section.data() {
        let fields: Vec<_> = line.text.split_whitespace().collect();
        let [label, index, effective_z, x, y, z] = fields.as_slice() else {
            return Err(Error::at(
                line.number,
                "An [Atoms] record requires label, index, atomic number and xyz",
            ));
        };
        check_atom_count(geometry.atoms.len() + 1, line.number)?;
        let atom = symbol(label, line.number)?;
        let index: usize = integer(index, line.number, "atom index")?;
        let effective_z: u32 = integer(effective_z, line.number, "effective atomic number")?;
        if index == 0 || indices.insert(index, geometry.atoms.len() as u32).is_some() {
            return Err(Error::at(
                line.number,
                "Atom indices must be positive and distinct",
            ));
        }
        if effective_z > u32::from(atom.0) {
            return Err(Error::at(
                line.number,
                "Effective atomic number exceeds the element's atomic number",
            ));
        }
        core.push(u32::from(atom.0) - effective_z);
        geometry.atoms.push(atom);
        geometry
            .positions
            .push(coordinates(&[x, y, z], line.number, scale)?);
    }
    check_atom_count(geometry.atoms.len(), section.line)?;
    Ok(Atoms {
        geometry,
        indices,
        core,
    })
}

fn coordinate_geometry(section: &Section<'_>, scale: f64) -> Result<Geometry, Error> {
    let mut geometry = Geometry {
        atoms: Vec::new(),
        positions: Vec::new(),
        label: None,
    };
    for line in section.data() {
        let fields: Vec<_> = line.text.split_whitespace().collect();
        let [label, x, y, z] = fields.as_slice() else {
            return Err(Error::at(
                line.number,
                "Expected element and xyz coordinates",
            ));
        };
        check_atom_count(geometry.atoms.len() + 1, line.number)?;
        geometry.atoms.push(symbol(label, line.number)?);
        geometry
            .positions
            .push(coordinates(&[x, y, z], line.number, scale)?);
    }
    check_atom_count(geometry.atoms.len(), section.line)?;
    Ok(geometry)
}

/// The [Pseudo] extension records label, source atom index and effective nuclear
/// charge. Core electrons come only from this metadata or [Atoms], never from a
/// short occupied spectrum.
fn read_pseudo(section: &Section<'_>, atoms: &mut Atoms) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for line in section.data() {
        let fields: Vec<_> = line.text.split_whitespace().collect();
        let [label, index, effective] = fields.as_slice() else {
            return Err(Error::at(
                line.number,
                "A [Pseudo] record requires label, atom index and effective nuclear charge",
            ));
        };
        let index: usize = integer(index, line.number, "pseudopotential atom index")?;
        let atom = atoms
            .indices
            .get(&index)
            .and_then(|&index| atoms.geometry.atoms.get(index as usize))
            .ok_or_else(|| {
                Error::at(line.number, "[Pseudo] atom index is not present in [Atoms]")
            })?;
        if symbol(label, line.number)?.0 != atom.0 {
            return Err(Error::at(
                line.number,
                "[Pseudo] label does not match [Atoms]",
            ));
        }
        if !seen.insert(index) {
            return Err(Error::at(line.number, "Repeated [Pseudo] atom index"));
        }
        let effective = near_integer(real(effective, line.number, "effective nuclear charge")?)
            .filter(|&effective| effective <= i64::from(atom.0))
            .ok_or_else(|| Error::at(line.number, "Effective nuclear charge must be an integer between zero and the atomic number"))?;
        let core = u32::from(atom.0) - effective as u32;
        let recorded = atoms
            .indices
            .get(&index)
            .and_then(|&index| atoms.core.get_mut(index as usize))
            .ok_or_else(|| {
                Error::at(line.number, "[Pseudo] atom index is not present in [Atoms]")
            })?;
        if *recorded != 0 && *recorded != core {
            return Err(Error::at(
                line.number,
                "[Pseudo] and [Atoms] effective charges disagree",
            ));
        }
        *recorded = core;
    }
    Ok(())
}

fn read_representations(sections: &[Section<'_>]) -> Result<BTreeMap<u8, bool>, Error> {
    let split_pure_flags = ["5D", "7F"]
        .iter()
        .all(|flag| sections.iter().any(|s| s.name == *flag));
    let mut representations = BTreeMap::new();
    for section in sections {
        let flags: &[(u8, bool)] = match section.name.as_str() {
            // PySCF and orca_2mkl can write the pure pair on separate lines.
            "5D" if split_pure_flags => &[(2, true)],
            "7F" if split_pure_flags => &[(3, true)],
            // A lone short flag follows the Molden specification.
            "5D" | "5D7F" => &[(2, true), (3, true)],
            "5D10F" => &[(2, true), (3, false)],
            "7F" | "6D7F" => &[(2, false), (3, true)],
            "6D10F" => &[(2, false), (3, false)],
            "6D" => &[(2, false)],
            "10F" => &[(3, false)],
            "9G" => &[(4, true)],
            "15G" => &[(4, false)],
            _ => &[],
        };
        for &(l, pure) in flags {
            if representations
                .insert(l, pure)
                .is_some_and(|old| old != pure)
            {
                return Err(Error::at(
                    section.line,
                    format!("Conflicting pure and Cartesian flags for l={l}"),
                ));
            }
        }
    }
    Ok(representations)
}

fn read_basis(
    section: &Section<'_>,
    atoms: &Atoms,
    representations: &BTreeMap<u8, bool>,
    orca: bool,
) -> Result<GaussianBasis, Error> {
    let mut lines = section
        .lines()
        .filter(|line| line.text.trim().is_empty() || is_data(line.text));
    let mut centre = None;
    let mut shells = Some(Vec::new());
    let mut n_ao = 0usize;
    while let Some(line) = lines.next() {
        if line.text.trim().is_empty() {
            centre = None;
            continue;
        }
        let fields: Vec<_> = line.text.split_whitespace().collect();
        if let Some(index) = fields.first().filter(|s| s.parse::<usize>().is_ok()) {
            if !matches!(fields.as_slice(), [_] | [_, "0"]) {
                return Err(Error::at(line.number, "Invalid GTO atom header"));
            }
            let index: usize = integer(index, line.number, "basis atom index")?;
            centre = Some(*atoms.indices.get(&index).ok_or_else(|| {
                Error::at(line.number, "GTO atom index is not present in [Atoms]")
            })?);
            continue;
        }
        let (label, count) = match fields.as_slice() {
            [label, count] | [label, count, _] => (*label, *count),
            _ => return Err(Error::at(line.number, "Invalid GTO shell header")),
        };
        let l = match label.to_ascii_lowercase().as_str() {
            "s" | "sp" => 0,
            "p" => 1,
            "d" => 2,
            "f" => 3,
            "g" => 4,
            label => {
                return Err(Error::Unsupported(format!(
                    "Molden shell {label} is not supported"
                )));
            }
        };
        let sp = label.eq_ignore_ascii_case("sp");
        let atom = centre.ok_or_else(|| Error::at(line.number, "GTO shell has no atom header"))?;
        let primitives: usize = integer(count, line.number, "primitive count")?;
        if primitives == 0 || primitives > MAX_PRIMITIVES {
            return Err(Error::Unsupported(format!(
                "Molden shells must have 1 to {MAX_PRIMITIVES} primitives"
            )));
        }
        // The shell header's obsolete third field is nonfunctional in Molden.
        let mut exponents = Vec::with_capacity(primitives);
        let mut coefficients = Vec::with_capacity(primitives);
        let mut p_coefficients = Vec::with_capacity(if sp { primitives } else { 0 });
        for _ in 0..primitives {
            let primitive = lines
                .next()
                .ok_or_else(|| Error::at(line.number, "Truncated GTO primitive list"))?;
            let values: Vec<_> = primitive.text.split_whitespace().collect();
            let (exponent, coefficient, p_coefficient) = match (sp, values.as_slice()) {
                (false, [exponent, coefficient]) => (*exponent, *coefficient, None),
                (true, [exponent, coefficient, p_coefficient]) => {
                    (*exponent, *coefficient, Some(*p_coefficient))
                }
                _ => return Err(Error::at(primitive.number, "Invalid GTO primitive record")),
            };
            exponents.push(real(exponent, primitive.number, "Gaussian exponent")?);
            coefficients.push(real(
                coefficient,
                primitive.number,
                "contraction coefficient",
            )?);
            if let Some(coefficient) = p_coefficient {
                p_coefficients.push(real(
                    coefficient,
                    primitive.number,
                    "p contraction coefficient",
                )?);
            }
        }
        let mut shell = Shell {
            atom,
            l,
            pure: representations.get(&l).copied().unwrap_or(false),
            exponents_per_bohr2: exponents,
            coefficients,
        };
        let mut p_shell = sp.then(|| Shell {
            l: 1,
            pure: false,
            coefficients: p_coefficients,
            ..shell.clone()
        });
        for shell in std::iter::once(&mut shell).chain(p_shell.as_mut()) {
            if orca {
                for (&alpha, coefficient) in shell
                    .exponents_per_bohr2
                    .iter()
                    .zip(&mut shell.coefficients)
                {
                    *coefficient /= primitive_norm(alpha, shell.l);
                }
            }
            shell.validate(atoms.geometry.atoms.len())?;
        }
        n_ao = n_ao
            .checked_add(shell.n_functions() + p_shell.as_ref().map_or(0, Shell::n_functions))
            .ok_or_else(|| Error::Unsupported("Too many Molden basis functions".into()))?;
        if n_ao > MAX_BASIS_FUNCTIONS {
            shells = None;
        }
        if let Some(shells) = &mut shells {
            shells.push(shell);
            shells.extend(p_shell);
        }
    }
    if n_ao == 0 {
        return Err(Error::at(section.line, "Empty [GTO] basis"));
    }
    Ok(GaussianBasis {
        basis: shells.map(|shells| BasisSet { shells }),
        n_ao,
    })
}

fn read_mos(
    section: &Section<'_>,
    basis: Option<&GaussianBasis>,
    keep_shapes: bool,
    orca: bool,
) -> Result<MolecularOrbitals, Error> {
    let mut molecular = MolecularOrbitals::default();
    let mut current = Mo::default();
    for line in section.data() {
        if let Some((key, value)) = line.text.split_once('=') {
            let key = key.trim().to_ascii_uppercase();
            let field = match key.as_str() {
                "SYM" => MoHeader::Symmetry,
                "ENE" => MoHeader::Energy,
                "SPIN" => MoHeader::Spin,
                "OCCUP" => MoHeader::Occupation,
                _ => return Err(Error::at(line.number, format!("Unknown MO header {key}"))),
            };
            if !current.coefficients.is_empty() || current.fields & field as u8 != 0 {
                finish_mo(
                    &mut molecular,
                    std::mem::take(&mut current),
                    basis,
                    keep_shapes,
                    orca,
                )?;
            }
            let value = value.trim();
            match field {
                MoHeader::Symmetry => current.symmetry = Some(value.into()),
                MoHeader::Energy => {
                    current.energy = Some(real(value, line.number, "orbital energy")?)
                }
                MoHeader::Spin => {
                    current.beta = if value.eq_ignore_ascii_case("beta") {
                        true
                    } else if value.eq_ignore_ascii_case("alpha") {
                        false
                    } else {
                        return Err(Error::at(line.number, "MO spin must be Alpha or Beta"));
                    }
                }
                MoHeader::Occupation => {
                    current.occupation = Some(real(value, line.number, "orbital occupation")?)
                }
            }
            current.fields |= field as u8;
        } else {
            let fields: Vec<_> = line.text.split_whitespace().collect();
            let [index, coefficient] = fields.as_slice() else {
                return Err(Error::at(
                    line.number,
                    "Expected AO index and MO coefficient",
                ));
            };
            let basis = basis
                .ok_or_else(|| Error::at(line.number, "MO coefficients require a [GTO] basis"))?;
            let index: usize = integer(index, line.number, "AO index")?;
            if index == 0 || index > basis.n_ao {
                return Err(Error::at(
                    line.number,
                    "MO coefficient AO index is out of range",
                ));
            }
            let value = real(coefficient, line.number, "MO coefficient")?;
            if !(value as f32).is_finite() {
                return Err(Error::at(
                    line.number,
                    "MO coefficient is outside the supported range",
                ));
            }
            current.coefficients.push((index - 1, value));
            if current.coefficients.len() > basis.n_ao {
                return Err(Error::at(line.number, "Repeated MO coefficient AO index"));
            }
        }
    }
    if current.fields != 0 || !current.coefficients.is_empty() {
        finish_mo(&mut molecular, current, basis, keep_shapes, orca)?;
    }
    // Sparse entries in an existing row are zero. A header without any
    // coefficients supplies only a level, not evidence of a zero-valued MO.
    Ok(molecular)
}

#[allow(
    clippy::indexing_slicing,
    reason = "Sparse indices were checked by read_mos and canonical blocks by mo_from_molden"
)]
fn finish_mo(
    molecular: &mut MolecularOrbitals,
    mut mo: Mo,
    basis: Option<&GaussianBasis>,
    keep_shapes: bool,
    orca: bool,
) -> Result<(), Error> {
    mo.coefficients.sort_unstable_by_key(|&(index, _)| index);
    if mo
        .coefficients
        .windows(2)
        .any(|pair| matches!(pair, [a, b] if a.0 == b.0))
    {
        return Err(Error::Invalid("Repeated MO coefficient AO index".into()));
    }
    if mo.fields & MoHeader::Spin as u8 == 0 {
        molecular.missing_spins += 1;
    }
    let channel = if mo.beta {
        &mut molecular.beta
    } else {
        &mut molecular.alpha
    };
    channel.occupations.push(mo.occupation);
    let Some(energy) = mo.energy else {
        molecular.missing_energies += 1;
        return Ok(());
    };
    let coefficients = if !keep_shapes || channel.shapes_dropped || mo.coefficients.is_empty() {
        None
    } else if let Some(GaussianBasis {
        basis: Some(basis),
        n_ao,
    }) = basis
    {
        if n_ao
            .checked_mul(channel.rows + 1)
            .is_none_or(|n| n > MAX_COEFFICIENTS)
        {
            channel.shapes_dropped = true;
            for level in &mut channel.levels {
                level.coefficients = None;
            }
            None
        } else {
            let mut row = vec![0.0; *n_ao];
            for (index, coefficient) in mo.coefficients {
                row[index] = coefficient;
            }
            channel.rows += 1;
            let mut canonical = mo_from_molden(basis, &row, CartesianNorm::Unit)?;
            if orca {
                let mut offset = 0;
                for shell in &basis.shells {
                    let end = offset + shell.n_functions();
                    if shell.l == 3 && shell.pure {
                        // The ORCA water TZVP implied overlap verifies the f(±3) sign flips.
                        canonical[offset] *= -1.0;
                        canonical[end - 1] *= -1.0;
                    }
                    offset = end;
                }
            }
            Some(canonical)
        }
    } else {
        None
    };
    channel.levels.push(Level {
        energy,
        occupation: mo.occupation,
        symmetry: mo.symmetry,
        coefficients,
    });
    Ok(())
}

fn near_integer(value: f64) -> Option<i64> {
    let rounded = value.round();
    (value.is_finite()
        && (value - rounded).abs() <= 1e-6
        && rounded >= 0.0
        && rounded < i64::MAX as f64)
        .then_some(rounded as i64)
}

fn infer_state(
    molecular: &MolecularOrbitals,
    basis: Option<&GaussianBasis>,
    atoms: &[(u8, u16)],
    core: &[u32],
) -> Result<Option<InferredState>, Error> {
    let unrestricted = !molecular.beta.occupations.is_empty();
    let max = if unrestricted { 1.0 } else { 2.0 };
    let all = molecular
        .alpha
        .occupations
        .iter()
        .chain(&molecular.beta.occupations);
    if all
        .clone()
        .flatten()
        .any(|&n| !(-1e-6..=max + 1e-6).contains(&n))
    {
        return Err(Error::Invalid(
            "MO occupation is outside its spin channel's range".into(),
        ));
    }
    // Occupied-only and truncated exports cannot establish molecular charge.
    // Count every MO header, including those without an energy, because its
    // recorded Occup= still contributes known electrons.
    let complete = basis.is_some_and(|basis| {
        molecular.alpha.occupations.len() == basis.n_ao
            && (!unrestricted || molecular.beta.occupations.len() == basis.n_ao)
    });
    if !complete || all.clone().any(Option::is_none) {
        return Ok(None);
    }
    let alpha = molecular.alpha.occupations.iter().flatten().sum::<f64>();
    let beta = molecular.beta.occupations.iter().flatten().sum::<f64>();
    let Some(electrons) = near_integer(alpha + beta) else {
        return Ok(None);
    };
    let spin = if unrestricted {
        near_integer(alpha)
            .zip(near_integer(beta))
            .map(|(alpha, beta)| {
                u32::try_from(alpha.abs_diff(beta) + 1).map_err(|_| {
                    Error::Invalid("Inferred Molden spin is outside the supported range".into())
                })
            })
            .transpose()?
    } else {
        None
    };
    let electrons = electrons + core.iter().map(|&n| i64::from(n)).sum::<i64>();
    let nuclei = atoms.iter().map(|&(z, _)| i64::from(z)).sum::<i64>();
    let charge = i32::try_from(nuclei - electrons).map_err(|_| {
        Error::Invalid("Inferred Molden charge is outside the supported range".into())
    })?;
    let multiplicity = spin.unwrap_or_else(|| lowest_multiplicity(atoms, charge));
    let max_unpaired = molecular
        .alpha
        .occupations
        .iter()
        .flatten()
        .map(|&n| n.min(2.0 - n).max(0.0))
        .sum();
    Ok(Some(InferredState {
        charge,
        multiplicity,
        unrestricted,
        known_spin: spin.is_some(),
        max_unpaired,
    }))
}

fn spectrum(mut channel: Channel) -> OrbitalSet {
    // Preserve source order for equal energies, including the two representations of zero.
    channel.levels.sort_by(|a, b| {
        if a.energy == b.energy {
            std::cmp::Ordering::Equal
        } else {
            a.energy.total_cmp(&b.energy)
        }
    });
    let occupations = channel
        .levels
        .iter()
        .map(|l| l.occupation)
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    let symmetries = if channel.levels.iter().any(|l| l.symmetry.is_some()) {
        channel
            .levels
            .iter()
            .map(|l| l.symmetry.clone().unwrap_or_default())
            .collect()
    } else {
        Vec::new()
    };
    let mut set = OrbitalSet {
        energies_hartree: Vec::with_capacity(channel.levels.len()),
        occupations,
        symmetries,
        rows: Vec::new(),
        coefficients: Vec::new(),
    };
    for (index, level) in channel.levels.into_iter().enumerate() {
        set.energies_hartree.push(level.energy);
        if let Some(coefficients) = level.coefficients {
            set.rows.push(index as u32);
            set.coefficients.extend(coefficients);
        }
    }
    set
}

fn make_orbitals(
    molecular: MolecularOrbitals,
    basis: Option<GaussianBasis>,
    core_electrons: Vec<u32>,
    multiplicity: u32,
    warnings: &mut Vec<String>,
) -> Option<Orbitals> {
    if molecular.alpha.levels.is_empty() && molecular.beta.levels.is_empty() {
        return None;
    }
    let unrestricted = !molecular.beta.occupations.is_empty();
    // A spin-summed record can be split uniquely only when the complete
    // spectrum has integer 0/1/2 occupations and every singly occupied MO
    // accounts for one electron of the selected spin imbalance.
    let shared_spin_known = basis.as_ref().is_some_and(|basis| {
        molecular.alpha.occupations.len() == basis.n_ao
            && molecular
                .alpha
                .occupations
                .iter()
                .all(|n| n.is_some_and(|n| near_integer(n).is_some_and(|n| n <= 2)))
            && molecular
                .alpha
                .occupations
                .iter()
                .flatten()
                .filter(|&&n| (n - 1.0).abs() <= 1e-6)
                .count() as u64
                == u64::from(multiplicity - 1)
    });
    let mut alpha = spectrum(molecular.alpha);
    let beta = if unrestricted {
        Beta::Unrestricted(spectrum(molecular.beta))
    } else if multiplicity != 1 {
        // Shared open-shell orbitals require per-spin populations. Preserve
        // them only when the source determines the split; never invent which
        // singly occupied MO carries alpha or beta spin.
        let mut occupations = Vec::with_capacity(alpha.occupations.len());
        if shared_spin_known {
            for occupation in &mut alpha.occupations {
                let (a, b) = if (*occupation - 1.0).abs() <= 1e-6 {
                    (*occupation, 0.0)
                } else {
                    (*occupation / 2.0, *occupation / 2.0)
                };
                *occupation = a;
                occupations.push(b);
            }
        } else {
            let had_occupations = !alpha.occupations.is_empty();
            alpha.occupations.clear();
            if had_occupations {
                warnings.push("Per-spin occupations left out: spin-summed MO occupations do not uniquely determine their spin assignment; orbital energies and available shapes are kept".into());
            }
        }
        Beta::SameOrbitals { occupations }
    } else {
        Beta::Restricted
    };
    let has_rows =
        !alpha.rows.is_empty() || matches!(&beta, Beta::Unrestricted(s) if !s.rows.is_empty());
    Some(Orbitals {
        basis: if has_rows {
            basis.and_then(|b| b.basis)
        } else {
            None
        },
        core_electrons,
        alpha,
        beta,
    })
}

fn same_geometry(left: &Geometry, right: &Geometry, tolerance: f64) -> bool {
    left.atoms
        .iter()
        .map(|a| a.0)
        .eq(right.atoms.iter().map(|a| a.0))
        && left.positions.len() == right.positions.len()
        && left
            .positions
            .iter()
            .zip(&right.positions)
            .all(|(&[ax, ay, az], &[bx, by, bz])| {
                (ax - bx).hypot(ay - by).hypot(az - bz) <= tolerance
            })
}

fn read_vibrations(
    sections: &[Section<'_>],
    binding: &Geometry,
    frequency_geometry: Option<&Geometry>,
    warnings: &mut Vec<String>,
) -> Result<Option<Vibrations>, Error> {
    let Some(section) = unique(sections, "FREQ")? else {
        return Ok(None);
    };
    if !frequency_geometry.is_some_and(|g| same_geometry(binding, g, 1e-4)) {
        warnings.push("Frequencies left out: [FR-COORD] is missing or does not match the result atom order and geometry within 0.0001 Å".into());
        return Ok(None);
    }
    let mut frequencies = Vec::new();
    for line in section.data() {
        frequencies.push(real(line.text.trim(), line.number, "frequency")?);
        if frequencies.len() > 3 * binding.atoms.len() {
            return Err(Error::at(
                line.number,
                "More frequencies than Cartesian coordinates",
            ));
        }
    }
    if binding.atoms.len() > MAX_HESSIAN_ATOMS {
        warnings.push(format!(
            "Frequencies left out: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms"
        ));
        return Ok(None);
    }
    let intensities = unique(sections, "INT")?
        .map(|s| {
            s.data()
                .map(|line| {
                    let first =
                        line.text.split_whitespace().next().ok_or_else(|| {
                            Error::at(line.number, "Expected an infrared intensity")
                        })?;
                    let value = real(first, line.number, "infrared intensity")?;
                    if value < 0.0 {
                        return Err(Error::at(
                            line.number,
                            "Infrared intensity must be nonnegative",
                        ));
                    }
                    Ok(value)
                })
                .collect::<Result<Vec<_>, Error>>()
        })
        .transpose()?
        .unwrap_or_default();
    if !intensities.is_empty() && intensities.len() != frequencies.len() {
        return Err(Error::Invalid("[INT] count does not match [FREQ]".into()));
    }
    let modes = unique(sections, "FR-NORM-COORD")?
        .map(|s| read_modes(s, frequencies.len(), binding.atoms.len()))
        .transpose()?;
    let mut intensity_values = intensities.into_iter();
    let mut mode_values = modes.iter().flatten();
    // Form aligned records before filtering and sorting, so the optional arrays
    // stay with the frequency they describe without indexing parallel vectors.
    let mut records: Vec<_> = frequencies
        .into_iter()
        .map(|frequency| (frequency, intensity_values.next(), mode_values.next()))
        .filter(|(frequency, _, _)| *frequency != 0.0)
        .collect();
    records.sort_by(|(a, _, _), (b, _, _)| a.total_cmp(b));
    let mut vibrations = Vibrations {
        frequencies_cm1: Vec::with_capacity(records.len()),
        ir_intensities_km_mol: Vec::new(),
        modes: Vec::new(),
        reduced_masses_amu: Vec::new(),
        masses_amu: Vec::new(),
    };
    for (frequency, intensity, mode) in records {
        vibrations.frequencies_cm1.push(frequency);
        if let Some(intensity) = intensity {
            vibrations.ir_intensities_km_mol.push(intensity);
        }
        if let Some(mode) = mode {
            let max = mode.iter().fold(0.0_f64, |max, value| max.max(value.abs()));
            if max == 0.0 {
                return Err(Error::Invalid(
                    "A vibrational mode has zero displacement".into(),
                ));
            }
            let norm = mode
                .iter()
                .map(|value| (value / max).powi(2))
                .sum::<f64>()
                .sqrt();
            vibrations
                .modes
                .extend(mode.iter().map(|value| (value / max) / norm));
        }
    }
    Ok(Some(vibrations))
}

fn read_modes(section: &Section<'_>, count: usize, n_atoms: usize) -> Result<Vec<Vec<f64>>, Error> {
    let mut modes: Vec<Option<Vec<f64>>> = vec![None; count];
    let mut lines = section.data();
    while let Some(line) = lines.next() {
        let fields: Vec<_> = line.text.split_whitespace().collect();
        let [label, index] = fields.as_slice() else {
            return Err(Error::at(
                line.number,
                "Expected vibration and its mode number",
            ));
        };
        if !label.eq_ignore_ascii_case("vibration") {
            return Err(Error::at(line.number, "Expected a vibration header"));
        }
        let index: usize = integer(index, line.number, "vibration number")?;
        let target = index
            .checked_sub(1)
            .and_then(|index| modes.get_mut(index))
            .ok_or_else(|| {
                Error::at(line.number, "Vibration number is repeated or out of range")
            })?;
        if target.is_some() {
            return Err(Error::at(
                line.number,
                "Vibration number is repeated or out of range",
            ));
        }
        let mut mode = Vec::with_capacity(n_atoms * 3);
        for _ in 0..n_atoms {
            let atom = lines
                .next()
                .ok_or_else(|| Error::at(line.number, "Truncated vibrational displacement list"))?;
            let values: Vec<_> = atom.text.split_whitespace().collect();
            mode.extend(coordinates(&values, atom.number, 1.0)?);
        }
        *target = Some(mode);
    }
    modes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            Error::at(
                section.line,
                "[FR-NORM-COORD] has missing vibrational modes",
            )
        })
}

fn read_geometries(section: &Section<'_>) -> Result<Vec<Geometry>, Error> {
    if !section
        .qualifier
        .trim_matches(['(', ')'])
        .trim()
        .eq_ignore_ascii_case("XYZ")
    {
        return Err(Error::Unsupported(
            "Only XYZ [GEOMETRIES] records are supported".into(),
        ));
    }
    let mut lines = section.lines();
    let mut geometries = Vec::new();
    let mut total_atoms = 0usize;
    while let Some(line) = lines.find(|line| is_data(line.text)) {
        let count: usize = integer(line.text.trim(), line.number, "XYZ atom count")?;
        check_atom_count(count, line.number)?;
        total_atoms = total_atoms
            .checked_add(count)
            .ok_or_else(|| Error::Unsupported("Too many Molden trajectory atoms".into()))?;
        if total_atoms > MAX_TRAJECTORY_ATOMS {
            return Err(Error::Unsupported(format!(
                "Molden geometries contain more than {MAX_TRAJECTORY_ATOMS} trajectory atoms"
            )));
        }
        let title = lines
            .next()
            .ok_or_else(|| Error::at(line.number, "Missing XYZ title line"))?;
        let mut geometry = Geometry {
            atoms: Vec::with_capacity(count),
            positions: Vec::with_capacity(count),
            label: (!title.text.trim().is_empty()).then(|| title.text.trim().into()),
        };
        for _ in 0..count {
            let atom = lines
                .next()
                .ok_or_else(|| Error::at(line.number, "Truncated XYZ geometry"))?;
            let fields: Vec<_> = atom.text.split_whitespace().collect();
            let [label, x, y, z] = fields.as_slice() else {
                return Err(Error::at(
                    atom.number,
                    "Expected XYZ element and coordinates",
                ));
            };
            geometry.atoms.push(symbol(label, atom.number)?);
            geometry
                .positions
                .push(coordinates(&[x, y, z], atom.number, 1.0)?);
        }
        geometries.push(geometry);
    }
    if geometries.is_empty() {
        return Err(Error::at(section.line, "Empty [GEOMETRIES] section"));
    }
    Ok(geometries)
}

fn read_geometry_energies(section: &Section<'_>) -> Result<Vec<f64>, Error> {
    let mut energy = false;
    let mut found = false;
    let mut values = Vec::new();
    for line in section.data() {
        let text = line.text.trim();
        if text.eq_ignore_ascii_case("energy") {
            if found {
                return Err(Error::at(line.number, "Repeated GEOCONV energy block"));
            }
            found = true;
            energy = true;
        } else if ["max-force", "rms-force", "max-step", "rms-step"]
            .iter()
            .any(|label| text.eq_ignore_ascii_case(label))
        {
            energy = false;
        } else if energy {
            values.push(real(text, line.number, "geometry energy")?);
        }
    }
    Ok(values)
}

fn producer(text: &str, sections: &[Section<'_>]) -> Option<Producer> {
    let metadata = sections
        .iter()
        .filter(|s| matches!(s.name.as_str(), "TITLE" | "MOLDEN FORMAT"))
        .flat_map(|s| s.lines().map(|l| l.text));
    let comments = text.lines().filter(|line| {
        line.trim_start().starts_with(['#', '!', ';']) || line.trim_start().starts_with("//")
    });
    let lines: Vec<_> = comments.chain(metadata).collect();
    // Explicit generators win over titles such as "Molden output".
    let explicit = lines.iter().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        let (_, offset) = [
            "generated by ",
            "created by ",
            "written by ",
            "produced by ",
            "made by ",
            "producer:",
        ]
        .into_iter()
        .filter_map(|prefix| {
            lower
                .find(prefix)
                .map(|start| (start, start + prefix.len()))
        })
        .min()?;
        // ASCII case folding preserves all original byte boundaries.
        let token = producer_tokens(&line[offset..]).next()?;
        let name = if token.eq_ignore_ascii_case("molden") {
            "Molden"
        } else {
            named_producer(&token.to_ascii_lowercase()).unwrap_or(token)
        };
        Some(name.to_owned())
    });
    let name = explicit.or_else(|| {
        let mut generic_molden = false;
        for line in &lines {
            let lower = line.to_ascii_lowercase();
            if let Some(name) = named_producer(&lower) {
                return Some(name.into());
            }
            generic_molden |= producer_tokens(&lower).any(|token| token == "molden");
        }
        generic_molden.then(|| "Molden".into())
    })?;
    let verified_writer = match name.as_str() {
        "ORCA" => lines.iter().any(|line| {
            line.split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_'))
                .any(|token| token.eq_ignore_ascii_case("orca_2mkl"))
        }),
        "PySCF" => lines
            .iter()
            .find_map(|line| pyscf_major_version(&line.to_ascii_lowercase()))
            .is_some_and(|major| major >= 2),
        _ => false,
    };
    Some(Producer {
        name,
        verified_writer,
    })
}

fn producer_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|token| !token.is_empty())
}

fn named_producer(lower: &str) -> Option<&'static str> {
    producer_tokens(lower).find_map(|token| match token {
        "orca" => Some("ORCA"),
        "psi4" => Some("Psi4"),
        "pyscf" => Some("PySCF"),
        "xtb" => Some("xtb"),
        "gaussian" => Some("Gaussian"),
        "gamess" => Some("GAMESS"),
        "turbomole" => Some("Turbomole"),
        "q-chem" => Some("Q-Chem"),
        "nwchem" => Some("NWChem"),
        "molpro" => Some("Molpro"),
        "cfour" => Some("CFOUR"),
        _ => None,
    })
}

fn pyscf_major_version(lower: &str) -> Option<u32> {
    let (_, version) = lower.split_once("made by pyscf v[")?;
    let (version, _) = version.split_once(']')?;
    let (major, rest) = version.split_once('.')?;
    let (minor, patch) = rest.split_once('.')?;
    minor.parse::<u32>().ok()?;
    patch.parse::<u32>().ok()?;
    major.parse().ok()
}

fn unsupported_shapes(basis: &BasisSet, producer: Option<&Producer>) -> Option<String> {
    let Producer {
        name,
        verified_writer,
    } = producer?;
    let orca = name == "ORCA";
    if orca && !verified_writer {
        return Some("Orbital shapes left out: ORCA radial primitive normalization is unverified without the orca_2mkl writer signature; orbital energies are kept".into());
    }
    if name == "Molden" || (name == "PySCF" && *verified_writer) {
        return None;
    }
    let mut functions = BTreeSet::new();
    for shell in &basis.shells {
        let name = match (shell.l, shell.pure) {
            (2, true) if !orca => "pure d",
            (3, true) if !orca => "pure f",
            (4, true) => "pure g",
            (2, false) => "Cartesian d",
            (3, false) => "Cartesian f",
            (4, false) => "Cartesian g",
            _ => continue,
        };
        functions.insert(name);
    }
    if functions.is_empty() {
        return None;
    }
    Some(format!(
        "Orbital shapes left out: {} functions from {name} have no verified Molden convention; orbital energies are kept",
        functions.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

#[cfg(test)]
#[path = "tests/molden.rs"]
mod tests;
