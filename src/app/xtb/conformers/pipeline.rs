//! Desktop conformer sampling, clustering and xTB ranking.

use super::super::{run, state::Failure};
use super::{Diversity, Minimizer, Phase};
use iced::futures::{Stream, channel::mpsc};
use reshiki::geometry::{self, Conformer, Ensemble, ForceField, Prepared};
use reshiki_optimize::{
    KCAL_PER_HARTREE,
    conformers::{self, Candidate, Clustering, PerCluster, Ranked, Settings, Start, Topology},
};
use reshiki_xtb_worker::{client::Cancel, protocol::Engine};
use std::{
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
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

pub(crate) struct Search {
    /// The drawing component as the geometry worker sees it.
    pub prepared: Prepared,
    /// Atomic numbers in frame order (the model's atoms, then added hydrogens).
    pub numbers: Vec<u8>,
    /// Drawing atom of each frame atom, or `None` for model hydrogens.
    pub atoms: Vec<Option<u64>>,
    /// Drawing atom each model hydrogen is bonded to, or `None` elsewhere.
    pub parents: Vec<Option<u64>>,
    /// Bonds in frame order, for clustering.
    pub bonds: Vec<[usize; 2]>,
    pub config: super::Config,
    /// Template for every xTB optimization. The pipeline replaces positions,
    /// enables optimization, clears fixed atoms and uses one thread per job.
    /// For an xTB start minimizer, supply that minimizer's method and settings.
    pub job: run::Job,
}

#[derive(Clone, Debug)]
pub(crate) enum Event {
    Phase(Phase),
    Tick,
    /// Always the last event.
    Done(Result<Found, Failure>),
}

#[derive(Clone, Debug)]
pub(crate) struct Found {
    pub engine: Engine,
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

/// Start `search` on a new thread. The stream ends after `Event::Done`.
pub(crate) fn start(search: Search, cancel: Cancel) -> impl Stream<Item = Event> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    let worker_tx = tx.clone();
    let worker = thread::Builder::new()
        .name("conformer-search".into())
        .spawn(move || {
            let (stop_ticks, tick_wait) = sync_channel::channel::<()>();
            let tick_tx = worker_tx.clone();
            let tick_cancel = cancel.clone();
            let ticker = thread::Builder::new()
                .name("conformer-tick".into())
                .spawn(move || {
                    while let Err(sync_channel::RecvTimeoutError::Timeout) =
                        tick_wait.recv_timeout(Duration::from_millis(250))
                    {
                        if tick_tx.unbounded_send(Event::Tick).is_err() {
                            tick_cancel.cancel();
                            break;
                        }
                    }
                });
            let result = match ticker {
                Ok(ticker) => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        search_conformers(search, &cancel, &worker_tx)
                    }))
                    .unwrap_or_else(|_| {
                        Err(Failure::Crashed("Conformer search thread panicked".into()))
                    });
                    drop(stop_ticks);
                    let _ = ticker.join();
                    result
                }
                Err(error) => Err(Failure::Other(error.to_string())),
            };
            let _ = worker_tx.unbounded_send(Event::Done(result));
            worker_tx.close_channel();
        });
    if let Err(error) = worker {
        let _ = tx.unbounded_send(Event::Done(Err(Failure::Other(error.to_string()))));
        tx.close_channel();
    }
    rx
}

