//! The command palette: `:` opens a fuzzy search over commands, and Enter
//! runs the selected one. Unlike Help, it executes. Each command runs through
//! the path its button, menu or key already uses, so an edit is one undo step.
use super::{
    App, Message, Pending,
    help::{chord, modal, row_button, search},
    workspace::{control, muted_text},
};
use crate::canvas::Edit;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::text::{Span, Wrapping};
use iced::widget::{Space, column, container, rich_text, row, scrollable, stack, text};
use iced::{Alignment, Element, Font, Length, Task};
use reshiki::{atom_text::Mode, document::Point, keyboard_drawing::Target as Hotspot};
use std::cmp::Reverse;

pub(super) mod catalog;
use catalog::{Category, Command, Need, Ring, Run};

const SEARCH: &str = "command-palette-search";
const RESULTS: &str = "command-palette-results";
const RECENT: usize = 8;
/// Rows that fit the list, for Page Up and Page Down.
const PAGE: isize = 10;
/// A whole word of a command's name scores 60; weaker best matches also
/// offer to insert the query as a chemical name or SMILES.
const STRONG: u32 = 60;

#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Close,
    Query(String),
    /// Move the selection by this many rows, stopping at either end.
    Move(isize),
    /// Show one category's results, or all, as its chip does.
    Filter(Option<Category>),
    /// Step through the filter chips, wrapping, as Tab and Shift+Tab do.
    Cycle(isize),
    /// Run this row of the current items, as a click does.
    Run(usize),
    /// Run the selected row, as Enter does.
    Submit,
}

#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) open: bool,
    pub(super) query: String,
    /// The catalog, then rows made from this query.
    items: Vec<Command>,
    catalog: usize,
    /// Ranked indexes into `items`, in every category.
    hits: Vec<usize>,
    /// The hits in the filtered category.
    results: Vec<usize>,
    /// The category chip; None shows all.
    pub(super) filter: Option<Category>,
    /// The selected position in `results`.
    selected: usize,
    /// Names of recently run commands, newest first.
    recent: Vec<String>,
}

impl State {
    pub(super) fn current(&self) -> Option<&Command> {
        self.results
            .get(self.selected)
            .and_then(|i| self.items.get(*i))
    }
    #[cfg(test)]
    pub(super) fn ranked(&self) -> Vec<&Command> {
        self.results
            .iter()
            .filter_map(|i| self.items.get(*i))
            .collect()
    }
    /// The commands built on opening, without this query's extra rows.
    pub(super) fn catalog(&self) -> &[Command] {
        self.items.get(..self.catalog).unwrap_or_default()
    }
    fn query(&self) -> &str {
        self.query.trim().trim_start_matches(':').trim()
    }
    /// Categories with hits and their counts, in catalog order.
    fn categories(&self) -> Vec<(Category, usize)> {
        Category::ALL
            .into_iter()
            .map(|category| {
                let hits = self.hits.iter().filter(|i| self.in_category(**i, category));
                (category, hits.count())
            })
            .filter(|(_, count)| *count > 0)
            .collect()
    }
    fn in_category(&self, index: usize, category: Category) -> bool {
        self.items
            .get(index)
            .is_some_and(|c| c.category == category)
    }
    /// Show one category's hits, or all of them when it has none.
    fn set_filter(&mut self, filter: Option<Category>) {
        self.filter = filter.filter(|f| self.categories().iter().any(|(c, _)| c == f));
        self.results = (self.hits.iter().copied())
            .filter(|i| self.filter.is_none_or(|f| self.in_category(*i, f)))
            .collect();
        self.selected = 0;
    }
}

/// Keys the palette takes before its search field: closing, filtering and
/// moving.
pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    if matches!(key, Key::Named(Named::Escape)) {
        return Some(Message::Commands(Action::Close));
    }
    if matches!(key, Key::Named(Named::Tab))
        && !(modifiers.control() || modifiers.alt() || modifiers.logo())
    {
        let step = if modifiers.shift() { -1 } else { 1 };
        return Some(Message::Commands(Action::Cycle(step)));
    }
    if !modifiers.is_empty() {
        return None;
    }
    let step = match key {
        Key::Named(Named::ArrowUp) => -1,
        Key::Named(Named::ArrowDown) => 1,
        Key::Named(Named::PageUp) => -PAGE,
        Key::Named(Named::PageDown) => PAGE,
        Key::Named(Named::Home) => isize::MIN,
        Key::Named(Named::End) => isize::MAX,
        _ => return None,
    };
    Some(Message::Commands(Action::Move(step)))
}

