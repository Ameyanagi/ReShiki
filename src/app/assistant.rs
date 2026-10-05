use super::{App, InspectorTab, Message, document_tab::TabId};
use crate::canvas::layered::canvas;
use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, opaque, rich_text, row, scrollable,
    stack, text, text_editor, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length, Task};
use reshiki::assistant::settings::{Preferences, effort_label};
use reshiki::{
    assistant::{self, DrawingSettings, Proposal, codex},
    document::Document,
};
use serde_json::json;

#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Connect,
    Connected(u64, Result<codex::Account, String>),
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

    pub(super) fn with_assistant_image<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(source) = &self.assistant.viewed_image else {
            return base;
        };
        let Some(handle) = source.handle(false) else {
            return base;
        };
        let base = reshiki::accessibility::inert(base);
        let close = Message::Assistant(Action::ViewImage(None));
        let popup = container(
            column![
                row![
                    text("Sent image").size(18),
                    Space::new().width(Length::Fill),
                    reshiki::accessibility::button(
                        "image-close",
                        "Close sent image",
                        "Close · Esc"
                    )
                    .on_press(close.clone())
                    .style(super::workspace::control(false))
                ]
                .align_y(Alignment::Center),
                iced::widget::image::viewer(handle)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Contain),
                text(format!(
                    "{} × {} · Scroll to zoom · Drag to pan",
                    source.width(),
                    source.height()
                ))
                .size(12)
                .style(super::workspace::muted_text)
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme| crate::appearance::container(theme, card()));
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| surface(Color::from_rgba8(20, 30, 30, 0.45), 0.))
                )
                .on_press(close)
            ),
            container(opaque(popup))
                .padding(28)
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .into()
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
        let mut scroll = matches!(
            &action,
            Action::Send | Action::Improve | Action::JumpToResult | Action::Reject
        );
        match action {
            Action::ViewImage(image) => self.assistant.viewed_image = image,
            Action::Open => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Assistant;
                self.palette = None;
                if self.assistant.account.is_none() && !self.assistant.busy {
                    return self.assistant_action(Action::Connect);
                }
                return if self.assistant.follow_chat {
                    iced::widget::operation::snap_to_end("assistant-chat")
                } else {
                    iced::widget::operation::scroll_to(
                        "assistant-chat",
                        iced::widget::operation::AbsoluteOffset {
                            x: Some(0.),
                            y: Some(self.assistant.chat_offset),
                        },
                    )
                };
            }
            Action::Input(action) => {
                self.assistant.input.perform(action);
            }
            Action::Paste { image_only } => {
                self.assistant.menu = None;
                if self.assistant.reading_image || (image_only && self.assistant.busy) {
                    return Task::none();
                }
                self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
                let serial = self.assistant.image_serial;
                let epoch = self.tab.file_epoch;
                self.assistant.reading_image = true;
                if !reshiki::clipboard::available() {
                    return iced::clipboard::read().map(move |text| {
                        Message::Assistant(Action::TextPasted {
                            serial,
                            epoch,
                            text,
                        })
                    });
                }
                return Task::perform(reshiki::clipboard::picture(), move |result| {
                    Message::Assistant(Action::ImageRead {
                        serial,
                        epoch,
                        image_only,
                        result,
                    })
                });
            }
            Action::OpenImage => {
                self.assistant.menu = None;
                if self.assistant.busy || self.assistant.reading_image {
                    return Task::none();
                }
                self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
                let serial = self.assistant.image_serial;
                let epoch = self.tab.file_epoch;
                self.assistant.reading_image = true;
                return Task::perform(
                    async {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_title("Attach a chemical drawing")
                            .add_filter("Images", &["png", "jpg", "jpeg", "tif", "tiff", "webp"])
                            .pick_file()
                            .await
                        else {
                            return Ok(None);
                        };
                        let path = file.path().to_owned();
                        tokio::task::spawn_blocking(move || {
                            reshiki::pictures::Picture::open(&path).map(Some)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    move |result| {
                        Message::Assistant(Action::ImageRead {
                            serial,
                            epoch,
                            image_only: true,
                            result,
                        })
                    },
                );
            }
            Action::ImageRead {
                serial,
                epoch,
                image_only,
                result,
            } => {
                if serial != self.assistant.image_serial {
                    return Task::none();
                }
                self.assistant.reading_image = false;
                if epoch != self.tab.file_epoch {
                    return Task::none();
                }
                match result {
                    Ok(Some(_)) if self.assistant.busy => {
                        self.assistant.status =
                            "Finish or stop the current request, then paste the image again."
                                .into();
                    }
                    Ok(Some(image)) => {
                        // Keep the exact source for follow-up requests and show it in the composer.
                        self.assistant.source_image = Some(image);
                        self.assistant.error = false;
                        self.assistant.status =
                            "Image attached · Add instructions or Send to draw its structure."
                                .into();
                    }
                    Ok(None) if !image_only => {
                        self.assistant.reading_image = true;
                        return iced::clipboard::read().map(move |text| {
                            Message::Assistant(Action::TextPasted {
                                serial,
                                epoch,
                                text,
                            })
                        });
                    }
                    Ok(None) => {
                        self.assistant.status =
                            "Copy an image, then choose Paste image, or open an image file.".into();
                    }
                    Err(error) => {
                        self.assistant.error = true;
                        self.assistant.status = format!("Could not read image: {error}");
                    }
                }
            }
            Action::TextPasted {
                serial,
                epoch,
                text,
            } => {
                if serial != self.assistant.image_serial {
                    return Task::none();
                }
                self.assistant.reading_image = false;
                if epoch == self.tab.file_epoch
                    && let Some(text) = text
                {
                    self.assistant.input.perform(text_editor::Action::Edit(
                        text_editor::Edit::Paste(std::sync::Arc::new(text)),
                    ));
                }
            }
            Action::ClearImage => {
                self.assistant.menu = None;
                self.assistant.source_image = None;
                self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
                self.assistant.reading_image = false;
            }
            Action::Example(value) => {
                self.assistant.input = text_editor::Content::with_text(value);
            }
            Action::Replace(value) => self.assistant.replace = value,
            Action::AutoApply(value) => {
                if !value {
                    self.assistant.waiting_for_canvas_edit = false;
                }
                self.assistant.preferences.auto_apply = value;
                self.assistant.preferences_dirty = true;
                self.assistant.menu = None;
                if value
                    && self
                        .assistant
                        .draft
                        .as_ref()
                        .is_some_and(|d| d.review.can_auto_apply())
                    && !self.assistant.busy
                {
                    return self.assistant_action(Action::Apply);
                }
            }
            Action::Model(value) => {
                self.assistant.preferences.model = value;
                self.assistant.preferences_dirty = true;
                self.assistant.menu = None;
            }
            Action::Menu(value) => {
                self.assistant.menu = if self.assistant.menu == value {
                    None
                } else {
                    value
                };
                self.assistant.search.clear();
            }
            Action::Search(value) => self.assistant.search = value,
            Action::ChatScrolled { follow, offset } => {
                self.assistant.follow_chat = follow;
                self.assistant.chat_offset = offset;
            }
            Action::JumpToResult => self.assistant.follow_chat = true,
            Action::Effort(value) => {
                if let Some(id) = self.assistant.model().map(|m| m.id.clone()) {
                    self.assistant.preferences.efforts.insert(id, value);
                    self.assistant.preferences_dirty = true;
                }
                self.assistant.menu = None;
            }
            Action::Tier(value) => {
                if let Some(id) = self.assistant.model().map(|m| m.id.clone()) {
                    self.assistant.preferences.tiers.insert(id, value);
                    self.assistant.preferences_dirty = true;
                }
                self.assistant.menu = None;
            }
            Action::PreferencesSaved(result) => {
                self.assistant.preferences_saving = false;
                if let Err(error) = result {
                    self.assistant.status =
                        format!("Could not save assistant preferences: {error}");
                    self.assistant.error = true;
                }
            }
            Action::Reset => {
                self.assistant.tab = None;
                self.assistant.canvas = None;
                self.assistant.pending_scope = None;
                self.assistant.pending_proposal = None;
                self.assistant.viewed_image = None;
                self.assistant.source_image = None;
                self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
                self.assistant.reading_image = false;
                self.assistant.waiting_for_canvas_edit = false;
                self.assistant.cancel.stop();
                let serial = self.assistant.serial.wrapping_add(1);
                self.assistant.draft = None;
                self.assistant.preview = None;
                self.assistant.plan.clear();
                self.assistant.completed = None;
                self.assistant.messages.clear();
                self.assistant.status.clear();
                self.assistant.error = false;
                self.assistant.busy = false;
                self.assistant.serial = serial;
                self.assistant.progress = None;
                self.assistant.reply.clear();
                self.assistant.started = None;
                self.assistant.chat_offset = 0.;
                self.assistant.follow_chat = true;
            }
            Action::Stop => {
                self.assistant.image_serial = self.assistant.image_serial.wrapping_add(1);
                self.assistant.reading_image = false;
                self.assistant.waiting_for_canvas_edit = false;
                self.assistant.cancel.stop();
                self.assistant.serial = self.assistant.serial.wrapping_add(1);
                self.assistant.busy = false;
                self.assistant.progress = None;
                self.assistant.reply.clear();
                self.assistant.elapsed = self
                    .assistant
                    .started
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                self.assistant.started = None;
                self.assistant.retain_preview(
                    "Quality review has not finished. Review the draft before applying.",
                );
                self.assistant.status = "Stopped · Completed previews are retained".into();
                self.assistant.record(
                    "ReShiki",
                    "Stopped. Completed previews remain available below.".into(),
                );
            }
            Action::Connect => {
                if self.assistant.busy {
                    return Task::none();
                }
                self.assistant.serial = self.assistant.serial.wrapping_add(1);
                self.assistant.cancel = Default::default();
                self.assistant.busy = true;
                self.assistant.error = false;
                self.assistant.status = "Connecting to Codex…".into();
                self.assistant.started = Some(std::time::Instant::now());
                let serial = self.assistant.serial;
                let cancel = self.assistant.cancel.clone();
                return Task::perform(codex::connect(cancel), move |result| {
                    Message::Assistant(Action::Connected(serial, result))
                });
            }
            Action::Connected(serial, result) => {
                if serial != self.assistant.serial {
                    return Task::none();
                }
                self.assistant.busy = false;
                self.assistant.started = None;
                match result {
                    Ok(account) => {
                        self.assistant.error = !account.connected;
                        self.assistant.status = if account.connected {
                            "Connected to Codex".into()
                        } else {
                            "Run `codex login` to sign in, then reconnect.".into()
                        };
                        self.assistant.account = Some(account);
                    }
                    Err(error) => {
                        self.assistant.status = error;
                        self.assistant.error = true;
                    }
                }
            }
            Action::Poll => {
                if self.assistant.waiting_for_canvas_edit
                    && self.tab.inline_text.is_none()
                    && self.tab.joining.is_none()
                {
                    self.assistant.waiting_for_canvas_edit = false;
                    return self.assistant_action(Action::Apply);
                }
                if self.assistant.busy
                    && let Some(canvas) = &self.assistant.canvas
                    && let Ok(mut snapshot) = canvas.write()
                    && (snapshot.revision != self.tab.revision
                        || snapshot.epoch != self.tab.file_epoch
                        || snapshot.selected != self.tab.selected)
                {
                    *snapshot = assistant::canvas_tools::Snapshot {
                        document: self.tab.doc.clone(),
                        selected: self.tab.selected.clone(),
                        revision: self.tab.revision,
                        epoch: self.tab.file_epoch,
                    };
                }
                if let Some(progress) = &mut self.assistant.progress {
                    while let Ok(message) = progress.try_recv() {
                        if matches!(
                            message,
                            codex::Progress::Preview(_)
                                | codex::Progress::Plan(_)
                                | codex::Progress::Checking { .. }
                        ) && self.assistant.follow_chat
                        {
                            scroll = true;
                        }
                        self.assistant.last_activity = Some(std::time::Instant::now());
                        match message {
                            codex::Progress::Proposal(proposal) => {
                                if !proposal.replace_ids.is_empty()
                                    && let Some((_, _, ids)) = &mut self.assistant.pending_scope
                                {
                                    *ids = proposal.replace_ids.clone();
                                }
                                self.assistant.pending_proposal = Some(*proposal);
                            }
                            codex::Progress::Plan(plan) => {
                                if self.assistant.plan.is_empty() {
                                    self.assistant.plan = plan;
                                }
                                self.assistant.status = "Composition planned".into();
                            }
                            codex::Progress::Structures { completed, total } => {
                                self.assistant.structures = Some((completed, total));
                                self.assistant.status = "Preparing structures…".into();
                            }
                            codex::Progress::Preview(doc) => {
                                self.assistant.preview = Some(*doc);
                                self.assistant.status = "Layout assembled".into();
                            }
                            codex::Progress::Checking { pass } => {
                                self.assistant.checking = pass;
                                self.assistant.status =
                                    format!("Checking the rendered draft · Pass {pass} of 3");
                            }
                            codex::Progress::Status(message) => self.assistant.status = message,
                            codex::Progress::Catalog(account) => {
                                self.assistant.account = Some(account)
                            }
                            codex::Progress::Reply(reply) => self.assistant.reply = reply,
                            codex::Progress::Started { model, effort } => {
                                self.assistant.running_model =
                                    format!("{model} · {}", effort_label(&effort));
                            }
                        }
                    }
                }
                if self.assistant.preferences_dirty
                    && !self.assistant.preferences_saving
                    && !cfg!(test)
                {
                    self.assistant.preferences_dirty = false;
                    self.assistant.preferences_saving = true;
                    let preferences = self.assistant.preferences.clone();
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || preferences.save())
                                .await
                                .map_err(|e| e.to_string())?
                        },
                        |result| Message::Assistant(Action::PreferencesSaved(result)),
                    );
                }
            }
            Action::Reject => {
                self.assistant.waiting_for_canvas_edit = false;
                self.assistant.draft = None;
                self.assistant.preview = None;
                self.assistant.record(
                    "ReShiki",
                    "Proposal rejected. The drawing was not changed.".into(),
                );
                self.assistant.status = "Rejected · Your drawing is unchanged".into();
            }
            Action::Apply => {
                scroll = self.assistant.follow_chat;
                if self.assistant.busy || self.tab.cleanup.is_some() {
                    return Task::none();
                }
                if self.tab.inline_text.is_some() || self.tab.joining.is_some() {
                    self.assistant.waiting_for_canvas_edit = true;
                    self.assistant.status =
                        "Ready · Finish or cancel the current canvas edit to apply".into();
                    return Task::none();
                }
                let Some(draft) = &self.assistant.draft else {
                    return Task::none();
                };
                if draft.epoch != self.tab.file_epoch
                    || (!draft.replace.is_empty() && draft.revision != self.tab.revision)
                {
                    self.assistant.status = "This proposal targets a drawing or selection that changed. Send a follow-up to refresh it.".into();
                    self.assistant.error = true;
                    return Task::none();
                }
                match assistant::candidate(&self.tab.doc, &draft.fragment, &draft.replace) {
                    Ok((document, ids)) => {
                        let before = self.tab.doc.clone();
                        self.tab.doc = document;
                        self.tab.selected = ids;
                        self.changed(before);
                        self.assistant.completed = self
                            .assistant
                            .draft
                            .as_ref()
                            .map(|d| (d.fragment.clone(), d.review.clone()));
                        self.assistant.draft = None;
                        self.tool = crate::canvas::Tool::Select;
                        if let Some((lo, hi)) =
                            reshiki::scene::selection_bounds(&self.tab.doc, &self.tab.selected)
                        {
                            self.tab.camera.center = reshiki::document::Point::new(
                                (lo.x + hi.x) / 2.,
                                (lo.y + hi.y) / 2.,
                            );
                            let paper =
                                self.guides.paper(iced::Rectangle::with_size(self.viewport));
                            self.tab.camera.zoom = ((paper.width - 70.).max(100.)
                                / (hi.x - lo.x).max(240.))
                            .min((paper.height - 70.).max(100.) / (hi.y - lo.y).max(180.))
                            .clamp(0.05, 2.5);
                            self.tab.fit_to_view = false;
                        }
                        self.assistant.record(
                            "ReShiki",
                            "Applied to the drawing. Undo restores the previous drawing.".into(),
                        );
                        self.assistant.status = "Applied · Undo is available".into();
                        self.status =
                            "Assistant drawing applied · Undo restores the previous drawing".into();
                    }
                    Err(error) => {
                        self.assistant.error = true;
                        self.assistant.status = error;
                    }
                }
            }
            Action::PreviewTarget(target) => {
                self.assistant.preview_target = (target != "Overview").then_some(target);
            }
            Action::PreviewEdit(edit) => {
                self.assistant.waiting_for_canvas_edit = false;
                if self.assistant.busy {
                    let _ = self.assistant_action(Action::Stop);
                }
                if let Some(draft) = &mut self.assistant.draft {
                    match assistant::review::apply(
                        &draft.fragment,
                        &[edit],
                        !draft.proposal.composition.preserve_details,
                    ) {
                        Ok(doc) => {
                            draft.fragment = doc.clone();
                            draft.review.verified = false;
                            draft.review.summary =
                                "Draft edited. Run Improve layout to check this version.".into();
                            draft.review.issues =
                                vec!["This edited version has not been visually checked.".into()];
                            self.assistant.preview = Some(doc);
                        }
                        Err(error) => {
                            self.assistant.status = error;
                            self.assistant.error = true;
                        }
                    }
                }
            }
            action @ (Action::Send | Action::Improve) => {
                let improving = matches!(action, Action::Improve);

                if self.assistant.busy || self.assistant.reading_image || self.tab.cleanup.is_some()
                {
                    return Task::none();
                }
                let prompt = self.assistant.input.text().trim().to_string();
                let prompt = if improving && prompt.is_empty() {
                    self.assistant.messages.iter().rev().find(|m| m.role == "You").map(|m| m.text.clone()).unwrap_or_else(|| "Improve this scheme’s spacing, alignment and captions while preserving all chemistry and structural detail.".into())
                } else if prompt.is_empty() && self.assistant.source_image.is_some() {
                    "Draw the molecular structures shown in the attached image as editable objects, preserving the depicted chemistry and arrangement.".into()
                } else {
                    prompt
                };
                if prompt.is_empty() {
                    return Task::none();
                }
                self.assistant.waiting_for_canvas_edit = false;
                if prompt.len() > 12_000 {
                    self.assistant.error = true;
                    self.assistant.status =
                        "Please shorten your request to 12,000 characters".into();
                    return Task::none();
                }
                if !improving && self.assistant.replace && self.tab.selected.is_empty() {
                    self.assistant.error = true;
                    self.assistant.status = "Select the objects to replace first".into();
                    return Task::none();
                }
                let context = if self.tab.selected.is_empty() {
                    self.tab.doc.clone()
                } else {
                    reshiki::editing::selection(&self.tab.doc, &self.tab.selected)
                };
                if context.atoms.len() > 1000 {
                    self.assistant.error = true;
                    self.assistant.status =
                        "Select a smaller part of the drawing to share with Codex".into();
                    return Task::none();
                }
                self.assistant.tab = Some(self.tab.id);
                let settings = DrawingSettings {
                    drawing_style: self.tab.doc.drawing_style.clone(),
                    format: self.tab.caption_format.clone(),
                    bond_length: self.tab.bond_drawing.length,
                    // The field shows display colors; the canvas ink stays Ink.
                    bond_color: super::graphics::parse_color(&self.tab.bond_color_input)
                        .filter(|rgb| *rgb != self.tab.doc.canvas_theme.color([0; 3]))
                        .map_or(
                            reshiki::palette::Color::Ink,
                            reshiki::palette::Color::Custom,
                        ),
                    arrow_style: self.tab.arrows.style.clone(),
                    labels: self.tab.doc.atom_labels.clone(),
                };
                let request = json!({"request":prompt,"conversation":self.assistant.conversation(),"previous_proposal":self.assistant.draft.as_ref().map(|d|&d.proposal),"drawing_summary":{"atoms":context.atoms.len(),"bonds":context.bonds.len(),"arrows":context.arrows.len()},"selected_ids":self.tab.selected,"placement":if self.assistant.replace {"replace selected objects"} else {"add new drawing objects"},"style":{"name":self.tab.doc.drawing_style.name,"bond_length_pt":self.tab.bond_drawing.length * reshiki::style::DEFAULT.points_per_world(),"text":settings.format,"bond_color":settings.bond_color}}).to_string();
                let seed = if improving {
                    Some(if let Some(draft) = &self.assistant.draft {
                        if draft.epoch != self.tab.file_epoch
                            || (!draft.replace.is_empty() && draft.revision != self.tab.revision)
                        {
                            self.assistant.status = "Your drawing changed. Discard this draft, then review the current selection.".into();
                            self.assistant.error = true;
                            return Task::none();
                        }
                        assistant::review::Outcome {
                            proposal: draft.proposal.clone(),
                            document: draft.fragment.clone(),
                            review: Default::default(),
                        }
                    } else {
                        let ids = assistant::review::scope(&self.tab.doc, &self.tab.selected);
                        let mut proposal = Proposal {
                            replace_ids: ids.clone(),
                            ..Default::default()
                        };
                        proposal.composition.preserve_details = true;
                        assistant::review::Outcome {
                            proposal,
                            document: assistant::review::fragment(&self.tab.doc, &ids),
                            review: Default::default(),
                        }
                    })
                } else {
                    None
                };
                if seed
                    .as_ref()
                    .is_some_and(|s| s.document.all_ids().is_empty())
                {
                    self.assistant.status = "Draw or select a scheme to improve first".into();
                    return Task::none();
                }
                let replace = if improving {
                    self.assistant
                        .draft
                        .as_ref()
                        .map(|d| d.replace.clone())
                        .unwrap_or_else(|| {
                            assistant::review::scope(&self.tab.doc, &self.tab.selected)
                        })
                } else if self.assistant.replace {
                    self.tab
                        .doc
                        .expand_abbreviation_selection(&self.tab.selected)
                } else {
                    vec![]
                };
                self.assistant.record(
                    "You",
                    if improving {
                        format!("Improve layout: {prompt}")
                    } else {
                        prompt
                    },
                );
                self.assistant.input = text_editor::Content::new();
                self.assistant.serial = self.assistant.serial.wrapping_add(1);
                self.assistant.cancel = Default::default();
                self.assistant.busy = true;
                self.assistant.error = false;
                self.assistant.status = "Preparing your scheme…".into();
                self.assistant.completed = None;
                self.assistant.follow_chat = true;
                self.assistant.preview = seed.as_ref().map(|s| s.document.clone());
                self.assistant.draft = None;
                self.assistant.pending_proposal = seed.as_ref().map(|s| s.proposal.clone());
                self.assistant.plan.clear();
                self.assistant.preview_target = None;
                self.assistant.structures = None;
                self.assistant.checking = 0;
                self.assistant.last_activity = Some(std::time::Instant::now());
                self.assistant.pending_scope =
                    Some((self.tab.file_epoch, self.tab.revision, replace.clone()));
                let (tx, rx) = tokio::sync::mpsc::channel(32);
                self.assistant.progress = Some(rx);
                let cancel = self.assistant.cancel.clone();
                let serial = self.assistant.serial;
                let revision = self.tab.revision;
                let epoch = self.tab.file_epoch;
                let preferences = self.assistant.preferences.clone();
                let source_image = self.assistant.source_image.clone();
                let shared = std::sync::Arc::new(std::sync::RwLock::new(
                    assistant::canvas_tools::Snapshot {
                        document: self.tab.doc.clone(),
                        selected: self.tab.selected.clone(),
                        revision,
                        epoch,
                    },
                ));
                self.assistant.canvas = Some(shared.clone());
                let canvas = assistant::canvas_tools::CanvasTools {
                    canvas: shared,
                    settings: settings.clone(),
                    replace: replace.clone(),
                    epoch,
                    revision,
                };
                self.assistant.reply.clear();
                self.assistant.running_model.clear();
                self.assistant.started = Some(std::time::Instant::now());
                self.assistant.menu = None;
                // Generation and visual review run independently of the editor’s chemistry stream.
                return Task::batch([
                    iced::widget::operation::snap_to_end("assistant-chat"),
                    Task::perform(
                        async move {
                            if let Some(seed) = seed {
                                codex::improve_with_image(
                                    request,
                                    preferences,
                                    cancel,
                                    tx,
                                    canvas,
                                    seed,
                                    source_image,
                                )
                                .await
                            } else if let Some(image) = source_image {
                                codex::propose_image(
                                    request,
                                    preferences,
                                    cancel,
                                    tx,
                                    Some(canvas),
                                    image,
                                )
                                .await
                            } else {
                                codex::propose(request, preferences, cancel, tx, Some(canvas)).await
                            }
                        },
                        move |result| {
                            Message::Assistant(Action::Done {
                                serial,
                                epoch,
                                revision,
                                replace: replace.clone(),
                                result: Box::new(result),
                            })
                        },
                    ),
                ]);
            }
            Action::Done {
                serial,
                epoch,
                revision,
                replace,
                result,
            } => {
                if serial != self.assistant.serial {
                    return Task::none();
                }
                scroll = self.assistant.follow_chat;
                self.assistant.busy = false;
                self.assistant.progress = None;
                self.assistant.reply.clear();
                self.assistant.elapsed = self
                    .assistant
                    .started
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                self.assistant.started = None;
                match *result {
                    Ok(assistant::review::Outcome {
                        proposal,
                        document: fragment,
                        review,
                    }) => {
                        let replace = if proposal.replace_ids.is_empty() {
                            replace
                        } else {
                            proposal.replace_ids.clone()
                        };
                        self.assistant.record("Codex", proposal.explanation.clone());
                        if !fragment.all_ids().is_empty() {
                            self.assistant.preview = None;
                            let can_auto_apply = review.can_auto_apply();
                            self.assistant.draft = Some(Draft {
                                proposal,
                                fragment,
                                review,
                                revision,
                                epoch,
                                replace,
                            });
                            self.assistant.status =
                                "Ready for review · Nothing has been applied".into();
                            if self.assistant.preferences.auto_apply && can_auto_apply {
                                return self.assistant_action(Action::Apply);
                            }
                        } else {
                            self.assistant.draft = None;
                            self.assistant.status = "Waiting for your reply".into();
                        }
                    }
                    Err(error) => {
                        self.assistant
                            .retain_preview(&format!("Generation or review stopped: {error}"));
                        self.assistant.status = error;
                        self.assistant.error = true;
                    }
                }
            }
        }
        if scroll {
            iced::widget::operation::snap_to_end("assistant-chat")
        } else {
            Task::none()
        }
    }
    fn assistant_preview_controls<'a>(&'a self, doc: &'a Document) -> Element<'a, Message> {
        let targets = assistant::review::targets(doc);
        let labels: Vec<_> = std::iter::once("Overview".to_string())
            .chain(targets.iter().map(|t| t.label()))
            .collect();
        let selected = self
            .assistant
            .preview_target
            .as_ref()
            .filter(|s| labels.contains(s));
        let mut controls = column![
            crate::appearance::pick_list(
                labels.clone(),
                Some(selected.cloned().unwrap_or_else(|| "Overview".into())),
                |s| Message::Assistant(Action::PreviewTarget(s))
            )
            .placeholder("Inspect or edit a panel…")
            .text_size(11)
            .width(Length::Fill)
        ]
        .spacing(6);
        if let Some(target) =
            selected.and_then(|label| targets.iter().find(|t| t.label() == *label))
        {
            let moving = |label, dx_pt, dy_pt| {
                action(
                    label,
                    Action::PreviewEdit(assistant::review::Edit::Move {
                        target: target.name.clone(),
                        dx_pt,
                        dy_pt,
                    }),
                )
                .padding([4, 8])
            };
            let mut buttons = row![
                text("Move").size(11),
                moving("←", -6., 0.),
                moving("→", 6., 0.),
                moving("↑", 0., -6.),
                moving("↓", 0., 6.)
            ]
            .spacing(3)
            .align_y(Alignment::Center);
            if target.kind == "arrow" {
                let length = doc
                    .arrows
                    .iter()
                    .find(|a| target.ids.contains(&a.id))
                    .map(|a| a.start.distance(a.end) * reshiki::style::DEFAULT.points_per_world())
                    .unwrap_or(40.);
                buttons = buttons.push(
                    action(
                        "Shorten",
                        Action::PreviewEdit(assistant::review::Edit::ArrowLength {
                            target: target.name.clone(),
                            length_pt: (length - 6.).max(12.),
                        }),
                    )
                    .padding([4, 6]),
                );
            }
            controls = controls.push(buttons);
        }
        controls.into()
    }
    fn assistant_preview_canvas<'a>(&'a self, doc: &'a Document) -> Element<'a, Message> {
        if let Some(target) = self.assistant.preview_target.as_ref().and_then(|label| {
            assistant::review::targets(doc)
                .into_iter()
                .find(|t| t.label() == *label)
        }) {
            let ids = if target.kind == "panel" {
                target
                    .name
                    .strip_prefix("reaction:")
                    .and_then(|s| s.parse::<usize>().ok())
                    .and_then(|i| doc.reactions.get(i))
                    .map(|r| r.ids())
                    .unwrap_or(target.ids)
            } else {
                target.ids
            };
            return canvas(crate::canvas::OwnedDrawingPreview(
                assistant::review::fragment(doc, &ids),
            ))
            .width(Length::Fill)
            .height(200)
            .into();
        }
        canvas(crate::canvas::DrawingPreview(doc))
            .width(Length::Fill)
            .height(200)
            .into()
    }
    pub(super) fn assistant_panel(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        if let Some(id) = state.tab.filter(|id| *id != self.tab.id) {
            let name = self.strip().find(|tab| tab.id == id).map(|tab| tab.name());
            return column![
                text(format!(
                    "This conversation belongs to {}.",
                    name.unwrap_or_else(|| "a closed drawing".into())
                ))
                .size(13),
                button(text("Go to drawing").size(12))
                    .on_press(Message::Tabs(super::tabs::Action::Select(id))),
                action("New conversation here", Action::Reset),
            ]
            .spacing(12)
            .padding(12)
            .into();
        }
        let mut chat = column![].spacing(12).padding([2, 2]).width(Length::Fill);
        if state.messages.is_empty() {
            chat = chat.push(Space::new().height(20))
                .push(text("What would you like to draw?").size(19))
                .push(text("Molecules, reactions, or a starting point for your next scheme.").size(13).style(super::workspace::muted_text))
                .push(action("Draw a molecule", Action::Example("Draw caffeine.")))
                .push(action("Build a reaction", Action::Example("Draw the esterification of acetic acid with ethanol to ethyl acetate. Put H₂SO₄ and heat above the arrow.")))
                .push(text("Review each proposal before applying. Changes stay editable and can be undone.").size(11).style(super::workspace::muted_text));
        }
        for message in &state.messages {
            let role = &message.role;
            if role == "You" {
                let mut content =
                    column![text(&message.text).size(13).width(Length::Fill)].spacing(8);
                if let Some(source) = &message.image
                    && let Some(handle) = source.handle(false)
                {
                    content = content
                        .push(
                            button(
                                iced::widget::image(handle)
                                    .width(Length::Fill)
                                    .height(140)
                                    .content_fit(iced::ContentFit::Contain),
                            )
                            .padding(4)
                            .width(Length::Fill)
                            .style(super::workspace::control(false))
                            .on_press(Message::Assistant(Action::ViewImage(Some(source.clone())))),
                        )
                        .push(
                            text("Sent image · Click to enlarge")
                                .size(11)
                                .style(super::workspace::muted_text),
                        );
                }
                chat = chat.push(
                    container(content)
                        .padding([10, 12])
                        .width(Length::Fill)
                        .style(|theme| {
                            crate::appearance::container(
                                theme,
                                surface(Color::from_rgb8(234, 241, 239), 12.),
                            )
                        }),
                );
            } else if role == "ReShiki" {
                chat = chat.push(
                    text(&message.text)
                        .size(11)
                        .style(super::workspace::muted_text),
                );
            } else {
                chat = chat.push(
                    column![
                        text("Codex").size(11).style(crate::appearance::text_color(
                            Color::from_rgb8(17, 126, 108)
                        )),
                        text(&message.text).size(13).width(Length::Fill)
                    ]
                    .spacing(6),
                );
            }
        }
        if !state.reply.is_empty() && state.busy {
            chat =
                chat.push(
                    column![
                        text("Codex").size(11).style(crate::appearance::text_color(
                            Color::from_rgb8(17, 126, 108)
                        )),
                        text(&state.reply).size(13).width(Length::Fill)
                    ]
                    .spacing(6),
                );
        }
        if state.busy {
            let seconds = state.started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            let mut activity = column![
                row![
                    text(&state.status).size(13).width(Length::Fill),
                    action("Stop", Action::Stop)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(format!(
                    "Elapsed {seconds}s · You can keep drawing or close this panel"
                ))
                .size(11)
                .style(super::workspace::muted_text),
            ]
            .spacing(10);
            if !state.running_model.is_empty() {
                activity = activity.push(
                    text(&state.running_model)
                        .size(10)
                        .style(super::workspace::muted_text),
                );
            }
            if !state.plan.is_empty() {
                activity = activity.push(text(&state.plan).size(12));
            }
            if let Some((completed, total)) = state.structures {
                activity = activity
                    .push(text(format!("{completed} of {total} structures prepared")).size(11));
                if total > 0 {
                    activity = activity.push(
                        iced::widget::progress_bar(0. ..=total as f32, completed as f32).girth(3),
                    );
                }
            }
            if state
                .last_activity
                .is_some_and(|t| t.elapsed().as_secs() >= 8)
            {
                activity = activity.push(
                    text("Generation is still running. Waiting for the next completed step…")
                        .size(11)
                        .style(super::workspace::muted_text),
                );
            }
            if let Some(doc) = &state.preview {
                activity = activity.push(self.assistant_preview_canvas(doc));
                activity = activity.push(
                    text("Editable draft · Changes here stop generation and retain this preview.")
                        .size(11),
                );
                activity = activity.push(self.assistant_preview_controls(doc));
            }
            chat = chat.push(
                container(activity)
                    .padding(12)
                    .width(Length::Fill)
                    .style(|theme| crate::appearance::container(theme, card())),
            );
        } else if state.draft.is_none()
            && let Some(doc) = &state.preview
        {
            chat = chat.push(
                container(column![
                    text("Retained preview").size(12),
                    self.assistant_preview_canvas(doc),
                    text("Review did not finish. The completed preview is retained.").size(11)
                ])
                .padding(12)
                .style(|theme| crate::appearance::container(theme, card())),
            );
        }
        if let Some(draft) = &state.draft {
            let current = draft.epoch == self.tab.file_epoch
                && (draft.replace.is_empty() || draft.revision == self.tab.revision);
            let mut proposal = column![
                text(if draft.review.can_auto_apply() {
                    "Quality checked"
                } else {
                    "Draft · Review needed"
                })
                .size(13),
                self.assistant_preview_canvas(&draft.fragment),
                text(if draft.replace.is_empty() {
                    "Adds editable objects to your drawing"
                } else {
                    "Replaces the targeted objects; keeps the rest of your drawing"
                })
                .size(11)
                .style(super::workspace::muted_text)
            ]
            .spacing(10);
            for change in &draft.review.changes {
                proposal = proposal.push(text(change).size(12));
            }
            proposal = proposal.push(text(&draft.review.summary).size(12));
            for issue in &draft.review.issues {
                proposal = proposal.push(
                    text(format!("• {issue}"))
                        .size(11)
                        .style(crate::appearance::text_color(Color::from_rgb8(151, 86, 24))),
                );
            }
            proposal = proposal.push(self.assistant_preview_controls(&draft.fragment));
            if !current {
                proposal = proposal.push(
                    text("Your drawing changed. Send a follow-up to refresh this proposal.")
                        .size(11),
                );
            }
            proposal = proposal.push(
                row![
                    action("Apply", Action::Apply)
                        .on_press_maybe(
                            (current && !state.busy && self.tab.cleanup.is_none())
                                .then_some(Message::Assistant(Action::Apply))
                        )
                        .style(crate::appearance::primary),
                    action("Improve layout", Action::Improve).on_press_maybe(
                        (current && !state.busy).then_some(Message::Assistant(Action::Improve))
                    ),
                    action("Discard", Action::Reject).on_press_maybe(
                        (!state.busy).then_some(Message::Assistant(Action::Reject))
                    )
                ]
                .spacing(6),
            );
            chat = chat.push(
                container(proposal)
                    .padding(12)
                    .style(|theme| crate::appearance::container(theme, card())),
            );
        }
        if let Some((doc, report)) = &state.completed {
            let mut result = column![
                text(format!("Applied · Elapsed {}s", state.elapsed)).size(12),
                canvas(crate::canvas::DrawingPreview(doc))
                    .width(Length::Fill)
                    .height(180)
            ]
            .spacing(8);
            for change in &report.changes {
                result = result.push(text(change).size(12).width(Length::Fill));
            }
            result = result.push(text(&report.summary).size(12).width(Length::Fill));
            for issue in &report.issues {
                result = result.push(text(format!("• {issue}")).size(11).width(Length::Fill));
            }
            chat = chat.push(
                container(result)
                    .padding(12)
                    .width(Length::Fill)
                    .style(|theme| crate::appearance::container(theme, card())),
            );
        }
        if !state.busy && state.draft.is_none() && !self.tab.doc.all_ids().is_empty() {
            chat = chat.push(action(
                if self.tab.selected.is_empty() {
                    "Improve drawing layout"
                } else {
                    "Improve selected layout"
                },
                Action::Improve,
            ));
        }
        let header = row![
            super::workspace::hover_hint(
                button(text("‹").size(18))
                    .padding([3, 8])
                    .style(super::workspace::control(false))
                    .on_press(Message::Inspector(InspectorTab::Properties)),
                "Back to inspector",
                tooltip::Position::Bottom
            ),
            text("Assistant").size(16),
            Space::new().width(Length::Fill),
            super::workspace::hover_hint(
                action("＋", Action::Reset),
                "New conversation",
                tooltip::Position::Bottom
            ),
            super::workspace::hover_hint(
                button(text("×").size(18))
                    .padding([3, 8])
                    .style(super::workspace::control(false))
                    .on_press(Message::ToggleInspector),
                "Close assistant",
                tooltip::Position::Bottom
            )
        ]
        .align_y(Alignment::Center);
        let connected = state.account.as_ref().is_some_and(|a| a.connected);
        let mut activity = column![].spacing(6);
        if state.busy {
            let seconds = state.started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            activity = activity.push(
                text(format!(
                    "{} · {seconds}s",
                    if state.checking > 0 {
                        "Checking draft"
                    } else if state.structures.is_some() {
                        "Preparing structures"
                    } else {
                        "Preparing scheme"
                    }
                ))
                .size(11)
                .style(super::workspace::muted_text),
            );
        }
        if state.error {
            activity = activity.push(
                text(&state.status)
                    .size(11)
                    .style(crate::appearance::text_color(Color::from_rgb8(175, 54, 54))),
            );
        }
        let model_label = match state.model() {
            Some(model) => model.label.clone(),
            None => state
                .preferences
                .model
                .clone()
                .unwrap_or_else(|| "GPT-6 Astra".into()),
        };
        let effort = state
            .model()
            .map(|m| effort_label(state.preferences.effort(m)))
            .unwrap_or("Reasoning");
        let editor = text_editor(&state.input)
            .id("assistant-input")
            .placeholder(if state.messages.is_empty() {
                "Describe a molecule, or paste an image…"
            } else {
                "Ask for changes or another drawing…"
            })
            .on_action(|a| Message::Assistant(Action::Input(a)))
            .key_binding(|key| {
                if !matches!(key.status, text_editor::Status::Focused { .. }) {
                    return text_editor::Binding::from_key_press(key);
                }
                if key.modifiers.command()
                    && key.key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
                {
                    Some(text_editor::Binding::Custom(Message::Assistant(
                        Action::Send,
                    )))
                } else if key.modifiers.command() && matches!(&key.key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                    Some(text_editor::Binding::Custom(Message::Assistant(Action::Paste { image_only:false })))
                } else {
                    text_editor::Binding::from_key_press(key)
                }
            })
            .size(13)
            .height(64)
            .padding(4)
            .style(|theme, _| text_editor::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                placeholder: crate::appearance::muted(theme),
                value: crate::appearance::themed(theme, Color::from_rgb8(37, 46, 48)),
                selection: crate::appearance::themed(theme, Color::from_rgb8(198, 223, 215)),
            });
        let mut input_row = row![editor].spacing(8).align_y(Alignment::Center);
        if let Some(source) = &state.source_image
            && let Some(handle) = source.handle(false)
        {
            input_row = input_row.push(super::workspace::hover_hint(
                button(
                    iced::widget::image(handle)
                        .width(46)
                        .height(46)
                        .content_fit(iced::ContentFit::Contain),
                )
                .padding(3)
                .style(super::workspace::control(false))
                .on_press(Message::Assistant(Action::Menu(Some(Menu::Attachments)))),
                format!(
                    "Attached image · {} × {} · Click for options",
                    source.width(),
                    source.height()
                ),
                tooltip::Position::Top,
            ));
        }
        let submit = if state.busy {
            action("■ Stop", Action::Stop)
        } else {
            action("↑ Send", Action::Send)
                .style(crate::appearance::primary)
                .on_press_maybe(
                    ((!state.input.text().trim().is_empty() || state.source_image.is_some())
                        && !state.reading_image
                        && self.tab.cleanup.is_none())
                    .then_some(Message::Assistant(Action::Send)),
                )
        };
        let connection = super::workspace::hover_hint(
            action(
                if connected {
                    "● Codex"
                } else {
                    "Connect Codex"
                },
                Action::Connect,
            )
            .on_press_maybe((!state.busy).then_some(Message::Assistant(Action::Connect))),
            if connected {
                "Connected · Click to refresh models"
            } else {
                "Connect using your Codex sign-in"
            },
            tooltip::Position::Top,
        );
        let attach = super::workspace::hover_hint(
            action("＋", Action::Menu(Some(Menu::Attachments))).padding([5, 8]),
            "Attach or paste an image",
            tooltip::Position::Top,
        );
        let composer = container(
            column![
                input_row,
                row![
                    attach,
                    action(format!("{model_label} ⌄"), Action::Menu(Some(Menu::Models)))
                        .padding([5, 3]),
                    Space::new().width(Length::Fill),
                    submit.padding([6, 10])
                ]
                .spacing(4)
                .align_y(Alignment::Center)
            ]
            .spacing(4),
        )
        .padding(8)
        .style(|theme| crate::appearance::container(theme, card()));
        let footer = column![
            activity,
            composer,
            row![
                connection,
                Space::new().width(Length::Fill),
                action(format!("{effort} ⌄"), Action::Menu(Some(Menu::Effort))).padding([3, 4]),
                action(
                    if state.preferences.auto_apply {
                        "Auto apply ⌄"
                    } else {
                        "Review ⌄"
                    },
                    Action::Menu(Some(Menu::Edits))
                )
                .padding([3, 4])
            ]
            .spacing(4)
            .align_y(Alignment::Center)
        ]
        .spacing(5);
        let result_navigation: Element<'_, Message> = if !state.follow_chat
            && (state.draft.is_some() || state.completed.is_some() || state.busy)
        {
            action(
                if state.busy {
                    "↓ Jump to activity"
                } else {
                    "↓ Jump to result"
                },
                Action::JumpToResult,
            )
            .width(Length::Fill)
            .into()
        } else {
            Space::new().height(0).into()
        };
        let base: Element<'_, Message> = container(
            column![
                header,
                scrollable(chat)
                    .id("assistant-chat")
                    .height(Length::Fill)
                    .on_scroll(|v| Message::Assistant(Action::ChatScrolled {
                        follow: v.content_bounds().height <= v.bounds().height
                            || v.relative_offset().y >= 0.97,
                        offset: v.absolute_offset().y,
                    })),
                result_navigation,
                footer
            ]
            .spacing(9),
        )
        .padding(10)
        .height(Length::Fill)
        .into();
        // Always keep the chat at the same location in the widget tree.
        let layers = stack![base];
        let Some(menu) = state.menu else {
            return layers.into();
        };
        let popup = container(self.assistant_menu(menu))
            .padding(10)
            .width(340)
            .style(|theme| {
                let mut style = card();
                style.shadow = crate::appearance::surface_shadow(iced::Shadow {
                    color: Color::from_rgba8(25, 40, 36, 0.16),
                    offset: iced::Vector::new(0., 4.),
                    blur_radius: 18.,
                });
                crate::appearance::container(theme, style)
            });
        layers
            .push(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .on_press(Message::Assistant(Action::Menu(None))),
            )
            .push(
                container(opaque(popup))
                    .height(Length::Fill)
                    .align_y(Alignment::End)
                    .padding(iced::Padding {
                        top: 8.,
                        right: 14.,
                        bottom: 110.,
                        left: 14.,
                    }),
            )
            .into()
    }
    fn assistant_menu(&self, menu: Menu) -> Element<'_, Message> {
        let state = &self.assistant;
        let mut options = column![].spacing(4);
        match menu {
            Menu::Attachments => {
                options = options.push(text("Reference image").size(12)).push(
                    action("Choose image…", Action::OpenImage)
                        .width(Length::Fill)
                        .on_press_maybe(
                            (!state.busy && !state.reading_image)
                                .then_some(Message::Assistant(Action::OpenImage)),
                        ),
                );
                if reshiki::clipboard::available() {
                    options =
                        options.push(
                            action("Paste image", Action::Paste { image_only: true })
                                .width(Length::Fill)
                                .on_press_maybe((!state.busy && !state.reading_image).then_some(
                                    Message::Assistant(Action::Paste { image_only: true }),
                                )),
                        );
                }
                if state.source_image.is_some() {
                    options = options.push(
                        action("Remove attached image", Action::ClearImage)
                            .width(Length::Fill)
                            .on_press_maybe(
                                (!state.busy).then_some(Message::Assistant(Action::ClearImage)),
                            ),
                    );
                }
                options = options.push(
                    rich_text(super::shortcuts::spans(&format!(
                        "Paste with {} · Send with {}",
                        super::shortcuts::label(&Message::Paste).unwrap_or_default(),
                        super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Enter")
                    )))
                    .size(11),
                );
            }
            Menu::Edits => {
                options = options
                    .push(
                        checkbox(state.replace)
                            .label("Replace selection")
                            .text_size(12)
                            .on_toggle(|v| Message::Assistant(Action::Replace(v))),
                    )
                    .push(iced::widget::rule::horizontal(1));
                options = options.push(
                    text("Assistant edits")
                        .size(12)
                        .style(super::workspace::muted_text),
                );
                for (label, description, auto) in [
                    (
                        "Review edits",
                        "Preview each proposal, then apply or discard.",
                        false,
                    ),
                    (
                        "Accept all edits",
                        "Apply valid changes to the canvas automatically. Every change can be undone.",
                        true,
                    ),
                ] {
                    options = options.push(
                        button(
                            column![
                                text(label).size(13),
                                text(description)
                                    .size(11)
                                    .style(super::workspace::muted_text)
                            ]
                            .spacing(4),
                        )
                        .width(Length::Fill)
                        .padding([9, 10])
                        .style(super::workspace::control(
                            state.preferences.auto_apply == auto,
                        ))
                        .on_press(Message::Assistant(Action::AutoApply(auto))),
                    );
                }
            }
            Menu::Models => {
                options = options.push(
                    crate::appearance::text_input("Search models…", &state.search)
                        .on_input(|s| Message::Assistant(Action::Search(s)))
                        .size(13)
                        .padding(9),
                );
                options = options.push(
                    action("Default · GPT-6 Astra", Action::Model(None))
                        .width(Length::Fill)
                        .style(super::workspace::control(state.preferences.model.is_none())),
                );
                options = options.push(
                    text("Uses GPT-6 Astra when available; otherwise your account default")
                        .size(10)
                        .style(super::workspace::muted_text),
                );
                if let Some(account) = &state.account {
                    let search = state.search.to_lowercase();
                    let mut models: Vec<_> = account
                        .models
                        .iter()
                        .filter(|m| {
                            m.label.to_lowercase().contains(&search)
                                || m.id.to_lowercase().contains(&search)
                        })
                        .collect();
                    // Keep the resolved model visible first; retain catalog ordering otherwise.
                    models
                        .sort_by_key(|m| state.model().is_none_or(|selected| selected.id != m.id));
                    for model in models {
                        let selected = state.preferences.model.as_deref() == Some(&model.id);
                        options = options.push(
                            button(
                                column![
                                    row![
                                        text(&model.label).size(13),
                                        Space::new().width(Length::Fill),
                                        text(if selected { "✓" } else { "" }).size(13)
                                    ],
                                    text(&model.description)
                                        .size(10)
                                        .style(super::workspace::muted_text)
                                ]
                                .spacing(4),
                            )
                            .width(Length::Fill)
                            .padding([9, 10])
                            .style(super::workspace::control(selected))
                            .on_press(Message::Assistant(Action::Model(Some(model.id.clone())))),
                        );
                    }
                } else {
                    options = options.push(text("Connect to load your available models.").size(12));
                }
            }
            Menu::Effort => {
                options = options.push(
                    text("Reasoning")
                        .size(12)
                        .style(super::workspace::muted_text),
                );
                if let Some(model) = state.model() {
                    for effort in &model.efforts {
                        let label = if effort.id == model.initial_effort() {
                            format!("{}  ·  Default", effort.label())
                        } else {
                            effort.label().into()
                        };
                        options = options.push(super::workspace::hover_hint(
                            action(label, Action::Effort(effort.id.clone()))
                                .width(Length::Fill)
                                .style(super::workspace::control(
                                    state.preferences.effort(model) == effort.id,
                                )),
                            effort.description.clone(),
                            tooltip::Position::Left,
                        ));
                    }
                    options = options.push(iced::widget::rule::horizontal(1)).push(
                        text("Service tier")
                            .size(12)
                            .style(super::workspace::muted_text),
                    );
                    options = options.push(
                        action("Standard · Default", Action::Tier("default".into()))
                            .width(Length::Fill)
                            .style(super::workspace::control(
                                state.preferences.tier(model).is_none_or(|t| t == "default"),
                            )),
                    );
                    for tier in &model.tiers {
                        options = options.push(super::workspace::hover_hint(
                            action(tier.name.clone(), Action::Tier(tier.id.clone()))
                                .width(Length::Fill)
                                .style(super::workspace::control(
                                    state.preferences.tier(model) == Some(tier.id.as_str()),
                                )),
                            tier.description.clone(),
                            tooltip::Position::Left,
                        ));
                    }
                } else {
                    options = options.push(
                        text("Choose an available model to see its reasoning levels.").size(12),
                    );
                }
            }
        }
        scrollable(options)
            .height(if menu == Menu::Edits { 170 } else { 310 })
            .into()
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
mod tests;
