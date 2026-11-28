use super::{super::plan::Step, *};
use crate::app::xtb::ts::{self as transition, job};
use crate::app::xtb::{self, CalcTask, state};
use iced::futures::{StreamExt, channel::mpsc};
use reshiki::{
    geometry::{Client, ForceField, Mode, Prepared},
    qcdata,
    science::{self, Launch},
};
use reshiki_xtb_jobs::run as driver;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) struct Request {
    pub plan: Plan,
    pub options: Vec<StepOptions>,
    pub method: XtbMethod,
    pub inputs: Vec<Result<Option<TsInput>, String>>,
    pub threads: u32,
    pub retry: Option<usize>,
    pub cache: Vec<Cached>,
    pub serial: u64,
}

pub(super) fn start(request: Request, cancel: Cancel) -> mpsc::UnboundedReceiver<Event> {
    let (tx, rx) = mpsc::unbounded();
    let serial = request.serial;
    let failure_tx = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("xtb-scheme".into())
        .spawn(move || {
            let result = std::env::current_exe()
                .map_err(|error| error.to_string())
                .and_then(|executable| {
                    let mut backend = Native {
                        executable,
                        threads: request.threads,
                    };
                    calculate(&request, &mut backend, &cancel, |kind| {
                        if tx.unbounded_send(Event { serial, kind }).is_err() {
                            cancel.cancel();
                        }
                    })
                });
            let _ = tx.unbounded_send(Event {
                serial,
                kind: EventKind::Finished(result),
            });
            tx.close_channel();
        });
    if let Err(error) = spawned {
        let _ = failure_tx.unbounded_send(Event {
            serial,
            kind: EventKind::Finished(Err(error.to_string())),
        });
        failure_tx.close_channel();
    }
    rx
}

struct SpeciesComputed {
    value: SpeciesValue,
    warning: Option<String>,
}

enum SpeciesFailure {
    Calculation(String),
    Stereo(String),
}

impl From<String> for SpeciesFailure {
    fn from(reason: String) -> Self {
        Self::Calculation(reason)
    }
}

impl SpeciesFailure {
    fn reason(&self) -> &str {
        match self {
            Self::Calculation(reason) | Self::Stereo(reason) => reason,
        }
    }
}

struct TransitionComputed {
    result: Arc<CalcResult>,
    frames: Vec<Arc<Frame>>,
    launch: Launch,
    discovered: Option<DiscoveredPath>,
}

struct SpeciesJob<'a> {
    index: usize,
    species: &'a super::super::plan::Species,
    method: XtbMethod,
    frequencies: bool,
    cached: Option<SpeciesValue>,
}

struct TransitionJob<'a> {
    input: &'a TsInput,
    optimized: Option<Launch>,
    arrow: u64,
    method: XtbMethod,
    free: bool,
}

/// Queue policy is independent of native execution so fake jobs exercise cache,
/// partial failure, retry and cancellation without chemistry timing assumptions.
trait Backend {
    /// Publish the electronic minimum before starting a requested Hessian.
    fn species(
        &mut self,
        job: SpeciesJob<'_>,
        cancel: &Cancel,
        minimum: &mut dyn FnMut(&SpeciesValue),
    ) -> Result<SpeciesComputed, SpeciesFailure>;
    fn transition(
        &mut self,
        job: TransitionJob<'_>,
        cancel: &Cancel,
        progress: &mut dyn FnMut(job::Event),
    ) -> Result<TransitionComputed, String>;
}

fn checked(cancel: &Cancel) -> Result<(), String> {
    if cancel.is_cancelled() {
        Err("Scheme calculation canceled".into())
    } else {
        Ok(())
    }
}

fn has_thermo(value: &SpeciesValue) -> bool {
    transition_thermo(&value.result, &value.frame)
}

