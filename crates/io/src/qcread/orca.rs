//! ORCA output read job by job into geometries, energies, orbital levels and vibrations.
use super::{
    Completion, Error, Format, Job, JobKind, Origin, QcFile, QcProgram, Step, element, frame,
};
use reshiki_qcdata::{
    AtomicCharges, Beta, ChargeScheme, Frame, MAX_HESSIAN_ATOMS, OrbitalSet, Orbitals, Status,
    Task, Vibrations,
};
use std::collections::BTreeMap;

const COORDINATES: &str = "CARTESIAN COORDINATES (ANGSTROEM)";
const STATIONARY: &str = "FINAL ENERGY EVALUATION AT THE STATIONARY POINT";

#[derive(Default)]
struct Values {
    terms: BTreeMap<String, f64>,
    charges: Option<Vec<f64>>,
    dipole: Option<[f64; 3]>,
    orbitals: Option<Orbitals>,
    frequencies: Option<Vec<(usize, f64)>>,
    modes: Option<BTreeMap<usize, Vec<f64>>>,
    intensities: Option<BTreeMap<usize, f64>>,
    scf_failure: Option<String>,
}

/// Read ORCA 5 and 6 output without assigning data across geometry or job boundaries.
pub(super) fn read(text: &str, origin: &Origin<'_>) -> Result<QcFile, Error> {
    let lines: Vec<_> = text.lines().collect();
    let version = program_version(&lines);
    let mut boundaries = vec![0];
    let mut seen_banner = false;
    let mut has_output = false;
    let mut has_geometry = false;
    let mut terminated = false;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let record = trimmed.trim_matches('*').trim();
        let banner = job_banner(line);
        let prologue = record == "O   R   C   A";
        let split = if let Some(number) = banner {
            // A compound echo can precede its first numbered job.
            seen_banner || number != 1 || has_geometry || terminated
        } else if prologue {
            has_output
        } else {
            terminated
                && !trimmed.is_empty()
                && !trimmed.starts_with("TOTAL RUN TIME:")
                && !trimmed.bytes().all(|byte| b"*-$= ".contains(&byte))
        };
        if split && i > 0 {
            boundaries.push(i);
            if banner.is_none() {
                seen_banner = false;
            }
            has_output = false;
            has_geometry = false;
            terminated = false;
        }
        seen_banner |= banner.is_some();
        has_geometry |= trimmed == COORDINATES;
        terminated |= termination(line).is_some_and(|(_, ends_job)| ends_job);
        has_output |= prologue || has_geometry || trimmed == "INPUT FILE" || terminated;
    }
    boundaries.push(lines.len());
    if boundaries.len() > super::MAX_JOBS + 1 {
        return Err(Error::Unsupported(format!(
            "The file holds more than {} ORCA jobs",
            super::MAX_JOBS
        )));
    }
    let mut jobs = Vec::new();
    let mut inputs = Vec::new();
    let mut next_input = 0;
    let mut invocation_version = None;
    for (index, &[start, end]) in boundaries.array_windows::<2>().enumerate() {
        let job_lines = lines
            .get(start..end)
            .ok_or_else(|| Error::at(start + 1, "The ORCA job boundary is outside the file"))?;
        if start > 0
            && (job_lines
                .first()
                .is_some_and(|line| job_banner(line).is_none())
                || job_lines
                    .iter()
                    .any(|line| line.trim().trim_matches('*').trim() == "O   R   C   A"))
        {
            inputs.clear();
            next_input = 0;
            invocation_version = None;
        }
        if let Some(version) = program_version(job_lines) {
            invocation_version = Some(version);
        }
        let program = QcProgram {
            name: "ORCA".into(),
            version: invocation_version.clone(),
        };
        let echo = input_lines(job_lines);
        if !echo.is_empty() {
            inputs = echo
                .split(|line| line.eq_ignore_ascii_case("$new_job"))
                .map(<[&str]>::to_vec)
                .collect();
            next_input = 0;
        }
        let input = inputs.get(next_input).map_or(&[][..], Vec::as_slice);
        next_input += 1;
        jobs.push(read_job(
            job_lines,
            input,
            start,
            index as u32,
            lines
                .get(end)
                .is_some_and(|line| job_banner(line).is_some()),
            origin,
            &program,
        )?);
    }
    Ok(QcFile {
        format: Format::OrcaOutput,
        program: Some("ORCA".into()),
        program_version: version,
        jobs,
        warnings: Vec::new(),
    })
}

fn program_version(lines: &[&str]) -> Option<String> {
    lines.iter().find_map(|line| {
        line.trim()
            .strip_prefix("Program Version")?
            .split_whitespace()
            .next()
            .map(str::to_owned)
    })
}

fn job_banner(line: &str) -> Option<u32> {
    let line = line.trim();
    if !line.starts_with('$') || !line.ends_with('$') {
        return None;
    }
    let words: Vec<_> = line.trim_matches('$').split_whitespace().collect();
    match words.as_slice() {
        ["JOB", "NUMBER", number] => number.parse().ok(),
        _ => None,
    }
}

