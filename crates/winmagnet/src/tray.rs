use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, GetCursorPos,
    IDI_APPLICATION, LoadIconW, MF_CHECKED, MF_SEPARATOR, MF_STRING, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP,
    WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};

use crate::app;
use crate::util::wide;

const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
const CMD_TOGGLE: usize = 1;
const CMD_EXIT: usize = 2;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Notification-area icon with an Enabled / Exit menu.
pub struct Tray {
    hwnd: HWND,
}

impl Tray {
    pub fn create() -> Result<Self, String> {
        let class = wide("WinMagnetTray");
        // SAFETY: registering a class / creating a hidden window we own.
        unsafe {
            let instance = GetModuleHandleW(null());
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
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
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("WinMagnet").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                return Err("Could not create the tray window.".into());
            }
            add_icon(hwnd);
            Ok(Self { hwnd })
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: removing our own icon and window.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &icon_data(self.hwnd));
            DestroyWindow(self.hwnd);
        }
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    // SAFETY: NOTIFYICONDATAW is plain data; zero is a valid starting point.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid
}

unsafe fn add_icon(hwnd: HWND) {
    let mut nid = icon_data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    // SAFETY: stock icon.
    nid.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    let tip: Vec<u16> = "WinMagnet".encode_utf16().collect();
    nid.szTip[..tip.len()].copy_from_slice(&tip);
    // SAFETY: fully initialised struct.
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

unsafe fn show_menu(hwnd: HWND) {
    // SAFETY: menu we create and destroy here.
    unsafe {
        let menu = CreatePopupMenu();
        let toggle_flags = MF_STRING | if app::is_enabled() { MF_CHECKED } else { 0 };
        AppendMenuW(menu, toggle_flags, CMD_TOGGLE, wide("Enabled").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_EXIT, wide("Exit").as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Required so the menu closes when clicking elsewhere.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);

        match cmd as usize {
            CMD_TOGGLE => app::set_enabled(!app::is_enabled()),
            CMD_EXIT => PostQuitMessage(0),
            _ => {}
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            if matches!(lparam as u32 & 0xFFFF, WM_RBUTTONUP | WM_LBUTTONUP | WM_CONTEXTMENU) {
                unsafe { show_menu(hwnd) };
            }
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            // Explorer restarted; put the icon back.
            unsafe { add_icon(hwnd) };
            0
        }
        // SAFETY: default handling.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
