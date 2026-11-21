use super::*;
use reshiki::{cycles::Shape, graphics::GraphicKind};

fn field(app: &mut App, field: Field, value: &str) {
    let _ = app.update(Message::Cycle(Action::Edit(field, value.into())));
    let _ = app.update(Message::Cycle(Action::Apply(field)));
}

fn frame(doc: &Document) -> &reshiki::graphics::Graphic {
    doc.graphics.iter().find(|g| g.cycle.is_some()).unwrap()
}

fn dimensions(doc: &Document) -> [f32; 2] {
    let g = frame(doc);
    [
        g.axis_x.distance(Point::default()),
        g.axis_y.distance(Point::default()),
    ]
}

fn approximate(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.003, "{actual} != {expected}");
}

fn suzuki_intermediates() -> Document {
    let source = Document::from_json(include_bytes!(
        "../../../tests/fixtures/cycles/tidy-before.rsk"
    ))
    .unwrap();
    let selected: Vec<_> = source
        .graphics
        .iter()
        .find(|g| g.id == 191)
        .unwrap()
        .cycle
        .as_ref()
        .unwrap()
        .intermediates
        .iter()
        .flatten()
        .copied()
        .collect();
    let doc = reshiki::editing::selection(&source, &selected);
    assert!(doc.graphics.iter().all(|g| g.cycle.is_none()));
    doc.validate().unwrap();
    doc
}

fn evidence(shape: Shape, before: &Document, preview: &Document) {
    if let Some(dir) = std::env::var_os("RESHIKI_CYCLE_SHAPE_CONTROL_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (state, doc) in [("before", before), ("preview", preview)] {
            let name = format!("make-{shape:?}-{state}");
            std::fs::write(dir.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
            std::fs::write(dir.join(format!("{name}.svg")), reshiki::scene::svg(doc)).unwrap();
        }
    }
}

#[test]
fn all_four_make_shapes_keep_size_and_corner_through_review_reverse_apply_and_undo() {
    for shape in [
        Shape::Circle,
        Shape::Ellipse,
        Shape::RoundedRectangle,
        Shape::Rectangle,
    ] {
        let (mut app, _) = App::new();
        app.tab.doc = suzuki_intermediates();
        app.tab.selected = app.tab.doc.all_ids();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Cycle(Action::Make));
        let _ = app.update(Message::Cycle(Action::Shape(shape)));
        if shape == Shape::Circle {
            field(&mut app, Field::Radius, "180");
        } else {
            field(&mut app, Field::Width, "400");
            field(&mut app, Field::Height, "250");
            if shape == Shape::RoundedRectangle {
                field(&mut app, Field::Corner, "12");
            }
        }
        assert!(!app.error, "{shape:?}: {}", app.status);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Cycle(Action::MakeReverse));
        let shown = app.display_document().clone();
        let g = frame(&shown);
        assert_eq!(
            g.kind,
            GraphicKind::Ellipse,
            "the cycle host stays independent of its outline"
        );
        assert_eq!(g.cycle.as_ref().unwrap().shape, shape);
        assert!(g.cycle.as_ref().unwrap().counterclockwise);
        let expected = if shape == Shape::Circle {
            [360., 360.]
        } else {
            [400., 250.]
        };
        for (actual, pt) in dimensions(&shown).into_iter().zip(expected) {
            approximate(actual, reshiki::style::DEFAULT.world(pt));
        }
        assert_eq!(
            g.style.corner_pt,
            (shape == Shape::RoundedRectangle).then_some(12.)
        );
        assert!(app.cycle_proposal_card().is_some());
        evidence(shape, &before, &shown);
        let _ = app.update(Message::ContextKey("Enter".into()));
        assert!(!app.error, "{shape:?}: {}", app.status);
        assert_eq!(
            app.tab.doc, shown,
            "Apply must use exactly the reviewed shape and size"
        );
        assert_eq!(app.tab.history.undo_frames().len(), 1);
        let saved = app.tab.doc.file_json().unwrap();
        assert_eq!(
            Document::from_json(&saved).unwrap().file_json().unwrap(),
            saved
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, shown);
    }
}

