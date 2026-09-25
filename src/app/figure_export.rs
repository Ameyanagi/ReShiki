//! Figure export preserves drawable content independently of molecular validation.
use super::{App, Message};
use iced::Task;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Saved {
    pub path: PathBuf,
    pub details: Vec<String>,
}

impl App {
    pub(super) fn export_figure(&mut self, format: &'static str, pages: bool) -> Task<Message> {
        if self.figure_exporting {
            return Task::none();
        }
        self.figure_exporting = true;
        self.error = false;
        self.status = format!("Preparing {} export…", format.to_uppercase());
        let doc = self.doc.clone();
        let engine = self.engine.clone();
        Task::perform(
            async move {
                let (doc, notice) = reshiki::export::figure_document(&engine, doc).await?;
                let figure = tokio::task::spawn_blocking(move || {
                    if pages {
                        reshiki::export::pages_pdf(&doc).map(|bytes| reshiki::export::Figure {
                            bytes,
                            detail: None,
                        })
                    } else {
                        reshiki::export::figure(&doc, format)
                    }
                })
                .await
                .map_err(|error| error.to_string())??;
                let details = figure.detail.into_iter().chain(notice).collect();
                Ok(super::save_export(figure.bytes, format)
                    .await?
                    .map(|path| Saved { path, details }))
            },
            Message::FigureExported,
        )
    }

    pub(super) fn figure_exported(&mut self, result: Result<Option<Saved>, String>) {
        self.figure_exporting = false;
        match result {
            Ok(Some(saved)) => {
                self.status = format!(
                    "Exported {}",
                    saved.path.file_name().unwrap_or_default().to_string_lossy()
                );
                for detail in saved.details {
                    self.status.push_str(" · ");
                    self.status.push_str(&detail);
                }
                self.error = false;
            }
            Ok(None) => {
                self.status = "Export canceled".into();
                self.error = false;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporting_a_snapshot_blocks_duplicate_exports_and_always_resets() {
        let (mut app, _) = App::new();
        let snapshot = app.doc.clone();
        let _task = app.update(Message::Export("png"));
        assert!(app.figure_exporting);
        assert!(!app.error);
        assert!(app.update(Message::Export("pdf")).units() == 0);
        let _ = app.update(Message::FigureExported(Err("Disk is full".into())));
        assert!(!app.figure_exporting);
        assert!(app.error);
        assert_eq!(app.status, "Disk is full");
        app.figure_exporting = true;
        let _ = app.update(Message::FigureExported(Ok(None)));
        assert!(!app.figure_exporting);
        assert!(!app.error);
        let _ = app.update(Message::FigureExported(Ok(Some(Saved {
            path: PathBuf::from("gallery.png"),
            details: vec!["PNG: 6627 × 5285 pixels at 300 dpi".into()],
        }))));
        assert!(app.status.contains("300 dpi"));
        assert_eq!(app.doc, snapshot);
        assert!(super::super::atom_text::background(
            &Message::FigureExported(Ok(None))
        ));
    }
}
