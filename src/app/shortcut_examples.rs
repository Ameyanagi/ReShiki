//! An editable, unbound copy of the bundled reference in a document tab.
use super::{App, InspectorTab, Message};
use iced::Task;
use reshiki::{
    document::{Document, Point},
    style::DEFAULT as STYLE,
};

const DRAWING: &str = include_str!("../../assets/examples/shortcut-examples.rsk");
const NAME: &str = "Shortcut examples";

impl App {
    pub(super) fn open_shortcut_examples(&mut self) -> Task<Message> {
        self.help_open = false;
        if let Err(error) = self.load_shortcut_examples() {
            self.status = format!("Could not open shortcut examples: {error}");
            self.error = true;
        }
        Task::none()
    }

    pub(super) fn load_shortcut_examples(&mut self) -> Result<(), String> {
        let existing = self.strip().position(|tab| {
            tab.untitled_name == Some(NAME) && tab.path.is_none() && !self.edited(tab)
        });
        if let Some(index) = existing {
            self.select_tab(index);
            self.error = false;
            return Ok(());
        }
        let mut doc: Document = serde_json::from_str(DRAWING).map_err(|e| e.to_string())?;
        doc.validate()?;
        reshiki::atom_labels::clear_computed(&mut doc);
        self.target_tab();
        self.reset_tab();
        self.tab.doc = doc;
        self.tab.saved = self.tab.doc.clone();
        self.tab.untitled_name = Some(NAME);
        self.sync_drawing_defaults();
        self.inspector_open = false;
        self.inspector_tab = InspectorTab::Properties;
        // Start at the common groups at a readable scale on the ordinary canvas.
        self.tab.camera.center = Point::new(STYLE.world(270.), STYLE.world(210.));
        self.tab.camera.zoom = 0.5;
        self.tab.fit_to_view = false;
        self.status =
            "Shortcut examples · Pan or zoom to browse; double-click to select, then copy".into();
        self.error = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
