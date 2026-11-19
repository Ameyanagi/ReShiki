use super::*;
use reshiki::document::{Arrow, Document, Point};

#[test]
fn arrow_semantics_refusal_does_not_edit_the_drawing() {
    use reshiki::arrows::{ArrowStyle, Preset};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atom = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc.arrows.push(Arrow::new(
        app.tab.doc.next_id(),
        Point::new(80., 0.),
        Point::new(160., 0.),
        Preset::Resonance,
        ArrowStyle::preset(Preset::Resonance),
    ));
    app.tab.selected = vec![atom];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Reaction(Action::Assign(Role::Reactant)));
    assert!(app.error);
    assert!(app.status.contains("Resonance"), "{}", app.status);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn role_edits_are_undoable_and_arrow_reversal_swaps_roles() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let c = app.tab.doc.add_atom("C", Point::new(300., 0.));
    app.tab.doc.arrows.push(Arrow::new(
        4,
        Point::new(100., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    app.tab.saved = app.tab.doc.clone();
    let original = app.tab.doc.clone();
    app.tab.selected = vec![a];
    let _ = app.update(Message::Reaction(Action::Assign(Role::Reactant)));
    assert_eq!(app.tab.doc.reactions[0].reactants[0].atoms, [a, b]);
    assert!(app.dirty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    app.tab.selected = vec![c];
    let _ = app.update(Message::Reaction(Action::Assign(Role::Product)));
    let ready = app.tab.doc.clone();
    app.tab.selected = vec![4];
    app.sync_arrows();
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    let _ = app.update(Message::ArrowAction(super::super::arrows::Action::Reverse));
    assert_eq!(
        app.tab.doc.reactions[0].reactants,
        ready.reactions[0].products
    );
    assert_eq!(
        app.tab.doc.reactions[0].products,
        ready.reactions[0].reactants
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, ready);
    // An incompatible chemistry edit is rejected as a whole.
    app.tab.doc.add_bond(a, c, 1, "plain");
    app.changed(ready.clone());
    assert!(app.error);
    assert_eq!(app.tab.doc, ready);
    let _ = app.update(Message::Reaction(Action::SelectAll));
    assert_eq!(app.tab.selected.len(), 4);
    let _ = app.update(Message::Reaction(Action::Unlink));
    assert!(app.tab.doc.reactions.is_empty());
    assert_eq!(app.tab.doc.atoms, ready.atoms);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, ready);
}

#[test]
fn an_arrow_is_named_by_its_panel_column_title() {
    use reshiki::graphics::{
        BracketSides, Graphic, GraphicKind,
        panel::{Divider, Panel},
    };
    let mut doc = Document::default();
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(400., 100.),
        Default::default(),
        BracketSides::Both,
        false,
    );
    g.panel = Some(Panel {
        titles: vec!["Previous work".into(), "This work".into()],
        dividers: vec![Divider {
            vertical: true,
            at: 0.5,
        }],
        ..Default::default()
    });
    doc.graphics.push(g);
    let arrow = |id, x: f32| {
        Arrow::new(
            id,
            Point::new(x, 50.),
            Point::new(x + 60., 50.),
            Default::default(),
            Default::default(),
        )
    };
    assert_eq!(panel_title(&doc, &arrow(2, 50.)), Some("Previous work"));
    assert_eq!(panel_title(&doc, &arrow(3, 250.)), Some("This work"));
    // Outside every panel, the arrow keeps its kind for a name.
    assert_eq!(panel_title(&doc, &arrow(4, 500.)), None);
    // Turned a quarter turn, the columns sit one above the other.
    let g = &mut doc.graphics[0];
    (g.origin, g.axis_x, g.axis_y) = (
        Point::new(100., 0.),
        Point::new(0., 400.),
        Point::new(-100., 0.),
    );
    let down = |id, y: f32| {
        Arrow::new(
            id,
            Point::new(50., y),
            Point::new(50., y + 60.),
            Default::default(),
            Default::default(),
        )
    };
    assert_eq!(panel_title(&doc, &down(5, 50.)), Some("Previous work"));
    assert_eq!(panel_title(&doc, &down(6, 250.)), Some("This work"));
}
