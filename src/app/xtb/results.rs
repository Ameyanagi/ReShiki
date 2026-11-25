//! The results palette's content, kept free of iced: the method line, the
//! Summary rows, the optimization steps, the log and file sizes, built from
//! a stored result and what this session still knows about its run.
use super::state::{
    Config, NEAR_DEGENERATE, SOLVENTS, Task as CardTask, capitalized, formula, scientific, signed,
    symbol,
};
use reshiki::qcdata;
use reshiki::qcdata::{
    Beta, CalcResult, Frame, OrbitalSet, Orbitals, Settings, Status, Task, XtbConventions,
};

/// Always shown under the method line.
pub(crate) const LIMITATIONS: &str = "Semi-empirical: geometries good, frequencies approximate, orbitals qualitative (minimal basis).";
/// The gap under which the electronic structure counts as near-degenerate.
const NEAR_DEGENERATE_EV: f64 = 0.1;
/// CODATA 2022, as the worker converts its gap.
const EV_PER_HARTREE: f64 = 27.211386245981;
/// Debye per e·bohr.
const DEBYE_PER_E_BOHR: f64 = 2.541746473;
const KCAL_PER_MOL_PER_HARTREE: f64 = 627.509474;
/// Bond orders listed from this order up: what the worker reports and a
/// saved file keeps.
pub(crate) const BOND_ORDER_MIN: f64 = reshiki::qcdata::LIGHT_BOND_ORDER_MIN;

/// The palette's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    Summary,
    Vibrations,
    Steps,
    Log,
}

impl View {
    /// The tabs supported by the stored data.
    pub(crate) fn available(result: &CalcResult) -> Vec<Self> {
        let mut views = vec![Self::Summary];
        if result.vibrations.is_some() {
            views.push(Self::Vibrations);
        }
        if energies(result).len() > 1 {
            views.push(Self::Steps);
        }
        views.push(Self::Log);
        views
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Vibrations => "Vibrations",
            Self::Steps => "Steps",
            Self::Log => "Log",
        }
    }
}

/// One SCF the worker ran.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Evaluation {
    pub energy: f64,
    pub scf_iterations: usize,
    pub seconds: f64,
}

/// What this session knows about a run beyond the stored result. The file
/// does not keep it, so a reopened result shows less.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RunInfo {
    pub gap_ev: Option<f64>,
    pub near_degenerate: bool,
    pub evaluations: Vec<Evaluation>,
    pub numerics: String,
    pub tight: bool,
    pub threads: u32,
    /// Drawing atoms the optimizer held fixed.
    pub fixed: Vec<u64>,
    /// When the run finished, e.g. "25 Nov 2026 14:03".
    pub finished: String,
    pub seconds: f64,
}

/// "GFN2-xTB · axtb 0.2.0 · water (ALPB) · charge 0 · multiplicity 1 · 25 Nov 2026 14:03".
pub(crate) fn method_line(result: &CalcResult, frame: &Frame, info: Option<&RunInfo>) -> String {
    let provenance = &result.provenance;
    let mut parts = vec![provenance.method.clone().unwrap_or_else(|| "xTB".into())];
    parts.push(match &provenance.program_version {
        Some(version) => format!("{} {version}", provenance.program),
        None => provenance.program.clone(),
    });
    parts.push(solvent(result));
    if let Settings::Xtb(settings) = &result.settings {
        if settings.conventions == XtbConventions::Xtb68 {
            parts.push("xtb 6.8 compatible".into());
        }
        if let Some(kelvin) = settings.electronic_temperature_k {
            parts.push(format!("{kelvin} K"));
        }
    }
    parts.push(format!("charge {}", signed(f64::from(frame.charge), 0)));
    parts.push(format!("multiplicity {}", frame.multiplicity));
    if let Some(info) = info.filter(|info| !info.finished.is_empty()) {
        parts.push(info.finished.clone());
    }
    parts.join(" · ")
}

