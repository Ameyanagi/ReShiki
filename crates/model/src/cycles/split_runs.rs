//! Relative topology of child steps cut from one undeformed source span.
//!
//! Native v40 readers default missing runs to empty. Runs store only the
//! original Bow/lane mode, normalized outline baseline and dimensionless
//! cut stations. The current frame maps that frozen baseline without late
//! labels changing its endpoints. An older v40
//! writer does not understand this optional field and can discard it on
//! save. Editable exchange keeps the complete cycle in its native metadata;
//! ordinary curve/mesh export materializes the resolved painted children.
//!
//! Insert, remove and reverse own relative slot changes. A cut drag slides
//! only while both adjacent children's required lanes still clip its body;
//! otherwise normal take-out coalesces it. Host/outer edits rebase the source
//! and transport affected cuts on the preview candidate. Independent child
//! Bow/reversible edits, Circle display and Tidy reflow the affected cuts
//! onto the selected native perimeter and retire their source family. Whole
//! copy retains relative slots; partial selection normalizes only the copy.
use super::{Cycle, MAX_PARTS, Step};
use serde::{Deserialize, Serialize};

/// A contiguous cyclic run of `cuts.len()+1` child steps. Stations refer to
/// the directed forward, outer-trimmed, pre-Bow and pre-arrowhead baseline.
/// Relative slots need no extra atom/object-ID remapping when copied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitRun {
    pub first_step: usize,
    pub cuts: Vec<f64>,
    /// Canonical clockwise outline fraction at the source baseline start.
    pub source_start: f64,
    /// Signed outline fraction of the outer-trimmed source baseline.
    pub source_span: f64,
    pub source_bow_pt: f32,
    pub source_reversible: bool,
}

/// A full-source cut station and capture values for a newly created run.
/// Existing runs retain their stored baseline and use only `u`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SplitStation {
    pub(super) u: f64,
    pub(super) start: f64,
    pub(super) span: f64,
}

impl SplitRun {
    pub(super) fn last_step(&self, n: usize) -> usize {
        (self.first_step + self.cuts.len()) % n
    }

    pub(super) fn end_slot(&self, n: usize) -> usize {
        (self.last_step(n) + 1) % n
    }

    fn child(&self, step: usize, n: usize) -> Option<usize> {
        if n == 0 || step >= n || self.first_step >= n || self.cuts.len() >= n {
            return None;
        }
        let child = (step + n - self.first_step) % n;
        (child <= self.cuts.len()).then_some(child)
    }
}

/// A surviving former cut needing document-level reflow after its source
/// endpoint is removed. Indices already refer to the shortened cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RetiredCut {
    pub index: usize,
    pub u: f64,
}

fn stations(cuts: &[f64]) -> bool {
    fn increasing(mut values: impl Iterator<Item = f64>) -> bool {
        let mut previous = 0.;
        values.all(|u| {
            let valid = u.is_finite() && u > previous && u < 1.;
            previous = u;
            valid
        })
    }
    increasing(cuts.iter().copied())
        // Reverse must not round a strictly positive cut to 1, or collapse
        // distinct cuts. Reject unaddressable metadata before it is stored.
        && increasing(cuts.iter().rev().map(|&u| 1. - u))
}

fn source_baseline(start: f64, span: f64) -> bool {
    start.is_finite()
        && (0. ..1.).contains(&start)
        && span.is_finite()
        && span > -1.
        && span < 1.
        && span != 0.
}

