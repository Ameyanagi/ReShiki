//! Real widget dispatch, followed by the same ignored-key routing as the app.
use super::{App, InspectorTab, Message, assistant, shortcuts, updates};
use iced::advanced::{
    Layout, Shell, layout, mouse,
    renderer::Headless,
    widget::{
        Id, Operation, Tree,
        operation::{Focusable, TextInput},
    },
};
use iced::keyboard::{
    self, Key, Modifiers,
    key::{Code, Named, Physical},
};
use iced::{Event, Rectangle, Size};

mod review_close;

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    viewport: Rectangle,
}

impl Ui {
    async fn new() -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            tree: Tree::empty(),
            viewport: Rectangle::with_size(Size::new(1280., 1000.)),
        }
    }

    fn event(
        &mut self,
        app: &App,
        event: Event,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Vec<Message>) {
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        );
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        // An open popover sees the event first, as in the app.
        if let Some(overlay) = view.as_widget_mut().overlay(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &self.viewport,
            iced::Vector::ZERO,
        ) {
            let mut nested = iced::advanced::overlay::Nested::new(overlay);
            let node = nested.layout(&self.renderer, self.viewport.size());
            nested.update(
                &event,
                Layout::new(&node),
                cursor,
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
            );
        }
        if shell.event_status() == iced::event::Status::Ignored {
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
        }
        let status = shell.event_status();
        if status == iced::event::Status::Ignored
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            }) = &event
            && let Some(message) = shortcuts::key_message(key, modified_key, *modifiers)
        {
            messages.push(message);
        }
        (status, messages)
    }

    fn inspect(&mut self, app: &App, operation: &mut dyn Operation) {
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            operation,
        );
        // Open popovers, as the app's accessibility tree sees them.
        if let Some(overlay) = view.as_widget_mut().overlay(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &self.viewport,
            iced::Vector::ZERO,
        ) {
            let mut nested = iced::advanced::overlay::Nested::new(overlay);
            let node = nested.layout(&self.renderer, self.viewport.size());
            nested.operate(Layout::new(&node), &self.renderer, operation);
        }
    }

    fn input(&mut self, app: &App, target: &'static str) -> (Rectangle, bool) {
        struct Find(Id, Option<(Rectangle, bool)>);
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
                if id == Some(&self.0) {
                    self.1 = Some((bounds, state.is_focused()));
                }
            }
        }
        let mut find = Find(Id::new(target), None);
        self.inspect(app, &mut find);
        find.1.expect("input in the real app view")
    }

    fn input_value(&mut self, app: &App, target: &'static str) -> String {
        struct Find(Id, Option<String>);
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text_input(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn TextInput) {
                if id == Some(&self.0) {
                    self.1 = Some(state.text().into());
                }
            }
        }
        let mut find = Find(Id::new(target), None);
        self.inspect(app, &mut find);
        find.1.expect("input value in the real app view")
    }

    fn semantics(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
        let mut collect = reshiki::accessibility::Collect::new(self.viewport);
        self.inspect(
            app,
            &mut iced::advanced::widget::operation::black_box(&mut collect),
        );
        collect.snapshot().clone()
    }

    fn text(&mut self, app: &App, target: &'static str) -> Rectangle {
        struct Find(&'static str, Option<Rectangle>);
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
                if text == self.0 {
                    self.1 = Some(bounds);
                }
            }
        }
        let mut find = Find(target, None);
        self.inspect(app, &mut find);
        find.1.expect("visible dialog control")
    }

    fn input_text(&mut self, app: &App, target: &str) -> (Rectangle, bool) {
        struct Find<'a> {
            target: &'a str,
            bounds: Option<Rectangle>,
            matching: bool,
            focused: bool,
        }
        impl Operation for Find<'_> {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text_input(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
                self.matching = state.text() == self.target;
                if self.matching {
                    self.bounds = Some(bounds);
                }
            }
            fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
                if self.matching {
                    self.focused |= state.is_focused();
                }
                self.matching = false;
            }
        }
        let mut find = Find {
            target,
            bounds: None,
            matching: false,
            focused: false,
        };
        self.inspect(app, &mut find);
        (
            find.bounds.expect("style input in the real app view"),
            find.focused,
        )
    }

    fn click(&mut self, app: &mut App, point: iced::Point) {
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            let (_, messages) =
                self.event(app, Event::Mouse(event), mouse::Cursor::Available(point));
            apply(app, messages);
        }
    }

    async fn key(
        &mut self,
        app: &mut App,
        key: Key,
        code: Code,
        modifiers: Modifiers,
        text: Option<&str>,
    ) {
        let (_, messages) = self.event(
            app,
            press(key, code, modifiers, text),
            mouse::Cursor::Unavailable,
        );
        for message in messages {
            let task = app.update(message);
            self.run_task(app, task).await;
        }
    }

    async fn run_task(&mut self, app: &mut App, task: iced::Task<Message>) {
        use iced::advanced::widget::operation::Outcome;
        use iced::futures::StreamExt;

        let mut tasks = std::collections::VecDeque::from([task]);
        while let Some(task) = tasks.pop_front() {
            let Some(mut stream) = iced_runtime::task::into_stream(task) else {
                continue;
            };
            while let Some(action) = stream.next().await {
                match action {
                    iced_runtime::Action::Output(message) => tasks.push_back(app.update(message)),
                    iced_runtime::Action::Widget(operation) => {
                        let mut current = Some(operation);
                        while let Some(mut operation) = current.take() {
                            self.inspect(app, operation.as_mut());
                            if let Outcome::Chain(next) = operation.finish() {
                                current = Some(next);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn assert_unfocused(&mut self, app: &App) {
        #[derive(Default)]
        struct Focus(Vec<Option<Id>>);
        impl Operation for Focus {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
                if state.is_focused() {
                    self.0.push(id.cloned());
                }
            }
        }
        let mut focus = Focus::default();
        self.inspect(app, &mut focus);
        assert!(focus.0.is_empty(), "controls still focused: {:?}", focus.0);
    }
}

fn apply(app: &mut App, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

fn press(key: Key, code: Code, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

fn assistant_app() -> App {
    let (mut app, _) = App::new();
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Assistant;
    let _ = app.update(Message::Assistant(assistant::Action::Input(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
            "Draw ferrocene".to_owned().into(),
        )),
    )));
    app
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn tabs_style_buttons_and_canvas_theme_are_named_controls() {
    use reshiki::accessibility::Role;
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    let first = app.tab.id.0;
    app.add_tab();
    app.tab.path = Some("aldol.rsk".into());
    let second = app.tab.id.0;
    let snapshot = ui.semantics(&app);
    assert!(snapshot.duplicate_ids.is_empty());
    let node = |id: String| {
        (snapshot.nodes.iter())
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("{id} is in the accessibility tree"))
    };
    assert_eq!(node(format!("document-tab-{first}")).name, "Untitled tab");
    assert_eq!(
        node(format!("document-tab-{second}")).name,
        "aldol.rsk tab, selected"
    );
    assert_eq!(
        node(format!("document-tab-close-{second}")).name,
        "Close aldol.rsk"
    );
    assert_eq!(node("document-tab-new".into()).name, "New tab");
    let properties = node("inspector-tab-Properties".into());
    assert_eq!(
        (properties.name.as_str(), properties.checked),
        ("Properties tab", Some(true))
    );
    let export = node("inspector-tab-Export".into());
    assert_eq!(
        (export.name.as_str(), export.checked),
        ("Export tab", Some(false))
    );
    let bold = node("text-style-bold".into());
    assert_eq!((bold.role, bold.checked), (Role::ToggleButton, Some(false)));
    assert_eq!(node("text-style-size".into()).role, Role::TextInput);
    let theme = node("canvas-theme".into());
    assert_eq!(
        (theme.name.as_str(), theme.checked),
        ("Dark canvas", Some(false))
    );
    for (id, name) in [
        ("text-style-font", "Font"),
        ("journal-preset", "Journal preset"),
        ("color-theme", "Color theme"),
    ] {
        let select = node(id.into());
        assert_eq!((select.name.as_str(), select.role), (name, Role::ComboBox));
        assert!(select.value.as_ref().is_some_and(|v| !v.is_empty()), "{id}");
    }
}

/// The journal preset and color theme choose options from the keyboard and
/// by name from assistive technology; the font list by name.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn style_lists_choose_options_by_keyboard_and_by_name() {
    use reshiki::document_styles::Preset;
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let value = |ui: &mut Ui, app: &App, id: &str| {
        (ui.semantics(app).nodes.into_iter())
            .find(|node| node.id == id)
            .and_then(|node| node.value)
            .unwrap()
    };
    let first = Preset::ALL[0].to_string();
    let second = Preset::ALL[1].to_string();
    let message = |ui: &mut Ui, app: &App, id: &str, name: &str| {
        let mut set = reshiki::accessibility::SetValue::<Message>::new(id, name.into());
        ui.inspect(
            app,
            &mut iced::advanced::widget::operation::black_box(&mut set),
        );
        match iced::advanced::widget::Operation::finish(&set) {
            iced::advanced::widget::operation::Outcome::Some(message) => Some(message),
            _ => None,
        }
    };
    // By name: an option name chooses it; any other name does nothing.
    let chosen = message(&mut ui, &app, "journal-preset", &first).expect("a preset");
    apply(&mut app, vec![chosen]);
    assert_eq!(value(&mut ui, &app, "journal-preset"), first);
    assert!(message(&mut ui, &app, "journal-preset", "Manage styles…").is_none());
    assert!(message(&mut ui, &app, "color-theme", "Manage themes…").is_none());
    let family = reshiki::style::font_families()[0];
    assert!(matches!(
        message(&mut ui, &app, "text-style-font", family),
        Some(Message::TextStyle(reshiki::typography::StyleChange::Family(f))) if f == family
    ));

    // Keyboard: ↓ and ↑ choose the next and previous preset.
    ui.focus_control(&mut app, "journal-preset").await;
    assert_eq!(ui.focused(&app), vec!["journal-preset".to_owned()]);
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::ArrowDown),
            Code::ArrowDown,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    apply(&mut app, messages);
    assert_eq!(value(&mut ui, &app, "journal-preset"), second);
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::ArrowUp),
            Code::ArrowUp,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    apply(&mut app, messages);
    assert_eq!(value(&mut ui, &app, "journal-preset"), first);

    // A click on the color theme moves focus to it.
    let theme = ui.bounds(&app, "color-theme").center();
    ui.drag(&mut app, theme, theme).await;
    assert_eq!(ui.focused(&app), vec!["color-theme".to_owned()]);
}

/// Tab reaches the font list's search field, which keeps typing and ↑ ↓
/// for the open list; choosing a font leaves no control focused.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn tab_reaches_the_font_search_field_and_arrows_move_its_list() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    ui.focus_control(&mut app, "text-style-size").await;
    ui.key(
        &mut app,
        Key::Named(Named::Tab),
        Code::Tab,
        Modifiers::SHIFT,
        None,
    )
    .await;
    assert_eq!(ui.focused(&app), vec!["text-style-font".to_owned()]);

    ui.key(
        &mut app,
        Key::Character("a".into()),
        Code::KeyA,
        Modifiers::empty(),
        Some("a"),
    )
    .await;
    assert_eq!(ui.input_value(&app, "text-style-font"), "a");
    ui.key(
        &mut app,
        Key::Named(Named::Backspace),
        Code::Backspace,
        Modifiers::empty(),
        None,
    )
    .await;

    // ↓ moves the open list's highlight; Enter chooses the highlighted font.
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::ArrowDown),
            Code::ArrowDown,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(messages.is_empty(), "{messages:?}");
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    let second = reshiki::style::font_families()[1];
    assert!(
        matches!(
            messages.as_slice(),
            [Message::TextStyle(reshiki::typography::StyleChange::Family(f))] if f == second
        ),
        "{messages:?}"
    );
    apply(&mut app, messages);
    ui.assert_unfocused(&app);
}

