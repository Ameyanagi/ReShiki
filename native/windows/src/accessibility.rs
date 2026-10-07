//! Owner-thread AccessKit lifecycle. HWNDs and subclass adapters never leave
//! this module; callers receive generation tokens with checked thread identity.
use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use accesskit_windows::SubclassingAdapter;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    thread::{self, ThreadId},
};
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            GetPropW, GetWindowThreadProcessId, IsWindowVisible, RemovePropW, SetPropW,
        },
    },
    core::w,
};

#[derive(Debug, Clone, Copy)]
pub struct Handle {
    key: usize,
    owner: ThreadId,
}

/// The window property disappears on destruction. It prevents a queued update
/// from targeting an unrelated window that reuses the numeric HWND later.
struct Lifetime {
    hwnd: HWND,
    key: usize,
}
impl Lifetime {
    fn matches(&self) -> bool {
        // SAFETY: Read-only Win32 handle query on the owner thread. GetPropW
        // safely returns null for a destroyed window; no pointer is dereferenced.
        unsafe {
            GetPropW(self.hwnd, w!("ReShiki.Accessibility.Generation")).0 as usize == self.key
        }
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        if self.matches() {
            // SAFETY: This exact property was installed by this owner-thread
            // lifetime and still contains its unique, non-reused generation.
            unsafe {
                let _ = RemovePropW(self.hwnd, w!("ReShiki.Accessibility.Generation"));
            }
        }
    }
}

struct Entry {
    // Field order restores the prior window procedure before removing our
    // lifetime property. AccessKit handles WM_NCDESTROY as a no-op Drop fallback.
    adapter: SubclassingAdapter,
    lifetime: Lifetime,
    full_tree: Rc<RefCell<TreeUpdate>>,
}

#[derive(Default)]
struct Registry {
    entries: BTreeMap<usize, Entry>,
}
thread_local! { static ADAPTERS: RefCell<Registry> = RefCell::new(Registry::default()); }
// HWNDs can be reused across GUI threads. A thread-local counter would allow
// an old adapter's Drop to remove a new thread's identically numbered marker.
static NEXT_GENERATION: AtomicUsize = AtomicUsize::new(1);

fn generation(counter: &AtomicUsize) -> Result<usize, String> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| "Accessibility window identifiers exhausted".into())
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
    if thread::current().id() != handle.owner {
        return Err("Accessibility must be updated on the window's owner thread".into());
    }
    Ok(())
}

/// Install on the window's owning thread while the real Iced window is hidden.
/// The action callback may run on another thread: it must only enqueue a
/// bounded request, never dereference editor state or synchronously wait for UI.
pub fn install<W: HasWindowHandle + ?Sized>(
    window: &W,
    initial_tree: TreeUpdate,
    send_action: impl Fn(ActionRequest) + Send + 'static,
) -> Result<Handle, String> {
    let borrowed = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(raw) = borrowed.as_raw() else {
        return Err("Accessibility requires a Win32 window handle".into());
    };
    let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
    // SAFETY: The borrowed WindowHandle guarantees this HWND is valid for the
    // call. These queries do not mutate the window or retain borrowed pointers.
    let (owner_thread, current_thread, visible, installed, existing) = unsafe {
        (
            GetWindowThreadProcessId(hwnd, None),
            GetCurrentThreadId(),
            IsWindowVisible(hwnd).as_bool(),
            GetPropW(hwnd, w!("AccessKitAdapter")),
            GetPropW(hwnd, w!("ReShiki.Accessibility.Generation")),
        )
    };
    if owner_thread != current_thread {
        return Err("Accessibility must be installed on the window's owner thread".into());
    }
    if visible {
        return Err("Accessibility must be installed before the window is shown".into());
    }
    if !installed.0.is_null() || !existing.0.is_null() {
        return Err("Accessibility is already installed on this window".into());
    }
    ADAPTERS.with(|slot| {
        let mut registry = slot
            .try_borrow_mut()
            .map_err(|_| "Accessibility installation is already in progress")?;
        if registry.entries.len() >= 16 {
            return Err("Too many accessibility windows".into());
        }
        let key = generation(&NEXT_GENERATION)?;
        // SAFETY: The HWND is borrowed, hidden and owned by this thread. This
        // property stores an opaque integer, not a dereferenceable allocation;
        // Lifetime removes it after the adapter has unhooked the window.
        unsafe {
            SetPropW(
                hwnd,
                w!("ReShiki.Accessibility.Generation"),
                HANDLE(key as *mut std::ffi::c_void),
            )
        }
        .map_err(|error| format!("Could not mark accessibility window: {error}"))?;
        let lifetime = Lifetime { hwnd, key };
        let full_tree = Rc::new(RefCell::new(initial_tree));
        // AccessKit uses windows0.62; only the opaque HWND value crosses the
        // windows-rs version boundary, never a COM object or borrowed wrapper.
        let adapter = SubclassingAdapter::new(
            accesskit_windows::HWND(raw.hwnd.get() as *mut std::ffi::c_void),
            InitialTree(full_tree.clone()),
            Actions(send_action),
        );
        registry.entries.insert(
            key,
            Entry {
                adapter,
                lifetime,
                full_tree,
            },
        );
        Ok(Handle {
            key,
            owner: thread::current().id(),
        })
    })
}

/// Update a complete validated tree. The adapter receives native window focus
/// directly from WM_SETFOCUS/WM_KILLFOCUS, so `window_focused` is not duplicated.
pub fn update(handle: Handle, tree: TreeUpdate, _window_focused: bool) -> Result<(), String> {
    check_thread(handle)?;
    let events = ADAPTERS.with(|slot| {
        let mut registry = slot
            .try_borrow_mut()
            .map_err(|_| "Accessibility update is already in progress")?;
        let entry = registry
            .entries
            .get_mut(&handle.key)
            .ok_or("Accessibility window has closed")?;
        if !entry.lifetime.matches() {
            return Err("Accessibility window has been destroyed".into());
        }
        *entry
            .full_tree
            .try_borrow_mut()
            .map_err(|_| "Accessibility tree is being queried")? = tree.clone();
        Ok::<_, String>(entry.adapter.update_if_active(|| tree))
    })?;
    // Raising UIA events can synchronously call a provider. No adapter, tree,
    // registry or editor-state borrow/lock may be held across this boundary.
    if let Some(events) = events {
        events.raise();
    }
    Ok(())
}

/// Call on the owning thread before iced closes the host window. The take is
/// idempotent and the adapter is dropped outside the registry borrow.
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

/// Owner-thread fallback after the event loop returns. Only this thread's
/// registry is touched; WM_NCDESTROY makes already-destroyed HWNDs safe to drop.
pub fn shutdown() -> Result<(), String> {
    let entries = ADAPTERS.with(|slot| {
        slot.try_borrow_mut()
            .map(|mut registry| std::mem::take(&mut registry.entries))
            .map_err(|_| "Accessibility teardown is already in progress".to_string())
    })?;
    drop(entries);
    Ok(())
}

#[cfg(test)]
mod tests;
