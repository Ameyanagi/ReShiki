//! The activity timer: drains Codex progress, mirrors the canvas for drawing tools, saves preferences and resumes an Apply that waited for a canvas edit.

use super::{Action, setup::Connection};
use crate::app::{App, Message};
use iced::Task;
use reshiki::assistant::settings::effort_label;
use reshiki::assistant::{self, codex};

impl App {
    pub(super) fn assistant_poll(&mut self) -> Task<Message> {
        let mut scroll = false;
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
        while let Some(Ok(message)) = self
            .assistant
            .progress
            .as_mut()
            .map(|progress| progress.try_recv())
        {
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
                codex::Progress::Pass(pass) => {
                    self.assistant.preparation = self.assistant.preview.is_some().then_some(pass);
                    self.assistant.structures = None;
                    self.assistant.status = preparing(self.assistant.preparation).into();
                }
                codex::Progress::Structures { completed, total } => {
                    if self.assistant.preparation.is_none() {
                        self.assistant.structures = Some((completed, total));
                    }
                    self.assistant.status = preparing(self.assistant.preparation).into();
                }
                codex::Progress::Preview(doc) => {
                    self.assistant.preview = Some(*doc);
                    self.assistant.status = match self.assistant.preparation {
                        None => "Layout assembled",
                        Some(codex::Pass::Preview) => "Draft revised",
                        Some(codex::Pass::Final) => "Final draft assembled",
                    }
                    .into();
                }
                codex::Progress::Checking { pass } => {
                    self.assistant.checking = pass;
                    self.assistant.status =
                        format!("Checking the rendered draft · Pass {pass} of 3");
                }
                codex::Progress::Status(message) => self.assistant.status = message,
                codex::Progress::Catalog(account) => {
                    self.assistant.connection = Connection::from_account(&account);
                    self.assistant.account = Some(account);
                }
                codex::Progress::ConnectionFailed(error) => {
                    self.assistant_connection_failed(error);
                }
                codex::Progress::Reply(reply) => self.assistant.reply = reply,
                codex::Progress::Started { model, effort } => {
                    self.assistant.running_model = format!("{model} · {}", effort_label(&effort));
                }
            }
        }
        if self.assistant.preferences_dirty && !self.assistant.preferences_saving && !cfg!(test) {
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
        super::chat_scroll(scroll)
    }
}

/// The status while a pass prepares structures; later passes name what they revise.
fn preparing(pass: Option<codex::Pass>) -> &'static str {
    match pass {
        None => "Preparing structures…",
        Some(codex::Pass::Preview) => "Revising the draft…",
        Some(codex::Pass::Final) => "Preparing the final draft…",
    }
}
