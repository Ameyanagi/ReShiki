//! Place a reaction result on the drawing and keep its linked figure current.
use super::{card::ArrowResult, energy::Totals, irc, optimized_reactant_energy, view};
use crate::app::{App, Message};
use reshiki::document::{Document, Point};
use reshiki::graphics::{
    EnergyProfile, EnergyProfileModels, EnergyProfileSample, EnergyProfileState,
    EnergyProfileStateKind as Kind, EnergyProfileStep, EnergyUnit, Graphic, GraphicStyle,
};
use reshiki::qcdata::{CalcResult, Frame, IrcEndpoint, KCAL_PER_HARTREE};
use reshiki::reactions::{self, Reaction, Role};
use reshiki::scene;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Edit {
    Models(EnergyProfileModels),
    Unlink,
}

pub(in crate::app) fn edit_message(id: u64, edit: Edit) -> Message {
    Message::Xtb(super::super::Action::Ts(super::Action::EditEnergyProfile(
        id, edit,
    )))
}

/// Fingerprint the arrow's chemical roles in drawing order, independent of
/// drawing IDs, coordinates, compound labels, and calculated models.
fn source_key(doc: &Document, reaction: &Reaction) -> String {
    let roles = Role::ALL.map(|role| {
        let mut participants: Vec<_> = reaction
            .participants(role)
            .iter()
            .map(|participant| {
                let mut atoms: Vec<_> = doc
                    .atoms
                    .iter()
                    .filter(|atom| participant.atoms.contains(&atom.id))
                    .collect();
                atoms.sort_by_key(|atom| atom.map_num);
                let maps: Vec<_> = atoms.iter().map(|atom| atom.map_num).collect();
                let ids: Vec<_> = atoms.iter().map(|atom| atom.id).collect();
                (
                    participant.coefficient,
                    maps,
                    reshiki::science::baseline(doc, &ids),
                )
            })
            .collect();
        participants.sort_unstable();
        participants
    });
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&roles).unwrap_or_default())
    )
}

pub(in crate::app) fn source_current(doc: &Document, profile: &EnergyProfile) -> bool {
    profile
        .source
        .and_then(|arrow| reactions::preview(doc, arrow).ok())
        .is_some_and(|reaction| source_key(doc, &reaction) == profile.source_key)
}

fn profile(doc: &Document, id: u64) -> Option<&EnergyProfile> {
    doc.graphics
        .iter()
        .find(|g| g.id == id)?
        .energy_profile
        .as_deref()
}

/// Keep the authored frame and appearance on the same graphic during a rerun.
fn refresh(doc: &mut Document, fresh: EnergyProfile) {
    for placed in doc
        .graphics
        .iter_mut()
        .filter_map(|g| g.energy_profile.as_deref_mut())
    {
        if placed.linked && placed.source == fresh.source && placed.source_key == fresh.source_key {
            placed.relink(fresh.clone());
        }
    }
}

fn has_link(doc: &Document, arrow: u64) -> bool {
    doc.graphics
        .iter()
        .filter_map(|g| g.energy_profile.as_deref())
        .any(|profile| profile.linked && profile.source == Some(arrow))
}

