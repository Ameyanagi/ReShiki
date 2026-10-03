//! Window lifecycle and a bounded, generation-checked native action queue.
use super::{App, Message};
use iced::{Subscription, Task, futures::SinkExt, window};
use reshiki::accessibility::{
    Activate, Collect, FocusControl, SetValue, Snapshot,
    tree::{NativeTree, Request},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::mpsc;

#[cfg(target_os = "macos")]
use reshiki_macos::accessibility as native;
#[cfg(windows)]
use reshiki_windows::accessibility as native;
#[cfg(not(any(target_os = "macos", windows)))]
mod native {
    #[derive(Debug, Clone, Copy)]
    pub struct Handle;
}

pub(crate) const SUPPORTED: bool = cfg!(any(target_os = "macos", windows));
static NEXT: AtomicU64 = AtomicU64::new(1);
static EVENTS: Mutex<Option<mpsc::Receiver<Action>>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub enum Action {
    Window(window::Id, window::Event),
    Geometry(window::Id, iced::Size, f32),
    Refresh,
    Snapshot(u64, u64, Snapshot),
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Installed(u64, Result<native::Handle, String>),
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Native(u64, u64, accesskit::ActionRequest),
    Dispatch(u64, Box<Message>),
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Published(u64, Result<(), String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminal {
    Close(window::Id),
    Exit,
}

struct Context {
    document: (super::document_tab::TabId, u64, u64),
    selected: Vec<u64>,
    inline_session: Option<iced::widget::Id>,
    surface: (
        super::InspectorTab,
        crate::canvas::Tool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
    ),
}

pub(super) struct State {
    generation: u64,
    window: Option<window::Id>,
    handle: Option<native::Handle>,
    installing: bool,
    install_failed: bool,
    closed: bool,
    terminal: Option<Terminal>,
    sequence: u64,
    context: Option<Context>,
    context_version: u64,
    context_epoch: Arc<AtomicU64>,
    viewport: iced::Size,
    scale: f32,
    focused: bool,
    tree: NativeTree,
    last: Option<accesskit::TreeUpdate>,
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    sender: mpsc::Sender<Action>,
}
impl Default for State {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel(64);
        if SUPPORTED
            && !cfg!(test)
            && let Ok(mut slot) = EVENTS.lock()
        {
            *slot = Some(receiver);
        }
        Self {
            generation: NEXT.fetch_add(1, Ordering::Relaxed),
            window: None,
            handle: None,
            installing: false,
            install_failed: false,
            closed: false,
            terminal: None,
            sequence: 0,
            context: None,
            context_version: 0,
            context_epoch: Arc::new(AtomicU64::new(0)),
            viewport: iced::Size::new(1280., 820.),
            scale: 1.,
            focused: false,
            tree: NativeTree::default(),
            last: None,
            sender,
        }
    }
}

#[cfg(any(target_os = "macos", windows, test))]
fn enqueue_native(
    sender: &mpsc::Sender<Action>,
    context_epoch: &AtomicU64,
    generation: u64,
    request: accesskit::ActionRequest,
) {
    // Capture the context before the request enters the queue. Reading it only
    // when App resolves the request would let an old request adopt a new edit.
    let version = context_epoch.load(Ordering::Acquire);
    if request.data.as_ref().is_some_and(
        |data| !matches!(data, accesskit::ActionData::Value(value) if value.len() <= 16384),
    ) {
        return;
    }
    let _ = sender.try_send(Action::Native(generation, version, request));
}

pub(super) fn subscription() -> Subscription<Message> {
    if !SUPPORTED {
        return Subscription::none();
    }
    Subscription::batch([
        Subscription::run(|| {
            iced::stream::channel(64, async |mut output| {
                let receiver = EVENTS.lock().ok().and_then(|mut slot| slot.take());
                if let Some(mut receiver) = receiver {
                    while let Some(action) = receiver.recv().await {
                        if output.send(Message::Accessibility(action)).await.is_err() {
                            break;
                        }
                    }
                }
            })
        }),
        iced::event::listen_with(|event, _, id| {
            let action = match event {
                iced::Event::Window(event)
                    if !matches!(
                        event,
                        window::Event::RedrawRequested(_) | window::Event::Moved(_)
                    ) =>
                {
                    Action::Window(id, event)
                }
                iced::Event::Keyboard(_)
                | iced::Event::InputMethod(_)
                | iced::Event::Mouse(
                    iced::mouse::Event::ButtonPressed(_)
                    | iced::mouse::Event::ButtonReleased(_)
                    | iced::mouse::Event::WheelScrolled { .. },
                ) => Action::Refresh,
                _ => return None,
            };
            Some(Message::Accessibility(action))
        }),
    ])
}

impl App {
    pub(super) fn accessibility_refresh(&mut self) -> Task<Message> {
        if !SUPPORTED && !cfg!(test) {
            return Task::none();
        }
        let document = (self.tab.id, self.tab.file_epoch, self.tab.revision);
        let inline_session = self
            .tab
            .inline_text
            .as_ref()
            .map(|draft| draft.session.clone());
        let surface = (
            self.inspector_tab,
            self.tool,
            self.help_open,
            self.updates.open,
            self.assistant.viewed_image.is_some(),
            self.tab.atom_text.is_some(),
            self.palette.is_some(),
            self.context_menu.is_some(),
            self.imports.menu || self.imports.examples_menu,
            self.style_menu.is_some() || self.tab.inspector_ui.menu_open(),
        );
        let state = &mut self.accessibility;
        let identity_changed = state.context.as_ref().is_none_or(|context| {
            (context.document.0, context.document.1) != (document.0, document.1)
                || context.selected != self.tab.selected
                || context.inline_session != inline_session
                || context.surface != surface
        });
        if identity_changed {
            state.context = Some(Context {
                document,
                selected: self.tab.selected.clone(),
                inline_session,
                surface,
            });
            state.context_version = state.context_version.wrapping_add(1);
            state.tree.invalidate();
            state.last = None;
        } else if let Some(context) = &mut state.context
            && context.document.2 != document.2
        {
            context.document = document;
            state.context_version = state.context_version.wrapping_add(1);
        }
        state
            .context_epoch
            .store(state.context_version, Ordering::Release);
        if !SUPPORTED || state.closed || state.window.is_none() {
            return Task::none();
        }
        state.sequence = state.sequence.wrapping_add(1);
        let (generation, sequence) = (state.generation, state.sequence);
        iced::advanced::widget::operate(Collect::new(iced::Rectangle::with_size(state.viewport)))
            .map(move |snapshot| {
                Message::Accessibility(Action::Snapshot(generation, sequence, snapshot))
            })
    }

    pub(super) fn accessibility_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Window(id, window::Event::Opened { .. }) if !self.accessibility.closed => {
                self.accessibility.window = Some(id);
                window::size(id).then(move |size| {
                    window::scale_factor(id)
                        .map(move |scale| Message::Accessibility(Action::Geometry(id, size, scale)))
                })
            }
            Action::Geometry(id, size, scale) if self.accessibility.window == Some(id) => {
                self.accessibility.viewport = size;
                self.accessibility.scale = scale;
                self.accessibility_refresh()
            }
            Action::Window(id, event) if self.accessibility.window == Some(id) => {
                match event {
                    window::Event::Resized(size) => self.accessibility.viewport = size,
                    window::Event::Rescaled(scale) => self.accessibility.scale = scale,
                    window::Event::Focused => self.accessibility.focused = true,
                    window::Event::Unfocused => self.accessibility.focused = false,
                    window::Event::Closed => {
                        self.accessibility.closed = true;
                        return Task::none();
                    }
                    _ => return Task::none(),
                }
                self.accessibility.last = None;
                self.accessibility_refresh()
            }
            Action::Refresh => self.accessibility_refresh(),
            Action::Snapshot(generation, sequence, snapshot)
                if generation == self.accessibility.generation
                    && sequence == self.accessibility.sequence
                    && !self.accessibility.closed =>
            {
                let title = self.title();
                let state = &mut self.accessibility;
                let tree = match state.tree.update(
                    &snapshot,
                    &title,
                    iced::Rectangle::with_size(state.viewport),
                    state.scale,
                ) {
                    Ok(tree) => tree,
                    Err(error) => {
                        eprintln!("Could not publish accessibility controls: {error}");
                        // Revoke native targets too, including when a duplicate or
                        // invalid control made the previous full tree unusable.
                        match state.tree.update(
                            &Snapshot::default(),
                            &title,
                            iced::Rectangle::with_size(state.viewport),
                            state.scale,
                        ) {
                            Ok(tree) => tree,
                            Err(_) => return Task::none(),
                        }
                    }
                };
                if state.last.as_ref() == Some(&tree) {
                    return Task::none();
                }
                state.last = Some(tree.clone());
                #[cfg(any(target_os = "macos", windows))]
                if let Some(id) = state.window {
                    if let Some(handle) = state.handle {
                        let focused = state.focused;
                        return window::run(id, move |_| native::update(handle, tree, focused))
                            .map(move |result| {
                                Message::Accessibility(Action::Published(generation, result))
                            });
                    }
                    if !state.installing && !state.install_failed {
                        state.installing = true;
                        let sender = state.sender.clone();
                        let context_epoch = state.context_epoch.clone();
                        return window::run(id, move |window| {
                            native::install(window, tree, move |request| {
                                // Native callbacks may run on UIA worker threads. Never
                                // block, access App, or carry unbounded text into its queue.
                                enqueue_native(&sender, &context_epoch, generation, request);
                            })
                        })
                        .map(move |result| {
                            Message::Accessibility(Action::Installed(generation, result))
                        });
                    }
                }
                Task::none()
            }
            Action::Installed(generation, result)
                if generation == self.accessibility.generation =>
            {
                self.accessibility.installing = false;
                match result {
                    Ok(handle) => self.accessibility.handle = Some(handle),
                    Err(error) => {
                        self.accessibility.install_failed = true;
                        eprintln!("Could not install native accessibility: {error}");
                    }
                }
                if self.accessibility.terminal.is_some() {
                    return self.accessibility_finish_terminal();
                }
                self.accessibility.last = None;
                // Installation runs before visibility. Even a failed adapter must
                // leave the real application usable, with a diagnostic above.
                let show = self.accessibility.window.map_or_else(Task::none, |id| {
                    window::set_mode(id, window::Mode::Windowed)
                });
                Task::batch([show, self.accessibility_refresh()])
            }
            Action::Native(generation, version, request)
                if generation == self.accessibility.generation
                    && version == self.accessibility.context_version
                    && !self.accessibility.closed =>
            {
                match self.accessibility.tree.resolve(&request) {
                    Some(Request::Activate(id)) => {
                        iced::advanced::widget::operate(Activate::<Message>::new(id)).map(
                            move |message| {
                                Message::Accessibility(Action::Dispatch(version, Box::new(message)))
                            },
                        )
                    }
                    Some(Request::SetValue(id, value)) => iced::advanced::widget::operate(
                        SetValue::<Message>::new(id, value),
                    )
                    .map(move |message| {
                        Message::Accessibility(Action::Dispatch(version, Box::new(message)))
                    }),
                    Some(Request::Focus(id)) => {
                        iced::advanced::widget::operate(FocusControl::new(id))
                            .discard()
                            .chain(Task::done(Message::Accessibility(Action::Refresh)))
                    }
                    Some(Request::Reveal(id)) => {
                        iced::advanced::widget::operate(FocusControl::reveal(id))
                            .discard()
                            .chain(Task::done(Message::Accessibility(Action::Refresh)))
                    }
                    None => Task::none(),
                }
            }
            Action::Dispatch(version, message)
                if version == self.accessibility.context_version && !self.accessibility.closed =>
            {
                self.update(*message)
            }
            Action::Published(generation, Err(error))
                if generation == self.accessibility.generation =>
            {
                eprintln!("Could not update native accessibility: {error}");
                Task::none()
            }
            _ => Task::none(),
        }
    }

    pub(super) fn accessibility_close(&mut self, id: window::Id) -> Task<Message> {
        self.accessibility.terminal = Some(Terminal::Close(id));
        self.accessibility_finish_terminal()
    }
    pub(super) fn accessibility_exit(&mut self) -> Task<Message> {
        self.accessibility.terminal = Some(Terminal::Exit);
        self.accessibility_finish_terminal()
    }
    fn accessibility_finish_terminal(&mut self) -> Task<Message> {
        self.accessibility.closed = true;
        self.accessibility.last = None;
        // An install callback can already own the adapter while its completion
        // is queued. Keep its host alive until that completion supplies the token.
        if self.accessibility.installing {
            return Task::none();
        }
        let Some(terminal) = self.accessibility.terminal.take() else {
            return Task::none();
        };
        #[cfg(any(target_os = "macos", windows))]
        if let Some(handle) = self.accessibility.handle.take()
            && let Some(id) = self.accessibility.window
        {
            return window::run(id, move |_| native::uninstall(handle)).then(move |result| {
                if let Err(error) = result {
                    eprintln!("Could not remove native accessibility: {error}");
                }
                finish_terminal(terminal)
            });
        }
        finish_terminal(terminal)
    }
}
fn finish_terminal(terminal: Terminal) -> Task<Message> {
    match terminal {
        Terminal::Close(id) => window::close(id),
        Terminal::Exit => iced::exit(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_request(
        app: &mut App,
        id: &str,
        role: reshiki::accessibility::Role,
    ) -> accesskit::ActionRequest {
        let bounds = iced::Rectangle::new(iced::Point::new(10., 10.), iced::Size::new(80., 30.));
        let snapshot = Snapshot {
            nodes: vec![reshiki::accessibility::Node {
                id: id.into(),
                name: "Test control".into(),
                role,
                enabled: true,
                focused: true,
                checked: None,
                expanded: None,
                value: None,
                bounds,
                visible_bounds: Some(bounds),
            }],
            duplicate_ids: Vec::new(),
        };
        let tree = app
            .accessibility
            .tree
            .update(
                &snapshot,
                "ReShiki",
                iced::Rectangle::with_size(app.accessibility.viewport),
                1.,
            )
            .unwrap();
        accesskit::ActionRequest {
            action: if role == reshiki::accessibility::Role::Button {
                accesskit::Action::Click
            } else {
                accesskit::Action::SetValue
            },
            target_tree: accesskit::TreeId::ROOT,
            target_node: tree.focus,
            data: if role == reshiki::accessibility::Role::Button {
                None
            } else {
                Some(accesskit::ActionData::Value("stale caption".into()))
            },
        }
    }

    #[test]
    fn native_callback_keeps_its_enqueue_context_across_a_document_revision() {
        let (mut app, _) = App::new();
        let _ = app.accessibility_refresh();
        let request = native_request(&mut app, "header-new", reshiki::accessibility::Role::Button);
        let (sender, mut receiver) = mpsc::channel(2);
        enqueue_native(
            &sender,
            &app.accessibility.context_epoch,
            app.accessibility.generation,
            request.clone(),
        );
        app.tab.revision += 1;
        let _ = app.accessibility_refresh();
        // Document edits keep the live control's ID. The enqueue stamp, not
        // accidental removal of the control, must reject this stale request.
        assert!(app.accessibility.tree.resolve(&request).is_some());
        assert_eq!(
            app.accessibility_action(receiver.try_recv().unwrap())
                .units(),
            0
        );
        enqueue_native(
            &sender,
            &app.accessibility.context_epoch,
            app.accessibility.generation,
            request,
        );
        assert!(
            app.accessibility_action(receiver.try_recv().unwrap())
                .units()
                > 0
        );
    }

    #[test]
    fn cancelled_and_reopened_caption_rejects_old_native_and_dispatched_edits() {
        use super::super::inline_text::Action as Inline;
        use reshiki::document::Point;
        let (mut app, _) = App::new();
        let _ = app.update(Message::InlineText(Inline::Begin(
            None,
            Point::new(20., 30.),
        )));
        let version = app.accessibility.context_version;
        let document = (app.tab.id, app.tab.file_epoch, app.tab.revision);
        let selected = app.tab.selected.clone();
        let tool = app.tool;
        let request = native_request(
            &mut app,
            "inline-caption",
            reshiki::accessibility::Role::TextArea,
        );
        let (sender, mut receiver) = mpsc::channel(1);
        enqueue_native(
            &sender,
            &app.accessibility.context_epoch,
            app.accessibility.generation,
            request.clone(),
        );
        let _ = app.update(Message::InlineText(Inline::Finish(false)));
        assert!(
            app.accessibility
                .context
                .as_ref()
                .unwrap()
                .inline_session
                .is_none()
        );
        let _ = app.update(Message::InlineText(Inline::Begin(
            None,
            Point::new(60., 70.),
        )));
        assert_eq!(document, (app.tab.id, app.tab.file_epoch, app.tab.revision));
        assert_eq!(selected, app.tab.selected);
        assert_eq!(tool, app.tool);
        assert_ne!(version, app.accessibility.context_version);
        let reopened = native_request(
            &mut app,
            "inline-caption",
            reshiki::accessibility::Role::TextArea,
        );
        assert_ne!(request.target_node, reopened.target_node);
        assert!(app.accessibility.tree.resolve(&request).is_none());
        assert_eq!(
            app.accessibility_action(receiver.try_recv().unwrap())
                .units(),
            0
        );
        assert_eq!(
            app.accessibility_action(Action::Dispatch(
                version,
                Box::new(Message::InlineText(Inline::ReplaceText("old draft".into())))
            ))
            .units(),
            0
        );
        assert!(app.tab.caption.is_empty());

        // A new Begin may replace an empty draft in a single update, without
        // ever publishing a context in which the editor is absent.
        let version = app.accessibility.context_version;
        let _ = app.update(Message::InlineText(Inline::Begin(
            None,
            Point::new(60., 70.),
        )));
        assert_ne!(version, app.accessibility.context_version);
        assert!(app.accessibility.tree.resolve(&reopened).is_none());
    }

    #[test]
    fn closing_during_install_waits_for_completion_before_destroying_the_host() {
        let (mut app, _) = App::new();
        let id = window::Id::unique();
        app.accessibility.window = Some(id);
        app.accessibility.installing = true;
        assert_eq!(app.accessibility_close(id).units(), 0);
        assert!(app.accessibility.closed);
        assert_eq!(app.accessibility.terminal, Some(Terminal::Close(id)));
        let generation = app.accessibility.generation;
        let task = app.accessibility_action(Action::Installed(
            generation,
            Err("injected install failure".into()),
        ));
        assert!(task.units() > 0);
        assert!(!app.accessibility.installing);
        assert!(app.accessibility.terminal.is_none());
    }
    #[test]
    fn a_failed_install_is_not_retried_on_the_visible_fallback_window() {
        let (mut app, _) = App::new();
        app.accessibility.window = Some(window::Id::unique());
        app.accessibility.installing = true;
        let generation = app.accessibility.generation;
        let _ = app.accessibility_action(Action::Installed(
            generation,
            Err("injected install failure".into()),
        ));
        assert!(app.accessibility.install_failed);
        let sequence = app.accessibility.sequence;
        let task =
            app.accessibility_action(Action::Snapshot(generation, sequence, Snapshot::default()));
        assert_eq!(task.units(), 0);
        assert!(!app.accessibility.installing);
        assert!(app.accessibility.install_failed);
    }
    #[test]
    fn queued_native_command_cannot_move_to_a_new_selection() {
        let (mut app, _) = App::new();
        let _ = app.accessibility_refresh();
        let version = app.accessibility.context_version;
        app.tab.selected.push(123);
        let _ = app.accessibility_refresh();
        assert_ne!(version, app.accessibility.context_version);
        let before = app.tab.id;
        assert_eq!(
            app.accessibility_action(Action::Dispatch(version, Box::new(Message::New)))
                .units(),
            0
        );
        assert_eq!(app.tab.id, before);
    }
    #[test]
    fn updater_exit_uses_the_same_pending_install_barrier() {
        let (mut app, _) = App::new();
        app.accessibility.installing = true;
        assert_eq!(app.accessibility_exit().units(), 0);
        assert_eq!(app.accessibility.terminal, Some(Terminal::Exit));
        let generation = app.accessibility.generation;
        assert!(
            app.accessibility_action(Action::Installed(
                generation,
                Err("injected install failure".into())
            ))
            .units()
                > 0
        );
        assert!(app.accessibility.terminal.is_none());
    }
}

#[cfg(test)]
mod renderer_tests {
    use super::*;
    use iced::advanced::{renderer::Headless, widget::operation};
    use iced_runtime::{UserInterface, user_interface::Cache};

    fn snapshot(app: &App, renderer: &mut iced::Renderer, size: iced::Size) -> Snapshot {
        let mut ui = UserInterface::build(app.view(), size, Cache::new(), renderer);
        let mut collect = Collect::new(iced::Rectangle::with_size(size));
        ui.operate(renderer, &mut operation::black_box(&mut collect));
        let snapshot = collect.snapshot().clone();
        assert!(
            snapshot.duplicate_ids.is_empty(),
            "{:?}",
            snapshot.duplicate_ids
        );
        NativeTree::default()
            .update(&snapshot, "ReShiki", iced::Rectangle::with_size(size), 2.)
            .expect("the native consumer receives a valid live tree");
        snapshot
    }

    #[tokio::test]
    #[ignore = "Opt-in real App widget tree and overlay check"]
    async fn changed_surfaces_expose_live_controls_and_mask_foreground_backgrounds() {
        let mut renderer =
            <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
                .await
                .unwrap();
        for size in [iced::Size::new(1280., 820.), iced::Size::new(1040., 680.)] {
            let (mut app, _) = App::new();
            app.viewport = size;
            let a = app
                .tab
                .doc
                .add_atom("C", reshiki::document::Point::default());
            let b = app
                .tab
                .doc
                .add_atom("O", reshiki::document::Point { x: 42., y: 42. });
            app.tab.selected = vec![a, b];
            app.sync_numeric_transforms();
            app.tab
                .inspector_ui
                .update(super::super::inspector::Action::Section(
                    super::super::inspector::Section::Transform,
                    true,
                ));
            let tree = snapshot(&app, &mut renderer, size);
            for id in [
                "header-new",
                "header-export",
                "help-open",
                "transform-rotation",
                "transform-width",
                "transform-more",
                "transform-proportions",
                "transform-apply",
            ] {
                assert!(tree.nodes.iter().any(|node| node.id == id), "missing {id}");
            }
            assert!(
                tree.nodes
                    .iter()
                    .find(|node| node.id == "transform-width")
                    .unwrap()
                    .name
                    .contains("pt")
            );
            assert_eq!(
                tree.nodes
                    .iter()
                    .find(|node| node.id == "transform-more")
                    .unwrap()
                    .expanded,
                Some(false)
            );
            assert!(!tree.nodes.iter().any(|node| node.id == "transform-tilt-x"));
            app.help_open = true;
            let help = snapshot(&app, &mut renderer, size);
            assert_eq!(help.nodes.len(), 3);
            assert!(help.nodes.iter().all(|node| node.id.starts_with("help-")));
            app.help_open = false;
            let _ = app.context_action(super::super::context_menu::Action::Open(
                super::super::context_menu::Page::Arrange,
                200.,
            ));
            let menu = snapshot(&app, &mut renderer, size);
            assert!(!menu.nodes.is_empty());
            assert!(menu.nodes.iter().all(|node| node.id.starts_with("menu-")));
            app.context_menu = None;
            app.inspector_tab = super::super::InspectorTab::Export;
            app.tab
                .inspector_ui
                .update(super::super::inspector::Action::FigureMenu(true));
            let formats = snapshot(&app, &mut renderer, size);
            assert!(!formats.nodes.is_empty());
            assert!(
                formats
                    .nodes
                    .iter()
                    .all(|node| node.id.starts_with("figure-format-"))
            );
            app.tab.inspector_ui.close_menu();
            app.tab
                .inspector_ui
                .update(super::super::inspector::Action::Section(
                    super::super::inspector::Section::ExportChemical,
                    true,
                ));
            app.tab
                .inspector_ui
                .update(super::super::inspector::Action::ChemicalMenu(true));
            let chemicals = snapshot(&app, &mut renderer, size);
            assert_eq!(chemicals.nodes.len(), 4);
            assert!(
                chemicals
                    .nodes
                    .iter()
                    .all(|node| node.id.starts_with("chemical-format-"))
            );
            app.tab.inspector_ui.close_menu();
            app.inspector_tab = super::super::InspectorTab::Import;
            app.imports.set_text("CCO");
            app.imports.examples_menu = true;
            let examples = snapshot(&app, &mut renderer, size);
            assert_eq!(examples.nodes.len(), 4);
            assert!(
                examples
                    .nodes
                    .iter()
                    .all(|node| node.id.starts_with("import-example-"))
            );
            app.imports.examples_menu = false;
            app.imports.menu = true;
            let insert = snapshot(&app, &mut renderer, size);
            assert_eq!(insert.nodes.len(), 1);
            assert_eq!(insert.nodes[0].id, "import-replace");
            app.imports.menu = false;
            let import = snapshot(&app, &mut renderer, size);
            assert!(import.nodes.iter().any(|node| node.id == "import-input"
                && node.role == reshiki::accessibility::Role::TextArea));
            let _ = app.style_menu_action(super::super::color_popover::Action::Color);
            let color = snapshot(&app, &mut renderer, size);
            assert!(color.nodes.iter().any(|node| node.id == "color-input"));
            assert!(
                color
                    .nodes
                    .iter()
                    .all(|node| node.id == "color-input" || node.id == "style-color-apply")
            );
            app.style_menu = None;
            let _ = app.inline_action(super::super::inline_text::Action::Begin(
                None,
                reshiki::document::Point::default(),
            ));
            let caption = snapshot(&app, &mut renderer, size);
            assert!(caption.nodes.iter().any(|node| node.id == "inline-caption"
                && node.role == reshiki::accessibility::Role::TextArea));
            app.finish_inline(false);
            app.inspector_tab = super::super::InspectorTab::Properties;
            app.tool = crate::canvas::Tool::Graphic(reshiki::graphics::GraphicKind::Arc);
            let arcs = snapshot(&app, &mut renderer, size);
            assert!(arcs.nodes.iter().any(|node| node.id == "arc-start"));
            assert!(arcs.nodes.iter().any(|node| node.id == "arc-sweep"));
            assert!(
                arcs.nodes
                    .iter()
                    .any(|node| node.id.starts_with("arc-preset-context-"))
            );
            assert!(
                arcs.nodes
                    .iter()
                    .any(|node| node.id.starts_with("arc-preset-inspector-"))
            );
        }
    }
}
