//! The scheme's common-reference figure uses the same native graphic, model
//! placement, link editing and undo path as a single-arrow energy profile.
use super::plan::preview;
use super::{
    plan::{Level, Plan, Term, sum_energy},
    run::{Report, StepValue},
};
use crate::app::{
    App,
    xtb::ts::{irc, profile, thumbnails},
};
use reshiki::document::{Document, Point};
use reshiki::graphics::{
    EnergyProfile, EnergyProfileGeometry, EnergyProfileKind, EnergyProfileStateKind as Kind,
    EnergyProfileStep, EnergyUnit, Graphic, GraphicStyle,
};
use reshiki::qcdata::{CalcResult, Frame, KCAL_PER_HARTREE};
use std::sync::Arc;

/// Every level describes the same chemical inventory, including co-reactants
/// waiting for later steps and coproducts retained from earlier steps.
pub(super) fn build(
    doc: &Document,
    plan: &Plan,
    report: &Report,
    gibbs: bool,
    unit: EnergyUnit,
) -> Option<EnergyProfile> {
    let levels = profile_levels(plan, report, gibbs)?;
    let first = plan.steps.first()?;
    let reaction = preview(doc, first.arrow).ok()?;
    let (initial, _) = levels.first()?;
    let mut reactant = profile::state(Kind::Reactant, initial.label(), 0.);
    reactant.geometry = side_geometry(plan, report, initial.terms(), None);
    let mut states = vec![reactant];
    let mut steps = Vec::new();
    let mut rotation = None;
    for (index, step) in plan.steps.iter().enumerate() {
        let value = report.steps.get(index)?.as_ref()?;
        let (inventory, level) = levels.get(index)?;
        let (minimum, next_level) = levels.get(index + 1)?;
        let (level, next_level) = (*level, *next_level);
        let kind = if index + 1 == plan.steps.len() {
            Kind::Product
        } else {
            Kind::Intermediate
        };
        let mut state = profile::state(kind, minimum.label(), next_level);
        state.geometry = side_geometry(plan, report, minimum.terms(), None);
        if let Some(path) = &value.discovered {
            let spectators = inventory.spectators(&step.reactants)?;
            // Use exactly the same separated-species reference as the ordinary scheme barrier.
            let reference = sum_energy(step.reactants.iter().copied(), |index| {
                super::run::species_energy(report.species(index)?, gibbs)
            })? - level / KCAL_PER_HARTREE;
            let reaction = preview(doc, step.arrow).ok()?;
            let source_key = profile::source_key(doc, &reaction);
            let mut product = Some(state);
            for (leg, pair) in path.order.array_windows::<2>().enumerate() {
                let minimum = path.minima.get(pair[1])?;
                let to_state = if leg + 2 == path.order.len() {
                    product.take()?
                } else {
                    let mut intermediate = profile::state(
                        Kind::Intermediate,
                        format!("I{}.{}", index + 1, leg + 1),
                        (minimum.energy_hartree
                            + if gibbs {
                                minimum.gibbs_correction_hartree?
                            } else {
                                0.
                            }
                            - reference)
                            * KCAL_PER_HARTREE,
                    );
                    let geometry = thumbnails::profile_side(&[&minimum.frame]);
                    intermediate.geometry = geometry.and_then(|geometry| {
                        side_geometry(plan, report, spectators.terms(), Some(geometry))
                    });
                    intermediate
                };
                let from = states.len() - 1;
                let from_level = states.last()?.energy_kcal;
                let segment = path
                    .segments
                    .iter()
                    .find(|s| [s.reactant, s.product] == *pair);
                let mut transition = None;
                let mut samples = Vec::new();
                if let Some(segment) = segment {
                    let result = &segment.ts;
                    let saddle = result.energy_hartree?
                        + if gibbs {
                            result.thermo.as_ref()?.gibbs_correction_hartree
                        } else {
                            0.
                        };
                    let height = (saddle - reference) * KCAL_PER_HARTREE;
                    let mut state = profile::state(
                        Kind::TransitionState,
                        format!("TS{}.{}‡", index + 1, leg + 1),
                        height,
                    );
                    if let Some((geometry, view)) = saddle_geometry(result, &segment.frames) {
                        state.geometry =
                            side_geometry(plan, report, spectators.terms(), Some(geometry));
                        rotation.get_or_insert(view);
                    }
                    if !gibbs && let Some(irc) = irc::saved(result) {
                        samples = profile::irc_samples(
                            irc,
                            reference + from_level / KCAL_PER_HARTREE,
                            height - from_level,
                            to_state.energy_kcal - from_level,
                        );
                        for sample in &mut samples {
                            sample.energy_kcal += from_level;
                        }
                    }
                    transition = Some(states.len());
                    states.push(state);
                }
                states.push(to_state);
                let later = !steps.is_empty();
                steps.push(EnergyProfileStep {
                    from,
                    to: states.len() - 1,
                    transition,
                    unresolved: segment.is_none(),
                    source: later.then_some(step.arrow),
                    source_key: later.then(|| source_key.clone()),
                    irc: samples,
                });
            }
            continue;
        }
        let delta = next_level - level;
        let from = states.len() - 1;
        let barrier = if gibbs {
            value.free_barrier
        } else {
            value.barrier.or_else(|| {
                let result = value.ts.as_ref()?;
                if result.status != reshiki::qcdata::Status::Converged {
                    return None;
                }
                let saddle = result.energy_hartree?;
                let reactant = sum_energy(step.reactants.iter().copied(), |index| {
                    super::run::species_energy(report.species(index)?, false)
                })?;
                Some((saddle - reactant) * KCAL_PER_HARTREE)
            })
        }
        .filter(|value| value.is_finite());
        let transition = barrier.map(|barrier| {
            let state = profile::state(
                Kind::TransitionState,
                format!("TS{}‡", index + 1),
                level + barrier,
            );
            states.push(state);
            states.len() - 1
        });
        let mut samples = Vec::new();
        if let Some(result) = &value.ts {
            if let Some(transition) = transition
                && let Some((ts, view)) = saddle_geometry(result, &value.frames)
            {
                let spectators = inventory.spectators(&step.reactants)?;
                states.get_mut(transition)?.geometry =
                    side_geometry(plan, report, spectators.terms(), Some(ts));
                rotation.get_or_insert(view);
            }
            // IRC energies are electronic samples; Gibbs corrections exist at
            // the stationary states only, so they do not define a Gibbs trace.
            if !gibbs
                && let (Some(path), Some(saddle), Some(barrier)) =
                    (irc::saved(result), result.energy_hartree, barrier)
            {
                samples =
                    profile::irc_samples(path, saddle - barrier / KCAL_PER_HARTREE, barrier, delta);
                for sample in &mut samples {
                    sample.energy_kcal += level;
                }
            }
        }
        states.push(state);
        let current = preview(doc, step.arrow).ok()?;
        steps.push(EnergyProfileStep {
            unresolved: false,
            from,
            to: states.len() - 1,
            transition,
            source: (index > 0).then_some(step.arrow),
            source_key: (index > 0).then(|| profile::source_key(doc, &current)),
            irc: samples,
        });
    }
    let method = report.species.first()?.method.label().into();
    let mut figure = EnergyProfile::new(
        first.arrow,
        profile::source_key(doc, &reaction),
        method,
        states,
        steps,
    );
    figure.energy_kind = if gibbs {
        EnergyProfileKind::Gibbs
    } else {
        EnergyProfileKind::Electronic
    };
    figure.unit = unit;
    if let Some(rotation) = rotation {
        figure.rotation = rotation;
    }
    Some(figure)
}