/// What a command acts on: the selection, else the keyboard hotspot or the
/// atom or bond under the pointer.
#[derive(Debug, Default)]
struct Target {
    atoms: Vec<u64>,
    bonds: Vec<(u64, u64)>,
    pointed: bool,
}

impl Target {
    fn atom(&self) -> Option<u64> {
        match (self.atoms.as_slice(), self.bonds.is_empty()) {
            ([id], true) => Some(*id),
            _ => None,
        }
    }
    fn bond(&self) -> Option<(u64, u64)> {
        match (self.bonds.as_slice(), self.atoms.len()) {
            ([bond], 2) => Some(*bond),
            _ => None,
        }
    }
    fn ids(&self) -> Vec<u64> {
        self.atoms.clone()
    }
}

impl App {
    fn keyboard_target(&self) -> bool {
        self.tool.selects() && self.tab.keyboard_drawing.active()
    }

    fn command_target(&self) -> Target {
        let doc = &self.tab.doc;
        let selected = &self.tab.selected;
        let target = Target {
            atoms: selected
                .iter()
                .copied()
                .filter(|id| doc.atom(*id).is_some())
                .collect(),
            bonds: doc
                .bonds
                .iter()
                .filter(|b| selected.contains(&b.a) && selected.contains(&b.b))
                .map(|b| (b.a, b.b))
                .collect(),
            pointed: false,
        };
        if !selected.is_empty() {
            return target;
        }
        let pointed = if self.keyboard_target() {
            match self.tab.keyboard_drawing.target() {
                Hotspot::Atom(id) => (Some(id), None),
                Hotspot::Bond(a, b) => (None, Some((a, b))),
                Hotspot::Blank(_) => (None, None),
            }
        } else {
            self.hovered_target()
        };
        match pointed {
            (Some(id), _) => Target {
                atoms: vec![id],
                pointed: true,
                ..Target::default()
            },
            (_, Some((a, b))) => Target {
                atoms: vec![a, b],
                bonds: vec![(a, b)],
                pointed: true,
            },
            _ => Target::default(),
        }
    }

