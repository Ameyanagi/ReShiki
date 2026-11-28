//! Scheme-only discovery on the geometries evaluated by the native TS search.

use super::*;
use reshiki_optimize::{
    Accuracy, Convergence, InitialHessian, Level, Options, Potential, Status, optimize,
    ts::discovery,
};
use std::cell::RefCell;

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct Discovered {
    pub minima: Vec<ts::Minimum>,
    pub minimum_gibbs_corrections_hartree: Vec<Option<f64>>,
    pub order: Vec<usize>,
    pub edges: Vec<DiscoveredEdge>,
    pub gaps: Vec<DiscoveredGap>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct DiscoveredEdge {
    pub reactant: usize,
    pub product: usize,
    pub report: Arc<Report>,
}

#[derive(Clone, Debug)]
pub(in crate::app::xtb::ts) struct DiscoveredGap {
    pub reactant: usize,
    pub product: usize,
    pub reason: String,
}

#[derive(Clone)]
struct Analysis {
    checked: Result<Arc<Computed>, ts::TsError>,
    trace: SearchTrace,
    evaluations: Vec<Evaluation>,
    seconds: f64,
}

/// Retain the ordinary initial report for existing TS displays, and attach a
/// separate, ordered path only when the calculation found additional minima.
pub(super) fn complete_scheme(
    launch: &Launch,
    request: &super::super::Request,
    done: Completion,
    workers: &mut [Worker],
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
) -> Result<Report, Failure> {
    let Completion {
        report,
        checked,
        endpoint_thermo: _,
        method,
        engine,
        changes,
        evaluations,
        masses,
        seconds,
        trace,
    } = done;
    let initial = Analysis {
        checked: checked.map(Arc::new),
        trace,
        evaluations,
        seconds,
    };
    let complete = |report: TsReport, analysis: Analysis, changes: Changes| {
        complete_with_thermo(
            launch,
            Completion {
                report,
                checked: analysis.checked,
                endpoint_thermo: None,
                method,
                engine: engine.clone(),
                changes,
                evaluations: analysis.evaluations,
                masses: masses.clone(),
                seconds: analysis.seconds,
                trace: analysis.trace,
            },
            request.free_energy,
        )
    };
    let mut original = complete(report.clone(), initial.clone(), changes)?;
    let mut backend = Native {
        workers,
        frame: &launch.frame,
        numbers: &launch.frame.numbers,
        masses: &masses,
        method,
        images: request.images,
        cancel,
        tx,
        free: request.free_energy,
        minimum_thermo: Vec::new(),
    };
    let found = discovery::discover(
        &launch.frame.numbers,
        (report, initial),
        &discovery::Options::default(),
        &mut backend,
    )
    .map_err(failed)?;
    if found.minima.len() <= 2 {
        if found.error.is_none()
            && !found.cancelled
            && found.gaps.is_empty()
            && let Some(edge) = found
                .edges
                .iter()
                .find(|edge| edge.reactant == 0 && edge.product == 1)
            && let Some(attempt) = found.attempts.get(edge.attempt)
        {
            // Use the planner's observed endpoint correction, then apply the
            // same independent worker-spectrum acceptance as every other TS.
            let changes = Changes {
                form: original
                    .extras
                    .forming
                    .iter()
                    .map(|&[a, b]| (a, b))
                    .collect(),
                break_: original
                    .extras
                    .breaking
                    .iter()
                    .map(|&[a, b]| (a, b))
                    .collect(),
            };
            original = complete(
                attempt.report.clone(),
                attempt.verification.clone(),
                changes,
            )?;
        }
        apply_discovery_verification(&mut original, &found);
        add_diagnostics(&mut original, &found.diagnostics);
        return Ok(original);
    }
    apply_discovery_verification(&mut original, &found);
    let minimum_gibbs_corrections_hartree = found
        .minima
        .iter()
        .map(|minimum| {
            backend
                .minimum_thermo
                .iter()
                .find_map(|(positions, correction)| {
                    (positions == &minimum.positions).then_some(*correction)
                })
        })
        .collect();
    let mut attempts: Vec<_> = found.attempts.into_iter().map(Some).collect();
    let mut diagnostics = found.diagnostics;
    let mut edges = Vec::with_capacity(found.edges.len());
    for edge in found.edges {
        let Some(attempt) = attempts.get_mut(edge.attempt).and_then(Option::take) else {
            return Err(Failure::Other(
                "A discovered transition state has no calculation".into(),
            ));
        };
        let changes = local_changes(&launch.frame.numbers, &attempt.report)?;
        let mut local = complete(attempt.report, attempt.verification, changes)?;
        local.extras.diagnostics.push((
            "Discovered elementary step".into(),
            format!(
                "Mapped minima {} → {}; independently checked saddle and both relaxed IRC ends",
                edge.reactant + 1,
                edge.product + 1
            ),
        ));
        edges.push(DiscoveredEdge {
            reactant: edge.reactant,
            product: edge.product,
            report: Arc::new(local),
        });
    }
    for (index, attempt) in attempts.into_iter().enumerate() {
        let Some(attempt) = attempt else { continue };
        // A gap keeps the characterized failure even though it cannot be
        // published as an elementary-step saddle.
        match complete(
            attempt.report,
            attempt.verification,
            Changes {
                form: Vec::new(),
                break_: Vec::new(),
            },
        ) {
            Ok(report) => diagnostics.extend(
                report
                    .extras
                    .diagnostics
                    .into_iter()
                    .map(|(label, detail)| format!("Search {} · {label}: {detail}", index + 1)),
            ),
            Err(error) => diagnostics.push(format!("Search {}: {error}", index + 1)),
        }
    }
    add_diagnostics(&mut original, &diagnostics);
    original.discovered = Some(Discovered {
        minima: found.minima,
        minimum_gibbs_corrections_hartree,
        order: found.order,
        edges,
        gaps: found
            .gaps
            .into_iter()
            .map(|gap| DiscoveredGap {
                reactant: gap.reactant,
                product: gap.product,
                reason: gap.reason,
            })
            .collect(),
        diagnostics,
    });
    Ok(original)
}

fn apply_discovery_verification<T>(original: &mut Report, found: &discovery::Report<T>) {
    for (index, gap) in found.gaps.iter().enumerate() {
        original.extras.diagnostics.push((
            format!("Path discovery gap {}", index + 1),
            gap.reason.clone(),
        ));
    }
    let reason = if let Some(error) = &found.error {
        original
            .extras
            .diagnostics
            .push(("Path discovery stopped".into(), error.to_string()));
        Some(format!("Path discovery stopped: {error}"))
    } else if found.cancelled {
        Some("Path discovery canceled".into())
    } else if found.minima.len() > 2 {
        // An initial global saddle cannot supply an overall barrier for the
        // independently verified elementary steps of a discovered path.
        Some("The reaction resolves into elementary steps; inspect the discovered path".into())
    } else if !found.gaps.is_empty()
        || !found
            .edges
            .iter()
            .any(|edge| edge.reactant == 0 && edge.product == 1)
    {
        Some(found.gaps.first().map_or_else(
            || "IRC did not verify a connection between the requested minima".into(),
            |gap| format!("The requested minima were not verified: {}", gap.reason),
        ))
    } else {
        None
    };
    if let Some(reason) = reason {
        original.result.status = qcdata::Status::NotConverged { reason };
        if found.minima.len() <= 2 {
            original.extras.connects = false;
        }
    }
}

fn add_diagnostics(report: &mut Report, diagnostics: &[String]) {
    report.extras.diagnostics.extend(
        diagnostics.iter().enumerate().map(|(index, diagnostic)| {
            (format!("Path discovery {}", index + 1), diagnostic.clone())
        }),
    );
}

/// Local bond changes describe each observed pair. The user's global edits
/// were used only to build the initial endpoint and are never replayed here.
fn local_changes(numbers: &[u8], report: &TsReport) -> Result<Changes, Failure> {
    let reactant: BTreeSet<_> = ts::bonds(numbers, &report.reactant.positions)
        .map_err(failed)?
        .into_iter()
        .collect();
    let product: BTreeSet<_> = ts::bonds(numbers, &report.product.positions)
        .map_err(failed)?
        .into_iter()
        .collect();
    Ok(Changes {
        form: product.difference(&reactant).copied().collect(),
        break_: reactant.difference(&product).copied().collect(),
    })
}

struct Native<'a> {
    workers: &'a mut [Worker],
    frame: &'a Frame,
    numbers: &'a [u8],
    masses: &'a [f64],
    method: qcdata::XtbMethod,
    images: usize,
    cancel: &'a Cancel,
    tx: &'a mpsc::UnboundedSender<Event>,
    free: bool,
    minimum_thermo: Vec<(Vec<f64>, f64)>,
}

