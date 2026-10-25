//! Matched real renderer fixtures for molecular Properties continuity.
use super::*;
use crate::app::inspector::{Action, Section};
use reshiki::engine::{ChemistryEngine, Request};

async fn imported(smiles: &str) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.appearance.mode = crate::appearance::Mode::Light;
    let result = app
        .engine
        .execute(Request::import_smiles(smiles))
        .await
        .unwrap();
    app.tab.doc = result.document.unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.analysis = result.analysis;
    app.tab.camera.zoom = 1.;
    let (lo, hi) = app.tab.doc.bounds();
    app.tab.camera.center = World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    app
}

async fn properties(app: &mut App) {
    let key = app.property_request_key();
    let _ = app.inspector_action(Action::RefreshProperties);
    let result = app
        .engine
        .execute(Request::molecule("analyze", app.property_document(&key)))
        .await
        .unwrap()
        .analysis
        .unwrap();
    let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(result))));
}

#[tokio::test]
#[ignore = "Opt-in matched Properties renderer evidence; set RESHIKI_PROPERTY_QA_DIR"]
async fn property_continuity_snapshots() {
    let directory = std::env::var_os("RESHIKI_PROPERTY_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-property-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1600., 1000.);
    let mut app = imported("CCc1ccccc1").await;
    std::fs::write(
        directory.join("ethylbenzene.rsk"),
        serde_json::to_string_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    let first = app.tab.doc.atoms[0].id;
    let second = app.tab.doc.atoms[1].id;
    let _ = app.context_key_at("2", reshiki::keyboard_drawing::Target::Bond(first, second));
    let selected = std::mem::take(&mut app.tab.selected);
    properties(&mut app).await;
    app.tab.selected = selected;
    properties(&mut app).await;
    snapshot(
        &mut app,
        &mut renderer,
        size,
        &directory.join("styrene-default.png"),
    );
    for expanded in [true, false] {
        let _ = app.update(Message::InspectorAction(Action::Section(
            Section::Molecule,
            expanded,
        )));
        snapshot(
            &mut app,
            &mut renderer,
            size,
            &directory.join(format!(
                "styrene-{}.png",
                if expanded { "expanded" } else { "collapsed" }
            )),
        );
    }
    let mut app = imported("O=Cc1ccccc1").await;
    std::fs::write(
        directory.join("benzaldehyde.rsk"),
        serde_json::to_string_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    for (name, tool) in [
        ("hydrogen-tool", Tool::Atom),
        (
            "benzene-tool",
            Tool::RingPreset(reshiki::rings::Preset::Benzene),
        ),
    ] {
        app.tool = tool;
        app.element = "H".into();
        snapshot(
            &mut app,
            &mut renderer,
            size,
            &directory.join(format!("{name}.png")),
        );
    }
    let mut app = imported("CCc1ccccc1").await;
    app.tab.selected = vec![app.tab.doc.atoms[0].id];
    let _ = app.update(Message::InspectorAction(Action::Section(
        Section::Molecule,
        true,
    )));
    properties(&mut app).await;
    snapshot(
        &mut app,
        &mut renderer,
        size,
        &directory.join("one-atom.png"),
    );
    let mut app = imported("COc1ccccc1").await;
    std::fs::write(
        directory.join("anisole.rsk"),
        serde_json::to_string_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    app.tab.selected = app.tab.doc.atoms[..2].iter().map(|a| a.id).collect();
    let _ = app.update(Message::InspectorAction(Action::Section(
        Section::Molecule,
        true,
    )));
    properties(&mut app).await;
    snapshot(
        &mut app,
        &mut renderer,
        size,
        &directory.join("two-atoms-one-bond.png"),
    );
    let _ = app.update(Message::InspectorAction(Action::Section(
        Section::Molecule,
        true,
    )));
    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
    snapshot(
        &mut app,
        &mut renderer,
        size,
        &directory.join("replacement-tab.png"),
    );
    eprintln!("Properties evidence: {}", directory.display());
}
