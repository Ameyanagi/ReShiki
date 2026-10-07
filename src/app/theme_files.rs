//! Theme picker and portable library, independent of journal drawing styles.
use super::{App, Message};
use iced::Task;
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    theme_files::{self, ThemeFile},
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Builtin(ColorTheme),
    Library { id: String, name: String },
    Manage,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin(theme) => theme.fmt(f),
            Self::Library { name, .. } => f.write_str(name),
            Self::Manage => f.write_str("Manage themes…"),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Choose(Choice),
    Import,
    Loaded(u64, u64, Result<Option<Box<ThemeFile>>, String>),
    Saved(Result<bool, String>),
}
#[derive(Default)]
pub struct State {
    themes: Vec<ThemeFile>,
    pub(super) serial: u64,
    pub(super) editor: Option<super::theme_generator::Editor>,
}
pub(super) fn directory() -> Result<PathBuf, String> {
    let root = std::env::var_os("RESHIKI_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
                .map(|d| d.data_local_dir().to_owned())
        })
        .ok_or("No application data directory")?;
    Ok(root.join("themes"))
}
impl State {
    pub(super) fn themes(&self) -> &[ThemeFile] {
        &self.themes
    }
    pub fn load() -> Self {
        let mut state = Self::default();
        if !cfg!(test)
            && let Ok(dir) = directory()
        {
            state.load_from(&dir);
        }
        state
    }
    fn load_from(&mut self, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut themes: Vec<_> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "reshiki-theme"))
            .filter_map(|e| theme_files::load(&e.path()).ok().map(|t| (e.path(), t)))
            .collect();
        // Canonical app saves win over older, manually named files with the same ID.
        themes.sort_by_key(|(p, t)| (p.file_stem().is_some_and(|s| s == t.id.as_str()), p.clone()));
        for (_, theme) in themes {
            self.insert(theme);
        }
    }
    pub(super) fn persist(&mut self, theme: ThemeFile) -> Result<(), String> {
        if !cfg!(test) {
            let dir = directory()?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            theme_files::save(&dir.join(format!("{}.reshiki-theme", theme.id)), &theme)?;
        }
        self.insert(theme);
        Ok(())
    }
    pub(super) fn remove(&mut self, id: &str) -> Result<(), String> {
        if !self.themes.iter().any(|t| t.id == id) {
            return Err("Theme is not in the library".into());
        }
        if !cfg!(test) {
            self.archive_from(&directory()?, id)?;
        }
        self.themes.retain(|t| t.id != id);
        Ok(())
    }
    /// Archive only validated library files with the selected ID. Original imports
    /// and embedded document snapshots are never touched. Keep a recoverable copy.
    fn archive_from(&self, dir: &std::path::Path, id: &str) -> Result<(), String> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        let paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "reshiki-theme"))
            .filter(|p| theme_files::load(p).is_ok_and(|t| t.id == id))
            .collect();
        if paths.is_empty() {
            return Ok(());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let archive = dir.join(".deleted").join(stamp.to_string());
        std::fs::create_dir_all(&archive).map_err(|e| e.to_string())?;
        let mut moved = Vec::new();
        for path in paths {
            let dest = archive.join(path.file_name().ok_or("Invalid library filename")?);
            if let Err(e) = std::fs::rename(&path, &dest) {
                for (old, new) in moved.iter().rev() {
                    let _ = std::fs::rename(new, old);
                }
                return Err(format!("Could not remove theme: {e}"));
            }
            moved.push((path, dest));
        }
        Ok(())
    }
    pub(super) fn insert(&mut self, theme: ThemeFile) {
        self.themes.retain(|t| t.id != theme.id);
        self.themes.push(theme);
        self.themes
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    }
}
impl App {
    pub(super) fn theme_choices(&self) -> (Vec<Choice>, Choice) {
        let mut choices: Vec<_> = ColorTheme::ALL.into_iter().map(Choice::Builtin).collect();
        choices.extend(self.theme_library.themes.iter().map(|t| Choice::Library {
            id: t.id.clone(),
            name: t.name.clone(),
        }));
        let selected = self.tab.doc.custom_theme.as_ref().map_or(
            Choice::Builtin(self.tab.doc.color_theme),
            |t| Choice::Library {
                id: t.id.clone(),
                name: t.name.clone(),
            },
        );
        // A document may retain a deleted theme; show its name as selected,
        // without adding that embedded snapshot back to the saved library menu.
        choices.push(Choice::Manage);
        (choices, selected)
    }
    pub(super) fn theme_file_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Choose(Choice::Manage) => {
                return self.theme_generator_action(super::theme_generator::Action::Open);
            }
            Action::Choose(Choice::Builtin(theme)) => {
                return self.update(Message::ColorTheme(theme));
            }
            Action::Choose(Choice::Library { id, .. }) => {
                let theme = self
                    .theme_library
                    .themes
                    .iter()
                    .find(|t| t.id == id)
                    .cloned()
                    .or_else(|| {
                        self.tab
                            .doc
                            .custom_theme
                            .as_deref()
                            .filter(|t| t.id == id)
                            .cloned()
                    });
                if let Some(theme) = theme {
                    self.apply_theme_file(theme);
                }
            }
            Action::Import => {
                if self.theme_library.editor.is_none() {
                    return Task::none();
                }
                self.theme_library.serial = self.theme_library.serial.wrapping_add(1);
                let (epoch, serial) = (self.tab.file_epoch, self.theme_library.serial);
                return Task::perform(
                    async {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_title("Import color theme")
                            .add_filter("ReShiki theme", &["reshiki-theme", "json"])
                            .pick_file()
                            .await
                        else {
                            return Ok(None);
                        };
                        let path = file.path().to_path_buf();
                        tokio::task::spawn_blocking(move || theme_files::load(&path))
                            .await
                            .map_err(|e| e.to_string())?
                            .map(|theme| Some(Box::new(theme)))
                    },
                    move |result| Message::ThemeFile(Action::Loaded(epoch, serial, result)),
                );
            }
            Action::Loaded(epoch, serial, result) => {
                if epoch != self.tab.file_epoch || serial != self.theme_library.serial {
                    return Task::none();
                }
                match result {
                    Ok(Some(theme)) => {
                        self.review_imported_theme(*theme);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        self.status = e;
                        self.error = true;
                    }
                }
            }
            Action::Saved(result) => match result {
                Ok(true) => {
                    self.status = "Theme exported · Includes light and dark palettes".into();
                    self.error = false;
                }
                Ok(false) => {}
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            },
        }
        Task::none()
    }
    pub(super) fn apply_theme_file(&mut self, theme: ThemeFile) -> bool {
        if !self.finish_inline(true) {
            return false;
        }
        let name = theme.name.clone();
        let before = self.tab.doc.clone();
        match theme.apply(&mut self.tab.doc) {
            Ok(()) => {
                self.changed(before);
                self.sync_color_input();
                if self.error {
                    return false;
                }
                self.status = format!(
                    "{name} theme · Light and dark palettes loaded · Undo restores previous colors"
                );
                !self.error
            }
            Err(e) => {
                self.status = e;
                self.error = true;
                false
            }
        }
    }
    pub(super) fn apply_color_theme(&mut self, theme: ColorTheme) {
        if !self.finish_inline(true) {
            return;
        }
        let before = self.tab.doc.clone();
        theme.apply(&mut self.tab.doc);
        self.changed(before);
        self.sync_color_input();
        self.status = format!(
            "{theme} colors · Journal dimensions unchanged · Undo restores previous colors"
        );
    }
    pub(super) fn set_canvas_theme(&mut self, theme: CanvasTheme) {
        if !self.finish_inline(true) {
            return;
        }
        if self.tab.doc.canvas_theme != theme {
            let before = self.tab.doc.clone();
            self.tab.doc.canvas_theme = theme;
            self.changed(before);
            self.sync_color_input();
            self.status =
                format!("{theme} canvas · Copies retain ink colors on a transparent background");
        }
    }
}

#[cfg(test)]
mod tests;
