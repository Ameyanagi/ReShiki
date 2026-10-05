//! Selection, grouping, framing and arrangement commands.
use super::{App, Message};
use crate::canvas::Tool;
use reshiki::{
    document::Point,
    editing,
    graphics::{BracketSides, Graphic, GraphicStyle},
};

impl App {
    pub(super) fn add_frame(&mut self, kind: reshiki::graphics::GraphicKind) {
        let mut ids = self.tab.doc.complete_selection(&self.tab.selected);
        if let Some((lo, hi)) = reshiki::scene::selection_bounds(&self.tab.doc, &ids) {
            let before = self.tab.doc.clone();
            let id = self.tab.doc.next_id();
            let padding = reshiki::style::DEFAULT.world(6.0);
            self.tab.doc.graphics.push(Graphic::dragged(
                id,
                kind,
                lo.offset(-padding, -padding),
                hi.offset(padding, padding),
                GraphicStyle {
                    width_pt: self.tab.doc.drawing_style.line_width_pt,
                    ..Default::default()
                },
                BracketSides::Both,
                false,
            ));
            ids.push(id);
            if let Ok(ids) = self.tab.doc.group_selection(&ids) {
                self.tab.selected = ids;
            }
            self.changed(before);
            self.tool = Tool::Select;
            self.status = "Frame added and grouped with the selection".into();
        }
    }
    pub(super) fn group_selected(&mut self) {
        let before = self.tab.doc.clone();
        match self.tab.doc.group_selection(&self.tab.selected) {
            Ok(ids) => {
                self.tab.selected = ids;
                self.changed(before);
                self.tool = Tool::Select;
                self.status = format!(
                    "Grouped · {}-click selects a member · {} ungroups",
                    super::shortcuts::keys(iced::keyboard::Modifiers::ALT, ""),
                    super::shortcuts::label(&Message::Ungroup).unwrap_or_default()
                );
            }
            Err(e) => {
                self.status = e;
                self.error = true;
            }
        }
    }
    pub(super) fn ungroup_selected(&mut self) {
        let before = self.tab.doc.clone();
        if self.tab.doc.ungroup_selection(&self.tab.selected) {
            self.changed(before);
            self.status = "Ungrouped one level".into();
        }
    }
    pub(super) fn set_integral_groups(&mut self, integral: bool) {
        let before = self.tab.doc.clone();
        let ids = self.tab.doc.outer_selected_groups(&self.tab.selected);
        for g in &mut self.tab.doc.groups {
            if ids.contains(&g.id) {
                g.integral = integral;
            }
        }
        self.changed(before);
    }
    pub(super) fn invert_selection(&mut self) {
        let selected = self.tab.doc.expand_groups(&self.tab.selected);
        self.tab.selected = self
            .tab
            .doc
            .all_ids()
            .into_iter()
            .filter(|id| !selected.contains(id))
            .collect();
        self.tool = Tool::Select;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
    }
    pub(super) fn duplicate_selection(&mut self) {
        let part = editing::selection(&self.tab.doc, &self.tab.selected);
        let before = self.tab.doc.clone();
        self.tab.selected = editing::append(&mut self.tab.doc, &part, Point::new(28.0, 28.0));
        self.changed(before);
        self.tool = Tool::Select;
    }
    pub(super) fn transform_selection(&mut self, transform: reshiki::editing::Transform) {
        let before = self.tab.doc.clone();
        editing::transform(&mut self.tab.doc, &self.tab.selected, transform);
        self.changed(before);
    }
    pub(super) fn arrange_selection(&mut self, arrange: reshiki::editing::Arrange) {
        let before = self.tab.doc.clone();
        editing::arrange(&mut self.tab.doc, &self.tab.selected, arrange);
        self.changed(before);
    }
    pub(super) fn delete_selection(&mut self) {
        let before = self.tab.doc.clone();
        self.tab.doc.delete(&self.tab.selected);
        self.changed(before);
    }
    pub(super) fn select_all_objects(&mut self) {
        self.tab.selected = self.tab.doc.all_ids();
        self.tool = Tool::Select;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
    }
}
