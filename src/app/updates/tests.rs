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
