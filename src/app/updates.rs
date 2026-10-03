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
            checkbox(state.automatic)
                .label("Check automatically")
                .on_toggle_maybe(
                    (!state.saving)
                        .then_some(|enabled| Message::Updates(Action::Automatic(enabled)))
                )
                .size(16)
                .text_size(13),
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
        stack![content, backdrop, dialog].into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_modal_rejects_drawing_and_file_messages_before_they_change_state() {
        let (mut app, _) = App::new();
        let atom = app
            .tab
            .doc
            .add_atom("C", reshiki::document::Point::default());
        app.tab.selected = vec![atom];
        let _ = app.update(Message::Assistant(super::super::assistant::Action::Input(
            iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
                "Unsent draft".to_owned().into(),
            )),
        )));
        let drawing = app.tab.doc.clone();
        let revision = app.tab.revision;
        let tab = app.tab.id;
        let tool = app.tool;
        let _ = app.update(Message::Updates(Action::Show(true)));
        for message in [
            Message::Delete,
            Message::Shortcut(super::super::shortcuts::Action::Nudge(1., 0.)),
            Message::ContextKey("N".into()),
            Message::ToggleHelp,
            Message::New,
            Message::Open,
            Message::Save,
            Message::SaveAs,
            Message::Tabs(super::super::tabs::Action::Close(None)),
            Message::Paste,
            Message::Assistant(super::super::assistant::Action::Send),
        ] {
            let _ = app.update(message);
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.revision, revision);
            assert_eq!(app.tab.id, tab);
            assert_eq!(app.tab.selected, [atom]);
            assert_eq!(app.tool, tool);
            assert_eq!(app.assistant.input_text(), "Unsent draft");
            assert!(!app.assistant.busy);
            assert!(app.updates.open);
            assert!(!app.help_open);
            assert!(!app.tab.history.can_undo());
            assert!(!app.file_io.saving);
            assert!(app.pending.is_none());
        }
        // Escape belongs to the topmost dialog even with an older panel open.
        app.help_open = true;
        let _ = app.update(Message::Escape);
        assert!(!app.updates.open);
        assert!(app.help_open);
        assert_eq!(app.tab.doc, drawing);
    }

    #[test]
    fn updates_modal_keeps_async_results_and_save_completion_running() {
        let (mut app, _) = App::new();
        app.updates.open = true;
        app.tab.busy = true;
        app.file_io.saving = true;
        let drawing = app.tab.doc.clone();
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: super::super::Job::Analyze,
            result: Box::new(Err("Analysis completed with an error".into())),
        });
        assert!(!app.tab.busy);
        assert!(app.status.contains("Analysis completed"));
        let _ = app.update(Message::Saved(
            app.tab.file_epoch,
            Box::new(drawing.clone()),
            Err("Save completed with an error".into()),
        ));
        assert!(!app.file_io.saving);
        assert!(app.status.contains("Save completed"));
        let _ = app.update(Message::Updates(Action::Check(true)));
        assert!(app.updates.checking);
        let _ = app.update(Message::Updates(Action::Checked(
            app.updates.check_id,
            Ok(Release {
                version: "99.0.0".into(),
            }),
        )));
        assert!(!app.updates.checking);
        assert!(app.updates.available());
        assert!(app.updates.open);
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.tab.history.can_undo());

        // A result from an insertion started before opening Updates still
        // lands in this tab and records its normal undo step.
        let mut inserted = reshiki::document::Document::default();
        inserted.add_atom("O", reshiki::document::Point::default());
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: super::super::Job::Insert,
            result: Box::new(Ok(reshiki::engine::Response {
                document: Some(inserted),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            })),
        });
        assert_eq!(app.tab.doc.atoms.len(), 1);
        assert_eq!(app.tab.doc.atoms[0].element, "O");
        assert!(app.tab.history.can_undo());
        assert!(app.updates.open);
    }

    #[tokio::test]
    async fn update_reopens_saved_tabs_in_order_and_restores_each_front() {
        let directory = tempfile::tempdir().unwrap();
        let paths: Vec<_> = ["first drawing.rsk", "構造 β.rsk", "last drawing.rsk"]
            .map(|name| directory.path().join(name))
            .to_vec();
        for (i, path) in paths.iter().enumerate() {
            let mut doc = reshiki::document::Document::default();
            doc.add_atom(["C", "N", "O"][i], reshiki::document::Point::default());
            std::fs::write(path, doc.file_json().unwrap()).unwrap();
        }
        let (mut original, _) = App::new();
        for path in &paths {
            let opened = super::super::files::read(path.clone()).await;
            let _ = original.update(Message::FilePrepared(opened));
        }
        for front in 0..paths.len() {
            original.select_tab(front);
            assert!(original.update_restart_blocker().is_none());
            let reopen = original.update_reopen_paths();
            assert_eq!(&reopen[..paths.len()], paths);
            assert_eq!(reopen.last(), paths.get(front));
            let args = super::super::startup::parse(reopen.iter().flat_map(|path| {
                [
                    std::ffi::OsString::from("--open"),
                    path.as_os_str().to_owned(),
                ]
            }));
            let (mut relaunched, _) = App::new();
            for path in args.paths {
                let opened = super::super::files::read(path).await;
                let _ = relaunched.update(Message::FilePrepared(opened));
            }
            assert_eq!(
                relaunched
                    .strip()
                    .filter_map(|tab| tab.path.clone())
                    .collect::<Vec<_>>(),
                paths
            );
            assert_eq!(relaunched.tabs.active, front);
            assert_eq!(relaunched.tab.path, paths.get(front).cloned());
        }
        let _ = original.update(Message::New);
        assert!(original.update_restart_blocker().is_none());
        assert_eq!(original.update_reopen_paths(), paths);
    }

    #[test]
    fn empty_and_single_file_reopen_lists_remain_compatible() {
        let (mut app, _) = App::new();
        assert!(app.update_reopen_paths().is_empty());
        app.tab.path = Some("saved drawing.rsk".into());
        assert_eq!(
            app.update_reopen_paths(),
            [std::path::PathBuf::from("saved drawing.rsk")]
        );
    }

    #[test]
    fn unbound_examples_must_be_saved_before_update_restart() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::OpenShortcutExamples);
        assert!(!app.dirty());
        assert!(app.update_restart_blocker().unwrap().contains("Save"));
        let _ = app.update(Message::New);
        assert!(app.update_restart_blocker().unwrap().contains("Save"));
        app.tabs.background[0].path = Some("My examples.rsk".into());
        assert!(app.update_restart_blocker().is_none());
    }

    #[test]
    fn pending_atom_label_prevents_update_restart() {
        let (mut app, _) = App::new();
        let atom = app
            .tab
            .doc
            .add_atom("C", reshiki::document::Point::default());
        app.tab.saved = app.tab.doc.clone();
        app.tab.path = Some("saved drawing.rsk".into());
        let _ = app.atom_text_action(super::super::atom_text::Action::Begin(Some(atom)));
        let _ = app.atom_text_action(super::super::atom_text::Action::Input("Boc".into()));
        assert!(!app.dirty());
        assert!(app.update_restart_blocker().is_some());
        let _ = app.restart_for_update();
        assert!(!app.updates.restarting);
        assert!(app.tab.atom_text.is_some());
        let _ = app.atom_text_action(super::super::atom_text::Action::Cancel);
        assert!(app.update_restart_blocker().is_none());
    }

    #[test]
    fn work_in_a_tab_behind_the_front_prevents_update_restart() {
        let (mut app, _) = App::new();
        app.tab.busy = true;
        let _ = app.update(Message::New);
        assert!(app.tabs.background[0].busy, "A busy tab is not reused");
        assert!(app.update_restart_blocker().is_some());
        app.tabs.background[0].busy = false;
        assert!(app.update_restart_blocker().is_none());
    }

    #[test]
    fn clipboard_work_in_any_tab_prevents_update_restart() {
        for background in [false, true] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            let id = app.tab.id;
            app.tab.clipboard_busy = true;
            if background {
                app.add_tab();
            }
            assert!(app.update_restart_blocker().is_some());
            let _ = app.restart_for_update();
            assert!(!app.updates.restarting);
            app.in_tab(id, |app| app.tab.clipboard_busy = false);
            assert!(app.update_restart_blocker().is_none());
        }
    }

    #[test]
    fn updates_cannot_discard_unsaved_drawing_or_assistant_work() {
        let (mut app, _) = App::new();
        app.updates.latest = Some(Release {
            version: "99.0.0".into(),
        });
        app.tab
            .doc
            .add_atom("N", reshiki::document::Point::default());
        let original = app.tab.doc.clone();
        let _ = app.update_action(Action::Install);
        assert!(!app.updates.installing);
        assert!(!app.updates.restarting);
        assert!(app.updates.error.as_ref().unwrap().contains("Save"));
        app.tab.saved = app.tab.doc.clone();
        app.tab.path = Some("saved drawing.rsk".into());
        app.assistant.busy = true;
        let _ = app.update_action(Action::Install);
        assert!(!app.updates.installing);
        assert!(app.updates.error.as_ref().unwrap().contains("assistant"));
        assert_eq!(app.tab.doc, original);
        app.assistant.busy = false;
        assert!(app.update_restart_blocker().is_none());
        app.updates.restarting = true;
        let _ = app.update(Message::Delete);
        assert_eq!(app.tab.doc, original);
    }

    #[test]
    fn background_checks_respect_opt_out_and_do_not_change_a_drawing() {
        let (mut app, _) = App::new();
        let original = app.tab.doc.clone();
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
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update_action(Action::Check(true));
        let _ = app.update_action(Action::Checked(app.updates.check_id, Err("Offline".into())));
        assert!(!app.updates.checking);
        assert!(app.updates.available());
        assert_eq!(app.tab.doc, original);
    }

    #[test]
    fn switching_channels_ignores_old_results_and_keeps_automatic_checks_disabled() {
        let (mut app, _) = App::new();
        let original = app.tab.doc.clone();
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
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
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
