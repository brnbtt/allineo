use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Align, Layout, Margin, RichText, Vec2};

use crate::config::Config;
use crate::paths::{self, APP};
use crate::theme::{self, BG, RowGlyph, TEXT, TEXT_2, TEXT_3, semibold};
use crate::{icon, install, win};

// ---------------------------------------------------------------------------------------------
// Window chrome
// ---------------------------------------------------------------------------------------------

fn native_options(title: &str, size: [f32; 2]) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_resizable(false)
            .with_maximize_button(false)
            .with_minimize_button(false)
            .with_icon(egui::IconData { rgba: icon::rgba(64), width: 64, height: 64 }),
        centered: true,
        ..Default::default()
    }
}

/// Tints the native title bar to match the window, once.
fn style_chrome(frame: &eframe::Frame, done: &mut bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if *done {
        return;
    }
    *done = true;
    if let Ok(handle) = frame.window_handle()
        && let RawWindowHandle::Win32(h) = handle.as_raw()
    {
        win::style_title_bar(h.hwnd.get() as *mut _, BG.r(), 34, TEXT_3.r());
    }
}

fn panel() -> egui::CentralPanel {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(22)))
}

// ---------------------------------------------------------------------------------------------
// Settings window
// ---------------------------------------------------------------------------------------------

const WIDTH: f32 = 440.0;
const ROW_HEIGHT: f32 = 46.0;
const FOOTER_BUTTON: f32 = 40.0;
/// How long the "installed ✓" pill stays before the footer settles.
const DONE_PILL_SECS: f64 = 1.6;
/// Measured height with every row showing except "reset to mac feel".
const HEIGHT: f32 = 856.0;

type Outcome = Arc<Mutex<Option<Result<(), String>>>>;

