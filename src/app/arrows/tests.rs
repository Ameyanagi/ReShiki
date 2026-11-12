use super::*;
use crate::app::arrows;

#[test]
fn mechanism_independent_tangents_undo_redo_and_head_changes_preserve_curve() {
    use reshiki::arrows::{Head, Preset};
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_native_file(include_bytes!(
        "../../../tests/fixtures/mechanism-curvature-91/before.rsk"
    ))
    .unwrap();
    app.tab.selected = vec![101];
    let original = app.tab.doc.clone();
    app.edit(Edit::ArrowHandle(101, 3, Point::new(42., -55.)));
    let departure = app.tab.doc.clone();
    assert_eq!(
        (departure.arrows[0].start, departure.arrows[0].end),
        (original.arrows[0].start, original.arrows[0].end)
    );
    app.edit(Edit::ArrowHandle(101, 4, Point::new(139., -55.)));
    let arrival = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, departure);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, arrival);
    app.arrow_action(arrows::Action::Head(Head::Right));
    assert_eq!(app.tab.doc.arrows[0].cubic, arrival.arrows[0].cubic);
    let _ = app.update(Message::ArrowStyle(Preset::Fishhook));
    assert_eq!(app.tab.doc.arrows[0].cubic, arrival.arrows[0].cubic);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(app.tab.doc.arrows[1], original.arrows[1]);
    let bytes = app.tab.doc.file_json().unwrap();
    assert_eq!(
        Document::from_native_file(&bytes).unwrap(),
        app.tab.doc.current()
    );
}
#[test]
fn edit_points_adds_shapes_retypes_and_deletes_arrow_nodes_in_undoable_steps() {
    use reshiki::arrows::{ArrowStyle, FIRST_NODE_HANDLE, Preset};
    use reshiki::graphics::PathNodeType;
    let (mut app, _) = App::new();
    let mut arrow = reshiki::document::Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(120., 0.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.cubic = arrow.bezier_controls();
    arrow.control = None;
    app.tab.doc.arrows.push(arrow);
    app.tab.selected = vec![1];
    let _ = app.update(Message::Tool(Tool::EditPoints));
    let original = app.tab.doc.clone();
    let on_shaft = original.arrows[0].point(0.5);
    app.edit(Edit::ArrowNodeInsert(1, on_shaft));
    let node = FIRST_NODE_HANDLE;
    assert_eq!(app.tab.doc.arrows[0].nodes.len(), 1);
    assert_eq!(app.tab.path_point, Some((1, node)));
    assert_eq!(
        app.selected_arrow_node().map(|n| n.2),
        Some(PathNodeType::Smooth)
    );
    let inserted = app.tab.doc.clone();

    app.edit(Edit::ArrowHandle(1, node, Point::new(60., 40.)));
    assert_eq!(app.tab.doc.arrows[0].nodes[0].point, Point::new(60., 40.));
    let moved = app.tab.doc.clone();

    // A double-click on the node cycles Smooth to Symmetric.
    let now = iced::time::Instant::now();
    app.edit(Edit::ArrowNodeClick(1, node));
    app.arrow_node_click(1, node, now + std::time::Duration::from_millis(100));
    assert_eq!(app.tab.doc.arrows[0].nodes[0].kind, PathNodeType::Symmetric);

    // Option/Alt-dragging a direction control makes the node a Corner.
    app.edit(Edit::ArrowControlFree(1, node + 2, Point::new(90., 60.)));
    let freed = app.tab.doc.arrows[0].nodes[0];
    assert_eq!(freed.kind, PathNodeType::Corner);
    assert_eq!(freed.controls[1], Point::new(90., 60.));

    // Save and reopen keep the node; the file needs the multi-node version.
    let bytes = app.tab.doc.file_json().unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("\"nodes\""));
    assert_eq!(
        Document::from_native_file(&bytes).unwrap().arrows[0].nodes,
        app.tab.doc.arrows[0].nodes
    );

    app.arrow_action(arrows::Action::DeleteNode);
    assert!(app.tab.doc.arrows[0].nodes.is_empty());
    assert_eq!(app.tab.path_point, None);
    assert!(!String::from_utf8_lossy(&app.tab.doc.file_json().unwrap()).contains("nodes"));

    // Delete, free control and type change each took one Undo step.
    for _ in 0..3 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, moved);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, inserted);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn bent_arrows_refuse_nodes_with_a_status() {
    use reshiki::arrows::{ArrowStyle, Preset};
    let (mut app, _) = App::new();
    app.tab.doc.arrows.push(reshiki::document::Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(120., 0.),
        Preset::Bent,
        ArrowStyle::default(),
    ));
    app.tab.selected = vec![1];
    let before = app.tab.doc.clone();
    app.edit(Edit::ArrowNodeInsert(1, Point::new(30., 30.)));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert!(app.selected_node_arrow().is_none());
}
