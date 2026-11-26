//! Developer-only command for timing the desktop app's conformer search.
//! The opt-in feature is never enabled in shipped release builds.

use super::{
    Diversity, Event, Found, MAX_REPRESENTATIVES, MAX_STARTS, Minimizer, Phase, Search, Settings,
    pipeline, search,
};
use crate::app::{App, xtb::state};
use iced::futures::{StreamExt, executor::block_on};
use reshiki::{
    engine::{LocalEngine, Request},
    geometry::{Client, DEFAULT_SEED, Ensemble, ForceField, Prepared},
    models3d::{self, Display},
    molecule_names, science,
};
use reshiki_xtb_worker::client::Cancel;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::File,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const USAGE: &str = "Usage: reshiki --conformer-bench FILE.smi [--starts N] \
    [--minimizer mmff94s|gfnff|gfn2] [--diversity fine|normal|coarse] \
    [--representatives N] [--threads N] [--embed-probe]

--embed-probe times only embedding the same starts (no minimization) and skips the search.";

impl App {
    pub(crate) fn xtb_conformer_bench(args: impl Iterator<Item = OsString>) -> i32 {
        let args = match Args::parse(args) {
            Ok(args) => args,
            Err(error) => {
                eprintln!("{error}\n{USAGE}");
                return 2;
            }
        };
        match benchmark(args).and_then(|report| {
            writeln!(io::stdout().lock(), "{report}")
                .map_err(|error| format!("Could not write the benchmark report: {error}"))
        }) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Conformer benchmark failed: {error}");
                1
            }
        }
    }
}

struct Args {
    file: PathBuf,
    search: Settings,
    threads: u32,
    embed_probe: bool,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let file = args
            .next()
            .filter(|arg| !arg.is_empty() && !arg.as_encoded_bytes().starts_with(b"-"))
            .ok_or("Expected a SMILES file")?;
        let mut search = Settings::default();
        let mut threads = state::Settings::default().threads;
        let mut embed_probe = false;
        while let Some(flag) = args.next() {
            let flag = flag.to_str().ok_or("Option names must be valid Unicode")?;
            let mut value = || {
                args.next()
                    .and_then(|arg| arg.into_string().ok())
                    .ok_or_else(|| format!("Expected a value for {flag}"))
            };
            match flag {
                "--starts" | "--representatives" => {
                    let value = value()?;
                    let (field, max) = if flag == "--starts" {
                        (&mut search.starts, MAX_STARTS as usize)
                    } else {
                        (&mut search.representatives, MAX_REPRESENTATIVES)
                    };
                    if !value
                        .trim()
                        .parse::<usize>()
                        .is_ok_and(|n| (1..=max).contains(&n))
                    {
                        return Err(format!("{flag} expects a whole number from 1 to {max}"));
                    }
                    *field = value;
                }
                "--minimizer" => {
                    search.minimizer = match value()?.as_str() {
                        "mmff94s" => Minimizer::Mmff94s,
                        "gfnff" => Minimizer::GfnFf,
                        "gfn2" => Minimizer::Gfn2,
                        _ => return Err("--minimizer expects mmff94s, gfnff, or gfn2".into()),
                    };
                }
                "--diversity" => {
                    let value = value()?;
                    search.diversity = Diversity::ALL
                        .into_iter()
                        .find(|diversity| diversity.label() == value)
                        .ok_or("--diversity expects fine, normal, or coarse")?;
                }
                "--threads" => {
                    threads = value()?
                        .parse()
                        .map_err(|_| "--threads expects a nonnegative whole number (0 is Auto)")?;
                }
                "--embed-probe" => embed_probe = true,
                _ => return Err(format!("Unknown option: {flag}")),
            }
        }
        Ok(Self {
            file: file.into(),
            search,
            threads,
            embed_probe,
        })
    }
}