/// Identity-based reuse only: chemically equal drawings with different IDs do
/// not establish a correspondence for implicit H atoms or stereochemistry.
fn optimized_start<'a>(
    input: &TsInput,
    values: impl IntoIterator<Item = &'a SpeciesValue>,
) -> Option<Launch> {
    let mut candidates = values.into_iter().filter_map(|value| {
        let source = value.launch.as_ref()?;
        let ids: Vec<_> = source.atoms.iter().flatten().copied().collect();
        if ids.len() != input.plan.start_atoms.len()
            || !input.plan.start_atoms.iter().all(|id| ids.contains(id))
            || source.atoms.len() != value.frame.numbers.len()
            || value.frame.charge != input.charge
            || value.frame.multiplicity != input.multiplicity
            || value.frame.validate().is_err()
            || science::baseline(&input.document, &ids) != source.baseline
        {
            return None;
        }
        for (index, id) in source.atoms.iter().enumerate() {
            let Some(id) = id else {
                continue;
            };
            let atom = input.document.atom(*id)?;
            let element =
                reshiki::chemistry::ELEMENTS.get(usize::from(*value.frame.numbers.get(index)?))?;
            if atom.element != element.symbol
                || atom.isotope != u32::from(value.frame.isotopes.get(index).copied().unwrap_or(0))
            {
                return None;
            }
        }
        Some(Launch {
            frame: Arc::clone(&value.frame),
            atoms: source.atoms.clone(),
            baseline: source.baseline.clone(),
        })
    });
    let launch = candidates.next()?;
    candidates.next().is_none().then_some(launch)
}

#[derive(Default)]
struct Minima {
    values: BTreeMap<usize, SpeciesValue>,
    failures: BTreeMap<usize, SpeciesFailure>,
    warnings: BTreeMap<usize, String>,
}

impl Minima {
    fn stereo_failed(&self, step: &Step) -> bool {
        step.reactants.iter().chain(&step.products).any(|term| {
            matches!(
                self.failures.get(&term.species),
                Some(SpeciesFailure::Stereo(_))
            )
        })
    }
}

fn endpoint_value(
    request: &Request,
    step: &Step,
    options: StepOptions,
    minima: &Minima,
) -> StepValue {
    let mut value = StepValue::empty(step.arrow, request.method);
    let sum = |terms: &[super::super::plan::Term], free| {
        super::super::plan::sum_energy(terms.iter().copied(), |index| {
            species_energy(minima.values.get(&index)?, free)
        })
    };
    let kcal = qcdata::KCAL_PER_HARTREE;
    if options.reaction_energy {
        value.energy = sum(&step.reactants, false)
            .zip(sum(&step.products, false))
            .map(|(r, p)| (p - r) * kcal);
    }
    if options.free_energy {
        value.gibbs = sum(&step.reactants, true)
            .zip(sum(&step.products, true))
            .map(|(r, p)| (p - r) * kcal);
    }
    let mut reasons: Vec<String> = step
        .reactants
        .iter()
        .map(|term| (term, options.frequencies()))
        .chain(step.products.iter().map(|term| (term, options.free_energy)))
        .filter_map(|(term, needs_frequency)| {
            minima
                .failures
                .get(&term.species)
                .map(SpeciesFailure::reason)
                .or_else(|| {
                    needs_frequency
                        .then(|| minima.warnings.get(&term.species).map(String::as_str))
                        .flatten()
                })
                .map(|reason| {
                    format!(
                        "{}: {reason}",
                        request
                            .plan
                            .species
                            .get(term.species)
                            .map_or("Species", |species| species.label.as_str())
                    )
                })
        })
        .collect();
    reasons.sort();
    reasons.dedup();
    if options.reaction_energy && value.energy.is_none() && reasons.is_empty() {
        reasons.push("Reaction energy is unavailable".into());
    }
    if options.free_energy && value.gibbs.is_none() && reasons.is_empty() {
        reasons.push("Reaction free energy is unavailable".into());
    }
    value.error = (!reasons.is_empty()).then(|| reasons.join("; "));
    value
}

