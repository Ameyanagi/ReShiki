//! Show a selected molecule as its 2D drawing, a 3D model or both (#386).
//!
//! A molecule without a model gets one from the geometry worker: one abortable
//! job per tab, whose result is dropped when the drawing changed meanwhile.
//! Each switch, including the one that adds the model, is one Undo step, and
//! so is each turn or resize of a model on the canvas.
use super::workspace::{accessible_command, control, muted_text};
use super::{App, Message, document_tab::TabId, inspector::Section};
use crate::canvas::Edit;
use iced::widget::{column, row, text};
use iced::{Element, Length, Task};
use reshiki::document::Document;
use reshiki::geometry::{Client, ForceField, Optimized, Prepared};
use reshiki::models3d::{self, Display};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    tab: TabId,
    epoch: u64,
    revision: u64,
    serial: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Show(Display),
    /// Show as 2D, the drawing flattened from the model as it is turned now.
    Flatten,
    /// The 2D layout that the 2D buttons of the bar and the section use.
    Layout(Layout),
    Generated(Key, Result<Arc<Optimized>, String>),
}

/// The 2D layout a molecule shown in 3D returns to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Layout {
    /// The layout as drawn, unchanged.
    #[default]
    Original,
    /// The model flattened as it is turned now, far bonds lighter.
    View,
}

/// The generation in flight; dropping it aborts the worker.
pub(crate) struct Job {
    /// The key the worker answers with; it never changes.
    key: Key,
    /// The drawing state the result still applies to: `key` until switching
    /// another molecule's display moves it on.
    current: Key,
    display: Display,
    prepared: Arc<Prepared>,
    _handle: iced::task::Handle,
}

