//! Transition-state table and optional machine-readable outputs.

use reshiki_optimize::{
    ANGSTROM_PER_BOHR,
    ts::{IrcBranch, Minimum, Reached},
};
use std::fmt::Write as _;

use super::geometry::symbol;

fn xyz(text: &mut String, numbers: &[u8], positions: &[f64], comment: &str, annotation: &str) {
    let _ = writeln!(text, "{}\n{comment}{annotation}", numbers.len());
    for (&number, &[x, y, z]) in numbers.iter().zip(positions.as_chunks::<3>().0) {
        let [x, y, z] = [x, y, z].map(|coordinate| coordinate * ANGSTROM_PER_BOHR);
        let _ = writeln!(text, "{:<2} {x:.10} {y:.10} {z:.10}", symbol(number));
    }
}

fn path_xyz(numbers: &[u8], path: &[Minimum], annotation: &str) -> String {
    let mut text = String::new();
    for (i, point) in path.iter().enumerate() {
        let comment = format!("image {} E={:.8} Eh", i + 1, point.energy);
        xyz(&mut text, numbers, &point.positions, &comment, annotation);
    }
    text
}

fn irc_xyz(
    numbers: &[u8],
    ts: &Minimum,
    branches: Option<&[IrcBranch; 2]>,
    annotation: &str,
) -> String {
    let mut text = String::new();
    let Some([plus, minus]) = branches else {
        xyz(
            &mut text,
            numbers,
            &ts.positions,
            &format!("TS E={:.8} Eh", ts.energy),
            annotation,
        );
        return text;
    };
    let ((left, left_mode), (right, right_mode)) =
        if (plus.reached, minus.reached) == (Reached::Reactant, Reached::Product) {
            ((plus, "+mode"), (minus, "-mode"))
        } else {
            ((minus, "-mode"), (plus, "+mode"))
        };
    let mut frame = |point: &Minimum, label| {
        xyz(
            &mut text,
            numbers,
            &point.positions,
            &format!("{label} E={:.8} Eh", point.energy),
            annotation,
        );
    };
    frame(&left.end, left_mode);
    for point in left.path.iter().rev() {
        frame(point, left_mode);
    }
    frame(ts, "TS");
    for point in &right.path {
        frame(point, right_mode);
    }
    frame(&right.end, right_mode);
    text
}