#[test]
fn make_shape_configuration_discards_without_source_edits_and_rejects_invalid_fields() {
    let (mut app, _) = App::new();
    app.tab.selected = vec![
        text(&mut app, -120., 0., "A"),
        text(&mut app, 120., 0., "B"),
    ];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Make));
    let _ = app.update(Message::Cycle(Action::Shape(Shape::Ellipse)));
    field(&mut app, Field::Width, "280");
    let shown = app.display_document().clone();
    for value in ["NaN", "0", "-1", "4001"] {
        field(&mut app, Field::Height, value);
        assert!(app.error);
        assert_eq!(app.display_document(), &shown);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn failed_make_reverse_retains_the_reviewed_direction_and_configuration() {
    let (mut app, _) = App::new();
    let a = text(&mut app, -120., 0., "A");
    let b = text(&mut app, 120., 0., "B");
    app.tab.selected = vec![a, b];
    let _ = app.update(Message::Cycle(Action::Make));
    let _ = app.update(Message::Cycle(Action::Shape(Shape::Ellipse)));
    field(&mut app, Field::Width, "240");
    let shown = app.display_document().clone();
    // A changed source may no longer contain enough parts to remake a cycle.
    app.tab.doc.annotations.retain(|a| a.id != b);
    let _ = app.update(Message::Cycle(Action::MakeReverse));
    assert!(app.error);
    assert_eq!(app.display_document(), &shown);
    let proposal = app.tab.cycle_inputs.proposal.as_ref().unwrap();
    let super::super::ProposalKind::Make { reversed, .. } = proposal.kind else {
        panic!("Make proposal");
    };
    assert!(!reversed);
}

#[test]
fn loaded_explicit_shapes_keep_their_frame_and_parts_and_each_change_is_one_undo() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    make(&mut app);
    let id = cycle(&app).0;
    let g = app
        .tab
        .doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap();
    g.cycle.as_mut().unwrap().shape = Shape::Ellipse;
    g.cycle.as_mut().unwrap().label = "Suzuki–Miyaura".into();
    g.cycle.as_mut().unwrap().steps[0].name = "Oxidative addition".into();
    g.axis_x = Point::new(-620., -80.);
    g.axis_y = Point::new(50., 260.);
    g.style.corner_pt = Some(12.);
    let reopened = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    app.tab.doc = reopened;
    app.tab.selected = vec![id];
    app.sync_cycle();
    assert_eq!(cycle(&app).1.shape, Shape::Ellipse);
    let loaded = app.tab.doc.clone();
    let loaded_frames = app.tab.history.undo_frames().len();
    let width = app.tab.cycle_inputs.width.clone();
    let height = app.tab.cycle_inputs.height.clone();
    field(&mut app, Field::Width, &width);
    field(&mut app, Field::Height, &height);
    field(&mut app, Field::Radius, "120");
    assert_eq!(
        app.tab.doc, loaded,
        "unchanged displayed dimensions and inactive Radius are inert"
    );
    assert_eq!(app.tab.history.undo_frames().len(), loaded_frames);
    for shape in [
        Shape::RoundedRectangle,
        Shape::Rectangle,
        Shape::Circle,
        Shape::Ellipse,
    ] {
        let before = app.tab.doc.clone();
        let count = app.tab.history.undo_frames().len();
        let _ = app.update(Message::Cycle(Action::Shape(shape)));
        assert!(!app.error, "{}", app.status);
        let mut expected = before.clone();
        expected
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap()
            .cycle
            .as_mut()
            .unwrap()
            .shape = shape;
        assert_eq!(
            app.tab.doc, expected,
            "shape changes do not silently Tidy or normalize the frame"
        );
        assert_eq!(app.tab.history.undo_frames().len(), count + 1);
        let _ = app.update(Message::Cycle(Action::Shape(shape)));
        assert_eq!(app.tab.history.undo_frames().len(), count + 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, expected);
    }
}

