//! Conformer sampling, clustering and xTB ranking.

use super::{Config, Diversity, Minimizer, Phase};
use crate::{Failure, method, run};
use reshiki_io::geometry::{self, Client, Conformer, Ensemble, ForceField, Prepared};
use reshiki_model::{
    document::Document,
    models3d::Model3d,
    science::{self, Launch},
};
use reshiki_optimize::{
    KCAL_PER_HARTREE,
    conformers::{self, Candidate, Clustering, PerCluster, Ranked, Settings, Start, Topology},
};
use reshiki_qcdata::{Frame, Trajectory};
use reshiki_xtb_worker::{
    client::Cancel,
    protocol::{Engine, Method},
};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc as sync_channel,
    },
    thread,
    time::{Duration, Instant},
};

const BASE_SEED: i32 = 0x52534b;
const CANCELED: &str = "Calculation canceled";
const FRAME_MISMATCH: &str =
    "The 3D model does not match the generated conformers. Update the 3D model and run again.";

pub struct Search {
    /// The drawing component as the geometry worker sees it.
    prepared: Prepared,
    /// Atomic numbers in frame order (the model's atoms, then added hydrogens).
    numbers: Vec<u8>,
    /// Drawing atom of each frame atom, or `None` for model hydrogens.
    atoms: Vec<Option<u64>>,
    /// Drawing atom each model hydrogen is bonded to, or `None` elsewhere.
    parents: Vec<Option<u64>>,
    /// Bonds in frame order, for clustering.
    bonds: Vec<[usize; 2]>,
    config: Config,
    geometry: Client,
    executable: Option<PathBuf>,
    /// Template for every xTB optimization. The pipeline replaces positions,
    /// enables optimization, clears fixed atoms and uses one thread per job.
    job: run::Job,
}

impl Search {
    /// Build the search in the atom order of the molecule's 3D model.
    /// With no explicit xTB executable, resolve the current executable at the first xTB job.
    pub fn from_launch(
        doc: &Document,
        ids: &[u64],
        launch: &Launch,
        config: Config,
        mut job: run::Job,
        geometry: Client,
        executable: Option<PathBuf>,
    ) -> Result<Self, Failure> {
        let prepared =
            Prepared::new(doc, ids).map_err(|error| Failure::Other(error.to_string()))?;
        let parents = science::hydrogen_parents(doc, launch).ok_or_else(|| {
            Failure::Other("The 3D model's hydrogens cannot be matched; update the 3D model".into())
        })?;
        let bonds = science::frame_bonds(doc, &launch.atoms, &launch.frame.positions_angstrom)
            .into_iter()
            .map(|(a, b, _)| [a, b])
            .collect();
        let selected = config.minimizer.effective_method(job.method.xtb_method());
        if !method::accepts_spin(selected, job.uhf) {
            return Err(Failure::Other(
                "GFN-FF calculates closed shells only".into(),
            ));
        }
        job.method = run::worker_method(selected);
        job.scf = method::scf_options(selected, || job.scf).unwrap_or_default();
        job.solvent = job.solvent.filter(|_| method::supports_solvent(selected));
        // GFN-FF keeps one topology, from the 3D model, for every start.
        job.topology =
            (job.method == Method::Gfnff).then(|| launch.frame.positions_angstrom.clone());
        Ok(Self {
            prepared,
            numbers: launch.frame.numbers.clone(),
            atoms: launch.atoms.clone(),
            parents,
            bonds,
            config,
            job,
            geometry,
            executable,
        })
    }

    /// The prepared drawing component used to sample conformer starts.
    pub fn prepared(&self) -> &Prepared {
        &self.prepared
    }
}