impl App {
    pub(super) fn models3d_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(display) => self.show_model(display),
            Action::Flatten => {
                self.flatten_model();
                Task::none()
            }
            Action::Layout(layout) => {
                self.tab.model_layout = layout;
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
                    Ok(model) => self.add_model(model, job.display),
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
    fn pending(&self, atoms: &[u64]) -> Option<Display> {
        self.tab
            .model_job
            .as_ref()
            .filter(|job| {
                let ids = job.prepared.ids();
                job.current == self.model_key()
                    && ids.len() == atoms.len()
                    && atoms.iter().all(|id| ids.contains(id))
            })
            .map(|job| job.display)
    }

    /// The selected molecule and the index of its model, if it has one.
    pub(super) fn selected_model(&self) -> Option<(Vec<u64>, Option<usize>)> {
        let atoms = models3d::selected_molecule(&self.tab.doc, &self.tab.selected)?;
        let index =
            self.tab.doc.models.iter().position(|m| {
                m.atoms.len() == atoms.len() && atoms.iter().all(|id| m.contains(*id))
            });
        Some((atoms, index))
    }

    fn idle(&self) -> bool {
        !self.tab.busy && self.tab.optimization.is_none() && !self.tab.drawing_edit_active()
    }

    /// A button of the bar or the section: 2D follows the chosen 2D layout.
    pub(super) fn show_as(&self, display: Display) -> Action {
        let modelled = self
            .selected_model()
            .is_some_and(|(_, index)| index.is_some());
        if display == Display::TwoD && modelled && self.tab.model_layout == Layout::View {
            Action::Flatten
        } else {
            Action::Show(display)
        }
    }

    /// Commit a change to the models' displays, views or frames. A generation
    /// in flight for another molecule stays valid (unless already stale).
    fn commit_view(&mut self, before: Document) {
        let current = self.model_key();
        self.changed(before);
        let next = self.model_key();
        if let Some(job) = &mut self.tab.model_job
            && job.current == current
        {
            job.current = next;
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
        self.commit_view(before);
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
            if models3d::set_display(&mut self.tab.doc, index, display) {
                let previous = before
                    .models
                    .get(index)
                    .map_or(Display::TwoD, |m| m.display);
                self.commit_view(before);
                if !self.error {
                    self.status = status(display, previous);
                    self.start_morph(&atoms, previous, display);
                }
            }
            return Task::none();
        }
        if display == Display::TwoD {
            if self.pending(&atoms).is_some() {
                self.tab.model_job = None;
                self.status = "3D model cancelled · Drawing unchanged".into();
                self.error = false;
            }
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
                    .generate(&request, ForceField::Mmff94s)
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
            prepared,
            _handle: handle.abort_on_drop(),
        });
        self.status = "Generating 3D model · MMFF94s".into();
        self.error = false;
        task
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
        let previous = model.display;
        let flat = Prepared::new(&self.tab.doc, &atoms)
            .and_then(|prepared| prepared.flattened(model))
            .map_err(|error| error.to_string())
            .and_then(|mut doc| {
                models3d::set_display(&mut doc, index, Display::TwoD);
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
                self.commit_view(before);
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
            self.tab.model_morph = Some((*id, std::time::Instant::now()));
        }
    }

    /// The display the floating Show as bar marks above a whole selected
    /// molecule, when the bar shows.
    pub(super) fn model_bar(&self) -> Option<Display> {
        if !self.tool.selects() || !self.idle() || self.context_menu.is_some() {
            return None;
        }
        let (atoms, index) = self.selected_model()?;
        let selected = self
            .tab
            .doc
            .expand_abbreviation_selection(&self.tab.selected);
        if !atoms.iter().all(|id| selected.contains(id)) {
            return None;
        }
        Some(self.pending(&atoms).unwrap_or_else(|| {
            index
                .and_then(|i| self.tab.doc.models.get(i))
                .map_or(Display::TwoD, |m| m.display)
        }))
    }

    fn add_model(&mut self, model: models3d::Model3d, display: Display) {
        let before = self.tab.doc.clone();
        let atoms: Vec<_> = model.atoms.iter().map(|a| a.id).collect();
        let previous = before
            .models
            .iter()
            .find(|m| m.atoms.iter().any(|a| atoms.contains(&a.id)))
            .map_or(Display::TwoD, |m| m.display);
        self.tab
            .doc
            .models
            .retain(|m| !m.atoms.iter().any(|a| atoms.contains(&a.id)));
        self.tab.doc.models.push(model);
        self.tab.doc.require_version(reshiki::document::MODELS_3D);
        let index = self.tab.doc.models.len() - 1;
        models3d::set_display(&mut self.tab.doc, index, display);
        self.changed(before);
        if !self.error {
            self.status = status(display, previous);
            self.start_morph(&atoms, previous, display);
        }
    }

    /// Show as 2D / 3D / Both for one selected molecule.
    pub(super) fn structure_section(&self) -> Option<Element<'_, Message>> {
        let (atoms, index) = self.selected_model()?;
        let current = index
            .and_then(|i| self.tab.doc.models.get(i))
            .map_or(Display::TwoD, |m| m.display);
        let pending = self.pending(&atoms);
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
                    .on_press_maybe(enabled.then(|| Message::Models3d(self.show_as(display))))
                    .into()
            }),)
            .spacing(4),
            text(if pending.is_some() {
                "Generating the conformer · ETKDG, then MMFF94s"
            } else {
                "Ball & stick · Chemistry and atom IDs stay the same"
            })
            .size(11)
            .style(muted_text),
        ]
        .spacing(6);
        // The 2D layout the drawing returns to, while the model is shown.
        if current != Display::TwoD && pending.is_none() {
            let layout = self.tab.model_layout;
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
                        .on_press(Message::Models3d(Action::Layout(choice)))
                        .into()
                }))
                .spacing(4),
            );
            body = body.push(
                text(match layout {
                    Layout::Original => "The layout you drew, unchanged.",
                    Layout::View => {
                        "Flatten the model as it is turned now, with far bonds lighter."
                    }
                })
                .size(11)
                .style(muted_text),
            );
        }
        Some(self.inspector_section(Section::Structure, "Structure", "", true, body))
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
            force_field: ForceField::Mmff94s,
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
