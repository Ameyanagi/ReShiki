use super::{App, Message};
use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, opaque, row, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task};
use reshiki::updates::{self, Channel, Preferences, Release};

#[derive(Debug, Clone)]
pub enum Action {
    Show(bool),
    Check(bool),
    Checked(u64, Result<Release, String>),
    Channel(Channel),
    Portable,
    Automatic(bool),
    Saved(Result<(), String>),
    Download,
    Install,
    Poll,
    Prepared(Result<std::sync::Arc<updates::install::Prepared>, String>),
    RecoveryCleared,
    Restarted(Result<(), String>),
    Opened(Result<(), String>),
}

/// Already-started work and window/save lifecycle events keep running while
/// the dialog owns input. Document results retain their usual epoch guards.
pub(super) fn background(message: &Message) -> bool {
    super::tabs::document_result(message)
        || matches!(
            message,
            Message::Tick
                | Message::Viewport(_)
                | Message::Close(_)
                | Message::Cancel
                | Message::Discard
                | Message::Opened(_)
                | Message::ThemeFile(
                    super::theme_files::Action::Loaded(..) | super::theme_files::Action::Saved(..)
                )
                | Message::Assistant(
                    super::assistant::Action::Connected(..)
                        | super::assistant::Action::PreferencesSaved(_)
                        | super::assistant::Action::ImageRead { .. }
                        | super::assistant::Action::TextPasted { .. }
                )
        )
}

