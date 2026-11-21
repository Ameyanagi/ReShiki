//! Chemical names and Assistant are pages within the ordinary inspector tabs.
use super::{App, InspectorTab, Message, document_tab::TabId, inspector};
use iced::widget::{row, text};
use iced::{Alignment, Element, Length, Task};
use std::collections::HashSet;

pub(super) struct State {
    names_parent: InspectorTab,
    assistant_previous: InspectorTab,
    pub(super) structure_first: bool,
    selection: Option<Selection>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            names_parent: InspectorTab::Properties,
            assistant_previous: InspectorTab::Properties,
            structure_first: false,
            selection: None,
        }
    }
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

    /// Called before switching pages so Assistant can return to the exact
    /// preceding page, including Chemical names, History or Publication pages.
    pub(super) fn enter_inspector_page(&mut self, tab: InspectorTab) {
        self.templates.add_menu_open = false;
        if tab == InspectorTab::Names {
            if self.inspector_tab != tab {
                self.inspector_pages.names_parent = self.inspector_parent_tab();
            }
            self.inspector_pages.structure_first = self
                .tab
                .doc
                .atoms
                .iter()
                .any(|atom| self.tab.selected.contains(&atom.id));
        }
        if tab == InspectorTab::Assistant && self.inspector_tab != tab {
            self.inspector_pages.assistant_previous = self.inspector_tab;
        }
        self.inspector_pages.selection = is_page(tab).then(|| self.page_selection());
        if tab != InspectorTab::Assistant {
            self.assistant.menu = None;
        }
    }

    pub(super) fn inspector_parent_tab(&self) -> InspectorTab {
        let mut tab = self.inspector_tab;
        if tab == InspectorTab::Assistant {
            tab = self.inspector_pages.assistant_previous;
        }
        if tab == InspectorTab::Names {
            tab = self.inspector_pages.names_parent;
        }
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
        let previous = match self.inspector_tab {
            InspectorTab::Names => self.inspector_pages.names_parent,
            InspectorTab::Assistant => self.inspector_pages.assistant_previous,
            _ => return Task::none(),
        };
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
        self.assistant.menu = None;
        self.inspector_pages.selection = None;
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

    pub(super) fn inspector_page_header(&self) -> Option<Element<'_, Message>> {
        let (title, previous) = match self.inspector_tab {
            InspectorTab::Names => ("Chemical names", self.inspector_pages.names_parent),
            InspectorTab::Assistant => ("Assistant", self.inspector_pages.assistant_previous),
            _ => return None,
        };
        let label = match previous {
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
            InspectorTab::Assistant => "Properties",
        };
        let back = Message::InspectorAction(inspector::Action::BackPage);
        #[cfg(not(target_arch = "wasm32"))]
        let new_conversation = (self.inspector_tab == InspectorTab::Assistant).then(|| {
            reshiki::accessibility::button(
                "assistant-new-conversation",
                "New conversation",
                text("＋").size(16),
            )
            .padding([3, 5])
            .style(iced::widget::button::text)
            .on_press(Message::Assistant(super::assistant::Action::Reset))
        });
        #[cfg(target_arch = "wasm32")]
        let new_conversation: Option<Element<'_, Message>> = None;
        Some(
            row![
                reshiki::accessibility::button(
                    "inspector-page-back",
                    format!("Back to {label}"),
                    text(format!("‹ {label}")).size(12)
                )
                .padding([4, 0])
                .style(iced::widget::button::text)
                .on_press(back.clone()),
                text(format!("· {title}")).size(14).width(Length::Fill),
                new_conversation,
                reshiki::accessibility::button(
                    "inspector-page-close",
                    format!("Close {title}"),
                    text("×").size(18)
                )
                .padding([3, 5])
                .style(iced::widget::button::text)
                .on_press(back),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        )
    }
}

#[cfg(test)]
mod tests;
