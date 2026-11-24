//! Small Win32 helpers, and how the settings window talks to the background agent.

use std::path::Path;
use std::ptr::{null, null_mut};
use std::time::Duration;

use allineo_core::Rect;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, RECT, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetWindowTextW, GetWindowThreadProcessId, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
    MB_ICONQUESTION, MB_YESNO, MessageBoxW, PostMessageW, SW_RESTORE, SetForegroundWindow, ShowWindow, WM_APP,
};
use windows_sys::core::BOOL;

use crate::paths::APP;

/// Window class of the agent's hidden window (also its tray icon's owner).
pub const AGENT_CLASS: &str = "allineo.agent";
/// Settings changed: re-read `config.json`.
pub const WM_AGENT_RELOAD: u32 = WM_APP + 10;
/// Exit (uninstall, update).
pub const WM_AGENT_QUIT: u32 = WM_APP + 11;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub const fn rect_zero() -> RECT {
    RECT { left: 0, top: 0, right: 0, bottom: 0 }
}

pub const fn from_win(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

pub fn error_box(text: &str) {
    unsafe {
        MessageBoxW(null_mut(), wide(text).as_ptr(), wide(APP).as_ptr(), MB_ICONERROR);
    }
}

pub fn info_box(text: &str) {
    unsafe {
        MessageBoxW(null_mut(), wide(text).as_ptr(), wide(APP).as_ptr(), MB_ICONINFORMATION);
    }
}

pub fn confirm(text: &str) -> bool {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide(APP).as_ptr(), MB_YESNO | MB_ICONQUESTION) == IDYES }
}

/// Opts out of legacy injection (global window hooks, AppInit DLLs). Overlays like RivaTuner /
/// MSI Afterburner use these to draw FPS counters into anything that renders with the GPU,
/// which is wrong for a small utility window. Must run before any window is created.
pub fn block_overlay_injection() {
    use windows_sys::Win32::System::Threading::{ProcessExtensionPointDisablePolicy, SetProcessMitigationPolicy};
    const DISABLE_EXTENSION_POINTS: u32 = 1;
    unsafe {
        SetProcessMitigationPolicy(
            ProcessExtensionPointDisablePolicy,
            &DISABLE_EXTENSION_POINTS as *const u32 as *const _,
            std::mem::size_of::<u32>(),
        );
    }
}

/// Blends the native title bar into the window: dark caption in our background color,
/// a hairline border and rounded corners (Windows 11; ignored on older versions).
pub fn style_title_bar(hwnd: *mut core::ffi::c_void, caption: u8, border: u8, text: u8) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    const DARK_MODE: u32 = 20;
    const CORNERS: u32 = 33;
    const BORDER_COLOR: u32 = 34;
    const CAPTION_COLOR: u32 = 35;
    const TEXT_COLOR: u32 = 36;
    let rgb = |g: u8| g as u32 | (g as u32) << 8 | (g as u32) << 16;
    let set = |attr: u32, value: u32| unsafe {
        DwmSetWindowAttribute(hwnd, attr, &value as *const u32 as *const _, 4);
    };
    set(DARK_MODE, 1);
    set(CORNERS, 2);
    set(BORDER_COLOR, rgb(border));
    set(CAPTION_COLOR, rgb(caption));
    set(TEXT_COLOR, rgb(text));
}

// ---------------------------------------------------------------------------------------------
// The background agent
// ---------------------------------------------------------------------------------------------

fn agent_window() -> HWND {
    unsafe { FindWindowW(wide(AGENT_CLASS).as_ptr(), null()) }
}

pub fn agent_running() -> bool {
    !agent_window().is_null()
}

/// Tells a running agent to pick up new settings.
pub fn reload_agent() {
    let hwnd = agent_window();
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, WM_AGENT_RELOAD, 0, 0) };
    }
}

/// Asks the agent to exit and waits until its process is gone (so its exe can be replaced).
pub fn stop_agent() {
    let hwnd = agent_window();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        PostMessageW(hwnd, WM_AGENT_QUIT, 0, 0);
        if !process.is_null() {
            let _ = WaitForSingleObject(process, 3000) == WAIT_OBJECT_0;
            CloseHandle(process);
        } else {
            for _ in 0..30 {
                if !agent_running() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

pub fn start_agent(exe: &Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    // Null stdio: the agent outlives us and mustn't hold on to our parent's pipes.
    Command::new(exe)
        .arg("agent")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

// ---------------------------------------------------------------------------------------------
// One settings window at a time
// ---------------------------------------------------------------------------------------------

pub struct SingleSettings(HANDLE);

impl SingleSettings {
    pub fn acquire() -> Option<Self> {
        unsafe {
            let h = CreateMutexW(null(), 0, wide("Local\\allineo.settings").as_ptr());
            if h.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(h);
                return None;
            }
            Some(Self(h))
        }
    }
}

impl Drop for SingleSettings {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Brings the already open settings window to the front.
pub fn focus_settings() {
    unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> BOOL {
        let mut title = [0u16; 32];
        let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) }.max(0) as usize;
        if String::from_utf16_lossy(&title[..n]) == APP && owned_by_us(hwnd) {
            unsafe { *(found as *mut HWND) = hwnd };
            return 0;
        }
        1
    }
    let mut found: HWND = null_mut();
    unsafe {
        EnumWindows(Some(each), &mut found as *mut HWND as LPARAM);
        if !found.is_null() {
            ShowWindow(found, SW_RESTORE);
            SetForegroundWindow(found);
        }
    }
}

/// Whether `hwnd` belongs to a process running an exe with our name.
fn owned_by_us(hwnd: HWND) -> bool {
    let ours = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_lowercase());
        ok && name.is_some() && name == ours
    }
}
