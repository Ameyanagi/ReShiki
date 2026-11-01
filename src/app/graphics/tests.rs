use super::*;
use reshiki::graphics::GraphicStyle;
use reshiki::pictures::Picture;
use reshiki::scientific::{OrbitalKind, Phase, SymbolKind};

fn shape(id: u64, kind: GraphicKind) -> Graphic {
    Graphic::dragged(
        id,
        kind,
        Point::new(id as f32 * 100., 0.),
        Point::new(id as f32 * 100. + 80., 40.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    )
}

fn picture(id: u64) -> Graphic {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(1, 1)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    Picture::import(&bytes.into_inner())
        .unwrap()
        .graphic(id, Point::new(id as f32 * 100., 0.))
}

fn ready(graphics: Vec<Graphic>, selected: Vec<u64>) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.graphics = graphics;
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = selected;
    app.sync_graphics();
    app
}

fn assert_undo_redo(app: &mut App, before: &Document) {
    assert!(!app.error, "{}", app.status);
    let after = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    assert_ne!(&after, before);
    let _ = app.update(Message::Undo);
    assert_eq!(&app.tab.doc, before, "one undo restores the entire edit");
    assert_eq!(app.tab.selected, selected);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    assert_eq!(app.tab.selected, selected);
}

#[test]
fn invalid_graphic_inputs_preserve_drafts_document_and_redo() {
    let mut app = ready(vec![shape(1, GraphicKind::Rectangle)], vec![1]);
    let _ = app.update(Message::Graphics(Action::Width("2".into())));
    let _ = app.update(Message::Graphics(Action::ApplyWidth));
    let _ = app.update(Message::Graphics(Action::Stroke("#117E6C".into())));
    let _ = app.update(Message::Graphics(Action::ApplyStroke));
    let redo = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    let before = app.tab.doc.clone();
    let style = app.tab.graphic_style.clone();
    let revision = app.tab.revision;
    assert!(app.tab.history.can_undo());
    assert!(app.tab.history.can_redo());

    for draft in ["NaN", "inf", "0", "12.1", " 2"] {
        let _ = app.update(Message::Graphics(Action::Width(draft.into())));
        let _ = app.update(Message::Graphics(Action::ApplyWidth));
        assert!(app.error);
        assert_eq!(app.status, "Line width must be 0.1–12 pt");
        assert_eq!(app.tab.graphic_width_input, draft);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.graphic_style, style);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [1]);
        assert!(app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());
    }
    for draft in ["#abcd", "ZZZZZZ", "日本語"] {
        let _ = app.update(Message::Graphics(Action::Stroke(draft.into())));
        let _ = app.update(Message::Graphics(Action::ApplyStroke));
        assert!(app.error);
        assert_eq!(app.status, "Enter a six-digit hex color, such as #117E6C");
        assert_eq!(app.tab.graphic_stroke_input, draft);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.graphic_style, style);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [1]);
        assert!(app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());

        let _ = app.update(Message::Graphics(Action::Fill(draft.into())));
        let _ = app.update(Message::Graphics(Action::ApplyFill));
        assert!(app.error);
        assert_eq!(app.status, "Enter a six-digit hex color, such as #DCEFE9");
        assert_eq!(app.tab.graphic_fill_input, draft);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.graphic_style, style);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [1]);
        assert!(app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());
    }
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, redo, "invalid inputs retain the redo snapshot");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.graphics[0].style, GraphicStyle::default());
}

