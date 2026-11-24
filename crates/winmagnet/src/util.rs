use magnet_core::Rect;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

/// Debug-build logging to stderr; compiled out of release builds.
macro_rules! log {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            eprintln!("[winmagnet] {}", format_args!($($arg)*));
        }
    };
}
pub(crate) use log;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub const fn rect_zero() -> RECT {
    RECT { left: 0, top: 0, right: 0, bottom: 0 }
}

pub const fn from_win(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

pub fn error_box(msg: &str) {
    let text = wide(msg);
    let title = wide("WinMagnet");
    // SAFETY: valid, NUL-terminated strings.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
}
