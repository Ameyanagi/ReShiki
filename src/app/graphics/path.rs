//! Contextual pen authoring and node edits.
use super::*;
use iced::widget::{column, row, text};
use reshiki::graphics::{Graphic, PathHandle};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Choice {
    ordinal: usize,
    index: usize,
}
fn path_button(
    id: &'static str,
    label: &'static str,
) -> reshiki::accessibility::Button<'static, Message> {
    reshiki::accessibility::button(id, label, text(label).size(11))
        .padding([6, 8])
        .style(control(false))
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node {}", self.ordinal)
    }
}
impl App {
    fn selected_path(&self) -> Option<&Graphic> {
        let [id] = self.tab.selected.as_slice() else {
            return None;
        };
        self.tab
            .doc
            .graphics
            .iter()
            .find(|g| g.id == *id && g.path_handles().is_some())
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
        let mut panel = column![
            section("PEN PATH"),
            row![
                path_button("pen-new", "New path").on_press(send(Action::New)),
                path_button("pen-finish", "Finish").on_press(send(Action::Finish))
            ]
            .spacing(5)
        ]
        .spacing(8);
        if let Some(graphic) = self.selected_path()
            && let Some(handles) = graphic.path_handles()
        {
            let choices: Vec<_> = handles
                .iter()
                .filter(|h| h.node)
                .enumerate()
                .map(|(i, h)| Choice {
                    ordinal: i + 1,
                    index: h.index,
                })
                .collect();
            let point = self.path_point(graphic, &handles);
            let node = choices
                .iter()
                .position(|choice| Some(choice.index) == point);
            let closed = graphic.path_closed();
            let following = node.is_some_and(|i| closed || i + 1 < choices.len());
            panel=panel.push(text(format!("{} nodes · {}",choices.len(),if closed {"Closed"} else {"Open"})).size(12))
                .push(crate::appearance::pick_list(choices.clone(),node.and_then(|i|choices.get(i).copied()),move |choice|send(Action::Node(choice.index))).text_size(12).padding(5).width(Length::Fill))
                .push(row![path_button("pen-insert", "Insert after").on_press_maybe(following.then(||send(Action::Insert))),path_button("pen-delete", "Delete node").on_press_maybe((node.is_some() && choices.len()>if closed {3} else {2}).then(||send(Action::Delete)))].spacing(5))
                .push(row![path_button("pen-straight", "Straight segment").on_press_maybe(following.then(||send(Action::Straight))),path_button("pen-curved", "Curved segment").on_press_maybe(following.then(||send(Action::Curved)))].spacing(5))
                .push(row![path_button("pen-close",if closed {"Open path"} else {"Close path"}).on_press_maybe((closed || choices.len()>=3).then(||send(if closed {Action::Open} else {Action::Close}))),path_button("pen-continue", "Continue drawing").on_press_maybe((!closed).then(||send(Action::Continue)))].spacing(5))
                .push(text(if node.is_some() {"Round nodes carry adjacent controls. Square controls change each tangent independently."} else {"A direction control is selected. Drag its square, or choose a node above."}).size(11).style(muted_text));
        }
        panel.push(text("Drag the first segment. Then click for a line or drag a new node for a curve. Click the first node to close. Escape cancels the active gesture.").size(11).style(muted_text)).into()
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
                self.select_tool(Tool::Graphic(GraphicKind::Path));
                return;
            }
            _ => {}
        }
        let Some(graphic) = self.selected_path() else {
            self.status = "Select one continuous path".into();
            self.error = true;
            return;
        };
        let id = graphic.id;
        let index = self
            .path_point(graphic, &graphic.path_handles().unwrap_or_default())
            .unwrap_or(0);
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
