//! Records the README demo (docs/images/demo.gif): two app-like windows on a plain backdrop,
//! one dragged against the other (snaps, holds), slid up until the tops line up, then resized
//! until the bottoms line up.
//!
//!     cargo build --workspace --all-targets
//!     .\target\debug\examples\demo.exe <out.mp4>
//!
//! Needs the allineo agent running and ffmpeg on PATH. Takes over the mouse for ~8 s. The
//! backdrop covers the whole screen, so nothing else ends up in the recording (and nothing
//! behind it is a snap target).

use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, FindWindowW, GWL_EXSTYLE, GetClassNameW,
    GetClientRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IsWindowVisible, MSG,
    PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SetCursorPos,
    SetWindowPos, ShowWindow, TranslateMessage, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WM_SIZE, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW, WS_POPUP,
};

const CLASS: &str = "allineoDemoWindow";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    match args.get(1).map(String::as_str) {
        Some("window") => {
            let n = |i: usize| args[i].parse::<i32>().unwrap();
            run_window(&args[2], n(3), n(4), n(5), n(6));
        }
        Some("plan") => {
            let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
            let (mut bg, backdrop) = spawn("backdrop", (0, 0, sw, sh));
            pause(0.5);
            let avoid = above(backdrop);
            let ex = unsafe { GetWindowLongPtrW(backdrop, GWL_EXSTYLE) } as u32;
            println!("backdrop ex=0x{ex:08X} topmost={}", ex & 0x8 != 0);
            let _ = bg.kill();
            let _ = bg.wait();
            for r in &avoid {
                println!("avoid ({}, {}) - ({}, {})", r.left, r.top, r.right, r.bottom);
            }
            println!("screen {sw}x{sh}, stage at {:?}", stage_origin(&avoid, sw, sh, 1200, 676));
        }
        out => direct(out.unwrap_or("demo.mp4")),
    }
}

// ---------------------------------------------------------------------------------------------
// The windows (each in its own process, so the drag loop runs independently of the director)
// ---------------------------------------------------------------------------------------------

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

fn fill(dc: *mut core::ffi::c_void, x0: i32, y0: i32, x1: i32, y1: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, &RECT { left: x0, top: y0, right: x1, bottom: y1 }, brush);
        DeleteObject(brush);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            unsafe { InvalidateRect(hwnd, null(), 0) };
            0
        }
        WM_PAINT => {
            unsafe {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd, &mut r);
                let mut title = [0u16; 32];
                let n = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, title.as_mut_ptr(), 32);
                let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
                paint(dc, &title, r.right, r.bottom);
                EndPaint(hwnd, &ps);
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Simple stand-ins for app content: a desktop gradient, a notes list, a photo grid.
fn paint(dc: *mut core::ffi::c_void, title: &str, w: i32, h: i32) {
    let s = |v: i32| v * 3 / 2; // drawn at 150%
    match title {
        "backdrop" => {
            let bands = 48;
            for i in 0..bands {
                let t = i as f32 / (bands - 1) as f32;
                let l = |a: f32, b: f32| (a + (b - a) * t) as u8;
                fill(
                    dc,
                    0,
                    h * i / bands,
                    w,
                    h * (i + 1) / bands + 1,
                    rgb(l(44.0, 18.0), l(48.0, 19.0), l(58.0, 23.0)),
                );
            }
        }
        "notes" => {
            fill(dc, 0, 0, w, h, rgb(28, 28, 30));
            fill(dc, 0, 0, s(120), h, rgb(34, 34, 37));
            for i in 0..6 {
                let y = s(18) + i * s(30);
                fill(dc, s(14), y, s(14) + s(60 + (i * 23) % 40), y + s(8), rgb(70, 70, 76));
            }
            for i in 0..9 {
                let y = s(22) + i * s(22);
                let width = (w - s(160)) * (60 + (i * 37) % 40) / 100;
                fill(
                    dc,
                    s(140),
                    y,
                    s(140) + width,
                    y + s(7),
                    if i == 0 { rgb(120, 120, 128) } else { rgb(58, 58, 64) },
                );
            }
        }
        _ => {
            fill(dc, 0, 0, w, h, rgb(24, 24, 26));
            let (cols, gap) = (3, s(8));
            let cell = (w - gap * (cols + 1)) / cols;
            let shades =
                [rgb(64, 78, 96), rgb(92, 70, 64), rgb(66, 88, 72), rgb(88, 84, 60), rgb(74, 66, 92), rgb(60, 82, 88)];
            for i in 0..12 {
                let (c, r) = (i % cols, i / cols);
                let x = gap + c * (cell + gap);
                let y = gap + r * (cell + gap);
                if y < h {
                    fill(dc, x, y, x + cell, (y + cell).min(h), shades[i as usize % shades.len()]);
                }
            }
        }
    }
}

