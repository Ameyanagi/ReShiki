//! Gaussian log jobs, coordinate steps, and results.
use super::{Completion, Error, Job, JobKind, Origin, QcFile, QcProgram, Step, frame};
use reshiki_qcdata::{
    AtomicCharges, Beta, ChargeScheme, DEBYE_PER_E_BOHR, Frame, MAX_HESSIAN_ATOMS, OrbitalSet,
    Orbitals, Status, Task, Vibrations,
};
use std::collections::BTreeMap;

type Lines<'a> = [(usize, &'a str)];

const ARCHIVE_ENERGIES: &[&str] = &[
    "HF", "MP2", "MP3", "MP4D", "MP4DQ", "MP4SDQ", "MP4SDTQ", "MP5", "CISD", "QCISD", "QCISD(T)",
    "CCSD", "CCSD(T)",
];
const NORMAL_TERMINATION: &str = "Normal termination of Gaussian ";
const STANDARD_ORIENTATION: &str = "Standard orientation:";
const HARMONIC_FREQUENCIES: &str = "Harmonic frequencies (cm**-1)";

/// Read Gaussian 03, 09, and 16 log jobs in file order.
pub(super) fn read(text: &str, origin: &Origin<'_>) -> Result<QcFile, Error> {
    let lines: Vec<_> = text
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect();
    let mut program = QcProgram {
        name: "Gaussian".into(),
        version: None,
    };

    let mut first_version = None;
    let mut jobs = Vec::new();
    let mut warnings = Vec::new();
    let mut start = None;
    for (i, &(_, line)) in lines.iter().enumerate() {
        let line = line.trim();
        let invocation = line.starts_with("Entering Gaussian System");
        let restart = invocation
            || line.starts_with("Link1:")
                && line.contains("Proceeding to internal job step number");
        if restart {
            if let Some(job) = start.replace(i).and_then(|from| lines.get(from..i)) {
                jobs.push(read_job(job, jobs.len(), origin, &program, &mut warnings)?);
            }
            if invocation {
                program.version = None;
            }
        } else if let Some(version) = version(line) {
            first_version.get_or_insert_with(|| version.clone());
            program.version = Some(version);
            start.get_or_insert(i);
        } else if line == "Initial command:" || line.starts_with('#') {
            start.get_or_insert(i);
        }
        if termination(line).is_some()
            && let Some(job) = start.take().and_then(|from| lines.get(from..=i))
        {
            jobs.push(read_job(job, jobs.len(), origin, &program, &mut warnings)?);
        }
    }
    if let Some(job) = start.and_then(|from| lines.get(from..)) {
        jobs.push(read_job(job, jobs.len(), origin, &program, &mut warnings)?);
    }
    Ok(QcFile {
        format: origin.format,
        program: Some(program.name),
        program_version: first_version,
        jobs,
        warnings,
    })
}

fn version(line: &str) -> Option<String> {
    let (number, rest) = line.strip_prefix("Gaussian ")?.split_once([',', ':'])?;
    if !matches!(number, "03" | "09" | "16") {
        return None;
    }
    let rest = rest.trim_start();
    let revision = if let Some(revision) = rest.strip_prefix("Revision ") {
        revision.split(',').next()?
    } else {
        rest.split_whitespace().next()?.split_once("Rev")?.1
    };
    Some(format!("{number} Revision {revision}"))
}

enum Termination {
    Normal,
    Error,
}

fn termination(line: &str) -> Option<Termination> {
    if line.starts_with("Error termination via ") {
        return Some(Termination::Error);
    }
    let (version, _) = line.strip_prefix(NORMAL_TERMINATION)?.split_once(" at ")?;
    (!version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit()))
        .then_some(Termination::Normal)
}

fn completion(lines: &Lines<'_>) -> Completion {
    let Some(((.., last), before)) = lines.split_last() else {
        return Completion::Truncated;
    };
    let last = last.trim();
    match termination(last) {
        Some(Termination::Normal) => return Completion::Normal,
        Some(Termination::Error) => {}
        None => return Completion::Truncated,
    }
    let message = before
        .iter()
        .rev()
        .map(|&(_, line)| line.trim())
        .take_while(|line| !line.starts_with("(Enter "))
        .find(|line| {
            if line.starts_with("Error termination request") || line.starts_with("Leave Link") {
                return false;
            }
            let line = line.to_ascii_lowercase();
            [
                "error",
                "failed",
                "failure",
                "exceeded",
                "cannot",
                "unable",
                "not converg",
                "unrecognized",
                "impossible",
                "not implemented",
                "problem with",
                "too close",
                "run terminated",
                "aborting",
            ]
            .iter()
            .any(|word| line.contains(word))
        })
        .unwrap_or(last)
        .into();
    Completion::Error { message }
}

