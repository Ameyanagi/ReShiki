use super::*;

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.tool = Tool::Graphic(GraphicKind::Arc);
    app.update_arc(Action::Preset(270.));
    app.edit(Edit::Graphic(
        Point::new(20., 30.),
        Point::new(180., 110.),
        false,
    ));
    app
}

#[test]
fn arc_presets_apply_to_new_drawings_and_numeric_edits_undo_as_one_action() {
    let mut app = drawing();
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.doc.graphics[0].arc.unwrap().sweep_degrees, 270.);
    let before = app.tab.doc.clone();
    app.update_arc(Action::Start("-45.5".into()));
    app.update_arc(Action::Sweep("123.4".into()));
    assert_eq!(app.tab.doc, before, "typing must not change the drawing");
    app.update_arc(Action::Apply);
    let after = app.tab.doc.clone();
    assert_eq!(after.graphics[0].arc.unwrap().start_degrees, 314.5);
    assert_eq!(after.graphics[0].arc.unwrap().sweep_degrees, 123.4);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.arc_editor.geometry.sweep_degrees, 270.);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn full_circle_joins_the_preset_strip_and_keeps_the_start() {
    assert_eq!(ArcGeometry::PRESETS.last(), Some(&360.));
    let mut app = drawing();
    app.update_arc(Action::Start("32".into()));
    app.update_arc(Action::Apply);
    app.update_arc(Action::Preset(360.));
    let arc = app.tab.doc.graphics[0].arc.unwrap();
    assert_eq!((arc.start_degrees, arc.sweep_degrees), (32., 360.));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.graphics[0].arc.unwrap().sweep_degrees, 270.);
}

#[test]
fn invalid_arc_input_is_rejected_atomically() {
    let mut app = drawing();
    let before = app.tab.doc.clone();
    let parameters = app.tab.arc_editor.geometry;
    for (start, sweep) in [
        ("NaN", "90"),
        ("0", "NaN"),
        ("0", "361"),
        ("0", "0"),
        ("oops", "90"),
    ] {
        app.update_arc(Action::Start(start.into()));
        app.update_arc(Action::Sweep(sweep.into()));
        app.update_arc(Action::Apply);
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.arc_editor.geometry, parameters);
    }
}

#[test]
fn applying_a_preset_to_multiple_arcs_preserves_each_start_angle() {
    let mut app = drawing();
    let mut second = app.tab.doc.graphics[0].clone();
    second.id = app.tab.doc.next_id();
    second.set_arc(ArcGeometry {
        start_degrees: 37.,
        sweep_degrees: 120.,
    });
    app.tab.doc.graphics.push(second);
    app.tab.selected = app.tab.doc.all_ids();
    app.update_arc(Action::Preset(90.));
    assert_eq!(app.tab.doc.graphics[0].arc.unwrap().start_degrees, 180.);
    assert_eq!(app.tab.doc.graphics[1].arc.unwrap().start_degrees, 37.);
    assert!(
        app.tab
            .doc
            .graphics
            .iter()
            .all(|g| g.arc.unwrap().sweep_degrees == 90.)
    );
}

#[test]
fn endpoint_drag_and_undo_keep_arc_type_selection_and_inspector_values() {
    let mut app = drawing();
    let id = app.tab.doc.graphics[0].id;
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::EditPoints));
    app.edit(Edit::GraphicPoint(id, 1, Point::new(180., 70.)));
    assert_eq!(app.tab.doc.graphics[0].kind, GraphicKind::Arc);
    assert_eq!(app.tab.arc_editor.geometry.sweep_degrees, 180.);
    assert_eq!(app.tab.selected, vec![id]);
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.arc_editor.geometry.sweep_degrees, 270.);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}
