use super::*;
use iced::futures::StreamExt;

#[test]
fn start_returns_after_the_last_object_is_removed_and_keeps_idle_status_empty() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    assert!(app.start_visible());
    assert!(app.status.is_empty());
    let before = app.tab.doc.clone();
    let id = app.tab.doc.add_atom("C", Default::default());
    app.changed(before);
    assert!(!app.start_visible());
    let _ = app.update(Message::Undo);
    assert!(app.start_visible());
    let _ = app.update(Message::Redo);
    assert!(!app.start_visible());
    app.tab.selected = vec![id];
    let _ = app.update(Message::Delete);
    assert!(app.start_visible());
    let _ = app.inline_action(super::super::inline_text::Action::Begin(
        None,
        Default::default(),
    ));
    assert!(!app.start_visible(), "A text draft owns the empty canvas");
    let _ = app.inline_action(super::super::inline_text::Action::Finish(false));
    assert!(app.start_visible());
}

#[tokio::test]
async fn start_smiles_uses_normal_insertion_and_one_undo_without_changing_other_tabs() {
    let (mut app, _) = App::new();
    app.engine = reshiki::engine::LocalEngine::in_process();
    app.tab.busy = false;
    let drawing = app.tab.doc.clone();
    let _ = app.start_action(Action::Input("CCO".into()));
    assert_eq!(app.tab.doc, drawing);
    assert!(!app.tab.history.can_undo());
    let mut effects =
        iced_runtime::task::into_stream(app.update(Message::Start(Action::Insert))).unwrap();
    while let Some(effect) = effects.next().await {
        if let iced_runtime::Action::Output(message) = effect {
            let _ = app.update(message);
        }
    }
    assert_eq!(app.tab.doc.atoms.len(), 3, "{}", app.status);
    assert!(!app.start_visible());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, drawing);
    assert!(app.start_visible());
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Requires the built local naming worker"]
async fn start_name_uses_the_real_local_import_worker() {
    let (mut app, _) = App::new();
    app.engine = reshiki::engine::LocalEngine::in_process();
    app.tab.busy = false;
    let _ = app.start_action(Action::Input("ethanol".into()));
    let mut effects =
        iced_runtime::task::into_stream(app.update(Message::Start(Action::Insert))).unwrap();
    while let Some(effect) = effects.next().await {
        if let iced_runtime::Action::Output(message) = effect {
            let _ = app.update(message);
        }
    }
    assert_eq!(
        app.tab.doc.atoms.len(),
        3,
        "{:?}",
        app.start_name_notice(&app.start.input)
    );
    assert!(!app.start_visible());
    let _ = app.update(Message::Undo);
    assert!(app.start_visible());
    assert!(!app.tab.history.can_undo());
}

#[test]
fn start_rejects_hidden_stale_actions_and_preserves_import_feedback() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.start_action(Action::Input("CCO".into()));
    app.tab.doc.add_atom("N", Default::default());
    assert_eq!(app.start_action(Action::Insert).units(), 0);
    assert_eq!(app.tab.doc.atoms.len(), 1);
    app.status = "Could not open drawing".into();
    app.error = true;
    app.tab.doc.clear_content();
    let _ = app.with_start(iced::widget::Space::new().into());
    assert_eq!(app.status, "Could not open drawing");
    assert!(app.error);
}

