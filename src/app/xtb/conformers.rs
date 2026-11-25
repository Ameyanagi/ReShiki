//! Conformer search: the Calculate card's Conformers settings, the running
//! search and its result, the Conformers tab of the results palette (table,
//! preview, overlay, Use this conformer, export).
//!
//! ETKDG makes the starts in the geometry worker and the chosen minimizer
//! relaxes them; Butina clustering on symmetry-aware heavy-atom RMSD picks
//! representatives, which xTB optimizes and ranks. The result is a
//! `Task::Conformers` result: the lowest conformer is its frame and the
//! ranked conformers (at most 32) its trajectory, saved as compact frames.
use reshiki::qcdata::{self, CalcResult, XtbMethod};

#[cfg(not(target_arch = "wasm32"))]
mod pipeline;
#[cfg(not(target_arch = "wasm32"))]
pub(super) use pipeline::{Event, Found, Search};

pub(super) mod export;
mod search;
mod view;

pub(super) use view::footer;

#[cfg(test)]
mod tests;

/// Starts ETKDG may make; the engine's limit.
pub(crate) const MAX_STARTS: u32 = 300;
/// xTB re-ranking (and so the whole search) works up to this many atoms.
pub(crate) const MAX_ATOMS: usize = qcdata::MAX_HESSIAN_ATOMS;
/// GFN2-xTB may minimize every start only up to this many atoms.
pub(crate) const MAX_GFN2_START_ATOMS: usize = 120;
/// CODATA 2022 hartree in thermochemical kcal/mol, as the optimizer uses it.
const KCAL_PER_HARTREE: f64 = 627.509_474_063;
/// Representatives xTB re-ranks at most.
pub(crate) const MAX_REPRESENTATIVES: usize = 60;

/// What relaxes the ETKDG starts before clustering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Minimizer {
    #[default]
    Mmff94s,
    Mmff94,
    Uff,
    GfnFf,
    Gfn2,
}

impl Minimizer {
    pub(crate) const ALL: [Self; 5] = [
        Self::Mmff94s,
        Self::Mmff94,
        Self::Uff,
        Self::GfnFf,
        Self::Gfn2,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Mmff94s => "MMFF94s",
            Self::Mmff94 => "MMFF94",
            Self::Uff => "UFF",
            Self::GfnFf => "GFN-FF",
            Self::Gfn2 => "GFN2-xTB",
        }
    }

    /// The xTB method that minimizes every start, for GFN-FF and GFN2-xTB.
    pub(crate) fn xtb(self) -> Option<XtbMethod> {
        match self {
            Self::GfnFf => Some(XtbMethod::GfnFf),
            Self::Gfn2 => Some(XtbMethod::Gfn2Xtb),
            _ => None,
        }
    }
}

impl std::fmt::Display for Minimizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Clustering threshold, as the card names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Diversity {
    Fine,
    #[default]
    Normal,
    Coarse,
}

impl Diversity {
    pub(crate) const ALL: [Self; 3] = [Self::Fine, Self::Normal, Self::Coarse];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Fine => "fine",
            Self::Normal => "normal",
            Self::Coarse => "coarse",
        }
    }

    /// Heavy-atom RMSD cutoff in Å, as the engine uses it.
    pub(crate) fn angstrom(self) -> f64 {
        match self {
            Self::Fine => 0.5,
            Self::Normal => 0.75,
            Self::Coarse => 1.25,
        }
    }
}

/// The card's Conformers fields as typed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub starts: String,
    pub minimizer: Minimizer,
    pub diversity: Diversity,
    /// Energy window above the lowest minimized start, kcal/mol.
    pub window: String,
    /// Representatives xTB re-ranks in total.
    pub representatives: String,
    /// Lowest 3 per cluster instead of the lowest 1.
    pub three: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            starts: "100".into(),
            minimizer: Minimizer::default(),
            diversity: Diversity::default(),
            window: "15".into(),
            representatives: "20".into(),
            three: false,
        }
    }
}

/// A search as it runs: the card's fields, checked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Config {
    pub starts: u32,
    pub minimizer: Minimizer,
    pub diversity: Diversity,
    pub window_kcal: f64,
    pub representatives: usize,
    pub three: bool,
}

