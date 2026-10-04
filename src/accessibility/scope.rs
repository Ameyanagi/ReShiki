use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{
        Id, Operation, Tree,
        operation::{self, Focusable, Outcome, Scrollable},
        tree,
    },
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, keyboard};

/// Adds keyboard traversal to one active focus scope. A modal must wrap only
/// its active content; the application must mask the background separately.
/// Existing editable widgets participate through their actual Focusable state.
pub fn focus_scope<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Scope(content.into()))
}

struct Scope<'a, Message>(Element<'a, Message>);

#[derive(Clone)]
struct Target {
    id: Option<Id>,
    bounds: Rectangle,
    path: Vec<usize>,
    focused: bool,
}

#[derive(Clone)]
struct Scroll {
    bounds: Rectangle,
    content: Rectangle,
    translation: Vector,
}

#[derive(Clone, Default)]
struct Targets {
    targets: Vec<Target>,
    scrolls: Vec<Scroll>,
    path: Vec<usize>,
    pending: Option<usize>,
    foreground: bool,
}

impl Operation for Targets {
    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn std::any::Any) {
        if state.is::<super::Foreground>() {
            *self = Self {
                foreground: true,
                ..Self::default()
            };
        }
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let nested = self.pending.take();
        if let Some(index) = nested {
            self.path.push(index);
        }
        operate(self);
        if nested.is_some() {
            self.path.pop();
        }
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _: &mut dyn Scrollable,
    ) {
        self.pending = Some(self.scrolls.len());
        self.scrolls.push(Scroll {
            bounds,
            content,
            translation,
        });
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        self.targets.push(Target {
            id: id.cloned(),
            bounds,
            path: self.path.clone(),
            focused: state.is_focused(),
        });
    }
}

impl Targets {
    fn next(&self, backwards: bool) -> Option<usize> {
        let count = self.targets.len();
        if count == 0 {
            return None;
        }
        let focused = self.targets.iter().position(|target| target.focused);
        Some(match (focused, backwards) {
            (Some(0), true) | (None, true) => count - 1,
            (Some(index), true) => index - 1,
            (Some(index), false) => (index + 1) % count,
            (None, false) => 0,
        })
    }

    /// Reveal from the innermost scroll viewport outward. Adjusted inner
    /// translations are included when calculating the next ancestor's offset.
    fn reveal(&mut self, target: usize) {
        let Some(target) = self.targets.get(target).cloned() else {
            return;
        };
        let mut translation = Vector::ZERO;
        for index in target.path.iter().rev() {
            let Some(scroll) = self.scrolls.get_mut(*index) else {
                continue;
            };
            let bounds = target.bounds - translation - scroll.translation;
            let dx = reveal_delta(bounds.x, bounds.width, scroll.bounds.x, scroll.bounds.width);
            let dy = reveal_delta(
                bounds.y,
                bounds.height,
                scroll.bounds.y,
                scroll.bounds.height,
            );
            scroll.translation = Vector::new(
                (scroll.translation.x + dx)
                    .clamp(0., (scroll.content.width - scroll.bounds.width).max(0.)),
                (scroll.translation.y + dy)
                    .clamp(0., (scroll.content.height - scroll.bounds.height).max(0.)),
            );
            translation += scroll.translation;
        }
    }
}

fn reveal_delta(position: f32, length: f32, start: f32, extent: f32) -> f32 {
    if position < start {
        position - start
    } else if position + length > start + extent {
        (position + length - start - extent).min(position - start)
    } else {
        0.
    }
}

struct Focus {
    target: usize,
    wait_foreground: bool,
    change_focus: bool,
    current: usize,
    scroll: usize,
    offsets: Vec<Vector>,
}

impl Operation for Focus {
    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn std::any::Any) {
        if state.is::<super::Foreground>() {
            self.wait_foreground = false;
            self.current = 0;
            self.scroll = 0;
        }
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if self.wait_foreground {
            return;
        }
        if self.change_focus {
            if self.current == self.target {
                state.focus();
            } else {
                state.unfocus();
            }
        }
        self.current += 1;
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        _: Rectangle,
        _: Rectangle,
        _: Vector,
        state: &mut dyn Scrollable,
    ) {
        if self.wait_foreground {
            return;
        }
        if let Some(offset) = self.offsets.get(self.scroll) {
            state.scroll_to(operation::scrollable::AbsoluteOffset {
                x: Some(offset.x),
                y: Some(offset.y),
            });
        }
        self.scroll += 1;
    }
}

