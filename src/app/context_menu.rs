//! Commands for the object under a secondary click, or the current selection.
use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message, inspector};
use iced::advanced::Widget;
use iced::widget::{Space, button, column, container, mouse_area, opaque, scrollable, stack, text};
use iced::{Border, Color, Element, Length, Point, Task};
use reshiki::{bonds::BondPreset, editing::Arrange};

#[derive(Debug, Clone, Copy, Default)]
pub enum Page {
    #[default]
    Main,
    Align,
    Bonds,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Edit;
    use reshiki::document::{Arrow, Point as World};

    #[test]
    fn context_alignment_preserves_molecular_geometry_and_undo_restores_every_group() {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let source = app.doc.clone();
        reshiki::editing::append(&mut app.doc, &source, World::new(240., 80.));
        let arrow = Arrow::new(
            app.doc.next_id(),
            World::new(90., -60.),
            World::new(160., -60.),
            Default::default(),
            Default::default(),
        );
        app.doc.arrows.push(arrow);
        let before = app.doc.clone();
        let ids = app.doc.all_ids();
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            selected: ids.clone(),
        });
        assert_eq!(app.alignment_count(), 3);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::RefreshLabels);
        assert!(
            app.context_menu.is_some(),
            "Background label refresh must leave the menu open"
        );
        let _ = app.context_action(Action::Run(Box::new(Message::Arrange(
            Arrange::AlignVertical,
        ))));
        assert!(app.context_menu.is_none());
        let centers: Vec<_> = reshiki::editing::groups(&app.doc, &ids)
            .iter()
            .map(|ids| {
                let (lo, hi) = reshiki::scene::selection_bounds(&app.doc, ids).unwrap();
                (lo.y + hi.y) / 2.
            })
            .collect();
        assert!(centers.iter().all(|y| (y - centers[0]).abs() < 0.001));
        for bond in &app.doc.bonds {
            let length = |doc: &reshiki::document::Document| {
                doc.atom(bond.a)
                    .unwrap()
                    .position
                    .distance(doc.atom(bond.b).unwrap().position)
            };
            assert!((length(&app.doc) - length(&before)).abs() < 0.001);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            selected: ids,
        });
        let _ = app.update(Message::Escape);
        assert!(app.context_menu.is_none());
        assert_eq!(app.doc, before);
    }

    #[tokio::test]
    async fn inserted_examples_keep_existing_objects_and_can_be_removed_in_one_undo() {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let before = app.doc.clone();
        let result = app
            .engine
            .request(reshiki::engine::Request::import_smiles("CCO"))
            .await
            .unwrap();
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
            kind: super::super::Job::Insert,
            result: Box::new(Ok(result)),
        });
        assert_eq!(app.doc.atoms.len(), before.atoms.len() + 3);
        assert_eq!(app.selected.len(), 3);
        for atom in &before.atoms {
            assert_eq!(app.doc.atom(atom.id), Some(atom));
        }
        let (_, old_max) = before.bounds();
        assert!(
            app.selected
                .iter()
                .all(|id| app.doc.atom(*id).unwrap().position.x > old_max.x)
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Close,
    Page(Page),
    Run(Box<Message>),
    Properties(bool),
}
pub(super) struct State {
    pub position: Point,
    pub page: Page,
}

