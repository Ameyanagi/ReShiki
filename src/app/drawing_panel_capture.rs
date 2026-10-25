//! Opt-in real-renderer evidence for the drawing-panel state transitions.
use super::{App, InspectorTab, Message, abbreviations, tabs};
use iced::advanced::{renderer::Headless, widget::Tree};
use reshiki::{
    document::{Document, Point},
    engine::{LocalEngine, Request},
};

fn snapshot(app: &mut App, renderer: &mut iced::Renderer, path: &std::path::Path) {
    let size = iced::Size::new(1280., 820.);
    let mut tree = Tree::empty();
    for _ in 0..3 {
        let messages =
            super::workspace::layout_snapshots::pass(app, renderer, &mut tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
    let _ = super::workspace::layout_snapshots::pass(app, renderer, &mut tree, size, Some(path));
}

#[tokio::test]
#[ignore = "Capture under isolated RESHIKI_DATA_DIR with RESHIKI_PANEL_QA_DIR set"]
async fn drawing_panel_state_renderer_evidence() -> anyhow::Result<()> {
    let output =
        std::path::PathBuf::from(std::env::var_os("RESHIKI_PANEL_QA_DIR").expect("Capture folder"));
    std::env::var_os("RESHIKI_DATA_DIR").expect("Isolated application data folder");
    std::fs::create_dir_all(&output)?;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Renderer");
    let engine = LocalEngine::default();
    let piperazine = engine
        .request(Request::import_smiles("CN1CCN(c2ccccc2)CC1"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .expect("Piperazine");
    let propane = engine
        .request(Request::import_smiles("CCC"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .expect("Propane");
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.inspector_open = true;
    app.show_opened(propane, None);
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![2])));
    let _ = app.update(Message::Abbreviations(abbreviations::Action::Label(
        "Bridge".into(),
    )));
    let _ = app.update(Message::Abbreviations(abbreviations::Action::Define));
    let _ = app.update(Message::New);
    app.show_opened(piperazine.clone(), None);
    let _ = app.update(Message::KeyboardDrawing(
        super::keyboard_drawing::Action::Leave,
    ));
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![
        1, 2, 3, 4, 5, 12, 13,
    ])));
    let _ = app.update(Message::Inspector(InspectorTab::Abbreviations));
    app.tab.camera.zoom = 2.;
    app.tab.fit_to_view = false;
    snapshot(&mut app, &mut renderer, &output.join("fragment-fields.png"));
    let fields = serde_json::json!({"label":app.abbreviations.label,"reverse":app.abbreviations.reverse_label});
    let _ = app.update(Message::Abbreviations(abbreviations::Action::Label(
        "MePz".into(),
    )));
    let _ = app.update(Message::Abbreviations(abbreviations::Action::ReverseLabel(
        "PzMe".into(),
    )));
    let _ = app.update(Message::Abbreviations(abbreviations::Action::Define));
    assert!(!app.error, "{}", app.status);
    snapshot(&mut app, &mut renderer, &output.join("define-status.png"));
    let saved = app.status.clone();
    let _ = app.update(Message::New);
    let mut ethane = Document::default();
    let a = ethane.add_atom("C", Point::new(-21., 0.));
    let b = ethane.add_atom("C", Point::new(21., 0.));
    ethane.add_bond(a, b, 1, "plain");
    let before = std::mem::replace(&mut app.tab.doc, ethane.clone());
    app.changed(before);
    let _ = app.update(Message::Inspector(InspectorTab::Reactions));
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    let _ = app.update(Message::Discard);
    // Close the other scratch drawings too, so the last close creates Untitled.
    while app.strip().count() > 1 {
        let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
        let _ = app.update(Message::Discard);
    }
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    let _ = app.update(Message::Discard);
    let _ = app.update(Message::KeyboardDrawing(
        super::keyboard_drawing::Action::Leave,
    ));
    let before = std::mem::replace(&mut app.tab.doc, ethane);
    app.changed(before);
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![a])));
    app.tab.camera.zoom = 2.5;
    app.tab.fit_to_view = false;
    snapshot(
        &mut app,
        &mut renderer,
        &output.join("new-drawing-properties.png"),
    );
    std::fs::write(
        output.join("state.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"fragment_fields":fields,"define_status":saved,"new_drawing_page":format!("{:?}",app.inspector_tab),"renderer":renderer.name(),"window":[1280,820],"zoom":[2.,2.5]}),
        )?,
    )?;
    Ok(())
}