fn read_job(
    lines: &Lines<'_>,
    index: usize,
    origin: &Origin<'_>,
    program: &QcProgram,
    warnings: &mut Vec<String>,
) -> Result<Job, Error> {
    if index >= super::MAX_JOBS {
        return Err(Error::Unsupported(format!(
            "The file holds more than {} Gaussian jobs",
            super::MAX_JOBS
        )));
    }
    let completion = completion(lines);
    let partial = completion != Completion::Normal;
    let keywords = route(lines);
    let archive = archive(lines);
    let kind = job_kind(lines, &keywords, archive.as_ref());
    let (method, basis) = match &archive {
        Some(archive) => (Some(archive.method.clone()), Some(archive.basis.clone())),
        None => route_level(&keywords),
    };
    let mut job = Job {
        index: index as u32,
        kind,
        method,
        basis,
        keywords,
        completion: Some(completion.clone()),
        steps: Vec::new(),
        result: None,
    };

    let mut orientations = lines
        .iter()
        .enumerate()
        .filter_map(|(i, &(_, line))| match line.trim() {
            kind @ (STANDARD_ORIENTATION | "Input orientation:" | "Z-Matrix orientation:") => {
                Some((i, kind))
            }
            _ => None,
        })
        .peekable();

    let mut step_starts = Vec::new();
    let mut state = None;
    let mut bound = None;
    let mut energy_terms = BTreeMap::new();
    #[expect(
        clippy::indexing_slicing,
        reason = "the orientation boundaries come from the same line slice"
    )]
    while let Some((start, kind)) = orientations.next() {
        step_starts.push(start);
        let first_end = orientations.peek().map_or(lines.len(), |&(i, _)| i);
        let mut end = first_end;
        // Gaussian prints the input coordinates before rotating the same geometry.
        let standard = if kind != STANDARD_ORIENTATION
            && let Some(&(standard, STANDARD_ORIENTATION)) = orientations.peek()
            && !lines[start..standard]
                .iter()
                .any(|&(_, line)| line.trim_start().starts_with("SCF Done:"))
        {
            orientations.next();
            end = orientations.peek().map_or(lines.len(), |&(i, _)| i);
            geometry(&lines[standard..end], partial && end == lines.len())?
        } else {
            None
        };
        let coordinates = match standard {
            Some(coordinates) => Some(coordinates),
            None => geometry(
                &lines[start..first_end],
                partial && first_end == lines.len(),
            )?,
        };
        let interrupted = partial && end == lines.len();
        let Some((atoms, positions)) = coordinates else {
            break;
        };
        let (charge, multiplicity) = match state {
            Some(state) => state,
            None => *state.insert(electronic_state(lines)?),
        };
        let frame = frame(&atoms, positions, charge, multiplicity)?;
        let (energy_hartree, terms) = step_energy(&lines[start..end], interrupted)?;
        if energy_hartree.is_some() {
            bound = Some(job.steps.len());
            energy_terms = terms;
        }
        job.steps.push(Step {
            frame,
            energy_hartree,
            label: None,
        });
    }
    let at = bound.unwrap_or(job.steps.len().saturating_sub(1));
    let Some((step, &start)) = job.steps.get(at).zip(step_starts.get(at)) else {
        return Ok(job);
    };

    // Link 601 may print final populations after a reprint of the converged geometry.
    let end = at
        + job
            .steps
            .iter()
            .skip(at)
            .take_while(|s| s.frame == step.frame)
            .count();
    let stop = step_starts.get(end).copied().unwrap_or(lines.len());
    let analysis = lines.get(start..stop).unwrap_or_default();
    let partial_analysis = partial && stop == lines.len();
    let within_mode_limit = step.frame.numbers.len() <= MAX_HESSIAN_ATOMS;
    let task = match job.kind {
        JobKind::Optimization => Task::Optimize,
        JobKind::Frequencies if within_mode_limit => Task::Hessian,
        _ => Task::Energy,
    };
    let mut result = origin.result(
        &step.frame,
        task,
        status(lines, completion, kind),
        program,
        &job,
    );
    result.energy_hartree = step.energy_hartree;
    if let Some(archive) = archive.filter(|archive| {
        analysis
            .first()
            .zip(analysis.last())
            .is_some_and(|(first, last)| first.0 <= archive.line && archive.line <= last.0)
    }) {
        for &key in ARCHIVE_ENERGIES {
            if let Some(value) = archive.energy(key) {
                // A scan archive lists one value per point, in the same order as its steps.
                let value = value.rsplit(',').next().unwrap_or(value);
                let energy = number(value, archive.line, "archive energy")?;
                energy_terms.insert(key.to_ascii_lowercase(), energy);
                result.energy_hartree = Some(energy);
            }
        }
    }
    result.energy_terms = energy_terms;
    result.orbitals = orbitals(
        analysis,
        step.frame.multiplicity,
        job.method.as_deref(),
        partial_analysis,
    )?;
    if let Some(charges) = mulliken(analysis, &step.frame, partial_analysis)? {
        result.charges.push(charges);
    }
    result.dipole_e_bohr = dipole(analysis, partial_analysis)?;
    if end == job.steps.len() {
        if within_mode_limit {
            result.vibrations = vibrations(analysis, &step.frame, partial_analysis)?;
        } else if analysis
            .iter()
            .any(|&(_, line)| line.trim_start().starts_with(HARMONIC_FREQUENCIES))
        {
            warnings.push(format!(
                "Job {}: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms",
                job.index + 1
            ));
        }
    }
    job.result = Some(result);
    Ok(job)
}

