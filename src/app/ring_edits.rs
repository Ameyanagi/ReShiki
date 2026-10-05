//! Ring tool settings and the selected ring's aromaticity.
use super::{App, Message};
use crate::canvas::Tool;
use iced::Task;

impl App {
    pub(super) fn set_ring_size(&mut self, n: u8) {
        self.toolbar.ring = Tool::Ring;
        self.ring_size = n;
        self.tool = Tool::Ring;
    }
    pub(super) fn set_aromatic_ring(&mut self, value: bool) {
        self.toolbar.ring = Tool::Ring;
        self.aromatic_ring = value;
        self.tool = Tool::Ring;
    }
    pub(super) fn toggle_aromatic_ring(&mut self) -> Task<Message> {
        if self.tool.selects()
            && reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some()
        {
            return self.update(Message::ToggleSelectedRing);
        }
        self.update(Message::AromaticRing(!self.aromatic_ring))
    }
    pub(super) fn toggle_selected_ring(&mut self) {
        let before = self.tab.doc.clone();
        match reshiki::rings::toggle_selected_aromatic(&mut self.tab.doc, &self.tab.selected) {
            Ok(aromatic) => {
                self.changed(before);
                self.status = if aromatic {
                    "Selected ring set to aromatic"
                } else {
                    "Selected ring set to saturated"
                }
                .into();
            }
            Err(error) => {
                self.error = true;
                self.status = error;
            }
        }
    }
}
