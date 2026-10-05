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
pub(super) mod tests {
    use super::*;
    use crate::app::inspector;
    use reshiki::document::{Document, Point};
    use reshiki::engine::Response;

    fn ready() -> App {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app
    }

    fn edit(app: &mut App, element: &str) -> u64 {
        let before = app.tab.doc.clone();
        let id = app.tab.doc.add_atom(element, Point::default());
        app.changed(before);
        id
    }

    fn imported(element: &str) -> Box<Result<Response, String>> {
        let mut document = Document::default();
        document.add_atom(element, Point::default());
        Box::new(Ok(Response {
            document: Some(document),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        }))
    }

    fn select(app: &mut App, id: TabId) {
        let _ = app.update(Message::Tabs(Action::Select(id)));
    }

    pub(in crate::app) struct Front {
        id: TabId,
        index: usize,
        document: Document,
        revision: u64,
        selected: Vec<u64>,
    }

    impl Front {
        pub(in crate::app) fn new(app: &mut App) -> Self {
            app.add_tab();
            let atom = edit(app, "S");
            app.tab.selected = vec![atom];
            app.tab.camera.center = Point::new(90., -30.);
            app.tab.camera.zoom = 1.7;
            app.tool = crate::canvas::Tool::Erase;
            app.inspector_open = false;
            app.inspector_tab = super::super::InspectorTab::Import;
            app.tab.inspector_ui.update(inspector::Action::Section(
                inspector::Section::Transform,
                true,
            ));
            app.context_menu = Some(super::super::context_menu::State::new(
                iced::Point::new(12., 34.),
                Default::default(),
            ));
            app.style_menu = Some(super::super::color_popover::Menu::Align);
            app.imports.menu = true;
            app.tabs.menu = true;
            app.help_open = true;
            app.view_open = true;
            app.status = "Front tab status".into();
            app.error = true;
            Self {
                id: app.tab.id,
                index: app.tabs.active,
                document: app.tab.doc.clone(),
                revision: app.tab.revision,
                selected: app.tab.selected.clone(),
            }
        }

        pub(in crate::app) fn assert_unchanged(&self, app: &App) {
            assert_eq!(app.tab.id, self.id);
            assert_eq!(app.tabs.active, self.index);
            assert_eq!(app.tab.doc, self.document);
            assert_eq!(app.tab.revision, self.revision);
            assert_eq!(app.tab.selected, self.selected);
            assert_eq!(app.tab.camera.center, Point::new(90., -30.));
            assert_eq!(app.tab.camera.zoom, 1.7);
            assert_eq!(app.tool, crate::canvas::Tool::Erase);
            assert!(!app.inspector_open);
            assert_eq!(app.inspector_tab, super::super::InspectorTab::Import);
            assert_eq!(
                app.tab.inspector_ui.expanded(inspector::Section::Transform),
                Some(true)
            );
            assert_eq!(
                app.context_menu.as_ref().unwrap().position,
                iced::Point::new(12., 34.)
            );
            assert!(matches!(
                app.style_menu,
                Some(super::super::color_popover::Menu::Align)
            ));
            assert!(app.imports.menu && app.tabs.menu && app.help_open && app.view_open);
            assert_eq!(app.status, "Front tab status");
            assert!(app.error);
            assert!(!app.tab.busy && !app.tab.clipboard_busy);
            assert!(
                app.tab.labels_dirty,
                "Follow-up labels belong to the background tab"
            );
        }
    }

    #[test]
    fn tabs_shrink_to_a_minimum_then_the_rest_move_into_the_list() {
        let widths = [150., 100., 120.];
        let wide = 2000.;
        assert_eq!(fit(&widths, 0, wide), (0..3, f32::INFINITY));
        // Wide tabs shrink first, down to the others' width.
        let room = 100. + 100. + 100. + 2. * TAB_GAP + BUTTON + SPACING;
        let (visible, limit) = fit(&widths, 0, room);
        assert_eq!(visible, 0..3);
        assert!((limit - 100.).abs() < 0.01, "{limit}");
        // Below the minimum width, the tabs around the active one show.
        let room = 2. * TAB_MIN + TAB_GAP + BUTTON + SPACING;
        assert_eq!(fit(&widths, 0, room).0, 0..2);
        assert_eq!(fit(&widths, 2, room).0, 1..3);
        // Crowded by busy commands, the active tab alone shrinks, then the
        // list holds every tab.
        let room = TAB_FLOOR + BUTTON + SPACING;
        assert_eq!(fit(&widths, 2, room), (2..3, TAB_FLOOR));
        assert!(fit(&widths, 2, room - 1.).0.is_empty());
        assert_eq!(fit(&widths[..1], 0, room), (0..1, TAB_FLOOR));
    }

