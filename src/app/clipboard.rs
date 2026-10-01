use super::{App, Message, Point, Tool, editing};
use iced::Task;
use reshiki::clipboard::{CopyOutcome, PasteOutcome};

impl App {
    pub(super) fn copy_native(&mut self, cut: bool, image_only: bool) -> Task<Message> {
        if self.tab.clipboard_busy {
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
        self.tab.clipboard_busy = true;
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
        if self.tab.clipboard_busy {
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
    use reshiki::engine::Response;

    fn success() -> Result<CopyOutcome, String> {
        Ok(CopyOutcome {
            external_editable: true,
            image_only: false,
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
