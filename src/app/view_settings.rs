//! Window, camera and view-aid preferences.
use super::{App, Message};
use iced::advanced::widget::{Id, Operation, operation};
use iced::keyboard::{Key, key::Named};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length, Task};

/// View-aid commands, nested under `Message::View`.
/// A menu choice re-enters the chosen command's existing modal and draft gates.
#[derive(Debug, Clone)]
pub enum Action {
    /// Flips the grid regardless of the checkbox value.
    Grid,
    SmartGuides(bool),
    TwoDecimalWeight(bool),
    NameTypography(bool),
    /// Put the editable drawing in copied and exported pictures by default.
    ExportEditableDrawing(bool),
    /// Keep text editable in plain SVG exports.
    ExportSvgLiveText(bool),
    Rulers(bool),
    Crosshair(bool),
    RulerUnit(crate::canvas::guides::Unit),
    /// Shows or hides the anchored View menu (was `ToggleView`).
    Toggle,
    Close,
    Choose(Item),
    Focus(Direction),
}

#[derive(Debug, Clone, Copy)]
pub enum Item {
    Grid,
    Rulers,
    Crosshair,
    SmartGuides,
    ArrangeControls,
    DarkCanvas,
    PageSetup,
}

impl Item {
    const ALL: [Self; 7] = [
        Self::Grid,
        Self::Rulers,
        Self::Crosshair,
        Self::SmartGuides,
        Self::ArrangeControls,
        Self::DarkCanvas,
        Self::PageSetup,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Grid => "view-grid",
            Self::Rulers => "view-rulers",
            Self::Crosshair => "view-crosshair",
            Self::SmartGuides => "view-smart-guides",
            Self::ArrangeControls => "view-arrange-controls",
            Self::DarkCanvas => "canvas-theme",
            Self::PageSetup => "view-page-setup",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid",
            Self::Rulers => "Rulers",
            Self::Crosshair => "Crosshair",
            Self::SmartGuides => "Smart guides",
            Self::ArrangeControls => "Arrange controls",
            Self::DarkCanvas => "Dark canvas",
            Self::PageSetup => "Page setup…",
        }
    }

    fn checked(self, app: &App) -> Option<bool> {
        Some(match self {
            Self::Grid => app.grid,
            Self::Rulers => app.guides.rulers,
            Self::Crosshair => app.guides.crosshair,
            Self::SmartGuides => app.appearance.smart_guides,
            Self::ArrangeControls => app.appearance.arrange_controls,
            Self::DarkCanvas => app.tab.doc.canvas_theme.is_dark(),
            Self::PageSetup => return None,
        })
    }

    fn message(self, app: &App) -> Message {
        match self {
            Self::Grid => Message::View(Action::Grid),
            Self::Rulers => Message::View(Action::Rulers(!app.guides.rulers)),
            Self::Crosshair => Message::View(Action::Crosshair(!app.guides.crosshair)),
            Self::SmartGuides => Message::View(Action::SmartGuides(!app.appearance.smart_guides)),
            Self::ArrangeControls => Message::ObjectToolbar(
                super::object_toolbar::Action::Visible(!app.appearance.arrange_controls),
            ),
            Self::DarkCanvas => Message::CanvasTheme(app.tab.doc.canvas_theme.toggled()),
            Self::PageSetup => Message::Pages(super::pages::Action::Show),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Next,
    Previous,
    First,
    Last,
}

struct Navigate {
    direction: Direction,
    ids: Vec<Id>,
    focused: Option<usize>,
}

impl Operation for Navigate {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(
        &mut self,
        id: Option<&Id>,
        _: iced::Rectangle,
        state: &mut dyn operation::Focusable,
    ) {
        if state.is_focused() {
            self.focused = self
                .ids
                .iter()
                .position(|candidate| Some(candidate) == id)
                .or(self.focused);
        }
    }
    fn finish(&self) -> operation::Outcome<()> {
        let index = match self.direction {
            Direction::First => 0,
            Direction::Last => Item::ALL.len() - 1,
            Direction::Next => self
                .focused
                .map_or(0, |index| (index + 1) % Item::ALL.len()),
            Direction::Previous => self
                .focused
                .unwrap_or(0)
                .checked_sub(1)
                .unwrap_or(Item::ALL.len() - 1),
        };
        let Some(item) = Item::ALL.get(index) else {
            return operation::Outcome::None;
        };
        operation::Outcome::Chain(Box::new(reshiki::accessibility::FocusControl::new(
            item.id(),
        )))
    }
}

fn focus(id: &str) -> Task<Message> {
    iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(id))
}