fn run_window(title: &str, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorW(
                null_mut(),
                windows_sys::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            ),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let backdrop = title == "backdrop";
        let (style, ex) =
            if backdrop { (WS_POPUP, WS_EX_TOOLWINDOW | WS_EX_TOPMOST) } else { (WS_OVERLAPPEDWINDOW, WS_EX_TOPMOST) };
        let hwnd = CreateWindowExW(
            ex,
            class.as_ptr(),
            wide(title).as_ptr(),
            style,
            x,
            y,
            w,
            h,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if !backdrop {
            // Dark title bar matching the content.
            let set = |attr: u32, value: u32| {
                DwmSetWindowAttribute(hwnd, attr, &value as *const u32 as *const _, 4);
            };
            set(20, 1); // dark mode
            set(35, rgb(28, 28, 30)); // caption
            set(36, rgb(200, 200, 205)); // caption text
            set(34, rgb(60, 60, 66)); // border
        }
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOACTIVATE | 0x0001 | 0x0002);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The director
// ---------------------------------------------------------------------------------------------

#[allow(clippy::zombie_processes)]
fn spawn(title: &str, r: (i32, i32, i32, i32)) -> (Child, HWND) {
    let exe = std::env::current_exe().unwrap();
    let args = [r.0, r.1, r.2, r.3].map(|v| v.to_string());
    let child = Command::new(exe).arg("window").arg(title).args(args).spawn().unwrap();
    let (class, t) = (wide(CLASS), wide(title));
    let start = Instant::now();
    loop {
        let hwnd = unsafe { FindWindowW(class.as_ptr(), t.as_ptr()) };
        if !hwnd.is_null() {
            return (child, hwnd);
        }
        assert!(start.elapsed() < Duration::from_secs(5), "{title} did not appear");
        sleep(Duration::from_millis(20));
    }
}

fn frame(hwnd: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS as u32, (&mut r as *mut RECT).cast(), 16);
    }
    r
}

/// Places a window so its *visible frame* is exactly `(x, y, w, h)`.
fn place(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        let mut wr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(hwnd, &mut wr);
        let f = frame(hwnd);
        let (l, t, r, b) = (f.left - wr.left, f.top - wr.top, wr.right - f.right, wr.bottom - f.bottom);
        SetWindowPos(hwnd, HWND_TOPMOST, x - l, y - t, w + l + r, h + t + b, SWP_NOACTIVATE);
    }
}

fn mouse(flags: u32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}

/// Smooth pointer move (ease in-out) from `from` to `to` over `secs`.
fn glide(from: (i32, i32), to: (i32, i32), secs: f32) -> (i32, i32) {
    let steps = (secs * 120.0) as i32;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let e = t * t * (3.0 - 2.0 * t);
        let x = from.0 as f32 + (to.0 - from.0) as f32 * e;
        let y = from.1 as f32 + (to.1 - from.1) as f32 * e;
        unsafe { SetCursorPos(x.round() as i32, y.round() as i32) };
        sleep(Duration::from_secs_f32(secs / steps as f32));
    }
    to
}

fn pause(secs: f32) {
    sleep(Duration::from_secs_f32(secs));
}

/// Windows that sit *above* the backdrop (other apps' always-on-top windows, and apps in higher
/// system layers such as accessibility tools): they'd end up in the recording, so the stage is
/// placed clear of them. Enumeration is top-down, so it's everything listed before `backdrop`.
fn above(backdrop: HWND) -> Vec<RECT> {
    struct Walk {
        stop: HWND,
        out: Vec<RECT>,
    }
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> windows_sys::core::BOOL {
        let walk = unsafe { &mut *(data as *mut Walk) };
        if hwnd == walk.stop {
            return 0;
        }
        unsafe {
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(hwnd, 14, (&mut cloaked as *mut u32).cast(), 4);
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, class.as_mut_ptr(), 64).max(0) as usize;
            let ours = String::from_utf16_lossy(&class[..n]) == CLASS;
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if IsWindowVisible(hwnd) != 0 && cloaked == 0 && !ours && ex & WS_EX_TRANSPARENT == 0 {
                let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetWindowRect(hwnd, &mut r);
                if r.right > r.left && r.bottom > r.top {
                    walk.out.push(r);
                }
            }
        }
        1
    }
    let mut walk = Walk { stop: backdrop, out: Vec::new() };
    unsafe { EnumWindows(Some(each), &mut walk as *mut Walk as LPARAM) };
    walk.out
}

