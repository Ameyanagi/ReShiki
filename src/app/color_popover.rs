//! Style bar menus: text alignment and the color popover, which holds the theme
//! palette, recent custom colors, a typed color box and the hue editor.
use super::icons::{Glyph, Icon};
use super::popover::popover;
use super::typography::ColorScope;
use super::workspace::{caret, control, hover_hint, muted_text};
use super::{App, Message};
use iced::widget::{Space, button, canvas, column, container, row, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Point, Rectangle, Renderer, Task, Theme};
use reshiki::canvas_theme::CanvasTheme;
use reshiki::color_contrast::{Rgb, contrast};
use reshiki::palette::{Color as Paint, Hue, Hues, Palette, Row, Tones};
use reshiki::theme_files::ThemeFile;
use reshiki::typography::{StyleChange, TextAlign};

const INPUT: &str = "color-input";
const WIDTH: f32 = 336.;
const SWATCH: f32 = 26.;
pub(super) const HINT: &str = "Use #1F4E79, 31, 78, 121 or oklch(0.42 0.09 250)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open or close the alignment menu.
    Align,
    /// Open or close the color popover.
    Color,
    /// Open the color popover for ring interiors.
    RingColor,
    Close,
    /// Focus the color box to type a custom color.
    AddCustom,
    EditHues,
    Slot(Hue),
    /// A chip of the hue strip, −6 to 6 from its middle.
    Chip(i32),
    /// Shift the strip by this many 10° chips.
    Page(i32),
    /// Move the hue by this many 10° steps.
    Step(i32),
    ResetHue,
    RestoreHues,
    Cancel,
    Done,
}

pub(super) enum Menu {
    Align,
    Color {
        /// The typed color could not be read.
        invalid: bool,
        edit: Option<HueEdit>,
    },
}

/// One Edit hues… session: the theme it started from and the working hues.
pub(super) struct HueEdit {
    theme: Option<Box<ThemeFile>>,
    version: u32,
    original: Hues,
    hues: Hues,
    slot: Hue,
    /// Chips the strip is shifted from the current hue.
    shift: i32,
}

/// Messages from the popover itself and background updates keep it open.
pub(super) fn keeps_open(message: &Message) -> bool {
    matches!(
        message,
        Message::StyleMenu(_)
            | Message::ColorScope(_)
            | Message::TextColor(_)
            | Message::ApplyTextColor
            | Message::ClearRingFill
            | Message::TextStyle(StyleChange::Color(_))
            | Message::Canvas(crate::canvas::Edit::Hover(_))
            | Message::InspectorScroll(_)
            | Message::InspectorAction(
                super::inspector::Action::RefreshProperties
                    | super::inspector::Action::PropertiesCalculated(..)
            )
    ) || super::atom_text::background(message)
}

fn edit_keys(key: &iced::keyboard::Key) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};
    match key {
        Key::Named(Named::ArrowLeft) => Some(Message::StyleMenu(Action::Step(-1))),
        Key::Named(Named::ArrowRight) => Some(Message::StyleMenu(Action::Step(1))),
        _ => None,
    }
}