pub(super) fn keeps_open(message: &Message) -> bool {
    super::results::async_result(message).is_some()
        || matches!(
            message,
            Message::View(_)
                | Message::Shortcut(
                    super::shortcuts::Action::Rulers | super::shortcuts::Action::Crosshair
                )
                | Message::Escape
                | Message::Tick
                | Message::Viewport(_)
                | Message::InspectorScroll(_)
                | Message::Canvas(
                    crate::canvas::Edit::Hover(_) | crate::canvas::Edit::PointerHover(_)
                )
                | Message::Nmr(super::nmr::Action::RenderedSize(_))
                | Message::Updates(
                    super::updates::Action::Poll | super::updates::Action::Check(false)
                )
        )
}

impl App {
    pub(super) fn view_action(&mut self, action: Action) -> Task<Message> {
        let was_open = self.view_open;
        match action {
            Action::Grid => self.grid = !self.grid,
            Action::SmartGuides(enabled) => self.set_smart_guides(enabled),
            Action::TwoDecimalWeight(enabled) => self.set_two_decimal_weight(enabled),
            Action::NameTypography(enabled) => {
                self.appearance.name_typography = enabled;
                self.save_preference(
                    super::settings_page::Section::Names,
                    "chemical name typography",
                );
            }
            Action::ExportEditableDrawing(enabled) => {
                self.appearance.export_editable_drawing = enabled;
                // The Export tab follows the new default.
                self.export_tab_editable = None;
                self.save_preference(super::settings_page::Section::CopyExport, "export");
            }
            Action::ExportSvgLiveText(enabled) => {
                self.appearance.export_svg_live_text = enabled;
                self.save_preference(super::settings_page::Section::CopyExport, "export");
            }
            Action::Toggle => {
                self.view_open = !self.view_open;
                return focus(if self.view_open {
                    Item::Grid.id()
                } else {
                    "status-view"
                });
            }
            Action::Close => {
                if !self.view_open {
                    return Task::none();
                }
                self.view_open = false;
                return focus("status-view");
            }
            Action::Focus(direction) => {
                if !self.view_open {
                    return Task::none();
                }
                return iced::advanced::widget::operate(Navigate {
                    direction,
                    ids: Item::ALL.iter().map(|item| Id::new(item.id())).collect(),
                    focused: None,
                });
            }
            Action::Choose(item) => {
                if !self.view_open {
                    return Task::none();
                }
                self.view_open = false;
                let message = item.message(self);
                let canvas = self.tab.doc.canvas_theme;
                let task = self.update(message);
                return if matches!(item, Item::PageSetup)
                    || (matches!(item, Item::DarkCanvas) && self.tab.doc.canvas_theme == canvas)
                {
                    task
                } else {
                    Task::batch([task, focus("status-view")])
                };
            }
            Action::Rulers(enabled) => self.set_rulers(enabled),
            Action::Crosshair(enabled) => self.guides.crosshair = enabled,
            Action::RulerUnit(unit) => {
                self.appearance.ruler_unit = unit;
                self.sync_ruler_unit();
                self.save_preference(super::settings_page::Section::Drawing, "ruler units");
            }
        }
        if was_open {
            self.view_open = false;
            focus("status-view")
        } else {
            Task::none()
        }
    }

    pub(super) fn sync_ruler_unit(&mut self) {
        self.guides.unit = self.appearance.ruler_unit;
        self.sync_arrow_label_gap_input();
    }

