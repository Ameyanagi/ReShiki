use super::*;
use iced::advanced::{renderer::Headless, widget::operation::Focusable};
use iced::widget::{Space, canvas, container, mouse_area, opaque, text_input};
use std::{cell::Cell, rc::Rc};

struct CanvasCursor(Rc<Cell<Option<Option<Point>>>>);
impl canvas::Program<Message> for CanvasCursor {
    type State = ();

    fn draw(
        &self,
        _: &(),
        _: &Renderer,
        _: &Theme,
        _: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        self.0.set(Some(cursor.position()));
        vec![]
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::None
        }
    }
}

fn anchored(popup: Element<'_, Message>, drawn: Rc<Cell<Option<Option<Point>>>>) -> Anchored<'_> {
    Anchored::new(
        mouse_area(
            canvas(CanvasCursor(drawn))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(Message::Tick)
        .on_move(|_| Message::Fit)
        .into(),
        popup,
        Id::unique(),
        None,
        Vector::new(1., 0.),
        Camera::default(),
        Guides::default(),
        None,
        true,
    )
}

fn send(
    view: &mut Anchored<'_>,
    tree: &mut Tree,
    node: &layout::Node,
    renderer: &Renderer,
    viewport: &Rectangle,
    event: Event,
    cursor: mouse::Cursor,
) -> Vec<Message> {
    let mut messages = vec![];
    view.update(
        tree,
        &event,
        Layout::new(node),
        cursor,
        renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        viewport,
    );
    messages
}

#[tokio::test]
#[ignore = "Opt-in real widget/renderer check for transparent popup pointer routing"]
async fn popup_hit_policy_preserves_canvas_cursor_clicks_and_opaque_focus() {
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(800., 600.);
    let viewport = Rectangle::with_size(size);
    let theme = Theme::Light;
    for is_opaque in [false, true] {
        let content = container(Space::new()).width(200).height(100);
        let popup = if is_opaque {
            opaque(content)
        } else {
            content.into()
        };
        let drawn = Rc::new(Cell::new(None));
        let mut view = anchored(popup, drawn.clone());
        let mut tree = Tree::new(&view as &dyn Widget<Message, Theme, Renderer>);
        let node = view.layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let popup = Layout::new(&node).children().nth(1).unwrap().bounds();
        for point in [popup.center(), Point::new(16., 16.)] {
            let cursor = mouse::Cursor::Available(point);
            let through = !is_opaque || !popup.contains(point);
            let messages = send(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                &viewport,
                Event::Mouse(mouse::Event::CursorMoved { position: point }),
                cursor,
            );
            assert_eq!(messages.iter().any(|m| matches!(m, Message::Fit)), through);
            let messages = send(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                &viewport,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                cursor,
            );
            assert_eq!(messages.iter().any(|m| matches!(m, Message::Tick)), through);
            assert_eq!(
                view.mouse_interaction(&tree, Layout::new(&node), cursor, &viewport, &renderer),
                if through {
                    mouse::Interaction::Crosshair
                } else {
                    mouse::Interaction::Idle
                },
            );
            drawn.set(None);
            renderer.reset(viewport);
            view.draw(
                &tree,
                &mut renderer,
                &theme,
                &renderer::Style {
                    text_color: iced::Color::BLACK,
                },
                Layout::new(&node),
                cursor,
                &viewport,
            );
            assert_eq!(drawn.get(), Some(through.then_some(point)));
        }
    }

    // A real opaque editor still owns its click and receives field focus.
    let popup = opaque(
        container(
            text_input("Label", "C")
                .id("anchored-test-input")
                .on_input(|_| Message::Tick),
        )
        .center(Length::Fill)
        .width(200)
        .height(100),
    );
    let mut view = anchored(popup, Rc::new(Cell::new(None)));
    let mut tree = Tree::new(&view as &dyn Widget<Message, Theme, Renderer>);
    let node = view.layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    let point = Layout::new(&node)
        .children()
        .nth(1)
        .unwrap()
        .bounds()
        .center();
    let messages = send(
        &mut view,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    assert!(messages.is_empty(), "Popup click must not reach the canvas");
    struct Focused(bool);
    impl Operation for Focused {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
            if id == Some(&Id::from("anchored-test-input")) {
                self.0 = state.is_focused();
            }
        }
    }
    let mut focused = Focused(false);
    view.operate(&mut tree, Layout::new(&node), &renderer, &mut focused);
    assert!(focused.0, "Opaque popup field must retain click-to-focus");
}
