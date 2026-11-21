//! Pointer ownership for the disposable test piece, separate from drawing gestures.
use crate::canvas::{Edit, Gesture, MoleculeCanvas, State, World};
use iced::{Event, Point, Rectangle, keyboard, mouse, widget::canvas::Action};

#[derive(Clone, Copy)]
pub(in crate::canvas) struct Capture {
    epoch: u64,
    offset: World,
}

#[cfg(test)]
#[path = "equipment_trial_tests.rs"]
mod tests;

impl Capture {
    pub(super) fn cancelled(self) -> Edit {
        Edit::Touch(vec![
            Edit::EquipmentAuthoringTestCancel(self.epoch),
            Edit::Hover(None),
        ])
    }
}

impl MoleculeCanvas<'_> {
    pub(super) fn equipment_trial_event(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        point: Option<Point>,
        inside: bool,
    ) -> Option<Action<Edit>> {
        let Some((id, at, epoch)) = self.equipment_authoring.and_then(|c| c.trial) else {
            state.equipment_trial = None;
            return None;
        };
        if state
            .equipment_trial
            .is_some_and(|capture| capture.epoch != epoch)
        {
            state.equipment_trial = None;
        }
        let reach = self.camera.world_len(reshiki::equipment::solve::REACH_PX);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                let p = self.camera.world(point?, bounds);
                if crate::canvas::hit::hit_object(self.doc, p, self.camera.world_len(6.))
                    != Some(id)
                {
                    return None;
                }
                state.end_gesture();
                state.last_click = None;
                state.equipment_trial = Some(Capture {
                    epoch,
                    offset: World::new(at.x - p.x, at.y - p.y),
                });
                Some(Action::publish(Edit::EquipmentAuthoringTestStart(epoch)).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.equipment_trial.is_some() => {
                let capture = state.equipment_trial?;
                Some(if inside {
                    let p = self.camera.world(point?, bounds);
                    Action::publish(Edit::EquipmentAuthoringTest {
                        at: p.offset(capture.offset.x, capture.offset.y),
                        epoch,
                        reach,
                        finish: false,
                    })
                    .and_capture()
                } else {
                    Action::capture()
                })
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.equipment_trial.is_some() =>
            {
                let capture = state.equipment_trial.take()?;
                Some(
                    Action::publish(if inside {
                        let p = self.camera.world(point?, bounds);
                        Edit::EquipmentAuthoringTest {
                            at: p.offset(capture.offset.x, capture.offset.y),
                            epoch,
                            reach,
                            finish: true,
                        }
                    } else {
                        capture.cancelled()
                    })
                    .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::ButtonReleased(_) | mouse::Event::WheelScrolled { .. })
                if state.equipment_trial.is_some() =>
            {
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
                if state.equipment_trial.is_some() =>
            {
                let capture = state.equipment_trial.take()?;
                state.gesture = Some(Gesture::Pan { last: point? });
                Some(Action::publish(capture.cancelled()).and_capture())
            }
            Event::Mouse(
                mouse::Event::CursorLeft | mouse::Event::ButtonPressed(mouse::Button::Right),
            )
            | Event::Window(iced::window::Event::Unfocused)
            | Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) if state.equipment_trial.is_some() => {
                let capture = state.equipment_trial.take()?;
                state.cursor = None;
                state.hovering = false;
                state.bond_text_hover = None;
                Some(Action::publish(capture.cancelled()).and_capture())
            }
            _ => None,
        }
    }
}
