//! Gas-phase transition-state searches using the isolated xTB worker.

mod geometry;
mod output;
#[cfg(all(test, feature = "xtb"))]
pub(super) mod tests;
#[cfg(feature = "xtb")]
mod worker;

#[cfg(feature = "xtb")]
use crate::cli::{args::Ts, calc::Geometry, host::CliError};
#[cfg(feature = "xtb")]
use reshiki_optimize::{
    ANGSTROM_PER_BOHR,
    ts::{self, Event, Search, Stage, TsOptions},
};
#[cfg(feature = "xtb")]
use reshiki_xtb_worker::{
    client::{Cancel, Limits},
    protocol::{Conventions, Method, Open, Units},
};
#[cfg(feature = "xtb")]
use std::{io::Write, ops::ControlFlow, time::Instant};
#[cfg(feature = "xtb")]
use worker::Worker;

#[cfg(feature = "xtb")]
pub(super) fn run(command: Ts, out: &mut dyn Write, err: &mut dyn Write) -> Result<bool, CliError> {
    output::check_destinations(&command).map_err(CliError::Failed)?;
    let (reactant, product) = geometry::load(&command).map_err(CliError::Failed)?;
    let start = reactant
        .as_ref()
        .or(product.as_ref())
        .ok_or_else(|| CliError::Failed("no XYZ endpoint".into()))?;
    let masses: Vec<_> = start
        .numbers
        .iter()
        .map(|&z| {
            crate::chemistry::ELEMENTS
                .get(usize::from(z))
                .map(|element| element.average_mass_amu())
                .ok_or_else(|| CliError::Failed(format!("unknown element Z={z}")))
        })
        .collect::<Result<_, _>>()?;
    let executable = std::env::current_exe().map_err(|error| {
        CliError::Failed(format!("cannot locate the xTB worker executable: {error}"))
    })?;
    let limits = Limits::default();
    let open = Open {
        numbers: start.numbers.clone(),
        positions: start.positions.clone(),
        units: Units::Angstrom,
        charge: command.charge,
        uhf: command.uhf,
        method: Method::Gfn2,
        conventions: Conventions::Axtb,
        solvent: None,
        scf: Default::default(),
        gfnff: None,
        threads: 1,
        heap_bytes: limits.heap_bytes,
    };
    let started = Instant::now();
    let mut summary = (|| {
        let (first, engine, uhf) =
            Worker::start(executable.clone(), open.clone(), limits, Cancel::default())
                .map_err(failed)?;
        let mut workers = vec![first];
        for _ in 1..command.workers {
            let (worker, _, _) =
                Worker::start(executable.clone(), open.clone(), limits, Cancel::default())
                    .map_err(failed)?;
            workers.push(worker);
        }
        let first = workers
            .first_mut()
            .ok_or_else(|| CliError::Failed("no xTB worker".into()))?;
        let Endpoints {
            reactant,
            mut product,
            reactive_start,
        } = match (&reactant, &product) {
            (Some(r), Some(p)) => Endpoints {
                reactant: bohr(r),
                product: bohr(p),
                reactive_start: None,
            },
            (_, from_product) => {
                build_endpoint(first, start, &command, from_product.is_some(), err)?
            }
        };
        let moving_hydrogens = if command.remap_hydrogens {
            let moved = geometry::remap(&open.numbers, &reactant, &mut product)
                .map_err(CliError::Failed)?;
            review_hydrogens(&moved, &open.numbers, err);
            moved
        } else {
            Vec::new()
        };
        let options = TsOptions {
            images: command.images,
            optimize_endpoints: !command.keep_endpoints,
            ..TsOptions::default()
        };
        let search = Search {
            numbers: &open.numbers,
            masses_amu: &masses,
            reactant: &reactant,
            product: &product,
            options: &options,
        };
        let mut progress = Progress::default();
        let report =
            ts::search(&mut workers, &search, |event| progress.show(event, err)).map_err(failed)?;
        let _ = writeln!(err, "Checking TS frequencies with the xTB worker");
        let worker_check = workers
            .first_mut()
            .ok_or_else(|| CliError::Failed("no xTB worker".into()))?
            .check_hessian(&report.ts.positions, &masses)
            .map_err(|error| ts::TsError::from(error).to_string());
        Ok::<_, CliError>(output::Summary {
            command: &command,
            numbers: &open.numbers,
            masses: &masses,
            report,
            engine,
            uhf,
            worker_check,
            iterations: progress.iterations,
            steps: progress.steps,
            evaluations: workers.iter().map(|worker| worker.gradients).sum(),
            seconds: 0.,
            moving_hydrogens,
            reactive_start,
        })
    })()?;
    summary.seconds = started.elapsed().as_secs_f64();
    finish(&summary, out, err)
}

