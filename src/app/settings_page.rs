//! Settings: one page for the app-wide preferences. Each row changes the same
//! preference as its control elsewhere in the app, and saves at once.
use super::{
    App, Message,
    help::{Placement, modal},
    workspace::{accessible_checkbox, control, horizontal_line, muted_text},
};
use crate::appearance::Background;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{Space, canvas, column, container, row, scrollable, stack, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Point, Rectangle, Renderer, Size, Task};
use reshiki::canvas_theme::CanvasTheme;
use reshiki::export::Background as Paper;
use reshiki::style::units::Unit;

const SEARCH: &str = "settings-search";
const COLOR: &str = "settings-color";
const PANE: &str = "settings-pane";
/// Saved background colors, newest first.
const SAVED_COLORS: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Appearance,
    Drawing,
    CopyExport,
    Names,
    Assistant,
    Updates,
}

impl Section {
    const ALL: [Self; 6] = [
        Self::Appearance,
        Self::Drawing,
        Self::CopyExport,
        Self::Names,
        Self::Assistant,
        Self::Updates,
    ];

    /// The browser editor has no local assistant and updates with its page.
    fn shown(self) -> bool {
        !cfg!(target_arch = "wasm32") || !matches!(self, Self::Assistant | Self::Updates)
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Drawing => "Drawing",
            Self::CopyExport => "Copy & export",
            Self::Names => "Names & checks",
            Self::Assistant => "Assistant",
            Self::Updates => "Updates",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::Appearance => "The interface, and the canvas that new drawings start with.",
            Self::Drawing => "Guides and controls while you draw.",
            Self::CopyExport => "Pictures that Copy, Copy image, Copy as and file export write.",
            Self::Names => "Chemical names, molecular weight and Check drawing.",
            Self::Assistant => "The model, and how its edits reach the drawing.",
            Self::Updates => "How ReShiki finds new versions.",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Open the page at a section, or at the last one shown.
    Open(Option<Section>),
    Close,
    Section(Section),
    Query(String),
    NewCanvas(CanvasTheme),
    DrawingUnit(Unit),
    CopyBackground(Background),
    ExportBackground(Background),
    /// Use this saved color as Custom.
    Custom([u8; 3]),
    /// Show or hide the box for typing a new color.
    AddColor,
    ColorInput(String),
    SaveColor,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    section: Section,
    query: String,
    /// The color being typed, and whether it could not be read.
    color: Option<(String, bool)>,
}

/// Command+, (Ctrl+, on Windows and Linux).
pub(super) fn is_shortcut(key: &Key, modifiers: Modifiers) -> bool {
    modifiers == Modifiers::COMMAND && matches!(key, Key::Character(c) if c == ",")
}

/// Keys Settings takes before its fields: closing.
pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    (matches!(key, Key::Named(Named::Escape)) || is_shortcut(key, modifiers))
        .then_some(Message::Settings(Action::Close))
}

impl App {
    pub(super) fn settings_action(&mut self, action: Action) -> Task<Message> {
        let settings = &mut self.settings;
        match action {
            Action::Open(section) => {
                settings.open = true;
                settings.section = section.unwrap_or(settings.section);
                settings.query.clear();
                settings.color = None;
                self.help_open = false;
                self.commands.open = false;
                self.palette = None;
                self.context_menu = None;
                return iced::widget::operation::focus(SEARCH);
            }
            Action::Close => {
                settings.open = false;
                settings.color = None;
            }
            Action::Section(section) => {
                settings.section = section;
                settings.query.clear();
                return iced::widget::operation::snap_to(
                    PANE,
                    iced::widget::operation::RelativeOffset::START,
                );
            }
            Action::Query(query) => settings.query = query,
            Action::NewCanvas(theme) => {
                self.appearance.canvas_theme = theme;
                self.save_preference("canvas");
            }
            Action::DrawingUnit(unit) => {
                self.appearance.drawing_style_unit = unit;
                self.save_preference("dimension unit");
                self.sync_drawing_style_unit();
            }
            Action::CopyBackground(background) => {
                self.appearance.copy_background = background;
                self.save_preference("background");
            }
            Action::ExportBackground(background) => {
                self.appearance.export_background = background;
                self.save_preference("background");
            }
            Action::Custom(rgb) => {
                self.appearance.custom_background = rgb;
                self.save_preference("background");
            }
            Action::AddColor => {
                if settings.color.take().is_none() {
                    settings.color = Some((String::new(), false));
                    return iced::widget::operation::focus(COLOR);
                }
            }
            Action::ColorInput(input) => settings.color = Some((input, false)),
            Action::SaveColor => {
                let Some((input, _)) = settings.color.take() else {
                    return Task::none();
                };
                let Some(rgb) = reshiki::palette::parse_color(&input) else {
                    settings.color = Some((input, true));
                    return Task::none();
                };
                let colors = &mut self.appearance.background_colors;
                colors.retain(|saved| *saved != rgb);
                colors.insert(0, rgb);
                colors.truncate(SAVED_COLORS);
                self.appearance.custom_background = rgb;
                self.save_preference("background");
            }
        }
        Task::none()
    }

