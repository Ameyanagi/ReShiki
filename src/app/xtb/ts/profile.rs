//! Place a reaction result on the drawing and keep its linked figure current.
use super::{card::ReactionAnswer, energy::EndpointResult, irc, optimized_reactant_energy, view};
use crate::app::{App, Message};
use reshiki::document::{Document, Point};
use reshiki::graphics::{
    EnergyProfile, EnergyProfileGeometry, EnergyProfileModels, EnergyProfileSample,
    EnergyProfileState, EnergyProfileStateKind as Kind, EnergyProfileStep, EnergyUnit, Graphic,
    GraphicStyle,
};
use reshiki::qcdata::{CalcResult, IrcEndpoint, IrcPath, KCAL_PER_HARTREE};
use reshiki::reactions::{self, Reaction, Role};
use reshiki::scene;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Hash the role chemistry and correspondence in drawing order. Atom
/// positions and first-seen stereo group labels survive paste renumbering.
pub(in crate::app) fn source_key(doc: &Document, reaction: &Reaction) -> String {
    let mut mapped = [HashMap::<u32, Vec<usize>>::new(), HashMap::new()];
    let mut stereo_groups = HashMap::new();
    let roles = Role::ALL.map(|role| {
        let mut participants: Vec<_> = reaction
            .participants(role)
            .iter()
            .map(|participant| {
                let atoms: Vec<_> = doc
                    .atoms
                    .iter()
                    .enumerate()
                    .filter(|(_, atom)| participant.atoms.contains(&atom.id))
                    .collect();
                (atoms, participant.coefficient)
            })
            .collect();
        participants.sort_by_key(|(atoms, _)| atoms.first().map(|(i, _)| *i));
        let side = match role {
            Role::Reactant => Some(&mut mapped[0]),
            Role::Product => Some(&mut mapped[1]),
            Role::Agent => None,
        };
        if let Some(side) = side {
            for (i, (_, atom)) in participants.iter().flat_map(|(atoms, _)| atoms).enumerate() {
                if atom.map_num != 0 {
                    side.entry(atom.map_num).or_default().push(i);
                }
            }
        }
        participants
            .into_iter()
            .map(|(atoms, coefficient)| {
                let ids: Vec<_> = atoms.iter().map(|(_, atom)| atom.id).collect();
                let baseline =
                    reshiki::science::baseline_with_stereo_groups(doc, &ids, |mut group| {
                        let next = stereo_groups.len() as u32 + 1;
                        group.number = *stereo_groups.entry(group).or_insert(next);
                        group
                    });
                (coefficient, baseline)
            })
            .collect::<Vec<_>>()
    });
    let [reactants, mut products] = mapped;
    let mut pairs: Vec<_> = reactants
        .into_iter()
        .filter_map(|(map, atoms)| Some((atoms, products.remove(&map)?)))
        .collect();
    pairs.sort_unstable();
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(roles, pairs)).unwrap_or_default())
    )
}

pub(in crate::app) fn source_current(doc: &Document, profile: &EnergyProfile) -> bool {
    source_reaction(doc, profile).is_some()
}

