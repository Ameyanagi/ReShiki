//! The scheme's common-reference figure uses the same native graphic, model
//! placement, link editing and undo path as a single-arrow energy profile.
use super::{
    plan::{Plan, Term},
    run::Report,
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
use reshiki::reactions;
use std::sync::Arc;

/// Minima are accumulated reaction changes, so a later step may add or remove
/// a co-reactant without comparing the absolute energies of different systems.
pub(super) fn build(
    doc: &Document,
    plan: &Plan,
    report: &Report,
    gibbs: bool,
    unit: EnergyUnit,
) -> Option<EnergyProfile> {
    let deltas = profile_deltas(plan, report, gibbs)?;
    let first = plan.steps.first()?;
    let reaction = reactions::preview(doc, first.arrow).ok()?;
    let mut reactant = profile::state(Kind::Reactant, plan.endpoint_label(0, false), 0.);
    reactant.geometry = side_geometry(plan, report, &first.reactants);
    let mut states = vec![reactant];
    let mut steps = Vec::new();
    let mut level = 0.;
    let mut rotation = None;
    for (index, step) in plan.steps.iter().enumerate() {
        let value = report.steps.get(index)?.as_ref()?;
        if value.arrow != step.arrow {
            return None;
        }
        let delta = *deltas.get(index)?;
        let from = states.len() - 1;
        let barrier = if gibbs {
            value.free_barrier
        } else {
            value.barrier
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
            if let Some(transition) = transition {
                let ends = result.endpoints;
                let frame = |hash| {
                    value
                        .frames
                        .iter()
                        .find(|frame| frame.hash() == hash)
                        .map(AsRef::as_ref)
                };
                if let Some(ends) = ends
                    && let (Some(r), Some(ts), Some(p)) = (
                        frame(ends.reactant),
                        frame(result.frame),
                        frame(ends.product),
                    )
                    && let Some(([_, ts, _], view)) =
                        thumbnails::profile_geometry_from_frames(result, r, ts, p)
                {
                    states.get_mut(transition)?.geometry = Some(ts);
                    rotation.get_or_insert(view);
                }
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
        level += delta;
        if !level.is_finite() {
            return None;
        }
        let kind = if index + 1 == plan.steps.len() {
            Kind::Product
        } else {
            Kind::Intermediate
        };
        let minimum = minimum_terms(plan, index);
        let mut state = profile::state(kind, plan.minimum_label(index), level);
        state.geometry = side_geometry(plan, report, &minimum);
        states.push(state);
        let current = reactions::preview(doc, step.arrow).ok()?;
        steps.push(EnergyProfileStep {
            from,
            to: states.len() - 1,
            transition,
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

fn side_energy(report: &Report, terms: &[Term], gibbs: bool) -> Option<f64> {
    terms.iter().try_fold(0., |total, term| {
        let value = report.species(term.species)?;
        let energy = value.result.energy_hartree?;
        let correction = if gibbs {
            value.result.thermo.as_ref()?.gibbs_correction_hartree
        } else {
            0.
        };
        let total = total + f64::from(term.coefficient) * (energy + correction);
        total.is_finite().then_some(total)
    })
}

fn profile_deltas(plan: &Plan, report: &Report, gibbs: bool) -> Option<Vec<f64>> {
    report.species.first()?;
    let mut level = 0.;
    plan.steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let value = report.steps.get(index)?.as_ref()?;
            if value.arrow != step.arrow {
                return None;
            }
            let delta = if gibbs { value.gibbs } else { value.energy }
                .or_else(|| {
                    Some(
                        (side_energy(report, &step.products, gibbs)?
                            - side_energy(report, &step.reactants, gibbs)?)
                            * KCAL_PER_HARTREE,
                    )
                })
                .filter(|value| value.is_finite())?;
            level += delta;
            level.is_finite().then_some(delta)
        })
        .collect()
}

fn default_gibbs(options: &[super::run::StepOptions]) -> bool {
    let mut active = options.iter().filter(|options| options.any()).peekable();
    active.peek().is_some() && active.all(|options| !options.reaction_energy && !options.barrier)
}

/// The shared level also names any additional material entering the next
/// arrow. Keeping the maximum drawn coefficient avoids naming a shared
/// intermediate twice and retains coproducts from the preceding arrow.
fn minimum_terms(plan: &Plan, index: usize) -> Vec<Term> {
    let Some(step) = plan.steps.get(index) else {
        return vec![];
    };
    let mut terms = step.products.clone();
    if let Some(next) = plan.steps.get(index + 1) {
        for term in &next.reactants {
            if let Some(existing) = terms
                .iter_mut()
                .find(|existing| existing.species == term.species)
            {
                existing.coefficient = existing.coefficient.max(term.coefficient);
            } else {
                terms.push(*term);
            }
        }
    }
    terms
}

fn side_geometry(plan: &Plan, report: &Report, terms: &[Term]) -> Option<EnergyProfileGeometry> {
    let mut frames: Vec<&Frame> = Vec::new();
    let mut count = 0usize;
    for term in terms {
        plan.species.get(term.species)?;
        let frame = report.species(term.species)?.frame.as_ref();
        count = count.checked_add(frame.numbers.len().checked_mul(term.coefficient as usize)?)?;
        if count > reshiki::qcdata::MAX_CALC_ATOMS {
            return None;
        }
        frames.extend(std::iter::repeat_n(frame, term.coefficient as usize));
    }
    thumbnails::profile_side(&frames)
}

fn same_scheme(a: &EnergyProfile, b: &EnergyProfile) -> bool {
    a.source == b.source
        && a.source_key == b.source_key
        && a.steps.len() == b.steps.len()
        && a.steps
            .iter()
            .zip(&b.steps)
            .all(|(a, b)| a.source_key == b.source_key)
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
        self.xtb_scheme_profile_for(default_gibbs(&state.options))
    }

    pub(in crate::app) fn xtb_scheme_profile_available(&self) -> bool {
        self.xtb_scheme_current()
            && self.tab.xtb.scheme.as_ref().is_some_and(|state| {
                profile_deltas(
                    &state.plan,
                    &state.work.report,
                    default_gibbs(&state.options),
                )
                .is_some()
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
        if !self.xtb_scheme_current()
            || figure.source != state.plan.arrows.first().copied()
            || figure.steps.len() != state.plan.steps.len()
            || !profile::source_current(&self.tab.doc, figure)
            || figure
                .steps
                .iter()
                .skip(1)
                .zip(state.plan.steps.iter().skip(1))
                .any(|(figure, step)| {
                    reactions::preview(&self.tab.doc, step.arrow)
                        .ok()
                        .is_none_or(|reaction| {
                            figure.source_key.as_deref()
                                != Some(profile::source_key(&self.tab.doc, &reaction).as_str())
                        })
                })
        {
            return vec![];
        }
        state
            .work
            .report
            .steps
            .iter()
            .flatten()
            .filter_map(|value| value.ts.as_ref().map(Arc::clone))
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
        if let Some(id) = self.tab.doc.graphics.iter().find_map(|g| {
            let profile = g.energy_profile.as_deref()?;
            (profile.linked && same_scheme(profile, &fresh)).then_some(g.id)
        }) {
            self.select_energy_profile(id);
            return;
        }
        let Some(state) = &self.tab.xtb.scheme else {
            return;
        };
        let ids: Vec<_> = state
            .plan
            .arrows
            .iter()
            .filter_map(|arrow| reactions::preview(&self.tab.doc, *arrow).ok())
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
            if profile.linked
                && let Some(fresh) = fresh
                && fresh.states.len() <= EnergyProfile::MAX_STATES
                && same_scheme(profile, fresh)
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
