//! Physical monitor/window measurements. Calls stay on the HWND's owner thread.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    System::Threading::GetCurrentThreadId,
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            GetClientRect, GetWindowRect, GetWindowThreadProcessId, SWP_NOACTIVATE, SWP_NOSIZE,
            SWP_NOZORDER, SetWindowPos,
        },
    },
};

#[derive(Debug, Clone, Copy)]
pub struct Measurements {
    pub work: [i32; 4],
    pub outer: [i32; 4],
    pub client: [i32; 2],
    pub dpi: u32,
}

fn handle(window: &(impl HasWindowHandle + ?Sized)) -> Result<HWND, String> {
    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(raw) = handle.as_raw() else {
        return Err("Window sizing requires a Win32 window".into());
    };
    let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
    // SAFETY: The borrowed window guarantees a valid HWND for this call. These
    // queries only check ownership; the handle never escapes the public call.
    if unsafe { GetWindowThreadProcessId(hwnd, None) != GetCurrentThreadId() } {
        return Err("Window sizing must run on the window's owner thread".into());
    }
    Ok(hwnd)
}

/// Read physical work-area and frame dimensions at the actual window's DPI.
pub fn measurements(window: &(impl HasWindowHandle + ?Sized)) -> Result<Measurements, String> {
    let hwnd = handle(window)?;
    let mut outer = RECT::default();
    let mut client = RECT::default();
    let mut monitor = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: The borrowed HWND is valid and owned by this thread. Output
    // structures have their required sizes and are not retained by Windows.
    let dpi = unsafe {
        GetWindowRect(hwnd, &mut outer).map_err(|error| error.to_string())?;
        GetClientRect(hwnd, &mut client).map_err(|error| error.to_string())?;
        if !GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .as_bool()
        {
            return Err("Could not read the window's monitor work area".into());
        }
        GetDpiForWindow(hwnd)
    };
    if dpi == 0 {
        return Err("Could not read the window's DPI".into());
    }
    let work = monitor.rcWork;
    Ok(Measurements {
        work: [work.left, work.top, work.right, work.bottom],
        outer: [outer.left, outer.top, outer.right, outer.bottom],
        client: [client.right - client.left, client.bottom - client.top],
        dpi,
    })
}

/// Place the outer frame in physical screen coordinates, including monitors
/// with negative origins. Size/constraints remain owned by Iced/winit.
pub fn position(window: &(impl HasWindowHandle + ?Sized), [x, y]: [i32; 2]) -> Result<(), String> {
    let hwnd = handle(window)?;
    let mut current = RECT::default();
    // SAFETY: The HWND is borrowed on its owner thread. This changes only its
    // position, without activating it, changing its size or reordering windows.
    unsafe {
        GetWindowRect(hwnd, &mut current).map_err(|error| error.to_string())?;
        if current.left != x || current.top != y {
            SetWindowPos(
                hwnd,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
