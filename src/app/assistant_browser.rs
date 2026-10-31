//! The assistant drives the Codex app-server, a desktop process. The browser
//! build keeps its panel and explains that it needs the desktop app.
use super::{App, InspectorTab, Message, document_tab::TabId};
use iced::widget::{button, column, row, text};
use iced::{Element, Length, Task};

#[derive(Debug, Clone)]
pub enum Action {
    Open,
    #[expect(
        dead_code,
        reason = "Matched by the inline editor's shortcuts; never sent here"
    )]
    Send,
    #[expect(
        dead_code,
        reason = "Matched by the inline editor's shortcuts; never sent here"
    )]
    Retry,
    Poll,
    ViewImage(Option<reshiki::pictures::Picture>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {}

#[derive(Default)]
pub struct State {
    pub(super) tab: Option<TabId>,
    pub(super) viewed_image: Option<reshiki::pictures::Picture>,
    pub(super) menu: Option<Menu>,
    pub busy: bool,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn has_unfinished_work(&self) -> bool {
        false
    }

    pub(super) fn needs_poll(&self) -> bool {
        false
    }
}

impl App {
    pub(super) fn retire_assistant(&mut self, _id: TabId) {}

    pub(super) fn assistant_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Assistant;
                self.palette = None;
            }
            Action::ViewImage(image) => self.assistant.viewed_image = image,
            Action::Send | Action::Retry | Action::Poll => {}
        }
        Task::none()
    }

    pub(in crate::app) fn assistant_panel(&self) -> Element<'_, Message> {
        column![
            row![
                button(text("‹").size(18))
                    .padding([3, 8])
                    .style(super::workspace::control(false))
                    .on_press(Message::Inspector(InspectorTab::Properties)),
                text("Assistant").size(16),
            ]
            .spacing(8),
            text("The assistant needs the ReShiki desktop app.").size(13),
        ]
        .spacing(12)
        .padding(12)
        .width(Length::Fill)
        .into()
    }

    pub(in crate::app) fn with_assistant_image<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        base
    }
}
