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
use reshiki::qcdata::{self, CalcResult, Settings, Spin};
use reshiki::science::{self, Stored};
use std::sync::Arc;

mod compute;
mod levels;
mod view;

#[cfg(test)]
mod tests;

/// The isovalue a new preview starts at, and the slider's range (a.u.).
pub(crate) const ISO: f32 = 0.05;
pub(crate) const ISO_RANGE: std::ops::RangeInclusive<f32> = 0.01..=0.10;
pub(crate) const PRESETS: [f32; 3] = [0.02, 0.05, 0.10];
/// Recomputed orbital sets kept per document: one per recent result.
const COMPUTED: usize = 4;
/// Orbitals recomputed on each side of the frontier, beyond those asked for.
const AROUND: u32 = 8;

fn send(action: Action) -> Message {
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
    /// ↑ (+1) or ↓ (−1) while nothing is selected on the canvas.
    Step(i32),
    /// The isovalue slider: a placed figure changes at once, one Undo step
    /// per drag.
    Iso(f32),
    /// An isovalue preset: its own Undo step.
    Preset(f32),
    Opacity(u8),
    /// A slider let go: the next change starts a new Undo step.
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
        let lines = look.mesh && look.fill != Fill::None;
        let (fill, mesh) = match self {
            Self::Solid => (Fill::Solid, lines),
            Self::Shaded => (Fill::Shaded, lines),
            Self::Outline => (Fill::Outline, lines),
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
    /// The drawing state after the last slider change to a placed figure:
    /// the next change from that state joins its Undo step.
    sliding: Option<(u64, u64)>,
    /// The drawing state when an orbital was chosen in the tab: ↑ and ↓ go
    /// on choosing while it lasts.
    keys: Option<(u64, u64)>,
    /// A level chosen for a placed figure while its coefficients are being
    /// computed: applied when they arrive.
    pending: Option<Choice>,
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
    /// What the last sync saw; the same state needs no work.
    synced: Option<Synced>,
}

impl Default for Orbitals {
    fn default() -> Self {
        Self {
            choice: None,
            iso: ISO,
            look: Look::default(),
            sliding: None,
            keys: None,
            pending: None,
            computed: vec![],
            computing: None,
            building: None,
            failed: vec![],
            errors: vec![],
            preview: None,
            synced: None,
        }
    }
}

/// The chosen orbital: a result, a spin column and a level.
#[derive(Clone, Debug, PartialEq)]
struct Choice {
    pub result: String,
    pub column: usize,
    pub index: usize,
}

/// The shown result's storage index and Arc address.
type ResultId = (usize, usize);

/// The document state and figure a preview was made for.
#[derive(Clone, Debug, PartialEq)]
struct Stamp {
    revision: (u64, u64),
    shown: ResultId,
    model: usize,
    figure: Figure,
}

/// Everything the sync reads besides the drawing's revision.
#[derive(Clone, Debug, PartialEq)]
struct Synced {
    revision: (u64, u64, u64),
    visible: bool,
    shown: Option<ResultId>,
    choice: Option<Choice>,
    pending: Option<Choice>,
    iso: u32,
    look: Look,
    building: Option<u64>,
    computing: bool,
    /// Recomputed row sets by result and row count.
    computed: Vec<(String, usize)>,
    counts: [usize; 2],
    meshes: u64,
    /// Which drawing the canvas shows: another preview may take over.
    display: usize,
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

/// What the tab shows for the palette's result.
struct Target {
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
    /// The Orbitals tab is on screen: the palette shows it docked at the
    /// bottom and expanded, or in the inspector's xTB page.
    fn xtb_orbitals_visible(&self) -> bool {
        self.xtb_palette_visible()
            && self.xtb_failed().is_none()
            && self.tab.xtb.palette.view == View::Orbitals
            && if self.xtb.dock.inspector {
                self.inspector_open
                    && self.inspector_tab == crate::app::InspectorTab::Predict
                    && self.xtb.page == super::Page::Xtb
            } else {
                !self.xtb.dock.collapsed
            }
    }

    /// The tab's result, its levels and where its surface would go.
    fn xtb_orbital_target(&self) -> Option<Target> {
        let shown = self.xtb_shown()?;
        let result = &shown.stored.result;
        let orbitals = result.orbitals.as_ref()?;
        let columns = levels::columns(orbitals);
        if columns.is_empty() {
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
                        // The renderer's own test (`figure_ready`).
                        .all(|((_, p), q)| {
                            (0..3)
                                .map(|k| (f64::from(p[k]) - q[k]).powi(2))
                                .sum::<f64>()
                                <= 1e-6
                        })
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
        } else if orbitals.basis.is_none()
            && !super::available()
            && self
                .tab
                .xtb
                .orbitals
                .computed
                .iter()
                .all(|(k, _)| *k != key)
        {
            Some("Needs xTB to show: this build cannot recompute orbitals.".into())
        } else if cfg!(target_arch = "wasm32") {
            Some("Orbital surfaces are drawn in the desktop app.".into())
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
    fn xtb_orbital_choice(&self, target: &Target) -> Choice {
        let chosen = self
            .tab
            .xtb
            .orbitals
            .choice
            .as_ref()
            .filter(|c| c.result == target.key && c.column < target.columns.len());
        if let Some(figure) = &target.placed {
            // Open shells sharing orbitals show one orbital in both columns:
            // keep the column the user picked.
            let column = chosen
                .filter(|c| {
                    c.index == figure.orbital.index as usize
                        && target.columns[c.column].spin == figure.orbital.spin
                })
                .map(|c| c.column)
                .or_else(|| {
                    target
                        .columns
                        .iter()
                        .position(|c| c.spin == figure.orbital.spin)
                })
                .unwrap_or(0);
            return Choice {
                result: target.key.clone(),
                column,
                index: figure.orbital.index as usize,
            };
        }
        chosen.cloned().unwrap_or_else(|| Choice {
            result: target.key.clone(),
            column: 0,
            index: target.columns[0].homo().unwrap_or(0),
        })
    }

    /// The figure a preview or Place shows for `choice`.
    fn xtb_orbital_figure(&self, target: &Target, choice: &Choice) -> Figure {
        let state = &self.tab.xtb.orbitals;
        let mut figure = target.placed.clone().unwrap_or_else(|| Figure {
            orbital: OrbitalRef {
                result: target.key.clone(),
                spin: Spin::Alpha,
                index: 0,
            },
            iso: state.iso,
            look: state.look,
            snapshot: None,
        });
        figure.orbital.spin = target.columns[choice.column].spin;
        figure.orbital.index = choice.index as u32;
        figure.snapshot = None;
        figure
    }

    fn xtb_orbital_result_id(&self) -> Option<ResultId> {
        self.xtb_shown_entry()
            .map(|(index, stored)| (index, Arc::as_ptr(&stored.result) as usize))
    }

    /// The canvas's drawing with the previewed orbital, while the Orbitals
    /// tab is in front.
    pub(super) fn xtb_orbital_preview(&self) -> Option<&Document> {
        let (stamp, doc) = self.tab.xtb.orbitals.preview.as_ref()?;
        (stamp.revision == (self.tab.file_epoch, self.tab.revision)
            && Some(stamp.shown) == self.xtb_orbital_result_id())
        .then_some(doc.as_ref())
    }

    /// After every message: keep the preview in step with the tab and the
    /// drawing, and start the work the shown figures still need. Cheap when
    /// nothing it reads changed.
    pub(in crate::app) fn xtb_orbitals_sync(&mut self) -> Task<Message> {
        let state = &self.tab.xtb.orbitals;
        let visible = self.xtb_orbitals_visible();
        let shown = self.xtb_orbital_result_id();
        let synced = Synced {
            revision: (self.tab.id.0, self.tab.file_epoch, self.tab.revision),
            visible,
            shown,
            choice: state.choice.clone(),
            pending: state.pending.clone(),
            iso: state.iso.to_bits(),
            look: state.look,
            building: state.building,
            computing: state.computing.is_some(),
            computed: state
                .computed
                .iter()
                .map(|(k, o)| (k.clone(), o.alpha.rows.len()))
                .collect(),
            counts: [state.failed.len(), state.errors.len()],
            meshes: surface::draw::generation(),
            display: std::ptr::from_ref(self.display_document()) as usize,
        };
        if state.synced.as_ref() == Some(&synced) {
            return Task::none();
        }
        self.tab.xtb.orbitals.synced = Some(synced);
        self.xtb_orbitals_preview_sync(shown.filter(|_| visible));
        self.xtb_orbitals_feed()
    }

    fn xtb_orbitals_preview_sync(&mut self, shown: Option<ResultId>) {
        let wanted = (|| {
            let shown = shown?;
            let target = self.xtb_orbital_target()?;
            let model = target.model.filter(|_| target.problem.is_none())?;
            // A placed figure shows itself.
            if target.placed.is_some() {
                return None;
            }
            let choice = self.xtb_orbital_choice(&target);
            let figure = self.xtb_orbital_figure(&target, &choice);
            Some(Stamp {
                revision: (self.tab.file_epoch, self.tab.revision),
                shown,
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
        // The browser draws saved snapshots only: no threads for meshes.
        if cfg!(target_arch = "wasm32") {
            return Task::none();
        }
        let doc = self.display_document();
        if doc.models.iter().all(|m| m.surface.is_none()) {
            return Task::none();
        }
        let state = &self.tab.xtb.orbitals;
        let mut recompute = None;
        let mut build = None;
        let mut failed = vec![];
        for model in &doc.models {
            let Some(figure) = &model.surface else {
                continue;
            };
            let Some(key) = surface::draw::key(doc, model) else {
                continue;
            };
            if state.building == Some(key)
                || state.failed.iter().any(|(k, _)| *k == key)
                || !surface::draw::figure_ready(doc, model)
                || surface::draw::registered(key).is_some()
            {
                continue;
            }
            let Some((_, stored)) = science::figure_result(doc, model) else {
                continue;
            };
            let result = &stored.result;
            let index = figure.orbital.index as usize;
            match self.xtb_orbital_row(stored, figure.orbital.spin, index) {
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
                None if !levels_hold(result, figure.orbital.spin, index) => {
                    failed.push((key, "This orbital is not in the result".to_owned()));
                }
                None if matches!(result.settings, Settings::Xtb(_))
                    && super::available()
                    && state.computing.is_none()
                    && !state
                        .errors
                        .iter()
                        .any(|(k, _)| *k == figure.orbital.result) =>
                {
                    let frame = doc.science.frame(&result.frame).cloned();
                    recompute = recompute
                        .or(frame.map(|f| (result.clone(), f, vec![figure.orbital.index])));
                }
                None => {}
            }
        }
        // A level chosen for a placed figure waits for its coefficients.
        if recompute.is_none()
            && state.computing.is_none()
            && let Some(pending) = &state.pending
            && let Some(target) = self.xtb_orbital_target()
            && target.key == pending.result
            && let Some(model) = target.model
            && let Some((_, stored)) =
                science::figure_result(&self.tab.doc, &self.tab.doc.models[model])
            && matches!(stored.result.settings, Settings::Xtb(_))
            && !state.errors.iter().any(|(k, _)| *k == pending.result)
        {
            let frame = self.tab.doc.science.frame(&stored.result.frame).cloned();
            recompute = frame.map(|f| (stored.result.clone(), f, vec![pending.index as u32]));
        }
        let tab = self.tab.id;
        self.tab.xtb.orbitals.failed.extend(failed);
        let mut tasks = vec![];
        if let Some((key, basis, row, points, iso)) = build {
            self.tab.xtb.orbitals.building = Some(key);
            tasks.push(Task::perform(
                off_thread(move || compute::meshes(&basis, &row, &points, iso)),
                move |meshes| routed(tab, Action::Built(key, meshes)),
            ));
        }
        if let Some((result, frame, rows)) = recompute {
            tasks.push(self.xtb_orbitals_recompute(result, frame, rows));
        }
        Task::batch(tasks)
    }

    /// The coefficient row of an orbital of `stored`: kept with the result,
    /// or recomputed in this session.
    fn xtb_orbital_row<'a>(
        &'a self,
        stored: &'a Stored,
        spin: Spin,
        index: usize,
    ) -> Option<(&'a qcdata::BasisSet, &'a [f64])> {
        let key = science::result_key(&stored.result);
        stored
            .result
            .orbitals
            .as_ref()
            .and_then(|o| compute::row(o, spin, index))
            .or_else(|| {
                let state = &self.tab.xtb.orbitals;
                let (_, orbitals) = state.computed.iter().find(|(k, _)| *k == key)?;
                compute::row(orbitals, spin, index)
            })
    }

    #[cfg(target_arch = "wasm32")]
    fn xtb_orbitals_recompute(
        &mut self,
        _: Arc<CalcResult>,
        _: Arc<qcdata::Frame>,
        _: Vec<u32>,
    ) -> Task<Message> {
        Task::none()
    }

    /// Recompute the frontier orbitals of `result`, and `wanted`.
    #[cfg(not(target_arch = "wasm32"))]
    fn xtb_orbitals_recompute(
        &mut self,
        result: Arc<CalcResult>,
        frame: Arc<qcdata::Frame>,
        wanted: Vec<u32>,
    ) -> Task<Message> {
        let key = science::result_key(&result);
        let levels = result
            .orbitals
            .as_ref()
            .map_or(0, |o| o.alpha.energies_hartree.len() as u32);
        let frontier = result
            .orbitals
            .as_ref()
            .and_then(|o| levels::columns(o).first().and_then(|c| c.occupied))
            .unwrap_or(0) as u32;
        let mut rows: Vec<u32> = (frontier.saturating_sub(AROUND)..(frontier + AROUND).min(levels))
            .chain(wanted.into_iter().filter(|&row| row < levels))
            .collect();
        rows.sort_unstable();
        rows.dedup();
        let cancel = reshiki_xtb_worker::client::Cancel::default();
        let worker = cancel.clone();
        let reply = key.clone();
        let tab = self.tab.id;
        let (task, handle) = Task::perform(
            off_thread(move || compute::orbitals(&result, &frame, rows, &worker).map(Arc::new)),
            move |orbitals| routed(tab, Action::Computed(reply.clone(), orbitals)),
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
        if !matches!(
            action,
            Action::Iso(_) | Action::Opacity(_) | Action::Computed(..) | Action::Built(..)
        ) {
            self.tab.xtb.orbitals.sliding = None;
        }
        match action {
            Action::Computed(key, outcome) => {
                let state = &mut self.tab.xtb.orbitals;
                if state.computing.as_ref().is_some_and(|c| c.key == key) {
                    state.computing = None;
                }
                match outcome {
                    Ok(orbitals) => {
                        state.computed.retain(|(k, _)| *k != key);
                        state.computed.push((key.clone(), orbitals));
                        if state.computed.len() > COMPUTED {
                            state.computed.remove(0);
                        }
                        // A level chosen for this result while it ran.
                        if let Some(pending) = state.pending.take_if(|p| p.result == key)
                            && self
                                .xtb_orbital_target()
                                .is_some_and(|t| t.key == pending.result)
                        {
                            self.xtb_orbital_choose(pending.column, pending.index);
                        }
                    }
                    Err(error) => {
                        state.pending.take_if(|p| p.result == key);
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
                // The same level name in the other spin, counted from its
                // frontier (its LUMO when it has no occupied level).
                let from = target.columns[choice.column].occupied.unwrap_or(0) as i64;
                let to = spectrum.occupied.unwrap_or(0) as i64;
                let last = spectrum.energies_ev.len() as i64 - 1;
                let index = (to + choice.index as i64 - from).clamp(0, last.max(0));
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
                let index = (choice.index as i64 + i64::from(step)).clamp(0, last.max(0));
                self.xtb_orbital_choose(choice.column, index as usize);
            }
            Action::Iso(iso) | Action::Preset(iso) => {
                let iso = iso.clamp(*ISO_RANGE.start(), *ISO_RANGE.end());
                self.xtb_orbital_setting(|figure| figure.iso = iso, |state| state.iso = iso);
            }
            Action::Opacity(opacity) => {
                let opacity = opacity.clamp(20, 100);
                self.xtb_orbital_setting(
                    |figure| figure.look.opacity = opacity,
                    |state| state.look.opacity = opacity,
                );
            }
            Action::Settle => {}
            Action::Look(choice) => self.xtb_orbital_setting(
                |figure| figure.look = choice.apply(figure.look),
                |state| state.look = choice.apply(state.look),
            ),
            Action::MeshLines(on) => {
                let lines = move |look: Look| match look.fill {
                    Fill::None => look,
                    _ => Look { mesh: on, ..look },
                };
                self.xtb_orbital_setting(
                    |figure| figure.look = lines(figure.look),
                    |state| state.look = lines(state.look),
                );
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
    /// one Undo step once its coefficients are at hand.
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
        // Choosing again retries a recomputation that failed.
        self.tab
            .xtb
            .orbitals
            .errors
            .retain(|(k, _)| *k != target.key);
        if target.placed.is_some() {
            let spin = spectrum.spin;
            if !self.xtb_orbital_pin(&target, spin, index, |figure| {
                figure.orbital.spin = spin;
                figure.orbital.index = index as u32;
            }) {
                // Only xTB orbitals can be recomputed; a file's missing row
                // stays missing.
                let recomputable = target
                    .model
                    .and_then(|m| science::figure_result(&self.tab.doc, &self.tab.doc.models[m]))
                    .is_some_and(|(_, s)| matches!(s.result.settings, Settings::Xtb(_)));
                if self.error {
                } else if recomputable && super::available() {
                    self.status = "Computing the orbital… it is placed when ready".into();
                    self.tab.xtb.orbitals.pending = Some(choice);
                } else {
                    self.status = "This orbital's shape is not saved with this result".into();
                }
                return;
            }
        }
        self.tab.xtb.orbitals.pending = None;
        self.tab.xtb.orbitals.choice = Some(choice);
        self.tab.xtb.orbitals.keys = Some((self.tab.file_epoch, self.tab.revision));
    }

    /// A setting: the preview takes it, a placed figure changes as an Undo
    /// step (a held slider's changes share one).
    fn xtb_orbital_setting(
        &mut self,
        placed: impl FnOnce(&mut Figure),
        preview: impl FnOnce(&mut Orbitals),
    ) {
        let Some(model) = self
            .xtb_orbital_target()
            .and_then(|t| t.placed.and(t.model))
        else {
            preview(&mut self.tab.xtb.orbitals);
            return;
        };
        let mut doc = self.tab.doc.clone();
        let Some(figure) = doc.models[model].surface.as_mut() else {
            return;
        };
        placed(figure);
        figure.snapshot = None;
        if doc == self.tab.doc {
            return;
        }
        let continuing =
            self.tab.xtb.orbitals.sliding == Some((self.tab.file_epoch, self.tab.revision));
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed_continuing(before, continuing);
        self.tab.xtb.orbitals.sliding = Some((self.tab.file_epoch, self.tab.revision));
    }

    /// Change the placed figure (or place one) for orbital `index` of
    /// `spin`, keeping that orbital's coefficients with an xTB result, as one
    /// Undo step. False while the coefficients are not at hand.
    fn xtb_orbital_pin(
        &mut self,
        target: &Target,
        spin: Spin,
        index: usize,
        change: impl FnOnce(&mut Figure),
    ) -> bool {
        let Some(model) = target.model else {
            return false;
        };
        let mut doc = self.tab.doc.clone();
        // A figure of another result on this model gives way to this one.
        if target.placed.is_none() {
            doc.models[model].surface =
                Some(self.xtb_orbital_figure(target, &self.xtb_orbital_choice(target)));
        }
        let Some((stored_index, stored)) = science::figure_result(&doc, &doc.models[model]) else {
            return false;
        };
        let kept = stored
            .result
            .orbitals
            .as_ref()
            .and_then(|o| compute::row(o, spin, index))
            .is_some();
        if !kept {
            let Some((basis, row)) = self
                .xtb_orbital_row(stored, spin, index)
                .map(|(basis, row)| (basis.clone(), row.to_vec()))
            else {
                return false;
            };
            if let Err(error) =
                science::pin_orbital(&mut doc, stored_index, spin, index as u32, &basis, &row)
            {
                self.status = format!("The orbital cannot be kept with the drawing: {error}");
                self.error = true;
                return false;
            }
        }
        if let Some(figure) = doc.models[model].surface.as_mut() {
            change(figure);
            figure.snapshot = None;
        }
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

    /// ↑ and ↓ choose orbitals after one was chosen in the tab, while it is
    /// on screen and nothing is selected.
    pub(in crate::app) fn xtb_orbital_keys(&self) -> bool {
        self.tab.xtb.orbitals.keys == Some((self.tab.file_epoch, self.tab.revision))
            && self.tab.selected.is_empty()
            && self.connecting().is_none()
            && self.xtb_orbitals_visible()
            && self.xtb_orbital_target().is_some()
    }

    /// What the tab says about the work in flight for the shown figure.
    fn xtb_orbital_busy(&self, target: &Target) -> Option<String> {
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
        if state.building == Some(key) {
            return Some("Drawing the surface…".into());
        }
        surface::draw::registered(key)
            .filter(|meshes| meshes[0].triangles.is_empty())
            .map(|_| "No surface at this isovalue: lower it.".into())
    }

    /// The row of the chosen orbital is at hand: Place can keep it.
    fn xtb_orbital_ready(&self, target: &Target, choice: &Choice) -> bool {
        let spin = target.columns[choice.column].spin;
        let Some(model) = target.model else {
            return false;
        };
        let doc = self.display_document();
        doc.models
            .get(model)
            .and_then(|model| science::figure_result(doc, model))
            .is_some_and(|(_, stored)| self.xtb_orbital_row(stored, spin, choice.index).is_some())
    }

    /// The tab's isovalue and look: the placed figure's, else the preview's.
    fn xtb_orbital_settings(&self, target: &Target) -> (f32, Look) {
        let state = &self.tab.xtb.orbitals;
        target
            .placed
            .as_ref()
            .map_or((state.iso, state.look), |f| (f.iso, f.look))
    }
}

/// Whether `result` lists orbital `index` of `spin`.
fn levels_hold(result: &CalcResult, spin: Spin, index: usize) -> bool {
    result.orbitals.as_ref().is_some_and(|orbitals| {
        let set = match (&orbitals.beta, spin) {
            (qcdata::Beta::Unrestricted(beta), Spin::Beta) => beta,
            _ => &orbitals.alpha,
        };
        index < set.energies_hartree.len()
    })
}

/// A reply for the tab that started the work, wherever the user is now.
fn routed(tab: crate::app::document_tab::TabId, action: Action) -> Message {
    Message::Tab(tab, Box::new(send(action)))
}

/// Run `work` on its own thread; the UI thread only waits for the answer.
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