impl Ui {
    fn bounds(&mut self, app: &App, id: &str) -> Rectangle {
        (self.semantics(app).nodes.into_iter())
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("{id} is in the accessibility tree"))
            .bounds
    }

    /// Focus a control as a native Focus request does.
    async fn focus_control(&mut self, app: &mut App, id: &str) {
        let task = iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(
            id.to_owned(),
        ))
        .discard();
        self.run_task(app, task).await;
    }

    /// The focused controls, after checking the native tree accepts them.
    fn focused(&mut self, app: &App) -> Vec<String> {
        let snapshot = self.semantics(app);
        reshiki::accessibility::tree::NativeTree::default()
            .update(&snapshot, "ReShiki", self.viewport, 1.)
            .expect("the native tree converts");
        (snapshot.nodes.into_iter())
            .filter(|node| node.focused)
            .map(|node| node.id)
            .collect()
    }

    /// Press at `from`, move to `to`, release there; messages and tasks run.
    async fn drag(&mut self, app: &mut App, from: iced::Point, to: iced::Point) -> Vec<Message> {
        let mut all = Vec::new();
        for (event, at) in [
            (mouse::Event::CursorMoved { position: from }, from),
            (mouse::Event::ButtonPressed(mouse::Button::Left), from),
            (mouse::Event::CursorMoved { position: to }, to),
            (mouse::Event::ButtonReleased(mouse::Button::Left), to),
        ] {
            let (_, messages) = self.event(app, Event::Mouse(event), mouse::Cursor::Available(at));
            for message in messages {
                all.push(message.clone());
                let task = app.update(message);
                self.run_task(app, task).await;
            }
        }
        all
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn a_tab_and_its_close_button_never_hold_focus_together() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    let behind = app.tab.id.0;
    app.add_tab();
    let front = app.tab.id.0;
    let away = iced::Point::new(640., 500.);
    for id in [front, behind] {
        let (tab, close) = (
            format!("document-tab-{id}"),
            format!("document-tab-close-{id}"),
        );
        // Focus the tab itself first, so a stale focus would show: click
        // the front tab, and reach the tab behind from the keyboard.
        if id == front {
            let label = ui.bounds(&app, &tab);
            let label = iced::Point::new(label.x + 12., label.center_y());
            let selected = ui.drag(&mut app, label, label).await;
            assert!(
                matches!(
                    selected.as_slice(),
                    [Message::Tabs(super::tabs::Action::Select(_))]
                ),
                "{selected:?}"
            );
        } else {
            ui.focus_control(&mut app, &tab).await;
        }
        assert_eq!(ui.focused(&app), vec![tab.clone()]);
        // Press ×, drag off it and release: nothing closes, only × has focus.
        let x = ui.bounds(&app, &close).center();
        let dragged = ui.drag(&mut app, x, away).await;
        assert!(
            !dragged.iter().any(|m| matches!(m, Message::Tabs(_))),
            "{id}: {dragged:?}"
        );
        assert_eq!(ui.focused(&app), vec![close], "{id}");
        assert_eq!(app.tabs.background.len(), 1);
        assert_eq!(app.tab.id.0, front);
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn tab_focus_survives_bringing_the_tab_to_the_front() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    let behind = app.tab.id.0;
    app.add_tab();
    let tab = format!("document-tab-{behind}");
    ui.focus_control(&mut app, &tab).await;
    assert_eq!(ui.focused(&app), vec![tab.clone()]);
    ui.key(
        &mut app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::empty(),
        None,
    )
    .await;
    assert_eq!(app.tab.id.0, behind, "Enter brings the tab to the front");
    assert_eq!(ui.focused(&app), vec![tab]);
    let (status, _) = ui.event(
        &app,
        press(Key::Named(Named::Tab), Code::Tab, Modifiers::empty(), None),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert_eq!(
        ui.focused(&app),
        vec![format!("document-tab-close-{behind}")],
        "Tab continues from the tab, not from the window's first control"
    );
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn help_runs_a_clicked_row_with_enter() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let task = app.update(Message::ToggleHelp);
    ui.run_task(&mut app, task).await;
    let task = app.update(Message::Help(super::help::Action::Query(
        "select all".into(),
    )));
    ui.run_task(&mut app, task).await;
    let index = (super::help::entries().iter())
        .position(|entry| entry.name == "Select all")
        .unwrap();
    let row = ui.bounds(&app, &format!("help-row-{index}")).center();
    ui.drag(&mut app, row, row).await;
    assert!(app.help_open);
    assert!(ui.input(&app, "help-search").1, "the search keeps focus");
    ui.key(
        &mut app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::empty(),
        None,
    )
    .await;
    assert!(!app.help_open, "Enter runs the clicked row");
    assert!(!app.tab.selected.is_empty(), "Select all ran");
}

/// A control focused before Help opens, such as by a Tab typed ahead of `?`,
/// keeps its focus behind the dialog so closing Help returns to it, but only
/// the dialog's focused control shows a ring.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn help_hides_the_focus_ring_of_the_control_behind_it() {
    use iced::advanced::{Renderer as _, renderer};
    fn pixels(ui: &mut Ui, app: &App) -> Vec<u8> {
        let theme = app.theme();
        let mut view = app.view();
        ui.tree.diff(view.as_widget());
        let size = ui.viewport.size();
        let node = (view.as_widget_mut()).layout(
            &mut ui.tree,
            &ui.renderer,
            &layout::Limits::new(size, size),
        );
        ui.renderer.reset(ui.viewport);
        view.as_widget().draw(
            &ui.tree,
            &mut ui.renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &ui.viewport,
        );
        let size = Size::new(size.width as u32, size.height as u32);
        Headless::screenshot(&mut ui.renderer, size, 1., theme.palette().background)
    }
    let mut shots = Vec::new();
    let mut ring = Rectangle::default();
    for focused_behind in [false, true] {
        let mut ui = Ui::new().await;
        let (mut app, _) = App::new();
        let tab = Key::Named(Named::Tab);
        if focused_behind {
            ui.key(&mut app, tab.clone(), Code::Tab, Modifiers::empty(), None)
                .await;
            assert_eq!(ui.focused(&app), ["header-about"]);
            ring = ui.bounds(&app, "header-about").expand(4.);
        }
        let task = app.update(Message::ToggleHelp);
        ui.run_task(&mut app, task).await;
        ui.key(&mut app, tab, Code::Tab, Modifiers::empty(), None)
            .await;
        assert_eq!(ui.focused(&app), ["help-about"]);
        shots.push(pixels(&mut ui, &app));
        if focused_behind {
            let task = app.update(Message::ToggleHelp);
            ui.run_task(&mut app, task).await;
            assert_eq!(ui.focused(&app), ["header-about"], "Help returns focus");
        }
    }
    // RGBA rows of the 1280 px wide window, cropped to the ring's band.
    let crop = |pixels: &[u8]| {
        (pixels.chunks(1280 * 4))
            .skip(ring.y as usize)
            .take(ring.height as usize)
            .flat_map(|row| {
                row.chunks(4)
                    .skip(ring.x as usize)
                    .take(ring.width as usize)
            })
            .flatten()
            .copied()
            .collect::<Vec<_>>()
    };
    assert!(crop(&shots[0]) == crop(&shots[1]), "a ring behind Help");
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn help_run_never_runs_a_row_selected_after_it_started() {
    use super::help::{Action, entries};
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let task = app.update(Message::ToggleHelp);
    ui.run_task(&mut app, task).await;
    let index = |name: &str| entries().iter().position(|e| e.name == name).unwrap();
    let (first, second) = (index("Select all"), index("Fit drawing"));
    let _ = app.update(Message::Help(Action::Select(first)));
    let run = ui.bounds(&app, &format!("help-run-{first}")).center();

    // Pointer: press Run, select another row, release on its Run.
    let (_, pressed) = ui.event(
        &app,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(run),
    );
    assert!(pressed.is_empty());
    let _ = app.update(Message::Help(Action::Select(second)));
    assert!(ui.bounds(&app, &format!("help-run-{second}")).contains(run));
    let (_, released) = ui.event(
        &app,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(run),
    );
    assert!(released.is_empty(), "{released:?}");

    // Native: an activation queued for the first row finds no control.
    let mut activate =
        reshiki::accessibility::Activate::<Message>::new(format!("help-run-{first}"));
    ui.inspect(
        &app,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(activate.message().is_none());
    let mut activate =
        reshiki::accessibility::Activate::<Message>::new(format!("help-run-{second}"));
    ui.inspect(
        &app,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(
        activate.message(),
        Some(Message::Help(Action::Run(i))) if *i == second
    ));
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn tool_palette_pointer_gestures_and_keyboard_activation_have_one_owner() {
    use super::palettes;
    use crate::canvas::Tool;
    use std::time::{Duration, Instant};

    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let left_press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let left_release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));

    for tool in [Tool::Ring, Tool::Arrow, app.toolbar.bond] {
        app.palette = None;
        app.tool = Tool::Select;
        let id = format!("tool-{tool:?}");
        let bounds = ui
            .semantics(&app)
            .nodes
            .into_iter()
            .find(|node| node.id == id)
            .expect("tool palette control")
            .bounds;
        let cursor = mouse::Cursor::Available(bounds.center());
        let (status, messages) = ui.event(&app, left_press.clone(), cursor);
        assert_eq!(status, iced::event::Status::Captured);
        if palettes::family(tool) == Some(palettes::Family::Bonds) {
            assert!(
                matches!(messages.as_slice(),
                [Message::Palette(palettes::Action::Open(t))] if *t == tool),
                "{tool:?}: {messages:?}"
            );
            apply(&mut app, messages);
        } else {
            assert!(messages.is_empty(), "{tool:?}: {messages:?}");
            let (_, messages) = ui.event(
                &app,
                Event::Window(iced::window::Event::RedrawRequested(
                    Instant::now() + Duration::from_millis(500),
                )),
                cursor,
            );
            // The first real redraw also measures the drawing viewport.
            // This layout message is unrelated to the tool's pointer
            // command ownership.
            let messages: Vec<_> = messages
                .into_iter()
                .filter(|message| !matches!(message, Message::Viewport(_)))
                .collect();
            assert!(
                matches!(messages.as_slice(),
                [Message::Palette(palettes::Action::Open(t))] if *t == tool),
                "{tool:?}: {messages:?}"
            );
            apply(&mut app, messages);
        }
        assert!(app.palette.is_some());
        // Opening the palette makes the background inert, so the real
        // release must not activate either the canvas or its wrapper.
        let (_, messages) = ui.event(&app, left_release.clone(), cursor);
        assert!(messages.is_empty(), "{tool:?}: {messages:?}");
        apply(&mut app, messages);
        assert!(app.palette.is_some(), "release must retain the flyout");
        assert_eq!(app.tool, Tool::Select);
    }

    app.palette = None;
    let point = ui.input(&app, "tool-Ring").0.center();
    let cursor = mouse::Cursor::Available(point);
    assert!(ui.event(&app, left_press, cursor).1.is_empty());
    let (_, messages) = ui.event(&app, left_release, cursor);
    assert!(matches!(messages.as_slice(), [Message::Tool(Tool::Ring)]));
    apply(&mut app, messages);
    assert_eq!(app.tool, Tool::Ring);
    assert!(app.palette.is_none());

    // Pointer focus belongs to the semantic wrapper; its Enter and Space
    // activation still selects the tool once without invoking the canvas.
    for event in [
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        ),
        Event::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Named(Named::Space),
            modified_key: Key::Named(Named::Space),
            physical_key: Physical::Code(Code::Space),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
        }),
    ] {
        if matches!(&event, Event::Keyboard(keyboard::Event::KeyReleased { .. })) {
            let (_, messages) = ui.event(
                &app,
                press(
                    Key::Named(Named::Space),
                    Code::Space,
                    Modifiers::empty(),
                    None,
                ),
                mouse::Cursor::Unavailable,
            );
            assert!(messages.is_empty());
        }
        let (status, messages) = ui.event(&app, event, mouse::Cursor::Unavailable);
        assert_eq!(status, iced::event::Status::Captured);
        assert!(matches!(messages.as_slice(), [Message::Tool(Tool::Ring)]));
        apply(&mut app, messages);
    }
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn first_escape_cancels_focused_atom_label_without_applying_the_draft() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    let atom = app
        .tab
        .doc
        .add_atom("N", reshiki::document::Point::default());
    app.tab.selected = vec![atom];
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::AtomText(super::atom_text::Action::Begin(Some(
        atom,
    ))));
    let bounds = ui.input(&app, "atom-text").0;
    ui.click(&mut app, bounds.center());
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Character("H".into()),
            Code::KeyH,
            Modifiers::SHIFT,
            Some("H"),
        ),
        mouse::Cursor::Unavailable,
    );
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::AtomText(super::atom_text::Action::Editor(_))
    )));
    apply(&mut app, messages);
    assert!(ui.input(&app, "atom-text").1);
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::AtomText(super::atom_text::Action::Cancel)]
    ));
    apply(&mut app, messages);
    assert!(app.tab.atom_text.is_none());
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.selected, [atom]);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn first_escape_cancels_focused_partial_style_without_changing_either_document() {
    use super::document_styles::{Action, Field};
    use reshiki::style::units::Unit;

    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab
        .doc
        .add_atom("O", reshiki::document::Point::default());
    app.tab.saved = app.tab.doc.clone();
    let other_document = app.tab.doc.clone();
    let other_revision = app.tab.revision;
    let _ = app.update(Message::DrawingStyle(Action::Open));
    let _ = app.update(Message::DrawingStyle(Action::Input(
        Field::Bond,
        "1e".into(),
    )));
    app.add_tab();
    app.tab.busy = false;
    let atom = app
        .tab
        .doc
        .add_atom("N", reshiki::document::Point::default());
    app.tab.selected = vec![atom];
    app.tab.saved = app.tab.doc.clone();
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::DrawingStyle(Action::Open));
    let _ = app.update(Message::DrawingStyle(Action::DisplayUnit(
        Unit::Millimetres,
    )));

    let bounds = ui.input_text(&app, "5.08").0;
    ui.click(&mut app, bounds.center());
    let command = Modifiers::COMMAND;
    for event in [
        Event::Keyboard(keyboard::Event::ModifiersChanged(command)),
        press(Key::Character("a".into()), Code::KeyA, command, None),
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
    ] {
        let (_, messages) = ui.event(&app, event, mouse::Cursor::Unavailable);
        apply(&mut app, messages);
    }
    for (text, code) in [("5", Code::Digit5), (" ", Code::Space), ("m", Code::KeyM)] {
        let (_, messages) = ui.event(
            &app,
            press(
                Key::Character(text.into()),
                code,
                Modifiers::empty(),
                Some(text),
            ),
            mouse::Cursor::Unavailable,
        );
        assert!(messages.iter().any(|message| matches!(
            message,
            Message::DrawingStyle(Action::Input(Field::Bond, _))
        )));
        apply(&mut app, messages);
    }
    assert!(
        ui.input_text(&app, "5 m").1,
        "partial style field stays focused"
    );
    assert_eq!(app.tab.doc, drawing);
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(messages.as_slice(), [Message::Escape]));
    apply(&mut app, messages);
    assert!(app.tab.styles.editor.is_none());
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.saved, drawing);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.selected, [atom]);
    assert!(!app.dirty());
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
    assert_eq!(app.appearance.drawing_style_unit, Unit::Millimetres);

    app.select_tab(0);
    assert_eq!(app.tab.doc, other_document);
    assert_eq!(app.tab.saved, other_document);
    assert_eq!(app.tab.revision, other_revision);
    assert!(!app.dirty());
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
    assert!(
        app.tab.styles.editor.is_some(),
        "the other tab keeps its draft"
    );
    app.inspector_tab = InspectorTab::DrawingStyle;
    let _ = ui.input_text(&app, "1e");
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn style_escape_keeps_context_menu_and_inline_caption_priority() {
    use super::{
        context_menu::{self, Page},
        document_styles::{Action, Field},
        inline_text,
    };
    use reshiki::document::Point;

    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let drawing = app.tab.doc.clone();
    let saved = app.tab.saved.clone();
    let revision = app.tab.revision;
    let escape = || {
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
    };
    let _ = app.update(Message::DrawingStyle(Action::Open));
    let _ = app.update(Message::DrawingStyle(Action::Input(
        Field::Bond,
        "1e".into(),
    )));
    app.context_menu = Some(context_menu::State::new(
        iced::Point::new(20., 20.),
        Page::Main,
    ));
    let (status, messages) = ui.event(&app, escape(), mouse::Cursor::Unavailable);
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::ContextMenu(context_menu::Action::Key(
            Named::Escape
        ))]
    ));
    apply(&mut app, messages);
    assert!(app.context_menu.is_none());
    assert!(app.tab.styles.editor.is_some());
    let _ = ui.input_text(&app, "1e");
    let (status, messages) = ui.event(&app, escape(), mouse::Cursor::Unavailable);
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(messages.as_slice(), [Message::Escape]));
    apply(&mut app, messages);
    assert!(app.tab.styles.editor.is_none());

    for inspector_open in [true, false] {
        let _ = app.update(Message::DrawingStyle(Action::Open));
        let _ = app.update(Message::DrawingStyle(Action::Input(
            Field::Bond,
            "1e".into(),
        )));
        app.inspector_open = inspector_open;
        let _ = app.update(Message::InlineText(inline_text::Action::Begin(
            None,
            Point::default(),
        )));
        let bounds = ui.input(&app, "inline-caption").0;
        ui.click(&mut app, bounds.center());
        let (_, messages) = ui.event(
            &app,
            press(
                Key::Character("x".into()),
                Code::KeyX,
                Modifiers::empty(),
                Some("x"),
            ),
            mouse::Cursor::Unavailable,
        );
        apply(&mut app, messages);
        assert_eq!(app.tab.caption, "x");
        assert!(ui.input(&app, "inline-caption").1);
        let (status, messages) = ui.event(&app, escape(), mouse::Cursor::Unavailable);
        assert_eq!(status, iced::event::Status::Captured);
        assert!(matches!(
            messages.as_slice(),
            [Message::InlineText(inline_text::Action::Finish(false))]
        ));
        apply(&mut app, messages);
        assert!(app.tab.inline_text.is_none());
        assert!(app.tab.styles.editor.is_some());
        app.inspector_open = true;
        let _ = ui.input_text(&app, "1e");
    }
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.saved, saved);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn assistant_shortcuts_follow_actual_prompt_focus() {
    let mut ui = Ui::new().await;
    let mut app = assistant_app();
    let draft = app.assistant.input_text();
    let events = [
        press(
            Key::Character("v".into()),
            Code::KeyV,
            Modifiers::COMMAND,
            Some("v"),
        ),
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::COMMAND,
            None,
        ),
    ];
    // Unfocused on first display, focused by a click, then unfocused by a
    // canvas click. Keep the same widget tree through each transition.
    for focused in [false, true, false] {
        if focused {
            let bounds = ui.input(&app, "assistant-input").0;
            ui.click(&mut app, bounds.center());
        } else {
            ui.click(&mut app, iced::Point::new(500., 400.));
        }
        assert_eq!(ui.input(&app, "assistant-input").1, focused);
        for (index, event) in events.iter().enumerate() {
            let (status, messages) = ui.event(&app, event.clone(), mouse::Cursor::Unavailable);
            if focused {
                assert_eq!(status, iced::event::Status::Captured);
                assert!(
                    matches!(messages.as_slice(),
                        [Message::Assistant(assistant::Action::Paste { image_only: false })] if index == 0
                    ) || matches!(messages.as_slice(),
                        [Message::Assistant(assistant::Action::Send)] if index == 1
                    ),
                    "{messages:?}"
                );
            } else {
                assert_eq!(status, iced::event::Status::Ignored);
                assert!(!messages.iter().any(|m| matches!(m, Message::Assistant(_))));
                if index == 0 {
                    assert!(matches!(messages.as_slice(), [Message::Paste]));
                }
            }
        }
    }
    // Dispatch only: no real clipboard read or Assistant request is started.
    assert_eq!(app.assistant.input_text(), draft);
    assert!(!app.assistant.busy);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn updates_dialog_captures_keys_before_editors_file_commands_and_canvas() {
    let mut ui = Ui::new().await;
    let mut app = assistant_app();
    let atom = app
        .tab
        .doc
        .add_atom("C", reshiki::document::Point::default());
    let bounds = ui.input(&app, "assistant-input").0;
    ui.click(&mut app, bounds.center());
    assert!(ui.input(&app, "assistant-input").1);
    app.tab.selected = vec![atom];
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let draft = app.assistant.input_text();
    let _ = app.update(Message::Updates(updates::Action::Show(true)));
    let mut events = vec![
        press(
            Key::Named(Named::Delete),
            Code::Delete,
            Modifiers::empty(),
            None,
        ),
        press(
            Key::Named(Named::ArrowRight),
            Code::ArrowRight,
            Modifiers::empty(),
            None,
        ),
        press(Key::Named(Named::F1), Code::F1, Modifiers::empty(), None),
        press(
            Key::Character("N".into()),
            Code::KeyN,
            Modifiers::SHIFT,
            Some("N"),
        ),
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::COMMAND,
            None,
        ),
        press(Key::Named(Named::Tab), Code::Tab, Modifiers::CTRL, None),
        press(
            Key::Named(Named::Tab),
            Code::Tab,
            Modifiers::CTRL | Modifiers::SHIFT,
            None,
        ),
        Event::InputMethod(iced::advanced::input_method::Event::Commit("窒素".into())),
    ];
    events.extend(
        [
            ("v", Code::KeyV),
            ("n", Code::KeyN),
            ("o", Code::KeyO),
            ("s", Code::KeyS),
            ("w", Code::KeyW),
            ("p", Code::KeyP),
            ("1", Code::Digit1),
        ]
        .map(|(text, code)| {
            press(
                Key::Character(text.into()),
                code,
                Modifiers::COMMAND,
                Some(text),
            )
        }),
    );
    for event in events {
        let (status, messages) = ui.event(&app, event.clone(), mouse::Cursor::Unavailable);
        assert_eq!(status, iced::event::Status::Captured, "{event:?}");
        assert!(messages.is_empty(), "{event:?}: {messages:?}");
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [atom]);
        assert_eq!(app.assistant.input_text(), draft);
        assert!(!app.tab.history.can_undo());
        assert!(app.updates.open);
        assert!(!app.help_open);
    }
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::Updates(updates::Action::Show(false))]
    ));
    apply(&mut app, messages);
    assert!(!app.updates.open);
    let _ = app.update(Message::Updates(updates::Action::Show(true)));
    let close = ui.text(&app, "Close");
    ui.click(&mut app, close.center());
    assert!(
        !app.updates.open,
        "the modal still accepts its mouse controls"
    );
    assert_eq!(app.tab.doc, drawing);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn about_keyboard_traversal_copies_version_and_opens_update_settings() {
    let mut ui = Ui::new().await;
    let mut app = assistant_app();
    let bounds = ui.input(&app, "assistant-input").0;
    ui.click(&mut app, bounds.center());
    assert!(ui.input(&app, "assistant-input").1);
    let drawing = app.tab.doc.clone();
    let draft = app.assistant.input_text();
    app.updates.open = true;
    app.updates.data_folder = Some(std::path::PathBuf::from("/home/me/.reshiki"));
    for (named, code) in [(Named::Enter, Code::Enter), (Named::Space, Code::Space)] {
        let (status, messages) = ui.event(
            &app,
            press(Key::Named(named), code, Modifiers::empty(), None),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(messages.is_empty(), "{messages:?}");
    }
    for expected in [
        "updates-copy-version",
        "updates-close",
        "updates-check",
        "updates-notes",
        "updates-star",
        "updates-website",
        "updates-license",
        "updates-settings",
        "updates-data-folder",
        "updates-copy-version",
    ] {
        let (status, messages) = ui.event(
            &app,
            press(Key::Named(Named::Tab), Code::Tab, Modifiers::empty(), None),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(messages.is_empty());
        let snapshot = ui.semantics(&app);
        assert!(snapshot.duplicate_ids.is_empty());
        assert!(
            snapshot
                .nodes
                .iter()
                .all(|node| node.id.starts_with("updates-"))
        );
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .filter(|node| node.focused)
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            [expected]
        );
    }

    let enter = || {
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        )
    };
    let (status, messages) = ui.event(&app, enter(), mouse::Cursor::Unavailable);
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::Updates(updates::Action::CopyVersion)]
    ));
    apply(&mut app, messages);
    assert!(
        ui.semantics(&app)
            .nodes
            .iter()
            .any(|node| { node.id == "updates-copy-version" && node.name == "Version copied" })
    );

    for _ in 0..7 {
        let (_, messages) = ui.event(
            &app,
            press(Key::Named(Named::Tab), Code::Tab, Modifiers::empty(), None),
            mouse::Cursor::Unavailable,
        );
        assert!(messages.is_empty());
    }
    assert_eq!(ui.focused(&app), ["updates-settings"]);
    let (_, messages) = ui.event(&app, enter(), mouse::Cursor::Unavailable);
    assert!(matches!(
        messages.as_slice(),
        [Message::Updates(updates::Action::OpenSettings)]
    ));
    apply(&mut app, messages);
    assert!(!app.updates.open);
    assert!(app.settings.open);
    assert!(
        ui.semantics(&app)
            .nodes
            .iter()
            .any(|node| node.id == "settings-update-check")
    );
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.assistant.input_text(), draft);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn focused_numeric_field_keeps_arrow_keys_out_of_drawing_transforms() {
    let mut ui = Ui::new().await;
    // Keep the selected-object controls visible for a real input click.
    ui.viewport = Rectangle::with_size(Size::new(1280., 1800.));
    let (mut app, _) = App::new();
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app.tab.doc = serde_json::from_str(include_str!(
        "../../tests/fixtures/pyrrole-rotation-drift.rsk"
    ))
    .unwrap();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.saved = app.tab.doc.clone();
    app.tab.history = super::History::default();
    app.tab.busy = false;
    let _ = app.update(Message::Canvas(crate::canvas::Edit::BeginTransform(
        crate::canvas::TransformField::Rotation,
    )));
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let selection = app.tab.selected.clone();
    let bounds = ui.input(&app, "transform-rotation").0;
    assert!(ui.viewport.contains(bounds.center()), "{bounds:?}");
    ui.click(&mut app, bounds.center());
    assert!(ui.input(&app, "transform-rotation").1);
    let primary = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    for event in [
        Event::Keyboard(keyboard::Event::ModifiersChanged(primary)),
        press(Key::Character("a".into()), Code::KeyA, primary, Some("a")),
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
    ] {
        let (_, messages) = ui.event(&app, event, mouse::Cursor::Unavailable);
        apply(&mut app, messages);
    }
    for (text, code) in [
        ("1", Code::Digit1),
        ("2", Code::Digit2),
        ("3", Code::Digit3),
    ] {
        let (_, messages) = ui.event(
            &app,
            press(
                Key::Character(text.into()),
                code,
                Modifiers::empty(),
                Some(text),
            ),
            mouse::Cursor::Unavailable,
        );
        apply(&mut app, messages);
    }
    assert!(ui.input_text(&app, "123").1);
    assert_eq!(app.tab.doc, drawing, "Typing is only a draft");
    for modifiers in [
        Modifiers::ALT,
        Modifiers::ALT | Modifiers::SHIFT,
        Modifiers::empty(),
    ] {
        let (_, messages) = ui.event(
            &app,
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
            mouse::Cursor::Unavailable,
        );
        apply(&mut app, messages);
        for (named, code) in [
            (Named::ArrowUp, Code::ArrowUp),
            (Named::ArrowDown, Code::ArrowDown),
            (Named::ArrowLeft, Code::ArrowLeft),
            (Named::ArrowRight, Code::ArrowRight),
        ] {
            let (status, messages) = ui.event(
                &app,
                press(Key::Named(named), code, modifiers, None),
                mouse::Cursor::Unavailable,
            );
            assert_eq!(
                status,
                iced::event::Status::Captured,
                "{named:?} {modifiers:?}: {messages:?}"
            );
            assert!(messages.is_empty(), "{named:?} {modifiers:?}: {messages:?}");
            apply(&mut app, messages);
            assert!(ui.input_text(&app, "123").1);
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.saved, drawing);
            assert_eq!(app.tab.revision, revision);
            assert_eq!(app.tab.selected, selection);
            assert!(!app.tab.history.can_undo());
            assert!(!app.tab.history.can_redo());
            assert!(!app.dirty());
        }
    }
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    apply(&mut app, messages);
    assert!(!ui.input(&app, "transform-rotation").1);
    let (_, messages) = ui.event(
        &app,
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::ALT)),
        mouse::Cursor::Unavailable,
    );
    apply(&mut app, messages);
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::ArrowDown),
            Code::ArrowDown,
            Modifiers::ALT,
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Ignored);
    assert!(matches!(
        messages.as_slice(),
        [Message::Transform(reshiki::editing::Transform::Rotate(15.))]
    ));
    apply(&mut app, messages);
    assert_ne!(app.tab.doc, drawing, "Blur restores the drawing shortcut");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, drawing);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn keyboard_drawing_mode_keeps_real_fields_in_charge_of_typing_arrows_and_f8() {
    use super::keyboard_drawing::Action as Drawing;
    use reshiki::{document::Point, keyboard_drawing::Target};

    for panel in ["numeric", "sidebar labels", "assistant", "atom label"] {
        let mut ui = Ui::new().await;
        ui.viewport = Rectangle::with_size(Size::new(1280., 1800.));
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![a];
        app.tab.saved = app.tab.doc.clone();
        app.tab.history = super::History::default();
        app.tab.busy = false;
        if !app.tab.keyboard_drawing.enabled() {
            let _ = app.update(Message::KeyboardDrawing(Drawing::Toggle));
        }
        app.sync_keyboard_drawing();
        assert!(app.tab.keyboard_drawing.enabled());
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
        app.inspector_open = true;
        let field = match panel {
            "numeric" => {
                app.tab.selected = vec![a, b];
                app.inspector_tab = InspectorTab::Properties;
                let _ = app.update(Message::Canvas(crate::canvas::Edit::BeginTransform(
                    crate::canvas::TransformField::Rotation,
                )));
                ui.input(&app, "transform-rotation").0
            }
            "sidebar labels" => {
                app.inspector_tab = InspectorTab::Labels;
                app.tab.labels.seed = "keyboard-focus-seed".into();
                ui.input_text(&app, "keyboard-focus-seed").0
            }
            "assistant" => {
                app.inspector_tab = InspectorTab::Assistant;
                ui.input(&app, "assistant-input").0
            }
            _ => {
                let _ = app.update(Message::AtomText(super::atom_text::Action::Begin(Some(a))));
                ui.input(&app, "atom-text").0
            }
        };
        assert!(ui.viewport.contains(field.center()), "{panel}: {field:?}");
        ui.click(&mut app, field.center());
        let drawing = app.tab.doc.clone();
        let revision = app.tab.revision;
        let selection = app.tab.selected.clone();
        let target = app.tab.keyboard_drawing.target();

        for (text, code, modifiers) in [
            ("3", Code::Digit3, Modifiers::empty()),
            ("n", Code::KeyN, Modifiers::empty()),
            ("N", Code::KeyN, Modifiers::SHIFT),
            ("[", Code::BracketLeft, Modifiers::empty()),
            ("]", Code::BracketRight, Modifiers::empty()),
            (":", Code::Semicolon, Modifiers::SHIFT),
        ] {
            let (status, messages) = ui.event(
                &app,
                press(Key::Character(text.into()), code, modifiers, Some(text)),
                mouse::Cursor::Unavailable,
            );
            assert_eq!(
                status,
                iced::event::Status::Captured,
                "{panel}: {text}: {messages:?}"
            );
            assert!(
                !messages.iter().any(|message| matches!(
                    message,
                    Message::ContextKey(_)
                        | Message::KeyboardDrawing(_)
                        | Message::Shortcut(_)
                        | Message::Commands(_)
                )),
                "{panel}: {messages:?}"
            );
            apply(&mut app, messages);
        }

        for modifiers in [
            Modifiers::empty(),
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            for (named, code) in [
                (Named::ArrowLeft, Code::ArrowLeft),
                (Named::ArrowRight, Code::ArrowRight),
                (Named::ArrowUp, Code::ArrowUp),
                (Named::ArrowDown, Code::ArrowDown),
            ] {
                let (status, messages) = ui.event(
                    &app,
                    press(Key::Named(named), code, modifiers, None),
                    mouse::Cursor::Unavailable,
                );
                assert_eq!(
                    status,
                    iced::event::Status::Captured,
                    "{panel}: {named:?}: {messages:?}"
                );
                assert!(
                    !messages.iter().any(|message| matches!(
                        message,
                        Message::Shortcut(_) | Message::Transform(_) | Message::KeyboardDrawing(_)
                    )),
                    "{panel}: {messages:?}"
                );
                apply(&mut app, messages);
            }
        }
        let (status, messages) = ui.event(
            &app,
            press(Key::Named(Named::F8), Code::F8, Modifiers::empty(), None),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            status,
            iced::event::Status::Captured,
            "{panel}: F8: {messages:?}"
        );
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::KeyboardDrawing(_))),
            "{panel}: {messages:?}"
        );
        apply(&mut app, messages);
        assert!(
            app.tab.keyboard_drawing.enabled(),
            "{panel}: F8 must stay in the field"
        );
        assert_eq!(app.tab.keyboard_drawing.target(), target, "{panel}");
        assert_eq!(
            app.tab.keyboard_drawing.marked(),
            None,
            "{panel}: typed brackets are field content"
        );
        assert_eq!(app.tab.doc, drawing, "{panel}");
        assert_eq!(app.tab.saved, drawing, "{panel}");
        assert_eq!(app.tab.revision, revision, "{panel}");
        assert_eq!(app.tab.selected, selection, "{panel}");
        assert!(!app.tab.history.can_undo(), "{panel}");
        assert!(!app.tab.history.can_redo(), "{panel}");

        let focused = match panel {
            "numeric" => ui.input(&app, "transform-rotation").1,
            "sidebar labels" => ui.input_text(&app, &app.tab.labels.seed).1,
            "assistant" => ui.input(&app, "assistant-input").1,
            _ => ui.input(&app, "atom-text").1,
        };
        assert!(focused, "{panel}: F8 must preserve actual field focus");
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn optimization_chord_stays_in_real_focused_fields_and_routes_after_blur() {
    use super::optimization::Action as Optimize;
    use reshiki::document::Point;
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };

    for panel in ["numeric", "sidebar labels"] {
        let mut ui = Ui::new().await;
        ui.viewport = Rectangle::with_size(Size::new(1280., 1800.));
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![a, b];
        app.tab.saved = app.tab.doc.clone();
        app.tab.history = super::History::default();
        app.sync_keyboard_drawing();
        app.inspector_open = true;
        let field = if panel == "numeric" {
            app.inspector_tab = InspectorTab::Properties;
            let _ = app.update(Message::Canvas(crate::canvas::Edit::BeginTransform(
                crate::canvas::TransformField::Rotation,
            )));
            ui.input(&app, "transform-rotation").0
        } else {
            app.inspector_tab = InspectorTab::Labels;
            app.tab.labels.seed = "optimization-focus-seed".into();
            ui.input_text(&app, "optimization-focus-seed").0
        };
        ui.click(&mut app, field.center());
        let drawing = app.tab.doc.clone();
        let revision = app.tab.revision;
        let target = app.tab.keyboard_drawing.target();
        let draft = if panel == "numeric" {
            ui.input_value(&app, "transform-rotation")
        } else {
            app.tab.labels.seed.clone()
        };
        let (status, messages) = ui.event(
            &app,
            press(
                Key::Character("D".into()),
                Code::KeyD,
                command | Modifiers::SHIFT,
                Some("D"),
            ),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            status,
            iced::event::Status::Captured,
            "{panel}: {messages:?}"
        );
        assert!(
            messages.is_empty(),
            "{panel}: Command+Shift+D must not publish or type: {messages:?}"
        );
        apply(&mut app, messages);
        let after_command = if panel == "numeric" {
            assert!(ui.input(&app, "transform-rotation").1);
            ui.input_value(&app, "transform-rotation")
        } else {
            assert!(ui.input_text(&app, &app.tab.labels.seed).1);
            app.tab.labels.seed.clone()
        };
        assert_eq!(
            after_command, draft,
            "{panel}: command text must not be inserted"
        );
        assert!(
            app.tab.optimization.is_none(),
            "{panel}: no preview while typing"
        );
        assert_eq!(app.tab.doc, drawing, "{panel}");
        assert_eq!(app.tab.keyboard_drawing.target(), target, "{panel}");

        let (status, messages) = ui.event(
            &app,
            press(
                Key::Character("D".into()),
                Code::KeyD,
                Modifiers::SHIFT,
                Some("D"),
            ),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            status,
            iced::event::Status::Captured,
            "{panel}: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| matches!(
                message,
                Message::Optimization(_) | Message::ContextKey(_) | Message::Shortcut(_)
            )),
            "{panel}: {messages:?}"
        );
        apply(&mut app, messages);
        let typed = if panel == "numeric" {
            ui.input_value(&app, "transform-rotation")
        } else {
            app.tab.labels.seed.clone()
        };
        assert_ne!(typed, draft, "{panel}: Shift+D remains text input");
        assert!(typed.contains('D'), "{panel}: {typed}");
        assert!(app.tab.optimization.is_none());
        assert_eq!(
            app.tab.doc, drawing,
            "{panel}: typing must not seed, duplicate or modify the drawing"
        );
        assert_eq!(app.tab.saved, drawing, "{panel}");
        assert_eq!(app.tab.revision, revision, "{panel}");
        assert_eq!(app.tab.selected, vec![a, b], "{panel}");
        assert!(
            !app.tab.history.can_undo() && !app.tab.history.can_redo(),
            "{panel}"
        );

        let (_, messages) = ui.event(
            &app,
            press(
                Key::Named(Named::Escape),
                Code::Escape,
                Modifiers::empty(),
                None,
            ),
            mouse::Cursor::Unavailable,
        );
        apply(&mut app, messages);
        let focused = if panel == "numeric" {
            ui.input(&app, "transform-rotation").1
        } else {
            ui.input_text(&app, &app.tab.labels.seed).1
        };
        assert!(!focused, "{panel}: Escape releases field focus");
        let (status, messages) = ui.event(
            &app,
            press(
                Key::Character("D".into()),
                Code::KeyD,
                command | Modifiers::SHIFT,
                Some("D"),
            ),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            status,
            iced::event::Status::Ignored,
            "{panel}: {messages:?}"
        );
        assert!(matches!(
            messages.as_slice(),
            [Message::Optimization(Optimize::Begin)]
        ));
        apply(&mut app, messages);
        assert!(
            app.tab.optimization.is_some(),
            "{panel}: blurred shortcut opens the preview"
        );
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.tab.history.can_undo() && !app.tab.history.can_redo());
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn join_chord_stays_in_real_fields_and_coordinates_selected_en_after_blur() {
    use reshiki::atom_labels::refresh::Refresh;
    use reshiki::document::Document;

    for panel in ["numeric", "sidebar labels"] {
        for typed in [false, true] {
            let mut ui = Ui::new().await;
            ui.viewport = Rectangle::with_size(Size::new(1280., 1800.));
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc = Document::from_json(include_bytes!(
                "../../tests/fixtures/coordination/co-en3-before.rsk"
            ))
            .unwrap();
            // Seed derived labels as after a native load's background refresh settles.
            Refresh::calculate(&app.tab.doc, &Refresh::default())
                .unwrap()
                .apply(&mut app.tab.doc);
            if typed {
                app.tab.doc = reshiki::atom_text::apply(
                    &app.tab.doc,
                    3,
                    "[C,N]",
                    reshiki::atom_text::Mode::Auto,
                )
                .unwrap();
            }
            app.tab.saved = app.tab.doc.clone();
            app.tab.selected = vec![1, 5];
            app.inspector_open = true;
            let field = if panel == "numeric" {
                app.inspector_tab = InspectorTab::Properties;
                let _ = app.update(Message::Canvas(crate::canvas::Edit::BeginTransform(
                    crate::canvas::TransformField::Rotation,
                )));
                ui.input(&app, "transform-rotation").0
            } else {
                app.inspector_tab = InspectorTab::Labels;
                app.tab.labels.seed = "join-focus-seed".into();
                ui.input_text(&app, "join-focus-seed").0
            };
            assert!(
                ui.viewport.contains(field.center()),
                "{panel}: field bounds {field:?}"
            );
            ui.click(&mut app, field.center());
            let focused = if panel == "numeric" {
                ui.input(&app, "transform-rotation").1
            } else {
                ui.input_text(&app, "join-focus-seed").1
            };
            assert!(focused, "{panel}: field did not acquire focus at {field:?}");
            let original = app.tab.doc.clone();
            let revision = app.tab.revision;
            let chord = || {
                press(
                    Key::Character("j".into()),
                    Code::KeyJ,
                    Modifiers::COMMAND,
                    Some("j"),
                )
            };
            let (status, messages) = ui.event(&app, chord(), mouse::Cursor::Unavailable);
            apply(&mut app, messages.clone());
            assert_eq!(
                app.tab.doc.bonds.len(),
                original.bonds.len(),
                "{panel}: focused field must not add a contact through {messages:?}"
            );
            assert_eq!(app.tab.doc, original);
            assert_eq!(
                status,
                iced::event::Status::Captured,
                "{panel}: {messages:?}"
            );
            assert!(messages.is_empty(), "{panel}: {messages:?}");
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());

            let (_, messages) = ui.event(
                &app,
                press(
                    Key::Named(Named::Escape),
                    Code::Escape,
                    Modifiers::empty(),
                    None,
                ),
                mouse::Cursor::Unavailable,
            );
            apply(&mut app, messages);
            let focused = if panel == "numeric" {
                ui.input(&app, "transform-rotation").1
            } else {
                ui.input_text(&app, &app.tab.labels.seed).1
            };
            assert!(!focused, "{panel}: Escape releases the real field focus");

            if typed {
                let selected = app.tab.selected.clone();
                let camera = (app.tab.camera.center, app.tab.camera.zoom);
                let (status, messages) = ui.event(&app, chord(), mouse::Cursor::Unavailable);
                assert_eq!(status, iced::event::Status::Ignored);
                assert!(matches!(
                    messages.as_slice(),
                    [Message::Shortcut(shortcuts::Action::Join)]
                ));
                apply(&mut app, messages);
                assert!(app.error);
                assert_eq!(app.status, reshiki::generic::JOIN_NOTICE);
                assert_eq!(app.tab.doc, original);
                assert_eq!(app.tab.selected, selected);
                assert_eq!((app.tab.camera.center, app.tab.camera.zoom), camera);
                assert_eq!(app.tab.revision, revision);
                assert!(!app.tab.history.can_undo() && !app.tab.history.can_redo());
                continue;
            }

            // Widget dispatch must leave this chord to the app's ignored-key route.
            // Alternate selection order and close each chelate within one component.
            for (index, donor) in [5, 2, 6, 9, 10, 13].into_iter().enumerate() {
                app.tab.selected = if index % 2 == 0 {
                    vec![1, donor]
                } else {
                    vec![donor, 1]
                };
                let (status, messages) = ui.event(&app, chord(), mouse::Cursor::Unavailable);
                assert_eq!(
                    status,
                    iced::event::Status::Ignored,
                    "{panel}: {messages:?}"
                );
                assert!(matches!(
                    messages.as_slice(),
                    [Message::Shortcut(shortcuts::Action::Join)]
                ));
                apply(&mut app, messages);
                assert!(!app.error, "{panel}: {}", app.status);
                assert_eq!(app.tab.doc.atoms, original.atoms);
                assert_eq!(&app.tab.doc.bonds[..9], original.bonds.as_slice());
                assert_eq!(app.tab.doc.bonds.len(), 10 + index);
                let contact = app.tab.doc.bonds.last().unwrap();
                assert_eq!((contact.a, contact.b, contact.order), (donor, 1, 5));
            }
            let completed = app.tab.doc.clone();
            let revision = app.tab.revision;
            let (_, messages) = ui.event(&app, chord(), mouse::Cursor::Unavailable);
            apply(&mut app, messages);
            assert_eq!(app.tab.doc, completed);
            assert_eq!(app.tab.revision, revision);
            assert_eq!(app.tab.history.undo_frames().len(), 6);
            for modifiers in [Modifiers::COMMAND, Modifiers::COMMAND | Modifiers::SHIFT] {
                let (_, messages) = ui.event(
                    &app,
                    press(Key::Character("z".into()), Code::KeyZ, modifiers, Some("z")),
                    mouse::Cursor::Unavailable,
                );
                apply(&mut app, messages);
                assert_eq!(
                    app.tab.doc.bonds.len(),
                    if modifiers.shift() { 15 } else { 14 }
                );
            }
            assert_eq!(app.tab.doc, completed);
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn command_palette_search_field_types_colon() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Commands(super::command_palette::Action::Open));
    let field = ui.input(&app, "command-palette-search").0;
    ui.click(&mut app, field.center());
    for (text, code, modifiers) in [
        (":", Code::Semicolon, Modifiers::SHIFT),
        ("w", Code::KeyW, Modifiers::empty()),
    ] {
        let (status, messages) = ui.event(
            &app,
            press(Key::Character(text.into()), code, modifiers, Some(text)),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            status,
            iced::event::Status::Captured,
            "{text}: {messages:?}"
        );
        apply(&mut app, messages);
    }
    assert!(app.commands.open);
    assert_eq!(app.commands.query, ":w");
}

