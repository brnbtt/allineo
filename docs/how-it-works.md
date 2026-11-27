# How allineo works

## The feel

AquaSnap-style magnets *attract*: once an edge is within N pixels of another edge, the window jumps
onto it, whichever way you're moving. macOS works like a **detent** instead, and that's what the
engine (`crates/allineo-core/src/engine.rs`) implements:

| | AquaMagnet | allineo (macOS-like) |
|---|---|---|
| Approaching an edge | Pulled in from N px away | Follows the pointer exactly and stops when its edge touches, like hitting a wall: no pull, as on a Mac |
| Pushing past it | Released after N px | **Held** until the pointer is `hold` px past the edge (12 px for a deliberate push, less the faster you push; creeping in never gets through), then the window jumps back under the pointer |
| Pulling back | Held for N px | Released immediately |
| Quick moves | Optional threshold | Pass straight through (above 1500 px/s) |
| Crossing to another display | Resists | Free, like on a Mac |
| Invisible resize borders | Gaps on Win10/11 | Uses the DWM frame, so windows sit flush |

Apple describes the behaviour ("as the window nears the other one, it aligns without overlapping",
and when resizing "it aligns with the edge and stops") but doesn't publish the distances. Mac users
describe it as *"a subtle snap, it's not a hard stick. If you're overly aggressive with the mouse
cursor it will go beyond the edge"*. So the hold depends on how fast the pointer moves *across* the
edge (`v`, measured over the last 60 ms, since mouse events can arrive every millisecond):

- **creeping** (below 150 px/s): a real edge never lets go. It takes a deliberate push;
- **just above that**, up to 225 px/s: firmer than usual, easing down to `hold`;
- **beyond that**: `hold × (1 − (v / 1500 px/s)²)`, so an ordinary move only feels a slight bump;
- **above 1500 px/s**: nothing is caught, which also keeps long drags from stuttering over every
  edge they cross.

Using the speed across the edge, not the overall speed, means sliding a docked window quickly
along its neighbour never pulls it away.

**Alignment lines** (matching edges) are not walls: they hold ⅔ as much and are never solid, so a
window can always slide past. While moving, a window lines up only its top and left side with its
neighbours'. Lining up bottoms and right sides too made a window sliding along a neighbour pause
wherever those happened to meet, a wall that isn't there. While resizing, the edge being dragged
lines up with any matching edge, which is how side-by-side windows end up the same height.

There is also no pull. A window creeping toward an edge on a Mac doesn't jump onto it when it gets
close; it follows the pointer exactly and hits a wall when its edge gets there. `catch_distance`
(the distance at which an edge would be pulled onto a line) is therefore 0 by default.
`dragtest.exe` fails if the window ever gets ahead of the pointer before contact.

The defaults (`Config::DEFAULT`) treat one macOS point as one pixel at 100% scaling, and are scaled
per monitor. `config.json` records which set of defaults it was written with (`defaults`), so an
update that changes the built-in feel also moves untouched old values to the new ones.

Snap targets, per edge of the dragged window:

- **other windows**: the facing edge of a window that overlaps on the other axis (my right
  against your left);
- **matching edges**: the same edge of a window that sits directly above/below or beside (top to
  top and left to left while moving; any matching edge while resizing);
- **screen edges**: each monitor's work area, so the taskbar edge counts, except where two
  displays meet;
- **between displays** (config file only, `between_displays`): resist at shared display edges
  too.

Only windows you can actually see count: minimised, cloaked (other virtual desktops) and tool
windows are skipped, and so is any window completely covered by another one above it, snappable or
not (a full-screen video or game hides everything behind it). Click-through and layered windows
(overlays, screen dimmers) don't count as covering anything.

The Windows move/size loop computes every proposed rect from the pointer offset relative to the
drag start, not from the rect returned last time. So the engine is fed the *unmodified* proposal on
every event and only has to remember which line it's holding, and the window catching up with the
pointer on release comes for free.

## Architecture

```
allineo.exe agent (tray, ~2 MB)               the app whose window you drag
──────────────────────────────────           ──────────────────────────────
SetWinEventHook(MOVESIZESTART)  ──────────►  (out-of-context, no injection)
  │ capture snapshot: other windows'
  │ DWM frames, monitors, DPI-scaled config
  │ → named shared memory (seqlock)
  └ SetWindowsHookEx(WH_CALLWNDPROC,  ─────►  allineo_hook.dll loads
       thread = dragged window's thread)      subclasses the window
                                              WM_MOVING / WM_SIZING:
                                                rect → physical px → engine
                                                → write snapped rect back
MOVESIZEEND → UnhookWindowsHookEx             WM_EXITSIZEMOVE → unsubclass
```

- **No global hook.** The DLL is loaded only into the process whose window you're dragging, and
  only during the drag. Nothing runs in other processes and nothing polls while idle.
