//! Cascading context panels share one event boundary and use laid-out rows as anchors.
use super::{Action, Message, Page, State};
use iced::advanced::Renderer as _;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{
        Id, Operation, Tree,
        operation::{Scrollable, scrollable},
        tree,
    },
};
use iced::time::Instant;
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, keyboard};
use std::time::Duration;

const MARGIN: f32 = 6.;
pub(super) const HOVER_DELAY: Duration = Duration::from_millis(120);

pub(super) fn row_id(level: usize, index: usize) -> Id {
    Id::from(format!("context-menu-{level}-item-{index}"))
}

pub(super) fn scroll_id(level: usize) -> Id {
    Id::from(format!("context-menu-{level}-scroll"))
}

pub(super) struct Panel<'a> {
    pub page: Page,
    pub content: Element<'a, Message>,
    pub items: Vec<usize>,
    pub pinned: Option<usize>,
    pub submenus: Vec<usize>,
    pub anchor: Option<usize>,
}

pub(super) struct Cascade<'a> {
    base: Element<'a, Message>,
    panels: Vec<Panel<'a>>,
    menu: Option<&'a State>,
}

struct Pages {
    open: bool,
    panels: Vec<Page>,
    hovered: Option<(usize, usize)>,
    pending: Option<Hover>,
}

struct Hover {
    target: (usize, usize),
    deadline: Option<Instant>,
}

fn application_shortcut(event: &Event) -> Option<Message> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modified_key,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    if !modifiers.command() {
        return None;
    }
    super::super::shortcuts::key_message(key, modified_key, *modifiers)
}

impl<'a> Cascade<'a> {
    pub fn new(
        base: Element<'a, Message>,
        panels: Vec<Panel<'a>>,
        menu: Option<&'a State>,
    ) -> Self {
        Self { base, panels, menu }
    }
}

