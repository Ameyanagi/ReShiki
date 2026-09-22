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
            repeat: false,
            name: String::new(),
            category: "My templates".into(),
            draft: None,
            editing: false,
            notice: None,
            undo: None,
        }
    }
}
impl State {
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
    fn commit(&mut self, next: Library) -> Result<(), String> {
        next.validate()?;
        if let Some(path) = &self.path {
            next.save_checked(path, &self.library)?;
        } else if !cfg!(test) {
            return Err("Template storage is unavailable".into());
        }
        self.library = next;
        if self.collection != "All collections"
            && !self.library.iter().any(|t| category(t) == self.collection)
        {
            self.collection = "All collections".into();
        }
        self.notice = None;
        self.undo = None;
        Ok(())
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
impl App {
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
    pub(super) fn template_async(&mut self, action: &Action) -> Option<Task<Message>> {
        match action {
            Action::Import => Some(Task::perform(
                async {
                    let Some(file) = rfd::AsyncFileDialog::new()
                        .set_title("Import a ReShiki template collection")
                        .pick_file()
                        .await
                    else {
                        return Ok(None);
                    };
                    let bytes = std::fs::read(file.path()).map_err(|e| e.to_string())?;
                    Library::from_bytes(&bytes).map(Some)
                },
                |r| Message::Templates(Action::Imported(r)),
            )),
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
                        library.save(&path)?;
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
                let doc = editing::selection(&self.doc, &self.selected);
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
            Action::SaveDetails => {
                let mut next = state.library.clone();
                if let Some(doc) = &state.draft {
                    let index =
                        next.add(&state.name, &state.category, doc.clone(), Anchor::Auto)?;
                    state.commit(next)?;
                    self.template_index = index;
                    state.anchor = Anchor::Auto;
                    state.active = true;
                    state.query.clear();
                    state.filter = Filter::Mine;
                    state.collection = "All collections".into();
                } else {
                    let t = next
                        .templates
                        .get_mut(
                            self.template_index
                                .checked_sub(LIBRARY.len())
                                .ok_or("Save a copy of a built-in template to edit it")?,
                        )
                        .ok_or("Choose a custom template")?;
                    t.name = state.name.trim().into();
                    t.group = state.category.trim().into();
                    state.commit(next)?;
                }
                state.editing = false;
                state.draft = None;
                self.status = "Template saved to your library".into();
                self.error = false;
            }
            Action::Replace | Action::RememberAnchor | Action::Remove => {
                let i = self.template_index.checked_sub(LIBRARY.len()).ok_or(
                    "This is a built-in template. Save a selection to make your own copy.",
                )?;
                let mut next = state.library.clone();
                let t = next
                    .templates
                    .get_mut(i)
                    .ok_or("Choose a custom template")?;
                match action {
                    Action::Replace => {
                        let doc = editing::selection(&self.doc, &self.selected);
                        if doc.all_ids().is_empty() {
                            return Err(
                                "Select the revised drawing to replace this template.".into()
                            );
                        }
                        doc.validate()?;
                        t.document = doc;
                        t.smiles.clear();
                        t.anchor = Anchor::Auto;
                    }
                    Action::RememberAnchor => t.anchor = state.anchor,
                    Action::Remove => {
                        let id = t.id.clone();
                        next.templates.remove(i);
                        next.favorites.retain(|f| *f != id);
                    }
                    _ => return Err("This action cannot modify a template".into()),
                }
                let previous = state.library.clone();
                state.commit(next)?;
                state.undo = Some(previous);
                if matches!(action, Action::Remove) {
                    state.active = false;
                    self.template_index = 0;
                    self.tool = Tool::Select;
                    state.editing = false;
                }
                if matches!(action, Action::Replace) {
                    state.anchor = Anchor::Auto;
                }
                self.status = "Library updated · Undo library change is available".into();
                self.error = false;
            }
            Action::Restore => {
                if let Some(previous) = state.undo.clone() {
                    state.commit(previous)?;
                    state.undo = None;
                    state.active = false;
                    self.template_index = 0;
                    self.tool = Tool::Select;
                }
            }
            Action::Favorite(index) => {
                let id = state
                    .library
                    .get(index)
                    .ok_or("Choose a template")?
                    .id
                    .clone();
                let mut next = state.library.clone();
                if next.favorite(&id) {
                    next.favorites.retain(|f| *f != id);
                } else {
                    next.favorites.push(id);
                }
                state.commit(next)?;
            }
            Action::Imported(result) => {
                if let Some(incoming) = result? {
                    let mut next = state.library.clone();
                    let count = next.merge(incoming)?;
                    state.commit(next)?;
                    state.query.clear();
                    state.filter = Filter::Mine;
                    state.collection = "All collections".into();
                    self.status = format!("Imported {count} new template(s)");
                    self.error = false;
                }
            }
            Action::Reload => {
                let path = state
                    .path
                    .as_ref()
                    .ok_or("Template storage is unavailable")?;
                let library = Library::load(path)?;
                state.library = library;
                state.notice = None;
                state.active = false;
                state.editing = false;
                state.draft = None;
                state.undo = None;
                self.template_index = 0;
                self.tool = Tool::Select;
            }
            Action::Import | Action::Export => {}
        }
        Ok(())
    }
}

/// Mouse buttons used by browsers, including native macOS auxiliary button codes.
pub fn navigation_event(event: &iced::Event) -> Option<bool> {
    use iced::{
        keyboard::{self, Key, key::Named},
        mouse,
    };
    match event {
        iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Back | mouse::Button::Other(3),
        )) => Some(false),
        iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Forward | mouse::Button::Other(4),
        )) => Some(true),
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.alt() && !modifiers.command() =>
        {
            match key {
                Key::Named(Named::ArrowLeft) => Some(false),
                Key::Named(Named::ArrowRight) => Some(true),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

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
