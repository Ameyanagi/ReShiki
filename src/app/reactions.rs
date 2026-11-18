use super::{App, InspectorTab, Job, Message, Request, Tool};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length, Task};
use reshiki::arrows::Preset;
use reshiki::reactions::{Reaction, Role};

#[derive(Default)]
pub struct State {
    arrow: Option<u64>,
    epoch: u64,
    /// What takes part in a reaction, shown until the drawing changes.
    pub(super) preview: Option<(u64, Reaction)>,
    /// Color this arrow's reaction once its reviewed atom maps are applied.
    pub(super) color_after_maps: Option<u64>,
}

#[cfg(test)]
mod tests;
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Choose(u64),
    Assign(Role),
    ClearSelected,
    Unlink,
    SelectAll,
    SelectParticipant(Role, usize),
    Export(&'static str),
    /// Outline what takes part in the chosen arrow's reaction, or hide it.
    Preview,
    /// Color the reaction by origin, mapping its atoms first if needed.
    ColorByOrigin,
    ColorSetting(ColorSetting),
    ClearColors,
}

/// One color by origin setting of the chosen reaction.
#[derive(Debug, Clone, Copy)]
pub enum ColorSetting {
    Reactant(usize, HueChoice),
    Formed(HueChoice),
    Bonds(bool),
    Changes(bool),
    Rings(bool),
    Legend(bool),
}

