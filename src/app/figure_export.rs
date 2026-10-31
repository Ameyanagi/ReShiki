//! Figure export preserves drawable content independently of molecular validation.
use super::{App, Message};
use iced::Task;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Saved {
    pub path: PathBuf,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Figure,
    Structure,
    Nmr,
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub format: &'static str,
    /// The suggested file name: the drawing's name, or "Molecule" while untitled.
    pub name: String,
    pub details: Vec<String>,
    pub kind: Kind,
}

fn extension(format: &'static str) -> &'static str {
    match format {
        "svg-office" => "svg",
        format => format,
    }
}

impl Prepared {
    pub(super) fn new(
        bytes: Vec<u8>,
        format: &'static str,
        stem: Option<String>,
        details: Vec<String>,
        kind: Kind,
    ) -> Self {
        let name = format!(
            "{}.{}",
            stem.as_deref().unwrap_or("Molecule"),
            extension(format)
        );
        Self {
            bytes,
            format,
            name,
            details,
            kind,
        }
    }

    pub(super) fn start(self) -> Task<Message> {
        if cfg!(target_arch = "wasm32") {
            Task::done(Message::ExportReady(self))
        } else {
            let name = self.name.clone();
            self.save(name)
        }
    }

    pub(super) fn completed(self, result: Result<Option<PathBuf>, String>) -> Message {
        let result = result.map(|path| {
            path.map(|path| Saved {
                path,
                details: self.details,
            })
        });
        match self.kind {
            Kind::Figure => Message::FigureExported(result),
            Kind::Structure => Message::Exported(result),
            Kind::Nmr => Message::Nmr(super::nmr::Action::Exported(
                result.map(|saved| saved.map(|saved| saved.path)),
            )),
        }
    }

    pub(super) fn save(self, name: String) -> Task<Message> {
        Task::perform(
            async move {
                let title = match self.kind {
                    Kind::Nmr => "Export predicted NMR shifts".into(),
                    _ => format!("Export {}", super::export_format_label(self.format)),
                };
                let result =
                    super::files::save_bytes(&title, &name, extension(self.format), &self.bytes)
                        .await;
                self.completed(result)
            },
            std::convert::identity,
        )
    }
}

impl App {
    /// The browser asks for a name only while the drawing is untitled.
    pub(super) fn export_ready(&mut self, export: Prepared, browser: bool) -> Task<Message> {
        let name = export.name.clone();
        if browser && self.tab.path.is_none() && self.prompt.is_none() {
            self.prompt = Some(super::prompt::Prompt::Download {
                name,
                action: super::prompt::Download::Export {
                    tab: self.tab.id,
                    export,
                },
            });
            return iced::widget::operation::focus("download-name");
        }
        export.save(name)
    }

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
                Ok::<_, String>(Prepared::new(
                    p.bytes,
                    format,
                    stem,
                    p.details,
                    Kind::Figure,
                ))
            },
            std::convert::identity,
        )
        .then(|result| match result {
            Ok(export) => export.start(),
            Err(error) => Task::done(Message::FigureExported(Err(error))),
        })
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