#[test]
fn mixed_selection_styles_skip_pictures_and_apply_one_undo_step() {
    let mut app = ready(
        vec![
            shape(1, GraphicKind::Rectangle),
            picture(2),
            shape(3, GraphicKind::Ellipse),
        ],
        vec![1, 2],
    );
    let untouched_picture = app.tab.doc.graphics[1].clone();
    let unselected = app.tab.doc.graphics[2].clone();
    for (draft, apply) in [
        (
            Message::Graphics(Action::Width("2.5".into())),
            Message::Graphics(Action::ApplyWidth),
        ),
        (
            Message::Graphics(Action::Stroke("#117E6C".into())),
            Message::Graphics(Action::ApplyStroke),
        ),
        (
            Message::Graphics(Action::Fill("#DCEFE9".into())),
            Message::Graphics(Action::ApplyFill),
        ),
    ] {
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        let _ = app.update(draft);
        assert_eq!(app.tab.doc, before, "typing only updates the draft");
        assert_eq!(app.tab.revision, revision);
        let _ = app.update(apply);
        assert_eq!(app.tab.revision, revision + 1);
        assert_eq!(app.tab.doc.graphics[1], untouched_picture);
        assert_eq!(app.tab.doc.graphics[2], unselected);
        assert_eq!(app.tab.selected, [1, 2]);
        assert_undo_redo(&mut app, &before);
    }
    let style = &app.tab.doc.graphics[0].style;
    assert_eq!(style.width_pt, 2.5);
    assert_eq!(style.stroke, Paint::Custom([0x11, 0x7e, 0x6c]));
    assert_eq!(style.fill, Some(Paint::Custom([0xdc, 0xef, 0xe9])));
    assert_eq!(app.tab.graphic_style, *style);
}

#[test]
fn scientific_kind_keeps_families_while_phase_and_sides_include_pictures() {
    let mut app = ready(
        vec![
            shape(1, GraphicKind::Symbol(SymbolKind::CirclePlus)),
            shape(2, GraphicKind::Orbital(OrbitalKind::P)),
            shape(3, GraphicKind::Rectangle),
            picture(4),
            shape(5, GraphicKind::Symbol(SymbolKind::CirclePlus)),
        ],
        vec![1, 2, 3, 4],
    );
    let unselected = app.tab.doc.graphics[4].clone();
    let symbol = GraphicKind::Symbol(SymbolKind::CircleMinus);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::ScientificKind(symbol)));
    assert_eq!(app.tool, Tool::Graphic(symbol));
    assert_eq!(app.toolbar.symbol, Tool::Graphic(symbol));
    assert_eq!(app.tab.doc.graphics[0].kind, symbol);
    assert_eq!(app.tab.doc.graphics[1..], before.graphics[1..]);
    assert_undo_redo(&mut app, &before);

    let orbital = GraphicKind::Orbital(OrbitalKind::Hybrid);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::ScientificKind(orbital)));
    assert_eq!(app.tool, Tool::Graphic(orbital));
    assert_eq!(app.toolbar.orbital, Tool::Graphic(orbital));
    assert_eq!(app.toolbar.symbol, Tool::Graphic(symbol));
    assert_eq!(app.tab.doc.graphics[0], before.graphics[0]);
    assert_eq!(app.tab.doc.graphics[1].kind, orbital);
    assert_eq!(app.tab.doc.graphics[2..], before.graphics[2..]);
    assert_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::OrbitalPhase(Phase::Shaded)));
    assert_eq!(app.tab.orbital_phase, Phase::Shaded);
    assert!(
        app.tab.doc.graphics[..4]
            .iter()
            .all(|g| g.phase == Phase::Shaded)
    );
    assert_eq!(app.tab.doc.graphics[4], unselected);
    assert_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::FlipPhase(true)));
    assert!(app.tab.phase_flipped);
    assert!(app.tab.doc.graphics[..4].iter().all(|g| g.phase_flipped));
    assert_eq!(app.tab.doc.graphics[4], unselected);
    assert_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::Sides(BracketSides::Right)));
    assert_eq!(app.tab.bracket_sides, BracketSides::Right);
    assert!(
        app.tab.doc.graphics[..4]
            .iter()
            .all(|g| g.sides == BracketSides::Right)
    );
    assert_eq!(app.tab.doc.graphics[4], unselected);
    assert_undo_redo(&mut app, &before);
}

