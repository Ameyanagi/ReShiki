use super::{App, InspectorTab, Message};
use crate::canvas::layered::canvas;
use crate::canvas::pick::PICK_RADIUS_PX;
use crate::canvas::{TemplateAnchorPreview, Tool};
use iced::widget::{column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    document::{Document, Point},
    joining::{Coordination, JoinPlan, Prepared},
    templates::{Anchor, Connection},
};

#[derive(Debug, Clone)]
pub enum Action {
    Begin,
    BeginCoordination,
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
impl State {
    fn is_coordination(&self) -> bool {
        self.mode == Connection::Coordinate
    }
}

const CONNECT_DETAIL: &str =
    "Click a destination atom to add a single bond. Drag from that atom to set the direction.";
const SHARE_ATOM_DETAIL: &str = concat!(
    "Click a matching destination atom to merge the two atoms. ",
    "Drag from it to choose the orientation."
);
const FUSE_BOND_DETAIL: &str =
    "Click a matching destination bond to share its two atoms. Drag from it to choose the side.";
const COORDINATION_DETAIL: &str = concat!(
    "Click a transition-metal atom to add a directed donor → metal contact. ",
    "Positions, donor hydrogens and metal charge are retained, ",
    "including within an existing chelate."
);
const COORDINATION_HINT: &str = concat!(
    "No atoms move. Escape cancels. Each new contact is one Undo step. ",
    "Wedge/hash styling depicts projection; it does not assign complex stereochemistry."
);
const JOIN_HINT: &str =
    "Shift/Ctrl drag snaps the direction. Escape cancels. Joining is one Undo step.";

struct JoinModeTexts {
    panel_title: &'static str,
    instruction: &'static str,
    footer: &'static str,
    bar_title: &'static str,
    bar_subtitle: &'static str,
}

fn join_mode_texts(mode: Connection) -> JoinModeTexts {
    let instruction = match mode {
        Connection::Connect | Connection::Auto => CONNECT_DETAIL,
        Connection::ShareAtom => SHARE_ATOM_DETAIL,
        Connection::FuseBond => FUSE_BOND_DETAIL,
        Connection::Coordinate => COORDINATION_DETAIL,
    };
    let coordination = mode == Connection::Coordinate;
    let title = if coordination {
        "Coordinate in place"
    } else {
        "Move & attach"
    };
    JoinModeTexts {
        panel_title: title,
        instruction,
        footer: if coordination {
            COORDINATION_HINT
        } else {
            JOIN_HINT
        },
        bar_title: title,
        bar_subtitle: if coordination {
            "Choose a metal · Positions stay fixed"
        } else {
            "Choose a destination · Drag to orient"
        },
    }
}

impl App {
    /// Before committing style/text drafts: inspect only actual Join routes.
    /// Pure model guards remain authoritative for direct callers and previews.
    pub(super) fn generic_join_rejection(&self, message: &Message) -> bool {
        if self.tab.atom_text.is_some() || self.tab.cleanup.is_some() {
            return false;
        }
        let mut message = message;
        while let Message::ContextMenu(super::context_menu::Action::Run(inner)) = message {
            message = inner;
        }
        if let Message::Canvas(crate::canvas::Edit::Template(point, _)) = message {
            return self.tab.joining.as_ref().is_some_and(|state| {
                state.revision == self.tab.revision
                    && state.epoch == self.tab.file_epoch
                    && state.prepared.generic_rejection(
                        *point,
                        self.tab.camera.world_len(PICK_RADIUS_PX),
                        state.anchor,
                        state.mode,
                    )
            });
        }
        let doc = &self.tab.doc;
        // A pending preparation is cancelled by these initiating commands;
        // cancel_join restores exactly these moving IDs as the selection.
        let selected = self
            .tab
            .joining
            .as_ref()
            .map_or(self.tab.selected.as_slice(), |state| {
                state.prepared.moving.as_slice()
            });
        if let Message::Join(Action::Begin | Action::BeginCoordination) = message {
            return !selected.is_empty()
                && selected
                    .iter()
                    .all(|id| doc.object_ids().any(|object| object == *id))
                && reshiki::joining::source_intersects_generic(doc, selected);
        }
        let Message::Shortcut(
            action @ (super::shortcuts::Action::Join | super::shortcuts::Action::MergeAtoms),
        ) = message
        else {
            return false;
        };
        if selected.iter().any(|id| doc.atom(*id).is_none())
            || selected
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != selected.len()
        {
            return false;
        }
        let plan = if matches!(action, super::shortcuts::Action::MergeAtoms) {
            (selected.len() >= 3).then_some(JoinPlan::Merge)
        } else {
            reshiki::joining::join_plan(doc, selected)
        };
        plan.is_some_and(|plan| plan.generic_blocked(doc, selected))
    }

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
        if reshiki::generic::touches(&self.tab.doc, &self.tab.selected) {
            return Err(reshiki::generic::JOIN_NOTICE.into());
        }
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
            begin @ (Action::Begin | Action::BeginCoordination) => {
                if self.tab.cleanup.is_some() {
                    return Task::none();
                }
                // Reject before committing an inline draft or cancelling an
                // existing preparation, either of which can change editor state.
                if reshiki::joining::source_intersects_generic(&self.tab.doc, &self.tab.selected) {
                    self.status = reshiki::generic::JOIN_NOTICE.into();
                    self.error = true;
                    return Task::none();
                }
                if !self.finish_inline(true) {
                    return Task::none();
                }
                self.cancel_join();
                let mode = if matches!(begin, Action::BeginCoordination) {
                    Connection::Coordinate
                } else {
                    Connection::Connect
                };
                match Prepared::with_mode(&self.tab.doc, &self.tab.selected, mode) {
                    Ok(prepared) => {
                        let selected_donor = (mode == Connection::Coordinate)
                            .then(|| {
                                self.tab.selected.iter().copied().find(|id| {
                                    prepared
                                        .fragment
                                        .atom(*id)
                                        .is_some_and(reshiki::joining::coordination_donor)
                                })
                            })
                            .flatten();
                        let anchor = selected_donor
                            .or_else(|| {
                                self.tab
                                    .hover
                                    .filter(|(_, epoch)| *epoch == self.tab.file_epoch)
                                    .and_then(|(p, _)| {
                                        prepared
                                            .fragment
                                            .nearest(p, self.tab.camera.world_len(PICK_RADIUS_PX))
                                    })
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
                        let status = if mode == Connection::Coordinate {
                            "Choose the donor N/O/S/P in the preview, then click the metal. All positions are retained."
                        } else {
                            "Choose the source atom or bond in the preview, then its destination on the canvas"
                        };
                        self.status = status.into();
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
                        if state.mode == Connection::Coordinate {
                            self.status = "Coordination requires a donor atom, not a bond.".into();
                            self.error = true;
                            state.anchor = state.prepared.default_anchor(state.mode);
                            return Task::none();
                        }
                        state.mode = Connection::FuseBond;
                    } else if state.mode == Connection::FuseBond {
                        state.mode = Connection::Connect;
                    }
                }
            }
            Action::Mode(mode) => {
                if let Some(state) = &mut self.tab.joining {
                    if (state.mode == Connection::Coordinate) != (mode == Connection::Coordinate) {
                        match Prepared::with_mode(
                            &state.prepared.original,
                            &state.prepared.moving,
                            mode,
                        ) {
                            Ok(prepared) => state.prepared = prepared,
                            Err(error) => {
                                self.status = error;
                                self.error = true;
                                return Task::none();
                            }
                        }
                    }
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
        let coordination = state.is_coordination();
        let texts = join_mode_texts(state.mode);
        let preview: Element<'_, Anchor> = canvas(TemplateAnchorPreview {
            document: &state.prepared.fragment,
            anchor: state.anchor,
        })
        .width(Length::Fill)
        .height(200)
        .into();
        column![
            text(texts.panel_title).size(18),
            text("Choose the atom or bond on this fragment to use as its attachment point.")
                .size(12)
                .style(super::workspace::muted_text),
            crate::appearance::pick_list(
                [
                    Connection::Connect,
                    Connection::ShareAtom,
                    Connection::FuseBond,
                    Connection::Coordinate
                ],
                Some(state.mode),
                |mode| Message::Join(Action::Mode(mode))
            )
            .text_size(12)
            .width(Length::Fill),
            reshiki::accessibility::button(
                "coordination.mode",
                "Coordinate in place: donor to metal",
                text("Coordinate in place").size(12)
            )
            .checked(coordination)
            .style(super::workspace::control(coordination))
            .on_press(Message::Join(Action::Mode(Connection::Coordinate))),
            preview.map(|anchor| Message::Join(Action::Anchor(anchor))),
            text(state.anchor.to_string())
                .size(11)
                .style(super::workspace::muted_text),
            text(texts.instruction).size(12),
            text(texts.footer)
                .size(11)
                .style(super::workspace::muted_text),
            reshiki::accessibility::button(
                "coordination.cancel",
                "Cancel attachment",
                text("Cancel")
            )
            .on_press(Message::Join(Action::Cancel))
            .style(super::workspace::control(false))
        ]
        .spacing(12)
        .into()
    }
    pub(super) fn join_bar(&self) -> Element<'_, Message> {
        let coordination = self
            .tab
            .joining
            .as_ref()
            .is_some_and(State::is_coordination);
        let texts = join_mode_texts(if coordination {
            Connection::Coordinate
        } else {
            Connection::Connect
        });
        container(
            row![
                text(texts.bar_title).size(12),
                text(texts.bar_subtitle)
                    .size(11)
                    .style(super::workspace::muted_text),
                reshiki::accessibility::button(
                    "coordination.cancel-bar",
                    "Cancel attachment",
                    text("Cancel")
                )
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

/// The canvas status for a coordination placement that changed the drawing.
pub(super) const CONTACT_ADDED: &str = "Donor → metal contact added in place · Undo removes it";

pub(super) fn coordination_status(outcome: &Coordination, marked: bool) -> &'static str {
    match (outcome, marked) {
        (Coordination::Added(_), false) => CONTACT_ADDED,
        (Coordination::AlreadyPresent, false) => "This donor is already coordinated to that metal",
        (Coordination::Added(_), true) => {
            "Marked donor → metal contact added in place · Undo removes it"
        }
        (Coordination::AlreadyPresent, true) => {
            "This marked donor is already coordinated to that metal"
        }
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
mod generic_tests;
#[cfg(test)]
mod tests;