fn calculate(
    request: &Request,
    backend: &mut impl Backend,
    cancel: &Cancel,
    mut emit: impl FnMut(EventKind),
) -> Result<(), String> {
    if !matches!(request.method, XtbMethod::Gfn2Xtb | XtbMethod::GXtb) {
        return Err("Scheme calculations require GFN2-xTB or g-xTB".into());
    }
    if request.options.len() != request.plan.steps.len()
        || request.inputs.len() != request.plan.steps.len()
    {
        return Err("Review every scheme step before calculating".into());
    }
    if request
        .retry
        .is_some_and(|index| index >= request.plan.steps.len())
    {
        return Err("The scheme step to retry is unavailable".into());
    }
    let active: Vec<_> = request
        .plan
        .steps
        .iter()
        .zip(&request.options)
        .zip(&request.inputs)
        .enumerate()
        .filter(|(index, _)| request.retry.is_none_or(|retry| retry == *index))
        .filter(|(_, ((_, options), _))| options.any())
        .map(|(index, ((step, options), input))| (index, step, *options, input))
        .collect();
    let mut needed = BTreeSet::new();
    let mut owners = Vec::new();
    for &(index, step, _, _) in &active {
        for term in step.reactants.iter().chain(&step.products) {
            if needed.insert(term.species) {
                owners.push((term.species, index));
            }
        }
    }
    let frequencies = frequency_species(
        &request.plan,
        &request.options,
        active.iter().map(|(index, _, _, _)| *index),
    );
    let mut minima = Minima::default();
    let mut ready = BTreeSet::new();
    let mut endpoints = BTreeMap::new();
    let mut publish = |minima: &Minima,
                       pending_hessian: Option<usize>,
                       emit: &mut dyn FnMut(EventKind)| {
        for &(index, step, options, _) in &active {
            if ready.contains(&index)
                || !step.reactants.iter().chain(&step.products).all(|term| {
                    minima.values.contains_key(&term.species)
                        || minima.failures.contains_key(&term.species)
                })
            {
                continue;
            }
            let waiting_for_thermo = pending_hessian.is_some_and(|species| {
                (options.frequencies() && step.reactants.iter().any(|t| t.species == species))
                    || (options.free_energy && step.products.iter().any(|t| t.species == species))
            });
            if waiting_for_thermo {
                let electronic = StepOptions {
                    free_energy: false,
                    free_barrier: false,
                    ..options
                };
                let value = endpoint_value(request, step, electronic, minima);
                if value.energy.is_some() {
                    emit(EventKind::Endpoints { index, value });
                }
                continue;
            }
            let value = endpoint_value(request, step, options, minima);
            if options.transition_state() && !minima.stereo_failed(step) {
                emit(EventKind::Endpoints {
                    index,
                    value: value.clone(),
                });
                endpoints.insert(index, value);
            } else {
                emit(EventKind::Step { index, value });
            }
            ready.insert(index);
        }
    };
    // Every required minimum is ready before a slow TS can hold up another
    // step's ground-state calculation. Each shared species is visited once.
    for &(species_index, index) in &owners {
        checked(cancel)?;
        let Some(species) = request.plan.species.get(species_index) else {
            return Err("The scheme refers to a missing species".into());
        };
        let progress = Progress {
            step: index,
            species: Some(species_index),
            label: format!("Optimizing {}", species.label),
            completed_species: minima.values.len(),
            total_species: needed.len(),
            stage: None,
        };
        emit(EventKind::Progress(progress));
        let cached = request
            .cache
            .iter()
            .rev()
            .find(|value| value.key == species.key && value.method == request.method)
            .map(|value| SpeciesValue {
                species: species_index,
                method: value.method,
                frame: Arc::clone(&value.frame),
                result: Arc::clone(&value.result),
                launch: value.launch.clone(),
            });
        let needs_freq = frequencies.contains(&species_index);
        let computed = if let Some(cached) = cached
            .as_ref()
            .filter(|value| !needs_freq || has_thermo(value))
        {
            Ok(SpeciesComputed {
                value: cached.clone(),
                warning: None,
            })
        } else {
            backend.species(
                SpeciesJob {
                    index: species_index,
                    species,
                    method: request.method,
                    frequencies: needs_freq,
                    cached,
                },
                cancel,
                &mut |value| {
                    emit(EventKind::Species {
                        key: species.key.clone(),
                        value: value.clone(),
                    });
                    minima.values.insert(species_index, value.clone());
                    // A Hessian for this minimum can block other steps that
                    // already have all their electronic energies.
                    publish(&minima, Some(species_index), &mut emit);
                },
            )
        };
        match computed {
            Ok(computed) => {
                emit(EventKind::Species {
                    key: species.key.clone(),
                    value: computed.value.clone(),
                });
                minima.values.insert(species_index, computed.value);
                if let Some(warning) = computed.warning {
                    minima.warnings.insert(species_index, warning);
                }
            }
            Err(reason) => {
                checked(cancel)?;
                minima.failures.insert(species_index, reason);
            }
        }
        publish(&minima, None, &mut emit);
    }
    for &(index, step, options, input) in &active {
        if !options.transition_state() || minima.stereo_failed(step) {
            continue;
        }
        checked(cancel)?;
        let mut progress = Progress {
            step: index,
            species: None,
            label: "Preparing step".into(),
            completed_species: minima.values.len(),
            total_species: needed.len(),
            stage: None,
        };
        emit(EventKind::Progress(progress.clone()));
        let Some(mut value) = endpoints.remove(&index) else {
            return Err("The scheme step has no endpoint results".into());
        };
        let sum = |terms: &[super::super::plan::Term], free| {
            super::super::plan::sum_energy(terms.iter().copied(), |index| {
                species_energy(minima.values.get(&index)?, free)
            })
        };
        let reactant = sum(&step.reactants, false);
        let reactant_g = options
            .frequencies()
            .then(|| sum(&step.reactants, true))
            .flatten();
        let kcal = qcdata::KCAL_PER_HARTREE;
        let mut reasons: Vec<_> = value.error.take().into_iter().collect();
        match input {
            Err(reason) => reasons.push(reason.clone()),
            Ok(None) => {
                reasons.push("Review the atom map before searching for a transition state".into())
            }
            Ok(Some(input)) => {
                progress.species = None;
                progress.label = "Finding a transition state".into();
                progress.stage = Some(Stage::Endpoints);
                emit(EventKind::Progress(progress.clone()));
                let result = backend.transition(
                    TransitionJob {
                        input,
                        optimized: optimized_start(input, minima.values.values()),
                        arrow: step.arrow,
                        method: request.method,
                        free: options.free_barrier,
                    },
                    cancel,
                    &mut |event| {
                        match event {
                            job::Event::Stage(stage) => {
                                progress.stage = Some(stage);
                                progress.label = match stage {
                                    Stage::Endpoints => "Building both ends",
                                    Stage::Idpp | Stage::Neb => "Finding a path",
                                    Stage::Saddle => "Refining the transition state",
                                    Stage::Irc => "Checking where it leads",
                                    Stage::Thermochemistry => "Calculating frequencies",
                                }
                                .into();
                            }
                            job::Event::Neb { iteration, .. } => {
                                progress.label = format!("Finding a path · iteration {iteration}")
                            }
                            job::Event::Saddle { step } => {
                                progress.label =
                                    format!("Refining the transition state · step {step}")
                            }
                            _ => return,
                        }
                        emit(EventKind::Progress(progress.clone()));
                    },
                );
                match result {
                    Ok(ts) => {
                        if let Some(discovered) = &ts.discovered {
                            if !discovered.gaps.is_empty() {
                                reasons.push(format!(
                                    "Discovered path has {} unresolved gap{}",
                                    discovered.gaps.len(),
                                    if discovered.gaps.len() == 1 { "" } else { "s" },
                                ));
                            }
                            if options.free_barrier
                                && (discovered.order.iter().any(|&id| {
                                    id > 1
                                        && discovered.minima.get(id).is_none_or(|minimum| {
                                            minimum.gibbs_correction_hartree.is_none()
                                        })
                                }) || discovered
                                    .segments
                                    .iter()
                                    .any(|segment| segment.ts.thermo.is_none()))
                            {
                                reasons.push(
                                    "Some discovered elementary-step free energies are unavailable"
                                        .into(),
                                );
                            }
                        } else if ts.result.status == qcdata::Status::Converged {
                            if options.barrier {
                                value.barrier = reactant
                                    .zip(ts.result.energy_hartree)
                                    .map(|(r, t)| (t - r) * kcal);
                            }
                            if options.free_barrier {
                                value.free_barrier = reactant_g
                                    .zip(ts.result.energy_hartree.zip(ts.result.thermo.as_ref()))
                                    .map(|(r, (t, g))| (t + g.gibbs_correction_hartree - r) * kcal)
                                    .filter(|g| g.is_finite());
                                if value.free_barrier.is_none() {
                                    reasons
                                        .push("Transition-state free energy is unavailable".into());
                                }
                            }
                        } else {
                            let reason = match &ts.result.status {
                                qcdata::Status::NotConverged { reason }
                                | qcdata::Status::Failed { reason } => reason.as_str(),
                                _ => "No reliable transition state found",
                            };
                            reasons.push(reason.into());
                        }
                        value.ts = Some(ts.result);
                        value.discovered = ts.discovered;
                        value.frames = ts.frames;
                        value.launch = Some(ts.launch);
                    }
                    Err(reason) => reasons.push(reason),
                }
            }
        }
        value.error = (!reasons.is_empty()).then(|| reasons.join("; "));
        emit(EventKind::Step { index, value });
        checked(cancel)?;
    }
    checked(cancel)
}

