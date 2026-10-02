//! Cascading context panels share one event boundary and use laid-out rows as anchors.
use super::{Action, Message, Page, State};
use iced::advanced::Renderer as _;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{
        Id, Operation, Tree,
        operation::{Scrollable, scrollable},
        tree,
    },
};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, keyboard};

const MARGIN: f32 = 6.;

pub(super) fn row_id(level: usize, index: usize) -> Id {
    Id::new(format!("context-menu-{level}-item-{index}"))
}

pub(super) fn scroll_id(level: usize) -> Id {
    Id::new(format!("context-menu-{level}-scroll"))
}

pub(super) struct Panel<'a> {
    pub page: Page,
    pub content: Element<'a, Message>,
    pub items: Vec<usize>,
    pub anchor: Option<usize>,
}

pub(super) struct Cascade<'a> {
    base: Element<'a, Message>,
    panels: Vec<Panel<'a>>,
    menu: &'a State,
}

struct Pages(Vec<Page>);

impl<'a> Cascade<'a> {
    pub fn new(base: Element<'a, Message>, panels: Vec<Panel<'a>>, menu: &'a State) -> Self {
        Self { base, panels, menu }
    }
}

impl Widget<Message, Theme, Renderer> for Cascade<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Pages>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Pages(self.panels.iter().map(|panel| panel.page).collect()))
    }

    fn children(&self) -> Vec<Tree> {
        std::iter::once(&self.base)
            .chain(self.panels.iter().map(|panel| &panel.content))
            .map(Tree::new)
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let previous = tree.state.downcast_mut::<Pages>();
        for (level, panel) in self.panels.iter().enumerate() {
            if previous.0.get(level) != Some(&panel.page)
                && let Some(child) = tree.children.get_mut(level + 1)
            {
                // A sibling submenu starts at its own first row, not at the
                // old submenu's scroll offset or pressed-button state.
                *child = Tree::new(&panel.content);
            }
        }
        previous.0 = self.panels.iter().map(|panel| panel.page).collect();
        let children: Vec<_> = std::iter::once(self.base.as_widget())
            .chain(self.panels.iter().map(|panel| panel.content.as_widget()))
            .collect();
        tree.diff_children(&children);
    }

    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.base.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let base = self
            .base
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let size = base.size();
        let panel_limits = layout::Limits::new(
            Size::ZERO,
            Size::new(
                (size.width - MARGIN * 2.).max(1.),
                (size.height - MARGIN * 2.).max(1.),
            ),
        );
        let mut nodes = vec![base];
        for level in 0..self.panels.len() {
            let anchor = if level > 0 {
                let index = self.panels[level].anchor.unwrap();
                let rows = rows(
                    &mut self.panels[level - 1],
                    level - 1,
                    &mut tree.children[level],
                    Layout::new(&nodes[level]),
                    renderer,
                );
                Some((
                    nodes[level].bounds(),
                    rows.bounds(index).unwrap_or(nodes[level].bounds()),
                ))
            } else {
                None
            };
            let panel = &mut self.panels[level];
            let node = panel.content.as_widget_mut().layout(
                &mut tree.children[level + 1],
                renderer,
                &panel_limits,
            );
            if self.menu.keyboard
                && let Some((focused_level, index)) = self.menu.focused
                && focused_level == level
            {
                reveal(
                    panel,
                    level,
                    index,
                    &mut tree.children[level + 1],
                    Layout::new(&node),
                    renderer,
                );
            }
            let position = match anchor {
                Some((parent, row)) => child_position(parent, row, node.size(), size),
                None => clamp(self.menu.position, node.size(), size),
            };
            nodes.push(node.move_to(position));
        }
        layout::Node::with_children(size, nodes)
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
        let mut layouts = layout.children();
        self.base.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layouts.next().unwrap(),
            mouse::Cursor::Unavailable,
            viewport,
        );
        let layouts: Vec<_> = layouts.collect();
        let hovered = layouts
            .iter()
            .rposition(|layout| cursor.is_over(layout.bounds()));
        for (level, (panel, layout)) in self.panels.iter().zip(layouts).enumerate() {
            renderer.with_layer(*viewport, |renderer| {
                panel.content.as_widget().draw(
                    &tree.children[level + 1],
                    renderer,
                    theme,
                    style,
                    layout,
                    if hovered == Some(level) {
                        cursor
                    } else {
                        mouse::Cursor::Unavailable
                    },
                    viewport,
                );
            });
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut layouts = layout.children();
        self.base.as_widget_mut().operate(
            &mut tree.children[0],
            layouts.next().unwrap(),
            renderer,
            operation,
        );
        for (level, (panel, layout)) in self.panels.iter_mut().zip(layouts).enumerate() {
            panel.content.as_widget_mut().operate(
                &mut tree.children[level + 1],
                layout,
                renderer,
                operation,
            );
        }
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
        if matches!(event, Event::Keyboard(_) | Event::InputMethod(_)) {
            if let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key),
                ..
            }) = event
            {
                shell.publish(Message::ContextMenu(Action::Key(*key)));
                shell.invalidate_layout();
            }
            shell.capture_event();
            return;
        }
        let layouts: Vec<_> = layout.children().collect();
        let hovered = layouts
            .iter()
            .skip(1)
            .rposition(|layout| cursor.is_over(layout.bounds()));
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_)))
            && hovered.is_none()
            && cursor.is_over(layout.bounds())
        {
            shell.publish(Message::ContextMenu(Action::Close));
            shell.capture_event();
            return;
        }
        if let Some(level) = hovered {
            if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. })) {
                let rows = rows(
                    &mut self.panels[level],
                    level,
                    &mut tree.children[level + 1],
                    layouts[level + 1],
                    renderer,
                );
                if let Some(index) = cursor.position().and_then(|point| rows.at(point))
                    && (self.menu.focused != Some((level, index)) || self.menu.keyboard)
                {
                    shell.publish(Message::ContextMenu(Action::Hover(level, index)));
                }
            }
            if matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. }))
                && self.menu.children.len() > level
            {
                shell.publish(Message::ContextMenu(Action::CloseAfter(level)));
            }
        }
        for (level, panel) in self.panels.iter_mut().enumerate() {
            panel.content.as_widget_mut().update(
                &mut tree.children[level + 1],
                event,
                layouts[level + 1],
                if hovered == Some(level) {
                    cursor
                } else {
                    mouse::Cursor::Unavailable
                },
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        if matches!(event, Event::Mouse(_) | Event::Touch(_)) {
            if cursor.is_over(layout.bounds()) {
                shell.capture_event();
            }
        } else {
            self.base.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layouts[0],
                mouse::Cursor::Unavailable,
                renderer,
                clipboard,
                shell,
                viewport,
            );
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
        for (level, (panel, layout)) in self
            .panels
            .iter()
            .zip(layout.children().skip(1))
            .enumerate()
            .rev()
        {
            if cursor.is_over(layout.bounds()) {
                return match panel.content.as_widget().mouse_interaction(
                    &tree.children[level + 1],
                    layout,
                    cursor,
                    viewport,
                    renderer,
                ) {
                    mouse::Interaction::None => mouse::Interaction::Idle,
                    interaction => interaction,
                };
            }
        }
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Idle
        } else {
            mouse::Interaction::None
        }
    }
}