#[test]
fn no_selection_changes_drawing_defaults_without_document_history() {
    let mut app = ready(vec![shape(1, GraphicKind::Rectangle), picture(2)], vec![]);
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    for width in ["0.1", "12"] {
        let _ = app.update(Message::Graphics(Action::Width(width.into())));
        let _ = app.update(Message::Graphics(Action::ApplyWidth));
        assert_eq!(
            app.tab.graphic_style.width_pt,
            width.parse::<f32>().unwrap()
        );
        assert_eq!(app.tab.graphic_width_input, width);
    }
    let _ = app.update(Message::Graphics(Action::Stroke(" ##a1B2c3 ".into())));
    let _ = app.update(Message::Graphics(Action::ApplyStroke));
    assert_eq!(
        app.tab.graphic_style.stroke,
        Paint::Custom([0xa1, 0xb2, 0xc3])
    );
    assert_eq!(app.tab.graphic_stroke_input, "#A1B2C3");
    let _ = app.update(Message::Graphics(Action::Fill("#DCEFE9".into())));
    let _ = app.update(Message::Graphics(Action::ApplyFill));
    assert_eq!(
        app.tab.graphic_style.fill,
        Some(Paint::Custom([0xdc, 0xef, 0xe9]))
    );
    assert_eq!(app.tab.graphic_fill_input, "#DCEFE9");
    let _ = app.update(Message::Graphics(Action::OrbitalPhase(Phase::Open)));
    let _ = app.update(Message::Graphics(Action::FlipPhase(true)));
    let _ = app.update(Message::Graphics(Action::Sides(BracketSides::Left)));
    let _ = app.update(Message::Graphics(Action::AttachSymbols(false)));
    let _ = app.update(Message::Graphics(Action::SnapOrbitals(false)));
    let kind = GraphicKind::Orbital(OrbitalKind::Dxy);
    let _ = app.update(Message::Graphics(Action::ScientificKind(kind)));
    assert_eq!(app.tool, Tool::Graphic(kind));
    assert_eq!(app.toolbar.orbital, Tool::Graphic(kind));
    assert_eq!(app.tab.orbital_phase, Phase::Open);
    assert!(app.tab.phase_flipped);
    assert_eq!(app.tab.bracket_sides, BracketSides::Left);
    assert!(!app.tab.attach_symbols);
    assert!(!app.tab.snap_orbitals);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(app.tab.selected.is_empty());
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
}

#[test]
fn style_message_restyles_selected_graphics_and_defaults() {
    let mut app = ready(vec![shape(1, GraphicKind::Rectangle)], vec![1]);
    let dashed = reshiki::graphics::LinePattern::Dashed;
    let _ = app.update(Message::Graphics(Action::Style(GraphicChange::Pattern(
        dashed,
    ))));
    assert_eq!(app.tab.graphic_style.pattern, dashed);
    assert_eq!(app.tab.doc.graphics[0].style.pattern, dashed);
    assert!(!app.error, "{}", app.status);
}

#[test]
fn orbital_snap_preference_and_free_node_history_preserve_the_drawing() {
    let mut app = ready(vec![], vec![]);
    let atom = app.tab.doc.add_atom("N", Point::default());
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::SnapOrbitals(false)));
    assert!(!app.tab.snap_orbitals);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let orbital = GraphicKind::Orbital(OrbitalKind::P);
    let _ = app.update(Message::Tool(Tool::Graphic(orbital)));
    let _ = app.update(Message::Tool(Tool::Select));
    let _ = app.update(Message::Tool(Tool::Graphic(orbital)));
    assert!(
        !app.tab.snap_orbitals,
        "Preference survives switching tools"
    );
    app.edit(crate::canvas::Edit::Orbital(
        Point::new(4., 0.),
        Point::new(4., -42.),
        false,
        false,
    ));
    assert_eq!(app.tab.doc.atom(atom), before.atom(atom));
    assert_eq!(app.tab.doc.graphics[0].origin, Point::new(4., 0.));
    assert!(!app.error, "{}", app.status);
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before, "One undo removes only the new orbital");
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.tab.doc, after,
        "Redo restores its node, axis, phase and layer"
    );
    let saved = app.tab.doc.file_json().unwrap();
    assert_eq!(
        Document::from_native_file(&saved).unwrap(),
        app.tab.doc.current()
    );
}

