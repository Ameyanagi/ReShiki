//! The results palette's Orbitals tab: the level diagram and list, the
//! chosen orbital previewed on the molecule's 3D model, its isovalue and
//! look, and Place on canvas, which makes it part of the drawing. See
//! docs/plans/xtb-surfaces.md, "Phase 5: real orbitals in the drawing".
//!
//! Meshes are built off the UI thread and registered with the renderer
//! (`surface::draw::register`); xTB results keep no coefficients, so the
//! worker recomputes them first. Nothing here is saved but a placed figure.
// Model indices come from `xtb_model` on the same document, and column and
// level indices from a choice made for the same result's columns.
#![allow(clippy::indexing_slicing)]
use super::results::View;
use crate::app::{App, Message};
use iced::Task;
use levels::Column;
use reshiki::document::Document;
use reshiki::models3d::surface::{self, Figure, Fill, Look, OrbitalRef, mesh::Mesh};
use reshiki::qcdata::{self, CalcResult, Settings};
use reshiki::science::{self, Freshness};
use std::sync::Arc;

mod compute;
pub(crate) mod levels;
mod view;

pub(crate) use view::HEIGHT;

#[cfg(test)]
mod tests;

/// The isovalue a new preview starts at, and the slider's range (a.u.).
pub(crate) const ISO: f32 = 0.05;
pub(crate) const ISO_RANGE: std::ops::RangeInclusive<f32> = 0.01..=0.10;
pub(crate) const PRESETS: [f32; 3] = [0.02, 0.05, 0.10];
/// Recomputed orbital sets kept per document: one per recent result.
const COMPUTED: usize = 4;

pub(crate) fn send(action: Action) -> Message {
    Message::Xtb(super::Action::Orbitals(action))
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    /// A level in a spin column, from the list or the diagram.
    Choose(usize, usize),
    /// The list's spin column, for open shells.
    Column(usize),
    Homo,
    Lumo,
    /// ↑ or ↓ while nothing is selected on the canvas.
    Step(i32),
    Iso(f32),
    Opacity(u8),
    /// A slider let go: a placed figure takes its value as one Undo step.
    Settle,
    Look(LookChoice),
    MeshLines(bool),
    Place,
    Remove,
    Computed(String, Result<Arc<qcdata::Orbitals>, String>),
    Built(u64, Result<Meshes, String>),
}

/// Built meshes in a message; debug output shows their size only.
#[derive(Clone)]
pub(crate) struct Meshes(pub Arc<[Mesh; 2]>);

impl std::fmt::Debug for Meshes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Meshes({} triangles)", self.0[0].triangles.len())
    }
}

/// The Look segmented control: three fills, or mesh lines alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LookChoice {
    Solid,
    Shaded,
    Outline,
    Mesh,
}

impl LookChoice {
    pub(crate) const ALL: [Self; 4] = [Self::Solid, Self::Shaded, Self::Outline, Self::Mesh];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Shaded => "Shaded",
            Self::Outline => "Outline",
            Self::Mesh => "Mesh",
        }
    }

    pub(crate) fn of(look: Look) -> Self {
        match look.fill {
            Fill::Solid => Self::Solid,
            Fill::Shaded => Self::Shaded,
            Fill::Outline => Self::Outline,
            Fill::None => Self::Mesh,
        }
    }

    /// `look` with this choice; fills keep the mesh lines, unless they come
    /// from Mesh, which is mesh lines alone.
    fn apply(self, look: Look) -> Look {
        let (fill, mesh) = match self {
            Self::Solid => (Fill::Solid, look.mesh && look.fill != Fill::None),
            Self::Shaded => (Fill::Shaded, look.mesh && look.fill != Fill::None),
            Self::Outline => (Fill::Outline, look.mesh && look.fill != Fill::None),
            Self::Mesh => (Fill::None, true),
        };
        Look { fill, mesh, ..look }
    }
}