pub struct State {
    pub open: bool,
    pub automatic: bool,
    pub channel: Channel,
    check_id: u64,
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
        let preferences = if cfg!(test) {
            Preferences::default()
        } else {
            updates::preferences()
        };
        Self {
            open: false,
            automatic: !cfg!(test) && preferences.automatic,
            channel: preferences.channel,
            check_id: 0,
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
        self.latest.as_ref().is_some_and(|release| {
            release.channel() == Some(self.channel)
                && release.available_for(updates::CURRENT_VERSION)
        })
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
                if self.updates.channel != Channel::Stable
                    || self.updates.installing
                    || self.updates.restarting
                {
                    return Task::none();
                }
                if let Some(reason) = self.update_restart_blocker() {
                    self.updates.error = Some(reason.into());
                    return Task::none();
                }
                if self.updates.prepared.is_some() {
                    return self.restart_for_update();
                }
                let Some(release) = self.updates.latest.clone().filter(|r| {
                    r.channel() == Some(Channel::Stable)
                        && r.available_for(updates::CURRENT_VERSION)
                }) else {
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
            Action::RecoveryCleared => return self.handoff_update(),
            Action::Restarted(result) => match result {
                Ok(()) => {
                    self.commit_exit();
                    return self.accessibility_exit();
                }
                Err(error) => {
                    self.update_restart_failed(error);
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
                self.updates.check_id += 1;
                let check_id = self.updates.check_id;
                let channel = self.updates.channel;
                self.updates.error = None;
                return Task::perform(
                    async move {
                        // Let the first canvas appear before checking a release.
                        if !manual {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        }
                        updates::check(channel, manual).await
                    },
                    move |result| Message::Updates(Action::Checked(check_id, result)),
                );
            }
            Action::Checked(check_id, result) => {
                if check_id != self.updates.check_id {
                    return Task::none();
                }
                self.updates.checking = false;
                match result {
                    Ok(release) if release.channel() == Some(self.updates.channel) => {
                        self.updates.latest = Some(release)
                    }
                    Ok(_) => {
                        self.updates.error =
                            Some("The release did not match the selected update channel.".into())
                    }
                    Err(error) => self.updates.error = Some(error),
                }
            }
            Action::Channel(channel) => {
                if channel == self.updates.channel
                    || self.updates.saving
                    || self.updates.installing
                    || self.updates.restarting
                {
                    return Task::none();
                }
                self.updates.channel = channel;
                self.updates.latest = None;
                self.updates.prepared = None;
                self.updates.progress.clear();
                self.updates.error = None;
                self.updates.checking = false;
                let save = self.save_update_preferences();
                let check = self.update_action(Action::Check(true));
                return Task::batch([save, check]);
            }
            Action::Portable => {
                if self.updates.channel == Channel::Nightly
                    && self.updates.available()
                    && let Some(release) = self.updates.latest.clone()
                {
                    return Task::perform(updates::open_nightly_download(release), |result| {
                        Message::Updates(Action::Opened(result))
                    });
                }
            }
            Action::Automatic(enabled) => {
                if self.updates.saving {
                    return Task::none();
                }
                self.updates.automatic = enabled;
                return self.save_update_preferences();
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

    fn save_update_preferences(&mut self) -> Task<Message> {
        self.updates.saving = true;
        Task::perform(
            updates::save_preferences(Preferences {
                automatic: self.updates.automatic,
                channel: self.updates.channel,
            }),
            |result| Message::Updates(Action::Saved(result)),
        )
    }

    fn update_restart_blocker(&self) -> Option<&'static str> {
        #[cfg(any(windows, target_os = "linux"))]
        if self.desktop.opening {
            return Some("Wait for the requested drawings to finish opening.");
        }
        if self
            .strip()
            .any(|tab| self.edited(tab) || (tab.path.is_none() && !tab.doc.all_ids().is_empty()))
        {
            Some("Save your drawings, then click Update and restart.")
        } else if self.assistant.has_unfinished_work() {
            Some("Finish or clear the assistant draft and input before restarting.")
        } else if self.file_io.saving || self.templates.pending() {
            Some("Wait for the current file or library save before restarting.")
        } else if self.strip().any(|tab| {
            tab.busy
                || tab.clipboard_busy
                || tab.cleanup.is_some()
                || tab.joining.is_some()
                || tab.atom_text.is_some()
        }) {
            Some("Finish the current editing operation before restarting.")
        } else {
            None
        }
    }
    pub(super) fn update_restart_failed(&mut self, error: String) {
        self.cancel_close();
        self.updates.restarting = false;
        self.updates.error = Some(error);
        self.updates.open = true;
    }

    fn restart_for_update(&mut self) -> Task<Message> {
        if let Some(reason) = self.update_restart_blocker() {
            self.updates.error = Some(format!("Update ready. {reason}"));
            self.updates.open = true;
            return Task::none();
        }
        if self.updates.prepared.is_none() {
            return Task::none();
        }
        self.updates.restarting = true;
        self.updates.open = true;
        self.updates.error = None;
        // The installer helper has a limited lifetime waiting for this process.
        // Drain recovery writes and clear the draft before launching that helper.
        self.restart_after_recovery()
    }

    fn handoff_update(&mut self) -> Task<Message> {
        if !self.updates.restarting {
            return Task::none();
        }
        let Some(prepared) = self.updates.prepared.clone() else {
            self.cancel_close();
            self.updates.restarting = false;
            return Task::none();
        };
        Task::perform(
            updates::install::handoff(prepared, self.update_reopen_paths()),
            |result| Message::Updates(Action::Restarted(result)),
        )
    }

    fn update_reopen_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths: Vec<_> = self.strip().filter_map(|tab| tab.path.clone()).collect();
        // Opening an already open file restores the front without changing order.
        if let Some(front) = &self.tab.path
            && paths.last() != Some(front)
        {
            paths.push(front.clone());
        }
        paths
    }

    pub(super) fn with_updates<'a>(
        &'a self,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if !self.updates.open {
            return content;
        }
        let content = reshiki::accessibility::inert(content);
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
                if state.channel == Channel::Stable
                    && (Release {
                        version: updates::CURRENT_VERSION.into(),
                    })
                    .channel()
                        == Some(Channel::Nightly)
                {
                    format!("Switch from Nightly to stable ReShiki {}.", latest.version)
                } else {
                    format!("{} ReShiki {} is available.", state.channel, latest.version)
                }
            } else {
                format!("You’re up to date on {}.", state.channel)
            }
        } else {
            format!("Check for the latest {} release.", state.channel)
        };
        let msg = |action| Message::Updates(action);
        let panel = column![
            row![
                crate::branding::wordmark(24.0),
                Space::new().width(Length::Fill),
                reshiki::accessibility::button("updates-close", "Close updates", "Close")
                    .on_press(msg(Action::Show(false)))
                    .padding([7, 10])
                    .style(button::text)
            ]
            .align_y(Alignment::Center),
            text(format!("Version {}", updates::CURRENT_VERSION))
                .size(13)
                .style(super::workspace::muted_text),
            row![
                text("Channel").size(13),
                reshiki::accessibility::button("updates-stable", "Stable update channel", "Stable").checked(state.channel == Channel::Stable)
                    .on_press_maybe((!state.saving && !state.installing && !state.restarting).then_some(msg(Action::Channel(Channel::Stable))))
                    .style(if state.channel == Channel::Stable { button::primary } else { button::secondary }),
                reshiki::accessibility::button("updates-nightly", "Nightly update channel", "Nightly").checked(state.channel == Channel::Nightly)
                    .on_press_maybe((!state.saving && !state.installing && !state.restarting).then_some(msg(Action::Channel(Channel::Nightly))))
                    .style(if state.channel == Channel::Nightly { button::primary } else { button::secondary }),
            ].spacing(8).align_y(Alignment::Center),
            text(status).size(15),
            row![
                reshiki::accessibility::button("updates-check", "Check for updates", "Check for updates")
                    .padding([9, 12])
                    .on_press_maybe((!state.checking && !state.installing && !state.restarting).then_some(msg(Action::Check(true)))),
                reshiki::accessibility::button("updates-install", "Download or install update", if state.channel == Channel::Nightly { "Download nightly ↗" } else if state.installing { "Downloading…" } else { "Update and restart" })
                    .padding([9, 12])
                    .on_press_maybe((state.available() && !state.installing && !state.restarting).then_some(msg(if state.channel == Channel::Nightly { Action::Portable } else { Action::Install })))
            ]
            .spacing(10),
            reshiki::accessibility::button(
                "updates-automatic",
                "Check automatically",
                checkbox(state.automatic)
                    .label("Check automatically")
                    .on_toggle_maybe(
                        (!state.saving)
                            .then_some(|enabled| Message::Updates(Action::Automatic(enabled)))
                    )
                    .size(16)
                    .text_size(13),
            )
            .checked(state.automatic)
            .on_press_maybe((!state.saving).then_some(msg(Action::Automatic(!state.automatic))))
            .padding(0)
            .style(|theme, _| button::Style {
                text_color: theme.palette().text,
                ..Default::default()
            }),
            reshiki::accessibility::button("updates-notes", "Open release notes", "Release notes ↗").on_press(msg(Action::Download)).style(button::text),
            text(if state.channel == Channel::Nightly { "Checks once a day. Nightlies are installed manually. Downloads prefer installers when available; Release notes also links portable archives." } else { "Checks once a day. Stable updates are verified before installation. Your saved tabs reopen after restarting." })
                .size(12)
                .style(super::workspace::muted_text),
        ]
        .spacing(18);
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|theme| {
                    crate::appearance::container(
                        theme,
                        container::Style {
                            background: Some(iced::Color::from_rgba(0., 0., 0., 0.18).into()),
                            ..Default::default()
                        },
                    )
                }),
        )
        .on_press(msg(Action::Show(false)));
        let dialog = container(opaque(container(panel).padding(24).width(460).style(
            |theme| {
                crate::appearance::container(
                    theme,
                    container::Style {
                        background: Some(Color::WHITE.into()),
                        border: Border {
                            color: Color::from_rgb8(201, 212, 207),
                            width: 1.,
                            radius: 12.0.into(),
                        },
                        ..Default::default()
                    },
                )
            },
        )))
        .center_x(Length::Fill)
        .center_y(Length::Fill);
        reshiki::accessibility::focus_scope(stack![content, backdrop, dialog])
    }
}

#[cfg(test)]
mod tests;
