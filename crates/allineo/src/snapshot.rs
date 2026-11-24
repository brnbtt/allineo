//! Captures everything the hook needs for one drag: the target's frame
//! insets, the visible frames of the other windows and the monitor layout.

use std::ptr::null_mut;

use allineo_core::shared::{MAX_WINDOWS, MonitorRects, Snapshot};
use allineo_core::{Config, Insets, Rect};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible,
    IsZoomed, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_THICKFRAME,
};
use windows_sys::core::BOOL;

use crate::win::{from_win, rect_zero};

const MIN_SIZE: i32 = 40;
const IGNORED_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

pub fn capture(target: HWND, config: &Config) -> Snapshot {
    let mut s = Snapshot::empty();
    s.target_hwnd = target as u64;

    // SAFETY: plain Win32 queries on a window handle; failures leave zeros.
    unsafe {
        let mut wr = rect_zero();
        GetWindowRect(target, &mut wr);
        let outer = from_win(wr);
        s.insets = Insets::between(outer, frame_bounds(target).unwrap_or(outer));

        let monitor = MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST);
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        s.config = config.scaled(dpi_x);

        EnumDisplayMonitors(null_mut(), std::ptr::null(), Some(monitor_cb), &mut s as *mut Snapshot as LPARAM);

        let mut ctx = WindowCtx { target, windows: Vec::with_capacity(96) };
        EnumWindows(Some(window_cb), &mut ctx as *mut WindowCtx as LPARAM);

        // EnumWindows walks top-down. Skip windows that are completely covered by any window above
        // them (snappable or not: a full-screen video or game hides everything behind it), since
        // snapping to something you can't see is confusing.
        for (i, w) in ctx.windows.iter().enumerate() {
            let Some(f) = w.frame else { continue };
            let hidden = ctx.windows[..i].iter().any(|above| above.cover.is_some_and(|c| c.contains(&f)));
            if !hidden && !s.push_window(f) {
                break;
            }
        }
    }
    s
}

/// Visible frame (without the invisible resize borders) in physical pixels.
fn frame_bounds(hwnd: HWND) -> Option<Rect> {
    let mut r = rect_zero();
    // SAFETY: out-pointer to a RECT of the size we pass.
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut r as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
    };
    (hr >= 0).then(|| from_win(r)).filter(|r| !r.is_empty())
}

struct WindowCtx {
    target: HWND,
    windows: Vec<Seen>,
}

/// One top-level window, in z-order.
struct Seen {
    /// Its frame, if it's something to snap to.
    frame: Option<Rect>,
    /// What it hides behind it, if it's opaque and on screen (snappable or not).
    cover: Option<Rect>,
}

unsafe extern "system" fn window_cb(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the &mut WindowCtx passed to EnumWindows.
    let ctx = unsafe { &mut *(data as *mut WindowCtx) };
    if hwnd == ctx.target || unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 || cloaked(hwnd) {
        return 1;
    }
    let seen = Seen { frame: candidate_frame(hwnd), cover: cover(hwnd) };
    if seen.frame.is_some() || seen.cover.is_some() {
        ctx.windows.push(seen);
    }
    (ctx.windows.len() < MAX_WINDOWS * 4) as BOOL
}

fn cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: out-pointer to a u32, as the attribute expects.
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&mut cloaked as *mut u32).cast(), size_of::<u32>() as u32)
    };
    cloaked != 0
}

/// The area a visible window hides. Click-through overlays (FPS counters, recording frames) and
/// layered windows (often fully or partly transparent, like screen dimmers) hide nothing.
fn cover(hwnd: HWND) -> Option<Rect> {
    // SAFETY: read-only queries on a window handle.
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if ex & (WS_EX_TRANSPARENT | WS_EX_LAYERED) != 0 {
        return None;
    }
    frame_bounds(hwnd).or_else(|| {
        let mut r = rect_zero();
        (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then(|| from_win(r)).filter(|r| !r.is_empty())
    })
}

/// Visible, not minimised, not cloaked: checked by the caller.
fn candidate_frame(hwnd: HWND) -> Option<Rect> {
    // SAFETY: read-only queries on a window handle.
    unsafe {
        if IsZoomed(hwnd) != 0 {
            return None;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_CAPTION != WS_CAPTION && style & WS_THICKFRAME == 0 {
            return None;
        }
        if ex & WS_EX_TOOLWINDOW != 0 && ex & WS_EX_APPWINDOW == 0 {
            return None;
        }
        if ex & (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE) != 0 {
            return None;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if IGNORED_CLASSES.contains(&class.as_str()) {
            return None;
        }
    }
    frame_bounds(hwnd).filter(|f| f.width() >= MIN_SIZE && f.height() >= MIN_SIZE)
}

unsafe extern "system" fn monitor_cb(monitor: HMONITOR, _hdc: HDC, _clip: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the &mut Snapshot passed to EnumDisplayMonitors.
    let s = unsafe { &mut *(data as *mut Snapshot) };
    // SAFETY: zeroed MONITORINFO with cbSize set is a valid input.
    let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut mi) } != 0 {
        s.push_monitor(MonitorRects { bounds: from_win(mi.rcMonitor), work: from_win(mi.rcWork) });
    }
    1
}
