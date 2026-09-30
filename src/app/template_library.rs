use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use iced::Task;
use reshiki::{
    document::Document,
    editing,
    template_library::{Library, standard_path},
    templates::{Anchor, Connection, LIBRARY},
};
use std::{collections::VecDeque, path::PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Mine,
    Favorites,
}
impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All",
            Self::Mine => "My templates",
            Self::Favorites => "Favorites",
        })
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Browse,
    Forward,
    Search(String),
    Filter(Filter),
    Collection(String),
    Name(String),
    Category(String),
    BeginSave,
    EditDetails,
    SaveDetails,
    CancelDetails,
    Replace,
    Remove,
    Restore,
    Favorite(usize),
    Anchor(Anchor),
    Connection(Connection),
    RememberAnchor,
    Repeat(bool),
    Import,
    Imported(Result<Option<Library>, String>),
    Export,
    Reload,
    Finished(u64, Result<Box<Transaction>, String>),
}
#[derive(Clone)]
struct Location {
    query: String,
    filter: Filter,
    collection: String,
    template: Option<String>,
    anchor: Anchor,
    connection: Connection,
    scroll: f32,
}

pub fn category(template: &reshiki::templates::Template) -> &str {
    if template.id.starts_with("builtin:") {
        if matches!(template.group.as_str(), "Rings" | "Heterocycles")
            && template
                .smiles
                .chars()
                .any(|c| matches!(c, 'c' | 'n' | 'o' | 's' | 'p' | 'b'))
        {
            return "Aromatics";
        }
        if template.group == "Rings" {
            return "Cycloalkanes";
        }
    }
    &template.group
}

