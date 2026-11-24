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
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, FindWindowW, GetClassNameW, GetMessageW,
    GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsWindowVisible, MSG, PostQuitMessage,
    RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SendMessageW, SetCursorPos, SetWindowPos, ShowWindow, TranslateMessage,
    WM_DESTROY, WNDCLASSW, WS_OVERLAPPEDWINDOW,
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
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // chrome-user <hwnd>: an existing (the user's own) Chrome window, put back afterwards.
    let existing = (mode == "chrome-user").then(|| {
        let s = args.get(2).expect("chrome-user needs a window handle");
        usize::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex window handle") as HWND
    });
    run_test(mode == "unaware", mode.starts_with("chrome"), mode == "chrome-tab", existing);
}

/// A throwaway Chrome instance (temporary profile, so it never touches the user's own windows).
#[allow(clippy::zombie_processes)]
fn spawn_chrome() -> (Child, HWND) {
    let profile = std::env::temp_dir().join("allineo-chrome-test");
    let child = Command::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--no-first-run", "--no-default-browser-check", "--new-window", "--window-position=200,300"])
        .args(["--window-size=600,400", "about:blank"])
        .spawn()
        .expect("Chrome not found");
    let pid = child.id();
    let start = Instant::now();
    loop {
        if let Some(hwnd) = top_level_of(pid, "Chrome_WidgetWin_1") {
            sleep(Duration::from_millis(1500));
            return (child, hwnd);
        }
        assert!(start.elapsed() < Duration::from_secs(15), "Chrome window did not appear");
        sleep(Duration::from_millis(100));
    }
}

/// A point Chrome reports as caption (HTCAPTION), scanning its top strip from the right.
fn caption_point(hwnd: HWND, f: &RECT) -> (i32, i32) {
    for dy in [10, 14, 20, 30, 6] {
        for i in (1..40).rev() {
            let x = f.left + (f.right - f.left) * i / 40;
            let y = f.top + dy;
            let lp = ((y as u32 & 0xFFFF) << 16 | (x as u32 & 0xFFFF)) as isize;
            if unsafe { SendMessageW(hwnd, 0x0084, 0, lp) } == 2 {
                println!("grabbing Chrome at caption point ({x}, {y})");
                return (x, y);
            }
        }
    }
    panic!("no caption area found on the Chrome window");
}

fn top_level_of(pid: u32, class: &str) -> Option<HWND> {
    struct Find {
        pid: u32,
        class: String,
        found: HWND,
        area: i64,
    }
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> windows_sys::core::BOOL {
        let f = unsafe { &mut *(data as *mut Find) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 64) }.max(0) as usize;
        if owner == f.pid && unsafe { IsWindowVisible(hwnd) } != 0 && String::from_utf16_lossy(&buf[..n]) == f.class {
            // Fresh profiles also show small dialogs; the browser frame is the biggest window.
            let r = frame(hwnd);
            let area = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
            if area > f.area {
                f.area = area;
                f.found = hwnd;
            }
        }
        1
    }
    let mut f = Find { pid, class: class.into(), found: null_mut(), area: 160_000 };
    unsafe { EnumWindows(Some(each), &mut f as *mut Find as LPARAM) };
    (!f.found.is_null()).then_some(f.found)
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

trait MapFirst<A, B> {
    fn map_first<C>(self, f: impl FnOnce(A) -> C) -> (C, B);
}

impl<A, B> MapFirst<A, B> for (A, B) {
    fn map_first<C>(self, f: impl FnOnce(A) -> C) -> (C, B) {
        (f(self.0), self.1)
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

fn run_test(unaware: bool, chrome: bool, by_tab: bool, existing: Option<HWND>) {
    let (mut a_proc, a) = match existing {
        Some(hwnd) => (None, hwnd),
        None if chrome => spawn_chrome().map_first(Some),
        None => spawn("allineo test A", 200, 300, 400, 300, unaware).map_first(Some),
    };
    let mut original = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetWindowRect(a, &mut original) };
    let (mut b_proc, b) = spawn("allineo test B", 800, 300, 400, 300, false);
    // Unaware windows get scaled, so lay the windows out in physical pixels.
    unsafe {
        let flags = SWP_NOACTIVATE | 0x0001; // NOSIZE
        if existing.is_some() {
            SetWindowPos(a, HWND_TOPMOST, 200, 300, 900, 600, SWP_NOACTIVATE);
        }
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

    // Chrome: grab the empty caption area right of the tab strip.
    // Chrome: grab the empty caption area, or (chrome-tab) the only tab itself.
    let (sx, sy) = if by_tab {
        (fa.left + 120, fa.top + 28)
    } else if chrome {
        caption_point(a, &fa)
    } else {
        (fa.left + 120, fa.top + 12)
    };
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
    let ok = held.len() >= 5 && free_after;
    println!("{}", if ok { "PASS" } else { "FAIL" });

    if existing.is_some() {
        // Put the user's window back exactly where it was.
        let r = original;
        unsafe { SetWindowPos(a, HWND_NOTOPMOST, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE) };
    }
    if let Some(a_proc) = &mut a_proc {
        if chrome {
            // Chrome spreads over many processes; take down the throwaway instance's whole tree.
            let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &a_proc.id().to_string()]).output();
        }
        let _ = a_proc.kill();
        let _ = a_proc.wait();
    }
    let _ = b_proc.kill();
    let _ = b_proc.wait();
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
