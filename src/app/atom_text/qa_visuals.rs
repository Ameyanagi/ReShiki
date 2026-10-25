//! Matched images from the complete application renderer and actual properties.
use super::qa_ui::Ui;
use super::*;
use crate::app::{InspectorTab, inspector};
use iced::{Size, advanced::mouse};
use reshiki::{
    document::Document,
    engine::{Analysis, ChemistryEngine, Request},
};
use std::path::Path;

fn app(doc: Document) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.grid = false;
    app.tab.keyboard_drawing.leave();
    app.tab.doc = doc;
    app.tab.saved = app.tab.doc.clone();
    app.tab.fit_to_view = false;
    app.tab.camera.zoom = 2.;
    app.appearance.mode = crate::appearance::Mode::Light;
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app.status = "Ready".into();
    app
}
async fn refresh(app: &mut App) -> Analysis {
    let labels =
        reshiki::atom_labels::refresh::Refresh::calculate(&app.tab.doc, &Default::default())
            .unwrap();
    labels.apply(&mut app.tab.doc);
    let key = app.property_request_key();
    let request = Request::molecule("analyze", app.property_document(&key));
    let _ = app.inspector_action(inspector::Action::RefreshProperties);
    let result = app.engine.execute(request).await.and_then(|response| {
        response
            .analysis
            .ok_or_else(|| "Missing molecular properties".into())
    });
    let analysis = result.as_ref().unwrap().clone();
    let _ = app.inspector_action(inspector::Action::PropertiesCalculated(
        key,
        Box::new(result),
    ));
    let _ = app.inspector_action(inspector::Action::Section(
        inspector::Section::Molecule,
        true,
    ));
    analysis
}
fn snapshot(ui: &mut Ui, app: &mut App, directory: &Path, name: &str) {
    for _ in 0..3 {
        ui.redraw(
            app,
            iced::time::Instant::now(),
            mouse::Cursor::Unavailable,
            false,
        );
    }
    app.tab.camera.zoom = 2.;
    let pixels = ui.redraw(
        app,
        iced::time::Instant::now(),
        mouse::Cursor::Unavailable,
        false,
    );
    image::save_buffer(
        directory.join(format!("{name}.png")),
        &pixels,
        ui.size.width as u32,
        ui.size.height as u32,
        image::ColorType::Rgba8,
    )
    .unwrap();
}
fn edit(app: &mut App, id: u64, text: &str) {
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let _ = app.update(Message::AtomText(Action::Input(text.into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
}
fn read(directory: &Path, name: &str) -> Document {
    Document::from_json(&std::fs::read(directory.join(name)).unwrap()).unwrap()
}

#[tokio::test]
#[ignore = "Opt-in matched full-app stereo, polyol and repeat renderer evidence"]
async fn stereo_repeat_visual_evidence() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_STEREO_REPEAT_QA_DIR").expect("Set RESHIKI_STEREO_REPEAT_QA_DIR"),
    );
    let before = std::env::var_os("RESHIKI_STEREO_REPEAT_QA_BASELINE").is_some();
    let prefix = if before { "before" } else { "after" };
    let mut ui = Ui::new(Size::new(1600., 1000.)).await;
    let mut glucose = app(read(&directory, "glucose-input.rsk"));
    let glucose_input = refresh(&mut glucose).await;
    snapshot(
        &mut ui,
        &mut glucose,
        &directory,
        &format!("{prefix}-glucose-input"),
    );
    edit(&mut glucose, 10, "OAc");
    let glucose_result = refresh(&mut glucose).await;
    assert_eq!(
        glucose
            .tab
            .doc
            .atoms
            .iter()
            .filter(|atom| atom.stereo.is_some())
            .count(),
        if before { 4 } else { 5 }
    );
    snapshot(
        &mut ui,
        &mut glucose,
        &directory,
        &format!("{prefix}-glucose-acetate"),
    );
    std::fs::write(
        directory.join(format!("{prefix}-glucose-acetate.rsk")),
        glucose.tab.doc.file_json().unwrap(),
    )
    .unwrap();

    let mut polyol = app(read(&directory, "pentaerythritol-input.rsk"));
    let _ = polyol.update(Message::Tool(crate::canvas::Tool::Text));
    edit(&mut polyol, 7, "OAc");
    for id in [1, 9] {
        let _ = polyol.update(Message::AtomText(Action::Repeat(id)));
        assert!(polyol.tab.atom_text.is_none());
    }
    let _ = refresh(&mut polyol).await;
    snapshot(
        &mut ui,
        &mut polyol,
        &directory,
        &format!("{prefix}-polyol-three"),
    );
    let last = polyol.tab.doc.atom(5).unwrap().position;
    let hit = reshiki::scene::atom_label_hit(&polyol.tab.doc, last, 0.);
    assert_eq!(hit, Some(if before { 9 } else { 5 }));
    let _ = polyol.update(Message::Canvas(crate::canvas::Edit::RepeatLabel(last)));
    if polyol.tab.atom_text.is_some() {
        let _ = polyol.update(Message::AtomText(Action::Cancel));
    }
    let polyol_result = refresh(&mut polyol).await;
    assert_eq!(
        polyol.tab.doc.abbreviations.len(),
        if before { 3 } else { 4 }
    );
    snapshot(
        &mut ui,
        &mut polyol,
        &directory,
        &format!("{prefix}-polyol-result"),
    );
    std::fs::write(
        directory.join(format!("{prefix}-polyol-result.rsk")),
        polyol.tab.doc.file_json().unwrap(),
    )
    .unwrap();

    let mut durene = app(read(&directory, "durene-input.rsk"));
    let _ = durene.update(Message::Tool(crate::canvas::Tool::Text));
    edit(&mut durene, 7, "OMe");
    for id in [10, 1] {
        let _ = durene.update(Message::AtomText(Action::Repeat(id)));
    }
    let _ = refresh(&mut durene).await;
    snapshot(
        &mut ui,
        &mut durene,
        &directory,
        &format!("{prefix}-durene-three"),
    );
    let point = durene.tab.doc.atom(5).unwrap().position;
    let _ = durene.update(Message::Canvas(crate::canvas::Edit::Click(point)));
    let stamp = durene.text_click.unwrap().0;
    let pixels = ui.redraw(
        &mut durene,
        stamp + std::time::Duration::from_millis(200),
        mouse::Cursor::Unavailable,
        true,
    );
    image::save_buffer(
        directory.join(format!("{prefix}-repeat-200ms.png")),
        &pixels,
        1600,
        1000,
        image::ColorType::Rgba8,
    )
    .unwrap();
    durene.text_click = Some((iced::time::Instant::now(), 5));
    let _ = durene.update(Message::Canvas(crate::canvas::Edit::Click(point)));
    assert!(durene.tab.atom_text.is_none());
    let durene_result = refresh(&mut durene).await;
    snapshot(
        &mut ui,
        &mut durene,
        &directory,
        &format!("{prefix}-durene-result"),
    );
    let record = serde_json::json!({"glucose_input":glucose_input,"glucose_result":glucose_result,"polyol_fourth_oxygen_hit":hit,"polyol_result":polyol_result,"durene_result":durene_result});
    std::fs::write(
        directory.join(format!("{prefix}-properties.json")),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
}