pub(in crate::app) fn source_reaction(doc: &Document, profile: &EnergyProfile) -> Option<Reaction> {
    let reaction = reactions::preview(doc, profile.source?).ok()?;
    (source_key(doc, &reaction) == profile.source_key).then_some(reaction)
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

fn linked_reaction(doc: &Document, arrow: u64) -> Option<Reaction> {
    let linked = doc
        .graphics
        .iter()
        .filter_map(|g| g.energy_profile.as_deref())
        .any(|profile| profile.linked && profile.source == Some(arrow));
    linked
        .then(|| reactions::preview(doc, arrow).ok())
        .flatten()
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
        let answer = self.xtb_resolve_reaction(doc, reaction.arrow, None);
        let fresh = self.xtb_resolved_profile(doc, &reaction, answer);
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

    fn xtb_saved_energy_profile(
        &self,
        doc: &Document,
        reaction: &Reaction,
        result: &CalcResult,
        presentation: view::Presentation,
        energy: f64,
    ) -> Option<EnergyProfile> {
        let [reactant, product] = self.xtb_reaction_label(reaction.arrow).endpoint_names();
        let mut states = vec![state(Kind::Reactant, reactant, 0.)];
        let mut samples = vec![];
        if let Some(barrier) = presentation.refined_kcal.filter(|e| e.is_finite()) {
            states.push(state(Kind::TransitionState, "TS‡".into(), barrier));
            if let Some(reference) = optimized_reactant_energy(result)
                && let Some(path) = irc::saved(result)
            {
                samples = irc_samples(path, reference, barrier, energy);
            }
        }
        states.push(state(Kind::Product, product, energy));
        let mut profile =
            self.make_energy_profile(doc, reaction, presentation.method, states, samples);
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

    fn xtb_endpoint_energy_profile(
        &self,
        doc: &Document,
        reaction: &Reaction,
        EndpointResult { totals, frames }: EndpointResult<'_>,
    ) -> EnergyProfile {
        let [reactant, product] = self.xtb_reaction_label(reaction.arrow).endpoint_names();
        let mut states = vec![
            state(Kind::Reactant, reactant, 0.),
            state(Kind::Product, product, totals.energy),
        ];
        for (state, frames) in states.iter_mut().zip(frames) {
            state.geometry = super::thumbnails::profile_side(&frames);
        }
        self.make_energy_profile(doc, reaction, totals.method.label().into(), states, vec![])
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

    fn xtb_resolved_profile(
        &self,
        doc: &Document,
        reaction: &Reaction,
        answer: ReactionAnswer<'_>,
    ) -> Option<EnergyProfile> {
        if let Some(batch) = answer.batch {
            return Some(self.xtb_endpoint_energy_profile(doc, reaction, batch));
        }
        let result = answer.result.filter(|_| !answer.stale)?;
        self.xtb_saved_energy_profile(
            doc,
            reaction,
            result,
            answer.presentation?,
            answer.endpoint_energy?,
        )
    }

    pub(in crate::app::xtb) fn xtb_refresh_reaction_profile(
        &self,
        doc: &mut Document,
        arrow: u64,
        finishing_serial: Option<u64>,
    ) {
        let Some(reaction) = linked_reaction(doc, arrow) else {
            return;
        };
        let answer = self.xtb_resolve_reaction(doc, arrow, finishing_serial);
        // An unsuccessful search or refinement can leave the card on an older result.
        let new_answer = if finishing_serial.is_some() {
            answer.batch.is_some()
        } else {
            answer
                .result
                .zip(doc.science.results.last())
                .is_some_and(|(shown, stored)| Arc::ptr_eq(shown, &stored.result))
        };
        if new_answer && let Some(fresh) = self.xtb_resolved_profile(doc, &reaction, answer) {
            refresh(doc, fresh);
        }
    }

    pub(super) fn xtb_follow_energy_batch_profile(&mut self, arrow: u64) {
        let doc = &self.tab.doc;
        if let Some(reaction) = linked_reaction(doc, arrow)
            && let Some(batch) = self.xtb_resolve_reaction(doc, arrow, None).batch
        {
            let fresh = self.xtb_endpoint_energy_profile(doc, &reaction, batch);
            let before = doc.clone();
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

#[derive(Clone)]
pub(in crate::app) struct ProfileIrc {
    result: Arc<CalcResult>,
    pub samples: Vec<IrcSample>,
}

#[derive(Clone, Copy)]
pub(in crate::app) struct IrcSample {
    pub point: EnergyProfileSample,
    pub coordinate: f64,
    pub frame: usize,
}

impl ProfileIrc {
    pub(in crate::app) fn geometry(&self, index: usize) -> Option<EnergyProfileGeometry> {
        let frame = irc::saved(&self.result)?.frame(index)?;
        let bonds = super::thumbnails::bonds(&frame)?;
        Some(EnergyProfileGeometry {
            frame,
            bonds,
            forming: vec![],
            breaking: vec![],
        })
    }
}

impl App {
    pub(in crate::app) fn energy_profile_paths(
        &self,
        profile: &EnergyProfile,
    ) -> Vec<Option<ProfileIrc>> {
        let doc = &self.tab.doc;
        let resolved = profile
            .source
            .filter(|_| source_current(doc, profile))
            .and_then(|arrow| self.xtb_resolve_reaction(doc, arrow, None).result);
        profile
            .steps
            .iter()
            .map(|step| {
                if step.irc.is_empty() {
                    return None;
                }
                let from = profile.states.get(step.from)?.energy_kcal;
                let to = profile.states.get(step.to)?.energy_kcal;
                let ts = profile.states.get(step.transition?)?;
                let bind = |result: &Arc<CalcResult>| {
                    let path = irc::saved(result).filter(|p| p.has_frames())?;
                    let reference = optimized_reactant_energy(result)? - from / KCAL_PER_HARTREE;
                    let mut samples = irc_trace(path, reference, ts.energy_kcal)?;
                    let endpoints = [
                        EnergyProfileSample {
                            x: 0.,
                            energy_kcal: from,
                        },
                        EnergyProfileSample {
                            x: 1.,
                            energy_kcal: to,
                        },
                    ];
                    let expected = iter_irc_points(&samples, endpoints);
                    // Full saves round positions before reconstructing arc lengths.
                    let agrees = step.irc.iter().zip(expected).all(|(a, b)| {
                        (a.x - b.x).abs() < 1e-4 && (a.energy_kcal - b.energy_kcal).abs() < 1e-6
                    });
                    if step.irc.len() != samples.len() + 2 || !agrees {
                        return None;
                    }
                    for (sample, &point) in samples.iter_mut().zip(step.irc.iter().skip(1)) {
                        sample.point = point;
                    }
                    Some(ProfileIrc {
                        result: Arc::clone(result),
                        samples,
                    })
                };
                resolved.and_then(bind).or_else(|| {
                    // A multistep figure can refer to more than one saved calculation.
                    // Require its saddle geometry as well as its trace to match.
                    let saddle = ts.geometry.as_ref()?.frame.hash();
                    doc.science.results.iter().rev().find_map(|stored| {
                        (stored.result.frame == saddle)
                            .then(|| bind(&stored.result))
                            .flatten()
                    })
                })
            })
            .collect()
    }
}

fn irc_samples(
    path: &IrcPath,
    reference: f64,
    barrier: f64,
    product: f64,
) -> Vec<EnergyProfileSample> {
    irc_trace(path, reference, barrier).map_or_else(Vec::new, |trace| {
        iter_irc_points(
            &trace,
            [
                EnergyProfileSample {
                    x: 0.,
                    energy_kcal: 0.,
                },
                EnergyProfileSample {
                    x: 1.,
                    energy_kcal: product,
                },
            ],
        )
        .collect()
    })
}

fn iter_irc_points(
    trace: &[IrcSample],
    [from, to]: [EnergyProfileSample; 2],
) -> impl Iterator<Item = EnergyProfileSample> {
    std::iter::once(from)
        .chain(trace.iter().map(|p| p.point))
        .chain(std::iter::once(to))
}

fn irc_trace(path: &IrcPath, reference: f64, barrier: f64) -> Option<Vec<IrcSample>> {
    use IrcEndpoint::{Product, Reactant};
    let reversed = match path.reached {
        [Reactant, Product] => false,
        [Product, Reactant] => true,
        _ => return None,
    };
    let arc = &path.arc_lengths_sqrt_amu_bohr;
    let energies = &path.trajectory.energies_hartree;
    let &[lo, .., hi] = arc.as_slice() else {
        return None;
    };
    if arc.len() != energies.len()
        || !(1..arc.len() - 1).contains(&path.ts_index)
        || !(lo < 0. && hi > 0.)
    {
        return None;
    }
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
            let energy_kcal = if index == path.ts_index {
                barrier
            } else {
                (energy - reference) * KCAL_PER_HARTREE
            };
            IrcSample {
                point: EnergyProfileSample {
                    x: if reversed { 1. - x } else { x },
                    energy_kcal,
                },
                coordinate: if reversed { -s } else { s },
                frame: index,
            }
        })
        .collect();
    if reversed {
        samples.reverse();
    }
    samples.dedup_by(|a, b| a.point.x == b.point.x);
    Some(samples)
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
        .chain(reshiki::models3d::zero_order_obstacles(doc))
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