fn read_job(
    lines: &[&str],
    input: &[&str],
    offset: usize,
    index: u32,
    followed_by_numbered_job: bool,
    origin: &Origin<'_>,
    program: &QcProgram,
) -> Result<Job, Error> {
    let keywords: Vec<String> = input
        .iter()
        .filter_map(|line| line.trim_start().strip_prefix('!'))
        .flat_map(str::split_whitespace)
        .map(str::to_owned)
        .collect();
    let kind = job_kind(&keywords, input, lines);
    let completion = completion(lines, followed_by_numbered_job);
    let mut job = Job {
        index,
        kind,
        method: keywords
            .iter()
            .filter(|word| is_method(word))
            .min_by_key(|word| is_hf_reference(word))
            .cloned(),
        basis: keywords.iter().find(|word| is_basis(word)).cloned(),
        keywords,
        completion: Some(completion.clone()),
        steps: Vec::new(),
        result: None,
    };
    let (charge, multiplicity) = electronic_state(lines, input, offset)?;
    let starts: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| (line.trim() == COORDINATES).then_some(i))
        .collect();
    let mut values: Vec<Values> = Vec::new();
    for (position, &start) in starts.iter().enumerate() {
        let end = starts.get(position + 1).copied().unwrap_or(lines.len());
        let interval = lines.get(start..end).ok_or_else(|| {
            Error::at(
                offset + start + 1,
                "The ORCA geometry boundary is outside this job",
            )
        })?;
        let interrupted = end == lines.len() && completion != Completion::Normal;
        let Some(geometry) =
            coordinates(interval, offset + start, charge, multiplicity, interrupted)?
        else {
            break;
        };
        let stationary = position
            .checked_sub(1)
            .and_then(|previous| starts.get(previous))
            .and_then(|&previous| lines.get(previous..start))
            .is_some_and(|interval| interval.iter().any(|line| line.contains(STATIONARY)));
        // ORCA may reprint exactly the final geometry for its stationary-point energy.
        let fold = stationary && job.steps.last().is_some_and(|step| step.frame == geometry);
        if !fold {
            job.steps.push(Step {
                frame: geometry,
                energy_hartree: None,
                label: None,
            });
            values.push(Values::default());
        }
        // A newly parsed geometry either adds to both vectors or folds into their last
        // entry. That invariant guarantees both vectors have the same nonzero length.
        #[expect(
            clippy::indexing_slicing,
            reason = "each complete geometry adds to both vectors; folding retains their last entries"
        )]
        {
            let at = job.steps.len() - 1;
            read_values(
                interval,
                offset + start,
                &mut job.steps[at],
                &mut values[at],
                interrupted,
            )?;
        }
    }
    let bound = job
        .steps
        .iter()
        .rposition(|step| step.energy_hartree.is_some())
        .or_else(|| job.steps.len().checked_sub(1));
    if let Some(bound) = bound {
        #[expect(
            clippy::indexing_slicing,
            reason = "bound is obtained from a position or the last index in job.steps"
        )]
        let step = &job.steps[bound];
        let task = match kind {
            JobKind::Optimization => Task::Optimize,
            JobKind::Frequencies => Task::Hessian,
            _ => Task::Energy,
        };
        let mut result = origin.result(
            &step.frame,
            task,
            status(lines, &completion, kind),
            program,
            &job,
        );
        result.energy_hartree = step.energy_hartree;
        #[expect(
            clippy::indexing_slicing,
            reason = "values and job.steps receive one entry per complete geometry; bound indexes job.steps"
        )]
        let data = &mut values[bound];
        if completion == Completion::Normal
            && let Some(reason) = &data.scf_failure
        {
            result.status = Status::NotConverged {
                reason: reason.clone(),
            };
        }
        result.energy_terms = std::mem::take(&mut data.terms);
        if let Some(charges) = data.charges.take() {
            result.charges.push(AtomicCharges {
                scheme: ChargeScheme::Mulliken,
                values: charges,
            });
        }
        result.dipole_e_bohr = data.dipole;
        result.orbitals = data.orbitals.take();
        result.vibrations = vibrations(data, step.frame.numbers.len())?;
        job.result = Some(result);
    }
    Ok(job)
}

fn input_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut in_input = false;
    let mut input = Vec::new();
    for line in lines {
        if line.trim() == "INPUT FILE" {
            in_input = true;
        } else if in_input && line.contains("END OF INPUT") {
            in_input = false;
        } else if in_input
            && line.trim_start().starts_with('|')
            && let Some((_, text)) = line.split_once('>')
        {
            input.push(text.trim());
        }
    }
    input
}

fn is_basis(word: &str) -> bool {
    let word = word.to_ascii_lowercase();
    if ["/j", "/c", "/jk"]
        .iter()
        .any(|suffix| word.ends_with(suffix))
    {
        return false;
    }
    [
        "def2-", "ma-def2-", "cc-p", "aug-cc-", "6-31", "pcseg-", "pc-",
    ]
    .iter()
    .any(|prefix| word.starts_with(prefix))
        || ["sto-3g", "3-21g", "minix"].contains(&word.as_str())
}

