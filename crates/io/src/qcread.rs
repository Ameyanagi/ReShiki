//! Readers for quantum chemistry files, from geometries to calculated results.
//!
//! Geometries come from multi-frame XYZ, SDF and Gaussian, ORCA and GAMESS inputs; results from
//! Molden, Gaussian fchk, Gaussian logs and ORCA outputs.
//!
//! A file becomes jobs, and each job a list of steps (geometries) and at most one result
//! bound by hash to the step it describes, so a value is never paired with the geometry of
//! another job or step. Readers build exact `f64` frames from the file's own records:
//! explicit hydrogens stay, nothing is deduplicated, and the drawing is never touched.
use reshiki_qcdata::{
    CalcResult, Frame, FrameHash, MAX_FRAMES, MAX_HESSIAN_ATOMS, Provenance, Settings, Source,
    Status, Task, Trajectory,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod fchk;
mod gaussian;
mod input;
mod molden;
mod orca;
mod sdf;
mod xyz;

#[cfg(test)]
mod tests;

/// The largest file the readers accept.
pub const MAX_FILE_BYTES: usize = 128 * 1024 * 1024;

/// A file that cannot be read, in words for the user.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// The file breaks its format, with the line where it does when known.
    #[error("{0}")]
    Invalid(String),
    /// The file is valid but uses something these readers do not support.
    #[error("{0}")]
    Unsupported(String),
}

impl Error {
    /// An invalid-file error at a one-based line.
    pub(crate) fn at(line: usize, message: impl std::fmt::Display) -> Self {
        Self::Invalid(format!("Line {line}: {message}"))
    }
}

impl From<reshiki_qcdata::Error> for Error {
    fn from(error: reshiki_qcdata::Error) -> Self {
        match error {
            reshiki_qcdata::Error::Unsupported(message) => Self::Unsupported(message),
            error => Self::Invalid(error.to_string()),
        }
    }
}

/// The file formats read here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Xyz,
    Sdf,
    Molden,
    Fchk,
    GaussianLog,
    OrcaOutput,
    GaussianInput,
    OrcaInput,
    GamessInput,
}

impl Format {
    /// The format's name for messages and provenance.
    pub fn name(self) -> &'static str {
        match self {
            Self::Xyz => "XYZ",
            Self::Sdf => "SDF",
            Self::Molden => "Molden",
            Self::Fchk => "Gaussian fchk",
            Self::GaussianLog => "Gaussian log",
            Self::OrcaOutput => "ORCA output",
            Self::GaussianInput => "Gaussian input",
            Self::OrcaInput => "ORCA input",
            Self::GamessInput => "GAMESS input",
        }
    }

    /// Whether the format can carry calculated results, not only geometries.
    pub fn has_results(self) -> bool {
        matches!(
            self,
            Self::Molden | Self::Fchk | Self::GaussianLog | Self::OrcaOutput
        )
    }
}

/// The format of a file from its name and, where the extension is shared between programs
/// (`.log`, `.out`, `.inp`) or missing, from its text.
pub fn detect(file_name: &str, text: &str) -> Option<Format> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".molden.input") {
        return Some(Format::Molden);
    }
    let extension = lower.rsplit_once('.').map_or("", |(_, e)| e);
    match extension {
        "xyz" | "extxyz" => Some(Format::Xyz),
        "sdf" | "sd" | "mol" => Some(Format::Sdf),
        "molden" | "mold" => Some(Format::Molden),
        "fchk" | "fch" => Some(Format::Fchk),
        "gjf" | "com" | "gau" => Some(Format::GaussianInput),
        _ => sniff(text),
    }
}

