use allineo_core::config::{Config as Engine, flags};
use serde::{Deserialize, Serialize};

use crate::paths;

/// User settings, stored as `config.json` in the install folder. Distances are in pixels at 100%
/// display scaling and are scaled to each monitor.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Master switch (also toggled from the tray).
    pub enabled: bool,
    pub other_windows: bool,
    pub matching_edges: bool,
    pub screen_edges: bool,
    pub resizing: bool,
    /// Resist where two displays meet. Not on macOS, so off and not shown in settings.
    pub between_displays: bool,
    /// How far you keep pushing past an edge before the window breaks free.
    pub hold: u32,
    /// How close an edge has to get before it's caught.
    pub reach: u32,
    pub fast_flicks_pass: bool,
    pub ctrl_bypass: bool,
    pub start_with_windows: bool,
    pub tray_icon: bool,
}

/// What the − / + buttons step through for "hold".
pub const HOLD_STEPS: [u32; 10] = [6, 10, 14, 20, 26, 32, 40, 50, 60, 80];

impl Default for Config {
    fn default() -> Self {
        let e = Engine::DEFAULT;
        Self {
            enabled: true,
            other_windows: e.flags & flags::OUTER_EDGES != 0,
            matching_edges: e.flags & flags::ALIGNED_EDGES != 0,
            screen_edges: e.flags & flags::SCREEN_EDGES != 0,
            resizing: e.flags & flags::RESIZE != 0,
            between_displays: e.flags & flags::MONITOR_BOUNDS != 0,
            hold: e.resistance as u32,
            reach: e.catch_distance as u32,
            fast_flicks_pass: e.fast_speed > 0,
            ctrl_bypass: e.bypass_vk != 0,
            start_with_windows: true,
            tray_icon: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(paths::config_file())
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .map(Config::normalized)
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(paths::install_dir())?;
        let json = serde_json::to_string_pretty(self).expect("config serializes");
        std::fs::write(paths::config_file(), json)
    }

    pub fn normalized(mut self) -> Self {
        self.hold = self.hold.clamp(HOLD_STEPS[0], 200);
        self.reach = self.reach.clamp(1, 40).min(self.hold.saturating_sub(1).max(1));
        self
    }

    /// The engine settings these map to (still at 96 DPI).
    pub fn engine(&self) -> Engine {
        let flag = |on: bool, f: u32| if on { f } else { 0 };
        Engine {
            flags: flag(self.other_windows, flags::OUTER_EDGES)
                | flag(self.matching_edges, flags::ALIGNED_EDGES)
                | flag(self.screen_edges, flags::SCREEN_EDGES)
                | flag(self.resizing, flags::RESIZE)
                | flag(self.between_displays, flags::MONITOR_BOUNDS),
            catch_distance: self.reach as i32,
            resistance: self.hold as i32,
            fast_speed: if self.fast_flicks_pass { Engine::DEFAULT.fast_speed } else { 0 },
            bypass_vk: if self.ctrl_bypass { Engine::DEFAULT.bypass_vk } else { 0 },
        }
    }

    /// True when everything that affects how snapping feels is at the macOS defaults.
    pub fn feels_like_mac(&self) -> bool {
        let d = Config::default();
        Config { enabled: self.enabled, start_with_windows: self.start_with_windows, tray_icon: self.tray_icon, ..d }
            == *self
    }

    pub fn restore_mac_feel(&mut self) {
        *self = Config {
            enabled: self.enabled,
            start_with_windows: self.start_with_windows,
            tray_icon: self.tray_icon,
            ..Config::default()
        };
    }

    pub fn hold_down(&mut self) {
        self.hold = HOLD_STEPS.iter().rev().copied().find(|&s| s < self.hold).unwrap_or(HOLD_STEPS[0]);
        self.reach = self.reach.min(self.hold - 1);
    }

    pub fn hold_up(&mut self) {
        self.hold = HOLD_STEPS.iter().copied().find(|&s| s > self.hold).unwrap_or(self.hold);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_engine() {
        assert_eq!(Config::default().engine(), Engine::DEFAULT);
        assert!(Config::default().feels_like_mac());
    }

    #[test]
    fn restore_keeps_app_settings() {
        let mut c = Config {
            hold: 50,
            other_windows: false,
            tray_icon: false,
            start_with_windows: false,
            ..Default::default()
        };
        assert!(!c.feels_like_mac());
        c.restore_mac_feel();
        assert!(c.feels_like_mac());
        assert!(!c.tray_icon && !c.start_with_windows);
    }

    #[test]
    fn partial_and_odd_files_load() {
        let c: Config = serde_json::from_str(r#"{"hold": 1, "reach": 99, "unknown": true}"#).unwrap();
        let c = c.normalized();
        assert_eq!(c.hold, HOLD_STEPS[0]);
        assert!(c.reach < c.hold);
        assert!(c.enabled);
    }

    #[test]
    fn hold_steps() {
        let mut c = Config::default();
        c.hold_up();
        assert_eq!(c.hold, 26);
        c.hold = 6;
        c.hold_down();
        assert_eq!(c.hold, 6);
        assert!(c.reach < c.hold);
    }
}