impl App {
    pub(super) fn style_menu_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Align | Action::Color => {
                let open = matches!(
                    (&self.style_menu, action),
                    (Some(Menu::Align), Action::Align) | (Some(Menu::Color { .. }), Action::Color)
                );
                self.close_style_menu();
                if !open {
                    self.style_menu = Some(if action == Action::Align {
                        Menu::Align
                    } else {
                        Menu::Color {
                            invalid: false,
                            edit: None,
                        }
                    });
                    self.sync_color_input();
                }
            }
            Action::RingColor => {
                self.close_style_menu();
                let _ = self.update(Message::ColorScope(ColorScope::Rings));
                self.style_menu = Some(Menu::Color {
                    invalid: false,
                    edit: None,
                });
            }
            Action::Close => self.close_style_menu(),
            Action::AddCustom => {
                return Task::batch([
                    iced::widget::operation::focus(INPUT),
                    iced::widget::operation::select_all(INPUT),
                ]);
            }
            Action::EditHues => {
                let current = self.current_selection_color();
                let hues = Hues::of(&self.doc);
                if let Some(Menu::Color { edit, .. }) = &mut self.style_menu {
                    *edit = Some(HueEdit {
                        theme: self.doc.custom_theme.clone(),
                        version: self.doc.version,
                        original: hues,
                        hues,
                        slot: match current {
                            Some(Paint::Palette(hue, _)) => hue,
                            _ => Hue::Blue,
                        },
                        shift: 0,
                    });
                }
            }
            Action::Cancel => self.end_hue_edit(false),
            Action::Done => self.end_hue_edit(true),
            action => {
                let Some(Menu::Color {
                    edit: Some(edit), ..
                }) = &mut self.style_menu
                else {
                    return Task::none();
                };
                let degrees = i32::from(edit.hues.get(edit.slot));
                let turn = |steps: i32| (degrees + steps * 10).rem_euclid(360) as u16;
                match action {
                    Action::Slot(hue) => {
                        edit.slot = hue;
                        edit.shift = 0;
                    }
                    // The strip stays in place: the clicked chip becomes current.
                    Action::Chip(k) => {
                        edit.hues.set(edit.slot, turn(k + edit.shift));
                        edit.shift = -k;
                    }
                    Action::Page(chips) => edit.shift += chips,
                    Action::Step(steps) => {
                        edit.hues.set(edit.slot, turn(steps));
                        edit.shift -= steps;
                        // Past either end, show the next six chips.
                        if edit.shift > 6 {
                            edit.shift -= 6;
                        } else if edit.shift < -6 {
                            edit.shift += 6;
                        }
                    }
                    Action::ResetHue => {
                        edit.hues.set(edit.slot, edit.slot.default_degrees());
                        edit.shift = 0;
                    }
                    Action::RestoreHues => {
                        edit.hues = Hues::default();
                        edit.shift = 0;
                    }
                    _ => {}
                }
                self.preview_hues();
            }
        }
        Task::none()
    }

    /// Escape leaves the hue editor first, then closes the menu.
    pub(super) fn style_menu_escape(&self) -> Action {
        match self.style_menu {
            Some(Menu::Color { edit: Some(_), .. }) => Action::Cancel,
            _ => Action::Close,
        }
    }

    /// Closing keeps edited hues, as Done does.
    pub(super) fn close_style_menu(&mut self) {
        self.end_hue_edit(true);
        self.style_menu = None;
    }

    pub(super) fn flag_color_input(&mut self, invalid: bool) {
        if let Some(Menu::Color { invalid: flag, .. }) = &mut self.style_menu {
            *flag = invalid;
        }
    }

    /// Recolor the drawing with the working hues. Hues equal to those the
    /// session started with restore its theme exactly.
    fn preview_hues(&mut self) {
        let Some(Menu::Color {
            edit: Some(edit), ..
        }) = &self.style_menu
        else {
            return;
        };
        self.doc.custom_theme = edit.theme.clone();
        self.doc.version = edit.version;
        if edit.hues != edit.original {
            reshiki::palette::set_hues(&mut self.doc, edit.hues);
        }
        self.sync_color_input();
    }

    /// Done keeps the session as one Undo step; Cancel restores its theme.
    fn end_hue_edit(&mut self, keep: bool) {
        let Some(Menu::Color { edit, .. }) = &mut self.style_menu else {
            return;
        };
        let Some(edit) = edit.take() else {
            return;
        };
        if keep {
            let mut before = self.doc.clone();
            before.custom_theme = edit.theme;
            before.version = edit.version;
            if before != self.doc {
                self.changed(before);
                if let Some(theme) = &self.doc.custom_theme {
                    self.status = format!("Hues saved in “{}”", theme.name);
                }
            }
        } else {
            self.doc.custom_theme = edit.theme;
            self.doc.version = edit.version;
        }
        self.sync_color_input();
    }

    /// Ring interiors are the target and every selected ring is unfilled.
    fn rings_unfilled(&self) -> bool {
        self.color_scope == ColorScope::Rings && {
            let fills = self.selected_ring_fills();
            !fills.is_empty() && fills.iter().all(Option::is_none)
        }
    }

    fn theme_name(&self) -> String {
        self.doc
            .custom_theme
            .as_ref()
            .map_or_else(|| self.doc.color_theme.to_string(), |t| t.name.clone())
    }

    pub(super) fn alignment_menu(&self, groups_only: bool) -> Element<'_, Message> {
        const OPTIONS: [(&str, TextAlign); 4] = [
            ("Left", TextAlign::Left),
            ("Center", TextAlign::Center),
            ("Right", TextAlign::Right),
            ("Justify", TextAlign::Justified),
        ];
        let current = self.toolbar_alignment();
        let open = matches!(self.style_menu, Some(Menu::Align));
        let name = OPTIONS
            .iter()
            .find(|(_, align)| Some(*align) == current)
            .map_or("Mixed or automatic", |(name, _)| name);
        let anchor = hover_hint(
            button(
                row![
                    canvas(Glyph(
                        Icon::TextAlign(current.unwrap_or(TextAlign::Left)),
                        current.is_some()
                    ))
                    .width(24)
                    .height(24),
                    caret(9.)
                ]
                .spacing(1)
                .align_y(Alignment::Center),
            )
            .height(36)
            .padding([6, 3])
            .style(control(open))
            .on_press(Message::StyleMenu(Action::Align)),
            format!("Text alignment · {name}"),
            tooltip::Position::Bottom,
        );
        let popup = open.then(|| {
            let items = OPTIONS
                .into_iter()
                .filter(|(_, align)| !(groups_only && *align == TextAlign::Justified))
                .map(|(label, align)| {
                    button(
                        row![
                            canvas(Glyph(Icon::TextAlign(align), true))
                                .width(24)
                                .height(24),
                            text(label).size(12)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([3, 8])
                    .style(control(current == Some(align)))
                    .on_press(Message::TextAlign(align))
                    .into()
                });
            container(column(items).spacing(2))
                .width(140)
                .padding(5)
                .style(surface)
                .into()
        });
        Element::new(popover(anchor, popup, Message::StyleMenu(Action::Close)))
    }

    pub(super) fn color_button(&self) -> Element<'_, Message> {
        let palette = Palette::of(&self.doc);
        let current = self.current_selection_color();
        let canvas_theme = self.doc.canvas_theme;
        // A mixed selection shows an empty chip.
        let shown = current.map(|c| palette.rgb(c));
        let fill = crate::appearance::from_rgb(shown.unwrap_or(canvas_theme.background()));
        let anchor = hover_hint(
            button(
                row![
                    container(Space::new().width(16).height(16)).style(move |_| {
                        container::Style {
                            background: Some(fill.into()),
                            border: Border {
                                color: Color::from_rgba8(128, 128, 128, 0.45),
                                width: 1.,
                                radius: 4.into(),
                            },
                            ..Default::default()
                        }
                    }),
                    caret(9.)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 8])
            .style(crate::appearance::secondary)
            .on_press(Message::StyleMenu(Action::Color)),
            format!(
                "Color · {} · Apply to {}",
                current.map_or_else(
                    || String::from(if self.rings_unfilled() {
                        "No fill"
                    } else {
                        "Mixed"
                    }),
                    Paint::name
                ),
                self.color_scope
            ),
            tooltip::Position::Bottom,
        );
        let (popup, escape, keys): (_, _, fn(&iced::keyboard::Key) -> Option<Message>) =
            match &self.style_menu {
                Some(Menu::Color {
                    edit: Some(edit), ..
                }) => (Some(self.hue_editor(edit)), Action::Cancel, edit_keys),
                Some(Menu::Color { invalid, .. }) => {
                    (Some(self.color_picker(*invalid)), Action::Close, |_| None)
                }
                _ => (None, Action::Close, |_| None),
            };
        Element::new(
            popover(anchor, popup, Message::StyleMenu(Action::Close))
                .on_escape(Message::StyleMenu(escape))
                .keys(keys),
        )
    }

    fn color_picker(&self, invalid: bool) -> Element<'_, Message> {
        let palette = Palette::of(&self.doc);
        let hues = Hues::of(&self.doc);
        let canvas_theme = self.doc.canvas_theme;
        let current = self.current_selection_color();
        let pick = |color: Paint| Message::TextStyle(StyleChange::Color(color));
        let chosen = |color: Paint| (current == Some(color), Some(pick(color)));
        let head = row![
            text("Apply to").size(12).style(muted_text),
            crate::appearance::pick_list(
                ColorScope::ALL,
                Some(self.color_scope),
                Message::ColorScope
            )
            .text_size(12)
            .padding([5, 8])
            .width(Length::Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let no_fill = (
            self.rings_unfilled(),
            (self.color_scope == ColorScope::Rings).then_some(Message::ClearRingFill),
        );
        let mut custom = row![hover_hint(
            button(
                text("+")
                    .size(16)
                    .center()
                    .width(Length::Fill)
                    .color(paper_text(canvas_theme))
            )
            .width(SWATCH)
            .height(SWATCH)
            .padding(0)
            .style(move |_, _| button::Style {
                border: empty_border(),
                ..Default::default()
            })
            .on_press(Message::StyleMenu(Action::AddCustom)),
            "Type a custom color below",
            // Above, so it never covers the box being typed in.
            tooltip::Position::Top,
        )]
        .spacing(SWATCH / 4.);
        for i in 0..reshiki::palette::RECENT_LIMIT {
            custom = custom.push(match self.doc.recent_colors.get(i) {
                Some(&rgb) => {
                    let (active, message) = chosen(Paint::Custom(rgb));
                    swatch(
                        Some(rgb),
                        canvas_theme,
                        SWATCH,
                        active,
                        format!("Custom {} · stays exact", reshiki::palette::hex(rgb)),
                        message,
                    )
                }
                None => container(Space::new().width(SWATCH).height(SWATCH))
                    .style(|_| container::Style {
                        border: empty_border(),
                        ..Default::default()
                    })
                    .into(),
            });
        }
        let swatches = paper(
            column![
                paper_label("STRONG", canvas_theme),
                palette_row(
                    &palette,
                    Row::Strong,
                    hues,
                    SWATCH,
                    chosen(Paint::Ink),
                    |hue| chosen(Paint::Palette(hue, Row::Strong))
                ),
                paper_label("TINT", canvas_theme),
                palette_row(&palette, Row::Tint, hues, SWATCH, no_fill, |hue| {
                    chosen(Paint::Palette(hue, Row::Tint))
                }),
                paper_label("CUSTOM · EXACT ON BOTH CANVASES", canvas_theme),
                custom,
            ]
            .spacing(5),
            canvas_theme,
        );
        let input = row![
            text("Color").size(12).style(muted_text),
            crate::appearance::text_input(
                "#1F4E79 · 31, 78, 121 · oklch(…)",
                &self.text_color_input
            )
            .id(INPUT)
            .on_input(Message::TextColor)
            .on_submit(Message::ApplyTextColor)
            .size(12)
            .padding(6)
            .width(Length::Fill),
            button(text("Apply").size(12))
                .padding([6, 10])
                .style(crate::appearance::secondary)
                .on_press(Message::ApplyTextColor),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut body = column![head, swatches, input].spacing(10);
        if invalid {
            body = body.push(text(HINT).size(11).style(text::danger));
        } else if let Some(Paint::Custom(rgb)) = current {
            let (near, distance) = palette.closest(rgb);
            body = body.push(
                row![
                    text(format!(
                        "Closest palette color: {} · ΔE {distance:.1}",
                        near.name()
                    ))
                    .size(11)
                    .style(muted_text)
                    .width(Length::Fill),
                    hover_hint(
                        link("Use", pick(near)),
                        format!("Use {} instead; it follows the theme", near.name()),
                        tooltip::Position::Bottom,
                    )
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
            // Rounded down, so a ratio shown as 3.0 always passes 3:1.
            let ratio = (contrast(rgb, canvas_theme.background()) * 10.).floor() / 10.;
            let side = canvas_theme.to_string().to_lowercase();
            if ratio < 3. {
                body = body.push(
                    text(format!(
                        "Low contrast on the {side} canvas · {ratio:.1}:1. Lines need 3:1, labels 4.5:1."
                    ))
                    .size(11)
                    .style(text::danger),
                );
            } else if ratio < 4.5 {
                body = body.push(
                    text(format!(
                        "Fine for bonds and lines on the {side} canvas ({ratio:.1}:1). Labels need 4.5:1."
                    ))
                    .size(11)
                    .style(muted_text),
                );
            }
        }
        body = body.push(
            row![
                text(format!("{} · {canvas_theme} canvas", self.theme_name()))
                    .size(11)
                    .style(muted_text)
                    .width(Length::Fill),
                hover_hint(
                    link("Edit hues…", Message::StyleMenu(Action::EditHues)),
                    "Move the theme's hues; the drawing recolors as you go",
                    tooltip::Position::Bottom,
                )
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        container(body)
            .width(WIDTH)
            .padding(12)
            .style(surface)
            .into()
    }

    fn hue_editor(&self, edit: &HueEdit) -> Element<'_, Message> {
        let palette = Palette::of(&self.doc);
        let canvas_theme = self.doc.canvas_theme;
        let slot = edit.slot;
        let degrees = edit.hues.get(slot);
        let theme = edit
            .theme
            .as_ref()
            .map_or_else(|| self.doc.color_theme.to_string(), |t| t.name.clone());
        let select = |hue: Hue| (hue == slot, Some(Message::StyleMenu(Action::Slot(hue))));
        let rows = paper(
            column![
                paper_label("STRONG", canvas_theme),
                palette_row(
                    &palette,
                    Row::Strong,
                    edit.hues,
                    SWATCH,
                    (false, None),
                    select
                ),
                paper_label("TINT", canvas_theme),
                palette_row(
                    &palette,
                    Row::Tint,
                    edit.hues,
                    SWATCH,
                    (false, None),
                    select
                ),
            ]
            .spacing(5),
            canvas_theme,
        );
        let default = slot.default_degrees();
        let detail = row![
            text(format!("{} · {degrees}°", slot.name()))
                .size(12)
                .width(Length::Fill),
            hover_hint(
                button(text(format!("↺ Default {default}°")).size(12))
                    .padding([5, 9])
                    .style(crate::appearance::secondary)
                    .on_press_maybe(
                        (degrees != default).then_some(Message::StyleMenu(Action::ResetHue))
                    ),
                format!("Reset {} to {default}°", slot.name()),
                tooltip::Position::Bottom,
            )
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let tones = Tones::of(&self.doc, canvas_theme);
        let at = |k: i32| (i32::from(degrees) + (k + edit.shift) * 10).rem_euclid(360) as u16;
        let chips = (-6..=6).map(|k| {
            let hue = at(k);
            let band = |row: Row, portion: u16| {
                let rgb = tones.rgb(row, hue);
                container(
                    Space::new()
                        .width(Length::Fill)
                        .height(Length::FillPortion(portion)),
                )
                .style(move |_| flat(rgb))
            };
            let current = hue == degrees;
            hover_hint(
                button(column![band(Row::Strong, 3), band(Row::Tint, 2)])
                    .width(Length::Fill)
                    .height(34)
                    .padding(2)
                    .style(move |theme: &Theme, _| button::Style {
                        border: outline(theme, current, 3.),
                        ..Default::default()
                    })
                    .on_press(Message::StyleMenu(Action::Chip(k))),
                format!("{} → {hue}°", slot.name()),
                tooltip::Position::Bottom,
            )
            .into()
        });
        let nav = |label: &'static str, chips: i32, hint: &'static str| {
            hover_hint(
                button(text(label).size(14).center().width(Length::Fill))
                    .width(20)
                    .height(34)
                    .padding(0)
                    .style(crate::appearance::secondary)
                    .on_press(Message::StyleMenu(Action::Page(chips))),
                hint,
                tooltip::Position::Bottom,
            )
        };
        let strip = paper(
            row![
                nav("‹", -6, "Show hues 60° lower"),
                row(chips).spacing(3).width(Length::Fill),
                nav("›", 6, "Show hues 60° higher"),
            ]
            .spacing(3)
            .align_y(Alignment::Center),
            canvas_theme,
        )
        .padding(6);
        let degree = |k| text(format!("{}°", at(k))).size(11).style(muted_text);
        let scale = row![
            degree(-6),
            Space::new().width(Length::Fill),
            degree(0),
            Space::new().width(Length::Fill),
            degree(6)
        ]
        .padding([0, 26]);
        let preview = |canvas: CanvasTheme| {
            let tones = Tones::of(&self.doc, canvas);
            column![
                iced::widget::canvas(HuePreview {
                    background: canvas.background(),
                    strong: tones.rgb(Row::Strong, degrees),
                    tint: tones.rgb(Row::Tint, degrees),
                })
                .width(Length::Fill)
                .height(44),
                text(format!("{canvas} canvas")).size(11).style(muted_text),
            ]
            .spacing(3)
            .width(Length::Fill)
        };
        let changed = !edit.hues.is_default();
        let note = if !changed {
            format!("Default hues for {theme}.")
        } else if edit.theme.is_none() {
            format!("Saved as “{theme} · custom hues”. The built-in theme stays unchanged.")
        } else {
            format!("Saved with “{theme}” in this drawing.")
        };
        let foot = row![
            button(text("Restore default hues").size(12))
                .padding([6, 10])
                .style(crate::appearance::secondary)
                .on_press_maybe(changed.then_some(Message::StyleMenu(Action::RestoreHues))),
            Space::new().width(Length::Fill),
            button(text("Cancel").size(12))
                .padding([6, 10])
                .style(crate::appearance::secondary)
                .on_press(Message::StyleMenu(Action::Cancel)),
            button(text("Done").size(12))
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press(Message::StyleMenu(Action::Done)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        container(
            column![
                row![
                    text("Edit hues").size(13),
                    text(theme).size(12).style(muted_text)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                rows,
                detail,
                column![strip, scale].spacing(3),
                row![preview(CanvasTheme::Light), preview(CanvasTheme::Dark)].spacing(6),
                foot,
                text(note).size(11).style(muted_text),
            ]
            .spacing(10),
        )
        .width(WIDTH)
        .padding(12)
        .style(surface)
        .into()
    }
}

/// Ink (Strong row) or no fill (Tint row), then the row's eight hues. Each
/// entry is (current, message); a missing message disables the swatch.
pub(super) fn palette_row<'a>(
    palette: &Palette,
    row: Row,
    hues: Hues,
    size: f32,
    first: (bool, Option<Message>),
    hue: impl Fn(Hue) -> (bool, Option<Message>),
) -> Element<'a, Message> {
    let canvas_theme = palette.canvas;
    let (active, message) = first;
    let first = match row {
        Row::Strong => swatch(
            Some(palette.rgb(Paint::Ink)),
            canvas_theme,
            size,
            active,
            "Ink · black on the light canvas, white on the dark one".into(),
            message,
        ),
        Row::Tint => swatch(None, canvas_theme, size, active, "No fill".into(), message),
    };
    let swatches = Hue::ALL.into_iter().map(|h| {
        let (active, message) = hue(h);
        swatch(
            Some(palette.swatch(h, row)),
            canvas_theme,
            size,
            active,
            format!("{} · {} · {}°", h.name(), row.name(), hues.get(h)),
            message,
        )
    });
    iced::widget::row(std::iter::once(first).chain(swatches))
        .spacing(size / 4.)
        .into()
}

/// The palette sits on the canvas color, so Ink and tints read as in the drawing.
pub(super) fn paper<'a>(
    content: impl Into<Element<'a, Message>>,
    canvas_theme: CanvasTheme,
) -> container::Container<'a, Message> {
    let background = crate::appearance::from_rgb(canvas_theme.background());
    container(content)
        .padding([8, 9])
        .width(Length::Fill)
        .style(move |theme| container::Style {
            background: Some(background.into()),
            border: Border {
                color: crate::appearance::themed(theme, Color::from_rgb8(214, 220, 226)),
                width: 1.,
                radius: 8.into(),
            },
            ..Default::default()
        })
}

fn paper_text(canvas_theme: CanvasTheme) -> Color {
    if canvas_theme.is_dark() {
        Color::from_rgba8(227, 235, 232, 0.75)
    } else {
        Color::from_rgba8(23, 33, 30, 0.7)
    }
}

fn paper_label(label: &str, canvas_theme: CanvasTheme) -> iced::widget::Text<'_> {
    text(label).size(10).color(paper_text(canvas_theme))
}

/// A color square; None draws "no color" on the canvas color.
fn swatch_body<'a>(rgb: Option<Rgb>, size: f32, enabled: bool) -> Element<'a, Message> {
    match rgb {
        Some(_) => Space::new().width(size).height(size).into(),
        None => canvas(NoColor(if enabled { 1. } else { 0.3 }))
            .width(size)
            .height(size)
            .into(),
    }
}

fn swatch<'a>(
    rgb: Option<Rgb>,
    canvas_theme: CanvasTheme,
    size: f32,
    current: bool,
    name: String,
    message: Option<Message>,
) -> Element<'a, Message> {
    let fill = crate::appearance::from_rgb(rgb.unwrap_or(canvas_theme.background()));
    hover_hint(
        button(swatch_body(rgb, size, message.is_some()))
            .padding(0)
            .style(move |theme: &Theme, status| button::Style {
                background: Some(
                    if status == button::Status::Disabled {
                        Color { a: 0.3, ..fill }
                    } else {
                        fill
                    }
                    .into(),
                ),
                border: outline(theme, current, 6.),
                ..Default::default()
            })
            .on_press_maybe(message),
        name,
        tooltip::Position::Bottom,
    )
    .into()
}

fn outline(theme: &Theme, current: bool, radius: f32) -> Border {
    Border {
        color: if current {
            theme.palette().primary
        } else {
            Color::from_rgba8(128, 128, 128, 0.45)
        },
        width: if current { 2.5 } else { 1. },
        radius: radius.into(),
    }
}

fn empty_border() -> Border {
    Border {
        color: Color::from_rgba8(128, 128, 128, 0.35),
        width: 1.,
        radius: 6.into(),
    }
}

fn flat(rgb: Rgb) -> container::Style {
    container::Style {
        background: Some(crate::appearance::from_rgb(rgb).into()),
        ..Default::default()
    }
}

fn link(label: &str, message: Message) -> button::Button<'_, Message> {
    button(text(label).size(12))
        .padding([4, 4])
        .style(|theme: &Theme, status| button::Style {
            text_color: if status == button::Status::Hovered {
                theme.palette().text
            } else {
                theme.palette().primary
            },
            ..Default::default()
        })
        .on_press(message)
}

