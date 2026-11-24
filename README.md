# WinMagnet

macOS-style magnetic window edges for Windows. A lightweight alternative to
AquaSnap's *AquaMagnet* feature.

## What's different from AquaMagnet

AquaMagnet *attracts*: once an edge gets within N pixels of another edge, the
window jumps onto it, whichever way you're moving. macOS works more like a
**detent**:

| | AquaMagnet | WinMagnet (macOS-like) |
|---|---|---|
| Approaching an edge | Pulled in from N px away | Moves freely until it touches (tiny 4px catch zone) |
| Pushing past it | Released after N px | **Held** until the cursor travels `resistance` px past it, then jumps back under the cursor |
| Pulling back | Held for N px | Released immediately |
| Fast flicks | Optional threshold | Pass straight through |
| Invisible resize borders | Gaps on Win10/11 | Uses DWM frame bounds: windows sit flush |

Snap targets: screen (work area) edges, monitor boundaries (brief resistance
when crossing displays), outer edges of neighbouring windows, and aligned
sides of windows stacked above/below each other. Works for moving and
resizing. Hold **Ctrl** while dragging to bypass.

## How it works

```
winmagnet.exe (tray app, ~170 KB)            target app process
──────────────────────────────────           ──────────────────────────────
SetWinEventHook(MOVESIZESTART)  ──────────►  (out-of-context, no injection)
  │ capture snapshot: other windows'
  │ DWM frames, monitors, DPI-scaled config
  │ → named shared memory (seqlock)
  └ SetWindowsHookEx(WH_CALLWNDPROC,  ─────►  magnet_hook.dll loads
       thread = dragged window's thread)      subclasses the window
                                              WM_MOVING / WM_SIZING:
                                                rect → physical px → engine
                                                → write snapped rect back
MOVESIZEEND → UnhookWindowsHookEx             WM_EXITSIZEMOVE → unsubclass
```

* **No global hook.** The DLL is only loaded into the process whose window
  you're dragging, only while you drag it. Nothing runs in any other process,
  and nothing polls when idle.
* **Native move loop is kept**, so Aero Snap / Snap Layouts, window shake and
  so on all keep working. We only adjust the rect Windows proposes.
* **The engine is pure Rust** (`crates/magnet-core`) and unit-tested without
  Windows. The move loop always proposes `start + cursor delta`, so the engine
  just has to remember which line it's holding.
* **DPI:** the app is per-monitor-v2 aware and works in physical pixels. The
  hook maps the rects of DPI-unaware and system-aware apps between logical
  and physical pixels.

```
crates/
  magnet-core/   snapping engine, geometry, config, shared-memory layout (no deps)
  magnet-hook/   cdylib injected into the dragged window's process
  winmagnet/     tray app: event hook, snapshot capture, hook management
    examples/dragtest.rs   end-to-end test with synthetic mouse input
```

## Build & run

Requires Rust (MSVC toolchain).

```powershell
cargo build --release
.\target\release\winmagnet.exe      # tray icon → Enabled / Exit
```

`magnet_hook.dll` must sit next to `winmagnet.exe`. At startup it is copied to
`%TEMP%\WinMagnet\` under a versioned name and loaded from there. Processes
that used it keep it pinned, so this keeps rebuilds from failing on a locked file.

### Tests

```powershell
cargo test                                   # engine unit tests
cargo build --workspace --all-targets        # note: --examples alone skips the DLL
.\target\debug\winmagnet.exe                 # in another terminal (debug build logs to stderr)
.\target\debug\examples\dragtest.exe         # drags a test window; takes over the mouse for ~3s
.\target\debug\examples\dragtest.exe unaware # same, with a DPI-unaware window
```

Debug builds of the hook also write `%TEMP%\WinMagnet\hook.log`.

## Defaults (96-DPI pixels, scaled per monitor)

| Setting | Default |
|---|---|
| Catch distance | 4 px |
| Resistance | 18 px |
| Fast-movement threshold | 2200 px/s |
| Bypass key | Ctrl |

These are compile-time defaults for now (`magnet_core::Config::DEFAULT`).

## Known limitations / roadmap

- [ ] **32-bit apps**: need an x86 build of the DLL plus a tiny x86 helper
      process to install hooks into WOW64 threads. They're skipped for now.
- [ ] **Elevated windows**: UIPI blocks hooks from a non-elevated process. Run
      WinMagnet elevated (e.g. a "highest privileges" scheduled task) to cover them.
- [ ] **Apps with their own drag loop** (some custom-chrome/Electron apps, tab
      tear-off) never send `WM_MOVING`. Possible fallback: track the
      `EVENT_OBJECT_LOCATIONCHANGE` stream and correct the position afterwards.
- [ ] **AppContainer processes**: the shared memory and the staged DLL need
      ACLs for `ALL APPLICATION PACKAGES` and a low integrity label.
- [ ] Settings: config file + small settings window, per-app exclusion list
      (e.g. games with anti-cheat), autostart.
- [ ] Optional "release animation": ease the window back under the cursor
      instead of jumping.
- [ ] Code signing (hooks + DLL loading trip AV heuristics on unsigned binaries).
