//! Bounded discovery of elementary steps from an actual mapped NEB profile.
//!
//! The backend owns electronic-structure work. This planner only selects real
//! band images, reuses verified relaxed IRC ends, and searches remaining gaps.

use std::{collections::BTreeMap, ops::ControlFlow};

use super::{Minimum, Reached, TsError, TsReport, endpoint, mapping, validate_geometry};
use crate::KCAL_PER_HARTREE;

/// Independent limits for the complete discovery, including the initial search.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub max_searches: usize,
    pub max_minima: usize,
    /// Combined bound on minimum relaxation and tight identity normalization calls.
    pub max_relaxations: usize,
    /// Each flanking interior peak must rise this far above a valley and its
    /// respective outer endpoint. Smaller fluctuations are treated as noise.
    pub valley_prominence_kcal: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_searches: 24,
            max_minima: 12,
            max_relaxations: 32,
            valley_prominence_kcal: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Started,
    Relaxing {
        /// An actual band image, or `None` for a relaxed IRC end.
        image: Option<usize>,
        relaxation: usize,
    },
    Searching {
        reactant: usize,
        product: usize,
        attempt: usize,
    },
    Finished,
}

/// Work supplied by the caller, using one consistent method and atom map.
pub trait Backend {
    type Verification;

    /// Relax and independently certify a minimum. `None` means the candidate
    /// did not converge to a stationary point without significant imaginary modes.
    fn relax(&mut self, seed: &Minimum) -> Result<Option<Minimum>, TsError>;

    /// Normalize an already relaxed minimum within its basin using stringent
    /// stationarity/displacement convergence. These coordinates are identity
    /// evidence only. Return `None` when soft modes or numerical precision leave
    /// the basin undecided; ordinary force convergence alone is insufficient.
    fn canonicalize(&mut self, seed: &Minimum) -> Result<Option<Minimum>, TsError>;

    /// Search the supplied, already relaxed endpoints without preparing or
    /// remapping them. A verification rejection must prevent IRC in the report.
    fn search(
        &mut self,
        reactant: &Minimum,
        product: &Minimum,
    ) -> Result<(TsReport, Self::Verification), TsError>;

