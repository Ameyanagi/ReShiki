//! Show a selected molecule as its 2D drawing, a 3D model or both (#386).
//!
//! A molecule without a model gets one from the geometry worker: one abortable
//! job per tab, whose result is dropped when the drawing changed meanwhile.
//! Each switch, including the one that adds the model, is one Undo step, and
//! so is each turn or resize of a model on the canvas.
use super::icons::{Glyph, Icon};
use super::workspace::{accessible_command, control, muted_text};
use super::{App, Message, document_tab::TabId, inspector::Section};
use crate::canvas::Edit;
use iced::widget::{Space, canvas, column, row, slider, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::geometry::{
    Client, DEFAULT_SEED, ForceField, GENERATION_SEED_MAX, Optimized, Prepared,
};
use reshiki::models3d::{
    self, Background, Display, Hydrogens, Labels, Layout, Look, Probability, Style,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    tab: TabId,
    epoch: u64,
    revision: u64,
    serial: u64,
}

/// Chemistry preparation is too expensive to repeat while drawing the panels.
/// A tab's epoch distinguishes file loads that reuse a revision number.
#[derive(Default)]
pub(super) struct StaleCache {
    key: Option<(TabId, u64, u64)>,
    values: Vec<Option<bool>>,
    #[cfg(test)]
    comparisons: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Show(Display),
    Update,
    Regenerate,
    /// Show as 2D, the drawing flattened from the model as it is turned now.
    Flatten,
    /// The 2D layout that the Structure section's 2D button uses.
    Layout(Layout),
    Generated(Key, Result<Arc<Optimized>, String>),
    /// Change the selected model; each change is one Undo step, and a slider
    /// drag is one step until [`Action::SlideEnd`].
    Set(Setting),
    SlideEnd,
    /// The button of what is shown already: it keeps its selected look
    /// rather than looking disabled, and changes nothing.
    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Setting {
    Style(Style),
    Probability(Probability),
    MoleculeOnly(bool),
    IncludeSolvent(bool),
    Look(Look),
    Hydrogens(Hydrogens),
    Labels(Labels),
    Background(Background),
    DepthFade(f32),
    Perspective(f32),
    ResetView,
    /// Show a crystal's ion charges in its 2D drawing.
    Charges(bool),
    /// How many cells along a, b and c a crystal shows.
    Repeat([u8; 3]),
    /// Turn a crystal to look along a lattice direction.
    LookAlong(models3d::Along),
}

/// The generation in flight; dropping it aborts the worker.
pub(crate) struct Job {
    /// The key the worker answers with; it never changes.
    key: Key,
    /// The drawing state the result still applies to: `key` until switching
    /// another molecule's display moves it on.
    current: Key,
    display: Display,
    /// An existing model to replace, keeping its view at completion.
    target: Option<usize>,
    seed: i32,
    force_field: ForceField,
    prepared: Arc<Prepared>,
    started: iced::time::Instant,
    _handle: iced::task::Handle,
}

impl App {
    pub(super) fn models3d_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(display) => self.show_model(display),
            Action::Update => self.regenerate_model(false),
            Action::Regenerate => self.regenerate_model(true),
            Action::Set(setting) => {
                self.set_model(setting);
                Task::none()
            }
            Action::SlideEnd => {
                self.tab.model_sliding = None;
                Task::none()
            }
            Action::Unchanged => Task::none(),
            Action::Flatten => {
                self.flatten_model();
                Task::none()
            }
            Action::Layout(layout) => {
                if let Some((_, Some(index))) = self.selected_model() {
                    if layout == Layout::View && self.model_stale(index) {
                        self.status = "Update the model first".into();
                        self.error = false;
                        return Task::none();
                    }
                    let before = self.tab.doc.clone();
                    if let Some(model) = self.tab.doc.models.get_mut(index) {
                        model.layout = layout;
                    }
                    // Original drawing always means the drawn layout is in
                    // place, so a flattened drawing comes back in this step.
                    let restored = layout == Layout::Original
                        && models3d::restore_drawing(&mut self.tab.doc, index);
                    if self.tab.doc != before {
                        self.commit_model(before, false);
                        if restored && !self.error {
                            self.status = "Original 2D drawing restored".into();
                        }
                    }
                }
                Task::none()
            }
            Action::Generated(key, result) => {
                let Some(job) = self.tab.model_job.take_if(|job| job.key == key) else {
                    return Task::none();
                };
                if job.current != self.model_key() {
                    self.status = "Drawing changed · 3D model not applied".into();
                    self.error = false;
                    return Task::none();
                }
                // A preview or label editor owns the drawing; adding a model
                // under it would cancel or outdate it.
                if self.tab.drawing_edit_active() {
                    self.status = "3D model not applied · Choose 3D again after this edit".into();
                    self.error = false;
                    return Task::none();
                }
                let model = result.and_then(|optimized| {
                    job.prepared
                        .model(&optimized)
                        .map_err(|error| error.to_string())
                });
                match model {
                    Ok(mut model) => {
                        model.seed = job.seed;
                        if let Some(index) = job.target {
                            self.replace_model(index, model);
                        } else {
                            self.add_model(model, job.display);
                        }
                    }
                    Err(error) => {
                        self.status = format!("3D model not generated: {error}");
                        self.error = true;
                    }
                }
                Task::none()
            }
        }
    }

    fn model_key(&self) -> Key {
        Key {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            serial: self.tab.model_serial,
        }
    }

    /// The display a still-current generation for these atoms will show.
    pub(super) fn pending(&self, atoms: &[u64]) -> Option<Display> {
        self.tab
            .model_job
            .as_ref()
            .filter(|job| {
                let ids = job.prepared.ids();
                job.current == self.model_key()
                    && (job.target.is_some() || ids.len() == atoms.len())
                    && !atoms.is_empty()
                    && atoms.iter().all(|id| ids.contains(id))
            })
            .map(|job| {
                job.target
                    .and_then(|i| self.tab.doc.models.get(i))
                    .map_or(job.display, |model| model.display)
            })
    }

    /// The current generation's own target, independent of later selection.
    pub(super) fn model_generation(&self) -> Option<crate::canvas::ModelGeneration<'_>> {
        let job = self.tab.model_job.as_ref()?;
        (job.current == self.model_key()).then_some(crate::canvas::ModelGeneration {
            atoms: job.prepared.ids(),
            started: job.started,
        })
    }

    pub(super) fn cancel_model_generation(&mut self) -> bool {
        if self.tab.model_job.take().is_none() {
            return false;
        }
        self.status = "3D model cancelled · Drawing unchanged".into();
        self.error = false;
        true
    }

    /// The selected molecule and the index of its model, if it has one.
    pub(super) fn selected_model(&self) -> Option<(Vec<u64>, Option<usize>)> {
        let atoms = models3d::selected_molecule(&self.tab.doc, &self.tab.selected)?;
        let index = self
            .tab
            .doc
            .models
            .iter()
            .position(|model| {
                let linked = model.linked_ids(&self.tab.doc);
                linked.len() == atoms.len() && atoms.iter().all(|id| linked.contains(id))
            })
            .or_else(|| {
                // A newly attached atom can select its one existing model without
                // becoming a source link until Update. Joined views need an
                // explicit selection of one model's links.
                let mut matching = self
                    .tab
                    .doc
                    .models
                    .iter()
                    .enumerate()
                    .filter(|(_, model)| atoms.iter().any(|id| model.contains(*id)));
                let (index, _) = matching.next()?;
                matching.next().is_none().then_some(index)
            });
        Some((atoms, index))
    }

    /// An existing model only. Unmodeled molecules still use `selected_model`
    /// for the Structure section's generation controls.
    pub(super) fn selected_existing_model(&self) -> Option<&models3d::Model3d> {
        if self.tab.doc.models.is_empty() {
            return None;
        }
        self.selected_model()
            .and_then(|(_, index)| self.tab.doc.models.get(index?))
    }

    fn idle(&self) -> bool {
        !self.tab.busy && self.tab.optimization.is_none() && !self.tab.drawing_edit_active()
    }

    /// A button of the Structure section: 2D follows the model's 2D layout.
    pub(super) fn show_as(&self, display: Display) -> Action {
        let model = self
            .selected_model()
            .and_then(|(_, index)| self.tab.doc.models.get(index?));
        if display == Display::TwoD && model.is_some_and(|m| m.layout == Layout::View) {
            Action::Flatten
        } else {
            Action::Show(display)
        }
    }

    /// A turn or a frame resize from the canvas.
    pub(super) fn model_edit(&mut self, edit: Edit) {
        let before = self.tab.doc.clone();
        match edit {
            Edit::ModelView { atoms, rotation } => {
                if let Some(id) = atoms.first() {
                    models3d::set_rotation(&mut self.tab.doc, *id, rotation);
                }
            }
            Edit::ResizeModels { ids, pivot, scale } => {
                models3d::resize(&mut self.tab.doc, &ids, pivot, scale);
                self.tab.selected = ids;
            }
            _ => return,
        }
        self.commit_model(before, false);
    }

    fn show_model(&mut self, display: Display) -> Task<Message> {
        if !self.idle() {
            return Task::none();
        }
        let Some((atoms, index)) = self.selected_model() else {
            self.status = "Select one molecule to show it in 3D".into();
            self.error = true;
            return Task::none();
        };
        if let Some(index) = index {
            let before = self.tab.doc.clone();
            let Some(model) = before.models.get(index) else {
                return Task::none();
            };
            let previous = model.display;
            // 2D is the drawn layout, brought back if it was flattened. The
            // current display again changes nothing, so a resized frame stays.
            let restore = display == Display::TwoD && model.drawn.is_some();
            if previous != display || restore {
                if display == Display::TwoD {
                    models3d::restore_drawing(&mut self.tab.doc, index);
                    if let Some(model) = self.tab.doc.models.get_mut(index) {
                        model.layout = Layout::Original;
                    }
                }
                models3d::set_display(&mut self.tab.doc, index, display);
                self.commit_model(before, false);
                if !self.error {
                    self.status = if restore {
                        "Original 2D drawing restored".into()
                    } else {
                        status(display, previous)
                    };
                    self.start_morph(&atoms, previous, display);
                    self.reveal_model(&atoms, display);
                }
            }
            return Task::none();
        }
        if display == Display::TwoD {
            if self.pending(&atoms).is_some() {
                self.cancel_model_generation();
            }
            return Task::none();
        }
        self.generate_model(atoms, display, None, ForceField::Mmff94s, DEFAULT_SEED)
    }

    fn regenerate_model(&mut self, fresh: bool) -> Task<Message> {
        if !self.idle() {
            return Task::none();
        }
        let Some((_, Some(index))) = self.selected_model() else {
            return Task::none();
        };
        let Some(model) = self.tab.doc.models.get(index) else {
            return Task::none();
        };
        if model.is_file_derived() || !fresh && !model.is_stale(&self.tab.doc) {
            return Task::none();
        }
        let force_field = match model.force_field {
            models3d::ForceField::Mmff94 => ForceField::Mmff94,
            models3d::ForceField::Mmff94s => ForceField::Mmff94s,
            models3d::ForceField::Uff => ForceField::Uff,
        };
        let seed = if model.seed == 0 {
            DEFAULT_SEED
        } else {
            model.seed
        };
        // ETKDG samples consecutive seeds; a wide stride avoids overlapping batches.
        let seed = if fresh {
            ((i64::from(seed) + 1_000_003) % i64::from(GENERATION_SEED_MAX)) as i32
        } else {
            seed
        };
        let atoms = model.chemistry_ids(&self.tab.doc);
        if atoms.first().is_some_and(|id| {
            reshiki::molecule_names::component(&self.tab.doc, *id).len() != atoms.len()
        }) || self.tab.doc.models.iter().any(|other| {
            other.linked_atoms.iter().any(|id| atoms.contains(id))
                && other.linked_atoms.iter().any(|id| !atoms.contains(id))
        }) {
            self.status =
                "3D model not generated: the source must be one connected structure".into();
            self.error = true;
            return Task::none();
        }
        self.generate_model(atoms, model.display, Some(index), force_field, seed)
    }

    fn generate_model(
        &mut self,
        atoms: Vec<u64>,
        display: Display,
        target: Option<usize>,
        force_field: ForceField,
        seed: i32,
    ) -> Task<Message> {
        if target.is_some()
            && self
                .tab
                .doc
                .models
                .iter()
                .any(|model| model.is_file_derived() && atoms.iter().any(|id| model.contains(*id)))
        {
            self.status =
                "3D model not generated: the structure includes file-derived geometry".into();
            self.error = true;
            return Task::none();
        }
        let prepared = match Prepared::new(&self.tab.doc, &atoms) {
            Ok(prepared) => Arc::new(prepared.without_retained_depth()),
            Err(error) => {
                self.status = format!("3D model not generated: {error}");
                self.error = true;
                return Task::none();
            }
        };
        self.tab.model_serial = self.tab.model_serial.wrapping_add(1);
        let key = self.model_key();
        let request = prepared.clone();
        let (task, handle) = Task::perform(
            async move {
                Client::default()
                    .generate_seeded(&request, force_field, seed)
                    .await
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            },
            move |result| Message::Models3d(Action::Generated(key, result)),
        )
        .abortable();
        self.tab.model_job = Some(Job {
            key,
            current: key,
            display,
            target,
            seed,
            force_field,
            prepared,
            started: iced::time::Instant::now(),
            _handle: handle.abort_on_drop(),
        });
        self.status = format!("Generating 3D model · {}", force_field.name());
        self.error = false;
        task
    }

    fn set_model(&mut self, setting: Setting) {
        if self.tab.busy || self.tab.drawing_edit_active() {
            return;
        }
        let Some((_, Some(index))) = self.selected_model() else {
            return;
        };
        if matches!(setting, Setting::Repeat(_) | Setting::LookAlong(_)) && self.model_stale(index)
        {
            self.status = "Update the model first".into();
            self.error = false;
            return;
        }
        let before = self.tab.doc.clone();
        // The opening view is a search for a crystal; only Reset view needs it.
        let reset = matches!(setting, Setting::ResetView)
            .then(|| before.models.get(index).map(|m| m.initial_view(&before)))
            .flatten();
        let Some(model) = self.tab.doc.models.get_mut(index) else {
            return;
        };
        match setting {
            Setting::Style(style) => model.style = style,
            Setting::Probability(probability) => model.probability = probability,
            Setting::MoleculeOnly(only) => model.molecule_only = only,
            Setting::IncludeSolvent(include) => model.include_solvent = include,
            Setting::Look(look) => model.look = look,
            Setting::Hydrogens(shown) => model.show_hydrogens = shown,
            Setting::Labels(labels) => model.labels = labels,
            Setting::Background(background) => model.background = background,
            Setting::DepthFade(value) => model.depth_fade = value.clamp(0., 1.),
            Setting::Perspective(value) => model.perspective = value.clamp(0., 1.),
            Setting::ResetView => model.rotation = reset.unwrap_or(model.rotation),
            Setting::Charges(shown) => model.charges = shown,
            Setting::Repeat(repeat) => {
                let reach = model.reach(&before);
                model.repeat = repeat;
                model.size *= model.reach(&before) / reach;
            }
            Setting::LookAlong(_) => {}
        }
        let display = model.display;
        // A first repeat lays out a visible 2D drawing along the axis whose
        // labels keep farthest apart. A 3D-only model keeps its view.
        let first_repeat = matches!(setting, Setting::Repeat(r) if r != [1; 3])
            && before.models.get(index).is_some_and(|m| m.repeat == [1; 3])
            && display != Display::ThreeD;
        let along = match setting {
            Setting::LookAlong(along) => Some(along),
            _ if first_repeat => self
                .tab
                .doc
                .models
                .get(index)
                .map(|m| models3d::best_along(&self.tab.doc, m)),
            _ => None,
        };
        if let Some(along) = along {
            if display == Display::ThreeD {
                if let Some(model) = self.tab.doc.models.get_mut(index)
                    && let Some(cell) = &model.cell
                {
                    model.rotation = along.rotation(cell);
                }
            } else {
                // Both shares the tilt that keeps the 2D labels apart.
                models3d::look_along(&mut self.tab.doc, index, along);
            }
        }
        if display == Display::Both && matches!(setting, Setting::Repeat(_) | Setting::LookAlong(_))
        {
            models3d::set_display(&mut self.tab.doc, index, Display::Both);
        }
        // The active choice again changes nothing and adds no Undo step.
        if self.tab.doc == before {
            return;
        }
        models3d::require_version(&mut self.tab.doc);
        // A drag continues only on the same slider while nothing else has
        // been committed.
        let sliding = matches!(setting, Setting::DepthFade(_) | Setting::Perspective(_))
            .then(|| (self.tab.revision, index, std::mem::discriminant(&setting)));
        let continuing = sliding.is_some() && self.tab.model_sliding == sliding;
        self.commit_model(before, continuing);
        self.tab.model_sliding =
            sliding.map(|(_, index, slider)| (self.tab.revision, index, slider));
    }

    /// Commit a change to the models' displays, views, frames or styles. A
    /// generation in flight for another molecule stays valid (unless already
    /// stale).
    fn commit_model(&mut self, before: Document, continuing: bool) {
        let current = self.model_key();
        if continuing {
            self.changed_continuing(before, true);
        } else {
            self.changed(before);
        }
        let next = self.model_key();
        if let Some(job) = &mut self.tab.model_job
            && job.current == current
        {
            job.current = next;
        }
    }

    /// Show the selected molecule as 2D, flattened from its model as turned
    /// now with far bonds lighter, as 3D optimize's projection does.
    fn flatten_model(&mut self) {
        if !self.idle() {
            return;
        }
        let Some((atoms, Some(index))) = self.selected_model() else {
            self.status = "Select a molecule with a 3D model to flatten it".into();
            self.error = true;
            return;
        };
        let Some(model) = self.tab.doc.models.get(index) else {
            return;
        };
        if self.model_stale(index) {
            self.status = "Update the model first".into();
            self.error = false;
            return;
        }
        let previous = model.display;
        let origin = models3d::frame_origin(&self.tab.doc, model);
        // The drawn layout stays on the model, for Original drawing.
        let drawn = model
            .drawn
            .clone()
            .unwrap_or_else(|| models3d::Drawn::of(&self.tab.doc, model));
        let flat = if model.cell.is_some() {
            // A crystal's distances are drawing data, not chemical stereo.
            // Flatten its retained coordinates directly, keeping the drawing
            // centre instead of preparing a molecular identity.
            let mut doc = self.tab.doc.clone();
            let centre = reshiki::editing::center(&doc, &atoms);
            models3d::lay_out_flat(&mut doc, model);
            let projected = reshiki::editing::center(&doc, &atoms);
            doc.translate(&atoms, centre.x - projected.x, centre.y - projected.y);
            Ok(doc)
        } else {
            Prepared::new(&self.tab.doc, &atoms)
                .and_then(|prepared| prepared.flattened(model))
                .map_err(|error| error.to_string())
        }
        .and_then(|mut doc| {
            // The model keeps its place for showing it in 3D again.
            if let Some(origin) = origin {
                models3d::keep_place(&mut doc, index, origin);
            }
            models3d::set_display(&mut doc, index, Display::TwoD);
            // Kept where the flattened drawing is, to come back wherever
            // the molecule goes.
            let drawn = doc.models.get(index).map(|m| drawn.kept(&doc, m));
            if let Some(model) = doc.models.get_mut(index) {
                model.layout = Layout::View;
                model.drawn = drawn;
            }
            reshiki::depth_appearance::enable(
                &mut doc,
                &atoms,
                reshiki::depth_appearance::DEFAULT_STRENGTH,
            )?;
            Ok(doc)
        });
        match flat {
            Ok(doc) => {
                let before = std::mem::replace(&mut self.tab.doc, doc);
                self.commit_model(before, false);
                if !self.error {
                    self.status = format!(
                        "Shown as 2D from the 3D view · Undo restores {}",
                        restored(previous)
                    );
                }
            }
            Err(error) => {
                self.status = format!("2D layout not flattened: {error}");
                self.error = true;
            }
        }
    }

    /// Start the 2D → 3D morph when a molecule's model first appears.
    fn start_morph(&mut self, atoms: &[u64], previous: Display, display: Display) {
        if previous == Display::TwoD
            && display != Display::TwoD
            && let Some(id) = atoms.first()
        {
            self.tab.model_morph = Some((self.model_key(), *id, iced::time::Instant::now()));
        }
    }

    /// The morph playing, until Undo, Redo or any other change to the drawing.
    pub(super) fn model_morph(&self) -> Option<(u64, iced::time::Instant)> {
        let (key, id, start) = self.tab.model_morph?;
        (key == self.model_key()).then_some((id, start))
    }

    fn add_model(&mut self, model: models3d::Model3d, display: Display) {
        let before = self.tab.doc.clone();
        let atoms = model.linked_ids(&before);
        let previous = before
            .models
            .iter()
            .find(|m| atoms.iter().any(|id| m.contains(*id)))
            .map_or(Display::TwoD, |m| m.display);
        self.tab.doc.models.push(model);
        models3d::require_version(&mut self.tab.doc);
        let index = self.tab.doc.models.len() - 1;
        models3d::set_display(&mut self.tab.doc, index, display);
        self.changed(before);
        if !self.error {
            self.status = status(display, previous);
            self.start_morph(&atoms, previous, display);
            self.reveal_model(&atoms, display);
        }
    }

    fn replace_model(&mut self, index: usize, mut model: models3d::Model3d) {
        let before = self.tab.doc.clone();
        let Some(old) = before.models.get(index) else {
            return;
        };
        let origin = models3d::frame_origin(&before, old);
        align_conformer(&mut model, old, &old.linked_ids(&before));
        model.display = old.display;
        model.rotation = old.rotation;
        model.offset = old.offset;
        model.size = old.size;
        model.style = old.style;
        model.look = old.look;
        model.show_hydrogens = old.show_hydrogens;
        model.labels = old.labels;
        model.depth_fade = old.depth_fade;
        model.perspective = old.perspective;
        model.background = old.background;
        model.layout = old.layout;
        let atoms = model.linked_ids(&before);
        let overlaps = |other: &models3d::Model3d| atoms.iter().any(|id| other.contains(*id));
        model.drawn = models3d::Drawn::combined(
            &before,
            &model,
            std::iter::once(old).chain(
                before
                    .models
                    .iter()
                    .enumerate()
                    .filter(|(i, other)| *i != index && overlaps(other))
                    .map(|(_, other)| other),
            ),
        );
        // Joining sources makes one molecule: replace all its models, keeping
        // the selected view's place among the unaffected models.
        let index = before
            .models
            .iter()
            .take(index)
            .filter(|other| !overlaps(other))
            .count();
        self.tab.doc.models.retain(|other| !overlaps(other));
        self.tab.doc.models.insert(index, model);
        if old.linked_ids(&before) != atoms
            && let Some(origin) = origin
            && let Some(now) = self
                .tab
                .doc
                .models
                .get(index)
                .and_then(|model| models3d::frame_origin(&self.tab.doc, model))
            && let Some(model) = self.tab.doc.models.get_mut(index)
        {
            model.offset = model.offset.offset(origin.x - now.x, origin.y - now.y);
        }
        self.changed(before);
        if !self.error {
            self.status = "3D conformer regenerated · Undo restores the previous model".into();
        }
    }

    fn stale_model(&self) -> Option<&models3d::Model3d> {
        let (_, index) = self.selected_model()?;
        let index = index?;
        self.model_stale(index)
            .then(|| self.tab.doc.models.get(index))
            .flatten()
    }

    fn model_stale(&self, index: usize) -> bool {
        let key = (self.tab.id, self.tab.file_epoch, self.tab.revision);
        let mut cache = self.tab.model_stale.borrow_mut();
        if cache.key != Some(key) || cache.values.len() != self.tab.doc.models.len() {
            cache.key = Some(key);
            cache.values = vec![None; self.tab.doc.models.len()];
        }
        let Some(value) = cache.values.get(index) else {
            return false;
        };
        if let Some(value) = value {
            return *value;
        }
        let stale = self
            .tab
            .doc
            .models
            .get(index)
            .is_some_and(|model| model.is_stale(&self.tab.doc));
        if let Some(value) = cache.values.get_mut(index) {
            *value = Some(stale);
        }
        #[cfg(test)]
        {
            cache.comparisons += 1;
        }
        stale
    }

    fn update_model_button(&self, id: &'static str) -> Element<'_, Message> {
        accessible_command(
            id,
            "Update the model from the current structure",
            "Update model",
        )
        .on_press_maybe(
            (self.idle() && self.tab.model_job.is_none())
                .then_some(Message::Models3d(Action::Update)),
        )
        .into()
    }

    /// The old Show-as chooser is gone; only a stale model needs a floating action.
    pub(super) fn with_model_notice<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        use iced::widget::{container, pin, stack};
        let notice: Option<Element<'_, Message>> = self.stale_model().and_then(|model| {
            self.pending(&model.linked_ids(&self.tab.doc))
                .is_none()
                .then(|| {
                    container(
                        row![
                            text("Structure changed ·").size(12),
                            self.update_model_button("model-floating-update")
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([4, 8])
                    .style(super::workspace::panel)
                    .into()
                })
        });
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let at = self
            .stale_model()
            .and_then(|model| {
                reshiki::scene::selection_bounds(&self.tab.doc, &model.linked_ids(&self.tab.doc))
            })
            .map(|(lo, hi)| {
                self.tab.camera.screen(
                    reshiki::document::Point::new((lo.x + hi.x) / 2., lo.y),
                    paper,
                )
            })
            .unwrap_or_default();
        let x = (paper.x + at.x - 135.).clamp(
            paper.x + 6.,
            (paper.x + paper.width - 276.).max(paper.x + 6.),
        );
        let y = (paper.y + at.y - 48.).clamp(
            paper.y + 6.,
            (paper.y + paper.height - 40.).max(paper.y + 6.),
        );
        stack![base, pin(column(notice)).x(x).y(y)].into()
    }

    fn reveal_model(&mut self, atoms: &[u64], display: Display) {
        if display == Display::TwoD {
            return;
        }
        let ids = reshiki::molecule_names::include_annotations(&self.tab.doc, atoms);
        if let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.tab.doc, &ids) {
            // Include the visible selected-model outline without moving an
            // already visible result or zooming in on a smaller molecule.
            let margin = 12. / self.tab.camera.zoom;
            self.reveal_if_clipped(lo.offset(-margin, -margin), hi.offset(margin, margin));
        }
    }

    /// Show as 2D / 3D / Both for one selected molecule.
    pub(super) fn structure_section(&self) -> Option<Element<'_, Message>> {
        let (atoms, index) = self.selected_model()?;
        let current = index
            .and_then(|i| self.tab.doc.models.get(i))
            .map_or(Display::TwoD, |m| m.display);
        let pending = self.pending(&atoms);
        let model = index.and_then(|i| self.tab.doc.models.get(i));
        let stale = index.is_some_and(|i| self.model_stale(i));
        let enabled = !self.tab.busy && self.tab.optimization.is_none();
        let mut body = column![
            row([
                (Display::TwoD, "2D", "Show the 2D drawing"),
                (Display::ThreeD, "3D", "Show the 3D model in place"),
                (
                    Display::Both,
                    "Both",
                    "Show the 2D drawing and the 3D model"
                ),
            ]
            .map(|(display, label, name)| {
                let active = pending.unwrap_or(current) == display;
                accessible_command(format!("structure-show-{label}"), name, label)
                    .checked(active)
                    .style(control(active))
                    .width(Length::Fill)
                    .on_press_maybe(
                        (enabled
                            && !(stale
                                && display == Display::TwoD
                                && model.is_some_and(|m| m.layout == Layout::View)))
                        .then(|| {
                            Message::Models3d(if active {
                                Action::Unchanged
                            } else {
                                self.show_as(display)
                            })
                        }),
                    )
                    .into()
            }),)
            .spacing(4),
            text(
                if let Some(job) = self.tab.model_job.as_ref().filter(|_| pending.is_some()) {
                    format!(
                        "Generating the conformer · ETKDG, then {}",
                        job.force_field.name()
                    )
                } else {
                    subtitle(pending, model)
                }
            )
            .size(11)
            .style(muted_text),
        ]
        .spacing(6);
        if self.stale_model().is_some() {
            body = body.push(
                row![
                    text("Structure changed ·").size(11).style(muted_text),
                    self.update_model_button("structure-update-model"),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
        // The 2D layout the drawing returns to, while the model is shown.
        if let Some(model) = model.filter(|m| m.display != Display::TwoD && pending.is_none()) {
            let layout = model.layout;
            body = body.push(text("2D layout").size(12)).push(
                row([
                    (Layout::Original, "Original drawing", "original"),
                    (Layout::View, "From the 3D view", "view"),
                ]
                .map(|(choice, label, id)| {
                    let active = layout == choice;
                    accessible_command(format!("structure-layout-{id}"), label, label)
                        .checked(active)
                        .style(control(active))
                        .width(Length::Fill)
                        .on_press_maybe(
                            (enabled && (choice != Layout::View || !stale))
                                .then_some(Message::Models3d(Action::Layout(choice))),
                        )
                        .into()
                }))
                .spacing(4),
            );
            body = body.push(
                text(if stale {
                    "Update the model first"
                } else {
                    match layout {
                        Layout::Original => "The layout you drew, unchanged.",
                        Layout::View => {
                            "Flatten the model as it is turned now, with far bonds lighter."
                        }
                    }
                })
                .size(11)
                .style(muted_text),
            );
        }
        // A crystal's ion charges, hidden in 2D unless asked for.
        let charged = |m: &&models3d::Model3d| {
            m.cell.is_some()
                && m.atoms
                    .iter()
                    .any(|a| self.tab.doc.atom(a.id).is_some_and(|a| a.charge != 0))
        };
        if let Some(model) = model.filter(charged) {
            body = body.push(text("Ion charges").size(12)).push(
                row(
                    [(false, "Hide", "hide"), (true, "Show", "show")].map(|(shown, label, id)| {
                        let active = model.charges == shown;
                        accessible_command(
                            format!("structure-charges-{id}"),
                            format!("{label} ion charges"),
                            label,
                        )
                        .checked(active)
                        .style(control(active))
                        .width(Length::Fill)
                        .on_press_maybe(
                            enabled
                                .then_some(Message::Models3d(Action::Set(Setting::Charges(shown)))),
                        )
                        .into()
                    }),
                )
                .spacing(4),
            );
        }
        if let Some(model) = model.filter(|m| m.cell.is_some()) {
            body = body.push(crystal_controls("structure", model, enabled && !stale));
        }
        Some(self.inspector_section(Section::Structure, "Structure", "", true, body))
    }
}

impl App {
    /// Style, look and view of the selected molecule's model while it is shown.
    pub(super) fn model_section(&self) -> Option<Element<'_, Message>> {
        let model = self.selected_existing_model()?;
        if model.display == Display::TwoD {
            return None;
        }
        let enabled = !self.tab.busy && self.tab.optimization.is_none();
        let stale = self.stale_model().is_some();
        let set = move |setting| enabled.then_some(Message::Models3d(Action::Set(setting)));
        let choice = |id: String, name: &str, label: &'static str, active: bool, setting| {
            accessible_command(format!("model-{id}"), name, label)
                .checked(active)
                .style(control(active))
                .width(Length::Fill)
                .on_press_maybe(set(setting))
        };
        let field = |label: &'static str, content: Element<'static, Message>| {
            column![text(label).size(12), content].spacing(4)
        };
        let style_button = |style| {
            let active = model.style == style;
            reshiki::accessibility::button(
                format!("model-style-{}", slug(style)),
                format!("{} style", name(style)),
                row![
                    canvas(Glyph(Icon::ModelStyle(style), true))
                        .width(24)
                        .height(24),
                    text(name(style)).size(12)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .checked(active)
            .style(control(active))
            .padding([3, 6])
            .width(Length::Fill)
            .on_press_maybe(set(Setting::Style(style)))
        };
        let [ball, stick, wire, space] = [
            Style::BallStick,
            Style::Stick,
            Style::Wireframe,
            Style::SpaceFilling,
        ]
        .map(style_button);
        let mut styles =
            column![row![ball, stick].spacing(4), row![wire, space].spacing(4)].spacing(4);
        if model.sites.iter().any(|site| site.adp.is_some()) {
            styles = styles.push(style_button(Style::Ellipsoids));
        }
        let segments = |options: Vec<(String, &'static str, &'static str, bool, Setting)>| {
            row(options
                .into_iter()
                .map(|(id, name, label, active, setting)| {
                    choice(id, name, label, active, setting).into()
                }))
            .spacing(4)
            .into()
        };
        let slider_field = |label: &'static str, value: f32, setting: fn(f32) -> Setting| {
            column![
                row![
                    text(label).size(12),
                    Space::new().width(Length::Fill),
                    text(format!("{:.0}%", value * 100.))
                        .size(12)
                        .style(muted_text)
                ],
                slider(0_f32..=100., value * 100., move |v| {
                    Message::Models3d(Action::Set(setting(v / 100.)))
                })
                .step(1_f32)
                .on_release(Message::Models3d(Action::SlideEnd)),
            ]
            .spacing(4)
        };
        let look = |look: Look, id: &str, name, label| {
            (
                format!("look-{id}"),
                name,
                label,
                model.look == look,
                Setting::Look(look),
            )
        };
        let hydrogens = |shown: Hydrogens, label, name| {
            (
                format!("hydrogens-{label}"),
                name,
                label,
                model.show_hydrogens == shown,
                Setting::Hydrogens(shown),
            )
        };
        let background = |background: Background, id: &str, name, label| {
            (
                format!("background-{id}"),
                name,
                label,
                model.background == background,
                Setting::Background(background),
            )
        };
        let toggle = |id: &str, name: &str, active: bool, setting: fn(bool) -> Setting| {
            row(
                [(false, "Off", "off"), (true, "On", "on")].map(|(enabled, label, suffix)| {
                    choice(
                        format!("{id}-{suffix}"),
                        &format!("{name} {label}"),
                        label,
                        active == enabled,
                        setting(enabled),
                    )
                    .into()
                }),
            )
            .spacing(4)
            .into()
        };
        let mut body = column![field("Style", styles.into())].spacing(10);
        if model.style == Style::Ellipsoids {
            body = body.push(field(
                "Probability",
                row(Probability::ALL.map(|probability| {
                    let percent = probability.percent();
                    let active = model.probability == probability;
                    accessible_command(
                        format!("model-probability-{percent}"),
                        format!("{percent}% ellipsoid probability"),
                        format!("{percent}%"),
                    )
                    .checked(active)
                    .style(control(active))
                    .width(Length::Fill)
                    .on_press_maybe(set(Setting::Probability(probability)))
                    .into()
                }))
                .spacing(4)
                .into(),
            ));
        }
        if model.cell.is_some() {
            body = body.push(crystal_controls("model", model, enabled && !stale));
            body = body.push(field(
                "Molecule only",
                toggle(
                    "molecule-only",
                    "Molecule only",
                    model.molecule_only,
                    Setting::MoleculeOnly,
                ),
            ));
            if model.molecule_only {
                body = body.push(field(
                    "Include solvent",
                    toggle(
                        "include-solvent",
                        "Include solvent",
                        model.include_solvent,
                        Setting::IncludeSolvent,
                    ),
                ));
            }
        }
        body = body.extend([
            field(
                "Look",
                segments(vec![
                    look(
                        Look::Flat,
                        "flat",
                        "Flat look, for journals",
                        "Flat · journal",
                    ),
                    look(
                        Look::Glossy,
                        "glossy",
                        "Glossy look, for slides",
                        "Glossy · slides",
                    ),
                ]),
            )
            .into(),
            field(
                "Hydrogens",
                segments(vec![
                    hydrogens(Hydrogens::All, "All", "Show all hydrogens"),
                    hydrogens(Hydrogens::Polar, "Polar", "Show hydrogens on N, O and S"),
                    hydrogens(Hydrogens::None, "None", "Hide hydrogens"),
                ]),
            )
            .into(),
            field(
                "Atom labels",
                row([
                    (Labels::None, "none", "None", "Hide atom labels"),
                    (
                        Labels::Heteroatoms,
                        "heteroatoms",
                        "Heteroatoms",
                        "Label atoms except C and H",
                    ),
                    (Labels::All, "all", "All", "Label all visible atoms"),
                ]
                .map(|(labels, id, label, name)| {
                    choice(
                        format!("labels-{id}"),
                        name,
                        label,
                        model.labels == labels,
                        Setting::Labels(labels),
                    )
                    .width(Length::FillPortion(if labels == Labels::Heteroatoms {
                        2
                    } else {
                        1
                    }))
                    .into()
                }))
                .spacing(4)
                .into(),
            )
            .into(),
            slider_field("Depth fade", model.depth_fade, Setting::DepthFade).into(),
            slider_field("Perspective", model.perspective, Setting::Perspective).into(),
            field(
                "Background",
                segments(vec![
                    background(Background::None, "none", "No background", "None"),
                    background(
                        Background::Card,
                        "card",
                        "Soft card background",
                        "Soft card",
                    ),
                ]),
            )
            .into(),
            accessible_command("model-reset-view", "Reset the 3D view", "Reset view")
                .on_press_maybe(set(Setting::ResetView))
                .into(),
        ]);
        if stale {
            body = column![
                row![
                    text("Structure changed ·").size(11).style(muted_text),
                    self.update_model_button("model-update")
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                body,
            ]
            .spacing(10);
        }
        if !model.is_file_derived() {
            body = body.push(
                accessible_command(
                    "model-regenerate",
                    "Generate a fresh conformer, keeping the view",
                    "Regenerate conformer",
                )
                .on_press_maybe(
                    (self.idle() && self.tab.model_job.is_none())
                        .then_some(Message::Models3d(Action::Regenerate)),
                ),
            );
        }
        Some(self.inspector_section(Section::Model3d, "3D model", name(model.style), true, body))
    }
}

/// The same crystal repeat and direction choices in Structure and 3D model.
fn crystal_controls(
    prefix: &'static str,
    model: &models3d::Model3d,
    enabled: bool,
) -> Element<'static, Message> {
    let set = move |setting| enabled.then_some(Message::Models3d(Action::Set(setting)));
    let mut body = column![text("Repeat").size(12)].spacing(6);
    for (k, axis) in ["a", "b", "c"].into_iter().enumerate() {
        let counts = (1..=models3d::MAX_REPEAT).map(|n| {
            let active = model.repeat.get(k) == Some(&n);
            let mut repeat = model.repeat;
            if let Some(slot) = repeat.get_mut(k) {
                *slot = n;
            }
            accessible_command(
                format!("{prefix}-repeat-{axis}-{n}"),
                format!("{n} cells along {axis}"),
                n.to_string(),
            )
            .checked(active)
            .style(control(active))
            .width(Length::Fill)
            .on_press_maybe(set(Setting::Repeat(repeat)))
            .into()
        });
        body = body.push(
            row![text(axis).size(12).width(16)]
                .extend(counts)
                .spacing(4)
                .align_y(Alignment::Center),
        );
    }
    body.push(text("Look along").size(12))
        .push(
            row(models3d::Along::ALL.map(|along| {
                let name = along.name();
                accessible_command(
                    format!("{prefix}-look-{name}"),
                    format!("Look along {name}"),
                    name,
                )
                .width(Length::Fill)
                .on_press_maybe(set(Setting::LookAlong(along)))
                .into()
            }))
            .spacing(4),
        )
        .into()
}