impl Cycle {
    pub(super) fn validate_split_runs(&self) -> Result<(), String> {
        let n = self.intermediates.len();
        if self.shape == super::Shape::Circle && !self.split_runs.is_empty() {
            return Err("Circle cycles do not store fitted step runs".into());
        }
        if n > MAX_PARTS || self.split_runs.len() > MAX_PARTS / 2 {
            return Err("A cycle has too many split runs".into());
        }
        let mut occupied = vec![false; n];
        let mut work = 0;
        for run in &self.split_runs {
            if run.first_step >= n
                || run.cuts.is_empty()
                || run.cuts.len() >= n
                || !stations(&run.cuts)
                || !source_baseline(run.source_start, run.source_span)
                || !run.source_bow_pt.is_finite()
                || !(-720. ..=720.).contains(&run.source_bow_pt)
            {
                return Err(
                    "A cycle split run has invalid slots, stations, source baseline or Bow".into(),
                );
            }
            work += run.cuts.len() + 1;
            if work > MAX_PARTS {
                return Err("Cycle split runs exceed the step budget".into());
            }
            for child in 0..=run.cuts.len() {
                let step = (run.first_step + child) % n;
                let Some(occupied) = occupied.get_mut(step) else {
                    return Err("A cycle split run has an invalid child slot".into());
                };
                if std::mem::replace(occupied, true) {
                    return Err("Cycle split runs must not overlap steps".into());
                }
            }
        }
        Ok(())
    }

    /// The child ordinal follows the cycle's logical direction, once.
    pub(super) fn split_run(&self, step: usize) -> Option<(&SplitRun, usize)> {
        let n = self.steps.len();
        if step >= n {
            return None;
        }
        self.split_runs
            .iter()
            .find_map(|run| Some((run, run.child(step, n)?)))
    }

    /// Insert a member, optionally at a station in the complete donor run.
    /// The station is global within that run, rather than local to its child.
    /// A rejected station or full cycle leaves every field unchanged.
    pub(super) fn insert_intermediate(
        &mut self,
        i: usize,
        part: Vec<u64>,
        station: Option<SplitStation>,
    ) -> bool {
        let mut planned = self.clone();
        if !planned.insert_planned(i, part, station) {
            return false;
        }
        *self = planned;
        true
    }

    fn insert_planned(&mut self, i: usize, part: Vec<u64>, station: Option<SplitStation>) -> bool {
        let n = self.intermediates.len();
        if n >= MAX_PARTS || self.validate().is_err() {
            return false;
        }
        let i = i.min(n);
        if station.is_some_and(|s| !stations(&[s.u]) || self.shape == super::Shape::Circle) {
            return false;
        }
        let donor = (n > 0).then(|| (i + n - 1) % n);
        let member = donor.and_then(|step| {
            self.split_runs
                .iter()
                .enumerate()
                .find_map(|(r, run)| Some((r, run.child(step, n)?)))
        });
        let mut child = Step::default();
        let source = if let (Some(station), Some(donor)) = (station, donor) {
            let u = station.u;
            if let Some((r, ordinal)) = member {
                let Some(run) = self.split_runs.get(r) else {
                    return false;
                };
                let lo = if let Some(j) = ordinal.checked_sub(1) {
                    let Some(&u) = run.cuts.get(j) else {
                        return false;
                    };
                    u
                } else {
                    0.
                };
                let hi = run.cuts.get(ordinal).copied().unwrap_or(1.);
                let mut cuts = run.cuts.clone();
                cuts.insert(ordinal, u);
                if u <= lo || u >= hi || !stations(&cuts) {
                    return false;
                }
            } else if !source_baseline(station.start, station.span) {
                return false;
            }
            let Some(step) = self.steps.get_mut(donor) else {
                return false;
            };
            let source = (step.bow_pt, step.reversible);
            child.trail_pt = step.trail_pt;
            step.trail_pt = 0.;
            Some(source)
        } else {
            // A legacy/explicit insertion without a station cannot safely
            // retain a donor run with an unknown extra child boundary.
            if let Some((r, _)) = member {
                self.split_runs.remove(r);
            }
            None
        };
        for run in &mut self.split_runs {
            if run.first_step >= i {
                run.first_step += 1;
            }
        }
        self.intermediates.insert(i, part);
        self.steps.insert(i, child);
        if let (Some(station), Some(donor), Some((bow, reversible))) = (station, donor, source) {
            if let Some((r, ordinal)) = member {
                let Some(run) = self.split_runs.get_mut(r) else {
                    return false;
                };
                run.cuts.insert(ordinal, station.u);
            } else {
                self.split_runs.push(SplitRun {
                    first_step: donor + usize::from(donor >= i),
                    cuts: vec![station.u],
                    source_start: if station.start == 0. {
                        0.
                    } else {
                        station.start
                    },
                    source_span: station.span,
                    source_bow_pt: bow,
                    source_reversible: reversible,
                });
            }
        }
        debug_assert!(self.validate().is_ok());
        true
    }