fn status(lines: &Lines<'_>, completion: Completion, kind: JobKind) -> Status {
    match completion {
        Completion::Error { message } => Status::Failed { reason: message },
        Completion::Truncated => Status::NotConverged {
            reason: "the file ends before the job finished".into(),
        },
        Completion::Normal if kind == JobKind::Optimization => lines
            .iter()
            .rev()
            .find_map(|&(_, line)| {
                let line = line.trim();
                if matches!(
                    line,
                    "Optimization completed." | "-- Stationary point found."
                ) {
                    Some(Status::Converged)
                } else if line.starts_with("Optimization stopped.")
                    || line.contains("Number of steps exceeded")
                {
                    Some(Status::NotConverged {
                        reason: line.into(),
                    })
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Status::NotConverged {
                reason: "Gaussian did not report optimization convergence".into(),
            }),
        Completion::Normal => Status::Converged,
    }
}

fn dashed(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.bytes().all(|ch| ch == b'-')
}

fn route(lines: &Lines<'_>) -> Vec<String> {
    let Some((i, first)) = lines
        .iter()
        .enumerate()
        .find_map(|(i, &(_, line))| Some((i, line.trim_start().strip_prefix('#')?)))
    else {
        return Vec::new();
    };
    let mut first = first.split_whitespace().peekable();
    if first
        .peek()
        .is_some_and(|token| matches!(token.to_ascii_lowercase().as_str(), "p" | "n" | "t"))
    {
        first.next();
    }
    first
        .chain(
            lines
                .iter()
                .skip(i + 1)
                .take_while(|&(_, line)| !dashed(line))
                .flat_map(|&(_, line)| line.split_whitespace()),
        )
        .map(str::to_string)
        .collect()
}

fn route_level(keywords: &[String]) -> (Option<String>, Option<String>) {
    let route = keywords.join(" ");
    for (slash, _) in route.match_indices('/') {
        let Some((before, after)) = route.split_at_checked(slash) else {
            continue;
        };
        let method = before.split_whitespace().next_back().unwrap_or("");
        let basis = after
            .strip_prefix('/')
            .unwrap_or(after)
            .split_whitespace()
            .next()
            .unwrap_or("");
        if method.starts_with(|ch: char| ch.is_ascii_alphabetic())
            && !method.contains('=')
            && basis.starts_with(|ch: char| ch.is_ascii_alphanumeric())
        {
            return (Some(method.into()), Some(basis.into()));
        }
    }
    (None, None)
}

struct Archive {
    line: usize,
    kind: String,
    method: String,
    basis: String,
    record: String,
}

impl Archive {
    fn energy(&self, key: &str) -> Option<&str> {
        self.record.split('\\').skip(6).find_map(|field| {
            let (name, value) = field.split_once('=')?;
            (name == key).then_some(value)
        })
    }
}

fn archive(lines: &Lines<'_>) -> Option<Archive> {
    let start = lines
        .iter()
        .rposition(|&(_, line)| line.trim_start().starts_with("1\\1\\"))?;
    let parts = lines.get(start..)?;
    let line = parts.first()?.0;
    let mut record = String::new();
    for &(_, part) in parts {
        record.push_str(part.strip_prefix(' ').unwrap_or(part));
        if record.ends_with("\\@") {
            let mut fields = record.split('\\').skip(3);
            return Some(Archive {
                line,
                kind: fields.next()?.into(),
                method: fields.next()?.into(),
                basis: fields.next()?.into(),
                record,
            });
        }
    }
    None
}

fn job_kind(lines: &Lines<'_>, keywords: &[String], archive: Option<&Archive>) -> JobKind {
    let route = keywords.join(" ").to_ascii_lowercase();
    let has = |wanted| {
        route
            .split([' ', '=', '(', ')', ','])
            .any(|word| word == wanted)
    };
    if has("opt")
        && has("modredundant")
        && lines.iter().any(|&(_, line)| {
            let mut fields = line.split_whitespace().rev();
            matches!((fields.next(), fields.next(), fields.next(), fields.next()),
            (Some(delta), Some(points), Some("S" | "s"), Some(_))
                if delta.replace(['D', 'd'], "E").parse::<f64>().is_ok()
                    && points.parse::<u32>().is_ok())
        })
    {
        return JobKind::Scan;
    }
    if let Some(archive) = archive {
        return match archive.kind.as_str() {
            "SP" => JobKind::SinglePoint,
            "FOpt" | "POpt" => JobKind::Optimization,
            "Freq" => JobKind::Frequencies,
            "Scan" => JobKind::Scan,
            _ => JobKind::Other,
        };
    }
    if has("scan") {
        JobKind::Scan
    } else if has("opt") {
        JobKind::Optimization
    } else if has("freq") {
        JobKind::Frequencies
    } else if ["irc", "td", "nmr", "stable"].into_iter().any(has) {
        JobKind::Other
    } else {
        JobKind::SinglePoint
    }
}

fn electronic_state(lines: &Lines<'_>) -> Result<(i32, u32), Error> {
    for &(i, line) in lines {
        let Some(charge) = line.trim().strip_prefix("Charge") else {
            continue;
        };
        let Some((charge, multiplicity)) = charge.split_once("Multiplicity") else {
            continue;
        };
        let charge = charge.trim().trim_start_matches('=').trim();
        let multiplicity = multiplicity.trim().trim_start_matches('=').trim();
        return Ok((
            charge
                .parse()
                .map_err(|_| Error::at(i, "invalid Gaussian charge"))?,
            multiplicity
                .parse()
                .map_err(|_| Error::at(i, "invalid Gaussian multiplicity"))?,
        ));
    }
    Err(Error::at(
        lines.first().map_or(1, |(line, _)| *line),
        "Gaussian job has no charge and multiplicity",
    ))
}

type Coordinates = (Vec<(u8, u16)>, Vec<[f64; 3]>);

fn geometry(lines: &Lines<'_>, partial: bool) -> Result<Option<Coordinates>, Error> {
    let heading = lines.first().map_or(1, |(line, _)| *line);
    let mut separators = lines
        .iter()
        .enumerate()
        .filter_map(|(i, &(_, line))| dashed(line).then_some(i));
    let (Some(first), Some(header), Some(end)) =
        (separators.next(), separators.next(), separators.next())
    else {
        return if partial {
            Ok(None)
        } else {
            Err(Error::at(heading, "incomplete Gaussian orientation table"))
        };
    };
    if first != 1 || header != 4 {
        return Err(Error::at(
            heading,
            "invalid Gaussian orientation table header",
        ));
    }
    let mut atoms = Vec::new();
    let mut positions = Vec::new();
    for &(i, row) in lines.iter().take(end).skip(header + 1) {
        let fields: Vec<_> = row.split_whitespace().collect();
        let (z, xyz) = match fields.as_slice() {
            [_, z, _, x, y, zz] | [_, z, x, y, zz] => (z, [x, y, zz]),
            _ => return Err(Error::at(i, "invalid Gaussian orientation row")),
        };
        let z = match *z {
            "-1" => continue,
            "0" => {
                return Err(Error::Unsupported(format!(
                    "Line {i}: Gaussian ghost atoms (Bq) are unsupported"
                )));
            }
            z => z
                .parse::<u8>()
                .ok()
                .filter(|z| (1..=118).contains(z))
                .ok_or_else(|| Error::at(i, "invalid Gaussian atomic number"))?,
        };
        atoms.push((z, 0));
        positions.push([
            number(xyz[0], i, "X coordinate")?,
            number(xyz[1], i, "Y coordinate")?,
            number(xyz[2], i, "Z coordinate")?,
        ]);
    }
    Ok(Some((atoms, positions)))
}

fn number(value: &str, line: usize, what: &str) -> Result<f64, Error> {
    let parsed = if value.contains(['D', 'd']) {
        value.replace(['D', 'd'], "E").parse::<f64>()
    } else {
        value.parse::<f64>()
    };
    parsed
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| Error::at(line, format!("invalid Gaussian {what}")))
}

fn step_energy(
    lines: &Lines<'_>,
    partial: bool,
) -> Result<(Option<f64>, BTreeMap<String, f64>), Error> {
    let mut terms = BTreeMap::new();
    let mut energy = None;
    'lines: for (offset, &(i, line)) in lines.iter().enumerate() {
        let last_partial = partial && offset + 1 == lines.len();
        let line = line.trim();
        if let Some(scf) = line.strip_prefix("SCF Done:") {
            let value = scf.split_once('=').map_or("", |(_, value)| value);
            let scf = match number(
                value.split_whitespace().next().unwrap_or(""),
                i,
                "SCF energy",
            ) {
                Ok(scf) => scf,
                Err(_) if last_partial => break,
                Err(error) => return Err(error),
            };
            terms.clear();
            terms.insert("scf".into(), scf);
            energy = Some(scf);
            continue;
        }
        let mut fields = line.split_whitespace().peekable();
        while let Some(field) = fields.next() {
            let assignment = field.split_once('=');
            let tag = assignment.map_or(field, |(tag, _)| tag);
            let key = match tag {
                "EUMP2" | "ERMP2" | "E(MP2)" => "mp2",
                "EUMP3" | "ERMP3" | "UMP3" => "mp3",
                "UMP4(D)" | "EUMP4(D)" => "mp4d",
                "UMP4(DQ)" | "EUMP4(DQ)" => "mp4dq",
                "UMP4(SDQ)" | "EUMP4(SDQ)" => "mp4sdq",
                "UMP4(SDTQ)" | "EUMP4(SDTQ)" => "mp4sdtq",
                "MP5" | "EUMP5" | "ERMP5" => "mp5",
                "E(CORR)" => "correlated",
                "CISD" | "E(CISD)" => "cisd",
                "QCISD" | "E(QCISD)" => "qcisd",
                "QCISD(T)" | "E(QCISD(T))" => "qcisd(t)",
                "CCSD" | "E(CCSD)" => "ccsd",
                "CCSD(T)" | "E(CCSD(T))" => "ccsd(t)",
                _ => continue,
            };
            let value = match assignment {
                Some((_, value)) => value,
                None if fields.peek() == Some(&"=") => {
                    fields.next();
                    ""
                }
                None => continue,
            };
            let value = if value.is_empty() {
                fields.next().unwrap_or("")
            } else {
                value
            };
            let value = match number(value, i, "correlated energy") {
                Ok(value) => value,
                Err(_) if last_partial => break 'lines,
                Err(error) => return Err(error),
            };
            terms.insert(key.into(), value);
            energy = Some(value);
        }
    }
    Ok((energy, terms))
}

