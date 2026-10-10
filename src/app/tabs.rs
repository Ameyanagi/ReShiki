//! Document tabs: the strip in the header, switching and closing, and the
//! results of work that a tab started before it left the front.
use super::document_tab::{DocumentTab, TabId};
use super::workspace::{caret, control, hover_hint, text_width};
use super::{App, Job, Message, Pending};
use iced::widget::{Space, button, column, container, hover, row, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum Action {
    Select(TabId),
    /// ⌃Tab or ⌃⇧Tab: the next or previous tab, wrapping around.
    Cycle(bool),
    /// ⌘1–⌘8 bring that tab to the front, ⌘9 the last one.
    Number(usize),
    /// Close this tab, or the one in front.
    Close(Option<TabId>),
    /// Open or close the ▾ list of the tabs that do not fit.
    Menu(bool),
}

#[derive(Default)]
pub(super) struct State {
    /// The other tabs. The strip is `background[..active]`, the tab in
    /// front, then `background[active..]`.
    pub(super) background: Vec<DocumentTab>,
    pub(super) active: usize,
    next: u64,
    epochs: u64,
    pub(super) menu: bool,
    /// Where new tabs keep their recovery drafts.
    pub(super) recovery_root: Option<PathBuf>,
    /// Closed tabs whose draft is removed once its last write finishes.
    pub(super) retiring: Vec<super::autosave::Retired>,
    /// Results held while an exit can still fail; replayed if it is cancelled.
    pub(super) deferred_results: Vec<(TabId, Message)>,
}

impl State {
    pub(super) fn new(recovery_root: Option<PathBuf>) -> Self {
        Self {
            recovery_root,
            ..Self::default()
        }
    }
}

const TAB_HEIGHT: f32 = 30.;
const TAB_PADDING: f32 = 8.;
const TAB_MIN: f32 = 88.;
/// The narrowest the tab in front gets when it alone shows.
const TAB_FLOOR: f32 = 64.;
const TAB_MAX: f32 = 200.;
const TAB_GAP: f32 = 2.;
const CLOSE: f32 = 14.;
const DOT: f32 = 7.;
const SPACING: f32 = 6.;
/// The width of the + and ▾ buttons, which share a place.
const BUTTON: f32 = 24.;

/// Two paths name the same file, also through links or different spellings.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .is_some_and(|a| std::fs::canonicalize(b).ok() == Some(a))
}

/// Which tabs show when the strip is `room` wide, and the width limit for
/// tabs of natural widths `widths`. Wide tabs shrink first. Below the minimum
/// width, the tabs around the active one show and the ▾ list, in place of +,
/// holds the rest; New stays in the header. When busy commands crowd the
/// header, the active tab alone shrinks further, and then the list holds all.
fn fit(widths: &[f32], active: usize, room: f32) -> (std::ops::Range<usize>, f32) {
    let gaps = |n: usize| n.saturating_sub(1) as f32 * TAB_GAP;
    let n = widths.len();
    let room = room - BUTTON - SPACING;
    if widths.iter().sum::<f32>() + gaps(n) <= room {
        return (0..n, f32::INFINITY);
    }
    if n as f32 * TAB_MIN + gaps(n) <= room {
        return (0..n, cap(widths, room - gaps(n)));
    }
    if room < TAB_MIN {
        let shown = if room < TAB_FLOOR { 0 } else { 1 };
        return (active..active + shown, room);
    }
    let shown = (((room + TAB_GAP) / (TAB_MIN + TAB_GAP)).floor() as usize).clamp(1, n);
    let start = (active + 1).saturating_sub(shown).min(n - shown);
    let visible = start..start + shown;
    let widths = widths.get(visible.clone()).unwrap_or_default();
    (visible, cap(widths, room - gaps(shown)).max(TAB_MIN))
}

/// The largest width limit under which `widths` add up to at most `room`.
fn cap(widths: &[f32], room: f32) -> f32 {
    let mut sorted = widths.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut used = 0.;
    for (i, width) in sorted.iter().enumerate() {
        let rest = (sorted.len() - i) as f32;
        if used + width * rest > room {
            return (room - used) / rest;
        }
        used += width;
    }
    f32::INFINITY
}

