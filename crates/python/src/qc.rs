//! Quantum chemistry files and the shared, full-precision Python result shape.

use pyo3::prelude::*;
use reshiki_io::{qc_input, qcread};
use reshiki_qcdata::{
    Beta, CalcResult, DEBYE_PER_E_BOHR, Frame, Orbitals, Provenance, Settings, Source, Status,
    Task, Thermo, Vibrations,
};
use reshiki_xtb_worker::protocol::{Computed, HARTREE_TO_EV, NEAR_DEGENERATE_GAP_EV};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GeometryData {
    pub numbers: Vec<u8>,
    pub positions: Vec<[f64; 3]>,
    pub charge: i32,
    pub multiplicity: u32,
    #[serde(default)]
    pub title: String,
}

impl GeometryData {
    pub fn to_frame(&self) -> Frame {
        Frame {
            numbers: self.numbers.clone(),
            positions_angstrom: self.positions.clone(),
            isotopes: Vec::new(),
            masses_amu: Vec::new(),
            charge: self.charge,
            multiplicity: self.multiplicity,
            drawing_atoms: Vec::new(),
        }
    }

    pub fn from_frame(frame: &Frame, title: &str) -> Self {
        Self {
            numbers: frame.numbers.clone(),
            positions: frame.positions_angstrom.clone(),
            charge: frame.charge,
            multiplicity: frame.multiplicity,
            title: title.into(),
        }
    }
}

fn nonempty(values: &[f64]) -> Option<&[f64]> {
    (!values.is_empty()).then_some(values)
}

fn thermo_data(thermo: Option<&Thermo>, energy: Option<f64>) -> Value {
    thermo.map_or(Value::Null, |t| {
        json!({
            "temperature": t.temperature_k,
            "pressure": t.pressure_pa,
            "zero_point": t.zero_point_energy_hartree,
            "enthalpy": energy.map(|e| e + t.enthalpy_correction_hartree),
            "entropy": t.entropy_hartree_per_k,
            "free_energy": energy.map(|e| e + t.gibbs_correction_hartree),
        })
    })
}

fn base_data(geometry: &GeometryData, method: &str, program: Option<&str>) -> Value {
    json!({
        "geometry": geometry, "method": method, "program": program,
        "energy": null, "gradient": null, "hessian": null, "charges": null,
        "dipole": null, "bond_orders": null, "orbital_energies": null,
        "occupations": null, "beta_orbital_energies": null, "beta_occupations": null,
        "homo_lumo_gap": null, "near_degenerate": false, "frequencies": null,
        "ir_intensities": null, "normal_modes": null, "thermo": null, "warnings": [],
    })
}

fn orbital_data(data: &mut Value, orbitals: Option<&Orbitals>) {
    let Some(orbitals) = orbitals else { return };
    data["orbital_energies"] = json!(nonempty(&orbitals.alpha.energies_hartree));
    data["occupations"] = json!(nonempty(&orbitals.alpha.occupations));
    match &orbitals.beta {
        Beta::Restricted => {}
        Beta::SameOrbitals { occupations } => {
            data["beta_orbital_energies"] = json!(nonempty(&orbitals.alpha.energies_hartree));
            data["beta_occupations"] = json!(nonempty(occupations));
        }
        Beta::Unrestricted(beta) => {
            data["beta_orbital_energies"] = json!(nonempty(&beta.energies_hartree));
            data["beta_occupations"] = json!(nonempty(&beta.occupations));
        }
    }
}

fn orbital_gap(orbitals: &Orbitals) -> Option<f64> {
    let mut homo: Option<f64> = None;
    let mut lumo: Option<f64> = None;
    let mut channel = |energies: &[f64], occupations: &[f64]| {
        for (&energy, &occupation) in energies.iter().zip(occupations) {
            if occupation > 0.0 {
                homo = Some(homo.map_or(energy, |previous| previous.max(energy)));
            } else {
                lumo = Some(lumo.map_or(energy, |previous| previous.min(energy)));
            }
        }
    };
    channel(
        &orbitals.alpha.energies_hartree,
        &orbitals.alpha.occupations,
    );
    match &orbitals.beta {
        Beta::Restricted => {}
        Beta::SameOrbitals { occupations } => {
            channel(&orbitals.alpha.energies_hartree, occupations);
        }
        Beta::Unrestricted(beta) => channel(&beta.energies_hartree, &beta.occupations),
    }
    Some((lumo? - homo?) * HARTREE_TO_EV)
}

