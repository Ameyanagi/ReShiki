//! Reveal and focus a numeric field even when its inspector was scrolled away.
use super::Field;
use iced::advanced::widget::{
    Id, Operation,
    operation::{Focusable, Outcome, Scrollable, TextInput, scrollable},
};
use iced::{Rectangle, Vector};

pub(super) struct FieldOperation {
    target: Id,
    field: Option<Rectangle>,
    viewport: Option<Rectangle>,
}

impl FieldOperation {
    pub(super) fn new(field: Field) -> Self {
        Self {
            target: field.id().into(),
            field: None,
            viewport: None,
        }
    }
}

impl<T: 'static> Operation<T> for FieldOperation {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        _: Vector,
        _: &mut dyn Scrollable,
    ) {
        if id == Some(&Id::from("inspector-content")) {
            self.viewport = Some(bounds);
        }
    }

    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            state.focus();
        } else {
            state.unfocus();
        }
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn TextInput) {
        if id == Some(&self.target) {
            self.field = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<T> {
        let (Some(field), Some(viewport)) = (self.field, self.viewport) else {
            return Outcome::None;
        };
        // Operate sees content coordinates before the scroll translation.
        // Center the field so the section's heading and Apply remain nearby.
        let y = (field.center_y() - viewport.center_y()).max(0.);
        Outcome::Chain(Box::new(Reveal {
            target: self.target.clone(),
            y,
        }))
    }
}

struct Reveal {
    target: Id,
    y: f32,
}

impl<T> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn text_input(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn TextInput) {
        if id == Some(&self.target) {
            // TextInput::focus moves its caret to the end, so select in the
            // following pass, after focus has been acquired.
            state.select_all();
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        _: Rectangle,
        _: Rectangle,
        _: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id == Some(&Id::from("inspector-content")) {
            state.scroll_to(scrollable::AbsoluteOffset {
                x: None,
                y: Some(self.y),
            });
        }
    }
}

#[cfg(test)]
mod tests;
