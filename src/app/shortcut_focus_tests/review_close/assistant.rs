use super::*;
use crate::app::assistant::Draft;
use reshiki::{assistant as assistant_model, document::Document};

async fn assistant_review(ui: &mut Ui) -> App {
    // Keep the setup card, local draft and composer visible together.
    ui.viewport = Rectangle::with_size(Size::new(1280., 1800.));
    let mut app = drawing_app();
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Assistant;
    let mut fragment = Document::default();
    fragment.add_atom("O", Point::default());
    app.assistant.draft = Some(Draft {
        proposal: assistant_model::Proposal::default(),
        fragment,
        review: assistant_model::review::Report {
            summary: "Local draft waiting for review".into(),
            ..Default::default()
        },
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        replace: vec![],
    });
    let bounds = ui.input(&app, "assistant-input").0;
    assert!(ui.viewport.contains(bounds.center()), "{bounds:?}");
    ui.click(&mut app, bounds.center());
    assert!(ui.input(&app, "assistant-input").1);
    ui.key(
        &mut app,
        Key::Character("x".into()),
        Code::KeyX,
        Modifiers::empty(),
        Some("x"),
    )
    .await;
    assert_eq!(app.assistant.input_text(), "x");
    assert!(app.assistant.draft.is_some());
    app
}

/// These ordinary Iced buttons currently require a pointer. The real click
/// must release the composer before the next drawing shortcut is delivered.
async fn activate_assistant_review(ui: &mut Ui, app: &mut App, label: &'static str) {
    assert!(ui.input(app, "assistant-input").1);
    let bounds = ui.text(app, label);
    assert!(ui.viewport.contains(bounds.center()), "{label}: {bounds:?}");
    for event in [
        mouse::Event::CursorMoved {
            position: bounds.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let (_, messages) = ui.event(
            app,
            Event::Mouse(event),
            mouse::Cursor::Available(bounds.center()),
        );
        for message in messages {
            let task = app.update(message);
            ui.run_task(app, task).await;
        }
    }
    assert!(app.assistant.draft.is_none(), "{label}: local draft closed");
    assert_eq!(app.assistant.input_text(), "x");
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_assistant_apply_pointer() {
    let mut ui = Ui::new().await;
    let mut app = assistant_review(&mut ui).await;
    let drawing = app.tab.doc.clone();
    activate_assistant_review(&mut ui, &mut app, "Apply").await;
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), drawing.atoms.len() + 1);
    let [inserted] = app.tab.selected.as_slice() else {
        panic!("the applied draft is selected: {:?}", app.tab.selected);
    };
    assert!(!drawing.all_ids().contains(inserted));
    assert_eq!(app.tab.doc.atom(*inserted).unwrap().element, "O");
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert_canvas_delete(&mut ui, &mut app).await;
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_assistant_discard_pointer() {
    let mut ui = Ui::new().await;
    let mut app = assistant_review(&mut ui).await;
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    activate_assistant_review(&mut ui, &mut app, "Discard").await;
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selected);
    assert!(!app.tab.history.can_undo());
    assert_canvas_delete(&mut ui, &mut app).await;
}
