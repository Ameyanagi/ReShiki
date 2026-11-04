use super::*;
use crate::app::reaction_mapping;
use reshiki::{
    document::{Arrow, Point},
    reaction_mapping::{Budget, propose},
    reactions::{self, Role},
};
use std::sync::Arc;

/// Complete the same local Auto-map job the inspector starts, without waiting
/// for its worker. Both sides are the same three-atom bonded drawing.
async fn mapping_review(ui: &mut Ui) -> App {
    let mut app = drawing_app();
    let reactants = app.tab.selected.clone();
    assert_eq!(reactants, [1, 2, 3]);
    let atoms = app.tab.doc.atoms.clone();
    let bonds = app.tab.doc.bonds.clone();
    let mut products = Vec::new();
    for atom in &atoms {
        products.push(app.tab.doc.add_atom(
            &atom.element,
            Point::new(atom.position.x + 360., atom.position.y),
        ));
    }
    for bond in &bonds {
        let product = |id| products[atoms.iter().position(|atom| atom.id == id).unwrap()];
        app.tab
            .doc
            .add_bond(product(bond.a), product(bond.b), bond.order, &bond.display);
    }
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(140., 0.),
        Point::new(260., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut app.tab.doc, arrow, &reactants, Role::Reactant).unwrap();
    reactions::assign(&mut app.tab.doc, arrow, &products, Role::Product).unwrap();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.saved = app.tab.doc.clone();

    let task = app.update(Message::Mapping(reaction_mapping::Action::Open));
    ui.run_task(&mut app, task).await;
    let proposal = Arc::new(propose(&app.tab.doc, arrow, Budget::default()).unwrap());
    assert_eq!(proposal.pairs.len(), 3);
    // Dropping the returned task leaves the first request pending; Ready below
    // supplies a deterministic result through its normal stale-result guards.
    let _ = app.update(Message::Mapping(reaction_mapping::Action::Auto));
    assert!(app.tab.mapping.is_pending());
    let task = app.update(Message::Mapping(reaction_mapping::Action::Ready(
        app.tab.file_epoch,
        1,
        Ok(proposal),
    )));
    ui.run_task(&mut app, task).await;
    assert!(app.tab.mapping.proposal().is_some());
    assert!(!app.tab.mapping.is_pending());
    assert_eq!(app.tab.selected, app.tab.doc.all_ids());
    app
}

async fn activate_mapping_review(ui: &mut Ui, app: &mut App, id: &'static str) {
    ui.run_task(
        app,
        iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(id)).discard(),
    )
    .await;
    let snapshot = ui.semantics(app);
    let control = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
    assert!(control.enabled && control.focused, "{id}: {control:?}");
    assert!(control.visible_bounds.is_some(), "{id}: {control:?}");
    ui.key(
        app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::empty(),
        None,
    )
    .await;
    assert!(app.tab.mapping.proposal().is_none(), "{id}: review closed");
    assert!(
        ui.semantics(app)
            .nodes
            .iter()
            .all(|node| node.id != "mapping.apply" && node.id != "mapping.discard"),
        "the proposal controls leave the real widget tree"
    );
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_mapping_apply() {
    let mut ui = Ui::new().await;
    let mut app = mapping_review(&mut ui).await;
    let selected = app.tab.selected.clone();
    let atoms = app.tab.doc.atoms.len();
    let bonds = app.tab.doc.bonds.len();
    activate_mapping_review(&mut ui, &mut app, "mapping.apply").await;
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Atom mapping updated");
    assert_eq!(app.tab.doc.atoms.len(), atoms);
    assert_eq!(app.tab.doc.bonds.len(), bonds);
    assert!(app.tab.doc.atoms.iter().all(|atom| atom.map_num != 0));
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert_canvas_delete(&mut ui, &mut app).await;
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_mapping_discard() {
    let mut ui = Ui::new().await;
    let mut app = mapping_review(&mut ui).await;
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    activate_mapping_review(&mut ui, &mut app, "mapping.discard").await;
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Atom-map proposal discarded");
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selected);
    assert!(!app.tab.history.can_undo());
    assert_canvas_delete(&mut ui, &mut app).await;
}
