//! The GitHub account dialog: sign in with a code, choose repositories, sign out.
use super::{Account, Action, NOT_SET_UP};
use crate::app::{App, Message, workspace::muted_text};
use iced::widget::{Space, column, row, text};
use iced::{Element, Length};

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
            Account::NotConfigured | Account::Requesting => "github-close",
            Account::SignedOut(_) => "github-sign-in",
            Account::Waiting(_) => "github-open-device",
            Account::NeedsInstall { .. } => "github-choose-repos",
            Account::SignedIn { .. } => "github-choose-repos",
        };
        iced::widget::operation::focus(id)
    }

    pub(super) fn account_view(&self) -> Element<'_, Message> {
        let title = text("GitHub").size(20);
        let muted = |line: String| text(line).size(12).style(muted_text);
        let body: Element<'_, Message> = match &self.github.account {
            Account::NotConfigured => muted(NOT_SET_UP.into()).into(),
            Account::SignedOut(note) => column![
                text("Keep drawings in your GitHub repositories.").size(14),
                button(
                    "github-sign-in",
                    "Sign in with GitHub",
                    Action::SignIn,
                    true
                ),
            ]
            .push(note.clone().map(muted))
            .spacing(14)
            .into(),
            Account::Requesting => muted("Asking GitHub for a code…".into()).into(),
            Account::Waiting(code) => column![
                text(code.user_code.clone())
                    .size(32)
                    .font(iced::Font::MONOSPACE),
                text("Code copied. Paste it on GitHub.").size(14),
                row![
                    button(
                        "github-open-device",
                        "Open GitHub ↗",
                        Action::OpenVerification,
                        true
                    ),
                    button("github-copy-code", "Copy code", Action::CopyCode, false),
                    button("github-cancel-sign-in", "Cancel", Action::Close, false),
                ]
                .spacing(8),
                muted("Waiting for GitHub…".into()),
            ]
            .spacing(14)
            .into(),
            Account::NeedsInstall { login } => column![
                text(format!("Signed in as @{login}")).size(14),
                text("Choose which repositories ReShiki can use.").size(14),
                row![
                    button(
                        "github-choose-repos",
                        "Choose repositories ↗",
                        Action::ChooseRepositories,
                        true
                    ),
                    button(
                        "github-check-again",
                        "Check again",
                        Action::CheckAgain,
                        false
                    ),
                ]
                .spacing(8),
                button("github-sign-out", "Sign out", Action::SignOut, false),
            ]
            .spacing(14)
            .into(),
            Account::SignedIn { login, remembered } => {
                let count = self.github.repos.len();
                column![text(format!("Signed in as @{login}")).size(14),]
                    .push((count > 0).then(|| {
                        muted(match count {
                            1 => "1 repository".into(),
                            n => format!("{n} repositories"),
                        })
                    }))
                    .push(
                        row![
                            button(
                                "github-choose-repos",
                                "Choose repositories ↗",
                                Action::ChooseRepositories,
                                false
                            ),
                            button("github-sign-out", "Sign out", Action::SignOut, false),
                        ]
                        .spacing(8),
                    )
                    .push(
                        (!remembered)
                            .then(|| muted("Sign-in won't be remembered on this computer.".into())),
                    )
                    .spacing(14)
                    .into()
            }
        };
        column![
            row![
                title,
                Space::new().width(Length::Fill),
                reshiki::accessibility::button("github-close", "Close", text("Close").size(13))
                    .padding([6, 10])
                    .style(iced::widget::button::text)
                    .on_press(Message::GitHub(Action::Close)),
            ],
            body
        ]
        .spacing(18)
        .into()
    }
}