fn eigenvalues(line: &str) -> Option<(bool, bool, &str)> {
    let (label, values) = line.trim_start().split_once("eigenvalues --")?;
    let mut fields = label.split_whitespace();
    let is_beta = match fields.next()? {
        "Alpha" => false,
        "Beta" => true,
        _ => return None,
    };
    let occupied = match fields.next()? {
        "occ." => true,
        "virt." => false,
        _ => return None,
    };
    Some((
        is_beta,
        occupied,
        values.strip_prefix(' ').unwrap_or(values),
    ))
}

fn orbitals(
    lines: &Lines<'_>,
    multiplicity: u32,
    method: Option<&str>,
    partial: bool,
) -> Result<Option<Orbitals>, Error> {
    let Some((end, &(end_line, _))) = lines
        .iter()
        .enumerate()
        .rfind(|&(_, &(_, line))| eigenvalues(line).is_some())
    else {
        return Ok(None);
    };
    let (mut alpha, mut beta) = (Vec::new(), Vec::new());
    let last_line = lines.last().map(|(line, _)| *line);
    for &(i, line) in lines.iter().take(end + 1).rev() {
        let Some((is_beta, occupied, values)) = eigenvalues(line) else {
            break;
        };
        let levels = if is_beta { &mut beta } else { &mut alpha };
        for field in values.trim_end().as_bytes().chunks(10) {
            let field = std::str::from_utf8(field)
                .map_err(|_| Error::at(i, "invalid Gaussian orbital energy"))?;
            match number(field.trim(), i, "orbital energy") {
                Ok(energy) => levels.push((energy, occupied)),
                Err(_) if partial && Some(i) == last_line => return Ok(None),
                Err(error) => return Err(error),
            }
        }
    }
    if alpha.is_empty() {
        return if partial {
            Ok(None)
        } else {
            Err(Error::at(
                end_line,
                "Gaussian orbital spectrum has no alpha levels",
            ))
        };
    }
    let set = |mut levels: Vec<(f64, bool)>, occupied| {
        levels.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
        OrbitalSet {
            energies_hartree: levels.iter().map(|&(energy, _)| energy).collect(),
            occupations: levels
                .iter()
                .map(|&(_, filled)| if filled { occupied } else { 0.0 })
                .collect(),
            symmetries: Vec::new(),
            rows: Vec::new(),
            coefficients: Vec::new(),
        }
    };
    let unrestricted = method.is_some_and(|method| method.starts_with(['U', 'u']))
        || lines.iter().any(|&(_, line)| {
            line.trim_start()
                .strip_prefix("SCF Done:")
                .is_some_and(|scf| scf.trim_start().starts_with("E(U"))
        });
    let (alpha, beta) = if !beta.is_empty() {
        (set(alpha, 1.0), Beta::Unrestricted(set(beta, 1.0)))
    } else if multiplicity == 1 && !unrestricted {
        (set(alpha, 2.0), Beta::Restricted)
    } else if partial {
        return Ok(None);
    } else if unrestricted {
        return Err(Error::at(
            end_line,
            "Gaussian unrestricted orbital spectrum has no beta levels",
        ));
    } else {
        return Err(Error::Unsupported(format!(
            "Line {end_line}: Gaussian printed only one orbital spectrum for an open-shell job",
        )));
    };
    Ok(Some(Orbitals {
        basis: None,
        core_electrons: Vec::new(),
        alpha,
        beta,
    }))
}