fn is_method(word: &str) -> bool {
    let word = word.to_ascii_uppercase();
    let known = |word: &str| {
        matches!(
            word,
            "HF" | "RHF"
                | "UHF"
                | "ROHF"
                | "MP2"
                | "MP3"
                | "MP4"
                | "MP4(SDQ)"
                | "MP4(SDTQ)"
                | "RI-MP2"
                | "RI-SCS-MP2"
                | "RI-SOS-MP2"
                | "SCS-MP2"
                | "SOS-MP2"
                | "OOMP2"
                | "CCSD"
                | "CCSD(T)"
                | "CCSD(T0)"
                | "CIS"
                | "CIS(D)"
                | "CISD"
                | "QCISD"
                | "QCISD(T)"
                | "B3LYP"
                | "B3LYP/G"
                | "PBE0"
                | "PBE"
                | "BP86"
                | "BLYP"
                | "TPSS"
                | "TPSSH"
                | "M06"
                | "M06-L"
                | "M06-2X"
                | "M06-HF"
                | "WB97X"
                | "WB97X-D3"
                | "WB97X-D4"
                | "WB97X-V"
                | "WB97M-V"
                | "WB97M-D3BJ"
                | "R2SCAN"
                | "R2SCAN-3C"
                | "SCAN"
                | "B97"
                | "B97-3C"
                | "PBEH-3C"
                | "HF-3C"
                | "XTB"
                | "XTB1"
                | "XTB2"
                | "GFN-XTB"
                | "GFN1-XTB"
                | "GFN2-XTB"
                | "B2PLYP"
                | "DSD-BLYP"
                | "DSD-PBEP86"
        )
    };
    known(&word)
        || word.strip_prefix("DLPNO-").is_some_and(known)
        || word.strip_prefix(['R', 'U']).is_some_and(known)
}

fn is_hf_reference(word: &str) -> bool {
    ["HF", "RHF", "UHF", "ROHF"]
        .iter()
        .any(|reference| word.eq_ignore_ascii_case(reference))
}

fn job_kind(keywords: &[String], input: &[&str], lines: &[&str]) -> JobKind {
    let words: Vec<_> = keywords
        .iter()
        .map(|word| word.to_ascii_lowercase())
        .collect();
    let mut geom = false;
    let mut scan = false;
    let mut other = false;
    for line in input {
        let line = line.to_ascii_lowercase();
        if line.starts_with("%geom") {
            geom = true;
        }
        if geom && line.split_whitespace().any(|word| word == "scan") {
            scan = true;
        }
        if line.trim() == "end" {
            geom = false;
        }
        other |= ["%eprnmr", "%tddft", "%cis", "%eom"]
            .iter()
            .any(|prefix| line.starts_with(prefix));
    }
    let has = |choices: &[&str]| words.iter().any(|word| choices.contains(&word.as_str()));
    if scan
        || lines
            .iter()
            .any(|line| line.contains("RELAXED SURFACE SCAN"))
    {
        JobKind::Scan
    } else if has(&[
        "opt",
        "copt",
        "zopt",
        "tightopt",
        "looseopt",
        "optts",
        "verytightopt",
    ]) {
        JobKind::Optimization
    } else if has(&["freq", "numfreq", "anfreq"]) {
        JobKind::Frequencies
    } else if other
        || has(&["nmr", "td-dft", "tddft", "eom", "cis", "cis(d)"])
        || words.iter().any(|word| word.starts_with("eom-"))
    {
        JobKind::Other
    } else {
        JobKind::SinglePoint
    }
}

/// Recognize termination evidence outside the input echo. The boolean distinguishes
/// official end records from diagnostics that affect completion but do not split jobs.
fn termination(line: &str) -> Option<(Completion, bool)> {
    let line = line.trim();
    if line.starts_with('|') {
        return None;
    }
    if line.trim_matches('*').trim() == "ORCA TERMINATED NORMALLY" {
        return Some((Completion::Normal, true));
    }
    let lower = line.to_ascii_lowercase();
    let record = line.trim_matches('*').trim();
    let ends_job = record == "ORCA TERMINATED ABNORMALLY"
        || record.starts_with("ORCA finished by error termination");
    (lower.contains("orca finished by error termination")
        || lower.contains("orca terminated abnormally")
        || lower.contains("aborting the run")
        || lower.contains("error :"))
    .then(|| {
        (
            Completion::Error {
                message: line.into(),
            },
            ends_job,
        )
    })
}

fn completion(lines: &[&str], followed_by_numbered_job: bool) -> Completion {
    let mut normal = followed_by_numbered_job;
    for line in lines.iter().rev() {
        match termination(line) {
            Some((error @ Completion::Error { .. }, _)) => return error,
            Some((Completion::Normal, _)) => normal = true,
            _ => {}
        }
    }
    if normal {
        Completion::Normal
    } else {
        Completion::Truncated
    }
}