fn sniff(text: &str) -> Option<Format> {
    let head = &text[..text.floor_char_boundary(1024 * 1024)];
    let lines = || head.lines().map(str::trim);
    if lines()
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.eq_ignore_ascii_case("[Molden Format]"))
    {
        return Some(Format::Molden);
    }
    if lines().any(|line| {
        line == "Entering Gaussian System"
            || line.starts_with("Entering Gaussian System,")
            || line
                .strip_prefix("Gaussian, Inc.")
                .is_some_and(|rest| matches!(rest.trim(), "" | "All Rights Reserved."))
            || (line.starts_with("Copyright (c) ") && line.ends_with(", Gaussian, Inc."))
    }) {
        return Some(Format::GaussianLog);
    }
    if lines().any(|line| matches!(line, "O   R   C   A" | "* O   R   C   A *")) {
        return Some(Format::OrcaOutput);
    }
    if lines().any(|line| {
        let heading = line.split_once('=').map_or(line, |(name, _)| name.trim());
        heading.eq_ignore_ascii_case("GAMESS VERSION")
    }) {
        // GAMESS output echoes its input; it is not read yet.
        return None;
    }
    if head.lines().any(|line| {
        line.strip_prefix(" $")
            .and_then(|card| {
                card.split(|c: char| c.is_ascii_whitespace() || c == '!')
                    .next()
            })
            .is_some_and(|name| name.eq_ignore_ascii_case("DATA"))
    }) {
        return Some(Format::GamessInput);
    }
    if lines().any(|line| {
        let star = line.strip_prefix('*');
        let token = star
            .unwrap_or(line)
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default();
        if star.is_some() {
            matches!(
                token.to_ascii_lowercase().as_str(),
                "xyz" | "xyzfile" | "int" | "gzmt" | "internal"
            )
        } else {
            token.eq_ignore_ascii_case("%coords")
        }
    }) {
        return Some(Format::OrcaInput);
    }

    let mut input = lines().skip_while(|line| !line.starts_with('#'));
    input.find(|line| line.is_empty())?; // After the route.
    input.find(|line| !line.is_empty())?; // Start of the title.
    input.find(|line| line.is_empty())?; // After the title.
    let mut spin = input
        .find(|line| !line.is_empty())?
        .split_ascii_whitespace();
    let (Some(charge), Some(multiplicity), None) = (spin.next(), spin.next(), spin.next()) else {
        return None;
    };
    if charge.parse::<i32>().is_err() || multiplicity.parse::<i32>().is_err() {
        return None;
    }
    input
        .take_while(|line| !line.is_empty())
        .any(|line| {
            let label = line.split([',', ' ', '\t']).next().unwrap_or_default();
            let symbol = label
                .split(['(', '-'])
                .next()
                .unwrap_or_default()
                .trim_end_matches(|c: char| c.is_ascii_digit());
            element(symbol).is_some() || label.parse::<u8>().is_ok_and(|z| (1..=118).contains(&z))
        })
        .then_some(Format::GaussianInput)
}

/// What a file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct QcFile {
    pub format: Format,
    /// The producing program, such as "Gaussian", "ORCA" or "xtb", when the file names it.
    pub program: Option<String>,
    /// The program's version as the file prints it, such as "16 Revision A.03".
    pub program_version: Option<String>,
    /// Jobs in file order. Geometry files have one job.
    pub jobs: Vec<Job>,
    /// What was skipped or assumed, in words for the user.
    pub warnings: Vec<String>,
}

/// What a job did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// Geometries without a calculation: XYZ, SDF and program inputs.
    Geometry,
    SinglePoint,
    Optimization,
    Frequencies,
    Scan,
    /// A calculation of another kind (NMR, excited states, ...); its geometry and energy are
    /// still read.
    Other,
}

impl JobKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Geometry => "Geometry",
            Self::SinglePoint => "Single point",
            Self::Optimization => "Optimization",
            Self::Frequencies => "Frequencies",
            Self::Scan => "Scan",
            Self::Other => "Calculation",
        }
    }
}

/// How a program run ended, as its output says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// "Normal termination" (Gaussian) or "ORCA TERMINATED NORMALLY".
    Normal,
    /// The program reported an error; `message` is its own words where it gives them.
    Error { message: String },
    /// The output ends before the job finished: still running, killed or cut off.
    Truncated,
}

impl Completion {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "Normal termination",
            Self::Error { .. } => "Error",
            Self::Truncated => "Truncated",
        }
    }
}

/// One geometry of a job: an optimization or scan step, a trajectory frame or an SDF record.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub frame: Frame,
    /// The total energy the file reports at exactly this geometry.
    pub energy_hartree: Option<f64>,
    /// The record's title or comment line, when the format has one.
    pub label: Option<String>,
}