/// Tab in the search field cycles the category filter; focus and typing stay
/// in the field.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn command_palette_tab_cycles_the_filter_from_the_search_field() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Commands(super::command_palette::Action::Open));
    let _ = app.update(Message::Commands(super::command_palette::Action::Query(
        "bond".into(),
    )));
    let field = ui.input(&app, "command-palette-search").0;
    ui.click(&mut app, field.center());
    let mut filters = vec![];
    for modifiers in [Modifiers::empty(), Modifiers::empty(), Modifiers::SHIFT] {
        let (status, messages) = ui.event(
            &app,
            press(Key::Named(Named::Tab), Code::Tab, modifiers, None),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(status, iced::event::Status::Captured, "{messages:?}");
        apply(&mut app, messages);
        filters.push(app.commands.filter.map(|c| c.label()));
    }
    assert_eq!(filters, [Some("Insert"), Some("Bond"), Some("Insert")]);
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Character("s".into()),
            Code::KeyS,
            Modifiers::empty(),
            Some("s"),
        ),
        mouse::Cursor::Unavailable,
    );
    apply(&mut app, messages);
    assert_eq!(app.commands.query, "bonds", "the search field keeps focus");
}

/// Click the control with this accessibility id and run the widget
/// operations its messages return, as the event loop does.
async fn click_running_tasks(ui: &mut Ui, app: &mut App, id: &str) {
    let bounds = ui
        .semantics(app)
        .nodes
        .into_iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("{id}"))
        .bounds;
    for event in [
        mouse::Event::CursorMoved {
            position: bounds.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let cursor = mouse::Cursor::Available(bounds.center());
        let (_, messages) = ui.event(app, Event::Mouse(event), cursor);
        for message in messages {
            let task = app.update(message);
            ui.run_task(app, task).await;
        }
    }
}

