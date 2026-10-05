//! Cursor motion: advance the active gesture, otherwise report hover.

use crate::canvas::{Edit, Gesture, MoleculeCanvas, State, Tool};
use iced::widget::canvas::Action;
use iced::{Point, Rectangle};
use reshiki::chains;

impl MoleculeCanvas<'_> {
    pub(in crate::canvas) fn cursor_moved(
        &self,
        state: &mut State,
        point: Option<Point>,
        bounds: Rectangle,
        inside: bool,
        was_inside: bool,
        pointer_moved: bool,
    ) -> Option<Action<Edit>> {
        if let Some(Gesture::Erase { last }) = &mut state.gesture {
            if self.tool != Tool::Erase {
                state.gesture = None;
                return Some(Action::publish(Edit::EraseEnd));
            }
            let p = self.camera.world(point?, bounds);
            let from = *last;
            *last = p;
            return Some(Action::publish(Edit::EraseTo(from, p)).and_capture());
        }
        if let Some(Gesture::Chain {
            start,
            source,
            points,
            snaking,
            pressed,
            dragged,
        }) = &mut state.gesture
        {
            let p = self.camera.world(point?, bounds);
            *dragged |= pressed.distance(p) > 3.0 / self.camera.zoom;
            let bond = self.bond_drawing.unconstrained(state.modifiers.alt());
            *snaking |= state.modifiers.control();
            if *snaking {
                chains::snake(
                    points,
                    p,
                    source.is_some(),
                    bond,
                    self.chain_drawing,
                    state.modifiers.shift(),
                );
            } else {
                *points = chains::straight(
                    *start,
                    p,
                    source.is_some(),
                    bond,
                    self.chain_drawing,
                    state.modifiers.shift(),
                );
            }
        }
        if let Some(Gesture::Transform(drag)) = &mut state.gesture {
            drag.track_pointer(self.camera.world(point?, bounds), self.camera.zoom);
        }
        if let Some(Gesture::Lasso { points }) = &mut state.gesture {
            let p = self.camera.world(point?, bounds);
            if points
                .last()
                .is_none_or(|last| last.distance(p) > 2.0 / self.camera.zoom)
            {
                points.push(p);
            }
        }
        if let Some(Gesture::Pan { last }) = &mut state.gesture {
            let p = point?;
            let delta = p - *last;
            *last = p;
            return Some(
                Action::publish(Edit::Pan(
                    delta.x / self.camera.zoom,
                    delta.y / self.camera.zoom,
                ))
                .and_capture(),
            );
        }
        if state.gesture.is_some() {
            // A drag only changes its canvas preview. Avoid rebuilding and laying
            // out the whole application for every intermediate pointer event.
            return Some(Action::request_redraw().and_capture());
        }
        if inside || was_inside {
            if self.keyboard_target.is_some() && !pointer_moved {
                return Some(Action::request_redraw());
            }
            let hover = if inside && state.gesture.is_none() {
                point.map(|p| self.camera.world(p, bounds))
            } else {
                None
            };
            Some(Action::publish(Edit::Hover(hover)))
        } else {
            None
        }
    }
}
