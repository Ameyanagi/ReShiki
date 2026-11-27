//! Text lines in the xTB card and results palette: muted, good, warning or error.
use crate::app::Message;
use crate::app::workspace::muted_text;
use iced::widget::text;
use iced::{Color, Element, Theme};

/// A line in the card's tone: muted, good, warning or error.
#[derive(Clone, Copy)]
pub(super) enum Tone {
    Muted,
    Good,
    Warning,
    Error,
}

pub(super) fn line<'a>(content: impl Into<String>, tone: Tone) -> Element<'a, Message> {
    toned_text(content, tone).into()
}

pub(super) fn toned_text<'a>(content: impl Into<String>, tone: Tone) -> text::Text<'a> {
    text(content.into()).size(11).style(move |theme: &Theme| {
        let seed = match tone {
            Tone::Muted => return muted_text(theme),
            Tone::Good => theme.palette().success,
            Tone::Warning => theme.palette().warning,
            Tone::Error => crate::appearance::themed(theme, Color::from_rgb8(168, 52, 47)),
        };
        text::Style {
            color: Some(crate::appearance::readable(
                seed,
                &[theme.palette().background],
                reshiki::color_contrast::TEXT_TARGET,
            )),
        }
    })
}