    fn on_event(&mut self, _event: Event) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attempt<T> {
    /// Requested endpoints, even if IRC subsequently discovered other minima.
    pub reactant: usize,
    pub product: usize,
    /// For an accepted edge, endpoints and IRC classifications refer to that
    /// elementary edge. `path` retains the actual searched band, which can have
    /// different endpoints and must not be presented as a local edge's NEB.
    pub report: TsReport,
    pub verification: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub reactant: usize,
    pub product: usize,
    pub attempt: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub reactant: usize,
    pub product: usize,
    pub reason: String,
    pub attempt: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report<T> {
    /// Cancellation retains all work already verified and leaves explicit gaps.
    pub cancelled: bool,
    /// A fatal backend/input error stops further work without discarding
    /// verified results. Geometry-specific SCF failures remain local diagnostics.
    pub error: Option<TsError>,
    /// Stable IDs into this list are used throughout the report.
    pub minima: Vec<Minimum>,
    /// Ordered route, starting at minimum 0 and ending at minimum 1.
    pub order: Vec<usize>,
    /// Verified elementary edges, in route order.
    pub edges: Vec<Edge>,
    /// Unresolved adjacent pairs, in route order. An empty list certifies that
    /// every adjacent pair has a distinct-minimum, independently accepted IRC.
    pub gaps: Vec<Gap>,
    /// Successful and unsuccessful characterized candidates and their payloads.
    pub attempts: Vec<Attempt<T>>,
    pub diagnostics: Vec<String>,
}

impl<T> Report<T> {
    pub fn complete(&self) -> bool {
        self.gaps.is_empty()
    }
}

fn poll(backend: &mut impl Backend, event: Event) -> Result<(), TsError> {
    match backend.on_event(event) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(()) => Err(TsError::Cancelled),
    }
}

fn validate_minimum(numbers: &[u8], point: &Minimum) -> Result<(), TsError> {
    validate_geometry(numbers, &point.positions)?;
    endpoint::validate_nuclei_separation(&point.positions)?;
    if !point.energy.is_finite() {
        return Err(TsError::Invalid("discovery energies must be finite".into()));
    }
    Ok(())
}

fn validate_report(numbers: &[u8], report: &TsReport) -> Result<(), TsError> {
    if report.path.len() < 2 {
        return Err(TsError::Invalid(
            "discovery requires an actual path with both endpoints".into(),
        ));
    }
    for point in [&report.reactant, &report.product, &report.ts]
        .into_iter()
        .chain(&report.path)
        .chain(
            report
                .irc
                .iter()
                .flatten()
                .flat_map(|b| std::iter::once(&b.end).chain(&b.path)),
        )
    {
        validate_minimum(numbers, point)?;
    }
    Ok(())
}

fn rmsd(reference: &[f64], mobile: &[f64]) -> f64 {
    let weights = vec![1.; reference.len() / 3];
    mapping::superpose(reference, mobile, &weights)
        .iter()
        .zip(reference)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
        / (weights.len() as f64).sqrt()
}

/// Discovery uses a much stricter identity test than endpoint classification:
/// a labeled graph alone must never merge conformers or separated complexes.
fn same_minimum(numbers: &[u8], a: &Minimum, b: &Minimum) -> bool {
    let tolerance = 2.0 * 3.0_f64.sqrt() * crate::Level::Normal.criteria().max_step;
    plausible_identity(numbers, a, b) && rmsd(&a.positions, &b.positions) <= tolerance
}

fn plausible_identity(numbers: &[u8], a: &Minimum, b: &Minimum) -> bool {
    (a.energy - b.energy).abs() * KCAL_PER_HARTREE <= 0.1
        && endpoint::bonds_prevalidated(numbers, &a.positions)
            == endpoint::bonds_prevalidated(numbers, &b.positions)
}

fn known_minimum(numbers: &[u8], minima: &[Minimum], point: &Minimum) -> Option<usize> {
    // Ambiguous matches are not evidence that two existing minima are equal.
    let mut matches = minima
        .iter()
        .enumerate()
        .filter(|(_, reference)| same_minimum(numbers, reference, point));
    let first = matches.next()?.0;
    matches.next().is_none().then_some(first)
}

/// Return well-separated troughs between two prominent, distinct interior peaks.
fn valley_candidates(path: &[Minimum], prominence_kcal: f64) -> Vec<usize> {
    let prominence = prominence_kcal / KCAL_PER_HARTREE;
    let mut result: Vec<usize> = Vec::new();
    for i in 2..path.len().saturating_sub(2) {
        let energy = path[i].energy;
        if energy > path[i - 1].energy || energy > path[i + 1].energy {
            continue;
        }
        let left = path[1..i]
            .iter()
            .map(|p| p.energy)
            .fold(f64::NEG_INFINITY, f64::max);
        let right = path[i + 1..path.len() - 1]
            .iter()
            .map(|p| p.energy)
            .fold(f64::NEG_INFINITY, f64::max);
        if left - energy.max(path[0].energy) < prominence
            || right - energy.max(path[path.len() - 1].energy) < prominence
        {
            continue;
        }
        if let Some(previous) = result.last_mut() {
            let separator = path[*previous..=i]
                .iter()
                .map(|p| p.energy)
                .fold(f64::NEG_INFINITY, f64::max);
            if separator - energy.max(path[*previous].energy) < prominence {
                if energy < path[*previous].energy {
                    *previous = i;
                }
                continue;
            }
        }
        result.push(i);
    }
    result
}

/// Locate a minimum along the supplied band using fitted Cartesian segment
/// projections. This chooses route order, never minimum identity or connectivity.
fn progress(path: &[Minimum], point: &Minimum) -> f64 {
    let weights = vec![1.; point.positions.len() / 3];
    let mut best = (f64::INFINITY, 0.);
    for (i, pair) in path.windows(2).enumerate() {
        let origin = &pair[0].positions;
        let end = mapping::superpose(origin, &pair[1].positions, &weights);
        let mobile = mapping::superpose(origin, &point.positions, &weights);
        let length = end
            .iter()
            .zip(origin)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>();
        let t = if length > 0. {
            (mobile
                .iter()
                .zip(origin)
                .zip(&end)
                .map(|((x, a), b)| (x - a) * (b - a))
                .sum::<f64>()
                / length)
                .clamp(0., 1.)
        } else {
            0.
        };
        let loss = mobile
            .iter()
            .zip(origin)
            .zip(&end)
            .map(|((x, a), b)| (x - a - t * (b - a)).powi(2))
            .sum::<f64>();
        if loss < best.0 {
            best = (loss, i as f64 + t);
        }
    }
    best.1
}

struct Planner<'a, T> {
    numbers: &'a [u8],
    options: &'a Options,
    result: Report<T>,
    relaxations: usize,
    /// Tight normalization is cached and never replaces a certified geometry.
    canonical: BTreeMap<usize, Option<Minimum>>,
}