#[test]
fn click_lone_pairs_and_manual_marks_have_separate_exact_history_steps() {
    let mut app = ready(vec![], vec![]);
    let id = app.tab.doc.add_atom("O", Point::default());
    app.tab.doc.atom_mut(id).unwrap().label_h = 1;
    let pair = GraphicKind::Symbol(SymbolKind::LonePair);
    app.tool = Tool::Graphic(pair);
    let original = app.tab.doc.clone();
    app.edit(crate::canvas::Edit::Graphic(
        Point::default(),
        Point::default(),
        false,
    ));
    let auto = app.tab.doc.atom(id).unwrap().marks[0].clone();
    assert!(auto.offset.distance(Point::default()) < app.tab.doc.drawing_style.font_size());
    assert_undo_redo(&mut app, &original);
    let before_drag = app.tab.doc.clone();
    app.tool = Tool::Graphic(pair);
    // Option/Alt or Shift frees the drag from the orbit.
    app.edit(crate::canvas::Edit::Graphic(
        Point::default(),
        Point::new(8., -15.),
        true,
    ));
    assert_eq!(app.tab.doc.atom(id).unwrap().marks[0], auto);
    let dragged = &app.tab.doc.atom(id).unwrap().marks[1];
    assert_eq!(dragged.offset, Point::new(8., -15.));
    let tangent = (-15_f32).atan2(8.).to_degrees() + 90.;
    assert!((dragged.angle - tangent).abs() < 1e-3, "{}", dragged.angle);
    assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 1);
    assert_undo_redo(&mut app, &before_drag);
    assert_eq!(
        Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap(),
        app.tab.doc.current()
    );
}

fn path_heads_app() -> App {
    use reshiki::graphics::PathCommand;
    let path = |id| {
        Graphic::pen_curve(
            id,
            Point::new(0., 0.),
            Point::new(100., 40.),
            Default::default(),
        )
    };
    let mut closed = path(4);
    closed
        .append_pen_node(Point::new(100., 100.), None)
        .unwrap();
    closed.set_path_closed(true).unwrap();
    let mut compound = path(7);
    compound.path.extend([
        PathCommand::Move(Point::new(200., 0.)),
        PathCommand::Line(Point::new(300., 0.)),
    ]);
    compound.node_types.clear();
    let mut app = ready(
        vec![
            shape(1, GraphicKind::Rectangle),
            path(2),
            shape(3, GraphicKind::Curve),
            closed,
            path(5),
            picture(6),
            compound,
        ],
        vec![1, 2, 3, 4, 6, 7],
    );
    app.tool = Tool::Select;
    app
}

#[test]
fn path_heads_context_toggles_each_end_in_one_undo_step() {
    use reshiki::arrows::Head;
    for tool in [Tool::Select, Tool::Lasso] {
        let mut app = path_heads_app();
        app.tool = tool;
        for (label, head, tail) in [
            ("End head", Head::Full, Head::None),
            ("Start head", Head::Full, Head::Full),
            ("No end head", Head::None, Head::Full),
            ("No start head", Head::None, Head::None),
        ] {
            let before = app.tab.doc.clone();
            let revision = app.tab.revision;
            let command = app
                .context_commands()
                .into_iter()
                .find(|command| command.label == label)
                .unwrap();
            assert!(command.enabled);
            let _ = app.update(command.message);
            assert_eq!(app.tab.revision, revision + 1);
            for graphic in &app.tab.doc.graphics[1..=2] {
                let heads = graphic.heads.unwrap();
                assert_eq!((heads.head, heads.tail), (head, tail));
            }
            assert_undo_redo(&mut app, &before);
            let _ = app.update(Message::Undo);
            assert!(!app.tab.history.can_undo(), "each toggle is one undo step");
            let _ = app.update(Message::Redo);
            app.tab.history = Default::default();
        }
    }
}

