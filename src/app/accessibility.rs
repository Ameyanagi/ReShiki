//! Window lifecycle and a bounded, generation-checked native action queue.
use super::{App, Message};
use iced::{Subscription, Task, futures::SinkExt, window};
use reshiki::accessibility::{
    Activate, Collect, FocusControl, SetValue, Snapshot,
    tree::{NativeTree, Request},
};
use std::sync::{
    Mutex,
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
    Installed(u64, Result<native::Handle, String>),
    Native(u64, accesskit::ActionRequest),
    Published(u64, Result<(), String>),
}

pub(super) struct State {
    generation: u64,
    window: Option<window::Id>,
    handle: Option<native::Handle>,
    installing: bool,
    closed: bool,
    sequence: u64,
    viewport: iced::Size,
    scale: f32,
    focused: bool,
    tree: NativeTree,
    last: Option<accesskit::TreeUpdate>,
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
            closed: false,
            sequence: 0,
            viewport: iced::Size::new(1280., 820.),
            scale: 1.,
            focused: false,
            tree: NativeTree::default(),
            last: None,
            sender,
        }
    }
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
        let state = &mut self.accessibility;
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
                    if !state.installing {
                        state.installing = true;
                        let sender = state.sender.clone();
                        return window::run(id, move |window| {
                            native::install(window, tree, move |request| {
                                // Native callbacks may run on UIA worker threads. Never
                                // block, access App, or carry unbounded text into its queue.
                                if request.data.as_ref().is_some_and(|data| {
                                    !matches!(data,
                                accesskit::ActionData::Value(value) if value.len() <= 16384)
                                }) {
                                    return;
                                }
                                let _ = sender.try_send(Action::Native(generation, request));
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
                if generation == self.accessibility.generation && !self.accessibility.closed =>
            {
                self.accessibility.installing = false;
                match result {
                    Ok(handle) => self.accessibility.handle = Some(handle),
                    Err(error) => eprintln!("Could not install native accessibility: {error}"),
                }
                // Installation runs before visibility. Even a failed adapter must
                // leave the real application usable, with a diagnostic above.
                self.accessibility.window.map_or_else(Task::none, |id| {
                    window::set_mode(id, window::Mode::Windowed)
                })
            }
            Action::Native(generation, request)
                if generation == self.accessibility.generation && !self.accessibility.closed =>
            {
                match self.accessibility.tree.resolve(&request) {
                    Some(Request::Activate(id)) => {
                        iced::advanced::widget::operate(Activate::<Message>::new(id))
                    }
                    Some(Request::SetValue(id, value)) => {
                        iced::advanced::widget::operate(SetValue::<Message>::new(id, value))
                    }
                    Some(Request::Focus(id)) => {
                        iced::advanced::widget::operate(FocusControl::new(id))
                            .chain(Task::done(Message::Accessibility(Action::Refresh)))
                    }
                    Some(Request::Reveal(id)) => {
                        iced::advanced::widget::operate(FocusControl::reveal(id))
                            .chain(Task::done(Message::Accessibility(Action::Refresh)))
                    }
                    None => Task::none(),
                }
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
        self.accessibility.closed = true;
        self.accessibility.last = None;
        #[cfg(any(target_os = "macos", windows))]
        if let Some(handle) = self.accessibility.handle.take() {
            // Restore the native class/window procedure while its window lives.
            return window::run(id, move |_| native::uninstall(handle)).then(move |result| {
                if let Err(error) = result {
                    eprintln!("Could not remove native accessibility: {error}");
                }
                window::close(id)
            });
        }
        window::close(id)
    }
}
