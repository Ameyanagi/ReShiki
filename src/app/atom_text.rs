use super::{
    App, Message,
    workspace::{control, muted_text},
};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, stack, text};
use iced::{Element, Length, Task};
use reshiki::abbreviations::LabelAlignment;
use reshiki::atom_text::{self, Mode};

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Option<u64>),
    Input(String),
    Mode(Mode),
    ContractSelection,
    ReverseInput(String),
    Apply,
    Cancel,
}
pub struct State {
    id: u64,
    epoch: u64,
    input: String,
    mode: Mode,
    label_edited: bool,
    alignment: LabelAlignment,
    members: Option<Vec<u64>>,
    reverse: String,
    reverse_edited: bool,
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
                if !atom.centroid.is_empty() && self.doc.abbreviation(id).is_none() {
                    self.status =
                        "This is a tracked attachment point. Edit the atom bonded to it instead."
                            .into();
                    return Task::none();
                }
                let group = self.doc.abbreviation(id);
                self.atom_text = Some(State {
                    id,
                    epoch: self.file_epoch,
                    input: group
                        .map(|g| g.label.clone())
                        .unwrap_or_else(|| atom_text::entry(atom)),
                    mode: if group.is_some() {
                        Mode::Group
                    } else if atom.display.variable.is_some() {
                        Mode::Text
                    } else {
                        Mode::Auto
                    },
                    error: None,
                    label_edited: false,
                    alignment: group.map(|g| g.alignment).unwrap_or_default(),
                    members: None,
                    reverse: group.map(|g| g.reverse_label.clone()).unwrap_or_default(),
                    reverse_edited: false,
                });
                self.context_menu = None;
                return Task::batch([
                    iced::widget::operation::focus("atom-text"),
                    iced::widget::operation::select_all("atom-text"),
                ]);
            }
            Action::Input(input) => {
                if let Some(state) = &mut self.atom_text {
                    state.label_edited |= state.input != input;
                    state.input = input;
                    state.error = None;
                }
            }
            Action::ContractSelection => {
                if self.cleanup.is_some() || self.busy || !self.finish_inline(true) {
                    return Task::none();
                }
                let mut candidate = self.doc.clone();
                match candidate.contract(&self.selected, "Group", "") {
                    Ok(()) => {
                        if let Some(group) = candidate.abbreviations.last() {
                            self.atom_text = Some(State {
                                id: group.anchor,
                                epoch: self.file_epoch,
                                input: String::new(),
                                mode: Mode::Group,
                                label_edited: true,
                                alignment: LabelAlignment::Auto,
                                members: Some(group.members.clone()),
                                reverse: String::new(),
                                reverse_edited: false,
                                error: None,
                            });
                            self.context_menu = None;
                            return iced::widget::operation::focus("atom-text");
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::ReverseInput(value) => {
                if let Some(state) = &mut self.atom_text {
                    state.reverse = value;
                    state.reverse_edited = true;
                    state.error = None;
                }
            }
            Action::Mode(mode) => {
                if let Some(state) = &mut self.atom_text {
                    state.label_edited |= state.mode != mode;
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
                } else if !state.label_edited {
                    Ok(self.doc.clone())
                } else if let Some(members) = &state.members {
                    let mut doc = self.doc.clone();
                    doc.contract(members, &state.input, &state.reverse)
                        .map(|()| doc)
                } else {
                    atom_text::apply(&self.doc, state.id, &state.input, state.mode)
                };
                match result {
                    Ok(mut doc) => {
                        if let Some(group) =
                            doc.abbreviations.iter_mut().find(|g| g.anchor == state.id)
                        {
                            group.alignment = state.alignment;
                            if state.members.is_some()
                                || state.reverse_edited
                                || self
                                    .doc
                                    .abbreviation(state.id)
                                    .is_some_and(|g| g.label == group.label)
                            {
                                group.reverse_label = state.reverse.trim().into();
                            }
                        }
                        if let Err(error) = doc.validate() {
                            if let Some(state) = &mut self.atom_text {
                                state.error = Some(error);
                            }
                            return Task::none();
                        }
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
            text(if state.members.is_some() {
                "Create group label"
            } else {
                "Edit atom label"
            })
            .size(20),
            crate::appearance::text_input("N, NH3, C2H5, Boc, Cp*, M…", &state.input)
                .id("atom-text")
                .padding(10)
                .size(18)
                .on_input(|s| Message::AtomText(Action::Input(s)))
                .on_submit(Message::AtomText(Action::Apply)),
        ]
        .spacing(12);
        if state.members.is_none() {
            content = content.push(
                crate::appearance::pick_list(
                    [Mode::Auto, Mode::Text, Mode::Group],
                    Some(state.mode),
                    |mode| Message::AtomText(Action::Mode(mode)),
                )
                .width(Length::Fill),
            );
        }
        content = content.push(text(if state.members.is_some() {
                "The selected atoms define this group. Its formula and bonds are retained; expand it to edit the structure."
            } else { atom_text::description(&state.input, state.mode) })
                .size(12)
                .style(muted_text));
        if state.members.is_some() || self.doc.abbreviation(state.id).is_some() {
            content = content.push(
                crate::appearance::text_input("Label when facing left (optional)", &state.reverse)
                    .on_input(|s| Message::AtomText(Action::ReverseInput(s)))
                    .on_submit(Message::AtomText(Action::Apply))
                    .padding(8),
            );
        }
        if let Some(error) = &state.error {
            content = content.push(text(error).size(12).style(crate::appearance::text_color(
                iced::Color::from_rgb8(175, 54, 54),
            )));
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
                        .style(crate::appearance::primary),
                ]
                .spacing(8),
            ),
        )
        .padding(22)
        .width(430)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(iced::Color::WHITE.into()),
                    border: iced::Border {
                        radius: 12.into(),
                        width: 1.,
                        color: iced::Color::from_rgb8(213, 224, 220),
                    },
                    ..Default::default()
                },
            )
        });
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|theme| crate::appearance::container(
                            theme,
                            container::Style {
                                background: Some(iced::Color::from_rgba8(20, 30, 30, 0.3).into()),
                                ..Default::default()
                            }
                        ))
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
            | Message::FigureExported(_)
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
    fn accepting_an_unchanged_label_preserves_imported_atom_properties() -> Result<(), String> {
        for element in ["H", "N", "Pt"] {
            let (mut app, _) = App::new();
            let id = app.doc.add_atom(element, Point::default());
            let atom = app.doc.atom_mut(id).ok_or("Atom")?;
            atom.no_implicit = true;
            atom.charge = 1;
            atom.isotope = 2;
            let before = app.doc.clone();
            let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
            if element == "H" {
                assert_eq!(app.atom_text.as_ref().ok_or("Draft")?.input, "H");
            }
            let _ = app.update(Message::AtomText(Action::Apply));
            assert_eq!(app.doc, before);
        }
        Ok(())
    }

    #[test]
    fn explicit_label_draft_and_history_retain_the_entered_count() -> Result<(), String> {
        let (mut app, _) = App::new();
        let pt = app.doc.add_atom("Pt", Point::default());
        let n = app.doc.add_atom("N", Point::new(42., 0.));
        app.doc.add_bond(n, pt, 1, "plain");
        let _ = app.update(Message::AtomText(Action::Begin(Some(n))));
        let _ = app.update(Message::AtomText(Action::Input("NH3".into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert_eq!(app.doc.atom(n).ok_or("N")?.explicit_h, 3);
        let _ = app.update(Message::AtomText(Action::Begin(Some(n))));
        assert_eq!(app.atom_text.as_ref().ok_or("Draft")?.input, "NH3");
        let _ = app.update(Message::AtomText(Action::Cancel));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc.atom(n).ok_or("N")?.explicit_h, 0);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc.atom(n).ok_or("N")?.explicit_h, 3);
        Ok(())
    }

    #[test]
    fn defining_and_renaming_a_group_keeps_chemistry_and_invalid_drafts() -> Result<(), String> {
        let (mut app, _) = App::new();
        let n = app.doc.add_atom("N", Point::default());
        let a = app.doc.add_atom("C", Point::new(42., 0.));
        let b = app.doc.add_atom("O", Point::new(84., 0.));
        app.doc.add_bond(n, a, 1, "plain");
        app.doc.add_bond(a, b, 1, "plain");
        let original = app.doc.clone();
        app.selected = vec![a, b];
        let _ = app.update(Message::ContextKey("Enter".into()));
        assert!(
            app.atom_text
                .as_ref()
                .ok_or("Group draft")?
                .members
                .is_some()
        );
        let _ = app.update(Message::AtomText(Action::Input("Custom".into())));
        let _ = app.update(Message::AtomText(Action::ReverseInput("LeftCustom".into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(app.atom_text.is_none());
        assert_eq!(app.doc.atoms, original.atoms);
        assert_eq!(app.doc.bonds, original.bonds);
        assert_eq!(
            app.doc.abbreviation(a).ok_or("Group")?.reverse_label,
            "LeftCustom"
        );
        let _ = app.update(Message::AtomText(Action::Begin(None)));
        let _ = app.update(Message::AtomText(Action::ReverseInput("a".repeat(33))));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(
            app.atom_text
                .as_ref()
                .ok_or("Invalid draft must remain")?
                .error
                .is_some()
        );
        assert_eq!(
            app.doc.abbreviation(a).ok_or("Group")?.reverse_label,
            "LeftCustom"
        );
        let _ = app.update(Message::AtomText(Action::Cancel));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc.abbreviations.len(), 1);
        Ok(())
    }

    #[test]
    fn label_dialog_preserves_toolbar_alignment_and_chemical_graph() -> Result<(), String> {
        for label in ["Boc", "Cp*"] {
            let (mut app, _) = App::new();
            let id = app.doc.add_atom("C", Point::default());
            app.doc = reshiki::atom_text::apply(&app.doc, id, label, Mode::Group)?;
            app.selected = vec![id];
            let before = app.doc.clone();
            for alignment in LabelAlignment::ALL {
                let _ = app.update(Message::GroupLabelAlign(alignment));
                let _ = app.update(Message::AtomText(Action::Begin(None)));
                let _ = app.update(Message::AtomText(Action::Apply));
                assert_eq!(
                    app.doc.abbreviation(id).ok_or("group")?.alignment,
                    alignment
                );
                assert_eq!(app.doc.atoms, before.atoms);
                assert_eq!(app.doc.bonds, before.bonds);
            }
            let _ = app.update(Message::Undo);
            assert_eq!(
                app.doc.abbreviation(id).ok_or("group")?.alignment,
                LabelAlignment::Right
            );
            let _ = app.update(Message::Redo);
            assert_eq!(
                app.doc.abbreviation(id).ok_or("group")?.alignment,
                LabelAlignment::Above
            );
            assert_eq!(app.doc.atoms, before.atoms);
        }
        Ok(())
    }

    #[test]
    fn collapsed_haptic_groups_can_be_edited_and_undone() -> Result<(), String> {
        let (mut app, _) = App::new();
        let end = app.doc.add_atom("C", Point::default());
        app.doc = reshiki::atom_text::apply(&app.doc, end, "Cp*", Mode::Auto)?;
        app.selected = app
            .doc
            .abbreviation(end)
            .ok_or("Missing group")?
            .members
            .clone();
        let before = app.doc.clone();
        let _ = app.update(Message::AtomText(Action::Begin(None)));
        assert_eq!(app.atom_text.as_ref().ok_or("Missing editor")?.input, "Cp*");
        let _ = app.update(Message::AtomText(Action::Input("Cp".into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(app.atom_text.is_none());
        assert_eq!(app.doc.atoms.iter().filter(|a| a.element == "C").count(), 5);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc.atoms.iter().filter(|a| a.element == "C").count(), 5);
        Ok(())
    }

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
