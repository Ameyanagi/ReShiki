//! Quantum chemistry files as compact job summaries and calculated values.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    files::{MAX_PATH_BYTES, check_length},
    result::ToolResult,
    wire::warnings,
};
use crate::{
    access::Grants,
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
};
use reshiki_io::{
    qcdata::{Beta, DEBYE_PER_E_BOHR, Orbitals},
    qcread::{self, Completion, Job, JobKind, JobSummary, QcFile, Step},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fmt::Write as _, path::Path, sync::Arc};

const EV_PER_HARTREE: f64 = 27.211_386_245_988;

pub const READ_OUTPUT: ToolSpec = ToolSpec {
    name: "read_output",
    title: Some("Read calculation output"),
    description: "Summarize quantum chemistry jobs with calculated values and XYZ geometry.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": ["string", "null"], "description": "Inline file, at most max_text_bytes; needs file_name. Give text or path, never both."},
            "file_name": {"type": ["string", "null"], "description": "Format detection name; null uses path's file name."},
            "path": {"type": ["string", "null"], "maxLength": MAX_PATH_BYTES, "description": "File in a granted READ folder, at most max_text_bytes."},
            "job": {"type": ["integer", "null"], "minimum": 0, "description": "Zero-based job index; null selects the default job."},
        },
        "required": ["text", "file_name", "path", "job"],
        "additionalProperties": false,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    job: Option<usize>,
}

#[derive(Debug)]
enum Source {
    Text(String),
    Path(String),
}

/// A bounded inline file or a granted file, with its detection name resolved.
#[derive(Debug)]
pub(crate) struct ReadOutput {
    source: Source,
    file_name: String,
    job: Option<usize>,
}

/// Resolve the exclusive sources and bound inline bytes before parsing.
pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<ReadOutput, OpError> {
    let Args {
        text,
        file_name,
        path,
        job,
    } = arguments(args)?;
    let (source, file_name) = match (text, path) {
        (Some(text), None) => {
            let file_name = file_name.ok_or_else(|| {
                OpError::invalid("file_name is required with text").param("file_name")
            })?;
            if text.len() > budgets.max_text_bytes {
                return Err(OpError::new(
                    ErrorKind::Budget,
                    format!(
                        "The text is {} bytes; max_text_bytes allows at most {}",
                        text.len(),
                        budgets.max_text_bytes
                    ),
                )
                .param("text"));
            }
            (Source::Text(text), file_name)
        }
        (None, Some(path)) => {
            check_length(&path)?;
            let file_name = file_name.unwrap_or_else(|| {
                Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_owned()
            });
            (Source::Path(path), file_name)
        }
        _ => return Err(OpError::invalid("Give exactly one of text or path")),
    };
    Ok(ReadOutput {
        source,
        file_name,
        job,
    })
}

fn round(value: f64) -> f64 {
    (value * 1e9).round() / 1e9 + 0.0
}

fn kind(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Geometry => "geometry",
        JobKind::SinglePoint => "single_point",
        JobKind::Optimization => "optimization",
        JobKind::Frequencies => "frequencies",
        JobKind::Scan => "scan",
        JobKind::Other => "other",
    }
}

fn completion(completion: Option<&Completion>) -> Option<&'static str> {
    completion.map(|completion| match completion {
        Completion::Normal => "normal",
        Completion::Error { .. } => "error",
        Completion::Truncated => "truncated",
    })
}

#[derive(Serialize)]
struct Summary {
    index: u32,
    kind: &'static str,
    title: String,
    completion: Option<&'static str>,
    steps: usize,
    atoms: usize,
    energy_hartree: Option<f64>,
    frequencies: usize,
    orbitals: usize,
}

impl From<JobSummary> for Summary {
    fn from(summary: JobSummary) -> Self {
        Self {
            index: summary.index,
            kind: kind(summary.kind),
            title: summary.title,
            completion: completion(summary.completion.as_ref()),
            steps: summary.steps,
            atoms: summary.atoms,
            energy_hartree: summary.energy_hartree.map(round),
            frequencies: summary.frequencies,
            orbitals: summary.orbitals,
        }
    }
}

