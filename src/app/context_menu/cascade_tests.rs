//! Drive the real menu widgets, including scrolling, instead of dispatching menu actions.
use super::*;
use iced::advanced::{
    Layout, Shell, layout, mouse,
    renderer::Headless,
    widget::{Id, Operation, Tree, operation::Scrollable},
};
use iced::{Event, Rectangle, Size, Vector};

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    viewport: Rectangle,
}

fn view(app: &App) -> Element<'_, Message> {
    app.with_context_menu(
        iced::widget::mouse_area(
            container(iced::widget::Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(Message::Delete)
        .into(),
    )
}

impl Ui {
    async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            tree: Tree::empty(),
            viewport: Rectangle::with_size(size),
        }
    }

    fn event(&mut self, app: &mut App, event: Event, cursor: mouse::Cursor) {
        let mut messages = Vec::new();
        {
            let mut view = view(app);
            self.tree.diff(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut self.tree,
                &self.renderer,
                &layout::Limits::new(self.viewport.size(), self.viewport.size()),
            );
            let mut shell = Shell::new(&mut messages);
            view.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&node),
                cursor,
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &self.viewport,
            );
            assert_eq!(shell.event_status(), iced::event::Status::Captured);
        }
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::Delete)),
            "Context menu input must not reach the drawing behind it"
        );
        for message in messages {
            // Deliberately do not run asynchronous clipboard or I/O tasks.
            let _ = app.update(message);
        }
    }

    fn hover(&mut self, app: &mut App, point: Point) {
        self.event(
            app,
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            mouse::Cursor::Available(point),
        );
    }

    fn click(&mut self, app: &mut App, point: Point) {
        // No preceding CursorMoved: clicking a submenu must also work.
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            self.event(app, Event::Mouse(event), mouse::Cursor::Available(point));
        }
    }

    fn key(&mut self, app: &mut App, key: Named) {
        self.event(
            app,
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(key),
                modified_key: iced::keyboard::Key::Named(key),
                physical_key: iced::keyboard::key::Physical::Unidentified(
                    iced::keyboard::key::NativeCode::Unidentified,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: iced::keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            mouse::Cursor::Unavailable,
        );
    }

    fn inspect(
        &mut self,
        app: &App,
        level: usize,
        index: usize,
    ) -> (Rectangle, Rectangle, Vec<Rectangle>) {
        struct Find {
            target: Id,
            row: Option<(Rectangle, Rectangle)>,
            viewport: Rectangle,
            translation: Vector,
        }
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
                if id == Some(&self.target) {
                    self.row = Some((
                        Rectangle {
                            x: bounds.x - self.translation.x,
                            y: bounds.y - self.translation.y,
                            ..bounds
                        },
                        self.viewport,
                    ));
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
                self.viewport = bounds;
                self.translation = translation;
            }
        }
        let mut view = view(app);
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        );
        let panels = Layout::new(&node)
            .children()
            .skip(1)
            .map(|panel| panel.bounds())
            .collect();
        let mut find = Find {
            target: cascade::row_id(level, index),
            row: None,
            viewport: self.viewport,
            translation: Vector::ZERO,
        };
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut find,
        );
        let (row, viewport) = find.row.expect("real menu row in widget tree");
        (row, viewport, panels)
    }

    fn row(&mut self, app: &mut App, level: usize, label: &str) -> Point {
        let page = app.context_menu.as_ref().unwrap().page_at(level).unwrap();
        let index = index(app, page, label);
        for _ in 0..40 {
            let (row, viewport, _) = self.inspect(app, level, index);
            if viewport.contains(row.center()) {
                return row.center();
            }
            let point = Point::new(viewport.x + viewport.width - 20., viewport.center_y());
            self.event(
                app,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines {
                        x: 0.,
                        y: if row.y < viewport.y { 3. } else { -3. },
                    },
                }),
                mouse::Cursor::Available(point),
            );
        }
        panic!("Cannot scroll to {label}");
    }
}

fn index(app: &App, page: Page, label: &str) -> usize {
    app.context_entries(page)
        .iter()
        .position(|entry| matches!(entry, Entry::Item { label: actual, .. } if *actual == label))
        .unwrap()
}

