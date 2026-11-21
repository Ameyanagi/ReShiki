//! F6 moves between the palette search and its live action buttons.
use super::{Message, SEARCH};
use iced::advanced::widget::{Operation, operation};
use iced::{Rectangle, Task};
use std::any::Any;

pub(super) fn cycle(backwards: bool) -> Task<Message> {
    iced::advanced::widget::operate(Actions {
        backwards,
        ids: Vec::new(),
        focused: None,
    })
    .discard()
}

struct Actions {
    backwards: bool,
    ids: Vec<String>,
    focused: Option<usize>,
}

impl Operation for Actions {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        _: Rectangle,
        state: &mut dyn Any,
    ) {
        if state.is::<reshiki::accessibility::Foreground>() {
            self.ids.clear();
            self.focused = None;
        }
        if let Some(node) = state.downcast_ref::<reshiki::accessibility::Node>()
            && node.enabled
            && (matches!(
                node.id.as_str(),
                SEARCH | super::super::help::SEARCH | "help-about" | "help-examples"
            ) || node.id.starts_with("help-run-"))
        {
            if node.focused {
                self.focused = Some(self.ids.len());
            }
            self.ids.push(node.id.clone());
        }
    }

    fn finish(&self) -> operation::Outcome<()> {
        if self.ids.is_empty() {
            return operation::Outcome::None;
        }
        let index = if self.backwards {
            self.focused
                .unwrap_or(0)
                .checked_sub(1)
                .unwrap_or(self.ids.len() - 1)
        } else {
            self.focused.map_or(0, |index| (index + 1) % self.ids.len())
        };
        let Some(id) = self.ids.get(index) else {
            return operation::Outcome::None;
        };
        operation::Outcome::Chain(Box::new(reshiki::accessibility::FocusControl::new(
            id.clone(),
        )))
    }
}