#[cfg(feature = "xtb")]
fn finish(
    summary: &output::Summary<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<bool, CliError> {
    summary.write_table(out)?;
    summary.write_files().map_err(CliError::Failed)?;
    for diagnostic in &summary.report.saddle_diagnostics {
        let _ = writeln!(err, "Saddle refinement: {diagnostic}");
    }
    if let Some(reason) = summary.failure_reason() {
        out.flush().map_err(CliError::Stdout)?;
        let _ = writeln!(err, "reshiki: {reason}");
        return Ok(false);
    }
    Ok(true)
}

#[cfg(feature = "xtb")]
fn failed(error: impl Into<ts::TsError>) -> CliError {
    CliError::Failed(error.into().to_string())
}

#[cfg(feature = "xtb")]
fn bohr(geometry: &Geometry) -> Vec<f64> {
    geometry
        .positions
        .as_flattened()
        .iter()
        .map(|x| x / ANGSTROM_PER_BOHR)
        .collect()
}

#[cfg(feature = "xtb")]
struct Endpoints {
    reactant: Vec<f64>,
    product: Vec<f64>,
    reactive_start: Option<ts::ReactiveStart>,
}

#[cfg(feature = "xtb")]
fn build_endpoint(
    worker: &mut Worker,
    start: &Geometry,
    command: &Ts,
    from_product: bool,
    err: &mut dyn Write,
) -> Result<Endpoints, CliError> {
    let indices = |pairs: &[(usize, usize)]| {
        pairs
            .iter()
            .map(|&(i, j)| (i - 1, j - 1))
            .collect::<Vec<_>>()
    };
    let form = indices(&command.form);
    let break_ = indices(&command.break_);
    let (name, form, break_) = if from_product {
        ("reactant", &break_, &form)
    } else {
        ("product", &form, &break_)
    };
    let _ = writeln!(err, "Building the {name} endpoint");
    let start_xyz = bohr(start);
    let (start_xyz, reactive_start) = if command.keep_endpoints {
        (start_xyz, None)
    } else {
        ts::prepare_reactive_start(worker, &start.numbers, &start_xyz, form, break_, |_, _| {
            ControlFlow::Continue(())
        })
        .map_err(failed)?
    };
    if let Some(fold) = &reactive_start {
        let detail = fold.description(|i| {
            format!(
                "{}{}",
                start.numbers.get(i).copied().map_or("?", geometry::symbol),
                i + 1
            )
        });
        let _ = writeln!(err, "Folded the starting conformer: {detail}");
    }
    let (built, matched) =
        ts::build_endpoint(worker, &start.numbers, &start_xyz, form, break_, |_, _| {
            ControlFlow::Continue(())
        })
        .map_err(failed)?;
    if !matched {
        return Err(CliError::Failed(format!(
            "the constructed {name} does not match the requested bond changes"
        )));
    }
    let (reactant, product) = if from_product {
        (built.positions, start_xyz)
    } else {
        (start_xyz, built.positions)
    };
    Ok(Endpoints {
        reactant,
        product,
        reactive_start,
    })
}

#[cfg(feature = "xtb")]
fn review_hydrogens(moved: &[geometry::MovingHydrogen], numbers: &[u8], err: &mut dyn Write) {
    if moved.is_empty() {
        let _ = writeln!(err, "Moving hydrogens: none");
        return;
    }
    let label = |index: usize| {
        let element = numbers
            .get(index - 1)
            .copied()
            .map_or("?", geometry::symbol);
        format!("{element}{index}")
    };
    let _ = writeln!(err, "Moving hydrogens:");
    for change in moved {
        let from = label(change.reactant_heavy_atom);
        let to = label(change.product_heavy_atom);
        let _ = writeln!(err, "  H{}: {from} → {to}", change.hydrogen);
    }
}

#[cfg(feature = "xtb")]
struct Progress {
    iterations: usize,
    steps: usize,
    trace: bool,
}

#[cfg(feature = "xtb")]
fn trace_enabled(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true"))
}

#[cfg(feature = "xtb")]
impl Default for Progress {
    fn default() -> Self {
        Self {
            iterations: 0,
            steps: 0,
            trace: trace_enabled(std::env::var("RESHIKI_TS_TRACE").as_deref().ok()),
        }
    }
}

#[cfg(feature = "xtb")]
impl Progress {
    fn show(&mut self, event: Event, err: &mut dyn Write) -> ControlFlow<()> {
        match event {
            Event::Stage(stage) => {
                let label = match stage {
                    Stage::Endpoints => "Preparing endpoints",
                    Stage::Interpolation => "Interpolating the path",
                    Stage::Neb => "Running climbing-image NEB",
                    Stage::Saddle => "Refining the saddle",
                    Stage::Frequencies => "Analyzing TS frequencies",
                    Stage::Irc => "Tracing both IRC branches",
                };
                let _ = writeln!(err, "{label}");
            }
            Event::Neb {
                iteration,
                max_force,
                climbing,
                energies,
            } => {
                if self.trace {
                    let peak = energies.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    let climbing_energy = climbing
                        .and_then(|index| energies.get(index))
                        .map_or_else(|| "pending".into(), |value| format!("{value:.12}"));
                    let _ = writeln!(
                        err,
                        "NEB trace: iteration={iteration} band_peak_energy={peak:.12} climbing_image_energy={climbing_energy}"
                    );
                }
                if iteration > 0 && iteration.is_multiple_of(10) {
                    let climbing =
                        climbing.map_or_else(|| "pending".into(), |image| (image + 1).to_string());
                    let _ = writeln!(
                        err,
                        "NEB {iteration}: max force {max_force:.4} Eh/bohr, climbing image {climbing}"
                    );
                }
                self.iterations = iteration;
            }
            Event::Saddle {
                step,
                energy,
                max_gradient,
                rms_gradient,
                eigenvalue,
                overlap,
                radius,
                mode_displacement,
                exact,
            } => {
                if self.trace {
                    let displacement = mode_displacement
                        .map_or_else(|| "unchecked".into(), |value| format!("{value:.8e}"));
                    let _ = writeln!(
                        err,
                        "Saddle metrics: step={step} energy={energy:.12} max_gradient={max_gradient:.8e} rms_gradient={rms_gradient:.8e} eigenvalue={eigenvalue:.8e} overlap={overlap:.6} radius={radius:.8} mode_displacement={displacement} exact={exact}"
                    );
                }
                if step > self.steps && step.is_multiple_of(10) {
                    let _ = writeln!(err, "Saddle {step}: max gradient {max_gradient:.4} Eh/bohr");
                }
                self.steps = step;
            }
            Event::SaddleTrial {
                energy,
                max_gradient,
                rms_gradient,
                length,
                energy_ratio,
                force_ratio,
                decision,
                ..
            } => {
                if self.trace {
                    let _ = writeln!(
                        err,
                        "Saddle trial: energy={energy:.12} max_gradient={max_gradient:.8e} rms_gradient={rms_gradient:.8e} length={length:.8} energy_ratio={energy_ratio:.6} force_ratio={force_ratio:.6} decision={decision}"
                    );
                }
            }
            Event::SaddleStatus(status) => {
                if self.trace {
                    let _ = writeln!(err, "Saddle status: {status}");
                }
            }
            Event::Endpoint { .. } | Event::Irc { .. } => {}
        }
        ControlFlow::Continue(())
    }
}