fn traverse_key(event: &Event, mut operate: impl FnMut(&mut dyn Operation)) -> bool {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(keyboard::key::Named::Tab),
        modifiers,
        ..
    }) = event
    else {
        return false;
    };
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return false;
    }
    let mut targets = Targets::default();
    operate(&mut targets);
    let Some(target) = targets.next(modifiers.shift()) else {
        // Tooltips and standard menu overlays have no focusable descendants.
        // Let their own handling and then the base widget see the key.
        return false;
    };
    targets.reveal(target);
    operate(&mut Focus {
        target,
        wait_foreground: targets.foreground,
        change_focus: true,
        current: 0,
        scroll: 0,
        offsets: targets
            .scrolls
            .into_iter()
            .map(|scroll| scroll.translation)
            .collect(),
    });
    true
}

/// Resolve a current control, reveal it and change the actual widget focus.
/// A missing or duplicate ID does nothing. `reveal` preserves current focus.
pub struct FocusControl {
    target: Id,
    change_focus: bool,
    targets: Targets,
}
impl FocusControl {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: Id::from(target.into()),
            change_focus: true,
            targets: Targets::default(),
        }
    }
    pub fn reveal(target: impl Into<String>) -> Self {
        Self {
            change_focus: false,
            ..Self::new(target)
        }
    }
}
impl Operation for FocusControl {
    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn std::any::Any) {
        self.targets.custom(id, bounds, state);
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let nested = self.targets.pending.take();
        if let Some(index) = nested {
            self.targets.path.push(index);
        }
        operate(self);
        if nested.is_some() {
            self.targets.path.pop();
        }
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        self.targets.focusable(id, bounds, state);
    }
    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        self.targets
            .scrollable(id, bounds, content, translation, state);
    }
    fn finish(&self) -> Outcome<()> {
        let mut matches = self
            .targets
            .targets
            .iter()
            .enumerate()
            .filter(|(_, target)| target.id.as_ref() == Some(&self.target));
        let Some((target, _)) = matches.next() else {
            return Outcome::None;
        };
        if matches.next().is_some() {
            return Outcome::None;
        }
        let mut targets = self.targets.clone();
        targets.reveal(target);
        Outcome::Chain(Box::new(Focus {
            target,
            wait_foreground: targets.foreground,
            change_focus: self.change_focus,
            current: 0,
            scroll: 0,
            offsets: targets
                .scrolls
                .into_iter()
                .map(|scroll| scroll.translation)
                .collect(),
        }))
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Scope<'_, Message> {
    fn tag(&self) -> tree::Tag {
        self.0.as_widget().tag()
    }
    fn state(&self) -> tree::State {
        self.0.as_widget().state()
    }
    fn children(&self) -> Vec<Tree> {
        self.0.as_widget().children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.0.as_widget().diff(tree);
    }
    fn size(&self) -> Size<Length> {
        self.0.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.0.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.0.as_widget_mut().layout(tree, renderer, limits)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.0
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.0
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if traverse_key(event, |operation| {
            self.0
                .as_widget_mut()
                .operate(tree, layout, renderer, operation)
        }) {
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        self.0.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.0
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(|overlay| overlay::Element::new(Box::new(FocusOverlay(overlay))))
    }
}

struct FocusOverlay<'a, Message>(overlay::Element<'a, Message, Theme, Renderer>);

impl<Message> overlay::Overlay<Message, Theme, Renderer> for FocusOverlay<'_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.0.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.0
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        let mut targets = Targets::default();
        self.0
            .as_overlay_mut()
            .operate(layout, renderer, &mut targets);
        if !targets.targets.is_empty() {
            operation.custom(None, layout.bounds(), &mut super::Foreground);
        }
        self.0.as_overlay_mut().operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        if traverse_key(event, |operation| {
            self.0.as_overlay_mut().operate(layout, renderer, operation)
        }) {
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        self.0
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'b>(
        &'b mut self,
        layout: Layout<'b>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.0
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|overlay| overlay::Element::new(Box::new(FocusOverlay(overlay))))
    }
    fn index(&self) -> f32 {
        self.0.as_overlay().index()
    }
}
