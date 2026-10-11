//! The request lifecycle: Send/Improve, completion, Stop and Reset. Serial, cancel, epoch and revision guard late results.

use super::{Action, Draft};
use crate::app::{App, Message};
use iced::Task;
use iced::widget::text_editor;
use reshiki::assistant::{self, DrawingSettings, Proposal, codex};
use serde_json::json;

impl App {
    pub(super) fn assistant_request(&mut self, improving: bool) -> Task<Message> {
        if self.assistant.busy || self.assistant.reading_image || self.tab.cleanup.is_some() {
            return Task::none();
        }
        if !matches!(
            self.assistant.connection,
            super::setup::Connection::Ready | super::setup::Connection::Unchecked
        ) {
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
            self.assistant.status = "Please shorten your request to 12,000 characters".into();
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
            bond_color: super::super::graphics::parse_color(&self.tab.bond_color_input)
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
                .unwrap_or_else(|| assistant::review::scope(&self.tab.doc, &self.tab.selected))
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
        self.assistant.requires_apply =
            self.assistant.guided_example || (improving && self.assistant.requires_apply);
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
        let shared =
            std::sync::Arc::new(std::sync::RwLock::new(assistant::canvas_tools::Snapshot {
                document: self.tab.doc.clone(),
                selected: self.tab.selected.clone(),
                revision,
                epoch,
            }));
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
        Task::batch([
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
                        codex::propose_image(request, preferences, cancel, tx, Some(canvas), image)
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
        ])
    }
    pub(super) fn assistant_done(
        &mut self,
        serial: u64,
        epoch: u64,
        revision: u64,
        replace: Vec<u64>,
        result: Box<Result<assistant::review::Outcome, String>>,
    ) -> Task<Message> {
        if serial != self.assistant.serial {
            return Task::none();
        }
        let scroll = self.assistant.follow_chat;
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
                    self.assistant.status = "Ready for review · Nothing has been applied".into();
                    if self.assistant.preferences.auto_apply
                        && can_auto_apply
                        && !self.assistant.requires_apply
                    {
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
        super::chat_scroll(scroll)
    }
    pub(super) fn assistant_stop(&mut self) {
        let checking_connection = self.assistant.connection == super::setup::Connection::Checking;
        if checking_connection {
            self.assistant.connection = super::setup::Connection::Cancelled;
        }
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
        self.assistant
            .retain_preview("Quality review has not finished. Review the draft before applying.");
        if checking_connection {
            self.assistant.status = codex::ConnectionError::Cancelled.to_string();
            self.assistant.error = false;
            return;
        }
        self.assistant.status = "Stopped · Completed previews are retained".into();
        self.assistant.record(
            "ReShiki",
            "Stopped. Completed previews remain available below.".into(),
        );
    }
    pub(super) fn assistant_reset(&mut self) {
        self.assistant.tab = None;
        self.assistant.canvas = None;
        self.assistant.pending_scope = None;
        self.assistant.pending_proposal = None;
        self.assistant.viewed_image = None;
        self.assistant.source_image = None;
        self.assistant.guided_example = false;
        self.assistant.requires_apply = false;
        self.assistant.example_reference = None;
        if self.assistant.connection == super::setup::Connection::Checking {
            self.assistant.connection = super::setup::Connection::Cancelled;
        }
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
}