impl<T> Planner<'_, T> {
    fn fail(&mut self, error: TsError) {
        self.result
            .diagnostics
            .push(format!("discovery stopped: {error}"));
        self.result.error = Some(error);
    }

    fn relaxation_event(
        &mut self,
        image: Option<usize>,
        backend: &mut impl Backend<Verification = T>,
    ) -> bool {
        if self.result.cancelled || self.result.error.is_some() {
            return false;
        }
        if self.relaxations >= self.options.max_relaxations {
            self.result
                .diagnostics
                .push("discovery relaxation limit reached".into());
            return false;
        }
        self.relaxations += 1;
        if poll(
            backend,
            Event::Relaxing {
                image,
                relaxation: self.relaxations,
            },
        )
        .is_err()
        {
            self.result.cancelled = true;
            return false;
        }
        true
    }

    fn canonicalize(
        &mut self,
        seed: &Minimum,
        known: Option<usize>,
        backend: &mut impl Backend<Verification = T>,
    ) -> Option<Minimum> {
        if let Some(cached) = known.and_then(|id| self.canonical.get(&id)) {
            return cached.clone();
        }
        if !self.relaxation_event(None, backend) {
            return None;
        }
        let canonical = match backend.canonicalize(seed) {
            Ok(Some(point)) => {
                if let Err(error) = validate_minimum(self.numbers, &point) {
                    self.fail(error);
                    return None;
                }
                if endpoint::bonds_prevalidated(self.numbers, &seed.positions)
                    != endpoint::bonds_prevalidated(self.numbers, &point.positions)
                {
                    self.result.diagnostics.push("minimum normalization changed its mapped bond graph; basin identity is unresolved".into());
                    None
                } else {
                    Some(point)
                }
            }
            Ok(None) => {
                self.result
                    .diagnostics
                    .push("stringent minimum normalization did not resolve basin identity".into());
                None
            }
            Err(TsError::Cancelled) => {
                self.result.cancelled = true;
                return None;
            }
            Err(error @ TsError::Scf(_)) => {
                self.result
                    .diagnostics
                    .push(format!("minimum normalization: {error}"));
                None
            }
            Err(error) => {
                self.fail(error);
                return None;
            }
        };
        if let Some(id) = known {
            self.canonical.insert(id, canonical.clone());
        }
        canonical
    }

    /// Strictly compare normalized minima, never a loose structural tolerance.
    /// An undecided plausible reference prevents certification of a new state.
    fn canonical_match(
        &mut self,
        seed: &Minimum,
        canonical: &Minimum,
        backend: &mut impl Backend<Verification = T>,
    ) -> Option<Option<usize>> {
        let references: Vec<_> = self
            .result
            .minima
            .iter()
            .enumerate()
            .filter(|(_, minimum)| {
                plausible_identity(self.numbers, seed, minimum)
                    || plausible_identity(self.numbers, canonical, minimum)
            })
            .map(|(id, minimum)| (id, minimum.clone()))
            .collect();
        let mut matched = None;
        for (id, reference) in references {
            let normalized = self.canonicalize(&reference, Some(id), backend)?;
            if same_minimum(self.numbers, canonical, &normalized) {
                if matched.is_some() {
                    self.result.diagnostics.push("minimum normalization matches multiple registered minima; identity is unresolved".into());
                    return None;
                }
                matched = Some(id);
            }
        }
        Some(matched)
    }

    fn minimum(
        &mut self,
        seed: &Minimum,
        image: Option<usize>,
        backend: &mut impl Backend<Verification = T>,
    ) -> Option<usize> {
        if self.result.cancelled || self.result.error.is_some() {
            return None;
        }
        if let Some(id) = known_minimum(self.numbers, &self.result.minima, seed) {
            return Some(id);
        }
        // An IRC end is already relaxed. Establish its basin before any extra
        // minimization, which must not silently move it into another basin.
        let mut canonical = if image.is_none() {
            let normalized = self.canonicalize(seed, None, backend)?;
            if let Some(id) = self.canonical_match(seed, &normalized, backend)? {
                return Some(id);
            }
            Some(normalized)
        } else {
            None
        };
        if !self.relaxation_event(image, backend) {
            return None;
        }
        let minimum = match backend.relax(seed) {
            Ok(Some(minimum)) => minimum,
            Ok(None) => {
                self.result
                    .diagnostics
                    .push("candidate did not verify as a minimum".into());
                return None;
            }
            Err(TsError::Cancelled) => {
                self.result.cancelled = true;
                return None;
            }
            Err(error @ TsError::Scf(_)) => {
                self.result
                    .diagnostics
                    .push(format!("candidate relaxation: {error}"));
                return None;
            }
            Err(error) => {
                self.fail(error);
                return None;
            }
        };
        if let Err(error) = validate_minimum(self.numbers, &minimum) {
            self.fail(error);
            return None;
        }
        if let Some(before) = &canonical {
            if !same_minimum(self.numbers, seed, &minimum) {
                let after = self.canonicalize(&minimum, None, backend)?;
                if !same_minimum(self.numbers, before, &after) {
                    self.result.diagnostics.push("additional IRC endpoint relaxation changed basin; the branch is unresolved".into());
                    return None;
                }
            }
        } else {
            if let Some(id) = known_minimum(self.numbers, &self.result.minima, &minimum) {
                return Some(id);
            }
            if self
                .result
                .minima
                .iter()
                .any(|reference| plausible_identity(self.numbers, &minimum, reference))
            {
                let normalized = self.canonicalize(&minimum, None, backend)?;
                if let Some(id) = self.canonical_match(&minimum, &normalized, backend)? {
                    return Some(id);
                }
                canonical = Some(normalized);
            }
        }
        if self.result.minima.len() >= self.options.max_minima {
            self.result
                .diagnostics
                .push("discovery minimum limit reached".into());
            return None;
        }
        let id = self.result.minima.len();
        self.result.minima.push(minimum);
        if let Some(normalized) = canonical {
            self.canonical.insert(id, Some(normalized));
        }
        Some(id)
    }

    fn process(
        &mut self,
        reactant: usize,
        product: usize,
        report: TsReport,
        verification: T,
        backend: &mut impl Backend<Verification = T>,
    ) -> (usize, String) {
        let attempt = self.result.attempts.len();
        if let Err(error) = validate_report(self.numbers, &report) {
            let reason = error.to_string();
            self.fail(error);
            self.result.attempts.push(Attempt {
                reactant,
                product,
                report,
                verification,
            });
            return (attempt, reason);
        }
        let mut insertions = Vec::new();
        for image in valley_candidates(&report.path, self.options.valley_prominence_kcal) {
            if let Some(id) = self.minimum(&report.path[image], Some(image), backend) {
                insertions.push((id, image as f64));
            }
            if self.result.cancelled || self.result.error.is_some() {
                break;
            }
        }
        let mut branch_ids = None;
        let mut reason = if !report.saddle_converged {
            "saddle refinement did not converge".to_string()
        } else if report.imaginary != 1 {
            "candidate does not have exactly one significant imaginary frequency".to_string()
        } else {
            "candidate has no independently accepted two-branch IRC".to_string()
        };
        if !self.result.cancelled
            && self.result.error.is_none()
            && report.saddle_converged
            && report.imaginary == 1
            && let Some(branches) = &report.irc
        {
            if branches
                .iter()
                .all(|b| b.path.first().is_some_and(|p| p.energy < report.ts.energy))
            {
                let ends = branches.each_ref().map(|b| &b.end);
                let first = self.minimum(ends[0], None, backend);
                let second = self.minimum(ends[1], None, backend);
                for id in [first, second].into_iter().flatten() {
                    insertions.push((id, progress(&report.path, &self.result.minima[id])));
                }
                match (first, second) {
                    (Some(a), Some(b)) if a != b => {
                        branch_ids = Some([a, b]);
                    }
                    (Some(_), Some(_)) => {
                        reason = "both IRC branches reach the same minimum; no progress".into()
                    }
                    _ => reason = "IRC endpoints did not verify as distinct minima".into(),
                }
            } else {
                reason = "IRC has no downhill evidence in both directions".into();
            }
        }
        if self.result.cancelled {
            reason = "discovery cancelled".into();
        } else if let Some(error) = &self.result.error {
            reason = format!("discovery stopped: {error}");
        }

        let position = self
            .result
            .order
            .iter()
            .position(|&id| id == reactant)
            .unwrap();
        debug_assert_eq!(self.result.order[position + 1], product);
        insertions.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let mut local = vec![reactant];
        for (id, _) in insertions {
            if id == reactant || id == product || local.contains(&id) {
                continue;
            }
            if self.result.order.contains(&id) {
                self.result.diagnostics.push(format!(
                    "minimum {id} lies outside requested gap {reactant}–{product}; route not reordered"
                ));
                continue;
            }
            local.push(id);
        }
        local.push(product);
        self.result.order.splice(position..=position + 1, local);
        self.result.attempts.push(Attempt {
            reactant,
            product,
            report,
            verification,
        });

        if let Some([a, b]) = branch_ids {
            if let Some(pair) = self
                .result
                .order
                .windows(2)
                .find(|pair| (pair[0] == a && pair[1] == b) || (pair[0] == b && pair[1] == a))
            {
                let [left, right] = [pair[0], pair[1]];
                if !self
                    .result
                    .edges
                    .iter()
                    .any(|e| e.reactant == left && e.product == right)
                {
                    let report = &mut self.result.attempts[attempt].report;
                    report.reactant = self.result.minima[left].clone();
                    report.product = self.result.minima[right].clone();
                    for (branch, id) in report.irc.as_mut().unwrap().iter_mut().zip([a, b]) {
                        let minimum = &self.result.minima[id];
                        // Keep each IRC endpoint in its actual trace frame.
                        // Registry geometries can come from a different segment.
                        branch.end = Minimum {
                            positions: mapping::superpose(
                                &branch.end.positions,
                                &minimum.positions,
                                &vec![1.; self.numbers.len()],
                            ),
                            energy: minimum.energy,
                        };
                        if let Some(last) = branch.energies.last_mut() {
                            *last = minimum.energy;
                        }
                        branch.reached = if id == left {
                            Reached::Reactant
                        } else {
                            Reached::Product
                        };
                    }
                    report.connects = true;
                    self.result.edges.push(Edge {
                        reactant: left,
                        product: right,
                        attempt,
                    });
                }
            } else {
                reason = "IRC connects nonadjacent discovered minima; adjacent gaps remain".into();
            }
        }
        (attempt, reason)
    }
}

