#![windows_subsystem = "windows"]

mod agent;
mod config;
mod hooklib;
mod icon;
mod install;
mod paths;
mod sharedmem;
mod snapshot;
mod theme;
mod tray;
mod ui;
mod win;

use config::Config;
use paths::APP;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quiet = args.iter().any(|a| a == "--quiet");
    win::block_overlay_injection();

    let result = match args.first().map(String::as_str) {
        None | Some("settings") => ui::run_settings(),
        Some("agent") => agent::run(),
        Some("install") => install::install(&Config::load()).map(|_| {
            if !quiet {
                win::info_box(&format!("{APP} is installed and running. Drag a window toward another to try it."));
            }
        }),
        Some("uninstall") => {
            if quiet || win::confirm(&format!("Remove {APP} from this computer?")) {
                install::uninstall();
                if !quiet {
                    win::info_box(&format!("{APP} has been removed."));
                }
            }
            Ok(())
        }
        Some(other) => Err(format!(
            "Unknown command: {other}\n\nUsage:\n  {APP} [settings]\n  {APP} agent\n  {APP} install [--quiet]\n  {APP} uninstall [--quiet]"
        )),
    };

    if let Err(e) = result {
        if !quiet {
            win::error_box(&e);
        }
        std::process::exit(1);
    }
}