#[test]
fn make_preview_matches_apply_for_visible_stereo_and_linked_caption_precision() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    let lactic: Vec<_> = app.tab.doc.atoms.iter().map(|a| a.id).collect();
    reshiki::linked_properties::toggle(
        &mut app.tab.doc,
        &lactic,
        reshiki::linked_properties::Property::Weight,
        Default::default(),
        !app.appearance.two_decimal_weight,
    )
    .unwrap();
    let a = app.tab.doc.add_atom("C", Point::new(210., 80.));
    let b = app.tab.doc.add_atom("C", Point::new(252., 80.));
    let left = app.tab.doc.add_atom("C", Point::new(189., 44.));
    let right = app.tab.doc.add_atom("C", Point::new(273., 116.));
    app.tab.doc.add_bond(a, b, 2, "plain");
    app.tab.doc.add_bond(a, left, 1, "plain");
    app.tab.doc.add_bond(b, right, 1, "plain");
    let catalyst = text(&mut app, -240., 120., "Catalyst");
    app.tab.doc.atom_labels.stereo = true;
    let labels =
        reshiki::atom_labels::refresh::Refresh::calculate(&app.tab.doc, &Default::default())
            .unwrap();
    labels.apply(&mut app.tab.doc);
    let indicators = reshiki::atom_labels::indicators(&app.tab.doc);
    assert!(indicators.iter().any(|label| label.text == "(R)"));
    assert!(indicators.iter().any(|label| label.text == "(E)"));
    app.tab.selected = lactic;
    app.tab.selected.extend([a, b, left, right, catalyst]);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Make));
    let _ = app.update(Message::Cycle(Action::Shape(Shape::Ellipse)));
    field(&mut app, Field::Width, "300");
    field(&mut app, Field::Height, "160");
    assert!(!app.error, "{}", app.status);
    let preview = app.display_document().clone();
    assert_eq!(app.tab.doc, before);
    assert!(preview.atoms.iter().all(|a| a.cip_label.is_none()));
    assert!(preview.bonds.iter().all(|b| b.cip_label.is_none()));
    assert!(
        preview
            .property_captions
            .iter()
            .all(|c| c.two_decimal_weight == app.appearance.two_decimal_weight)
    );
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    // Undo restores the molecular drawing. Computed stereo labels and the
    // current Mol. Wt. display preference follow the ordinary Undo policy.
    let mut source_chemistry = before.clone();
    reshiki::atom_labels::clear_computed(&mut source_chemistry);
    assert_eq!(app.tab.doc.atoms, source_chemistry.atoms);
    assert_eq!(app.tab.doc.bonds, source_chemistry.bonds);
    assert_eq!(app.tab.doc.graphics, before.graphics);
    assert_eq!(app.tab.doc.groups, before.groups);
    assert_eq!(
        app.tab.doc.property_captions[0].atoms,
        before.property_captions[0].atoms
    );
    assert!(!app.tab.history.can_undo());
}

#[test]
fn dimensions_and_rounded_corner_controls_commit_once_and_keep_explicit_values() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::Cycle(Action::Insert));
    let id = cycle(&app).0;
    let _ = app.update(Message::Cycle(Action::Shape(Shape::RoundedRectangle)));
    let before = app.tab.doc.clone();
    let frames = app.tab.history.undo_frames().len();
    field(&mut app, Field::Width, "240");
    field(&mut app, Field::Height, "100");
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.history.undo_frames().len(), frames + 2);
    approximate(
        dimensions(&app.tab.doc)[0],
        reshiki::style::DEFAULT.world(240.),
    );
    approximate(
        dimensions(&app.tab.doc)[1],
        reshiki::style::DEFAULT.world(100.),
    );
    let sized = app.tab.doc.clone();
    field(&mut app, Field::Height, "100");
    assert_eq!(app.tab.doc, sized);
    assert_eq!(app.tab.history.undo_frames().len(), frames + 2);
    for (value, expected) in [
        ("0", Some(0.)),
        ("12", Some(12.)),
        ("1000", Some(1000.)),
        ("", None),
    ] {
        let count = app.tab.history.undo_frames().len();
        field(&mut app, Field::Corner, value);
        assert!(!app.error, "{}", app.status);
        assert_eq!(frame(&app.tab.doc).style.corner_pt, expected);
        assert_eq!(app.tab.history.undo_frames().len(), count + 1);
        let g = frame(&app.tab.doc);
        assert!(g.corner_radius() <= 0.5 * dimensions(&app.tab.doc)[1]);
        let back = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
        assert_eq!(frame(&back).style.corner_pt, expected);
    }
    for value in ["NaN", "-1", "1001"] {
        let source = app.tab.doc.clone();
        let count = app.tab.history.undo_frames().len();
        field(&mut app, Field::Corner, value);
        assert!(app.error);
        assert_eq!(app.tab.doc, source);
        assert_eq!(app.tab.history.undo_frames().len(), count);
    }
    let count = app.tab.history.undo_frames().len();
    field(&mut app, Field::Corner, "");
    assert!(
        !app.error,
        "a corrected Auto value clears the previous refusal"
    );
    assert!(app.status.is_empty());
    assert_eq!(app.tab.history.undo_frames().len(), count);
    assert_eq!(frame(&before).id, id);
}

#[test]
fn a_legacy_singular_circle_cannot_silently_become_a_noncircle() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::Cycle(Action::Insert));
    app.tab.doc.graphics[0].axis_y = app.tab.doc.graphics[0].axis_x;
    app.tab.doc.validate().unwrap();
    let before = app.tab.doc.clone();
    let count = app.tab.history.undo_frames().len();
    let _ = app.update(Message::Cycle(Action::Shape(Shape::Ellipse)));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), count);
}