#[test]
fn path_heads_properties_apply_to_open_selection_with_one_undo_each() {
    use reshiki::arrows::{Head, HeadShape};
    use reshiki::graphics::PathHeads;
    let mut app = path_heads_app();
    // Each path keeps its own shape and sizes when changing an end.
    app.tab.doc.graphics[2].heads = Some(PathHeads {
        head: Head::Right,
        shape: HeadShape::Open,
        head_length_pt: 9.,
        ..Default::default()
    });
    for action in [
        Action::Head(Head::Full),
        Action::Tail(Head::Left),
        Action::HeadShape(HeadShape::Hollow),
        Action::ApplyHeadNumber(HeadField::Length),
        Action::ApplyHeadNumber(HeadField::Width),
        Action::ApplyHeadNumber(HeadField::Notch),
    ] {
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        if let Action::ApplyHeadNumber(field) = action {
            let _ = app.update(Message::Graphics(Action::HeadNumber(field, "0.75".into())));
            assert_eq!(app.tab.doc, before, "typing only updates the draft");
            assert_eq!(app.tab.revision, revision);
        }
        let _ = app.update(Message::Graphics(action.clone()));
        assert_eq!(app.tab.revision, revision + 1);
        for index in [0, 3, 4, 5, 6] {
            assert_eq!(app.tab.doc.graphics[index], before.graphics[index]);
        }
        for (old, graphic) in before.graphics[1..=2]
            .iter()
            .zip(&app.tab.doc.graphics[1..=2])
        {
            let mut expected = old.heads.unwrap_or_default();
            match action {
                Action::Head(value) => expected.head = value,
                Action::Tail(value) => expected.tail = value,
                Action::HeadShape(value) => expected.shape = value,
                Action::ApplyHeadNumber(field) => match field {
                    HeadField::Length => expected.head_length_pt = 0.75,
                    HeadField::Width => expected.head_width_pt = 0.75,
                    HeadField::Notch => expected.head_notch = 0.75,
                },
                _ => unreachable!(),
            }
            assert_eq!(graphic.heads, Some(expected));
        }
        let heads = app.tab.doc.graphics[1].heads.unwrap();
        assert_eq!(
            app.tab.path_head_inputs,
            HeadField::ALL.map(|field| field.get(&heads).to_string())
        );
        assert_undo_redo(&mut app, &before);
        let _ = app.update(Message::Undo);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        app.tab.history = Default::default();
    }
    app.tool = Tool::Graphic(GraphicKind::Path);
    assert!(app.selected_open_paths().next().is_some());
    let _ = app.graphic_panel();
    assert!(!app.context_commands().iter().any(|command| matches!(
        command.message,
        Message::Graphics(Action::Head(_) | Action::Tail(_))
    )));
}

#[test]
fn path_heads_bad_numbers_preserve_document_drafts_and_history() {
    use reshiki::arrows::Head;
    let mut app = path_heads_app();
    let _ = app.update(Message::Graphics(Action::Head(Head::Full)));
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    for (field, value, error) in [
        (HeadField::Length, "bad", "Type a number, then press Enter"),
        (HeadField::Length, "0.49", "Arrowhead length must be 0.5–24"),
        (HeadField::Length, "NaN", "Arrowhead length must be 0.5–24"),
        (
            HeadField::Width,
            "16.1",
            "Arrowhead half-width must be 0.25–16",
        ),
        (HeadField::Notch, "0.91", "Arrowhead notch must be 0–0.9"),
    ] {
        let _ = app.update(Message::Graphics(Action::HeadNumber(field, value.into())));
        let _ = app.update(Message::Graphics(Action::ApplyHeadNumber(field)));
        assert!(app.error);
        assert_eq!(app.status, error);
        assert_eq!(app.tab.path_head_inputs[field as usize], value);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
    }
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.graphics.iter().all(|g| g.heads.is_none()));
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, before);
}