/// One document's Orbitals tab.
pub(crate) struct Orbitals {
    choice: Option<Choice>,
    /// The preview's isovalue and look; a placed figure keeps its own.
    iso: f32,
    look: Look,
    /// A slider held on a placed figure: shown until it is let go.
    drag: Option<Drag>,
    /// Orbitals the worker recomputed, by result key.
    computed: Vec<(String, Arc<qcdata::Orbitals>)>,
    computing: Option<Computing>,
    /// The mesh being built, by registry key, and meshes that failed.
    building: Option<u64>,
    failed: Vec<(u64, String)>,
    /// Why the orbitals of a result could not be recomputed, by key.
    errors: Vec<(String, String)>,
    /// The canvas's drawing copy with the previewed figure, and what it
    /// was made from.
    preview: Option<(Stamp, Box<Document>)>,
}

impl Default for Orbitals {
    fn default() -> Self {
        Self {
            choice: None,
            iso: ISO,
            look: Look::default(),
            drag: None,
            computed: vec![],
            computing: None,
            building: None,
            failed: vec![],
            errors: vec![],
            preview: None,
        }
    }
}

/// The chosen orbital: a result, a spin column and a level.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Choice {
    pub result: String,
    pub column: usize,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Iso(f32),
    Opacity(u8),
}

/// The document state and figure a preview was made for.
#[derive(Clone, Debug, PartialEq)]
struct Stamp {
    revision: (u64, u64),
    model: usize,
    figure: Figure,
}

/// What the tab shows for the palette's result.
pub(crate) struct Target {
    pub key: String,
    pub columns: Vec<Column>,
    /// The result's current 3D model, by index, when it shows the
    /// result's geometry.
    pub model: Option<usize>,
    /// Why no surface can be shown, in words.
    pub problem: Option<String>,
    pub placed: Option<Figure>,
}

