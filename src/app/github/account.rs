//! The sign-in dialog: the code, large, copied already; click it to copy it
//! again. It closes by itself once GitHub approves.
use super::{Account, Action};
use crate::app::{App, Message, workspace::muted_text};
use iced::widget::{column, row, text};
use iced::{Alignment, Element};

fn button<'a>(
    id: &'static str,
    label: &'static str,
    action: Action,
    primary: bool,
) -> reshiki::accessibility::Button<'a, Message> {
    reshiki::accessibility::button(id, label, text(label).size(13))
        .padding([8, 14])
        .style(if primary {
            crate::appearance::primary
        } else {
            crate::appearance::secondary
        })
        .on_press(Message::GitHub(action))
}

impl App {
    /// Focus on the button the user most likely wants next.
    pub(super) fn account_focus(&self) -> iced::Task<Message> {
        let id = match &self.github.account {
            Account::Waiting(_) => "github-open-device",
            Account::SignedOut(Some(_)) => "github-sign-in",
            _ => "github-cancel-sign-in",
        };
        iced::widget::operation::focus(id)
    }

    pub(super) fn account_view(&self) -> Element<'_, Message> {
        let cancel = button("github-cancel-sign-in", "Cancel", Action::Close, false);
        let body: Element<'_, Message> = match &self.github.account {
            Account::Waiting(code) => column![
                reshiki::accessibility::button(
                    "github-code",
                    format!("Code {}, copy again", code.user_code),
                    text(code.user_code.clone())
                        .size(34)
                        .font(iced::Font::MONOSPACE),
                )
                .padding([2, 6])
                .style(iced::widget::button::text)
                .on_press(Message::GitHub(Action::CopyCode)),
                text("Copied. Paste it on GitHub.").size(13),
                row![
                    button(
                        "github-open-device",
                        "Open GitHub ↗",
                        Action::OpenVerification,
                        true
                    ),
                    cancel,
                ]
                .spacing(8),
            ]
            .spacing(14)
            .align_x(Alignment::Center)
            .into(),
            // Why the last sign-in ended, such as an expired code.
            Account::SignedOut(Some(note)) => column![
                text(note.clone()).size(13),
                row![
                    button("github-sign-in", "Sign in…", Action::SignIn, true),
                    cancel
                ]
                .spacing(8),
            ]
            .spacing(14)
            .align_x(Alignment::Center)
            .into(),
            _ => column![
                text("Asking GitHub for a code…").size(13).style(muted_text),
                cancel,
            ]
            .spacing(14)
            .align_x(Alignment::Center)
            .into(),
        };
        iced::widget::container(body)
            .center_x(iced::Length::Fill)
            .into()
    }
}
