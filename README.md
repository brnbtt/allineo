<p align="center">
  <img src="docs/images/logo.png" width="104" alt="allineo logo">
</p>

<h1 align="center">allineo</h1>

<p align="center">
  <b>Windows snap together as you drag them, just like on a Mac.</b><br>
  Line windows up edge to edge without lining them up by hand.
</p>

<p align="center">
  <a href="https://github.com/brnbtt/allineo/releases/latest/download/allineo.exe"><img alt="Download for Windows" src="https://img.shields.io/badge/download-for%20Windows-ffffff?style=for-the-badge&labelColor=111111"></a>
</p>

<p align="center">
  <a href="https://github.com/brnbtt/allineo/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/brnbtt/allineo?style=flat-square&color=333333&label=release"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-333333?style=flat-square">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-333333?style=flat-square"></a>
</p>

On a Mac, a window you drag toward another one *aligns without overlapping*: it stops flush against
the edge, holds there for a moment, and lets go if you keep pushing. *allineo* (Italian for "I line
up") brings that feeling to Windows. Install it once and forget about it: there's nothing to learn,
your windows just behave.

## Install

1. **[Download allineo.exe](https://github.com/brnbtt/allineo/releases/latest/download/allineo.exe)**
   (about 6 MB).
2. Open it and press **install**. No admin rights needed, and it takes a couple of seconds.
3. That's it. You can delete the file you downloaded.

> [!NOTE]
> allineo isn't code-signed yet, so Windows may show **"Windows protected your PC"** the first
> time. Click **More info → Run anyway**. The app is open source, and you can read every line of it here.

## How it feels

- **Drag a window toward another one.** As the edges meet, it snaps flush, with no gap and no
  overlap.
- **Keep pushing** and it holds for a moment, then lets go and follows your pointer again.
- **Pull back** and it lets go straight away.
- **Fling it** and it flies straight past; only deliberate moves snap.
- **Resize a window** and its edge stops at its neighbour's, so side-by-side windows end up the
  same height.
- **Windows stacked on top of each other** line up their sides, and windows side by side line up
  their tops.
- The edges of the screen and the taskbar work the same way. Moving a window to another display
  is never in the way.
- Hold **Ctrl** while you drag to move a window freely.

Windows' own Snap (drag to the top to maximise, to a side for half the screen) keeps working.

## Make it yours

Open **allineo** from the Start menu (or click its tray icon) to choose what windows snap to and how
firmly they hold. Changes save by themselves, and **restore mac feel** puts everything back to the
macOS defaults.

<p align="center">
  <img src="docs/images/settings.png" width="400" alt="The allineo settings window">
</p>

## Why allineo

- **The Mac feel, not a copy of AquaSnap.** Edges stop and hold; they don't jump at your window
  from across the screen.
- **Truly flush.** Windows 10 and 11 windows have invisible borders; allineo ignores them, so
  there are no 7-pixel gaps.
- **Light.** About 2 MB of memory in the background, and it does nothing at all until you drag a
  window.
- **Only touches what you drag.** allineo helps the one window you're moving, only while you move
  it, instead of loading into every app on your PC.
- **Sharp on any display.** Works with mixed scaling (100%, 150%, 200%…) and older apps.
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
