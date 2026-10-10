//! Commands for the object under a secondary click, or the current selection.
use super::object_toolbar::Command;
use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message, inspector};
use crate::canvas::Tool;
use iced::widget::{button, column, container, rich_text, row, scrollable, text};
use iced::{Border, Color, Element, Length, Point, Task, keyboard::key::Named};

mod cascade;
#[cfg(test)]
mod cascade_tests;
use reshiki::{
    bonds::BondPreset,
    editing::{Arrange, Transform},
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Page {
    #[default]
    Main,
    Align,
    Bonds,
    Tilt,
    Attachments,
    CopyAs,
    // Menus anchored under context row buttons.
    AlignObjects,
    Distribute,
    Order,
    Arrange,
    /// The first n context row commands, folded into ⋯.
    More(usize),
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_parity_tests;
const SHORTCUT_GAP: f32 = 12.;
const SCROLLBAR_WIDTH: f32 = 10.;
const SCROLLBAR_GAP: f32 = 4.;

/// The shortcut shown right-aligned beside a command.
fn shortcut(action: &Action) -> Option<String> {
    match action {
        Action::Run(message) => super::shortcuts::label(message),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Close,
    /// Toggle a context row menu at this x offset over the canvas.
    Open(Page, f32),
    Page(Page),
    Hover(usize, usize),
    Activate(usize, usize),
    CloseAfter(usize),
    Key(Named),
    FocusedKey(usize, usize, Named),
    Run(Box<Message>),
    Properties(bool),
}
pub(super) struct State {
    pub position: Point,
    pub page: Page,
    children: Vec<Child>,
    focused: Option<(usize, usize)>,
    keyboard: bool,
}
#[derive(Clone, Copy)]
struct Child {
    page: Page,
    anchor: usize,
}
impl State {
    pub(super) fn new(position: Point, page: Page) -> Self {
        Self {
            position,
            page,
            children: vec![],
            focused: None,
            keyboard: false,
        }
    }

    fn page_at(&self, level: usize) -> Option<Page> {
        if level == 0 {
            Some(self.page)
        } else {
            self.children.get(level - 1).map(|child| child.page)
        }
    }
}

#[derive(Clone)]
enum Entry {
    Item {
        label: &'static str,
        action: Action,
        enabled: bool,
    },
    Separator,
    Hint(&'static str),
}
impl Entry {
    fn command(label: &'static str, message: Message, enabled: bool) -> Self {
        Self::Item {
            label,
            action: Action::Run(Box::new(message)),
            enabled,
        }
    }
    fn page(label: &'static str, page: Page) -> Self {
        Self::Item {
            label,
            action: Action::Page(page),
            enabled: true,
        }
    }
}

impl App {
    fn context_entries(&self, page: Page) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        let submenu = Entry::page;
        let atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .any(|a| self.tab.selected.contains(&a.id));
        let bonds = self
            .tab
            .doc
            .bonds
            .iter()
            .any(|b| self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b));
        let tilt = crate::canvas::tilt::available(&self.tab.doc, &self.tab.selected);
        let multiple = self.alignment_count() >= 2;
        match page {
            Page::Main if self.tab.selected.is_empty() => vec![
                command("Undo", Message::Undo, self.tab.history.can_undo()),
                command("Redo", Message::Redo, self.tab.history.can_redo()),
                Separator,
                command("Paste", Message::Paste, !self.tab.clipboard_busy),
                submenu("Copy as", Page::CopyAs),
                command(
                    "Select all",
                    Message::SelectAll,
                    !self.tab.doc.all_ids().is_empty(),
                ),
                Separator,
                command("Fit drawing", Message::Fit, true),
            ],
            Page::Main => self.main_entries(atoms, bonds, tilt),
            Page::CopyAs => self.copy_as_entries(),
            Page::Tilt => vec![
                Hint("3D tilt · selected objects"),
                command("Drag to tilt", Message::Tool(Tool::Tilt), tilt),
                Separator,
                command("X −15°", Message::Transform(Transform::TiltX(-15.)), tilt),
                command("X +15°", Message::Transform(Transform::TiltX(15.)), tilt),
                command("Y −15°", Message::Transform(Transform::TiltY(-15.)), tilt),
                command("Y +15°", Message::Transform(Transform::TiltY(15.)), tilt),
                Separator,
                command(
                    "Emphasize front bonds",
                    Message::InspectorAction(inspector::Action::DepthBonds),
                    bonds,
                ),
                Hint("Labels stay upright. Undo restores the previous view."),
            ],
            Page::Align => self.align_entries(atoms, multiple),
            Page::Bonds => self.bond_entries(bonds),
            Page::AlignObjects => self.arrange_entries(&[&Command::HORIZONTAL, &Command::VERTICAL]),
            Page::Distribute => self.arrange_entries(&[&Command::DISTRIBUTE]),
            Page::Order => self.arrange_entries(&[&Command::ORDER]),
            Page::Arrange => [
                self.arrange_entries(&[&Command::HORIZONTAL, &Command::VERTICAL]),
                self.arrange_entries(&[&Command::DISTRIBUTE]),
                self.arrange_entries(&[&Command::ORDER]),
                self.arrange_entries(&[&Command::TRANSFORM]),
            ]
            .join(&Separator),
            Page::More(folded) => self
                .context_commands()
                .into_iter()
                .take(folded)
                .map(|c| command(c.menu, c.message, c.enabled))
                .collect(),
            Page::Attachments => vec![
                Hint("Attach to selected atoms"),
                command(
                    "Multi-center attachment",
                    Message::InspectorAction(inspector::Action::Attachment(
                        reshiki::attachments::Kind::MultiCenter,
                    )),
                    true,
                ),
                command(
                    "Variable attachment",
                    Message::InspectorAction(inspector::Action::Attachment(
                        reshiki::attachments::Kind::Variable,
                    )),
                    true,
                ),
                Separator,
                command(
                    "Drawing centroid",
                    Message::InspectorAction(inspector::Action::Centroid),
                    true,
                ),
                Hint("Multi-center: all selected atoms. Variable: one of the selected atoms."),
            ],
        }
    }

    fn main_entries(&self, atoms: bool, bonds: bool, tilt: bool) -> Vec<Entry> {
        use Entry::Separator;
        let command = Entry::command;
        let submenu = Entry::page;
        let mut entries = vec![];
        if atoms {
            entries.extend(
                self.selected_join_commands()
                    .into_iter()
                    .map(|c| command(c.menu, c.message, c.enabled)),
            );
        }
        if atoms && self.atom_text_target().is_some() {
            entries.push(command(
                "Edit atom label…",
                Message::AtomText(super::atom_text::Action::Begin(None)),
                true,
            ));
        }
        if atoms
            && self
                .tab
                .selected
                .iter()
                .filter(|id| self.tab.doc.atom(**id).is_some())
                .count()
                > 1
        {
            entries.push(command(
                "Create group label…",
                Message::AtomText(super::atom_text::Action::ContractSelection),
                true,
            ));
        }
        if atoms {
            let connected: Vec<_> =
                reshiki::editing::groups(&self.tab.doc, &self.tab.doc.all_ids())
                    .into_iter()
                    .filter(|ids| {
                        ids.iter().any(|id| {
                            self.tab.selected.contains(id) && self.tab.doc.atom(*id).is_some()
                        })
                    })
                    .flatten()
                    .collect();
            if connected.len() != self.tab.selected.len()
                || connected.iter().any(|id| !self.tab.selected.contains(id))
            {
                entries.push(command(
                    "Select molecule",
                    Message::Canvas(crate::canvas::Edit::Select(connected)),
                    true,
                ));
            }
        }
        if tilt {
            entries.push(submenu("3D tilt", Page::Tilt));
        }
        if self.tab.doc.atoms.iter().any(|a| {
            self.tab.selected.contains(&a.id)
                && a.attachment.is_some()
                && self.tab.doc.abbreviation(a.id).is_none()
        }) {
            entries.push(command(
                "Move attachment point only",
                Message::Tool(Tool::EditPoints),
                true,
            ));
        }
        entries.push(submenu("Arrange & transform", Page::Align));
        if bonds {
            entries.push(submenu("Bond appearance", Page::Bonds));
        }
        let real_atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| {
                self.tab.selected.contains(&a.id) && a.element != "*" && a.centroid.is_empty()
            })
            .count();
        if (2..=300).contains(&real_atoms) && real_atoms == self.tab.selected.len() {
            entries.push(submenu("Attachment points", Page::Attachments));
        }
        if self
            .tab
            .doc
            .arrows
            .iter()
            .any(|a| self.tab.selected.contains(&a.id))
        {
            entries.push(command(
                "Reverse arrow",
                Message::ArrowAction(super::arrows::Action::Reverse),
                true,
            ));
            entries.push(command(
                "Straighten arrow",
                Message::ArrowAction(super::arrows::Action::Straighten),
                true,
            ));
        }
        entries.push(Separator);
        entries.push(command("Cut", Message::Copy(true), true));
        entries.push(command("Copy", Message::Copy(false), true));
        entries.push(submenu("Copy as", Page::CopyAs));
        entries.push(command("Paste", Message::Paste, !self.tab.clipboard_busy));
        entries.push(command("Duplicate", Message::Duplicate, true));
        entries.push(Separator);
        entries.push(Entry::Item {
            label: "Properties…",
            action: Action::Properties(false),
            enabled: true,
        });
        entries.push(command("Delete", Message::Delete, true));
        entries
    }

    fn copy_as_entries(&self) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        use reshiki::clipboard::{CopyFormat, chemical_snapshot, selection_or_drawing};
        let snapshot = selection_or_drawing(&self.tab.doc, &self.tab.selected);
        let chemical = chemical_snapshot(&self.tab.doc, &snapshot);
        let busy = self.clipboard_working();
        let mut entries = vec![Hint(if self.tab.selected.is_empty() {
            "Copy as · whole drawing"
        } else {
            "Copy as · selected objects"
        })];
        let mut reasons = Vec::new();
        for format in CopyFormat::ALL {
            if matches!(
                format,
                CopyFormat::Mol | CopyFormat::Cdxml | CopyFormat::Rxn
            ) {
                entries.push(Separator);
            }
            let reason = if format.is_chemical() {
                match &chemical {
                    Ok(doc) => format.unavailable_reason(doc),
                    Err(reason) => Some(*reason),
                }
            } else {
                format.unavailable_reason(&snapshot)
            };
            entries.push(command(
                format.label(),
                Message::CopyAs(format),
                !busy && reason.is_none(),
            ));
            if let Some(reason) = reason
                && !reasons.contains(&reason)
            {
                reasons.push(reason);
            }
        }
        if busy {
            reasons.push("A clipboard operation is already in progress.");
        }
        if !reasons.is_empty() {
            entries.push(Separator);
            entries.extend(reasons.into_iter().map(Hint));
        }
        entries
    }

    fn align_entries(&self, atoms: bool, multiple: bool) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        let mut entries = vec![
            Hint("Rotate & reflect"),
            command(
                "Rotate −30°",
                Message::Transform(Transform::Rotate(-30.)),
                true,
            ),
            command(
                "Rotate +30°",
                Message::Transform(Transform::Rotate(30.)),
                true,
            ),
            command(
                "Flip horizontal",
                Message::Transform(Transform::FlipHorizontal),
                true,
            ),
            command(
                "Flip vertical",
                Message::Transform(Transform::FlipVertical),
                true,
            ),
        ];
        if multiple {
            entries.push(Separator);
            for (label, action) in [
                ("Align left edges", Arrange::AlignLeft),
                ("Align horizontal centers", Arrange::AlignHorizontal),
                ("Align right edges", Arrange::AlignRight),
                ("Align top edges", Arrange::AlignTop),
                ("Align middles", Arrange::AlignVertical),
                ("Align bottom edges", Arrange::AlignBottom),
                ("Distribute horizontally", Arrange::DistributeHorizontal),
                ("Distribute vertically", Arrange::DistributeVertical),
            ] {
                entries.push(command(label, Message::Arrange(action), true));
            }
        }
        if atoms
            || self.can_group()
            || !self
                .tab
                .doc
                .outer_selected_groups(&self.tab.selected)
                .is_empty()
        {
            entries.push(Separator);
            if atoms {
                entries.push(command(
                    "Move & attach…",
                    Message::Join(super::joining::Action::Begin),
                    true,
                ));
            }
            if self.can_group() {
                entries.push(command("Group", Message::Group, true));
            }
            if !self
                .tab
                .doc
                .outer_selected_groups(&self.tab.selected)
                .is_empty()
            {
                entries.push(command("Ungroup", Message::Ungroup, true));
            }
        }
        entries
    }

    fn bond_entries(&self, bonds: bool) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        let mut entries = vec![Hint("Bond appearance")];
        entries.push(command("Bond in front", Message::BondDepth(true), bonds));
        entries.push(command("Bond behind", Message::BondDepth(false), bonds));
        if reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some() {
            entries.push(command(
                "Saturated ↔ Aromatic",
                Message::ToggleSelectedRing,
                true,
            ));
        }
        if !reshiki::ring_fills::selected_cycles(&self.tab.doc, &self.tab.selected).is_empty() {
            entries.push(command(
                "Color ring interior…",
                Message::StyleMenu(super::color_popover::Action::RingColor),
                true,
            ));
            entries.push(command(
                "Clear ring fill",
                Message::ClearRingFill,
                self.tab
                    .doc
                    .ring_fills
                    .iter()
                    .any(|f| f.atoms.iter().all(|id| self.tab.selected.contains(id))),
            ));
        }
        if reshiki::ring_arcs::toggle(&mut self.tab.doc.clone(), &self.tab.selected).is_ok() {
            entries.push(command(
                "Toggle inner ring curve",
                Message::InspectorAction(inspector::Action::RingArc),
                true,
            ));
        }
        entries.push(Separator);
        entries.extend(
            BondPreset::ALL
                .into_iter()
                .map(|preset| command(preset.name(), Message::ApplyBondPreset(preset), bonds)),
        );
        entries
    }

    /// Groups of arrange commands that share one availability rule, or a
    /// single disabled row saying what they need.
    fn arrange_entries(&self, groups: &[&[Command]]) -> Vec<Entry> {
        let mut commands = groups.iter().flat_map(|group| group.iter());
        if let Some(first) = commands
            .next()
            .filter(|c| !c.enabled(self, self.alignment_count()))
        {
            return vec![Entry::Item {
                label: first.unavailable(),
                action: Action::Close,
                enabled: false,
            }];
        }
        groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|c| Entry::command(c.name(), c.message(), true))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .join(&Entry::Separator)
    }

    pub(super) fn context_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Close => self.context_menu = None,
            Action::Open(page, x) => {
                let open = self
                    .context_menu
                    .as_ref()
                    .is_some_and(|menu| menu.page == page);
                self.context_menu = (!open).then(|| State::new(Point::new(x, 0.), page));
            }
            Action::Page(Page::Main) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.children.clear();
                    menu.focused = None;
                }
            }
            Action::Page(page) => {
                let target = self.context_menu.as_ref().and_then(|menu| {
                    (0..=menu.children.len()).rev().find_map(|level| {
                        self.context_entries(menu.page_at(level)?)
                            .iter()
                            .position(|entry| {
                                matches!(entry, Entry::Item {
                                action: Action::Page(target), enabled: true, ..
                            } if *target == page)
                            })
                            .map(|index| (level, index))
                    })
                });
                if let Some((level, index)) = target {
                    self.context_hover(level, index, true);
                }
            }
            Action::Hover(level, index) => self.context_hover(level, index, true),
            Action::Activate(level, index) => {
                let entry = self
                    .context_menu
                    .as_ref()
                    .and_then(|menu| menu.page_at(level))
                    .and_then(|page| self.context_entries(page).get(index).cloned());
                if let Some(Entry::Item {
                    action,
                    enabled: true,
                    ..
                }) = entry
                {
                    if matches!(action, Action::Page(_)) {
                        self.context_hover(level, index, true);
                    } else {
                        return self.context_action(action);
                    }
                }
            }
            Action::CloseAfter(level) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.children.truncate(level);
                    menu.focused = None;
                    menu.keyboard = false;
                }
            }
            Action::Key(key) => return self.context_key_action(key),
            Action::FocusedKey(level, index, key) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.focused = Some((level, index));
                }
                return self.context_key_action(key);
            }
            Action::Run(message) => {
                self.context_menu = None;
                return self.update(*message);
            }
            Action::Properties(molecular) => {
                self.context_menu = None;
                if molecular {
                    self.tab.inspector_ui.update(inspector::Action::Section(
                        inspector::Section::Molecule,
                        true,
                    ));
                }
                return self.update(Message::Inspector(InspectorTab::Properties));
            }
        }
        Task::none()
    }

    fn context_hover(&mut self, level: usize, index: usize, open: bool) {
        let entry = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.page_at(level))
            .and_then(|page| self.context_entries(page).get(index).cloned());
        let Some(menu) = &mut self.context_menu else {
            return;
        };
        let Some(Entry::Item {
            action, enabled, ..
        }) = entry
        else {
            return;
        };
        menu.focused = Some((level, index));
        menu.keyboard = false;
        let child = match action {
            Action::Page(page) if enabled && open => Some(page),
            _ => None,
        };
        if menu
            .children
            .get(level)
            .is_some_and(|current| Some(current.page) == child && current.anchor == index)
        {
            return;
        }
        menu.children.truncate(level);
        if let Some(page) = child {
            menu.children.push(Child {
                page,
                anchor: index,
            });
        }
    }

    fn context_key_action(&mut self, key: Named) -> Task<Message> {
        if key == Named::Escape {
            self.context_menu = None;
            return Task::none();
        }
        let Some(menu) = &self.context_menu else {
            return Task::none();
        };
        let level = menu.focused.map_or(menu.children.len(), |(level, _)| level);
        let focused = menu
            .focused
            .filter(|(at, _)| *at == level)
            .map(|(_, index)| index);
        let Some(page) = menu.page_at(level) else {
            return Task::none();
        };
        let entries = self.context_entries(page);
        let enabled: Vec<_> = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                matches!(entry, Entry::Item { enabled: true, .. }).then_some(index)
            })
            .collect();
        match key {
            Named::ArrowDown | Named::ArrowUp | Named::Home | Named::End => {
                let current =
                    focused.and_then(|index| enabled.iter().position(|value| *value == index));
                let next = match key {
                    Named::Home => enabled.first(),
                    Named::End => enabled.last(),
                    Named::ArrowDown => current
                        .map(|at| (at + 1) % enabled.len())
                        .and_then(|at| enabled.get(at))
                        .or_else(|| enabled.first()),
                    _ => current
                        .map(|at| (at + enabled.len() - 1) % enabled.len())
                        .and_then(|at| enabled.get(at))
                        .or_else(|| enabled.last()),
                }
                .copied();
                if let Some(index) = next {
                    self.context_hover(level, index, false);
                    if let Some(menu) = &mut self.context_menu {
                        menu.keyboard = true;
                    }
                }
            }
            Named::ArrowLeft => {
                if let Some(menu) = &mut self.context_menu
                    && let Some(child) = menu.children.pop()
                {
                    menu.focused = Some((menu.children.len(), child.anchor));
                    menu.keyboard = true;
                }
            }
            Named::ArrowRight | Named::Enter | Named::Space => {
                let index = focused.or_else(|| enabled.first().copied());
                if let Some(index) = index {
                    if let Some(Entry::Item {
                        action: Action::Page(child),
                        enabled: true,
                        ..
                    }) = entries.get(index)
                    {
                        let first = self
                            .context_entries(*child)
                            .iter()
                            .position(|entry| matches!(entry, Entry::Item { enabled: true, .. }));
                        self.context_hover(level, index, true);
                        if let Some(menu) = &mut self.context_menu {
                            menu.focused = first.map(|index| (level + 1, index));
                            menu.keyboard = true;
                        }
                    } else if key != Named::ArrowRight {
                        return self.context_action(Action::Activate(level, index));
                    }
                }
            }
            _ => {}
        }
        if let Some(menu) = &self.context_menu
            && menu.keyboard
            && let Some((level, index)) = menu.focused
            && let Some(page) = menu.page_at(level)
        {
            return iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(
                format!("menu-{page:?}-{index}"),
            ))
            .discard()
            .chain(Task::done(Message::Accessibility(
                super::accessibility::Action::Refresh,
            )));
        }
        Task::none()
    }

    fn context_width(&self, page: Page) -> f32 {
        use super::workspace::{font_width, text_width};
        let content = self
            .context_entries(page)
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item { label, action, .. } => Some(
                    text_width(label, 12.)
                        + if matches!(action, Action::Page(_)) {
                            24.
                        } else {
                            shortcut(action).map_or(0., |keys| {
                                SHORTCUT_GAP
                                    + super::shortcuts::spans(&keys)
                                        .iter()
                                        .map(|run| match run.font {
                                            Some(font) => font_width(&run.text, 11., font),
                                            None => text_width(&run.text, 11.),
                                        })
                                        .sum::<f32>()
                            })
                        },
                ),
                _ => None,
            })
            .fold(0., f32::max);
        // Keep the measured text width after the scrollbar takes its own space.
        (content + 30. + SCROLLBAR_WIDTH + SCROLLBAR_GAP).max(232.)
    }

    fn context_panel(&self, menu: &State, page: Page, level: usize) -> cascade::Panel<'_> {
        let mut entries = column![].spacing(1);
        let mut items = Vec::new();
        let keyboard_commands = if matches!(page, Page::More(_)) {
            self.context_commands()
        } else {
            Vec::new()
        };
        if matches!(page, Page::Main) && !self.tab.selected.is_empty() {
            entries = entries.push(
                container(
                    text(self.selection_summary())
                        .size(11)
                        .style(super::workspace::muted_text),
                )
                .padding([5, 10]),
            );
        }
        for (index, entry) in self.context_entries(page).into_iter().enumerate() {
            entries = match entry {
                Entry::Item {
                    label,
                    action,
                    enabled,
                } => {
                    let destructive = matches!(&action, Action::Run(message) if matches!(message.as_ref(), Message::Delete));
                    let keyboard_hint = match &action {
                        Action::Run(message) => super::workspace::keyboard_control_id(message)
                            .and_then(|id| {
                                keyboard_commands.iter().find(|command| {
                                    super::workspace::keyboard_control_id(&command.message)
                                        == Some(id)
                                })
                            })
                            .map(|command| command.hint),
                        _ => None,
                    };
                    let accessible_name = keyboard_hint.unwrap_or(label);
                    let label = text(label).size(12).width(Length::Fill);
                    let label = if destructive {
                        label.style(crate::appearance::text_color(Color::from_rgb8(167, 59, 51)))
                    } else {
                        label
                    };
                    let mut content = row![label]
                        .spacing(SHORTCUT_GAP)
                        .align_y(iced::Alignment::Center);
                    if matches!(action, Action::Page(_)) {
                        content = content.push(text("›").size(16));
                    } else if let Some(keys) = shortcut(&action) {
                        content = content.push(
                            rich_text(super::shortcuts::spans(&keys))
                                .size(11)
                                .style(super::workspace::muted_text),
                        );
                    }
                    let active = enabled
                        && (menu.focused == Some((level, index))
                            || menu
                                .children
                                .get(level)
                                .is_some_and(|child| child.anchor == index));
                    let id = match &action {
                        Action::Run(message) => {
                            super::workspace::keyboard_control_id(message).map(str::to_owned)
                        }
                        _ => None,
                    }
                    .unwrap_or_else(|| format!("menu-{page:?}-{index}"));
                    let item = reshiki::accessibility::button(id, accessible_name, content)
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(move |theme: &iced::Theme, status| {
                            let mut style = button::text(theme, status);
                            if active {
                                style.background = Some(
                                    Color {
                                        a: 0.12,
                                        ..theme.palette().primary
                                    }
                                    .into(),
                                );
                            }
                            style
                        })
                        .on_press_maybe(
                            enabled.then_some(Message::ContextMenu(Action::Activate(level, index))),
                        );
                    items.push(index);
                    let item: Element<'_, Message> = if let Some(hint) = keyboard_hint {
                        super::workspace::hover_hint(
                            item,
                            hint.to_owned(),
                            iced::widget::tooltip::Position::Right,
                        )
                        .into()
                    } else {
                        item.into()
                    };
                    entries.push(container(item).id(cascade::row_id(level, index)))
                }
                Entry::Separator => entries.push(horizontal_line()),
                Entry::Hint(label) => entries.push(
                    container(text(label).size(11).style(super::workspace::muted_text))
                        .padding([5, 10]),
                ),
            };
        }
        let content = container(
            scrollable(entries)
                .id(cascade::scroll_id(level))
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(SCROLLBAR_WIDTH)
                        .scroller_width(SCROLLBAR_WIDTH)
                        .spacing(SCROLLBAR_GAP),
                ))
                .height(Length::Shrink),
        )
        .padding(5)
        .width(self.context_width(page))
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::WHITE.into()),
                    border: Border {
                        color: Color::from_rgb8(192, 204, 201),
                        width: 1.,
                        radius: 7.into(),
                    },
                    shadow: crate::appearance::surface_shadow(iced::Shadow {
                        color: Color::from_rgba8(20, 40, 35, 0.18),
                        offset: iced::Vector::new(0., 4.),
                        blur_radius: 12.,
                    }),
                    ..Default::default()
                },
            )
        })
        .into();
        cascade::Panel {
            page,
            content,
            items,
            anchor: level
                .checked_sub(1)
                .and_then(|at| menu.children.get(at))
                .map(|child| child.anchor),
        }
    }

    pub(super) fn with_context_menu<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(menu) = &self.context_menu else {
            return base;
        };
        let panels = (0..=menu.children.len())
            .filter_map(|level| {
                menu.page_at(level)
                    .map(|page| self.context_panel(menu, page, level))
            })
            .collect();
        Element::new(cascade::Cascade::new(base, panels, menu))
    }
}
