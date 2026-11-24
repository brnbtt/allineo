//! Hook DLL, loaded into the process of the window that is being dragged.
//!
//! The app installs a thread-specific `WH_CALLWNDPROC` hook on the dragged
//! window's thread only while a move/size loop is running. The first time the
//! hook sees a move/size message for the target window it subclasses that
//! window; the subclass rewrites the RECT carried by `WM_MOVING`/`WM_SIZING`
//! before the app (and the system move loop) sees it. The subclass removes
//! itself on `WM_EXITSIZEMOVE`.
//!
//! The DLL pins itself once it has subclassed a window so it can never be
//! unloaded underneath a live subclass procedure.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use allineo_core::Session;
use allineo_core::engine::edges;
use allineo_core::geom::{Insets, Rect};
use allineo_core::shared::{SHARED_NAME, SharedBlock};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW,
};
use windows_sys::Win32::System::Memory::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CWPSTRUCT, CallNextHookEx, GetWindowRect, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING, WM_NCDESTROY, WM_SIZING,
};

const SUBCLASS_ID: usize = 0x574D_4147; // "WMAG"

static SHARED: AtomicPtr<SharedBlock> = AtomicPtr::new(null_mut());
static PINNED: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<Active>> = Mutex::new(None);

struct Active {
    hwnd: usize,
    session_id: u32,
    insets: Insets,
    bypass_vk: u32,
    dpi: DpiMap,
    session: Session,
}

fn shared() -> Option<&'static SharedBlock> {
    let mut p = SHARED.load(Ordering::Acquire);
    if p.is_null() {
        let name: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
        // SAFETY: plain Win32 calls; the view is never unmapped, the DLL is
        // pinned for the lifetime of the process once it does real work.
        unsafe {
            let h = OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr());
            if h.is_null() {
                return None;
            }
            let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, size_of::<SharedBlock>());
            CloseHandle(h);
            if view.Value.is_null() {
                return None;
            }
            p = match SHARED.compare_exchange(null_mut(), view.Value.cast(), Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => view.Value.cast(),
                Err(existing) => existing, // another thread won; leak our view
            };
        }
    }
    // SAFETY: points into a live read-only mapping.
    let block = unsafe { &*p };
    block.is_valid().then_some(block)
}

fn pin_self() {
    if PINNED.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut module = null_mut();
    // SAFETY: the address is a function in this module.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            allineo_hook_proc as *const u16,
            &mut module,
        );
    }
}

/// Debug builds append a trace to %TEMP%\allineo\hook.log.
macro_rules! trace {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            use std::io::Write;
            let path = std::env::temp_dir().join("allineo").join("hook.log");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "[{}] {}", std::process::id(), format_args!($($arg)*));
            }
        }
    };
}

fn now_ms() -> f64 {
    let (mut c, mut f) = (0i64, 0i64);
    // SAFETY: out-pointers to locals.
    unsafe {
        QueryPerformanceCounter(&mut c);
        QueryPerformanceFrequency(&mut f);
    }
    if f == 0 { 0.0 } else { c as f64 * 1000.0 / f as f64 }
}

/// Hook procedure exported for `SetWindowsHookExW(WH_CALLWNDPROC, ..)`.
///
/// # Safety
/// Called by the system with a valid `CWPSTRUCT` pointer when `code >= 0`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn allineo_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        // SAFETY: lparam is a CWPSTRUCT for HC_ACTION.
        let cwp = unsafe { &*(lparam as *const CWPSTRUCT) };
        if matches!(cwp.message, WM_ENTERSIZEMOVE | WM_MOVING | WM_SIZING) {
            let _ = catch_unwind(|| maybe_subclass(cwp.hwnd));
        }
    }
    // SAFETY: forwarding the arguments we were given.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn maybe_subclass(hwnd: HWND) {
    let Some(shared) = shared() else { return };
    if shared.target_hwnd() != hwnd as u64 {
        return;
    }
    pin_self();
    // SAFETY: we're on the window's own thread (hooks run in its context);
    // SetWindowSubclass is idempotent for the same proc/id pair.
    unsafe {
        SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _ref_data: usize,
) -> LRESULT {
    match msg {
        WM_MOVING | WM_SIZING if lparam != 0 => {
            // SAFETY: lparam of WM_MOVING / WM_SIZING is a writable RECT.
            let rect = unsafe { &mut *(lparam as *mut RECT) };
            let _ = catch_unwind(AssertUnwindSafe(|| snap(hwnd, msg, wparam, rect)));
        }
        WM_ENTERSIZEMOVE => reset(),
        WM_EXITSIZEMOVE | WM_NCDESTROY => {
            reset();
            // SAFETY: removing our own subclass from within it is allowed.
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), id) };
        }
        _ => {}
    }
    // We go first and let the app have the final say, so apps that enforce
    // an aspect ratio or a minimum size keep working.
    // SAFETY: forwarding the arguments we were given.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn state() -> std::sync::MutexGuard<'static, Option<Active>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn reset() {
    *state() = None;
}