fn saddle_geometry(
    result: &CalcResult,
    frames: &[Arc<Frame>],
) -> Option<(EnergyProfileGeometry, [f32; 4])> {
    let ends = result.endpoints?;
    let frame = |hash| {
        frames
            .iter()
            .find(|frame| frame.hash() == hash)
            .map(AsRef::as_ref)
    };
    let ([_, ts, _], view) = thumbnails::profile_geometry_from_frames(
        result,
        frame(ends.reactant)?,
        frame(result.frame)?,
        frame(ends.product)?,
    )?;
    Some((ts, view))
}

fn profile_levels(plan: &Plan, report: &Report, gibbs: bool) -> Option<Vec<(Level, f64)>> {
    // The intermediate geometries carry electronic energies. A Gibbs profile
    // requires a thermal correction for every minimum on the expanded path.
    if gibbs
        && report
            .steps
            .iter()
            .flatten()
            .filter_map(|step| step.discovered.as_ref())
            .any(|path| !path.has_gibbs())
    {
        return None;
    }
    let mut reference = None;
    energy_levels(plan, report, |level, _| {
        let energy = sum_energy(level.terms(), |index| {
            super::run::species_energy(report.species(index)?, gibbs)
        })?;
        Some((energy - *reference.get_or_insert(energy)) * KCAL_PER_HARTREE)
    })
}