pub fn run_settings() -> Result<(), String> {
    let Some(_instance) = win::SingleSettings::acquire() else {
        win::focus_settings();
        return Ok(());
    };
    let cfg = Config::load();
    let installed = install::is_installed();
    // Opening the settings also brings the magnet back if it isn't running (e.g. after "quit").
    if installed && (cfg.enabled || cfg.tray_icon) && !win::agent_running() {
        let _ = win::start_agent(&paths::installed_exe());
    }
    eframe::run_native(
        APP,
        native_options(APP, [WIDTH, HEIGHT]),
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(SettingsApp {
                saved: cfg.clone(),
                cfg,
                installed,
                running_installed: paths::running_from_install_dir(),
                task: None,
                finished_at: None,
                message: None,
                logo: theme::load_logo(&cc.egui_ctx),
                styled: false,
                height: HEIGHT,
                changed_at: None,
                saved_at: None,
                polled_at: 0.0,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

struct SettingsApp {
    cfg: Config,
    saved: Config,
    installed: bool,
    running_installed: bool,
    /// Install / update running on a worker thread.
    task: Option<Outcome>,
    /// When the last install/update finished (drives the "installed ✓" pill).
    finished_at: Option<f64>,
    /// A one-line note under the footer buttons: (is_error, text).
    message: Option<(bool, String)>,
    logo: egui::TextureHandle,
    styled: bool,
    height: f32,
    /// Settings auto-save shortly after the last change.
    changed_at: Option<f64>,
    saved_at: Option<f64>,
    /// Last check for changes made elsewhere (pausing from the tray).
    polled_at: f64,
}

impl SettingsApp {
    fn start_install(&mut self, ctx: &egui::Context) {
        self.cfg = self.cfg.clone().normalized();
        let cfg = self.cfg.clone();
        self.message = None;
        self.finished_at = None;
        let outcome: Outcome = Arc::default();
        let (slot, ctx) = (outcome.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = install::install(&cfg);
            *slot.lock().unwrap() = Some(result);
            ctx.request_repaint();
        });
        self.task = Some(outcome);
    }

    fn save(&mut self, now: f64) {
        let cfg = self.cfg.clone().normalized();
        let autostart_changed = cfg.start_with_windows != self.saved.start_with_windows;
        let result = cfg.save().map_err(|e| e.to_string()).and_then(|_| {
            if autostart_changed {
                install::set_autostart(cfg.start_with_windows).map_err(|e| e.to_string())?;
            }
            if win::agent_running() {
                win::reload_agent();
            } else if cfg.enabled || cfg.tray_icon {
                win::start_agent(&paths::installed_exe()).map_err(|e| e.to_string())?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                self.saved = self.cfg.clone();
                self.saved_at = Some(now);
                self.message = None;
            }
            Err(e) => self.message = Some((true, format!("couldn't save: {e}"))),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Sides::new().shrink_left().show(
            ui,
            |ui| {
                theme::logo_with_shadow(ui, &self.logo, 52.0);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(APP).size(25.0).family(semibold()).color(TEXT));
                    ui.label(RichText::new("windows line up as you drag, like on a mac").size(13.0).color(TEXT_2));
                });
            },
            |ui| {
                // Centered against the 52 pt logo, in a slot just big enough for the switch.
                ui.allocate_ui_with_layout(Vec2::new(38.0, 52.0), Layout::top_down(Align::Min), |ui| {
                    ui.add_space(15.0);
                    let tip = if self.cfg.enabled { "on · click to pause" } else { "paused · click to turn on" };
                    theme::toggle(ui, &mut self.cfg.enabled).on_hover_text(tip);
                });
            },
        );
    }

    /// One row of a group: glyph, `left`, and `right` aligned right, all vertically centered.
    fn row(ui: &mut egui::Ui, glyph: RowGlyph, left: impl FnOnce(&mut egui::Ui), right: impl FnOnce(&mut egui::Ui)) {
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), ROW_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_height(ROW_HEIGHT);
                theme::row_glyph(ui, glyph, theme::glyph_ink());
                ui.add_space(12.0);
                left(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), right);
            },
        );
    }

    fn label(ui: &mut egui::Ui, title: &str, sub: &str) {
        ui.label(RichText::new(title).color(TEXT));
        if !sub.is_empty() {
            ui.label(RichText::new(sub).size(12.0).color(TEXT_3));
        }
    }

    fn toggle_row(ui: &mut egui::Ui, glyph: RowGlyph, title: &str, sub: &str, value: &mut bool) {
        Self::row(
            ui,
            glyph,
            |ui| Self::label(ui, title, sub),
            |ui| {
                theme::toggle(ui, value);
            },
        );
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        let c = &mut self.cfg;
        theme::caption(ui, "windows stop at");
        theme::group(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            Self::toggle_row(ui, RowGlyph::OtherWindows, "other windows", "side by side, no gap", &mut c.other_windows);
            theme::hairline(ui);
            Self::toggle_row(
                ui,
                RowGlyph::Matching,
                "matching edges",
                "lines up tops and sides",
                &mut c.matching_edges,
            );
            theme::hairline(ui);
            Self::toggle_row(ui, RowGlyph::Screen, "screen edges", "and the taskbar", &mut c.screen_edges);
        });

        ui.add_space(16.0);
        theme::caption(ui, "feel");
        theme::group(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            Self::row(
                ui,
                RowGlyph::Hold,
                |ui| Self::label(ui, "resistance", "before it lets go"),
                |ui| {
                    let (less, more) =
                        theme::stepper(ui, &format!("{} px", c.hold), "lets go sooner", "holds on longer");
                    if less {
                        c.hold_down();
                    }
                    if more {
                        c.hold_up();
                    }
                },
            );
            theme::hairline(ui);
            Self::toggle_row(ui, RowGlyph::Resize, "resizing", "edges stop too", &mut c.resizing);
            theme::hairline(ui);
            Self::toggle_row(ui, RowGlyph::Flick, "quick moves", "go straight past edges", &mut c.fast_flicks_pass);
            theme::hairline(ui);
            Self::toggle_row(ui, RowGlyph::Key, "hold ctrl", "to move a window freely", &mut c.ctrl_bypass);
            if !c.feels_like_mac() {
                theme::hairline(ui);
                if theme::action_row(ui, ROW_HEIGHT, RowGlyph::Restore, "reset to mac feel").clicked() {
                    c.restore_mac_feel();
                }
            }
        });

        ui.add_space(16.0);
        theme::caption(ui, "app");
        theme::group(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            Self::toggle_row(
                ui,
                RowGlyph::Startup,
                "start with windows",
                "on every time you sign in",
                &mut c.start_with_windows,
            );
            theme::hairline(ui);
            Self::toggle_row(ui, RowGlyph::Tray, "tray icon", "quick pause and settings", &mut c.tray_icon);
        });
    }

    /// Fixed-height footer: a 40 pt action row and one line of status underneath. Every state
    /// (install button, progress, done, installed) uses the same space, so the window never jumps.
    fn footer(&mut self, ui: &mut egui::Ui, now: f64) {
        let ctx = ui.ctx().clone();
        let mut note: (bool, String) = (false, String::new());

        if self.task.is_some() {
            theme::progress_pill(ui, FOOTER_BUTTON, None, "installing", now);
            note.1 = "one moment".into();
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if let Some(t) = self.finished_at.filter(|t| now - t < DONE_PILL_SECS) {
            theme::done_pill(ui, FOOTER_BUTTON, "installed", ((now - t) / 0.45) as f32);
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if !self.installed {
            if ui.add_sized([ui.available_width(), FOOTER_BUTTON], theme::primary(&format!("install {APP}"))).clicked()
            {
                self.start_install(&ctx);
            }
            note.1 = "runs quietly in the background · no admin needed".into();
        } else {
            let (mut uninstall, mut update) = (false, false);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), FOOTER_BUTTON),
                Layout::left_to_right(Align::Center),
                |ui| {
                    uninstall = ui.add(theme::ghost("uninstall")).clicked();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.running_installed {
                            ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(12.0).color(TEXT_3));
                        } else {
                            update = ui.add(theme::secondary("update installed copy")).clicked();
                        }
                    });
                },
            );
            if update {
                self.start_install(&ctx);
            }
            if uninstall && win::confirm(&format!("Remove {APP} from this computer?")) {
                install::uninstall();
                win::info_box(&format!("{APP} has been removed."));
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            note.1 = match (self.cfg != self.saved, self.saved_at) {
                (true, _) => "saving…".into(),
                (false, Some(t)) if now - t < 2.5 => "saved".into(),
                _ if !self.cfg.enabled => "paused · windows move freely".into(),
                _ => "changes save automatically".into(),
            };
        }
        if self.task.is_none()
            && let Some(m) = &self.message
        {
            note = m.clone();
        }

        ui.add_space(6.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), egui::Sense::hover());
        let color = if note.0 { TEXT } else { TEXT_3 };
        let font = egui::FontId::proportional(12.0);
        let galley = ui.painter().layout(note.1.clone(), font, color, f32::INFINITY);
        let fits = galley.size().x <= rect.width();
        let text = if fits {
            note.1.clone()
        } else {
            // Long errors: keep one line here, the full text is in the tooltip.
            let mut t: String = note.1.lines().next().unwrap_or_default().chars().take(64).collect();
            t.push('…');
            t
        };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), color);
        if !fits {
            response.on_hover_text(&note.1);
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        style_chrome(frame, &mut self.styled);
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);

        let finished = self.task.as_ref().and_then(|t| t.lock().unwrap().take());
        if let Some(result) = finished {
            self.task = None;
            match result {
                Ok(()) => {
                    self.installed = true;
                    self.saved = self.cfg.clone();
                    self.finished_at = Some(now);
                    self.message = Some((false, "drag a window toward another to try it".into()));
                }
                Err(e) => self.message = Some((true, e)),
            }
        }
        let busy = self.task.is_some();

        // Auto-save (once installed) a moment after the last change.
        if self.installed && !busy && self.cfg != self.saved {
            let since = *self.changed_at.get_or_insert(now);
            if now - since > 0.4 && !ctx.egui_is_using_pointer() {
                self.changed_at = None;
                self.save(now);
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        } else {
            self.changed_at = None;
        }
        if self.saved_at.is_some_and(|t| now - t < 2.6) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // Pick up changes made from the tray (pause / resume) while nothing is pending here.
        if self.installed && !busy && self.cfg == self.saved && now - self.polled_at > 1.0 {
            self.polled_at = now;
            let on_disk = Config::load();
            if on_disk != self.saved {
                self.cfg = on_disk.clone();
                self.saved = on_disk;
            }
        }
        ctx.request_repaint_after(Duration::from_secs(1));

        let content = panel()
            .show(ui, |ui| {
                theme::top_glow(ui);
                let top = ui.cursor().top();
                ui.add_space(2.0);
                ui.add_enabled_ui(!busy, |ui| self.header(ui));
                ui.add_space(24.0);
                ui.add_enabled_ui(!busy, |ui| self.settings(ui));
                ui.add_space(20.0);
                self.footer(ui, now);
                ui.cursor().top() - top
            })
            .inner;

        // Fit the window to its content ("reset to mac feel" comes and goes).
        let wanted = (content + 2.0 * 22.0 - 8.0).round();
        if (wanted - self.height).abs() > 1.0 {
            self.height = wanted;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(WIDTH, wanted)));
        }
    }
}
