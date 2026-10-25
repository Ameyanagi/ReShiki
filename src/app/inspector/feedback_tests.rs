use super::*;
use crate::app::atom_text;
use reshiki::document::Point;

fn ethane(app: &mut App, x: f32) -> [u64; 2] {
    let a = app.tab.doc.add_atom("C", Point::new(x, 0.));
    let b = app.tab.doc.add_atom("C", Point::new(x + 42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    [a, b]
}

async fn refresh(app: &mut App) -> Result<Analysis, String> {
    let key = app.property_request_key();
    let request = Request::molecule("analyze", app.property_document(&key));
    let _ = app.inspector_action(Action::RefreshProperties);
    let result = app.engine.execute(request).await.and_then(|response| {
        response
            .analysis
            .ok_or_else(|| "No molecular properties were returned.".into())
    });
    let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(result.clone())));
    result
}

#[tokio::test]
async fn generic_choices_defer_properties_but_an_ordinary_selection_still_calculates() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let generic = ethane(&mut app, 0.);
    app.tab.doc = reshiki::atom_text::apply(
        &app.tab.doc,
        generic[0],
        "[Cl,Br,I]",
        reshiki::atom_text::Mode::Auto,
    )
    .unwrap();
    let ordinary = ethane(&mut app, 200.);
    let source = app.tab.doc.clone();
    let error = refresh(&mut app).await.unwrap_err();
    assert_eq!(
        deferred_generic_properties(&error),
        Some(GENERIC_PROPERTIES)
    );
    assert!(app.property_analysis().is_none());
    // The Molecular properties Check structure button takes this same path.
    assert_eq!(
        deferred_generic_properties(&refresh(&mut app).await.unwrap_err()),
        Some(GENERIC_PROPERTIES)
    );
    app.tab.selected = ordinary.to_vec();
    assert!(!reshiki::generic::present(
        &app.property_document(&app.property_request_key())
    ));
    let analysis = refresh(&mut app).await.unwrap();
    assert_eq!(
        (analysis.formula.as_str(), analysis.smiles.as_str()),
        ("C2H6", "CC")
    );
    assert_eq!(app.tab.doc, source);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
async fn genuine_valence_errors_are_not_neutralized_by_generic_content_elsewhere() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let generic = ethane(&mut app, 0.);
    app.tab.doc = reshiki::atom_text::apply(
        &app.tab.doc,
        generic[0],
        "[Cl,Br,I]",
        reshiki::atom_text::Mode::Auto,
    )
    .unwrap();
    let central = app.tab.doc.add_atom("C", Point::new(200., 0.));
    let mut invalid = vec![central];
    for i in 0..5 {
        let angle = std::f32::consts::TAU * i as f32 / 5.;
        let id = app
            .tab
            .doc
            .add_atom("F", Point::new(200. + angle.cos() * 42., angle.sin() * 42.));
        app.tab.doc.add_bond(central, id, 1, "plain");
        invalid.push(id);
    }
    app.tab.selected = invalid;
    let error = refresh(&mut app).await.unwrap_err();
    assert!(error.to_ascii_lowercase().contains("valence"), "{error}");
    assert_eq!(deferred_generic_properties(&error), None);
    for error in [
        "Invalid drawing: Malformed attachment",
        "Generic choices retained",
        "No molecular properties were returned.",
    ] {
        assert_eq!(deferred_generic_properties(error), None);
    }
}

#[tokio::test]
async fn changed_status_stays_accurate_when_properties_arrive_and_preserves_newer_feedback() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let [_, end] = ethane(&mut app, -42.);
    let _ = app.update(Message::AtomText(atom_text::Action::Begin(Some(end))));
    let _ = app.update(Message::AtomText(atom_text::Action::Input("CONMe2".into())));
    let _ = app.update(Message::AtomText(atom_text::Action::Apply));
    assert_eq!(app.status, "Drawing changed");
    app.tab.selected.clear();
    let key = app.property_request_key();
    let request = Request::molecule("analyze", app.property_document(&key));
    let _ = app.inspector_action(Action::RefreshProperties);
    app.status = "Newer feedback".into();
    let analysis = app.engine.execute(request).await.unwrap().analysis.unwrap();
    assert_eq!(analysis.formula, "C4H9NO");
    let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(analysis))));
    assert_eq!(app.property_analysis().unwrap().formula, "C4H9NO");
    assert_eq!(app.status, "Newer feedback");
}
