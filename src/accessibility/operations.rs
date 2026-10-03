use super::{LiveAction, Node};
use iced::advanced::widget::{
    Id, Operation,
    operation::{Outcome, Scrollable},
};
use iced::{Rectangle, Vector};
use std::{any::Any, collections::BTreeSet};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub nodes: Vec<Node>,
    /// A native adapter must reject this snapshot when this list is nonempty.
    pub duplicate_ids: Vec<String>,
}

#[derive(Clone, Copy)]
struct Context {
    translation: Vector,
    clip: Option<Rectangle>,
}

/// Collects semantics from live widgets, never from a second UI description.
pub struct Collect {
    snapshot: Snapshot,
    ids: BTreeSet<String>,
    context: Context,
    pending_scroll: Option<Context>,
}

impl Collect {
    pub fn new(viewport: Rectangle) -> Self {
        Self {
            snapshot: Snapshot::default(),
            ids: BTreeSet::new(),
            context: Context {
                translation: Vector::ZERO,
                clip: Some(viewport),
            },
            pending_scroll: None,
        }
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}

impl Operation<Snapshot> for Collect {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Snapshot>)) {
        let previous = self.context;
        if let Some(context) = self.pending_scroll.take() {
            self.context = context;
        }
        operate(self);
        self.context = previous;
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn Scrollable,
    ) {
        let bounds = bounds + self.context.translation;
        self.pending_scroll = Some(Context {
            translation: self.context.translation - translation,
            clip: self
                .context
                .clip
                .and_then(|clip| clip.intersection(&bounds)),
        });
    }
    fn custom(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn Any) {
        if let Some(node) = state.downcast_ref::<Node>() {
            if !self.ids.insert(node.id.clone()) {
                self.snapshot.duplicate_ids.push(node.id.clone());
            }
            let mut node = node.clone();
            node.bounds = bounds + self.context.translation;
            node.visible_bounds = self
                .context
                .clip
                .and_then(|clip| clip.intersection(&node.bounds));
            self.snapshot.nodes.push(node);
        }
    }
    fn finish(&self) -> Outcome<Snapshot> {
        Outcome::Some(self.snapshot.clone())
    }
}

/// Resolves an activation against the current widget's current action.
/// A removed, disabled or ambiguously identified control produces no message.
pub struct Activate<Message> {
    target: String,
    matches: usize,
    message: Option<Message>,
}

impl<Message> Activate<Message> {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            matches: 0,
            message: None,
        }
    }
    pub fn message(&self) -> Option<&Message> {
        (self.matches == 1)
            .then_some(self.message.as_ref())
            .flatten()
    }
}

impl<Message: Clone + Send + 'static> Operation<Message> for Activate<Message> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }
    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Any) {
        if let Some(action) = state.downcast_ref::<LiveAction<Message>>()
            && action.id == self.target
        {
            self.matches += 1;
            self.message = action.message.clone();
        }
    }
    fn finish(&self) -> Outcome<Message> {
        self.message().cloned().map_or(Outcome::None, Outcome::Some)
    }
}

/// Edit the current enabled field through the same callback as keyboard input.
pub struct SetValue<Message> {
    target: String,
    value: String,
    matches: usize,
    message: Option<Message>,
}
impl<Message> SetValue<Message> {
    pub fn new(target: impl Into<String>, value: String) -> Self {
        Self {
            target: target.into(),
            value,
            matches: 0,
            message: None,
        }
    }
}
impl<Message: Clone + Send + 'static> Operation<Message> for SetValue<Message> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }
    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Any) {
        if let Some(query) = state.downcast_mut::<super::ValueQuery>()
            && query.id == self.target
            && self.value.len() <= 16384
        {
            query.value = Some(self.value.clone());
        }
        if let Some(action) = state.downcast_ref::<super::LiveValueAction<Message>>()
            && action.id == self.target
        {
            self.matches += 1;
            self.message = action.message.clone();
        }
    }
    fn finish(&self) -> Outcome<Message> {
        if self.matches == 1 {
            self.message.clone().map_or(Outcome::None, Outcome::Some)
        } else {
            Outcome::None
        }
    }
}