/// A palette hue in a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HueChoice(pub reshiki::palette::Hue);
impl std::fmt::Display for HueChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.name())
    }
}
const HUES: [HueChoice; 8] = [
    HueChoice(reshiki::palette::Hue::Red),
    HueChoice(reshiki::palette::Hue::Orange),
    HueChoice(reshiki::palette::Hue::Amber),
    HueChoice(reshiki::palette::Hue::Green),
    HueChoice(reshiki::palette::Hue::Teal),
    HueChoice(reshiki::palette::Hue::Blue),
    HueChoice(reshiki::palette::Hue::Indigo),
    HueChoice(reshiki::palette::Hue::Purple),
];
impl App {
    pub(super) fn reaction_arrow(&self) -> Option<u64> {
        let selected: Vec<_> = self
            .tab
            .doc
            .arrows
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id))
            .collect();
        if let [arrow] = selected.as_slice() {
            return Some(arrow.id);
        }
        if self.tab.reactions.epoch == self.tab.file_epoch
            && let Some(id) = self.tab.reactions.arrow
            && self.tab.doc.arrows.iter().any(|a| a.id == id)
        {
            return Some(id);
        }
        if let [reaction] = self.tab.doc.reactions.as_slice() {
            return Some(reaction.arrow);
        }
        if let [arrow] = self.tab.doc.arrows.as_slice() {
            return Some(arrow.id);
        }
        None
    }
    /// The reaction preview, while the drawing is unchanged since it was made.
    pub(super) fn reaction_preview(&self) -> Option<&Reaction> {
        self.tab
            .reactions
            .preview
            .as_ref()
            .filter(|(revision, _)| *revision == self.tab.revision)
            .map(|(_, reaction)| reaction)
    }
    fn preview_reaction(&mut self) -> Task<Message> {
        let shown = self.reaction_preview().map(|r| r.arrow);
        self.tab.reactions.preview = None;
        let Some(arrow) = self.reaction_arrow() else {
            self.error = true;
            self.status = "Select a reaction arrow first.".into();
            return Task::none();
        };
        if shown == Some(arrow) {
            self.status = "Reaction preview hidden".into();
            return super::focus_drawing();
        }
        match reshiki::reactions::preview(&self.tab.doc, arrow) {
            Ok(reaction) => {
                let agents = match reaction.agents.len() {
                    0 => String::new(),
                    1 => " + 1 reagent".into(),
                    n => format!(" + {n} reagents"),
                };
                self.error = false;
                self.status = format!(
                    "Preview · {} → {}{agents} · Esc hides it",
                    reaction.reactants.len(),
                    reaction.products.len()
                );
                self.tab.reactions.preview = Some((self.tab.revision, reaction));
            }
            Err(reason) => {
                self.error = true;
                self.status = reason.into();
            }
        }
        super::focus_drawing()
    }
    /// Color `arrow`'s reaction by origin as one Undo step. Without atom
    /// maps, run Auto-map first; applying its maps colors the reaction.
    pub(super) fn color_by_origin(&mut self, arrow: u64) -> Task<Message> {
        use reshiki::reaction_colors::{Refusal, paint};
        let before = self.tab.doc.clone();
        match paint(&mut self.tab.doc, arrow) {
            Ok(()) => {
                self.changed(before);
                if !self.error {
                    self.status = "Colored by origin".into();
                }
                Task::none()
            }
            Err(Refusal::Maps) => {
                self.tab.doc = before;
                self.tab.reactions.color_after_maps = Some(arrow);
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Reactions;
                Task::done(Message::Mapping(super::reaction_mapping::Action::Auto))
            }
            Err(refusal) => {
                self.tab.doc = before;
                self.error = true;
                self.status = refusal.message().into();
                Task::none()
            }
        }
    }
    fn color_setting(&mut self, arrow: u64, setting: ColorSetting) {
        use reshiki::reaction_colors::{set_legend, update};
        let before = self.tab.doc.clone();
        let result = match setting {
            ColorSetting::Legend(shown) => set_legend(
                &mut self.tab.doc,
                arrow,
                shown,
                self.tab.caption_format.clone(),
            ),
            setting => update(&mut self.tab.doc, arrow, |colors| match setting {
                ColorSetting::Reactant(index, hue) => {
                    if let Some(slot) = colors.reactants.get_mut(index) {
                        *slot = hue.0;
                    }
                }
                ColorSetting::Formed(hue) => colors.formed = hue.0,
                ColorSetting::Bonds(on) => colors.bonds = on,
                ColorSetting::Changes(on) => colors.changes = on,
                ColorSetting::Rings(on) => colors.rings = on,
                ColorSetting::Legend(_) => {}
            }),
        };
        match result {
            Ok(()) => self.changed(before),
            Err(refusal) => {
                self.tab.doc = before;
                self.error = true;
                self.status = refusal.message().into();
            }
        }
    }
    pub(super) fn reaction_action(&mut self, action: Action) -> Task<Message> {
        if let Action::Preview = action {
            return self.preview_reaction();
        }
        if let Action::ColorByOrigin = action {
            let Some(arrow) = self.reaction_arrow() else {
                self.error = true;
                self.status = "Select a reaction arrow first.".into();
                return Task::none();
            };
            return self.color_by_origin(arrow);
        }
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Reactions;
        self.tool = Tool::Select;
        if let Action::Choose(id) = action {
            self.tab.reactions = State {
                arrow: Some(id),
                epoch: self.tab.file_epoch,
                preview: self.tab.reactions.preview.take(),
                color_after_maps: self.tab.reactions.color_after_maps.take(),
            };
            self.tab.selected = vec![id];
            return Task::none();
        }
        let Some(arrow) = self.reaction_arrow() else {
            return Task::none();
        };
        self.tab.reactions = State {
            arrow: Some(arrow),
            epoch: self.tab.file_epoch,
            preview: self.tab.reactions.preview.take(),
            color_after_maps: self.tab.reactions.color_after_maps.take(),
        };
        if let Action::Export(format) = action {
            if self.tab.busy {
                return Task::none();
            }
            let mut request = Request::molecule("export", self.tab.doc.clone());
            request.selected_ids = Some(vec![arrow]);
            request.format = Some(format.into());
            return self.run(request, Job::Export(format));
        }
        let before = self.tab.doc.clone();
        match action {
            Action::Assign(role) => {
                if let Err(error) =
                    reshiki::reactions::assign(&mut self.tab.doc, arrow, &self.tab.selected, role)
                {
                    self.error = true;
                    self.status = error;
                    return Task::none();
                }
            }
            Action::ClearSelected => {
                if let Some(reaction) = self.tab.doc.reactions.iter_mut().find(|r| r.arrow == arrow)
                {
                    for role in Role::ALL {
                        reaction
                            .participants_mut(role)
                            .retain(|p| !p.atoms.iter().any(|id| self.tab.selected.contains(id)));
                    }
                }
            }
            Action::Unlink => self.tab.doc.reactions.retain(|r| r.arrow != arrow),
            Action::ColorSetting(setting) => {
                self.color_setting(arrow, setting);
                return Task::none();
            }
            Action::ClearColors => {
                if !reshiki::reaction_colors::clear_paint(&mut self.tab.doc, arrow) {
                    return Task::none();
                }
                self.changed(before);
                if !self.error {
                    self.status = "Colors removed".into();
                }
                return Task::none();
            }
            Action::SelectAll => {
                if let Some(reaction) = self.tab.doc.reactions.iter().find(|r| r.arrow == arrow) {
                    self.tab.selected = reaction.ids();
                }
            }
            Action::SelectParticipant(role, index) => {
                if let Some(participant) = self
                    .tab
                    .doc
                    .reactions
                    .iter()
                    .find(|r| r.arrow == arrow)
                    .and_then(|r| r.participants(role).get(index))
                {
                    self.tab.selected = participant.atoms.clone();
                }
            }
            _ => {}
        }
        self.changed(before);
        Task::none()
    }
    pub(super) fn reactions_inspector(&self) -> Element<'_, Message> {
        use super::workspace::{command, muted_text, panel};
        let ready = self
            .reaction_arrow()
            .and_then(|arrow| self.tab.doc.reactions.iter().find(|r| r.arrow == arrow))
            .is_some_and(Reaction::ready);
        let mut exports = row![].spacing(8);
        for (label, format) in [("RXN · V3000", "rxn"), ("Reaction SMILES", "rsmi")] {
            exports = exports.push(
                command(label, Message::Reaction(Action::Export(format)))
                    .on_press_maybe(
                        (ready && !self.tab.busy)
                            .then_some(Message::Reaction(Action::Export(format))),
                    )
                    .width(Length::Fill),
            );
        }
        container(column![
            container(
                column![
                    command("‹ Properties", Message::Inspector(InspectorTab::Properties)),
                    text("Reaction roles").size(20)
                ]
                .spacing(8)
            )
            .padding([12, 16]),
            scrollable(
                container(column![self.reactions_panel(), self.mapping_section()].spacing(16))
                    .padding([0, 16])
            )
            .id("inspector-content")
            .height(Length::Fill),
            container(
                column![
                    text(if ready {
                        "Export reaction data"
                    } else {
                        "Assign reactants and products to export"
                    })
                    .size(12),
                    exports,
                    text("Save .rsk to retain the complete scheme and captions.")
                        .size(11)
                        .style(muted_text)
                ]
                .spacing(7)
            )
            .padding(16),
        ])
        .width(320)
        .height(Length::Fill)
        .style(panel)
        .into()
    }
    pub(super) fn reactions_panel(&self) -> Element<'_, Message> {
        use super::workspace::{command, control, muted_text};
        let mut body = column![
            text("Choose an arrow, then assign molecules from the canvas.")
                .size(12)
                .style(muted_text),
        ]
        .spacing(12);
        let active = self.reaction_arrow();
        if self.tab.doc.arrows.is_empty() {
            return body
                .push(text("Add a reaction arrow with A to begin.").size(13))
                .into();
        }
        for (index, arrow) in self.tab.doc.arrows.iter().enumerate() {
            let preset = Preset::from_kind(&arrow.kind);
            let reaction = self.tab.doc.reactions.iter().find(|r| r.arrow == arrow.id);
            let summary = reaction
                .map(|r| format!("{} → {}", r.reactants.len(), r.products.len()))
                .unwrap_or_else(|| {
                    if !arrow.purpose.is_chemistry() {
                        arrow.purpose.label()
                    } else if preset.reaction_role_refusal().is_some() {
                        "Roles unavailable"
                    } else {
                        "Assign roles"
                    }
                    .into()
                });
            body = body.push(
                button(row![
                    text(format!("Arrow {} · {preset}", index + 1))
                        .size(13)
                        .width(Length::Fill),
                    text(summary).size(12)
                ])
                .padding([9, 10])
                .width(Length::Fill)
                .style(control(active == Some(arrow.id)))
                .on_press(Message::Reaction(Action::Choose(arrow.id))),
            );
        }
        let Some(arrow) = active.and_then(|id| self.tab.doc.arrows.iter().find(|a| a.id == id))
        else {
            return body.into();
        };
        let refusal = Preset::from_kind(&arrow.kind).reaction_role_refusal();
        if let Some(reason) = refusal {
            body = body.push(text(reason).size(12).style(text::danger));
        }
        let empty = Reaction::new(arrow.id);
        let reaction = self
            .tab
            .doc
            .reactions
            .iter()
            .find(|r| r.arrow == arrow.id)
            .unwrap_or(&empty);
        let has_atoms = self
            .tab
            .selected
            .iter()
            .any(|id| self.tab.doc.atom(*id).is_some());
        for role in Role::ALL {
            let parts = reaction.participants(role);
            let mut card = column![row![
                text(role.label()).size(13).width(Length::Fill),
                text(parts.len().to_string()).size(12).style(muted_text)
            ]]
            .spacing(5);
            for (index, part) in parts.iter().enumerate() {
                let selected = part.atoms.iter().all(|id| self.tab.selected.contains(id));
                let mut preview = reshiki::editing::selection(&self.tab.doc, &part.atoms);
                for atom in &mut preview.atoms {
                    if let Some(source) = self.tab.doc.atom(atom.id) {
                        atom.label_h = source.label_h;
                    }
                }
                card = card.push(
                    button(
                        row![
                            canvas(DrawingThumbnail(preview)).width(92).height(28),
                            text(format!(
                                "{}{} {}",
                                if part.coefficient > 1 {
                                    format!("{} × ", part.coefficient)
                                } else {
                                    String::new()
                                },
                                part.atoms.len(),
                                if part.atoms.len() == 1 {
                                    "atom"
                                } else {
                                    "atoms"
                                }
                            ))
                            .size(12),
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .padding([3, 8])
                    .width(Length::Fill)
                    .style(control(selected))
                    .on_press(Message::Reaction(Action::SelectParticipant(role, index))),
                );
            }
            if parts.is_empty() {
                card = card.push(text("No molecules assigned").size(11).style(muted_text));
            }
            if refusal.is_none() {
                card = card.push(
                    command(
                        "+ Assign selected molecules",
                        Message::Reaction(Action::Assign(role)),
                    )
                    .on_press_maybe(has_atoms.then_some(Message::Reaction(Action::Assign(role))))
                    .width(Length::Fill),
                );
            }
            body = body.push(
                container(card)
                    .padding(10)
                    .width(Length::Fill)
                    .style(|theme| {
                        crate::appearance::container(
                            theme,
                            container::Style {
                                background: Some(iced::Color::WHITE.into()),
                                border: iced::Border {
                                    color: iced::Color::from_rgb8(218, 228, 225),
                                    width: 1.,
                                    radius: 8.into(),
                                },
                                ..Default::default()
                            },
                        )
                    }),
            );
        }
        if refusal.is_none() {
            body = body.push(text("Selecting any atom assigns its whole molecule. Reassigning moves it to the new role.").size(11).style(muted_text));
        }
        body = body.push(
            row![
                command("Preview", Message::Reaction(Action::Preview)),
                command("Select reaction", Message::Reaction(Action::SelectAll)),
                command(
                    "Clear selected roles",
                    Message::Reaction(Action::ClearSelected)
                )
                .on_press_maybe(has_atoms.then_some(Message::Reaction(Action::ClearSelected)))
            ]
            .spacing(4),
        );
        if reaction.ready() || reaction.colors.is_some() {
            body = body.push(self.color_section(reaction));
        }
        if !self.tab.doc.reactions.is_empty() {
            body = body.push(command(
                "Remove reaction roles",
                Message::Reaction(Action::Unlink),
            ));
        }
        body.into()
    }

    /// Color by origin: the command, then the settings once it has run.
    fn color_section(&self, reaction: &Reaction) -> Element<'static, Message> {
        use super::workspace::{accessible_checkbox, command, muted_text, section};
        let send = |setting| Message::Reaction(Action::ColorSetting(setting));
        let mut body = column![section("Color by origin")].spacing(6);
        let Some(colors) = &reaction.colors else {
            return body
                .push(
                    text("Color each reactant and carry it into the products along the atom maps.")
                        .size(11)
                        .style(muted_text),
                )
                .push(
                    command("Color by origin", Message::Reaction(Action::ColorByOrigin))
                        .width(Length::Fill),
                )
                .into();
        };
        for (index, hue) in colors.reactants.iter().enumerate() {
            body = body.push(
                row![
                    text(format!("Reactant {}", index + 1))
                        .size(12)
                        .width(Length::Fill),
                    crate::appearance::pick_list(HUES, Some(HueChoice(*hue)), move |h| {
                        send(ColorSetting::Reactant(index, h))
                    })
                    .text_size(12)
                    .width(120),
                ]
                .align_y(iced::Alignment::Center),
            );
        }
        body = body.push(
            row![
                text("New bonds and rings").size(12).width(Length::Fill),
                crate::appearance::pick_list(HUES, Some(HueChoice(colors.formed)), move |h| {
                    send(ColorSetting::Formed(h))
                })
                .text_size(12)
                .width(120),
            ]
            .align_y(iced::Alignment::Center),
        );
        for (id, label, on, setting) in [
            (
                "colors-bonds",
                "Mark new bonds",
                colors.bonds,
                ColorSetting::Bonds(!colors.bonds),
            ),
            (
                "colors-changes",
                "Mark broken and changed bonds",
                colors.changes,
                ColorSetting::Changes(!colors.changes),
            ),
            (
                "colors-rings",
                "Fill new rings",
                colors.rings,
                ColorSetting::Rings(!colors.rings),
            ),
            (
                "colors-legend",
                "Legend",
                colors.legend.is_some(),
                ColorSetting::Legend(colors.legend.is_none()),
            ),
        ] {
            body = body.push(accessible_checkbox(id, label, on, Some(send(setting))));
        }
        body.push(
            row![
                command("Update colors", Message::Reaction(Action::ColorByOrigin))
                    .width(Length::Fill),
                command("Clear colors", Message::Reaction(Action::ClearColors)).width(Length::Fill),
            ]
            .spacing(4),
        )
        .into()
    }
}