impl App {
    /// The tab's result, its levels and where its surface would go.
    pub(super) fn xtb_orbital_target(&self) -> Option<Target> {
        let shown = self.xtb_shown()?;
        let result = &shown.stored.result;
        let orbitals = result.orbitals.as_ref()?;
        let columns = levels::columns(orbitals);
        if columns.first()?.energies_ev.is_empty() {
            return None;
        }
        let key = science::result_key(result);
        let atom = shown.atoms.iter().flatten().next().copied();
        let model = atom.and_then(|atom| self.xtb_model(atom));
        let doc = &self.tab.doc;
        let shows = model.is_some_and(|m| {
            doc.models[m].points(doc).is_some_and(|points| {
                points.len() == shown.frame.positions_angstrom.len()
                    && points
                        .iter()
                        .zip(&shown.frame.positions_angstrom)
                        .all(|((_, p), q)| (0..3).all(|k| (f64::from(p[k]) - q[k]).abs() < 1e-3))
            })
        });
        let placed = model
            .and_then(|m| doc.models[m].surface.clone())
            .filter(|figure| figure.orbital.result == key);
        let problem = if shown.stale {
            Some(
                "The molecule changed after this calculation. Recalculate to show orbitals.".into(),
            )
        } else if model.is_none() {
            Some("The molecule has no current 3D model to show orbitals on.".into())
        } else if !shows {
            Some(
                "The 3D model shows another geometry than this result. Use the result's geometry in Steps to show orbitals."
                    .into(),
            )
        } else if let Some(bytes) = result.omitted_bytes {
            Some(format!(
                "Not saved in the file ({}). Reopen the original file to see orbitals.",
                super::results::size(bytes)
            ))
        } else if orbitals.basis.is_none() && !matches!(result.settings, Settings::Xtb(_)) {
            Some(
                "This file's orbital shapes are not available: its basis uses functions ReShiki does not read yet (see the warnings when it opened)."
                    .into(),
            )
        } else if orbitals.basis.is_none() && !super::available() {
            Some("Needs xTB to show: this build cannot recompute orbitals.".into())
        } else {
            self.tab
                .xtb
                .orbitals
                .errors
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, error)| error.clone())
        };
        Some(Target {
            key,
            columns,
            model: model.filter(|_| shows),
            problem,
            placed,
        })
    }

    /// The chosen orbital of `target`: the placed figure's, else the tab's,
    /// else the HOMO (the first level without occupations).
    pub(super) fn xtb_orbital_choice(&self, target: &Target) -> Choice {
        if let Some(figure) = &target.placed {
            let column = target
                .columns
                .iter()
                .position(|c| c.spin == figure.orbital.spin)
                .unwrap_or(0);
            let chosen = self.tab.xtb.orbitals.choice.as_ref();
            // An open shell's columns can share one orbital: keep the
            // column the user picked.
            let column = chosen
                .filter(|c| {
                    c.result == target.key
                        && c.index == figure.orbital.index as usize
                        && target.columns.get(c.column).map(|c| c.spin) == Some(figure.orbital.spin)
                })
                .map_or(column, |c| c.column);
            return Choice {
                result: target.key.clone(),
                column,
                index: figure.orbital.index as usize,
            };
        }
        if let Some(choice) = self
            .tab
            .xtb
            .orbitals
            .choice
            .as_ref()
            .filter(|c| c.result == target.key)
        {
            return choice.clone();
        }
        Choice {
            result: target.key.clone(),
            column: 0,
            index: target.columns[0].homo().unwrap_or(0),
        }
    }

    /// The figure a preview or Place would show for `choice`.
    fn xtb_orbital_figure(&self, target: &Target, choice: &Choice) -> Figure {
        let state = &self.tab.xtb.orbitals;
        let mut figure = target.placed.clone().unwrap_or_else(|| Figure {
            orbital: OrbitalRef {
                result: target.key.clone(),
                spin: qcdata::Spin::Alpha,
                index: 0,
            },
            iso: state.iso,
            look: state.look,
            snapshot: None,
        });
        figure.orbital.spin = target.columns[choice.column].spin;
        figure.orbital.index = choice.index as u32;
        match state.drag {
            Some(Drag::Iso(iso)) => figure.iso = iso,
            Some(Drag::Opacity(opacity)) => figure.look.opacity = opacity,
            None => {}
        }
        figure.snapshot = None;
        figure
    }

    /// The canvas's drawing with the previewed orbital, while the Orbitals
    /// tab is in front.
    pub(super) fn xtb_orbital_preview(&self) -> Option<&Document> {
        let (stamp, doc) = self.tab.xtb.orbitals.preview.as_ref()?;
        (stamp.revision == (self.tab.file_epoch, self.tab.revision)).then_some(doc.as_ref())
    }

    /// After every message: keep the preview in step with the tab and the
    /// drawing, and start the work the shown figures still need.
    pub(in crate::app) fn xtb_orbitals_sync(&mut self) -> Task<Message> {
        self.xtb_orbitals_preview_sync();
        self.xtb_orbitals_feed()
    }

    fn xtb_orbitals_preview_sync(&mut self) {
        let wanted = (|| {
            if !self.xtb_palette_visible() || self.tab.xtb.palette.view != View::Orbitals {
                return None;
            }
            let target = self.xtb_orbital_target()?;
            let model = target.model.filter(|_| target.problem.is_none())?;
            // A placed figure shows itself, except while a slider is held.
            if target.placed.is_some() && self.tab.xtb.orbitals.drag.is_none() {
                return None;
            }
            let choice = self.xtb_orbital_choice(&target);
            let figure = self.xtb_orbital_figure(&target, &choice);
            Some(Stamp {
                revision: (self.tab.file_epoch, self.tab.revision),
                model,
                figure,
            })
        })();
        let state = &mut self.tab.xtb.orbitals;
        match wanted {
            None => state.preview = None,
            Some(stamp) if state.preview.as_ref().is_some_and(|(s, _)| *s == stamp) => {}
            Some(stamp) => {
                let mut doc = self.tab.doc.clone();
                doc.models[stamp.model].surface = Some(stamp.figure.clone());
                // A model shown as 2D only appears beside the drawing.
                if doc.models[stamp.model].display == reshiki::models3d::Display::TwoD {
                    reshiki::models3d::set_display(
                        &mut doc,
                        stamp.model,
                        reshiki::models3d::Display::Both,
                    );
                }
                self.tab.xtb.orbitals.preview = Some((stamp, Box::new(doc)));
            }
        }
    }

    /// Start a mesh build or a recomputation for the first figure on the
    /// canvas that needs one; one of each at a time.
    fn xtb_orbitals_feed(&mut self) -> Task<Message> {
        let doc = self.display_document();
        if doc.models.iter().all(|m| m.surface.is_none()) {
            return Task::none();
        }
        let state = &self.tab.xtb.orbitals;
        let mut recompute = None;
        let mut build = None;
        for model in &doc.models {
            let Some(figure) = &model.surface else {
                continue;
            };
            let Some(key) = surface::draw::key(doc, model) else {
                continue;
            };
            if state.building == Some(key)
                || state.failed.iter().any(|(k, _)| *k == key)
                || surface::draw::registered(key).is_some()
            {
                continue;
            }
            let Some(result) = current_result(doc, &figure.orbital.result) else {
                continue;
            };
            let index = figure.orbital.index as usize;
            let stored = result
                .orbitals
                .as_ref()
                .and_then(|o| compute::row(o, figure.orbital.spin, index));
            let computed = state
                .computed
                .iter()
                .find(|(k, _)| *k == figure.orbital.result)
                .and_then(|(_, o)| compute::row(o, figure.orbital.spin, index));
            match stored.or(computed) {
                Some((basis, row)) if build.is_none() && state.building.is_none() => {
                    let points: Vec<[f32; 3]> = model
                        .points(doc)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(_, p)| p)
                        .collect();
                    build = Some((key, basis.clone(), row.to_vec(), points, figure.iso));
                }
                Some(_) => {}
                None if matches!(result.settings, Settings::Xtb(_))
                    && super::available()
                    && state.computing.is_none()
                    && !state
                        .errors
                        .iter()
                        .any(|(k, _)| *k == figure.orbital.result) =>
                {
                    let frame = doc.science.frame(&result.frame).cloned();
                    recompute = recompute.or(frame.map(|f| (result.clone(), f)));
                }
                None => {}
            }
        }
        let mut tasks = vec![];
        if let Some((key, basis, row, points, iso)) = build {
            self.tab.xtb.orbitals.building = Some(key);
            tasks.push(Task::perform(
                off_thread(move || compute::meshes(&basis, &row, &points, iso)),
                move |meshes| send(Action::Built(key, meshes)),
            ));
        }
        if let Some((result, frame)) = recompute {
            tasks.push(self.xtb_orbitals_recompute(result, frame));
        }
        Task::batch(tasks)
    }

    #[cfg(target_arch = "wasm32")]
    fn xtb_orbitals_recompute(
        &mut self,
        _: Arc<CalcResult>,
        _: Arc<qcdata::Frame>,
    ) -> Task<Message> {
        Task::none()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn xtb_orbitals_recompute(
        &mut self,
        result: Arc<CalcResult>,
        frame: Arc<qcdata::Frame>,
    ) -> Task<Message> {
        let key = science::result_key(&result);
        let cancel = reshiki_xtb_worker::client::Cancel::default();
        let worker = cancel.clone();
        let reply = key.clone();
        let (task, handle) = Task::perform(
            off_thread(move || compute::orbitals(&result, &frame, &worker).map(Arc::new)),
            move |orbitals| send(Action::Computed(reply.clone(), orbitals)),
        )
        .abortable();
        self.tab.xtb.orbitals.computing = Some(Computing {
            key,
            _task: handle.abort_on_drop(),
            cancel,
        });
        task
    }

    pub(super) fn xtb_orbitals(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Computed(key, outcome) => {
                let state = &mut self.tab.xtb.orbitals;
                if state.computing.as_ref().is_some_and(|c| c.key == key) {
                    state.computing = None;
                }
                match outcome {
                    Ok(orbitals) => {
                        state.computed.retain(|(k, _)| *k != key);
                        state.computed.push((key, orbitals));
                        if state.computed.len() > COMPUTED {
                            state.computed.remove(0);
                        }
                    }
                    Err(error) => {
                        state.errors.retain(|(k, _)| *k != key);
                        state
                            .errors
                            .push((key, format!("Orbitals could not be computed: {error}")));
                    }
                }
            }
            Action::Built(key, outcome) => {
                let state = &mut self.tab.xtb.orbitals;
                if state.building == Some(key) {
                    state.building = None;
                }
                match outcome {
                    Ok(meshes) => surface::draw::register(key, meshes.0),
                    Err(error) => state.failed.push((key, error)),
                }
            }
            Action::Choose(column, index) => self.xtb_orbital_choose(column, index),
            Action::Column(column) => {
                let Some(target) = self.xtb_orbital_target() else {
                    return Task::none();
                };
                let choice = self.xtb_orbital_choice(&target);
                let Some(spectrum) = target.columns.get(column) else {
                    return Task::none();
                };
                // The same level name in the other spin.
                let homo = target.columns[choice.column].homo().unwrap_or(0) as i64;
                let offset = choice.index as i64 - homo;
                let index = (spectrum.homo().unwrap_or(0) as i64 + offset)
                    .clamp(0, spectrum.energies_ev.len() as i64 - 1);
                self.xtb_orbital_choose(column, index as usize);
            }
            Action::Homo | Action::Lumo => {
                let Some(target) = self.xtb_orbital_target() else {
                    return Task::none();
                };
                let column = self.xtb_orbital_choice(&target).column;
                let spectrum = &target.columns[column];
                let index = if matches!(action, Action::Homo) {
                    spectrum.homo()
                } else {
                    spectrum.lumo()
                };
                if let Some(index) = index {
                    self.xtb_orbital_choose(column, index);
                }
            }
            Action::Step(step) => {
                let Some(target) = self.xtb_orbital_target() else {
                    return Task::none();
                };
                let choice = self.xtb_orbital_choice(&target);
                let last = target.columns[choice.column].energies_ev.len() as i64 - 1;
                // ↑ goes up the diagram: to higher levels.
                let index = (choice.index as i64 + i64::from(step)).clamp(0, last);
                self.xtb_orbital_choose(choice.column, index as usize);
            }
            Action::Iso(iso) => self.xtb_orbital_setting(Drag::Iso(iso)),
            Action::Opacity(opacity) => self.xtb_orbital_setting(Drag::Opacity(opacity)),
            Action::Settle => {
                if let Some(drag) = self.tab.xtb.orbitals.drag.take() {
                    self.xtb_orbital_edit(|figure| match drag {
                        Drag::Iso(iso) => figure.iso = iso,
                        Drag::Opacity(opacity) => figure.look.opacity = opacity,
                    });
                }
            }
            Action::Look(choice) => {
                let placed = self
                    .xtb_orbital_target()
                    .is_some_and(|t| t.placed.is_some());
                if placed {
                    self.xtb_orbital_edit(|figure| figure.look = choice.apply(figure.look));
                } else {
                    let state = &mut self.tab.xtb.orbitals;
                    state.look = choice.apply(state.look);
                }
            }
            Action::MeshLines(on) => {
                let lines = |look: Look| match look.fill {
                    Fill::None => look,
                    _ => Look { mesh: on, ..look },
                };
                let placed = self
                    .xtb_orbital_target()
                    .is_some_and(|t| t.placed.is_some());
                if placed {
                    self.xtb_orbital_edit(|figure| figure.look = lines(figure.look));
                } else {
                    let state = &mut self.tab.xtb.orbitals;
                    state.look = lines(state.look);
                }
            }
            Action::Place => self.xtb_orbital_place(),
            Action::Remove => {
                let Some(model) = self
                    .xtb_orbital_target()
                    .and_then(|t| t.placed.and(t.model))
                else {
                    return Task::none();
                };
                let mut doc = self.tab.doc.clone();
                doc.models[model].surface = None;
                let before = std::mem::replace(&mut self.tab.doc, doc);
                self.changed(before);
                self.status = "Removed the orbital from the canvas · ⌘Z restores it".into();
                self.error = false;
            }
        }
        Task::none()
    }

    /// Choose a level: the preview follows, or a placed figure changes as
    /// one Undo step.
    fn xtb_orbital_choose(&mut self, column: usize, index: usize) {
        let Some(target) = self.xtb_orbital_target() else {
            return;
        };
        let Some(spectrum) = target.columns.get(column) else {
            return;
        };
        if index >= spectrum.energies_ev.len() {
            return;
        }
        let choice = Choice {
            result: target.key.clone(),
            column,
            index,
        };
        self.status = format!(
            "{} · {:.2} eV",
            spectrum.name(index),
            spectrum.energies_ev[index]
        );
        self.error = false;
        if target.placed.is_some() {
            let spin = spectrum.spin;
            if !self.xtb_orbital_pin(&target, spin, index, |figure| {
                figure.orbital.spin = spin;
                figure.orbital.index = index as u32;
            }) {
                return;
            }
        }
        self.tab.xtb.orbitals.choice = Some(choice);
    }

    /// A slider moved: the preview takes it at once; a placed figure shows
    /// it until the slider is let go.
    fn xtb_orbital_setting(&mut self, drag: Drag) {
        let placed = self
            .xtb_orbital_target()
            .is_some_and(|t| t.placed.is_some());
        let state = &mut self.tab.xtb.orbitals;
        if placed {
            state.drag = Some(drag);
            return;
        }
        match drag {
            Drag::Iso(iso) => state.iso = iso,
            Drag::Opacity(opacity) => state.look.opacity = opacity,
        }
    }

    /// Change the placed figure as one Undo step.
    fn xtb_orbital_edit(&mut self, change: impl FnOnce(&mut Figure)) {
        let Some(model) = self
            .xtb_orbital_target()
            .and_then(|t| t.placed.and(t.model))
        else {
            return;
        };
        let mut doc = self.tab.doc.clone();
        let Some(figure) = doc.models[model].surface.as_mut() else {
            return;
        };
        change(figure);
        figure.snapshot = None;
        if doc != self.tab.doc {
            let before = std::mem::replace(&mut self.tab.doc, doc);
            self.changed(before);
        }
    }

    /// Change the placed figure to orbital `index` of `spin`, keeping that
    /// orbital's coefficients with an xTB result, as one Undo step. False
    /// (with a status) while the coefficients are still being computed.
    fn xtb_orbital_pin(
        &mut self,
        target: &Target,
        spin: qcdata::Spin,
        index: usize,
        change: impl FnOnce(&mut Figure),
    ) -> bool {
        let Some(model) = target.model else {
            return false;
        };
        let mut doc = self.tab.doc.clone();
        let Some(result) = current_result(&doc, &target.key).cloned() else {
            return false;
        };
        let stored = result
            .orbitals
            .as_ref()
            .and_then(|o| compute::row(o, spin, index))
            .is_some();
        if !stored {
            let computed = self
                .tab
                .xtb
                .orbitals
                .computed
                .iter()
                .find(|(k, _)| *k == target.key)
                .and_then(|(_, o)| compute::row(o, spin, index));
            let Some((basis, row)) = computed else {
                self.status = "Computing the orbitals… try again in a moment".into();
                self.error = false;
                return false;
            };
            if let Err(error) =
                science::pin_orbital(&mut doc, &target.key, spin, index as u32, basis, row)
            {
                self.status = format!("The orbital cannot be kept with the drawing: {error}");
                self.error = true;
                return false;
            }
        }
        let figure = doc.models[model].surface.get_or_insert_with(|| {
            self.xtb_orbital_figure(target, &self.xtb_orbital_choice(target))
        });
        change(figure);
        figure.snapshot = None;
        if doc.models[model].display == reshiki::models3d::Display::TwoD {
            reshiki::models3d::set_display(&mut doc, model, reshiki::models3d::Display::Both);
        }
        self.xtb_follow_results(&doc);
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
        true
    }

    /// Place on canvas: the previewed orbital becomes part of the drawing.
    fn xtb_orbital_place(&mut self) {
        let Some(target) = self.xtb_orbital_target() else {
            return;
        };
        if target.placed.is_some() || target.problem.is_some() {
            return;
        }
        let choice = self.xtb_orbital_choice(&target);
        let figure = self.xtb_orbital_figure(&target, &choice);
        let name = target.columns[choice.column].name(choice.index);
        if self.xtb_orbital_pin(&target, figure.orbital.spin, choice.index, |placed| {
            *placed = figure;
        }) {
            self.status = format!("Placed {name} on the canvas · ⌘Z removes it");
            self.error = false;
        }
    }

    /// ↑ and ↓ choose orbitals while the Orbitals tab is in front and
    /// nothing on the canvas would move instead.
    pub(in crate::app) fn xtb_orbital_keys(&self) -> bool {
        self.tab.selected.is_empty()
            && self.xtb_palette_visible()
            && self.tab.xtb.palette.view == View::Orbitals
            && self.xtb_orbital_target().is_some()
    }

    /// What the tab says about the work in flight for the shown figure.
    pub(super) fn xtb_orbital_busy(&self, target: &Target) -> Option<String> {
        let state = &self.tab.xtb.orbitals;
        if state
            .computing
            .as_ref()
            .is_some_and(|c| c.key == target.key)
        {
            return Some("Computing orbitals…".into());
        }
        let doc = self.display_document();
        let model = doc.models.get(target.model?)?;
        let key = surface::draw::key(doc, model)?;
        if let Some((_, error)) = state.failed.iter().find(|(k, _)| *k == key) {
            return Some(error.clone());
        }
        (state.building == Some(key)).then(|| "Drawing the surface…".into())
    }

    /// The row of the chosen orbital is at hand: Place can keep it.
    pub(super) fn xtb_orbital_ready(&self, target: &Target, choice: &Choice) -> bool {
        let spin = target.columns[choice.column].spin;
        let state = &self.tab.xtb.orbitals;
        current_result(&self.tab.doc, &target.key).is_some_and(|result| {
            result
                .orbitals
                .as_ref()
                .and_then(|o| compute::row(o, spin, choice.index))
                .is_some()
                || state
                    .computed
                    .iter()
                    .find(|(k, _)| *k == target.key)
                    .and_then(|(_, o)| compute::row(o, spin, choice.index))
                    .is_some()
        })
    }

    /// The tab's isovalue and look: the placed figure's (with a held
    /// slider), else the preview's.
    pub(super) fn xtb_orbital_settings(&self, target: &Target) -> (f32, Look) {
        let state = &self.tab.xtb.orbitals;
        let (mut iso, mut look) = target
            .placed
            .as_ref()
            .map_or((state.iso, state.look), |f| (f.iso, f.look));
        match state.drag {
            Some(Drag::Iso(value)) => iso = value,
            Some(Drag::Opacity(value)) => look.opacity = value,
            None => {}
        }
        (iso, look)
    }
}

