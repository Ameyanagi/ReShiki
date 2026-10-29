use super::*;

fn crystal() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::crystal::read(
        "cif",
        include_str!("../../../tests/fixtures/crystal/co2.cif"),
    )
    .unwrap()
    .0;
    app.tab.selected = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    models3d::set_display(&mut app.tab.doc, 0, Display::ThreeD);
    app.tab.saved = app.tab.doc.clone();
    app
}

#[test]
fn ortep_settings_use_the_document_transaction_and_round_trip_with_undo_and_redo() {
    let mut app = crystal();
    let original = app.tab.doc.clone();
    let model = &original.models[0];
    assert_eq!(model.probability, Probability::Fifty);
    assert!(!model.molecule_only);
    assert!(model.include_solvent);
    let mut states = vec![original.clone()];
    for setting in [
        Setting::Style(Style::Ellipsoids),
        Setting::Probability(Probability::Thirty),
        Setting::Probability(Probability::Seventy),
        Setting::Probability(Probability::Ninety),
        Setting::MoleculeOnly(true),
        Setting::IncludeSolvent(false),
        Setting::MoleculeOnly(false),
        Setting::MoleculeOnly(true),
    ] {
        let revision = app.tab.revision;
        let _ = app.update(Message::Models3d(Action::Set(setting)));
        assert!(!app.error, "{setting:?}: {}", app.status);
        assert_eq!(app.tab.history.undo_frames().count(), states.len());
        assert_eq!(app.tab.revision, revision + 1);
        assert_eq!(
            app.tab.doc.version,
            models3d::required_version(&app.tab.doc.models).unwrap()
        );
        assert_eq!(app.tab.doc.version, reshiki::document::ORTEP);
        assert_eq!(app.tab.doc.file_version(), reshiki::document::ORTEP);
        let json = app.tab.doc.file_json().unwrap();
        assert_eq!(Document::from_json(&json).unwrap(), app.tab.doc);
        assert_eq!(app.tab.doc.atoms, original.atoms);
        assert_eq!(app.tab.doc.bonds, original.bonds);
        assert_eq!(app.tab.doc.models[0].cell, model.cell);
        assert_eq!(app.tab.doc.models[0].sites, model.sites);
        assert_eq!(app.tab.doc.models[0].atoms, model.atoms);
        states.push(app.tab.doc.clone());

        // Choosing the active control again adds no transaction or Undo step.
        let _ = app.update(Message::Models3d(Action::Set(setting)));
        assert_eq!(app.tab.revision, revision + 1);
        assert_eq!(app.tab.history.undo_frames().count(), states.len() - 1);
    }
    assert!(!app.tab.doc.models[0].include_solvent);
    for expected in states.iter().rev().skip(1) {
        let _ = app.update(Message::Undo);
        assert_eq!(&app.tab.doc, expected);
    }
    assert!(!app.tab.history.can_undo());
    for expected in states.iter().skip(1) {
        let _ = app.update(Message::Redo);
        assert_eq!(&app.tab.doc, expected);
    }
}

#[test]
fn ortep_active_defaults_and_busy_actions_leave_the_document_unchanged() {
    let mut app = crystal();
    let original = app.tab.doc.clone();
    let revision = app.tab.revision;
    for setting in [
        Setting::Probability(Probability::Fifty),
        Setting::MoleculeOnly(false),
        Setting::IncludeSolvent(true),
    ] {
        let _ = app.update(Message::Models3d(Action::Set(setting)));
    }
    app.tab.busy = true;
    for setting in [
        Setting::Style(Style::Ellipsoids),
        Setting::Probability(Probability::Ninety),
        Setting::MoleculeOnly(true),
        Setting::IncludeSolvent(false),
    ] {
        let _ = app.update(Message::Models3d(Action::Set(setting)));
    }
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
}

fn semantics(app: &App, renderer: &mut iced::Renderer) -> reshiki::accessibility::Snapshot {
    use iced::advanced::widget::operation;
    let size = iced::Size::new(288., 900.);
    let mut ui = iced_runtime::UserInterface::build(
        app.model_section().expect("3D model section"),
        size,
        Default::default(),
        renderer,
    );
    let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
    ui.operate(renderer, &mut operation::black_box(&mut collect));
    collect.snapshot().clone()
}

