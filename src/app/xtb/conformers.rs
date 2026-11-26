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
use reshiki::science::{self, TetrahedralStereo, UndefinedCentre};

#[cfg(all(feature = "conformer-bench", not(target_arch = "wasm32")))]
mod bench;
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

pub(crate) use reshiki_xtb_jobs::conformers::{
    Config, Diversity, MAX_ATOMS, MAX_REPRESENTATIVES, MAX_STARTS, Minimizer, Phase,
};
#[cfg(test)]
use reshiki_xtb_jobs::conformers::{KCAL_PER_HARTREE, MAX_GFN2_START_ATOMS};

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
    pub invert_undefined_stereo: bool,
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
            invert_undefined_stereo: false,
        }
    }
}

impl Settings {
    /// The search these fields describe, with hydrogens counted when known.
    pub(crate) fn config(&self, atoms: Option<usize>) -> Result<Config, String> {
        let mut config = Config::new(
            self.starts.trim().parse().unwrap_or_default(),
            self.minimizer,
            self.diversity,
            self.window.trim().parse().unwrap_or_default(),
            if self.minimizer.xtb().is_some() {
                MAX_REPRESENTATIVES
            } else {
                self.representatives.trim().parse().unwrap_or_default()
            },
            if self.three { 3 } else { 1 },
        )?;
        config.invert_undefined_stereo = self.invert_undefined_stereo;
        if let Some(count) = atoms {
            config.check_atoms(count).map_err(|failure| match failure {
                super::state::Failure::Other(message) => format!("{message}."),
                failure => failure.to_string(),
            })?;
        }
        Ok(config)
    }
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
    let mut settings = settings.clone();
    if settings.task == super::CalcTask::Conformers {
        settings.method = search.minimizer.effective_method(settings.method);
    }
    settings
}

/// A stored search's settings to run again with the card's Conformers
/// fields; an xTB minimizer there sets the method, as on Run.
pub(crate) fn recalculation(
    config: &mut super::state::Config,
    search: &Settings,
) -> Result<Config, String> {
    let search = search.config(None)?;
    use reshiki_xtb_jobs::method;
    let selected = search.minimizer.effective_method(config.method);
    if !method::accepts_spin(selected, config.unpaired) {
        return Err(super::state::CLOSED_SHELL.into());
    }
    (config.accuracy, config.temperature_k, config.max_iterations) =
        method::scf_options(selected, || {
            (config.accuracy, config.temperature_k, config.max_iterations)
        })
        .unwrap_or_default();
    config.solvent = config
        .solvent
        .filter(|_| method::supports_solvent(selected));
    config.method = selected;
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
    InvertUndefinedStereo(bool),
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
    table: std::cell::RefCell<Table>,
}

/// The table rows of one result, measured once.
#[derive(Default)]
pub(crate) struct Table {
    result: std::sync::Weak<CalcResult>,
    revision: (u64, u64),
    pub rows: Vec<Row>,
    pub stereo_note: Option<String>,
}

impl Table {
    pub fn rank(&self, step: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.step == step || row.mirror_step == Some(step))
    }
}

/// One kept conformer as the tab lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    /// Index in the saved trajectory, independent of the displayed rank.
    pub step: usize,
    pub delta_kcal: f64,
    /// ΔE by the minimizer that made the start, when it was a force field.
    pub start_kcal: Option<f64>,
    pub percent: f64,
    /// Symmetry-aware heavy-atom RMSD to the first conformer, Å.
    pub rmsd: Option<f64>,
    pub cluster: Option<u32>,
    /// Whether the drawing's stereocentres and double bonds hold; `None`
    /// when the check cannot run (a stale result, an error).
    pub stereo: Option<bool>,
    pub tetrahedral: Option<TetrahedralStereo>,
    pub mirror_step: Option<usize>,
}

impl Row {
    pub fn undefined(&self) -> &[UndefinedCentre] {
        self.tetrahedral
            .as_ref()
            .map_or(&[], |stereo| &stereo.undefined)
    }
}

