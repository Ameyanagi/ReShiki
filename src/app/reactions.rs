use super::{App, InspectorTab, Job, Message, Request, Tool};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length, Task};
use reshiki::reactions::{Reaction, Role};

#[derive(Default)]
pub struct State {
    arrow: Option<u64>,
    epoch: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::{Arrow, Point};
    use reshiki::engine::{ChemistryEngine, LocalEngine};

    #[test]
    fn role_edits_are_undoable_and_arrow_reversal_swaps_roles() {
        let (mut app, _) = App::new();
        app.busy = false;
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        let c = app.doc.add_atom("C", Point::new(300., 0.));
        app.doc.arrows.push(Arrow::new(
            4,
            Point::new(100., 0.),
            Point::new(240., 0.),
            Default::default(),
            Default::default(),
        ));
        app.saved = app.doc.clone();
        let original = app.doc.clone();
        app.selected = vec![a];
        let _ = app.update(Message::Reaction(Action::Assign(Role::Reactant)));
        assert_eq!(app.doc.reactions[0].reactants[0].atoms, [a, b]);
        assert!(app.dirty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        app.selected = vec![c];
        let _ = app.update(Message::Reaction(Action::Assign(Role::Product)));
        let ready = app.doc.clone();
        app.selected = vec![4];
        app.sync_arrows();
        assert_eq!(app.inspector_tab, InspectorTab::Reactions);
        let _ = app.update(Message::ArrowAction(super::super::arrows::Action::Reverse));
        assert_eq!(app.doc.reactions[0].reactants, ready.reactions[0].products);
        assert_eq!(app.doc.reactions[0].products, ready.reactions[0].reactants);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, ready);
        // An incompatible chemistry edit is rejected as a whole.
        app.doc.add_bond(a, c, 1, "plain");
        app.changed(ready.clone());
        assert!(app.error);
        assert_eq!(app.doc, ready);
        let _ = app.update(Message::Reaction(Action::SelectAll));
        assert_eq!(app.selected.len(), 4);
        let _ = app.update(Message::Reaction(Action::Unlink));
        assert!(app.doc.reactions.is_empty());
        assert_eq!(app.doc.atoms, ready.atoms);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, ready);
    }

    #[tokio::test]
    #[ignore = "Manual GPU snapshots without opening desktop windows"]
    async fn reaction_headless_snapshot() {
        use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
        let (mut app, _) = App::new();
        app.doc = LocalEngine::default()
            .execute(Request::import(
                "rsmi",
                "CC(=O)O.CCO>OS(=O)(=O)O>CCOC(C)=O.O",
            ))
            .await
            .unwrap()
            .document
            .unwrap();
        app.busy = false;
        app.status = "Reaction roles ready · Editable molecules and reaction data".into();
        let _ = app.update(Message::Reaction(Action::Open));
        let directory = std::path::Path::new("artifacts/reaction-qa");
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(
            directory.join("esterification.reshiki"),
            serde_json::to_vec_pretty(&app.doc).unwrap(),
        )
        .unwrap();
        for (name, width, height) in [("desktop", 1280, 820), ("compact", 1040, 680)] {
            app.viewport = iced::Size::new(width as f32 - 410., height as f32 - 200.);
            app.fit();
            let mut renderer = <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap();
            let size = iced::Size::new(width as f32, height as f32);
            let theme = app.theme();
            let mut view = app.view();
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
            let mut messages = Vec::new();
            view.as_widget_mut().update(
                &mut tree,
                &iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
            view.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &iced::Rectangle::with_size(size),
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                iced::Color::WHITE,
            );
            image::save_buffer(
                directory.join(format!("{name}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            for event in [
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                view.as_widget_mut().update(
                    &mut tree,
                    &event,
                    iced::advanced::Layout::new(&node),
                    mouse::Cursor::Available(iced::Point::new(
                        width as f32 - 240.,
                        height as f32 - 108.,
                    )),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut messages),
                    &iced::Rectangle::with_size(size),
                );
            }
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::Reaction(Action::Export("rxn")))),
                "RXN export stays clickable at {width}×{height}"
            );
        }
    }
}
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
}
impl App {
    fn reaction_arrow(&self) -> Option<u64> {
        let selected: Vec<_> = self
            .doc
            .arrows
            .iter()
            .filter(|a| self.selected.contains(&a.id))
            .collect();
        if let [arrow] = selected.as_slice() {
            return Some(arrow.id);
        }
        if self.reactions.epoch == self.file_epoch
            && let Some(id) = self.reactions.arrow
            && self.doc.arrows.iter().any(|a| a.id == id)
        {
            return Some(id);
        }
        if let [reaction] = self.doc.reactions.as_slice() {
            return Some(reaction.arrow);
        }
        if let [arrow] = self.doc.arrows.as_slice() {
            return Some(arrow.id);
        }
        None
    }
    pub(super) fn reaction_action(&mut self, action: Action) -> Task<Message> {
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Reactions;
        self.tool = Tool::Select;
        if let Action::Choose(id) = action {
            self.reactions = State {
                arrow: Some(id),
                epoch: self.file_epoch,
            };
            self.selected = vec![id];
            return Task::none();
        }
        let Some(arrow) = self.reaction_arrow() else {
            return Task::none();
        };
        self.reactions = State {
            arrow: Some(arrow),
            epoch: self.file_epoch,
        };
        if let Action::Export(format) = action {
            if self.busy {
                return Task::none();
            }
            let mut request = Request::molecule("export", self.doc.clone());
            request.selected_ids = Some(vec![arrow]);
            request.format = Some(format.into());
            return self.run(request, Job::Export(format));
        }
        let before = self.doc.clone();
        match action {
            Action::Assign(role) => {
                if let Err(error) =
                    reshiki::reactions::assign(&mut self.doc, arrow, &self.selected, role)
                {
                    self.error = true;
                    self.status = error;
                    return Task::none();
                }
            }
            Action::ClearSelected => {
                if let Some(reaction) = self.doc.reactions.iter_mut().find(|r| r.arrow == arrow) {
                    for role in Role::ALL {
                        reaction
                            .participants_mut(role)
                            .retain(|p| !p.atoms.iter().any(|id| self.selected.contains(id)));
                    }
                }
            }
            Action::Unlink => self.doc.reactions.retain(|r| r.arrow != arrow),
            Action::SelectAll => {
                if let Some(reaction) = self.doc.reactions.iter().find(|r| r.arrow == arrow) {
                    self.selected = reaction.ids();
                }
            }
            Action::SelectParticipant(role, index) => {
                if let Some(participant) = self
                    .doc
                    .reactions
                    .iter()
                    .find(|r| r.arrow == arrow)
                    .and_then(|r| r.participants(role).get(index))
                {
                    self.selected = participant.atoms.clone();
                }
            }
            _ => {}
        }
        self.changed(before);
        Task::none()
    }
    pub(super) fn reactions_inspector(&self) -> Element<'_, Message> {
        use super::workspace::{command, muted, panel};
        let ready = self
            .reaction_arrow()
            .and_then(|arrow| self.doc.reactions.iter().find(|r| r.arrow == arrow))
            .is_some_and(Reaction::ready);
        let mut exports = row![].spacing(8);
        for (label, format) in [("RXN · V3000", "rxn"), ("Reaction SMILES", "rsmi")] {
            exports = exports.push(
                command(label, Message::Reaction(Action::Export(format)))
                    .on_press_maybe(
                        (ready && !self.busy).then_some(Message::Reaction(Action::Export(format))),
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
            scrollable(container(self.reactions_panel()).padding([0, 16])).height(Length::Fill),
            container(
                column![
                    text(if ready {
                        "Export reaction data"
                    } else {
                        "Assign reactants and products to export"
                    })
                    .size(12),
                    exports,
                    text("Save .reshiki to retain the complete scheme and captions.")
                        .size(11)
                        .color(muted())
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
        use super::workspace::{command, control, muted};
        let mut body = column![
            text("Choose an arrow, then assign molecules from the canvas.")
                .size(12)
                .color(muted()),
        ]
        .spacing(12);
        let active = self.reaction_arrow();
        if self.doc.arrows.is_empty() {
            return body
                .push(text("Add a reaction arrow with A to begin.").size(13))
                .into();
        }
        for (index, arrow) in self.doc.arrows.iter().enumerate() {
            let reaction = self.doc.reactions.iter().find(|r| r.arrow == arrow.id);
            let summary = reaction
                .map(|r| format!("{} → {}", r.reactants.len(), r.products.len()))
                .unwrap_or_else(|| "Assign roles".into());
            body = body.push(
                button(row![
                    text(format!("Arrow {}", index + 1))
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
        let Some(arrow) = active else {
            return body.into();
        };
        let empty = Reaction::new(arrow);
        let reaction = self
            .doc
            .reactions
            .iter()
            .find(|r| r.arrow == arrow)
            .unwrap_or(&empty);
        let has_atoms = self.selected.iter().any(|id| self.doc.atom(*id).is_some());
        for role in Role::ALL {
            let parts = reaction.participants(role);
            let mut card = column![row![
                text(role.label()).size(13).width(Length::Fill),
                text(parts.len().to_string()).size(12).color(muted())
            ]]
            .spacing(5);
            for (index, part) in parts.iter().enumerate() {
                let selected = part.atoms.iter().all(|id| self.selected.contains(id));
                let mut preview = reshiki::editing::selection(&self.doc, &part.atoms);
                for atom in &mut preview.atoms {
                    if let Some(source) = self.doc.atom(atom.id) {
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
                card = card.push(text("No molecules assigned").size(11).color(muted()));
            }
            card = card.push(
                command(
                    "+ Assign selected molecules",
                    Message::Reaction(Action::Assign(role)),
                )
                .on_press_maybe(has_atoms.then_some(Message::Reaction(Action::Assign(role))))
                .width(Length::Fill),
            );
            body = body.push(container(card).padding(10).width(Length::Fill).style(|_| {
                container::Style {
                    background: Some(iced::Color::WHITE.into()),
                    border: iced::Border {
                        color: iced::Color::from_rgb8(218, 228, 225),
                        width: 1.,
                        radius: 8.into(),
                    },
                    ..Default::default()
                }
            }));
        }
        body = body.push(text("Selecting any atom assigns its whole molecule. Reassigning moves it to the new role.").size(11).color(muted()));
        body = body.push(
            row![
                command("Select reaction", Message::Reaction(Action::SelectAll)),
                command(
                    "Clear selected roles",
                    Message::Reaction(Action::ClearSelected)
                )
                .on_press_maybe(has_atoms.then_some(Message::Reaction(Action::ClearSelected)))
            ]
            .spacing(4),
        );
        if !self.doc.reactions.is_empty() {
            body = body.push(command(
                "Remove reaction roles",
                Message::Reaction(Action::Unlink),
            ));
        }
        body.into()
    }
}
