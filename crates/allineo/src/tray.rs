//! The agent's hidden window and its notification-area icon.

use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICON_VERSION_4,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, GetCursorPos, HICON, IMAGE_ICON, LR_DEFAULTCOLOR,
    LoadImageW, MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassW, RegisterWindowMessageW, SM_CXSMICON,
    SM_CYSMICON, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_CONTEXTMENU, WM_NULL,
    WNDCLASSW, WNDPROC,
};

use crate::paths::APP;
use crate::win::{AGENT_CLASS, wide};

pub const WM_TRAY: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;
const ICON_ID: u32 = 1;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn taskbar_created() -> u32 {
    TASKBAR_CREATED.load(Ordering::Relaxed)
}

/// A hidden top-level window (message-only windows don't get the "taskbar created" broadcast,
/// and the settings window finds the agent by this window's class).
pub fn create_window(proc_: WNDPROC) -> Result<HWND, String> {
    let class = wide(AGENT_CLASS);
    // SAFETY: registering a class / creating a hidden window we own.
    unsafe {
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: proc_,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);
        let title = wide(&format!("{APP} agent"));
        let hwnd =
            CreateWindowExW(0, class.as_ptr(), title.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), instance, null());
        if hwnd.is_null() { Err("Couldn't create the agent window.".into()) } else { Ok(hwnd) }
    }
}

/// The app icon (resource 1) or its dimmed "paused" twin (resource 2), at tray size.
fn icon(paused: bool) -> HICON {
    // SAFETY: loading a resource from our own exe.
    unsafe {
        let dpi = GetDpiForSystem();
        let (cx, cy) = (GetSystemMetricsForDpi(SM_CXSMICON, dpi), GetSystemMetricsForDpi(SM_CYSMICON, dpi));
        let id = if paused { 2 } else { 1 };
        LoadImageW(GetModuleHandleW(null()), id as usize as *const u16, IMAGE_ICON, cx, cy, LR_DEFAULTCOLOR)
    }
}

fn data(hwnd: HWND, paused: bool) -> NOTIFYICONDATAW {
    // SAFETY: NOTIFYICONDATAW is plain data; zero is a valid starting point.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon(paused);
    let tip = if paused { format!("{APP} · paused") } else { APP.to_string() };
    let tip: Vec<u16> = tip.encode_utf16().take(nid.szTip.len() - 1).collect();
    nid.szTip[..tip.len()].copy_from_slice(&tip);
    nid
}

pub fn add(hwnd: HWND, paused: bool) -> bool {
    let mut nid = data(hwnd, paused);
    // SAFETY: fully initialised struct.
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, &nid) == 0 {
            return false;
        }
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &nid);
    }
    true
}

pub fn update(hwnd: HWND, paused: bool) {
    // SAFETY: fully initialised struct.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data(hwnd, paused)) };
}

pub fn remove(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    // SAFETY: identifies our own icon.
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

pub enum Click {
    /// Left click (or Enter / Space on the icon).
    Primary,
    /// Right click (or the menu key).
    Menu,
}

/// Decodes a `WM_TRAY` callback (NOTIFYICON_VERSION_4: the event is in the low word of lParam).
pub fn event(lparam: LPARAM) -> Option<Click> {
    match lparam as u32 & 0xFFFF {
        NIN_SELECT => Some(Click::Primary),
        WM_CONTEXTMENU => Some(Click::Menu),
        _ => None,
    }
}

pub enum Choice {
    Settings,
    TogglePause,
    Quit,
}

pub fn menu(hwnd: HWND, paused: bool) -> Option<Choice> {
    const SETTINGS: usize = 1;
    const PAUSE: usize = 2;
    const QUIT: usize = 3;
    // SAFETY: a menu we create and destroy here.
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, SETTINGS, wide("settings…").as_ptr());
        AppendMenuW(menu, MF_STRING, PAUSE, wide(if paused { "resume" } else { "pause" }).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, QUIT, wide(&format!("quit {APP}")).as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Required so the menu closes when clicking elsewhere.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        match cmd as usize {
            SETTINGS => Some(Choice::Settings),
            PAUSE => Some(Choice::TogglePause),
            QUIT => Some(Choice::Quit),
            _ => None,
        }
    }
}