fn solvent(result: &CalcResult) -> String {
    match &result.settings {
        Settings::Xtb(settings) => settings.solvent.as_ref().map_or_else(
            || "gas phase".into(),
            |s| {
                let model = match s.model {
                    qcdata::SolventModel::Alpb => "ALPB",
                    qcdata::SolventModel::Gbsa => "GBSA",
                };
                match s.correction {
                    Some(_) => format!("{} ({model}, GFN2-xTB correction)", s.name),
                    None => format!("{} ({model})", s.name),
                }
            },
        ),
        Settings::Imported { .. } => "imported".into(),
    }
}

/// The energy of each optimization step, the start included.
pub(crate) fn energies(result: &CalcResult) -> &[f64] {
    result
        .trajectory
        .as_ref()
        .filter(|_| result.task == Task::Optimize)
        .map_or(&[], |path| path.energies_hartree.as_slice())
}

/// Step `step`'s geometry in Å. The file keeps only the first and the last
/// (the input and the result frame); the others exist while the full
/// result is in memory.
pub(crate) fn step_positions(
    result: &CalcResult,
    step: usize,
    input: &Frame,
    output: &Frame,
) -> Option<Vec<[f64; 3]>> {
    let count = energies(result).len();
    let atoms = output.numbers.len();
    let path = result.trajectory.as_ref()?;
    if path.positions_angstrom.len() == count * atoms * 3 {
        let block = path
            .positions_angstrom
            .get(step * atoms * 3..(step + 1) * atoms * 3)?;
        return Some(block.as_chunks::<3>().0.to_vec());
    }
    if step == 0 {
        Some(input.positions_angstrom.clone())
    } else if step + 1 == count {
        Some(output.positions_angstrom.clone())
    } else {
        None
    }
}

/// The Summary's label/value rows. Rows the result has no data for are left out.
pub(crate) fn summary_rows(
    result: &CalcResult,
    input: &Frame,
    output: &Frame,
    info: Option<&RunInfo>,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(energy) = result.energy_hartree {
        rows.push(("Total energy".into(), format!("{} Eh", signed(energy, 6))));
    }
    if let Some(gap) = gap_ev(result, info) {
        rows.push(("HOMO–LUMO gap".into(), format!("{gap:.2} eV")));
    }
    if let Some(dipole) = result.dipole_e_bohr {
        let debye = dipole.iter().map(|x| x * x).sum::<f64>().sqrt() * DEBYE_PER_E_BOHR;
        rows.push(("Dipole".into(), format!("{debye:.2} D")));
    }
    if let Some(charges) = result.charges.first() {
        let scheme = format!("{:?}", charges.scheme);
        let values = &charges.values;
        let text = if values.len() <= 4 {
            values
                .iter()
                .zip(&output.numbers)
                .map(|(q, &z)| format!("{} {}", symbol(z), signed_charge(*q)))
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            let (low, high) = values
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &q| {
                    (lo.min(q), hi.max(q))
                });
            format!("{} to {}", signed_charge(low), signed_charge(high))
        };
        rows.push((
            "Charges".into(),
            format!("{text} ({})", scheme.to_lowercase()),
        ));
    }
    if result.task == Task::Optimize {
        let steps = energies(result).len().saturating_sub(1);
        let plural = if steps == 1 { "" } else { "s" };
        let mut outcome = match &result.status {
            Status::Converged => format!("converged in {steps} step{plural}"),
            _ => format!("not converged after {steps} step{plural}"),
        };
        if let Some(rmsd) = rmsd(&input.positions_angstrom, &output.positions_angstrom) {
            outcome.push_str(&format!(" · RMSD from start {rmsd:.2} Å"));
        }
        rows.push(("Optimization".into(), outcome));
    }
    let gradient = &result.gradient_hartree_per_bohr;
    if !gradient.is_empty() {
        let rms = (gradient.iter().map(|g| g * g).sum::<f64>() / gradient.len() as f64).sqrt();
        rows.push((
            "RMS gradient".into(),
            format!("{} Eh/bohr", scientific(rms)),
        ));
    }
    if let Some(vibrations) = &result.vibrations {
        let frequencies = &vibrations.frequencies_cm1;
        let imaginary = vibrations.imaginary_count();
        let lowest_real = frequencies.get(imaginary);
        let summary = if let Some(&lowest) = frequencies.first().filter(|&&f| f < 0.0) {
            let mut text = format!("{imaginary} imaginary ({}i cm⁻¹)", signed(lowest, 0));
            if let Some(real) = lowest_real {
                text.push_str(&format!(" · lowest real {real:.0} cm⁻¹"));
            }
            text
        } else if let Some(real) = lowest_real {
            format!("lowest {real:.0} cm⁻¹")
        } else {
            "No vibrational modes".into()
        };
        rows.push(("Frequencies".into(), summary));
    }
    if let Some(thermo) = &result.thermo {
        let zpve = thermo.zero_point_energy_hartree;
        rows.push((
            "ZPVE".into(),
            format!(
                "{} Eh ({} kcal/mol)",
                signed(zpve, 6),
                signed(zpve * KCAL_PER_MOL_PER_HARTREE, 2)
            ),
        ));
        if let Some(energy) = result.energy_hartree {
            for (quantity, correction) in [
                ("H", thermo.enthalpy_correction_hartree),
                ("G", thermo.gibbs_correction_hartree),
            ] {
                rows.push((
                    format!("{quantity} ({:.2} K)", thermo.temperature_k),
                    format!("{} Eh", signed(energy + correction, 6)),
                ));
            }
        }
        if let Some(group) = &thermo.point_group {
            rows.push(("Point group".into(), point_group(group)));
        }
    }
    rows.push((
        "Molecule".into(),
        format!(
            "{} · {} atoms",
            formula(&output.numbers),
            output.numbers.len()
        ),
    ));
    if let Some(info) = info.filter(|info| info.seconds > 0.) {
        rows.push((
            "Run time".into(),
            format!(
                "{:.1} s · {} {}",
                info.seconds,
                info.evaluations.len(),
                // GFN-FF has no SCF.
                if matches!(&result.settings, Settings::Xtb(s) if !super::state::electronic(s.method)) {
                    "calculations"
                } else {
                    "SCF calculations"
                }
            ),
        ));
    }
    rows
}