impl Widget<Message, Theme, Renderer> for Cascade<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Pages>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Pages {
            open: self.menu.is_some(),
            panels: self.panels.iter().map(|panel| panel.page).collect(),
            hovered: None,
            pending: None,
        })
    }

    fn children(&self) -> Vec<Tree> {
        std::iter::once(&self.base)
            .chain(self.panels.iter().map(|panel| &panel.content))
            .map(Tree::new)
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let previous = tree.state.downcast_mut::<Pages>();
        let boundary = previous.open != self.menu.is_some();
        let pages: Vec<_> = self.panels.iter().map(|panel| panel.page).collect();
        for (level, panel) in self.panels.iter().enumerate() {
            if previous.panels.get(level) != Some(&panel.page)
                && let Some(child) = tree.children.get_mut(level + 1)
            {
                // A sibling submenu starts at its own first row, not at the
                // old submenu's scroll offset or pressed-button state.
                *child = Tree::new(&panel.content);
            }
        }
        if boundary || previous.panels != pages {
            previous.hovered = None;
            previous.pending = None;
        }
        previous.open = self.menu.is_some();
        previous.panels = pages;
        let children: Vec<_> = std::iter::once(self.base.as_widget())
            .chain(self.panels.iter().map(|panel| panel.content.as_widget()))
            .collect();
        tree.diff_children(&children);
        if boundary && let Some(base) = tree.children.first_mut() {
            crate::canvas::reset_menu_pointer(base);
        }
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
        let Some(base_state) = tree.children.first_mut() else {
            return layout::Node::new(limits.min());
        };
        let base = self
            .base
            .as_widget_mut()
            .layout(base_state, renderer, limits);
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
            let Some(menu) = self.menu else {
                break;
            };
            let anchor = if let Some(parent_level) = level.checked_sub(1) {
                let Some(index) = self.panels.get(level).and_then(|panel| panel.anchor) else {
                    break;
                };
                let Some(parent_node) = nodes.get(level) else {
                    break;
                };
                let Some(parent_panel) = self.panels.get_mut(parent_level) else {
                    break;
                };
                let Some(parent_state) = tree.children.get_mut(level) else {
                    break;
                };
                let rows = rows(
                    parent_panel,
                    parent_level,
                    parent_state,
                    Layout::new(parent_node),
                    renderer,
                );
                Some((
                    parent_node.bounds(),
                    rows.bounds(index).unwrap_or(parent_node.bounds()),
                ))
            } else {
                None
            };
            let Some(panel) = self.panels.get_mut(level) else {
                break;
            };
            let Some(panel_state) = tree.children.get_mut(level + 1) else {
                break;
            };
            // Measure the natural height, then reserve the footer before the
            // scrolling body takes the remaining space in a compact panel.
            let limits = if panel.pinned.is_some() {
                let natural = panel.content.as_widget_mut().layout(
                    panel_state,
                    renderer,
                    &layout::Limits::new(
                        Size::ZERO,
                        Size::new(panel_limits.max().width, f32::INFINITY),
                    )
                    .height(Length::Shrink),
                );
                panel_limits.height(natural.size().height.min(panel_limits.max().height))
            } else {
                panel_limits
            };
            let node = panel
                .content
                .as_widget_mut()
                .layout(panel_state, renderer, &limits);
            if menu.keyboard
                && let Some((focused_level, index)) = menu.focused
                && focused_level == level
            {
                reveal(
                    panel,
                    level,
                    index,
                    panel_state,
                    Layout::new(&node),
                    renderer,
                );
            }
            let position = match anchor {
                Some((parent, row)) => child_position(parent, row, node.size(), size),
                None => clamp(menu.position, node.size(), size),
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
        let Some((base_state, panel_states)) = tree.children.split_first() else {
            return;
        };
        let Some(base_layout) = layouts.next() else {
            return;
        };
        self.base.as_widget().draw(
            base_state,
            renderer,
            theme,
            style,
            base_layout,
            if self.menu.is_some() {
                mouse::Cursor::Unavailable
            } else {
                cursor
            },
            viewport,
        );
        let layouts: Vec<_> = layouts.collect();
        let hovered = layouts
            .iter()
            .rposition(|layout| cursor.is_over(layout.bounds()));
        for (level, ((panel, state), layout)) in self
            .panels
            .iter()
            .zip(panel_states)
            .zip(layouts)
            .enumerate()
        {
            renderer.with_layer(*viewport, |renderer| {
                panel.content.as_widget().draw(
                    state,
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
        let Some((base_state, panel_states)) = tree.children.split_first_mut() else {
            return;
        };
        let Some(base_layout) = layouts.next() else {
            return;
        };
        if self.menu.is_none() {
            self.base
                .as_widget_mut()
                .operate(base_state, base_layout, renderer, operation);
            return;
        }
        operation.custom(
            None,
            layout.bounds(),
            &mut reshiki::accessibility::Foreground,
        );
        for ((panel, state), layout) in self.panels.iter_mut().zip(panel_states).zip(layouts) {
            panel
                .content
                .as_widget_mut()
                .operate(state, layout, renderer, operation);
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
        let Some(menu) = self.menu else {
            if let Some((base_state, base_layout)) =
                tree.children.first_mut().zip(layout.children().next())
            {
                self.base.as_widget_mut().update(
                    base_state,
                    event,
                    base_layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            return;
        };
        if matches!(
            event,
            Event::Mouse(
                mouse::Event::ButtonPressed(_)
                    | mouse::Event::WheelScrolled { .. }
                    | mouse::Event::CursorLeft
            ) | Event::Touch(_)
                | Event::Window(iced::window::Event::Unfocused)
        ) {
            let state = tree.state.downcast_mut::<Pages>();
            state.hovered = None;
            state.pending = None;
        }
        if matches!(event, Event::Keyboard(keyboard::Event::KeyPressed { .. })) {
            tree.state.downcast_mut::<Pages>().pending = None;
        }
        if matches!(event, Event::Keyboard(keyboard::Event::ModifiersChanged(_))) {
            // Keep retained canvas modifiers current while the menu owns keys.
            // This event carries no pointer action; the unavailable cursor also
            // prevents background hit testing by the base widgets.
            if let Some((base_state, base_layout)) =
                tree.children.first_mut().zip(layout.children().next())
            {
                self.base.as_widget_mut().update(
                    base_state,
                    event,
                    base_layout,
                    mouse::Cursor::Unavailable,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            shell.capture_event();
            return;
        }
        if let Some(message) = application_shortcut(event) {
            // Close before running the command, just like selecting its menu
            // row. Keep it captured so neither the base nor the application's
            // shortcut subscription dispatches the same key a second time.
            shell.publish(Message::ContextMenu(Action::Run(Box::new(message))));
            shell.capture_event();
            return;
        }
        if matches!(
            event,
            Event::Keyboard(
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(
                        keyboard::key::Named::Enter | keyboard::key::Named::Space
                    ),
                    ..
                } | keyboard::Event::KeyReleased {
                    key: keyboard::Key::Named(keyboard::key::Named::Space),
                    ..
                }
            )
        ) {
            for ((panel, state), layout) in self
                .panels
                .iter_mut()
                .zip(tree.children.iter_mut().skip(1))
                .zip(layout.children().skip(1))
            {
                panel.content.as_widget_mut().update(
                    state, event, layout, cursor, renderer, clipboard, shell, viewport,
                );
                if shell.is_event_captured() {
                    return;
                }
            }
        }
        if matches!(event, Event::Keyboard(_) | Event::InputMethod(_)) {
            if let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key),
                ..
            }) = event
            {
                // Tab and native Focus change real widget focus without a
                // model message. Start arrow navigation from that live target.
                let hovered = tree.state.downcast_mut::<Pages>().hovered.take();
                let mut focused = None;
                if matches!(
                    key,
                    keyboard::key::Named::ArrowUp
                        | keyboard::key::Named::ArrowDown
                        | keyboard::key::Named::ArrowLeft
                        | keyboard::key::Named::ArrowRight
                        | keyboard::key::Named::Home
                        | keyboard::key::Named::End
                ) {
                    for (level, ((panel, state), layout)) in self
                        .panels
                        .iter_mut()
                        .zip(tree.children.iter_mut().skip(1))
                        .zip(layout.children().skip(1))
                        .enumerate()
                    {
                        let mut probe = FocusedRow::default();
                        panel
                            .content
                            .as_widget_mut()
                            .operate(state, layout, renderer, &mut probe);
                        if let Some(index) = probe.0.filter(|index| panel.items.contains(index)) {
                            focused = Some((level, index));
                        }
                    }
                }
                // Without a live focused control, start from the pointer row.
                let focused = focused.or(hovered.filter(|_| !menu.keyboard));
                shell.publish(Message::ContextMenu(
                    focused.map_or(Action::Key(*key), |(level, index)| {
                        Action::FocusedKey(level, index, *key)
                    }),
                ));
                shell.invalidate_layout();
            }
            shell.capture_event();
            return;
        }
        // Touch events carry their own location even when no mouse cursor is
        // available (or the mouse is still over a different menu panel).
        let cursor = match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                mouse::Cursor::Available(*position)
            }
            Event::Mouse(mouse::Event::CursorLeft) => mouse::Cursor::Unavailable,
            Event::Touch(
                iced::touch::Event::FingerPressed { position, .. }
                | iced::touch::Event::FingerMoved { position, .. }
                | iced::touch::Event::FingerLifted { position, .. }
                | iced::touch::Event::FingerLost { position, .. },
            ) => mouse::Cursor::Available(*position),
            _ => cursor,
        };
        let layouts: Vec<_> = layout.children().collect();
        let hovered = layouts
            .iter()
            .skip(1)
            .rposition(|layout| cursor.is_over(layout.bounds()));
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(_))
                | Event::Touch(iced::touch::Event::FingerPressed { .. })
        ) && hovered.is_none()
            && cursor.is_over(layout.bounds())
        {
            shell.publish(Message::ContextMenu(Action::Close));
            shell.capture_event();
            return;
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::CursorMoved { .. })
                | Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            let target = hovered.and_then(|level| {
                let panel = self.panels.get_mut(level)?;
                let state = tree.children.get_mut(level + 1)?;
                let rows = rows(panel, level, state, *layouts.get(level + 1)?, renderer);
                cursor
                    .position()
                    .and_then(|point| rows.at(point))
                    .map(|index| (level, index))
            });
            let state = tree.state.downcast_mut::<Pages>();
            if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
                && state.hovered != target
            {
                let pointer = menu.keyboard && state.hovered.is_none();
                state.hovered = target;
                state.pending = None;
                if let Some((level, index)) = target {
                    if pointer {
                        shell.publish(Message::ContextMenu(Action::Pointer));
                    }
                    let open = menu
                        .children
                        .get(level)
                        .is_some_and(|child| child.anchor == index);
                    if !open
                        && self
                            .panels
                            .get(level)
                            .is_some_and(|panel| panel.submenus.contains(&index))
                    {
                        state.pending = Some(Hover {
                            target: (level, index),
                            deadline: None,
                        });
                        shell.request_redraw();
                    } else if !open && menu.children.len() > level {
                        shell.publish(Message::ContextMenu(Action::Hover(level, index)));
                    }
                }
            }
            if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
                if state
                    .pending
                    .as_ref()
                    .is_some_and(|hover| Some(hover.target) != target)
                {
                    state.pending = None;
                }
                if let Some(hover) = &mut state.pending {
                    let deadline = *hover.deadline.get_or_insert(*now + HOVER_DELAY);
                    if *now >= deadline {
                        let (level, index) = hover.target;
                        shell.publish(Message::ContextMenu(Action::Hover(level, index)));
                        state.pending = None;
                    } else {
                        shell.request_redraw_at(deadline);
                    }
                }
            }
        }
        if let Some(level) = hovered
            && matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. }))
            && menu.children.len() > level
        {
            shell.publish(Message::ContextMenu(Action::CloseAfter(level)));
        }
        for (level, ((panel, state), panel_layout)) in self
            .panels
            .iter_mut()
            .zip(tree.children.iter_mut().skip(1))
            .zip(layouts.iter().skip(1))
            .enumerate()
        {
            panel.content.as_widget_mut().update(
                state,
                event,
                *panel_layout,
                if hovered == Some(level) {
                    cursor
                } else {
                    // An already grabbed scrollbar still needs its pointer
                    // position after leaving this panel. Levitating suppresses
                    // hover/press handling while preserving drag coordinates.
                    cursor.levitate()
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
        } else if let Some((base_state, base_layout)) =
            tree.children.first_mut().zip(layouts.first())
        {
            self.base.as_widget_mut().update(
                base_state,
                event,
                *base_layout,
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
        if self.menu.is_none() {
            return tree.children.first().zip(layout.children().next()).map_or(
                mouse::Interaction::None,
                |(state, layout)| {
                    self.base
                        .as_widget()
                        .mouse_interaction(state, layout, cursor, viewport, renderer)
                },
            );
        }
        for ((panel, state), layout) in self
            .panels
            .iter()
            .zip(tree.children.iter().skip(1))
            .zip(layout.children().skip(1))
            .rev()
        {
            if cursor.is_over(layout.bounds()) {
                return match panel
                    .content
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
                {
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

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        if self.menu.is_some() {
            return None;
        }
        self.base.as_widget_mut().overlay(
            tree.children.first_mut()?,
            layout.children().next()?,
            renderer,
            viewport,
            translation,
        )
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
    pinned: Option<usize>,
    panel: Rectangle,
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
            .map(|(_, bounds)| {
                if self.pinned.is_some_and(|first| index >= first) {
                    *bounds
                } else {
                    Rectangle {
                        x: bounds.x - self.translation.x,
                        y: bounds.y - self.translation.y,
                        ..*bounds
                    }
                }
            })
    }

    fn at(&self, point: Point) -> Option<usize> {
        self.found.iter().find_map(|(index, _)| {
            let viewport = if self.pinned.is_some_and(|first| *index >= first) {
                Some(self.panel)
            } else {
                self.viewport
            };
            self.bounds(*index)
                .filter(|_| viewport.is_some_and(|bounds| bounds.contains(point)))
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
        pinned: panel.pinned,
        panel: layout.bounds(),
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
    if panel.pinned.is_some_and(|first| index >= first) {
        return;
    }
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

#[derive(Default)]
struct FocusedRow(Option<usize>);
impl Operation for FocusedRow {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn std::any::Any) {
        if let Some(node) = state.downcast_ref::<reshiki::accessibility::Node>()
            && node.focused
        {
            self.0 = node
                .id
                .rsplit_once('-')
                .and_then(|(_, index)| index.parse().ok());
        }
    }
}

#[cfg(test)]
mod tests;
