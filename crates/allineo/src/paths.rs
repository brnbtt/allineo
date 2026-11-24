use std::path::{Path, PathBuf};

/// The app's name, used for the install folder, registry keys and window titles.
pub const APP: &str = "allineo";

fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local")))
        .unwrap_or_else(std::env::temp_dir)
}

/// Everything the app owns lives here, so uninstalling is a single folder delete.
pub fn install_dir() -> PathBuf {
    local_appdata().join("Programs").join(APP)
}

pub fn installed_exe() -> PathBuf {
    install_dir().join(format!("{APP}.exe"))
}

pub fn icon_file() -> PathBuf {
    install_dir().join(format!("{APP}.ico"))
}

pub fn config_file() -> PathBuf {
    install_dir().join("config.json")
}

/// The hook DLL is unpacked here. Apps that used it keep it loaded until they exit, so it can't
/// live in the install folder (it would block uninstalling and updating).
pub fn hook_dir() -> PathBuf {
    std::env::temp_dir().join(APP)
}

pub fn start_menu_shortcut() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| local_appdata().join("..").join("Roaming"))
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{APP}.lnk"))
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase();
    norm(a) == norm(b)
}

pub fn running_from_install_dir() -> bool {
    std::env::current_exe().map(|exe| same_path(&exe, &installed_exe())).unwrap_or(false)
}
