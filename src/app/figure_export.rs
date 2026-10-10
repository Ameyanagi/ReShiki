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
        let label = match format {
            "svg-office" => "SVG · Office picture".into(),
            _ => format.to_uppercase(),
        };
        self.status = format!("Preparing {label} export…");
        let doc = self.tab.doc.clone();
        let engine = self.engine.clone();
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
                Ok(super::save_export(p.bytes, format)
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
mod tests;