/// Clicking a filter chip hands focus back to the search field, so typing
/// edits the query and Enter runs the selected command.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn command_palette_chip_click_keeps_typing_in_the_search_field() {
    use super::command_palette::Action;
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Commands(Action::Open));
    let _ = app.update(Message::Commands(Action::Query("bond".into())));
    let field = ui.input(&app, "command-palette-search").0;
    ui.click(&mut app, field.center());
    click_running_tasks(&mut ui, &mut app, "command-palette-filter-bond").await;
    assert_eq!(app.commands.filter.map(|c| c.label()), Some("Bond"));
    for (key, code, text) in [
        (Key::Character("s".into()), Code::KeyS, Some("s")),
        (Key::Named(Named::Enter), Code::Enter, None),
    ] {
        let (_, messages) = ui.event(
            &app,
            press(key, code, Modifiers::empty(), text),
            mouse::Cursor::Unavailable,
        );
        apply(&mut app, messages);
        if text.is_some() {
            assert_eq!(app.commands.query, "bonds");
        }
    }
    assert!(!app.commands.open, "Enter ran the selected command");
}

/// Clicking a row that cannot run keeps the palette open with typing in the
/// search field.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn command_palette_unavailable_row_click_keeps_typing_in_the_search_field() {
    use super::command_palette::Action;
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Commands(Action::Open));
    let _ = app.update(Message::Commands(Action::Query("bring bond".into())));
    let field = ui.input(&app, "command-palette-search").0;
    ui.click(&mut app, field.center());
    let unavailable = app
        .commands
        .ranked()
        .into_iter()
        .find(|command| command.name == "Bring bond forward")
        .expect("Bring bond forward is a result")
        .clone();
    assert!(
        app.command_unavailable(&unavailable).is_some(),
        "an empty drawing has no bond to bring forward"
    );
    let index = app
        .commands
        .catalog()
        .iter()
        .position(|c| c.name == unavailable.name)
        .expect("a catalog row");
    click_running_tasks(&mut ui, &mut app, &format!("command-palette-row-{index}")).await;
    assert!(app.commands.open, "the palette stays open");
    assert_eq!(app.commands.current().unwrap().name, unavailable.name);
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Character("s".into()),
            Code::KeyS,
            Modifiers::empty(),
            Some("s"),
        ),
        mouse::Cursor::Unavailable,
    );
    apply(&mut app, messages);
    assert_eq!(
        app.commands.query, "bring bonds",
        "the search field keeps focus"
    );
}