impl Settings {
    /// The search these fields describe for a molecule of `atoms` atoms
    /// (with hydrogens, when known), or why it cannot run.
    pub(crate) fn config(&self, atoms: Option<usize>) -> Result<Config, String> {
        let starts = self
            .starts
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=MAX_STARTS).contains(n))
            .ok_or(format!(
                "Starting structures must be a whole number from 1 to {MAX_STARTS}"
            ))?;
        let window_kcal = self
            .window
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|w| w.is_finite() && *w >= 0.1 && *w <= 100.)
            .ok_or("Energy window must be from 0.1 to 100 kcal/mol")?;
        // When xTB minimized every start there is no re-ranking: every
        // cluster representative is kept, and the field does not apply.
        let representatives = if self.minimizer.xtb().is_some() {
            MAX_REPRESENTATIVES
        } else {
            self.representatives
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=MAX_REPRESENTATIVES).contains(n))
                .ok_or(format!(
                    "Representatives must be a whole number from 1 to {MAX_REPRESENTATIVES}"
                ))?
        };
        if let Some(atoms) = atoms {
            if atoms > MAX_ATOMS {
                return Err(format!(
                    "Conformer search re-ranks with xTB up to {MAX_ATOMS} atoms; this molecule has {atoms}."
                ));
            }
            if self.minimizer == Minimizer::Gfn2 && atoms > MAX_GFN2_START_ATOMS {
                return Err(format!(
                    "GFN2-xTB minimizes every start up to {MAX_GFN2_START_ATOMS} atoms; choose GFN-FF or MMFF94s."
                ));
            }
        }
        Ok(Config {
            starts,
            minimizer: self.minimizer,
            diversity: self.diversity,
            window_kcal,
            representatives,
            // Without re-ranking every cluster keeps its lowest member.
            three: self.three && self.minimizer.xtb().is_none(),
        })
    }
}

impl Config {
    /// The method line's search part, kept as the result's method name:
    /// "MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank".
    pub(crate) fn describe(&self, method: XtbMethod) -> String {
        let mut text = format!(
            "{} ETKDG {} starts · Butina {} Å",
            self.minimizer.label(),
            self.starts,
            self.diversity.angstrom()
        );
        if self.minimizer.xtb().is_none() {
            text.push_str(&format!(" · {} re-rank", method.label()));
        }
        text
    }
}

/// What the running search is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The geometry worker embeds (and, with a force field, minimizes) starts.
    #[default]
    Starts,
    /// Valid starts so far of those asked for.
    Sampled {
        done: usize,
        total: usize,
    },
    /// xTB minimizes every start (GFN-FF, GFN2-xTB).
    Minimizing {
        done: usize,
        total: usize,
    },
    Clustering,
    /// xTB optimizes the representatives.
    Reranking {
        done: usize,
        total: usize,
    },
}

/// A running search's state the card and status bar show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Live {
    pub config: Config,
    pub method: XtbMethod,
    pub phase: Phase,
    /// Cancel was pressed while re-ranking: finished conformers are kept.
    pub stopping: bool,
}

impl Live {
    /// "Embedding 100 · minimizing 64/100 (MMFF94s) · 12.3 s" and so on.
    pub(crate) fn line(&self, seconds: f64) -> String {
        let starts = self.config.starts;
        let field = self.config.minimizer.label();
        let text = match self.phase {
            Phase::Reranking { done, total } if self.stopping => {
                format!("Stopping · {done} of {total} finished")
            }
            _ if self.stopping => "Stopping".into(),
            Phase::Starts => format!("Embedding {starts} starts · {field}"),
            Phase::Sampled { done, total } => {
                format!("Embedding {total} · minimizing {done}/{total} ({field})")
            }
            Phase::Minimizing { done, total } => {
                format!("Embedding {total} · {field} {done}/{total}")
            }
            Phase::Clustering => format!("Clustering {starts} starts"),
            Phase::Reranking { done, total } => {
                format!("Clustered · {} {done}/{total}", self.method.label())
            }
        };
        format!("{text} · {seconds:.1} s")
    }

    /// The share of the work done, for the progress bar; `None` while it
    /// cannot be told.
    pub(crate) fn fraction(&self) -> Option<f32> {
        let part = |done: usize, total: usize| done as f32 / total.max(1) as f32;
        match self.phase {
            Phase::Starts => None,
            Phase::Sampled { done, total } | Phase::Minimizing { done, total } => {
                Some(0.5 * part(done, total))
            }
            Phase::Clustering => Some(0.5),
            Phase::Reranking { done, total } => Some(0.5 + 0.5 * part(done, total)),
        }
    }
}

/// Rough wall time of minimizing every start with GFN2-xTB, in seconds:
/// one gradient took about 8 ms for caffeine (24 atoms) and 93 ms for taxol
/// (113 atoms), about 100 steps per start, one thread each.
pub(crate) fn gfn2_estimate(atoms: usize, starts: u32, threads: u32) -> f64 {
    let gradient = 0.008 * (atoms as f64 / 24.).powf(1.6);
    gradient * 100. * f64::from(starts) / f64::from(threads.max(1))
}