pub(super) fn surface(theme: &Theme) -> container::Style {
    crate::appearance::container(
        theme,
        container::Style {
            background: Some(Color::WHITE.into()),
            border: Border {
                color: Color::from_rgb8(192, 204, 201),
                width: 1.,
                radius: 10.into(),
            },
            shadow: crate::appearance::surface_shadow(iced::Shadow {
                color: Color::from_rgba8(20, 40, 35, 0.18),
                offset: iced::Vector::new(0., 5.),
                blur_radius: 18.,
            }),
            ..Default::default()
        },
    )
}

/// A red diagonal at this opacity: no color.
struct NoColor(f32);
impl<M> canvas::Program<M> for NoColor {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);
        frame.stroke(
            &canvas::Path::line(Point::new(3., h - 3.), Point::new(w - 3., 3.)),
            canvas::Stroke::default()
                .with_width(1.5)
                .with_color(Color::from_rgba8(178, 58, 54, self.0)),
        );
        vec![frame.into_geometry()]
    }
}

/// A tint box, a bond and an N label in one hue on one canvas.
struct HuePreview {
    background: Rgb,
    strong: Rgb,
    tint: Rgb,
}
impl<M> canvas::Program<M> for HuePreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        use crate::appearance::from_rgb;
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let strong = from_rgb(self.strong);
        let paper = canvas::Path::rounded_rectangle(
            Point::new(0.5, 0.5),
            iced::Size::new(bounds.width - 1., bounds.height - 1.),
            6.into(),
        );
        frame.fill(&paper, from_rgb(self.background));
        // Edged like the palette box, so the light canvas shows on the white popover.
        frame.stroke(
            &paper,
            canvas::Stroke::default()
                .with_width(1.)
                .with_color(crate::appearance::themed(
                    theme,
                    Color::from_rgb8(214, 220, 226),
                )),
        );
        frame.fill(
            &canvas::Path::rounded_rectangle(
                Point::new(8., 8.),
                iced::Size::new(44., 28.),
                4.into(),
            ),
            from_rgb(self.tint),
        );
        frame.stroke(
            &canvas::Path::line(Point::new(62., 30.), Point::new(88., 14.)),
            canvas::Stroke::default().with_width(3.).with_color(strong),
        );
        frame.fill_text(canvas::Text {
            content: "N".into(),
            position: Point::new(94., 10.),
            size: 20.into(),
            color: strong,
            ..Default::default()
        });
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::History;

    fn app() -> App {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Benzene.document(42., false);
        app.selected = app.doc.all_ids();
        app.history = History::default();
        app
    }
    fn act(app: &mut App, action: Action) {
        let _ = app.update(Message::StyleMenu(action));
    }
    fn editing(app: &App) -> Option<&HueEdit> {
        match &app.style_menu {
            Some(Menu::Color { edit, .. }) => edit.as_ref(),
            _ => None,
        }
    }

    #[test]
    fn menus_toggle_close_on_other_commands_and_picks_keep_the_popover_open() {
        let mut app = app();
        act(&mut app, Action::Color);
        assert!(matches!(app.style_menu, Some(Menu::Color { .. })));
        act(&mut app, Action::Color);
        assert!(app.style_menu.is_none(), "The color button closes it again");
        act(&mut app, Action::Align);
        act(&mut app, Action::Color);
        assert!(
            matches!(app.style_menu, Some(Menu::Color { .. })),
            "One menu at a time"
        );
        let blue = Paint::Palette(Hue::Blue, Row::Strong);
        let _ = app.update(Message::ColorScope(ColorScope::Bonds));
        let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
        assert!(app.style_menu.is_some(), "Picking keeps the popover open");
        assert!(app.doc.bonds.iter().all(|b| b.color == blue));
        // The edit restarts the delayed property refresh, which must not close it.
        let _ = app.update(Message::InspectorAction(
            super::super::inspector::Action::RefreshProperties,
        ));
        assert!(app.style_menu.is_some());
        let _ = app.update(Message::Escape);
        assert!(app.style_menu.is_none());
        act(&mut app, Action::Align);
        let _ = app.update(Message::TextAlign(TextAlign::Center));
        assert!(
            app.style_menu.is_none(),
            "Choosing an alignment closes its menu"
        );
        act(&mut app, Action::Color);
        let _ = app.update(Message::SelectAll);
        assert!(app.style_menu.is_none(), "Other commands close it");
        act(&mut app, Action::RingColor);
        assert_eq!(app.color_scope, ColorScope::Rings);
        assert!(matches!(app.style_menu, Some(Menu::Color { .. })));
        assert!(app.rings_unfilled(), "No fill is the current swatch");
        let teal = Paint::Palette(Hue::Teal, Row::Tint);
        let _ = app.update(Message::TextStyle(StyleChange::Color(teal)));
        assert_eq!(app.current_selection_color(), Some(teal));
        assert!(!app.rings_unfilled() && app.style_menu.is_some());
        // The atom label editor ignores other messages, so it closes the popover.
        let atom = app.doc.atoms.first().map(|a| a.id);
        let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
            atom,
        )));
        assert!(app.atom_text.is_some() && app.style_menu.is_none());
    }

    #[test]
    fn typed_colors_accept_every_format_and_invalid_text_shows_the_hint() {
        let mut app = app();
        act(&mut app, Action::Color);
        let invalid = |app: &App| matches!(app.style_menu, Some(Menu::Color { invalid: true, .. }));
        let _ = app.update(Message::TextColor("blue-ish".into()));
        let _ = app.update(Message::ApplyTextColor);
        assert!(invalid(&app));
        assert_eq!(app.status, HINT);
        let _ = app.update(Message::TextColor("31, 78, 121".into()));
        assert!(!invalid(&app), "Editing clears the hint");
        let _ = app.update(Message::ApplyTextColor);
        assert_eq!(
            app.current_selection_color(),
            Some(Paint::Custom([31, 78, 121]))
        );
        let _ = app.update(Message::TextColor("oklch(0.62 0.2 30)".into()));
        let _ = app.update(Message::ApplyTextColor);
        let Some(Paint::Custom(rgb)) = app.current_selection_color() else {
            panic!("custom color");
        };
        assert_eq!(app.doc.recent_colors, [rgb, [31, 78, 121]]);
        assert_eq!(app.text_color_input, reshiki::palette::hex(rgb));
        assert!(app.style_menu.is_some());
    }

    #[test]
    fn a_hue_session_recolors_live_and_done_is_one_undo_step() {
        let mut app = app();
        let blue = Paint::Palette(Hue::Blue, Row::Strong);
        let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
        let before = app.doc.clone();
        let shown = Palette::of(&app.doc).rgb(blue);
        act(&mut app, Action::Color);
        act(&mut app, Action::EditHues);
        assert_eq!(editing(&app).map(|e| e.slot), Some(Hue::Blue));
        act(&mut app, Action::Chip(-3));
        assert_eq!(Hues::of(&app.doc).get(Hue::Blue), 225);
        assert_ne!(Palette::of(&app.doc).rgb(blue), shown, "Live recolor");
        assert_eq!(
            app.doc.custom_theme.as_ref().map(|t| t.name.as_str()),
            Some("Publication · custom hues")
        );
        // The strip stays in place: the clicked chip is now the current hue.
        assert_eq!(editing(&app).map(|e| e.shift), Some(3));
        act(&mut app, Action::Step(1));
        assert_eq!(Hues::of(&app.doc).get(Hue::Blue), 235);
        act(&mut app, Action::Slot(Hue::Red));
        act(&mut app, Action::Step(-1));
        assert_eq!(Hues::of(&app.doc).get(Hue::Red), 15);
        act(&mut app, Action::Done);
        assert!(editing(&app).is_none() && app.style_menu.is_some());
        let after = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before, "The whole session is one step");
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
    }

    #[test]
    fn cancel_and_escape_restore_the_theme_and_closing_keeps_edits() {
        let mut app = app();
        let before = app.doc.clone();
        act(&mut app, Action::Color);
        act(&mut app, Action::EditHues);
        act(&mut app, Action::Chip(4));
        assert_ne!(app.doc, before);
        act(&mut app, Action::Cancel);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        act(&mut app, Action::EditHues);
        act(&mut app, Action::Chip(4));
        let _ = app.update(Message::Escape);
        assert_eq!(app.doc, before, "Escape cancels the session");
        assert!(app.style_menu.is_some(), "and returns to picking");
        // Default hues on a built-in theme leave the drawing unchanged.
        act(&mut app, Action::EditHues);
        act(&mut app, Action::Chip(4));
        act(&mut app, Action::RestoreHues);
        act(&mut app, Action::Done);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        // A click outside keeps the edits, as Done does.
        act(&mut app, Action::EditHues);
        act(&mut app, Action::ResetHue);
        act(&mut app, Action::Chip(-6));
        act(&mut app, Action::Close);
        assert!(app.style_menu.is_none());
        assert_eq!(Hues::of(&app.doc).get(Hue::Blue), 195);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn stepping_past_the_strip_ends_shows_the_next_six_chips() {
        let mut app = app();
        act(&mut app, Action::Color);
        act(&mut app, Action::EditHues);
        for _ in 0..7 {
            act(&mut app, Action::Step(1));
        }
        let edit = editing(&app).map(|e| (e.hues.get(Hue::Blue), e.shift));
        // 255° + 70°; the current chip stays within the strip.
        assert_eq!(edit, Some((325, -1)));
        act(&mut app, Action::Page(-6));
        assert_eq!(editing(&app).map(|e| e.shift), Some(-7));
        act(&mut app, Action::Slot(Hue::Red));
        assert_eq!(editing(&app).map(|e| e.shift), Some(0));
        for _ in 0..3 {
            act(&mut app, Action::Step(-1));
        }
        assert_eq!(Hues::of(&app.doc).get(Hue::Red), 355, "Hues wrap at 0°");
    }
}
