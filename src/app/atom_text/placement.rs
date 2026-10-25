//! Measure the canvas and popup in the same layout pass before placing the editor.
use super::{Action, Message};
use crate::canvas::{Camera, guides::Guides};
use iced::advanced::Renderer as _;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Id, Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

const MARGIN: f32 = 8.;
const GAP: f32 = 56.;

pub(super) struct Anchored<'a> {
    children: [Element<'a, Message>; 2],
    session: Id,
    target: Option<reshiki::document::Point>,
    away: Vector,
    camera: Camera,
    guides: Guides,
    context: Option<(reshiki::document::Point, reshiki::document::Point)>,
    click_through: bool,
}

struct State {
    session: Id,
    // Keep the opening position, so moving toward the editor cannot move it away.
    pointer: Option<Option<Point>>,
    placed: Option<Placed>,
}

#[derive(Clone, Copy)]
struct Placed {
    target: Point,
    window: Size,
    context: Option<Rectangle>,
    zoom: f32,
    origin: Point,
}

fn clamp(point: Point, popup: Size, window: Size) -> Point {
    Point::new(
        point
            .x
            .clamp(MARGIN, (window.width - popup.width - MARGIN).max(MARGIN)),
        point
            .y
            .clamp(MARGIN, (window.height - popup.height - MARGIN).max(MARGIN)),
    )
}

impl<'a> Anchored<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        base: Element<'a, Message>,
        popup: Element<'a, Message>,
        session: Id,
        target: Option<reshiki::document::Point>,
        away: Vector,
        camera: Camera,
        guides: Guides,
        context: Option<(reshiki::document::Point, reshiki::document::Point)>,
        click_through: bool,
    ) -> Self {
        Self {
            children: [base, popup],
            session,
            target,
            away,
            camera,
            guides,
            context,
            click_through,
        }
    }
}

#[derive(Default)]
struct CanvasBounds(Option<Rectangle>);
impl Operation for CanvasBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::from("atom-text-canvas")) {
            self.0 = Some(bounds);
        }
    }
}

/// Pick a measured, clamped rectangle. Prefer the side opposite the outside bonds,
/// then the nearest alternative that leaves the atom and opening pointer clear.
fn position(
    target: Point,
    away: Vector,
    pointer: Option<Point>,
    context: Option<Rectangle>,
    popup: Size,
    window: Size,
) -> Point {
    let mut candidates = vec![
        Point::new(target.x + GAP, target.y - 50.),
        Point::new(target.x - GAP - popup.width, target.y - 50.),
        Point::new(target.x - popup.width / 2., target.y + GAP),
        Point::new(target.x - popup.width / 2., target.y - GAP - popup.height),
    ];
    if let Some(context) = context {
        candidates.extend([
            Point::new(context.x + context.width + 16., target.y - 50.),
            Point::new(context.x - popup.width - 16., target.y - 50.),
            Point::new(
                target.x - popup.width / 2.,
                context.y + context.height + 16.,
            ),
            Point::new(target.x - popup.width / 2., context.y - popup.height - 16.),
        ]);
    }
    let length = (away.x * away.x + away.y * away.y).sqrt().max(1.);
    candidates
        .into_iter()
        .map(|point| clamp(point, popup, window))
        .min_by(|a, b| {
            let score = |point: Point| {
                let bounds = Rectangle::new(point, popup);
                let protected = Rectangle::new(
                    Point::new(target.x - 30., target.y - 30.),
                    Size::new(60., 60.),
                );
                let target_overlap = bounds
                    .intersection(&protected)
                    .map_or(0., |r| r.width * r.height);
                let pointer_overlap = pointer.map_or(0., |pointer| {
                    let protected = Rectangle::new(
                        Point::new(pointer.x - 12., pointer.y - 12.),
                        Size::new(24., 24.),
                    );
                    bounds
                        .intersection(&protected)
                        .map_or(0., |r| r.width * r.height)
                });
                let center = bounds.center();
                let context_overlap = context
                    .and_then(|context| bounds.intersection(&context))
                    .map_or(0., |r| r.width * r.height);
                let delta = center - target;
                let distance = (delta.x * delta.x + delta.y * delta.y).sqrt().max(1.);
                let direction = (delta.x * away.x + delta.y * away.y) / (distance * length);
                // A control must not open under the pointer. Prefer target visibility
                // over bond direction when the viewport cannot fit the preferred side.
                target_overlap * 1000. + pointer_overlap * 10000. + context_overlap
                    - direction * 100.
                    + distance * 0.01
            };
            score(*a).total_cmp(&score(*b))
        })
        .unwrap_or_else(|| clamp(Point::ORIGIN, popup, window))
}