/// The card's settings as a run uses them: a search whose minimizer is
/// GFN-FF or GFN2-xTB runs every calculation with that method, and is
/// checked as such (closed shells for GFN-FF, its solvent and SCF fields).
pub(crate) fn effective(
    settings: &super::state::Settings,
    search: &Settings,
) -> super::state::Settings {
    match search.minimizer.xtb() {
        Some(method) if settings.task == super::CalcTask::Conformers => super::state::Settings {
            method,
            ..settings.clone()
        },
        _ => settings.clone(),
    }
}

/// A stored search's settings to run again with the card's Conformers
/// fields; an xTB minimizer there sets the method, as on Run.
pub(crate) fn recalculation(
    config: &mut super::state::Config,
    search: &Settings,
) -> Result<Config, String> {
    let search = search.config(None)?;
    if let Some(method) = search.minimizer.xtb() {
        if !super::state::electronic(method) {
            if config.unpaired > 0 {
                return Err(
                    "GFN-FF calculates closed shells only: set Unpaired electrons to 0".into(),
                );
            }
            (
                config.solvent,
                config.accuracy,
                config.temperature_k,
                config.max_iterations,
            ) = (None, None, None, None);
        }
        config.method = method;
    }
    Ok(search)
}

/// Whether `result` is a conformer search.
pub(crate) fn is_search(result: &CalcResult) -> bool {
    result.task == qcdata::Task::Conformers
}

/// The force field named at the start of a search's method line, for the
/// "ΔE MMFF94s" column; `None` when xTB minimized the starts (the column
/// would repeat ΔE) or the result does not say.
pub(crate) fn start_field(result: &CalcResult) -> Option<&'static str> {
    let method = result.provenance.method.as_deref()?;
    let first = method.split_whitespace().next()?;
    Minimizer::ALL
        .into_iter()
        .find(|m| m.label() == first && m.xtb().is_none())
        .map(Minimizer::label)
}

/// Boltzmann populations of the kept conformers at 298.15 K, in percent.
fn percent(result: &CalcResult) -> Vec<f64> {
    let energies = super::results::energies(result);
    export::populations(energies)
        .into_iter()
        .map(|p| 100. * p)
        .collect()
}

/// "Conformer 3" or "Step 3", for the palette's messages.
pub(crate) fn step_name(result: &CalcResult, index: usize) -> String {
    if is_search(result) {
        format!("Conformer {}", index + 1)
    } else {
        format!("Step {index}")
    }
}

/// The status while a step or conformer is previewed.
pub(crate) fn previewing(result: &CalcResult, index: usize, count: usize, overlay: bool) -> String {
    if !is_search(result) {
        return format!("Previewing step {index} of {} · Esc returns", count - 1);
    }
    format!(
        "Conformer {} of {count}{} · Esc returns",
        index + 1,
        if overlay {
            " · overlay: all conformers"
        } else {
            ""
        }
    )
}

/// The card fields and the Conformers tab's choices.
#[derive(Clone, Debug)]
pub(crate) enum Action {
    Starts(String),
    Minimizer(Minimizer),
    Diversity(Diversity),
    Window(String),
    Representatives(String),
    Three(bool),
    /// Draw the other kept conformers faint behind the shown one.
    Overlay(bool),
    Export(Format),
}

/// Files "Export" writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Xyz,
    Sdf,
}

/// The Conformers tab's state in one document's palette.
#[derive(Default)]
pub(crate) struct Ensemble {
    pub overlay: bool,
    /// Rows for one result and document state; the stereo check and the
    /// symmetry-aware RMSDs are too slow for every frame.
    table: std::cell::RefCell<Option<Table>>,
}

/// The table rows of one result, measured once.
pub(crate) struct Table {
    result: std::sync::Weak<CalcResult>,
    revision: (u64, u64),
    pub rows: Vec<Row>,
}

/// One kept conformer as the tab lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub delta_kcal: f64,
    /// ΔE by the minimizer that made the start, when it was a force field.
    pub start_kcal: Option<f64>,
    pub percent: f64,
    /// Symmetry-aware heavy-atom RMSD to the lowest, Å.
    pub rmsd: Option<f64>,
    pub cluster: Option<u32>,
    /// Whether the drawing's stereocentres and double bonds hold; `None`
    /// when the check cannot run (a stale result, an error).
    pub stereo: Option<bool>,
}