#[derive(Serialize)]
struct Thermo {
    temperature_k: f64,
    zero_point_energy_hartree: f64,
    enthalpy_hartree: f64,
    gibbs_hartree: f64,
}

#[derive(Serialize)]
struct Selected {
    index: u32,
    kind: &'static str,
    method: Option<String>,
    basis: Option<String>,
    keywords: Vec<String>,
    completion: Option<&'static str>,
    energy_hartree: Option<f64>,
    homo_lumo_gap_ev: Option<f64>,
    dipole_debye: Option<f64>,
    frequencies_cm1: Vec<f64>,
    ir_intensities_km_mol: Vec<f64>,
    imaginary: usize,
    thermo: Option<Thermo>,
    xyz: Option<String>,
}

/// The frontier across the available spin spectra with occupations.
fn gap(orbitals: &Orbitals) -> Option<f64> {
    let beta = match &orbitals.beta {
        Beta::Unrestricted(beta) => Some(beta),
        Beta::Restricted => None,
        Beta::SameOrbitals { .. } => return None,
    };
    // A spectrum without occupations leaves the combined frontier unknown.
    if std::iter::once(&orbitals.alpha)
        .chain(beta)
        .any(|set| set.occupations.len() != set.energies_hartree.len())
    {
        return None;
    }
    let levels = || {
        std::iter::once(&orbitals.alpha)
            .chain(beta)
            .flat_map(|set| set.energies_hartree.iter().zip(&set.occupations))
    };
    let homo = levels()
        .filter(|(_, occupation)| **occupation > 0.0)
        .map(|(&energy, _)| energy)
        .max_by(f64::total_cmp)?;
    let lumo = levels()
        .filter(|(_, occupation)| **occupation <= 0.0)
        .map(|(&energy, _)| energy)
        .min_by(f64::total_cmp)?;
    Some(round((lumo - homo) * EV_PER_HARTREE))
}

fn xyz(step: &Step, title: &str) -> Option<String> {
    let frame = &step.frame;
    let mut text = format!("{}\n{title}\n", frame.numbers.len());
    for (&number, [x, y, z]) in frame.numbers.iter().zip(&frame.positions_angstrom) {
        let symbol = crate::chemistry::ELEMENTS.get(usize::from(number))?.symbol;
        writeln!(&mut text, "{symbol} {x:.8} {y:.8} {z:.8}").ok()?;
    }
    Some(text)
}

impl From<&Job> for Selected {
    fn from(job: &Job) -> Self {
        let result = job.result.as_ref();
        let step = if result.is_some() {
            job.result_step()
        } else {
            job.steps.last()
        };
        let energy = result
            .and_then(|result| result.energy_hartree)
            .or_else(|| step.and_then(|step| step.energy_hartree));
        let vibrations = result.and_then(|result| result.vibrations.as_ref());
        let thermo = result
            .and_then(|result| result.thermo.as_ref())
            .zip(energy)
            .map(|(thermo, energy)| Thermo {
                temperature_k: round(thermo.temperature_k),
                zero_point_energy_hartree: round(thermo.zero_point_energy_hartree),
                enthalpy_hartree: round(energy + thermo.enthalpy_correction_hartree),
                gibbs_hartree: round(energy + thermo.gibbs_correction_hartree),
            });
        Self {
            index: job.index,
            kind: kind(job.kind),
            method: job.method.clone(),
            basis: job.basis.clone(),
            keywords: job.keywords.clone(),
            completion: completion(job.completion.as_ref()),
            energy_hartree: energy.map(round),
            homo_lumo_gap_ev: result
                .and_then(|result| result.orbitals.as_ref())
                .and_then(gap),
            dipole_debye: result
                .and_then(|result| result.dipole_e_bohr)
                .map(|[x, y, z]| round(x.hypot(y).hypot(z) * DEBYE_PER_E_BOHR)),
            frequencies_cm1: vibrations.map_or_else(Vec::new, |vibrations| {
                vibrations
                    .frequencies_cm1
                    .iter()
                    .copied()
                    .map(round)
                    .collect()
            }),
            ir_intensities_km_mol: vibrations.map_or_else(Vec::new, |vibrations| {
                vibrations
                    .ir_intensities_km_mol
                    .iter()
                    .copied()
                    .map(round)
                    .collect()
            }),
            imaginary: vibrations.map_or(0, |vibrations| vibrations.imaginary_count()),
            thermo,
            xyz: step.and_then(|step| xyz(step, &job.title())),
        }
    }
}

