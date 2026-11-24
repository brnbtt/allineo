use std::fs;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{HMODULE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{HHOOK, SetWindowsHookExW, WH_CALLWNDPROC};

use crate::agent::log;
use crate::paths;
use crate::win::wide;

/// `allineo_hook.dll`, built by `build.rs`.
static DLL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/allineo_hook.dll"));

type HookProc = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;

/// The hook DLL, loaded into our own process so we can pass its module handle
/// and hook procedure to `SetWindowsHookExW`.
pub struct HookLib {
    module: HMODULE,
    proc_: HookProc,
}

impl HookLib {
    pub fn load() -> Result<Self, String> {
        let path = unpack()?;
        let wpath = wide(&path.to_string_lossy());
        // SAFETY: valid NUL-terminated path / symbol name.
        unsafe {
            let module = LoadLibraryW(wpath.as_ptr());
            if module.is_null() {
                return Err(format!("Couldn't load {}", path.display()));
            }
            let Some(sym) = GetProcAddress(module, c"allineo_hook_proc".as_ptr().cast()) else {
                return Err("The hook DLL doesn't export allineo_hook_proc".into());
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

/// Apps that used the DLL keep it loaded (it pins itself), so every build gets its own file name
/// and older copies are swept once nothing uses them anymore.
fn unpack() -> Result<PathBuf, String> {
    let dir = paths::hook_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let name = format!("allineo_hook_{:016x}.dll", fnv1a(DLL));
    let dst = dir.join(&name);
    if fs::metadata(&dst).map(|m| m.len()).ok() != Some(DLL.len() as u64) {
        let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
        fs::write(&tmp, DLL).map_err(|e| format!("Couldn't unpack the hook DLL: {e}"))?;
        if fs::rename(&tmp, &dst).is_err() && !dst.is_file() {
            return Err("Couldn't unpack the hook DLL".into());
        }
        let _ = fs::remove_file(&tmp);
    }
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let stale = p != dst && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("allineo_hook_"));
            if stale && fs::remove_file(&p).is_ok() {
                log!("removed stale {}", p.display());
            }
        }
    }
    Ok(dst)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}
