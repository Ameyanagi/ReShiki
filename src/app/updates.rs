use super::{App, Message};
use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, opaque, row, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task};
use reshiki::updates::{self, Release};

#[derive(Debug, Clone)]
pub enum Action {
    Show(bool),
    Check(bool),
    Checked(Result<Release, String>),
    Automatic(bool),
    Saved(Result<(), String>),
    Download,
    Opened(Result<(), String>),
}

pub struct State {
    pub open: bool,
    pub automatic: bool,
    checking: bool,
    saving: bool,
    latest: Option<Release>,
    error: Option<String>,
}
impl State {
    pub fn new() -> Self {
        Self {
            open: false,
            automatic: !cfg!(test) && updates::automatic_enabled(),
            checking: false,
            saving: false,
            latest: None,
            error: None,
        }
    }
    pub fn available(&self) -> bool {
        self.latest
            .as_ref()
            .is_some_and(|release| release.newer_than(updates::CURRENT_VERSION))
    }
    pub fn subscription(&self) -> Subscription<Message> {
        if self.automatic {
            iced::time::every(updates::CHECK_INTERVAL)
                .map(|_| Message::Updates(Action::Check(false)))
        } else {
            Subscription::none()
        }
    }
}

impl App {
    pub(super) fn update_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(open) => self.updates.open = open,
            Action::Check(manual) => {
                if self.updates.checking || (!manual && !self.updates.automatic) {
                    return Task::none();
                }
                self.updates.checking = true;
                self.updates.error = None;
                return Task::perform(
                    async move {
                        // Let the first canvas appear before checking a release.
                        if !manual {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        }
                        updates::check(manual).await
                    },
                    |result| Message::Updates(Action::Checked(result)),
                );
            }
            Action::Checked(result) => {
                self.updates.checking = false;
                match result {
                    Ok(release) => self.updates.latest = Some(release),
                    Err(error) => self.updates.error = Some(error),
                }
            }
            Action::Automatic(enabled) => {
                if self.updates.saving {
                    return Task::none();
                }
                self.updates.automatic = enabled;
                self.updates.saving = true;
                return Task::perform(updates::save_automatic(enabled), |result| {
                    Message::Updates(Action::Saved(result))
                });
            }
            Action::Saved(result) => {
                self.updates.saving = false;
                if let Err(error) = result {
                    self.updates.error =
                        Some(format!("Could not save update preferences: {error}"));
                }
            }
            Action::Download => {
                return Task::perform(
                    updates::open_release(self.updates.latest.clone()),
                    |result| Message::Updates(Action::Opened(result)),
                );
            }
            Action::Opened(result) => {
                if let Err(error) = result {
                    self.updates.error = Some(error);
                }
            }
        }
        Task::none()
    }

    pub(super) fn with_updates<'a>(
        &'a self,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if !self.updates.open {
            return content;
        }
        let state = &self.updates;
        let status = if state.checking {
            "Checking for updates…".into()
        } else if let Some(error) = &state.error {
            error.clone()
        } else if let Some(latest) = &state.latest {
            if state.available() {
                format!("ReShiki {} is available.", latest.version)
            } else {
                "You’re up to date.".into()
            }
        } else {
            "Check for the latest stable release.".into()
        };
        let msg = |action| Message::Updates(action);
        let panel = column![
            row![
                text("ReShiki").size(24),
                Space::new().width(Length::Fill),
                button("Close")
                    .on_press(msg(Action::Show(false)))
                    .padding([7, 10])
                    .style(button::text)
            ]
            .align_y(Alignment::Center),
            text(format!("Version {}", updates::CURRENT_VERSION))
                .size(13)
                .color(super::workspace::muted()),
            text(status).size(15),
            row![
                button("Check for updates")
                    .padding([9, 12])
                    .on_press_maybe((!state.checking).then_some(msg(Action::Check(true)))),
                button(if state.available() {
                    "Download update ↗"
                } else {
                    "Downloads ↗"
                })
                .padding([9, 12])
                .on_press(msg(Action::Download))
                .style(super::workspace::control(false))
            ]
            .spacing(10),
            checkbox(state.automatic)
                .label("Check automatically")
                .on_toggle_maybe(
                    (!state.saving)
                        .then_some(|enabled| Message::Updates(Action::Automatic(enabled)))
                )
                .size(16)
                .text_size(13),
            text("Checks once a day. Download and install when you’re ready.")
                .size(12)
                .color(super::workspace::muted()),
        ]
        .spacing(18);
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Color::from_rgba(0., 0., 0., 0.18).into()),
                    ..Default::default()
                }),
        )
        .on_press(msg(Action::Show(false)));
        let dialog = container(opaque(container(panel).padding(24).width(460).style(
            |_| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    color: Color::from_rgb8(201, 212, 207),
                    width: 1.,
                    radius: 12.0.into(),
                },
                ..Default::default()
            },
        )))
        .center_x(Length::Fill)
        .center_y(Length::Fill);
        stack![content, backdrop, dialog].into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_checks_respect_opt_out_and_do_not_change_a_drawing() {
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        let _ = app.update_action(Action::Check(false));
        assert!(!app.updates.checking);
        let _ = app.update_action(Action::Check(true));
        assert!(app.updates.checking);
        let _ = app.update(Message::Updates(Action::Checked(Ok(Release {
            version: "99.0.0".into(),
        }))));
        assert!(app.updates.available());
        assert!(!app.updates.checking);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        let _ = app.update_action(Action::Check(true));
        let _ = app.update_action(Action::Checked(Err("Offline".into())));
        assert!(!app.updates.checking);
        assert!(app.updates.available());
        assert_eq!(app.doc, original);
    }
}
