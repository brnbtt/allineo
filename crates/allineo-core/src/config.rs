/// Bit flags for [`Config::flags`].
pub mod flags {
    /// Edges of the desktop and the taskbar (each monitor's work area), except where two displays
    /// meet.
    pub const SCREEN_EDGES: u32 = 1 << 0;
    /// Outer edges of other windows (dock windows side by side).
    pub const OUTER_EDGES: u32 = 1 << 1;
    /// Same-side edges of windows that sit directly above/below or beside
    /// the dragged one (line up the left edges of two stacked windows).
    pub const ALIGNED_EDGES: u32 = 1 << 2;
    /// Also resist where two displays meet, so a window pauses briefly when it crosses from one
    /// display to another. macOS doesn't do this, so it's off by default.
    pub const MONITOR_BOUNDS: u32 = 1 << 3;
    /// Also apply the magnet while resizing.
    pub const RESIZE: u32 = 1 << 4;
}

/// Tunables. Distances are expressed at 96 DPI in the user-facing config and
/// converted with [`Config::scaled`] before being handed to the engine.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub flags: u32,
    /// How close an edge must get to a line before it is pulled onto it. 0 = no pull: the window
    /// stops only once its edge actually reaches the line.
    pub catch_distance: i32,
    /// How far the cursor has to keep pushing past a line before the window
    /// breaks free when moving slowly (the macOS-style "detent"). Faster
    /// pushes break free sooner.
    pub resistance: i32,
    /// Movements faster than this (px/s) never get caught, and the hold fades
    /// towards zero as the speed approaches it. 0 disables both.
    pub fast_speed: i32,
    /// Virtual-key code that suspends snapping while held. 0 = none.
    pub bypass_vk: u32,
}

impl Config {
    /// Tuned to match macOS. Apple describes it as "as the window nears the other one, it aligns
    /// without overlapping", and for resizing "it aligns with the edge and stops", but doesn't
    /// publish the distances. Mac users describe it as "a subtle snap, not a hard stick: if you're
    /// overly aggressive with the mouse it will go beyond the edge", hence a light hold that fades
    /// with speed and quick moves passing straight through. There is no pull: on a Mac a window
    /// creeping toward an edge doesn't jump onto it, it just hits a wall when it gets there, so
    /// `catch_distance` is 0. One macOS point is about one pixel at 100% scaling.
    pub const DEFAULT: Config = Config {
        flags: flags::SCREEN_EDGES | flags::OUTER_EDGES | flags::ALIGNED_EDGES | flags::RESIZE,
        catch_distance: 0,
        resistance: 12,
        fast_speed: 1500,
        bypass_vk: 0x11, // VK_CONTROL
    };

    /// Convert 96-DPI distances to physical pixels for a monitor at `dpi`.
    pub fn scaled(&self, dpi: u32) -> Config {
        let dpi = if dpi == 0 { 96 } else { dpi as i64 };
        let s = |v: i32| ((v as i64 * dpi + 48) / 96) as i32;
        Config {
            catch_distance: s(self.catch_distance),
            resistance: s(self.resistance),
            fast_speed: s(self.fast_speed),
            ..*self
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling() {
        let c = Config::DEFAULT.scaled(144);
        assert_eq!(c.resistance, 18);
        assert_eq!(c.catch_distance, 0);
        assert_eq!(c.flags, Config::DEFAULT.flags);
        assert_eq!(Config::DEFAULT.scaled(96), Config::DEFAULT);
    }
}