struct Native {
    executable: PathBuf,
    threads: u32,
}

impl Native {
    fn launch(
        &self,
        document: &Document,
        atoms: &[u64],
        charge: i32,
        multiplicity: u32,
        cancel: &Cancel,
    ) -> Result<Launch, String> {
        checked(cancel)?;
        let atom = *atoms.first().ok_or("The calculation has no atoms")?;
        if let Ok(mut launch) = science::launch(document, atom, charge, multiplicity) {
            launch.frame = Arc::new(with_masses(&launch.frame));
            return Ok(launch);
        }
        let mut doc = document.clone();
        let prepared = Prepared::starting(&doc, atoms, Mode::Strict)
            .or_else(|_| Prepared::starting(&doc, atoms, Mode::Relaxed))
            .map_err(|error| error.to_string())?;
        let client = Client::new(self.executable.clone(), Default::default())
            .map_err(|error| error.to_string())?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let field = if prepared.mmff_available() {
            ForceField::Mmff94s
        } else {
            ForceField::Uff
        };
        let generated = runtime.block_on(async {
            tokio::select! {
                result = client.generate(&prepared, field) => result.map_err(|error| error.to_string()),
                _ = async { while !cancel.is_cancelled() { tokio::time::sleep(Duration::from_millis(20)).await; } } => Err("Scheme calculation canceled".into()),
            }
        })?;
        checked(cancel)?;
        let model = prepared
            .model(&generated)
            .map_err(|error| error.to_string())?;
        doc.models
            .retain(|old| !old.linked_atoms.iter().any(|id| atoms.contains(id)));
        doc.models.push(model);
        let mut launch =
            science::launch(&doc, atom, charge, multiplicity).map_err(|error| error.to_string())?;
        launch.frame = Arc::new(with_masses(&launch.frame));
        Ok(launch)
    }

