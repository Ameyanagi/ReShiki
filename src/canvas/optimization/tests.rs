use super::super::World;
use super::*;
use iced::widget::canvas::Program;

fn pointer(canvas: &MoleculeCanvas<'_>, state: &mut State, event: mouse::Event) -> Option<Edit> {
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    canvas
        .update(
            state,
            &Event::Mouse(event),
            bounds,
            mouse::Cursor::Available(Point::new(200., 150.)),
        )
        .and_then(|action| action.into_inner().0)
}

#[test]
fn optimizer_drag_publishes_targets_instead_of_drawing_moves_and_fixed_pins_do_not_drag() {
    let mut doc = reshiki::document::Document::default();
    let atom = doc.add_atom("C", World::default());
    let ids = [atom];
    let mut canvas = super::super::tests::chain_canvas(&doc, reshiki::chains::ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.optimizer = Some(Context {
        session: 7,
        atoms: &ids,
        pins: &[],
        interactive: true,
    });
    let mut state = State::default();
    assert!(
        matches!(pointer(&canvas, &mut state, mouse::Event::ButtonPressed(mouse::Button::Left)), Some(Edit::RelaxDragStart { atom: id, .. }) if id == atom)
    );
    assert!(
        matches!(pointer(&canvas, &mut state, mouse::Event::CursorMoved { position: Point::new(230., 160.) }), Some(Edit::RelaxDragTarget { target, .. }) if target == World::new(30., 10.))
    );
    assert!(matches!(
        pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left)
        ),
        Some(Edit::RelaxDragEnd { .. })
    ));
    canvas.optimizer = Some(Context {
        session: 7,
        atoms: &ids,
        pins: &ids,
        interactive: true,
    });
    state.cursor = None;
    assert!(
        matches!(pointer(&canvas, &mut state, mouse::Event::ButtonPressed(mouse::Button::Left)), Some(Edit::Select(selected)) if selected == ids)
    );
    assert!(state.relaxation.is_none());
}

#[test]
fn leaving_the_canvas_removes_the_temporary_constraint() {
    let mut state = State {
        relaxation: Some(Drag::Atom {
            session: 3,
            atom: 1,
        }),
        ..Default::default()
    };
    assert!(matches!(
        cancelled(&mut state),
        Some(Edit::RelaxDragCancel { session: 3 })
    ));
    assert!(state.relaxation.is_none());
}