fn status(lines: &[&str], completion: &Completion, kind: JobKind) -> Status {
    match completion {
        Completion::Error { message } => Status::Failed {
            reason: message.clone(),
        },
        Completion::Truncated => Status::NotConverged {
            reason: "the file ends before the job finished".into(),
        },
        Completion::Normal
            if kind == JobKind::Optimization
                && !lines
                    .iter()
                    .any(|line| line.contains("THE OPTIMIZATION HAS CONVERGED")) =>
        {
            let reason = lines
                .iter()
                .rev()
                .find(|line| {
                    let lower = line.to_ascii_lowercase();
                    (lower.contains("maximum") || lower.contains("max."))
                        && lower.contains("cycles")
                        || lower.contains("optimization did not converge")
                        || lower.contains("optimization has not converged")
                })
                .map_or_else(
                    || "the optimization did not converge".into(),
                    |line| line.trim().into(),
                );
            Status::NotConverged { reason }
        }
        Completion::Normal => Status::Converged,
    }
}

fn electronic_state(
    lines: &[&str],
    input: &[&str],
    offset: usize,
) -> Result<(Option<i32>, Option<u32>), Error> {
    let mut charge = None;
    let mut multiplicity = None;
    for line in input {
        let compact = line.trim_start_matches('*').trim();
        let words: Vec<_> = compact.split_whitespace().collect();
        if words.first().is_some_and(|word| {
            ["xyz", "xyzfile", "int", "gzmt"]
                .iter()
                .any(|command| word.eq_ignore_ascii_case(command))
        }) && let (Some(c), Some(m)) = (words.get(1), words.get(2))
        {
            // Printed electronic-state fields below take precedence over the echo.
            charge =
                Some(c.parse().map_err(|_| {
                    Error::Invalid("The ORCA input charge is not an integer".into())
                })?);
            multiplicity = Some(m.parse().map_err(|_| {
                Error::Invalid("The ORCA input multiplicity is not a positive integer".into())
            })?);
        }
    }
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Total Charge")
            && trimmed.contains("Charge")
            && let Some((_, value)) = line.split_once("....")
        {
            charge = Some(value.trim().parse().map_err(|_| {
                Error::at(offset + i + 1, "The ORCA total charge is not an integer")
            })?);
        } else if trimmed.starts_with("Multiplicity")
            && trimmed.contains("Mult")
            && let Some((_, value)) = line.split_once("....")
        {
            multiplicity = Some(value.trim().parse().map_err(|_| {
                Error::at(
                    offset + i + 1,
                    "The ORCA multiplicity is not a positive integer",
                )
            })?);
        }
    }
    Ok((charge, multiplicity))
}

fn coordinates(
    lines: &[&str],
    offset: usize,
    charge: Option<i32>,
    multiplicity: Option<u32>,
    allow_partial: bool,
) -> Result<Option<Frame>, Error> {
    let divider = |line: &&str| {
        let line = line.trim();
        line.is_empty() || line.bytes().all(|byte| byte == b'-')
    };
    let first = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(i, line)| (!divider(line)).then_some(i));
    let Some(first) = first else {
        if allow_partial {
            return Ok(None);
        }
        return Err(Error::at(
            offset + 1,
            "The ORCA Cartesian coordinate block has no atoms",
        ));
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(first)
        .find_map(|(i, line)| divider(line).then_some(i));
    // An interrupted final coordinate panel does not describe a complete molecule.
    // Keep the preceding complete geometry and any energy already bound to it.
    if end.is_none() && allow_partial {
        return Ok(None);
    }
    let end = end.unwrap_or(lines.len());
    let mut atoms = Vec::new();
    let mut positions = Vec::new();
    for (i, line) in lines.iter().enumerate().take(end).skip(first) {
        let words: Vec<_> = line.split_whitespace().collect();
        let symbol = words
            .first()
            .ok_or_else(|| Error::at(offset + i + 1, "The ORCA coordinate row has no element"))?;
        let atom = element(symbol).ok_or_else(|| Error::Unsupported(format!(
            "Line {}: ORCA atom label '{symbol}' is not a plain element symbol; ghost atoms and atom suffixes are not supported",
            offset + i + 1
        )))?;
        let [_, x, y, z] = words.as_slice() else {
            return Err(Error::at(
                offset + i + 1,
                "An ORCA coordinate row needs an element and three coordinates in angstrom",
            ));
        };
        atoms.push(atom);
        positions.push([
            number(x, offset + i + 1, "x coordinate")?,
            number(y, offset + i + 1, "y coordinate")?,
            number(z, offset + i + 1, "z coordinate")?,
        ]);
    }
    let charge = charge.ok_or_else(|| {
        Error::at(
            offset + 1,
            "This ORCA job does not report its total charge in the output or input echo",
        )
    })?;
    let multiplicity = multiplicity.ok_or_else(|| {
        Error::at(
            offset + 1,
            "This ORCA job does not report its multiplicity in the output or input echo",
        )
    })?;
    frame(&atoms, positions, charge, multiplicity)
        .map(Some)
        .map_err(|error| Error::at(offset + 1, error))
}

fn number(text: &str, line: usize, what: &str) -> Result<f64, Error> {
    let value = text
        .replace(['D', 'd'], "E")
        .parse::<f64>()
        .map_err(|_| Error::at(line, format!("The ORCA {what} is not a number: '{text}'")))?;
    if !value.is_finite() {
        return Err(Error::at(line, format!("The ORCA {what} must be finite")));
    }
    Ok(value)
}

