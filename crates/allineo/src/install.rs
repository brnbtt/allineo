use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

use crate::config::Config;
use crate::paths::{self, APP};
use crate::{icon, win};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn uninstall_key() -> String {
    format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP}")
}

pub fn is_installed() -> bool {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(uninstall_key(), KEY_READ).is_ok()
        && paths::installed_exe().is_file()
}

/// Copies the running exe into the install folder, registers everything and starts the magnet.
pub fn install(cfg: &Config) -> Result<(), String> {
    let dir = paths::install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let target = paths::installed_exe();
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    if !paths::same_path(&current, &target) {
        // The installed copy can't be replaced while it runs.
        win::stop_agent();
        std::fs::copy(&current, &target).map_err(|e| format!("Couldn't copy {APP} to {}: {e}", target.display()))?;
    }
    let _ = std::fs::write(paths::icon_file(), icon::ico_bytes());
    cfg.save().map_err(|e| format!("Couldn't save settings: {e}"))?;
    register_uninstaller(&target).map_err(|e| format!("Couldn't register the uninstaller: {e}"))?;
    set_autostart(cfg.start_with_windows).map_err(|e| format!("Couldn't set up start with Windows: {e}"))?;
    create_shortcut(&target);
    if cfg.enabled || cfg.tray_icon {
        if win::agent_running() {
            win::reload_agent();
        } else {
            win::start_agent(&target).map_err(|e| format!("Couldn't start {APP}: {e}"))?;
        }
    }
    Ok(())
}

/// Starts (or stops starting) the magnet when you sign in.
pub fn set_autostart(on: bool) -> io::Result<()> {
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    if on {
        run.set_value(APP, &format!("\"{}\" agent", paths::installed_exe().display()))
    } else {
        match run.delete_value(APP) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// A Start menu entry that opens the settings (so "allineo" can be found by searching).
fn create_shortcut(exe: &Path) {
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}'); $s.TargetPath = '{exe}'; \
         $s.IconLocation = '{ico}'; $s.Description = 'allineo settings'; $s.Save()",
        lnk = q(&paths::start_menu_shortcut()),
        exe = q(exe),
        ico = q(&paths::icon_file()),
    );
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn register_uninstaller(exe: &Path) -> io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(uninstall_key())?;
    let exe_s = exe.display().to_string();
    let icon = paths::icon_file();
    let size_kb = std::fs::metadata(exe).map(|m| m.len() / 1024).unwrap_or(0) as u32;
    key.set_value("DisplayName", &APP)?;
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
    key.set_value("Publisher", &format!("{APP} (open source)"))?;
    key.set_value("DisplayIcon", &if icon.is_file() { icon.display().to_string() } else { exe_s.clone() })?;
    key.set_value("InstallLocation", &paths::install_dir().display().to_string())?;
    key.set_value("URLInfoAbout", &env!("CARGO_PKG_REPOSITORY"))?;
    key.set_value("UninstallString", &format!("\"{exe_s}\" uninstall"))?;
    key.set_value("QuietUninstallString", &format!("\"{exe_s}\" uninstall --quiet"))?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value("EstimatedSize", &size_kb)?;
    Ok(())
}

/// Stops the magnet, removes registry entries and deletes the install folder (after we exit, if
/// we're running from it).
pub fn uninstall() {
    win::stop_agent();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = set_autostart(false);
    let _ = hkcu.delete_subkey_all(uninstall_key());
    let _ = std::fs::remove_file(paths::start_menu_shortcut());
    // Copies of the hook still loaded by running apps stay until they exit; Windows cleans %TEMP%.
    let _ = std::fs::remove_dir_all(paths::hook_dir());

    let dir = paths::install_dir();
    if paths::running_from_install_dir() {
        // Our exe is locked while we run, so retry the delete for a while after we exit.
        let d = dir.display();
        let _ = Command::new("cmd.exe")
            .raw_arg(format!(
                "/C for /L %i in (1,1,60) do (ping -n 2 127.0.0.1 >NUL & rmdir /S /Q \"{d}\" 2>NUL & if not exist \"{d}\" exit)"
            ))
            .current_dir(std::env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    } else {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