    fn ordinary(
        &self,
        input: &Frame,
        method: XtbMethod,
        optimize: bool,
        frequencies: bool,
        cancel: &Cancel,
    ) -> Result<(Arc<Frame>, Arc<CalcResult>), String> {
        checked(cancel)?;
        let config = state::Config {
            task: if optimize {
                CalcTask::Optimize
            } else {
                CalcTask::Frequencies
            },
            method,
            charge: input.charge,
            unpaired: input
                .multiplicity
                .checked_sub(1)
                .ok_or("Multiplicity must be positive")?,
            solvent: None,
            xtb68: false,
            accuracy: None,
            temperature_k: None,
            max_iterations: None,
            threads: self.threads.max(1),
            tight: true,
            fixed: Vec::new(),
        };
        let mut job = xtb::job(&config, input, Vec::new());
        job.frequencies = frequencies;
        let started = Instant::now();
        let report = driver::run(&self.executable, job, cancel, |_| {})
            .map_err(|error| error.to_string())?;
        checked(cancel)?;
        if report.computed.method.xtb_method() != method {
            return Err("The worker returned a different electronic method".into());
        }
        if !report.converged {
            return Err(format!(
                "Species optimization did not converge after {} steps",
                report.steps
            ));
        }
        let frame = Arc::new(Frame {
            positions_angstrom: report.positions.clone(),
            ..input.clone()
        });
        let mut result = report
            .computed
            .to_calc_result(
                &frame,
                if optimize {
                    qcdata::Task::Optimize
                } else {
                    qcdata::Task::Hessian
                },
                &report.engine,
                None,
            )
            .map_err(|error| error.to_string())?;
        result.input_frame = Some(input.hash());
        if let qcdata::Settings::Xtb(settings) = &mut result.settings {
            settings.threads = Some(config.threads);
            settings.convergence = Some(qcdata::Convergence::Tight);
        }
        result.status = qcdata::Status::Converged;
        result.run = Some(qcdata::RunSummary {
            seconds: started.elapsed().as_secs_f64(),
            scf_iterations: report
                .evaluations
                .iter()
                .map(|value| value.scf_iterations as u32)
                .collect(),
            finished: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            search: None,
            transition_state: None,
        });
        if optimize {
            result.provenance.method = Some(driver::optimizer_provenance(
                result.provenance.method.as_deref().unwrap_or("xTB"),
                &report.optimizers,
            ));
        }
        result.validate(&frame).map_err(|error| error.to_string())?;
        Ok((frame, Arc::new(result)))
    }