#[derive(Serialize)]
struct Read {
    format: &'static str,
    program: Option<String>,
    program_version: Option<String>,
    jobs: Vec<Summary>,
    job: Option<Selected>,
}

impl Read {
    fn from_file(file: &QcFile, requested: Option<usize>) -> Result<Self, OpError> {
        let selected = match requested {
            Some(index) => Some(file.jobs.get(index).ok_or_else(|| {
                OpError::invalid(format!(
                    "job {index} is out of range; the file has {} jobs",
                    file.jobs.len()
                ))
                .param("job")
            })?),
            None => file.default_job().and_then(|index| file.jobs.get(index)),
        };
        Ok(Self {
            format: file.format.name(),
            program: file.program.clone(),
            program_version: file.program_version.clone(),
            jobs: file.summaries().into_iter().map(Summary::from).collect(),
            job: selected.map(Selected::from),
        })
    }
}

/// Read and parse on the blocking pool; file warnings retain their order.
pub(crate) async fn read_output(
    ctx: Context,
    grants: Arc<Grants>,
    versions: Versions,
    budgets: Budgets,
    args: ReadOutput,
) -> Result<ToolResult, OpError> {
    let (max_text_bytes, max_output_bytes) = (budgets.max_text_bytes, budgets.max_output_bytes);
    let (value, messages) = ctx
        .blocking(move || {
            let bytes = match args.source {
                Source::Text(text) => text.into_bytes(),
                Source::Path(path) => grants.read_qc(&path, max_text_bytes).map_err(|error| {
                    let mut error = OpError::from(error);
                    if error.reason == Some("file_too_large") {
                        error.message = format!(
                            "file_too_large: the file exceeds max_text_bytes ({max_text_bytes} bytes)"
                        );
                    }
                    error
                })?,
            };
            let file = qcread::read(&args.file_name, &bytes).map_err(|error| match error {
                qcread::Error::Invalid(message) => OpError::invalid(message),
                qcread::Error::Unsupported(message) => {
                    OpError::new(ErrorKind::Unsupported, message)
                }
            })?;
            let value = Read::from_file(&file, args.job)?;
            let bytes = serde_json::to_vec(&value).map_or(usize::MAX, |json| json.len());
            if bytes > max_output_bytes {
                return Err(OpError::new(
                    ErrorKind::Budget,
                    format!(
                        "The summary is more than {max_output_bytes} bytes; max_output_bytes allows at most {max_output_bytes}. Choose one job with job"
                    ),
                ));
            }
            Ok((value, file.warnings))
        })
        .await?;
    Ok(ToolResult::valid(value, warnings(messages), versions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki_io::qcdata::OrbitalSet;

    #[test]
    fn geometry_uses_the_last_step_and_has_no_calculated_values() {
        let file = qcread::read(
            "trajectory.xyz",
            b"1\nfirst\nH 0 0 0\n1\nlast\nH 1.234567891 2 3\n",
        )
        .unwrap();
        let value = Read::from_file(&file, None).unwrap();
        assert_eq!(value.format, "XYZ");
        assert_eq!(value.jobs[0].steps, 2);
        assert_eq!(value.jobs[0].atoms, 1);
        let job = value.job.unwrap();
        assert_eq!(job.kind, "geometry");
        assert_eq!(job.completion, None);
        assert_eq!(job.energy_hartree, None);
        assert_eq!(job.homo_lumo_gap_ev, None);
        assert_eq!(job.dipole_debye, None);
        assert!(job.frequencies_cm1.is_empty());
        assert!(job.ir_intensities_km_mol.is_empty());
        assert_eq!(job.imaginary, 0);
        assert!(job.thermo.is_none());
        assert_eq!(
            job.xyz.as_deref(),
            Some("1\nGeometry\nH 1.23456789 2.00000000 3.00000000\n")
        );
    }

    #[test]
    fn real_gaussian_values_and_thermochemical_corrections() {
        let mut file = qcread::read(
            "H2O_gau_vib.out",
            include_bytes!("../../../../tests/fixtures/qc/pymatgen/H2O_gau_vib.out"),
        )
        .unwrap();
        let index = file.default_job().unwrap();
        let job = &mut file.jobs[index];
        assert!(Selected::from(&*job).thermo.is_none());
        let result = job.result.as_mut().unwrap();
        // Readers currently leave thermochemistry absent. Supply the corrections
        // printed by this fixture to exercise the result conversion.
        let original = reshiki_io::qcdata::Thermo {
            temperature_k: 298.15,
            pressure_pa: 101_325.0,
            zero_point_energy_hartree: 0.021095,
            enthalpy_correction_hartree: 0.024874,
            entropy_hartree_per_k: (0.024874 - 0.002780) / 298.15,
            gibbs_correction_hartree: 0.002780,
            heat_capacity_hartree_per_k: None,
            frequency_scale: 1.0,
            rotor_cutoff_cm1: 0.0,
            imaginary_cutoff_cm1: 0.0,
            symmetry_number: 2,
        };
        result.thermo = Some(original.clone());
        let energy = result.energy_hartree.unwrap();
        let selected = Selected::from(&*job);
        let thermo = selected.thermo.unwrap();
        assert_eq!(selected.completion, Some("normal"));
        assert_eq!(selected.frequencies_cm1, [1662.8033, 3736.2607, 3860.5869]);
        assert_eq!(selected.ir_intensities_km_mol.len(), 3);
        assert_eq!(selected.imaginary, 0);
        assert_eq!(thermo.temperature_k, round(original.temperature_k));
        assert_eq!(
            thermo.zero_point_energy_hartree,
            round(original.zero_point_energy_hartree)
        );
        assert_eq!(
            thermo.enthalpy_hartree,
            round(energy + original.enthalpy_correction_hartree)
        );
        assert_eq!(
            thermo.gibbs_hartree,
            round(energy + original.gibbs_correction_hartree)
        );
        assert!(selected.homo_lumo_gap_ev.is_some_and(|gap| gap > 0.0));
        assert_eq!(selected.dipole_debye, Some(2.2494));
    }

    #[test]
    fn truncated_gaussian_pairs_xyz_with_the_last_evaluated_step() {
        let text = include_str!("../../../../tests/fixtures/qc/cclib/g16_dvb_gopt.out");
        let cut = text
            .find(" SCF Done:  E(RB3LYP) =  -382.308265702")
            .unwrap();
        let mut file = qcread::read("interrupted.log", &text.as_bytes()[..cut]).unwrap();
        let job = &mut file.jobs[0];
        assert_eq!(job.steps.len(), 4);
        let expected_xyz = xyz(&job.steps[2], &job.title()).unwrap();
        assert_ne!(expected_xyz, xyz(&job.steps[3], &job.title()).unwrap());
        let selected = Selected::from(&*job);
        assert_eq!(selected.completion, Some("truncated"));
        assert_eq!(selected.xyz.as_deref(), Some(expected_xyz.as_str()));
        assert_eq!(selected.energy_hartree, Some(-382.308239279));

        job.result.as_mut().unwrap().energy_hartree = None;
        assert_eq!(Selected::from(&*job).energy_hartree, Some(-382.308239279));
    }

    #[test]
    fn unrestricted_fchk_gap_uses_both_spins() {
        let file = qcread::read(
            "g09_dvb_un_sp.fchk",
            include_bytes!("../../../../tests/fixtures/qc/cclib/g09_dvb_un_sp.fchk"),
        )
        .unwrap();
        let selected = Read::from_file(&file, None).unwrap().job.unwrap();
        let combined = selected.homo_lumo_gap_ev.unwrap();
        assert!((combined - 1.88).abs() < 0.01, "gap was {combined} eV");
        // Without beta occupations the combined frontier is unknown, not alpha's.
        let mut orbitals = file.jobs[0]
            .result
            .as_ref()
            .unwrap()
            .orbitals
            .clone()
            .unwrap();
        if let Beta::Unrestricted(beta) = &mut orbitals.beta {
            beta.occupations.clear();
        }
        assert_eq!(gap(&orbitals), None);
    }

    #[test]
    fn selected_job_and_default_job_keep_their_own_completion() {
        let mut file = qcread::read("hydrogen.xyz", b"1\nH\nH 0 0 0\n").unwrap();
        file.jobs[0].completion = Some(Completion::Normal);
        let mut partial = file.jobs[0].clone();
        partial.index = 1;
        partial.completion = Some(Completion::Truncated);
        partial.steps[0].energy_hartree = Some(-0.123456789123);
        file.jobs.push(partial);
        let default = Read::from_file(&file, None).unwrap();
        assert_eq!(default.job.unwrap().index, 0);
        assert_eq!(default.jobs[1].completion, Some("truncated"));
        assert_eq!(default.jobs[1].energy_hartree, Some(-0.123456789));
        let selected = Read::from_file(&file, Some(1)).unwrap().job.unwrap();
        assert_eq!(selected.index, 1);
        assert_eq!(selected.completion, Some("truncated"));
        assert_eq!(selected.energy_hartree, Some(-0.123456789));
        let error = Read::from_file(&file, Some(2)).err().unwrap();
        assert_eq!(error.kind, ErrorKind::InvalidArguments);
        assert_eq!(error.param, Some("job"));
        assert_eq!(
            completion(Some(&Completion::Error {
                message: "SCF".into()
            })),
            Some("error")
        );
    }

    #[test]
    fn restricted_gap_uses_the_alpha_occupation_frontier_and_needs_both_levels() {
        let set = OrbitalSet {
            energies_hartree: vec![-1.0, -0.25, 0.1],
            occupations: vec![2.0, 2.0, 0.0],
            symmetries: Vec::new(),
            rows: Vec::new(),
            coefficients: Vec::new(),
        };
        let mut orbitals = Orbitals {
            basis: None,
            core_electrons: Vec::new(),
            alpha: set,
            beta: Beta::Restricted,
        };
        let expected = Some(round(0.35 * EV_PER_HARTREE));
        assert_eq!(gap(&orbitals), expected);
        orbitals.alpha.occupations.fill(2.0);
        assert_eq!(gap(&orbitals), None);
        orbitals.alpha.occupations.clear();
        assert_eq!(gap(&orbitals), None);
        orbitals.alpha.occupations = vec![1.0, 1.0, 0.0];
        orbitals.beta = Beta::SameOrbitals {
            occupations: vec![1.0, 0.0, 0.0],
        };
        assert_eq!(gap(&orbitals), None);
    }

    #[test]
    fn decoding_resolves_path_names_and_counts_text_bytes() {
        let path = decode(
            json!({"text": null, "file_name": null, "path": "/read/run.output", "job": null}),
            &Budgets::default(),
        )
        .unwrap();
        assert_eq!(path.file_name, "run.output");
        let budgets = Budgets {
            max_text_bytes: 3,
            ..Budgets::default()
        };
        let error = decode(
            json!({"text": "éé", "file_name": "run.xyz", "path": null, "job": null}),
            &budgets,
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Budget);
        assert_eq!(error.param, Some("text"));
    }
}
