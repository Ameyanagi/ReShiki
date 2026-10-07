use super::*;
#[test]
fn remembered_tools_and_shape_presets_do_not_change_selected_objects() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::Palette(Action::RingPreset(RingPreset::ChairDown)));
    let _ = app.update(Message::Tool(Tool::Bond(2)));
    assert_eq!(app.toolbar.ring, Tool::RingPreset(RingPreset::ChairDown));
    let _ = app.update(Message::Tool(app.toolbar.ring));
    assert!(app.palette.is_none());
    assert_eq!(app.tool, Tool::RingPreset(RingPreset::ChairDown));
    let circle = graphic_options(Family::Ellipses)
        .into_iter()
        .find(|(label, _)| label == "Circle")
        .unwrap()
        .1;
    let _ = app.update(Message::Palette(Action::Graphic(circle)));
    app.edit(crate::canvas::Edit::Graphic(
        Point::default(),
        Point::new(100., 30.),
        true,
    ));
    let drawing = app.tab.doc.clone();
    let g = &app.tab.doc.graphics[0];
    assert!(
        (g.axis_x.distance(Point::default()) - g.axis_y.distance(Point::default())).abs() < 0.001
    );
    let filled = graphic_options(Family::Rectangles)
        .into_iter()
        .find(|(_, option)| option.style.fill.is_some())
        .unwrap()
        .1;
    let _ = app.update(Message::Palette(Action::Graphic(filled)));
    assert_eq!(app.tab.doc, drawing);
    assert!(app.tab.graphic_style.fill.is_some());
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Ellipse)));
    assert!(app.toolbar.ellipse.constrain);
    assert!(app.tab.graphic_style.fill.is_none());
    for family in [
        Family::Rectangles,
        Family::Ellipses,
        Family::Brackets,
        Family::Arrows,
        Family::Symbols,
        Family::Orbitals,
    ] {
        app.palette = Some(family);
        let _ = app.view();
    }
}
#[test]
fn an_eraser_drag_is_one_undo_step_and_empty_strokes_leave_history_unchanged() {
    use crate::canvas::Edit;
    let (mut app, _) = App::new();
    app.tool = Tool::Erase;
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    app.tab.doc.add_atom("O", Point::new(0., 50.));
    let keep = app.tab.doc.add_atom("N", Point::new(50., 50.));
    let original = app.tab.doc.clone();
    app.edit(Edit::EraseStart(Point::new(0., -20.)));
    app.edit(Edit::EraseTo(Point::new(0., -20.), Point::new(0., 20.)));
    assert!(app.tab.doc.atom(a).is_none());
    app.edit(Edit::EraseTo(Point::new(0., 20.), Point::new(0., 80.)));
    app.edit(Edit::EraseEnd);
    let erased = app.tab.doc.clone();
    assert_eq!(erased.atoms.len(), 1);
    assert!(erased.atom(keep).is_some());
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.history.redo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, erased);
    app.edit(Edit::EraseStart(Point::new(1000., 1000.)));
    app.edit(Edit::EraseEnd);
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, original);
}
#[test]
fn toolbar_flyouts_choose_tools_without_mutating_the_drawing() {
    let (mut app, _) = App::new();
    app.tab.doc.arrows.push(Arrow::new(
        1,
        Point::default(),
        Point::new(80., 0.),
        ArrowPreset::Forward,
        ArrowStyle::default(),
    ));
    app.tab.selected = vec![1];
    let original = app.tab.doc.clone();
    for tool in [Tool::Atom, Tool::Wedge, Tool::Ring, Tool::Arrow] {
        let _ = app.update(Message::Palette(Action::Open(tool)));
        assert!(app.palette.is_some());
        let _ = app.view();
        let _ = app.update(Message::Tool(Tool::Select));
        assert!(app.palette.is_none());
        assert_eq!(app.tab.doc, original);
    }
    let _ = app.update(Message::Palette(Action::Atom("Br".into())));
    assert_eq!(app.element, "Br");
    let _ = app.update(Message::Palette(Action::Bond(BondPreset::HollowWedge)));
    assert_eq!(app.tool.bond_preset(), Some(BondPreset::HollowWedge));
    let _ = app.update(Message::Palette(Action::Ring(7, false)));
    assert_eq!(app.ring_size, 7);
    let _ = app.update(Message::Palette(Action::Arrow(ArrowPreset::Bent)));
    assert_eq!(app.tab.arrow_style, ArrowPreset::Bent);
    assert_eq!(app.tab.doc, original);
}
#[test]
fn undo_during_an_eraser_drag_does_not_merge_later_motion_into_older_edits() {
    use crate::canvas::Edit;
    let (mut app, _) = App::new();
    let original = app.tab.doc.clone();
    app.tab.doc.add_atom("C", Point::default());
    app.changed(original.clone());
    app.tool = Tool::Erase;
    app.edit(Edit::EraseStart(Point::default()));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atoms.len(), 1);
    app.edit(Edit::EraseTo(Point::default(), Point::new(10., 0.)));
    app.edit(Edit::EraseEnd);
    assert_eq!(app.tab.doc.atoms.len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}
#[test]
fn crossing_depth_changes_are_undoable_and_preserve_chemistry() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a, b];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::BondDepth(true));
    assert_eq!(app.tab.doc.bonds[0].z_order, 1);
    assert!(!super::super::chemistry_changed(&before, &app.tab.doc));
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, before);
}