    fn compute_species(
        &self,
        job: SpeciesJob<'_>,
        cancel: &Cancel,
        minimum: &mut dyn FnMut(&SpeciesValue),
        optimize: impl FnOnce(&Frame) -> Result<(Arc<Frame>, Arc<CalcResult>), String>,
    ) -> Result<SpeciesComputed, SpeciesFailure> {
        let SpeciesJob {
            index,
            species,
            method,
            frequencies,
            cached,
        } = job;
        let mut value = if let Some(cached) = cached {
            cached
        } else {
            let atoms: Vec<_> = species
                .calculation
                .atoms
                .iter()
                .map(|atom| atom.id)
                .collect();
            let launch = self.launch(
                &species.calculation,
                &atoms,
                species.charge,
                species.multiplicity,
                cancel,
            )?;
            let (frame, result) = optimize(&launch.frame)?;
            check_species_stereo(&species.calculation, &launch.atoms, &frame)
                .map_err(SpeciesFailure::Stereo)?;
            SpeciesValue {
                species: index,
                method,
                frame,
                result,
                launch: Some(launch),
            }
        };
        let mut warning = None;
        if frequencies && !has_thermo(&value) {
            minimum(&value);
            match self.ordinary(&value.frame, method, false, true, cancel) {
                Ok((_, result)) => {
                    let mut combined = (*result).clone();
                    // Frequencies analyze the exact cached minimum. Retain the
                    // optimization's energy and provenance as the shared input.
                    combined.task = qcdata::Task::Optimize;
                    combined.energy_hartree = value.result.energy_hartree;
                    combined.provenance = value.result.provenance.clone();
                    combined.input_frame = value.result.input_frame;
                    value.result = Arc::new(combined);
                    if !has_thermo(&value) {
                        let mut electronic = (*value.result).clone();
                        electronic.thermo = None;
                        value.result = Arc::new(electronic);
                        warning = Some("Free energy unavailable: the frequency check did not confirm a minimum".into());
                    }
                }
                Err(reason) => {
                    checked(cancel)?;
                    warning = Some(format!("Free energy unavailable: {reason}"));
                }
            }
        }
        Ok(SpeciesComputed { value, warning })
    }
}

