use super::*;
use iced::widget::canvas::Program;

fn fixture() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-20., -10.));
    let b = doc.add_atom("C", World::new(20., 10.));
    doc.add_bond(a, b, 1, "plain");
    doc
}

fn bounds() -> Rectangle {
    Rectangle::with_size(iced::Size::new(400., 300.))
}

fn pointer(canvas: &MoleculeCanvas<'_>, state: &mut State, event: mouse::Event) -> Option<Edit> {
    canvas
        .update(
            state,
            &Event::Mouse(event),
            bounds(),
            mouse::Cursor::Unavailable,
        )
        .and_then(|action| action.into_inner().0)
}

fn click(canvas: &MoleculeCanvas<'_>, state: &mut State, position: Point) -> Option<Edit> {
    pointer(canvas, state, mouse::Event::CursorMoved { position });
    assert!(
        pointer(
            canvas,
            state,
            mouse::Event::ButtonPressed(mouse::Button::Left)
        )
        .is_none()
    );
    pointer(
        canvas,
        state,
        mouse::Event::ButtonReleased(mouse::Button::Left),
    )
}

#[test]
fn handle_double_click_routes_to_the_matching_field_at_each_zoom() {
    let doc = fixture();
    let selected = doc.all_ids();
    for zoom in [0.5, 1., 3.] {
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        canvas.selected = &selected;
        canvas.camera = Camera {
            center: World::new(7., -9.),
            zoom,
        };
        let a =
            canvas.camera.screen(World::new(-20., -10.), bounds()) - iced::Vector::new(12., 12.);
        let b = canvas.camera.screen(World::new(20., 10.), bounds()) + iced::Vector::new(12., 12.);
        for (position, expected) in [
            (
                Point::new((a.x + b.x) / 2., a.y - 28.),
                TransformField::Rotation,
            ),
            (a, TransformField::Scale),
            (Point::new(b.x, a.y), TransformField::Scale),
            (b, TransformField::Scale),
            (Point::new(a.x, b.y), TransformField::Scale),
            (Point::new((a.x + b.x) / 2., a.y), TransformField::Height),
            (Point::new(b.x, (a.y + b.y) / 2.), TransformField::Width),
            (Point::new((a.x + b.x) / 2., b.y), TransformField::Height),
            (Point::new(a.x, (a.y + b.y) / 2.), TransformField::Width),
        ] {
            let mut state = State::default();
            assert!(
                click(&canvas, &mut state, position).is_none(),
                "First click is a no-op"
            );
            canvas.update(
                &mut state,
                &Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                bounds(),
                mouse::Cursor::Unavailable,
            );
            assert!(
                matches!(click(&canvas, &mut state, position), Some(Edit::BeginTransform(field)) if field == expected),
                "zoom {zoom}, {expected:?}"
            );
            assert!(state.gesture.is_none());
            assert!(state.last_transform_click.is_none());
        }
    }
}

#[test]
fn handle_click_pairs_expire_and_intervening_actions_cancel_them() {
    let doc = fixture();
    let selected = doc.all_ids();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.selected = &selected;
    let grip = Point::new(200., 100.);
    let mut state = State::default();
    assert!(click(&canvas, &mut state, grip).is_none());
    state.last_transform_click.as_mut().unwrap().at -= std::time::Duration::from_millis(500);
    assert!(
        click(&canvas, &mut state, grip).is_none(),
        "Slow clicks remain no-ops"
    );
    // The other handle begins a new pair, rather than completing Rotate.
    assert!(click(&canvas, &mut state, Point::new(232., 172.)).is_none());
    assert!(click(&canvas, &mut state, grip).is_none());
    // A selection change is also a different pair even at the same position.
    state.last_transform_click.as_mut().unwrap().ids = vec![999];
    assert!(click(&canvas, &mut state, grip).is_none());
    for event in [
        Event::Window(iced::window::Event::Unfocused),
        Event::Mouse(mouse::Event::CursorLeft),
        Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0., y: 1. },
        }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
    ] {
        canvas.update(&mut state, &event, bounds(), mouse::Cursor::Unavailable);
        assert!(state.last_transform_click.is_none());
        assert!(click(&canvas, &mut state, grip).is_none());
    }
    // Pressing elsewhere must cancel the previous handle click.
    pointer(
        &canvas,
        &mut state,
        mouse::Event::CursorMoved {
            position: Point::new(10., 10.),
        },
    );
    pointer(
        &canvas,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
    );
    assert!(state.last_transform_click.is_none());
    pointer(
        &canvas,
        &mut state,
        mouse::Event::ButtonReleased(mouse::Button::Left),
    );
    assert!(click(&canvas, &mut state, grip).is_none());
    pointer(
        &canvas,
        &mut state,
        mouse::Event::CursorMoved {
            position: Point::new(-10., -10.),
        },
    );
    pointer(
        &canvas,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
    );
    assert!(state.last_transform_click.is_none());
}

#[test]
fn a_second_press_that_drags_still_transforms_and_out_and_back_is_not_a_click() {
    let doc = fixture();
    let selected = doc.all_ids();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.selected = &selected;
    let grip = Point::new(200., 100.);
    for return_to_start in [false, true] {
        let mut state = State::default();
        assert!(click(&canvas, &mut state, grip).is_none());
        pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Left),
        );
        pointer(
            &canvas,
            &mut state,
            mouse::Event::CursorMoved {
                position: Point::new(250., 150.),
            },
        );
        if return_to_start {
            pointer(
                &canvas,
                &mut state,
                mouse::Event::CursorMoved { position: grip },
            );
        }
        let edit = pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
        );
        assert!(
            matches!(edit, Some(Edit::Transform { rotation, .. }) if (rotation - if return_to_start { 0. } else { 90. }).abs() < 0.001)
        );
        assert!(state.last_transform_click.is_none());
    }
}
