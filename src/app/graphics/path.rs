//! Contextual pen authoring and node edits.
use super::super::workspace::{accessible_command, text_width};
use super::*;
use iced::widget::{Column, column, row, text, tooltip};
use reshiki::graphics::{Graphic, PathHandle, PathNodeType, PathSummary};

/// The block's one line of help; the full gestures are in the guide.
const DRAWING_HINT: &str = "Click adds a line · Drag adds a curve · Click the first node to close";
/// Shown instead while a direction control, not a node, is selected.
const CONTROL_HINT: &str = "Direction control selected · Pick a node above";

#[derive(Debug, Clone, Copy)]
pub enum Action {
    New,
    Finish,
    Continue,
    Node(usize),
    Insert,
    Delete,
    Straight,
    Curved,
    Open,
    Close,
    NodeType(PathNodeType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Choice {
    ordinal: usize,
    index: usize,
}
/// A pen command, two to a row.
fn path_button(
    id: &'static str,
    label: &'static str,
) -> reshiki::accessibility::Button<'static, Message> {
    accessible_command(id, label, label)
}
/// A row label, as wide as the widest one so that the controls line up.
fn path_label(label: &'static str) -> Element<'static, Message> {
    let width = [SELECTED_NODE, NEXT_SEGMENT]
        .map(|label| text_width(label, 11.))
        .into_iter()
        .fold(0., f32::max)
        + 2.;
    text(label).size(11).width(width).into()
}
const SELECTED_NODE: &str = "Selected node";
const NEXT_SEGMENT: &str = "Next segment";
fn path_hint(hint: &'static str) -> Element<'static, Message> {
    text(hint).size(11).style(muted_text).into()
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node {}", self.ordinal)
    }
}
impl App {
    pub(in crate::app) fn selected_path(&self) -> Option<(&Graphic, PathSummary)> {
        let [id] = self.tab.selected.as_slice() else {
            return None;
        };
        let graphic = self.tab.doc.graphics.iter().find(|g| g.id == *id)?;
        Some((graphic, graphic.path_summary()?))
    }
    fn edit_points_node(&self) -> Option<(u64, usize, PathNodeType)> {
        let (graphic, summary) = self.selected_path()?;
        let (id, index) = self.tab.path_point.filter(|(id, _)| *id == graphic.id)?;
        let index = graphic.path_node_index(index)?;
        let ordinal = summary.nodes().position(|node| node.index == index)?;
        Some((id, index, *graphic.path_node_types()?.get(ordinal)?))
    }
    pub(in crate::app) fn edit_points_node_options(&self) -> Vec<Element<'_, Message>> {
        if self.selected_path().is_none() {
            return vec![];
        }
        let selected = self.edit_points_node();
        [
            (
                "edit-points-corner",
                "Corner",
                PathNodeType::Corner,
                "Move directions independently",
            ),
            (
                "edit-points-smooth",
                "Smooth",
                PathNodeType::Smooth,
                "Keep directions in line",
            ),
            (
                "edit-points-symmetric",
                "Symmetric",
                PathNodeType::Symmetric,
                "Keep directions equal and opposite",
            ),
        ]
        .into_iter()
        .map(|(id, label, kind, hint)| {
            hover_hint(
                accessible_command(id, label, label)
                    .on_press_maybe(
                        selected.map(|_| {
                            Message::Graphics(super::Action::Path(Action::NodeType(kind)))
                        }),
                    )
                    .style(control(
                        selected.is_some_and(|(_, _, current)| current == kind),
                    )),
                hint,
                tooltip::Position::Bottom,
            )
            .into()
        })
        .collect()
    }
    /// The canvas sends individual releases because rebuilding it resets its click state.
    pub(in crate::app) fn path_node_click(
        &mut self,
        id: u64,
        index: usize,
        now: iced::time::Instant,
    ) {
        self.tab.path_point = Some((id, index));
        let Some((id, index, kind)) = self.edit_points_node() else {
            return;
        };
        let target = (self.tab.id, id, index);
        let double = self.path_click.is_some_and(|(time, last)| {
            last == target && now.duration_since(time) < std::time::Duration::from_millis(450)
        });
        self.path_click = (!double).then_some((now, target));
        if double {
            self.path_action(Action::NodeType(match kind {
                PathNodeType::Corner => PathNodeType::Smooth,
                PathNodeType::Smooth => PathNodeType::Symmetric,
                PathNodeType::Symmetric => PathNodeType::Corner,
            }));
        }
    }
    fn path_point(&self, graphic: &Graphic, handles: &[PathHandle]) -> Option<usize> {
        self.tab
            .path_point
            .filter(|(id, index)| *id == graphic.id && handles.iter().any(|h| h.index == *index))
            .map(|(_, index)| index)
            .or_else(|| handles.iter().find(|h| h.node).map(|h| h.index))
    }
    pub(super) fn path_controls(&self) -> Element<'_, Message> {
        let send = |a| Message::Graphics(super::Action::Path(a));
        let mut panel = column![section("PEN PATH")].spacing(8);
        // Outside the Pen path tool, Done in the context row ends editing and
        // the tool itself starts a new path.
        let drawing = self.tool == Tool::Graphic(GraphicKind::Path);
        if drawing {
            panel = panel.push(
                row![
                    path_button("pen-new", "New path").on_press(send(Action::New)),
                    path_button("pen-finish", "Finish").on_press(send(Action::Finish))
                ]
                .spacing(4),
            );
        }
        let mut hint = drawing.then_some(DRAWING_HINT);
        if let Some((graphic, summary)) = self.selected_path() {
            let node = self.path_node(graphic, &summary).is_some();
            panel = self.path_node_controls(panel, graphic, &summary);
            if !node {
                hint = Some(CONTROL_HINT);
            }
        }
        match hint {
            Some(hint) => panel.push(path_hint(hint)).into(),
            None => panel.into(),
        }
    }
    /// The position of the selected node among the path's nodes, or `None`
    /// while a direction control is selected.
    fn path_node(&self, graphic: &Graphic, summary: &PathSummary) -> Option<usize> {
        let point = self.path_point(graphic, &summary.handles);
        summary.nodes().position(|h| Some(h.index) == point)
    }
    fn path_node_controls<'a>(
        &'a self,
        panel: Column<'a, Message>,
        graphic: &Graphic,
        summary: &PathSummary,
    ) -> Column<'a, Message> {
        let send = |a| Message::Graphics(super::Action::Path(a));
        let choices: Vec<_> = summary
            .nodes()
            .enumerate()
            .map(|(i, h)| Choice {
                ordinal: i + 1,
                index: h.index,
            })
            .collect();
        let node = self.path_node(graphic, summary);
        let closed = summary.closed;
        let following = node.is_some_and(|i| summary.has_following(i));
        panel
            .push(
                text(format!(
                    "{} nodes · {}",
                    summary.node_count(),
                    if closed { "Closed" } else { "Open" }
                ))
                .size(12),
            )
            .push(
                column![
                    row![
                        path_label(SELECTED_NODE),
                        crate::appearance::pick_list(
                            choices.clone(),
                            node.and_then(|i| choices.get(i).copied()),
                            move |choice| send(Action::Node(choice.index)),
                        )
                        .text_size(12)
                        .padding(5)
                        .width(Length::Fill),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                    // The segment that follows the selected node.
                    row![
                        path_label(NEXT_SEGMENT),
                        accessible_command("pen-straight", "Straight segment", "Straight")
                            .on_press_maybe(following.then(|| send(Action::Straight))),
                        accessible_command("pen-curved", "Curved segment", "Curved")
                            .on_press_maybe(following.then(|| send(Action::Curved)))
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                ]
                .spacing(4),
            )
            .push(
                column![
                    row![
                        path_button("pen-insert", "Insert after")
                            .on_press_maybe(following.then(|| send(Action::Insert))),
                        path_button("pen-delete", "Delete node").on_press_maybe(
                            (node.is_some() && summary.can_delete_node())
                                .then(|| send(Action::Delete))
                        )
                    ]
                    .spacing(4),
                    row![
                        path_button("pen-close", if closed { "Open path" } else { "Close path" })
                            .on_press_maybe((closed || summary.can_close()).then(|| {
                                send(if closed { Action::Open } else { Action::Close })
                            })),
                        path_button("pen-continue", "Continue drawing")
                            .on_press_maybe((!closed).then(|| send(Action::Continue)))
                    ]
                    .spacing(4),
                ]
                .spacing(2),
            )
    }
    pub(super) fn path_action(&mut self, action: Action) {
        match action {
            Action::New => {
                self.tab.selected.clear();
                self.tab.path_point = None;
                self.select_tool(Tool::Graphic(GraphicKind::Path));
                return;
            }
            Action::Finish => {
                self.select_tool(Tool::Select);
                return;
            }
            Action::Continue => {
                let Some(id) = self
                    .selected_path()
                    .filter(|(_, summary)| !summary.closed)
                    .map(|(graphic, _)| graphic.id)
                else {
                    self.status = "Select one open continuous path".into();
                    self.error = true;
                    return;
                };
                let point = self.tab.path_point;
                self.select_tool(Tool::Graphic(GraphicKind::Path));
                self.tab.selected = vec![id];
                self.tab.path_point = point;
                self.sync_graphics();
                return;
            }
            _ => {}
        }
        let Some((graphic, summary)) = self.selected_path() else {
            self.status = "Select one continuous path".into();
            self.error = true;
            return;
        };
        let id = graphic.id;
        let index = self.path_point(graphic, &summary.handles).unwrap_or(0);
        if let Action::Node(index) = action {
            self.tab.path_point = Some((id, index));
            return;
        }
        let before = self.tab.doc.clone();
        let Some(graphic) = self.tab.doc.graphics.iter_mut().find(|g| g.id == id) else {
            return;
        };
        let result = match action {
            Action::Insert => graphic.insert_path_node(index),
            Action::Delete => graphic.delete_path_node(index),
            Action::Straight => graphic.set_path_segment_curved(index, false),
            Action::Curved => graphic.set_path_segment_curved(index, true),
            Action::Open => graphic.set_path_closed(false).map(|()| 0),
            Action::Close => graphic.set_path_closed(true).map(|()| 0),
            Action::NodeType(kind) => graphic.set_path_node_type(index, kind).map(|()| index),
            _ => return,
        };
        match result {
            Ok(index) => {
                self.tab.path_point = Some((id, index));
                self.changed(before);
                self.sync_graphics();
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
    pub(in crate::app) fn place_pen_segment(
        &mut self,
        stroke: crate::canvas::pen::Stroke,
        before: Document,
    ) {
        if self.tool != Tool::Graphic(GraphicKind::Path) {
            return;
        }
        match crate::canvas::pen::apply(
            &self.tab.doc,
            &self.tab.selected,
            &self.tab.graphic_style,
            stroke,
        ) {
            Ok((doc, id, index)) => {
                self.tab.doc = doc;
                self.tab.selected = vec![id];
                self.tab.path_point = Some((id, index));
                self.changed(before);
                self.sync_graphics();
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
}