fn energy_levels(
    plan: &Plan,
    report: &Report,
    mut energy: impl FnMut(&Level, Option<&StepValue>) -> Option<f64>,
) -> Option<Vec<(Level, f64)>> {
    plan.levels()?
        .into_iter()
        .enumerate()
        .map(|(index, level)| {
            let incoming = if let Some(index) = index.checked_sub(1) {
                let value = report.steps.get(index)?.as_ref()?;
                if value.arrow != plan.steps.get(index)?.arrow {
                    return None;
                }
                Some(value)
            } else {
                None
            };
            let energy = energy(&level, incoming)?;
            energy.is_finite().then_some((level, energy))
        })
        .collect()
}

pub(super) fn overall(plan: &Plan, report: &Report, gibbs: bool) -> Option<f64> {
    let mut energy = 0.;
    energy_levels(plan, report, |_, incoming| {
        if let Some(step) = incoming {
            energy += if gibbs { step.gibbs } else { step.energy }?;
        }
        Some(energy)
    })?
    .last()
    .map(|(_, energy)| *energy)
}

fn default_gibbs(
    plan: &Plan,
    report: &Report,
    options: &[super::run::StepOptions],
) -> Option<bool> {
    let mut active = options.iter().filter(|options| options.any()).peekable();
    let preferred = active.peek().is_some()
        && active.all(|options| !options.reaction_energy && !options.barrier);
    [preferred, !preferred]
        .into_iter()
        .find(|gibbs| profile_levels(plan, report, *gibbs).is_some())
}

fn side_geometry(
    plan: &Plan,
    report: &Report,
    terms: impl IntoIterator<Item = Term>,
    saddle: Option<EnergyProfileGeometry>,
) -> Option<EnergyProfileGeometry> {
    let mut frames: Vec<&Frame> = saddle.iter().map(|geometry| &geometry.frame).collect();
    let mut count = saddle
        .as_ref()
        .map_or(0, |geometry| geometry.frame.numbers.len());
    for term in terms {
        plan.species.get(term.species)?;
        let frame = report.species(term.species)?.frame.as_ref();
        count = count.checked_add(frame.numbers.len().checked_mul(term.coefficient as usize)?)?;
        if count > reshiki::qcdata::MAX_CALC_ATOMS {
            return None;
        }
        frames.extend(std::iter::repeat_n(frame, term.coefficient as usize));
    }
    if frames.len() == 1 && saddle.is_some() {
        return saddle;
    }
    let mut geometry = thumbnails::profile_side(&frames)?;
    if let Some(saddle) = saddle {
        let count = saddle.frame.numbers.len();
        geometry
            .bonds
            .retain(|pair| pair.iter().all(|index| *index >= count));
        geometry.bonds.extend(saddle.bonds);
        geometry.forming = saddle.forming;
        geometry.breaking = saddle.breaking;
    }
    Some(geometry)
}

impl App {
    pub(in crate::app) fn xtb_scheme_profile_for(&self, gibbs: bool) -> Option<EnergyProfile> {
        let state = self.tab.xtb.scheme.as_ref()?;
        if !self.xtb_scheme_current() {
            return None;
        }
        build(
            &self.tab.doc,
            &state.plan,
            &state.work.report,
            gibbs,
            self.appearance.energy_unit,
        )
    }

    pub(in crate::app) fn xtb_scheme_profile(&self) -> Option<EnergyProfile> {
        let state = self.tab.xtb.scheme.as_ref()?;
        self.xtb_scheme_profile_for(default_gibbs(
            &state.plan,
            &state.work.report,
            &state.options,
        )?)
    }

