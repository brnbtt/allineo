use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use std::{env, fs};

use windows_sys::Win32::Foundation::{HMODULE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{HHOOK, SetWindowsHookExW, WH_CALLWNDPROC};

use crate::util::{log, wide};

const DLL_NAME: &str = "magnet_hook.dll";

type HookProc = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;

/// The hook DLL, loaded into our own process so we can pass its module handle
/// and hook procedure to `SetWindowsHookExW`.
pub struct HookLib {
    module: HMODULE,
    proc_: HookProc,
}

impl HookLib {
    pub fn load() -> Result<Self, String> {
        let path = stage()?;
        let wpath = wide(&path.to_string_lossy());
        // SAFETY: valid NUL-terminated path / symbol name.
        unsafe {
            let module = LoadLibraryW(wpath.as_ptr());
            if module.is_null() {
                return Err(format!("Could not load {}", path.display()));
            }
            let Some(sym) = GetProcAddress(module, c"magnet_hook_proc".as_ptr().cast()) else {
                return Err(format!("{DLL_NAME} does not export magnet_hook_proc"));
            };
            let proc_ = std::mem::transmute::<unsafe extern "system" fn() -> isize, HookProc>(sym);
            Ok(Self { module, proc_ })
        }
    }

    /// Hook a single thread (the one that owns the window being dragged).
    pub fn hook_thread(&self, thread_id: u32) -> HHOOK {
        // SAFETY: module/proc come from the loaded DLL.
        unsafe { SetWindowsHookExW(WH_CALLWNDPROC, Some(self.proc_), self.module, thread_id) }
    }
}

/// Target processes pin the DLL once they have used it, which would lock the
/// file in `target/` and break rebuilds. Load a uniquely named copy from
/// %TEMP% instead, and sweep stale copies that are no longer in use.
fn stage() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let src = exe.with_file_name(DLL_NAME);
    let meta = fs::metadata(&src).map_err(|_| format!("{DLL_NAME} was not found next to {}", exe.display()))?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as u64);
    let stamp = mtime ^ meta.len().rotate_left(32);

    let dir = env::temp_dir().join("WinMagnet");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dst = dir.join(format!("magnet_hook_{stamp:016x}.dll"));
    if !dst.exists() {
        fs::copy(&src, &dst).map_err(|e| format!("Could not stage {DLL_NAME}: {e}"))?;
    }

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let stale = p != dst && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("magnet_hook_"));
            if stale && fs::remove_file(&p).is_ok() {
                log!("removed stale {}", p.display());
            }
        }
    }
    Ok(dst)
}