    fn save_preference(&mut self, name: &str) {
        if let Err(error) = self.appearance.save() {
            self.status = format!("Could not save the {name} preference: {error}");
            self.error = true;
        }
    }

    pub(super) fn with_settings<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.settings.open {
            // The workspace keeps its child position, as with Help.
            return stack![base].into();
        }
        let state = &self.settings;
        let words: Vec<String> = state
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let sections = Section::ALL.into_iter().filter(|s| s.shown());
        let mut nav = column![
            reshiki::accessibility::text_input(
                SEARCH,
                "Search settings",
                "Search settings",
                &state.query
            )
            .style(crate::appearance::input_style)
            .on_input(|query| Message::Settings(Action::Query(query)))
            .size(13)
            .padding(8),
        ]
        .spacing(3)
        .width(190);
        let mut pane = column![].spacing(18);
        let mut found_any = false;
        for section in sections {
            let groups = self.settings_groups(section);
            let matches = groups
                .iter()
                .map(|(title, items)| {
                    items
                        .iter()
                        .filter(|item| found(&words, section, *title, item))
                        .count()
                })
                .sum::<usize>();
            found_any |= matches > 0;
            let current = words.is_empty() && section == state.section;
            nav = nav.push(
                reshiki::accessibility::button(
                    format!("settings-section-{section:?}").to_lowercase(),
                    section.label(),
                    row![
                        text(section.label()).size(13).width(Length::Fill),
                        (!words.is_empty() && matches > 0)
                            .then(|| text(matches.to_string()).size(11).style(muted_text)),
                    ]
                    .align_y(Alignment::Center),
                )
                .checked(current)
                .width(Length::Fill)
                .padding([8, 10])
                .style(control(current))
                .on_press_maybe(
                    (words.is_empty() || matches > 0)
                        .then_some(Message::Settings(Action::Section(section))),
                ),
            );
            if words.is_empty() {
                if section == state.section {
                    pane = pane.push(heading(section.label(), section.summary(), 20));
                    for (title, items) in groups {
                        pane = pane.push(group(title, items));
                    }
                    if section == Section::Updates
                        && let Some(error) = self.updates.error()
                    {
                        pane = pane.push(text(error).size(12).style(text::danger));
                    }
                }
            } else if matches > 0 {
                pane = pane.push(heading(section.label(), "", 15));
                for (title, items) in groups {
                    let items: Vec<_> = items
                        .into_iter()
                        .filter(|item| found(&words, section, title, item))
                        .collect();
                    if !items.is_empty() {
                        pane = pane.push(group(title, items));
                    }
                }
            }
        }
        if !words.is_empty() && !found_any {
            pane = pane.push(
                text("No setting matches. Try background, units or channel.")
                    .size(12)
                    .style(muted_text),
            );
        }
        let close = Message::Settings(Action::Close);
        let page = column![
            row![
                text("Settings").size(22),
                Space::new().width(Length::Fill),
                reshiki::accessibility::button(
                    "settings-close",
                    "Close Settings",
                    text("×").size(25).center()
                )
                .width(32)
                .height(32)
                .padding(0)
                .on_press(close.clone())
                .style(control(false)),
            ]
            .align_y(Alignment::Center),
            row![
                nav,
                scrollable(pane.padding(iced::Padding {
                    right: 14.,
                    ..Default::default()
                }))
                .id(PANE)
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .spacing(24)
            .height(Length::Fill),
            row![
                text("Saved as you change them. They apply to every drawing. Esc closes.")
                    .size(12)
                    .style(muted_text)
                    .width(Length::Fill),
                reshiki::accessibility::button(
                    "settings-done",
                    "Done; close Settings",
                    text("Done").size(13)
                )
                .padding([7, 18])
                .on_press(close.clone())
                .style(control(true)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(16);
        modal(base, page.into(), 880., 640., Placement::Center, close)
    }

    /// The section's groups: an optional title and its rows.
    fn settings_groups(&self, section: Section) -> Vec<(Option<&'static str>, Vec<Item<'_>>)> {
        use super::{object_toolbar, stereo, view_settings::Action as View};
        let a = &self.appearance;
        match section {
            Section::Appearance => vec![(
                None,
                vec![
                    Item::choice(
                        "Interface",
                        "Match canvas follows each drawing's canvas.",
                        choices(
                            "settings-interface",
                            &crate::appearance::Mode::ALL.map(|mode| (mode, mode.to_string())),
                            a.mode,
                            Message::Appearance,
                        ),
                    ),
                    Item::choice(
                        "Canvas for new drawings",
                        "Each drawing keeps its own canvas once made.",
                        choices(
                            "settings-new-canvas",
                            &CanvasTheme::ALL.map(|theme| (theme, theme.to_string())),
                            a.canvas_theme,
                            |theme| Message::Settings(Action::NewCanvas(theme)),
                        ),
                    ),
                ],
            )],
            Section::Drawing => vec![(
                None,
                vec![
                    Item::check(
                        "settings-smart-guides",
                        "Smart guides",
                        "Snap dragged objects to other objects' edges, centers and equal gaps. Hold Option/Alt to move freely.",
                        a.smart_guides,
                        Message::View(View::SmartGuides(!a.smart_guides)),
                    ),
                    Item::check(
                        "settings-arrange-controls",
                        "Arrange controls",
                        "Align, distribute, order, flip and rotate at the right end of the Select row.",
                        a.arrange_controls,
                        Message::ObjectToolbar(object_toolbar::Action::Visible(
                            !a.arrange_controls,
                        )),
                    ),
                    Item::choice(
                        "Drawing Style units",
                        "Bond length, line width and the other size fields.",
                        choices(
                            "settings-units",
                            &Unit::ALL.map(|unit| (unit, unit.to_string())),
                            a.drawing_style_unit,
                            |unit| Message::Settings(Action::DrawingUnit(unit)),
                        ),
                    ),
                ],
            )],
            Section::CopyExport => vec![
                (
                    Some("Background · PNG, SVG, PDF and EMF"),
                    vec![
                        Item::wide(
                            "Copied pictures",
                            "Copy, Copy image and Copy as.",
                            background_choices(
                                "settings-copy-background",
                                a.copy_background,
                                a.custom_background,
                                |background| Message::Settings(Action::CopyBackground(background)),
                                false,
                            ),
                        ),
                        Item::wide(
                            "Exported files",
                            "Export in the Export tab.",
                            background_choices(
                                "settings-export-background",
                                a.export_background,
                                a.custom_background,
                                |background| {
                                    Message::Settings(Action::ExportBackground(background))
                                },
                                false,
                            ),
                        ),
                        Item::wide(
                            "Your colors",
                            "Pick one to use it as Custom. On white, black or a color, black and white ink swap so a dark drawing stays readable; colored atoms keep their colors. Canvas keeps the drawing's own colors.",
                            self.saved_colors(),
                        ),
                    ],
                ),
                (
                    Some("Inside the file"),
                    vec![
                        Item::check(
                            "settings-editable-drawing",
                            "Put the editable drawing in pictures",
                            "Paste or open the picture in ReShiki to get the drawing back. Anyone who gets the picture can do the same. Copying between ReShiki windows keeps the drawing either way.",
                            a.export_editable_drawing,
                            Message::View(View::ExportEditableDrawing(!a.export_editable_drawing)),
                        ),
                        Item::check(
                            "settings-svg-text",
                            "Keep text editable in SVG files",
                            "Labels stay text in illustration apps, but each app draws them with its own font. SVG for Office and copied SVG always use outlines.",
                            a.export_svg_live_text,
                            Message::View(View::ExportSvgLiveText(!a.export_svg_live_text)),
                        ),
                    ],
                ),
            ],
            Section::Names => vec![(
                None,
                vec![
                    Item::check(
                        "settings-name-typography",
                        "Chemical name typography",
                        "Italic descriptors, primes and superscripts in new name captions.",
                        a.name_typography,
                        Message::View(View::NameTypography(!a.name_typography)),
                    ),
                    Item::choice(
                        "Molecular weight",
                        "Exact mass keeps 5 decimal places.",
                        choices(
                            "settings-weight",
                            &[(false, "3 decimals".into()), (true, "2 decimals".into())],
                            a.two_decimal_weight,
                            |two| Message::View(View::TwoDecimalWeight(two)),
                        ),
                    ),
                    Item::check(
                        "settings-iupac-notes",
                        "IUPAC style notes in Check drawing",
                        "Advisory notes on stereo drawing conventions.",
                        a.iupac_style_notes,
                        Message::Stereo(stereo::Action::IupacStyleNotes(!a.iupac_style_notes)),
                    ),
                    Item::check(
                        "settings-unknown-stereo",
                        "Write Ξ/ξ for unknown configuration",
                        "In generated names of atoms marked as explicitly unknown.",
                        a.write_unknown_stereo,
                        Message::Naming(super::naming::Action::WriteUnknownStereo(
                            !a.write_unknown_stereo,
                        )),
                    ),
                ],
            )],
            Section::Assistant => vec![(None, self.assistant_items())],
            Section::Updates => vec![(None, self.update_items())],
        }
    }

    fn saved_colors(&self) -> Element<'_, Message> {
        let a = &self.appearance;
        let mut colors = row![].spacing(8).align_y(Alignment::Center);
        for &rgb in &a.background_colors {
            let current = rgb == a.custom_background;
            let hex = reshiki::palette::hex(rgb);
            colors = colors.push(super::workspace::hover_hint(
                reshiki::accessibility::button(
                    format!("settings-color-{hex}"),
                    format!("Use {hex} as Custom"),
                    swatch(Paper::Color(rgb), 22., 22.),
                )
                .checked(current)
                .padding(2)
                .style(move |theme, status| {
                    let mut style = control(current)(theme, status);
                    style.border.radius = 7.0.into();
                    style
                })
                .on_press(Message::Settings(Action::Custom(rgb))),
                hex,
                tooltip::Position::Bottom,
            ));
        }
        let adding = self.settings.color.as_ref();
        colors = colors.push(super::workspace::hover_hint(
            reshiki::accessibility::button(
                "settings-add-color",
                "Add a color",
                text("+").size(16).center().width(22).height(22),
            )
            .checked(adding.is_some())
            .padding(2)
            .style(control(adding.is_some()))
            .on_press(Message::Settings(Action::AddColor)),
            "Add a color",
            tooltip::Position::Bottom,
        ));
        let mut body = column![colors].spacing(8);
        if let Some((input, invalid)) = adding {
            body = body.push(
                row![
                    reshiki::accessibility::text_input(
                        COLOR,
                        "New background color: hexadecimal, RGB or OKLCH",
                        "#1F4E79 · 31, 78, 121 · oklch(…)",
                        input
                    )
                    .style(crate::appearance::input_style)
                    .on_input(|input| Message::Settings(Action::ColorInput(input)))
                    .on_submit(Message::Settings(Action::SaveColor))
                    .size(12)
                    .padding(6)
                    .width(260),
                    reshiki::accessibility::button(
                        "settings-save-color",
                        "Add this color",
                        text("Add").size(12)
                    )
                    .padding([6, 10])
                    .style(crate::appearance::secondary)
                    .on_press(Message::Settings(Action::SaveColor)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
            if *invalid {
                body = body.push(
                    text(super::color_popover::HINT)
                        .size(11)
                        .style(text::danger),
                );
            }
        }
        body.into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn assistant_items(&self) -> Vec<Item<'_>> {
        use super::assistant::Action;
        /// A model choice; `None` is the default model.
        #[derive(Clone, PartialEq)]
        struct Model(Option<String>, String);
        impl std::fmt::Display for Model {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.1)
            }
        }
        let choices_now = self.assistant_choices();
        let models: Vec<_> = std::iter::once(Model(None, "Default · GPT-6 Astra".into()))
            .chain(
                choices_now
                    .models
                    .iter()
                    .map(|m| Model(Some(m.id.clone()), m.label.clone())),
            )
            .collect();
        let selected = models
            .iter()
            .find(|m| m.0.as_deref() == choices_now.model)
            .cloned();
        let effort: Element<'_, Message> = match choices_now.resolved {
            Some(model) => choices(
                "settings-effort",
                &model
                    .efforts
                    .iter()
                    .map(|e| (e.id.clone(), e.label().to_owned()))
                    .collect::<Vec<_>>(),
                choices_now.effort.unwrap_or_default().to_owned(),
                |id| Message::Assistant(Action::Effort(id)),
            ),
            None => text("Connect the assistant to see its reasoning levels.")
                .size(12)
                .style(muted_text)
                .into(),
        };
        vec![
            Item::choice(
                "Model",
                if choices_now.models.is_empty() {
                    "Connect the assistant to load your available models."
                } else {
                    "Default uses GPT-6 Astra when available; otherwise your account default."
                },
                crate::appearance::close_on_escape(
                    crate::appearance::pick_list(models, selected, |model: Model| {
                        Message::Assistant(Action::Model(model.0))
                    })
                    .text_size(12)
                    .padding([6, 10])
                    .width(220),
                ),
            ),
            Item::choice("Reasoning", "For the model in use.", effort),
            Item::check(
                "settings-auto-apply",
                "Accept all edits",
                "Apply valid changes to the canvas automatically. Every change can be undone.",
                choices_now.auto_apply,
                Message::Assistant(Action::AutoApply(!choices_now.auto_apply)),
            ),
        ]
    }

    #[cfg(target_arch = "wasm32")]
    fn assistant_items(&self) -> Vec<Item<'_>> {
        Vec::new()
    }

    fn update_items(&self) -> Vec<Item<'_>> {
        use super::updates::Action;
        use reshiki::updates::Channel;
        let state = &self.updates;
        let mut channel = row![].spacing(2);
        for option in Channel::ALL {
            let current = option == state.channel;
            channel = channel.push(
                reshiki::accessibility::button(
                    format!("settings-channel-{}", option.to_string().to_lowercase()),
                    format!("{option} update channel"),
                    text(option.to_string()).size(12),
                )
                .checked(current)
                .padding([5, 11])
                .style(control(current))
                .on_press_maybe(
                    state
                        .idle()
                        .then_some(Message::Updates(Action::Channel(option))),
                ),
            );
        }
        vec![
            Item {
                name: "Check for updates automatically",
                detail: "Once a day. Stable updates are verified before installation.",
                kind: Kind::Check(
                    accessible_checkbox(
                        "settings-update-check",
                        "Check for updates automatically",
                        state.automatic,
                        (!state.saving())
                            .then_some(Message::Updates(Action::Automatic(!state.automatic))),
                    )
                    .into(),
                ),
            },
            Item::choice(
                "Channel",
                "Nightly builds have the newest changes and are installed by hand.",
                segmented(channel),
            ),
        ]
    }
}