fn mulliken(
    lines: &Lines<'_>,
    frame: &Frame,
    partial: bool,
) -> Result<Option<AtomicCharges>, Error> {
    let Some((at, &(heading, _))) = lines.iter().enumerate().rfind(|&(_, &(_, line))| {
        matches!(
            line.trim().trim_end_matches(':'),
            "Mulliken charges" | "Mulliken atomic charges" | "Mulliken charges and spin densities"
        )
    }) else {
        return Ok(None);
    };
    let mut values = Vec::new();
    let mut rows = lines.get(at + 1..).unwrap_or_default().iter();
    let last_line = lines.last().map(|(line, _)| *line);
    // A column-number row precedes the atomic populations.
    if rows.as_slice().first().is_some_and(|&(_, row)| {
        row.split_whitespace()
            .all(|field| field.parse::<usize>().is_ok())
    }) {
        rows.next();
    }
    for &(i, row) in rows {
        let last_partial = partial && Some(i) == last_line;
        let mut fields = row.split_whitespace();
        if !fields
            .next()
            .is_some_and(|index| index.parse::<usize>().is_ok())
        {
            break;
        }
        let symbol = fields.next().and_then(super::element);
        if symbol.map(|(z, _)| z) != frame.numbers.get(values.len()).copied() || symbol.is_none() {
            if last_partial {
                return Ok(None);
            }
            return Err(Error::at(
                i,
                "Gaussian Mulliken charge row does not match the geometry",
            ));
        }
        let value = fields.next().unwrap_or("");
        match number(value, i, "Mulliken charge") {
            Ok(value) => values.push(value),
            Err(_) if last_partial => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    if values.len() != frame.numbers.len() {
        return if partial {
            Ok(None)
        } else {
            Err(Error::at(
                heading,
                "Gaussian Mulliken charge count does not match the geometry",
            ))
        };
    }
    Ok(Some(AtomicCharges {
        scheme: ChargeScheme::Mulliken,
        values,
    }))
}

fn dipole(lines: &Lines<'_>, partial: bool) -> Result<Option<[f64; 3]>, Error> {
    let Some((at, &(heading, _))) = lines
        .iter()
        .enumerate()
        .rfind(|&(_, &(_, line))| line.trim() == "Dipole moment (field-independent basis, Debye):")
    else {
        return Ok(None);
    };
    let Some(&(i, row)) = lines.get(at + 1) else {
        return if partial {
            Ok(None)
        } else {
            Err(Error::at(heading, "missing Gaussian dipole components"))
        };
    };
    let mut dipole = [0.0; 3];
    for (target, label) in dipole.iter_mut().zip(["X=", "Y=", "Z="]) {
        let Some(value) = row
            .split_once(label)
            .and_then(|(_, rest)| rest.split_whitespace().next())
        else {
            return if partial {
                Ok(None)
            } else {
                Err(Error::at(i, "missing Gaussian dipole component"))
            };
        };
        match number(value, i, "dipole component") {
            Ok(value) => *target = value / DEBYE_PER_E_BOHR,
            Err(_) if partial && at + 2 == lines.len() => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    Ok(Some(dipole))
}

struct Mode {
    frequency: f64,
    mass: Option<f64>,
    intensity: Option<f64>,
    displacement: Vec<f64>,
}

fn vibrations(
    lines: &Lines<'_>,
    frame: &Frame,
    partial: bool,
) -> Result<Option<Vibrations>, Error> {
    let mut sections = lines
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, &(_, line))| {
            line.trim_start()
                .starts_with(HARMONIC_FREQUENCIES)
                .then_some(i)
        });
    let Some((last, section)) = sections.next().and_then(|i| lines.get(i..).map(|s| (i, s))) else {
        return Ok(None);
    };
    let (mut modes, precise) = read_modes(section, frame, partial)?;
    // Gaussian prints the five-column HPModes table immediately before its rounded copy.
    if !precise
        && let Some(previous) = sections.next().and_then(|i| lines.get(i..last))
        && previous
            .iter()
            .any(|&(_, line)| line.trim() == "Coord Atom Element:")
    {
        let (high_precision, _) = read_modes(previous, frame, false)?;
        if (partial || high_precision.len() == modes.len())
            && high_precision
                .iter()
                .take(modes.len())
                .map(|m| m.frequency)
                .eq(modes.iter().map(|m| m.frequency))
        {
            modes = high_precision;
        }
    }
    if modes.is_empty() {
        return Ok(None);
    }
    modes.sort_by(|a, b| a.frequency.total_cmp(&b.frequency));
    Ok(Some(Vibrations {
        frequencies_cm1: modes.iter().map(|m| m.frequency).collect(),
        ir_intensities_km_mol: modes
            .iter()
            .map(|m| m.intensity)
            .collect::<Option<_>>()
            .unwrap_or_default(),
        reduced_masses_amu: modes
            .iter()
            .map(|m| m.mass)
            .collect::<Option<_>>()
            .unwrap_or_default(),
        modes: modes.into_iter().flat_map(|m| m.displacement).collect(),
        masses_amu: Vec::new(),
    }))
}

fn read_modes(lines: &Lines<'_>, frame: &Frame, partial: bool) -> Result<(Vec<Mode>, bool), Error> {
    let mut panels: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, &(_, line))| line.trim_start().starts_with("Frequencies --").then_some(i))
        .collect();
    panels.push(lines.len());
    let mut modes = Vec::new();
    let mut high_precision = false;
    'panels: for &[start, end] in panels.array_windows() {
        let Some(panel) = lines.get(start..end) else {
            continue;
        };
        let Some((&(line, frequencies), rows)) = panel.split_first() else {
            continue;
        };
        let partial_panel = partial && end == lines.len();
        let last_line = panel.last().map_or(line, |row| row.0);
        let (_, frequencies) = frequencies.split_once("--").unwrap_or_default();
        let precise = frequencies.starts_with('-');
        high_precision |= precise;
        let values = frequencies.trim_start_matches('-').split_whitespace();
        let max_columns = if precise { 5 } else { 3 };
        let columns = values.clone().take(max_columns + 1).count();
        if !(1..=max_columns).contains(&columns) {
            if partial_panel && columns == 0 && line == last_line {
                break;
            }
            return Err(Error::at(
                line,
                "invalid Gaussian normal mode count in a panel",
            ));
        }
        let block = values
            .map(|token| {
                Ok(Mode {
                    frequency: number(token, line, "frequency")?,
                    mass: None,
                    intensity: None,
                    displacement: Vec::with_capacity(frame.numbers.len() * 3),
                })
            })
            .collect::<Result<Vec<_>, Error>>();
        let mut block = match block {
            Ok(block) => block,
            Err(_) if partial_panel && line == last_line => break,
            Err(error) => return Err(error),
        };
        let mut rows = rows.iter().copied();
        let mut found = false;
        for (i, row) in rows.by_ref() {
            let row = row.trim();
            if (precise && row == "Coord Atom Element:")
                || (!precise && row.split_whitespace().take(2).eq(["Atom", "AN"]))
            {
                found = true;
                break;
            }
            if let Some((label, values)) = row.split_once("--") {
                let mass = matches!(label.trim(), "Red. masses" | "Reduced masses");
                let infrared = matches!(label.trim(), "IR Inten" | "IR Intensities");
                if mass || infrared {
                    let values = values.trim_start_matches('-').split_whitespace();
                    if values.clone().take(block.len() + 1).count() != block.len() {
                        if partial_panel && i == last_line {
                            break 'panels;
                        }
                        return Err(Error::at(
                            i,
                            "Gaussian mode property count does not match the frequencies",
                        ));
                    }
                    for (mode, value) in block.iter_mut().zip(values) {
                        let value = match number(
                            value,
                            i,
                            if mass { "reduced mass" } else { "IR intensity" },
                        ) {
                            Ok(value) => value,
                            Err(_) if partial_panel && i == last_line => break 'panels,
                            Err(error) => return Err(error),
                        };
                        if mass {
                            mode.mass = Some(value)
                        } else {
                            mode.intensity = Some(value)
                        }
                    }
                }
            }
        }
        if !found
            || !displacements(
                rows,
                frame,
                &mut block,
                precise,
                partial_panel.then_some(last_line),
            )?
        {
            if partial_panel {
                break;
            }
            return Err(Error::at(
                line,
                "incomplete Gaussian normal mode displacement table",
            ));
        }
        for mode in &mut block {
            let norm = mode.displacement.iter().map(|v| v * v).sum::<f64>().sqrt();
            if norm == 0.0 || !norm.is_finite() {
                return Err(Error::at(line, "invalid Gaussian normal mode norm"));
            }
            for component in &mut mode.displacement {
                *component /= norm;
            }
        }
        modes.extend(block);
    }
    Ok((modes, high_precision))
}

