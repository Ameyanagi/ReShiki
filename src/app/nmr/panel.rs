//! The floating panel's height follows its rendered rows, including wrapping.
//! A single body scrolls when the window cannot fit even the first full row.
use super::Message;
use super::palette::{MAX_SIZE, PANEL_PADDING, PREFERRED_ROWS};
use super::view::Placement;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use reshiki::accessibility::Button;
use std::any::Any;

const MAX_CONTENT_HEIGHT: f32 = MAX_SIZE.height - 2. * PANEL_PADDING;
const GAP: f32 = 8.;
// Iced rounds the scroll translation to logical pixels. Reserve this space
// inside the content as well as in the preferred height, so a rounded-down
// end offset cannot clip the final row's fractional-pixel lower edge.
pub(super) const CONTENT_END_INSET: f32 = 2.;
// The panel measures result rows by this accessibility id prefix; the view
// builds every result row id from it. It is a macro so the row id's format
// string keeps the prefix as a literal piece (same capacity estimate and
// allocations as a hand-written literal).
macro_rules! site_id_prefix {
    () => {
        "nmr.site."
    };
}
pub(super) use site_id_prefix;
pub(super) const SITE_ID_PREFIX: &str = site_id_prefix!();

pub(super) fn view<'a>(
    header: impl Into<Element<'a, Message>>,
    body: impl Into<Element<'a, Message>>,
    footer: impl Into<Element<'a, Message>>,
    requested: Option<f32>,
    placement: Placement,
) -> Element<'a, Message> {
    Element::new(Panel {
        children: vec![header.into(), body.into(), footer.into()],
        requested,
        placement,
    })
}

struct Panel<'a> {
    children: Vec<Element<'a, Message>>,
    requested: Option<f32>,
    placement: Placement,
}

#[derive(Default)]
struct Rows(Vec<Rectangle>);
impl Operation for Rows {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        // The height policy reads only the first row and row PREFERRED_ROWS
        // (or the last row when there are fewer), so stop once those are
        // known. A column operates every child in one traversal, so the rows
        // after them sit behind their own boundary (see table_row).
        if self.0.len() < PREFERRED_ROWS {
            operate(self);
        }
    }
    fn custom(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        _: Rectangle,
        state: &mut dyn Any,
    ) {
        if self.0.len() < PREFERRED_ROWS
            && let Some(node) = state.downcast_ref::<reshiki::accessibility::Node>()
            && node.id.starts_with(SITE_ID_PREFIX)
        {
            self.0.push(node.bounds);
        }
    }
}

/// The results table's row `index`. Rows after the first PREFERRED_ROWS ask
/// to be traversed before they operate: the height measurement holds every
/// row it reads by then and declines, so it never builds their accessibility
/// nodes. Every other operation traverses into them as before.
pub(super) fn table_row(index: usize, row: Button<'_, Message>) -> Element<'_, Message> {
    if index < PREFERRED_ROWS {
        row.into()
    } else {
        Element::new(LaterRow(row))
    }
}

/// The row's own tree, layout, drawing and events behind a traverse boundary.
struct LaterRow<'a>(Button<'a, Message>);

impl Widget<Message, Theme, Renderer> for LaterRow<'_> {
    fn tag(&self) -> tree::Tag {
        self.0.tag()
    }
    fn state(&self) -> tree::State {
        self.0.state()
    }
    fn children(&self) -> Vec<Tree> {
        self.0.children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.0.diff(tree);
    }
    fn size(&self) -> Size<Length> {
        self.0.size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.0.size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.0.layout(tree, renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| self.0.operate(tree, layout, renderer, operation));
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
        self.0.update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
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
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.0
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Heights {
    minimum: f32,
    preferred: f32,
}

fn heights(fixed: f32, natural_body: f32, rows: &[Rectangle]) -> Heights {
    let body_height = |count: usize| {
        rows.get(count.saturating_sub(1).min(rows.len().saturating_sub(1)))
            .map_or(natural_body, |row| row.y + row.height + CONTENT_END_INSET)
    };
    Heights {
        minimum: fixed + body_height(1),
        preferred: fixed + body_height(PREFERRED_ROWS),
    }
}

fn floating_height(requested: Option<f32>, heights: Heights, available: f32) -> f32 {
    requested
        .unwrap_or(heights.preferred)
        .max(heights.minimum)
        .min(available.min(MAX_CONTENT_HEIGHT))
}

impl Widget<Message, Theme, Renderer> for Panel<'_> {
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fill,
            if self.placement == Placement::Floating {
                Length::Shrink
            } else {
                Length::Fill
            },
        )
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let ([header, body, footer], [header_tree, body_tree, footer_tree]) =
            (self.children.as_mut_slice(), tree.children.as_mut_slice())
        else {
            return layout::Node::new(Size::ZERO);
        };
        let width = limits.max().width;
        let natural = layout::Limits::new(Size::new(width, 0.), Size::new(width, f32::INFINITY));
        let header_node = header
            .as_widget_mut()
            .layout(header_tree, renderer, &natural);
        let mut body_node = body.as_widget_mut().layout(body_tree, renderer, &natural);
        let footer_node = footer
            .as_widget_mut()
            .layout(footer_tree, renderer, &natural);
        let mut rows = Rows::default();
        body.as_widget_mut()
            .operate(body_tree, Layout::new(&body_node), renderer, &mut rows);
        // These are bounds of the actual result buttons, with limitations and
        // detail lines inside them. No character-count estimate controls fit.
        let fixed = header_node.size().height + footer_node.size().height + 2. * GAP;
        let heights = heights(fixed, body_node.size().height, &rows.0);
        let height = if self.placement == Placement::Floating {
            floating_height(self.requested, heights, limits.max().height)
        } else {
            limits.max().height
        };
        let body_space = (height - fixed).max(0.);
        body_node = body.as_widget_mut().layout(
            body_tree,
            renderer,
            &layout::Limits::new(Size::new(width, 0.), Size::new(width, body_space)),
        );
        let body_y = header_node.size().height + GAP;
        let body_node = body_node.move_to(Point::new(0., body_y));
        let footer_y = (height - footer_node.size().height).max(body_y);
        let footer_node = footer_node.move_to(Point::new(0., footer_y));
        layout::Node::with_children(
            Size::new(width, height),
            vec![header_node, body_node, footer_node],
        )
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
            for ((child, tree), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(tree, layout, renderer, operation);
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
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
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
        for ((child, tree), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
            .map(|((child, tree), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests;
