mod activity;
mod classifier;
mod commands;
mod db;
mod seed;

use commands::{cleanup_on_quit, ensure_seeded, update_tray};
use db::Db;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ActivationPolicy, Manager, RunEvent, WindowEvent,
};

/// Cached tray icon rect in physical pixels (x, y, width, height).
#[derive(Debug, Clone, Copy)]
pub struct TrayRectCache {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct AppState {
    pub db: Mutex<Db>,
    pub current_session_id: Mutex<Option<String>>,
    pub last_fingerprint: Mutex<Option<String>>,
    pub current_run_id: Mutex<Option<String>>,
    /// Monotonic clock of last successful collector tick (detects lid-sleep gaps).
    pub last_tick_instant: Mutex<Option<std::time::Instant>>,
    /// Wall-clock ISO of that tick — used to end the pre-sleep session cleanly.
    pub last_tick_wall: Mutex<Option<String>>,
    /// Latest soft nudge shown in the menu-bar toast window.
    pub last_soft_nudge: Mutex<Option<SoftNudgeBanner>>,
    /// Last known menu-bar status item rect (for anchoring the nudge toast).
    pub last_tray_rect: Mutex<Option<TrayRectCache>>,
    /// Tracking preference is ON, but collection is auto-paused (display asleep / lock).
    pub auto_paused: Mutex<bool>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftNudgeBanner {
    pub summary: String,
    pub message: String,
    pub at: String,
    /// `todo` (gentle nudge) or `alarmHeadsUp` (scheduled hard-alarm reminder).
    pub kind: String,
    /// Formatted alarm time for `alarmHeadsUp` (e.g. "2:30 PM CT").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alarm_time: Option<String>,
    /// `classic` or `liquidGlass`
    pub style: String,
    pub glass_level: u32,
    pub glass_reduce_transparency: bool,
    pub glass_increase_contrast: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Menu-bar style: no Dock icon.
            let _ = app.set_activation_policy(ActivationPolicy::Accessory);

            let resolver = app.path();
            let data_dir = resolver
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from(".").join("data"));
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("tracker.db");
            let db = Db::open(&db_path).expect("open database");
            ensure_seeded(&db).expect("seed database");

            // Close orphans + reconcile open runs after crash / force-quit.
            let _ = db.reconcile_tracking_on_startup();

            let open_run_id = db
                .get_open_tracking_run()
                .ok()
                .flatten()
                .map(|r| r.id);

            let hb = db.get_heartbeat().ok().flatten();
            let show_onboarding = db
                .get_settings()
                .map(|s| !s.onboarding_completed)
                .unwrap_or(true);
            let state = AppState {
                db: Mutex::new(db),
                current_session_id: Mutex::new(None),
                last_fingerprint: Mutex::new(None),
                current_run_id: Mutex::new(open_run_id),
                last_tick_instant: Mutex::new(Some(std::time::Instant::now())),
                last_tick_wall: Mutex::new(hb),
                last_soft_nudge: Mutex::new(None),
                last_tray_rect: Mutex::new(None),
                auto_paused: Mutex::new(false),
            };
            app.manage(state);

            let menu = commands::build_tray_menu(app.handle())?;
            // Single tray only — do not also define app.trayIcon in tauri.conf.json
            // (that caused a duplicate white template + blue app-icon status item).
            let tray_icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))
                .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());
            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Productivity Tracker")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        let state = app.state::<AppState>();
                        cleanup_on_quit(&state);
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "toggle_tracking" => {
                        let state = app.state::<AppState>();
                        let enabled = state
                            .db
                            .lock()
                            .get_settings()
                            .map(|s| s.tracking_enabled)
                            .unwrap_or(false);
                        let _ = commands::set_tracking_enabled_inner(app, &state, !enabled);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Keep a fresh rect so soft nudges can sit under the status item.
                    let rect = match &event {
                        TrayIconEvent::Click { rect, .. }
                        | TrayIconEvent::DoubleClick { rect, .. }
                        | TrayIconEvent::Enter { rect, .. }
                        | TrayIconEvent::Move { rect, .. }
                        | TrayIconEvent::Leave { rect, .. } => Some(rect),
                        _ => None,
                    };
                    if let Some(rect) = rect {
                        let scale = 2.0_f64; // fallback; refined below if monitor known
                        let scale = tray
                            .app_handle()
                            .primary_monitor()
                            .ok()
                            .flatten()
                            .map(|m| m.scale_factor())
                            .unwrap_or(scale);
                        let pos = rect.position.to_physical::<f64>(scale);
                        let size = rect.size.to_physical::<f64>(scale);
                        *tray.app_handle().state::<AppState>().last_tray_rect.lock() =
                            Some(TrayRectCache {
                                x: pos.x,
                                y: pos.y,
                                width: size.width,
                                height: size.height,
                            });
                    }

                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(5));
                let state = handle.state::<AppState>();
                update_tray(&handle, &state);
            });

            if let Some(w) = app.get_webview_window("main") {
                if std::env::var("PROMO_DEMO").ok().as_deref() == Some("1") {
                    let _ = app.set_activation_policy(ActivationPolicy::Regular);
                    // Force onto the primary display — off-screen windows render blank.
                    if let Ok(Some(monitor)) = app.primary_monitor() {
                        let scale = monitor.scale_factor();
                        let mpos = monitor.position();
                        let msize = monitor.size();
                        let win_w = (440.0 * scale) as i32;
                        let win_h = (680.0 * scale) as i32;
                        let x = mpos.x + (msize.width as i32 - win_w) / 2;
                        let y = mpos.y + (msize.height as i32 - win_h) / 2;
                        let _ = w.set_position(tauri::Position::Physical(
                            tauri::PhysicalPosition { x, y },
                        ));
                    }
                    let _ = w.show();
                    let _ = w.set_focus();
                    let _ = w.unminimize();
                } else if show_onboarding {
                    let _ = w.show();
                    let _ = w.set_focus();
                } else {
                    let _ = w.hide();
                }
            }
            if let Some(w) = app.get_webview_window("nudge") {
                // Clear the default white WKWebView fill so only the popover shows.
                let _ = w.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
                let _ = w.hide();
            }

            // Promo recording: after Today is visible, fire a soft heads-up toast.
            if std::env::var("PROMO_DEMO").ok().as_deref() == Some("1") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(6));
                    let state = handle.state::<AppState>();
                    commands::fire_promo_soft_nudge(&handle, &state);
                    // Give the toast a few seconds, then bring Today back to front.
                    std::thread::sleep(Duration::from_secs(5));
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_today,
            commands::set_tracking_enabled,
            commands::get_settings,
            commands::save_settings,
            commands::get_category_rules,
            commands::save_category_rules,
            commands::get_templates,
            commands::save_day_templates,
            commands::update_task,
            commands::add_task,
            commands::delete_task,
            commands::get_current_activity,
            commands::open_accessibility_prefs,
            commands::open_automation_prefs,
            commands::check_accessibility,
            commands::check_automation,
            commands::preview_soft_nudge,
            commands::dismiss_soft_nudge,
            commands::get_day_summary,
            commands::complete_onboarding,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<AppState>();
                cleanup_on_quit(&state);
            }
        });
}