- **The native move loop stays in charge**, so Snap, Snap Layouts and window shake keep working;
  allineo only adjusts the rect Windows proposes. The hook goes first and the app still gets the
  final say, so apps that enforce an aspect ratio or a minimum size keep working.
- **The DLL pins itself** once it has subclassed a window, so it can never be unloaded underneath
  a live subclass procedure. That's also why it's unpacked to `%TEMP%\allineo` under a name derived
  from its contents rather than kept in the install folder.
- **Shared memory outlives the agent.** Each hook keeps its handle to the shared section open for
  as long as its app runs. Windows drops a section's *name* when its last handle closes (a mapped
  view isn't enough), so without that, a restarted agent would create a new section the hooks in
  already-running apps never see, and snapping would silently stop in those apps. A re-opened
  section also keeps counting sessions instead of restarting at 0. Because old hooks keep reading
  the section across updates, `SHARED_NAME` and `VERSION` must change together with its layout.
- **DPI:** the agent is per-monitor-v2 aware and works in physical pixels. For DPI-unaware and
  system-aware apps the hook maps their scaled rects to physical pixels and back using the window
  rect read in both coordinate spaces at the start of the drag
  (`LogicalToPhysicalPointForPerMonitorDPI` fails for points outside the window, which is exactly
  where a drag proposal is). Those windows can only be placed in scale-factor steps, so at 150%
  they may land 1 px off.

## The app

One exe does everything, like comprimo:

| Command | What it does |
|---|---|
| `allineo` / `allineo settings` | The settings window (with an **install** button when not installed yet) |
| `allineo agent` | The background magnet and tray icon (started at sign-in) |
| `allineo install [--quiet]` | Install for the current user and start the agent |
| `allineo uninstall [--quiet]` | Stop the agent and remove everything |

Install details (no admin rights):

- `%LOCALAPPDATA%\Programs\allineo\`: `allineo.exe`, `allineo.ico`, `config.json`;
- `HKCU\...\Uninstall\allineo`: the entry in Settings → Apps;
- `HKCU\...\Run\allineo`: start with Windows (toggle in settings);
- a Start menu shortcut that opens the settings.

The settings window saves `config.json` and posts a message to the agent's hidden window
(class `allineo.agent`), which reloads it. Pausing from the tray writes the same file.

## Building

Requires Rust (MSVC toolchain).

```powershell
cargo build --release          # target\release\allineo.exe, with the hook DLL embedded
cargo test                     # engine, config and icon tests
```

`crates/allineo/build.rs` builds `allineo-hook` into `target/hook` (a separate target dir, since
sharing the outer one would deadlock on cargo's build lock) and embeds the DLL, plus the icons
drawn in `src/icon.rs`.

End-to-end checks with synthetic mouse input (each takes over the mouse for a few seconds and
needs the agent running):

```powershell
cargo build --workspace --all-targets         # --examples alone skips the app and the DLL
.\target\debug\examples\dragtest.exe          # pushes one test window into another: held, then through
.\target\debug\examples\dragtest.exe unaware  # same, with a DPI-unaware window
.\target\debug\examples\dragtest.exe chrome   # a throwaway Chrome (temporary profile), by its title bar
.\target\debug\examples\dragtest.exe chrome-tab   # same, dragged by its tab
$env:ALLINEO_DRAG_STEP = 1; .\target\debug\examples\dragtest.exe  # a creep: the wall must hold
```

`docs/images/demo.gif` is recorded with `.\target\debug\examples\demo.exe out.mp4` (needs ffmpeg),
then converted with ffmpeg's palette filters (720 px wide, 25 fps, 96 colours, Bayer dithering).
It puts up a full-screen backdrop, so nothing else on screen ends up in the recording.

Debug builds log to `%TEMP%\allineo\agent.log` and `%TEMP%\allineo\hook.log`.

### Releasing

Push an annotated tag like `v0.1.0`; its message becomes the release notes. The `release` workflow
tests, builds and attaches `allineo.exe` to a GitHub release.

### Layout

```
crates/
  allineo-core/   snapping engine, geometry, config, shared-memory layout (no dependencies)
  allineo-hook/   cdylib loaded into the dragged window's process
  allineo/        the app: settings window, agent, tray, install
    examples/dragtest.rs   end-to-end tests with synthetic mouse input
    examples/demo.rs       records the README demo
```

## Not done yet

- **32-bit apps** need an x86 build of the DLL plus a small x86 helper process to hook WOW64
  threads.
- **Elevated windows** need allineo itself to run elevated (e.g. a "highest privileges" logon
  task), since Windows blocks hooks from a normal process.
- **Apps with their own drag loop** never send `WM_MOVING`.
- **ARM64**: an `aarch64` build of the DLL (and app) would cover native ARM apps.
