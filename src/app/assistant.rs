//! The assistant: conversation state, the action dispatcher and shared widget helpers.
use super::{App, InspectorTab, Message, document_tab::TabId};
use iced::widget::{button, container, text, text_editor};
use iced::{Border, Color, Task};
use reshiki::assistant::settings::Preferences;
use reshiki::{
    assistant::{self, Proposal, codex},
    document::Document,
};

mod attachments;
mod connection;
mod drafts;
mod image_viewer;
mod menus;
mod poll;
mod preferences;
mod previews;
mod request;
mod setup;
mod view;

#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Connect,
    Connected(u64, Result<codex::Account, codex::ConnectionError>),
    SetupHelp(setup::Help),
    ImageExample,
    Input(text_editor::Action),
    Paste {
        image_only: bool,
    },
    OpenImage,
    ImageRead {
        serial: u64,
        epoch: u64,
        image_only: bool,
        result: Result<Option<reshiki::pictures::Picture>, String>,
    },
    TextPasted {
        serial: u64,
        epoch: u64,
        text: Option<String>,
    },
    ClearImage,
    ViewImage(Option<reshiki::pictures::Picture>),
    Example(&'static str),
    Model(Option<String>),
    Menu(Option<Menu>),
    Search(String),
    ChatScrolled {
        follow: bool,
        offset: f32,
    },
    JumpToResult,
    Effort(String),
    Tier(String),
    PreferencesSaved(Result<(), String>),
    Replace(bool),
    AutoApply(bool),
    Send,
    Improve,
    PreviewTarget(String),
    PreviewEdit(assistant::review::Edit),
    Stop,
    Poll,
    Reset,
    Apply,
    Reject,
    Done {
        serial: u64,
        epoch: u64,
        revision: u64,
        replace: Vec<u64>,
        result: Box<Result<assistant::review::Outcome, String>>,
    },
}
pub struct Draft {
    pub proposal: Proposal,
    pub fragment: Document,
    pub review: assistant::review::Report,
    pub revision: u64,
    pub epoch: u64,
    pub replace: Vec<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Models,
    Effort,
    Edits,
    Attachments,
}

struct ChatMessage {
    role: String,
    text: String,
    image: Option<reshiki::pictures::Picture>,
}

#[derive(Default)]
pub struct State {
    /// The drawing this conversation and its canvas tools belong to.
    pub(super) tab: Option<TabId>,
    waiting_for_canvas_edit: bool,
    input: text_editor::Content,
    source_image: Option<reshiki::pictures::Picture>,
    pub(super) viewed_image: Option<reshiki::pictures::Picture>,
    image_serial: u64,
    reading_image: bool,
    pub draft: Option<Draft>,
    messages: Vec<ChatMessage>,
    account: Option<codex::Account>,
    connection: setup::Connection,
    guided_example: bool,
    /// The pending/draft request waits for Apply even if its composer image changes.
    requires_apply: bool,
    example_reference: Option<Document>,
    preferences: Preferences,
    preferences_dirty: bool,
    preferences_saving: bool,
    pub(super) menu: Option<Menu>,
    search: String,
    follow_chat: bool,
    chat_offset: f32,
    completed: Option<(Document, assistant::review::Report)>,
    reply: String,
    plan: String,
    preview: Option<Document>,
    preview_target: Option<String>,
    structures: Option<(usize, usize)>,
    checking: usize,
    last_activity: Option<std::time::Instant>,
    elapsed: u64,
    pending_scope: Option<(u64, u64, Vec<u64>)>,
    pending_proposal: Option<Proposal>,
    started: Option<std::time::Instant>,
    running_model: String,
    replace: bool,
    pub busy: bool,
    status: String,
    error: bool,
    serial: u64,
    cancel: codex::Cancel,
    progress: Option<tokio::sync::mpsc::Receiver<codex::Progress>>,
    canvas: Option<assistant::canvas_tools::SharedCanvas>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel.stop();
    }
}
impl State {
    #[cfg(test)]
    pub(super) fn input_text(&self) -> String {
        self.input.text()
    }

    pub(super) fn has_unfinished_work(&self) -> bool {
        self.busy
            || self.draft.is_some()
            || self.reading_image
            || !self.input.text().trim().is_empty()
            || self.source_image.is_some()
    }

    pub(super) fn needs_poll(&self) -> bool {
        self.busy
            || self.waiting_for_canvas_edit
            || (self.preferences_dirty && !self.preferences_saving)
    }

    pub fn new() -> Self {
        let mut state = Self::default();
        if !cfg!(test) {
            state.preferences = Preferences::load();
        }
        state
    }
    fn retain_preview(&mut self, reason: &str) {
        if let Some(fragment) = self.preview.clone()
            && let Some((epoch, revision, replace)) = self.pending_scope.clone()
        {
            self.draft = Some(Draft {
                proposal: self.pending_proposal.clone().unwrap_or_default(),
                fragment,
                revision,
                epoch,
                replace,
                review: assistant::review::Report {
                    summary: "Completed preview retained for inspection and editing.".into(),
                    issues: vec![reason.into()],
                    ..Default::default()
                },
            });
        }
    }
    fn model(&self) -> Option<&codex::Model> {
        self.account
            .as_ref()
            .and_then(|a| self.preferences.resolve(&a.models).ok())
    }
    fn record(&mut self, role: &str, text: String) {
        self.messages.push(ChatMessage {
            role: role.into(),
            text,
            image: (role == "You").then(|| self.source_image.clone()).flatten(),
        });
    }
    // Bound model context without removing older messages or their images from
    // the visible conversation. Image bytes travel through the image input only.
    fn conversation(&self) -> Vec<(&str, &str)> {
        self.messages
            .iter()
            .rev()
            .take(24)
            .rev()
            .map(|m| (m.role.as_str(), m.text.as_str()))
            .collect()
    }
}
impl App {
    pub(super) fn retire_assistant(&mut self, id: TabId) {
        if self.assistant.tab == Some(id) {
            let _ = self.assistant_action(Action::Reset);
        }
    }

