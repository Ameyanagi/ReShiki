use super::{App, Message};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, stack, text};
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
    OpenSettings,
    CopyVersion,
    OpenAboutLink(&'static str),
    /// Opens the data folder in the file manager.
    OpenDataFolder,
}

/// Already-started work and window/save lifecycle events keep running while
/// the dialog owns input. Document results retain their usual epoch guards.
pub(super) fn background(message: &Message) -> bool {
    matches!(
        message,
        Message::Tick
            | Message::Viewport(_)
            | Message::Close(_)
            | Message::Cancel
            | Message::Discard
    ) || super::results::admitted(super::results::Modal::UpdatesDialog, message)
}

enum Failure {
    Check(String),
    Other(String),
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
    error: Option<Failure>,
    version_copied: bool,
    /// The data folder About names, as resolved after startup moved it.
    pub(super) data_folder: Option<std::path::PathBuf>,
    /// Why startup could not move the data folder to `~/.reshiki`.
    pub(super) data_warning: Option<String>,
}
impl State {
    /// Whether a preference save is in progress, which holds both preferences.
    pub(super) fn saving(&self) -> bool {
        self.saving
    }
    /// Whether the channel can change: no save, install or restart is running.
    pub(super) fn idle(&self) -> bool {
        !self.saving && !self.installing && !self.restarting
    }
    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_ref().map(|error| match error {
            Failure::Check(message) | Failure::Other(message) => message.as_str(),
        })
    }
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
            version_copied: false,
            data_folder: reshiki::compatibility::data_directory_location()
                .ok()
                .map(|location| location.path),
            data_warning: None,
        }
    }
    pub fn available(&self) -> bool {
        self.latest.as_ref().is_some_and(|release| {
            release.channel() == Some(self.channel)
                && release.available_for(updates::CURRENT_VERSION)
        })
    }
    fn card_action(&self) -> (&'static str, &'static str, Action) {
        if matches!(self.error, Some(Failure::Check(_))) {
            ("updates-check", "Try again", Action::Check(true))
        } else if self.available() {
            if self.channel == Channel::Nightly {
                ("updates-install", "Download nightly ↗", Action::Portable)
            } else {
                ("updates-install", "Update and restart", Action::Install)
            }
        } else {
            ("updates-check", "Check now", Action::Check(true))
        }
    }
    fn card_refresh_action(&self) -> Option<Action> {
        (!self.checking
            && !self.installing
            && !self.restarting
            && matches!(self.card_action().2, Action::Install | Action::Portable))
        .then_some(Action::Check(true))
    }
    fn installed_channel() -> Channel {
        Release {
            version: updates::CURRENT_VERSION.into(),
        }
        .channel()
        .unwrap_or(Channel::Stable)
    }
    fn version_report() -> String {
        let version = updates::CURRENT_VERSION;
        let installed = Self::installed_channel();
        let platform = match std::env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            _ => "Web",
        };
        let architecture = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            architecture => architecture,
        };
        format!("ReShiki {version} ({installed}, {platform} {architecture})")
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
                    self.updates.version_copied = false;
                    // About replaces either filter of the unified palette.
                    self.help_open &= !open;
                    self.commands.open &= !open;
                    #[cfg(not(target_arch = "wasm32"))]
                    if open {
                        return self.star_about_opened();
                    }
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
                    self.updates.error = Some(Failure::Other(reason.into()));
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
                    Err(error) => self.updates.error = Some(Failure::Other(error)),
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
                        #[cfg(not(target_arch = "wasm32"))]
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
                        if self.updates.latest.as_ref() != Some(&release) {
                            self.updates.prepared = None;
                        }
                        self.updates.latest = Some(release);
                    }
                    Ok(_) => {
                        self.updates.error = Some(Failure::Check(
                            "The release did not match the selected update channel.".into(),
                        ))
                    }
                    Err(error) => self.updates.error = Some(Failure::Check(error)),
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
                    self.updates.error = Some(Failure::Other(format!(
                        "Could not save update preferences: {error}"
                    )));
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
                    self.updates.error = Some(Failure::Other(error));
                }
            }
            Action::OpenSettings => {
                if !cfg!(target_arch = "wasm32") && !self.updates.restarting {
                    self.updates.open = false;
                    return self.settings_action(super::settings_page::Action::Open(Some(
                        super::settings_page::Section::Updates,
                    )));
                }
            }
            Action::CopyVersion => {
                self.updates.version_copied = true;
                return iced::clipboard::write(State::version_report());
            }
            Action::OpenAboutLink(url) => {
                return Task::perform(updates::open_url(url.into()), |result| {
                    Message::Updates(Action::Opened(result))
                });
            }
            Action::OpenDataFolder => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(folder) = self.updates.data_folder.clone() {
                    let open = move || {
                        // Nothing may have written there yet.
                        std::fs::create_dir_all(&folder).and_then(|()| open::that(&folder))
                    };
                    return Task::perform(
                        async move {
                            reshiki::task::spawn_blocking(open)
                                .await
                                .map_err(|error| error.to_string())?
                                .map_err(|error| format!("Could not open the data folder: {error}"))
                        },
                        |result| Message::Updates(Action::Opened(result)),
                    );
                }
            }
        }
        Task::none()
    }

    fn save_update_preferences(&mut self) -> Task<Message> {
        self.updates.saving = true;
        Task::perform(
            updates::save_preferences(self.update_preferences()),
            |result| Message::Updates(Action::Saved(result)),
        )
    }

    pub(super) fn update_preferences(&self) -> Preferences {
        Preferences {
            automatic: self.updates.automatic,
            channel: self.updates.channel,
        }
    }

    /// Automatic checks on the Stable channel, as at first launch.
    pub(super) fn restore_update_defaults(&mut self) -> Task<Message> {
        let defaults = Preferences::default();
        self.updates.automatic = defaults.automatic;
        if self.updates.channel == defaults.channel {
            self.save_update_preferences()
        } else {
            self.update_action(Action::Channel(defaults.channel))
        }
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
            tab.busy || tab.clipboard_busy || tab.joining.is_some() || tab.atom_text.is_some()
        }) {
            Some("Finish the current editing operation before restarting.")
        } else {
            None
        }
    }
    pub(super) fn update_restart_failed(&mut self, error: String) {
        self.cancel_close();
        self.updates.restarting = false;
        self.updates.error = Some(Failure::Other(error));
        self.updates.open = true;
    }

    fn restart_for_update(&mut self) -> Task<Message> {
        if let Some(reason) = self.update_restart_blocker() {
            self.updates.error = Some(Failure::Other(format!("Update ready. {reason}")));
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
        } else if let Some(error) = state.error() {
            error.to_owned()
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
        let header = row![
            column![
                crate::branding::wordmark(24.0),
                reshiki::accessibility::button(
                    "updates-copy-version",
                    if state.version_copied {
                        "Version copied"
                    } else {
                        "Copy ReShiki version"
                    },
                    text(if state.version_copied {
                        "Copied".into()
                    } else {
                        format!(
                            "Version {} · {}",
                            updates::CURRENT_VERSION,
                            State::installed_channel()
                        )
                    })
                    .size(12),
                )
                .on_press(msg(Action::CopyVersion))
                .padding(0)
                .style(button::text)
            ]
            .spacing(5),
            Space::new().width(Length::Fill),
            reshiki::accessibility::button("updates-close", "Close About", "×")
                .on_press(msg(Action::Show(false)))
                .padding([6, 10])
                .style(button::text),
        ]
        .align_y(Alignment::Start);
        let link = |id, label, action| {
            reshiki::accessibility::button(id, label, text(label).size(13))
                .on_press(msg(action))
                .padding([5, 4])
                .style(button::text)
        };
        let website = link(
            "updates-website",
            "Website ↗",
            Action::OpenAboutLink("https://reshiki.com/"),
        );
        let license = link(
            "updates-license",
            "License",
            Action::OpenAboutLink("https://github.com/Ameyanagi/ReShiki/blob/main/LICENSE"),
        );
        let panel = if cfg!(target_arch = "wasm32") {
            column![
                header,
                text("The browser editor updates with the page that hosts it.").size(14),
                row![
                    link("updates-notes", "Get the desktop app ↗", Action::Download),
                    website,
                    license
                ]
                .spacing(4),
            ]
        } else {
            let pending = state.checking || state.installing || state.restarting;
            let status = text(status).size(14).width(Length::Fill);
            let mut summary = row![status].spacing(10).align_y(Alignment::Center);
            if !pending {
                let (id, label, action) = state.card_action();
                let installs = matches!(action, Action::Install | Action::Portable);
                summary = summary.push(
                    reshiki::accessibility::button(id, label, text(label).size(13))
                        .on_press(msg(action))
                        .padding([7, 10])
                        .style(if installs {
                            crate::appearance::primary
                        } else {
                            crate::appearance::outline
                        }),
                );
            }
            let (label, hint) = self.star_link();
            let star =
                reshiki::accessibility::button("updates-star", label.clone(), text(label).size(13))
                    .on_press(Message::Star(super::star_prompt::Action::Star {
                        toast: false,
                    }))
                    .padding([5, 4])
                    .style(button::text);
            let star: Element<'_, Message> = match hint {
                Some(hint) => {
                    super::workspace::hover_hint(star, hint, iced::widget::tooltip::Position::Top)
                        .into()
                }
                None => star.into(),
            };
            column![
                header,
                summary,
                super::workspace::horizontal_line(),
                row![
                    link("updates-notes", "Release notes ↗", Action::Download),
                    star,
                    website,
                    license
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                row![
                    link("updates-settings", "Update settings…", Action::OpenSettings),
                    state
                        .card_refresh_action()
                        .map(|action| { link("updates-check-again", "Check again", action) }),
                ]
                .spacing(8),
            ]
            .push(state.data_folder.as_ref().map(|folder| {
                row![
                    text(format!("Data folder: {}", folder.display()))
                        .size(11)
                        .style(super::workspace::muted_text)
                        .width(Length::Fill),
                    reshiki::accessibility::button(
                        "updates-data-folder",
                        "Open data folder",
                        "Open"
                    )
                    .on_press(msg(Action::OpenDataFolder))
                    .padding([4, 8])
                    .style(button::text),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            }))
            .push(
                state
                    .data_warning
                    .as_ref()
                    .map(|warning| text(warning.as_str()).size(12).style(text::danger)),
            )
        }
        .spacing(10);
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