/// One setting: its name, a line on what it does, and its control.
struct Item<'a> {
    name: &'static str,
    detail: &'static str,
    kind: Kind<'a>,
}

enum Kind<'a> {
    /// A checkbox labeled with the name.
    Check(Element<'a, Message>),
    /// A control to the right of the name.
    Choice(Element<'a, Message>),
    /// A control under the name.
    Wide(Element<'a, Message>),
}

impl<'a> Item<'a> {
    fn check(
        id: &'static str,
        name: &'static str,
        detail: &'static str,
        checked: bool,
        toggle: Message,
    ) -> Self {
        Self {
            name,
            detail,
            kind: Kind::Check(accessible_checkbox(id, name, checked, Some(toggle)).into()),
        }
    }
    fn choice(
        name: &'static str,
        detail: &'static str,
        control: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            name,
            detail,
            kind: Kind::Choice(control.into()),
        }
    }
    fn wide(
        name: &'static str,
        detail: &'static str,
        control: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            name,
            detail,
            kind: Kind::Wide(control.into()),
        }
    }
    fn view(self) -> Element<'a, Message> {
        let detail = text(self.detail).size(11).style(muted_text);
        let label = column![text(self.name).size(13), detail].spacing(3);
        match self.kind {
            // The detail lines up with the checkbox's label.
            Kind::Check(check) => column![
                check,
                container(text(self.detail).size(11).style(muted_text)).padding(iced::Padding {
                    left: 22.,
                    ..Default::default()
                })
            ]
            .spacing(3)
            .into(),
            Kind::Choice(control) => row![label.width(Length::Fill), control]
                .spacing(16)
                .align_y(Alignment::Center)
                .into(),
            Kind::Wide(control) => column![label, control].spacing(9).into(),
        }
    }
}