/// Least-squares proper rotation from the new conformer onto the old one.
/// The 3×3 covariance SVD uses a Jacobi eigensolve of HᵀH; two independent
/// axes determine a proper rotation, while a line gets its shortest turn.
fn align_conformer(model: &mut models3d::Model3d, old: &models3d::Model3d, live: &[u64]) {
    type V = [f64; 3];
    let at = |v: V, i| v.get(i).copied().unwrap_or_default();
    let entry = |matrix: [V; 3], i, j| matrix.get(i).map_or(0., |row| at(*row, j));
    let dot = |a: V, b: V| a.into_iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let sub = |a: V, b: V| std::array::from_fn(|i| at(a, i) - at(b, i));
    let cross = |a: V, b: V| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let unit = |a: V| {
        let n = dot(a, a).sqrt();
        a.map(|x| x / n)
    };
    let old_sources = old.geometry_sources();
    let pairs: Vec<_> = model
        .geometry_sources()
        .into_iter()
        .zip(&model.atoms)
        .filter_map(|(id, atom)| {
            let id = id.filter(|id| live.contains(id))?;
            let previous = old_sources.iter().position(|source| *source == Some(id))?;
            Some((
                atom.xyz.map(f64::from),
                old.atoms.get(previous)?.xyz.map(f64::from),
            ))
        })
        .collect();
    if pairs.is_empty() {
        return;
    }
    let mean = |side: bool| -> V {
        std::array::from_fn(|i| {
            pairs
                .iter()
                .map(|(a, b)| at(if side { *b } else { *a }, i))
                .sum::<f64>()
                / pairs.len() as f64
        })
    };
    let (from, to) = (mean(false), mean(true));
    let mut h = [[0.; 3]; 3];
    for (a, b) in &pairs {
        let (a, b) = (sub(*a, from), sub(*b, to));
        for (row, a) in h.iter_mut().zip(a) {
            for (value, b) in row.iter_mut().zip(b) {
                *value += a * b;
            }
        }
    }
    let mut eigen: [V; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| h.iter().map(|row| at(*row, i) * at(*row, j)).sum())
    });
    let mut axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    for _ in 0..32 {
        let Some((p, q)) = [(0, 1), (0, 2), (1, 2)]
            .into_iter()
            .max_by(|(p, q), (r, s)| {
                entry(eigen, *p, *q)
                    .abs()
                    .total_cmp(&entry(eigen, *r, *s).abs())
            })
        else {
            break;
        };
        if entry(eigen, p, q).abs() <= 1e-14 * (eigen[0][0] + eigen[1][1] + eigen[2][2]).max(1e-30)
        {
            break;
        }
        let angle = 0.5 * (2. * entry(eigen, p, q)).atan2(entry(eigen, q, q) - entry(eigen, p, p));
        let (s, c) = angle.sin_cos();
        let turn = |row: &mut V| {
            let (a, b) = (at(*row, p), at(*row, q));
            if let Some(value) = row.get_mut(p) {
                *value = c * a - s * b;
            }
            if let Some(value) = row.get_mut(q) {
                *value = s * a + c * b;
            }
        };
        for row in &mut eigen {
            turn(row);
        }
        let Some((a, b)) = eigen.get(p).copied().zip(eigen.get(q).copied()) else {
            break;
        };
        if let Some(row) = eigen.get_mut(p) {
            *row = std::array::from_fn(|j| c * at(a, j) - s * at(b, j));
        }
        if let Some(row) = eigen.get_mut(q) {
            *row = std::array::from_fn(|j| s * at(a, j) + c * at(b, j));
        }
        for row in &mut axes {
            turn(row);
        }
    }
    let mut order = [0, 1, 2];
    order.sort_by(|i, j| entry(eigen, *j, *j).total_cmp(&entry(eigen, *i, *i)));
    let mut rotation = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    if entry(eigen, order[0], order[0]) > 1e-20 {
        let v0 = axes.map(|row| at(row, order[0]));
        let u0 = unit(h.map(|row| dot(row, v0)));
        if entry(eigen, order[1], order[1]) > 1e-12 * entry(eigen, order[0], order[0]) {
            let v1 = axes.map(|row| at(row, order[1]));
            let u1 = h.map(|row| dot(row, v1));
            let u1 = unit(sub(u1, u0.map(|x| x * dot(u0, u1))));
            let (u2, v2) = (cross(u0, u1), cross(v0, v1));
            rotation = std::array::from_fn(|i| {
                std::array::from_fn(|j| {
                    at(v0, i) * at(u0, j) + at(v1, i) * at(u1, j) + at(v2, i) * at(u2, j)
                })
            });
        } else {
            // One point supplies translation only. A line has no unique roll;
            // choose the shortest turn, with a stable axis for a half-turn.
            let cosine = dot(u0, v0).clamp(-1., 1.);
            let axis = cross(u0, v0);
            if cosine < -1. + 1e-10 {
                let i = (0..3)
                    .min_by(|i, j| at(u0, *i).abs().total_cmp(&at(u0, *j).abs()))
                    .unwrap_or_default();
                let axis = unit(cross(
                    u0,
                    std::array::from_fn(|j| if i == j { 1. } else { 0. }),
                ));
                rotation = std::array::from_fn(|i| {
                    std::array::from_fn(|j| {
                        2. * at(axis, i) * at(axis, j) - if i == j { 1. } else { 0. }
                    })
                });
            } else {
                let skew = [
                    [0., -axis[2], axis[1]],
                    [axis[2], 0., -axis[0]],
                    [-axis[1], axis[0], 0.],
                ];
                rotation = std::array::from_fn(|i| {
                    std::array::from_fn(|j| {
                        (if i == j { 1. } else { 0. })
                            + entry(skew, i, j)
                            + (0..3)
                                .map(|k| entry(skew, i, k) * entry(skew, k, j))
                                .sum::<f64>()
                                / (1. + cosine)
                    })
                });
            }
        }
    }
    let place = |xyz: [f32; 3]| {
        let xyz = sub(xyz.map(f64::from), from);
        std::array::from_fn(|i| {
            (rotation.get(i).map_or(0., |row| dot(*row, xyz)) + at(to, i)) as f32
        })
    };
    for atom in &mut model.atoms {
        atom.xyz = place(atom.xyz);
    }
    for hydrogen in &mut model.hydrogens {
        hydrogen.xyz = place(hydrogen.xyz);
    }
}