fn read_values(
    lines: &[&str],
    offset: usize,
    step: &mut Step,
    values: &mut Values,
    truncated: bool,
) -> Result<(), Error> {
    let mut cursor = 0;
    let mut scf_failure = values.scf_failure.clone();
    while let Some(line) = lines.get(cursor) {
        let i = cursor;
        cursor += 1;
        let trimmed = line.trim();
        if trimmed.contains("SCF NOT CONVERGED AFTER") {
            scf_failure = Some(trimmed.into());
        } else if trimmed.contains("SCF CONVERGED AFTER") {
            scf_failure = None;
        }
        if let Some(value) = trimmed.strip_prefix("FINAL SINGLE POINT ENERGY") {
            let Some(value) = value.split_whitespace().next() else {
                if truncated && i + 1 == lines.len() {
                    continue;
                }
                return Err(Error::at(
                    offset + i + 1,
                    "The ORCA final energy is missing",
                ));
            };
            if truncated
                && i + 1 == lines.len()
                && value.replace(['D', 'd'], "E").parse::<f64>().is_err()
            {
                continue;
            }
            step.energy_hartree = Some(number(value, offset + i + 1, "final energy")?);
            values.scf_failure = scf_failure.clone();
        }
        if let Some((label, right)) = trimmed.split_once(':') {
            let label = label
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            let words: Vec<_> = right.split_whitespace().collect();
            if let [value, "Eh", ..] = words.as_slice()
                && (label.contains("energy")
                    || label.contains("repulsion")
                    || ["e(x)", "e(c)", "e(xc)"].contains(&label.as_str()))
                && ![
                    "energy",
                    "zero point energy",
                    "thermal vibrational correction",
                ]
                .contains(&label.as_str())
            {
                values
                    .terms
                    .insert(label, number(value, offset + i + 1, "energy component")?);
            }
        }
        if trimmed == "ORBITAL ENERGIES" {
            if let Some(orbitals) = orbital_energies(
                lines,
                &mut cursor,
                offset,
                truncated,
                step.frame.multiplicity,
            )? {
                // A table that runs into interrupted EOF has no closing record. It
                // may expose an initial partial spectrum, but cannot replace the last
                // complete table, including one retained by a stationary-point fold.
                if !truncated || cursor < lines.len() || values.orbitals.is_none() {
                    values.orbitals = Some(orbitals);
                }
            }
        } else if trimmed.starts_with("MULLIKEN ATOMIC CHARGES") {
            if let Some(charges) = mulliken(lines, &mut cursor, offset, &step.frame, truncated)? {
                values.charges = Some(charges);
            }
        } else if let Some((_, right)) = trimmed.split_once("Total Dipole Moment") {
            let right = right.trim_start().strip_prefix(':').ok_or_else(|| {
                Error::at(offset + i + 1, "The ORCA dipole moment has no components")
            })?;
            let words: Vec<_> = right.split_whitespace().collect();
            let [x, y, z] = words.as_slice() else {
                if truncated && i + 1 == lines.len() {
                    continue;
                }
                return Err(Error::at(
                    offset + i + 1,
                    "The ORCA dipole moment needs three components in atomic units",
                ));
            };
            if truncated
                && i + 1 == lines.len()
                && words
                    .iter()
                    .any(|word| word.replace(['D', 'd'], "E").parse::<f64>().is_err())
            {
                continue;
            }
            values.dipole = Some([
                number(x, offset + i + 1, "dipole x component")?,
                number(y, offset + i + 1, "dipole y component")?,
                number(z, offset + i + 1, "dipole z component")?,
            ]);
        } else if trimmed == "VIBRATIONAL FREQUENCIES" {
            if let Some(frequencies) = frequencies(
                lines,
                &mut cursor,
                offset,
                step.frame.numbers.len(),
                truncated,
            )? {
                values.frequencies = Some(frequencies);
            }
        } else if trimmed == "NORMAL MODES" && step.frame.numbers.len() <= MAX_HESSIAN_ATOMS {
            if let Some(modes) = normal_modes(
                lines,
                &mut cursor,
                offset,
                step.frame.numbers.len(),
                truncated,
            )? {
                values.modes = Some(modes);
            }
        } else if trimmed == "IR SPECTRUM" {
            let last_mode = values
                .frequencies
                .as_ref()
                .and_then(|modes| modes.last())
                .map(|mode| mode.0);
            if let Some(intensities) =
                ir_spectrum(lines, &mut cursor, offset, truncated, last_mode)?
            {
                values.intensities = Some(intensities);
            }
        }
    }
    if step.energy_hartree.is_none() {
        values.scf_failure = scf_failure;
    }
    Ok(())
}

fn empty_orbital_set() -> OrbitalSet {
    OrbitalSet {
        energies_hartree: Vec::new(),
        occupations: Vec::new(),
        symmetries: Vec::new(),
        rows: Vec::new(),
        coefficients: Vec::new(),
    }
}

