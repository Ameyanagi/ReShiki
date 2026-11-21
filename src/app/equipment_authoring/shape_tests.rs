use super::*;
use reshiki::equipment::authoring::ShapeStyle;

#[test]
fn material_fixture_authors_every_layer_and_round_trips_the_preview() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = serde_json::from_str(include_str!(
        "../../../tests/fixtures/equipment-authoring/materials-and-layers.rsk"
    ))
    .unwrap();
    app.tab.doc.validate().unwrap();
    app.tab.selected = vec![7];
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    send(&mut app, Action::Point(Point::new(-24., -100.)));
    send(&mut app, Action::Point(Point::new(24., -100.)));
    send(&mut app, Action::Length("16".into()));
    send(&mut app, Action::Calibrate);
    for (index, layer, material) in [
        (2, Layer::Back, Material::Metal),
        (3, Layer::Back, Material::Metal),
        (4, Layer::Inner, Material::Metal),
        (5, Layer::Front, Material::Rubber),
    ] {
        send(&mut app, Action::SelectShape(shapes::Choice(index)));
        send(&mut app, Action::ShapeLayer(layer));
        send(&mut app, Action::ShapeMaterial(material));
    }
    let shown = app.display_document().clone();
    let plan = reshiki::equipment::placed::plan(&shown.graphics[0]);
    for pass in [
        reshiki::equipment::plan::Pass::Back,
        reshiki::equipment::plan::Pass::Glass,
        reshiki::equipment::plan::Pass::Inner,
        reshiki::equipment::plan::Pass::Front,
    ] {
        assert!(plan.iter().any(|p| p.pass == pass));
    }
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Accept);
    assert_eq!(app.tab.doc, shown);
    let saved = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(saved.piece_definitions, shown.piece_definitions);
    assert_eq!(reshiki::equipment::placed::plan(&saved.graphics[0]), plan);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn shape_controls_and_canvas_pick_edit_only_the_selected_draft_shape() {
    let mut app = drawing();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    let history = app.tab.equipment_authoring.as_ref().unwrap().past.len();
    send(&mut app, Action::SelectShape(shapes::Choice(1)));
    let _ = app.update(Message::Canvas(Edit::EquipmentAuthoringShape {
        at: Point::new(-20., 25.),
        reach: 1.,
    }));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.selected_shape, 0);
    assert_eq!(state.past.len(), history, "selection is not an edit");
    assert!(!state.measurement().shape.is_empty());
    send(&mut app, Action::ShapeLayer(Layer::Front));
    send(&mut app, Action::ShapeMaterial(Material::Metal));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.current.draft.definition.shapes[0].role, Role::Outline);
    assert_eq!(
        state.current.draft.definition.shapes[0].material,
        Material::Metal
    );
    assert_eq!(
        state.current.draft.definition.shapes[1].material,
        Material::Ink
    );
    let edited = app.display_document().clone();
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .draft
            .definition
            .shapes[0]
            .material,
        Material::Ink
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.display_document(), &edited);
    send(&mut app, Action::Accept);
    assert_eq!(app.tab.doc, edited);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let saved = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(
        ShapeStyle::from(&saved.piece_definitions[0].shapes[0]),
        ShapeStyle {
            layer: Layer::Front,
            material: Material::Metal,
            role: Role::Outline,
        }
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, edited);
}

#[test]
fn material_switch_uses_solid_paint_and_failed_edits_preserve_history_and_preview() {
    let (mut app, _) = joint_drawing();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    send(&mut app, Action::ShapeMaterial(Material::Rubber));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.current.draft.definition.shapes[0].role, Role::Solid);
    let edited = state.current.draft.clone();
    let shown = app.display_document().clone();
    let count = state.past.len();
    send(&mut app, Action::ShapeRole(Role::Glass));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.notice.as_ref().unwrap().contains("Glass material"));
    assert_eq!(state.current.draft, edited);
    assert_eq!(state.past.len(), count);
    assert_eq!(app.display_document(), &shown);
    send(&mut app, Action::Discard);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());

    let mut app = drawing();
    app.tab.doc.graphics[0].kind = GraphicKind::Line;
    send(&mut app, Action::Begin);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    let original = state.current.draft.clone();
    send(&mut app, Action::ShapeRole(Role::Solid));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.notice.as_ref().unwrap().contains("closed outline"));
    assert_eq!(state.current.draft, original);
    assert!(state.past.is_empty());
}

#[test]
fn shape_clicks_do_not_take_over_measurement_and_pending_joint_controls() {
    let (mut app, edges) = joint_drawing();
    send(&mut app, Action::Begin);
    send(&mut app, Action::SelectShape(shapes::Choice(99)));
    send(
        &mut app,
        Action::ShapeAt {
            at: edges[0],
            reach: 1.,
        },
    );
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .points
            .is_empty()
    );
    mark_joint(&mut app, edges);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    let draft = state.current.draft.clone();
    let candidate = state.joint_candidate.clone();
    let count = state.past.len();
    send(&mut app, Action::ShapeMaterial(Material::Metal));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.current.draft, draft);
    assert_eq!(state.joint_candidate, candidate);
    assert_eq!(state.past.len(), count);
    assert!(state.measurement().shape.is_empty());
}