fn with_masses(frame: &Frame) -> Frame {
    let mut frame = frame.clone();
    if frame.masses_amu.is_empty() {
        frame.masses_amu = science::isotope_masses(&frame).unwrap_or_default();
    }
    frame
}

impl Backend for Native {
    fn species(
        &mut self,
        job: SpeciesJob<'_>,
        cancel: &Cancel,
        minimum: &mut dyn FnMut(&SpeciesValue),
    ) -> Result<SpeciesComputed, SpeciesFailure> {
        let method = job.method;
        self.compute_species(job, cancel, minimum, |frame| {
            self.ordinary(frame, method, true, false, cancel)
        })
    }

    fn transition(
        &mut self,
        job: TransitionJob<'_>,
        cancel: &Cancel,
        progress: &mut dyn FnMut(job::Event),
    ) -> Result<TransitionComputed, String> {
        let TransitionJob {
            input,
            optimized,
            arrow,
            method,
            free,
        } = job;
        let launch = match optimized {
            Some(launch) => launch,
            None => self.launch(
                &input.document,
                &input.plan.start_atoms,
                input.charge,
                input.multiplicity,
                cancel,
            )?,
        };
        let mut bonds = transition::card::BondChanges::from_plan(&input.document, &input.plan);
        bonds.0.automatic = input.automatic;
        let reaction = reshiki::reaction_mapping::reaction(&input.document, arrow)
            .map_err(|error| error.to_string())?;
        let context = qcdata::TransitionStateContext {
            reactant_chemistry: transition::endpoint_chemistry(
                &input.document,
                reaction,
                reshiki::reactions::Role::Reactant,
            ),
            product_chemistry: transition::endpoint_chemistry(
                &input.document,
                reaction,
                reshiki::reactions::Role::Product,
            ),
            title: "Reaction scheme step".into(),
            reactant: "Reactant".into(),
            endpoint_complex: Some(input.plan.shape != (1, 1)),
        };
        let request = transition::Request {
            method,
            arrow,
            free_energy: free,
            bonds,
            context,
            plan: input.plan.clone(),
            images: input.images,
            retry: None,
        };
        let drawn = input
            .document
            .bonds
            .iter()
            .filter(|bond| {
                bond.order > 0
                    && input.plan.start_atoms.contains(&bond.a)
                    && input.plan.start_atoms.contains(&bond.b)
            })
            .map(|bond| (bond.a, bond.b))
            .collect();
        let stream = job::start_scheme(
            launch.clone(),
            request.clone(),
            drawn,
            job::SearchPath::New(input.images),
            cancel.clone(),
        );
        iced::futures::executor::block_on(async {
            let mut stream = Box::pin(stream);
            while let Some(event) = stream.next().await {
                match event {
                    job::Event::Done(result) => {
                        let report = result.map_err(|failure| failure.to_string())?;
                        let result = transition_result(&report, &request, false);
                        let template = report
                            .frames
                            .first()
                            .ok_or("The transition-state worker returned no geometry")?;
                        let discovered = report.discovered.as_ref().map(|path| DiscoveredPath {
                            minima: path
                                .minima
                                .iter()
                                .zip(&path.minimum_gibbs_corrections_hartree)
                                .map(|(minimum, &gibbs_correction_hartree)| DiscoveredMinimum {
                                    frame: Arc::new(Frame {
                                        positions_angstrom: minimum
                                            .positions
                                            .as_chunks::<3>()
                                            .0
                                            .iter()
                                            .map(|xyz| {
                                                xyz.map(|x| x * reshiki_optimize::ANGSTROM_PER_BOHR)
                                            })
                                            .collect(),
                                        ..template.clone()
                                    }),
                                    energy_hartree: minimum.energy,
                                    gibbs_correction_hartree,
                                })
                                .collect(),
                            order: path.order.clone(),
                            segments: path
                                .edges
                                .iter()
                                .map(|edge| DiscoveredSegment {
                                    reactant: edge.reactant,
                                    product: edge.product,
                                    ts: Arc::new(transition_result(&edge.report, &request, true)),
                                    frames: edge
                                        .report
                                        .frames
                                        .iter()
                                        .cloned()
                                        .map(Arc::new)
                                        .collect(),
                                })
                                .collect(),
                            gaps: path
                                .gaps
                                .iter()
                                .map(|gap| DiscoveredGap {
                                    reactant: gap.reactant,
                                    product: gap.product,
                                    reason: gap.reason.clone(),
                                })
                                .collect(),
                            diagnostics: path.diagnostics.clone(),
                        });
                        return Ok(TransitionComputed {
                            result: Arc::new(result),
                            frames: report.frames.iter().cloned().map(Arc::new).collect(),
                            launch,
                            discovered,
                        });
                    }
                    event => progress(event),
                }
            }
            Err("The transition-state worker ended before returning a result".into())
        })
    }
}