pub(super) fn opposite_cip(cip: Option<&str>) -> Option<&'static str> {
    match cip {
        Some("R") => Some("S"),
        Some("S") => Some("R"),
        _ => None,
    }
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
    let Some(&reference_energy) = path.energies_hartree.first() else {
        return vec![];
    };
    let field = start_field(result).is_some();
    let ids: Vec<_> = atoms.iter().flatten().copied().collect();
    let rows: Vec<_> =
        reshiki_xtb_jobs::conformers::trajectory_rows(doc, atoms, path, reference_energy)
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let frame = (!stale).then(|| path.frame(i)).flatten();
                Row {
                    step: i,
                    delta_kcal: row.delta_kcal,
                    start_kcal: row.start_kcal.filter(|_| field),
                    percent: row.percent,
                    rmsd: row.rmsd,
                    cluster: row.cluster,
                    stereo: frame.as_ref().and_then(|frame| {
                        science::stereo_changes(doc, atoms, frame)
                            .ok()
                            .map(|changes| changes.is_empty())
                    }),
                    tetrahedral: frame.as_ref().and_then(|frame| {
                        let mut stereo =
                            science::tetrahedral_stereo(doc, &ids, Some((atoms, frame))).ok()?;
                        stereo.undefined.sort_by_key(|centre| {
                            atoms.iter().position(|&id| id == Some(centre.atom))
                        });
                        Some(stereo)
                    }),
                    mirror_step: None,
                }
            })
            .collect();
    #[cfg(not(target_arch = "wasm32"))]
    let rows = merge_mirrors(doc, atoms, path, rows);
    rows
}

#[cfg(not(target_arch = "wasm32"))]
fn merge_mirrors(
    doc: &reshiki::document::Document,
    atoms: &[Option<u64>],
    path: &qcdata::Trajectory,
    rows: Vec<Row>,
) -> Vec<Row> {
    use reshiki_optimize::conformers::rmsd;
    use reshiki_xtb_jobs::conformers::{KCAL_PER_HARTREE, populations, trajectory_alignment};

    if rows.len() < 2
        || rows.iter().all(|r| r.undefined().is_empty())
        || rows
            .iter()
            .any(|r| r.tetrahedral.as_ref().is_some_and(|s| s.has_defined))
    {
        return rows;
    }
    let Some((topology, autos)) = path
        .frame(0)
        .and_then(|first| trajectory_alignment(doc, atoms, &first))
    else {
        return rows;
    };
    let count = rows.len();
    let mut kept = Vec::with_capacity(count);
    let mut pending = rows.into_iter().map(Some).collect::<Vec<_>>().into_iter();
    while let Some(next) = pending.next() {
        let Some(mut row) = next else {
            continue;
        };
        let a = row.undefined();
        if let Some(frame) = path.frame(row.step).filter(|_| !a.is_empty()) {
            let pair = pending.as_mut_slice().iter_mut().find(|entry| {
                let Some(other) = entry.as_ref() else {
                    return false;
                };
                let b = other.undefined();
                (row.delta_kcal - other.delta_kcal).abs() <= 0.05
                    && a.len() == b.len()
                    && a.iter().zip(b).all(|(a, b)| {
                        a.atom == b.atom
                            && matches!(
                                (a.winding, b.winding),
                                (Some(1), Some(2)) | (Some(2), Some(1))
                            )
                    })
                    && path.frame(other.step).is_some_and(|mut mirror| {
                        for [x, _, _] in &mut mirror.positions_angstrom {
                            *x = -*x;
                        }
                        rmsd(
                            &topology,
                            &autos,
                            &frame.positions_angstrom,
                            &mirror.positions_angstrom,
                        )
                        .is_ok_and(|r| r <= 0.1)
                    })
            });
            row.mirror_step = pair.and_then(Option::take).map(|other| other.step);
        }
        kept.push(row);
    }
    if kept.len() < count {
        // Boltzmann fractions depend only on relative energies.
        let energies: Vec<_> = kept
            .iter()
            .map(|r| r.delta_kcal / KCAL_PER_HARTREE)
            .collect();
        for (row, population) in kept.iter_mut().zip(populations(&energies)) {
            row.percent = population * 100.0;
        }
    }
    kept
}

/// Give the previewed model of `doc` the other kept conformers of `shown`,
/// superposed on conformer `index`, to draw faint behind it.
pub(crate) fn overlay(
    doc: &mut reshiki::document::Document,
    shown: &super::palette::Shown<'_>,
    step: usize,
    rows: &[Row],
) {
    let result = &shown.stored.result;
    let Some(path) = result.trajectory.as_ref().filter(|_| is_search(result)) else {
        return;
    };
    let Some(reference) = path.frame(step) else {
        return;
    };
    let ghosts: Vec<Vec<[f32; 3]>> = rows
        .iter()
        .filter(|row| row.step != step && row.mirror_step != Some(step))
        .filter_map(|row| superposed(&reference, &path.frame(row.step)?))
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