/// `name` shortened with an ellipsis to fit `width` at the tab text size.
fn elide(name: &str, width: f32) -> String {
    let width = width + 0.5;
    if text_width(name, 12.) <= width {
        return name.to_owned();
    }
    let chars: Vec<char> = name.chars().collect();
    let short = |n: usize| chars.iter().take(n).chain(['…'].iter()).collect::<String>();
    // The longest start of the name that fits beside the ellipsis.
    let (mut fits, mut over) = (0, chars.len());
    while over - fits > 1 {
        let middle = (fits + over) / 2;
        if text_width(&short(middle), 12.) <= width {
            fits = middle;
        } else {
            over = middle;
        }
    }
    short(fits)
}

impl App {
    /// Every tab in strip order.
    pub(super) fn strip(&self) -> impl Iterator<Item = &DocumentTab> {
        let (before, after) = self
            .tabs
            .background
            .split_at(self.tabs.active.min(self.tabs.background.len()));
        before.iter().chain([&self.tab]).chain(after)
    }

    /// Whether a tab has unsaved changes; tabs behind the front remember theirs.
    pub(super) fn edited(&self, tab: &DocumentTab) -> bool {
        if tab.id == self.tab.id {
            self.dirty()
        } else {
            tab.edited
        }
    }

    pub(super) fn tab_index(&self, id: TabId) -> Option<usize> {
        self.strip().position(|tab| tab.id == id)
    }

    /// Document epochs are unique across tabs, so an epoch check also rejects
    /// another tab's result.
    pub(super) fn next_epoch(&mut self) -> u64 {
        self.tabs.epochs += 1;
        self.tabs.epochs
    }

    /// A new empty tab with its own draft file, not yet in the strip.
    fn fresh_tab(&mut self) -> DocumentTab {
        let recovery = self
            .tabs
            .recovery_root
            .as_deref()
            .and_then(|root| reshiki::recovery::Recovery::in_directory(root).ok());
        let mut tab = DocumentTab::new(recovery);
        self.tabs.next += 1;
        tab.id = TabId(self.tabs.next);
        tab.file_epoch = self.next_epoch();
        tab
    }

    /// Adds an empty tab at the end of the strip and brings it to the front.
    pub(super) fn add_tab(&mut self) {
        let tab = self.fresh_tab();
        self.tabs.background.push(tab);
        self.select_tab(self.tabs.background.len());
    }

    /// Opening a drawing takes over the front tab if it is an unchanged empty
    /// Untitled drawing, and otherwise adds a tab.
    pub(super) fn target_tab(&mut self) {
        if !self.tab.reusable() {
            self.add_tab();
        }
    }

    /// Starts the front tab over as a new drawing. Late results for its old
    /// drawing meet a new epoch and revision.
    pub(super) fn reset_tab(&mut self) {
        self.retire_assistant(self.tab.id);
        let old = std::mem::replace(&mut self.tab, DocumentTab::new(None));
        self.tab.id = old.id;
        self.tab.recovery = old.recovery;
        self.tab.autosave = old.autosave;
        self.tab.revision = old.revision.wrapping_add(1);
        self.tab.file_epoch = self.next_epoch();
        self.clear_recovery();
    }

    /// Brings the tab at this strip index to the front. Menus belong to the
    /// drawing they were opened on, so they close.
    pub(super) fn select_tab(&mut self, index: usize) {
        let active = self.tabs.active;
        if index == active || index > self.tabs.background.len() {
            return;
        }
        self.leave_tab();
        let next = self
            .tabs
            .background
            .remove(if index < active { index } else { index - 1 });
        let mut previous = self.replace_front(next);
        previous.edited = previous.dirty();
        previous.hover = None;
        self.tabs
            .background
            .insert(if active < index { active } else { active - 1 }, previous);
        self.tabs.active = index;
        self.enter_tab();
    }

    fn leave_tab(&mut self) {
        self.pause_optimization();
        if self.style_menu.is_some() {
            self.close_style_menu();
        }
        self.palette = None;
        self.context_menu = None;
        self.tab.inspector_ui.close_menu();
        self.imports.menu = false;
        self.tabs.menu = false;
        self.theme_library.editor = None;
        self.assistant.menu = None;
    }

