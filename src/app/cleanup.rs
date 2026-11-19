use super::{App, Job, Message, Request};
use iced::Task;
use reshiki::cleanup::{Options, Orientation, Scope};

#[derive(Debug, Clone)]
pub struct CleanupJob {
    pub options: Options,
    pub selection: Vec<u64>,
    pub serial: u64,
    pub epoch: u64,
}

#[cfg(test)]
mod tests;

/// Cleanup commands, nested under `Message::Cleanup`.
#[derive(Debug, Clone)]
pub enum Action {
    ToWedge,
    ToFischer,
    /// Cleans up the selection, or the whole drawing when nothing is
    /// selected, as one Undo step (toolbar "Clean up", Shift+Cmd/Ctrl+K).
    Begin,
    /// Opens or closes the toolbar's Clean up options menu.
    Menu(bool),
    /// What a selection cleans: its atoms, or the molecules they belong to.
    Scope(Scope),
    Orientation(Orientation),
    StyleBondLength(bool),
    /// Tidies the selected scheme, or every one in the drawing, as one Undo
    /// step (Clean up reaction, Shift+Cmd/Ctrl+X). Reactions today; other
    /// schemes, such as catalytic cycles, belong behind this same command.
    Scheme,
}

impl App {
    pub(super) fn cleanup_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::ToWedge => return self.convert_projection("fischer_wedge", "Wedge drawing"),
            Action::ToFischer => return self.convert_projection("fischer", "Fischer projection"),
            // Like the other toolbar commands, the drawing keeps the keys.
            Action::Begin => return Task::batch([self.begin_cleanup(), super::focus_drawing()]),
            Action::Scheme => {
                self.cleanup_menu = false;
                self.clean_up_scheme();
            }
            Action::Menu(true) => {
                self.cleanup_menu = true;
                return Task::none();
            }
            Action::Menu(false) => self.cleanup_menu = false,
            Action::Scope(scope) => self.cleanup_options.scope = scope,
            Action::Orientation(mode) => self.cleanup_options.set_orientation(mode),
            Action::StyleBondLength(enabled) => {
                self.appearance.cleanup_style_bond_length = enabled;
                self.save_preference(
                    super::settings_page::Section::Drawing,
                    "clean up bond length",
                );
            }
        }
        super::focus_drawing()
    }
    fn convert_projection(&mut self, operation: &str, name: &'static str) -> Task<Message> {
        let mut request = Request::molecule(operation, self.tab.doc.clone());
        request.selected_ids = Some(self.tab.selected.clone());
        self.run(request, Job::Projection(name))
    }
    fn clean_up_scheme(&mut self) {
        if self.tab.busy {
            return;
        }
        let cleaned = reshiki::reaction_cleanup::clean(
            &self.tab.doc,
            &self.tab.selected,
            self.appearance.cleanup_style_bond_length,
        );
        match cleaned {
            Ok(document) if document == self.tab.doc => {
                self.status = "The reaction is already tidy".into();
                self.error = false;
            }
            Ok(document) => {
                let before = std::mem::replace(&mut self.tab.doc, document);
                self.changed(before);
                if !self.error {
                    self.status = format!(
                        "Cleaned up the reaction · {} undoes",
                        super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Z")
                    );
                }
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
    pub(super) fn begin_cleanup(&mut self) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        if self.tab.doc.atoms.is_empty() {
            self.status = "Draw a structure to clean up".into();
            self.error = true;
            return Task::none();
        }
        let selection: Vec<_> = self
            .tab
            .doc
            .expand_abbreviation_selection(&self.tab.selected)
            .into_iter()
            .filter(|id| self.tab.doc.atom(*id).is_some())
            .collect();
        let mut options = self.cleanup_options;
        options.style_bond_length = self.appearance.cleanup_style_bond_length;
        // Only an empty selection means the whole drawing.
        if self.tab.selected.is_empty() {
            options.scope = Scope::Drawing;
        } else if selection.is_empty() {
            self.status = "Select atoms or a molecule to clean up".into();
            self.error = true;
            return Task::none();
        }
        self.tab.cleanup_serial = self.tab.cleanup_serial.wrapping_add(1);
        let job = CleanupJob {
            options,
            selection,
            serial: self.tab.cleanup_serial,
            epoch: self.tab.file_epoch,
        };
        let mut request = Request::molecule("clean", self.tab.doc.clone());
        request.selected_ids = Some(job.selection.clone());
        request.cleanup = Some(job.options);
        self.run(request, Job::Clean(job))
    }
}
