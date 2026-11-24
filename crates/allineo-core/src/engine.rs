//! The snapping engine.
//!
//! AquaSnap-style magnets *attract*: once an edge is within N pixels of a
//! line it jumps onto it, in either direction. macOS behaves like a
//! *detent* instead:
//!
//! * the window moves freely until one of its edges actually reaches a line
//!   (a small `catch_distance` lets slow approaches land exactly on it);
//! * it then stays put while the cursor keeps pushing, until the cursor has
//!   travelled `resistance` pixels past the line, at which point the window
//!   jumps back under the cursor and continues;
//! * pulling back the way you came releases it immediately;
//! * fast flicks pass straight through.
//!
//! The Windows move/size loop computes every proposed rect from the cursor
//! offset relative to the drag start, not from the rect we returned last
//! time, so the engine is fed the *unmodified* proposal on every event and
//! only has to remember which line (if any) it is currently holding.

use crate::config::{Config, flags};
use crate::geom::Rect;
use crate::shared::{MonitorRects, Snapshot};

/// Bit mask of the edges being dragged during a resize.
pub mod edges {
    pub const LEFT: u8 = 1;
    pub const TOP: u8 = 2;
    pub const RIGHT: u8 = 4;
    pub const BOTTOM: u8 = 8;

    /// Map the `wParam` of `WM_SIZING` (`WMSZ_*`) to an edge mask.
    pub const fn from_wmsz(wmsz: usize) -> u8 {
        match wmsz {
            1 => LEFT,
            2 => RIGHT,
            3 => TOP,
            4 => TOP | LEFT,
            5 => TOP | RIGHT,
            6 => BOTTOM,
            7 => BOTTOM | LEFT,
            8 => BOTTOM | RIGHT,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Side {
    Left = 0,
    Top = 1,
    Right = 2,
    Bottom = 3,
}

const SIDES: [Side; 4] = [Side::Left, Side::Top, Side::Right, Side::Bottom];

impl Side {
    const fn mask(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// Applies while the two spans overlap on the perpendicular axis
    /// (windows next to each other, screen edges).
    Overlap,
    /// Applies while the two spans touch on the perpendicular axis
    /// (windows stacked on top of each other: line up their sides).
    Adjacent,
}

/// A line an edge of the dragged window can stick to.
#[derive(Clone, Copy, Debug)]
struct Line {
    pos: i32,
    /// Extent of the source along the perpendicular axis.
    lo: i32,
    hi: i32,
    rule: Rule,
}

impl Line {
    fn applies(&self, lo: i32, hi: i32, tolerance: i32) -> bool {
        match self.rule {
            Rule::Overlap => lo < self.hi && self.lo < hi,
            Rule::Adjacent => (lo - self.hi).abs() <= tolerance || (hi - self.lo).abs() <= tolerance,
        }
    }
}

/// Candidate lines grouped by the edge of the dragged window they apply to.
#[derive(Debug, Default)]
struct Lines {
    by_side: [Vec<Line>; 4],
}

impl Lines {
    fn build(windows: &[Rect], monitors: &[MonitorRects], cfg: &Config) -> Self {
        let mut l = Lines::default();
        let f = cfg.flags;
        for w in windows.iter().filter(|w| !w.is_empty()) {
            if f & flags::OUTER_EDGES != 0 {
                l.push(Side::Left, w.right, w.top, w.bottom, Rule::Overlap);
                l.push(Side::Right, w.left, w.top, w.bottom, Rule::Overlap);
                l.push(Side::Top, w.bottom, w.left, w.right, Rule::Overlap);
                l.push(Side::Bottom, w.top, w.left, w.right, Rule::Overlap);
            }
            if f & flags::ALIGNED_EDGES != 0 {
                l.push(Side::Left, w.left, w.top, w.bottom, Rule::Adjacent);
                l.push(Side::Right, w.right, w.top, w.bottom, Rule::Adjacent);
                l.push(Side::Top, w.top, w.left, w.right, Rule::Adjacent);
                l.push(Side::Bottom, w.bottom, w.left, w.right, Rule::Adjacent);
            }
        }
        let screen = f & flags::SCREEN_EDGES != 0;
        let between = f & flags::MONITOR_BOUNDS != 0;
        for (i, m) in monitors.iter().enumerate() {
            for side in SIDES {
                // Parts of this monitor's edge that another display continues past.
                let neighbours: Vec<(i32, i32)> = monitors
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .filter_map(|(_, o)| shared_span(&m.bounds, &o.bounds, side))
                    .collect();
                if screen {
                    let mut spans = vec![span(&m.work, side)];
                    // A taskbar edge is a real edge even next to another display; only the bare
                    // monitor edge is open.
                    if edge(&m.work, side) == edge(&m.bounds, side) {
                        for &cut in &neighbours {
                            subtract(&mut spans, cut);
                        }
                    }
                    for (lo, hi) in spans {
                        l.push(side, edge(&m.work, side), lo, hi, Rule::Overlap);
                    }
                }
                if between {
                    for &(lo, hi) in &neighbours {
                        l.push(side, edge(&m.bounds, side), lo, hi, Rule::Overlap);
                    }
                }
            }
        }
        l
    }

    fn push(&mut self, side: Side, pos: i32, lo: i32, hi: i32, rule: Rule) {
        if hi > lo {
            self.by_side[side as usize].push(Line { pos, lo, hi, rule });
        }
    }
}

const fn edge(r: &Rect, side: Side) -> i32 {
    match side {
        Side::Left => r.left,
        Side::Top => r.top,
        Side::Right => r.right,
        Side::Bottom => r.bottom,
    }
}

/// Extent of `r` along the axis perpendicular to `side`.
const fn span(r: &Rect, side: Side) -> (i32, i32) {
    match side {
        Side::Left | Side::Right => (r.top, r.bottom),
        Side::Top | Side::Bottom => (r.left, r.right),
    }
}

/// The stretch of `a`'s `side` edge that `b` sits directly against, if any.
fn shared_span(a: &Rect, b: &Rect, side: Side) -> Option<(i32, i32)> {
    let opposite = match side {
        Side::Left => Side::Right,
        Side::Top => Side::Bottom,
        Side::Right => Side::Left,
        Side::Bottom => Side::Top,
    };
    if edge(b, opposite) != edge(a, side) {
        return None;
    }
    let ((a0, a1), (b0, b1)) = (span(a, side), span(b, side));
    let (lo, hi) = (a0.max(b0), a1.min(b1));
    (hi > lo).then_some((lo, hi))
}

fn subtract(spans: &mut Vec<(i32, i32)>, (c0, c1): (i32, i32)) {
    let mut out = Vec::with_capacity(spans.len() + 1);
    for &(a, b) in spans.iter() {
        if c1 <= a || c0 >= b {
            out.push((a, b));
            continue;
        }
        if c0 > a {
            out.push((a, c0));
        }
        if c1 < b {
            out.push((c1, b));
        }
    }
    *spans = out;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    side: Side,
    index: u32,
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    /// Value the tracked coordinate must take for the edge to sit on the line.
    target: i32,
    key: Key,
}

#[derive(Clone, Copy, Debug)]
struct Lock {
    key: Key,
    target: i32,
    /// Direction of travel when the line was hit (+1 / -1).
    dir: i32,
    /// Proposed value past which the window breaks free.
    release_at: i32,
}

/// Detent state machine for one scalar coordinate.
#[derive(Debug, Default)]
struct Detent {
    lock: Option<Lock>,
    prev: Option<i32>,
}

impl Detent {
    fn passthrough(&mut self, v: i32) -> i32 {
        self.lock = None;
        self.prev = Some(v);
        v
    }

    fn step(&mut self, proposed: i32, cands: &[Candidate], cfg: &Config, allow_new: bool) -> i32 {
        let prev = self.prev.replace(proposed);

        if let Some(l) = self.lock {
            let still_applies = cands.iter().any(|c| c.key == l.key);
            let pushed = (proposed - l.target) * l.dir;
            let broke_through = (proposed - l.release_at) * l.dir > 0;
            if still_applies && pushed >= -cfg.catch_distance && !broke_through {
                return l.target;
            }
            // Released this frame; don't immediately grab a line again or a
            // pull-back would get caught by the same line from the other side.
            self.lock = None;
            return proposed;
        }

        if !allow_new {
            return proposed;
        }
        let Some(prev) = prev else { return proposed };
        let dir = (proposed - prev).signum();
        if dir == 0 {
            return proposed;
        }

        // First line reached along the direction of travel.
        let mut best: Option<(i32, &Candidate)> = None;
        for c in cands {
            let before = (prev - c.target) * dir;
            let now = (proposed - c.target) * dir;
            if before < 0 && now >= -cfg.catch_distance {
                let dist = -before;
                if best.is_none_or(|(d, _)| dist < d) {
                    best = Some((dist, c));
                }
            }
        }

        match best {
            Some((_, c)) => {
                let overshoot = ((proposed - c.target) * dir).max(0);
                self.lock = Some(Lock {
                    key: c.key,
                    target: c.target,
                    dir,
                    release_at: c.target + dir * (cfg.resistance + overshoot),
                });
                c.target
            }
            None => proposed,
        }
    }
}

/// Smoothed cursor speed in px/s.
#[derive(Debug, Default)]
struct Speed {
    last: Option<(i32, i32, f64)>,
    value: f64,
}

impl Speed {
    fn update(&mut self, x: i32, y: i32, now_ms: f64) -> f64 {
        if let Some((lx, ly, lt)) = self.last {
            let dt = (now_ms - lt).max(1.0);
            let (dx, dy) = ((x - lx) as f64, (y - ly) as f64);
            let instant = (dx * dx + dy * dy).sqrt() * 1000.0 / dt;
            self.value = self.value * 0.5 + instant * 0.5;
        }
        self.last = Some((x, y, now_ms));
        self.value
    }
}

/// Snapping state for one drag (one move or one resize of one window).
#[derive(Debug)]
pub struct Session {
    cfg: Config,
    lines: Lines,
    x: Detent,
    y: Detent,
    sides: [Detent; 4],
    speed: Speed,
    buf: Vec<Candidate>,
}

impl Session {
    pub fn new(windows: &[Rect], monitors: &[MonitorRects], cfg: Config) -> Self {
        Self {
            lines: Lines::build(windows, monitors, &cfg),
            cfg,
            x: Detent::default(),
            y: Detent::default(),
            sides: Default::default(),
            speed: Speed::default(),
            buf: Vec::with_capacity(64),
        }
    }

    pub fn from_snapshot(s: &Snapshot) -> Self {
        Self::new(s.windows(), s.monitors(), s.config)
    }

    fn is_fast(&mut self, x: i32, y: i32, now_ms: f64) -> bool {
        let v = self.speed.update(x, y, now_ms);
        self.cfg.fast_speed > 0 && v > self.cfg.fast_speed as f64
    }

    /// Collect the lines that currently apply, converted to values of the
    /// tracked coordinate (`pos + offset`).
    fn collect(&mut self, sides: &[(Side, i32)], lo: i32, hi: i32) {
        let tolerance = self.cfg.catch_distance + self.cfg.resistance;
        self.buf.clear();
        for &(side, offset) in sides {
            for (i, line) in self.lines.by_side[side as usize].iter().enumerate() {
                if line.applies(lo, hi, tolerance) {
                    self.buf.push(Candidate { target: line.pos + offset, key: Key { side, index: i as u32 } });
                }
            }
        }
    }

    /// `proposed` is the visible frame the system wants to move the window
    /// to. Returns the frame to use instead.
    pub fn on_move(&mut self, proposed: Rect, now_ms: f64, bypass: bool) -> Rect {
        let fast = self.is_fast(proposed.left, proposed.top, now_ms);
        if bypass {
            self.x.passthrough(proposed.left);
            self.y.passthrough(proposed.top);
            return proposed;
        }
        let (w, h) = (proposed.width(), proposed.height());

        self.collect(&[(Side::Top, 0), (Side::Bottom, -h)], proposed.left, proposed.right);
        let y = self.y.step(proposed.top, &self.buf, &self.cfg, !fast);

        self.collect(&[(Side::Left, 0), (Side::Right, -w)], y, y + h);
        let x = self.x.step(proposed.left, &self.buf, &self.cfg, !fast);

        Rect::new(x, y, x + w, y + h)
    }

    /// Same as [`Session::on_move`] for resizes; `moving` is an [`edges`] mask.
    pub fn on_size(&mut self, proposed: Rect, moving: u8, now_ms: f64, bypass: bool) -> Rect {
        let px = if moving & edges::LEFT != 0 { proposed.left } else { proposed.right };
        let py = if moving & edges::TOP != 0 { proposed.top } else { proposed.bottom };
        let fast = self.is_fast(px, py, now_ms);
        let skip = bypass || self.cfg.flags & flags::RESIZE == 0;

        let mut out = proposed;
        for side in SIDES {
            if moving & side.mask() == 0 {
                continue;
            }
            let (value, lo, hi) = match side {
                Side::Left => (proposed.left, proposed.top, proposed.bottom),
                Side::Top => (proposed.top, proposed.left, proposed.right),
                Side::Right => (proposed.right, proposed.top, proposed.bottom),
                Side::Bottom => (proposed.bottom, proposed.left, proposed.right),
            };
            let v = if skip {
                self.sides[side as usize].passthrough(value)
            } else {
                self.collect(&[(side, 0)], lo, hi);
                self.sides[side as usize].step(value, &self.buf, &self.cfg, !fast)
            };
            match side {
                Side::Left => out.left = v,
                Side::Top => out.top = v,
                Side::Right => out.right = v,
                Side::Bottom => out.bottom = v,
            }
        }
        // Never invert the rect; the app's min-size constraints handle the rest.
        if out.width() <= 0 {
            out.left = proposed.left;
            out.right = proposed.right;
        }
        if out.height() <= 0 {
            out.top = proposed.top;
            out.bottom = proposed.bottom;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0, 0, 1920, 1040);

    fn cfg() -> Config {
        Config { fast_speed: 2000, catch_distance: 4, resistance: 18, ..Config::DEFAULT }
    }

    fn monitor() -> MonitorRects {
        MonitorRects { bounds: Rect::new(0, 0, 1920, 1080), work: SCREEN }
    }

    fn at(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(x, y, x + w, y + h)
    }

    /// Drag `win` horizontally until its left edge is at `to`, in `step`
    /// pixel increments, `ms` apart. Returns (proposed, actual) left values.
    fn drag_x(s: &mut Session, win: Rect, to: i32, step: i32, ms: f64) -> Vec<(i32, i32)> {
        let (w, h) = (win.width(), win.height());
        let dir = (to - win.left).signum();
        let mut out = Vec::new();
        let mut t = 0.0;
        let mut x = win.left;
        loop {
            let r = s.on_move(at(x, win.top, w, h), t, false);
            out.push((x, r.left));
            if x == to {
                break;
            }
            x += dir * step.min((to - x).abs());
            t += ms;
        }
        out
    }

    #[test]
    fn moves_freely_without_lines() {
        let mut s = Session::new(&[], &[], cfg());
        for (p, a) in drag_x(&mut s, at(100, 100, 300, 200), 600, 3, 16.0) {
            assert_eq!(p, a);
        }
    }

    #[test]
    fn sticks_to_screen_edge_then_breaks_free() {
        let mut s = Session::new(&[], &[monitor()], cfg());
        // Window 300 wide: right edge reaches 1920 when left == 1620.
        let trace = drag_x(&mut s, at(1500, 100, 300, 200), 1700, 2, 16.0);
        for (p, a) in trace {
            if p < 1616 {
                assert_eq!(a, p, "free before the catch zone");
            } else if p <= 1620 + 18 {
                assert_eq!(a, 1620, "held at the edge while pushing (p={p})");
            } else {
                assert_eq!(a, p, "breaks free after the resistance (p={p})");
            }
        }
    }

    #[test]
    fn pulling_back_releases_immediately() {
        let mut s = Session::new(&[], &[monitor()], cfg());
        drag_x(&mut s, at(1500, 100, 300, 200), 1630, 2, 16.0);
        let back = drag_x(&mut s, at(1630, 100, 300, 200), 1550, 2, 16.0);
        // Stays on the line while the cursor is still past it ...
        assert!(back.iter().filter(|(p, _)| *p >= 1616).all(|(_, a)| *a == 1620));
        // ... and follows the cursor as soon as it's back on the inside.
        assert!(back.iter().filter(|(p, _)| *p < 1616).all(|(p, a)| p == a));
    }

    #[test]
    fn fast_flicks_pass_through() {
        let mut s = Session::new(&[], &[monitor()], cfg());
        // 40px every 8ms = 5000 px/s
        for (p, a) in drag_x(&mut s, at(1300, 100, 300, 200), 1900, 40, 8.0) {
            assert_eq!(p, a);
        }
    }

    #[test]
    fn docks_against_neighbour_window() {
        let other = at(1000, 100, 400, 400);
        let mut s = Session::new(&[other], &[], cfg());
        // Our right edge should stop at other.left == 1000 -> left == 700.
        let trace = drag_x(&mut s, at(500, 200, 300, 200), 760, 2, 16.0);
        assert!(trace.iter().any(|&(p, a)| p > 700 && a == 700));
        assert!(trace.iter().all(|&(_, a)| a <= 700 || a > 700 + 18));
    }

    #[test]
    fn ignores_windows_that_dont_overlap_vertically() {
        let other = at(1000, 900, 400, 100);
        let mut s = Session::new(&[other], &[], cfg());
        for (p, a) in drag_x(&mut s, at(500, 100, 300, 200), 900, 2, 16.0) {
            assert_eq!(p, a);
        }
    }

    #[test]
    fn aligns_sides_of_stacked_windows() {
        // Other window directly above ours (our top touches its bottom).
        let other = at(100, 0, 400, 300);
        let mut s = Session::new(&[other], &[], cfg());
        let trace = drag_x(&mut s, at(200, 300, 300, 200), 60, 2, 16.0);
        assert!(trace.iter().any(|&(p, a)| p < 100 && a == 100));
    }

    #[test]
    fn resize_snaps_moving_edge_only() {
        let other = at(1000, 100, 400, 400);
        let mut s = Session::new(&[other], &[], cfg());
        let mut t = 0.0;
        let mut last = Rect::default();
        for right in (800..=1010).step_by(2) {
            last = s.on_size(Rect::new(500, 200, right, 400), edges::RIGHT, t, false);
            t += 16.0;
        }
        assert_eq!(last, Rect::new(500, 200, 1000, 400));
    }

    #[test]
    fn bypass_key_disables_snapping() {
        let mut s = Session::new(&[], &[monitor()], cfg());
        let mut t = 0.0;
        for x in (1500..1700).step_by(2) {
            let r = s.on_move(at(x, 100, 300, 200), t, true);
            assert_eq!(r.left, x);
            t += 16.0;
        }
    }

    #[test]
    fn crossing_between_displays_is_free_by_default() {
        let left = MonitorRects { bounds: Rect::new(0, 0, 1920, 1080), work: Rect::new(0, 0, 1920, 1040) };
        let right = MonitorRects { bounds: Rect::new(1920, 0, 3840, 1080), work: Rect::new(1920, 0, 3840, 1040) };
        let mut s = Session::new(&[], &[left, right], cfg());
        for (p, a) in drag_x(&mut s, at(1500, 100, 300, 200), 1800, 2, 16.0) {
            assert_eq!(p, a, "no resistance at the shared edge");
        }

        let between = Config { flags: cfg().flags | flags::MONITOR_BOUNDS, ..cfg() };
        let mut s = Session::new(&[], &[left, right], between);
        let trace = drag_x(&mut s, at(1500, 100, 300, 200), 1800, 2, 16.0);
        assert!(trace.iter().any(|&(p, a)| p > 1620 && a == 1620), "opt-in resistance at the shared edge");
    }

    #[test]
    fn partially_shared_display_edge_still_snaps_beside_the_neighbour() {
        // The right display is shorter: below it, the left display's right edge is a real edge.
        let left = MonitorRects { bounds: Rect::new(0, 0, 1920, 1080), work: Rect::new(0, 0, 1920, 1080) };
        let right = MonitorRects { bounds: Rect::new(1920, 0, 3200, 720), work: Rect::new(1920, 0, 3200, 720) };
        let mut s = Session::new(&[], &[left, right], cfg());
        let low = drag_x(&mut s, at(1500, 800, 300, 200), 1700, 2, 16.0);
        assert!(low.iter().any(|&(p, a)| p > 1620 && a == 1620));
    }

    #[test]
    fn wmsz_mapping() {
        assert_eq!(edges::from_wmsz(8), edges::BOTTOM | edges::RIGHT);
        assert_eq!(edges::from_wmsz(1), edges::LEFT);
        assert_eq!(edges::from_wmsz(0), 0);
    }
}