pub struct State {
    back: VecDeque<Location>,
    forward: VecDeque<Location>,
    pub scroll: f32,
    pub library: Library,
    pub path: Option<PathBuf>,
    pub query: String,
    pub filter: Filter,
    pub collection: String,
    pub anchor: Anchor,
    pub connection: Connection,
    pub active: bool,
    pub repeat: bool,
    pub name: String,
    pub category: String,
    pub draft: Option<Document>,
    pub editing: bool,
    pub notice: Option<String>,
    pub undo: Option<Library>,
    pending: Option<u64>,
    importing: bool,
    serial: u64,
    pending_context: Option<Context>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            back: VecDeque::new(),
            forward: VecDeque::new(),
            scroll: 0.,
            library: Library::default(),
            path: None,
            query: String::new(),
            filter: Filter::All,
            collection: "All collections".into(),
            anchor: Anchor::Auto,
            connection: Connection::Connect,
            active: false,
            repeat: true,
            name: String::new(),
            category: "My templates".into(),
            draft: None,
            editing: false,
            notice: None,
            undo: None,
            pending: None,
            importing: false,
            serial: 0,
            pending_context: None,
        }
    }
}
impl State {
    pub(super) fn pending(&self) -> bool {
        self.pending.is_some() || self.importing
    }
    fn location(&self, index: usize) -> Location {
        Location {
            query: self.query.clone(),
            filter: self.filter,
            collection: self.collection.clone(),
            template: self
                .active
                .then(|| self.library.get(index).map(|t| t.id.clone()))
                .flatten(),
            anchor: self.anchor,
            connection: self.connection,
            scroll: self.scroll,
        }
    }
    pub fn remember(&mut self, index: usize) {
        if self.back.len() >= 64 {
            self.back.pop_front();
        }
        self.back.push_back(self.location(index));
        self.forward.clear();
        self.scroll = 0.;
    }
    fn restore_location(&mut self, location: Location, index: &mut usize) {
        self.query = location.query;
        self.filter = location.filter;
        self.collection = location.collection;
        self.anchor = location.anchor;
        self.connection = location.connection;
        self.scroll = location.scroll;
        let found = location
            .template
            .and_then(|id| self.library.iter().position(|t| t.id == id));
        self.active = found.is_some();
        if let Some(found) = found {
            *index = found;
        }
        self.editing = false;
        self.draft = None;
        if self.collection != "All collections"
            && !self.library.iter().any(|t| category(t) == self.collection)
        {
            self.collection = "All collections".into();
        }
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn load() -> Self {
        let mut state = Self {
            notice: reshiki::templates::builtin_error().map(str::to_owned),
            ..Self::default()
        };
        if !cfg!(test) {
            match standard_path() {
                Ok(path) => {
                    match Library::load(&path) {
                        Ok(library) => state.library = library,
                        Err(e) => state.notice = Some(e),
                    };
                    state.path = Some(path);
                }
                Err(e) => state.notice = Some(e),
            }
        }
        state
    }
    pub fn matches(&self, index: usize, t: &reshiki::templates::Template) -> bool {
        let query = self.query.to_lowercase();
        (match self.filter {
            Filter::All => true,
            Filter::Mine => index >= LIBRARY.len(),
            Filter::Favorites => self.library.favorite(&t.id),
        }) && (self.collection == "All collections" || self.collection == category(t))
            && query.split_whitespace().all(|word| {
                format!(
                    "{} {} {} {} {}",
                    category(t),
                    t.name,
                    t.group,
                    t.smiles,
                    t.keywords.join(" ")
                )
                .to_lowercase()
                .contains(word)
            })
    }
    pub fn search_rank(&self, t: &reshiki::templates::Template) -> u8 {
        let query = self.query.trim().to_lowercase();
        let name = t.name.to_lowercase();
        if name == query || t.keywords.iter().any(|s| s.eq_ignore_ascii_case(&query)) {
            0
        } else if name.starts_with(&query)
            || t.keywords
                .iter()
                .any(|s| s.to_lowercase().starts_with(&query))
        {
            1
        } else {
            2
        }
    }
}
/// An owned transaction snapshot. Only a completed checked disk write may
/// replace the visible library; failed saves preserve drafts, navigation and Undo.
#[derive(Debug, Clone)]
pub struct Transaction {
    context: Context,
    library: Library,
    undo: Option<Library>,
    effect: Effect,
}
#[derive(Debug, Clone)]
enum Effect {
    Saved(Option<usize>),
    Updated { remove: bool, replace: bool },
    Restored,
    Favorite,
    Imported(usize),
    Reloaded,
}
#[derive(Debug, Clone, Copy)]
struct Context {
    epoch: u64,
    revision: u64,
    tool: Tool,
    index: usize,
    active: bool,
}
impl Context {
    fn capture(app: &App) -> Self {
        Self {
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
            tool: app.tool,
            index: app.template_index,
            active: app.templates.active,
        }
    }
    fn current(self, app: &App) -> bool {
        self.epoch == app.tab.file_epoch
            && self.revision == app.tab.revision
            && self.tool == app.tool
            && self.index == app.template_index
            && self.active == app.templates.active
            && app.tab.inline_text.is_none()
            && app.tab.atom_text.is_none()
    }
}
struct Operation {
    context: Context,
    previous: Library,
    path: Option<PathBuf>,
    edit: Edit,
}
enum Edit {
    Save {
        index: usize,
        name: String,
        category: String,
        draft: Option<Document>,
    },
    Replace {
        index: usize,
        drawing: Document,
    },
    Anchor {
        index: usize,
        anchor: Anchor,
    },
    Remove(usize),
    Restore(Library),
    Favorite(String),
    Import(Library),
    Reload,
}
impl Operation {
    fn handles(action: &Action) -> bool {
        matches!(
            action,
            Action::SaveDetails
                | Action::Replace
                | Action::RememberAnchor
                | Action::Remove
                | Action::Restore
                | Action::Favorite(_)
                | Action::Imported(_)
                | Action::Reload
        )
    }
    fn capture(app: &App, action: Action) -> Result<Option<Self>, String> {
        let state = &app.templates;
        let custom_index = || {
            app.template_index
                .checked_sub(LIBRARY.len())
                .ok_or_else(|| {
                    "This is a built-in template. Save a selection to make your own copy."
                        .to_string()
                })
        };
        let edit = match action {
            Action::SaveDetails => Edit::Save {
                index: app.template_index,
                name: state.name.clone(),
                category: state.category.clone(),
                draft: state.draft.clone(),
            },
            Action::Replace => Edit::Replace {
                index: custom_index()?,
                drawing: editing::selection(&app.tab.doc, &app.tab.selected),
            },
            Action::RememberAnchor => Edit::Anchor {
                index: custom_index()?,
                anchor: state.anchor,
            },
            Action::Remove => Edit::Remove(custom_index()?),
            Action::Restore => match state.undo.clone() {
                Some(library) => Edit::Restore(library),
                None => return Ok(None),
            },
            Action::Favorite(index) => Edit::Favorite(
                state
                    .library
                    .get(index)
                    .ok_or("Choose a template")?
                    .id
                    .clone(),
            ),
            Action::Imported(result) => match result? {
                Some(library) => Edit::Import(library),
                None => return Ok(None),
            },
            Action::Reload => Edit::Reload,
            _ => return Ok(None),
        };
        Ok(Some(Self {
            context: Context::capture(app),
            previous: state.library.clone(),
            path: state.path.clone(),
            edit,
        }))
    }
    fn execute(self) -> Result<Transaction, String> {
        let mut library = self.previous.clone();
        let mut undo = None;
        let effect = match self.edit {
            Edit::Save {
                index,
                name,
                category,
                draft,
            } => {
                let saved_index = if let Some(drawing) = draft {
                    Some(library.add(&name, &category, drawing, Anchor::Auto)?)
                } else {
                    let template = library
                        .templates
                        .get_mut(
                            index
                                .checked_sub(LIBRARY.len())
                                .ok_or("Save a copy of a built-in template to edit it")?,
                        )
                        .ok_or("Choose a custom template")?;
                    template.name = name.trim().into();
                    template.group = category.trim().into();
                    None
                };
                Effect::Saved(saved_index)
            }
            Edit::Replace { index, drawing } => {
                if drawing.all_ids().is_empty() {
                    return Err("Select the revised drawing to replace this template.".into());
                }
                let template = library
                    .templates
                    .get_mut(index)
                    .ok_or("Choose a custom template")?;
                template.document = drawing;
                template.smiles.clear();
                template.anchor = Anchor::Auto;
                undo = Some(self.previous.clone());
                Effect::Updated {
                    remove: false,
                    replace: true,
                }
            }
            Edit::Anchor { index, anchor } => {
                library
                    .templates
                    .get_mut(index)
                    .ok_or("Choose a custom template")?
                    .anchor = anchor;
                undo = Some(self.previous.clone());
                Effect::Updated {
                    remove: false,
                    replace: false,
                }
            }
            Edit::Remove(index) => {
                let id = library
                    .templates
                    .get(index)
                    .ok_or("Choose a custom template")?
                    .id
                    .clone();
                library.templates.remove(index);
                library.favorites.retain(|f| *f != id);
                undo = Some(self.previous.clone());
                Effect::Updated {
                    remove: true,
                    replace: false,
                }
            }
            Edit::Restore(previous) => {
                library = previous;
                Effect::Restored
            }
            Edit::Favorite(id) => {
                if library.favorite(&id) {
                    library.favorites.retain(|f| *f != id);
                } else {
                    library.favorites.push(id);
                }
                Effect::Favorite
            }
            Edit::Import(incoming) => Effect::Imported(library.merge(incoming)?),
            Edit::Reload => {
                library = Library::load(
                    self.path
                        .as_ref()
                        .ok_or("Template storage is unavailable")?,
                )?;
                return Ok(Transaction {
                    context: self.context,
                    library,
                    undo,
                    effect: Effect::Reloaded,
                });
            }
        };
        if let Some(path) = self.path {
            library.save_checked(&path, &self.previous)?;
        } else if cfg!(test) {
            library.validate()?;
        } else {
            return Err("Template storage is unavailable".into());
        }
        Ok(Transaction {
            context: self.context,
            library,
            undo,
            effect,
        })
    }
}

impl App {
    fn apply_library_transaction(&mut self, transaction: Transaction) {
        let current = transaction.context.current(self);
        let previous_id = self
            .templates
            .library
            .get(self.template_index)
            .map(|t| t.id.clone());
        let state = &mut self.templates;
        state.library = transaction.library;
        state.undo = transaction.undo;
        state.notice = None;
        if !current {
            // The library is shared across drawings, but a completed write must
            // not select a template or change tools in a newer editing context.
            if let Some(index) =
                previous_id.and_then(|id| state.library.iter().position(|t| t.id == id))
            {
                self.template_index = index;
            } else {
                self.template_index = 0;
                state.active = false;
                if self.tool == Tool::Template {
                    self.tool = Tool::Select;
                }
            }
            return;
        }
        if state.collection != "All collections"
            && !state
                .library
                .iter()
                .any(|t| category(t) == state.collection)
        {
            state.collection = "All collections".into();
        }
        match transaction.effect {
            Effect::Saved(index) => {
                if let Some(index) = index {
                    self.template_index = index;
                    state.anchor = Anchor::Auto;
                    state.active = true;
                    state.query.clear();
                    state.filter = Filter::Mine;
                    state.collection = "All collections".into();
                }
                state.editing = false;
                state.draft = None;
                self.status = "Template saved to your library".into();
                self.error = false;
            }
            Effect::Updated { remove, replace } => {
                if remove {
                    state.active = false;
                    self.template_index = 0;
                    self.tool = Tool::Select;
                    state.editing = false;
                }
                if replace {
                    state.anchor = Anchor::Auto;
                }
                self.status = "Library updated · Undo library change is available".into();
                self.error = false;
            }
            Effect::Restored | Effect::Reloaded => {
                state.active = false;
                self.template_index = 0;
                self.tool = Tool::Select;
                if matches!(transaction.effect, Effect::Reloaded) {
                    state.editing = false;
                    state.draft = None;
                }
            }
            Effect::Favorite => {}
            Effect::Imported(count) => {
                state.query.clear();
                state.filter = Filter::Mine;
                state.collection = "All collections".into();
                self.status = format!("Imported {count} new template(s)");
                self.error = false;
            }
        }
    }
    pub(super) fn template_action(&mut self, action: Action) -> Task<Message> {
        let navigation = matches!(
            &action,
            Action::Browse | Action::Forward | Action::Collection(_)
        );
        let reveal = matches!(
            action,
            Action::Browse
                | Action::Forward
                | Action::Collection(_)
                | Action::BeginSave
                | Action::EditDetails
                | Action::SaveDetails
                | Action::CancelDetails
                | Action::Remove
                | Action::Restore
                | Action::Imported(_)
        );
        if let Err(error) = self.change_library(action) {
            self.templates.notice = Some(error.clone());
            self.status = error;
            self.error = true;
        }
        if navigation {
            iced::widget::operation::scroll_to(
                "inspector-content",
                iced::widget::operation::AbsoluteOffset {
                    x: Some(0.),
                    y: Some(self.templates.scroll),
                },
            )
        } else if reveal {
            iced::widget::operation::snap_to(
                "inspector-content",
                iced::widget::operation::RelativeOffset::START,
            )
        } else {
            Task::none()
        }
    }
    pub(super) fn template_finished(
        &mut self,
        serial: u64,
        result: Result<Box<Transaction>, String>,
    ) -> Task<Message> {
        if self.templates.pending != Some(serial) {
            return Task::none();
        }
        self.templates.pending = None;
        if result.is_err() {
            self.cancel_close();
        }
        let current = self
            .templates
            .pending_context
            .take()
            .is_some_and(|context| context.current(self));
        match result {
            Ok(transaction) => self.apply_library_transaction(*transaction),
            Err(error) => {
                self.templates.notice = Some(error.clone());
                if current {
                    self.status = error;
                    self.error = true;
                }
            }
        }
        Task::none()
    }
    pub(super) fn template_async(&mut self, action: &Action) -> Option<Task<Message>> {
        if matches!(action, Action::Imported(_)) {
            self.templates.importing = false;
            if matches!(action, Action::Imported(Err(_))) {
                self.cancel_close();
            }
        } else if self.templates.importing {
            self.status = "A template collection is being opened".into();
            return Some(Task::none());
        }
        if self.templates.pending.is_some() {
            // Serialize library transactions only; canvas editing and unrelated
            // commands remain available while a slow disk write is in flight.
            self.status = "Template library update is in progress".into();
            return Some(Task::none());
        }
        if self.templates.path.is_some() && Operation::handles(action) {
            match Operation::capture(self, action.clone()) {
                Ok(Some(operation)) => {
                    self.templates.serial = self.templates.serial.wrapping_add(1);
                    let serial = self.templates.serial;
                    self.templates.pending = Some(serial);
                    self.templates.pending_context = Some(operation.context);
                    return Some(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || operation.execute().map(Box::new))
                                .await
                                .map_err(|e| e.to_string())
                                .and_then(|r| r)
                        },
                        move |result| Message::Templates(Action::Finished(serial, result)),
                    ));
                }
                Ok(None) => return Some(Task::none()),
                Err(error) => {
                    self.templates.notice = Some(error.clone());
                    self.status = error;
                    self.error = true;
                    return Some(Task::none());
                }
            }
        }
        match action {
            Action::Import => {
                self.templates.importing = true;
                Some(Task::perform(
                    async {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_title("Import a ReShiki template collection")
                            .pick_file()
                            .await
                        else {
                            return Ok(None);
                        };
                        let path = file.path().to_path_buf();
                        tokio::task::spawn_blocking(move || {
                            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                            Library::from_bytes(&bytes).map(Some)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    |r| Message::Templates(Action::Imported(r)),
                ))
            }
            Action::Export => {
                let library = self.templates.library.clone();
                Some(Task::perform(
                    async move {
                        let Some(path) = super::files::save_path(
                            "Export my templates and favorites",
                            "My templates.reshiki-templates",
                            "reshiki-templates",
                        )
                        .await
                        else {
                            return Ok(None);
                        };
                        let target = path.clone();
                        tokio::task::spawn_blocking(move || library.save(&target))
                            .await
                            .map_err(|e| e.to_string())??;
                        Ok(Some(path))
                    },
                    Message::Exported,
                ))
            }
            _ => None,
        }
    }
    fn change_library(&mut self, action: Action) -> Result<(), String> {
        let state = &mut self.templates;
        match action {
            Action::Browse => {
                let current = state.location(self.template_index);
                if let Some(previous) = state.back.pop_back() {
                    if state.forward.len() >= 64 {
                        state.forward.pop_front();
                    }
                    state.forward.push_back(current);
                    state.restore_location(previous, &mut self.template_index);
                } else if state.active {
                    state.active = false;
                    state.editing = false;
                    state.draft = None;
                } else {
                    state.query.clear();
                    state.collection = "All collections".into();
                    state.filter = Filter::All;
                }
                self.tool = if state.active {
                    Tool::Template
                } else {
                    Tool::Select
                };
            }
            Action::Forward => {
                if let Some(next) = state.forward.pop_back() {
                    if state.back.len() >= 64 {
                        state.back.pop_front();
                    }
                    state.back.push_back(state.location(self.template_index));
                    state.restore_location(next, &mut self.template_index);
                    self.tool = if state.active {
                        Tool::Template
                    } else {
                        Tool::Select
                    };
                }
            }
            Action::Search(s) => state.query = s,
            Action::Filter(f) => {
                state.remember(self.template_index);
                state.filter = f;
            }
            Action::Collection(s) => {
                state.remember(self.template_index);
                state.collection = s;
                state.active = false;
                self.tool = Tool::Select;
            }
            Action::Name(s) => state.name = s,
            Action::Category(s) => state.category = s,
            Action::Repeat(on) => state.repeat = on,
            Action::Connection(mode) => {
                state.connection = mode;
                if matches!(
                    (mode, state.anchor),
                    (Connection::FuseBond, Anchor::Atom(_))
                        | (
                            Connection::Connect | Connection::ShareAtom,
                            Anchor::Bond(..)
                        )
                ) {
                    state.anchor = Anchor::Auto;
                }
                self.tool = Tool::Template;
            }
            Action::Anchor(anchor) => {
                if state
                    .library
                    .get(self.template_index)
                    .is_some_and(|t| anchor.valid(&t.document))
                {
                    state.anchor = anchor;
                    if matches!(anchor, Anchor::Bond(..)) {
                        state.connection = Connection::FuseBond;
                    } else if matches!(anchor, Anchor::Atom(_))
                        && state.connection == Connection::FuseBond
                    {
                        state.connection = Connection::Connect;
                    }
                    self.tool = Tool::Template;
                }
            }
            Action::BeginSave => {
                let doc = editing::selection(&self.tab.doc, &self.tab.selected);
                if doc.all_ids().is_empty() {
                    return Err(
                        "Select a fragment, caption or graphic to save as a template.".into(),
                    );
                }
                doc.validate()?;
                state.draft = Some(doc);
                state.editing = true;
                state.name.clear();
                state.category = "My templates".into();
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Templates;
            }
            Action::EditDetails => {
                let t = state
                    .library
                    .get(self.template_index)
                    .ok_or("Choose a template")?;
                state.name = t.name.clone();
                state.category = t.group.clone();
                state.draft = None;
                state.editing = true;
            }
            Action::CancelDetails => {
                state.editing = false;
                state.draft = None;
            }
            Action::SaveDetails
            | Action::Replace
            | Action::RememberAnchor
            | Action::Remove
            | Action::Restore
            | Action::Favorite(_)
            | Action::Imported(_)
            | Action::Reload => {
                if let Some(operation) = Operation::capture(self, action)? {
                    let transaction = operation.execute()?;
                    self.apply_library_transaction(transaction);
                }
            }
            Action::Finished(..) => {}
            Action::Import | Action::Export => {}
        }
        Ok(())
    }
}

/// Mouse buttons used by browsers, including native macOS auxiliary button codes.
pub fn navigation_event(event: &iced::Event) -> Option<bool> {
    use iced::mouse;
    match event {
        iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Back | mouse::Button::Other(3),
        )) => Some(false),
        iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Forward | mouse::Button::Other(4),
        )) => Some(true),

        _ => None,
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    fn persisted_app() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _) = App::new();
        app.templates.path = Some(dir.path().join("templates.json"));
        let mut fragment = Document::default();
        fragment.add_atom("O", Default::default());
        app.templates.draft = Some(fragment);
        app.templates.editing = true;
        app.templates.name = "Hydroxyl".into();
        (app, dir)
    }

    #[test]
    fn import_read_serializes_library_changes_and_releases_slot_on_cancellation() {
        let (mut app, _dir) = persisted_app();
        let task = app.update(Message::Templates(Action::Import));
        assert!(task.units() > 0 && app.templates.pending());
        assert!(app.templates.importing);
        let _ = app.update(Message::Templates(Action::SaveDetails));
        let _ = app.update(Message::Templates(Action::Reload));
        let _ = app.update(Message::Templates(Action::Import));
        assert!(
            app.templates.pending.is_none(),
            "No library write may overtake an import read"
        );
        assert!(app.templates.library.templates.is_empty());
        let _ = app.update(Message::Templates(Action::Imported(Ok(None))));
        assert!(!app.templates.pending());
        let _ = app.update(Message::Templates(Action::SaveDetails));
        assert!(app.templates.pending.is_some());
    }

    #[test]
    fn checked_worker_save_preserves_draft_until_success_and_rejects_competing_writer() {
        let (mut app, _dir) = persisted_app();
        let operation = Operation::capture(&app, Action::SaveDetails)
            .unwrap()
            .unwrap();
        let task = app.update(Message::Templates(Action::SaveDetails));
        assert!(task.units() > 0 && app.templates.pending());
        assert!(app.templates.library.templates.is_empty());
        assert!(app.templates.draft.is_some());
        assert!(!app.templates.path.as_ref().unwrap().exists());
        let serial = app.templates.pending.unwrap();
        let _ = app.update(Message::Templates(Action::Reload));
        assert_eq!(app.templates.pending, Some(serial));
        let mut competing = Library::default();
        competing
            .add(
                "Other window",
                "Mine",
                app.templates.draft.clone().unwrap(),
                Anchor::Auto,
            )
            .unwrap();
        competing
            .save(app.templates.path.as_ref().unwrap())
            .unwrap();
        let failure = operation.execute().map(Box::new);
        assert!(
            failure
                .as_ref()
                .unwrap_err()
                .contains("changed in another window")
        );
        let _ = app.update(Message::Templates(Action::Finished(serial, failure)));
        assert!(!app.templates.pending());
        assert!(app.templates.draft.is_some());
        assert!(app.templates.library.templates.is_empty());
        assert_eq!(
            Library::load(app.templates.path.as_ref().unwrap()).unwrap(),
            competing
        );
    }

    #[test]
    fn stale_library_completion_updates_storage_without_switching_new_drawing_tools() {
        let (mut app, _dir) = persisted_app();
        let operation = Operation::capture(&app, Action::SaveDetails)
            .unwrap()
            .unwrap();
        let _ = app.update(Message::Templates(Action::SaveDetails));
        let serial = app.templates.pending.unwrap();
        let transaction = operation.execute().map(Box::new);
        let _ = app.update(Message::New);
        app.status = "New drawing is active".into();
        let original = app.tab.doc.clone();
        let tool = app.tool;
        let _ = app.update(Message::Templates(Action::Finished(serial, transaction)));
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tool, tool);
        assert_eq!(app.status, "New drawing is active");
        assert_eq!(app.templates.library.templates.len(), 1);
        assert!(!app.templates.pending());
        let _ = app.update(Message::Templates(Action::Finished(
            serial,
            Err("duplicate late result".into()),
        )));
        assert_eq!(app.status, "New drawing is active");
    }

    #[test]
    fn library_completion_during_inline_edit_does_not_discard_draft() {
        let (mut app, _dir) = persisted_app();
        let operation = Operation::capture(&app, Action::SaveDetails)
            .unwrap()
            .unwrap();
        let _ = app.update(Message::Templates(Action::SaveDetails));
        let serial = app.templates.pending.unwrap();
        let transaction = operation.execute().map(Box::new);
        let _ = app.inline_action(super::super::inline_text::Action::Begin(
            None,
            Default::default(),
        ));
        assert!(app.tab.inline_text.is_some());
        let _ = app.update(Message::Templates(Action::Finished(serial, transaction)));
        assert!(app.tab.inline_text.is_some());
        assert!(!app.templates.pending());
        assert_eq!(app.templates.library.templates.len(), 1);
    }

    #[test]
    fn browsing_back_and_forward_restores_category_scroll_and_anchor() {
        let (mut app, _) = App::new();
        app.templates.scroll = 25.;
        let _ = app.template_action(Action::Collection("Aromatics".into()));
        app.templates.scroll = 70.;
        let index = app
            .templates
            .library
            .iter()
            .position(|t| t.name == "Furan")
            .unwrap();
        let _ = app.update(Message::InsertTemplate(index));
        let anchor = Anchor::Atom(app.templates.library.get(index).unwrap().document.atoms[0].id);
        let _ = app.template_action(Action::Anchor(anchor));
        let _ = app.template_action(Action::Browse);
        assert!(!app.templates.active);
        assert_eq!(app.templates.collection, "Aromatics");
        assert_eq!(app.templates.scroll, 70.);
        let _ = app.template_action(Action::Browse);
        assert_eq!(app.templates.collection, "All collections");
        assert_eq!(app.templates.scroll, 25.);
        let _ = app.template_action(Action::Forward);
        let _ = app.template_action(Action::Forward);
        assert!(app.templates.active);
        assert_eq!(app.template_index, index);
        assert_eq!(app.templates.anchor, anchor);
        let _ = app.template_action(Action::Browse);
        let _ = app.template_action(Action::Collection("Amino acids".into()));
        assert!(!app.templates.can_forward());
    }

    #[test]
    fn browser_mouse_buttons_have_the_correct_direction() {
        use iced::{
            Event,
            mouse::{Button, Event::ButtonPressed},
        };
        for button in [Button::Back, Button::Other(3)] {
            assert_eq!(
                navigation_event(&Event::Mouse(ButtonPressed(button))),
                Some(false)
            );
        }
        for button in [Button::Forward, Button::Other(4)] {
            assert_eq!(
                navigation_event(&Event::Mouse(ButtonPressed(button))),
                Some(true)
            );
        }
        assert_eq!(
            navigation_event(&Event::Mouse(ButtonPressed(Button::Left))),
            None
        );
    }
}
