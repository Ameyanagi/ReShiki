//! Theme picker and portable library, independent of journal drawing styles.
use super::{App, Message};
use iced::Task;
use reshiki::{
    canvas_theme::ColorTheme,
    theme_files::{self, ThemeFile},
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Builtin(ColorTheme),
    Library { id: String, name: String },
    Load,
    Save,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin(theme) => theme.fmt(f),
            Self::Library { name, .. } => f.write_str(name),
            Self::Load => f.write_str("Import theme…"),
            Self::Save => f.write_str("Export theme…"),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Choose(Choice),
    Loaded(u64, u64, Result<Option<Box<ThemeFile>>, String>),
    Saved(Result<bool, String>),
}
#[derive(Default)]
pub struct State {
    themes: Vec<ThemeFile>,
    serial: u64,
}
fn directory() -> Result<PathBuf, String> {
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
    pub fn load() -> Self {
        let mut state = Self {
            themes: theme_files::bundled().unwrap_or_default(),
            serial: 0,
        };
        if !cfg!(test)
            && let Ok(dir) =
                directory().and_then(|d| std::fs::read_dir(d).map_err(|e| e.to_string()))
        {
            for entry in dir.flatten().take(256) {
                if entry
                    .path()
                    .extension()
                    .is_some_and(|e| e == "reshiki-theme")
                    && let Ok(theme) = theme_files::load(&entry.path())
                {
                    state.insert(theme);
                }
            }
        }
        state
    }
    fn insert(&mut self, theme: ThemeFile) {
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
        let selected =
            self.doc
                .custom_theme
                .as_ref()
                .map_or(Choice::Builtin(self.doc.color_theme), |t| Choice::Library {
                    id: t.id.clone(),
                    name: t.name.clone(),
                });
        if !choices.contains(&selected) {
            choices.push(selected.clone());
        }
        choices.extend([Choice::Load, Choice::Save]);
        (choices, selected)
    }
    pub(super) fn theme_file_action(&mut self, action: Action) -> Task<Message> {
        match action {
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
                        self.doc
                            .custom_theme
                            .as_deref()
                            .filter(|t| t.id == id)
                            .cloned()
                    });
                if let Some(theme) = theme {
                    self.apply_theme_file(theme);
                }
            }
            Action::Choose(Choice::Load) => {
                self.theme_library.serial = self.theme_library.serial.wrapping_add(1);
                let (epoch, serial) = (self.file_epoch, self.theme_library.serial);
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
                if epoch != self.file_epoch || serial != self.theme_library.serial {
                    return Task::none();
                }
                match result {
                    Ok(Some(theme)) => {
                        let theme = *theme;
                        if !self.apply_theme_file(theme.clone()) {
                            return Task::none();
                        }
                        let saved = if cfg!(test) {
                            Ok(())
                        } else {
                            directory().and_then(|dir| {
                                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                                theme_files::save(
                                    &dir.join(format!("{}.reshiki-theme", theme.id)),
                                    &theme,
                                )
                            })
                        };
                        self.theme_library.insert(theme);
                        if let Err(e) = saved {
                            self.status = format!(
                                "Theme applied to drawing, but could not save to library: {e}"
                            );
                            self.error = true;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        self.status = e;
                        self.error = true;
                    }
                }
            }
            Action::Choose(Choice::Save) => {
                let theme = ThemeFile::capture(&self.doc);
                return Task::perform(
                    async move {
                        let Some(path) = super::files::save_path(
                            "Export color theme (light and dark)",
                            &format!("{}.reshiki-theme", theme.id),
                            "reshiki-theme",
                        )
                        .await
                        else {
                            return Ok(false);
                        };
                        tokio::task::spawn_blocking(move || theme_files::save(&path, &theme))
                            .await
                            .map_err(|e| e.to_string())??;
                        Ok(true)
                    },
                    |r| Message::ThemeFile(Action::Saved(r)),
                );
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
    fn apply_theme_file(&mut self, theme: ThemeFile) -> bool {
        if !self.finish_inline(true) {
            return false;
        }
        let name = theme.name.clone();
        let before = self.doc.clone();
        match theme.apply(&mut self.doc) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn importing_a_theme_is_undoable_and_stale_dialogs_do_not_change_drawings() {
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        let theme = theme_files::bundled().unwrap().remove(0);
        let _ = app.theme_file_action(Action::Loaded(
            app.file_epoch + 1,
            0,
            Ok(Some(Box::new(theme.clone()))),
        ));
        assert_eq!(app.doc, original);
        let _ = app.theme_file_action(Action::Loaded(
            app.file_epoch,
            0,
            Ok(Some(Box::new(theme.clone()))),
        ));
        let themed = app.doc.clone();
        assert!(themed.custom_theme.is_some());
        assert_eq!(themed.drawing_style, original.drawing_style);
        assert_eq!(themed.canvas_theme, original.canvas_theme);
        assert!(!super::super::same_drawing(&themed, &original));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, themed);
        let _ = app.update(Message::QuickDrawingStyle(
            super::super::document_styles::Choice::Journal(
                reshiki::document_styles::Preset::Nature,
            ),
        ));
        assert!(app.doc.custom_theme.is_some());
        assert_eq!(app.doc.version, 16);
        let _ = app.update(Message::ColorTheme(ColorTheme::Jmol));
        assert!(app.doc.custom_theme.is_none());
    }
}