/// Every command row is a named button that assistive technology can press.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn command_palette_rows_are_named_buttons() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Commands(super::command_palette::Action::Open));
    let _ = app.update(Message::Commands(super::command_palette::Action::Query(
        "benzene".into(),
    )));
    let snapshot = ui.semantics(&app);
    assert!(snapshot.duplicate_ids.is_empty());
    let rows: Vec<_> = snapshot
        .nodes
        .iter()
        .filter(|node| node.id.starts_with("command-palette-row-"))
        .collect();
    assert!(!rows.is_empty());
    assert_eq!(rows[0].name, "Benzene");
    assert!(
        rows.iter()
            .all(|row| row.role == reshiki::accessibility::Role::Button && !row.name.is_empty())
    );
    // The filter chips are named toggle buttons; the active one is All.
    let chips: Vec<_> = snapshot
        .nodes
        .iter()
        .filter(|node| node.id.starts_with("command-palette-filter-"))
        .map(|node| (node.role, node.name.as_str()))
        .collect();
    let all = format!("All, {} results", app.commands.ranked().len());
    assert_eq!(chips.first().map(|chip| chip.1), Some(all.as_str()));
    assert!(chips.len() > 1, "{chips:?}");
    // A single hit reads "1 result".
    assert!(chips.iter().all(|(role, name)| {
        *role == reshiki::accessibility::Role::ToggleButton
            && (name.ends_with(" results") || name.ends_with(", 1 result"))
    }));
    // Ids follow commands, not positions: a narrower search keeps Benzene's id.
    let benzene = rows[0].id.clone();
    let _ = app.update(Message::Commands(super::command_palette::Action::Query(
        "benzene ring".into(),
    )));
    let snapshot = ui.semantics(&app);
    assert!(
        snapshot
            .nodes
            .iter()
            .filter(|node| node.id == benzene)
            .all(|node| node.name == "Benzene")
    );
    let named = |query: &str, ui: &mut Ui, app: &mut App| {
        let _ = app.update(Message::Commands(super::command_palette::Action::Query(
            query.into(),
        )));
        ui.semantics(app)
            .nodes
            .into_iter()
            .find(|node| node.name.starts_with("Insert structure named"))
            .map(|node| node.id)
    };
    let hexanoic = named("hexanoic acid", &mut ui, &mut app).unwrap();
    let heptanoic = named("heptanoic acid", &mut ui, &mut app).unwrap();
    assert_ne!(
        hexanoic, heptanoic,
        "a query-created row changes identity with its query"
    );
    // A long pasted query keeps every id within the native 256-byte limit.
    let long = named(&"C".repeat(400), &mut ui, &mut app).unwrap();
    assert!(long.len() <= 256, "{long}");
    let snapshot = ui.semantics(&app);
    reshiki::accessibility::tree::NativeTree::default()
        .update(&snapshot, "ReShiki", ui.viewport, 1.)
        .expect("a long query keeps the native accessibility tree");
}

