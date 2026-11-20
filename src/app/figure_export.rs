//! Figure export preserves drawable content independently of molecular validation.
use super::{App, Message};
use iced::Task;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Saved {
    pub path: PathBuf,
    pub details: Vec<String>,
    pub recent: Option<Recent>,
}

/// The last successful picture export. Its destination belongs to the drawing
/// that chose it; a new drawing only inherits the dialog's starting folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recent {
    path: PathBuf,
    recipe: Recipe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recipe {
    format: String,
    pages: bool,
    editable: bool,
    background: crate::appearance::Background,
    custom_background: [u8; 3],
    dpi: crate::appearance::Dpi,
    svg_live_text: bool,
    source: Option<PathBuf>,
    #[serde(skip)]
    origin: Option<(super::document_tab::TabId, u64)>,
}

impl Recipe {
    fn format(&self) -> Option<&'static str> {
        match self.format.as_str() {
            "png" => Some("png"),
            "pdf" => Some("pdf"),
            "svg" => Some("svg"),
            "svg-office" => Some("svg-office"),
            #[cfg(windows)]
            "emf" => Some("emf"),
            _ => None,
        }
    }
    fn belongs_to(&self, tab: &super::document_tab::DocumentTab) -> bool {
        self.belongs_to_platform(tab, cfg!(target_arch = "wasm32"))
    }
    fn belongs_to_platform(&self, tab: &super::document_tab::DocumentTab, browser: bool) -> bool {
        self.origin == Some((tab.id, tab.file_epoch))
            || (!browser
                && self.source.as_ref().is_some_and(|source| {
                    source.is_absolute()
                        && tab.path.as_deref().and_then(source_identity).as_ref() == Some(source)
                }))
    }
}

/// Relative startup paths mean different drawings in different working
/// directories. Remember their resolved identity, never the spelling alone.
fn source_identity(path: &Path) -> Option<PathBuf> {
    source_identity_in(path, &std::env::current_dir().ok()?)
}

fn source_identity_in(path: &Path, directory: &Path) -> Option<PathBuf> {
    let path = directory.join(path);
    path.canonicalize()
        .or_else(|_| std::path::absolute(path))
        .ok()
}