#[derive(Clone, Debug)]
pub struct Found {
    pub engine: Engine,
    /// Effective start minimizer, including MMFF-to-UFF fallback.
    pub minimizer: Minimizer,
    /// Algorithms selected for the converged representatives, including fallbacks.
    pub optimizers: Vec<reshiki_optimize::Method>,
    /// The report at exactly the final positions of `ranked[0]`.
    pub best: Arc<run::Report>,
    /// Lowest first, aligned onto the best.
    pub ranked: Vec<Ranked>,
    /// Valid minimized starts passed to clustering.
    pub starts: usize,
    pub clusters: usize,
    /// Representatives with successful xTB reports, out of those planned.
    pub reranked: usize,
    pub planned: usize,
    /// Cancellation during re-ranking kept only completed representatives.
    pub stopped: bool,
    pub notes: Vec<String>,
    /// Seconds of the whole search.
    pub seconds: f64,
}

impl Found {
    /// The ensemble saved and exported by the Calculate card.
    pub fn trajectory(&self, frame: &Frame) -> Result<Trajectory, Failure> {
        let mut trajectory = conformers::trajectory(
            &self.ranked,
            &frame.numbers,
            frame.charge,
            frame.multiplicity,
        )
        .map_err(conformer_failure)?;
        trajectory.isotopes = frame.isotopes.clone();
        trajectory.masses_amu = frame.masses_amu.clone();
        Ok(trajectory)
    }
}