    fn enter_tab(&mut self) {
        self.sync_drawing_style_unit();
        if let Some(index) = self.tab.pages.fit {
            self.fit_pages(index);
        } else if self.tab.fit_to_view {
            self.fit();
        }
    }

    /// Takes the front tab out of the strip and brings its right neighbor, or
    /// else its left one, to the front. The last tab gives way to an empty one.
    fn remove_active(&mut self) -> DocumentTab {
        self.leave_tab();
        let active = self.tabs.active;
        let next = if active < self.tabs.background.len() {
            self.tabs.background.remove(active)
        } else if active > 0 {
            self.tabs.active -= 1;
            self.tabs.background.remove(active - 1)
        } else {
            self.fresh_tab()
        };
        let closed = self.replace_front(next);
        self.enter_tab();
        closed
    }

    /// Closes the front tab without asking and removes its draft.
    pub(super) fn close_active_tab(&mut self) -> Task<Message> {
        let closed = self.remove_active();
        self.retire(closed)
    }

    fn replace_front(&mut self, next: DocumentTab) -> DocumentTab {
        let mut previous = std::mem::replace(&mut self.tab, next);
        previous.status = std::mem::replace(&mut self.status, std::mem::take(&mut self.tab.status));
        previous.error = std::mem::replace(&mut self.error, self.tab.error);
        previous
    }

    /// Runs `f` with the tab `id` in front, without the effects of switching.
    pub(super) fn in_tab<R>(&mut self, id: TabId, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        if id == self.tab.id {
            return Some(f(self));
        }
        let index = self.tabs.background.iter().position(|tab| tab.id == id)?;
        let swap = |app: &mut Self| {
            if let Some(previous) = app.tabs.background.get_mut(index) {
                std::mem::swap(&mut app.tab, previous);
                std::mem::swap(&mut app.status, &mut previous.status);
                std::mem::swap(&mut app.status, &mut app.tab.status);
                std::mem::swap(&mut app.error, &mut previous.error);
                std::mem::swap(&mut app.error, &mut app.tab.error);
            }
        };
        swap(self);
        let result = f(self);
        self.tab.edited = self.tab.dirty();
        swap(self);
        Some(result)
    }