struct MinimumCheck {
    resolved: bool,
    gradient_norm: f64,
    residual_rmsd_bohr: Option<f64>,
}

impl discovery::Backend for Native<'_> {
    type Verification = Analysis;

    fn relax(&mut self, seed: &ts::Minimum) -> Result<Option<ts::Minimum>, ts::TsError> {
        let outcome = self.minimize(seed, &Options::default())?;
        if outcome.status != Status::Converged {
            return Ok(None);
        }
        let optimized = ts::Minimum {
            positions: outcome.positions,
            energy: outcome.energy,
        };
        let checked = self
            .workers
            .first_mut()
            .ok_or_else(|| ts::TsError::Failed("No xTB worker".into()))?
            .checked_hessian(&optimized.positions, self.masses)
            .map_err(ts::TsError::from)
            .and_then(|computed| checked_method(computed, self.method))?;
        if control(self.cancel, self.tx).is_break() {
            return Err(ts::TsError::Cancelled);
        }
        let is_minimum = self.resolved_minimum(&optimized, &checked)?.resolved;
        if is_minimum {
            self.retain_minimum_thermochemistry(&optimized, &checked);
        }
        Ok(is_minimum.then_some(optimized))
    }

    fn canonicalize(&mut self, seed: &ts::Minimum) -> Result<Option<ts::Minimum>, ts::TsError> {
        let mut current = seed.clone();
        let mut remaining_steps = 100;
        // Each pass tightens its force target relative to the measured force.
        // Acceptance always uses a fresh target Hessian and geometric residual,
        // including the final budget-limited point; no universal force floor
        // substitutes for resolved geometry.
        for pass in 0..=8 {
            if control(self.cancel, self.tx).is_break() {
                return Err(ts::TsError::Cancelled);
            }
            let checked = self.workers[0]
                .checked_hessian(&current.positions, self.masses)
                .map_err(ts::TsError::from)
                .and_then(|computed| checked_method(computed, self.method))?;
            let measurement = self.resolved_minimum(&current, &checked)?;
            if measurement.resolved {
                self.retain_minimum_thermochemistry(&current, &checked);
                return Ok(Some(current));
            }
            let Some(residual) = measurement.residual_rmsd_bohr else {
                return Ok(None);
            };
            if remaining_steps == 0 || pass == 8 || measurement.gradient_norm == 0. {
                return Ok(None);
            }
            let force_reduction =
                (discovery::minimum_resolution_rmsd_bohr() / residual).min(0.5) * 0.5;
            let outcome = self.minimize(
                &current,
                &Options {
                    level: Level::Tight,
                    initial_hessian: InitialHessian::Potential,
                    convergence: Convergence::EnergyGradient {
                        gradient_norm: measurement.gradient_norm * force_reduction,
                    },
                    max_steps: remaining_steps,
                    ..Options::default()
                },
            )?;
            if outcome.steps == 0 {
                return Ok(None);
            }
            remaining_steps -= outcome.steps;
            current = ts::Minimum {
                positions: outcome.positions,
                energy: outcome.energy,
            };
        }
        Ok(None)
    }

    fn search(
        &mut self,
        reactant: &ts::Minimum,
        product: &ts::Minimum,
    ) -> Result<(TsReport, Analysis), ts::TsError> {
        let started = Instant::now();
        let offsets: Vec<_> = self
            .workers
            .iter()
            .map(|worker| worker.evaluations.len())
            .collect();
        let trace = RefCell::new(SearchTrace::default());
        let mut last_band = None;
        let observe = |event: ts::Event<'_>| {
            trace.borrow_mut().observe(event);
            if let Some(event) = progress(event, &mut last_band, Instant::now)
                && send(self.tx, self.cancel, event).is_err()
            {
                return ControlFlow::Break(());
            }
            control(self.cancel, self.tx)
        };
        let options = ts::TsOptions {
            images: self.images,
            optimize_endpoints: false,
            preserve_endpoint_identity: true,
            ..ts::TsOptions::default()
        };
        let search = ts::Search {
            numbers: self.numbers,
            masses_amu: self.masses,
            reactant: &reactant.positions,
            product: &product.positions,
            options: &options,
        };
        let check = WorkerCheck {
            masses: self.masses,
            method: self.method,
            cancel: self.cancel,
            tx: self.tx,
            trace: &trace,
        };
        let (report, checked) = ts::search_with_verification(
            self.workers,
            &search,
            observe,
            || control(self.cancel, self.tx),
            |worker, positions, imaginary| check.verify(worker, positions, imaginary),
        )?;
        let mut trace = trace.into_inner();
        trace.finish();
        let evaluations = self
            .workers
            .iter()
            .zip(offsets)
            .flat_map(|(worker, offset)| {
                worker.evaluations.iter().skip(offset).map(|e| Evaluation {
                    energy: e.energy,
                    scf_iterations: e.scf_iterations,
                    seconds: e.seconds,
                })
            })
            .collect();
        Ok((
            report,
            Analysis {
                checked: checked.map(Arc::new),
                trace,
                evaluations,
                seconds: started.elapsed().as_secs_f64(),
            },
        ))
    }

    fn on_event(&mut self, event: discovery::Event) -> ControlFlow<()> {
        if let discovery::Event::Relaxing { .. } = event
            && send(self.tx, self.cancel, Event::Stage(Stage::Endpoints)).is_err()
        {
            return ControlFlow::Break(());
        }
        control(self.cancel, self.tx)
    }
}

