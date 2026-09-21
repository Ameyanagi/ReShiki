//! Safe entry points for the editor. Win32 ownership and pointer access stay in
//! this crate; clipboard data is bounded and validated before replacing it.
#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]

mod clipboard;
mod ole;
mod printing;

/// Enable editable Office clipboard objects for the application executable.
pub fn enable_office_embedding() {
    ole::enable();
}

/// Invalidate the previous Office acknowledgement before writing an edit.
pub fn prepare_office_save(path: &std::path::Path) {
    ole::prepare_save(path);
}

/// Wait for the Office container to accept the exact saved drawing.
/// Call on a worker thread, never on the editor event loop.
pub fn wait_for_office_save(
    path: &std::path::Path,
    bytes: &[u8],
) -> std::result::Result<(), String> {
    ole::wait_for_save(path, bytes)
}

/// Run the out-of-process OLE server on its main STA thread.
pub fn run_office_server(
    render: fn(&[u8]) -> std::result::Result<Vec<u8>, String>,
) -> std::result::Result<(), String> {
    ole::run(render).map_err(|error| error.to_string())
}

use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::w,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Owner(HWND);
impl Owner {
    fn new() -> Result<Self> {
        // SAFETY: STATIC is an OS-registered class; all string pointers are
        // static and the window is created and destroyed on this same thread.
        Ok(Self(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("ReShiki"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )?
        }))
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: uniquely owned window on its creating thread.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

/// Read/write native clipboard data using the editor's bounded JSON protocol.
pub fn clipboard(request: &[u8]) -> std::result::Result<Vec<u8>, String> {
    clipboard::invoke(request).map_err(|e| e.to_string())
}

/// Show the system print dialog, then print the immutable vector snapshot.
pub fn print(snapshot: &[u8], title: &str) -> std::result::Result<bool, String> {
    printing::show(snapshot, title).map_err(|e| e.to_string())
}

/// Render through the same Windows drawing pipeline used by the printer.
pub fn render(snapshot: &[u8], dpi: f32) -> std::result::Result<Vec<u8>, String> {
    printing::render(snapshot, dpi).map_err(|e| e.to_string())
}

/// Unknown/access-denied processes are treated as live to protect active drafts.
pub fn process_alive(pid: u32) -> bool {
    use windows::Win32::System::Threading::*;
    if pid == 0 {
        return false;
    }
    // SAFETY: read-only query, owned handle closed on every successful path.
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut code = 0;
                let result = GetExitCodeProcess(handle, &mut code);
                let _ = CloseHandle(handle);
                result.is_err() || code == STILL_ACTIVE.0 as u32
            }
            Err(error) => {
                error.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0)
            }
        }
    }
}