/// Whether every search word is in the setting's section, group, name or detail.
fn found(words: &[String], section: Section, group: Option<&str>, item: &Item<'_>) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        section.label(),
        group.unwrap_or_default(),
        item.name,
        item.detail
    )
    .to_lowercase();
    words.iter().all(|word| haystack.contains(word.as_str()))
}

fn heading<'a>(title: &'a str, summary: &'a str, size: u32) -> Element<'a, Message> {
    let mut heading = column![text(title).size(size)].spacing(4);
    if !summary.is_empty() {
        heading = heading.push(text(summary).size(12).style(muted_text));
    }
    heading.into()
}

/// Rows in a bordered group, under an optional title.
fn group<'a>(title: Option<&'static str>, items: Vec<Item<'a>>) -> Element<'a, Message> {
    let mut body = column![].spacing(0);
    if let Some(title) = title {
        body = body.push(container(text(title).size(13)).padding([10, 0]));
    }
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            body = body.push(horizontal_line());
        }
        body = body.push(container(item.view()).padding([8, 0]));
    }
    container(body)
        .padding([6, 16])
        .width(Length::Fill)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    border: Border {
                        color: Color::from_rgb8(224, 228, 233),
                        width: 1.,
                        radius: 10.into(),
                    },
                    ..Default::default()
                },
            )
        })
        .into()
}

