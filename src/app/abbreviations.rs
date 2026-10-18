use super::{App, InspectorTab, Job, Message, Request, Tool};
use iced::Task;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Action {
    Preset(String),
    Label(String),
    ReverseLabel(String),
    Replace,
    Find,
    Contract,
    Define,
    Expand,
    ExpandAll,
}
pub struct State {
    pub preset: String,
    pub label: String,
    pub reverse_label: String,
    /// Skipped user entries, kept in the panel after the status line changes.
    pub catalog_warning: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            preset: "OMe".into(),
            label: String::new(),
            reverse_label: String::new(),
            catalog_warning: None,
        }
    }
}
impl App {
    pub(super) fn abbreviation_action(&mut self, action: Action) -> Task<Message> {
        let before = self.tab.doc.clone();
        match action {
            Action::Preset(value) => self.abbreviations.preset = value,
            Action::Label(value) => self.abbreviations.label = value,
            Action::ReverseLabel(value) => self.abbreviations.reverse_label = value,
            Action::Replace | Action::Find => {
                let replace = matches!(action, Action::Replace);
                if replace && reshiki::ligands::LABELS.contains(&self.abbreviations.preset.as_str())
                {
                    let anchor = self
                        .tab
                        .doc
                        .abbreviations
                        .iter()
                        .find(|g| {
                            !self.tab.selected.is_empty()
                                && self.tab.selected.iter().all(|id| g.members.contains(id))
                        })
                        .map(|g| g.anchor)
                        .or_else(|| {
                            (self.tab.selected.len() == 1)
                                .then(|| self.tab.selected.first().copied())
                                .flatten()
                        });
                    match anchor
                        .ok_or_else(|| "Select one endpoint or an existing group".to_string())
                        .and_then(|id| {
                            reshiki::ligands::replace(&self.tab.doc, id, &self.abbreviations.preset)
                        }) {
                        Ok(doc) => {
                            self.tab.doc = doc;
                            self.changed(before);
                            self.tab.selected = anchor.into_iter().collect();
                            self.status = "Ligand abbreviated · Real atoms and five-center attachment retained".into();
                            self.error = false;
                            self.tool = Tool::Select;
                        }
                        Err(error) => {
                            self.status = error;
                            self.error = true;
                        }
                    }
                    return Task::none();
                }
                let mut request = Request::molecule("abbreviate", self.tab.doc.clone());
                request.selected_ids = Some(self.tab.selected.clone());
                request.format = Some(if replace { "replace" } else { "find" }.into());
                request.text = replace.then(|| self.abbreviations.preset.clone());
                return self.run(request, Job::Abbreviate);
            }
            Action::Contract => {
                if let Err(error) = self.tab.doc.contract(
                    &self.tab.selected,
                    &self.abbreviations.label,
                    &self.abbreviations.reverse_label,
                ) {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
                self.changed(before);
                self.tool = Tool::Select;
                self.status = "Fragment abbreviated · All atoms and bonds retained · Expand restores the drawing".into();
            }
            Action::Define => match self.define_user_group() {
                Ok(message) => {
                    self.changed(before);
                    self.tool = Tool::Select;
                    self.status = message;
                    self.error = false;
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
            },
            Action::Expand | Action::ExpandAll => {
                let ids = if matches!(action, Action::ExpandAll) {
                    self.tab.doc.all_ids()
                } else {
                    self.tab.selected.clone()
                };
                let count = self.tab.doc.expand_abbreviations(&ids);
                self.changed(before);
                self.status = format!(
                    "Expanded {count} abbreviation{}",
                    if count == 1 { "" } else { "s" }
                );
                self.tool = Tool::Select;
            }
        }
        self.inspector_tab = InspectorTab::Abbreviations;
        Task::none()
    }
}

/// The user's own groups, in the bundled catalog's JSON format.
pub(super) fn user_catalog_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("RESHIKI_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
                .map(|d| d.data_local_dir().to_owned())
        })
        .ok_or("No application data directory")?;
    Ok(root.join("abbreviations.json"))
}

/// Install the user's groups at startup. Returns a status message when the
/// file exists but some entries could not be used; the app still starts.
pub(super) fn load_user_catalog() -> Option<String> {
    if cfg!(test) {
        return None;
    }
    let path = user_catalog_path().ok()?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => return Some(format!("Could not read {}: {error}", path.display())),
    };
    let errors = reshiki::chemistry::abbreviations::set_user(Some(&text));
    catalog_warning(&errors, &path)
}

fn catalog_warning(errors: &[String], path: &std::path::Path) -> Option<String> {
    let first = errors.first()?;
    Some(format!(
        "Your abbreviations file skipped {} group{}: {first} ({})",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" },
        path.display()
    ))
}

impl App {
    /// Save the selected fragment as a user group, then show it collapsed
    /// under that label. The drawing keeps every atom; only the label changes.
    fn define_user_group(&mut self) -> Result<String, String> {
        use reshiki::chemistry::abbreviations as catalog;
        let label = self.abbreviations.label.trim().to_owned();
        let reverse = self.abbreviations.reverse_label.trim().to_owned();
        let defined = catalog::from_selection(&self.tab.doc, &self.tab.selected, &label, &reverse)?;
        let path = user_catalog_path()?;
        let existing = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Could not read {}: {error}", path.display())),
        };
        let smiles = defined.entry.smiles.clone();
        let text = catalog::upsert(existing.as_deref(), defined.entry)?;
        // Check the new file before writing it, so a bad save never lands on
        // disk: the new group must load and no group that loaded before may be
        // lost. Entries already skipped at startup stay as they were.
        let before = existing
            .as_deref()
            .map(|text| catalog::parse(text, true).0)
            .unwrap_or_default();
        let (after, errors) = catalog::parse(&text, true);
        let loads = |name: &str| after.iter().any(|p| p.label == name);
        let failed = if after.iter().any(|p| p.label == label && p.smiles == smiles) {
            before
                .iter()
                .map(|p| p.label.as_str())
                .find(|old| *old != label && !loads(old))
        } else {
            Some(label.as_str())
        };
        if let Some(name) = failed {
            let tag = format!("({name})");
            return Err(errors
                .into_iter()
                .find(|error| error.contains(&tag))
                .unwrap_or_else(|| format!("Could not save {label}")));
        }
        let mut doc = self.tab.doc.clone();
        doc.contract(&defined.members, &label, &reverse)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        reshiki::storage::write_atomic(&path, text.as_bytes())?;
        let skipped = catalog::set_user(Some(&text));
        self.abbreviations.catalog_warning = catalog_warning(&skipped, &path);
        self.tab.doc = doc;
        Ok(format!(
            "Saved {label} to your abbreviations · Type it on any endpoint · {}",
            path.display()
        ))
    }
}
