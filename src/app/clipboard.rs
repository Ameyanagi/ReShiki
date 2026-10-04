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
mod tests {
    use super::*;
    use crate::app::Job;
    use reshiki::document::Document;

    fn copy_key(app: &App) -> CopyAsKey {
        CopyAsKey {
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
            selected: app.tab.selected.clone(),
            format: CopyFormat::Smiles,
        }
    }

    #[test]
    fn copy_as_drops_stale_selection_revision_and_epoch_before_publication() {
        for stale in ["selection", "revision", "epoch"] {
            let (mut app, _) = App::new();
            let atom = app.tab.doc.add_atom("O", Point::default());
            app.tab.selected = vec![atom];
            let key = copy_key(&app);
            app.copy_as_busy = true;
            app.tab.clipboard_busy = true;
            match stale {
                "selection" => app.tab.selected.clear(),
                "revision" => app.tab.revision += 1,
                _ => app.tab.file_epoch += 1,
            }
            let before = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            app.status = "Current drawing".into();
            let task = app.copy_as_prepared(key, Err("old conversion failed".into()));
            assert_eq!(task.units(), 0);
            assert!(!app.copy_as_busy && !app.tab.clipboard_busy);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.selected, selected);
            assert!(!app.tab.history.can_undo());
            if stale == "epoch" {
                assert_eq!(app.status, "Current drawing");
            } else {
                assert!(app.status.contains("Clipboard unchanged"));
            }
        }
    }

    #[test]
    fn copy_as_conversion_failure_is_nonmutating_and_retains_the_error() {
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("O", Point::default());
        let before = app.tab.doc.clone();
        let key = copy_key(&app);
        app.copy_as_busy = true;
        app.tab.clipboard_busy = true;
        assert_eq!(
            app.copy_as_prepared(key, Err("Unsupported bond".into()))
                .units(),
            0
        );
        assert_eq!(app.tab.doc, before);
        assert!(app.error && app.status.contains("Unsupported bond"));
        assert!(app.status.contains("Clipboard unchanged"));
        assert!(!app.copy_as_busy && !app.tab.clipboard_busy);
    }

    #[test]
    fn copy_as_work_and_receipts_stay_with_the_originating_tab() {
        use crate::app::tabs::tests::Front;
        for closed in [false, true] {
            for preparing in [false, true] {
                let (mut app, _) = App::new();
                app.tab.busy = false;
                app.tab.doc.add_atom("O", Point::default());
                app.tab.saved = app.tab.doc.clone();
                let id = app.tab.id;
                let key = copy_key(&app);
                app.copy_as_busy = true;
                app.tab.clipboard_busy = true;
                if closed {
                    let _ = app.close_active_tab();
                }
                let front = Front::new(&mut app);
                assert!(app.clipboard_working());
                let message = if preparing {
                    Message::CopyAsPrepared(key, Box::new(Err("No structure output".into())))
                } else {
                    Message::CopyAsWritten(key, Ok(vec!["Retained format warning".into()]))
                };
                let _ = app.update(Message::Tab(id, Box::new(message)));
                front.assert_unchanged(&app);
                assert!(!app.copy_as_busy);
                if !closed {
                    let source = app.tabs.background.iter().find(|tab| tab.id == id).unwrap();
                    assert!(!source.clipboard_busy);
                    assert_eq!(source.doc.atoms.len(), 1);
                    assert!(source.status.contains(if preparing {
                        "No structure output"
                    } else {
                        "Retained format warning"
                    }));
                }
            }
        }
    }

    #[test]
    fn copy_as_uses_one_snapshot_without_changing_selection_or_undo() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let carbon = app.tab.doc.add_atom("C", Point::default());
        app.tab.doc.add_atom("O", Point::new(240., 0.));
        app.tab.selected = vec![carbon];
        let before = app.tab.doc.clone();
        let task = app.copy_as(CopyFormat::Smiles);
        assert!(task.units() > 0);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, [carbon]);
        assert!(!app.tab.history.can_undo());
        assert!(app.copy_as_busy && app.tab.clipboard_busy);
        assert_eq!(app.copy_as(CopyFormat::Mol).units(), 0);
    }

    #[test]
    fn an_ordinary_copy_outliving_its_tab_cannot_overtake_a_new_copy_as() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let atom = app.tab.doc.add_atom("O", Point::default());
        app.tab.selected = vec![atom];
        app.tab.saved = app.tab.doc.clone();
        let (id, epoch, revision) = (app.tab.id, app.tab.file_epoch, app.tab.revision);
        let _task = app.copy_native(false, false);
        assert!(app.native_copy_busy);
        let _ = app.close_active_tab();
        app.tab.doc.add_atom("N", Point::default());
        assert_eq!(app.copy_as(CopyFormat::Smiles).units(), 0);
        assert!(!app.copy_as_busy);
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::ClipboardWritten {
                epoch,
                revision,
                cut_ids: vec![],
                result: success(),
            }),
        ));
        assert!(!app.native_copy_busy);
        assert_eq!(app.tab.doc, before);
        assert!(app.copy_as(CopyFormat::Smiles).units() > 0);
    }
    use reshiki::engine::Response;

    fn success() -> Result<CopyOutcome, String> {
        Ok(CopyOutcome {
            external_editable: true,
            image_only: false,
            chemical_format: None,
            notices: vec![],
        })
    }

    #[test]
    fn background_clipboard_reads_and_cuts_keep_the_front_unchanged() {
        use crate::app::tabs::{Action, tests::Front};
        for cut in [false, true] {
            for stale in [false, true] {
                let (mut app, _) = App::new();
                app.tab.busy = false;
                app.tab.doc.add_atom("N", Point::default());
                let (id, epoch, revision) = (app.tab.id, app.tab.file_epoch, app.tab.revision);
                let cut_ids = app.tab.doc.all_ids();
                app.tab.clipboard_busy = true;
                if stale {
                    let before = app.tab.doc.clone();
                    app.tab.doc.add_atom("C", Point::new(42., 0.));
                    app.changed(before);
                }
                let before = app.tab.doc.clone();
                let front = Front::new(&mut app);
                let mut part = Document::default();
                part.add_atom("O", Point::default());
                let message = if cut {
                    Message::ClipboardWritten {
                        epoch,
                        revision,
                        cut_ids,
                        result: success(),
                    }
                } else {
                    Message::ClipboardRead {
                        epoch,
                        revision,
                        result: Box::new(Ok(part.into())),
                    }
                };
                let _ = app.update(Message::Tab(id, Box::new(message)));
                front.assert_unchanged(&app);
                assert!(!app.tabs.background[0].clipboard_busy);
                let _ = app.update(Message::Tabs(Action::Select(id)));
                if stale {
                    assert_eq!(app.tab.doc, before);
                    assert!(app.status.contains("changed"));
                } else {
                    assert_ne!(app.tab.doc, before);
                    assert!(app.status.contains(if cut { "cut" } else { "pasted" }));
                    let _ = app.update(Message::Undo);
                    assert_eq!(app.tab.doc, before);
                    assert!(!app.tab.history.can_undo());
                }
            }
        }
    }

    #[test]
    fn successful_picture_fallback_has_a_concise_status_and_retains_details() {
        let (mut app, _) = App::new();
        app.clipboard_written(
            app.tab.file_epoch,
            app.tab.revision,
            vec![],
            Ok(CopyOutcome {
                external_editable: false,
                image_only: false,
                chemical_format: None,
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
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let before = app.tab.doc.clone();
        let (epoch, revision) = (app.tab.file_epoch, app.tab.revision);
        app.clipboard_written(epoch, revision, vec![a, b], Err("Write failed".into()));
        assert_eq!(app.tab.doc, before);
        app.clipboard_written(epoch.wrapping_add(1), revision, vec![a, b], success());
        assert_eq!(app.tab.doc, before);
        app.clipboard_written(epoch, revision.wrapping_add(1), vec![a, b], success());
        assert_eq!(app.tab.doc, before);
        // A changed selection must not change the pending Cut's snapshot.
        app.tab.selected = vec![a];
        app.clipboard_written(epoch, revision, vec![b], success());
        assert!(app.tab.doc.atom(a).is_some());
        assert!(app.tab.doc.atom(b).is_none());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }

    #[test]
    fn paste_is_atomic_and_stale_data_does_not_replace_newer_work() {
        let (mut app, _) = App::new();
        let mut part = Document::default();
        let a = part.add_atom("N", Point::default());
        let b = part.add_atom("C", Point::new(42., 0.));
        part.add_bond(a, b, 1, "plain");
        let before = app.tab.doc.clone();
        app.clipboard_read(
            app.tab.file_epoch,
            app.tab.revision.wrapping_add(1),
            Ok(part.clone().into()),
        );
        assert_eq!(app.tab.doc, before);
        app.clipboard_read(app.tab.file_epoch, app.tab.revision, Ok(part.into()));
        assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 2);
        assert_eq!(app.tab.selected.len(), 2);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }

    #[test]
    fn reshiki_pastes_keep_palette_colors_and_other_sources_stay_exact() {
        use reshiki::palette::{Color, Hue, Row};
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.bonds[0].color = Color::Palette(Hue::Teal, Row::Strong);
        let part = editing::selection(&app.tab.doc, &[a, b]);
        app.clipboard_read(
            app.tab.file_epoch,
            app.tab.revision,
            Ok(PasteOutcome::native(part.clone())),
        );
        assert_eq!(app.tab.doc.bonds[1].color, app.tab.doc.bonds[0].color);
        assert!(
            app.tab.doc.atoms[2..]
                .iter()
                .all(|a| !a.display.color_override)
        );
        // The same drawing from ChemDraw keeps its source appearance.
        app.tab.doc.canvas_theme = reshiki::canvas_theme::CanvasTheme::Dark;
        let teal = reshiki::palette::Palette::of(&part).rgb(part.bonds[0].color);
        app.clipboard_read(
            app.tab.file_epoch,
            app.tab.revision,
            Ok(reshiki::canvas_theme::resolved_document(&part)
                .into_owned()
                .into()),
        );
        assert_eq!(app.tab.doc.bonds[2].color, Color::Custom(teal));
        assert!(
            app.tab.doc.atoms[4..]
                .iter()
                .all(|a| a.display.color_override)
        );
    }

    #[test]
    fn external_metadata_warnings_remain_visible_after_paste_and_file_import() {
        let warning = "External reaction roles and condition references are not retained";
        let mut doc = Document::default();
        doc.add_atom("O", Point::default());
        let (mut app, _) = App::new();
        app.clipboard_read(
            app.tab.file_epoch,
            app.tab.revision,
            Ok(PasteOutcome {
                document: doc.clone(),
                warnings: vec![warning.into()],
                native: false,
            }),
        );
        assert!(app.status.starts_with("Editable drawing pasted"));
        assert!(app.status.contains(warning));
        assert!(!app.error);
        for kind in [Job::Import, Job::ImportFile, Job::Insert] {
            let _ = app.update(Message::EngineDone {
                revision: app.tab.revision,
                kind,
                result: Box::new(Ok(Response {
                    document: Some(doc.clone()),
                    analysis: None,
                    output: None,
                    engine_version: "test".into(),
                    warnings: vec![warning.into()],
                })),
            });
            assert!(app.status.contains(warning));
            assert!(!app.error);
        }
    }
}