fn benchmark(args: Args) -> Result<Value, String> {
    let began = Instant::now();
    let line = File::open(&args.file)
        .and_then(|file| BufReader::new(file).lines().next().transpose())
        .map_err(|error| format!("Could not read {}: {error}", args.file.display()))?;
    let smiles = line
        .as_deref()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let smiles = smiles.ok_or("The first line must contain a SMILES string")?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("Could not start chemistry runtime: {error}"))?;
    let mut doc = runtime
        .block_on(LocalEngine::default().request(Request::import_smiles(smiles)))?
        .document
        .ok_or("Chemistry import returned no drawing")?;
    let component_error = "The SMILES must describe exactly one molecular component";
    let first = doc.atoms.first().ok_or(component_error)?.id;
    let atoms = molecule_names::component(&doc, first);
    if atoms.len() != doc.atoms.len() {
        return Err(component_error.into());
    }
    let import_seconds = began.elapsed().as_secs_f64();

    let began = Instant::now();
    let prepared = Prepared::new(&doc, &atoms)
        .map_err(|error| error.to_string())?
        .without_retained_depth();
    let optimized = runtime
        .block_on(Client::default().generate_seeded(&prepared, ForceField::Mmff94s, DEFAULT_SEED))
        .map_err(|error| error.to_string())?;
    let mut model = prepared
        .model(&optimized)
        .map_err(|error| error.to_string())?;
    model.seed = DEFAULT_SEED;
    let index = doc.models.len();
    doc.models.push(model);
    models3d::require_version(&mut doc);
    models3d::set_display(&mut doc, index, Display::Both);
    let model_seconds = began.elapsed().as_secs_f64();

    let began = Instant::now();
    let (charge, multiplicity) = science::drawn_state(&doc, first);
    let launch =
        science::launch(&doc, first, charge, multiplicity).map_err(|error| error.to_string())?;
    let launch_seconds = began.elapsed().as_secs_f64();

    let began = Instant::now();
    let numbers = &launch.frame.numbers;
    let calc = state::Settings {
        task: state::Task::Conformers,
        threads: args.threads,
        ..state::Settings::default()
    };
    let calc = super::effective(&calc, &args.search).config(
        (charge, multiplicity.saturating_sub(1)),
        false,
        vec![],
    )?;
    let settings = args.search.config(Some(numbers.len()))?;
    let failure_message = |failure: state::Failure| match failure {
        state::Failure::Other(message)
        | state::Failure::Scf(message)
        | state::Failure::Crashed(message) => message,
        failure => {
            let (message, detail) = failure.explain(calc.method, numbers);
            format!("{message} {detail}")
        }
    };
    let search =
        search::search_for(&doc, &atoms, &launch, &settings, &calc).map_err(failure_message)?;
    let prepare_search_seconds = began.elapsed().as_secs_f64();

    if args.embed_probe {
        // Embedding only, in a run of its own so it never warms the machine
        // before a timed search: the same starts without minimization.
        let began = Instant::now();
        // The pipeline's private BASE_SEED has the same value as DEFAULT_SEED.
        let embedded = runtime
            .block_on(Client::default().conformer_starts_with(
                &search.prepared,
                ForceField::Mmff94s,
                settings.starts,
                DEFAULT_SEED,
                Ensemble::Embedded,
                began + Duration::from_secs(600),
                |_| {},
            ))
            .map_err(|error| format!("Embedding probe failed: {error}"))?;
        if let Some(reason) = embedded.stopped {
            return Err(format!("Embedding probe stopped early: {reason}"));
        }
        return Ok(json!({
            "smiles": smiles,
            "atoms": numbers.len(),
            "starts_requested": settings.starts,
            "embed_seconds": began.elapsed().as_secs_f64(),
            "embed_count": embedded.conformers.len(),
        }));
    }

    let (found, phases) = block_on(run_search(search)).map_err(failure_message)?;
    let starts = phases.starts.ok_or("Missing Starts event")?;
    let clustering = phases.clustering.ok_or("Missing Clustering event")?;
    let done = phases.done.ok_or("Missing Done event")?;
    let sampling_end = if settings.minimizer.xtb().is_some() {
        phases.first_minimizing.ok_or("Missing Minimizing event")?
    } else {
        clustering
    };
    let minimizing_seconds = phases
        .first_minimizing
        .zip(phases.last_minimizing)
        .map(|(first, last)| last - first);

    let began = Instant::now();
    let file_bytes_model_only = doc.file_json()?.len();
    let pending =
        search::pending_for(&launch, &calc, &settings, false, &found).map_err(failure_message)?;
    science::attach(&mut doc, &pending.launch, pending.frames, pending.result)
        .map_err(|error| error.to_string())?;
    let file_bytes_with_result = doc.file_json()?.len();
    let file_size_seconds = began.elapsed().as_secs_f64();

    Ok(json!({
        "smiles": smiles,
        "atoms": numbers.len(),
        "heavy_atoms": numbers.iter().filter(|&&number| number > 1).count(),
        "settings": {
            "starts": settings.starts,
            "minimizer": settings.minimizer.label(),
            "diversity": settings.diversity.label(),
            "representatives": settings.representatives,
            "threads": calc.threads,
        },
        "import_seconds": import_seconds,
        "model_seconds": model_seconds,
        "launch_seconds": launch_seconds,
        "prepare_search_seconds": prepare_search_seconds,
        "phases": phases,
        "sampling_seconds": sampling_end - starts,
        "minimizing_seconds": minimizing_seconds,
        // Without re-ranking (xTB minimized every start) this also holds the final ranking.
        "clustering_seconds": phases.first_reranking.unwrap_or(done) - clustering,
        "rerank_seconds": phases.first_reranking.zip(phases.last_reranking).map(|(first, last)| last - first),
        // Everything after the last re-ranked representative: ranking, deduplication, ending the workers.
        "post_rerank_seconds": phases.last_reranking.map(|last| done - last),
        "search_seconds": found.seconds,
        "starts": found.starts,
        "clusters": found.clusters,
        "reranked": found.reranked,
        "planned": found.planned,
        "kept": found.ranked.len(),
        "stopped": found.stopped,
        "notes": found.notes,
        "best_energy_hartree": found.best.energy,
        "engine_version": found.engine.version,
        "file_bytes_model_only": file_bytes_model_only,
        "file_bytes_with_result": file_bytes_with_result,
        "file_size_seconds": file_size_seconds,
    }))
}

