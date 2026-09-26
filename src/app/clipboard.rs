use super::{App, Message, Point, Tool, editing};
use iced::Task;
use reshiki::clipboard::{CopyOutcome, PasteOutcome};

impl App {
    pub(super) fn copy_native(&mut self, cut: bool, image_only: bool) -> Task<Message> {
        if self.clipboard_busy {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if self.selected.is_empty() && !image_only {
            self.status = "Select objects to copy".into();
            return Task::none();
        }
        let snapshot = if self.selected.is_empty() {
            self.doc.clone()
        } else {
            editing::selection(&self.doc, &self.selected)
        };
        let cut_ids = if cut { self.selected.clone() } else { vec![] };
        let (epoch, revision) = (self.file_epoch, self.revision);
        self.clipboard_busy = true;
        self.error = false;
        self.status = "Preparing clipboard…".into();
        Task::perform(
            reshiki::clipboard::copy(self.engine.clone(), snapshot, image_only),
            move |result| Message::ClipboardWritten {
                epoch,
                revision,
                cut_ids: cut_ids.clone(),
                result,
            },
        )
    }

    pub(super) fn paste_native(&mut self, image_only: bool) -> Task<Message> {
        if self.clipboard_busy {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        self.clipboard_busy = true;
        self.error = false;
        self.status = "Reading clipboard…".into();
        let (epoch, revision) = (self.file_epoch, self.revision);
        Task::perform(
            reshiki::clipboard::paste_with_warnings(self.engine.clone(), image_only),
            move |result| Message::ClipboardRead {
                epoch,
                revision,
                result: Box::new(result),
            },
        )
    }

    pub(super) fn clipboard_written(
        &mut self,
        epoch: u64,
        revision: u64,
        cut_ids: Vec<u64>,
        result: Result<CopyOutcome, String>,
    ) {
        self.clipboard_busy = false;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.error = true;
                self.status = format!("Could not copy: {error}");
                return;
            }
        };
        let cut = !cut_ids.is_empty();
        let stale_cut = cut && (epoch != self.file_epoch || revision != self.revision);
        if cut && !stale_cut {
            let before = self.doc.clone();
            self.doc.delete(&cut_ids);
            self.changed(before);
            self.sync_typography();
            self.sync_graphics();
            self.sync_arrows();
            self.sync_bonds();
        }
        let action = if stale_cut {
            "Copied previous selection; Cut cancelled because the drawing changed"
        } else if cut {
            if cfg!(windows) {
                "Selection cut · editable drawing copied"
            } else {
                "Selection cut · editable drawing and images copied"
            }
        } else if outcome.image_only {
            "Image copied"
        } else if cfg!(windows) && outcome.external_editable {
            "Copied editable drawing · use Copy Image for a picture"
        } else if cfg!(windows) {
            "Copied · editable in ReShiki; use Copy Image for other apps"
        } else if outcome.external_editable {
            "Editable drawing and images copied"
        } else {
            "Copied · editable in ReShiki; picture in other apps"
        };
        self.status = if outcome.notices.is_empty() {
            action.to_owned()
        } else {
            format!("{action} · Review details\n{}", outcome.notices.join("\n"))
        };
        // A successfully written clipboard may have format limitations. Keep
        // their details available without presenting a successful copy as failure.
        self.error = false;
    }

    pub(super) fn clipboard_read(
        &mut self,
        epoch: u64,
        revision: u64,
        result: Result<PasteOutcome, String>,
    ) {
        self.clipboard_busy = false;
        if epoch != self.file_epoch
            || revision != self.revision
            || self.inline_text.is_some()
            || self.joining.is_some()
            || self.cleanup.is_some()
        {
            self.status =
                "Drawing changed while reading the clipboard · Paste again to insert here".into();
            return;
        }
        let outcome = match result.and_then(|outcome| {
            outcome.document.validate()?;
            Ok(outcome)
        }) {
            Ok(doc) => doc,
            Err(error) => {
                self.status = format!("Could not paste: {error}");
                self.error = true;
                return;
            }
        };
        let part = reshiki::canvas_theme::for_paste(outcome.document, self.doc.canvas_theme);
        let center = editing::center(&part, &part.all_ids());
        let before = self.doc.clone();
        let selected = editing::append(
            &mut self.doc,
            &part,
            Point::new(
                self.camera.center.x - center.x + 24.,
                self.camera.center.y - center.y + 24.,
            ),
        );
        if selected.is_empty() || self.doc.validate().is_err() {
            self.doc = before;
            self.status = "Could not insert the clipboard drawing".into();
            self.error = true;
            return;
        }
        self.selected = selected;
        self.changed(before);
        self.tool = Tool::Select;
        self.sync_typography();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        if part.atoms.is_empty()
            && part.annotations.is_empty()
            && part.arrows.is_empty()
            && part.graphics.iter().all(|g| g.picture.is_some())
        {
            if let Some(id) = self.selected.first().copied() {
                self.reveal_picture(id);
            }
            self.status = "Picture pasted · Drag the corner handles to resize".into();
        } else {
            self.status = "Editable drawing pasted".into();
        }
        if !outcome.warnings.is_empty() {
            self.status.push_str(" · ");
            self.status.push_str(&outcome.warnings.join(" · "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Document;

    fn success() -> Result<CopyOutcome, String> {
        Ok(CopyOutcome {
            external_editable: true,
            image_only: false,
            notices: vec![],
        })
    }

    #[test]
    fn successful_picture_fallback_has_a_concise_status_and_retains_details() {
        let (mut app, _) = App::new();
        app.clipboard_written(
            app.file_epoch,
            app.revision,
            vec![],
            Ok(CopyOutcome {
                external_editable: false,
                image_only: false,
                notices: vec!["Unsupported projected wedge style".into()],
            }),
        );
        assert!(!app.error);
        assert!(
            app.status
                .lines()
                .next()
                .is_some_and(|s| s.len() < 110 && s.contains("Copied"))
        );
        assert!(app.status.contains("Unsupported projected wedge style"));
    }

    #[test]
    fn cut_waits_for_success_and_refuses_stale_completion() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("O", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        let before = app.doc.clone();
        let (epoch, revision) = (app.file_epoch, app.revision);
        app.clipboard_written(epoch, revision, vec![a, b], Err("Write failed".into()));
        assert_eq!(app.doc, before);
        app.clipboard_written(epoch.wrapping_add(1), revision, vec![a, b], success());
        assert_eq!(app.doc, before);
        app.clipboard_written(epoch, revision.wrapping_add(1), vec![a, b], success());
        assert_eq!(app.doc, before);
        // A changed selection must not change the pending Cut's snapshot.
        app.selected = vec![a];
        app.clipboard_written(epoch, revision, vec![b], success());
        assert!(app.doc.atom(a).is_some());
        assert!(app.doc.atom(b).is_none());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn paste_is_atomic_and_stale_data_does_not_replace_newer_work() {
        let (mut app, _) = App::new();
        let mut part = Document::default();
        let a = part.add_atom("N", Point::default());
        let b = part.add_atom("C", Point::new(42., 0.));
        part.add_bond(a, b, 1, "plain");
        let before = app.doc.clone();
        app.clipboard_read(
            app.file_epoch,
            app.revision.wrapping_add(1),
            Ok(part.clone().into()),
        );
        assert_eq!(app.doc, before);
        app.clipboard_read(app.file_epoch, app.revision, Ok(part.into()));
        assert_eq!(app.doc.atoms.len(), before.atoms.len() + 2);
        assert_eq!(app.selected.len(), 2);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }
}
