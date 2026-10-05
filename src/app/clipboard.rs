use super::{App, Message, Point, Tool, editing};
use iced::Task;
use reshiki::clipboard::{CopyFormat, CopyOutcome, PasteOutcome, PreparedCopy};

#[derive(Debug, Clone)]
pub struct CopyAsKey {
    pub epoch: u64,
    pub revision: u64,
    pub format: CopyFormat,
    pub selected: Vec<u64>,
}

impl App {
    pub(super) fn clipboard_working(&self) -> bool {
        self.copy_as_busy
            || self.native_copy_busy
            || self.tab.clipboard_busy
            || self.tabs.background.iter().any(|tab| tab.clipboard_busy)
    }

    pub(super) fn copy_as(&mut self, format: CopyFormat) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        let snapshot = reshiki::clipboard::selection_or_drawing(&self.tab.doc, &self.tab.selected);
        let snapshot = if format.is_chemical() {
            match reshiki::clipboard::chemical_snapshot(&self.tab.doc, &snapshot) {
                Ok(std::borrow::Cow::Owned(chemical)) => chemical,
                Ok(std::borrow::Cow::Borrowed(_)) => snapshot,
                Err(reason) => {
                    self.status = reason.into();
                    self.error = true;
                    return Task::none();
                }
            }
        } else {
            snapshot
        };
        if let Some(reason) = format.unavailable_reason(&snapshot) {
            self.status = reason.into();
            self.error = true;
            return Task::none();
        }
        let key = CopyAsKey {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            format,
            selected: self.tab.selected.clone(),
        };
        self.copy_as_busy = true;
        self.tab.clipboard_busy = true;
        self.error = false;
        self.status = format!("Preparing {} for the clipboard…", format.label());
        Task::perform(
            reshiki::clipboard::prepare_as(self.engine.clone(), snapshot, format),
            move |result| Message::CopyAsPrepared(key.clone(), Box::new(result)),
        )
    }

    fn finish_copy_as(&mut self) {
        self.copy_as_busy = false;
        self.tab.clipboard_busy = false;
    }

    pub(super) fn copy_as_prepared(
        &mut self,
        key: CopyAsKey,
        result: Result<PreparedCopy, String>,
    ) -> Task<Message> {
        if key.epoch != self.tab.file_epoch {
            self.finish_copy_as();
            return Task::none();
        }
        if key.revision != self.tab.revision || key.selected != self.tab.selected {
            self.finish_copy_as();
            self.status = "Drawing or selection changed; copy again · Clipboard unchanged".into();
            self.error = false;
            return Task::none();
        }
        let copy = match result {
            Ok(copy) => copy,
            Err(error) => {
                self.finish_copy_as();
                self.status = format!(
                    "Could not copy {}: {error} · Clipboard unchanged",
                    key.format.label()
                );
                self.error = true;
                return Task::none();
            }
        };
        if !reshiki::clipboard::available() {
            let Some(text) = copy.text().map(str::to_owned) else {
                self.finish_copy_as();
                self.status = "Native clipboard is unavailable; use Export for this format".into();
                self.error = true;
                return Task::none();
            };
            return iced::clipboard::write(text)
                .chain(Task::done(Message::CopyAsWritten(key, Ok(copy.notices))));
        }
        Task::perform(reshiki::clipboard::write_prepared(copy), move |result| {
            Message::CopyAsWritten(key.clone(), result)
        })
    }

    pub(super) fn copy_as_written(&mut self, key: CopyAsKey, result: Result<Vec<String>, String>) {
        self.finish_copy_as();
        // A document replacement cannot receive an old operation's receipt.
        if key.epoch != self.tab.file_epoch {
            return;
        }
        match result {
            Ok(notices) => {
                let scope = match (
                    !key.selected.is_empty(),
                    key.revision == self.tab.revision && key.selected == self.tab.selected,
                ) {
                    (true, true) => "Selection",
                    (false, true) => "Drawing",
                    (true, false) => "Previous selection",
                    (false, false) => "Previous drawing snapshot",
                };
                self.status = format!("{scope} copied as {}", key.format.label());
                if !notices.is_empty() {
                    self.status.push_str(" · Review details\n");
                    self.status.push_str(&notices.join("\n"));
                }
                self.error = false;
            }
            Err(error) => {
                self.status = format!(
                    "Could not write {} to the clipboard: {error}",
                    key.format.label()
                );
                self.error = true;
            }
        }
    }

    pub(super) fn copy_native(&mut self, cut: bool, image_only: bool) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        if self.tab.selected.is_empty() && !image_only {
            self.status = "Select objects to copy".into();
            return Task::none();
        }
        let snapshot = if self.tab.selected.is_empty() {
            self.tab.doc.clone()
        } else {
            editing::selection(&self.tab.doc, &self.tab.selected)
        };
        let cut_ids = if cut {
            self.tab.selected.clone()
        } else {
            vec![]
        };
        let (epoch, revision) = (self.tab.file_epoch, self.tab.revision);
        let reaction = if image_only {
            Ok(None)
        } else {
            reshiki::reactions::copy_reaction(&self.tab.doc, &snapshot)
        };
        self.native_copy_busy = true;
        self.tab.clipboard_busy = true;
        self.error = false;
        self.status = "Preparing clipboard…".into();
        Task::perform(
            reshiki::clipboard::copy_with_reaction(
                self.engine.clone(),
                snapshot,
                image_only,
                reaction,
            ),
            move |result| Message::ClipboardWritten {
                epoch,
                revision,
                cut_ids: cut_ids.clone(),
                result,
            },
        )
    }

    pub(super) fn paste_native(&mut self, image_only: bool) -> Task<Message> {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        self.tab.clipboard_busy = true;
        self.error = false;
        self.status = "Reading clipboard…".into();
        let (epoch, revision) = (self.tab.file_epoch, self.tab.revision);
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
        self.native_copy_busy = false;
        self.tab.clipboard_busy = false;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.error = true;
                self.status = format!("Could not copy: {error}");
                return;
            }
        };
        let cut = !cut_ids.is_empty();
        let stale_cut = cut && (epoch != self.tab.file_epoch || revision != self.tab.revision);
        if cut && !stale_cut {
            let before = self.tab.doc.clone();
            self.tab.doc.delete(&cut_ids);
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
        let action = match outcome.chemical_format {
            Some(CopyFormat::Smiles) => {
                format!("{action} · SMILES text copied for structure input")
            }
            Some(CopyFormat::ChemDoodleReaction) => {
                format!("{action} · Reaction JSON copied for ChemDoodle Open")
            }
            _ => action.to_owned(),
        };
        self.status = if outcome.notices.is_empty() {
            action
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
        self.tab.clipboard_busy = false;
        if epoch != self.tab.file_epoch
            || revision != self.tab.revision
            || self.tab.inline_text.is_some()
            || self.tab.joining.is_some()
            || self.tab.cleanup.is_some()
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
        let part = if outcome.native {
            reshiki::canvas_theme::for_native_paste(outcome.document, self.tab.doc.canvas_theme)
        } else {
            reshiki::canvas_theme::for_paste(outcome.document, self.tab.doc.canvas_theme)
        };
        let center = editing::center(&part, &part.all_ids());
        let before = self.tab.doc.clone();
        let selected = editing::append(
            &mut self.tab.doc,
            &part,
            Point::new(
                self.tab.camera.center.x - center.x + 24.,
                self.tab.camera.center.y - center.y + 24.,
            ),
        );
        if selected.is_empty() || self.tab.doc.validate().is_err() {
            self.tab.doc = before;
            self.status = "Could not insert the clipboard drawing".into();
            self.error = true;
            return;
        }
        self.tab.selected = selected;
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
            if let Some(id) = self.tab.selected.first().copied() {
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
mod tests;
