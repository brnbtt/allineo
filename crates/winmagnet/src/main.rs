#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hooklib;
mod sharedmem;
mod snapshot;
mod tray;
mod util;

fn main() {
    if let Err(e) = app::run() {
        util::error_box(&e);
        std::process::exit(1);
    }
}
