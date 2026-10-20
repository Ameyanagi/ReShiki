//! Show a selected molecule as its 2D drawing, a 3D model or both (#386).
//!
//! A molecule without a model gets one from the geometry worker: one abortable
//! job per tab, whose result is dropped when the drawing changed meanwhile.
//! Each switch, including the one that adds the model, is one Undo step.
use super::workspace::{accessible_command, control, muted_text};
use super::{App, Message, document_tab::TabId, inspector::Section};
use iced::widget::{column, row, text};
use iced::{Element, Length, Task};
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
    Generated(Key, Result<Arc<Optimized>, String>),
}

/// The generation in flight; dropping it aborts the worker.
pub(crate) struct Job {
    key: Key,
    display: Display,
    prepared: Arc<Prepared>,
    _handle: iced::task::Handle,
}

impl App {
    pub(super) fn models3d_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(display) => self.show_model(display),
            Action::Generated(key, result) => {
                let current = Key {
                    serial: key.serial,
                    ..self.model_key()
                };
                let Some(job) = self.tab.model_job.take_if(|job| job.key == key) else {
                    return Task::none();
                };
                if key != current {
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

    /// The selected molecule and the index of its model, if it has one.
    pub(super) fn selected_model(&self) -> Option<(Vec<u64>, Option<usize>)> {
        let atoms = models3d::selected_molecule(&self.tab.doc, &self.tab.selected)?;
        let index =
            self.tab.doc.models.iter().position(|m| {
                m.atoms.len() == atoms.len() && atoms.iter().all(|id| m.contains(*id))
            });
        Some((atoms, index))
    }

    fn show_model(&mut self, display: Display) -> Task<Message> {
        if self.tab.busy || self.tab.optimization.is_some() || self.tab.drawing_edit_active() {
            return Task::none();
        }
        let Some((atoms, index)) = self.selected_model() else {
            self.status = "Select one molecule to show it in 3D".into();
            self.error = true;
            return Task::none();
        };
        if let Some(index) = index {
            self.tab.model_job = None;
            let before = self.tab.doc.clone();
            if models3d::set_display(&mut self.tab.doc, index, display) {
                self.changed(before);
                self.status = status(display).into();
            }
            return Task::none();
        }
        if display == Display::TwoD {
            self.tab.model_job = None;
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
            display,
            prepared,
            _handle: handle.abort_on_drop(),
        });
        self.status = "Generating 3D model · MMFF94s".into();
        self.error = false;
        task
    }

    fn add_model(&mut self, model: models3d::Model3d, display: Display) {
        let before = self.tab.doc.clone();
        let atoms: Vec<_> = model.atoms.iter().map(|a| a.id).collect();
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
            self.status = status(display).into();
        }
    }

    /// Show as 2D / 3D / Both for one selected molecule.
    pub(super) fn structure_section(&self) -> Option<Element<'_, Message>> {
        let (_, index) = self.selected_model()?;
        let current = index
            .and_then(|i| self.tab.doc.models.get(i))
            .map_or(Display::TwoD, |m| m.display);
        let pending = self.tab.model_job.as_ref().map(|job| job.display);
        let enabled = !self.tab.busy && self.tab.optimization.is_none();
        Some(
            self.inspector_section(
                Section::Structure,
                "Structure",
                "",
                true,
                column![
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
                                enabled.then_some(Message::Models3d(Action::Show(display))),
                            )
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
                .spacing(6),
            ),
        )
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

fn status(display: Display) -> &'static str {
    match display {
        Display::TwoD => "Shown as 2D · Undo restores the 3D model",
        Display::ThreeD => "Shown as 3D model · Undo restores the 2D drawing",
        Display::Both => "Shown as 2D and 3D · Undo restores the previous view",
    }
}

#[cfg(test)]
mod tests;