pub fn search_conformers(
    search: Search,
    cancel: &Cancel,
    mut on_event: impl FnMut(Phase),
) -> Result<Found, Failure> {
    let began = Instant::now();
    check_cancel(cancel)?;
    if search.atoms.len() != search.numbers.len() || search.numbers != search.job.numbers {
        return Err(frame_mismatch());
    }
    let xtb_minimizer = search.config.minimizer.xtb();
    let mut notes = Vec::new();
    let (mut starts, minimizer) = sample(&search, cancel, &mut on_event, &mut notes)?;
    let minimized = if xtb_minimizer.is_some() {
        let positions = starts
            .iter()
            .map(|start| start.positions.as_slice())
            .collect();
        let outcomes = optimize_all(
            search.executable.as_deref(),
            &search.job,
            positions,
            cancel,
            &mut on_event,
            |done, total| Phase::Minimizing { done, total },
        );
        check_cancel(cancel)?;
        let finished = select_finished(outcomes?, false)?;
        if finished.failed > 0 {
            let n = finished.failed;
            notes.push(format!("{n} starts failed with xTB and were left out."));
        }
        if finished.unconverged > 0 {
            let n = finished.unconverged;
            notes.push(format!(
                "{n} starts did not converge within the step limit and were left out."
            ));
        }
        starts = finished
            .reports
            .iter()
            .map(|(_, report)| Start {
                positions: report.positions.clone(),
                energy_hartree: report.energy,
            })
            .collect();
        Some(finished)
    } else {
        None
    };

    check_cancel(cancel)?;
    on_event(Phase::Clustering);
    let topology = Topology::new(search.numbers, search.bonds).map_err(conformer_failure)?;
    let settings = Settings {
        diversity: match search.config.diversity {
            Diversity::Fine => conformers::Diversity::Fine,
            Diversity::Normal => conformers::Diversity::Normal,
            Diversity::Coarse => conformers::Diversity::Coarse,
        },
        per_cluster: if search.config.three {
            PerCluster::Three
        } else {
            PerCluster::One
        },
        window_kcal: search.config.window_kcal,
        max_representatives: search.config.representatives,
        ..Settings::default()
    };
    let clustering = conformers::cluster(&topology, &starts, &settings);
    check_cancel(cancel)?;
    let clustering = clustering.map_err(conformer_failure)?;
    if clustering.truncated_automorphisms {
        notes.push(
            "Symmetry matching was truncated for this molecule; RMSDs may be slightly high.".into(),
        );
    }
    let representatives = representatives(&clustering, search.config.representatives)?;
    let planned = representatives.len();
    check_cancel(cancel)?;
    let missing_start = || Failure::Other("A conformer representative is missing its start".into());
    let start_for = |rep: &Representative| starts.get(rep.start).ok_or_else(missing_start);

    let reranking = minimized.is_none();
    let (engine, reports) = if let Some(minimized) = minimized {
        let reports = representatives
            .iter()
            .enumerate()
            .map(|(slot, rep)| {
                minimized
                    .reports
                    .get(rep.start)
                    .map(|(_, report)| (slot, report.clone()))
                    .ok_or_else(missing_start)
            })
            .collect::<Result<_, _>>()?;
        (minimized.engine, reports)
    } else {
        let positions = representatives
            .iter()
            .map(|rep| start_for(rep).map(|start| start.positions.as_slice()))
            .collect::<Result<_, _>>()?;
        let outcomes = optimize_all(
            search.executable.as_deref(),
            &search.job,
            positions,
            cancel,
            &mut on_event,
            |done, total| Phase::Reranking { done, total },
        );
        let stopped = cancel.is_cancelled();
        let finished = select_finished(outcomes?, stopped)?;
        if finished.failed > 0 {
            let n = finished.failed;
            notes.push(format!(
                "{n} representatives failed with xTB and were left out."
            ));
        }
        if finished.unconverged > 0 {
            let n = finished.unconverged;
            notes.push(format!(
                "{n} representatives did not converge within the step limit and were left out."
            ));
        }
        (finished.engine, finished.reports)
    };

    let candidates = reports
        .iter()
        .map(|(slot, report)| {
            let rep = representatives.get(*slot).ok_or_else(missing_start)?;
            Ok(Candidate {
                positions: report.positions.clone(),
                energy_hartree: report.energy,
                start_energy_hartree: start_for(rep)?.energy_hartree,
                cluster: rep.cluster,
                cluster_size: rep.cluster_size,
            })
        })
        .collect::<Result<_, Failure>>()?;
    let ranked = conformers::rank(&topology, candidates, &settings).map_err(conformer_failure)?;
    let lowest = ranked.first().ok_or_else(no_representatives)?;
    // Ranking preserves the best coordinates. The energy distinguishes
    // reports at identical coordinates when their energies differ.
    let best = reports
        .iter()
        .map(|(_, report)| report)
        .find(|report| {
            report.positions == lowest.positions && report.energy == lowest.energy_hartree
        })
        .cloned()
        .ok_or_else(|| Failure::Other("The lowest conformer's xTB report is missing".into()))?;
    if !reranking {
        check_cancel(cancel)?;
    }
    let mut optimizers = Vec::new();
    for (_, report) in &reports {
        for &method in &report.optimizers {
            if !optimizers.contains(&method) {
                optimizers.push(method);
            }
        }
    }
    Ok(Found {
        engine,
        minimizer,
        optimizers,
        best,
        ranked,
        starts: starts.len(),
        clusters: clustering.clusters.len(),
        reranked: reports.len(),
        planned,
        stopped: reranking && cancel.is_cancelled(),
        notes,
        seconds: began.elapsed().as_secs_f64(),
    })
}

/// The force field used to prepare starting geometries for this minimizer.
pub fn starting_force_field(minimizer: Minimizer) -> ForceField {
    match minimizer {
        Minimizer::Mmff94 => ForceField::Mmff94,
        Minimizer::Uff => ForceField::Uff,
        Minimizer::Mmff94s | Minimizer::GfnFf | Minimizer::Gfn2 => ForceField::Mmff94s,
    }
}

