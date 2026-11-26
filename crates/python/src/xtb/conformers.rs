//! Adapt a Python molecule to the shared conformer search.

use super::{WorkerSettings, failure_message, optimization_data, run_interruptibly};
use crate::qc::GeometryData;
use pyo3::prelude::*;
use reshiki_io::geometry;
use reshiki_model::{document::Document, molecule_names, science};
use reshiki_xtb_jobs::{
    Failure, XTB_NEAR_DEGENERATE,
    conformers::{self, Config, Diversity, KCAL_PER_HARTREE, Minimizer, Search},
    scf,
};
use reshiki_xtb_worker::{client::Cancel, protocol::Method};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Request {
    starts: u32,
    minimizer: String,
    diversity: String,
    window: f64,
    representatives: usize,
    per_cluster: u8,
    charge: Option<i32>,
    #[serde(flatten)]
    settings: WorkerSettings,
}

enum SearchError {
    Invalid(String),
    Failed(String),
}

impl From<Failure> for SearchError {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure_message(&failure))
    }
}

#[pyfunction]
pub fn xtb_conformers(
    py: Python<'_>,
    executable: PathBuf,
    document_json: Vec<u8>,
    request_json: &str,
) -> PyResult<String> {
    let request: Request = serde_json::from_str(request_json)
        .map_err(|error| crate::invalid(py)(format!("Invalid conformer settings: {error}")))?;
    let minimizer = match request.minimizer.as_str() {
        "mmff94s" => Minimizer::Mmff94s,
        "mmff94" => Minimizer::Mmff94,
        "uff" => Minimizer::Uff,
        "gfnff" => Minimizer::GfnFf,
        "gfn2" => Minimizer::Gfn2,
        _ => {
            return Err(crate::invalid(py)(
                "Minimizer must be mmff94s, mmff94, uff, gfnff or gfn2".into(),
            ));
        }
    };
    let diversity = Diversity::ALL
        .into_iter()
        .find(|diversity| diversity.label() == request.diversity)
        .ok_or_else(|| crate::invalid(py)("Diversity must be fine, normal or coarse".into()))?;
    let config = Config::new(
        request.starts,
        minimizer,
        diversity,
        request.window,
        request.representatives,
        request.per_cluster,
    )
    .map_err(crate::invalid(py))?;
    let method = request.settings.method().map_err(crate::invalid(py))?;
    let method = match minimizer {
        Minimizer::GfnFf => Method::Gfnff,
        Minimizer::Gfn2 => Method::Gfn2,
        _ => method,
    };
    if request.settings.threads > 64 {
        return Err(crate::invalid(py)("Threads must be at most 64".into()));
    }
    if method != Method::Gfnff {
        let invalid = |message: &str| crate::invalid(py)(message.into());
        scf::check(request.settings.accuracy, scf::ACCURACY).map_err(invalid)?;
        scf::check(request.settings.electronic_temperature, scf::TEMPERATURE).map_err(invalid)?;
        scf::check(request.settings.max_iterations, scf::ITERATIONS).map_err(invalid)?;
    }

    run_interruptibly(py, move |cancel| {
        let doc = Document::from_native_file(&document_json).map_err(SearchError::Invalid)?;
        request.run(doc, config, method, executable, cancel)
    })?
    .map_err(|error| match error {
        SearchError::Invalid(message) => crate::invalid(py)(message),
        SearchError::Failed(message) => crate::failed(py)(message),
    })
}

impl Request {
    fn run(
        self,
        mut doc: Document,
        config: Config,
        method: Method,
        executable: PathBuf,
        cancel: &Cancel,
    ) -> Result<String, SearchError> {
        let molecule_required =
            || SearchError::Invalid("Conformer search needs one molecule".into());
        let atom = doc.atoms.first().ok_or_else(molecule_required)?.id;
        let ids = molecule_names::component(&doc, atom);
        if ids.len() != doc.atoms.len() {
            return Err(molecule_required());
        }
        let (charge, multiplicity) = science::drawn_state(&doc, atom);
        let charge = self.charge.unwrap_or(charge);
        let uhf = self.settings.unpaired.unwrap_or(multiplicity - 1);
        let multiplicity = uhf.saturating_add(1);
        if method == Method::Gfnff && uhf != 0 {
            return Err(SearchError::Invalid(
                "GFN-FF calculates closed shells only: set unpaired=0".into(),
            ));
        }
        let client = geometry::Client::new(executable.clone(), geometry::Limits::default())
            .map_err(|error| SearchError::Failed(error.to_string()))?;
        let launch = match science::launch(&doc, atom, charge, multiplicity) {
            Err(science::ScienceError::NoModel) => {
                config
                    .check_atoms(ids.len())
                    .map_err(|failure| SearchError::Invalid(failure.to_string()))?;
                let model = conformers::generate_model(&doc, &ids, &client, cancel)?;
                doc.models.push(model);
                reshiki_model::models3d::require_version(&mut doc);
                science::launch(&doc, atom, charge, multiplicity)
            }
            other => other,
        }
        .map_err(|error| SearchError::Invalid(error.to_string()))?;
        config
            .check_atoms(launch.frame.numbers.len())
            .map_err(|failure| SearchError::Invalid(failure.to_string()))?;
        let geometry = GeometryData::from_frame(&launch.frame, "");
        let mut job = self.settings.job(&geometry, uhf, method);
        job.threads = reshiki_xtb_jobs::threads(job.threads);
        if method == Method::Gfnff {
            job.scf = Default::default();
            job.solvent = None;
        }

        let method_line = config.describe(method.xtb_method());
        let force_field = config.minimizer.xtb().is_none();
        let search = Search::from_launch(&doc, &ids, &launch, config, job, client, executable)?;
        let found = conformers::search_conformers(search, cancel, |_| {})?;
        let path = found.trajectory(&launch.frame)?;
        let (_, ranked) =
            conformers::ranked_trajectory(Some(&path)).map_err(SearchError::Failed)?;
        let start_minimum = path
            .minimizer_energies_hartree
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let rows: Vec<_> = ranked
            .into_iter()
            .enumerate()
            .map(|(index, conformer)| {
                let entry = &found.ranked[conformer.step];
                let stereo = science::stereo_changes(&doc, &launch.atoms, &conformer.frame)
                    .ok()
                    .map(|changes| changes.is_empty());
                json!({
                    "rank": index + 1,
                    "geometry": GeometryData::from_frame(&conformer.frame, ""),
                    "energy": conformer.energy,
                    "delta": conformer.delta_kcal,
                    "start_delta": force_field.then(|| {
                        (entry.start_energy_hartree - start_minimum) * KCAL_PER_HARTREE
                    }),
                    "population": conformer.percent / 100.0,
                    "rmsd": entry.rmsd_to_best,
                    "cluster_size": entry.cluster_size,
                    "stereo": stereo,
                })
            })
            .collect();
        let mut notes = found.notes;
        if found.best.computed.near_degenerate {
            notes.push(XTB_NEAR_DEGENERATE.into());
        }
        let mut best_geometry = geometry;
        best_geometry.positions.clone_from(&found.best.positions);
        let xyz = conformers::xyz(Some(&path), "Molecule").map_err(SearchError::Failed)?;
        Ok(json!({
            "conformers": rows,
            "best": optimization_data(&found.best, &best_geometry),
            "starts": found.starts,
            "clusters": found.clusters,
            "reranked": found.reranked,
            "planned": found.planned,
            "notes": notes,
            "method": method_line,
            "xyz": xyz,
        })
        .to_string())
    }
}