/// Options in one rounded strip; the chosen one is highlighted.
fn segmented<'a>(options: iced::widget::Row<'a, Message>) -> Element<'a, Message> {
    container(options.align_y(Alignment::Center))
        .padding(3)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::from_rgb8(242, 244, 246).into()),
                    border: Border {
                        color: Color::from_rgb8(224, 228, 233),
                        width: 1.,
                        radius: 8.into(),
                    },
                    ..Default::default()
                },
            )
        })
        .into()
}

/// A segmented choice between labeled values.
fn choices<'a, T: Clone + PartialEq + 'a>(
    id: &str,
    options: &[(T, String)],
    selected: T,
    message: impl Fn(T) -> Message,
) -> Element<'a, Message> {
    let mut strip = row![].spacing(2);
    for (value, label) in options {
        let current = *value == selected;
        strip = strip.push(
            reshiki::accessibility::button(
                format!("{id}-{}", label.to_lowercase().replace(' ', "-")),
                label.clone(),
                text(label.clone()).size(12),
            )
            .checked(current)
            .padding([5, 11])
            .style(control(current))
            .on_press(message(value.clone())),
        );
    }
    segmented(strip)
}

/// The five backgrounds with their swatches. `tiles` shows swatches alone in
/// equal columns, named on hover, for the narrow Export tab; its caller names
/// the chosen one.
pub(super) fn background_choices<'a>(
    id: &str,
    selected: Background,
    custom: [u8; 3],
    message: impl Fn(Background) -> Message,
    tiles: bool,
) -> Element<'a, Message> {
    let mut strip = row![].spacing(if tiles { 4 } else { 2 });
    for background in [
        Background::Transparent,
        Background::Canvas,
        Background::White,
        Background::Black,
        Background::Custom,
    ] {
        let current = background == selected;
        let label = background.label();
        let name = match background {
            Background::Custom => format!("Custom, {}", reshiki::palette::hex(custom)),
            _ => label.to_owned(),
        };
        let paper = background.paper(custom);
        let content: Element<'a, Message> = if tiles {
            container(swatch(paper, 26., 18.))
                .center_x(Length::Fill)
                .into()
        } else {
            row![swatch(paper, 12., 12.), text(label).size(12)]
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        };
        let name_for_hint = name.clone();
        let item =
            reshiki::accessibility::button(format!("{id}-{}", label.to_lowercase()), name, content)
                .checked(current)
                .padding(if tiles { [7, 0] } else { [5, 10] })
                .style(control(current))
                .on_press(message(background));
        let item: Element<'a, Message> = if tiles {
            super::workspace::hover_hint(
                item.width(Length::FillPortion(1)),
                name_for_hint,
                tooltip::Position::Bottom,
            )
            .into()
        } else {
            item.into()
        };
        strip = strip.push(item);
    }
    if tiles {
        strip.width(Length::Fill).into()
    } else {
        segmented(strip)
    }
}