/// "C2v", "D∞h": the engine's label as chemists write it.
pub(crate) fn point_group(group: &str) -> String {
    match group.to_ascii_lowercase().as_str() {
        "cinfv" | "c∞v" => "C∞v".into(),
        "dinfh" | "d∞h" => "D∞h".into(),
        _ => capitalized(group),
    }
}

/// The near-degenerate warning, when the gap calls for it.
pub(crate) fn warning(result: &CalcResult, info: Option<&RunInfo>) -> Option<&'static str> {
    (info.is_some_and(|info| info.near_degenerate)
        || gap_ev(result, info).is_some_and(|gap| gap < NEAR_DEGENERATE_EV))
    .then_some(NEAR_DEGENERATE)
}

/// "+0.280" or "−0.561".
pub(crate) fn signed_charge(value: f64) -> String {
    // −0.0004 rounds to zero and shows unsigned.
    if value.abs() < 5e-4 {
        return "0.000".into();
    }
    if value >= 0. {
        format!("+{value:.3}")
    } else {
        signed(value, 3)
    }
}

/// The HOMO–LUMO gap in eV from the stored orbital levels, else from this
/// session's run.
pub(crate) fn gap_ev(result: &CalcResult, info: Option<&RunInfo>) -> Option<f64> {
    // The worker's own value while this session knows it, absence included.
    match info {
        Some(info) => info.gap_ev,
        None => gap_hartree(result).map(|gap| gap * EV_PER_HARTREE),
    }
}

/// The occupied count of the alpha spectrum (half the electrons, rounded
/// up, for restricted levels); thermal smearing does not move it.
fn alpha_electrons(orbitals: &Orbitals) -> Option<usize> {
    let set = &orbitals.alpha;
    let total = filled(set, &set.occupations)?;
    Some(match orbitals.beta {
        Beta::Restricted => total.div_ceil(2),
        _ => total,
    })
}

fn filled(set: &OrbitalSet, occupations: &[f64]) -> Option<usize> {
    (occupations.len() == set.energies_hartree.len())
        .then(|| occupations.iter().sum::<f64>().round() as usize)
}

