//! End-to-end smoke test: spawns two test windows in separate processes,
//! drags one towards the other with synthetic mouse input and prints whether
//! it stuck to the neighbour's edge.
//!
//!     cargo build && cargo run --example dragtest
//!
//! The allineo agent must already be running (`allineo agent`). This takes over the mouse for ~3s.

use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_CONTEXT_UNAWARE, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, HWND_TOPMOST, MSG, PostQuitMessage,
    RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SetCursorPos, SetWindowPos, ShowWindow, TranslateMessage, WM_DESTROY,
    WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("window") {
        let n = |i: usize| args[i].parse::<i32>().unwrap();
        let unaware = args.get(7).is_some_and(|a| a == "unaware");
        run_window(&args[6], n(2), n(3), n(4), n(5), unaware);
        return;
    }
    let unaware = args.get(1).is_some_and(|a| a == "unaware");
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    run_test(unaware);
}

// The child is returned to the caller, which kills and waits on it.
#[allow(clippy::zombie_processes)]
fn spawn(title: &str, x: i32, y: i32, w: i32, h: i32, unaware: bool) -> (Child, HWND) {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = Command::new(exe);
    cmd.args(["window", &x.to_string(), &y.to_string(), &w.to_string(), &h.to_string(), title]);
    if unaware {
        cmd.arg("unaware");
    }
    let mut child = cmd.spawn().unwrap();
    let t = wide(title);
    let start = Instant::now();
    loop {
        let hwnd = unsafe { FindWindowW(null(), t.as_ptr()) };
        if !hwnd.is_null() {
            sleep(Duration::from_millis(300));
            return (child, hwnd);
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("test window did not appear");
        }
        sleep(Duration::from_millis(20));
    }
}

fn frame(hwnd: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut r as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
    };
    r
}

fn mouse(flags: u32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}

fn run_test(unaware: bool) {
    let (mut a_proc, a) = spawn("allineo test A", 200, 300, 400, 300, unaware);
    let (mut b_proc, b) = spawn("allineo test B", 800, 300, 400, 300, false);
    // Unaware windows get scaled, so lay the windows out in physical pixels.
    unsafe {
        let flags = SWP_NOACTIVATE | 0x0001; // NOSIZE
        SetWindowPos(a, HWND_TOPMOST, 200, 300, 0, 0, flags);
        sleep(Duration::from_millis(100));
        SetWindowPos(b, HWND_TOPMOST, frame(a).right + 200, 300, 0, 0, flags);
        sleep(Duration::from_millis(200));
    }

    let fa = frame(a);
    let fb = frame(b);
    let gap = fb.left - fa.right;
    let show = |r: RECT| format!("({}, {}, {}, {})", r.left, r.top, r.right, r.bottom);
    println!("A frame {}\nB frame {}\ngap {gap}px", show(fa), show(fb));

    let (sx, sy) = (fa.left + 120, fa.top + 12);
    unsafe { SetCursorPos(sx, sy) };
    sleep(Duration::from_millis(100));
    mouse(MOUSEEVENTF_LEFTDOWN);
    sleep(Duration::from_millis(150));

    let mut trace = Vec::new();
    let total = gap + 80;
    let mut dx = 0;
    while dx < total {
        dx += 2;
        unsafe { SetCursorPos(sx + dx, sy) };
        sleep(Duration::from_millis(12));
        trace.push((dx, frame(a).right));
    }
    mouse(MOUSEEVENTF_LEFTUP);
    sleep(Duration::from_millis(100));

    // DPI-unaware windows can only be placed in scale-factor steps (1.5px at
    // 150%), so allow being one pixel off.
    let held: Vec<_> = trace.iter().filter(|(d, r)| *d > gap && (*r - fb.left).abs() <= 1).collect();
    let free_after = trace.last().is_some_and(|(d, r)| (*r - (fa.right + d)).abs() <= 2);
    println!("\ncursor dx -> A.right (B.left = {})", fb.left);
    for chunk in trace.chunks(10) {
        println!("{}", chunk.iter().map(|(d, r)| format!("{d:>4}:{r:<5}")).collect::<String>());
    }
    println!(
        "\nheld at B.left for {} samples ({}px of cursor travel past contact); followed cursor afterwards: {}",
        held.len(),
        held.len() * 2,
        free_after
    );
    let ok = !held.is_empty() && free_after;
    println!("{}", if ok { "PASS" } else { "FAIL" });

    for p in [&mut a_proc, &mut b_proc] {
        let _ = p.kill();
        let _ = p.wait();
    }
    std::process::exit(if ok { 0 } else { 1 });
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn run_window(title: &str, x: i32, y: i32, w: i32, h: i32, unaware: bool) {
    unsafe {
        SetProcessDpiAwarenessContext(if unaware {
            DPI_AWARENESS_CONTEXT_UNAWARE
        } else {
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
        });
        let instance = GetModuleHandleW(null());
        let class = wide("allineoTestWindow");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: (5 + 1) as _, // COLOR_WINDOW + 1
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide(title).as_ptr(),
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            w,
            h,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        ShowWindow(hwnd, SW_SHOW);
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOACTIVATE | 0x0001 | 0x0002); // NOSIZE | NOMOVE
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
