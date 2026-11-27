<p align="center">
  <img src="docs/images/logo.png" width="104" alt="allineo logo">
</p>

<h1 align="center">allineo</h1>

<p align="center">
  <b>Windows that snap together, just like on a Mac.</b><br>
  Drag a window next to another and it lines up perfectly, without you lining it up.
</p>

<p align="center">
  <a href="https://github.com/brnbtt/allineo/releases/latest/download/allineo.exe"><img alt="Download for Windows" src="https://img.shields.io/badge/download-for%20Windows-ffffff?style=for-the-badge&labelColor=111111"></a>
</p>

<p align="center">
  <a href="https://github.com/brnbtt/allineo/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/brnbtt/allineo?style=flat-square&color=333333&label=release"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-333333?style=flat-square">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-333333?style=flat-square"></a>
</p>

<p align="center">
  <img src="docs/images/demo.gif" width="620" alt="A window dragged next to another snaps flush, lines up with its top, and stops level with its bottom when resized">
</p>

On a Mac, a window you drag toward another one *aligns without overlapping*: it stops flush against
the edge, holds there for a moment, and lets go if you keep pushing. *allineo* (Italian for "I line
up") brings that same feeling to Windows. Install it once and forget about it: there's nothing to
learn, your windows just behave.

## Install

1. **[Download allineo.exe](https://github.com/brnbtt/allineo/releases/latest/download/allineo.exe)**
   (about 6 MB).
2. Open it and press **install**. No admin rights needed, and it takes a couple of seconds.
3. That's it. You can delete the file you downloaded.

> [!NOTE]
> allineo isn't code-signed yet, so Windows may show **"Windows protected your PC"** the first
> time. Click **More info → Run anyway**. The app is open source, and you can read every line of it here.

## How to use it

### 1. Drag a window toward another

As the edges meet, the window snaps flush against its neighbour: no gap, no overlap. Screen edges
and the taskbar work the same way.

### 2. Keep pushing, or pull back

Keep pushing and it holds for a moment, then lets go and follows your pointer again. Like on a Mac,
it's a subtle snap, not a hard stick: the faster you push, the sooner it lets go. Pull back and it
lets go straight away. Fling a window quickly and it flies straight past: only deliberate moves
snap.

### 3. Line them up

Slide a window along its neighbour and their tops (or sides) line up. Resize it and its edge stops
level with the window next to it, so side-by-side windows end up exactly the same height.

> [!TIP]
> Hold **Ctrl** while you drag to move a window freely, without snapping.

Windows' own Snap (drag to the top to maximise, to a side for half the screen) keeps working as
usual.

## Make it yours

Open **allineo** from the Start menu (or click its tray icon) to choose what windows snap to and how
firmly they hold. Changes save by themselves, and **restore mac feel** puts everything back to the
macOS defaults.

<p align="center">
  <img src="docs/images/settings.png" width="400" alt="The allineo settings window">
</p>

## Why allineo

- **The Mac feel, not a copy of AquaSnap.** Edges stop and hold; they don't pull your window in
  from across the screen.
- **Truly flush.** Windows 10 and 11 windows have invisible borders around them; allineo ignores
  them, so there are no 7-pixel gaps.
- **Only what you can see.** Windows hidden behind others, minimised or on another desktop are
  never in the way.
- **Light.** About 2 MB of memory, and it does nothing at all until you drag a window.
- **Only touches what you drag.** allineo helps the one window you're moving, only while you move
  it, instead of loading itself into every app on your PC.
- **Sharp on any display.** Works with any scaling (100%, 150%, 200%…), multiple monitors and older
  apps.
- **Private.** No account, no internet, no telemetry.

## Questions

<details>
<summary><b>How do I pause it?</b></summary>
<br>
Right-click the tray icon → <b>pause</b>, or flip the switch at the top of the settings. To move
just one window freely, hold <b>Ctrl</b> while you drag it.
</details>

<details>
<summary><b>Some windows don't snap. Why?</b></summary>
<br>
<ul>
<li><b>Windows running as administrator</b> (like an admin terminal): Windows doesn't let a normal
app help them.</li>
<li><b>Old 32-bit apps</b>: not supported yet.</li>
<li><b>Apps that move their own windows</b> instead of using the normal title bar drag: rare, but
some custom-made window frames do this.</li>
</ul>
</details>

<details>
<summary><b>Does it work with AquaSnap, WinSnap or similar tools?</b></summary>
<br>
Turn off their magnet / snapping feature (or close them) first; two tools moving the same window
fight each other.
</details>

<details>
<summary><b>How do I uninstall it?</b></summary>
<br>
Go to <b>Settings → Apps → Installed apps → allineo → Uninstall</b>, or press <b>uninstall</b> in
allineo's settings. It removes everything.
</details>

## Under the hood

Curious how it works, or want to build it yourself? See
**[how allineo works](docs/how-it-works.md)**.

## License

[MIT](LICENSE).