// Table readers advance the shared cursor to the first line outside their table.
fn orbital_energies(
    lines: &[&str],
    cursor: &mut usize,
    offset: usize,
    truncated: bool,
    multiplicity: u32,
) -> Result<Option<Orbitals>, Error> {
    let mut alpha = empty_orbital_set();
    let mut beta = empty_orbital_set();
    let mut unrestricted = false;
    let mut down = false;
    let mut table = false;
    let mut previous = None;
    while let Some(line) = lines.get(*cursor) {
        let i = *cursor;
        let trimmed = line.trim();
        if trimmed.contains("SPIN UP ORBITALS") || trimmed.contains("SPIN DOWN ORBITALS") {
            unrestricted = true;
            down = trimmed.contains("SPIN DOWN");
            previous = None;
            table = false;
            *cursor += 1;
            continue;
        }
        if trimmed.contains("OCC") && trimmed.contains("E(Eh)") {
            table = true;
            *cursor += 1;
            continue;
        }
        if trimmed.is_empty() || trimmed.bytes().all(|byte| byte == b'-') {
            *cursor += 1;
            continue;
        }
        if !table {
            break;
        }
        let words: Vec<_> = trimmed.split_whitespace().collect();
        let Some(index) = words.first().and_then(|word| word.parse::<usize>().ok()) else {
            break;
        };
        let [_, occupation, energy, _, ..] = words.as_slice() else {
            if truncated && i + 1 == lines.len() {
                return Ok(None);
            }
            return Err(Error::at(
                offset + i + 1,
                "An ORCA orbital row needs increasing orbital indices, occupation and energy in hartree",
            ));
        };
        if truncated
            && i + 1 == lines.len()
            && [occupation, energy]
                .iter()
                .any(|word| word.replace(['D', 'd'], "E").parse::<f64>().is_err())
        {
            return Ok(None);
        }
        if previous.is_some_and(|last| index <= last) {
            if truncated && i + 1 == lines.len() {
                return Ok(None);
            }
            return Err(Error::at(
                offset + i + 1,
                "An ORCA orbital row needs increasing orbital indices, occupation and energy in hartree",
            ));
        }
        previous = Some(index);
        let set = if down { &mut beta } else { &mut alpha };
        set.occupations
            .push(number(occupation, offset + i + 1, "orbital occupation")?);
        set.energies_hartree
            .push(number(energy, offset + i + 1, "orbital energy")?);
        *cursor += 1;
    }
    if alpha.energies_hartree.is_empty() || (unrestricted && beta.energies_hartree.is_empty()) {
        return Ok(None);
    }
    let beta = if unrestricted {
        Beta::Unrestricted(beta)
    } else if multiplicity > 1 || alpha.occupations.contains(&1.0) {
        let occupations = alpha
            .occupations
            .iter_mut()
            .map(|occupation| {
                let beta = (*occupation - 1.0).max(0.0);
                *occupation = occupation.min(1.0);
                beta
            })
            .collect();
        Beta::SameOrbitals { occupations }
    } else {
        Beta::Restricted
    };
    Ok(Some(Orbitals {
        basis: None,
        core_electrons: Vec::new(),
        alpha,
        beta,
    }))
}

fn mulliken(
    lines: &[&str],
    cursor: &mut usize,
    offset: usize,
    frame: &Frame,
    truncated: bool,
) -> Result<Option<Vec<f64>>, Error> {
    let start = (*cursor).saturating_sub(1);
    let mut charges = Vec::new();
    while let Some(line) = lines.get(*cursor) {
        let i = *cursor;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.bytes().all(|byte| byte == b'-') {
            if charges.is_empty() {
                *cursor += 1;
                continue;
            }
            break;
        }
        let words: Vec<_> = trimmed.split_whitespace().collect();
        let Some(index) = words.first().and_then(|word| word.parse::<usize>().ok()) else {
            break;
        };
        let [_, symbol, separator, charge, ..] = words.as_slice() else {
            if truncated && i + 1 == lines.len() {
                break;
            }
            return Err(Error::at(
                offset + i + 1,
                "The ORCA Mulliken charges do not match the geometry's atom order",
            ));
        };
        if truncated
            && i + 1 == lines.len()
            && charge.replace(['D', 'd'], "E").parse::<f64>().is_err()
        {
            break;
        }
        if *separator != ":"
            || index != charges.len()
            || frame.numbers.get(index).copied() != element(symbol).map(|atom| atom.0)
        {
            return Err(Error::at(
                offset + i + 1,
                "The ORCA Mulliken charges do not match the geometry's atom order",
            ));
        }
        charges.push(number(charge, offset + i + 1, "Mulliken charge")?);
        *cursor += 1;
    }
    if charges.len() != frame.numbers.len() {
        if truncated {
            return Ok(None);
        }
        return Err(Error::at(
            offset + start + 1,
            "The ORCA Mulliken block needs one charge for every atom",
        ));
    }
    Ok(Some(charges))
}