impl Widget<Message, Theme, Renderer> for Anchored<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            session: self.session.clone(),
            pointer: None,
            placed: None,
        })
    }
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if state.session != self.session {
            state.session = self.session.clone();
            state.pointer = None;
            state.placed = None;
        }
        tree.diff_children(&self.children);
    }
    fn size(&self) -> Size<Length> {
        self.children[0].as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.children[0].as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let [base_tree, popup_tree] = tree.children.as_mut_slice() else {
            return layout::Node::new(limits.min());
        };
        let base = self.children[0]
            .as_widget_mut()
            .layout(base_tree, renderer, limits);
        let size = base.size();
        let mut canvas = CanvasBounds::default();
        self.children[0].as_widget_mut().operate(
            base_tree,
            Layout::new(&base),
            renderer,
            &mut canvas,
        );
        let paper = self
            .guides
            .paper(canvas.0.unwrap_or(Rectangle::with_size(size)));
        let target = self.target.map_or(paper.center(), |target| {
            self.camera.screen(target, paper) + Vector::new(paper.x, paper.y)
        });
        let popup = self.children[1].as_widget_mut().layout(
            popup_tree,
            renderer,
            &layout::Limits::new(
                Size::ZERO,
                Size::new(
                    (size.width - MARGIN * 2.).max(1.),
                    (size.height - MARGIN * 2.).max(1.),
                ),
            ),
        );
        let pointer = tree.state.downcast_ref::<State>().pointer.flatten();
        let context = self
            .context
            .map(|(lo, hi)| {
                let lo = self.camera.screen(lo, paper) + Vector::new(paper.x, paper.y);
                let hi = self.camera.screen(hi, paper) + Vector::new(paper.x, paper.y);
                Rectangle::new(lo, Size::new(hi.x - lo.x, hi.y - lo.y))
            })
            .filter(|bounds| {
                bounds.width < size.width * 0.65 && bounds.height < size.height * 0.75
            });
        let state = tree.state.downcast_mut::<State>();
        let at = state
            .placed
            .filter(|placed| {
                placed.target == target
                    && placed.window == size
                    && placed.context == context
                    && placed.zoom == self.camera.zoom
            })
            .map_or_else(
                || position(target, self.away, pointer, context, popup.size(), size),
                |placed| clamp(placed.origin, popup.size(), size),
            );
        // Helper text, reverse fields and suggestions can change height. Keep
        // the draft in place while it fits, instead of recentering its controls.
        state.placed = Some(Placed {
            target,
            window: size,
            context,
            zoom: self.camera.zoom,
            origin: at,
        });
        layout::Node::with_children(size, vec![base, popup.move_to(at)])
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for (index, ((child, child_tree), child_layout)) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .enumerate()
            {
                if index == 1 || self.click_through {
                    child
                        .as_widget_mut()
                        .operate(child_tree, child_layout, renderer, operation);
                }
            }
        });
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
        let state = tree.state.downcast_mut::<State>();
        if state.pointer.is_none() {
            state.pointer = Some(
                if matches!(
                    event,
                    Event::Window(iced::window::Event::RedrawRequested(_))
                ) {
                    cursor.position()
                } else {
                    // A pointer event aimed at the already laid-out editor is
                    // navigation, not the position that originally opened it.
                    None
                },
            );
            if state.pointer.flatten().is_some() {
                state.placed = None;
                // The first redraw supplies the real opening pointer. Relayout
                // before presenting a control beneath it.
                shell.invalidate_layout();
            }
        }
        let mut layouts = layout.children();
        let (Some(base), Some(popup)) = (layouts.next(), layouts.next()) else {
            return;
        };
        let [base_tree, popup_tree] = tree.children.as_mut_slice() else {
            return;
        };
        self.children[1].as_widget_mut().update(
            popup_tree, event, popup, cursor, renderer, clipboard, shell, viewport,
        );
        if shell.is_event_captured() {
            return;
        }
        if self.click_through || matches!(event, Event::Window(_)) {
            // A mounted popup can be transparent while waiting to appear.
            // Let its hit policy decide whether it covers the canvas cursor.
            let cursor = if self.children[1]
                .as_widget()
                .mouse_interaction(popup_tree, popup, cursor, viewport, renderer)
                != mouse::Interaction::None
            {
                cursor.levitate()
            } else {
                cursor
            };
            self.children[0].as_widget_mut().update(
                base_tree, event, base, cursor, renderer, clipboard, shell, viewport,
            );
        } else if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !cursor.is_over(popup.bounds())
        {
            shell.publish(Message::AtomText(Action::Cancel));
            shell.capture_event();
        }
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
        let popup_interaction = layout.children().nth(1).zip(tree.children.get(1)).map_or(
            mouse::Interaction::None,
            |(popup, popup_tree)| {
                self.children[1]
                    .as_widget()
                    .mouse_interaction(popup_tree, popup, cursor, viewport, renderer)
            },
        );
        let canvas_cursor = if self.click_through && popup_interaction == mouse::Interaction::None {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        for (index, ((child, child_tree), child_layout)) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
        {
            if index == 0 {
                child.as_widget().draw(
                    child_tree,
                    renderer,
                    theme,
                    style,
                    child_layout,
                    canvas_cursor,
                    viewport,
                );
            } else {
                // The canvas batches its primitives; a distinct layer keeps
                // those batches below the popup's background and controls.
                renderer.with_layer(*viewport, |renderer| {
                    child.as_widget().draw(
                        child_tree,
                        renderer,
                        theme,
                        style,
                        child_layout,
                        cursor,
                        viewport,
                    );
                });
            }
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
            .rev()
            .find_map(|(index, ((child, child_tree), layout))| {
                if index == 0 && !self.click_through {
                    return None;
                }
                let interaction = child
                    .as_widget()
                    .mouse_interaction(child_tree, layout, cursor, viewport, renderer);
                (interaction != mouse::Interaction::None).then_some(interaction)
            })
            .unwrap_or_default()
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if self.click_through {
            overlay::from_children(
                &mut self.children,
                tree,
                layout,
                renderer,
                viewport,
                translation,
            )
        } else {
            let popup_tree = tree.children.get_mut(1)?;
            let popup_layout = layout.children().nth(1)?;
            self.children[1].as_widget_mut().overlay(
                popup_tree,
                popup_layout,
                renderer,
                viewport,
                translation,
            )
        }
    }
}

