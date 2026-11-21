use super::*;
use reshiki::equipment::{
    authoring::ShapeStyle,
    plan::{Ink, Pass, Weight},
};

#[test]
fn material_fixture_authors_every_layer_and_round_trips_the_preview() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = serde_json::from_str(include_str!(
        "../../../tests/fixtures/equipment-authoring/materials-and-layers.rsk"
    ))
    .unwrap();
    app.tab.doc.validate().unwrap();
    app.tab.selected = app.tab.doc.click_selection(&[], &[1]);
    assert_eq!(app.tab.selected, [1, 2, 3, 4, 5, 6]);
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    assert_eq!(
        app.tab
            .equipment_authoring
            .as_ref()
            .expect("a grouped-shape selection starts an equipment draft")
            .current
            .draft
            .definition
            .shapes
            .len(),
        before.graphics.len()
    );
    send(&mut app, Action::Point(Point::new(-24., -100.)));
    send(&mut app, Action::Point(Point::new(24., -100.)));
    send(&mut app, Action::Length("16".into()));
    send(&mut app, Action::Calibrate);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
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
    let solid = Ink::FillStroke(Weight::Outline);
    assert_eq!(
        plan.iter()
            .map(|p| (p.pass, p.material, p.ink))
            .collect::<Vec<_>>(),
        [
            (Pass::Back, Material::Metal, solid),
            (Pass::Back, Material::Metal, solid),
            (Pass::Outline, Material::Glass, Ink::Stroke(Weight::Outline)),
            (Pass::Glass, Material::Glass, Ink::Fill),
            (Pass::Inner, Material::Metal, solid),
            (Pass::Body, Material::Ink, Ink::Stroke(Weight::Outline)),
            (Pass::Front, Material::Rubber, solid),
        ]
    );
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
fn bowtie_layer_edits_keep_filled_roles_and_round_trip_with_one_document_undo() {
    use reshiki::{graphics::PathCommand, palette::Color};

    let mut app = drawing();
    let body = &mut app.tab.doc.graphics[0];
    body.kind = GraphicKind::Path;
    body.origin = Point::default();
    body.axis_x = Point::new(1., 0.);
    body.axis_y = Point::new(0., 1.);
    body.style.fill = Some(Color::Paper);
    body.path = vec![
        PathCommand::Move(Point::new(-20., 0.)),
        PathCommand::Line(Point::new(20., 50.)),
        PathCommand::Line(Point::new(-20., 50.)),
        PathCommand::Line(Point::new(20., 0.)),
        PathCommand::Close,
    ];
    app.tab.doc.graphics[1].kind = GraphicKind::Line;
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.shape_fill.as_ref().unwrap().1);
    assert_eq!(state.current.draft.definition.shapes[0].role, Role::Glass);
    send(&mut app, Action::SelectShape(shapes::Choice(1)));
    assert!(
        !app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .shape_fill
            .as_ref()
            .unwrap()
            .1,
        "selecting an open outline refreshes filled-role eligibility"
    );
    send(&mut app, Action::SelectShape(shapes::Choice(0)));
    calibrate(&mut app);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.shape_fill.as_ref().unwrap().1);
    let path = state.current.draft.definition.shapes[0].path.clone();
    let history = state.past.len();
    send(&mut app, Action::ShapeLayer(Layer::Front));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.notice, None);
    assert_eq!(state.past.len(), history + 1);
    let shape = &state.current.draft.definition.shapes[0];
    assert_eq!(shape.layer, Layer::Front);
    assert_eq!(shape.role, Role::Glass);
    assert_eq!(shape.path, path);
    let shown = app.display_document().clone();
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Undo);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.current.draft.definition.shapes[0].layer, Layer::Body);
    assert!(state.shape_fill.as_ref().unwrap().1);
    let _ = app.update(Message::Redo);
    assert_eq!(app.display_document(), &shown);
    send(&mut app, Action::Accept);
    assert_eq!(app.tab.doc, shown);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let saved = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(saved.piece_definitions, shown.piece_definitions);
    assert_eq!(
        reshiki::equipment::placed::plan(&saved.graphics[0]),
        reshiki::equipment::placed::plan(&shown.graphics[0])
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
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
