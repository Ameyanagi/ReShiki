//! Caption bonding through actual iced pointer events and shared hit resolution.
use super::*;
use iced::widget::canvas::Program;
use reshiki::document::Annotation;

fn caption(doc: &mut Document, text: &str, position: World) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position,
        text: text.into(),
        format: Default::default(),
    });
    id
}

fn annotation(doc: &Document, id: u64) -> &Annotation {
    doc.annotations.iter().find(|a| a.id == id).unwrap()
}

/// Where a convertible caption's atom lands (inside its ink); a refused
/// caption has no atom, so use its first glyph as the pointer position.
fn ink(doc: &Document, id: u64) -> World {
    reshiki::atom_text::text_atom_position(doc, id)
        .or_else(|| reshiki::atom_text::text_first_glyph_center(annotation(doc, id)))
        .unwrap()
}

fn bounds() -> Rectangle {
    Rectangle::new(Point::new(20., 50.), iced::Size::new(1040., 680.))
}

fn screen(canvas: &MoleculeCanvas<'_>, p: World) -> Point {
    let paper = canvas.guides.paper(bounds());
    canvas.camera.screen(p, paper) + Vector::new(paper.x, paper.y)
}

fn send(canvas: &MoleculeCanvas<'_>, state: &mut State, event: mouse::Event) -> Option<Edit> {
    canvas
        .update(
            state,
            &Event::Mouse(event),
            bounds(),
            mouse::Cursor::Unavailable,
        )
        .and_then(|action| action.into_inner().0)
}

fn press(canvas: &MoleculeCanvas<'_>, state: &mut State, p: World) -> Option<Edit> {
    send(
        canvas,
        state,
        mouse::Event::CursorMoved {
            position: screen(canvas, p),
        },
    );
    send(
        canvas,
        state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
    )
}

fn release(canvas: &MoleculeCanvas<'_>, state: &mut State, p: World) -> Option<Edit> {
    send(
        canvas,
        state,
        mouse::Event::CursorMoved {
            position: screen(canvas, p),
        },
    );
    send(
        canvas,
        state,
        mouse::Event::ButtonReleased(mouse::Button::Left),
    )
}

#[test]
fn caption_ink_press_starts_text_draw_at_the_converted_atom() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::new(-30., 10.));
    let point = ink(&doc, text);
    let before = doc.clone();
    for tool in [
        Tool::Bond(1),
        Tool::Bond(2),
        Tool::StyledBond(reshiki::bonds::BondPreset::Dative),
        Tool::Wedge,
        Tool::Hash,
        Tool::Wavy,
    ] {
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = tool;
        let mut state = State::default();
        assert!(matches!(
            press(&canvas, &mut state, point),
            Some(Edit::Hover(None))
        ));
        let Some(Gesture::Draw {
            start,
            id,
            text: source_text,
            ..
        }) = state.gesture
        else {
            panic!("Caption ink must begin a draw gesture for {tool:?}");
        };
        assert_eq!(start, point);
        assert_eq!(id, None);
        assert_eq!(source_text, Some(text));
        assert_eq!(doc, before, "A press must not convert text");
    }
}

#[test]
fn caption_layout_padding_does_not_start_a_text_source() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::new(-30., 10.));
    let a = doc.annotations.iter_mut().find(|a| a.id == text).unwrap();
    a.format.width_pt = Some(120.);
    let (width, height) = a.size();
    let point = a.position.offset(width - 1., height / 2.);
    assert_eq!(hit_object(&doc, point, 10.), Some(text));
    assert_eq!(hit::bond_text_at(&doc, point, 10.), None);
    let before = doc.clone();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    let mut state = State::default();
    press(&canvas, &mut state, point);
    assert!(matches!(
        state.gesture,
        Some(Gesture::Draw {
            text: None,
            id: None,
            ..
        })
    ));
    assert_eq!(doc, before);
}