impl Native<'_> {
    fn retain_minimum_thermochemistry(&mut self, minimum: &ts::Minimum, checked: &Computed) {
        if self.free
            && let Some(correction) =
                minimum_thermochemistry(self.frame, &minimum.positions, self.masses, checked)
        {
            self.minimum_thermo
                .push((minimum.positions.clone(), correction));
        }
    }

    fn resolved_minimum(
        &mut self,
        minimum: &ts::Minimum,
        checked: &Computed,
    ) -> Result<MinimumCheck, ts::TsError> {
        if control(self.cancel, self.tx).is_break() {
            return Err(ts::TsError::Cancelled);
        }
        let mut gradient = vec![0.; minimum.positions.len()];
        self.workers[0].evaluate(&minimum.positions, &mut gradient, Accuracy::Normal)?;
        let residual_rmsd_bohr =
            discovery::minimum_residual_rmsd(self.numbers, minimum, &gradient, &checked.hessian)?;
        let resolved = checked
            .vibrations
            .as_ref()
            .is_some_and(|v| imaginary_count(&v.frequencies_cm1) == 0)
            && residual_rmsd_bohr
                .is_some_and(|residual| residual <= discovery::minimum_resolution_rmsd_bohr());
        Ok(MinimumCheck {
            resolved,
            gradient_norm: gradient
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt(),
            residual_rmsd_bohr,
        })
    }

    fn minimize(
        &mut self,
        seed: &ts::Minimum,
        options: &Options,
    ) -> Result<reshiki_optimize::Outcome, ts::TsError> {
        if control(self.cancel, self.tx).is_break() {
            return Err(ts::TsError::Cancelled);
        }
        let optimized = optimize(
            self.workers
                .first_mut()
                .ok_or_else(|| ts::TsError::Failed("No xTB worker".into()))?,
            self.numbers,
            &seed.positions,
            options,
            |_| control(self.cancel, self.tx),
        );
        match &optimized.status {
            Status::Converged | Status::MaxSteps => Ok(optimized),
            Status::ScfFailure(reason) => Err(ts::TsError::Scf(reason.clone())),
            Status::Failed(reason) => Err(ts::TsError::Failed(reason.clone())),
            Status::Cancelled => Err(ts::TsError::Cancelled),
        }
    }
}

fn minimum_thermochemistry(
    frame: &Frame,
    positions: &[f64],
    masses: &[f64],
    computed: &Computed,
) -> Option<f64> {
    let frequencies = &computed.vibrations.as_ref()?.frequencies_cm1;
    if frequencies
        .iter()
        .any(|&frequency| frequency < -THERMO_IMAGINARY_CM1)
    {
        return None;
    }
    let correction =
        thermochemistry(frame, &angstroms(positions), masses, frequencies).gibbs_correction_hartree;
    correction.is_finite().then_some(correction)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod evidence;
