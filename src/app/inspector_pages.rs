//! Chemical names and Assistant are pages within the ordinary inspector tabs.
use super::{App, InspectorTab, Message, document_tab::TabId, inspector};
use iced::Task;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct State {
    /// Exact preceding pages; tab-track styling is derived separately.
    previous: Vec<InspectorTab>,
    selection: Option<Selection>,
}

struct Selection {
    document: (TabId, u64),
    ids: HashSet<u64>,
}

fn is_page(tab: InspectorTab) -> bool {
    matches!(tab, InspectorTab::Names | InspectorTab::Assistant)
}

impl App {
    fn page_selection(&self) -> Selection {
        Selection {
            document: (self.tab.id, self.tab.file_epoch),
            ids: self.tab.selected.iter().copied().collect(),
        }
    }

    /// Keep exact page history, including nested Names/Assistant visits,
    /// independently of the tab highlighted in the shared track.
    pub(super) fn enter_inspector_page(&mut self, tab: InspectorTab) {
        self.templates.add_menu_open = false;
        if is_page(tab) {
            if self.inspector_tab != tab {
                self.inspector_pages.previous.push(self.inspector_tab);
            }
        } else {
            self.inspector_pages.previous.clear();
            // A nested page can return to its editor. An ordinary destination
            // abandons unrelated drafts, just like a direct tab activation.
            if tab != InspectorTab::ThemeGenerator {
                self.theme_library.editor = None;
            }
            if tab != InspectorTab::DrawingStyle {
                self.tab.styles.editor = None;
            }
        }
        self.inspector_pages.selection = is_page(tab).then(|| self.page_selection());
        if tab != InspectorTab::Assistant {
            self.assistant.menu = None;
        }
    }

    pub(super) fn inspector_parent_tab(&self) -> InspectorTab {
        let tab = if is_page(self.inspector_tab) {
            self.inspector_pages
                .previous
                .iter()
                .rev()
                .copied()
                .find(|tab| !is_page(*tab))
                .unwrap_or(InspectorTab::Properties)
        } else {
            self.inspector_tab
        };
        match tab {
            InspectorTab::Reactions
            | InspectorTab::Labels
            | InspectorTab::Abbreviations
            | InspectorTab::Pages
            | InspectorTab::DrawingStyle => InspectorTab::Properties,
            _ => tab,
        }
    }

    pub(super) fn close_inspector_page(&mut self) -> Task<Message> {
        if !is_page(self.inspector_tab) {
            return Task::none();
        }
        let previous = self
            .inspector_pages
            .previous
            .pop()
            .unwrap_or(InspectorTab::Properties);
        self.assistant.menu = None;
        self.inspector_tab = previous;
        self.inspector_open = true;
        self.inspector_pages.selection = is_page(previous).then(|| self.page_selection());
        super::focus_drawing()
    }

    /// Compare final selection state, including right-click and keyboard routes
    /// that assign IDs directly. A document-tab switch retains its page routing.
    pub(super) fn sync_inspector_page_selection(&mut self) -> Task<Message> {
        if !self.inspector_open || !is_page(self.inspector_tab) {
            if !is_page(self.inspector_tab) && !self.inspector_pages.previous.is_empty() {
                self.enter_inspector_page(self.inspector_tab);
            }
            self.inspector_pages.selection = None;
            return Task::none();
        }
        let document = (self.tab.id, self.tab.file_epoch);
        let Some(before) = self
            .inspector_pages
            .selection
            .as_ref()
            .filter(|before| before.document == document)
        else {
            self.inspector_pages.selection = Some(self.page_selection());
            return Task::none();
        };
        if before.ids.len() == self.tab.selected.len()
            && self.tab.selected.iter().all(|id| before.ids.contains(id))
        {
            return Task::none();
        }
        self.enter_inspector_page(InspectorTab::Properties);
        self.inspector_tab = InspectorTab::Properties;
        Task::batch([
            super::focus_drawing(),
            iced::widget::operation::snap_to(
                "inspector-content",
                iced::widget::operation::RelativeOffset::START,
            ),
            self.accessibility_refresh(),
        ])
    }

    pub(super) fn inspector_page_back_label(&self) -> Option<&'static str> {
        if !is_page(self.inspector_tab) {
            return None;
        }
        let previous = self
            .inspector_pages
            .previous
            .last()
            .copied()
            .unwrap_or(InspectorTab::Properties);
        Some(match previous {
            InspectorTab::Properties => "Properties",
            InspectorTab::Predict => "Predict",
            InspectorTab::Templates => "Add",
            InspectorTab::Import => "Import",
            InspectorTab::Export => "Export",
            InspectorTab::Names => "Chemical names",
            InspectorTab::Pages => "Publication pages",
            InspectorTab::Versions => "History",
            InspectorTab::DrawingStyle => "Drawing style",
            InspectorTab::ThemeGenerator => "Color theme",
            InspectorTab::Reactions => "Reaction roles",
            InspectorTab::Labels => "Atom labels",
            InspectorTab::Abbreviations => "Abbreviations",
            InspectorTab::Assistant => "Assistant",
        })
    }
}

#[cfg(test)]
mod tests;
