use super::*;
use reshiki::document::Point;
fn app() -> App {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("O", Point::new(63., 36.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    app.tab.selected = vec![b];
    app
}
#[test]
fn pending_prediction_survives_move_but_chemical_edits_reject_stale_result() {
    let mut app = app();
    let _ = app.nmr_action(Action::Predict(Nucleus::H1));
    let key = app.tab.nmr.pending.clone().unwrap();
    let request = engine::prepare(&app.tab.doc, &app.tab.selected).unwrap();
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].position.x += 42.;
    app.changed(before);
    assert_eq!(app.tab.nmr.pending.as_ref(), Some(&key));
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].element = "N".into();
    app.changed(before);
    assert!(app.tab.nmr.pending.is_none());
    let report = engine::Report {
        nucleus: Nucleus::H1,
        atom_ids: request.atom_ids,
        fingerprint: request.fingerprint,
        rows: vec![],
        method: String::new(),
        data_version: String::new(),
        conditions: String::new(),
        limitations: String::new(),
        attribution: String::new(),
    };
    let _ = app.nmr_action(Action::Finished(key, Ok(Arc::new(report))));
    assert!(app.tab.nmr.result.is_none());
}
#[test]
fn foreground_or_background_result_never_lands_in_another_tab() {
    use crate::app::tabs::tests::Front;
    let mut app = app();
    let _ = app.nmr_action(Action::Predict(Nucleus::H1));
    let key = app.tab.nmr.pending.clone().unwrap();
    let id = app.tab.id;
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Nmr(Action::Finished(
            key,
            Err("Expected dataset diagnostic".into()),
        ))),
    ));
    front.assert_unchanged(&app);
    assert!(app.tabs.background[0].nmr.pending.is_none());
    assert_eq!(
        app.tabs.background[0].nmr.notice.as_deref(),
        Some("Expected dataset diagnostic")
    );
}