    /// Coalesce the adjacent steps. Retired source endpoints return their
    /// surviving internal cuts for the document's separate geometry reflow.
    pub(super) fn remove_intermediate(&mut self, i: usize) -> Vec<RetiredCut> {
        let n = self.intermediates.len();
        if i >= n || self.steps.len() != n || self.validate_split_runs().is_err() {
            return vec![];
        }
        let map = |slot: usize| slot - usize::from(slot > i);
        let mut retired = vec![];
        let mut internal = false;
        let mut restore = None;
        let mut kept = vec![];
        for mut run in self.split_runs.iter().cloned() {
            if i == run.first_step || i == run.end_slot(n) {
                for (j, &u) in run.cuts.iter().enumerate() {
                    let slot = (run.first_step + j + 1) % n;
                    if slot != i {
                        retired.push(RetiredCut {
                            index: map(slot),
                            u,
                        });
                    }
                }
                continue;
            }
            if let Some(child) = run.child(i, n).filter(|child| *child > 0) {
                internal = true;
                let (Some(first), Some(last)) = (
                    self.steps.get(run.first_step),
                    self.steps.get(run.last_step(n)),
                ) else {
                    return vec![];
                };
                let (outer_lead, outer_trail) = (first.lead_pt, last.trail_pt);
                run.cuts.remove(child - 1);
                if run.cuts.is_empty() {
                    restore = Some((
                        run.source_bow_pt,
                        run.source_reversible,
                        outer_lead,
                        outer_trail,
                    ));
                    continue;
                }
            }
            run.first_step = map(run.first_step);
            kept.push(run);
        }
        self.split_runs = kept;
        self.intermediates.remove(i);
        let gone = self.steps.remove(i);
        if let Some(before) = self
            .steps
            .len()
            .checked_sub(1)
            .map(|last| if i == 0 { last } else { i - 1 })
            && let Some(step) = self.steps.get_mut(before)
        {
            // The old source owner becomes a later child after Reverse. If
            // that child coalesces, preserve its retained geometry settings.
            if internal {
                step.trail_pt = gone.trail_pt;
                if step.bow_pt == 0. {
                    step.bow_pt = gone.bow_pt;
                }
                step.reversible |= gone.reversible;
            }
            step.reagents.extend(gone.reagents);
            step.products.extend(gone.products);
            if step.name.trim().is_empty() {
                step.name = gone.name;
            }
            if let Some((bow, reversible, lead, trail)) = restore {
                step.bow_pt = bow;
                step.reversible = reversible;
                step.lead_pt = lead;
                step.trail_pt = trail;
            }
        }
        debug_assert!(self.validate().is_ok());
        retired
    }

    pub(super) fn reverse_split_runs(&mut self) {
        let n = self.steps.len();
        for run in &mut self.split_runs {
            run.first_step = (n - run.end_slot(n)) % n;
            let start = (run.source_start + run.source_span).rem_euclid(1.);
            run.source_start = if start >= 1. || start == 0. {
                0.
            } else {
                start
            };
            run.source_span = -run.source_span;
            run.cuts.reverse();
            for u in &mut run.cuts {
                *u = 1. - *u;
            }
        }
        debug_assert!(self.validate_split_runs().is_ok());
    }
}

#[cfg(test)]
#[path = "split_run_tests.rs"]
mod tests;
