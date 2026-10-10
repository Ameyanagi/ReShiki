use super::{App, InspectorTab, Message};
use crate::canvas::layered::canvas;
use crate::canvas::{TemplateAnchorPreview, Tool};
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    document::{Document, Point},
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
    /// Shared by the context row, its overflow menu and the selection menu.
    pub(super) fn selected_join_commands(&self) -> [super::workspace::RowCommand; 2] {
        use super::{shortcuts, workspace::RowCommand};
        let atoms = self.tab.selected.iter().all(|id| {
            self.tab.doc.atom(*id).is_some_and(|a| {
                a.centroid.is_empty()
                    && a.attachment.is_none()
                    && !self
                        .tab
                        .doc
                        .abbreviations
                        .iter()
                        .any(|group| group.members.contains(id))
            })
        });
        let active = self.tab.cleanup.is_none() && self.tab.joining.is_none();
        [
            RowCommand {
                label: "Join",
                menu: "Join selected atoms / bonds",
                hint: "Share two attachment sites or combine three or more selected atoms",
                message: Message::Shortcut(shortcuts::Action::Join),
                enabled: active && atoms && self.tab.selected.len() >= 2,
            },
            RowCommand {
                label: "Merge atoms",
                menu: "Merge selected atoms",
                hint: "Combine three or more atoms into the first selected atom; surrounding atoms stay in place",
                message: Message::Shortcut(shortcuts::Action::MergeAtoms),
                enabled: active && atoms && self.tab.selected.len() >= 3,
            },
        ]
    }

    /// Use only selected chemical labels, falling back to the atom position when
    /// its label is hidden. Other objects and selection handles do not contribute.
    pub(super) fn merge_selected_atoms(&self) -> Result<(Document, Vec<u64>), String> {
        let mut bounds: Option<(Point, Point)> = None;
        for id in &self.tab.selected {
            let atom = self
                .tab
                .doc
                .atom(*id)
                .ok_or("Select only atoms to merge.")?;
            let (lo, hi) = reshiki::scene::atom_label_ink_bounds(atom, &self.tab.doc)
                .unwrap_or((atom.position, atom.position));
            bounds = Some(bounds.map_or((lo, hi), |(a, b)| {
                (
                    Point::new(a.x.min(lo.x), a.y.min(lo.y)),
                    Point::new(b.x.max(hi.x), b.y.max(hi.y)),
                )
            }));
        }
        let (lo, hi) = bounds.ok_or("Select at least three distinct atoms to merge.")?;
        let center = Point::new(
            ((f64::from(lo.x) + f64::from(hi.x)) / 2.) as f32,
            ((f64::from(lo.y) + f64::from(hi.y)) / 2.) as f32,
        );
        reshiki::joining::merge_atoms_at(&self.tab.doc, &self.tab.selected, center)
    }

    pub(super) fn cancel_join(&mut self) {
        if let Some(state) = self.tab.joining.take() {
            self.tab.selected = state
                .prepared
                .moving
                .into_iter()
                .filter(|id| self.tab.doc.all_ids().contains(id))
                .collect();
            self.tool = Tool::Select;
            self.tab.hover = None;
        }
    }
    pub(super) fn join_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin => {
                if self.tab.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.cancel_join();
                match Prepared::new(&self.tab.doc, &self.tab.selected) {
                    Ok(prepared) => {
                        let mode = Connection::Connect;
                        let anchor = self
                            .tab
                            .hover
                            .filter(|(_, epoch)| *epoch == self.tab.file_epoch)
                            .and_then(|(p, _)| {
                                prepared.fragment.nearest(p, 10. / self.tab.camera.zoom)
                            })
                            .map(Anchor::Atom)
                            .unwrap_or_else(|| prepared.default_anchor(mode));
                        self.tab.selected = prepared.moving.clone();
                        self.tab.joining = Some(State {
                            prepared,
                            anchor,
                            mode,
                            revision: self.tab.revision,
                            epoch: self.tab.file_epoch,
                        });
                        self.tool = Tool::Template;
                        self.palette = None;
                        self.inspector_open = true;
                        self.inspector_tab = InspectorTab::Properties;
                        self.tab.fit_to_view = false;
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
                if let Some(state) = &mut self.tab.joining
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
                if let Some(state) = &mut self.tab.joining {
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
        let Some(state) = &self.tab.joining else {
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
mod tests;