fn snap(hwnd: HWND, msg: u32, wparam: WPARAM, rect: &mut RECT) {
    let Some(shared) = shared() else { return };
    if !shared.enabled() {
        return;
    }
    let mut guard = state();
    let current = shared.session();
    let fresh = match &*guard {
        Some(a) => a.hwnd != hwnd as usize || Some(a.session_id) != current,
        None => true,
    };
    if fresh {
        let Some((session_id, snapshot)) = shared.read() else { return };
        if snapshot.target_hwnd != hwnd as u64 {
            return;
        }
        let dpi = DpiMap::capture(hwnd);
        trace!(
            "session {session_id} for {hwnd:?}: {} windows, identity {} scale {:.3}x{:.3}",
            snapshot.window_count, dpi.identity, dpi.sx, dpi.sy
        );
        *guard = Some(Active {
            hwnd: hwnd as usize,
            session_id,
            insets: snapshot.insets,
            bypass_vk: snapshot.config.bypass_vk,
            dpi,
            session: Session::from_snapshot(&snapshot),
        });
    }
    let Some(active) = guard.as_mut() else { return };

    let proposed_window = active.dpi.to_physical(*rect);
    let proposed = active.insets.shrink(proposed_window);
    // SAFETY: trivial Win32 call.
    let bypass = active.bypass_vk != 0 && unsafe { GetAsyncKeyState(active.bypass_vk as i32) } < 0;
    let t = now_ms();

    let snapped = if msg == WM_MOVING {
        active.session.on_move(proposed, t, bypass)
    } else {
        active.session.on_size(proposed, edges::from_wmsz(wparam), t, bypass)
    };

    if snapped != proposed {
        *rect = active.dpi.to_logical(active.insets.grow(snapped));
    }
}

/// The RECT in `WM_MOVING` is in the window's own DPI space (scaled for
/// DPI-unaware / system-aware apps); the snapshot is in physical pixels.
///
/// `LogicalToPhysicalPointForPerMonitorDPI` can't be used because it fails
/// for points outside the window's current bounds, which is exactly where a
/// proposed drag rect is. Instead, read the window rect in both spaces at the
/// same instant and use the linear mapping between them.
#[derive(Clone, Copy)]
struct DpiMap {
    identity: bool,
    log0: (f64, f64),
    phys0: (f64, f64),
    sx: f64,
    sy: f64,
}

impl DpiMap {
    fn capture(hwnd: HWND) -> Self {
        let zero = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let (mut l, mut p) = (zero, zero);
        // SAFETY: we're on the window's thread; the previous awareness is
        // restored immediately.
        unsafe {
            GetWindowRect(hwnd, &mut l);
            let prev = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            GetWindowRect(hwnd, &mut p);
            if !prev.is_null() {
                SetThreadDpiAwarenessContext(prev);
            }
        }
        let same = (l.left, l.top, l.right, l.bottom) == (p.left, p.top, p.right, p.bottom);
        let ratio = |a: i32, b: i32| if b > 0 { a as f64 / b as f64 } else { 1.0 };
        Self {
            identity: same,
            log0: (l.left as f64, l.top as f64),
            phys0: (p.left as f64, p.top as f64),
            sx: ratio(p.right - p.left, l.right - l.left),
            sy: ratio(p.bottom - p.top, l.bottom - l.top),
        }
    }

    fn to_physical(self, r: RECT) -> Rect {
        if self.identity {
            return Rect::new(r.left, r.top, r.right, r.bottom);
        }
        let x = |v: i32| (self.phys0.0 + (v as f64 - self.log0.0) * self.sx).round() as i32;
        let y = |v: i32| (self.phys0.1 + (v as f64 - self.log0.1) * self.sy).round() as i32;
        Rect::new(x(r.left), y(r.top), x(r.right), y(r.bottom))
    }

    fn to_logical(self, r: Rect) -> RECT {
        if self.identity {
            return RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        }
        let x = |v: i32| (self.log0.0 + (v as f64 - self.phys0.0) / self.sx).round() as i32;
        let y = |v: i32| (self.log0.1 + (v as f64 - self.phys0.1) / self.sy).round() as i32;
        RECT { left: x(r.left), top: y(r.top), right: x(r.right), bottom: y(r.bottom) }
    }
}