    pub(in crate::app) fn xtb_scheme_profile_available(&self) -> bool {
        self.xtb_scheme_current()
            && self.tab.xtb.scheme.as_ref().is_some_and(|state| {
                default_gibbs(&state.plan, &state.work.report, &state.options).is_some()
            })
    }

    /// Live detached results may include synthetic balancing atoms whose map
    /// cannot be attached to the drawing. Geometry and trace agreement are
    /// still checked by the ordinary playback binder before they are used.
    pub(in crate::app::xtb) fn xtb_scheme_profile_results(
        &self,
        figure: &EnergyProfile,
    ) -> Vec<Arc<CalcResult>> {
        let Some(state) = &self.tab.xtb.scheme else {
            return vec![];
        };
        if !self.xtb_scheme_current() || !state.follows(&self.tab.doc, figure) {
            return vec![];
        }
        state
            .work
            .report
            .steps
            .iter()
            .flatten()
            .flat_map(|value| {
                value.ts.iter().cloned().chain(
                    value.discovered.iter().flat_map(|path| {
                        path.segments.iter().map(|segment| Arc::clone(&segment.ts))
                    }),
                )
            })
            .collect()
    }

    pub(in crate::app) fn xtb_place_scheme_profile(&mut self) {
        if self.tab.busy || self.tab.drawing_edit_active() {
            return;
        }
        let Some(fresh) = self.xtb_scheme_profile() else {
            self.status = "Complete the scheme energies to place a profile".into();
            self.error = false;
            return;
        };
        if fresh.states.len() > EnergyProfile::MAX_STATES {
            self.status = format!(
                "A placed energy profile fits up to {} states",
                EnergyProfile::MAX_STATES
            );
            self.error = true;
            return;
        }
        let Some(state) = &self.tab.xtb.scheme else {
            return;
        };
        if let Some(id) = self.tab.doc.graphics.iter().find_map(|g| {
            let profile = g.energy_profile.as_deref()?;
            (profile.energy_kind == fresh.energy_kind && state.follows(&self.tab.doc, profile))
                .then_some(g.id)
        }) {
            self.select_energy_profile(id);
            return;
        }
        let ids: Vec<_> = state
            .plan
            .arrows
            .iter()
            .filter_map(|arrow| preview(&self.tab.doc, *arrow).ok())
            .flat_map(|reaction| reaction.ids())
            .collect();
        let (width, height) = fresh.natural_size(&self.tab.doc.drawing_style);
        let size = Point::new(width, height);
        let Some(origin) = profile::profile_origin_for_ids(&self.tab.doc, &ids, size) else {
            return;
        };
        let before = self.tab.doc.clone();
        let id = self.tab.doc.next_id();
        let style = GraphicStyle {
            width_pt: self.tab.doc.drawing_style.line_width_pt,
            ..Default::default()
        };
        self.tab
            .doc
            .graphics
            .push(Graphic::energy_profile(id, origin, size, fresh, style));
        self.error = false;
        self.changed(before);
        if !self.error {
            self.select_energy_profile(id);
            self.status = "Placed the energy profile · ⌘Z removes it".into();
        }
    }

    /// Call once after a run completes. Authored position, style, model layout,
    /// view rotation, and detached figures retain the single-step behavior.
    pub(in crate::app) fn xtb_refresh_scheme_profile(&mut self, before: Document) {
        let electronic = self.xtb_scheme_profile_for(false);
        let gibbs = self.xtb_scheme_profile_for(true);
        let Some(state) = &self.tab.xtb.scheme else {
            return;
        };
        for profile in self
            .tab
            .doc
            .graphics
            .iter_mut()
            .filter_map(|g| g.energy_profile.as_deref_mut())
        {
            let fresh = match profile.energy_kind {
                EnergyProfileKind::Electronic => electronic.as_ref(),
                EnergyProfileKind::Gibbs => gibbs.as_ref(),
            };
            if let Some(fresh) = fresh
                && fresh.states.len() <= EnergyProfile::MAX_STATES
                && state.follows(&before, profile)
            {
                profile.relink(fresh.clone());
            }
        }
        if self.tab.doc != before {
            let status = self.status.clone();
            self.error = false;
            self.changed(before);
            if !self.error {
                self.status = status;
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
