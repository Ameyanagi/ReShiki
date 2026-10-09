//! Window lifecycle and a bounded, generation-checked native action queue.
#[cfg(test)]
mod point_controls_tests;

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

fn window_geometry(id: window::Id) -> Task<Message> {
    window::size(id).then(move |size| {
        window::scale_factor(id)
            .map(move |scale| Message::Accessibility(Action::Geometry(id, size, scale)))
    })
}

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
    keyboard_target: Option<String>,
    optimization: Option<super::optimization::AccessibilityContext>,
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
    #[cfg(windows)]
    fitting: bool,
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
            #[cfg(windows)]
            fitting: false,
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
        let keyboard_target = self.keyboard_drawing_active().then(|| {
            format!(
                "{} · {:?}",
                self.tab.keyboard_drawing.active_label(&self.tab.doc),
                self.tab.keyboard_drawing.marked()
            )
        });
        let optimization = self
            .tab
            .optimization
            .as_ref()
            .map(super::optimization::Session::accessibility_context);
        let state = &mut self.accessibility;
        let identity_changed = state.context.as_ref().is_none_or(|context| {
            (context.document.0, context.document.1) != (document.0, document.1)
                || context.selected != self.tab.selected
                || context.keyboard_target != keyboard_target
                || context.optimization != optimization
                || context.inline_session != inline_session
                || context.surface != surface
        });
        if identity_changed {
            state.context = Some(Context {
                document,
                selected: self.tab.selected.clone(),
                keyboard_target,
                optimization,
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
        #[cfg(windows)]
        if state.fitting {
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
                #[cfg(windows)]
                {
                    self.accessibility.fitting = true;
                    crate::window_fit::fit(id, true).chain(window_geometry(id))
                }
                #[cfg(not(windows))]
                window_geometry(id)
            }
            Action::Geometry(id, size, scale) if self.accessibility.window == Some(id) => {
                #[cfg(windows)]
                {
                    self.accessibility.fitting = false;
                }
                self.accessibility.viewport = size;
                self.accessibility.scale = scale;
                self.accessibility_refresh()
            }
            Action::Window(id, event) if self.accessibility.window == Some(id) => {
                match event {
                    window::Event::Resized(size) => self.accessibility.viewport = size,
                    window::Event::Rescaled(scale) => {
                        self.accessibility.scale = scale;
                        #[cfg(windows)]
                        if !self.accessibility.fitting {
                            self.accessibility.fitting = true;
                            return crate::window_fit::fit(id, false).chain(window_geometry(id));
                        }
                    }
                    window::Event::Focused => self.accessibility.focused = true,
                    window::Event::Unfocused => self.accessibility.focused = false,
                    window::Event::Closed => {
                        self.accessibility.closed = true;
                        return Task::none();
                    }
                    _ => return Task::none(),
                }
                #[cfg(windows)]
                if self.accessibility.fitting {
                    // A native resize may arrive before the startup fit's task
                    // finishes. Publish/install only after size and position fit.
                    return Task::none();
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
                #[cfg(windows)]
                if self.accessibility.fitting {
                    return Task::none();
                }
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
mod tests;

#[cfg(test)]
mod renderer_tests;