fn clamp(position: Point, size: Size, viewport: Size) -> Point {
    Point::new(
        position
            .x
            .clamp(MARGIN, (viewport.width - size.width - MARGIN).max(MARGIN)),
        position
            .y
            .clamp(MARGIN, (viewport.height - size.height - MARGIN).max(MARGIN)),
    )
}

pub(super) fn child_position(
    parent: Rectangle,
    row: Rectangle,
    size: Size,
    viewport: Size,
) -> Point {
    // Touching borders leave no dead gap while moving from the row into its child.
    let right = parent.x + parent.width - 1.;
    let x = if right + size.width <= viewport.width - MARGIN {
        right
    } else {
        parent.x - size.width + 1.
    };
    clamp(Point::new(x, row.y - 5.), size, viewport)
}

struct Rows {
    targets: Vec<(usize, Id)>,
    found: Vec<(usize, Rectangle)>,
    viewport: Option<Rectangle>,
    translation: Vector,
}

impl Rows {
    fn bounds(&self, index: usize) -> Option<Rectangle> {
        self.found
            .iter()
            .find(|(value, _)| *value == index)
            .map(|(_, bounds)| Rectangle {
                x: bounds.x - self.translation.x,
                y: bounds.y - self.translation.y,
                ..*bounds
            })
    }

    fn at(&self, point: Point) -> Option<usize> {
        if !self.viewport.is_some_and(|bounds| bounds.contains(point)) {
            return None;
        }
        self.found.iter().find_map(|(index, _)| {
            self.bounds(*index)
                .filter(|bounds| bounds.contains(point))
                .map(|_| *index)
        })
    }
}

impl Operation for Rows {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if let Some((index, _)) = self.targets.iter().find(|(_, target)| Some(target) == id) {
            self.found.push((*index, bounds));
        }
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn Scrollable,
    ) {
        self.viewport = Some(bounds);
        self.translation = translation;
    }
}

fn rows(
    panel: &mut Panel<'_>,
    level: usize,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
) -> Rows {
    let mut rows = Rows {
        targets: panel
            .items
            .iter()
            .map(|index| (*index, row_id(level, *index)))
            .collect(),
        found: vec![],
        viewport: None,
        translation: Vector::ZERO,
    };
    panel
        .content
        .as_widget_mut()
        .operate(tree, layout, renderer, &mut rows);
    rows
}

fn reveal(
    panel: &mut Panel<'_>,
    level: usize,
    index: usize,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &Renderer,
) {
    let rows = rows(panel, level, tree, layout, renderer);
    let (Some(row), Some(viewport)) = (rows.bounds(index), rows.viewport) else {
        return;
    };
    let delta = if row.y < viewport.y {
        row.y - viewport.y
    } else if row.y + row.height > viewport.y + viewport.height {
        row.y + row.height - viewport.y - viewport.height
    } else {
        0.
    };
    if delta != 0. {
        let mut scroll = scrollable::scroll_by::<()>(
            scroll_id(level),
            scrollable::AbsoluteOffset { x: 0., y: delta },
        );
        panel
            .content
            .as_widget_mut()
            .operate(tree, layout, renderer, &mut scroll);
    }
}