    pub(super) fn assistant_action(&mut self, action: Action) -> Task<Message> {
        if let Some(id) = self.assistant.tab.filter(|id| *id != self.tab.id) {
            match action {
                Action::Poll | Action::Done { .. } => {
                    let task = self.background_result(id, Message::Assistant(action));
                    return super::tagged(task, id);
                }
                Action::Send | Action::Improve | Action::Apply => return Task::none(),
                _ => {}
            }
        }
        let scroll = matches!(
            &action,
            Action::Send
                | Action::Improve
                | Action::JumpToResult
                | Action::Reject
                | Action::ImageExample
        );
        match action {
            Action::ViewImage(image) => self.assistant.viewed_image = image,
            Action::Open => return self.assistant_open(),
            Action::SetupHelp(help) => return self.assistant_setup_help(help),
            Action::ImageExample => self.assistant_image_example(),
            Action::Input(action) => {
                self.assistant.input.perform(action);
            }
            Action::Paste { image_only } => return self.assistant_paste(image_only),
            Action::OpenImage => return self.assistant_open_image(),
            Action::ImageRead {
                serial,
                epoch,
                image_only,
                result,
            } => return self.assistant_image_read(serial, epoch, image_only, result),
            Action::TextPasted {
                serial,
                epoch,
                text,
            } => return self.assistant_text_pasted(serial, epoch, text),
            Action::ClearImage => self.assistant_clear_image(),
            Action::Example(value) => {
                self.assistant.input = text_editor::Content::with_text(value);
            }
            Action::Replace(value) => self.assistant.replace = value,
            Action::AutoApply(value) => return self.assistant_auto_apply(value),
            Action::Model(value) => self.assistant_set_model(value),
            Action::Menu(value) => self.assistant_toggle_menu(value),
            Action::Search(value) => self.assistant.search = value,
            Action::ChatScrolled { follow, offset } => {
                self.assistant.follow_chat = follow;
                self.assistant.chat_offset = offset;
            }
            Action::JumpToResult => self.assistant.follow_chat = true,
            Action::Effort(value) => self.assistant_set_effort(value),
            Action::Tier(value) => self.assistant_set_tier(value),
            Action::PreferencesSaved(result) => self.assistant_preferences_saved(result),
            Action::Reset => self.assistant_reset(),
            Action::Stop => self.assistant_stop(),
            Action::Connect => return self.assistant_connect(),
            Action::Connected(serial, result) => return self.assistant_connected(serial, result),
            Action::Poll => return self.assistant_poll(),
            Action::Reject => self.assistant_reject(),
            Action::Apply => return self.assistant_apply(),
            Action::PreviewTarget(target) => {
                self.assistant.preview_target = (target != "Overview").then_some(target);
            }
            Action::PreviewEdit(edit) => self.assistant_preview_edit(edit),
            action @ (Action::Send | Action::Improve) => {
                return self.assistant_request(matches!(action, Action::Improve));
            }
            Action::Done {
                serial,
                epoch,
                revision,
                replace,
                result,
            } => return self.assistant_done(serial, epoch, revision, replace, result),
        }
        chat_scroll(scroll)
    }
    fn assistant_open(&mut self) -> Task<Message> {
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Assistant;
        self.palette = None;
        if self.assistant.connection == setup::Connection::Unchecked
            && self.assistant.account.as_ref().is_some_and(|a| a.connected)
        {
            self.assistant.connection = setup::Connection::Ready;
        }
        if self.assistant.connection == setup::Connection::Unchecked && !self.assistant.busy {
            return self.assistant_action(Action::Connect);
        }
        if self.assistant.follow_chat {
            iced::widget::operation::snap_to_end("assistant-chat")
        } else {
            iced::widget::operation::scroll_to(
                "assistant-chat",
                iced::widget::operation::AbsoluteOffset {
                    x: Some(0.),
                    y: Some(self.assistant.chat_offset),
                },
            )
        }
    }
}
fn action<'a>(
    label: impl Into<std::borrow::Cow<'a, str>>,
    value: Action,
) -> button::Button<'a, Message> {
    button(text(label.into()).size(12))
        .padding([7, 10])
        .on_press(Message::Assistant(value))
        .style(super::workspace::control(false))
}
/// The tail of assistant_action: follow the chat to its end when `scroll` is set.
fn chat_scroll(scroll: bool) -> Task<Message> {
    if scroll {
        iced::widget::operation::snap_to_end("assistant-chat")
    } else {
        Task::none()
    }
}
fn surface(background: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(background.into()),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn card() -> container::Style {
    let mut style = surface(Color::WHITE, 12.);
    style.border.width = 1.;
    style.border.color = Color::from_rgb8(213, 224, 220);
    style
}

#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod tests;