impl App {
    pub(super) fn context_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Close => self.context_menu = None,
            Action::Page(page) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.page = page;
                }
            }
            Action::Run(message) => {
                self.context_menu = None;
                return self.update(*message);
            }
            Action::Properties(molecular) => {
                self.context_menu = None;
                if molecular {
                    self.inspector_ui.update(inspector::Action::Section(
                        inspector::Section::Molecule,
                        true,
                    ));
                }
                return self.update(Message::Inspector(InspectorTab::Properties));
            }
        }
        Task::none()
    }

    pub(super) fn with_context_menu<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(menu) = &self.context_menu else {
            return base;
        };
        let item = |label: String, action: Action, enabled: bool| {
            button(text(label).size(12))
                .padding([6, 10])
                .width(Length::Fill)
                .style(button::text)
                .on_press_maybe(enabled.then_some(Message::ContextMenu(action)))
        };
        let command = |label: &str, message: Message, enabled| {
            item(label.into(), Action::Run(Box::new(message)), enabled)
        };
        let mut entries = column![].spacing(1);
        match menu.page {
            Page::Main => {
                let selected = !self.selected.is_empty();
                let atoms = self.doc.atoms.iter().any(|a| self.selected.contains(&a.id));
                let bonds = self
                    .doc
                    .bonds
                    .iter()
                    .any(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b));
                let arrows = self
                    .doc
                    .arrows
                    .iter()
                    .any(|a| self.selected.contains(&a.id));
                if selected {
                    entries = entries.push(
                        container(
                            text(self.selection_summary())
                                .size(11)
                                .color(super::workspace::muted()),
                        )
                        .padding([5, 10]),
                    );
                    for (label, action) in [
                        ("Cut", Message::Copy(true)),
                        ("Copy", Message::Copy(false)),
                        ("Duplicate", Message::Duplicate),
                        ("Delete", Message::Delete),
                    ] {
                        entries = entries.push(command(label, action, true));
                    }
                } else {
                    entries = entries
                        .push(command("Undo", Message::Undo, self.history.can_undo()))
                        .push(command("Redo", Message::Redo, self.history.can_redo()));
                }
                entries = entries
                    .push(command("Paste", Message::Paste, !self.clipboard_busy))
                    .push(horizontal_line());
                if self.alignment_count() >= 2 {
                    entries = entries
                        .push(command(
                            "Align middles",
                            Message::Arrange(Arrange::AlignVertical),
                            true,
                        ))
                        .push(item(
                            "Align & distribute…".into(),
                            Action::Page(Page::Align),
                            true,
                        ));
                }
                if self.can_group() {
                    entries = entries.push(command("Group", Message::Group, true));
                }
                if !self.doc.outer_selected_groups(&self.selected).is_empty() {
                    entries = entries.push(command("Ungroup", Message::Ungroup, true));
                }
                if bonds {
                    entries = entries
                        .push(item("Bond style…".into(), Action::Page(Page::Bonds), true))
                        .push(command("Bond in front", Message::BondDepth(true), true))
                        .push(command("Bond behind", Message::BondDepth(false), true));
                }
                if arrows {
                    entries = entries
                        .push(command(
                            "Reverse arrow",
                            Message::ArrowAction(super::arrows::Action::Reverse),
                            true,
                        ))
                        .push(command(
                            "Straighten arrow",
                            Message::ArrowAction(super::arrows::Action::Straighten),
                            true,
                        ));
                }
                if atoms {
                    if self.atom_text_target().is_some() {
                        entries = entries.push(command(
                            "Edit atom label…  Enter",
                            Message::AtomText(super::atom_text::Action::Begin(None)),
                            true,
                        ));
                    }
                    let connected: Vec<_> =
                        reshiki::editing::groups(&self.doc, &self.doc.all_ids())
                            .into_iter()
                            .filter(|ids| {
                                ids.iter().any(|id| {
                                    self.selected.contains(id) && self.doc.atom(*id).is_some()
                                })
                            })
                            .flatten()
                            .collect();
                    entries = entries.push(command(
                        "Select molecule",
                        Message::Canvas(crate::canvas::Edit::Select(connected)),
                        true,
                    ));
                    entries = entries.push(item(
                        "Molecular properties…".into(),
                        Action::Properties(true),
                        true,
                    ));
                }
                if selected {
                    entries =
                        entries.push(item("Properties…".into(), Action::Properties(false), true));
                } else {
                    entries = entries
                        .push(command(
                            "Select all",
                            Message::SelectAll,
                            !self.doc.all_ids().is_empty(),
                        ))
                        .push(command("Fit drawing", Message::Fit, true));
                }
            }
            Page::Align => {
                entries = entries.push(item("‹ Alignment".into(), Action::Page(Page::Main), true));
                for (label, action) in [
                    ("Align left edges", Arrange::AlignLeft),
                    ("Align horizontal centers", Arrange::AlignHorizontal),
                    ("Align right edges", Arrange::AlignRight),
                    ("Align top edges", Arrange::AlignTop),
                    ("Align middles", Arrange::AlignVertical),
                    ("Align bottom edges", Arrange::AlignBottom),
                    ("Distribute horizontally", Arrange::DistributeHorizontal),
                    ("Distribute vertically", Arrange::DistributeVertical),
                ] {
                    entries = entries.push(command(
                        label,
                        Message::Arrange(action),
                        self.alignment_count() >= 2,
                    ));
                }
            }
            Page::Bonds => {
                entries = entries.push(item("‹ Bond style".into(), Action::Page(Page::Main), true));
                for preset in BondPreset::ALL {
                    entries = entries.push(command(
                        &preset.to_string(),
                        Message::ApplyBondPreset(preset),
                        true,
                    ));
                }
            }
        }
        let width = 232_f32.min((self.viewport.width - 12.).max(1.));
        let height = ((entries.children().len() as f32 * 30.) + 12.)
            .min((self.viewport.height - 12.).max(1.));
        let x = menu
            .position
            .x
            .min((self.viewport.width - width - 6.).max(6.))
            .max(6.);
        let y = menu
            .position
            .y
            .min((self.viewport.height - height - 6.).max(6.))
            .max(6.);
        let popup = container(scrollable(entries))
            .padding(5)
            .width(width)
            .max_height(height)
            .style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    color: Color::from_rgb8(192, 204, 201),
                    width: 1.,
                    radius: 7.into(),
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba8(20, 40, 35, 0.18),
                    offset: iced::Vector::new(0., 4.),
                    blur_radius: 12.,
                },
                ..Default::default()
            });
        stack![
            base,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(Message::ContextMenu(Action::Close))
            .on_right_press(Message::ContextMenu(Action::Close)),
            container(opaque(popup)).padding(iced::Padding {
                left: x,
                top: y,
                right: 0.,
                bottom: 0.
            })
        ]
        .into()
    }
}