fn frequencies(
    lines: &[&str],
    cursor: &mut usize,
    offset: usize,
    atoms: usize,
    truncated: bool,
) -> Result<Option<Vec<(usize, f64)>>, Error> {
    let start = (*cursor).saturating_sub(1);
    let mut modes = Vec::new();
    while let Some(line) = lines.get(*cursor) {
        let i = *cursor;
        let trimmed = line.trim();
        let row = trimmed.split_once(':').and_then(|(left, right)| {
            left.trim()
                .parse::<usize>()
                .ok()
                .map(|index| (index, right))
        });
        let Some((index, right)) = row else {
            if modes.is_empty()
                && (trimmed.is_empty()
                    || trimmed.bytes().all(|byte| byte == b'-')
                    || trimmed.starts_with("Scaling factor for frequencies")
                    || trimmed.starts_with("Point group:")
                    || trimmed == "Irrep")
            {
                *cursor += 1;
                continue;
            }
            break;
        };
        let words: Vec<_> = right.split_whitespace().collect();
        let [frequency, unit, ..] = words.as_slice() else {
            if truncated && i + 1 == lines.len() {
                return Ok(None);
            }
            return Err(Error::at(
                offset + i + 1,
                "The ORCA frequency table needs consecutive modes in cm^-1",
            ));
        };
        if truncated
            && i + 1 == lines.len()
            && frequency.replace(['D', 'd'], "E").parse::<f64>().is_err()
        {
            return Ok(None);
        }
        if index != modes.len() || index >= 3 * atoms || *unit != "cm**-1" {
            if truncated && i + 1 == lines.len() {
                return Ok(None);
            }
            return Err(Error::at(
                offset + i + 1,
                "The ORCA frequency table needs consecutive modes in cm^-1",
            ));
        }
        let mut frequency = number(frequency, offset + i + 1, "vibrational frequency")?;
        if right.to_ascii_lowercase().contains("imaginary") {
            frequency = -frequency.abs();
        }
        modes.push((index, frequency));
        *cursor += 1;
        if modes.len() == 3 * atoms {
            break;
        }
    }
    if modes.len() != 3 * atoms {
        if truncated || modes.is_empty() {
            return Ok(None);
        }
        return Err(Error::at(
            offset + start + 1,
            "The ORCA frequency table ends before all 3N modes",
        ));
    }
    let rigid = modes
        .iter()
        .take_while(|(_, frequency)| *frequency == 0.0)
        .count();
    let remove = if rigid == 5 { 5 } else { 6.min(3 * atoms) };
    if rigid < remove {
        return Err(Error::at(
            offset + start + 1,
            "The ORCA frequencies do not identify five or six zero-frequency rigid-body modes",
        ));
    }
    Ok(Some(modes.into_iter().skip(remove).collect()))
}

fn normal_modes(
    lines: &[&str],
    cursor: &mut usize,
    offset: usize,
    atoms: usize,
    truncated: bool,
) -> Result<Option<BTreeMap<usize, Vec<f64>>>, Error> {
    let start = (*cursor).saturating_sub(1);
    let axes = 3 * atoms;
    let mut modes = BTreeMap::new();
    while let Some(line) = lines.get(*cursor) {
        let trimmed = line.trim();
        let mut words = trimmed.split_whitespace();
        // Allocate only for a numeric header, with space for this geometry's modes.
        // Check one excess token without storing it in the bounded column vector.
        let columns = if words
            .clone()
            .next()
            .is_some_and(|word| word.parse::<usize>().is_ok())
        {
            let mut columns = Vec::with_capacity(axes);
            let valid = words.by_ref().take(axes).all(|word| {
                if let Ok(mode) = word.parse::<usize>() {
                    columns.push(mode);
                    true
                } else {
                    false
                }
            });
            valid.then_some(columns)
        } else {
            None
        };
        let excess_columns = words.next().is_some();
        let Some(columns) = columns.filter(|columns| !columns.is_empty()) else {
            if trimmed.is_empty()
                || trimmed.bytes().all(|byte| byte == b'-')
                || (modes.is_empty()
                    && [
                        "These modes are the Cartesian displacements",
                        "M(i,i)=",
                        "Thus, these vectors are normalized",
                        "Point group:",
                    ]
                    .iter()
                    .any(|prefix| trimmed.starts_with(prefix)))
            {
                *cursor += 1;
                continue;
            }
            break;
        };
        if excess_columns
            || columns.iter().any(|&mode| mode >= axes)
            || columns
                .array_windows::<2>()
                .any(|[first, second]| first >= second)
        {
            if truncated && *cursor + 1 == lines.len() {
                return Ok(None);
            }
            return Err(Error::at(
                offset + *cursor + 1,
                "The ORCA normal-mode column indices are outside this geometry",
            ));
        }
        *cursor += 1;
        // ORCA can print one symmetry label per mode between columns and rows.
        if let Some(line) = lines.get(*cursor) {
            let mut labels = line.split_whitespace();
            let mut count = 0;
            if labels.by_ref().take(columns.len()).all(|label| {
                count += 1;
                label.chars().any(char::is_alphabetic)
            }) && count == columns.len()
                && labels.next().is_none()
            {
                *cursor += 1;
            }
        }
        let mut panel = vec![Vec::with_capacity(axes); columns.len()];
        for row in 0..axes {
            let Some(line) = lines.get(*cursor) else {
                if truncated {
                    return Ok(None);
                }
                return Err(Error::at(
                    offset + *cursor + 1,
                    "The ORCA normal-mode panel ends before all 3N Cartesian rows",
                ));
            };
            let expected = columns.len() + 1;
            let mut words = line.split_whitespace();
            let mut fields = Vec::with_capacity(expected);
            fields.extend(words.by_ref().take(expected));
            if fields.len() != expected
                || words.next().is_some()
                || fields.first().and_then(|word| word.parse::<usize>().ok()) != Some(row)
            {
                if truncated {
                    return Ok(None);
                }
                return Err(Error::at(
                    offset + *cursor + 1,
                    "The ORCA normal-mode panel needs 3N consecutive Cartesian rows",
                ));
            }
            for (word, vector) in fields.iter().skip(1).zip(&mut panel) {
                if truncated
                    && *cursor + 1 == lines.len()
                    && word.replace(['D', 'd'], "E").parse::<f64>().is_err()
                {
                    return Ok(None);
                }
                vector.push(number(
                    word,
                    offset + *cursor + 1,
                    "normal-mode displacement",
                )?);
            }
            *cursor += 1;
        }
        for (index, vector) in columns.into_iter().zip(panel) {
            if modes.insert(index, vector).is_some() {
                return Err(Error::at(
                    offset + *cursor,
                    "An ORCA normal mode is printed twice in one block",
                ));
            }
        }
        if modes.len() == axes {
            return Ok(Some(modes));
        }
    }
    if modes.is_empty() || truncated {
        return Ok(None);
    }
    Err(Error::at(
        offset + start + 1,
        "The ORCA normal-mode block ends before all 3N modes",
    ))
}

