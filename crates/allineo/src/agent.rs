//! The background magnet ("allineo agent"): watches for window drags and, while one is going on,
//! hooks the dragged window's thread so the hook DLL can adjust where Windows puts the window.

use std::cell::RefCell;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_UNKNOWN;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, IsWow64Process2, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, DefWindowProcW, DestroyWindow, DispatchMessageW, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART, GetMessageW, GetWindowThreadProcessId, HHOOK, MSG, OBJID_WINDOW, PostQuitMessage,
    TranslateMessage, UnhookWindowsHookEx, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::config::Config;
use crate::hooklib::HookLib;
use crate::sharedmem::SharedMemory;
use crate::win::{WM_AGENT_QUIT, WM_AGENT_RELOAD, wide};
use crate::{snapshot, tray};

/// Debug builds append a trace to %TEMP%\allineo\agent.log; compiled out of release builds.
macro_rules! log {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            use std::io::Write;
            let dir = crate::paths::hook_dir();
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("agent.log")) {
                let _ = writeln!(f, "{}", format_args!($($arg)*));
            }
        }
    };
}
pub(crate) use log;

struct Agent {
    cfg: Config,
    shared: SharedMemory,
    hook: HookLib,
    hwnd: HWND,
    tray_shown: bool,
    active: Option<ActiveDrag>,
}

struct ActiveDrag {
    hwnd: HWND,
    hook: HHOOK,
}

thread_local! {
    static AGENT: RefCell<Option<Agent>> = const { RefCell::new(None) };
}

/// Borrow the agent. Returns `None` on re-entrancy (e.g. a WinEvent arriving while the tray menu
/// is pumping messages) instead of panicking.
fn with_agent<R>(f: impl FnOnce(&mut Agent) -> R) -> Option<R> {
    AGENT.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

pub fn run() -> Result<(), String> {
    // SAFETY: process-wide setup call.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let Some(_instance) = SingleInstance::acquire() else {
        return Ok(()); // already running
    };

    let cfg = Config::load();
    let shared = SharedMemory::create()?;
    shared.set_enabled(cfg.enabled);
    let hook = HookLib::load()?;
    let hwnd = tray::create_window(Some(wnd_proc))?;
    AGENT.with(|cell| {
        *cell.borrow_mut() = Some(Agent { cfg, shared, hook, hwnd, tray_shown: false, active: None });
    });
    with_agent(Agent::sync_tray);

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
        return Err("Couldn't watch for window drags.".into());
    }
    log!("agent running");

    // SAFETY: standard message loop.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWinEvent(win_event);
    }
    with_agent(|a| {
        a.end_active();
        if a.tray_shown {
            tray::remove(a.hwnd);
        }
        unsafe { DestroyWindow(a.hwnd) };
    });
    AGENT.with(|cell| cell.borrow_mut().take());
    log!("agent stopped");
    Ok(())
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        tray::WM_TRAY => {
            match tray::event(lparam) {
                Some(tray::Click::Primary) => open_settings(),
                Some(tray::Click::Menu) => {
                    let paused = with_agent(|a| !a.cfg.enabled).unwrap_or(false);
                    match tray::menu(hwnd, paused) {
                        Some(tray::Choice::Settings) => open_settings(),
                        Some(tray::Choice::TogglePause) => {
                            with_agent(|a| {
                                a.cfg.enabled = !a.cfg.enabled;
                                let _ = a.cfg.save();
                                a.apply();
                            });
                        }
                        Some(tray::Choice::Quit) => unsafe { PostQuitMessage(0) },
                        None => {}
                    }
                }
                None => {}
            }
            0
        }
        WM_AGENT_RELOAD => {
            with_agent(|a| {
                a.cfg = Config::load();
                a.apply();
            });
            0
        }
        WM_AGENT_QUIT => {
            unsafe { PostQuitMessage(0) };
            0
        }
        m if m != 0 && m == tray::taskbar_created() => {
            // Explorer restarted; put the icon back.
            with_agent(|a| {
                a.tray_shown = false;
                a.sync_tray();
            });
            0
        }
        // SAFETY: default handling.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn open_settings() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("settings").spawn();
    }
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
    with_agent(|a| match event {
        EVENT_SYSTEM_MOVESIZESTART => a.begin(hwnd),
        EVENT_SYSTEM_MOVESIZEEND => a.end(hwnd),
        _ => {}
    });
}

impl Agent {
    /// Applies `self.cfg` (after a reload or a tray toggle).
    fn apply(&mut self) {
        self.shared.set_enabled(self.cfg.enabled);
        if !self.cfg.enabled {
            self.end_active();
        }
        self.sync_tray();
    }

    fn sync_tray(&mut self) {
        let paused = !self.cfg.enabled;
        match (self.cfg.tray_icon, self.tray_shown) {
            (true, false) => self.tray_shown = tray::add(self.hwnd, paused),
            (true, true) => tray::update(self.hwnd, paused),
            (false, true) => {
                tray::remove(self.hwnd);
                self.tray_shown = false;
            }
            (false, false) => {}
        }
    }

    fn begin(&mut self, hwnd: HWND) {
        self.end_active();
        if !self.cfg.enabled {
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

        let snap = snapshot::capture(hwnd, &self.cfg.engine());
        self.shared.publish(&snap);

        let hook = self.hook.hook_thread(tid);
        if hook.is_null() {
            // Typically an elevated window while we run unelevated (UIPI).
            log!("couldn't hook thread {tid} ({hwnd:?}): error {}", unsafe { GetLastError() });
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

/// The hook DLL has to match the bitness of the target process. Only the native (64-bit) DLL
/// ships for now.
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
        let name = wide("Local\\allineo.agent");
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
