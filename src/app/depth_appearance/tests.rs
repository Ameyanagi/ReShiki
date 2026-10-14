use super::*;
use crate::app::{Document, History, Point, chemistry_changed};

#[test]
fn rear_opacity_draft_apply_is_one_undo_and_rejects_stale_invalid_or_late_input()
-> Result<(), String> {
    use super::{Action, Draft};
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.atom_mut(a).unwrap().depth = -21.;
    app.tab.doc.atom_mut(b).unwrap().depth = 21.;
    app.tab.selected = vec![a];
    app.tab.history = History::default();
    app.tab.labels_dirty = false;
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let draft = |app: &App, value: &str| Draft {
        ids: app.depth_ids(),
        revision: app.tab.revision,
        file_epoch: app.tab.file_epoch,
        value: value.into(),
    };
    for value in ["nan", "101", "-1", ""] {
        let input = draft(&app, value);
        let _ = app.update(Message::DepthAppearance(Action::RearInput(input)));
        let _ = app.update(Message::DepthAppearance(Action::ApplyRear));
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
    let input = draft(&app, "100");
    let _ = app.update(Message::DepthAppearance(Action::RearInput(input)));
    let _ = app.update(Message::DepthAppearance(Action::ApplyRear));
    assert!(!app.error);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let stale = draft(&app, "10");
    let input = draft(&app, "25");
    let _ = app.update(Message::DepthAppearance(Action::RearInput(input)));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    let _ = app.update(Message::DepthAppearance(Action::ApplyRear));
    assert!(!app.error, "{}", app.status);
    let edited = app.tab.doc.clone();
    assert_eq!(
        reshiki::depth_appearance::rear_opacity(&edited, &[a, b]),
        Some(0.25)
    );
    assert_eq!(edited.atoms, before.atoms);
    assert_eq!(edited.bonds, before.bonds);
    assert!(!chemistry_changed(&before, &edited));
    assert_eq!(app.tab.revision, revision + 1);
    assert_eq!(app.tab.history.frames(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, edited);
    let _ = app.update(Message::DepthAppearance(Action::RearInput(stale)));
    let _ = app.update(Message::DepthAppearance(Action::ApplyRear));
    assert_eq!(app.tab.doc, edited);
    assert_eq!(app.tab.history.frames(), 1);
    let input = draft(&app, "0");
    let _ = app.update(Message::DepthAppearance(Action::RearInput(input)));
    app.tab.file_epoch += 1;
    let _ = app.update(Message::DepthAppearance(Action::ApplyRear));
    assert_eq!(app.tab.doc, edited);
    Ok(())
}

#[test]
fn rgb_first_coordinate_contact_rear_apply_targets_one_molecule_and_retains_history() {
    use super::{Action, Draft};
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let mut molecules = Vec::new();
    for (x, depths) in [
        (0., [-30., -10., 10., 30.]),
        (400., [100., 120., 140., 160.]),
    ] {
        let mut ids = Vec::new();
        for (i, z) in depths.into_iter().enumerate() {
            let id = app
                .tab
                .doc
                .add_atom("C", Point::new(x + i as f32 * 42., 0.));
            app.tab.doc.atom_mut(id).unwrap().depth = z;
            if let Some(previous) = ids.last() {
                app.tab.doc.add_bond(*previous, id, 1, "plain");
            }
            ids.push(id);
        }
        molecules.push(ids);
    }
    app.tab
        .doc
        .add_bond(molecules[0][3], molecules[1][0], 5, "plain");
    app.tab.selected.clear();
    app.tab.labels_dirty = false;
    let _ = app.update(Message::DepthAppearance(Action::Enhance(true)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.depth_appearance.len(), 1);
    let rgb_first = app.tab.doc.clone();
    app.tab.history = History::default();
    app.tab.selected = vec![molecules[0][0]];
    assert_eq!(app.depth_ids(), molecules[0]);
    let draft = Draft {
        ids: app.depth_ids(),
        revision: app.tab.revision,
        file_epoch: app.tab.file_epoch,
        value: "25".into(),
    };
    let _ = app.update(Message::DepthAppearance(Action::RearInput(draft)));
    let _ = app.update(Message::DepthAppearance(Action::ApplyRear));
    assert!(!app.error, "{}", app.status);
    let edited = app.tab.doc.clone();
    assert_eq!(
        reshiki::depth_appearance::rear_opacity(&edited, &molecules[0]),
        Some(0.25)
    );
    assert_eq!(
        reshiki::depth_appearance::rear_opacity(&edited, &molecules[1]),
        Some(1.)
    );
    assert_eq!(edited.atoms, rgb_first.atoms);
    assert_eq!(edited.bonds, rgb_first.bonds);
    for id in edited.all_ids() {
        assert_eq!(
            reshiki::depth_appearance::Paint::new(&edited).amount(id),
            reshiki::depth_appearance::Paint::new(&rgb_first).amount(id)
        );
    }
    assert_eq!(app.tab.history.frames(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, rgb_first);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, edited);
    app.tab.selected.clear();
    for on in [false, true] {
        let _ = app.update(Message::DepthAppearance(Action::Enhance(on)));
        assert!(!app.error, "{}", app.status);
        assert_eq!(
            reshiki::depth_appearance::rear_opacity(&app.tab.doc, &molecules[0]),
            Some(0.25)
        );
        assert_eq!(
            reshiki::depth_appearance::rear_opacity(&app.tab.doc, &molecules[1]),
            Some(1.)
        );
    }
    let reopened = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.depth_appearance, app.tab.doc.depth_appearance);
}

#[test]
fn rear_target_selection_includes_quadruple_and_partial_covalent_bonds() {
    for order in [6, 7] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, order, "plain");
        app.tab.selected = vec![a];
        assert_eq!(app.depth_ids(), [a, b]);
    }
}