#[derive(Default, serde::Serialize)]
struct Phases {
    starts: Option<f64>,
    last_sampled: Option<f64>,
    first_minimizing: Option<f64>,
    last_minimizing: Option<f64>,
    clustering: Option<f64>,
    first_reranking: Option<f64>,
    last_reranking: Option<f64>,
    done: Option<f64>,
}

async fn run_search(search: Search) -> Result<(Found, Phases), state::Failure> {
    let mut phases = Phases::default();
    let cancel = Cancel::default();
    let began = Instant::now();
    let events = pipeline::start(search, cancel);
    iced::futures::pin_mut!(events);
    while let Some(event) = events.next().await {
        let elapsed = began.elapsed().as_secs_f64();
        match event {
            Event::Phase(Phase::Starts) => {
                phases.starts.get_or_insert(elapsed);
            }
            Event::Phase(Phase::Sampled { .. }) => phases.last_sampled = Some(elapsed),
            Event::Phase(Phase::Minimizing { .. }) => {
                phases.first_minimizing.get_or_insert(elapsed);
                phases.last_minimizing = Some(elapsed);
            }
            Event::Phase(Phase::Clustering) => {
                phases.clustering.get_or_insert(elapsed);
            }
            Event::Phase(Phase::Reranking { .. }) => {
                phases.first_reranking.get_or_insert(elapsed);
                phases.last_reranking = Some(elapsed);
            }
            Event::Tick => {}
            Event::Done(found) => {
                phases.done = Some(elapsed);
                return Ok((found?, phases));
            }
        }
    }
    Err(state::Failure::Other(
        "The search ended without a result".into(),
    ))
}