/// The context row's Rotate field: click it, type an angle and press Enter
/// to rotate the selection by exactly that angle as one Undo step.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn context_row_rotation_field_applies_a_typed_angle() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    app.inspector_open = false;
    app.appearance.arrange_controls = true;
    app.tool = crate::canvas::Tool::Select;
    app.tab.doc = serde_json::from_str(include_str!(
        "../../tests/fixtures/pyrrole-rotation-drift.rsk"
    ))
    .unwrap();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.saved = app.tab.doc.clone();
    app.tab.history = super::History::default();
    app.tab.busy = false;
    app.sync_numeric_transforms();
    assert!(app.keyboard_drawing_active());
    assert_eq!(app.tab.keyboard_drawing.marker_point(&app.tab.doc), None);
    let drawing = app.tab.doc.clone();
    let mut expected = drawing.clone();
    let ids = expected.expand_abbreviation_selection(&app.tab.selected);
    reshiki::editing::transform(
        &mut expected,
        &ids,
        reshiki::editing::Transform::Rotate(7.5),
    );
    assert_eq!(ui.input_value(&app, "arrange-rotation"), "0");
    let bounds = ui.input(&app, "arrange-rotation").0;
    assert!(bounds.width > 0., "{bounds:?}");
    assert!(ui.viewport.contains(bounds.center()), "{bounds:?}");
    ui.click(&mut app, bounds.center());
    assert!(ui.input(&app, "arrange-rotation").1);
    let primary = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    let mut events = vec![
        Event::Keyboard(keyboard::Event::ModifiersChanged(primary)),
        press(Key::Character("a".into()), Code::KeyA, primary, Some("a")),
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
    ];
    for (text, code) in [
        ("7", Code::Digit7),
        (".", Code::Period),
        ("5", Code::Digit5),
    ] {
        events.push(press(
            Key::Character(text.into()),
            code,
            Modifiers::empty(),
            Some(text),
        ));
    }
    for event in events {
        let (_, messages) = ui.event(&app, event, mouse::Cursor::Unavailable);
        apply(&mut app, messages);
    }
    assert_eq!(ui.input_value(&app, "arrange-rotation"), "7.5");
    assert_eq!(app.tab.doc, drawing, "Typing is only a draft");
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    apply(&mut app, messages);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, expected);
    assert_eq!(ui.input_value(&app, "arrange-rotation"), "0");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, drawing);
    assert!(!app.tab.history.can_undo());
}
