//! Journal dimensions and color theme share one status-bar chooser.
use super::{App, Message, document_styles, theme_files};
use iced::widget::{column, container, row, text, tooltip};
use iced::{Element, Length, Task};
use reshiki::document_styles::Preset;
use unicode_segmentation::UnicodeSegmentation;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Toggle,
    Close,
}

pub(super) fn keeps_open(message: &Message) -> bool {
    super::results::async_result(message).is_some()
        || matches!(
            message,
            Message::StatusStyle(_)
                | Message::QuickDrawingStyle(_)
                | Message::ThemeFile(theme_files::Action::Choose(_))
                | Message::ColorTheme(_)
                | Message::Escape
                | Message::Tick
                | Message::Viewport(_)
                | Message::InspectorScroll(_)
                | Message::Canvas(
                    crate::canvas::Edit::Hover(_) | crate::canvas::Edit::PointerHover(_)
                )
                | Message::Nmr(super::nmr::Action::RenderedSize(_))
                | Message::Stereo(super::stereo::Action::Pulse)
                | Message::InspectorAction(super::inspector::Action::RefreshPropertiesFor(_))
                | Message::Updates(
                    super::updates::Action::Poll | super::updates::Action::Check(false)
                )
        )
}

fn focus(id: &str) -> Task<Message> {
    iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(id)).discard()
}

/// Keep imported theme names from squeezing the rest of the footer out of view.
/// The chooser and accessibility name retain the full values.
fn compact(label: &str) -> String {
    const WIDTH: f32 = 184.;
    if super::workspace::text_width(label, 11.) <= WIDTH {
        return label.into();
    }
    let mut result = String::new();
    for grapheme in label.graphemes(true) {
        let next = format!("{result}{grapheme}…");
        if super::workspace::text_width(&next, 11.) > WIDTH {
            break;
        }
        result.push_str(grapheme);
    }
    result.push('…');
    result
}

impl App {
    fn status_journal(&self) -> document_styles::Choice {
        Preset::ALL
            .into_iter()
            .find(|preset| preset.style() == self.tab.doc.drawing_style)
            .map(document_styles::Choice::Journal)
            .unwrap_or(document_styles::Choice::Custom)
    }

    pub(super) fn status_style_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Toggle => {
                self.status_style_open = !self.status_style_open;
                focus(if self.status_style_open {
                    "journal-preset"
                } else {
                    "status-style"
                })
            }
            Action::Close if self.status_style_open => {
                self.status_style_open = false;
                focus("status-style")
            }
            Action::Close => Task::none(),
        }
    }

    pub(super) fn status_style(&self) -> Element<'_, Message> {
        use document_styles::Choice;
        use theme_files::Choice as ThemeChoice;
        let current = self.status_journal();
        let (themes, theme) = self.theme_choices();
        let journal = if current == Choice::Journal(Preset::Jacs) {
            "JACS".into()
        } else {
            current.to_string()
        };
        let label = compact(&format!("{journal} · {theme}"));
        let name = format!("Journal preset and color theme: {current} · {theme}");
        let anchor = super::popover::choice_anchor(
            "status-style",
            name.clone(),
            label.clone(),
            11.,
            [4, 8],
            self.status_style_open,
            Message::StatusStyle(Action::Toggle),
        )
        .width(super::workspace::text_width(&label, 11.).ceil() + 29.);
        let popup = self.status_style_open.then(|| {
            let themes_only: Vec<_> = themes
                .iter()
                .filter(|choice| **choice != ThemeChoice::Manage)
                .cloned()
                .collect();
            // These are the original independent selectors. Their leaf messages
            // still pass through the editor's ordinary modal and draft gates.
            let journal = reshiki::accessibility::select(
                "journal-preset",
                "Journal preset",
                Preset::ALL.to_vec(),
                current.to_string(),
                |preset| Message::QuickDrawingStyle(Choice::Journal(preset)),
                crate::appearance::close_on_escape(
                    crate::appearance::pick_list(
                        Preset::ALL
                            .into_iter()
                            .map(Choice::Journal)
                            .collect::<Vec<_>>(),
                        Some(current),
                        Message::QuickDrawingStyle,
                    )
                    .width(Length::Fill)
                    .text_size(12)
                    .padding([6, 8]),
                ),
            );
            let colors = reshiki::accessibility::select(
                "color-theme",
                "Color theme",
                themes_only.clone(),
                theme.to_string(),
                |choice| Message::ThemeFile(theme_files::Action::Choose(choice)),
                crate::appearance::close_on_escape(
                    crate::appearance::pick_list(themes_only, Some(theme.clone()), |choice| {
                        Message::ThemeFile(theme_files::Action::Choose(choice))
                    })
                    .placeholder(theme.to_string())
                    .width(Length::Fill)
                    .text_size(12)
                    .padding([6, 8]),
                ),
            );
            reshiki::accessibility::focus_scope(
                container(
                    column![
                        text("Journal preset")
                            .size(11)
                            .style(super::workspace::muted_text),
                        journal,
                        text("Color theme")
                            .size(11)
                            .style(super::workspace::muted_text),
                        colors,
                        row![
                            super::settings_page::text_button(
                                "status-manage-styles",
                                "Manage styles…",
                                "Manage styles…",
                                Some(Message::QuickDrawingStyle(Choice::Details)),
                            ),
                            super::settings_page::text_button(
                                "status-manage-themes",
                                "Manage themes…",
                                "Manage themes…",
                                Some(Message::ThemeFile(theme_files::Action::Choose(
                                    ThemeChoice::Manage
                                ))),
                            ),
                        ]
                        .spacing(12),
                    ]
                    .spacing(6),
                )
                .width(280)
                .padding(12)
                .style(super::color_popover::surface),
            )
        });
        Element::new(
            super::popover::popover(
                super::workspace::hover_hint(anchor, name, tooltip::Position::Top),
                popup,
                Message::StatusStyle(Action::Close),
            )
            .align_end()
            .capture_plain_keys(),
        )
    }
}