/// One job of a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// Zero-based position in the file; also `Source::Imported::job`.
    pub index: u32,
    pub kind: JobKind,
    /// Method and basis as the program names them, such as "B3LYP" and "6-31G(d)".
    pub method: Option<String>,
    pub basis: Option<String>,
    /// The job's keywords as written: the Gaussian route or the ORCA `!` lines.
    pub keywords: Vec<String>,
    /// How the job ended; `None` for files that do not record it (geometry files, Molden,
    /// fchk).
    pub completion: Option<Completion>,
    /// Geometries in file order; the last is the job's final geometry.
    pub steps: Vec<Step>,
    /// The job's result, bound by `result.frame` to one of `steps`.
    pub result: Option<CalcResult>,
}

impl Job {
    /// The step the result describes.
    pub fn result_step(&self) -> Option<&Step> {
        let result = self.result.as_ref()?;
        self.steps.iter().find(|s| s.frame.hash() == result.frame)
    }

    /// A short title such as "Optimization · B3LYP/6-31G(d)".
    pub fn title(&self) -> String {
        let level = match (&self.method, &self.basis) {
            (Some(m), Some(b)) => format!("{m}/{b}"),
            (Some(m), None) => m.clone(),
            (None, Some(b)) => b.clone(),
            (None, None) => return self.kind.name().into(),
        };
        format!("{} · {level}", self.kind.name())
    }
}

/// One row of the "Choose what to import" list.
#[derive(Debug, Clone, PartialEq)]
pub struct JobSummary {
    pub index: u32,
    pub kind: JobKind,
    pub title: String,
    pub completion: Option<Completion>,
    pub steps: usize,
    /// Atoms in the last geometry.
    pub atoms: usize,
    pub last_frame: Option<FrameHash>,
    /// The result's energy, else the last step's.
    pub energy_hartree: Option<f64>,
    pub frequencies: usize,
    /// Orbital energies over both spins.
    pub orbitals: usize,
}

impl QcFile {
    /// The job opened by default: the last job that ended normally and has a geometry, else
    /// the last job with a geometry (partial results, with their status).
    pub fn default_job(&self) -> Option<usize> {
        let with_steps = || {
            self.jobs
                .iter()
                .enumerate()
                .filter(|(_, j)| !j.steps.is_empty())
        };
        with_steps()
            .rfind(|(_, j)| matches!(j.completion, None | Some(Completion::Normal)))
            .or_else(|| with_steps().next_back())
            .map(|(i, _)| i)
    }

    /// One summary per job, in file order.
    pub fn summaries(&self) -> Vec<JobSummary> {
        self.jobs
            .iter()
            .map(|job| {
                let last = job.steps.last();
                let result = job.result.as_ref();
                let orbitals = result.and_then(|r| r.orbitals.as_ref()).map_or(0, |o| {
                    o.alpha.energies_hartree.len()
                        + match &o.beta {
                            reshiki_qcdata::Beta::Unrestricted(set) => set.energies_hartree.len(),
                            _ => 0,
                        }
                });
                JobSummary {
                    index: job.index,
                    kind: job.kind,
                    title: job.title(),
                    completion: job.completion.clone(),
                    steps: job.steps.len(),
                    atoms: last.map_or(0, |s| s.frame.numbers.len()),
                    last_frame: last.map(|s| s.frame.hash()),
                    energy_hartree: result
                        .and_then(|r| r.energy_hartree)
                        .or_else(|| last.and_then(|s| s.energy_hartree)),
                    frequencies: result
                        .and_then(|r| r.vibrations.as_ref())
                        .map_or(0, |v| v.frequencies_cm1.len()),
                    orbitals,
                }
            })
            .collect()
    }
}