impl Recent {
    fn settings_path() -> Result<PathBuf, String> {
        Ok(reshiki::compatibility::data_directory_location()?
            .path
            .join("recent-export.json"))
    }
    pub(super) fn load() -> Option<Self> {
        if cfg!(test) {
            return None;
        }
        Self::load_from(&Self::settings_path().ok()?)
    }
    fn load_from(path: &Path) -> Option<Self> {
        let mut recent: Self = serde_json::from_slice(&reshiki::storage::read(path).ok()?).ok()?;
        // A relative source from an older record has lost its original cwd.
        // Keep the folder hint, but do not authorize rewriting its target.
        if recent
            .recipe
            .source
            .as_ref()
            .is_some_and(|source| !source.is_absolute())
        {
            recent.recipe.source = None;
        }
        recent.recipe.format()?;
        if !cfg!(target_arch = "wasm32") && !recent.path.is_absolute() {
            return None;
        }
        Some(recent)
    }
    fn save(&self) -> Result<(), String> {
        if cfg!(test) {
            return Ok(());
        }
        self.save_to(&Self::settings_path()?)
    }
    fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            reshiki::storage::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        reshiki::storage::write_atomic(path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
    pub(super) fn label(&self) -> String {
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        let format = super::export_format_label(&self.recipe.format);
        let options = if self.recipe.format == "png" {
            format!("{format} · {} dpi", self.recipe.dpi.get())
        } else if self.recipe.pages {
            format!("{format} · all pages")
        } else {
            format
        };
        format!("Export again: {name} ({options})")
    }
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
    recipe: Option<Recipe>,
    directory: Option<PathBuf>,
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
            recipe: None,
            directory: None,
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
                recent: self.recipe.map(|recipe| Recent {
                    path: path.clone(),
                    recipe,
                }),
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

    /// Reuse an explicitly named destination; unlike normal Export this has no
    /// file dialog. Browsers start another download under the remembered name.
    fn save_again(self, path: PathBuf) -> Task<Message> {
        Task::perform(
            async move {
                let result = super::files::write(&path, &self.bytes).map(|()| Some(path));
                self.completed(result)
            },
            std::convert::identity,
        )
    }

    pub(super) fn save(self, name: String) -> Task<Message> {
        Task::perform(
            async move {
                let title = match self.kind {
                    Kind::Nmr => "Export predicted NMR shifts".into(),
                    _ => format!("Export {}", super::export_format_label(self.format)),
                };
                let result = super::files::save_bytes(
                    &title,
                    &name,
                    extension(self.format),
                    &self.bytes,
                    self.directory.as_deref(),
                )
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

    pub(super) fn export_figure(
        &mut self,
        format: &'static str,
        pages: bool,
        editable: bool,
    ) -> Task<Message> {
        if self.figure_exporting {
            return Task::none();
        }
        self.figure_exporting = true;
        self.error = false;
        let label = super::export_format_label(format);
        self.status = format!("Preparing {label} export…");
        self.prepare_figure(format, pages, editable)
            .then(|result| match result {
                Ok(export) => export.start(),
                Err(error) => Task::done(Message::FigureExported(Err(error))),
            })
    }

    fn prepare_figure(
        &self,
        format: &'static str,
        pages: bool,
        editable: bool,
    ) -> Task<Result<Prepared, String>> {
        let recipe = Recipe {
            format: format.into(),
            pages,
            editable,
            background: self.appearance.export_background,
            custom_background: self.appearance.custom_background,
            dpi: self.appearance.export_dpi,
            svg_live_text: self.appearance.export_svg_live_text,
            source: if cfg!(target_arch = "wasm32") {
                None
            } else {
                self.tab.path.as_deref().and_then(source_identity)
            },
            origin: Some((self.tab.id, self.tab.file_epoch)),
        };
        self.prepare_recipe(recipe)
    }

    fn prepare_recipe(&self, recipe: Recipe) -> Task<Result<Prepared, String>> {
        let Some(format) = recipe.format() else {
            return Task::done(Err("This export format is unavailable.".into()));
        };
        let render_format = if format == "svg" && !recipe.svg_live_text {
            "svg-office"
        } else {
            format
        };
        let doc = self.tab.doc.clone();
        let engine = self.engine.clone();
        let stem = self.export_stem();
        let background = recipe.background.paper(recipe.custom_background);
        let dpi = recipe.dpi.get();
        let pages = recipe.pages;
        let editable = recipe.editable;
        let directory = self
            .recent_export
            .as_ref()
            .and_then(|recent| recent.path.parent())
            .map(Path::to_path_buf);
        Task::perform(
            async move {
                // Let "Preparing … export…" show before this holds the page's thread.
                #[cfg(target_arch = "wasm32")]
                crate::web::after_paint().await;
                let p = reshiki::export::publication(
                    &engine,
                    doc,
                    render_format,
                    pages,
                    editable,
                    background,
                    dpi,
                    reshiki::export::FILE_PIXELS,
                )
                .await?;
                let mut prepared = Prepared::new(p.bytes, format, stem, p.details, Kind::Figure);
                prepared.recipe = Some(recipe);
                prepared.directory = directory;
                Ok::<_, String>(prepared)
            },
            std::convert::identity,
        )
    }

    pub(super) fn repeat_export(&self) -> Option<&Recent> {
        self.recent_export
            .as_ref()
            .filter(|recent| recent.recipe.belongs_to(&self.tab))
    }

    pub(super) fn export_again(&mut self) -> Task<Message> {
        if self.tab.busy || self.figure_exporting {
            return Task::none();
        }
        let Some(recent) = self.repeat_export().cloned() else {
            return Task::none();
        };
        self.figure_exporting = true;
        self.error = false;
        self.status = "Preparing export…".into();
        let mut recipe = recent.recipe;
        recipe.origin = Some((self.tab.id, self.tab.file_epoch));
        recipe.source = if cfg!(target_arch = "wasm32") {
            None
        } else {
            self.tab.path.as_deref().and_then(source_identity)
        };
        self.prepare_recipe(recipe).then(move |result| {
            let path = recent.path.clone();
            match result {
                Ok(export) => export.save_again(path),
                Err(error) => Task::done(Message::FigureExported(Err(error))),
            }
        })
    }

    /// A successful Save as names the same drawing; retain its repeat target.
    /// An unrelated tab cannot take ownership of the previous export.
    pub(super) fn rebind_recent_export(&mut self, path: &Path) -> Result<(), String> {
        let Some(mut recent) = self.repeat_export().cloned() else {
            return Ok(());
        };
        recent.recipe.origin = Some((self.tab.id, self.tab.file_epoch));
        recent.recipe.source = if cfg!(target_arch = "wasm32") {
            None
        } else {
            source_identity(path)
        };
        self.remember_export(recent)
    }

    fn remember_completed_export(&mut self, mut recent: Recent) -> Result<(), String> {
        // Save as may finish while this picture is still being prepared.
        if recent.recipe.origin == Some((self.tab.id, self.tab.file_epoch)) {
            recent.recipe.source = if cfg!(target_arch = "wasm32") {
                None
            } else {
                self.tab.path.as_deref().and_then(source_identity)
            };
        }
        self.remember_export(recent)
    }

    pub(super) fn remember_export(&mut self, recent: Recent) -> Result<(), String> {
        let remembered = recent.save();
        self.recent_export = Some(recent);
        remembered
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
                if let Some(recent) = saved.recent
                    && self.remember_completed_export(recent).is_err()
                {
                    self.status
                        .push_str(" · Could not remember export settings");
                }
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