    #[test]
    fn long_names_end_with_an_ellipsis() {
        assert_eq!(elide("a.rsk", 200.), "a.rsk");
        let short = elide("a very long drawing name.rsk", 60.);
        assert!(
            short.ends_with('…') && text_width(&short, 12.) <= 60.5,
            "{short}"
        );
    }

    #[test]
    fn new_takes_an_unchanged_empty_tab_and_otherwise_adds_one() {
        let mut app = ready();
        let first = app.tab.id;
        app.tab.caption_format.style.bold = true;
        let _ = app.update(Message::New);
        assert_eq!(app.tab.id, first, "An empty Untitled tab is reused");
        assert!(app.tabs.background.is_empty());
        assert!(!app.tab.caption_format.style.bold, "New starts over");
        edit(&mut app, "N");
        let _ = app.update(Message::New);
        assert_ne!(app.tab.id, first);
        assert_eq!(app.strip().count(), 2);
        assert_eq!(app.tabs.active, 1, "A new tab opens at the end, in front");
        assert!(app.tab.doc.all_ids().is_empty() && !app.dirty());
        assert!(app.tabs.background[0].edited);
        assert_ne!(
            app.tabs.background[0].file_epoch, app.tab.file_epoch,
            "Epochs are unique across tabs"
        );
    }