/// Read a file of any [`Format`], told by [`detect`].
pub fn read(file_name: &str, bytes: &[u8]) -> Result<QcFile, Error> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::Unsupported(format!(
            "The file is larger than {} MB",
            MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    // Program outputs may hold a stray Latin-1 byte (a localized date); it never matters.
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let format = detect(file_name, text)
        .ok_or_else(|| Error::Unsupported(format!("{file_name} is not a format ReShiki reads")))?;
    let origin = Origin {
        format,
        file_name: file_name.rsplit(['/', '\\']).next().unwrap_or(file_name),
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    };
    let file = match format {
        Format::Xyz => xyz::read(text)?,
        Format::Sdf => sdf::read(text)?,
        Format::Molden => molden::read(text, &origin)?,
        Format::Fchk => fchk::read(text, &origin)?,
        Format::GaussianLog => gaussian::read(text, &origin)?,
        Format::OrcaOutput => orca::read(text, &origin)?,
        Format::GaussianInput => input::gaussian(text)?,
        Format::OrcaInput => input::orca(text)?,
        Format::GamessInput => input::gamess(text)?,
    };
    finish(file)
}

/// Where a result came from, for its provenance.
pub(crate) struct Origin<'a> {
    pub format: Format,
    pub file_name: &'a str,
    pub sha256: String,
}

impl Origin<'_> {
    /// An imported result at `frame` with everything optional left empty. `job.index`,
    /// `job.method` and `job.basis` give its provenance; readers fill the rest.
    pub fn result(
        &self,
        frame: &Frame,
        task: Task,
        status: Status,
        program: &QcProgram,
        job: &Job,
    ) -> CalcResult {
        CalcResult {
            frame: frame.hash(),
            input_frame: None,
            task,
            endpoints: None,
            settings: Settings::Imported {
                keywords: job.keywords.clone(),
            },
            provenance: Provenance {
                program: program.name.clone(),
                program_version: program.version.clone(),
                method: job.method.clone(),
                basis: job.basis.clone(),
                source: Source::Imported {
                    format: self.format.name().into(),
                    file_name: self.file_name.into(),
                    file_sha256: Some(self.sha256.clone()),
                    job: job.index,
                },
            },
            status,
            energy_hartree: None,
            energy_terms: BTreeMap::new(),
            gradient_hartree_per_bohr: Vec::new(),
            hessian_hartree_per_bohr2: Vec::new(),
            charges: Vec::new(),
            dipole_e_bohr: None,
            bond_orders: Vec::new(),
            orbitals: None,
            vibrations: None,
            thermo: None,
            trajectory: None,
            keep_full: false,
            omitted_bytes: None,
            run: None,
        }
    }
}

/// The producing program of a result file.
pub(crate) struct QcProgram {
    pub name: String,
    pub version: Option<String>,
}

/// The atomic number and mass number (0 = natural, 2 for D, 3 for T) of an element symbol in
/// any case, such as `C`, `cl` or `D`.
pub(crate) fn element(symbol: &str) -> Option<(u8, u16)> {
    match symbol {
        "D" | "d" => return Some((1, 2)),
        "T" | "t" => return Some((1, 3)),
        _ => {}
    }
    let z = crate::chemistry::ELEMENTS
        .iter()
        .skip(1)
        .position(|e| e.symbol.eq_ignore_ascii_case(symbol))?;
    Some((u8::try_from(z + 1).ok()?, 0))
}

/// A frame from atoms and their isotopes (empty when all natural), checked.
pub(crate) fn frame(
    atoms: &[(u8, u16)],
    positions_angstrom: Vec<[f64; 3]>,
    charge: i32,
    multiplicity: u32,
) -> Result<Frame, Error> {
    let frame = Frame {
        numbers: atoms.iter().map(|a| a.0).collect(),
        positions_angstrom,
        isotopes: if atoms.iter().any(|a| a.1 != 0) {
            atoms.iter().map(|a| a.1).collect()
        } else {
            Vec::new()
        },
        masses_amu: Vec::new(),
        charge,
        multiplicity,
        drawing_atoms: Vec::new(),
    };
    frame.validate()?;
    Ok(frame)
}

/// The lowest multiplicity the electron count allows: 1 for an even count, else 2. For
/// geometry files, which do not record the spin state.
pub(crate) fn lowest_multiplicity(atoms: &[(u8, u16)], charge: i32) -> u32 {
    let electrons = atoms.iter().map(|a| i64::from(a.0)).sum::<i64>() - i64::from(charge);
    if electrons.rem_euclid(2) == 0 { 1 } else { 2 }
}