fn selected_ring(position: Point) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    app.context_menu = Some(State::new(position, Page::Main));
    app
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn hover_cascades_keep_parent_and_follow_rows_at_both_window_edges() {
    for size in [Size::new(960., 740.), Size::new(600., 430.)] {
        let mut ui = Ui::new(size).await;
        for right in [false, true] {
            let position = if right {
                Point::new(size.width - 15., size.height - 15.)
            } else {
                Point::new(15., 15.)
            };
            let mut app = selected_ring(position);
            let original = app.tab.doc.clone();
            let selection = app.tab.selected.clone();
            for (label, page) in [
                ("Copy as", Page::CopyAs),
                ("Arrange & transform", Page::Align),
                ("Bond appearance", Page::Bonds),
                ("Attachment points", Page::Attachments),
                ("3D tilt", Page::Tilt),
            ] {
                let point = ui.row(&mut app, 0, label);
                ui.hover(&mut app, point);
                let menu = app.context_menu.as_ref().unwrap();
                assert_eq!(menu.page, Page::Main, "Parent remains open");
                assert_eq!(menu.children[0].page, page, "Hover opens {label}");
                let (row, _, panels) = ui.inspect(&app, 0, index(&app, Page::Main, label));
                assert_eq!(panels.len(), 2);
                let parent = panels[0];
                let child = panels[1];
                assert!(child.x >= 5.9 && child.y >= 5.9);
                assert!(child.x + child.width <= size.width - 5.9);
                assert!(child.y + child.height <= size.height - 5.9);
                assert_eq!(
                    child.position(),
                    cascade::child_position(parent, row, child.size(), size)
                );
                if right {
                    assert!(
                        child.x < parent.x,
                        "Child flips to the left at the right edge"
                    );
                } else {
                    assert!(
                        (child.x - (parent.x + parent.width - 1.)).abs() < 0.1,
                        "Child touches the right border"
                    );
                }
                for step in 1..=5 {
                    let x = point.x + (child.center_x() - point.x) * step as f32 / 5.;
                    ui.hover(&mut app, Point::new(x, point.y));
                    assert_eq!(
                        app.context_menu.as_ref().unwrap().children[0].page,
                        page,
                        "Direct pointer travel into {label} keeps both panels open"
                    );
                }
            }
            let point = ui.row(&mut app, 0, "Copy");
            ui.hover(&mut app, point);
            assert!(
                app.context_menu.as_ref().unwrap().children.is_empty(),
                "An ordinary sibling closes the child"
            );
            assert_eq!(app.tab.doc, original);
            assert_eq!(app.tab.selected, selection);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn submenu_clicks_recheck_disabled_commands_and_preserve_selection_and_undo() {
    let mut ui = Ui::new(Size::new(960., 740.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let original = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let point = ui.row(&mut app, 0, "Copy as");
    ui.click(&mut app, point);
    assert_eq!(
        app.context_menu.as_ref().unwrap().children[0].page,
        Page::CopyAs
    );
    let point = ui.row(&mut app, 1, reshiki::clipboard::CopyFormat::Rxn.label());
    ui.click(&mut app, point);
    assert!(
        app.context_menu.is_some(),
        "Disabled RXN keeps the menu open"
    );
    assert!(!app.copy_as_busy);
    assert_eq!(app.tab.doc, original);

    let point = ui.row(&mut app, 0, "Arrange & transform");
    ui.hover(&mut app, point);
    let point = ui.row(&mut app, 1, "Rotate +30°");
    ui.click(&mut app, point);
    assert!(app.context_menu.is_none());
    assert_ne!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selection);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(
        !app.tab.history.can_undo(),
        "One child click makes one undo step"
    );

    app.context_menu = Some(State::new(Point::new(15., 15.), Page::Main));
    ui.event(
        &mut app,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(Point::new(950., 730.)),
    );
    assert!(app.context_menu.is_none());
    assert_eq!(
        app.tab.doc, original,
        "Outside click dismisses without touching the drawing"
    );
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn keyboard_navigation_reveals_rows_and_escape_closes_the_whole_cascade() {
    let mut ui = Ui::new(Size::new(600., 430.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let original = app.tab.doc.clone();
    ui.key(&mut app, Named::End);
    let (level, focused) = app.context_menu.as_ref().unwrap().focused.unwrap();
    let (row, viewport, _) = ui.inspect(&app, level, focused);
    assert!(
        viewport.contains(row.center()),
        "Keyboard focus scrolls the last row into view"
    );
    ui.key(&mut app, Named::Home);
    let (level, focused) = app.context_menu.as_ref().unwrap().focused.unwrap();
    let (row, viewport, _) = ui.inspect(&app, level, focused);
    assert!(
        viewport.contains(row.center()),
        "Home scrolls the first enabled row into view"
    );

    let target = index(&app, Page::Main, "Copy as");
    for _ in 0..app.context_entries(Page::Main).len() {
        if app.context_menu.as_ref().unwrap().focused == Some((0, target)) {
            break;
        }
        ui.key(&mut app, Named::ArrowDown);
    }
    assert_eq!(
        app.context_menu.as_ref().unwrap().focused,
        Some((0, target))
    );
    ui.key(&mut app, Named::ArrowRight);
    assert_eq!(
        app.context_menu.as_ref().unwrap().children[0].page,
        Page::CopyAs
    );
    assert_eq!(app.context_menu.as_ref().unwrap().focused.unwrap().0, 1);
    ui.key(&mut app, Named::ArrowLeft);
    assert!(app.context_menu.as_ref().unwrap().children.is_empty());
    assert_eq!(
        app.context_menu.as_ref().unwrap().focused,
        Some((0, target))
    );
    ui.key(&mut app, Named::Enter);
    assert_eq!(
        app.context_menu.as_ref().unwrap().children[0].page,
        Page::CopyAs
    );
    ui.key(&mut app, Named::Escape);
    assert!(app.context_menu.is_none());
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn touch_uses_finger_position_for_submenu_clicks_and_outside_dismissal() {
    let mut ui = Ui::new(Size::new(960., 740.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let original = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let point = ui.row(&mut app, 0, "Copy as");
    let id = iced::touch::Finger(1);
    for event in [
        iced::touch::Event::FingerPressed {
            id,
            position: point,
        },
        iced::touch::Event::FingerLifted {
            id,
            position: point,
        },
    ] {
        ui.event(&mut app, Event::Touch(event), mouse::Cursor::Unavailable);
    }
    assert_eq!(
        app.context_menu.as_ref().unwrap().children[0].page,
        Page::CopyAs
    );
    ui.event(
        &mut app,
        Event::Touch(iced::touch::Event::FingerPressed {
            id,
            position: Point::new(950., 730.),
        }),
        // A stale mouse position must not override the finger outside the menu.
        mouse::Cursor::Available(point),
    );
    assert!(app.context_menu.is_none());
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selection);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn grabbed_scrollbar_keeps_scrolling_outside_panel_and_releases_there() {
    let mut ui = Ui::new(Size::new(600., 300.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let original = app.tab.doc.clone();
    let target = index(&app, Page::Main, "Copy as");
    let (before, viewport, panels) = ui.inspect(&app, 0, target);
    assert!(
        before.y > viewport.y + viewport.height,
        "Compact menu must overflow"
    );
    // The native scrollbar is 10px wide; its initial thumb begins at the top.
    let grab = Point::new(viewport.x + viewport.width - 5., viewport.y + 5.);
    ui.event(
        &mut app,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(grab),
    );
    let outside = Point::new(
        panels[0].x + panels[0].width + 40.,
        viewport.y + viewport.height - 10.,
    );
    assert!(!panels[0].contains(outside));
    ui.hover(&mut app, outside);
    let (after, _, _) = ui.inspect(&app, 0, target);
    assert!(
        after.y < before.y - 40.,
        "Dragging outside the panel must still move the content"
    );
    assert!(app.context_menu.is_some());
    ui.event(
        &mut app,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(outside),
    );
    ui.hover(&mut app, Point::new(outside.x, viewport.y + 10.));
    let (released, _, _) = ui.inspect(&app, 0, target);
    assert_eq!(
        released.y, after.y,
        "Releasing outside the panel ends the drag"
    );
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}