#[cfg(feature = "xtb")]
mod report {
    use super::*;
    use crate::cli::{args::Ts, host::CliError, io};
    use reshiki_io::{qc_input::write_orca_hess, qcdata::Frame};
    use reshiki_optimize::ts::{IMAGINARY_CM1, ReactiveStart, SaddleReason, TsReport};
    use reshiki_xtb_worker::protocol::{Engine, Method};
    use serde_json::json;
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
    };

    use crate::cli::ts::{geometry::MovingHydrogen, worker::HessianAnalysis};

    const OUTPUTS: [&str; 7] = [
        "reactant.xyz",
        "product.xyz",
        "ts.xyz",
        "path.xyz",
        "irc.xyz",
        "ts.hess",
        "report.json",
    ];

    fn resolve_destination(path: &Path) -> std::io::Result<PathBuf> {
        // Resolve existing symlinks and retain missing components, since the
        // output directory and its files may all be new.
        match path.canonicalize() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(parent) = path.parent() else {
                    return Err(error);
                };
                let mut resolved = resolve_destination(parent)?;
                if let Some(name) = path.file_name() {
                    resolved.push(name);
                } else {
                    // `absolute` and `Path` iteration omit redundant dots and
                    // separators. The remaining component is `..`.
                    resolved.pop();
                }
                match resolved.canonicalize() {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(resolved),
                    result => result,
                }
            }
            result => result,
        }
    }

    fn resolve(path: &Path) -> Result<PathBuf, String> {
        std::path::absolute(path)
            .and_then(|path| resolve_destination(&path))
            .map_err(|error| format!("cannot resolve {}: {error}", path.display()))
    }

    pub(in crate::cli::ts) fn check_destinations(command: &Ts) -> Result<(), String> {
        let vacant = |path: &Path| match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
            Ok(_) => Err(format!("{} already exists", path.display())),
        };
        if let Some(dir) = &command.out_dir {
            let targets = OUTPUTS.map(|name| dir.join(name));
            if let Some(json) = &command.json {
                let json = resolve(json)?;
                let report = resolve(&dir.join("report.json"))?;
                for target in
                    std::iter::once(dir.as_path()).chain(targets.iter().map(PathBuf::as_path))
                {
                    if json != resolve(target)? || json == report {
                        continue;
                    }
                    return Err(format!(
                        "--json and --out-dir both target {}",
                        target.display()
                    ));
                }
            }
            for target in targets {
                vacant(&target)?;
            }
        }
        if let Some(path) = &command.json {
            vacant(path)?;
        }
        Ok(())
    }

    pub(in crate::cli::ts) struct Summary<'a> {
        pub command: &'a Ts,
        pub numbers: &'a [u8],
        pub masses: &'a [f64],
        pub report: TsReport,
        pub engine: Engine,
        pub uhf: u32,
        pub worker_check: Result<HessianAnalysis, String>,
        pub iterations: usize,
        pub steps: usize,
        pub evaluations: usize,
        pub seconds: f64,
        pub moving_hydrogens: Vec<MovingHydrogen>,
        pub reactive_start: Option<ReactiveStart>,
        pub source_report: Option<&'a serde_json::Value>,
    }

    fn imaginary(frequencies: &[f64]) -> usize {
        frequencies.iter().filter(|&&f| f < -IMAGINARY_CM1).count()
    }

    fn lowest(frequencies: &[f64]) -> String {
        frequencies
            .first()
            .map_or_else(|| "unavailable".into(), |f| format!("{f:.1} cm-1"))
    }

    fn reached(label: Reached) -> &'static str {
        match label {
            Reached::Reactant => "Reactant",
            Reached::Product => "Product",
            Reached::Other => "another structure",
        }
    }

    fn grouped(value: usize) -> String {
        let digits = value.to_string();
        let mut text = String::new();
        for (i, digit) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                text.push(',');
            }
            text.push(digit);
        }
        text
    }

    fn skipped_irc_reason(
        converged: bool,
        reason: SaddleReason,
        imaginary: usize,
    ) -> Option<String> {
        if !converged || reason != SaddleReason::Stationary {
            Some(format!("saddle refinement did not converge ({reason})"))
        } else if imaginary != 1 {
            Some(format!(
                "expected one imaginary frequency below -{IMAGINARY_CM1} cm-1, found {imaginary}"
            ))
        } else {
            None
        }
    }

    pub(in crate::cli::ts) fn write_retained(
        source: &serde_json::Value,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        if let (Some(reactant), Some(product), Some(ts)) = (
            source
                .pointer("/energies/reactant")
                .and_then(serde_json::Value::as_f64),
            source
                .pointer("/energies/product")
                .and_then(serde_json::Value::as_f64),
            source
                .pointer("/energies/ts")
                .and_then(serde_json::Value::as_f64),
        ) {
            let forward = (ts - reactant) * reshiki_optimize::KCAL_PER_HARTREE;
            let reverse = (ts - product) * reshiki_optimize::KCAL_PER_HARTREE;
            writeln!(out,
                "Retained GFN2-xTB · forward {forward:.2} kcal/mol · reverse {reverse:.2} kcal/mol · one imaginary mode in both checks · IRC connects reactant and product"
            ).map_err(CliError::Stdout)?;
        }
        Ok(())
    }

    impl Summary<'_> {
        fn worker_failure_reason(&self) -> Option<String> {
            match &self.worker_check {
                Err(reason) => Some(format!("worker check failed: {reason}")),
                Ok(analysis) => {
                    let worker = imaginary(&analysis.frequencies_cm1);
                    (worker != self.report.imaginary).then(|| {
                        format!(
                            "engine/worker imaginary frequency counts disagree: engine {}, worker {worker}",
                            self.report.imaginary
                        )
                    })
                }
            }
        }

        pub fn failure_reason(&self) -> Option<String> {
            self.worker_failure_reason()
                .or_else(|| {
                    self.report
                        .irc_error
                        .as_ref()
                        .map(|reason| format!("IRC failed: {reason}"))
                })
                .or_else(|| {
                    skipped_irc_reason(
                        self.report.saddle_converged,
                        self.report.saddle_reason,
                        self.report.imaginary,
                    )
                })
                .or_else(|| {
                    let reached = self
                        .report
                        .irc
                        .as_ref()
                        .map(|[plus, minus]| (plus.reached, minus.reached));
                    (!self.report.connects
                        || !matches!(
                            reached,
                            Some(
                                (Reached::Reactant, Reached::Product)
                                    | (Reached::Product, Reached::Reactant)
                            )
                        ))
                    .then(|| "IRC did not connect the reactant and product".into())
                })
        }

        fn barriers_allowed(&self) -> bool {
            self.command.target_method() != Method::Gxtb || self.failure_reason().is_none()
        }

        pub fn write_table(&self, out: &mut dyn Write) -> Result<(), CliError> {
            let report = &self.report;
            let mut irc = match &report.irc {
                Some(_) if report.connects => "connects reactant and product".into(),
                Some([plus, minus]) => {
                    format!(
                        "+mode reaches {}, -mode reaches {}",
                        reached(plus.reached),
                        reached(minus.reached)
                    )
                }
                None => match &report.irc_error {
                    Some(reason) => format!("failed: {reason}"),
                    None => skipped_irc_reason(
                        report.saddle_converged,
                        report.saddle_reason,
                        report.imaginary,
                    )
                    .map_or_else(|| "skipped".into(), |reason| format!("skipped ({reason})")),
                },
            };
            if let Some(branches) = &report.irc {
                for (branch, mode) in branches.iter().zip(["+mode", "-mode"]) {
                    if !branch.completed {
                        let _ = write!(irc, " · {mode} descent ended before gradient convergence");
                    }
                }
            }
            let worker_check = match &self.worker_check {
                Ok(analysis) => {
                    let mut text = format!(
                        "worker check: {} · {}",
                        imaginary(&analysis.frequencies_cm1),
                        lowest(&analysis.frequencies_cm1)
                    );
                    if let Some(reason) = self.worker_failure_reason() {
                        let _ = write!(text, " · {reason}");
                    }
                    text
                }
                Err(reason) => format!("worker check failed: {reason}"),
            };
            let neb = if report.neb_converged {
                "converged in"
            } else {
                "unconverged after"
            };
            let saddle = if report.saddle_converged {
                "in"
            } else {
                "unconverged after"
            };
            if let Some(source) = self.source_report {
                write_retained(source, out)?;
            }
            let barrier = if self.barriers_allowed() {
                format!(
                    "forward {:.2} kcal/mol · reverse {:.2} kcal/mol",
                    report.forward_barrier_kcal(),
                    report.reverse_barrier_kcal()
                )
            } else {
                "unavailable · g-xTB candidate is unverified".into()
            };
            let path = if self.command.refine.is_some() {
                format!("R / TS / P profile · saddle {saddle} {} steps", self.steps)
            } else {
                format!(
                    "{} images · NEB {neb} {} iterations · saddle {saddle} {} steps",
                    report.path.len(),
                    self.iterations,
                    self.steps
                )
            };
            writeln!(
                out,
                "Transition state · {} · {} {}/{} · charge {} · uhf {}\n\
Reactant      {:.8} Eh\n\
Product       {:.8} Eh\n\
TS            {:.8} Eh\n\
Barrier       {barrier}\n\
Imaginary     {} · {} ({worker_check})\n\
IRC           {irc}\n\
Path          {path}\n\
Work          {} gradients · {} Hessians · {:.1} s",
                self.command.method_label(),
                self.engine.name,
                self.engine.version,
                self.engine.rev,
                self.command.charge,
                self.uhf,
                report.reactant.energy,
                report.product.energy,
                report.ts.energy,
                report.imaginary,
                lowest(&report.frequencies_cm1),
                grouped(self.evaluations),
                grouped(report.hessians + 1),
                self.seconds,
            )
            .map_err(CliError::Stdout)
        }

        pub fn write_files(&self) -> Result<(), String> {
            if let Some(dir) = &self.command.out_dir {
                self.write_structures(dir)?;
                self.write_json(&dir.join("report.json"))?;
            }
            if let Some(path) = &self.command.json {
                let already_written = self
                    .command
                    .out_dir
                    .as_ref()
                    .map(|dir| resolve(&dir.join("report.json")))
                    .transpose()?
                    == Some(resolve(path)?);
                if !already_written {
                    self.write_json(path)?;
                }
            }
            Ok(())
        }

        fn write_json(&self, path: &Path) -> Result<(), String> {
            let mut text = self.json().to_string();
            text.push('\n');
            io::write_output(path, text.as_bytes(), false)
        }

        fn write_structures(&self, dir: &Path) -> Result<(), String> {
            let report = &self.report;
            let frame = Frame {
                numbers: self.numbers.to_vec(),
                positions_angstrom: report
                    .ts
                    .positions
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|row| row.map(|coordinate| coordinate * ANGSTROM_PER_BOHR))
                    .collect(),
                isotopes: Vec::new(),
                masses_amu: self.masses.to_vec(),
                charge: self.command.charge,
                multiplicity: self.uhf + 1,
                drawing_atoms: Vec::new(),
            };
            let hessian = self
                .worker_check
                .as_ref()
                .map_or(&report.hessian, |analysis| &analysis.hessian);
            let hessian = write_orca_hess(&frame, hessian).map_err(|error| error.to_string())?;
            let annotation = format!(
                " method={} verification={}",
                self.command.method_key(),
                if self.failure_reason().is_none() {
                    "verified"
                } else {
                    "unverified-candidate"
                }
            );
            let single = |geometry: &Minimum, comment: String| {
                let mut text = String::new();
                xyz(
                    &mut text,
                    self.numbers,
                    &geometry.positions,
                    &comment,
                    &annotation,
                );
                text
            };
            let imaginary = report
                .frequencies_cm1
                .first()
                .filter(|&&f| f < -IMAGINARY_CM1)
                .map_or_else(|| "none".into(), |f| format!("{f:.1} cm-1"));
            let outputs = [
                single(
                    &report.reactant,
                    format!("Reactant E={:.8} Eh", report.reactant.energy),
                ),
                single(
                    &report.product,
                    format!("Product E={:.8} Eh", report.product.energy),
                ),
                single(
                    &report.ts,
                    format!("TS E={:.8} Eh imaginary={imaginary}", report.ts.energy),
                ),
                path_xyz(self.numbers, &report.path, &annotation),
                irc_xyz(self.numbers, &report.ts, report.irc.as_ref(), &annotation),
                hessian,
            ];
            fs::create_dir_all(dir)
                .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
            for (name, text) in OUTPUTS.into_iter().zip(outputs) {
                io::write_output(&dir.join(name), text.as_bytes(), false)?;
            }
            Ok(())
        }

        fn json(&self) -> serde_json::Value {
            let report = &self.report;
            let method = self.command.method_key();
            let branch = |branch: &IrcBranch| {
                json!({
                    "method": method,
                    "energy_units": "hartree",
                    "reached": reached(branch.reached),
                    "completed": branch.completed,
                    "energies": branch.energies,
                })
            };
            json!({
                "schema_version": 2,
                "verified": self.failure_reason().is_none(),
                "operation": if self.command.refine.is_some() { "refine" } else { "search" },
                "engine": self.engine,
                "settings": {
                    "method": method,
                    "conventions": "axtb",
                    "solvent": null,
                    "charge": self.command.charge,
                    "uhf": self.uhf,
                    "images": self.command.images,
                    "workers": if self.command.refine.is_some() { 1 } else { self.command.workers },
                    "threads_per_worker": 1,
                    "optimize_endpoints": !self.command.keep_endpoints,
                    "remap_hydrogens": self.command.remap_hydrogens,
                    "form": self.command.form,
                    "break": self.command.break_,
                },
                "energies": {
                    "method": method,
                    "units": "hartree",
                    "reactant": report.reactant.energy,
                    "product": report.product.energy,
                    "ts": report.ts.energy,
                },
                "barriers": {
                    "method": method,
                    "units": "kcal/mol",
                    "verified": self.failure_reason().is_none(),
                    "forward": self.barriers_allowed().then(|| report.forward_barrier_kcal()),
                    "reverse": self.barriers_allowed().then(|| report.reverse_barrier_kcal()),
                },
                "methods": {
                    "energies": method,
                    "barriers": method,
                    "endpoint_relaxation": method,
                    "saddle": method,
                    "frequencies": method,
                    "worker_frequencies": method,
                    "hessian": method,
                    "irc": method,
                    "path": method,
                    "reactive_start": method,
                    "work": method,
                },
                "structures": {
                    "numbers": self.numbers,
                    "position_units": "bohr",
                    "method": method,
                    "reactant": report.reactant.positions,
                    "product": report.product.positions,
                    "ts": report.ts.positions,
                },
                "source_result": self.source_report,
                "source_seed": (self.command.from.is_some() && self.source_report.is_none()).then(|| json!({
                    "directory": self.command.from.as_ref().map(|path| path.display().to_string()),
                    "geometry_only": true,
                    "method": null,
                    "verified": false,
                })),
                "frequencies_cm1": report.frequencies_cm1,
                "worker_frequencies_cm1": self.worker_check.as_ref().ok().map(|analysis| &analysis.frequencies_cm1),
                "worker_imaginary_count": self.worker_check.as_ref().ok().map(|analysis| imaginary(&analysis.frequencies_cm1)),
                "worker_check_error": self.worker_failure_reason(),
                "hessian_source": if self.worker_check.is_ok() { "worker" } else { "engine" },
                "imaginary_count": report.imaginary,
                "failure_reason": self.failure_reason(),
                "irc_error": report.irc_error,
                "irc": report.irc.as_ref().map(|[plus, minus]| json!({
                    "method": method,
                    "plus": branch(plus),
                    "minus": branch(minus),
                })),
                "connects": report.connects,
                "path_energies_eh": report.path.iter().map(|image| image.energy).collect::<Vec<_>>(),
                "neb": {"method": method, "performed": self.command.refine.is_none(), "converged": report.neb_converged, "iterations": self.iterations},
                "saddle": {
                    "method": method,
                    "converged": report.saddle_converged,
                    "steps": self.steps,
                    "reason": report.saddle_reason.to_string(),
                    "diagnostics": report.saddle_diagnostics,
                },
                "evaluations": self.evaluations,
                "hessians": report.hessians + 1,
                "seconds": self.seconds,
                "moving_hydrogens": self.moving_hydrogens,
                "reactive_start": self.reactive_start.as_ref().map(|fold| json!({
                    "pairs": fold.pairs.iter().map(|&(a, b)| [a + 1, b + 1]).collect::<Vec<_>>(),
                    "before_angstrom": fold.before_angstrom,
                    "after_angstrom": fold.after_angstrom,
                    "energy_change_kcal": fold.energy_change_kcal,
                })),
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn skipped_irc_distinguishes_stationarity_from_imaginary_count() {
            assert_eq!(
                skipped_irc_reason(true, SaddleReason::EnergyCeiling, 1).as_deref(),
                Some("saddle refinement did not converge (energy ceiling exhausted)")
            );
            for count in [0, 2] {
                assert_eq!(
                    skipped_irc_reason(true, SaddleReason::Stationary, count).unwrap(),
                    format!("expected one imaginary frequency below -50 cm-1, found {count}")
                );
            }
            assert_eq!(skipped_irc_reason(true, SaddleReason::Stationary, 1), None);
            // A retained earlier candidate keeps the reason but is not converged.
            assert!(
                skipped_irc_reason(false, SaddleReason::Stationary, 1)
                    .is_some_and(|reason| reason.contains("did not converge"))
            );
        }
    }
}

#[cfg(feature = "xtb")]
pub(super) use report::{Summary, check_destinations, write_retained};

#[cfg(test)]
mod tests {
    use super::*;

    fn point(energy: f64, z: f64) -> Minimum {
        Minimum {
            energy,
            positions: vec![0., 0., z, 0., 0., z + 2.],
        }
    }

    #[test]
    fn xyz_writers_preserve_atom_order_units_and_trajectory_order() {
        let numbers = [6, 1];
        let reactant = point(-3., 0.);
        let product = point(-2., 4.);
        let ts = point(-1., 2.);
        let plus = IrcBranch {
            energies: vec![-1., -1.5, -3.],
            path: vec![point(-1.5, 1.)],
            end: reactant.clone(),
            reached: Reached::Reactant,
            completed: true,
        };
        let minus = IrcBranch {
            energies: vec![-1., -1.25, -2.],
            path: vec![point(-1.25, 3.)],
            end: product.clone(),
            reached: Reached::Product,
            completed: true,
        };
        let path = path_xyz(&numbers, &[reactant, ts.clone(), product], "");
        assert_eq!(
            path.lines().skip(1).step_by(4).collect::<Vec<_>>(),
            [
                "image 1 E=-3.00000000 Eh",
                "image 2 E=-1.00000000 Eh",
                "image 3 E=-2.00000000 Eh",
            ]
        );
        let read = crate::cli::calc::xyz(path.as_bytes()).unwrap();
        assert_eq!(read.numbers, numbers);
        assert_eq!(read.positions, [[0., 0., 0.], [0., 0., 1.0583544211]]);
        let branches = [plus, minus];
        let irc = irc_xyz(&numbers, &ts, Some(&branches), "");
        assert_eq!(
            irc.lines().skip(1).step_by(4).collect::<Vec<_>>(),
            [
                "+mode E=-3.00000000 Eh",
                "+mode E=-1.50000000 Eh",
                "TS E=-1.00000000 Eh",
                "-mode E=-1.25000000 Eh",
                "-mode E=-2.00000000 Eh",
            ]
        );
        let [plus, minus] = branches;
        let unlabelled = [
            IrcBranch {
                reached: Reached::Other,
                ..plus
            },
            IrcBranch {
                reached: Reached::Other,
                ..minus
            },
        ];
        let irc = irc_xyz(&numbers, &ts, Some(&unlabelled), "");
        assert_eq!(irc.lines().nth(1), Some("-mode E=-2.00000000 Eh"));
        assert_eq!(irc.lines().count(), 20);
        assert_eq!(irc_xyz(&numbers, &ts, None, "").lines().count(), 4);
    }
}