/// Apply the frame limit, bind optimization paths, and validate every frame and result.
fn finish(mut file: QcFile) -> Result<QcFile, Error> {
    let total: usize = file.jobs.iter().map(|j| j.steps.len()).sum();
    if total == 0 {
        return Err(Error::Invalid("The file has no geometry".into()));
    }
    if total > MAX_FRAMES {
        // Keep each job's first and last geometry and the one its result describes.
        for job in &mut file.jobs {
            let bound = job.result.as_ref().map(|r| r.frame);
            let last = job.steps.len().saturating_sub(1);
            let mut i = 0;
            job.steps.retain(|step| {
                let keep = i == 0 || i == last || Some(step.frame.hash()) == bound;
                i += 1;
                keep
            });
        }
        let kept: usize = file.jobs.iter().map(|j| j.steps.len()).sum();
        if kept > MAX_FRAMES {
            return Err(Error::Unsupported(format!(
                "The file has {total} geometries; ReShiki reads at most {MAX_FRAMES}"
            )));
        }
        file.warnings.push(format!(
            "Kept {kept} of {total} geometries: the first and last of each job"
        ));
    }
    for job in &mut file.jobs {
        for step in &job.steps {
            step.frame.validate()?;
        }
        let Some(result) = &mut job.result else {
            continue;
        };
        // The steps up to and including the one the result describes.
        let Some((frame, path)) = job
            .steps
            .iter()
            .position(|s| s.frame.hash() == result.frame)
            .and_then(|at| job.steps.get(..=at))
            .and_then(|path| Some((&path.last()?.frame, path)))
        else {
            return Err(Error::Invalid(format!(
                "Job {} has a result without its geometry",
                job.index + 1
            )));
        };
        // Data above the stored-result limits is left out rather than refusing the file.
        if frame.numbers.len() > MAX_HESSIAN_ATOMS
            && (result.task == Task::Hessian
                || result.vibrations.is_some()
                || !result.hessian_hartree_per_bohr2.is_empty())
        {
            result.vibrations = None;
            result.hessian_hartree_per_bohr2.clear();
            if result.task == Task::Hessian {
                result.task = Task::Energy;
            }
            file.warnings.push(format!(
                "Job {}: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms",
                job.index + 1
            ));
        }
        if let Some(orbitals) = &mut result.orbitals
            && let Err(reshiki_qcdata::Error::TooLarge { what, .. }) = orbitals.validate(frame)
        {
            orbitals.basis = None;
            for set in [
                Some(&mut orbitals.alpha),
                match &mut orbitals.beta {
                    reshiki_qcdata::Beta::Unrestricted(set) => Some(set),
                    _ => None,
                },
            ]
            .into_iter()
            .flatten()
            {
                set.rows.clear();
                set.coefficients.clear();
            }
            file.warnings.push(format!(
                "Job {}: orbital shapes left out (too many {what}); orbital energies are kept",
                job.index + 1
            ));
        }
        if result.task == Task::Optimize
            && result.trajectory.is_none()
            && let [first, _, ..] = path
        {
            let first = &first.frame;
            if path.iter().all(|s| {
                s.frame.numbers == first.numbers
                    && s.frame.isotopes == first.isotopes
                    && s.frame.charge == first.charge
                    && s.frame.multiplicity == first.multiplicity
            }) {
                result.input_frame = Some(first.hash());
                result.trajectory = Some(Trajectory {
                    numbers: first.numbers.clone(),
                    isotopes: first.isotopes.clone(),
                    masses_amu: Vec::new(),
                    charge: first.charge,
                    multiplicity: first.multiplicity,
                    positions_angstrom: path
                        .iter()
                        .flat_map(|s| s.frame.positions_angstrom.iter().flatten().copied())
                        .collect(),
                    energies_hartree: path
                        .iter()
                        .map(|s| s.energy_hartree)
                        .collect::<Option<Vec<_>>>()
                        .unwrap_or_default(),
                    labels: Vec::new(),
                    minimizer_energies_hartree: Vec::new(),
                    cluster_sizes: Vec::new(),
                });
            }
        }
        result.validate(frame)?;
    }
    Ok(file)
}
