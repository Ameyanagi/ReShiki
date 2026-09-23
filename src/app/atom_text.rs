use super::{
    App, Message,
    workspace::{control, muted},
};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, pick_list, row, stack, text, text_input,
};
use iced::{Element, Length, Task};
use reshiki::atom_text::{self, Mode};

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Option<u64>),
    Input(String),
    Mode(Mode),
    Apply,
    Cancel,
}
pub struct State {
    id: u64,
    epoch: u64,
    input: String,
    mode: Mode,
    error: Option<String>,
}
impl App {
    pub(super) fn atom_text_target(&self) -> Option<u64> {
        if let [id] = self.selected.as_slice()
            && self.doc.atom(*id).is_some()
        {
            return Some(*id);
        }
        self.doc
            .abbreviations
            .iter()
            .find(|g| {
                g.members.len() == self.selected.len()
                    && g.members.iter().all(|id| self.selected.contains(id))
            })
            .map(|g| g.anchor)
    }
    pub(super) fn atom_text_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin(id) => {
                if self.cleanup.is_some() || self.busy || !self.finish_inline(true) {
                    return Task::none();
                }
                let Some(id) = id.or_else(|| self.atom_text_target()) else {
                    self.status = "Select one atom, then press Enter to edit its label".into();
                    return Task::none();
                };
                let Some(atom) = self.doc.atom(id) else {
                    return Task::none();
                };
                if !atom.centroid.is_empty() {
                    self.status =
                        "This is a tracked attachment point. Edit the atom bonded to it instead."
                            .into();
                    return Task::none();
                }
                let group = self.doc.abbreviation(id);
                self.atom_text = Some(State {
                    id,
                    epoch: self.file_epoch,
                    input: group.map(|g| g.label.clone()).unwrap_or_else(|| {
                        atom.display
                            .variable
                            .clone()
                            .unwrap_or_else(|| atom.element.clone())
                    }),
                    mode: if group.is_some() {
                        Mode::Group
                    } else if atom.display.variable.is_some() {
                        Mode::Text
                    } else {
                        Mode::Auto
                    },
                    error: None,
                });
                self.context_menu = None;
                return Task::batch([
                    iced::widget::operation::focus("atom-text"),
                    iced::widget::operation::select_all("atom-text"),
                ]);
            }
            Action::Input(input) => {
                if let Some(state) = &mut self.atom_text {
                    state.input = input;
                    state.error = None;
                }
            }
            Action::Mode(mode) => {
                if let Some(state) = &mut self.atom_text {
                    state.mode = mode;
                    state.error = None;
                }
            }
            Action::Cancel => self.atom_text = None,
            Action::Apply => {
                let Some(state) = &self.atom_text else {
                    return Task::none();
                };
                let result = if state.epoch != self.file_epoch {
                    Err("The drawing changed. Cancel and select the atom again.".into())
                } else {
                    atom_text::apply(&self.doc, state.id, &state.input, state.mode)
                };
                match result {
                    Ok(doc) => {
                        let before = std::mem::replace(&mut self.doc, doc);
                        self.atom_text = None;
                        self.changed(before);
                    }
                    Err(error) => {
                        if let Some(state) = &mut self.atom_text {
                            state.error = Some(error);
                        }
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn with_atom_text<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(state) = &self.atom_text else {
            return base;
        };
        let mut content = column![
            text("Edit atom label").size(20),
            text_input("M, L, X, Boc, Fe…", &state.input)
                .id("atom-text")
                .padding(10)
                .size(18)
                .on_input(|s| Message::AtomText(Action::Input(s)))
                .on_submit(Message::AtomText(Action::Apply)),
            pick_list(
                [Mode::Auto, Mode::Text, Mode::Group],
                Some(state.mode),
                |mode| Message::AtomText(Action::Mode(mode))
            )
            .width(Length::Fill),
            text(atom_text::description(&state.input, state.mode))
                .size(12)
                .color(muted()),
        ]
        .spacing(12);
        if let Some(error) = &state.error {
            content = content.push(
                text(error)
                    .size(12)
                    .color(iced::Color::from_rgb8(175, 54, 54)),
            );
        }
        let popup = container(
            content.push(
                row![
                    Space::new().width(Length::Fill),
                    button("Cancel · Esc")
                        .on_press(Message::AtomText(Action::Cancel))
                        .style(control(false)),
                    button("Apply · Enter")
                        .on_press(Message::AtomText(Action::Apply))
                        .style(button::primary),
                ]
                .spacing(8),
            ),
        )
        .padding(22)
        .width(430)
        .style(|_| container::Style {
            background: Some(iced::Color::WHITE.into()),
            border: iced::Border {
                radius: 12.into(),
                width: 1.,
                color: iced::Color::from_rgb8(213, 224, 220),
            },
            ..Default::default()
        });
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(iced::Color::from_rgba8(20, 30, 30, 0.3).into()),
                            ..Default::default()
                        })
                )
                .on_press(Message::AtomText(Action::Cancel))
            ),
            container(opaque(popup)).center(Length::Fill)
        ]
        .into()
    }
}