fn activate(app: &App, renderer: &mut iced::Renderer, id: &str) -> Setting {
    use iced::advanced::widget::{Operation, operation};
    let mut ui = iced_runtime::UserInterface::build(
        app.model_section().expect("3D model section"),
        iced::Size::new(288., 900.),
        Default::default(),
        renderer,
    );
    let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
    ui.operate(renderer, &mut operation::black_box(&mut activate));
    match activate.finish() {
        operation::Outcome::Some(Message::Models3d(Action::Set(setting))) => setting,
        _ => panic!("{id} did not offer a model setting"),
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer ORTEP panel availability and actions"]
async fn ortep_panel_offers_only_available_controls_and_their_live_actions() {
    use iced::advanced::renderer::Headless;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless ORTEP panel renderer");

    for has_cell in [false, true] {
        for has_adp in [false, true] {
            for style in [Style::BallStick, Style::Ellipsoids] {
                for molecule_only in [false, true].into_iter().filter(|only| has_cell || !only) {
                    let mut app = crystal();
                    let model = &mut app.tab.doc.models[0];
                    if !has_cell {
                        model.cell = None;
                    }
                    if !has_adp {
                        for site in &mut model.sites {
                            site.adp = None;
                        }
                    }
                    model.style = style;
                    model.molecule_only = molecule_only;
                    let before = app.tab.doc.clone();
                    let snapshot = semantics(&app, &mut renderer);
                    assert!(snapshot.duplicate_ids.is_empty());
                    let node = |id: &str| snapshot.nodes.iter().find(|node| node.id == id);
                    for id in ["ball-stick", "stick", "wireframe", "space-filling"] {
                        assert!(node(&format!("model-style-{id}")).is_some());
                    }
                    assert_eq!(node("model-style-ellipsoids").is_some(), has_adp);
                    for percent in [30, 50, 70, 90] {
                        let probability = node(&format!("model-probability-{percent}"));
                        assert_eq!(probability.is_some(), style == Style::Ellipsoids);
                        if let Some(probability) = probability {
                            assert_eq!(probability.checked, Some(percent == 50));
                            assert_eq!(
                                probability.name,
                                format!("{percent}% ellipsoid probability")
                            );
                        }
                    }
                    for suffix in ["off", "on"] {
                        assert_eq!(
                            node(&format!("model-molecule-only-{suffix}")).is_some(),
                            has_cell
                        );
                        assert_eq!(
                            node(&format!("model-include-solvent-{suffix}")).is_some(),
                            molecule_only
                        );
                    }
                    if molecule_only {
                        assert_eq!(
                            node("model-include-solvent-on").unwrap().checked,
                            Some(true)
                        );
                    }
                    assert_eq!(app.tab.doc, before, "Inspecting controls is read-only");
                }
            }
        }
    }

    let mut app = crystal();
    let mut choices = vec![(
        "model-style-ellipsoids".into(),
        Setting::Style(Style::Ellipsoids),
    )];
    choices.extend(Probability::ALL.map(|probability| {
        (
            format!("model-probability-{}", probability.percent()),
            Setting::Probability(probability),
        )
    }));
    choices.extend([
        ("model-molecule-only-on".into(), Setting::MoleculeOnly(true)),
        (
            "model-include-solvent-off".into(),
            Setting::IncludeSolvent(false),
        ),
        (
            "model-include-solvent-on".into(),
            Setting::IncludeSolvent(true),
        ),
        (
            "model-molecule-only-off".into(),
            Setting::MoleculeOnly(false),
        ),
    ]);
    for (id, expected) in choices {
        let setting = activate(&app, &mut renderer, &id);
        assert_eq!(setting, expected);
        let _ = app.update(Message::Models3d(Action::Set(setting)));
        assert!(!app.error, "{id}: {}", app.status);
    }
    app.tab.busy = true;
    let snapshot = semantics(&app, &mut renderer);
    for node in snapshot
        .nodes
        .iter()
        .filter(|node| node.id.starts_with("model-"))
    {
        assert!(!node.enabled, "{} stays disabled while busy", node.id);
    }

    // Follow the existing opt-in layout harness: actual application widgets
    // and renderer pixels, saved only when an evidence directory is requested.
    if let Some(output) = std::env::var_os("RESHIKI_ORTEP_QA_DIR") {
        use crate::app::workspace::layout_snapshots::pass;
        use iced::advanced::widget::Tree;
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        for (name, has_adp, molecule_only) in [
            ("ortep-packing-controls", true, false),
            ("ortep-molecule-controls", true, true),
            ("ortep-no-adp-controls", false, false),
        ] {
            let mut app = crystal();
            app.appearance.mode = crate::appearance::Mode::Light;
            app.inspector_open = true;
            app.inspector_tab = crate::app::InspectorTab::Properties;
            let _ = app.update(Message::InspectorAction(
                crate::app::inspector::Action::ModelControls(true),
            ));
            if !has_adp {
                for site in &mut app.tab.doc.models[0].sites {
                    site.adp = None;
                }
            } else {
                let _ = app.update(Message::Models3d(Action::Set(Setting::Style(
                    Style::Ellipsoids,
                ))));
            }
            let _ = app.update(Message::Models3d(Action::Set(Setting::MoleculeOnly(
                molecule_only,
            ))));
            let size = iced::Size::new(1600., 1000.);
            let mut tree = Tree::empty();
            for _ in 0..3 {
                let messages = pass(&app, &mut renderer, &mut tree, size, None);
                if messages.is_empty() {
                    break;
                }
                for message in messages {
                    let _ = app.update(message);
                }
            }
            if let Some((lo, hi)) =
                reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected)
            {
                app.tab.camera.center =
                    reshiki::document::Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
            }
            pass(
                &app,
                &mut renderer,
                &mut tree,
                size,
                Some(&output.join(format!("{name}.png"))),
            );
        }
    }
}