fn ir_spectrum(
    lines: &[&str],
    cursor: &mut usize,
    offset: usize,
    truncated: bool,
    last_mode: Option<usize>,
) -> Result<Option<BTreeMap<usize, f64>>, Error> {
    let start = (*cursor).saturating_sub(1);
    let mut intensity_column = None;
    let mut intensities = BTreeMap::new();
    while let Some(line) = lines.get(*cursor) {
        let i = *cursor;
        let trimmed = line.trim();
        let words: Vec<_> = trimmed.split_whitespace().collect();
        if words.first() == Some(&"Mode") {
            intensity_column = words
                .iter()
                .position(|word| ["Int", "Intensity"].contains(word));
            *cursor += 1;
            continue;
        }
        let Some(index) = words
            .first()
            .and_then(|word| word.strip_suffix(':'))
            .and_then(|word| word.parse::<usize>().ok())
        else {
            if intensities.is_empty()
                && (trimmed.is_empty()
                    || trimmed.bytes().all(|byte| byte == b'-')
                    || trimmed.starts_with("cm**-1"))
            {
                *cursor += 1;
                continue;
            }
            break;
        };
        let column = intensity_column.ok_or_else(|| {
            Error::Unsupported(format!(
                "Line {}: The ORCA IR spectrum has no recognized intensity column in km/mol",
                offset + start + 1
            ))
        })?;
        let Some(value) = words.get(column) else {
            if truncated && i + 1 == lines.len() {
                return Ok(None);
            }
            return Err(Error::at(
                offset + i + 1,
                "The ORCA IR spectrum row has no intensity",
            ));
        };
        if truncated
            && i + 1 == lines.len()
            && value.replace(['D', 'd'], "E").parse::<f64>().is_err()
        {
            return Ok(None);
        }
        let value = number(value, offset + i + 1, "IR intensity")?;
        if value < 0.0 || intensities.insert(index, value).is_some() {
            return Err(Error::at(
                offset + i + 1,
                "The ORCA IR spectrum has a negative intensity or repeated mode index",
            ));
        }
        *cursor += 1;
    }
    if intensities.is_empty()
        || last_mode.is_some_and(|mode| !intensities.contains_key(&mode))
        || (last_mode.is_none() && truncated && *cursor == lines.len())
    {
        Ok(None)
    } else {
        Ok(Some(intensities))
    }
}

fn vibrations(values: &mut Values, atoms: usize) -> Result<Option<Vibrations>, Error> {
    let Some(mut frequencies) = values.frequencies.take() else {
        return Ok(None);
    };
    frequencies.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut vibrations = Vibrations {
        frequencies_cm1: frequencies.iter().map(|mode| mode.1).collect(),
        ir_intensities_km_mol: Vec::new(),
        modes: Vec::new(),
        reduced_masses_amu: Vec::new(),
        masses_amu: Vec::new(),
    };
    if let Some(intensities) = &values.intensities {
        vibrations.ir_intensities_km_mol = frequencies
            .iter()
            .map(|(index, _)| intensities.get(index).copied().unwrap_or(0.0))
            .collect();
    }
    if let Some(modes) = &values.modes
        && frequencies.iter().all(|(index, _)| {
            modes
                .get(index)
                .is_some_and(|vector| vector.len() == 3 * atoms)
        })
    {
        for (index, _) in &frequencies {
            let Some(vector) = modes.get(index) else {
                continue;
            };
            let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
            if !norm.is_finite() || norm == 0.0 {
                return Err(Error::Invalid(format!(
                    "ORCA vibrational mode {index} has no nonzero finite Cartesian displacement"
                )));
            }
            vibrations
                .modes
                .extend(vector.iter().map(|value| value / norm));
        }
    }
    Ok(Some(vibrations))
}

#[cfg(test)]
#[path = "tests/orca.rs"]
mod tests;