// A modal draft must not let unhandled keyboard shortcuts edit the canvas or
// replace the document. Background work and close/save completion still run.
pub(super) fn background(message: &Message) -> bool {
    matches!(
        message,
        Message::Tick
            | Message::RefreshLabels
            | Message::EngineDone { .. }
            | Message::Viewport(_)
            | Message::Assistant(_)
            | Message::Updates(_)
            | Message::Close(_)
            | Message::Cancel
            | Message::Discard
            | Message::Saved(..)
            | Message::Opened(_)
            | Message::Exported(_)
            | Message::ClipboardRead { .. }
            | Message::ClipboardWritten { .. }
            | Message::Pictures(super::pictures::Action::Loaded(..))
            | Message::Printing(
                super::printing::Action::Prepared(..) | super::printing::Action::Finished(..)
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Edit, Tool};
    use reshiki::document::Point;

    #[test]
    fn atom_text_entry_preserves_bonds_is_undoable_and_cancel_is_nonmutating() -> Result<(), String>
    {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::new(0., 0.));
        let b = app.doc.add_atom("N", Point::new(80., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![a];
        let before = app.doc.clone();
        let _ = app.update(Message::AtomText(Action::Begin(None)));
        let _ = app.update(Message::AtomText(Action::Input("M".into())));
        let _ = app.update(Message::Delete);
        let _ = app.update(Message::New);
        assert_eq!(
            app.doc, before,
            "Shortcuts cannot mutate the drawing behind the editor"
        );
        let _ = app.update(Message::AtomText(Action::Apply));
        assert_eq!(
            app.doc
                .atom(a)
                .ok_or("Missing atom")?
                .display
                .variable
                .as_deref(),
            Some("M")
        );
        assert_eq!(app.doc.bonds, before.bonds);
        let named = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, named);
        app.tool = Tool::Text;
        let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
        assert!(app.atom_text.is_some());
        assert!(app.inline_text.is_none());
        let _ = app.update(Message::AtomText(Action::Input("L".into())));
        let _ = app.update(Message::Escape);
        assert_eq!(app.doc, named);
        assert!(app.atom_text.is_none());
        let _ = app.update(Message::Canvas(Edit::Click(Point::new(300., 300.))));
        assert!(
            app.inline_text.is_some(),
            "Empty space still creates a caption"
        );
        Ok(())
    }

    #[test]
    fn label_errors_and_stale_documents_retain_the_draft() -> Result<(), String> {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::default());
        app.selected = vec![a];
        let _ = app.update(Message::AtomText(Action::Begin(None)));
        let before = app.doc.clone();
        let _ = app.update(Message::AtomText(Action::Input("".into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(
            app.atom_text
                .as_ref()
                .ok_or("Missing draft")?
                .error
                .is_some()
        );
        assert_eq!(app.doc, before);
        let _ = app.update(Message::AtomText(Action::Input("X".into())));
        app.file_epoch += 1;
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(
            app.atom_text
                .as_ref()
                .ok_or("Missing draft")?
                .error
                .is_some()
        );
        assert_eq!(app.doc, before);
        Ok(())
    }
}