#[tokio::test]
#[ignore = "Requires the built local naming worker"]
async fn start_name_resets_hidden_peptide_mode_before_resolving_ordinary_names() {
    async fn insert(peptide: bool) -> (App, reshiki::document::Document) {
        let (mut app, _) = App::new();
        app.engine = reshiki::engine::LocalEngine::in_process();
        app.tab.busy = false;
        let before = app.tab.doc.clone();
        let _ = app.naming_action(super::super::naming::Action::PeptideSequence(peptide));
        let _ = app.start_action(Action::Input("alanine".into()));
        let mut effects =
            iced_runtime::task::into_stream(app.update(Message::Start(Action::Insert))).unwrap();
        while let Some(effect) = effects.next().await {
            if let iced_runtime::Action::Output(message) = effect {
                let _ = app.update(message);
            }
        }
        (app, before)
    }
    let (ordinary, _) = insert(false).await;
    let (mut app, before) = insert(true).await;
    assert_eq!(
        app.tab.doc.atoms.len(),
        6,
        "Start must import alanine, not the ALANINE peptide: {:?}",
        app.start_name_notice(&app.start.input)
    );
    let actual = reshiki::naming::structure_smiles(&app.tab.doc).unwrap();
    assert_eq!(
        actual,
        reshiki::naming::structure_smiles(&ordinary.tab.doc).unwrap(),
        "The start field must match ordinary-name mode, including stereochemistry"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn browser_recent_uses_tab_and_epoch_identity_instead_of_a_bare_name() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.path = Some("same.rsk".into());
    let first = (app.tab.id, app.tab.file_epoch);
    app.remember_start_browser_tab();
    app.add_tab();
    app.tab.path = Some("same.rsk".into());
    let second = (app.tab.id, app.tab.file_epoch);
    app.remember_start_browser_tab();
    assert_eq!(app.start.browser_recent, [second, first]);
    let _ = app.start_action(Action::BrowserOpen(first.0, first.1));
    assert_eq!(app.tab.id, first.0);
    app.reset_tab();
    let current = (app.tab.id, app.tab.file_epoch);
    let _ = app.start_action(Action::BrowserOpen(second.0, second.1));
    assert_eq!(app.tab.id, second.0);
    let _ = app.start_action(Action::BrowserOpen(current.0, first.1));
    assert_eq!(
        app.tab.id, second.0,
        "An old epoch cannot select a replacement drawing"
    );
}

#[tokio::test]
#[ignore = "Opt-in actual renderer start-block layout and accessibility check"]
async fn start_controls_have_visible_bounds_and_disappear_with_the_first_object()
-> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    for size in [iced::Size::new(900., 640.), iced::Size::new(320., 480.)] {
        let mut ui = iced_runtime::UserInterface::build(
            app.with_start(
                iced::widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
            ),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        assert!(collect.snapshot().duplicate_ids.is_empty());
        for id in [
            "start-input",
            "start-open",
            "start-paste",
            "start-templates",
        ] {
            let node = collect
                .snapshot()
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap();
            assert!(node.bounds.height >= 25., "{id}: {:?}", node.bounds);
            assert!(
                node.visible_bounds
                    .is_some_and(|bounds| bounds.height >= 25.)
            );
            println!("Start {size:?} {id}: {:?}", node.bounds);
        }
        let field = collect
            .snapshot()
            .nodes
            .iter()
            .find(|node| node.id == "start-input")
            .unwrap();
        assert!((field.bounds.center_x() - size.width / 2.).abs() < 2.);
        // Sample the card's empty top padding: it shares the inspector's quiet
        // paper surface rather than the theme's colored rounded-box fill.
        let sample = iced::Point::new(field.bounds.x + 5., field.bounds.y - 10.);
        for theme in [iced::Theme::Light, iced::Theme::Dark] {
            ui.draw(
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: theme.palette().text,
                },
                iced::mouse::Cursor::Unavailable,
            );
            let width = size.width as u32;
            let height = size.height as u32;
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                theme.palette().background,
            );
            let offset = ((sample.y as u32 * width + sample.x as u32) * 4) as usize;
            let rgb = &pixels[offset..offset + 3];
            if theme == iced::Theme::Light {
                assert!(rgb.iter().all(|channel| *channel >= 245), "{rgb:?}");
            } else {
                assert!(rgb.iter().all(|channel| *channel < 40), "{rgb:?}");
            }
            println!("Start {size:?} {theme:?} card padding: {rgb:?}");
            if let Some(directory) = std::env::var_os("RESHIKI_START_QA_DIR") {
                let path = std::path::PathBuf::from(directory)
                    .join(format!("start-{}x{}-{theme:?}.png", width, height));
                ::image::save_buffer(path, &pixels, width, height, ::image::ColorType::Rgba8)
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    // The start block belongs below the existing context-menu cascade, so
    // its controls do not leak into the menu's foreground accessibility scope.
    app.context_menu = Some(super::super::context_menu::State::new(
        iced::Point::new(450., 320.),
        super::super::context_menu::Page::Main,
    ));
    app.inspector_open = false;
    {
        let size = iced::Size::new(900., 640.);
        let mut ui = iced_runtime::UserInterface::build(
            app.workspace(),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        assert!(!collect.snapshot().nodes.is_empty());
        assert!(
            !collect
                .snapshot()
                .nodes
                .iter()
                .any(|node| node.id.starts_with("start-"))
        );
    }
    app.context_menu = None;
    app.tab.doc.add_atom("C", Default::default());
    let size = iced::Size::new(900., 640.);
    let mut ui = iced_runtime::UserInterface::build(
        app.with_start(iced::widget::Space::new().into()),
        size,
        iced_runtime::user_interface::Cache::new(),
        &mut renderer,
    );
    let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    assert!(
        !collect
            .snapshot()
            .nodes
            .iter()
            .any(|node| node.id.starts_with("start-"))
    );
    Ok(())
}

#[test]
fn editing_start_input_cancels_the_previous_local_name_request() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.start_action(Action::Input("ethanol".into()));
    let pending = app.start_action(Action::Insert);
    assert!(app.tab.naming.is_pending());
    let _ = app.start_action(Action::Input("methanol".into()));
    assert!(!app.tab.naming.is_pending());
    assert!(app.tab.doc.object_ids().next().is_none());
    assert!(!app.tab.history.can_undo());
    drop(pending);
}

#[test]
fn office_session_files_are_not_recent_but_normal_files_still_are() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let directory = tempfile::tempdir().unwrap();
    let document = reshiki::document::Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap();
    let session = directory.path().join("office-edit.rsk");
    std::fs::write(&session, document.file_json().unwrap()).unwrap();
    let binding =
        crate::session::Binding::standalone(session.clone(), crate::session::Host::Office);
    let _ = app.office_prepared(
        binding,
        Some((
            session,
            Ok(super::super::files::Prepared::Native(Box::new(
                document.clone(),
            ))),
        )),
    );
    assert!(app.start.recent.entries.is_empty());
    let ordinary = directory.path().join("normal.rsk");
    std::fs::write(&ordinary, document.file_json().unwrap()).unwrap();
    let _ = app.file_prepared(Some((
        ordinary.clone(),
        Ok(super::super::files::Prepared::Native(Box::new(document))),
    )));
    assert_eq!(app.start.recent.entries.len(), 1);
    assert_eq!(
        app.start.recent.entries[0].path,
        ordinary.canonicalize().unwrap()
    );
}
