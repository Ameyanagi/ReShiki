use super::*;
use crate::app::import::{self, InputKind};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.imports.kind = InputKind::Name;
    let _ = app.naming_action(Action::Name("olanzapine".into()));
    app
}

fn confirm(app: &mut App) -> Task<Message> {
    let serial = app
        .tab
        .naming
        .online_confirmation
        .as_ref()
        .map(|(serial, _)| *serial)
        .unwrap_or(u64::MAX);
    app.naming_action(Action::ConfirmOnline(serial))
}

#[test]
fn online_lookup_requires_fresh_confirmation_for_the_current_name() {
    let mut app = ready();
    let _ = app.insert_input();
    assert!(matches!(
        app.tab.naming.pending,
        Some(Pending::Import { .. })
    ));
    let ticket = app.tab.naming.pending.unwrap().ticket();
    assert_eq!(
        app.naming_action(Action::Finished(
            ticket,
            Box::new(Err(naming::Error::Rejected("Unknown local name".into()),))
        ))
        .units(),
        0
    );
    assert!(app.tab.naming.pending.is_none());
    assert!(app.tab.naming.online_confirmation.is_none());
    assert_eq!(confirm(&mut app).units(), 0);
    assert_eq!(app.naming_action(Action::AskOnline).units(), 0);
    assert_eq!(
        app.tab
            .naming
            .online_confirmation
            .as_ref()
            .map(|(_, name)| name.as_str()),
        Some("olanzapine")
    );
    assert!(app.tab.naming.pending.is_none());
    assert!(app.tab.naming.local_cancel.is_none());

    let _ = app.naming_action(Action::Name("clozapine".into()));
    assert_eq!(confirm(&mut app).units(), 0);
    let _ = app.naming_action(Action::AskOnline);
    let _ = app.naming_action(Action::DismissOnline);
    assert_eq!(confirm(&mut app).units(), 0);
    let _ = app.naming_action(Action::AskOnline);
    let _ = app.import_action(import::Action::InputKind(InputKind::Structure));
    assert_eq!(confirm(&mut app).units(), 0);

    let _ = app.import_action(import::Action::InputKind(InputKind::Name));
    let _ = app.naming_action(Action::AskOnline);
    // The task is deliberately not polled: this test never contacts PubChem.
    assert!(confirm(&mut app).units() > 0);
    assert!(matches!(app.tab.naming.pending, Some(Pending::Online(_))));
    assert!(app.tab.naming.online_confirmation.is_none());
    assert_eq!(confirm(&mut app).units(), 0);
    let _ = app.naming_action(Action::Cancel);
    assert!(app.tab.naming.pending.is_none());
    assert_eq!(confirm(&mut app).units(), 0);
}

#[tokio::test]
async fn online_result_requires_insert_and_keeps_identity_caption_and_history() -> Result<(), String>
{
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::AskOnline);
    let _ = confirm(&mut app);
    let ticket = app.tab.naming.pending.unwrap().ticket();
    // PubChem CID 135398745: the online response is already decoded here.
    // Backend tests independently exercise decoding and identity constraints.
    let smiles = "CC1=CC2=C(S1)NC3=CC=CC=C3N=C2N4CCN(CC4)C";
    let record = Record {
        title: "olanzapine".into(),
        name_status: None,
        systematic_name: Some(
            "2-methyl-4-(4-methylpiperazin-1-yl)-10H-thieno[2,3-b][1,5]benzodiazepine".into(),
        ),
        provenance: naming::Provenance::PubChem { cid: 135398745 },
        ..super::super::test_support::record(smiles)
    };
    let preview = prepare_preview(
        app.engine.clone(),
        record.clone(),
        smiles.into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Names(vec![record], Some(Box::new(preview))))),
    ));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.naming.preview_open);
    assert!(matches!(
        app.tab.naming.preview.as_ref().unwrap().record.provenance,
        naming::Provenance::PubChem { cid: 135398745 }
    ));
    let _ = app.insert_input();
    naming::verify_identity(smiles, &naming::document_identity(&app.tab.doc)?.smiles)?;
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(app.tab.doc.annotations[0].text, "olanzapine");
    let inserted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, inserted);
    Ok(())
}