pub(crate) fn result_data(result: &CalcResult, frame: &Frame) -> Value {
    let method = match (&result.provenance.method, &result.provenance.basis) {
        (Some(method), Some(basis)) => format!("{method}/{basis}"),
        (Some(method), None) => method.clone(),
        (None, Some(basis)) => basis.clone(),
        (None, None) => String::new(),
    };
    let program = match &result.provenance.program_version {
        Some(version) => format!("{} {version}", result.provenance.program),
        None => result.provenance.program.clone(),
    };
    let mut data = base_data(
        &GeometryData::from_frame(frame, ""),
        &method,
        (!program.is_empty()).then_some(program.as_str()),
    );
    data["energy"] = json!(result.energy_hartree);
    data["gradient"] = json!(nonempty(&result.gradient_hartree_per_bohr));
    data["hessian"] = json!(nonempty(&result.hessian_hartree_per_bohr2));
    data["charges"] = json!(result.charges.first().map(|set| &set.values));
    data["dipole"] = json!(
        result
            .dipole_e_bohr
            .map(|d| d.map(|v| v * DEBYE_PER_E_BOHR))
    );
    if !result.bond_orders.is_empty() {
        data["bond_orders"] = json!(
            result
                .bond_orders
                .iter()
                .map(|b| (b.a, b.b, b.order))
                .collect::<Vec<_>>()
        );
    }
    orbital_data(&mut data, result.orbitals.as_ref());
    let gap = result.orbitals.as_ref().and_then(orbital_gap);
    data["homo_lumo_gap"] = json!(gap);
    data["near_degenerate"] = json!(gap.is_some_and(|gap| gap < NEAR_DEGENERATE_GAP_EV));
    if let Some(vibrations) = &result.vibrations {
        data["frequencies"] = json!(nonempty(&vibrations.frequencies_cm1));
        data["ir_intensities"] = json!(nonempty(&vibrations.ir_intensities_km_mol));
        data["normal_modes"] = json!(nonempty(&vibrations.modes));
    }
    data["thermo"] = thermo_data(result.thermo.as_ref(), result.energy_hartree);
    if let Status::NotConverged { reason } | Status::Failed { reason } = &result.status {
        data["warnings"] = json!([reason]);
    }
    data
}

pub(crate) fn computed_data(
    result: &Computed,
    geometry: &GeometryData,
    method: &str,
    program: &str,
) -> Value {
    let mut data = base_data(geometry, method, Some(program));
    data["energy"] = json!(result.energy);
    data["gradient"] = json!(result.gradient);
    data["hessian"] = json!(nonempty(&result.hessian));
    data["charges"] = json!(result.charges.as_ref().map(|set| &set.values));
    data["dipole"] = json!(
        result
            .dipole_e_bohr
            .map(|d| d.map(|v| v * DEBYE_PER_E_BOHR))
    );
    data["bond_orders"] = json!(result.bond_orders.as_ref().map(|orders| {
        orders
            .iter()
            .map(|b| (b.a, b.b, b.order))
            .collect::<Vec<_>>()
    }));
    orbital_data(&mut data, result.orbitals.as_ref());
    data["homo_lumo_gap"] = json!(result.homo_lumo_gap_ev);
    data["near_degenerate"] = json!(result.near_degenerate);
    if let Some(vibrations) = &result.vibrations {
        data["frequencies"] = json!(vibrations.frequencies_cm1);
        data["ir_intensities"] = json!(vibrations.ir_intensities_km_mol);
        data["normal_modes"] = if vibrations.frequencies_cm1.is_empty() {
            json!([])
        } else {
            json!(nonempty(&result.normal_modes))
        };
    }
    data["thermo"] = thermo_data(result.thermo.as_ref(), Some(result.energy));
    data
}

fn file_data(file: &qcread::QcFile) -> Value {
    let jobs: Vec<_> = file.jobs.iter().map(|job| {
        let (completion, message) = match &job.completion {
            None => (None, None),
            Some(qcread::Completion::Normal) => (Some("normal"), None),
            Some(qcread::Completion::Truncated) => (Some("truncated"), None),
            Some(qcread::Completion::Error { message }) => (Some("error"), Some(message)),
        };
        let kind = match job.kind {
            qcread::JobKind::Geometry => "geometry",
            qcread::JobKind::SinglePoint => "single-point",
            qcread::JobKind::Optimization => "optimization",
            qcread::JobKind::Frequencies => "frequencies",
            qcread::JobKind::Scan => "scan",
            qcread::JobKind::Other => "other",
        };
        let steps: Vec<_> = job.steps.iter().map(|step| json!({
            "geometry": GeometryData::from_frame(&step.frame, step.label.as_deref().unwrap_or("")),
            "energy": step.energy_hartree, "label": step.label,
        })).collect();
        json!({
            "index": job.index, "kind": kind, "method": job.method, "basis": job.basis,
            "keywords": job.keywords, "completion": completion, "completion_message": message,
            "steps": steps,
            "result": job.result.as_ref().zip(job.result_step()).map(|(result, step)| result_data(result, &step.frame)),
        })
    }).collect();
    json!({
        "format": file.format.name(), "program": file.program,
        "program_version": file.program_version, "jobs": jobs,
        "default_job": file.default_job(), "warnings": file.warnings,
    })
}

