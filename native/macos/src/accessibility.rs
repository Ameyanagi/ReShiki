//! Owner-thread AccessKit lifecycle. Only opaque tokens leave this module;
//! retained AppKit objects and adapters stay on the main thread until Drop.
use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use accesskit_macos::SubclassingAdapter;
use objc2_app_kit::NSView;
use objc2_foundation::MainThreadMarker;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    thread::{self, ThreadId},
};

/// An identity, not an owning native pointer. Every operation checks its thread.
#[derive(Debug, Clone, Copy)]
pub struct Handle {
    key: usize,
    owner: ThreadId,
}

struct Entry {
    view_identity: usize,
    adapter: SubclassingAdapter,
    full_tree: Rc<RefCell<TreeUpdate>>,
}

#[derive(Default)]
struct Registry {
    next: usize,
    entries: BTreeMap<usize, Entry>,
}

thread_local! {
    static ADAPTERS: RefCell<Registry> = RefCell::new(Registry::default());
}

struct InitialTree(Rc<RefCell<TreeUpdate>>);
impl ActivationHandler for InitialTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.try_borrow().ok().map(|tree| tree.clone())
    }
}

struct Actions<F>(F);
impl<F: Fn(ActionRequest)> ActionHandler for Actions<F> {
    fn do_action(&mut self, request: ActionRequest) {
        (self.0)(request);
    }
}

fn check_thread(handle: Handle) -> Result<(), String> {
    if thread::current().id() != handle.owner || MainThreadMarker::new().is_none() {
        return Err("Accessibility must be updated on the window's main thread".into());
    }
    Ok(())
}

/// Install while the real Iced window is still hidden. The borrowed handle
/// remains valid throughout the call. AccessKit retains the NSView before the
/// borrow ends; neither the borrow nor its raw pointer escapes as a public API.
/// `send_action` must enqueue without blocking or directly calling App code.
pub fn install<W: HasWindowHandle + ?Sized>(
    window: &W,
    initial_tree: TreeUpdate,
    send_action: impl Fn(ActionRequest) + Send + 'static,
) -> Result<Handle, String> {
    let _main =
        MainThreadMarker::new().ok_or("Accessibility must be installed on the main thread")?;
    let borrowed = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(raw) = borrowed.as_raw() else {
        return Err("Accessibility requires an AppKit window handle".into());
    };
    let identity = raw.ns_view.as_ptr() as usize;
    // SAFETY: WindowHandle's borrowed AppKit handle guarantees a valid NSView
    // for this borrow; this function has verified the main thread. The
    // reference is used only within this call, never stored or sent elsewhere.
    let view = unsafe { raw.ns_view.cast::<NSView>().as_ref() };
    let native_window = view.window().ok_or("Accessibility view has no window")?;
    if native_window.isVisible() {
        return Err("Accessibility must be installed before the window is shown".into());
    }
    ADAPTERS.with(|slot| {
        let mut registry = slot
            .try_borrow_mut()
            .map_err(|_| "Accessibility installation is already in progress")?;
        if registry
            .entries
            .values()
            .any(|entry| entry.view_identity == identity)
        {
            return Err("Accessibility is already installed on this view".into());
        }
        if registry.entries.len() >= 16 {
            return Err("Too many accessibility windows".into());
        }
        let key = registry
            .next
            .checked_add(1)
            .ok_or("Accessibility window identifiers exhausted")?;
        let full_tree = Rc::new(RefCell::new(initial_tree));
        // SAFETY: The borrowed handle and main-thread check above establish
        // a live NSView. The window is hidden and this registry prevents a
        // second adapter on the same view. AccessKit retains the view and
        // restores its original class before releasing it in adapter Drop.
        let adapter = unsafe {
            SubclassingAdapter::new(
                raw.ns_view.as_ptr(),
                InitialTree(full_tree.clone()),
                Actions(send_action),
            )
        };
        registry.next = key;
        registry.entries.insert(
            key,
            Entry {
                view_identity: identity,
                adapter,
                full_tree,
            },
        );
        Ok(Handle {
            key,
            owner: thread::current().id(),
        })
    })
}

/// Apply a complete, validated live tree. Native notifications are raised only
/// after releasing our registry and tree borrows, permitting native re-entry.
pub fn update(handle: Handle, tree: TreeUpdate, window_focused: bool) -> Result<(), String> {
    check_thread(handle)?;
    let (tree_events, focus_events) = ADAPTERS.with(|slot| {
        let mut registry = slot
            .try_borrow_mut()
            .map_err(|_| "Accessibility update is already in progress")?;
        let entry = registry
            .entries
            .get_mut(&handle.key)
            .ok_or("Accessibility window has closed")?;
        *entry
            .full_tree
            .try_borrow_mut()
            .map_err(|_| "Accessibility tree is being queried")? = tree.clone();
        let tree_events = entry.adapter.update_if_active(|| tree);
        let focus_events = entry.adapter.update_view_focus_state(window_focused);
        Ok::<_, String>((tree_events, focus_events))
    })?;
    if let Some(events) = tree_events {
        events.raise();
    }
    if let Some(events) = focus_events {
        events.raise();
    }
    Ok(())
}

/// Call through `iced::window::run` before closing the host window. Repeated
/// calls are harmless. Dropping outside the registry borrow allows re-entry.
pub fn uninstall(handle: Handle) -> Result<(), String> {
    check_thread(handle)?;
    let entry = ADAPTERS.with(|slot| {
        slot.try_borrow_mut()
            .map(|mut registry| registry.entries.remove(&handle.key))
            .map_err(|_| "Accessibility teardown is already in progress".to_string())
    })?;
    drop(entry);
    Ok(())
}

/// Main-thread fallback after the event loop returns, including startup error.
pub fn shutdown() -> Result<(), String> {
    let _main = MainThreadMarker::new().ok_or("Accessibility must shut down on the main thread")?;
    let entries = ADAPTERS.with(|slot| {
        slot.try_borrow_mut()
            .map(|mut registry| std::mem::take(&mut registry.entries))
            .map_err(|_| "Accessibility teardown is already in progress".to_string())
    })?;
    drop(entries);
    Ok(())
}