    /// Why a command cannot run now; the palette shows it greyed with this.
    pub(super) fn command_unavailable(&self, command: &Command) -> Option<&'static str> {
        self.need_unavailable(command.need, &self.command_target())
    }

    fn need_unavailable(&self, need: Need, target: &Target) -> Option<&'static str> {
        match need {
            Need::Nothing => None,
            Need::Selection => self
                .tab
                .selected
                .is_empty()
                .then_some("Select something first"),
            Need::Resizable => self.need_unavailable(Need::Selection, target).or_else(|| {
                let ids = self
                    .tab
                    .doc
                    .expand_abbreviation_selection(&self.tab.selected);
                (ids.iter()
                    .any(|id| reshiki::models3d::hides(&self.tab.doc, *id)))
                .then_some(super::numeric_transforms::RESIZE_SHOWN_IN_3D)
            }),
            Need::Molecule => self
                .tab
                .doc
                .atoms
                .is_empty()
                .then_some("Draw a molecule first"),
            Need::Bond => target
                .bonds
                .is_empty()
                .then_some("Select or point at a bond first"),
            Need::Atom => target
                .atoms
                .is_empty()
                .then_some("Select or point at an atom first"),
            Need::OneAtom => target
                .atom()
                .is_none()
                .then_some("Select or point at one atom first"),
            Need::NmrReport => (!self.nmr_has_report()).then_some("Predict NMR first"),
        }
    }

    pub(super) fn commands_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open => {
                self.help_open = false;
                self.palette = None;
                self.context_menu = None;
                let items = catalog::build(self);
                let state = &mut self.commands;
                state.open = true;
                state.query.clear();
                state.filter = None;
                state.catalog = items.len();
                state.items = items;
                self.rank_commands();
                return iced::widget::operation::focus(SEARCH);
            }
            Action::Close => self.commands.open = false,
            _ if !self.commands.open => {}
            Action::Query(query) => {
                self.commands.query = query;
                self.rank_commands();
                return scroll_to_top();
            }
            Action::Filter(filter) => {
                self.commands.set_filter(filter);
                // A clicked chip takes focus; typing stays in the search.
                return Task::batch([scroll_to_top(), iced::widget::operation::focus(SEARCH)]);
            }
            Action::Cycle(step) => {
                let state = &mut self.commands;
                let mut chips = vec![None];
                chips.extend(state.categories().into_iter().map(|(c, _)| Some(c)));
                let at = chips.iter().position(|c| *c == state.filter).unwrap_or(0) as isize;
                let next = (at + step).rem_euclid(chips.len() as isize) as usize;
                state.set_filter(chips.get(next).copied().flatten());
                return scroll_to_top();
            }
            Action::Move(step) => {
                let state = &mut self.commands;
                let last = state.results.len().saturating_sub(1);
                state.selected = state.selected.saturating_add_signed(step).min(last);
                return iced::widget::operation::snap_to(
                    RESULTS,
                    iced::widget::operation::RelativeOffset {
                        x: 0.,
                        y: state.selected as f32 / last.max(1) as f32,
                    },
                );
            }
            Action::Run(index) => {
                let state = &mut self.commands;
                if let Some(position) = state.results.iter().position(|i| *i == index) {
                    // The clicked row shows why it cannot run.
                    state.selected = position;
                }
                if let Some(command) = self.commands.items.get(index).cloned() {
                    let task = self.run_command(command);
                    // A clicked row takes focus; while the palette stays open
                    // because the command cannot run, typing stays in the search.
                    return if self.commands.open {
                        Task::batch([task, iced::widget::operation::focus(SEARCH)])
                    } else {
                        task
                    };
                }
            }
            Action::Submit => {
                if let Some(command) = self.commands.current().cloned() {
                    return self.run_command(command);
                }
            }
        }
        Task::none()
    }

    /// Rank the catalog for the query, favoring commands for the target.
    fn rank_commands(&mut self) {
        let target = self.command_target();
        let boost = |category: Category| match category {
            Category::Bond if !target.bonds.is_empty() => 30,
            Category::Atom if target.atom().is_some() => 30,
            Category::Insert if target.atom().is_some() => 20,
            Category::Insert | Category::Tool if target.atoms.is_empty() => 10,
            _ => 0,
        };
        let query = self.commands.query().to_owned();
        let query = query.as_str();
        let extra = catalog::parameterised(self, query);
        // Commands that cannot run now go below those that can.
        let blocked: Vec<bool> = (self.commands.catalog().iter())
            .map(|c| self.need_unavailable(c.need, &target).is_some())
            .collect();
        let state = &mut self.commands;
        state.items.truncate(state.catalog);
        let catalog = &state.items;
        // (score, index, tier): an exact alias or recent command, then name or
        // key matches, then the rest.
        let mut scored: Vec<(u32, usize, u8)> = if query.is_empty() {
            catalog
                .iter()
                .enumerate()
                .map(|(i, command)| {
                    let recent = state.recent.iter().position(|name| *name == command.name);
                    match recent {
                        Some(age) => (1000 - age as u32, i, 2),
                        None => (boost(command.category), i, 0),
                    }
                })
                .collect()
        } else {
            // `:w file` saves as and `:e file` opens; paths come from the dialog.
            let word = query.split_whitespace().next().unwrap_or_default();
            let argument = word.len() < query.len();
            catalog
                .iter()
                .enumerate()
                .filter_map(|(i, command)| {
                    let alias = command.aliases.contains(&query)
                        || argument
                            && match word {
                                "w" | "sav" | "saveas" => command.aliases.contains(&"sav"),
                                "e" | "edit" => command.aliases.contains(&"e"),
                                _ => false,
                            };
                    if alias {
                        return Some((10_000, i, 2));
                    }
                    let score = search::score(command, query)?;
                    let tier = search::in_name_or_keys(command, query) as u8;
                    Some((score + boost(command.category), i, tier))
                })
                .collect()
        };
        // Catalog order puts direct actions before templates and examples, so
        // a repeated built-in title keeps the direct action. Saved templates
        // always stay: they may hold a different structure under the same
        // name, and their rows name the collection.
        let mut names = std::collections::HashSet::new();
        scored.retain(|(_, i, _)| {
            catalog.get(*i).is_some_and(|c| {
                matches!(&c.run, Run::Template(_, id) if !id.starts_with("builtin:"))
                    || names.insert(c.name.clone())
            })
        });
        let blocked = |i: usize| blocked.get(i).copied().unwrap_or(false);
        // An exact alias or recent command ranks first even when it cannot
        // run, to say why.
        let late = |i: usize, tier: u8| tier < 2 && blocked(i);
        scored.sort_by_key(|&(score, i, tier)| (late(i, tier), Reverse(tier), Reverse(score), i));
        let best = scored.iter().map(|(score, ..)| *score).max().unwrap_or(0);
        // Where the rows that cannot run begin.
        let mut split = scored
            .iter()
            .filter(|(_, i, tier)| !late(*i, *tier))
            .count();
        state.hits = scored.into_iter().map(|(_, i, _)| i).collect();
        // A command word with a value is what was meant, not a name.
        let named = extra.is_none();
        let mut rows = vec![];
        if let Some(command) = extra {
            rows.push((true, command));
        }
        if !query.is_empty() && best < STRONG && named {
            if reshiki::chemistry::smiles::parse(query).is_ok() {
                rows.push((
                    false,
                    Command::new(
                        format!("Insert SMILES {query}"),
                        Category::Insert,
                        "Insert the structure this SMILES describes",
                        "",
                        Run::Smiles(query.into()),
                    ),
                ));
            }
            // The browser build has no name parser yet.
            if !cfg!(target_arch = "wasm32") {
                rows.push((
                    false,
                    Command::new(
                        format!("Insert structure named “{query}”"),
                        Category::Insert,
                        "Convert the chemical name to a structure and insert it",
                        "",
                        Run::Name(query.into()),
                    ),
                ));
            }
        }
        // Query rows rank among the others by whether they can run too.
        let rows: Vec<_> = (rows.into_iter())
            .map(|(first, c)| (first, self.need_unavailable(c.need, &target).is_some(), c))
            .collect();
        let state = &mut self.commands;
        for (first, blocked, command) in rows {
            if !names.insert(command.name.clone()) {
                continue;
            }
            let index = state.items.len();
            state.items.push(command);
            let at = match (first, blocked) {
                (true, false) => 0,
                (false, true) => state.hits.len(),
                _ => split,
            };
            split += usize::from(!blocked);
            state.hits.insert(at, index);
        }
        let filter = state.filter;
        state.set_filter(filter);
    }

    fn run_command(&mut self, command: Command) -> Task<Message> {
        if let Some(reason) = self.command_unavailable(&command) {
            self.status = reason.into();
            self.error = true;
            return Task::none();
        }
        let state = &mut self.commands;
        state.open = false;
        if state.catalog().iter().any(|c| c.name == command.name) {
            state.recent.retain(|name| *name != command.name);
            state.recent.insert(0, command.name.clone());
            state.recent.truncate(RECENT);
        }
        // A pending Move & attach would take over template placement.
        if !matches!(command.run, Run::Send(_)) && self.tab.joining.is_some() {
            self.cancel_join();
        }
        let target = self.command_target();
        let explicit = self.keyboard_target();
        match command.run {
            Run::Send(messages) => {
                if command.need == Need::Molecule {
                    self.select_molecule(&target);
                }
                let mut tasks = vec![];
                for message in messages {
                    tasks.push(self.update(message));
                }
                Task::batch(tasks)
            }
            Run::Bond(preset) => {
                if target.bonds.is_empty() {
                    return self.update(Message::Tool(super::palettes::bond_tool(preset)));
                }
                self.select_target(&target);
                self.update(Message::ApplyBondPreset(preset))
            }
            Run::OnBonds(message) | Run::OnAtoms(message) => {
                self.select_target(&target);
                self.update(message)
            }
            Run::AtomKey(key) => match target.atom() {
                Some(id) => self.context_key_resolved(key, Some(id), None, explicit, false),
                None => Task::none(),
            },
            Run::Element(symbol) => match target.atom() {
                Some(id) => {
                    self.label_atom(id, symbol, Mode::Auto);
                    Task::none()
                }
                None => self.update(Message::Element(symbol.into())),
            },
            Run::Group(label) => {
                match target.atom() {
                    Some(id) => self.label_atom(id, &label, Mode::Group),
                    None => {
                        // A lone group at the center: one carbon, labeled as the
                        // group hotkey labels an atom, committed as one step.
                        let mut doc = self.tab.doc.clone();
                        let id = doc.add_atom("C", self.tab.camera.center);
                        let result = reshiki::hotkeys::label_edit(&doc, id, &label, Mode::Group)
                            .map(|(doc, id)| (doc, vec![id]));
                        if self.commit_hotkey(result, &format!("{label} added")) {
                            self.tab.labels_dirty = true;
                        }
                    }
                }
                Task::none()
            }
            Run::Ring(ring) => self.insert_ring(ring, &target, explicit),
            Run::Template(index, id) => {
                // The library may have changed since the palette opened.
                let Some(index) = self.template_index(index, &id) else {
                    self.status = "That template is no longer in the library".into();
                    self.error = true;
                    return Task::none();
                };
                let anchor = self.insert_point(&target);
                self.insert_template(index);
                self.edit(Edit::Template(anchor, None));
                Task::none()
            }
            Run::SetCharge(charge) => {
                let Some(atom) = target.atom().and_then(|id| self.tab.doc.atom(id)) else {
                    return Task::none();
                };
                let delta = charge - atom.charge;
                self.tab.selected = vec![atom.id];
                self.update(Message::Charge(delta))
            }
            Run::Name(name) => {
                self.imports.kind = super::import::InputKind::Name;
                let _ = self.update(Message::Naming(super::naming::Action::Name(name)));
                self.update(Message::Naming(super::naming::Action::InsertName))
            }
            Run::Smiles(smiles) => {
                self.imports.kind = super::import::InputKind::Structure;
                self.imports.set_text(&smiles);
                self.update(Message::InsertInput)
            }
            Run::SaveAndClose => {
                if self.dirty() {
                    // The save dialog's Save answer: close once the save succeeds.
                    self.pending = Some(Pending::CloseTab(self.tab.id));
                    return self.update(Message::Save);
                }
                self.update(Message::Tabs(super::tabs::Action::Close(None)))
            }
            Run::DiscardAndClose => {
                if self.file_io.saving {
                    // Closing keeps its own barrier for a save in progress.
                    return self.update(Message::Tabs(super::tabs::Action::Close(None)));
                }
                if self.dirty() {
                    // The save dialog's Don't Save answer.
                    self.pending = Some(Pending::CloseTab(self.tab.id));
                    return self.update(Message::Discard);
                }
                self.update(Message::Tabs(super::tabs::Action::Close(None)))
            }
            Run::CloseWindow => iced::window::latest().then(|window| match window {
                Some(id) => Task::done(Message::Close(id)),
                None => Task::none(),
            }),
        }
    }

    /// The template with this identity, preferably still at `index`.
    fn template_index(&self, index: usize, id: &str) -> Option<usize> {
        let library = &self.templates.library;
        if library.get(index).is_some_and(|t| t.id == id) {
            return Some(index);
        }
        library.iter().position(|t| t.id == id && !id.is_empty())
    }

    /// Analysis acts on the selection, or on the whole pointed molecule.
    fn select_molecule(&mut self, target: &Target) {
        if !target.pointed {
            return;
        }
        let atoms: Vec<_> = self.tab.doc.atoms.iter().map(|a| a.id).collect();
        if let Some(molecule) = reshiki::editing::groups(&self.tab.doc, &atoms)
            .into_iter()
            .find(|group| target.atoms.iter().any(|id| group.contains(id)))
        {
            self.tab.selected = molecule;
        }
    }

    /// A pointed target becomes the selection, as contextual keys select it.
    fn select_target(&mut self, target: &Target) {
        if target.pointed {
            self.tab.selected = target.ids();
        }
    }

    fn label_atom(&mut self, id: u64, label: &str, mode: Mode) {
        let result = reshiki::hotkeys::label_edit(&self.tab.doc, id, label, mode)
            .map(|(doc, id)| (doc, vec![id]));
        if self.commit_hotkey(result, &format!("{label} applied")) {
            self.tab.labels_dirty = true;
        }
    }

    /// Where an insertion goes: the target atom, the target bond's middle, or
    /// the center of the view.
    fn insert_point(&self, target: &Target) -> Point {
        let position = |id| self.tab.doc.atom(id).map(|a| a.position);
        if let Some(point) = target.atom().and_then(position) {
            return point;
        }
        if let Some((a, b)) = target.bond()
            && let (Some(a), Some(b)) = (position(a), position(b))
        {
            return Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
        }
        self.tab.camera.center
    }

    /// Attach with the ring hotkey where there is one, as pressing it over
    /// the target does; otherwise place as the ring tool does at that point.
    fn insert_ring(&mut self, ring: Ring, target: &Target, explicit: bool) -> Task<Message> {
        let (atom_key, bond_key) = ring.keys();
        if let (Some(id), Some(key)) = (target.atom(), atom_key) {
            return self.context_key_resolved(key, Some(id), None, explicit, false);
        }
        if let (Some(bond), Some(key)) = (target.bond(), bond_key) {
            return self.context_key_resolved(key, None, Some(bond), explicit, false);
        }
        let anchor = self.insert_point(target);
        let before = self.tab.doc.clone();
        match ring {
            Ring::Size(size, aromatic) => self.place_ring(anchor, None, size, aromatic, before),
            Ring::Preset(preset) => {
                self.place_ring_preset(preset, anchor, None, false, false, before)
            }
        }
        Task::none()
    }

    pub(super) fn with_commands<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.commands.open {
            // Keep the workspace at the same child position, as Help does.
            return stack![base].into();
        }
        let state = &self.commands;
        let current = state.current();
        let name = match current {
            Some(command) => format!(
                "Run a command; {} results; selected {}, {}",
                state.results.len(),
                command.name,
                command.category.label()
            ),
            None => "Run a command; no results".into(),
        };
        let search = reshiki::accessibility::text_input(
            SEARCH,
            name,
            "Search commands (e.g. benzene, double bond, nmr, :w)",
            &state.query,
        )
        .style(crate::appearance::input_style)
        .on_input(|query| Message::Commands(Action::Query(query)))
        .on_submit(Message::Commands(Action::Submit))
        .size(14)
        .padding(8);
        let query = state.query();
        let mut list = column![].spacing(2);
        for (position, &index) in state.results.iter().enumerate() {
            let Some(command) = state.items.get(index) else {
                continue;
            };
            // A stable identity, so a queued assistive press never lands on
            // a different command after the results change: the catalog
            // index, or a hash of the title of a row the query created
            // (bounded, since native ids are limited to 256 bytes).
            let id = if index < state.catalog {
                format!("command-palette-row-{index}")
            } else {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                command.name.hash(&mut hasher);
                format!("command-palette-row-query-{:016x}", hasher.finish())
            };
            list = list.push(result_row(
                id,
                command,
                query,
                position == state.selected,
                self.command_unavailable(command).is_none(),
                Message::Commands(Action::Run(index)),
            ));
        }
        if state.results.is_empty() {
            list = list.push(
                text("No command matches. Try benzene, wedge, nmr or save.")
                    .size(12)
                    .style(muted_text),
            );
        }
        let mut chips = row![].spacing(6);
        let categories = state.categories().into_iter();
        let all = (None, "All", state.hits.len());
        for (filter, label, count) in std::iter::once(all)
            .chain(categories.map(|(category, count)| (Some(category), category.label(), count)))
        {
            let active = state.filter == filter;
            chips = chips.push(
                reshiki::accessibility::button(
                    format!("command-palette-filter-{}", label.to_lowercase()),
                    format!(
                        "{label}, {count} result{}",
                        if count == 1 { "" } else { "s" }
                    ),
                    text(format!("{label} {count}")).size(12),
                )
                .padding([2, 8])
                .style(control(active))
                .checked(active)
                .on_press(Message::Commands(Action::Filter(filter)))
                .tab_stop(false),
            );
        }
        // The selected command's description, or why it cannot run.
        let detail = current
            .map(|c| self.command_unavailable(c).unwrap_or(&c.detail).to_owned())
            .unwrap_or_default();
        let content = column![
            row![
                text("Commands").size(22),
                Space::new().width(Length::Fill),
                reshiki::accessibility::button(
                    "command-palette-close",
                    "Close the command palette",
                    text("×").size(25).center()
                )
                .width(32)
                .height(32)
                .padding(0)
                .on_press(Message::Commands(Action::Close))
                .style(control(false)),
            ]
            .align_y(Alignment::Center),
            search,
            chips,
            scrollable(list.padding([0, 12]))
                .id(RESULTS)
                .width(Length::Fill)
                .height(Length::Fill),
            container(text(detail).size(12).wrapping(Wrapping::None))
                .width(Length::Fill)
                .clip(true),
            text("↑ ↓ move · Tab filters · ↩ runs · Esc closes · Acts on the selection, else the pointed atom or bond")
                .size(12)
                .style(muted_text),
        ]
        .spacing(14);
        modal(
            base,
            content.into(),
            760.,
            560.,
            Message::Commands(Action::Close),
        )
    }
}