/// The Structure section's note: the generation in flight, the 2D drawing
/// while a model is kept but not shown, or the model's style (ball & stick
/// before there is a model).
fn subtitle(pending: Option<Display>, model: Option<&models3d::Model3d>) -> String {
    if pending.is_some() {
        return "Generating the conformer · ETKDG, then MMFF94s".into();
    }
    if model.is_some_and(|m| m.display == Display::TwoD) {
        return "2D drawing · Chemistry and atom IDs stay the same".into();
    }
    let style = model.map_or(Style::BallStick, |m| m.style);
    format!("{} · Chemistry and atom IDs stay the same", name(style))
}

fn name(style: Style) -> &'static str {
    match style {
        Style::BallStick => "Ball & stick",
        Style::Stick => "Stick",
        Style::Wireframe => "Wireframe",
        Style::SpaceFilling => "Space-filling",
        Style::Ellipsoids => "Ellipsoids",
    }
}

fn slug(style: Style) -> &'static str {
    match style {
        Style::BallStick => "ball-stick",
        Style::Stick => "stick",
        Style::Wireframe => "wireframe",
        Style::SpaceFilling => "space-filling",
        Style::Ellipsoids => "ellipsoids",
    }
}

#[cfg(test)]
impl App {
    /// Answer the generation in flight with this conformer: drawing atoms in
    /// the prepared order, then force-field hydrogens bonded to `parents`.
    pub(super) fn finish_model(
        &mut self,
        positions: Vec<reshiki::geometry::Point3>,
        parents: Vec<usize>,
    ) -> Task<Message> {
        let Some(job) = &self.tab.model_job else {
            return Task::none();
        };
        let optimized = Optimized {
            conformer: reshiki::geometry::Conformer {
                positions,
                original_atom_count: job.prepared.ids().len(),
                hydrogen_parents: parents,
            },
            initial_energy: 1.,
            energy: 1.,
            gradient: None,
            converged: true,
            iterations: 1,
            force_field: job.force_field,
            diagnostics: vec![],
        };
        let key = job.key;
        self.update(Message::Models3d(Action::Generated(
            key,
            Ok(Arc::new(optimized)),
        )))
    }

    /// The atoms of the generation in flight, in conformer order.
    pub(super) fn model_atoms(&self) -> Vec<u64> {
        self.tab
            .model_job
            .as_ref()
            .map(|job| job.prepared.ids().to_vec())
            .unwrap_or_default()
    }
}

/// The new view and the one Undo brings back.
fn status(display: Display, previous: Display) -> String {
    let shown = match display {
        Display::TwoD => "2D",
        Display::ThreeD => "3D model",
        Display::Both => "2D and 3D",
    };
    format!("Shown as {shown} · Undo restores {}", restored(previous))
}

fn restored(previous: Display) -> &'static str {
    match previous {
        Display::TwoD => "the 2D drawing",
        Display::ThreeD => "the 3D model",
        Display::Both => "2D and 3D",
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod crystal_tests;

#[cfg(test)]
mod ortep_tests;
