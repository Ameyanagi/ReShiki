//! Retain the canvas program's geometry order across vector and raster layers.
//! Standard Canvas batches all pictures above all paths within one GPU layer.
use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Tree, tree},
};
use iced::widget::canvas::{self, Program};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use std::rc::Rc;

struct Shared<P>(Rc<P>);
impl<M, P: Program<M>> Program<M> for Shared<P> {
    type State = P::State;
    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        self.0.update(state, event, bounds, cursor)
    }
    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        self.0.draw(state, renderer, theme, bounds, cursor)
    }
    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        self.0.mouse_interaction(state, bounds, cursor)
    }
}
pub struct LayeredCanvas<P: Program<M>, M> {
    program: Rc<P>,
    inner: canvas::Canvas<Shared<P>, M>,
}
pub fn canvas<P: Program<M>, M>(program: P) -> LayeredCanvas<P, M> {
    let program = Rc::new(program);
    LayeredCanvas {
        inner: canvas::Canvas::new(Shared(program.clone())),
        program,
    }
}
impl<P: Program<M>, M> LayeredCanvas<P, M> {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }
}
impl<P: Program<M>, M> Widget<M, Theme, Renderer> for LayeredCanvas<P, M> {
    fn tag(&self) -> tree::Tag {
        self.inner.tag()
    }
    fn state(&self) -> tree::State {
        self.inner.state()
    }
    fn size(&self) -> Size<Length> {
        self.inner.size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.inner.layout(tree, renderer, limits)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.inner.update(
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
        self.inner
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width < 1. || bounds.height < 1. {
            return;
        }
        let Some(visible) = bounds.intersection(viewport) else {
            return;
        };
        let clip = Rectangle {
            x: visible.x - bounds.x,
            y: visible.y - bounds.y,
            width: visible.width,
            height: visible.height,
        };
        let state = tree.state.downcast_ref::<P::State>();
        let layers = self.program.draw(state, renderer, theme, bounds, cursor);
        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            for geometry in layers {
                renderer.with_layer(clip, |renderer| renderer.draw_geometry(geometry));
            }
        });
    }
}
impl<'a, P: Program<M> + 'a, M: 'a> From<LayeredCanvas<P, M>> for Element<'a, M> {
    fn from(canvas: LayeredCanvas<P, M>) -> Self {
        Element::new(canvas)
    }
}

/// Split only around pictures, keeping the ordinary all-vector path inexpensive.
pub struct Frame<'a> {
    renderer: &'a Renderer,
    clip: Rectangle,
    offset: Vector,
    current: canvas::Frame,
    layers: Vec<canvas::Geometry>,
}
impl<'a> Frame<'a> {
    pub fn new(renderer: &'a Renderer, size: Size) -> Self {
        Self::clipped(renderer, Rectangle::with_size(size), Vector::ZERO)
    }
    pub fn clipped(renderer: &'a Renderer, clip: Rectangle, offset: Vector) -> Self {
        let mut current = canvas::Frame::with_bounds(renderer, clip);
        current.translate(offset);
        Self {
            renderer,
            clip,
            offset,
            current,
            layers: vec![],
        }
    }
    pub fn split(&mut self) {
        let mut next = canvas::Frame::with_bounds(self.renderer, self.clip);
        next.translate(self.offset);
        self.layers
            .push(std::mem::replace(&mut self.current, next).into_geometry());
    }
    pub fn finish(mut self) -> Vec<canvas::Geometry> {
        self.split();
        self.layers
    }
}
impl std::ops::Deref for Frame<'_> {
    type Target = canvas::Frame;
    fn deref(&self) -> &Self::Target {
        &self.current
    }
}
impl std::ops::DerefMut for Frame<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.current
    }
}