/// A local saddle retains its actual endpoint frames and IRC, without claiming
/// that it connects the original drawing arrow's complete reaction.
fn transition_result(
    report: &job::Report,
    request: &transition::Request,
    local: bool,
) -> CalcResult {
    let mut result = report.result.clone();
    let extras = &report.extras;
    if let Some(run) = &mut result.run {
        run.transition_state = Some(Box::new(qcdata::TransitionStateRun {
            method: request.method,
            refined_from: None,
            arrow: (!local).then_some(request.arrow),
            free_energy: request.free_energy,
            endpoint_energies_hartree: Some(extras.endpoint_energies_hartree),
            endpoint_thermo: extras.endpoint_thermo.clone(),
            reached_positions_angstrom: extras.reached_positions_angstrom.clone(),
            saddle_converged: extras.saddle_converged,
            imaginary: extras.num_imaginary,
            imaginary_cm1: extras.engine_imaginary,
            climbing_image: extras.climbing,
            irc: extras.irc.clone(),
            forming_bonds: extras.forming.clone(),
            breaking_bonds: extras.breaking.clone(),
            mapping: if local {
                qcdata::TransitionStateMapping {
                    formed: Vec::new(),
                    broken: Vec::new(),
                    hydrogen_moves: Vec::new(),
                    automatic: true,
                }
            } else {
                request.bonds.0.clone()
            },
            diagnostics: {
                let mut rows = extras.diagnostics.clone();
                if request.free_energy {
                    rows.push((
                        "Free-energy convention".into(),
                        if local {
                            "Thermal corrections use the independently checked saddle and the exact certified minimum geometries."
                        } else {
                            "Barrier uses separately optimized reactant species; endpoint-complex frequencies were not calculated."
                        }.into(),
                    ));
                }
                rows
            },
            context: (!local).then(|| request.context.clone()),
        }));
    }
    result
}

#[cfg(test)]
mod tests;