#[test]
fn online_failures_leave_the_drawing_unchanged() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::AskOnline);
    let _ = confirm(&mut app);
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let detail = "PubChem returned more than one compound. Use a more specific name.";
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Err(naming::Error::Rejected(detail.into()))),
    ));
    assert_eq!(app.tab.naming.notice.as_ref().unwrap().summary(), detail);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn a_delayed_confirmation_cannot_authorize_a_different_dialog() {
    let mut app = ready();
    let _ = app.naming_action(Action::AskOnline);
    let old = app.tab.naming.online_confirmation.as_ref().unwrap().0;
    for name in ["clozapine", "olanzapine"] {
        let _ = app.naming_action(Action::Name(name.into()));
        let _ = app.naming_action(Action::AskOnline);
        let current = app.tab.naming.online_confirmation.clone();
        assert_eq!(app.naming_action(Action::ConfirmOnline(old)).units(), 0);
        assert_eq!(app.tab.naming.online_confirmation, current);
        assert!(app.tab.naming.pending.is_none());
    }
}

#[tokio::test]
async fn stale_online_success_cannot_install_a_preview_or_change_history() -> Result<(), String> {
    for action in ["edit", "cancel", "mode"] {
        let mut app = ready();
        let before = app.tab.doc.clone();
        let _ = app.naming_action(Action::AskOnline);
        let _ = confirm(&mut app);
        let ticket = app.tab.naming.pending.unwrap().ticket();
        let record = Record {
            title: "ethanol".into(),
            name_status: None,
            provenance: naming::Provenance::PubChem { cid: 702 },
            ..super::super::test_support::record("CCO")
        };
        let preview = prepare_preview(
            app.engine.clone(),
            record.clone(),
            "CCO".into(),
            Default::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
        let _ = match action {
            "edit" => app.naming_action(Action::Name("clozapine".into())),
            "cancel" => app.naming_action(Action::Cancel),
            _ => app.import_action(import::Action::InputKind(InputKind::Structure)),
        };
        let _ = app.naming_action(Action::Finished(
            ticket,
            Box::new(Ok(Outcome::Names(vec![record], Some(Box::new(preview))))),
        ));
        assert!(app.tab.naming.preview.is_none(), "{action}");
        assert!(app.tab.naming.candidates.is_empty(), "{action}");
        assert!(app.tab.naming.pending.is_none(), "{action}");
        assert_eq!(app.tab.doc, before, "{action}");
        assert!(!app.tab.history.can_undo(), "{action}");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "Opt-in actual renderer online consent controls and dispatch"]
async fn confirmation_controls_are_visible_and_dispatch_the_displayed_consent() -> Result<(), String>
{
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::{Activate, Collect};
    let mut app = ready();
    let _ = app.naming_action(Action::AskOnline);
    let serial = app.tab.naming.online_confirmation.as_ref().unwrap().0;
    let size = iced::Size::new(268., 700.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    for dock in [true, false] {
        let panel = if dock {
            app.import_panel()
        } else {
            app.naming_panel()
        };
        let mut ui =
            iced_runtime::UserInterface::build(panel, size, Default::default(), &mut renderer);
        let mut collect = Collect::new(iced::Rectangle::with_size(size));
        ui.operate(
            &renderer,
            &mut iced::advanced::widget::operation::black_box(&mut collect),
        );
        let snapshot = collect.snapshot();
        assert!(snapshot.duplicate_ids.is_empty());
        for id in ["naming.online-dismiss", "naming.online-confirm"] {
            let node = snapshot.nodes.iter().find(|node| node.id == id).expect(id);
            assert!(node.visible_bounds.is_some(), "{id}");
        }
        assert!(
            snapshot
                .nodes
                .iter()
                .all(|node| node.id != "naming.online-ask")
        );
        let mut activate = Activate::<Message>::new("naming.online-confirm");
        ui.operate(
            &renderer,
            &mut iced::advanced::widget::operation::black_box(&mut activate),
        );
        assert!(
            matches!(activate.message(), Some(Message::Naming(Action::ConfirmOnline(value))) if *value == serial)
        );
    }
    Ok(())
}