/// The free spot for a `w`×`h` stage closest to the middle of the screen.
fn stage_origin(avoid: &[RECT], sw: i32, sh: i32, w: i32, h: i32) -> (i32, i32) {
    let margin = 40;
    let hits = |x: i32, y: i32| {
        avoid
            .iter()
            .any(|r| x - margin < r.right && r.left < x + w + margin && y - margin < r.bottom && r.top < y + h + margin)
    };
    let (cx, cy) = ((sw - w) / 2, (sh - h) / 2 - 40);
    let mut best: Option<(i64, (i32, i32))> = None;
    for y in (margin..sh - h - 120).step_by(20) {
        for x in (margin..sw - w - margin).step_by(20) {
            if !hits(x, y) {
                let d = ((x - cx) as i64).pow(2) + ((y - cy) as i64).pow(2);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, (x, y)));
                }
            }
        }
    }
    best.map(|(_, p)| p).expect("no room on screen clear of always-on-top windows")
}

fn direct(out: &str) {
    let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    // A 16:9 stage, placed clear of anything that stays above the backdrop.
    let (w, h) = (1200, 676);
    let (mut bg, backdrop) = spawn("backdrop", (0, 0, sw, sh));
    pause(0.5);
    let (ox, oy) = stage_origin(&above(backdrop), sw, sh, w, h);
    let (mut b_proc, b) = spawn("photos", (ox + 660, oy + 120, 420, 380));
    let (mut a_proc, a) = spawn("notes", (ox + 110, oy + 230, 360, 290));
    pause(0.4);
    place(b, ox + 660, oy + 130, 420, 400);
    place(a, ox + 120, oy + 250, 360, 290);
    pause(0.6);

    let duration = 8.4;
    let mut ffmpeg = Command::new("ffmpeg")
        .env("__COMPAT_LAYER", "HighDpiAware")
        .args(["-y", "-loglevel", "error", "-f", "gdigrab", "-framerate", "30", "-draw_mouse", "1"])
        .args(["-offset_x", &ox.to_string(), "-offset_y", &oy.to_string(), "-video_size", &format!("{w}x{h}")])
        .args(["-t", &duration.to_string(), "-i", "desktop"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "16", "-pix_fmt", "yuv420p", out])
        .spawn()
        .expect("ffmpeg not found on PATH");
    pause(0.9);

    let fa = frame(a);
    let fb = frame(b);
    // 1. Grab "notes" by its title bar and drag it into "photos": it lands flush and holds.
    let grab = (fa.left + 110, fa.top + 16);
    let mut p = glide((ox + 330, oy + 600), grab, 0.6);
    pause(0.25);
    mouse(MOUSEEVENTF_LEFTDOWN);
    pause(0.15);
    let push = 26;
    p = glide(p, (p.0 + (fb.left - fa.right) + push, p.1), 1.4);
    pause(0.6);
    // 2. Slide it up: its top lines up with the neighbour's.
    p = glide(p, (p.0, p.1 - (fa.top - fb.top) - 14), 0.8);
    pause(0.5);
    mouse(MOUSEEVENTF_LEFTUP);
    pause(0.3);

    // 3. Resize it from the bottom edge: it stops level with the neighbour's bottom.
    let fa = frame(a);
    let edge = ((fa.left + fa.right) / 2, fa.bottom + 4);
    p = glide(p, edge, 0.5);
    pause(0.2);
    mouse(MOUSEEVENTF_LEFTDOWN);
    pause(0.1);
    p = glide(p, (p.0, p.1 + (fb.bottom - fa.bottom) + 22), 0.9);
    pause(0.4);
    mouse(MOUSEEVENTF_LEFTUP);
    glide(p, (p.0 + 90, p.1 + 60), 0.4);

    let _ = ffmpeg.wait();
    for c in [&mut a_proc, &mut b_proc, &mut bg] {
        let _ = c.kill();
        let _ = c.wait();
    }
    println!("recorded {out}");
}