/// A picture background: a checkerboard for transparent, light and dark
/// halves for the canvas, otherwise the color itself.
pub(super) fn swatch<'a>(paper: Paper, width: f32, height: f32) -> Element<'a, Message> {
    canvas(Swatch(paper)).width(width).height(height).into()
}

struct Swatch(Paper);

impl canvas::Program<Message> for Swatch {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        use canvas::{Path, Stroke};
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let size = Size::new(bounds.width - 1., bounds.height - 1.);
        let outline = Path::rounded_rectangle(Point::new(0.5, 0.5), size, 3.0.into());
        match self.0 {
            Paper::Transparent => {
                frame.fill(&outline, Color::WHITE);
                let cell = (bounds.height / 3.).max(3.);
                let grey = Color::from_rgb8(201, 206, 211);
                let (columns, rows) = (
                    (bounds.width / cell).ceil() as usize,
                    (bounds.height / cell).ceil() as usize,
                );
                for y in 0..rows {
                    for x in (y % 2..columns).step_by(2) {
                        let at = Point::new(x as f32 * cell, y as f32 * cell);
                        let cell = Size::new(
                            cell.min(bounds.width - at.x - 1.),
                            cell.min(bounds.height - at.y - 1.),
                        );
                        frame.fill_rectangle(Point::new(at.x + 1., at.y + 1.), cell, grey);
                    }
                }
            }
            Paper::Canvas => {
                frame.fill(&outline, Color::WHITE);
                let dark = Path::new(|p| {
                    p.move_to(Point::new(bounds.width - 0.5, 0.5));
                    p.line_to(Point::new(bounds.width - 0.5, bounds.height - 0.5));
                    p.line_to(Point::new(0.5, bounds.height - 0.5));
                    p.close();
                });
                frame.fill(&dark, Color::from_rgb8(35, 39, 43));
            }
            Paper::Color(rgb) => frame.fill(&outline, crate::appearance::from_rgb(rgb)),
        }
        frame.stroke(
            &outline,
            Stroke::default()
                .with_width(1.)
                .with_color(Color::from_rgba8(110, 120, 128, 0.55)),
        );
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