/// LUMO − HOMO (Eh) as the engine reports it: the alpha frontier, except
/// that unrestricted g-xTB in its own conventions takes the highest
/// occupied and lowest empty level of either spin.
fn gap_hartree(result: &CalcResult) -> Option<f64> {
    let orbitals = result.orbitals.as_ref()?;
    let alpha = &orbitals.alpha.energies_hartree;
    let n = alpha_electrons(orbitals)?;
    let xtb68 =
        matches!(&result.settings, Settings::Xtb(s) if s.conventions == XtbConventions::Xtb68);
    match &orbitals.beta {
        Beta::Unrestricted(beta) if !xtb68 => {
            let m = filled(beta, &beta.occupations)?;
            let both = |levels: &[f64], k: usize| k > 0 && k < levels.len();
            if !both(alpha, n) && !both(&beta.energies_hartree, m) {
                return None;
            }
            let levels = &beta.energies_hartree;
            let homo = [
                n.checked_sub(1).and_then(|i| alpha.get(i)),
                m.checked_sub(1).and_then(|i| levels.get(i)),
            ];
            let lumo = [alpha.get(n), levels.get(m)];
            let homo = homo.into_iter().flatten().copied().reduce(f64::max)?;
            let lumo = lumo.into_iter().flatten().copied().reduce(f64::min)?;
            Some(lumo - homo)
        }
        _ => Some(alpha.get(n)? - alpha.get(n.checked_sub(1)?)?),
    }
}

/// Orbital levels around the gap, "HOMO−2" … "LUMO+2", in eV, highest
/// first. Open shells list the alpha spin, marked "α".
pub(crate) fn levels(result: &CalcResult) -> Vec<(String, f64)> {
    let Some(orbitals) = result.orbitals.as_ref() else {
        return vec![];
    };
    let Some(n) = alpha_electrons(orbitals) else {
        return vec![];
    };
    let spin = if matches!(orbitals.beta, Beta::Restricted) {
        ""
    } else {
        "α "
    };
    let mut rows = around(&orbitals.alpha.energies_hartree, n, spin);
    if let Beta::Unrestricted(beta) = &orbitals.beta
        && let Some(n) = filled(beta, &beta.occupations)
    {
        rows.extend(around(&beta.energies_hartree, n, "β "));
    }
    rows
}

/// HOMO−2 … LUMO+2 of one spectrum with `n` occupied levels, highest
/// first, names prefixed by `spin`. An empty spin starts at its LUMO.
fn around(levels: &[f64], n: usize, spin: &str) -> Vec<(String, f64)> {
    let name = |index: usize| match index.checked_sub(n) {
        None if index + 1 == n => format!("{spin}HOMO"),
        None => format!("{spin}HOMO−{}", n - 1 - index),
        Some(0) => format!("{spin}LUMO"),
        Some(above) => format!("{spin}LUMO+{above}"),
    };
    let shown = n.saturating_sub(3)..=n + 2;
    let mut rows: Vec<_> = (levels.iter().enumerate())
        .filter(|(index, _)| shown.contains(index))
        .map(|(index, energy)| (name(index), energy * EV_PER_HARTREE))
        .collect();
    rows.reverse();
    rows
}

