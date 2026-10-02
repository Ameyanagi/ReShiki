//! Real pointer events for the native aniline H₂ click acceptance failure.
use super::*;
use iced::widget::canvas::Program;

pub(crate) fn aniline() -> (Document, u64) {
    let mut doc = Document::default();
    doc.drawing_style.font_family = "Arial".into();
    doc.drawing_style.font_size_pt = 10.;
    // The seven-atom MOL acceptance input, converted from angstroms to world
    // coordinates. Populate computed H after creating the complete graph.
    let ids: Vec<_> = [
        ("N", 2.5714, 0.),
        ("C", 1.0714, 0.),
        ("C", 0.3214, -1.299),
        ("C", -1.1786, -1.299),
        ("C", -1.9286, 0.),
        ("C", -1.1786, 1.299),
        ("C", 0.3214, 1.299),
    ]
    .into_iter()
    .map(|(element, x, y)| doc.add_atom(element, World::new(x * 28., -y * 28.)))
    .collect();
    for (a, b, order) in [
        (0, 1, 1),
        (1, 2, 2),
        (2, 3, 1),
        (3, 4, 2),
        (4, 5, 1),
        (5, 6, 2),
        (6, 1, 1),
    ] {
        doc.add_bond(ids[a], ids[b], order, "plain");
    }
    doc.atom_mut(ids[0]).unwrap().label_h = 2;
    (doc, ids[0])
}

pub(crate) fn subscript_ink(doc: &Document) -> World {
    primitives(doc)
        .into_iter()
        .find_map(|primitive| {
            let Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } = primitive
            else {
                return None;
            };
            if text != "2" {
                return None;
            }
            let (lo, hi) = reshiki::style::text_ink_boxes(&text, size, &style)
                .into_iter()
                .next()?;
            Some(position.offset((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.))
        })
        .expect("Actual rendered H₂ subscript ink")
}

fn bounds() -> Rectangle {
    Rectangle::new(Point::new(20., 50.), iced::Size::new(1040., 680.))
}

fn send(canvas: &MoleculeCanvas<'_>, state: &mut State, event: Event) -> Option<Edit> {
    canvas
        .update(state, &event, bounds(), mouse::Cursor::Unavailable)
        .and_then(|action| action.into_inner().0)
}

fn screen(canvas: &MoleculeCanvas<'_>, point: World) -> Point {
    let paper = canvas.guides.paper(bounds());
    canvas.camera.screen(point, paper) + Vector::new(paper.x, paper.y)
}

/// No synthetic motion between press/release or after release: that would
/// restore hover and hide the bug encountered by click-then-key users.
pub(crate) fn click_edits(
    doc: &Document,
    selected: &[u64],
    camera: Camera,
    point: World,
    rulers: bool,
) -> Vec<Edit> {
    let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.selected = selected;
    canvas.camera = camera;
    canvas.guides.rulers = rulers;
    let position = screen(&canvas, point);
    let mut state = State::default();
    [
        mouse::Event::CursorMoved { position },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ]
    .into_iter()
    .filter_map(|event| send(&canvas, &mut state, Event::Mouse(event)))
    .collect()
}

#[test]
fn label_click_keeps_foreground_artwork_and_adjacent_bond_precedence() {
    let (mut doc, target) = aniline();
    let point = subscript_ink(&doc);
    let zoom = 1.99;
    assert!(doc.nearest(point, 10. / zoom).is_none());
    assert_eq!(hit_selection(&doc, point, 10. / zoom), vec![target]);
    let bond = &doc.bonds[0];
    let a = doc.atom(bond.a).unwrap().position;
    let b = doc.atom(bond.b).unwrap().position;
    let middle = World::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    assert_eq!(
        hit_selection(&doc, middle, 10. / zoom),
        vec![bond.a, bond.b]
    );
    let id = doc.next_id();
    let mut graphic = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        point.offset(-10., -10.),
        point.offset(10., 10.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    graphic.style.fill = Some(reshiki::palette::Color::Custom([220, 239, 233]));
    graphic.layer = -1;
    doc.graphics.push(graphic);
    assert_eq!(hit_selection(&doc, point, 10. / zoom), vec![target]);
    doc.graphics[0].layer = 1;
    assert_eq!(hit_selection(&doc, point, 10. / zoom), vec![id]);
}

#[test]
fn canceled_label_gestures_never_restore_stale_hover() {
    let (doc, _) = aniline();
    let point = subscript_ink(&doc);
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.camera.zoom = 1.99;
    for cancel in [
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        Event::Window(iced::window::Event::Unfocused),
        Event::Mouse(mouse::Event::CursorLeft),
    ] {
        let mut state = State::default();
        let position = screen(&canvas, point);
        assert!(matches!(
            send(
                &canvas,
                &mut state,
                Event::Mouse(mouse::Event::CursorMoved { position })
            ),
            Some(Edit::Hover(Some(_)))
        ));
        assert!(matches!(
            send(
                &canvas,
                &mut state,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            ),
            Some(Edit::Hover(None))
        ));
        assert!(!matches!(
            send(&canvas, &mut state, cancel),
            Some(Edit::Hover(Some(_)))
        ));
        assert!(
            send(
                &canvas,
                &mut state,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            )
            .is_none()
        );
        assert!(state.gesture.is_none());
        assert!(matches!(
            send(&canvas, &mut state, Event::Mouse(mouse::Event::CursorLeft)),
            Some(Edit::Hover(None))
        ));
        assert!(state.cursor.is_none());
    }
}
