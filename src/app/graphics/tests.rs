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
    let _ = app.update(Message::GraphicWidth("2".into()));
    let _ = app.update(Message::ApplyGraphicWidth);
    let _ = app.update(Message::GraphicStroke("#117E6C".into()));
    let _ = app.update(Message::ApplyGraphicStroke);
    let redo = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    let before = app.tab.doc.clone();
    let style = app.tab.graphic_style.clone();
    let revision = app.tab.revision;
    assert!(app.tab.history.can_undo());
    assert!(app.tab.history.can_redo());

    for draft in ["NaN", "inf", "0", "12.1", " 2"] {
        let _ = app.update(Message::GraphicWidth(draft.into()));
        let _ = app.update(Message::ApplyGraphicWidth);
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
        let _ = app.update(Message::GraphicStroke(draft.into()));
        let _ = app.update(Message::ApplyGraphicStroke);
        assert!(app.error);
        assert_eq!(app.status, "Enter a six-digit hex color, such as #117E6C");
        assert_eq!(app.tab.graphic_stroke_input, draft);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.graphic_style, style);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [1]);
        assert!(app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());

        let _ = app.update(Message::GraphicFill(draft.into()));
        let _ = app.update(Message::ApplyGraphicFill);
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
            Message::GraphicWidth("2.5".into()),
            Message::ApplyGraphicWidth,
        ),
        (
            Message::GraphicStroke("#117E6C".into()),
            Message::ApplyGraphicStroke,
        ),
        (
            Message::GraphicFill("#DCEFE9".into()),
            Message::ApplyGraphicFill,
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
    let _ = app.update(Message::ScientificKind(symbol));
    assert_eq!(app.tool, Tool::Graphic(symbol));
    assert_eq!(app.toolbar.symbol, Tool::Graphic(symbol));
    assert_eq!(app.tab.doc.graphics[0].kind, symbol);
    assert_eq!(app.tab.doc.graphics[1..], before.graphics[1..]);
    assert_undo_redo(&mut app, &before);

    let orbital = GraphicKind::Orbital(OrbitalKind::Hybrid);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ScientificKind(orbital));
    assert_eq!(app.tool, Tool::Graphic(orbital));
    assert_eq!(app.toolbar.orbital, Tool::Graphic(orbital));
    assert_eq!(app.toolbar.symbol, Tool::Graphic(symbol));
    assert_eq!(app.tab.doc.graphics[0], before.graphics[0]);
    assert_eq!(app.tab.doc.graphics[1].kind, orbital);
    assert_eq!(app.tab.doc.graphics[2..], before.graphics[2..]);
    assert_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::OrbitalPhase(Phase::Shaded));
    assert_eq!(app.tab.orbital_phase, Phase::Shaded);
    assert!(
        app.tab.doc.graphics[..4]
            .iter()
            .all(|g| g.phase == Phase::Shaded)
    );
    assert_eq!(app.tab.doc.graphics[4], unselected);
    assert_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::FlipPhase(true));
    assert!(app.tab.phase_flipped);
    assert!(app.tab.doc.graphics[..4].iter().all(|g| g.phase_flipped));
    assert_eq!(app.tab.doc.graphics[4], unselected);
    assert_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::GraphicSides(BracketSides::Right));
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
        let _ = app.update(Message::GraphicWidth(width.into()));
        let _ = app.update(Message::ApplyGraphicWidth);
        assert_eq!(
            app.tab.graphic_style.width_pt,
            width.parse::<f32>().unwrap()
        );
        assert_eq!(app.tab.graphic_width_input, width);
    }
    let _ = app.update(Message::GraphicStroke(" ##a1B2c3 ".into()));
    let _ = app.update(Message::ApplyGraphicStroke);
    assert_eq!(
        app.tab.graphic_style.stroke,
        Paint::Custom([0xa1, 0xb2, 0xc3])
    );
    assert_eq!(app.tab.graphic_stroke_input, "#A1B2C3");
    let _ = app.update(Message::GraphicFill("#DCEFE9".into()));
    let _ = app.update(Message::ApplyGraphicFill);
    assert_eq!(
        app.tab.graphic_style.fill,
        Some(Paint::Custom([0xdc, 0xef, 0xe9]))
    );
    assert_eq!(app.tab.graphic_fill_input, "#DCEFE9");
    let _ = app.update(Message::OrbitalPhase(Phase::Open));
    let _ = app.update(Message::FlipPhase(true));
    let _ = app.update(Message::GraphicSides(BracketSides::Left));
    let _ = app.update(Message::AttachSymbols(false));
    let kind = GraphicKind::Orbital(OrbitalKind::Dxy);
    let _ = app.update(Message::ScientificKind(kind));
    assert_eq!(app.tool, Tool::Graphic(kind));
    assert_eq!(app.toolbar.orbital, Tool::Graphic(kind));
    assert_eq!(app.tab.orbital_phase, Phase::Open);
    assert!(app.tab.phase_flipped);
    assert_eq!(app.tab.bracket_sides, BracketSides::Left);
    assert!(!app.tab.attach_symbols);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(app.tab.selected.is_empty());
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
}