/// A stored result's settings, to calculate it again; what the file
/// does not keep comes from `info` (this session's run), else defaults.
/// `None` when the card cannot reproduce it: imported results, other
/// tasks, GBSA or a solvent the card does not list.
pub(crate) fn config(
    result: &qcdata::CalcResult,
    frame: &qcdata::Frame,
    info: Option<&RunInfo>,
    threads: u32,
) -> Option<Config> {
    let qcdata::Settings::Xtb(settings) = &result.settings else {
        return None;
    };
    let task = match result.task {
        qcdata::Task::Optimize if result.vibrations.is_some() => CardTask::OptFreq,
        qcdata::Task::Optimize => CardTask::Optimize,
        qcdata::Task::Hessian => CardTask::Frequencies,
        qcdata::Task::Energy => CardTask::Energy,
        _ => return None,
    };
    // Settings the card cannot send: a retry ladder, GFN-FF fragment
    // charges other than the default (the whole charge on the first
    // fragment, which the worker saves resolved), another solvation state.
    let topology = settings
        .gfnff
        .as_ref()
        .is_some_and(|gfnff| gfnff.fragment_charges.iter().skip(1).any(|&q| q != 0));
    if !settings.scf_retries.is_empty() || topology {
        return None;
    }
    let solvent = match &settings.solvent {
        None => None,
        Some(s) if s.model == qcdata::SolventModel::Alpb && s.state == Default::default() => Some(
            SOLVENTS
                .into_iter()
                .find(|name| name.eq_ignore_ascii_case(&s.name))?,
        ),
        Some(_) => return None,
    };
    Some(Config {
        task,
        method: settings.method,
        charge: frame.charge,
        unpaired: frame.multiplicity.saturating_sub(1),
        solvent,
        xtb68: settings.conventions == qcdata::XtbConventions::Xtb68,
        accuracy: settings.accuracy,
        temperature_k: settings.electronic_temperature_k,
        max_iterations: settings.max_iterations,
        threads: info.map_or(threads, |info| info.threads),
        tight: info.is_some_and(|info| info.tight),
        fixed: info.map_or_else(Vec::new, |info| info.fixed.clone()),
    })
}

/// "C1–O2" with "1.92": each stored bond order, in the frame's atom order.
pub(crate) fn bond_orders(result: &CalcResult, frame: &Frame) -> Vec<(String, String)> {
    let atom = |index: u32| {
        let z = frame.numbers.get(index as usize).copied().unwrap_or(0);
        format!("{}{}", symbol(z), index + 1)
    };
    result
        .bond_orders
        .iter()
        .filter(|bond| bond.order >= BOND_ORDER_MIN)
        .map(|bond| {
            (
                format!("{}–{}", atom(bond.a), atom(bond.b)),
                format!("{:.2}", bond.order),
            )
        })
        .collect()
}

/// Copy table and Export…: tab-separated, with a header row.
pub(crate) fn tsv(method: &str, rows: &[(String, String)]) -> String {
    let mut text = format!("# {method}\nQuantity\tValue\n");
    for (label, value) in rows {
        text.push_str(&format!("{label}\t{value}\n"));
    }
    text
}

/// Whether the stored quasi-RRHO cutoffs are xtb's defaults.
pub(super) fn thermo_defaults(thermo: &qcdata::Thermo) -> bool {
    thermo.rotor_cutoff_cm1 == 50. && thermo.imaginary_cutoff_cm1 == 20.
}

