//! 1. Embeds the app icon (drawn in `src/icon.rs`) and version info into the exe, so the
//!    downloaded `allineo.exe` already looks like allineo in Explorer and Task Manager.
//! 2. Builds the hook DLL (`crates/allineo-hook`) and hands it to the app, which embeds it, so
//!    allineo ships as a single exe.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "src/icon.rs"]
mod icon;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    build_hook(&out);
    embed_resources(&out);
}

fn embed_resources(out: &Path) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let app = out.join("allineo.ico");
    let paused = out.join("allineo-paused.ico");
    std::fs::write(&app, icon::ico_bytes()).unwrap();
    std::fs::write(&paused, icon::paused_ico_bytes()).unwrap();
    let mut res = winresource::WindowsResource::new();
    res.set_icon(app.to_str().unwrap());
    res.set_icon_with_id(paused.to_str().unwrap(), "2");
    res.set("FileDescription", "allineo");
    res.set("ProductName", "allineo");
    res.set("LegalCopyright", "MIT license");
    if let Err(e) = res.compile() {
        // Not fatal: the app works without it, it just has the generic exe icon.
        println!("cargo:warning=couldn't embed the icon: {e}");
    }
}

/// Builds `allineo-hook` with the same profile into its own target dir (sharing the outer one
/// would deadlock on cargo's build lock) and copies the DLL into OUT_DIR.
fn build_hook(out: &Path) {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().and_then(Path::parent).unwrap().to_path_buf();
    for watched in
        ["crates/allineo-hook/src", "crates/allineo-hook/Cargo.toml", "crates/allineo-core/src", "Cargo.lock"]
    {
        println!("cargo:rerun-if-changed={}", root.join(watched).display());
    }

    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    let target = std::env::var("TARGET").unwrap();
    let target_dir = root.join("target").join("hook");
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(&root)
        .args(["build", "-p", "allineo-hook", "--target", &target, "--target-dir"])
        .arg(&target_dir)
        .env_remove("CARGO_TARGET_DIR");
    if release {
        cmd.arg("--release");
    }
    let status = cmd.status().expect("couldn't run cargo to build the hook DLL");
    assert!(status.success(), "building allineo-hook failed");

    let dll = target_dir.join(&target).join(if release { "release" } else { "debug" }).join("allineo_hook.dll");
    std::fs::copy(&dll, out.join("allineo_hook.dll"))
        .unwrap_or_else(|e| panic!("couldn't copy {}: {e}", dll.display()));
}
