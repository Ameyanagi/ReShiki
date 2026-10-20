//! Show a selected molecule as its 2D drawing, a 3D model or both (#386).
//!
//! A molecule without a model gets one from the geometry worker: one abortable
//! job per tab, whose result is dropped when the drawing changed meanwhile.
//! Each switch, including the one that adds the model, is one Undo step.
use super::icons::{Glyph, Icon};
use super::workspace::{accessible_command, control, muted_text};
use super::{App, Message, document_tab::TabId, inspector::Section};
use iced::widget::{Space, canvas, column, row, slider, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::geometry::{Client, ForceField, Optimized, Prepared};
use reshiki::models3d::{self, Background, Display, Hydrogens, Look, Style};
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
    /// Change the selected model; each change is one Undo step, and a slider
    /// drag is one step until [`Action::SlideEnd`].
    Set(Setting),
    SlideEnd,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Setting {
    Style(Style),
    Look(Look),
    Hydrogens(Hydrogens),
    Background(Background),
    DepthFade(f32),
    Perspective(f32),
    ResetView,
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
            Action::Set(setting) => {
                self.set_model(setting);
                Task::none()
            }
            Action::SlideEnd => {
                self.tab.model_sliding = false;
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
    pub(super) fn pending(&self, atoms: &[u64]) -> Option<Display> {
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
            let before = self.tab.doc.clone();
            if models3d::set_display(&mut self.tab.doc, index, display) {
                let previous = before.models.get(index).map(|m| m.display);
                let current = self.model_key();
                self.changed(before);
                self.status = status(display, previous.unwrap_or(Display::TwoD));
                // Showing another molecule differently leaves a generation
                // in flight valid (unless it was already stale).
                let next = self.model_key();
                if let Some(job) = &mut self.tab.model_job
                    && job.current == current
                {
                    job.current = next;
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

    fn set_model(&mut self, setting: Setting) {
        if self.tab.busy || self.tab.drawing_edit_active() {
            return;
        }
        let Some((_, Some(index))) = self.selected_model() else {
            return;
        };
        let before = self.tab.doc.clone();
        let reset = before
            .models
            .get(index)
            .and_then(|model| models3d::initial_view(&before, model));
        let Some(model) = self.tab.doc.models.get_mut(index) else {
            return;
        };
        match setting {
            Setting::Style(style) => model.style = style,
            Setting::Look(look) => model.look = look,
            Setting::Hydrogens(shown) => model.show_hydrogens = shown,
            Setting::Background(background) => model.background = background,
            Setting::DepthFade(value) => model.depth_fade = value.clamp(0., 1.),
            Setting::Perspective(value) => model.perspective = value.clamp(0., 1.),
            Setting::ResetView => model.rotation = reset.unwrap_or(model.rotation),
        }
        if matches!(setting, Setting::DepthFade(_) | Setting::Perspective(_)) {
            self.changed_continuing(before, self.tab.model_sliding);
            self.tab.model_sliding = true;
        } else {
            self.changed(before);
        }
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

impl App {
    /// Style, look and view of the selected molecule's model while it is shown.
    pub(super) fn model_section(&self) -> Option<Element<'_, Message>> {
        let model = self.tab.doc.models.get(self.selected_model()?.1?)?;
        if model.display == Display::TwoD {
            return None;
        }
        let enabled = !self.tab.busy && self.tab.optimization.is_none();
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
        let [ball, stick, wire, space] = Style::ALL.map(|style| {
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
        });
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
        Some(
            self.inspector_section(
                Section::Model3d,
                "3D model",
                name(model.style),
                true,
                column![
                    field(
                        "Style",
                        column![row![ball, stick].spacing(4), row![wire, space].spacing(4)]
                            .spacing(4)
                            .into()
                    ),
                    field(
                        "Look",
                        segments(vec![
                            look(
                                Look::Flat,
                                "flat",
                                "Flat look, for journals",
                                "Flat · journal"
                            ),
                            look(
                                Look::Glossy,
                                "glossy",
                                "Glossy look, for slides",
                                "Glossy · slides"
                            ),
                        ])
                    ),
                    field(
                        "Hydrogens",
                        segments(vec![
                            hydrogens(Hydrogens::All, "All", "Show all hydrogens"),
                            hydrogens(Hydrogens::Polar, "Polar", "Show hydrogens on N, O and S"),
                            hydrogens(Hydrogens::None, "None", "Hide hydrogens"),
                        ])
                    ),
                    slider_field("Depth fade", model.depth_fade, Setting::DepthFade),
                    slider_field("Perspective", model.perspective, Setting::Perspective),
                    field(
                        "Background",
                        segments(vec![
                            background(Background::None, "none", "No background", "None"),
                            background(
                                Background::Card,
                                "card",
                                "Soft card background",
                                "Soft card"
                            ),
                        ])
                    ),
                    accessible_command("model-reset-view", "Reset the 3D view", "Reset view")
                        .on_press_maybe(set(Setting::ResetView)),
                ]
                .spacing(10),
            ),
        )
    }
}

fn name(style: Style) -> &'static str {
    match style {
        Style::BallStick => "Ball & stick",
        Style::Stick => "Stick",
        Style::Wireframe => "Wireframe",
        Style::SpaceFilling => "Space-filling",
    }
}

fn slug(style: Style) -> &'static str {
    match style {
        Style::BallStick => "ball-stick",
        Style::Stick => "stick",
        Style::Wireframe => "wireframe",
        Style::SpaceFilling => "space-filling",
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
    let restored = match previous {
        Display::TwoD => "the 2D drawing",
        Display::ThreeD => "the 3D model",
        Display::Both => "2D and 3D",
    };
    format!("Shown as {shown} · Undo restores {restored}")
}

#[cfg(test)]
mod tests;