fn scroll_to_top() -> Task<Message> {
    iced::widget::operation::snap_to(RESULTS, iced::widget::operation::RelativeOffset::START)
}

/// One compact row: the name with the query's matches in bold, its category,
/// and its key hint at the right. A row that cannot run now is muted.
fn result_row(
    id: String,
    command: &Command,
    query: &str,
    selected: bool,
    enabled: bool,
    on_press: Message,
) -> Element<'static, Message> {
    let name = rich_text(name_spans(&command.name, query))
        .size(13)
        .wrapping(Wrapping::None);
    let keys = command.hint();
    let body = container(
        row![
            container(
                row![
                    if enabled {
                        name
                    } else {
                        name.style(muted_text)
                    },
                    text(command.category.label())
                        .size(11)
                        .style(muted_text)
                        .wrapping(Wrapping::None),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .clip(true),
            if keys.is_empty() {
                Space::new().into()
            } else {
                chord(&keys, 11)
            },
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([0, 8])
    .height(28)
    .align_y(Alignment::Center);
    row_button(id, command.name.clone(), body, selected, on_press)
}

/// `name` split into runs, the characters the query matches in bold.
fn name_spans(name: &str, query: &str) -> Vec<Span<'static>> {
    let bold = Font {
        weight: iced::font::Weight::Bold,
        ..Font::with_name(reshiki::style::ui_font_family())
    };
    let mut spans = vec![];
    let mut run = String::new();
    let mut emphasised = false;
    for (c, matched) in name.chars().zip(matched(name, query)) {
        if matched != emphasised && !run.is_empty() {
            // cosmic-text shapes an ASCII word in the font of its first
            // letter; a word joiner sends the word through the per-letter path.
            if !c.is_whitespace() && !run.ends_with(char::is_whitespace) {
                run.push('\u{2060}');
            }
            let text = std::mem::take(&mut run);
            spans.push(iced::widget::span(text).font_maybe(emphasised.then_some(bold)));
        }
        emphasised = matched;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(iced::widget::span(run).font_maybe(emphasised.then_some(bold)));
    }
    spans
}

/// Which characters of `name` each query word matches, ignoring case: at a
/// word start, else anywhere, else its letters in order.
fn matched(name: &str, query: &str) -> Vec<bool> {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let name: Vec<char> = name.chars().map(fold).collect();
    let mut marks = vec![false; name.len()];
    for word in query.split_whitespace() {
        let word: Vec<char> = word.chars().map(fold).collect();
        let at = |i: usize| name.get(i..).is_some_and(|rest| rest.starts_with(&word));
        let start = |i: usize| i == 0 || name.get(i - 1).is_some_and(|c| !c.is_alphanumeric());
        let found = (0..name.len())
            .find(|&i| start(i) && at(i))
            .or_else(|| (0..name.len()).find(|&i| word.len() > 1 && at(i)));
        let picks: Vec<usize> = match found {
            Some(i) => (i..i + word.len()).collect(),
            // Letters in order within one word, as search matches them.
            None if word.len() > 2 => (0..name.len())
                .filter(|&i| start(i))
                .find_map(|first| {
                    let mut letters = word.iter().peekable();
                    let picks: Vec<usize> = (name.iter().enumerate().skip(first))
                        .take_while(|(_, c)| c.is_alphanumeric())
                        .filter(|(_, c)| letters.next_if_eq(c).is_some())
                        .map(|(i, _)| i)
                        .collect();
                    (picks.len() == word.len()).then_some(picks)
                })
                .unwrap_or_default(),
            None => vec![],
        };
        for i in picks {
            if let Some(mark) = marks.get_mut(i) {
                *mark = true;
            }
        }
    }
    marks
}

#[cfg(test)]
mod tests;
