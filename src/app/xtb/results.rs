//! The results palette's content, kept free of iced: the method line, the
//! Summary rows, the optimization steps, the log and file sizes, built from
//! a stored result and what this session still knows about its run.
use super::state::{NEAR_DEGENERATE, formula, scientific, signed, symbol};
use reshiki::qcdata::{
    Beta, CalcResult, Frame, OrbitalSet, Settings, Status, Task, XtbConventions,
};

/// Always shown under the method line.
pub(crate) const LIMITATIONS: &str = "Semi-empirical: geometries good, frequencies approximate, orbitals qualitative (minimal basis).";
/// The gap under which the electronic structure counts as near-degenerate.
const NEAR_DEGENERATE_EV: f64 = 0.1;
/// CODATA 2022, as the worker converts its gap.
const EV_PER_HARTREE: f64 = 27.211386245981;
/// Debye per e·bohr.
const DEBYE_PER_E_BOHR: f64 = 2.541746473;
/// Bond orders listed from this order up, as the worker reports them.
pub(crate) const BOND_ORDER_MIN: f64 = 0.1;

/// The palette's tabs. Orbitals and Vibrations join them once the worker
/// returns orbitals and frequencies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    Summary,
    Steps,
    Log,
}

impl View {
    /// The tabs `result` has: Steps only for an optimization with steps.
    pub(crate) fn available(result: &CalcResult) -> Vec<Self> {
        let mut views = vec![Self::Summary];
        if energies(result).len() > 1 {
            views.push(Self::Steps);
        }
        views.push(Self::Log);
        views
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
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
            |s| match s.correction {
                Some(_) => format!("{} (ALPB, GFN2-xTB correction)", s.name),
                None => format!("{} (ALPB)", s.name),
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
) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    if let Some(energy) = result.energy_hartree {
        rows.push(("Total energy", format!("{} Eh", signed(energy, 6))));
    }
    if let Some(gap) = gap_ev(result, info) {
        rows.push(("HOMO–LUMO gap", format!("{gap:.2} eV")));
    }
    if let Some(dipole) = result.dipole_e_bohr {
        let debye = dipole.iter().map(|x| x * x).sum::<f64>().sqrt() * DEBYE_PER_E_BOHR;
        rows.push(("Dipole", format!("{debye:.2} D")));
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
        rows.push(("Charges", format!("{text} ({})", scheme.to_lowercase())));
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
        rows.push(("Optimization", outcome));
    }
    let gradient = &result.gradient_hartree_per_bohr;
    if !gradient.is_empty() {
        let rms = (gradient.iter().map(|g| g * g).sum::<f64>() / gradient.len() as f64).sqrt();
        rows.push(("RMS gradient", format!("{} Eh/bohr", scientific(rms))));
    }
    rows.push((
        "Molecule",
        format!(
            "{} · {} atoms",
            formula(&output.numbers),
            output.numbers.len()
        ),
    ));
    if let Some(info) = info.filter(|info| info.seconds > 0.) {
        rows.push((
            "Run time",
            format!(
                "{:.1} s · {} SCF calculations",
                info.seconds,
                info.evaluations.len()
            ),
        ));
    }
    rows
}

/// The near-degenerate warning, when the gap calls for it.
pub(crate) fn warning(result: &CalcResult, info: Option<&RunInfo>) -> Option<&'static str> {
    (info.is_some_and(|info| info.near_degenerate)
        || gap_ev(result, info).is_some_and(|gap| gap < NEAR_DEGENERATE_EV))
    .then_some(NEAR_DEGENERATE)
}

/// "+0.280" or "−0.561".
pub(crate) fn signed_charge(value: f64) -> String {
    if value >= 0. {
        format!("+{value:.3}")
    } else {
        signed(value, 3)
    }
}

/// The HOMO–LUMO gap in eV from the stored orbital levels, else from this
/// session's run.
pub(crate) fn gap_ev(result: &CalcResult, info: Option<&RunInfo>) -> Option<f64> {
    gap_hartree(result)
        .map(|gap| gap * EV_PER_HARTREE)
        .or_else(|| info.and_then(|info| info.gap_ev))
}

/// The smallest LUMO − HOMO of either spin (Eh), filling each spin's
/// levels with its electron count; `None` without levels or a LUMO.
fn gap_hartree(result: &CalcResult) -> Option<f64> {
    let orbitals = result.orbitals.as_ref()?;
    let count = |set: &OrbitalSet, occupations: &[f64]| -> Option<usize> {
        (occupations.len() == set.energies_hartree.len())
            .then(|| occupations.iter().sum::<f64>().round() as usize)
    };
    let alpha = &orbitals.alpha;
    let spins: Vec<(&[f64], usize)> = match &orbitals.beta {
        Beta::Restricted => {
            let total = count(alpha, &alpha.occupations)?;
            let levels = &alpha.energies_hartree[..];
            vec![(levels, total.div_ceil(2)), (levels, total / 2)]
        }
        Beta::SameOrbitals { occupations } => vec![
            (
                &alpha.energies_hartree[..],
                count(alpha, &alpha.occupations)?,
            ),
            (&alpha.energies_hartree[..], count(alpha, occupations)?),
        ],
        Beta::Unrestricted(beta) => vec![
            (
                &alpha.energies_hartree[..],
                count(alpha, &alpha.occupations)?,
            ),
            (&beta.energies_hartree[..], count(beta, &beta.occupations)?),
        ],
    };
    spins
        .iter()
        .filter_map(|(levels, n)| Some(levels.get(*n)? - levels.get(n.checked_sub(1)?)?))
        .reduce(f64::min)
}

/// Orbital levels around the gap, "HOMO−2" … "LUMO+2", in eV, highest
/// first; closed shells only (open shells show their gap).
pub(crate) fn levels(result: &CalcResult) -> Vec<(String, f64)> {
    let Some(orbitals) = result.orbitals.as_ref() else {
        return vec![];
    };
    let set = &orbitals.alpha;
    if !matches!(orbitals.beta, Beta::Restricted)
        || set.occupations.len() != set.energies_hartree.len()
    {
        return vec![];
    }
    let homo = (set.occupations.iter().sum::<f64>().round() as usize).div_ceil(2);
    let Some(homo) = homo.checked_sub(1) else {
        return vec![];
    };
    let name = |index: usize| match index.cmp(&homo) {
        std::cmp::Ordering::Equal => "HOMO".to_owned(),
        std::cmp::Ordering::Less => format!("HOMO−{}", homo - index),
        std::cmp::Ordering::Greater if index == homo + 1 => "LUMO".to_owned(),
        std::cmp::Ordering::Greater => format!("LUMO+{}", index - homo - 1),
    };
    let shown = homo.saturating_sub(2)..=homo + 3;
    let mut rows: Vec<_> = (set.energies_hartree.iter().enumerate())
        .filter(|(index, _)| shown.contains(index))
        .map(|(index, energy)| (name(index), energy * EV_PER_HARTREE))
        .collect();
    rows.reverse();
    rows
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
pub(crate) fn tsv(method: &str, rows: &[(&str, String)]) -> String {
    let mut text = format!("# {method}\nQuantity\tValue\n");
    for (label, value) in rows {
        text.push_str(&format!("{label}\t{value}\n"));
    }
    text
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
        Task::Optimize => "optimize",
        Task::Energy => "energy",
        _ => "other",
    };
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
    if let Some(gap) = info.and_then(|info| info.gap_ev) {
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
