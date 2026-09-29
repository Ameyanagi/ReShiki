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
    Restarted(Result<(), String>),
    Opened(Result<(), String>),
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
                button("Close")
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
                button("Stable")
                    .on_press_maybe((!state.saving && !state.installing && !state.restarting).then_some(msg(Action::Channel(Channel::Stable))))
                    .style(if state.channel == Channel::Stable { button::primary } else { button::secondary }),
                button("Nightly")
                    .on_press_maybe((!state.saving && !state.installing && !state.restarting).then_some(msg(Action::Channel(Channel::Nightly))))
                    .style(if state.channel == Channel::Nightly { button::primary } else { button::secondary }),
            ].spacing(8).align_y(Alignment::Center),
            text(status).size(15),
            row![
                button("Check for updates")
                    .padding([9, 12])
                    .on_press_maybe((!state.checking && !state.installing && !state.restarting).then_some(msg(Action::Check(true)))),
                button(if state.channel == Channel::Nightly { "Download nightly ↗" } else if state.installing { "Downloading…" } else { "Update and restart" })
                    .padding([9, 12])
                    .on_press_maybe((state.available() && !state.installing && !state.restarting).then_some(msg(if state.channel == Channel::Nightly { Action::Portable } else { Action::Install })))
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
            text(if state.channel == Channel::Nightly { "Checks once a day. Nightlies are installed manually. Downloads prefer installers when available; Release notes also links portable archives." } else { "Checks once a day. Stable updates are verified before installation. Your saved drawing reopens after restarting." })
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
        let _ = app.update(Message::Updates(Action::Checked(
            app.updates.check_id,
            Ok(Release {
                version: "99.0.0".into(),
            }),
        )));
        assert!(app.updates.available());
        assert!(!app.updates.checking);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        let _ = app.update_action(Action::Check(true));
        let _ = app.update_action(Action::Checked(app.updates.check_id, Err("Offline".into())));
        assert!(!app.updates.checking);
        assert!(app.updates.available());
        assert_eq!(app.doc, original);
    }

    #[test]
    fn switching_channels_ignores_old_results_and_keeps_automatic_checks_disabled() {
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        assert_eq!(app.updates.channel, Channel::Stable);
        assert!(!app.updates.automatic);
        let _ = app.update_action(Action::Check(true));
        let old = app.updates.check_id;
        app.updates.latest = Some(Release {
            version: "99.0.0".into(),
        });
        app.updates.error = Some("Old error".into());
        let _ = app.update_action(Action::Channel(Channel::Nightly));
        let nightly = app.updates.check_id;
        assert!(nightly > old);
        assert!(app.updates.latest.is_none());
        assert!(app.updates.error.is_none());
        assert!(app.updates.saving);
        assert!(app.updates.checking);
        assert!(!app.updates.automatic);
        let _ = app.update_action(Action::Checked(
            old,
            Ok(Release {
                version: "99.0.0".into(),
            }),
        ));
        let _ = app.update_action(Action::Checked(old, Err("Stale failure".into())));
        assert!(app.updates.latest.is_none());
        assert!(app.updates.error.is_none());
        assert!(app.updates.checking);
        let _ = app.update_action(Action::Saved(Ok(())));
        let _ = app.update_action(Action::Channel(Channel::Stable));
        let _ = app.update_action(Action::Checked(
            nightly,
            Ok(Release {
                version: "99.0.0-nightly.20260929.20.1".into(),
            }),
        ));
        assert!(app.updates.latest.is_none());
        let _ = app.update_action(Action::Checked(
            app.updates.check_id,
            Ok(Release {
                version: "99.0.0".into(),
            }),
        ));
        assert!(app.updates.available());
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
    }

    #[test]
    fn nightly_downloads_cannot_enter_the_stable_installer() {
        let (mut app, _) = App::new();
        app.updates.channel = Channel::Nightly;
        app.updates.latest = Some(Release {
            version: "99.0.0-nightly.20260929.20.1".into(),
        });
        assert!(app.updates.available());
        let _ = app.update_action(Action::Install);
        assert!(!app.updates.installing);
        assert!(!app.updates.restarting);
        assert!(app.updates.receiver.is_none());
        app.updates.installing = true;
        let _ = app.update_action(Action::Channel(Channel::Stable));
        assert_eq!(app.updates.channel, Channel::Nightly);
        app.updates.installing = false;
        app.updates.latest = None;
        let _ = app.update_action(Action::Checked(
            app.updates.check_id,
            Ok(Release {
                version: "99.0.0".into(),
            }),
        ));
        assert!(app.updates.latest.is_none());
        assert!(!app.updates.available());
        assert!(app.updates.error.as_ref().unwrap().contains("channel"));
    }

    #[tokio::test]
    #[ignore = "Opt-in application renderer evidence; does not open desktop windows"]
    async fn update_channels_headless_snapshot() {
        use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
        let (mut app, _) = App::new();
        app.updates.open = true;
        let directory = std::path::Path::new("artifacts/update-channel-qa");
        std::fs::create_dir_all(directory).unwrap();
        for (name, channel, dark) in [
            ("stable", Channel::Stable, false),
            ("nightly", Channel::Nightly, false),
            ("nightly-dark", Channel::Nightly, true),
        ] {
            app.appearance.mode = if dark {
                crate::appearance::Mode::Dark
            } else {
                crate::appearance::Mode::Light
            };
            app.updates.channel = channel;
            app.updates.latest = Some(Release {
                version: if channel == Channel::Stable {
                    updates::CURRENT_VERSION.into()
                } else {
                    "0.9.1-nightly.20260929.36501221724.1".into()
                },
            });
            let mut renderer = <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap();
            let (width, height) = (560, 520);
            let size = iced::Size::new(width as f32, height as f32);
            let theme = app.theme();
            let mut view = app.with_updates(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
            );
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
            let bounds = iced::Rectangle::with_size(size);
            view.as_widget_mut().update(
                &mut tree,
                &iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut Vec::new()),
                &bounds,
            );
            view.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &bounds,
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                directory.join(format!("{name}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