fn displacements<'a>(
    rows: impl Iterator<Item = (usize, &'a str)>,
    frame: &Frame,
    block: &mut [Mode],
    precise: bool,
    partial_line: Option<usize>,
) -> Result<bool, Error> {
    let (axes, columns) = if precise { (1, 3) } else { (3, 2) };
    let mut at = 0;
    for (i, row) in rows {
        let last_partial = Some(i) == partial_line;
        let mut data = row.split_whitespace();
        let first = data.next();
        let Some(z) = data.nth(columns - 2).and_then(|z| z.parse::<i16>().ok()) else {
            if last_partial {
                return Ok(false);
            }
            return Err(Error::at(
                i,
                "invalid Gaussian normal mode displacement row",
            ));
        };
        if z == -1 {
            continue;
        }
        let width = block.len() * axes;
        if data.clone().take(width + 1).count() != width {
            if last_partial {
                return Ok(false);
            }
            return Err(Error::at(
                i,
                "invalid Gaussian normal mode displacement count",
            ));
        }
        if Some(z) != frame.numbers.get(at / 3).map(|z| i16::from(*z))
            || precise && first.and_then(|value| value.parse::<usize>().ok()) != Some(at % 3 + 1)
        {
            return Err(Error::at(
                i,
                "Gaussian normal mode atom order does not match the geometry",
            ));
        }
        for mode in block.iter_mut() {
            for value in data.by_ref().take(axes) {
                match number(value, i, "normal mode displacement") {
                    Ok(value) => mode.displacement.push(value),
                    Err(_) if last_partial => return Ok(false),
                    Err(error) => return Err(error),
                }
            }
        }
        at += axes;
        if at == frame.numbers.len() * 3 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "tests/gaussian.rs"]
mod tests;
