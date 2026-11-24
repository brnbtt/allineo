//! Captures everything the hook needs for one drag: the target's frame
//! insets, the visible frames of the other windows and the monitor layout.

use std::ptr::null_mut;

use magnet_core::shared::{MAX_WINDOWS, MonitorRects, Snapshot};
use magnet_core::{Config, Insets, Rect};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible,
    IsZoomed, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_THICKFRAME,
};
use windows_sys::core::BOOL;

use crate::util::{from_win, rect_zero};

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

        let mut ctx = WindowCtx { target, frames: Vec::with_capacity(64) };
        EnumWindows(Some(window_cb), &mut ctx as *mut WindowCtx as LPARAM);

        // EnumWindows walks top-down, so skip windows that are completely
        // covered by one above them: snapping to something you can't see is
        // confusing.
        for (i, f) in ctx.frames.iter().enumerate() {
            if !ctx.frames[..i].iter().any(|above| above.contains(f)) && !s.push_window(*f) {
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
    frames: Vec<Rect>,
}

unsafe extern "system" fn window_cb(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the &mut WindowCtx passed to EnumWindows.
    let ctx = unsafe { &mut *(data as *mut WindowCtx) };
    if let Some(f) = candidate_frame(hwnd, ctx.target) {
        ctx.frames.push(f);
    }
    (ctx.frames.len() < MAX_WINDOWS * 2) as BOOL
}

fn candidate_frame(hwnd: HWND, target: HWND) -> Option<Rect> {
    if hwnd == target {
        return None;
    }
    // SAFETY: read-only queries on a window handle.
    unsafe {
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 || IsZoomed(hwnd) != 0 {
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
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&mut cloaked as *mut u32).cast(), size_of::<u32>() as u32);
        if cloaked != 0 {
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
