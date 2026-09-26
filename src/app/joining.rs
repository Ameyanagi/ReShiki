use super::{App, InspectorTab, Message};
use crate::canvas::layered::canvas;
use crate::canvas::{TemplateAnchorPreview, Tool};
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    joining::Prepared,
    templates::{Anchor, Connection},
};

#[derive(Debug, Clone)]
pub enum Action {
    Begin,
    Cancel,
    Anchor(Anchor),
    Mode(Connection),
}
pub struct State {
    pub prepared: Prepared,
    pub anchor: Anchor,
    pub mode: Connection,
    pub revision: u64,
    pub epoch: u64,
}
impl App {
    pub(super) fn cancel_join(&mut self) {
        if let Some(state) = self.joining.take() {
            self.selected = state
                .prepared
                .moving
                .into_iter()
                .filter(|id| self.doc.all_ids().contains(id))
                .collect();
            self.tool = Tool::Select;
            self.hover = None;
        }
    }
    pub(super) fn join_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin => {
                if self.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.cancel_join();
                match Prepared::new(&self.doc, &self.selected) {
                    Ok(prepared) => {
                        let mode = Connection::Connect;
                        let anchor = self
                            .hover
                            .filter(|(_, epoch)| *epoch == self.file_epoch)
                            .and_then(|(p, _)| prepared.fragment.nearest(p, 10. / self.camera.zoom))
                            .map(Anchor::Atom)
                            .unwrap_or_else(|| prepared.default_anchor(mode));
                        self.selected = prepared.moving.clone();
                        self.joining = Some(State {
                            prepared,
                            anchor,
                            mode,
                            revision: self.revision,
                            epoch: self.file_epoch,
                        });
                        self.tool = Tool::Template;
                        self.palette = None;
                        self.inspector_open = true;
                        self.inspector_tab = InspectorTab::Properties;
                        self.fit_to_view = false;
                        self.status = "Choose the source atom or bond in the preview, then its destination on the canvas".into();
                        self.error = false;
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Cancel => {
                self.cancel_join();
                self.error = false;
                self.status = "Move & attach cancelled".into();
            }
            Action::Anchor(anchor) => {
                if let Some(state) = &mut self.joining
                    && anchor.valid(&state.prepared.fragment)
                {
                    state.anchor = anchor;
                    if matches!(anchor, Anchor::Bond(..)) {
                        state.mode = Connection::FuseBond;
                    } else if state.mode == Connection::FuseBond {
                        state.mode = Connection::Connect;
                    }
                }
            }
            Action::Mode(mode) => {
                if let Some(state) = &mut self.joining {
                    state.mode = mode;
                    if (mode == Connection::FuseBond) != matches!(state.anchor, Anchor::Bond(..)) {
                        state.anchor = state.prepared.default_anchor(mode);
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn join_panel(&self) -> Element<'_, Message> {
        let Some(state) = &self.joining else {
            return text("Select a fragment to attach").into();
        };
        let preview: Element<'_, Anchor> = canvas(TemplateAnchorPreview {
            document: &state.prepared.fragment,
            anchor: state.anchor,
        })
        .width(Length::Fill)
        .height(200)
        .into();
        column![
            text("Move & attach").size(18),
            text("Choose the atom or bond on this fragment to use as its attachment point.").size(12).style(super::workspace::muted_text),
            crate::appearance::pick_list([Connection::Connect,Connection::ShareAtom,Connection::FuseBond], Some(state.mode), |mode| Message::Join(Action::Mode(mode))).text_size(12).width(Length::Fill),
            preview.map(|anchor| Message::Join(Action::Anchor(anchor))),
            text(state.anchor.to_string()).size(11).style(super::workspace::muted_text),
            text(match state.mode {
                Connection::Connect | Connection::Auto => "Click a destination atom to add a single bond. Drag from that atom to set the direction.",
                Connection::ShareAtom => "Click a matching destination atom to merge the two atoms. Drag from it to choose the orientation.",
                Connection::FuseBond => "Click a matching destination bond to share its two atoms. Drag from it to choose the side."
            }).size(12),
            text("Shift/Ctrl drag snaps the direction. Escape cancels. Joining is one Undo step.").size(11).style(super::workspace::muted_text),
            button("Cancel move").on_press(Message::Join(Action::Cancel)).style(super::workspace::control(false))
        ].spacing(12).into()
    }
    pub(super) fn join_bar(&self) -> Element<'_, Message> {
        container(
            row![
                text("Move & attach").size(12),
                text("Choose a destination · Drag to orient")
                    .size(11)
                    .style(super::workspace::muted_text),
                button("Cancel")
                    .on_press(Message::Join(Action::Cancel))
                    .style(super::workspace::control(false))
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .height(46)
        .padding([5, 14])
        .center_y(46)
        .style(super::workspace::panel)
        .into()
    }
}

pub(super) fn cancels_draft(message: &Message) -> bool {
    (super::inline_text::commits_draft(message) && !matches!(message, Message::Canvas(_)))
        || matches!(
            message,
            Message::TemplateNavigate(_)
                | Message::Undo
                | Message::Redo
                | Message::TextStyle(_)
                | Message::ApplyFontSize
                | Message::ApplyTextColor
                | Message::TextAlign(_)
                | Message::TextSpacing(_)
                | Message::ApplyTextWidth
                | Message::CaptionAction(_)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Edit;
    use reshiki::document::{Document, Point};

    fn ready() -> App {
        let (mut app, _) = App::new();
        app.busy = false;
        app.doc = Document::default();
        app.doc.add_atom("C", Point::default());
        let source = app.doc.add_atom("C", Point::new(180., 0.));
        let end = app.doc.add_atom("C", Point::new(222., 0.));
        app.doc.add_bond(source, end, 1, "plain");
        app.selected = vec![source];
        app
    }
    #[test]
    fn joining_uses_the_preview_and_is_one_undo_step() {
        let mut app = ready();
        let before = app.doc.clone();
        let _ = app.update(Message::Join(Action::Begin));
        assert_eq!(app.doc, before);
        let state = app.joining.as_ref().unwrap();
        let (expected, _) = state
            .prepared
            .place(
                Point::default(),
                None,
                10. / app.camera.zoom,
                state.anchor,
                state.mode,
            )
            .unwrap();
        let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
        assert!(app.joining.is_none());
        assert_eq!(app.doc, expected);
        assert_eq!(app.doc.bonds.len(), 2);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, expected);
    }
    #[test]
    fn cancel_switching_tools_and_invalid_targets_keep_the_original() {
        let mut app = ready();
        let before = app.doc.clone();
        let _ = app.update(Message::Join(Action::Begin));
        let _ = app.update(Message::Canvas(Edit::Template(
            Point::new(500., 500.),
            None,
        )));
        assert!(app.joining.is_some());
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Escape);
        assert!(app.joining.is_none());
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::Join(Action::Begin));
        let _ = app.update(Message::Tool(Tool::Bond(2)));
        assert!(app.joining.is_none());
        assert_eq!(app.tool, Tool::Bond(2));
        assert_eq!(app.doc, before);
    }
    #[test]
    fn a_changed_document_cannot_be_overwritten_by_a_prepared_join() {
        let mut app = ready();
        let _ = app.update(Message::Join(Action::Begin));
        let before = app.doc.clone();
        app.doc.add_atom("O", Point::new(300., 100.));
        app.changed(before);
        let changed = app.doc.clone();
        let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
        assert!(app.joining.is_none());
        assert_eq!(app.doc, changed);
        assert!(app.error);
    }
    #[test]
    fn anchor_picker_switches_between_atom_and_bond_modes() {
        let mut app = ready();
        let _ = app.update(Message::Join(Action::Begin));
        let _ = app.update(Message::Join(Action::Anchor(Anchor::Bond(2, 3))));
        assert_eq!(app.joining.as_ref().unwrap().mode, Connection::FuseBond);
        let _ = app.update(Message::Join(Action::Mode(Connection::ShareAtom)));
        assert!(matches!(
            app.joining.as_ref().unwrap().anchor,
            Anchor::Atom(_)
        ));
        let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(3))));
        assert_eq!(app.joining.as_ref().unwrap().anchor, Anchor::Atom(3));
    }
}
