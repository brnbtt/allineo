use std::cell::RefCell;
use std::ptr::null_mut;

use magnet_core::Config;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND};
use windows_sys::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_UNKNOWN;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, IsWow64Process2, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, DispatchMessageW, EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GetMessageW,
    GetWindowThreadProcessId, HHOOK, MSG, OBJID_WINDOW, TranslateMessage, UnhookWindowsHookEx, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use crate::hooklib::HookLib;
use crate::sharedmem::SharedMemory;
use crate::snapshot;
use crate::tray::Tray;
use crate::util::{log, wide};

struct App {
    config: Config,
    shared: SharedMemory,
    hook: HookLib,
    enabled: bool,
    active: Option<ActiveDrag>,
}

struct ActiveDrag {
    hwnd: HWND,
    hook: HHOOK,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Borrow the app state. Returns `None` on re-entrancy (e.g. a WinEvent
/// arriving while a tray menu is pumping messages) instead of panicking.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

pub fn is_enabled() -> bool {
    with_app(|a| a.enabled).unwrap_or(false)
}

pub fn set_enabled(on: bool) {
    with_app(|a| {
        a.enabled = on;
        a.shared.set_enabled(on);
        if !on {
            a.end_active();
        }
    });
}

pub fn run() -> Result<(), String> {
    // SAFETY: process-wide setup calls.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let Some(_instance) = SingleInstance::acquire() else {
        return Err("WinMagnet is already running.".into());
    };

    let shared = SharedMemory::create()?;
    let hook = HookLib::load()?;
    APP.with(|cell| {
        *cell.borrow_mut() = Some(App { config: Config::DEFAULT, shared, hook, enabled: true, active: None })
    });

    // SAFETY: out-of-context hook; callbacks arrive through our message loop.
    let win_event = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_MOVESIZESTART,
            EVENT_SYSTEM_MOVESIZEEND,
            null_mut(),
            Some(on_win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if win_event.is_null() {
        return Err("Could not register the move/size event hook.".into());
    }

    let tray = Tray::create()?;
    log!("running");

    // SAFETY: standard message loop.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWinEvent(win_event);
    }
    drop(tray);
    with_app(|a| a.end_active());
    APP.with(|cell| cell.borrow_mut().take());
    Ok(())
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() || id_object != OBJID_WINDOW || id_child != CHILDID_SELF as i32 {
        return;
    }
    with_app(|app| match event {
        EVENT_SYSTEM_MOVESIZESTART => app.begin(hwnd),
        EVENT_SYSTEM_MOVESIZEEND => app.end(hwnd),
        _ => {}
    });
}

impl App {
    fn begin(&mut self, hwnd: HWND) {
        self.end_active();
        if !self.enabled {
            return;
        }
        let mut pid = 0u32;
        // SAFETY: valid out-pointer.
        let tid = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if tid == 0 {
            return;
        }
        if !same_architecture(pid) {
            log!("skipping {hwnd:?}: 32-bit process (not supported yet)");
            return;
        }

        let snap = snapshot::capture(hwnd, &self.config);
        self.shared.publish(&snap);

        let hook = self.hook.hook_thread(tid);
        if hook.is_null() {
            // Typically an elevated window while we run unelevated (UIPI).
            log!("could not hook thread {tid} ({hwnd:?}): error {}", unsafe { GetLastError() });
            self.shared.clear_target();
            return;
        }
        log!(
            "drag start {hwnd:?}: {} windows, {} monitors, insets {:?}",
            snap.window_count,
            snap.monitor_count,
            snap.insets
        );
        self.active = Some(ActiveDrag { hwnd, hook });
    }

    fn end(&mut self, hwnd: HWND) {
        if self.active.as_ref().is_some_and(|a| a.hwnd == hwnd) {
            self.end_active();
        }
    }

    fn end_active(&mut self) {
        if let Some(a) = self.active.take() {
            // SAFETY: hook handle we created.
            unsafe { UnhookWindowsHookEx(a.hook) };
            self.shared.clear_target();
            log!("drag end {:?}", a.hwnd);
        }
    }
}

/// The hook DLL has to match the bitness of the target process. Only the
/// native (64-bit) DLL ships for now.
fn same_architecture(pid: u32) -> bool {
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return true; // let SetWindowsHookEx decide
        }
        let (mut process, mut native) = (0u16, 0u16);
        let ok = IsWow64Process2(h, &mut process, &mut native);
        CloseHandle(h);
        ok == 0 || process == IMAGE_FILE_MACHINE_UNKNOWN
    }
}

struct SingleInstance(HANDLE);

impl SingleInstance {
    fn acquire() -> Option<Self> {
        let name = wide("Local\\WinMagnet.Instance");
        // SAFETY: valid NUL-terminated name.
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
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

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: handle we own.
        unsafe { CloseHandle(self.0) };
    }
}
