//! Exercise the real widget tree, focus, pointer delivery and rendered frames.
use super::*;
use iced::advanced::{
    Layout, layout, mouse,
    renderer::{self, Headless},
    widget::{Tree, operation},
};
use iced::{Event, Point, Rectangle, Size};

pub(super) struct Ui {
    renderer: iced::Renderer,
    pub(super) tree: Tree,
    pub(super) size: Size,
    pub(super) canvas: Option<Rectangle>,
    pub(super) editor: Option<Rectangle>,
}
impl Ui {
    pub(super) async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            tree: Tree::empty(),
            size,
            canvas: None,
            editor: None,
        }
    }
    pub(super) fn frame(
        &mut self,
        app: &mut App,
        event: Event,
        cursor: mouse::Cursor,
        focus: bool,
    ) -> Vec<u8> {
        let mut messages = Vec::new();
        let theme = app.theme();
        let canvas_size = app.viewport;
        let viewport = Rectangle::with_size(self.size);
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        fn canvas(layout: Layout<'_>, size: Size) -> Option<Rectangle> {
            if layout.bounds().size() == size && layout.children().next().is_none() {
                return Some(layout.bounds());
            }
            layout.children().find_map(|child| canvas(child, size))
        }
        self.canvas = canvas(Layout::new(&node), canvas_size);
        if focus {
            view.as_widget_mut().operate(
                &mut self.tree,
                Layout::new(&node),
                &self.renderer,
                &mut operation::focusable::focus::<()>(iced::advanced::widget::Id::new(
                    "atom-text",
                )),
            );
        }
        view.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            cursor,
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &viewport,
        );
        let mut collect = reshiki::accessibility::Collect::new(viewport);
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut operation::black_box(&mut collect),
        );
        self.editor = collect
            .snapshot()
            .nodes
            .iter()
            .find(|node| node.id == "atom-text")
            .map(|node| node.bounds);
        iced::advanced::Renderer::reset(&mut self.renderer, viewport);
        view.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            cursor,
            &viewport,
        );
        let pixels = Headless::screenshot(
            &mut self.renderer,
            Size::new(self.size.width as u32, self.size.height as u32),
            1.,
            theme.palette().background,
        );
        drop(view);
        for message in messages {
            let _ = app.update(message);
        }
        pixels
    }
    pub(super) fn redraw(
        &mut self,
        app: &mut App,
        now: iced::time::Instant,
        cursor: mouse::Cursor,
        focus: bool,
    ) -> Vec<u8> {
        self.frame(
            app,
            Event::Window(iced::window::Event::RedrawRequested(now)),
            cursor,
            focus,
        )
    }
    pub(super) fn click(&mut self, app: &mut App, point: Point) {
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            self.frame(
                app,
                Event::Mouse(event),
                mouse::Cursor::Available(point),
                false,
            );
        }
    }
}