#[pyfunction]
pub(crate) fn qc_read(py: Python<'_>, name: &str, data: Vec<u8>) -> PyResult<String> {
    py.detach(|| {
        let file = qcread::read(name, &data).map_err(|e| e.to_string())?;
        serde_json::to_string(&file_data(&file)).map_err(|e| e.to_string())
    })
    .map_err(crate::failed(py))
}

#[pyfunction]
pub(crate) fn qc_elements() -> String {
    json!(
        reshiki_model::chemistry::ELEMENTS
            .iter()
            .map(|e| e.symbol)
            .collect::<Vec<_>>()
    )
    .to_string()
}

#[derive(Deserialize)]
struct HessianData {
    geometry: GeometryData,
    hessian: Option<Vec<f64>>,
    method: String,
    frequencies: Option<Vec<f64>>,
}

impl HessianData {
    fn result(&self) -> CalcResult {
        CalcResult {
            frame: self.geometry.to_frame().hash(),
            input_frame: None,
            task: Task::Hessian,
            endpoints: None,
            settings: Settings::Imported {
                keywords: Vec::new(),
            },
            provenance: Provenance {
                program: String::new(),
                program_version: None,
                method: Some(self.method.clone()),
                basis: None,
                source: Source::Imported {
                    format: "Python".into(),
                    file_name: String::new(),
                    file_sha256: None,
                    job: 0,
                },
            },
            status: Status::Converged,
            energy_hartree: None,
            energy_terms: Default::default(),
            gradient_hartree_per_bohr: Vec::new(),
            hessian_hartree_per_bohr2: self.hessian.clone().unwrap_or_default(),
            charges: Vec::new(),
            dipole_e_bohr: None,
            bond_orders: Vec::new(),
            orbitals: None,
            vibrations: self.frequencies.as_ref().map(|frequencies| Vibrations {
                frequencies_cm1: frequencies.clone(),
                ir_intensities_km_mol: Vec::new(),
                modes: Vec::new(),
                reduced_masses_amu: Vec::new(),
                masses_amu: Vec::new(),
            }),
            thermo: None,
            trajectory: None,
            keep_full: false,
            omitted_bytes: None,
        }
    }
}

#[derive(Deserialize)]
struct InputOptions {
    geometry: GeometryData,
    program: qc_input::Program,
    job: String,
    method: String,
    basis: String,
    cores: u32,
    memory_gb: u32,
    title: String,
    extra: String,
    stem: String,
    hessian: Option<HessianData>,
    reactant: Option<GeometryData>,
    product: Option<GeometryData>,
}

fn input_data(options_json: &str) -> Result<String, String> {
    let options: InputOptions = serde_json::from_str(options_json).map_err(|e| e.to_string())?;
    let job = qc_input::Job::parse(&options.job)
        .ok_or_else(|| format!("Unknown calculation job: '{}'", options.job))?;
    let frame = options.geometry.to_frame();
    let reactant = options.reactant.map(|g| g.to_frame());
    let product = options.product.map(|g| g.to_frame());
    let result = options.hessian.map(|h| h.result());
    let settings = qc_input::Settings {
        program: options.program,
        job,
        method: options.method,
        basis: options.basis,
        charge: frame.charge,
        multiplicity: frame.multiplicity,
        cores: options.cores,
        memory_gb: options.memory_gb,
        title: options.title,
        extra: options.extra,
        irc_points: 30,
        neb_images: 12,
    };
    let output = qc_input::write(
        &settings,
        &qc_input::Geometry {
            frame: &frame,
            reactant: reactant.as_ref(),
            product: product.as_ref(),
            result: result.as_ref(),
        },
        &options.stem,
        concat!("ReShiki ", env!("CARGO_PKG_VERSION")),
    )
    .map_err(|e| e.to_string())?;
    let files: Vec<_> = std::iter::once(output.main)
        .chain(output.companions)
        .map(|file| json!({ "name": file.name, "text": file.text }))
        .collect();
    serde_json::to_string(&json!({ "files": files, "warnings": output.warnings }))
        .map_err(|e| e.to_string())
}

#[pyfunction]
pub(crate) fn qc_write_input(py: Python<'_>, options_json: &str) -> PyResult<String> {
    py.detach(|| input_data(options_json))
        .map_err(crate::invalid(py))
}