impl App {
    pub(super) fn xtb_place_energy_profile(&mut self, arrow: Option<u64>) {
        if self.tab.busy || self.tab.drawing_edit_active() {
            return;
        }
        let arrow = arrow
            .or_else(|| self.selected_reaction_arrow().map(|arrow| arrow.id))
            .or_else(|| match self.tab.selected.as_slice() {
                [id] => profile(&self.tab.doc, *id).filter(|p| p.linked)?.source,
                [] => self.xtb_shown()?.arrow,
                _ => None,
            })
            .or_else(|| self.xtb_review_target());
        let doc = &self.tab.doc;
        let Some(reaction) = arrow.and_then(|id| reactions::preview(doc, id).ok()) else {
            self.status = "Select a reaction arrow with a calculated energy".into();
            self.error = false;
            return;
        };
        let key = source_key(doc, &reaction);
        if let Some(id) = doc.graphics.iter().find_map(|g| {
            let profile = g.energy_profile.as_deref()?;
            (profile.linked && profile.source == arrow && profile.source_key == key).then_some(g.id)
        }) {
            self.select_energy_profile(id);
            return;
        }
        let result = match self.xtb_ts_card_display_result(reaction.arrow) {
            ArrowResult::Found(result) => Some(result),
            _ => None,
        };
        let endpoint = result.and_then(|result| view::reaction_energy_kcal(result));
        let fresh = self
            .xtb_energy_profile_batch(reaction.arrow, result.filter(|_| endpoint.is_some()))
            .or_else(|| {
                let result = result.filter(|r| !self.xtb_ts_result_stale(r))?;
                self.xtb_saved_energy_profile(doc, result, self.xtb_ts_refinement_session(result))
            });
        let Some(fresh) = fresh else {
            self.status = "Calculate a reaction energy first".into();
            self.error = false;
            return;
        };
        let (w, h) = fresh.natural_size(&doc.drawing_style);
        let size = Point::new(w, h);
        let Some(origin) = profile_origin(doc, &reaction, size) else {
            return;
        };
        let id = doc.next_id();
        let style = GraphicStyle {
            width_pt: doc.drawing_style.line_width_pt,
            ..Default::default()
        };
        let before = self.tab.doc.clone();
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

    fn select_energy_profile(&mut self, id: u64) {
        self.xtb_end_preview();
        self.set_selection(vec![id]);
        if let Some((lo, hi)) = scene::selection_bounds(&self.tab.doc, &[id]) {
            self.pan_to_show(lo, hi);
        }
    }

    pub(super) fn xtb_edit_energy_profile(&mut self, id: u64, edit: Edit) {
        if profile(&self.tab.doc, id).is_none() {
            return;
        }
        let before = self.tab.doc.clone();
        let doc = &mut self.tab.doc;
        let Some(graphic) = doc.graphics.iter_mut().find(|g| g.id == id) else {
            return;
        };
        let Some(profile) = graphic.energy_profile.as_deref_mut() else {
            return;
        };
        match edit {
            Edit::Models(models) => {
                let (w, h) = profile.natural_size(&doc.drawing_style);
                profile.models = models;
                let (new_w, new_h) = profile.natural_size(&doc.drawing_style);
                graphic.axis_x = Point::new(
                    graphic.axis_x.x * (new_w / w),
                    graphic.axis_x.y * (new_w / w),
                );
                graphic.axis_y = Point::new(
                    graphic.axis_y.x * (new_h / h),
                    graphic.axis_y.y * (new_h / h),
                );
            }
            Edit::Unlink => profile.unlink(),
        }
        self.error = false;
        self.changed(before);
    }

    pub(in crate::app) fn update_energy_profile_units(&mut self, unit: EnergyUnit) {
        self.each_tab(|app| {
            let doc = &app.tab.doc;
            let ids: Vec<_> = doc
                .graphics
                .iter()
                .filter_map(|g| {
                    let p = g.energy_profile.as_deref()?;
                    (p.linked && p.unit != unit && source_current(doc, p)).then_some(g.id)
                })
                .collect();
            if ids.is_empty() {
                return;
            }
            let before = app.tab.doc.clone();
            for g in &mut app.tab.doc.graphics {
                if ids.contains(&g.id)
                    && let Some(profile) = &mut g.energy_profile
                {
                    profile.unit = unit;
                }
            }
            app.changed(before);
        });
    }

    pub(in crate::app::xtb) fn xtb_refresh_ts_profile(
        &self,
        doc: &mut Document,
        result: &CalcResult,
        session: Option<&super::Session>,
    ) {
        if super::saved_search(result)
            .and_then(|search| search.arrow)
            .is_some_and(|arrow| has_link(doc, arrow))
            && let Some(fresh) = self.xtb_saved_energy_profile(doc, result, session)
        {
            refresh(doc, fresh);
        }
    }

    fn xtb_saved_energy_profile(
        &self,
        doc: &Document,
        result: &CalcResult,
        session: Option<&super::Session>,
    ) -> Option<EnergyProfile> {
        let arrow = super::placement_arrow(doc, result)?;
        let presentation = view::presentation(result, session);
        // The card keeps the verified source when g-xTB refinement fails.
        if super::saved_search(result)?.refined_from.is_some() && !presentation.reliable() {
            return None;
        }
        let reaction = reactions::preview(doc, arrow).ok()?;
        let [reactant, product] = self.xtb_reaction_label(arrow).endpoint_names();
        let energy = view::reaction_energy_kcal(result)?;
        let mut states = vec![state(Kind::Reactant, reactant, 0.)];
        let mut samples = vec![];
        if let Some(barrier) = presentation.refined_kcal.filter(|e| e.is_finite()) {
            states.push(state(Kind::TransitionState, "TS1‡".into(), barrier));
            if let Some(reference) = optimized_reactant_energy(result)
                && let Some(path) = irc::saved(result)
            {
                samples = irc_samples(path, reference, barrier, energy);
            }
        }
        states.push(state(Kind::Product, product, energy));
        let mut profile =
            self.make_energy_profile(doc, &reaction, presentation.method, states, samples);
        if let Some((models, rotation)) = super::thumbnails::profile_geometry(doc, result) {
            profile.rotation = rotation;
            let [r, ts, p] = models;
            let [reactant, middle @ .., product] = profile.states.as_mut_slice() else {
                return None;
            };
            reactant.geometry = Some(r);
            if let [transition] = middle {
                transition.geometry = Some(ts);
            }
            product.geometry = Some(p);
        }
        Some(profile)
    }

    pub(super) fn xtb_endpoint_energy_profile(
        &self,
        doc: &Document,
        arrow: u64,
        totals: &Totals,
        frames: [Vec<&Frame>; 2],
    ) -> Option<EnergyProfile> {
        let reaction = reactions::preview(doc, arrow).ok()?;
        let [reactant, product] = self.xtb_reaction_label(arrow).endpoint_names();
        let mut states = vec![
            state(Kind::Reactant, reactant, 0.),
            state(Kind::Product, product, totals.energy),
        ];
        for (state, frames) in states.iter_mut().zip(frames) {
            state.geometry = super::thumbnails::profile_side(&frames);
        }
        Some(self.make_energy_profile(doc, &reaction, totals.method.label().into(), states, vec![]))
    }

    fn make_energy_profile(
        &self,
        doc: &Document,
        reaction: &Reaction,
        method: String,
        states: Vec<EnergyProfileState>,
        irc: Vec<EnergyProfileSample>,
    ) -> EnergyProfile {
        let to = states.len() - 1;
        let step = EnergyProfileStep {
            from: 0,
            to,
            transition: (to == 2).then_some(1),
            irc,
        };
        let mut profile = EnergyProfile::new(
            reaction.arrow,
            source_key(doc, reaction),
            method,
            states,
            vec![step],
        );
        profile.unit = self.appearance.energy_unit;
        profile
    }

    pub(in crate::app::xtb) fn xtb_refresh_energy_batch(
        &self,
        doc: &mut Document,
        arrow: u64,
        serial: u64,
    ) {
        if has_link(doc, arrow)
            && let Some((totals, frames)) = self.tab.xtb.energy_state.finishing_profile(doc, serial)
            && let Some(fresh) = self.xtb_endpoint_energy_profile(doc, arrow, &totals, frames)
        {
            refresh(doc, fresh);
        }
    }

    pub(super) fn xtb_follow_energy_batch_profile(&mut self, arrow: u64) {
        if !has_link(&self.tab.doc, arrow) {
            return;
        }
        if let Some(fresh) = self.xtb_energy_profile_batch(arrow, None) {
            let before = self.tab.doc.clone();
            refresh(&mut self.tab.doc, fresh);
            if self.tab.doc != before {
                let status = self.status.clone();
                self.changed(before);
                if !self.error {
                    self.status = status;
                }
            }
        }
    }
}

fn state(kind: Kind, label: String, energy_kcal: f64) -> EnergyProfileState {
    let mut label = label.replace(char::is_control, " ");
    if label.len() > 256 {
        label.truncate(label.floor_char_boundary(253));
        label.push('…');
    }
    EnergyProfileState {
        kind,
        label,
        energy_kcal,
        geometry: None,
    }
}

fn irc_samples(
    path: &reshiki::qcdata::IrcPath,
    reference: f64,
    barrier: f64,
    product: f64,
) -> Vec<EnergyProfileSample> {
    use IrcEndpoint::{Product, Reactant};
    let reversed = match path.reached {
        [Reactant, Product] => false,
        [Product, Reactant] => true,
        _ => return vec![],
    };
    let arc = &path.arc_lengths_sqrt_amu_bohr;
    let energies = &path.trajectory.energies_hartree;
    let &[lo, .., hi] = arc.as_slice() else {
        return vec![];
    };
    if arc.len() != energies.len()
        || !(1..arc.len() - 1).contains(&path.ts_index)
        || !(lo < 0. && hi > 0.)
    {
        return vec![];
    }
    let sample = |x, energy_kcal| EnergyProfileSample { x, energy_kcal };
    let mut samples: Vec<_> = arc
        .iter()
        .zip(energies)
        .enumerate()
        .filter(|&(index, (&s, _))| index == path.ts_index || s != 0.)
        .map(|(index, (&s, &energy))| {
            // Leave room to connect the traced ends to the optimized states.
            let x = if index < path.ts_index {
                0.5 - 0.46 * s / lo
            } else {
                0.5 + 0.46 * s / hi
            };
            let x = if reversed { 1. - x } else { x };
            let energy = if index == path.ts_index {
                barrier
            } else {
                (energy - reference) * KCAL_PER_HARTREE
            };
            sample(x, energy)
        })
        .collect();
    if reversed {
        samples.reverse();
    }
    samples.dedup_by(|a, b| a.x == b.x);
    samples.insert(0, sample(0., 0.));
    samples.push(sample(1., product));
    samples
}

fn profile_origin(doc: &Document, reaction: &Reaction, size: Point) -> Option<Point> {
    let scheme = reshiki::molecule_names::include_annotations(doc, &reaction.ids());
    let other: Vec<_> = doc.object_ids().filter(|id| !scheme.contains(id)).collect();
    let gap = doc.drawing_style.bond_length_world;
    let mut selections = vec![scheme];
    selections.extend(reshiki::editing::groups(doc, &other));
    let bounds = scene::selections_bounds(doc, &selections);
    let [Some((lo, hi)), rest @ ..] = bounds.as_slice() else {
        return None;
    };
    let x = lo.x + (hi.x - lo.x - size.x) * 0.5;
    let mut obstacles: Vec<_> = rest
        .iter()
        .flatten()
        .copied()
        .filter(|(lo, hi)| lo.x < x + size.x + gap && hi.x > x - gap)
        .collect();
    obstacles.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    let mut y = hi.y + gap;
    for (lo, hi) in obstacles {
        if lo.y < y + size.y + gap && hi.y + gap > y {
            y = hi.y + gap;
        }
    }
    Some(Point::new(x, y))
}