#[test]
fn atom_select_text_and_dotted_tools_keep_their_caption_press_behavior() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::default());
    let point = ink(&doc, text);
    let before = doc.clone();
    for tool in [
        Tool::Atom,
        Tool::Select,
        Tool::Text,
        Tool::StyledBond(reshiki::bonds::BondPreset::Dotted),
    ] {
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = tool;
        let mut state = State::default();
        let edit = press(&canvas, &mut state, point);
        match tool {
            Tool::Atom | Tool::Text => {
                assert!(matches!(edit, Some(Edit::Click(_))));
                assert!(state.gesture.is_none());
            }
            Tool::Select => {
                assert!(
                    matches!(state.gesture, Some(Gesture::Move { ref clicked, .. }) if clicked == &[text])
                );
            }
            Tool::StyledBond(_) => {
                assert!(matches!(
                    state.gesture,
                    Some(Gesture::Draw { text: None, .. })
                ));
            }
            _ => unreachable!(),
        }
        assert_eq!(doc, before, "{tool:?}");
    }
}

#[test]
fn caption_drag_release_publishes_text_bond_without_mutating_the_document() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::new(-30., 10.));
    let point = ink(&doc, text);
    let cursor = point.offset(100., -30.);
    let before = doc.clone();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    let mut state = State::default();
    press(&canvas, &mut state, point);
    let Some(Edit::TextBond {
        source,
        source_text,
        start,
        end,
        target,
        target_text,
    }) = release(&canvas, &mut state, cursor)
    else {
        panic!("Caption drag must publish a conversion-and-bond edit");
    };
    assert_eq!(
        (source, source_text, target, target_text),
        (None, Some(text), None, None)
    );
    assert!(start.distance(point) < 0.001);
    assert!(
        end.unwrap()
            .distance(canvas.bond_drawing.endpoint(point, cursor))
            < 0.001
    );
    assert!(state.gesture.is_none());
    assert_eq!(doc, before);
}

#[test]
fn caption_click_release_requests_default_bond_growth() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::default());
    let point = ink(&doc, text);
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    let mut state = State::default();
    press(&canvas, &mut state, point);
    assert!(matches!(release(&canvas, &mut state, point),
        Some(Edit::TextBond { source: None, source_text: Some(id), end: None,
            target: None, target_text: None, .. }) if id == text));
    assert_eq!(doc.annotations.len(), 1);
    assert!(doc.atoms.is_empty());
}

#[test]
fn click_on_the_last_caption_glyph_still_requests_default_bond_growth() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::default());
    let first = ink(&doc, text);
    let (lo, hi) = reshiki::atom_text::text_ink_bounds(annotation(&doc, text)).unwrap();
    let pressed = World::new(hi.x - 0.2, (lo.y + hi.y) / 2.);
    assert!(pressed.distance(first) > 3.);
    assert_eq!(hit::bond_text_at(&doc, pressed, 10.), Some(text));
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    let mut state = State::default();
    press(&canvas, &mut state, pressed);
    assert!(
        matches!(state.gesture, Some(Gesture::Draw { start, text: Some(id), .. })
        if id == text && start == first)
    );
    assert!(matches!(release(&canvas, &mut state, pressed),
        Some(Edit::TextBond { source_text: Some(id), end: None, .. }) if id == text));
    assert_eq!(doc.annotations.len(), 1);
    assert!(doc.atoms.is_empty());
}

#[test]
fn atom_drag_release_targets_caption_at_the_real_cursor_position() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::new(-180., -70.));
    let text = caption(&mut doc, "Pd2+", World::new(80., 10.));
    let start = doc.atom(source).unwrap().position;
    let cursor = ink(&doc, text);
    let before = doc.clone();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    assert!(canvas.bond_drawing.endpoint(start, cursor).distance(cursor) > 100.);
    let mut state = State::default();
    press(&canvas, &mut state, start);
    let Some(Edit::TextBond {
        source: a,
        source_text,
        start: origin,
        end,
        target,
        target_text,
    }) = release(&canvas, &mut state, cursor)
    else {
        panic!("The cursor's caption must become the target");
    };
    assert_eq!(
        (a, source_text, target, target_text),
        (Some(source), None, None, Some(text))
    );
    assert!(origin.distance(start) < 0.001);
    assert!(end.unwrap().distance(cursor) < 0.001);
    assert_eq!(doc, before);
}

