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
        .interaction(mouse::Interaction::Pointer)
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

    fn event(&mut self, app: &mut App, event: Event, cursor: mouse::Cursor) -> Vec<Message> {
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
            if !matches!(event, Event::Window(_)) {
                assert_eq!(shell.event_status(), iced::event::Status::Captured);
            }
        }
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::Delete)),
            "Context menu input must not reach the drawing behind it"
        );
        for message in &messages {
            // Deliberately do not run asynchronous clipboard or I/O tasks.
            let _ = app.update(message.clone());
        }
        messages
    }

    fn hover(&mut self, app: &mut App, point: Point) {
        self.event(
            app,
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            mouse::Cursor::Available(point),
        );
    }

    fn redraw(&mut self, app: &mut App, point: Point, at: iced::time::Instant) -> Vec<Message> {
        self.event(
            app,
            Event::Window(iced::window::Event::RedrawRequested(at)),
            mouse::Cursor::Available(point),
        )
    }

    fn rest(&mut self, app: &mut App, point: Point) {
        self.hover(app, point);
        let now = iced::time::Instant::now();
        self.redraw(app, point, now);
        self.redraw(app, point, now + cascade::HOVER_DELAY);
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
        self.key_with_modifiers(
            app,
            iced::keyboard::Key::Named(key),
            iced::keyboard::Modifiers::empty(),
        );
    }

    fn key_with_modifiers(
        &mut self,
        app: &mut App,
        key: iced::keyboard::Key,
        modifiers: iced::keyboard::Modifiers,
    ) -> Vec<Message> {
        self.event(
            app,
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                physical_key: iced::keyboard::key::Physical::Unidentified(
                    iced::keyboard::key::NativeCode::Unidentified,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            }),
            mouse::Cursor::Unavailable,
        )
    }

    fn inspect(
        &mut self,
        app: &App,
        level: usize,
        index: usize,
    ) -> (Rectangle, Rectangle, Vec<Rectangle>) {
        struct Find {
            target: Id,
            pinned: bool,
            panel: Rectangle,
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
                            x: bounds.x - if self.pinned { 0. } else { self.translation.x },
                            y: bounds.y - if self.pinned { 0. } else { self.translation.y },
                            ..bounds
                        },
                        if self.pinned {
                            self.panel
                        } else {
                            self.viewport
                        },
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
        let panels: Vec<_> = Layout::new(&node)
            .children()
            .skip(1)
            .map(|panel| panel.bounds())
            .collect();
        let page = app.context_menu.as_ref().unwrap().page_at(level).unwrap();
        let pinned = page == Page::Main
            && matches!(
                &app.context_entries(page)[index],
                Entry::Item { action: Action::Run(message), .. }
                    if matches!(message.as_ref(), Message::Copy(false) | Message::Delete)
            );
        let mut find = Find {
            target: cascade::row_id(level, index),
            pinned,
            panel: panels[level],
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

    fn row_text_layout(
        &mut self,
        app: &App,
        level: usize,
        index: usize,
    ) -> (Rectangle, Rectangle, Vec<Rectangle>) {
        struct Find {
            row_id: Id,
            scroll_id: Id,
            row: Option<Rectangle>,
            scroll: Option<(Rectangle, Rectangle)>,
        }
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
                if id == Some(&self.row_id) {
                    self.row = Some(bounds);
                }
            }
            fn scrollable(
                &mut self,
                id: Option<&Id>,
                bounds: Rectangle,
                content: Rectangle,
                _: Vector,
                _: &mut dyn Scrollable,
            ) {
                if id == Some(&self.scroll_id) {
                    self.scroll = Some((bounds, content));
                }
            }
        }
        fn find_row<'a>(layout: Layout<'a>, row: Rectangle) -> Option<Layout<'a>> {
            if layout.bounds() == row {
                Some(layout)
            } else {
                layout.children().find_map(|child| find_row(child, row))
            }
        }
        fn text_leaves(layout: Layout<'_>, leaves: &mut Vec<Rectangle>) {
            if layout.children().next().is_none() {
                leaves.push(layout.bounds());
            } else {
                for child in layout.children() {
                    text_leaves(child, leaves);
                }
            }
        }
        let mut view = view(app);
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        );
        let mut find = Find {
            row_id: cascade::row_id(level, index),
            scroll_id: cascade::scroll_id(level),
            row: None,
            scroll: None,
        };
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut find,
        );
        let mut leaves = Vec::new();
        text_leaves(
            find_row(Layout::new(&node), find.row.unwrap()).unwrap(),
            &mut leaves,
        );
        let (viewport, content) = find.scroll.unwrap();
        (viewport, content, leaves)
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
                ui.rest(&mut app, point);
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
    ui.rest(&mut app, point);
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
        "Keyboard focus reaches the pinned last row"
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
async fn command_shortcuts_close_the_menu_and_dispatch_copy_undo_and_redo_once() {
    use iced::keyboard::{Key, Modifiers};

    let mut ui = Ui::new(Size::new(600., 430.)).await;
    let position = Point::new(15., 15.);
    let mut app = selected_ring(position);
    let original = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let messages = ui.key_with_modifiers(&mut app, Key::Character("c".into()), Modifiers::COMMAND);
    assert!(matches!(
        messages.as_slice(),
        [Message::ContextMenu(Action::Run(message))]
            if matches!(message.as_ref(), Message::Copy(false))
    ));
    assert!(app.context_menu.is_none());
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selection);
    assert!(!app.tab.history.can_undo());

    // Copy's asynchronous work is intentionally not executed by this harness.
    // Use a fresh app to exercise drawing history independently of that task.
    let mut app = selected_ring(position);
    let _ = app.update(Message::Transform(Transform::Rotate(30.)));
    let rotated = app.tab.doc.clone();
    assert_ne!(rotated, original);
    assert!(app.tab.history.can_undo());
    app.context_menu = Some(State::new(position, Page::Main));
    let messages = ui.key_with_modifiers(&mut app, Key::Character("z".into()), Modifiers::COMMAND);
    assert!(matches!(
        messages.as_slice(),
        [Message::ContextMenu(Action::Run(message))]
            if matches!(message.as_ref(), Message::Undo)
    ));
    assert!(app.context_menu.is_none());
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.history.can_redo());
    app.context_menu = Some(State::new(position, Page::Main));
    let messages = ui.key_with_modifiers(
        &mut app,
        Key::Character("Z".into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(matches!(
        messages.as_slice(),
        [Message::ContextMenu(Action::Run(message))]
            if matches!(message.as_ref(), Message::Redo)
    ));
    assert!(app.context_menu.is_none());
    assert_eq!(app.tab.doc, rotated);
    assert_eq!(app.tab.selected, selection);
    assert!(!app.tab.history.can_redo());
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn typing_ime_and_unbound_shortcuts_stay_inside_the_menu() {
    use iced::keyboard::{Key, Modifiers};

    let mut ui = Ui::new(Size::new(600., 430.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let original = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    for (character, modifiers) in [
        ("n", Modifiers::empty()),
        ("N", Modifiers::SHIFT),
        ("b", Modifiers::COMMAND),
    ] {
        assert!(
            ui.key_with_modifiers(&mut app, Key::Character(character.into()), modifiers)
                .is_empty()
        );
        assert!(app.context_menu.is_some());
    }
    assert!(
        ui.event(
            &mut app,
            Event::InputMethod(iced::advanced::input_method::Event::Commit("窒素".into())),
            mouse::Cursor::Unavailable,
        )
        .is_empty()
    );
    assert!(app.context_menu.is_some());
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selection);
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
async fn compact_menu_pins_copy_and_delete_with_keyboard_and_accessibility_order() {
    let mut ui = Ui::new(Size::new(600., 1000.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let copy = index(&app, Page::Main, "Copy");
    let delete = index(&app, Page::Main, "Delete");
    let (_, _, panels) = ui.inspect(&app, 0, delete);
    ui.viewport.height = panels[0].height / 2.;
    let (body, _, _) = ui.row_text_layout(&app, 0, copy);
    for index in [copy, delete] {
        let (row, _, panels) = ui.inspect(&app, 0, index);
        assert!(panels[0].contains(row.position()));
        assert!(panels[0].contains(Point::new(row.x + row.width, row.y + row.height)));
        assert!(
            !body.contains(row.center()),
            "Footer rows are outside the scroll area"
        );
    }

    let mut widget = view(&app);
    let node = widget.as_widget_mut().layout(
        &mut ui.tree,
        &ui.renderer,
        &layout::Limits::new(ui.viewport.size(), ui.viewport.size()),
    );
    let mut collect = reshiki::accessibility::Collect::new(ui.viewport);
    widget.as_widget_mut().operate(
        &mut ui.tree,
        Layout::new(&node),
        &ui.renderer,
        &mut iced::advanced::widget::operation::black_box(&mut collect),
    );
    let snapshot = collect.snapshot();
    let names: Vec<_> = snapshot
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(names.iter().filter(|name| **name == "Copy").count(), 1);
    assert_eq!(names.iter().filter(|name| **name == "Delete").count(), 1);
    assert_eq!(&names[names.len() - 2..], ["Copy", "Delete"]);
    drop(widget);

    let first = app
        .context_entries(Page::Main)
        .iter()
        .position(|entry| matches!(entry, Entry::Item { enabled: true, .. }))
        .unwrap();
    for (key, expected) in [
        (Named::Home, first),
        (Named::End, delete),
        (Named::ArrowDown, first),
        (Named::ArrowUp, delete),
        (Named::ArrowUp, copy),
    ] {
        ui.key(&mut app, key);
        assert_eq!(
            app.context_menu.as_ref().unwrap().focused,
            Some((0, expected))
        );
    }
    ui.key(&mut app, Named::Home);
    let properties = index(&app, Page::Main, "Properties…");
    let (before, _, _) = ui.inspect(&app, 0, properties);
    let (pinned, _, _) = ui.inspect(&app, 0, delete);
    ui.event(
        &mut app,
        Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0., y: -30. },
        }),
        mouse::Cursor::Available(body.center()),
    );
    let (after, _, _) = ui.inspect(&app, 0, properties);
    assert!(after.y < before.y, "The body scrolls");
    assert_eq!(
        ui.inspect(&app, 0, delete).0,
        pinned,
        "The footer stays put"
    );
    let original = app.tab.doc.clone();
    ui.click(&mut app, pinned.center());
    assert!(app.context_menu.is_none());
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn submenu_hover_requires_rest_and_pointer_highlight_does_not_wait_for_app_state() {
    let mut ui = Ui::new(Size::new(960., 1000.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let bond = ui.row(&mut app, 0, "Bond appearance");
    let copy_as = ui.row(&mut app, 0, "Copy as");
    let start = iced::time::Instant::now();
    ui.hover(&mut app, bond);
    assert!(ui.redraw(&mut app, bond, start).is_empty());
    ui.hover(&mut app, copy_as);
    let crossed = start + cascade::HOVER_DELAY / 2;
    assert!(ui.redraw(&mut app, copy_as, crossed).is_empty());
    assert!(
        ui.redraw(&mut app, copy_as, start + cascade::HOVER_DELAY)
            .is_empty()
    );
    assert!(
        app.context_menu.as_ref().unwrap().children.is_empty(),
        "Crossed Bond appearance never opens"
    );
    ui.redraw(&mut app, copy_as, crossed + cascade::HOVER_DELAY);
    assert_eq!(
        app.context_menu.as_ref().unwrap().children[0].page,
        Page::CopyAs
    );

    let cdx = index(
        &app,
        Page::CopyAs,
        reshiki::clipboard::CopyFormat::Cdx.label(),
    );
    let pdf = index(
        &app,
        Page::CopyAs,
        reshiki::clipboard::CopyFormat::Pdf.label(),
    );
    let cdx_row = ui.inspect(&app, 1, cdx).0;
    let pdf_row = ui.inspect(&app, 1, pdf).0;
    // Leave the application focused on CDX as if its hover message had just
    // arrived; PDF's pointer highlight must not need another application update.
    app.context_hover(1, cdx, true);
    assert!(
        ui.event(
            &mut app,
            Event::Mouse(mouse::Event::CursorMoved {
                position: pdf_row.center()
            }),
            mouse::Cursor::Available(cdx_row.center()),
        )
        .is_empty(),
        "Use each movement's own position, even in an event batch"
    );
    assert!(
        ui.redraw(&mut app, pdf_row.center(), crossed + cascade::HOVER_DELAY)
            .is_empty()
    );
    assert_eq!(app.context_menu.as_ref().unwrap().focused, Some((1, cdx)));
    let mut widget = view(&app);
    let node = widget.as_widget_mut().layout(
        &mut ui.tree,
        &ui.renderer,
        &layout::Limits::new(ui.viewport.size(), ui.viewport.size()),
    );
    let mut messages = Vec::new();
    widget.as_widget_mut().update(
        &mut ui.tree,
        &Event::Window(iced::window::Event::RedrawRequested(
            crossed + cascade::HOVER_DELAY,
        )),
        Layout::new(&node),
        mouse::Cursor::Available(pdf_row.center()),
        &ui.renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &ui.viewport,
    );
    assert!(messages.is_empty());
    iced::advanced::Renderer::reset(&mut ui.renderer, ui.viewport);
    widget.as_widget().draw(
        &ui.tree,
        &mut ui.renderer,
        &app.theme(),
        &iced::advanced::renderer::Style::default(),
        Layout::new(&node),
        mouse::Cursor::Available(pdf_row.center()),
        &ui.viewport,
    );
    let pixels = Headless::screenshot(&mut ui.renderer, Size::new(960, 1000), 1., Color::WHITE);
    let sample = |row: Rectangle| {
        let offset = ((row.center_y() as usize * 960) + (row.x + 3.) as usize) * 4;
        &pixels[offset..offset + 4]
    };
    assert_eq!(
        sample(cdx_row),
        [255, 255, 255, 255],
        "CDX is no longer drawn active"
    );
    assert_ne!(
        sample(pdf_row),
        sample(cdx_row),
        "PDF is highlighted locally"
    );
    let parent = ui.inspect(&app, 0, index(&app, Page::Main, "Copy as")).0;
    assert_ne!(
        sample(parent),
        sample(cdx_row),
        "Open submenu parents stay highlighted"
    );
    drop(widget);
    let previous = app
        .context_entries(Page::CopyAs)
        .iter()
        .enumerate()
        .take(pdf)
        .rev()
        .find_map(|(index, entry)| {
            matches!(entry, Entry::Item { enabled: true, .. }).then_some(index)
        })
        .unwrap();
    ui.key(&mut app, Named::ArrowUp);
    assert_eq!(
        app.context_menu.as_ref().unwrap().focused,
        Some((1, previous)),
        "Keyboard navigation starts from the locally hovered row"
    );

    let bond = ui.row(&mut app, 0, "Bond appearance");
    ui.hover(&mut app, bond);
    ui.click(&mut app, bond);
    assert_eq!(
        app.context_menu.as_ref().unwrap().children[0].page,
        Page::Bonds,
        "Click opens a submenu before its hover deadline"
    );
}

#[tokio::test]
#[ignore = "Opt-in real renderer/menu input regression"]
async fn grabbed_scrollbar_keeps_scrolling_outside_panel_and_releases_there() {
    let mut ui = Ui::new(Size::new(600., 1000.)).await;
    let mut app = selected_ring(Point::new(15., 15.));
    let original = app.tab.doc.clone();
    let target = index(&app, Page::Main, "Properties…");
    let (_, _, natural_panels) = ui.inspect(&app, 0, target);
    // Properties is the last scrolling row; Copy and Delete stay in the footer.
    let content_height = natural_panels[0].height;
    ui.viewport = Rectangle::with_size(Size::new(600., content_height / 2.));
    let (before, viewport, panels) = ui.inspect(&app, 0, target);
    assert!(
        before.y > viewport.y + viewport.height + 40.,
        "The measured compact menu must overflow by more than the drag assertion: row={before:?}, viewport={viewport:?}"
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

#[tokio::test]
#[ignore = "Opt-in real renderer/menu layout regression"]
async fn scrollable_bond_menu_reserves_gutter_for_text_and_shortcuts() {
    let mut ui = Ui::new(Size::new(960., 500.)).await;
    for size in [Size::new(960., 500.), Size::new(600., 430.)] {
        ui.viewport = Rectangle::with_size(size);
        let mut app = selected_ring(Point::new(size.width - 15., size.height - 15.));
        let point = ui.row(&mut app, 0, "Bond appearance");
        ui.rest(&mut app, point);
        for label in ["Bond in front", "Saturated ↔ Aromatic"] {
            let index = index(&app, Page::Bonds, label);
            let (viewport, content, text) = ui.row_text_layout(&app, 1, index);
            assert!(
                content.height > viewport.height,
                "Exercise a visible scrollbar"
            );
            assert_eq!(
                text.len(),
                2,
                "The native row contains label and rich shortcut text"
            );
            let label_bounds = text[0];
            let shortcut_bounds = text[1];
            let scrollbar_left = viewport.x + viewport.width - SCROLLBAR_WIDTH;
            assert!(
                content.x + content.width <= scrollbar_left - SCROLLBAR_GAP + 0.1,
                "Scrollable content must leave the scrollbar gutter"
            );
            assert!(
                shortcut_bounds.x + shortcut_bounds.width <= scrollbar_left - SCROLLBAR_GAP + 0.1,
                "The complete shortcut must stay clear of the scrollbar"
            );
            assert!(
                label_bounds.x + label_bounds.width + SHORTCUT_GAP <= shortcut_bounds.x + 0.1,
                "Labels and shortcuts must not overlap"
            );
            assert!(
                label_bounds.width + 0.1 >= super::super::workspace::text_width(label, 12.),
                "The complete label must fit on its line"
            );
            let action = match &app.context_entries(Page::Bonds)[index] {
                Entry::Item { action, .. } => action.clone(),
                _ => unreachable!(),
            };
            let keys = shortcut(&action).unwrap();
            let required: f32 = super::super::shortcuts::spans(&keys)
                .iter()
                .map(|span| {
                    span.font.map_or_else(
                        || super::super::workspace::text_width(&span.text, 11.),
                        |font| super::super::workspace::font_width(&span.text, 11., font),
                    )
                })
                .sum();
            assert!(
                shortcut_bounds.width + 0.1 >= required,
                "The shortcut's final glyph must fit"
            );
            let (_, _, panels) = ui.inspect(&app, 1, index);
            assert!(
                panels
                    .iter()
                    .all(|panel| panel.x >= 5.9 && panel.x + panel.width <= size.width - 5.9),
                "The wider menus must still fit the compact canvas"
            );
        }
    }
}

fn canvas_scene(tree: &Tree, app: &App) -> Option<std::rc::Rc<[reshiki::scene::Primitive]>> {
    if let iced::advanced::widget::tree::State::Some(state) = &tree.state
        && let Some(canvas) = state.downcast_ref::<crate::canvas::State>()
    {
        return Some(canvas.scene_cache_identity(&app.tab.doc, &app.tab.selected));
    }
    tree.children
        .iter()
        .find_map(|tree| canvas_scene(tree, app))
}

#[test]
fn actual_app_canvas_scene_survives_menu_open_submenu_and_pdf_dismissal() {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    let document = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let camera = app.tab.camera;
    let mut tree = Tree::new(app.view().as_widget());
    // Keep the Rc alive: allocator address reuse cannot fake continuity.
    let scene = canvas_scene(&tree, &app).expect("actual drawing canvas");
    app.context_menu = Some(State::new(Point::new(100., 100.), Page::Main));
    tree.diff(app.view().as_widget());
    assert!(std::rc::Rc::ptr_eq(
        &scene,
        &canvas_scene(&tree, &app).unwrap()
    ));
    let _ = app.context_action(Action::Open(Page::CopyAs, 120.));
    tree.diff(app.view().as_widget());
    assert!(std::rc::Rc::ptr_eq(
        &scene,
        &canvas_scene(&tree, &app).unwrap()
    ));
    let _ = app.update(Message::ContextMenu(Action::Run(Box::new(
        Message::CopyAs(reshiki::clipboard::CopyFormat::Pdf),
    ))));
    assert!(app.context_menu.is_none());
    tree.diff(app.view().as_widget());
    assert!(std::rc::Rc::ptr_eq(
        &scene,
        &canvas_scene(&tree, &app).unwrap()
    ));
    assert_eq!(app.tab.doc, document);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in real renderer/closed-menu forwarding regression"]
async fn closed_cascade_forwards_real_cursor_mouse_and_widget_operations() {
    let (mut app, _) = App::new();
    app.context_menu = None;
    let mut ui = Ui::new(Size::new(640., 480.)).await;
    let mut view = view(&app);
    ui.tree.diff(view.as_widget());
    let node = view.as_widget_mut().layout(
        &mut ui.tree,
        &ui.renderer,
        &layout::Limits::new(ui.viewport.size(), ui.viewport.size()),
    );
    let cursor = mouse::Cursor::Available(Point::new(200., 200.));
    let mut messages = vec![];
    view.as_widget_mut().update(
        &mut ui.tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Layout::new(&node),
        cursor,
        &ui.renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &ui.viewport,
    );
    assert!(matches!(messages.as_slice(), [Message::Delete]));
    assert_eq!(
        view.as_widget().mouse_interaction(
            &ui.tree,
            Layout::new(&node),
            cursor,
            &ui.viewport,
            &ui.renderer,
        ),
        mouse::Interaction::Pointer,
    );
    // The closed wrapper must expose the base to accessibility/focus operations.
    struct Count(usize);
    impl Operation for Count {
        fn container(&mut self, _: Option<&Id>, _: Rectangle) {
            self.0 += 1;
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let mut count = Count(0);
    view.as_widget_mut()
        .operate(&mut ui.tree, Layout::new(&node), &ui.renderer, &mut count);
    assert!(count.0 > 0);
}

fn input_canvas_view(app: &App) -> Element<'_, Message> {
    let canvas = crate::canvas::MoleculeCanvas::preview(
        &app.tab.doc,
        &app.tab.selected,
        app.tab.camera,
        crate::canvas::PreviewStyles {
            bond_drawing: app.tab.bond_drawing,
            chain_drawing: app.tab.chain_drawing,
            arrow_preset: app.tab.arrow_style,
            arrow_style: &app.tab.arrows.style,
            orbital_phase: app.tab.orbital_phase,
            graphic_style: &app.tab.graphic_style,
            bracket_sides: app.tab.bracket_sides,
        },
    );
    let canvas: Element<'_, crate::canvas::Edit> = iced::widget::canvas(canvas)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    app.with_context_menu(canvas.map(Message::Canvas))
}

fn input_canvas_event(
    ui: &mut Ui,
    app: &mut App,
    event: Event,
    cursor: mouse::Cursor,
) -> Vec<Message> {
    let mut messages = vec![];
    {
        let mut view = input_canvas_view(app);
        ui.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut ui.tree,
            &ui.renderer,
            &layout::Limits::new(ui.viewport.size(), ui.viewport.size()),
        );
        view.as_widget_mut().update(
            &mut ui.tree,
            &event,
            Layout::new(&node),
            cursor,
            &ui.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &ui.viewport,
        );
    }
    for message in &messages {
        if matches!(
            message,
            Message::ContextMenu(_) | Message::Canvas(crate::canvas::Edit::ContextMenu { .. })
        ) {
            let _ = app.update(message.clone());
        }
    }
    messages
}

#[tokio::test]
#[ignore = "Opt-in real canvas/menu retained-input regression"]
async fn menu_boundaries_keep_caches_but_not_stale_pointer_or_released_modifiers() {
    use iced::keyboard::Modifiers;
    for release_shift in [false, true] {
        let (mut app, _) = App::new();
        app.recovered.clear();
        app.tab.busy = false;
        app.tab.camera = Default::default();
        let a = app
            .tab
            .doc
            .add_atom("O", reshiki::document::Point::new(-100., 0.));
        let b = app
            .tab
            .doc
            .add_atom("N", reshiki::document::Point::new(300., 0.));
        app.tab.selected = vec![a];
        let original = app.tab.doc.clone();
        let mut ui = Ui::new(Size::new(800., 600.)).await;
        let pa = Point::new(300., 300.);
        let pb = Point::new(700., 300.); // Outside the menu opened at A.
        let cursor = mouse::Cursor::Available(pa);
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::SHIFT)),
            cursor,
        );
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::CursorMoved { position: pa }),
            cursor,
        );
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            cursor,
        );
        assert!(app.context_menu.is_some());
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
            cursor,
        );
        let scene = canvas_scene(&ui.tree, &app).unwrap();
        if release_shift {
            let messages = input_canvas_event(
                &mut ui,
                &mut app,
                Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::empty())),
                cursor,
            );
            assert!(
                messages.is_empty(),
                "Modifier bookkeeping must not edit/hover the drawing"
            );
        }
        let cursor = mouse::Cursor::Available(pb);
        let messages = input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::CursorMoved { position: pb }),
            cursor,
        );
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::Canvas(_))),
            "An open menu must keep canvas pointer interactions suspended"
        );
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            cursor,
        );
        assert!(app.context_menu.is_none());
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            cursor,
        );
        assert!(std::rc::Rc::ptr_eq(
            &scene,
            &canvas_scene(&ui.tree, &app).unwrap()
        ));
        // No CursorMoved after dismissal: the live click cursor must select B,
        // rather than the retained opening position A.
        input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            cursor,
        );
        let messages = input_canvas_event(
            &mut ui,
            &mut app,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            cursor,
        );
        let selected = messages
            .iter()
            .find_map(|message| match message {
                Message::Canvas(crate::canvas::Edit::Select(ids)) => Some(ids),
                _ => None,
            })
            .expect("The click must select an atom");
        if release_shift {
            assert_eq!(
                selected,
                &[b],
                "Released Shift must not extend the selection"
            );
        } else {
            assert!(
                selected.contains(&a) && selected.contains(&b),
                "Held Shift remains additive"
            );
        }
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }
}
