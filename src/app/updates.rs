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
    Install,
    Poll,
    Prepared(Result<std::sync::Arc<updates::install::Prepared>, String>),
    Restarted(Result<(), String>),
    Opened(Result<(), String>),
}

pub struct State {
    pub open: bool,
    pub automatic: bool,
    checking: bool,
    installing: bool,
    pub restarting: bool,
    progress: String,
    receiver: Option<tokio::sync::mpsc::Receiver<updates::install::Progress>>,
    prepared: Option<std::sync::Arc<updates::install::Prepared>>,
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
            installing: false,
            restarting: false,
            progress: String::new(),
            receiver: None,
            prepared: None,
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
        Subscription::batch([
            if self.automatic {
                iced::time::every(updates::CHECK_INTERVAL)
                    .map(|_| Message::Updates(Action::Check(false)))
            } else {
                Subscription::none()
            },
            if self.installing {
                iced::time::every(std::time::Duration::from_millis(250))
                    .map(|_| Message::Updates(Action::Poll))
            } else {
                Subscription::none()
            },
        ])
    }
}

impl App {
    pub(super) fn update_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(open) => {
                if !self.updates.restarting {
                    self.updates.open = open;
                }
            }
            Action::Poll => {
                if let Some(receiver) = &mut self.updates.receiver {
                    while let Ok(progress) = receiver.try_recv() {
                        self.updates.progress = progress.0;
                    }
                }
            }
            Action::Install => {
                if self.updates.installing || self.updates.restarting {
                    return Task::none();
                }
                if let Some(reason) = self.update_restart_blocker() {
                    self.updates.error = Some(reason.into());
                    return Task::none();
                }
                if self.updates.prepared.is_some() {
                    return self.restart_for_update();
                }
                let Some(release) = self
                    .updates
                    .latest
                    .clone()
                    .filter(|r| r.newer_than(updates::CURRENT_VERSION))
                else {
                    return Task::none();
                };
                self.updates.error = None;
                self.updates.installing = true;
                self.updates.progress = "Downloading update…".into();
                let (sender, receiver) = tokio::sync::mpsc::channel(16);
                self.updates.receiver = Some(receiver);
                return Task::perform(updates::install::prepare(release, sender), |result| {
                    Message::Updates(Action::Prepared(result))
                });
            }
            Action::Prepared(result) => {
                self.updates.installing = false;
                self.updates.receiver = None;
                match result {
                    Ok(prepared) => {
                        self.updates.prepared = Some(prepared);
                        return self.restart_for_update();
                    }
                    Err(error) => self.updates.error = Some(error),
                }
            }
            Action::Restarted(result) => match result {
                Ok(()) => {
                    self.clear_recovery();
                    return iced::exit();
                }
                Err(error) => {
                    self.updates.restarting = false;
                    self.updates.error = Some(error);
                }
            },
            Action::Check(manual) => {
                if self.updates.checking
                    || self.updates.installing
                    || self.updates.restarting
                    || (!manual && !self.updates.automatic)
                {
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

    fn update_restart_blocker(&self) -> Option<&'static str> {
        if self.dirty() {
            Some("Save your drawing, then click Update and restart.")
        } else if self.assistant.has_unfinished_work() {
            Some("Finish or clear the assistant draft and input before restarting.")
        } else if self.busy
            || self.cleanup.is_some()
            || self.joining.is_some()
            || self.atom_text.is_some()
        {
            Some("Finish the current editing operation before restarting.")
        } else {
            None
        }
    }
    fn restart_for_update(&mut self) -> Task<Message> {
        if let Some(reason) = self.update_restart_blocker() {
            self.updates.error = Some(format!("Update ready. {reason}"));
            self.updates.open = true;
            return Task::none();
        }
        let Some(prepared) = self.updates.prepared.clone() else {
            return Task::none();
        };
        self.updates.restarting = true;
        self.updates.open = true;
        self.updates.error = None;
        Task::perform(
            updates::install::handoff(prepared, self.path.clone()),
            |result| Message::Updates(Action::Restarted(result)),
        )
    }

    pub(super) fn with_updates<'a>(
        &'a self,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if !self.updates.open {
            return content;
        }
        let state = &self.updates;
        let status = if state.restarting {
            "Installing and restarting ReShiki…".into()
        } else if state.installing {
            state.progress.clone()
        } else if state.checking {
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
                crate::branding::wordmark(24.0),
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
                    .on_press_maybe((!state.checking && !state.installing && !state.restarting).then_some(msg(Action::Check(true)))),
                button(if state.installing { "Downloading…" } else { "Update and restart" })
                    .padding([9, 12])
                    .on_press_maybe((state.available() && !state.installing && !state.restarting).then_some(msg(Action::Install)))
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
            button("Release notes ↗").on_press(msg(Action::Download)).style(button::text),
            text("Checks once a day. Updates are verified before installation. Your saved drawing reopens after restarting.")
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
    fn pending_atom_label_prevents_update_restart() {
        let (mut app, _) = App::new();
        let atom = app.doc.add_atom("C", reshiki::document::Point::default());
        app.saved = app.doc.clone();
        let _ = app.atom_text_action(super::super::atom_text::Action::Begin(Some(atom)));
        let _ = app.atom_text_action(super::super::atom_text::Action::Input("Boc".into()));
        assert!(!app.dirty());
        assert!(app.update_restart_blocker().is_some());
        let _ = app.restart_for_update();
        assert!(!app.updates.restarting);
        assert!(app.atom_text.is_some());
        let _ = app.atom_text_action(super::super::atom_text::Action::Cancel);
        assert!(app.update_restart_blocker().is_none());
    }

    #[test]
    fn updates_cannot_discard_unsaved_drawing_or_assistant_work() {
        let (mut app, _) = App::new();
        app.updates.latest = Some(Release {
            version: "99.0.0".into(),
        });
        app.doc.add_atom("N", reshiki::document::Point::default());
        let original = app.doc.clone();
        let _ = app.update_action(Action::Install);
        assert!(!app.updates.installing);
        assert!(!app.updates.restarting);
        assert!(app.updates.error.as_ref().unwrap().contains("Save"));
        app.saved = app.doc.clone();
        app.assistant.busy = true;
        let _ = app.update_action(Action::Install);
        assert!(!app.updates.installing);
        assert!(app.updates.error.as_ref().unwrap().contains("assistant"));
        assert_eq!(app.doc, original);
        app.assistant.busy = false;
        assert!(app.update_restart_blocker().is_none());
        app.updates.restarting = true;
        let _ = app.update(Message::Delete);
        assert_eq!(app.doc, original);
    }

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