    /// Runs `f` for every tab in turn, the front one first.
    pub(super) fn each_tab(&mut self, mut f: impl FnMut(&mut Self)) {
        f(self);
        let ids: Vec<_> = self.tabs.background.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.in_tab(id, &mut f);
        }
    }

    pub(super) fn tab_action(&mut self, action: Action) -> Task<Message> {
        let count = self.tabs.background.len() + 1;
        match action {
            Action::Menu(open) => self.tabs.menu = open,
            // The save dialog is about the tab in front.
            _ if self.pending.is_some() => {}
            Action::Select(id) => {
                if let Some(index) = self.tab_index(id) {
                    self.select_tab(index);
                }
            }
            Action::Cycle(forward) => {
                let active = self.tabs.active;
                self.select_tab(if forward {
                    (active + 1) % count
                } else {
                    (active + count - 1) % count
                });
            }
            Action::Number(n) => {
                if n >= 9 {
                    self.select_tab(count - 1);
                } else if (1..=count).contains(&n) {
                    self.select_tab(n - 1);
                }
            }
            Action::Close(id) => {
                let id = id.unwrap_or(self.tab.id);
                if let Some(index) = self.tabs.background.iter().position(|tab| tab.id == id)
                    && !self
                        .tabs
                        .background
                        .get(index)
                        .is_some_and(|tab| tab.edited)
                {
                    // A saved tab behind the front one closes where it is.
                    let closed = self.tabs.background.remove(index);
                    if index < self.tabs.active {
                        self.tabs.active -= 1;
                    }
                    self.tabs.menu = false;
                    return self.retire(closed);
                }
                let Some(index) = self.tab_index(id) else {
                    return Task::none();
                };
                self.select_tab(index);
                if self.dirty() {
                    return self.ask(Pending::CloseTab(id));
                }
                return self.close_active_tab();
            }
        }
        Task::none()
    }

    /// Closes the window after the save dialog for each unsaved tab in strip
    /// order, skipping the tabs whose changes were already discarded.
    pub(super) fn close_window(
        &mut self,
        window: iced::window::Id,
        discarded: Vec<TabId>,
    ) -> Task<Message> {
        let unsaved = self
            .strip()
            .position(|tab| !discarded.contains(&tab.id) && self.edited(tab));
        let Some(index) = unsaved else {
            return self.close_after_recovery(window);
        };
        self.select_tab(index);
        self.ask(Pending::CloseWindow(window, self.tab.id, discarded))
    }

    /// Async results land in their originating tab with its normal guards and
    /// history. UI controls stay in front; follow-up tasks keep the tab tag.
    /// Closed tabs discard document results, but finish save/draft bookkeeping.
    pub(super) fn background_result(&mut self, id: TabId, message: Message) -> Task<Message> {
        if self.tab_index(id).is_none() {
            return match message {
                Message::Autosaved(key, result) => self.retired(id, key, result),
                Message::Saved(_, _, result) => {
                    self.file_io.saving = false;
                    if let Err(error) = result {
                        self.status = format!("Could not save a closed drawing: {error}");
                        self.error = true;
                    }
                    Task::none()
                }
                Message::EngineDone {
                    kind: Job::Export(format),
                    result,
                    ..
                } => export_result(*result, format),
                Message::FigureExported(_) => {
                    self.figure_exporting = false;
                    Task::none()
                }
                Message::CopyAsPrepared(..) | Message::CopyAsWritten(..) => {
                    self.copy_as_busy = false;
                    Task::none()
                }
                Message::ClipboardWritten { .. } => {
                    self.native_copy_busy = false;
                    Task::none()
                }
                Message::Printing(action) => {
                    self.discard_print_result(action);
                    Task::none()
                }
                message if document_result(&message) => Task::none(),
                message => self.update(message),
            };
        }
        let done = |task: Option<Task<Message>>| task.unwrap_or_else(Task::none);
        match message {
            Message::Autosaved(key, result) => {
                done(self.in_tab(id, |app| app.autosaved(key, result)))
            }
            Message::Saved(epoch, snapshot, result) => {
                // The save that the dialog asked for continues the dialog's
                // action, which acts on the tab in front.
                if let Some(Pending::CloseTab(asked) | Pending::CloseWindow(_, asked, _)) =
                    self.pending
                    && asked == id
                {
                    self.front_pending();
                    return self.file_saved(epoch, snapshot, result);
                }
                // A save dialog open now is about the tab in front.
                let pending = self.pending.take();
                let task = self.in_tab(id, |app| app.file_saved(epoch, snapshot, result));
                self.pending = pending;
                done(task)
            }
            // Freeze edits until exit succeeds, but keep a job's completion
            // available if recovery or a library write cancels the close.
            message if self.exit.frozen() && document_result(&message) => {
                self.defer_document_result(id, message);
                Task::none()
            }
            message if document_result(&message) => done(self.in_tab(id, |app| {
                let (tool, inspector_open, inspector_tab) =
                    (app.tool, app.inspector_open, app.inspector_tab);
                let task = app.update_front(message, true);
                (app.tool, app.inspector_open, app.inspector_tab) =
                    (tool, inspector_open, inspector_tab);
                task
            })),
            message => self.update(message),
        }
    }

    pub(super) fn defer_document_result(&mut self, id: TabId, message: Message) {
        // Polls resume through the timer, so a slow close cannot accumulate
        // an unbounded queue of identical polls.
        if !self.exit.committed()
            && !matches!(message, Message::Assistant(super::assistant::Action::Poll))
        {
            self.tabs.deferred_results.push((id, message));
        }
    }

    /// Makes room in front for an opened file, or brings the tab that already
    /// shows it to the front. Returns whether to go on opening it.
    pub(super) fn open_target(&mut self, path: Option<&Path>) -> bool {
        let open = path.and_then(|path| {
            self.strip()
                .position(|tab| tab.path.as_deref().is_some_and(|p| same_file(p, path)))
        });
        if let Some(index) = open {
            self.select_tab(index);
            self.status = format!("{} is already open", self.document_name());
            return false;
        }
        self.target_tab();
        true
    }

    /// The tabs in the header's file-name area, with + for a new tab and a ▾
    /// list of the tabs that do not fit `width`.
    pub(super) fn tab_strip(&self, width: f32) -> Element<'_, Message> {
        let tabs: Vec<_> = self.strip().collect();
        let widths: Vec<f32> = tabs
            .iter()
            .map(|tab| {
                let dot = if self.edited(tab) { DOT + SPACING } else { 0. };
                (2. * TAB_PADDING + text_width(&tab.name(), 12.) + dot + SPACING + CLOSE)
                    .clamp(TAB_MIN, TAB_MAX)
            })
            .collect();
        let (visible, limit) = fit(&widths, self.tabs.active, width);
        let mut strip = row![].spacing(TAB_GAP).align_y(Alignment::Center);
        for (index, (tab, natural)) in tabs.iter().zip(&widths).enumerate() {
            if visible.contains(&index) {
                strip = strip.push(self.tab_button(tab, natural.min(limit)));
            }
        }
        let new = hover_hint(
            button(text("+").size(17).center())
                .width(BUTTON)
                .height(28)
                .padding(0)
                .style(control(false))
                .on_press(Message::New),
            super::workspace::keyed("New tab", &Message::New),
            tooltip::Position::Bottom,
        );
        let hidden: Vec<_> = tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| !visible.contains(index))
            .map(|(_, tab)| *tab)
            .collect();
        let more = if hidden.is_empty() {
            new.into()
        } else {
            self.hidden_tabs(&hidden)
        };
        row![strip, more]
            .spacing(SPACING)
            .align_y(Alignment::Center)
            .into()
    }

    fn tab_button<'a>(&'a self, tab: &'a DocumentTab, width: f32) -> Element<'a, Message> {
        let active = tab.id == self.tab.id;
        let edited = self.edited(tab);
        let room =
            width - 2. * TAB_PADDING - SPACING - CLOSE - if edited { DOT + SPACING } else { 0. };
        let mut label = row![
            text(elide(&tab.name(), room))
                .size(12)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center);
        if edited {
            label = label.push(dot());
        }
        let close = || {
            button(text("×").size(15).center())
                .width(CLOSE)
                .height(CLOSE)
                .padding(0)
                .style(close_style)
                .on_press(Message::Tabs(Action::Close(Some(tab.id))))
        };
        let slot: Element<'a, Message> = if active {
            close().into()
        } else {
            Space::new().width(CLOSE).into()
        };
        let label = row![container(label).width(Length::Fill), slot]
            .spacing(SPACING)
            .height(Length::Fill)
            .align_y(Alignment::Center);
        let base = button(label)
            .width(width)
            .height(TAB_HEIGHT)
            .padding([0., TAB_PADDING])
            .style(tab_style(active))
            .on_press(Message::Tabs(Action::Select(tab.id)));
        // The × of a tab behind the front one shows while the pointer is on it.
        let body: Element<'a, Message> = if active {
            base.into()
        } else {
            hover(
                base,
                container(close())
                    .padding([0., TAB_PADDING])
                    .align_right(Length::Fill)
                    .center_y(Length::Fill),
            )
        };
        let mut hint = match &tab.path {
            Some(path) => path.display().to_string(),
            None => format!("{} · Not saved to a file", tab.name()),
        };
        if edited {
            hint.push_str(" · Edited");
        }
        if active && self.office_document() {
            hint.push_str(&format!(
                " · {} updates {}",
                super::shortcuts::label(&Message::Save).unwrap_or_default(),
                self.office_host,
            ));
        }
        hover_hint(body, hint, tooltip::Position::Bottom).into()
    }

    /// The ▾ button and list of the tabs that do not fit.
    fn hidden_tabs<'a>(&'a self, hidden: &[&'a DocumentTab]) -> Element<'a, Message> {
        let anchor = button(caret(12.).center())
            .width(BUTTON)
            .height(28)
            .padding(0)
            .style(control(self.tabs.menu))
            .on_press(Message::Tabs(Action::Menu(!self.tabs.menu)));
        let popup = self.tabs.menu.then(|| {
            let mut items = column![].spacing(1);
            for tab in hidden {
                let mut label = row![text(elide(&tab.name(), 170.)).size(12)]
                    .spacing(SPACING)
                    .align_y(Alignment::Center);
                if self.edited(tab) {
                    label = label.push(dot());
                }
                items = items.push(
                    button(label)
                        .width(Length::Fill)
                        .padding([6, 10])
                        .style(control(tab.id == self.tab.id))
                        .on_press(Message::Tabs(Action::Select(tab.id))),
                );
            }
            container(items)
                .width(220)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        let anchor = if self.tabs.menu {
            Element::from(anchor)
        } else {
            hover_hint(
                anchor,
                match hidden.len() {
                    _ if hidden.iter().any(|tab| tab.id == self.tab.id) => "Tabs".to_owned(),
                    1 => "1 more tab".to_owned(),
                    n => format!("{n} more tabs"),
                },
                tooltip::Position::Bottom,
            )
            .into()
        };
        Element::new(super::popover::popover(
            anchor,
            popup,
            Message::Tabs(Action::Menu(false)),
        ))
    }
}

/// Engine and clipboard results that change a drawing, dropped for a closed tab.
pub(super) fn document_result(message: &Message) -> bool {
    use super::{assistant, document_styles, import, pictures, printing};
    matches!(
        message,
        Message::EngineDone { .. }
            | Message::Optimization(super::optimization::Action::WorkerDone(..))
            | Message::Exported(_)
            | Message::FigureExported(_)
            | Message::Printing(printing::Action::Prepared(..) | printing::Action::Finished(..))
            | Message::DrawingStyle(
                document_styles::Action::Loaded(..) | document_styles::Action::Saved(..)
            )
            | Message::Assistant(assistant::Action::Poll | assistant::Action::Done { .. })
            | Message::LabelsReady(..)
            | Message::Mapping(super::reaction_mapping::Action::Ready(..))
            | Message::InspectorAction(super::inspector::Action::PropertiesCalculated(..))
            | Message::Imports(import::Action::Loaded(..))
            | Message::ClipboardWritten { .. }
            | Message::CopyAsPrepared(..)
            | Message::CopyAsWritten(..)
            | Message::ClipboardRead { .. }
            | Message::Pasted(_)
            | Message::Pictures(pictures::Action::Loaded(..))
    )
}

/// An exported structure file is written even after its tab left the front.
fn export_result(
    result: Result<reshiki::engine::Response, String>,
    format: &'static str,
) -> Task<Message> {
    match result {
        Ok(response) => super::export_file(response.output.unwrap_or_default(), format),
        Err(_) => Task::none(),
    }
}

/// The unsaved-changes dot, in the accent color.
fn dot<'a>() -> Element<'a, Message> {
    container(Space::new().width(DOT).height(DOT))
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().primary.into()),
            border: Border {
                radius: (DOT / 2.).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn tab_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let style = button::Style {
            background: Some(
                if active {
                    Color::WHITE
                } else if hovered {
                    Color::from_rgb8(233, 237, 241)
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            text_color: Color::from_rgb8(37, 43, 51),
            border: Border {
                color: if active {
                    Color::from_rgb8(214, 219, 225)
                } else {
                    Color::TRANSPARENT
                },
                width: 1.,
                radius: 7.0.into(),
            },
            ..Default::default()
        };
        let mut style = crate::appearance::button(theme, style);
        if !active {
            style.text_color = crate::appearance::muted(theme);
        }
        style
    }
}

fn close_style(theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let mut style = crate::appearance::button(
        theme,
        button::Style {
            background: hovered.then(|| Color::from_rgb8(222, 227, 232).into()),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    style.text_color = if hovered {
        theme.palette().text
    } else {
        crate::appearance::muted(theme)
    };
    style
}

#[cfg(test)]
pub(super) mod tests;