#[test]
fn caption_drag_to_caption_release_publishes_both_text_ids_and_ink_positions() {
    let mut doc = Document::default();
    let source = caption(&mut doc, "Pd2+", World::new(-80., 20.));
    let target = caption(&mut doc, "O", World::new(80., 20.));
    let start = ink(&doc, source);
    let end = ink(&doc, target);
    let before = doc.clone();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    assert!(canvas.bond_drawing.endpoint(start, end).distance(end) > 80.);
    let mut state = State::default();
    press(&canvas, &mut state, start);
    let Some(Edit::TextBond {
        source: source_atom,
        source_text,
        start: origin,
        end: endpoint,
        target: target_atom,
        target_text,
    }) = release(&canvas, &mut state, end)
    else {
        panic!("A caption-to-caption drag must convert both endpoint captions");
    };
    assert_eq!((source_atom, target_atom), (None, None));
    assert_eq!((source_text, target_text), (Some(source), Some(target)));
    assert_eq!((origin, endpoint), (start, Some(end)));
    let (converted, pd) = reshiki::atom_text::text_to_atom(&doc, source).unwrap();
    let (mut converted, oxygen) = reshiki::atom_text::text_to_atom(&converted, target).unwrap();
    converted.add_bond(pd, oxygen, 1, "plain");
    assert!(converted.annotations.is_empty());
    assert_eq!((converted.atoms.len(), converted.bonds.len()), (2, 1));
    assert_eq!(
        (
            converted.atom(pd).unwrap().position,
            converted.atom(pd).unwrap().charge
        ),
        (origin, 2)
    );
    assert_eq!(converted.atom(oxygen).unwrap().position, endpoint.unwrap());
    assert!(state.gesture.is_none());
    assert_eq!(doc, before);
}

#[test]
fn snapped_endpoint_over_caption_does_not_target_text_away_from_the_cursor() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::default());
    let cursor = World::new(220., 0.);
    let drawing = BondDrawing::default();
    let snapped = drawing.endpoint(World::default(), cursor);
    let text = caption(&mut doc, "Pd2+", World::default());
    let first = ink(&doc, text);
    doc.annotations[0].position = snapped.offset(-first.x, -first.y);
    assert_eq!(hit::bond_text_at(&doc, snapped, 12.), Some(text));
    assert_eq!(hit::bond_text_at(&doc, cursor, 12.), None);
    let (end, target, target_text) = hit::bond_target_with_text(
        &doc,
        World::default(),
        cursor,
        (Some(source), None),
        12.,
        drawing,
        Tool::Bond(1),
    );
    assert_eq!((end, target, target_text), (snapped, None, None));
}

#[test]
fn caption_under_the_cursor_beats_an_atom_at_the_snapped_endpoint() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::default());
    let text = caption(&mut doc, "Pd2+", World::new(200., 0.));
    let cursor = ink(&doc, text);
    let drawing = BondDrawing::default();
    let snapped = drawing.endpoint(World::default(), cursor);
    let near = doc.add_atom("O", snapped);
    assert!(cursor.distance(snapped) > 100.);
    assert_eq!(hit::bond_text_at(&doc, cursor, 12.), Some(text));
    let result = hit::bond_target_with_text(
        &doc,
        World::default(),
        cursor,
        (Some(source), None),
        12.,
        drawing,
        Tool::Bond(1),
    );
    assert_eq!(result, (cursor, None, Some(text)));
    // Away from caption ink the snapped atom still takes the bond.
    let open = cursor.offset(0., 60.);
    let snapped = drawing.endpoint(World::default(), open);
    doc.atom_mut(near).unwrap().position = snapped;
    assert_eq!(hit::bond_text_at(&doc, open, 12.), None);
    assert_eq!(
        hit::bond_target_with_text(
            &doc,
            World::default(),
            open,
            (Some(source), None),
            12.,
            drawing,
            Tool::Bond(1),
        ),
        (snapped, Some(near), None)
    );
}

