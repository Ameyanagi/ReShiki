//! Reviewing drafts: Apply with epoch/revision guards, Discard, and edits to the draft preview.

use super::Action;
use crate::app::{App, Message};
use iced::Task;
use reshiki::assistant;

impl App {
    pub(super) fn assistant_reject(&mut self) {
        self.assistant.waiting_for_canvas_edit = false;
        self.assistant.draft = None;
        self.assistant.preview = None;
        self.assistant.record(
            "ReShiki",
            "Proposal rejected. The drawing was not changed.".into(),
        );
        self.assistant.status = "Rejected · Your drawing is unchanged".into();
    }
    pub(super) fn assistant_apply(&mut self) -> Task<Message> {
        let scroll = self.assistant.follow_chat;
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
                    self.tab.camera.center =
                        reshiki::document::Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
                    let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
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
        super::chat_scroll(scroll)
    }
    pub(super) fn assistant_preview_edit(&mut self, edit: assistant::review::Edit) {
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
}
