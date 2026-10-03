//! Feed actual canvas events to the app's rotation/history acceptance test.
use super::*;
use iced::{keyboard, widget::canvas::Program};
use reshiki::scene;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Finish {
    Release,
    Escape,
    FocusLost,
}

pub(crate) struct GestureEvents {
    pub before_release: Vec<Edit>,
    pub release: Option<Edit>,
    pub preview: Document,
}

pub(crate) fn rotation_drag(
    doc: &Document,
    selected: &[u64],
    camera: Camera,
    expected_pivot: World,
    moves: usize,
    finish: Finish,
) -> GestureEvents {
    let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.selected = selected;
    canvas.camera = camera;
    let bounds = Rectangle::new(Point::new(80., 100.), iced::Size::new(1600., 1400.));
    let selection = SelectionBox::new(doc, selected, camera, bounds).unwrap();
    assert!(selection.pivot.distance(expected_pivot) < 0.0001);
    let (lo, hi) = scene::selection_bounds(doc, selected).unwrap();
    let top = camera.screen(World::new((lo.x + hi.x) * 0.5, lo.y), bounds);
    let grip = Point::new(top.x, top.y - 40.);
    // Verify this is the actual interactive rotation grip, including its
    // screen-space offset; an asymmetric selection need not sit below it.
    assert_eq!(selection.hit(grip), Some(Handle::Rotate));
    let pivot = camera.screen(expected_pivot, bounds);
    let offset = Vector::new(bounds.x, bounds.y);
    let mut state = State::default();
    let send = |state: &mut State, event: Event| {
        canvas
            .update(state, &event, bounds, mouse::Cursor::Unavailable)
            .and_then(|action| action.into_inner().0)
    };
    let mut before_release = Vec::new();
    for event in [
        Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::SHIFT,
        )),
        Event::Mouse(mouse::Event::CursorMoved {
            position: grip + offset,
        }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    ] {
        before_release.extend(send(&mut state, event));
    }
    assert!(matches!(
        state.gesture,
        Some(Gesture::Transform(ref drag)) if drag.handle == Handle::Rotate
    ));
    let mut end = grip;
    for step in 1..=moves {
        let (s, c) = (15_f64 * step as f64 / moves as f64).to_radians().sin_cos();
        let x = f64::from(grip.x - pivot.x);
        let y = f64::from(grip.y - pivot.y);
        end = Point::new(
            (f64::from(pivot.x) + x * c - y * s) as f32,
            (f64::from(pivot.y) + x * s + y * c) as f32,
        );
        before_release.extend(send(
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved {
                position: end + offset,
            }),
        ));
    }
    let mut preview = doc.clone();
    let Some(Gesture::Transform(drag)) = &state.gesture else {
        panic!("Pointer motions must retain the rotation gesture");
    };
    drag.apply(&mut preview, camera.world(end, bounds), true);
    let cancel = match finish {
        Finish::Release => None,
        Finish::Escape => Some(Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::SHIFT,
            text: None,
            repeat: false,
        })),
        Finish::FocusLost => Some(Event::Window(iced::window::Event::Unfocused)),
    };
    if let Some(event) = cancel {
        before_release.extend(send(&mut state, event));
        assert!(state.gesture.is_none());
    }
    assert!(
        before_release
            .iter()
            .all(|edit| matches!(edit, Edit::Hover(_)))
    );
    let release = send(
        &mut state,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    );
    assert!(state.gesture.is_none());
    if moves == 0 || !matches!(finish, Finish::Release) {
        assert!(release.is_none(), "Cancel/click must publish no transform");
    } else {
        assert!(matches!(
            release,
            Some(Edit::Transform {
                scale: 1.,
                rotation: 15.,
                ..
            })
        ));
    }
    GestureEvents {
        before_release,
        release,
        preview,
    }
}