/// The Log tab: settings, then each SCF the worker ran, then the result.
pub(crate) fn log(
    result: &CalcResult,
    frame: &Frame,
    info: Option<&RunInfo>,
    method: &str,
) -> String {
    let mut lines = vec![method.to_owned()];
    if let reshiki::qcdata::Source::Calculated { engine_revision } = &result.provenance.source {
        lines.push(format!("engine revision {engine_revision}"));
    }
    lines.push(format!(
        "{} · {} atoms · {} electrons",
        formula(&frame.numbers),
        frame.numbers.len(),
        frame.numbers.iter().map(|&z| i64::from(z)).sum::<i64>() - i64::from(frame.charge)
    ));
    let task = match result.task {
        Task::Optimize if result.vibrations.is_some() => "opt+freq",
        Task::Optimize => "optimize",
        Task::Hessian => "frequencies",
        Task::Energy => "energy",
        _ => "other",
    };
    if let Settings::Xtb(settings) = &result.settings {
        let scf: Vec<String> = [
            settings.accuracy.map(|a| format!("SCF accuracy {a}")),
            settings
                .max_iterations
                .map(|n| format!("max {n} SCF iterations")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !scf.is_empty() {
            lines.push(scf.join(" · "));
        }
    }
    match info {
        Some(info) => {
            let mut settings = format!("task {task} · {} threads", info.threads);
            if result.task == Task::Optimize {
                settings.push_str(if info.tight {
                    " · tight convergence"
                } else {
                    " · normal convergence"
                });
                if !info.fixed.is_empty() {
                    settings.push_str(&format!(" · {} atoms held fixed", info.fixed.len()));
                }
            }
            lines.push(settings);
            if !info.numerics.is_empty() {
                lines.push(format!("numerics {}", info.numerics));
            }
            lines.push(String::new());
            lines.push(" calc        energy (Eh)    SCF iterations    time (s)".into());
            for (index, e) in info.evaluations.iter().enumerate() {
                lines.push(format!(
                    "{:>5}  {:>17.8}  {:>16}  {:>10.3}",
                    index + 1,
                    e.energy,
                    e.scf_iterations,
                    e.seconds
                ));
            }
        }
        None => {
            lines.push(format!("task {task}"));
            lines.push(String::new());
            lines.push(
                "SCF details are kept only while the app is open; recalculate to see them.".into(),
            );
        }
    }
    lines.push(String::new());
    if let Some(energy) = result.energy_hartree {
        lines.push(format!("total energy    {energy:.8} Eh"));
    }
    if let Some(gap) = gap_ev(result, info) {
        lines.push(format!("HOMO-LUMO gap   {gap:.4} eV"));
    }
    if result.task == Task::Optimize {
        lines.push(match &result.status {
            Status::Converged => format!(
                "optimization converged in {} steps",
                energies(result).len().saturating_sub(1)
            ),
            Status::NotConverged { reason } => format!("optimization not converged: {reason}"),
            Status::Failed { reason } => format!("failed: {reason}"),
        });
    }
    if let Some(vibrations) = &result.vibrations {
        lines.push("frequencies (cm⁻¹)".into());
        for chunk in vibrations.frequencies_cm1.chunks(6) {
            lines.push(
                chunk
                    .iter()
                    .map(|f| format!("{f:8.2}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        lines.push(format!("imaginary modes {}", vibrations.imaginary_count()));
    }
    if let Some(thermo) = &result.thermo {
        lines.push(format!("ZPVE  {:.6} Eh", thermo.zero_point_energy_hartree));
        if let Some(energy) = result.energy_hartree {
            lines.push(format!(
                "H(T)  {:.6} Eh",
                energy + thermo.enthalpy_correction_hartree
            ));
        }
        lines.push(format!("S  {:.8} Eh/K", thermo.entropy_hartree_per_k));
        if let Some(energy) = result.energy_hartree {
            lines.push(format!(
                "G(T)  {:.6} Eh",
                energy + thermo.gibbs_correction_hartree
            ));
        }
        if let Some(group) = &thermo.point_group {
            lines.push(format!("point group {}", point_group(group)));
        }
        lines.push(format!(
            "thermochemistry: quasi-RRHO (Grimme 2012), free-rotor interpolation below {} cm⁻¹, imaginary modes above −{} cm⁻¹ inverted{}, T {:.2} K, p {} Pa",
            thermo.rotor_cutoff_cm1,
            thermo.imaginary_cutoff_cm1.abs(),
            if thermo_defaults(thermo) { " (xtb's defaults)" } else { "" },
            thermo.temperature_k,
            thermo.pressure_pa,
        ));
    }
    lines.join("\n")
}

/// RMSD in Å over every atom of two geometries in the same atom order,
/// after the best superposition. The browser build, which has no optimizer
/// crate, leaves it out.
pub(crate) fn rmsd(a: &[[f64; 3]], b: &[[f64; 3]]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    #[cfg(target_arch = "wasm32")]
    return None;
    #[cfg(not(target_arch = "wasm32"))]
    {
        use reshiki_optimize::conformers::{Topology, rmsd};
        // Every atom counts as heavy, and the one mapping kept is the identity.
        let topology = Topology::new(vec![6; a.len()], vec![]).ok()?;
        rmsd(&topology, &topology.automorphisms(1), a, b).ok()
    }
}

/// "84 KB", "1.4 MB": decimal units, as file browsers show them.
pub(crate) fn size(bytes: usize) -> String {
    if bytes < 1_000_000 {
        format!("{} KB", bytes.div_ceil(1000).max(1))
    } else {
        format!("{:.1} MB", bytes as f64 / 1e6)
    }
}

#[cfg(test)]
mod tests;