/// Rows for a stored conformer search on `doc`; `atoms` maps frame atoms to
/// the drawing, `stale` skips the stereo check.
pub(crate) fn rows(
    doc: &reshiki::document::Document,
    atoms: &[Option<u64>],
    result: &CalcResult,
    stale: bool,
) -> Vec<Row> {
    let Some(path) = result.trajectory.as_ref() else {
        return vec![];
    };
    let energies = super::results::energies(result);
    let Some(&lowest) = energies.first() else {
        return vec![];
    };
    let kcal = KCAL_PER_HARTREE;
    let start_lowest = path
        .minimizer_energies_hartree
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let field = start_field(result).is_some();
    let percent = percent(result);
    let first = path.frame(0);
    let bonds = first.as_ref().map(|first| {
        export::frame_bonds(doc, atoms, &first.positions_angstrom)
            .into_iter()
            .map(|(a, b, _)| [a, b])
            .collect::<Vec<_>>()
    });
    energies
        .iter()
        .enumerate()
        .map(|(i, energy)| {
            let frame = path.frame(i);
            Row {
                delta_kcal: (energy - lowest) * kcal,
                start_kcal: path
                    .minimizer_energies_hartree
                    .get(i)
                    .filter(|_| field)
                    .map(|e| (e - start_lowest) * kcal),
                percent: percent.get(i).copied().unwrap_or(0.),
                rmsd: match (&first, &frame, &bonds) {
                    (Some(a), Some(b), Some(bonds)) => symmetric_rmsd(a, b, bonds),
                    _ => None,
                },
                cluster: path.cluster_sizes.get(i).copied(),
                stereo: frame.filter(|_| !stale).and_then(|frame| {
                    reshiki::science::stereo_changes(doc, atoms, &frame)
                        .ok()
                        .map(|changes| changes.is_empty())
                }),
            }
        })
        .collect()
}

/// Heavy-atom RMSD after the best superposition over the molecule's
/// symmetry; the browser build, without the optimizer crate, has none.
fn symmetric_rmsd(a: &qcdata::Frame, b: &qcdata::Frame, bonds: &[[usize; 2]]) -> Option<f64> {
    #[cfg(target_arch = "wasm32")]
    return {
        let _ = (a, b, bonds);
        None
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        use reshiki_optimize::conformers::{Settings, Topology, rmsd};
        let topology = Topology::new(a.numbers.clone(), bonds.to_vec()).ok()?;
        let autos = topology.automorphisms(Settings::default().automorphism_cap);
        rmsd(
            &topology,
            &autos,
            &a.positions_angstrom,
            &b.positions_angstrom,
        )
        .ok()
    }
}

/// Give the previewed model of `doc` the other kept conformers of `shown`,
/// superposed on conformer `index`, to draw faint behind it.
pub(crate) fn overlay(
    doc: &mut reshiki::document::Document,
    shown: &super::palette::Shown<'_>,
    index: usize,
) {
    let result = &shown.stored.result;
    let Some(path) = result.trajectory.as_ref().filter(|_| is_search(result)) else {
        return;
    };
    let Some(reference) = path.frame(index) else {
        return;
    };
    let ghosts: Vec<Vec<[f32; 3]>> = (0..path.steps())
        .filter(|&i| i != index)
        .filter_map(|i| superposed(&reference, &path.frame(i)?))
        .map(|positions| positions.iter().map(|p| p.map(|x| x as f32)).collect())
        .collect();
    let Some(atom) = shown.atoms.iter().flatten().next() else {
        return;
    };
    if let Some(model) = doc
        .models
        .iter_mut()
        .find(|m| m.linked_atoms.contains(atom))
    {
        model.overlay = ghosts;
    }
}

/// `mobile`'s geometry moved onto `reference` by its heavy atoms, in its
/// own atom order.
pub(crate) fn superposed(
    reference: &qcdata::Frame,
    mobile: &qcdata::Frame,
) -> Option<Vec<[f64; 3]>> {
    // The browser build has no fitting: no overlay rather than a misplaced one.
    #[cfg(target_arch = "wasm32")]
    return {
        let _ = (reference, mobile);
        None
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        use reshiki_optimize::conformers::{Topology, superpose};
        // The identity mapping only: atom order must stay as the model's.
        let topology = Topology::new(reference.numbers.clone(), vec![]).ok()?;
        let autos = topology.automorphisms(1);
        superpose(
            &topology,
            &autos,
            &reference.positions_angstrom,
            &mobile.positions_angstrom,
        )
        .ok()
        .map(|(positions, _)| positions)
    }
}