#[cfg(test)]
mod interaction_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_popup_keeps_targets_and_opening_pointer_clear_at_window_edges() {
        for window in [
            Size::new(1040., 680.),
            Size::new(1280., 820.),
            Size::new(1600., 1000.),
        ] {
            for popup in [Size::new(430., 450.), Size::new(430., 600.)] {
                for target in [
                    Point::new(20., 20.),
                    Point::new(window.width - 20., 20.),
                    Point::new(20., window.height - 20.),
                    Point::new(window.width - 20., window.height - 20.),
                    Point::new(window.width / 2., window.height / 2.),
                ] {
                    let bounds = Rectangle::new(
                        position(
                            target,
                            Vector::new(1., 0.),
                            Some(target),
                            None,
                            popup,
                            window,
                        ),
                        popup,
                    );
                    assert!(bounds.x >= MARGIN && bounds.y >= MARGIN);
                    assert!(bounds.x + bounds.width <= window.width - MARGIN);
                    assert!(bounds.y + bounds.height <= window.height - MARGIN);
                    assert!(
                        !bounds.contains(target),
                        "{window:?} {target:?}: {bounds:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn opening_pointer_can_be_away_from_the_atom() {
        let target = Point::new(600., 400.);
        let pointer = Point::new(800., 550.);
        let popup = Size::new(430., 450.);
        let at = position(
            target,
            Vector::new(1., 0.),
            Some(pointer),
            None,
            popup,
            Size::new(1280., 820.),
        );
        let bounds = Rectangle::new(at, popup);
        assert!(!bounds.contains(pointer));
        assert!(!bounds.contains(target));
    }
}