fn sample(
    search: &Search,
    cancel: &Cancel,
    on_event: &mut impl FnMut(Phase),
    notes: &mut Vec<String>,
) -> Result<(Vec<Start>, Minimizer), Failure> {
    on_event(Phase::Starts);
    let minimizer = search.config.minimizer;
    let embedded = minimizer.xtb().is_some();
    let (field, effective) = sampling_minimizer(minimizer, search.prepared.mmff_available());
    if effective != minimizer {
        notes.push("MMFF cannot type this molecule (it has a metal); UFF minimized the starts. GFN-FF is recommended for metal complexes.".into());
    }
    let mode = if embedded {
        Ensemble::Embedded
    } else {
        Ensemble::Minimized
    };
    let total = search.config.starts as usize;
    let sampled = geometry_request(
        cancel,
        search.geometry.conformer_starts_with(
            &search.prepared,
            field,
            search.config.starts,
            BASE_SEED,
            mode,
            Instant::now() + Duration::from_secs(600),
            |done| on_event(Phase::Sampled { done, total }),
        ),
    )?;
    let count = sampled.conformers.len();
    if let Some(reason) = sampled.stopped {
        notes.push(format!(
            "Sampling stopped early: {reason}. {count} starts were used."
        ));
    } else if count < total {
        notes.push(format!(
            "The geometry worker produced {count} of {total} requested starts."
        ));
    }
    if count == 0 {
        return Err(Failure::Other("No conformer starts were generated".into()));
    }
    // A start the force field left unconverged would cluster and rank by a
    // geometry and energy that are not a minimum; keep them only when no
    // start converged.
    let mut conformers = sampled.conformers;
    if !embedded && conformers.iter().any(|entry| entry.converged) {
        let before = conformers.len();
        conformers.retain(|entry| entry.converged);
        let dropped = before - conformers.len();
        if dropped > 0 {
            notes.push(format!(
                "{dropped} starts did not converge with the force field and were left out."
            ));
        }
    } else if !embedded {
        notes.push(
            "No start converged with the force field: clustering used unconverged geometries, and xTB re-optimized the representatives.".into(),
        );
    }
    let starts = conformers
        .into_iter()
        .map(|entry| {
            let positions = frame_positions(
                search.prepared.ids(),
                &search.atoms,
                &search.parents,
                &entry.conformer,
            )?;
            let energy_hartree = if embedded {
                0.0
            } else {
                entry.energy / KCAL_PER_HARTREE
            };
            Ok(Start {
                positions,
                energy_hartree,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok((starts, effective))
}

fn sampling_minimizer(minimizer: Minimizer, mmff_available: bool) -> (ForceField, Minimizer) {
    let field = starting_force_field(minimizer);
    if field != ForceField::Uff && !mmff_available {
        let effective = if minimizer.xtb().is_some() {
            minimizer
        } else {
            Minimizer::Uff
        };
        (ForceField::Uff, effective)
    } else {
        (field, minimizer)
    }
}

fn frame_positions(
    ids: &[u64],
    atoms: &[Option<u64>],
    parents: &[Option<u64>],
    conformer: &Conformer,
) -> Result<Vec<[f64; 3]>, Failure> {
    if ids.len() != conformer.original_atom_count
        || atoms.len() != parents.len()
        || atoms.len() != conformer.positions.len()
        || conformer.positions.len().checked_sub(ids.len())
            != Some(conformer.hydrogen_parents.len())
    {
        return Err(frame_mismatch());
    }
    let (originals, hydrogens) = conformer.positions.split_at(ids.len());
    let mut by_id: HashMap<_, _> = ids.iter().copied().zip(originals).collect();
    if by_id.len() != ids.len() {
        return Err(frame_mismatch());
    }
    let mut by_parent: HashMap<_, VecDeque<_>> = HashMap::new();
    for (&parent, point) in conformer.hydrogen_parents.iter().zip(hydrogens) {
        let id = *ids.get(parent).ok_or_else(frame_mismatch)?;
        by_parent.entry(id).or_default().push_back(point);
    }
    atoms
        .iter()
        .zip(parents)
        .map(|(&atom, &parent)| {
            let point = match (atom, parent) {
                (Some(id), None) => by_id.remove(&id),
                (None, Some(id)) => by_parent.get_mut(&id).and_then(VecDeque::pop_front),
                _ => None,
            }
            .ok_or_else(frame_mismatch)?;
            Ok([point.x, point.y, point.z])
        })
        .collect()
}

struct Representative {
    start: usize,
    cluster: usize,
    cluster_size: u32,
}

fn representatives(clustering: &Clustering, limit: usize) -> Result<Vec<Representative>, Failure> {
    clustering
        .representatives
        .iter()
        .take(limit)
        .map(|&start| {
            let (cluster, members) = clustering
                .clusters
                .iter()
                .enumerate()
                .find(|(_, members)| members.contains(&start))
                .ok_or_else(|| {
                    Failure::Other("A conformer representative is missing its cluster".into())
                })?;
            Ok(Representative {
                start,
                cluster,
                cluster_size: members.len() as u32,
            })
        })
        .collect()
}

type Optimization = (usize, Result<Arc<run::Report>, Failure>);

fn optimize_all(
    executable: Option<&Path>,
    template: &run::Job,
    positions: Vec<&[[f64; 3]]>,
    cancel: &Cancel,
    on_event: &mut impl FnMut(Phase),
    phase: impl Fn(usize, usize) -> Phase,
) -> Result<Vec<Optimization>, Failure> {
    let total = positions.len();
    if total == 0 {
        return Err(no_representatives());
    }
    on_event(phase(0, total));
    let resolved;
    let executable = match executable {
        Some(path) => path,
        None => {
            resolved = std::env::current_exe()
                .map_err(|_| Failure::crashed("could not start the xTB worker"))?;
            &resolved
        }
    };
    // The card's Threads setting is how many xTB workers run at once.
    let workers = (template.threads.clamp(1, super::MAX_CONCURRENT_JOBS) as usize).min(total);
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let (completed_tx, completed) = sync_channel::channel();
        for index in 0..workers {
            let completed_tx = completed_tx.clone();
            let next = &next;
            let positions = &positions;
            let worker = thread::Builder::new()
                .name("conformer-xtb".into())
                .spawn_scoped(scope, move || {
                    while !cancel.is_cancelled() {
                        let slot = next.fetch_add(1, Ordering::Relaxed);
                        let Some(xyz) = positions.get(slot) else {
                            break;
                        };
                        let mut job = template.clone();
                        job.positions = xyz.to_vec();
                        job.optimize = true;
                        job.fixed.clear();
                        job.threads = 1;
                        let result = run::run(executable, job, cancel, |_| {}).map(Arc::new);
                        if completed_tx.send((slot, result)).is_err() {
                            break;
                        }
                    }
                });
            if let Err(error) = worker {
                if index == 0 {
                    return Err(Failure::Other(error.to_string()));
                }
                break;
            }
        }
        drop(completed_tx);
        let mut outcomes = Vec::with_capacity(total);
        let mut done = 0;
        for outcome in completed {
            done += 1;
            on_event(phase(done, total));
            outcomes.push(outcome);
        }
        Ok(outcomes)
    })
}

struct Finished {
    engine: Engine,
    reports: Vec<(usize, Arc<run::Report>)>,
    failed: usize,
    /// Optimizations that stopped at the step limit, left out.
    unconverged: usize,
}

fn select_finished(outcomes: Vec<Optimization>, stopped: bool) -> Result<Finished, Failure> {
    let mut reports = Vec::new();
    let mut first_failure = None;
    let mut failed = 0;
    let mut unconverged = 0;
    for (slot, result) in outcomes {
        match result {
            Ok(report) if !report.converged => {
                unconverged += 1;
                first_failure.get_or_insert(Failure::Other(
                    "The xTB optimizations did not converge within the step limit".into(),
                ));
            }
            Ok(report) => reports.push((slot, report)),
            Err(error)
                if stopped
                    && matches!(error.reason(), Failure::Other(message) if message == CANCELED) => {
            }
            Err(error) => {
                failed += 1;
                first_failure.get_or_insert(error);
            }
        }
    }
    let engine = reports
        .first()
        .map(|(_, report)| report.engine.clone())
        .ok_or_else(|| {
            if stopped {
                canceled()
            } else {
                first_failure.unwrap_or_else(no_representatives)
            }
        })?;
    // Keep start and representative order independent of worker scheduling.
    reports.sort_unstable_by_key(|(slot, _)| *slot);
    Ok(Finished {
        engine,
        reports,
        failed,
        unconverged,
    })
}

/// Generate a starting model with the shared fresh/Update parameters.
pub fn generate_model(
    doc: &Document,
    ids: &[u64],
    generation: geometry::ModelGeneration,
    client: &Client,
    cancel: &Cancel,
) -> Result<Model3d, Failure> {
    let prepared = Prepared::new(doc, ids)
        .map_err(geometry_failure)?
        .without_retained_depth();
    let optimized = geometry_request(
        cancel,
        client.generate_seeded(&prepared, generation.force_field, generation.seed),
    )?;
    let mut model = prepared.model(&optimized).map_err(geometry_failure)?;
    model.seed = generation.seed;
    Ok(model)
}

fn geometry_request<T>(
    cancel: &Cancel,
    request: impl Future<Output = Result<T, geometry::Error>>,
) -> Result<T, Failure> {
    check_cancel(cancel)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Failure::Other(error.to_string()))?;
    let result = runtime.block_on(async {
        let wait_for_cancel = async {
            while !cancel.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        tokio::select! {
            biased;
            () = wait_for_cancel => Err(canceled()),
            result = request => result.map_err(geometry_failure),
        }
    });
    check_cancel(cancel)?;
    result
}

fn check_cancel(cancel: &Cancel) -> Result<(), Failure> {
    if cancel.is_cancelled() {
        Err(canceled())
    } else {
        Ok(())
    }
}

fn canceled() -> Failure {
    Failure::Other(CANCELED.into())
}

fn frame_mismatch() -> Failure {
    Failure::Other(FRAME_MISMATCH.into())
}

fn no_representatives() -> Failure {
    Failure::Other("No conformer representatives were available to rank".into())
}

fn conformer_failure(error: conformers::Error) -> Failure {
    Failure::Other(error.to_string())
}

fn geometry_failure(error: geometry::Error) -> Failure {
    let summary = match &error {
        geometry::Error::Exit { code, .. } => match code {
            Some(code) => format!("Geometry worker exited with code {code}"),
            None => "Geometry worker stopped unexpectedly".into(),
        },
        geometry::Error::Timeout => "Geometry worker reached its request deadline".into(),
        geometry::Error::Protocol(_) => "Geometry worker protocol error".into(),
        geometry::Error::Io(_) => "Could not communicate with the geometry worker".into(),
        error => return Failure::Other(error.to_string()),
    };
    Failure::Crashed {
        summary,
        details: Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geometry::Point3;
    use reshiki_model::{document::Point, models3d};

    #[test]
    fn starting_force_field_matches_the_minimizer() {
        for (minimizer, expected) in [
            (Minimizer::Mmff94s, ForceField::Mmff94s),
            (Minimizer::Mmff94, ForceField::Mmff94),
            (Minimizer::Uff, ForceField::Uff),
            (Minimizer::GfnFf, ForceField::Mmff94s),
            (Minimizer::Gfn2, ForceField::Mmff94s),
        ] {
            assert_eq!(starting_force_field(minimizer), expected, "{minimizer}");
        }
    }

    #[test]
    fn unavailable_mmff_records_uff_but_keeps_xtb_start_minimizers() {
        for minimizer in [Minimizer::Mmff94s, Minimizer::Mmff94] {
            assert_eq!(
                sampling_minimizer(minimizer, false),
                (ForceField::Uff, Minimizer::Uff)
            );
            assert_eq!(sampling_minimizer(minimizer, true).1, minimizer);
        }
        for minimizer in [Minimizer::GfnFf, Minimizer::Gfn2] {
            assert_eq!(
                sampling_minimizer(minimizer, false),
                (ForceField::Uff, minimizer)
            );
        }
        assert_eq!(
            sampling_minimizer(Minimizer::Uff, true),
            (ForceField::Uff, Minimizer::Uff)
        );
    }

    fn water_search(minimizer: Minimizer, geometry: Client) -> Search {
        let mut doc = Document::default();
        let oxygen = doc.add_atom("O", Point::default());
        let placed = vec![models3d::Placed {
            id: oxygen,
            xyz: [0.; 3],
        }];
        let hydrogens = vec![
            models3d::Hydrogen {
                parent: oxygen,
                xyz: [0.96, 0., 0.],
            },
            models3d::Hydrogen {
                parent: oxygen,
                xyz: [-0.24, 0.93, 0.],
            },
        ];
        doc.models.push(Model3d::new(
            &doc,
            placed,
            hydrogens,
            models3d::ForceField::Mmff94s,
        ));
        models3d::require_version(&mut doc);
        let launch = science::launch(&doc, oxygen, 0, 1).unwrap();
        let job = run::Job {
            optimize: false,
            frequencies: false,
            masses: None,
            isotope_labels: Vec::new(),
            numbers: launch.frame.numbers.clone(),
            positions: launch.frame.positions_angstrom.clone(),
            charge: 0,
            uhf: 0,
            method: Method::Gxtb,
            conventions: Default::default(),
            scf: reshiki_xtb_worker::protocol::ScfSettings {
                accuracy: Some(0.1),
                ..Default::default()
            },
            solvent: Some(method::solvent(reshiki_qcdata::SolventModel::Alpb, "water")),
            threads: 1,
            fixed: vec![],
            tight: false,
            topology: None,
            max_steps: None,
            properties: None,
            thermo: Default::default(),
        };
        let config = Config::new(1, minimizer, Diversity::Normal, 15., 20, 1).unwrap();
        Search::from_launch(&doc, &[oxygen], &launch, config, job, geometry, None).unwrap()
    }

    #[test]
    fn a_geometry_startup_failure_stops_the_search_in_sampling() {
        let mut geometry = Client::default();
        geometry.limits.timeout = Duration::ZERO;
        let search = water_search(Minimizer::Mmff94s, geometry);
        let mut phases = vec![];
        let error =
            search_conformers(search, &Cancel::default(), |phase| phases.push(phase)).unwrap_err();
        assert_eq!(phases, [Phase::Starts]);
        assert!(error.to_string().starts_with("3D geometry: Worker limits"));
    }

    #[test]
    fn an_xtb_minimizer_sets_the_method_and_options_for_every_job() {
        let gfnff = water_search(Minimizer::GfnFf, Client::default()).job;
        assert_eq!(gfnff.method, Method::Gfnff);
        assert_eq!(gfnff.topology.as_ref(), Some(&gfnff.positions));
        assert_eq!(gfnff.scf, Default::default());
        assert_eq!(gfnff.solvent, None);

        let gfn2 = water_search(Minimizer::Gfn2, Client::default()).job;
        assert_eq!(gfn2.method, Method::Gfn2);
        assert_eq!(gfn2.topology, None);
        assert_eq!(gfn2.scf.accuracy, Some(0.1));
        assert!(gfn2.solvent.is_some());
    }

    fn conformer() -> Conformer {
        Conformer {
            positions: (0..5)
                .map(|x| Point3 {
                    x: f64::from(x),
                    y: 0.0,
                    z: 0.0,
                })
                .collect(),
            original_atom_count: 2,
            hydrogen_parents: vec![0, 1, 0],
        }
    }

    #[test]
    fn maps_original_ids_and_each_parents_hydrogens_in_order() {
        let positions = frame_positions(
            &[10, 20],
            &[Some(20), Some(10), None, None, None],
            &[None, None, Some(10), Some(10), Some(20)],
            &conformer(),
        )
        .unwrap();
        assert_eq!(
            positions,
            [
                [1., 0., 0.],
                [0., 0., 0.],
                [2., 0., 0.],
                [4., 0., 0.],
                [3., 0., 0.]
            ]
        );
    }

    #[test]
    fn rejects_a_missing_hydrogen() {
        let mut sample = conformer();
        sample.positions.pop();
        sample.hydrogen_parents.pop();
        assert_eq!(
            frame_positions(
                &[10, 20],
                &[Some(10), Some(20), None, None, None],
                &[None, None, Some(10), Some(10), Some(20)],
                &sample,
            )
            .unwrap_err()
            .to_string(),
            FRAME_MISMATCH
        );
    }

    #[test]
    fn rejects_an_extra_hydrogen() {
        assert_eq!(
            frame_positions(
                &[10, 20],
                &[Some(10), Some(20), None, None],
                &[None, None, Some(10), Some(20)],
                &conformer(),
            )
            .unwrap_err()
            .to_string(),
            FRAME_MISMATCH
        );
    }

    #[test]
    fn rejects_unknown_and_reused_atom_ids_and_wrong_parent_counts() {
        for (atoms, parents) in [
            (
                [Some(30), Some(20), None, None, None],
                [None, None, Some(10), Some(10), Some(20)],
            ),
            (
                [Some(20), Some(20), None, None, None],
                [None, None, Some(10), Some(10), Some(20)],
            ),
            (
                [Some(10), Some(20), None, None, None],
                [None, None, Some(10), Some(20), Some(20)],
            ),
            (
                [Some(10), Some(20), None, None, None],
                [None, None, Some(99), Some(10), Some(20)],
            ),
        ] {
            assert_eq!(
                frame_positions(&[10, 20], &atoms, &parents, &conformer())
                    .unwrap_err()
                    .to_string(),
                FRAME_MISMATCH
            );
        }
    }

    #[test]
    fn representatives_keep_order_and_match_any_member_of_their_cluster() {
        let clustering = Clustering {
            kept: vec![3, 1, 2, 4],
            clusters: vec![vec![3, 2, 4], vec![1]],
            representatives: vec![1, 2, 4],
            truncated_automorphisms: false,
        };
        let plan = representatives(&clustering, 2).unwrap();
        let rows: Vec<_> = plan
            .iter()
            .map(|rep| (rep.start, rep.cluster, rep.cluster_size))
            .collect();
        assert_eq!(rows, [(1, 1, 1), (2, 0, 3)]);
    }

    fn report(engine_name: &str) -> Arc<run::Report> {
        let mut engine = run::example_engine();
        engine.name = engine_name.into();
        Arc::new(run::Report {
            engine,
            computed: run::example_computed(-1.0, 1.0),
            energy: -1.0,
            converged: true,
            positions: vec![[0.0; 3]],
            steps: 0,
            optimizers: vec![],
            energies: vec![-1.0],
            path: vec![],
            evaluations: vec![],
        })
    }

    #[test]
    fn cancellation_keeps_completed_reports_in_plan_order() {
        let first = report("first finished");
        let second = report("second finished");
        let selected = select_finished(
            vec![
                (3, Ok(first.clone())),
                (0, Err(canceled())),
                (4, Err(Failure::Scf("SCF failed".into()))),
                (2, Ok(second.clone())),
            ],
            true,
        )
        .unwrap();
        assert_eq!(selected.engine.name, "first finished");
        assert_eq!(selected.failed, 1);
        assert_eq!(selected.reports.len(), 2);
        assert_eq!((selected.reports[0].0, selected.reports[1].0), (2, 3));
        assert!(Arc::ptr_eq(&selected.reports[0].1, &second));
        assert!(Arc::ptr_eq(&selected.reports[1].1, &first));
    }

    #[test]
    fn no_completed_reports_returns_cancellation_or_the_first_failure() {
        let failed = || Err(Failure::Scf("SCF failed".into()));
        assert!(
            matches!(select_finished(vec![(1, failed()), (0, Err(canceled()))], true), Err(Failure::Other(message)) if message == CANCELED)
        );
        assert!(
            matches!(select_finished(vec![(1, failed()), (0, Err(Failure::crashed("worker stopped")))], false), Err(Failure::Scf(message)) if message == "SCF failed")
        );
    }
}