/// The result with result key `key`, while it describes its molecule as it
/// is now.
fn current_result<'a>(doc: &'a Document, key: &str) -> Option<&'a Arc<CalcResult>> {
    let (_, stored) = science::find_result(doc, key)?;
    let link = doc.science.attachment(stored.attachment)?;
    (link.freshness(doc) == Freshness::Current).then_some(&stored.result)
}

/// Run `work` on its own thread; the UI thread only waits for the answer.
#[cfg(not(target_arch = "wasm32"))]
fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> impl std::future::Future<Output = Result<T, String>> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("orbitals".into())
        .spawn(move || {
            let _ = tx.send(work());
        });
    async move {
        spawned.map_err(|error| error.to_string())?;
        rx.await
            .unwrap_or_else(|_| Err("The orbital thread stopped".into()))
    }
}

/// The browser has no threads here: the work runs when the task does.
#[cfg(target_arch = "wasm32")]
fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> impl std::future::Future<Output = Result<T, String>> {
    async move { work() }
}

/// A recomputation in flight; dropping it (a closed tab, an answer) stops
/// the worker.
struct Computing {
    key: String,
    _task: iced::task::Handle,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: reshiki_xtb_worker::client::Cancel,
}

impl Drop for Computing {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.cancel.cancel();
    }
}
