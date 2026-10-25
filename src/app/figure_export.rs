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
        let label = super::export_format_label(format);
        self.status = format!("Preparing {label} export…");
        let doc = self.tab.doc.clone();
        let engine = self.engine.clone();
        let stem = self.export_stem();
        Task::perform(
            async move {
                let p = reshiki::export::publication(
                    &engine,
                    doc,
                    format,
                    pages,
                    reshiki::export::FILE_PIXELS,
                )
                .await?;
                Ok(super::save_export(p.bytes, format, stem)
                    .await?
                    .map(|path| Saved {
                        path,
                        details: p.details,
                    }))
            },
            Message::FigureExported,
        )
    }

    pub(super) fn figure_exported(&mut self, result: Result<Option<Saved>, String>) {
        self.figure_exporting = false;
        match result {
            Ok(Some(saved)) => {
                self.status = super::files::export_status(
                    &saved.path,
                    &saved.details,
                    cfg!(target_arch = "wasm32"),
                );
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
mod tests;