#[test]
fn text_target_helper_returns_the_converted_atom_position() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::new(150., 20.));
    let a = annotation(&doc, text);
    let (lo, hi) = reshiki::atom_text::text_ink_bounds(a).unwrap();
    let cursor = World::new(hi.x - 0.2, (lo.y + hi.y) / 2.);
    assert_eq!(hit::bond_text_at(&doc, cursor, 12.), Some(text));
    let point = ink(&doc, text);
    assert!(cursor.distance(point) > 5.);
    let before = doc.clone();
    let (end, target, target_text) = hit::bond_target_with_text(
        &doc,
        World::default(),
        cursor,
        (None, None),
        12.,
        BondDrawing::default(),
        Tool::Bond(1),
    );
    assert_eq!((end, target, target_text), (point, None, Some(text)));
    assert_eq!(doc, before);
}

#[test]
fn text_target_helper_preserves_existing_atom_attachment_positions() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::default());
    let target = doc.add_atom("O", World::new(110., -40.));
    let point = doc.atom(target).unwrap().position;
    let cursor = point.offset(2., 1.);
    let result = hit::bond_target_with_text(
        &doc,
        World::default(),
        cursor,
        (Some(source), None),
        12.,
        BondDrawing::default(),
        Tool::Bond(1),
    );
    assert_eq!(result, (point, Some(target), None));
    assert_eq!(
        bond_target_with(
            &doc,
            World::default(),
            cursor,
            Some(source),
            12.,
            BondDrawing::default()
        ),
        (point, Some(target))
    );
}

#[test]
fn text_target_helper_excludes_its_own_source_caption() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::default());
    let start = ink(&doc, text);
    let cursor = start.offset(1., 0.);
    assert_eq!(hit::bond_text_at(&doc, cursor, 12.), Some(text));
    let drawing = BondDrawing::default();
    let result = hit::bond_target_with_text(
        &doc,
        start,
        cursor,
        (None, Some(text)),
        12.,
        drawing,
        Tool::Bond(1),
    );
    assert_eq!(result, (drawing.endpoint(start, cursor), None, None));
}

#[test]
fn ineligible_tools_never_resolve_a_caption_as_a_bond_target() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::new(150., 20.));
    let cursor = ink(&doc, text);
    let drawing = BondDrawing::default();
    for tool in [
        Tool::Atom,
        Tool::Select,
        Tool::Text,
        Tool::Arrow,
        Tool::StyledBond(reshiki::bonds::BondPreset::Dotted),
    ] {
        assert_eq!(
            hit::bond_target_with_text(
                &doc,
                World::default(),
                cursor,
                (None, None),
                12.,
                drawing,
                tool,
            ),
            (drawing.endpoint(World::default(), cursor), None, None),
            "{tool:?}"
        );
    }
}

#[test]
fn refused_caption_press_publishes_the_reason_and_starts_no_gesture() {
    let mut doc = Document::default();
    let text = caption(&mut doc, &"x".repeat(40), World::default());
    let point = ink(&doc, text);
    let before = doc.clone();
    for tool in [Tool::Bond(1), Tool::Chain(ChainMode::Straight)] {
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = tool;
        let mut state = State::default();
        let Some(Edit::TextBondRefused(reason)) = press(&canvas, &mut state, point) else {
            panic!("A refused source must publish its reason for {tool:?}");
        };
        assert_eq!(reason, reshiki::atom_text::TEXT_TOO_LONG);
        assert!(state.gesture.is_none());
        assert!(release(&canvas, &mut state, point.offset(60., 0.)).is_none());
        assert_eq!(doc, before);
    }
}

#[test]
fn caption_chain_press_and_release_preserve_the_text_source_id() {
    let mut doc = Document::default();
    let text = caption(&mut doc, "Pd2+", World::new(-30., 10.));
    let point = ink(&doc, text);
    let before = doc.clone();
    let canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    let mut state = State::default();
    press(&canvas, &mut state, point);
    assert!(matches!(state.gesture, Some(Gesture::Chain {
        start, source: None, source_text: Some(id), ..
    }) if id == text && start.distance(point) < 0.001));
    let Some(Edit::Chain {
        points,
        source,
        source_text,
        target,
    }) = release(&canvas, &mut state, point.offset(150., 0.))
    else {
        panic!("Caption chain drag must retain its source annotation");
    };
    assert_eq!((source, source_text, target), (None, Some(text), None));
    assert!(points.len() >= 2);
    assert!(points[0].distance(point) < 0.001);
    assert_eq!(doc, before);
}