    pub(super) fn view_menu(&self) -> Element<'_, Message> {
        use super::workspace::{accessible_command, control, horizontal_line, muted_text};
        let anchor = accessible_command("status-view", "Drawing view options", "View")
            .on_press(Message::View(Action::Toggle))
            .expanded(self.view_open)
            .style(control(self.view_open));
        let popup = self.view_open.then(|| {
            let mut items = column![].spacing(1);
            for item in Item::ALL {
                if matches!(item, Item::DarkCanvas | Item::PageSetup) {
                    items = items.push(horizontal_line());
                }
                let shortcut = match item {
                    Item::Rulers => super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, ";"),
                    Item::Crosshair => super::shortcuts::keys(
                        iced::keyboard::Modifiers::COMMAND | iced::keyboard::Modifiers::ALT,
                        "X",
                    ),
                    _ => String::new(),
                };
                let checked = item.checked(self);
                let mut button = reshiki::accessibility::button(
                    item.id(),
                    item.label(),
                    row![
                        text(if checked == Some(true) { "✓" } else { "" })
                            .size(12)
                            .width(14),
                        text(item.label()).size(12).width(Length::Fill),
                        text(shortcut).size(11).style(muted_text),
                    ]
                    .spacing(7)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([7, 9])
                .style(control(false))
                .on_press(Message::View(Action::Choose(item)));
                if let Some(checked) = checked {
                    button = button.checked(checked);
                }
                items = items.push(button);
            }
            container(items)
                .width(240)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        Element::new(
            super::popover::popover(anchor, popup, Message::View(Action::Close))
                .align_end()
                .keys(|key| {
                    let direction = match key {
                        Key::Named(Named::ArrowDown) => Direction::Next,
                        Key::Named(Named::ArrowUp) => Direction::Previous,
                        Key::Named(Named::Home) => Direction::First,
                        Key::Named(Named::End) => Direction::Last,
                        _ => return None,
                    };
                    Some(Message::View(Action::Focus(direction)))
                }),
        )
    }
    pub(super) fn set_viewport(&mut self, size: iced::Size) {
        // Inspector width changes, such as docking NMR, do not request fitting
        // the drawing. Only suppress the expected layout transition; later
        // genuine window resizes keep their ordinary fit behavior.
        let preserve_camera = self.tab.inspector_ui.take_expected_viewport(size);
        if self.viewport != size {
            self.viewport = size;
            if !preserve_camera {
                if let Some(index) = self.tab.pages.fit {
                    self.fit_pages(index);
                } else if self.tab.fit_to_view {
                    self.fit();
                }
            }
        }
    }
    pub(super) fn set_appearance(&mut self, mode: crate::appearance::Mode) {
        self.appearance.mode = mode;
        self.save_preference(super::settings_page::Section::Appearance, "appearance");
    }
    pub(super) fn set_smart_guides(&mut self, enabled: bool) {
        self.appearance.smart_guides = enabled;
        self.save_preference(super::settings_page::Section::Drawing, "smart guides");
    }
    fn set_two_decimal_weight(&mut self, enabled: bool) {
        use reshiki::linked_properties::Property;
        self.appearance.two_decimal_weight = enabled;
        // Mol. Wt. captions in every tab change at once, in one Undo step each,
        // except during a preview or a text draft, and except those with
        // their own decimal places. Other edits leave a
        // caption's precision alone, so an imported caption keeps its text.
        // Syncing a selected caption's text box leaves the inspector as shown.
        let inspector = (self.inspector_open, self.inspector_tab);
        self.each_tab(|app| {
            let tab = &app.tab;
            if !tab.busy
                && !tab.drawing_edit_active()
                && tab.doc.property_captions.iter().any(|link| {
                    link.properties.contains(&Property::Weight)
                        && link.weight_places.is_none()
                        && link.two_decimal_weight != enabled
                })
            {
                let before = app.tab.doc.clone();
                for link in &mut app.tab.doc.property_captions {
                    if link.properties.contains(&Property::Weight) {
                        link.two_decimal_weight = enabled;
                    }
                }
                app.changed(before);
                app.sync_typography();
            }
        });
        (self.inspector_open, self.inspector_tab) = inspector;
        self.save_preference(
            super::settings_page::Section::Names,
            "molecular weight display",
        );
    }
    pub(super) fn set_rulers(&mut self, enabled: bool) {
        self.guides.rulers = enabled;
        if self.tab.fit_to_view {
            self.fit();
        }
    }
    pub(super) fn zoom_by(&mut self, factor: f32) {
        self.tab.pages.fit = None;
        self.tab.fit_to_view = false;
        self.tab.camera.zoom = (self.tab.camera.zoom * factor).clamp(0.005, 5.0);
    }
}

#[cfg(test)]
mod tests;