fn search_conformers(
    search: Search,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
) -> Result<Found, Failure> {
    let began = Instant::now();
    check_cancel(cancel)?;
    if search.atoms.len() != search.numbers.len() || search.numbers != search.job.numbers {
        return Err(frame_mismatch());
    }
    let xtb_minimizer = search.config.minimizer.xtb();
    if xtb_minimizer.is_some_and(|method| method != search.job.method.xtb_method()) {
        return Err(Failure::Other(
            "The xTB job method must match the selected conformer minimizer".into(),
        ));
    }
    let mut notes = Vec::new();
    let mut starts = sample(&search, cancel, tx, &mut notes)?;
    let minimized = if xtb_minimizer.is_some() {
        let positions = starts
            .iter()
            .map(|start| start.positions.as_slice())
            .collect();
        let outcomes = optimize_all(&search.job, positions, cancel, tx, |done, total| {
            Phase::Minimizing { done, total }
        });
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
    emit_phase(tx, Phase::Clustering);
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
        let outcomes = optimize_all(&search.job, positions, cancel, tx, |done, total| {
            Phase::Reranking { done, total }
        });
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
    Ok(Found {
        engine,
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

fn sample(
    search: &Search,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
    notes: &mut Vec<String>,
) -> Result<Vec<Start>, Failure> {
    emit_phase(tx, Phase::Starts);
    let minimizer = search.config.minimizer;
    let embedded = minimizer.xtb().is_some();
    let mut field = match minimizer {
        Minimizer::Mmff94 => ForceField::Mmff94,
        Minimizer::Uff => ForceField::Uff,
        Minimizer::Mmff94s | Minimizer::GfnFf | Minimizer::Gfn2 => ForceField::Mmff94s,
    };
    if field != ForceField::Uff && !search.prepared.mmff_available() {
        field = ForceField::Uff;
        if !embedded {
            notes.push("MMFF cannot type this molecule (it has a metal); UFF minimized the starts. GFN-FF is recommended for metal complexes.".into());
        }
    }
    let mode = if embedded {
        Ensemble::Embedded
    } else {
        Ensemble::Minimized
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Failure::Other(error.to_string()))?;
    let client = geometry::Client::default();
    let total = search.config.starts as usize;
    let sampled = runtime.block_on(async {
        let wait_for_cancel = async {
            while !cancel.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let starts = client.conformer_starts_with(
            &search.prepared,
            field,
            search.config.starts,
            BASE_SEED,
            mode,
            Instant::now() + Duration::from_secs(600),
            |done| emit_phase(tx, Phase::Sampled { done, total }),
        );
        tokio::select! {
            biased;
            () = wait_for_cancel => Err(canceled()),
            result = starts => result.map_err(geometry_failure),
        }
    });
    check_cancel(cancel)?;
    let sampled = sampled?;
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
    }
    conformers
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
        .collect()
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
    template: &run::Job,
    positions: Vec<&[[f64; 3]]>,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
    phase: impl Fn(usize, usize) -> Phase,
) -> Result<Vec<Optimization>, Failure> {
    let total = positions.len();
    if total == 0 {
        return Err(no_representatives());
    }
    emit_phase(tx, phase(0, total));
    // The card's Threads setting is how many xTB workers run at once.
    let workers = (template.threads.max(1) as usize).min(8).min(total);
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
                    // run::run needs a live receiver. Discard its step batches
                    // after each job instead of retaining them across the search.
                    let (steps_tx, mut steps) = mpsc::unbounded();
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
                        let result = run::run(job, cancel, &steps_tx).map(Arc::new);
                        while steps.try_recv().is_ok() {}
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
            emit_phase(tx, phase(done, total));
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
            Err(Failure::Other(message)) if stopped && message == CANCELED => {}
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

fn emit_phase(tx: &mpsc::UnboundedSender<Event>, phase: Phase) {
    let _ = tx.unbounded_send(Event::Phase(phase));
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
    match error {
        geometry::Error::Exit { code, .. } => Failure::Crashed(match code {
            Some(code) => format!("Geometry worker exited with code {code}"),
            None => "Geometry worker stopped unexpectedly".into(),
        }),
        geometry::Error::Timeout => {
            Failure::Crashed("Geometry worker reached its request deadline".into())
        }
        geometry::Error::Protocol(_) => Failure::Crashed("Geometry worker protocol error".into()),
        geometry::Error::Io(_) => {
            Failure::Crashed("Could not communicate with the geometry worker".into())
        }
        error => Failure::Other(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geometry::Point3;

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
            ),
            Err(frame_mismatch())
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
            ),
            Err(frame_mismatch())
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
                frame_positions(&[10, 20], &atoms, &parents, &conformer()),
                Err(frame_mismatch())
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
        let failure = Failure::Scf("SCF failed".into());
        assert!(
            matches!(select_finished(vec![(1, Err(failure.clone())), (0, Err(canceled()))], true), Err(error) if error == canceled())
        );
        assert!(
            matches!(select_finished(vec![(1, Err(failure.clone())), (0, Err(Failure::Crashed("worker stopped".into())))], false), Err(error) if error == failure)
        );
    }
}
