use super::super::test_support::{ethanol, record};
use super::*;

fn atom_ids(doc: &Document) -> Vec<u64> {
    doc.atoms.iter().map(|atom| atom.id).collect()
}

#[test]
fn clicked_molecule_is_independent_of_multi_selection_and_logical_groups() {
    let mut doc = ethanol();
    let first = atom_ids(&doc);
    let other = editing::append(&mut doc, &ethanol(), Point::new(250., 0.));
    let all = atom_ids(&doc);
    doc.group_selection(&all).unwrap();
    assert_eq!(target(&doc, &[first[0]], &all), Some(first.clone()));
    assert_eq!(target(&doc, &[other[0]], &all), Some(other));
    assert_eq!(target(&doc, &[], &all), None);
    let arrow = doc.next_id();
    doc.arrows.push(reshiki::document::Arrow {
        id: arrow,
        start: Point::new(400., 0.),
        end: Point::new(440., 0.),
        kind: "forward".into(),
        control: None,
        cubic: None,
        start_anchor: None,
        end_anchor: None,
        style: None,
    });
    assert_eq!(target(&doc, &[arrow], &first), None);
}

#[test]
fn caption_completion_keeps_original_target_when_selection_changes() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = ethanol();
    let first = atom_ids(&app.tab.doc);
    let other = editing::append(&mut app.tab.doc, &ethanol(), Point::new(250., 0.));
    app.tab.selected = other;
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::ShowMoleculeName(first.clone()));
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    ));
    assert_eq!(app.tab.doc.molecule_names[0].atoms, first);
    assert_eq!(app.tab.doc.annotations.len(), 1);
    let shown = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, shown);
}

#[test]
fn stale_or_unsupported_completion_changes_no_drawing_or_history() {
    for stale in [true, false] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = ethanol();
        let ids = atom_ids(&app.tab.doc);
        let _ = app.naming_action(Action::ShowMoleculeName(ids));
        let ticket = app.tab.naming.pending.unwrap().ticket();
        let before = app.tab.doc.clone();
        let result = if stale {
            app.tab.revision += 1;
            Ok(Outcome::Structure(record("CCO")))
        } else {
            Err(naming::Error::Rejected(
                "openclatura: isotopic naming not ported".into(),
            ))
        };
        let _ = app.naming_action(Action::Finished(ticket, Box::new(result)));
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        assert!(app.error);
        assert!(!app.status.is_empty());
        assert!(app.tab.naming.notice.is_none());
        assert!(app.tab.naming.structure_error.is_none());
        if stale {
            assert_eq!(
                app.status,
                "The molecule changed while naming. Show its name again"
            );
        } else {
            assert_eq!(app.status, "openclatura: isotopic naming not ported");
        }
    }
}

#[tokio::test]
async fn actual_local_worker_name_is_accepted_only_for_the_target_graph() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = ethanol();
    let ids = atom_ids(&app.tab.doc);
    let _ = app.naming_action(Action::ShowMoleculeName(ids.clone()));
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let identity = naming::selected_identity(&app.tab.doc, &ids)?;
    let generated = naming::generate_name(&identity, naming::Cancel::default())
        .await
        .map_err(|error| error.to_string())?;
    assert_eq!(generated.systematic_name.as_deref(), Some("ethanol"));
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Structure(generated))),
    ));
    assert_eq!(app.tab.doc.annotations[0].text, "ethanol");
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCO");
    Ok(())
}
