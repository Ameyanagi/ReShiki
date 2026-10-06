//! The full-window viewer for an image sent in the conversation.

use super::{Action, card, surface};
use crate::app::{App, Message};
use iced::widget::{Space, column, container, mouse_area, opaque, row, stack, text};
use iced::{Alignment, Color, Element, Length};

impl App {
    pub(in crate::app) fn with_assistant_image<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(source) = &self.assistant.viewed_image else {
            return base;
        };
        let Some(handle) = source.handle(false) else {
            return base;
        };
        let base = reshiki::accessibility::inert(base);
        let close = Message::Assistant(Action::ViewImage(None));
        let popup = container(
            column![
                row![
                    text("Sent image").size(18),
                    Space::new().width(Length::Fill),
                    reshiki::accessibility::button(
                        "image-close",
                        "Close sent image",
                        "Close · Esc"
                    )
                    .on_press(close.clone())
                    .style(super::super::workspace::control(false))
                ]
                .align_y(Alignment::Center),
                iced::widget::image::viewer(handle)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Contain),
                text(format!(
                    "{} × {} · Scroll to zoom · Drag to pan",
                    source.width(),
                    source.height()
                ))
                .size(12)
                .style(super::super::workspace::muted_text)
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme| crate::appearance::container(theme, card()));
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| surface(Color::from_rgba8(20, 30, 30, 0.45), 0.))
                )
                .on_press(close)
            ),
            container(opaque(popup))
                .padding(28)
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .into()
    }
}