    #[test]
    fn switching_keeps_each_tabs_view_selection_history_and_sections() {
        let mut app = ready();
        let atom = edit(&mut app, "N");
        let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![atom])));
        app.tab.camera.zoom = 2.;
        app.tab.camera.center = Point::new(40., -10.);
        let _ = app.update(Message::InspectorAction(inspector::Action::Section(
            inspector::Section::Transform,
            true,
        )));
        let first = app.tab.id;
        let _ = app.update(Message::New);
        edit(&mut app, "O");
        let second = app.tab.id;
        app.tool = crate::canvas::Tool::Erase;
        let _ = app.update(Message::Tabs(Action::Cycle(true)));
        assert_eq!(app.tab.id, first, "⌃Tab wraps around");
        assert_eq!(app.tab.selected, vec![atom]);
        assert_eq!(app.tab.camera.zoom, 2.);
        assert_eq!(app.tab.camera.center, Point::new(40., -10.));
        assert_eq!(
            app.tab.inspector_ui.expanded(inspector::Section::Transform),
            Some(true)
        );
        assert_eq!(app.tool, crate::canvas::Tool::Erase, "The tool is app-wide");
        let _ = app.update(Message::Undo);
        assert!(app.tab.doc.atoms.is_empty(), "Undo edits the tab in front");
        let _ = app.update(Message::Tabs(Action::Number(9)));
        assert_eq!(app.tab.id, second, "⌘9 is the last tab");
        assert_eq!(app.tab.doc.atoms[0].element, "O");
        assert!(app.tab.history.can_undo());
        let _ = app.update(Message::Tabs(Action::Number(1)));
        assert_eq!(app.tab.id, first);
        let _ = app.update(Message::Tabs(Action::Number(5)));
        assert_eq!(app.tab.id, first, "No fifth tab");
    }

    async fn keyboard_labels_fixture() -> (App, u64, u64, Message) {
        use iced::futures::StreamExt;
        use reshiki::keyboard_drawing::Target;

        let mut app = ready();
        let before = app.tab.doc.clone();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.changed(before);
        app.tab.selected = vec![b];
        app.sync_keyboard_drawing();
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(b), &app.tab.doc);
        app.tab.keyboard_drawing.mark(a);
        let mut labels = iced_runtime::task::into_stream(app.start_label_refresh())
            .expect("A real label calculation starts");
        let Some(iced_runtime::Action::Output(message)) = labels.next().await else {
            panic!("The label task must return its tagged completion");
        };
        assert!(matches!(&message, Message::LabelsReady(_, Ok(_))));
        (app, a, b, message)
    }

    #[tokio::test]
    async fn background_labels_preserve_keyboard_target_and_mark_while_front_tool_is_text() {
        use crate::canvas::Tool;
        use reshiki::keyboard_drawing::Target;

        let (mut app, a, b, labels) = keyboard_labels_fixture().await;
        let first = app.tab.id;
        let drawing = app.tab.doc.clone();
        let revision = app.tab.revision;
        let _ = app.update(Message::New);
        let second = app.tab.id;
        let _ = app.update(Message::Tool(Tool::Text));
        let _ = app.update(Message::Tab(first, Box::new(labels)));
        assert_eq!(app.tab.id, second);
        assert_eq!(app.tool, Tool::Text);
        let background = &app.tabs.background[0];
        assert_eq!(background.keyboard_drawing.target(), Target::Atom(b));
        assert_eq!(background.keyboard_drawing.marked(), Some(a));
        assert!(background.keyboard_drawing.enabled());
        assert_eq!(background.revision, revision);
        assert!(super::super::same_drawing(&background.doc, &drawing));
        assert_eq!(background.doc.atom(b).unwrap().label_h, 3);

        let _ = app.update(Message::Tool(Tool::Select));
        select(&mut app, first);
        assert!(app.keyboard_drawing_active());
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
        assert_eq!(app.tab.keyboard_drawing.marked(), Some(a));
    }

    #[tokio::test]
    async fn background_labels_reconcile_deleted_targets_and_new_document_epochs() {
        use crate::canvas::Tool;
        use reshiki::keyboard_drawing::Target;

        for new_epoch in [false, true] {
            let (mut app, a, b, labels) = keyboard_labels_fixture().await;
            let first = app.tab.id;
            app.tab
                .keyboard_drawing
                .set_target(Target::Atom(a), &app.tab.doc);
            if new_epoch {
                app.tab.keyboard_drawing.leave();
            }
            let _ = app.update(Message::New);
            let _ = app.update(Message::Tool(Tool::Text));
            if new_epoch {
                let epoch = app.next_epoch();
                let _ = app.in_tab(first, |app| {
                    let mut replacement = Document::default();
                    let reused = replacement.add_atom("N", Point::new(200., 100.));
                    assert_eq!(reused, a);
                    app.tab.doc = replacement;
                    app.tab.file_epoch = epoch;
                    app.tab.selected = vec![reused];
                });
            } else {
                let _ = app.in_tab(first, |app| app.tab.doc.delete(&[a]));
            }
            let _ = app.update(Message::Tab(first, Box::new(labels)));
            assert_eq!(app.tool, Tool::Text);
            let background = &app.tabs.background[0];
            assert_eq!(
                background.keyboard_drawing.target(),
                Target::Blank(Point::default())
            );
            assert_eq!(background.keyboard_drawing.marked(), None);
            assert_eq!(background.keyboard_drawing.enabled(), !new_epoch);
            if !new_epoch {
                assert!(
                    background.doc.atom(b).is_some(),
                    "A remaining hotspot is not selected"
                );
            }
        }
    }

    #[test]
    fn background_engine_results_apply_in_their_own_history() {
        for kind in [Job::Insert, Job::Import, Job::ImportFile] {
            let mut app = ready();
            let before = app.tab.doc.clone();
            let _ = app.update(Message::Analyze);
            let (id, revision) = (app.tab.id, app.tab.revision);
            let front = Front::new(&mut app);
            let _ = app.update(Message::Tab(
                id,
                Box::new(Message::EngineDone {
                    revision,
                    kind,
                    result: imported("O"),
                }),
            ));
            front.assert_unchanged(&app);
            let tab = &app.tabs.background[0];
            assert!(!tab.busy && tab.edited);
            assert_eq!(tab.doc.atoms[0].element, "O");
            select(&mut app, id);
            assert!(app.status.contains("structure") || app.status.contains("Structure"));
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert!(
                !app.tab.history.can_undo(),
                "Exactly one step for the result"
            );
        }
    }

    #[test]
    fn background_checks_and_cleanup_previews_stay_with_their_drawing() {
        for cleanup in [false, true] {
            let mut app = ready();
            let atom = app.tab.doc.add_atom("N", Point::default());
            app.tab.selected = vec![atom];
            app.tab.busy = true;
            let (id, revision) = (app.tab.id, app.tab.revision);
            let before = app.tab.doc.clone();
            let mut computed = before.clone();
            let kind = if cleanup {
                computed.atoms[0].position = Point::new(42., 10.);
                Job::Clean(super::super::cleanup::CleanupJob {
                    options: Default::default(),
                    selection: vec![atom],
                    serial: app.tab.cleanup_serial,
                    epoch: app.tab.file_epoch,
                })
            } else {
                computed.atoms[0].label_h = 3;
                Job::Analyze
            };
            let front = Front::new(&mut app);
            assert_eq!(
                app.update(Message::Tab(
                    id,
                    Box::new(Message::EngineDone {
                        revision,
                        kind,
                        result: Box::new(Ok(Response {
                            document: Some(computed.clone()),
                            analysis: None,
                            output: None,
                            engine_version: "test".into(),
                            warnings: vec![],
                        })),
                    })
                ))
                .units(),
                0
            );
            front.assert_unchanged(&app);
            let tab = &app.tabs.background[0];
            assert!(!tab.busy && !tab.history.can_undo());
            if cleanup {
                assert_eq!(tab.doc, before);
                assert_eq!(tab.cleanup.as_ref().unwrap().document, computed);
                select(&mut app, id);
                let _ = app.update(Message::ApplyCleanup);
                assert_eq!(app.tab.doc, computed);
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, before);
                assert!(!app.tab.history.can_undo());
            } else {
                assert_eq!(tab.doc, computed);
                assert_eq!(tab.status, "No chemistry errors found");
            }
        }
    }

    #[test]
    fn background_pasted_drawing_is_one_undo_step() {
        let mut app = ready();
        let before = app.tab.doc.clone();
        let id = app.tab.id;
        let mut part = Document::default();
        part.add_atom("O", Point::default());
        let contents = format!(
            "{}{}",
            reshiki::editing::CLIPBOARD_PREFIX,
            serde_json::to_string(&part.current()).unwrap()
        );
        let front = Front::new(&mut app);
        let _ = app.update(Message::Tab(id, Box::new(Message::Pasted(Some(contents)))));
        front.assert_unchanged(&app);
        select(&mut app, id);
        assert_eq!(app.status, "Selection pasted");
        assert_eq!(app.tab.doc.atoms[0].element, "O");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }

    #[test]
    fn background_edits_cannot_arrive_after_the_window_close_check() {
        let mut app = ready();
        let id = app.tab.id;
        let revision = app.tab.revision;
        app.tab.busy = true;
        app.add_tab();
        let directory = tempfile::tempdir().unwrap();
        app.tab.recovery =
            Some(reshiki::recovery::Recovery::in_directory(directory.path()).unwrap());
        let _ = app.close_window(iced::window::Id::unique(), vec![]);
        assert!(app.exit.closing());
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::EngineDone {
                revision,
                kind: Job::Insert,
                result: imported("O"),
            }),
        ));
        assert!(app.strip().all(|tab| tab.doc.all_ids().is_empty()));
        assert!(app.exit.closing());
    }

    #[test]
    fn background_pasted_text_starts_its_follow_up_in_the_same_tab() {
        let mut app = ready();
        let (id, revision) = (app.tab.id, app.tab.revision);
        let front = Front::new(&mut app);
        let task = app.update(Message::Tab(
            id,
            Box::new(Message::Pasted(Some("CO".into()))),
        ));
        assert!(task.units() > 0);
        front.assert_unchanged(&app);
        assert!(app.tabs.background[0].busy);
        assert_eq!(app.tabs.background[0].status, "Working…");
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::EngineDone {
                revision,
                kind: Job::Insert,
                result: imported("O"),
            }),
        ));
        front.assert_unchanged(&app);
        assert!(!app.tabs.background[0].busy);
        assert!(
            !app.tabs.background[0].labels_dirty,
            "Its label task was started too"
        );
        assert_eq!(app.tabs.background[0].doc.atoms[0].element, "O");
    }

    #[test]
    fn stale_and_failed_background_engine_results_release_their_tab() {
        for failed in [false, true] {
            let mut app = ready();
            let _ = app.update(Message::Analyze);
            let (id, revision) = (app.tab.id, app.tab.revision);
            edit(&mut app, "N");
            let before = app.tab.doc.clone();
            let front = Front::new(&mut app);
            let _ = app.update(Message::Tab(
                id,
                Box::new(Message::EngineDone {
                    revision,
                    kind: Job::Insert,
                    result: if failed {
                        Box::new(Err("Calculation failed".into()))
                    } else {
                        imported("O")
                    },
                }),
            ));
            front.assert_unchanged(&app);
            let tab = &app.tabs.background[0];
            assert_eq!(tab.doc, before);
            assert!(!tab.busy);
            assert_eq!(tab.error, failed);
            assert!(tab.status.contains(if failed {
                "Calculation failed"
            } else {
                "newer edits"
            }));
        }
    }

    #[test]
    fn closed_tab_results_are_dropped_and_do_not_leave_busy_flags() {
        let mut app = ready();
        let id = app.tab.id;
        app.tab.busy = true;
        app.tab.clipboard_busy = true;
        app.add_tab();
        let _ = app.update(Message::Tabs(Action::Close(Some(id))));
        assert!(app.tab_index(id).is_none());
        let front = Front::new(&mut app);
        for message in [
            Message::EngineDone {
                revision: 0,
                kind: Job::Insert,
                result: imported("O"),
            },
            Message::ClipboardRead {
                epoch: 0,
                revision: 0,
                result: Box::new(Err("late read".into())),
            },
            Message::ClipboardWritten {
                epoch: 0,
                revision: 0,
                cut_ids: vec![1],
                result: Err("late copy".into()),
            },
            Message::Pasted(Some("CO".into())),
        ] {
            assert_eq!(app.update(Message::Tab(id, Box::new(message))).units(), 0);
            front.assert_unchanged(&app);
            assert!(app.strip().all(|tab| !tab.busy && !tab.clipboard_busy));
        }
    }

    #[test]
    fn nested_tab_work_keeps_each_status_with_its_document() {
        let mut app = ready();
        let first = app.tab.id;
        app.status = "First".into();
        app.error = true;
        app.add_tab();
        let second = app.tab.id;
        app.status = "Second".into();
        app.in_tab(first, |app| {
            assert_eq!(app.status, "First");
            assert!(app.error);
            app.in_tab(second, |app| {
                assert_eq!(app.status, "Second");
                assert!(!app.error);
                app.status = "Second updated".into();
                app.error = true;
            });
            assert_eq!(app.status, "First");
            app.status = "First updated".into();
            app.error = false;
        });
        assert_eq!(app.tab.id, second);
        assert_eq!(app.status, "Second updated");
        assert!(app.error);
        select(&mut app, first);
        assert_eq!(app.status, "First updated");
        assert!(!app.error);
    }

    #[test]
    fn status_and_errors_follow_tabs_through_switches_and_background_results() {
        let mut app = ready();
        let first = app.tab.id;
        app.status = "First error".into();
        app.error = true;
        app.add_tab();
        let second = app.tab.id;
        assert_eq!(app.status, super::super::READY);
        assert!(!app.error);
        app.status = "Second status".into();
        for _ in 0..2 {
            select(&mut app, first);
            assert_eq!(app.status, "First error");
            assert!(app.error);
            select(&mut app, second);
            assert_eq!(app.status, "Second status");
            assert!(!app.error);
        }
        let _ = app.update(Message::Tab(
            first,
            Box::new(Message::EngineDone {
                revision: 0,
                kind: Job::Analyze,
                result: Box::new(Err("Background error".into())),
            }),
        ));
        assert_eq!(app.status, "Second status");
        assert!(!app.error);
        select(&mut app, first);
        assert_eq!(app.status, "Background error");
        assert!(app.error);
        select(&mut app, second);
        let _ = app.update(Message::Tabs(Action::Close(None)));
        assert_eq!(app.tab.id, first);
        assert_eq!(app.status, "Background error");
        assert!(app.error);
    }

    #[test]
    fn a_save_that_finishes_behind_the_front_marks_its_own_tab_saved() {
        let mut app = ready();
        edit(&mut app, "N");
        let (first, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
        app.file_io.saving = true;
        let _ = app.update(Message::New);
        edit(&mut app, "C");
        let _ = app.update(Message::Tabs(Action::Close(None)));
        assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
        let _ = app.update(Message::Tab(
            first,
            Box::new(Message::Saved(
                epoch,
                Box::new(snapshot),
                Ok(Some("first.rsk".into())),
            )),
        ));
        assert!(!app.file_io.saving);
        assert!(app.tab.path.is_none() && app.dirty());
        assert!(
            matches!(app.pending, Some(Pending::CloseTab(_))),
            "The dialog stays with the tab in front"
        );
        let saved = &app.tabs.background[0];
        assert_eq!(saved.path, Some("first.rsk".into()));
        assert!(!saved.edited);
    }

    #[test]
    fn a_dialogs_save_waits_only_for_its_own_tabs_save_in_progress() {
        let saved = |app: &mut App, id, epoch, snapshot| {
            let _ = app.update(Message::Tab(
                id,
                Box::new(Message::Saved(
                    epoch,
                    Box::new(snapshot),
                    Ok(Some("first.rsk".into())),
                )),
            ));
        };
        // Another tab's save cannot continue the dialog's close, so the
        // answer cancels it instead of leaving tabs and closing blocked.
        let mut app = ready();
        edit(&mut app, "N");
        app.tab.path = Some("first.rsk".into());
        let (first, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
        assert!(app.update(Message::Save).units() > 0);
        let _ = app.update(Message::New);
        edit(&mut app, "C");
        let _ = app.update(Message::Tabs(Action::Close(None)));
        assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
        let _ = app.update(Message::Save);
        assert!(app.pending.is_none());
        saved(&mut app, first, epoch, snapshot);
        assert!(app.pending.is_none() && !app.tabs.background[0].edited);
        let _ = app.update(Message::Tabs(Action::Cycle(true)));
        assert_eq!(app.tab.id, first, "Tabs switch again");
        // The tab's own save in progress still closes it once it lands.
        let mut app = ready();
        edit(&mut app, "N");
        app.tab.path = Some("first.rsk".into());
        let (first, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
        assert!(app.update(Message::Save).units() > 0);
        let _ = app.update(Message::Tabs(Action::Close(None)));
        let _ = app.update(Message::Save);
        assert!(matches!(app.pending, Some(Pending::CloseTab(id)) if id == first));
        saved(&mut app, first, epoch, snapshot);
        assert!(app.pending.is_none() && app.tab.id != first);
    }

    #[test]
    fn the_dialogs_save_closes_its_tab_after_a_finder_file_took_the_front() {
        let mut app = ready();
        edit(&mut app, "N");
        let (asked, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
        let _ = app.update(Message::Tabs(Action::Close(None)));
        assert!(app.update(Message::Save).units() > 0);
        let mut finder = Document::default();
        finder.add_atom("S", Point::default());
        let _ = app.update(Message::FilePrepared(Some((
            "finder.rsk".into(),
            Ok(super::super::files::Prepared::Native(Box::new(finder))),
        ))));
        assert_ne!(app.tab.id, asked);
        let _ = app.update(Message::Tab(
            asked,
            Box::new(Message::Saved(
                epoch,
                Box::new(snapshot),
                Ok(Some("asked.rsk".into())),
            )),
        ));
        assert!(app.pending.is_none(), "The dialog's close went on");
        assert!(app.tab_index(asked).is_none());
        assert_eq!(app.tab.path, Some("finder.rsk".into()));
    }

    #[test]
    fn a_structure_file_opened_beside_a_drawing_is_imported_into_its_new_tab() {
        let mut app = ready();
        edit(&mut app, "N");
        let first = app.tab.id;
        let opened = Some((
            "ethanol.mol".into(),
            Ok(super::super::files::Prepared::Import {
                format: "mol",
                contents: String::new(),
            }),
        ));
        assert!(app.update(Message::FilePrepared(opened)).units() > 0);
        assert_ne!(app.tab.id, first);
        assert!(app.tab.busy && !app.tabs.background[0].busy);
        // `update` marks the engine's result with the tab now in front.
        let _ = app.update(Message::Tab(
            app.tab.id,
            Box::new(Message::EngineDone {
                revision: app.tab.revision,
                kind: Job::ImportFile,
                result: imported("O"),
            }),
        ));
        assert_eq!(app.tab.doc.atoms[0].element, "O");
        assert_eq!(app.tabs.background[0].doc.atoms[0].element, "N");
    }

    #[test]
    fn closing_tabs_asks_only_for_unsaved_ones_and_the_last_leaves_an_empty_tab() {
        let mut app = ready();
        edit(&mut app, "N");
        let first = app.tab.id;
        let _ = app.update(Message::New);
        let second = app.tab.id;
        // A saved tab behind the front one closes where it is.
        let _ = app.update(Message::Tabs(Action::Close(Some(second))));
        assert_eq!(app.tab.id, first);
        assert!(app.pending.is_none());
        let _ = app.update(Message::New);
        let third = app.tab.id;
        // An unsaved tab comes to the front for the dialog.
        let task = app.update(Message::Tabs(Action::Close(Some(first))));
        assert!(task.units() > 0);
        assert_eq!(app.tab.id, first);
        assert!(matches!(app.pending, Some(Pending::CloseTab(id)) if id == first));
        let _ = app.update(Message::Discard);
        assert_eq!(app.tab.id, third);
        assert_eq!(app.strip().count(), 1);
        let _ = app.update(Message::Tabs(Action::Close(None)));
        assert_ne!(app.tab.id, third, "The last tab gives way to a new one");
        assert_eq!(app.strip().count(), 1);
        assert!(app.tab.reusable());
    }

    #[test]
    fn window_close_asks_about_each_unsaved_tab_in_turn_and_cancel_stops_it() {
        let window = iced::window::Id::unique();
        let mut app = ready();
        edit(&mut app, "N");
        let first = app.tab.id;
        let _ = app.update(Message::New);
        let saved = app.tab.id;
        app.tab.path = Some("saved.rsk".into());
        let _ = app.update(Message::New);
        edit(&mut app, "O");
        let third = app.tab.id;
        select(&mut app, saved);
        assert!(app.update(Message::Close(window)).units() > 0);
        assert_eq!(
            app.tab.id, first,
            "The first unsaved tab comes to the front"
        );
        let _ = app.update(Message::Discard);
        assert_eq!(app.tab.id, third);
        assert!(
            matches!(&app.pending, Some(Pending::CloseWindow(_, id, discarded))
            if *id == third && *discarded == [first])
        );
        let _ = app.update(Message::Cancel);
        assert!(app.pending.is_none() && !app.exit.closing());
        assert_eq!(app.strip().count(), 3, "Cancel keeps every tab");
        // Saving the last unsaved tab continues the close.
        let _ = app.update(Message::Close(window));
        let _ = app.update(Message::Discard);
        let epoch = app.tab.file_epoch;
        let snapshot = Box::new(app.tab.doc.clone());
        assert!(
            app.update(Message::Saved(
                epoch,
                snapshot,
                Ok(Some("third.rsk".into()))
            ))
            .units()
                > 0,
            "No draft to remove, so the window closes"
        );
        assert!(app.pending.is_none());
    }

    #[test]
    fn tab_shortcuts_route_before_fields_and_do_not_collide() {
        use super::super::{file_shortcuts::file_message, shortcuts::key_message};
        use iced::keyboard::{Key, Modifiers, key::Named};
        let command = if cfg!(target_os = "macos") {
            Modifiers::LOGO
        } else {
            Modifiers::CTRL
        };
        let tab = Key::Named(Named::Tab);
        let message = |key: &Key, modifiers| format!("{:?}", file_message(key, modifiers));
        assert_eq!(message(&tab, Modifiers::CTRL), "Some(Tabs(Cycle(true)))");
        assert_eq!(
            message(&tab, Modifiers::CTRL | Modifiers::SHIFT),
            "Some(Tabs(Cycle(false)))"
        );
        assert_eq!(
            message(&Key::Character("w".into()), command),
            "Some(Tabs(Close(None)))"
        );
        for n in 1..=9 {
            let key = Key::Character(n.to_string().into());
            assert_eq!(message(&key, command), format!("Some(Tabs(Number({n})))"));
            // No drawing command uses these keys.
            assert!(key_message(&key, &key, command).is_none());
        }
        assert!(key_message(&tab, &tab, Modifiers::CTRL).is_none());
        let w = Key::Character("w".into());
        assert!(key_message(&w, &w, command).is_none());
    }
}