/// Discover a bounded sequence from one completed search, retaining every
/// characterized attempt. Failed optional work leaves explicit gaps. Cancellation
/// stops calculations immediately and returns the verified partial report with
/// `cancelled` set, including the initial attempt even before its inspection.
pub fn discover<T>(
    numbers: &[u8],
    initial: (TsReport, T),
    options: &Options,
    backend: &mut impl Backend<Verification = T>,
) -> Result<Report<T>, TsError> {
    if options.max_searches == 0
        || options.max_minima < 2
        || !options.valley_prominence_kcal.is_finite()
        || options.valley_prominence_kcal <= 0.
    {
        return Err(TsError::Invalid(
            "discovery requires positive search and prominence limits and at least two minima"
                .into(),
        ));
    }
    validate_report(numbers, &initial.0)?;
    if same_minimum(numbers, &initial.0.reactant, &initial.0.product) {
        return Err(TsError::Invalid(
            "discovery endpoints identify the same minimum".into(),
        ));
    }
    let cancelled = poll(backend, Event::Started).is_err();
    let mut planner = Planner {
        numbers,
        options,
        result: Report {
            cancelled,
            error: None,
            minima: vec![initial.0.reactant.clone(), initial.0.product.clone()],
            order: vec![0, 1],
            edges: Vec::new(),
            gaps: Vec::new(),
            attempts: Vec::new(),
            diagnostics: Vec::new(),
        },
        relaxations: 0,
        canonical: BTreeMap::new(),
    };
    let mut tried = BTreeMap::new();
    let mut searches = 1;
    let (attempt, reason) = if cancelled {
        planner.result.attempts.push(Attempt {
            reactant: 0,
            product: 1,
            report: initial.0,
            verification: initial.1,
        });
        (0, "discovery cancelled".into())
    } else {
        planner.process(0, 1, initial.0, initial.1, backend)
    };
    tried.insert((0, 1), (Some(attempt), reason));
    loop {
        if planner.result.cancelled || planner.result.error.is_some() {
            break;
        }
        let next = planner.result.order.windows(2).find_map(|pair| {
            let key = (pair[0], pair[1]);
            (!tried.contains_key(&key)
                && !planner
                    .result
                    .edges
                    .iter()
                    .any(|e| (e.reactant, e.product) == key))
            .then_some(key)
        });
        let Some((reactant, product)) = next else {
            break;
        };
        if searches >= options.max_searches {
            break;
        }
        searches += 1;
        if poll(
            backend,
            Event::Searching {
                reactant,
                product,
                attempt: searches,
            },
        )
        .is_err()
        {
            planner.result.cancelled = true;
            break;
        }
        match backend.search(
            &planner.result.minima[reactant],
            &planner.result.minima[product],
        ) {
            Ok((report, verification)) => {
                let (attempt, reason) =
                    planner.process(reactant, product, report, verification, backend);
                tried.insert((reactant, product), (Some(attempt), reason));
            }
            Err(TsError::Cancelled) => {
                planner.result.cancelled = true;
                tried.insert((reactant, product), (None, "discovery cancelled".into()));
            }
            Err(error @ TsError::Scf(_)) => {
                tried.insert((reactant, product), (None, error.to_string()));
            }
            Err(error) => {
                tried.insert((reactant, product), (None, error.to_string()));
                planner.fail(error);
            }
        }
    }
    for pair in planner.result.order.windows(2) {
        let key = (pair[0], pair[1]);
        if planner
            .result
            .edges
            .iter()
            .any(|e| (e.reactant, e.product) == key)
        {
            continue;
        }
        let (attempt, reason) = tried.get(&key).cloned().unwrap_or_else(|| {
            (
                None,
                if planner.result.cancelled {
                    "discovery cancelled".into()
                } else if let Some(error) = &planner.result.error {
                    format!("discovery stopped: {error}")
                } else {
                    "discovery search limit reached".into()
                },
            )
        });
        planner.result.gaps.push(Gap {
            reactant: key.0,
            product: key.1,
            reason,
            attempt,
        });
    }
    planner.result.edges.sort_by_key(|edge| {
        planner
            .result
            .order
            .iter()
            .position(|&id| id == edge.reactant)
            .unwrap()
    });
    if poll(backend, Event::Finished).is_err() {
        planner.result.cancelled = true;
    }
    Ok(planner.result)
}

#[cfg(test)]
mod tests;
