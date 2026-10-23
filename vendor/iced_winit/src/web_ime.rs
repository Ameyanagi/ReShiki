//! ReShiki patch: input methods in the browser.
//!
//! winit's web backend implements no input method: it ignores
//! `set_ime_allowed` and `set_ime_cursor_area` and never sends
//! [`winit::event::Ime`] events, because a canvas cannot receive composed
//! text. The page can, with an editable element of its own. This module
//! tells the page where a text widget wants input, and takes the page's
//! composition events back into Iced as winit `Ime` events, so text widgets
//! show and commit them as on the desktop.
use crate::core::Rectangle;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Weak};

type Hook = Box<dyn Fn(Option<Rectangle>)>;

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static QUEUE: RefCell<VecDeque<winit::event::Ime>> = const { RefCell::new(VecDeque::new()) };
    static WINDOW: RefCell<Weak<winit::window::Window>> = const { RefCell::new(Weak::new()) };
}

/// Call `hook` with the text cursor's area, in logical pixels from the
/// canvas's top left corner, whenever a widget starts taking text or its
/// cursor moves, and with `None` when no widget takes text.
pub fn on_input_method(hook: impl Fn(Option<Rectangle>) + 'static) {
    HOOK.with_borrow_mut(|current| *current = Some(Box::new(hook)));
}

/// Stop calling the hook given to [`on_input_method`].
pub fn clear_input_method() {
    HOOK.with_borrow_mut(|current| *current = None);
}

/// Hand Iced an input method event, as winit would on the desktop. It
/// reaches the window with its next event, which this requests.
pub fn send(event: winit::event::Ime) {
    QUEUE.with_borrow_mut(|queue| queue.push_back(event));
    if let Some(window) = WINDOW.with_borrow(Weak::upgrade) {
        window.request_redraw();
    }
}

/// The window that [`send`] wakes.
pub(crate) fn set_window(window: &Arc<winit::window::Window>) {
    WINDOW.with_borrow_mut(|current| *current = Arc::downgrade(window));
}

/// The events [`send`] queued, oldest first.
pub(crate) fn take() -> Vec<winit::event::Ime> {
    QUEUE.with_borrow_mut(|queue| queue.drain(..).collect())
}

/// Tell the page where text input goes, or that none does.
pub(crate) fn input_area(area: Option<Rectangle>) {
    HOOK.with_borrow(|hook| {
        if let Some(hook) = hook {
            hook(area);
        }
    });
}
