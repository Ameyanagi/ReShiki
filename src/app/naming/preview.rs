//! A molecule canvas embedded in a scrollable inspector. Iced scrollables
//! translate the cursor but retain screen coordinates in mouse motion events.
use crate::canvas::{Edit, MoleculeCanvas};
use iced::widget::canvas::{Action, Geometry, Program};
use iced::{Event, Rectangle, Renderer, Theme, mouse};

pub(super) struct Preview<'a>(pub MoleculeCanvas<'a>);
impl Program<Edit> for Preview<'_> {
    type State = crate::canvas::State;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Edit>> {
        // Normalize every pointer event, including clicks after a scroll with no
        // intervening motion. A levitating/clipped cursor cannot start a gesture.
        if matches!(event, Event::Mouse(_)) {
            let motion = cursor
                .position()
                .map_or(Event::Mouse(mouse::Event::CursorLeft), |position| {
                    Event::Mouse(mouse::Event::CursorMoved { position })
                });
            let update = self.0.update(state, &motion, bounds, cursor);
            if matches!(
                event,
                Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            ) {
                return update;
            }
        }
        self.0.update(state, event, bounds, cursor)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        self.0.draw(state, renderer, theme, bounds, cursor)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        self.0.mouse_interaction(state, bounds, cursor)
    }
}
